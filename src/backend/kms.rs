use std::fs::{self, File, OpenOptions};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::ptr;

use drm::control::{connector, crtc, framebuffer, Device as ControlDevice};
use drm::Device;
use drm_fourcc::{DrmFourcc, DrmModifier};
use image::RgbaImage;
use rmcp::ErrorData as McpError;
use rustix::mm::{self, MapFlags, ProtFlags};

use super::pixel_format;
use super::MonitorInfo;

// -- DRM Card wrapper --

struct Card(File);

impl AsFd for Card {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}

impl Device for Card {}
impl ControlDevice for Card {}

impl Card {
    fn open(path: &str) -> std::io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        Ok(Card(file))
    }
}

// -- Active output: connector -> encoder -> crtc chain --

struct ActiveOutput {
    connector_name: String,
    crtc_handle: crtc::Handle,
    width: u32,
    height: u32,
    fb_handle: framebuffer::Handle,
}

// -- KMS backend --

pub struct KmsBackend {
    card: Card,
    outputs: Vec<ActiveOutput>,
}

impl KmsBackend {
    /// Open the first DRI card with connected outputs.
    /// Requires CAP_SYS_ADMIN for GET_FB/GET_FB2 ioctls.
    pub fn open() -> Result<Self, Box<dyn std::error::Error>> {
        let mut entries: Vec<_> = fs::read_dir("/dev/dri")?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .is_some_and(|n| n.starts_with("card"))
            })
            .collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in &entries {
            let path = entry.path();
            let path_str = path.to_string_lossy();
            let card = match Card::open(&path_str) {
                Ok(c) => c,
                Err(e) => {
                    tracing::debug!("Cannot open {path_str}: {e}");
                    continue;
                }
            };

            match Self::probe_outputs(&card) {
                Ok(outputs) if !outputs.is_empty() => {
                    tracing::info!(
                        "KMS: using {path_str} with {} active output(s)",
                        outputs.len()
                    );
                    return Ok(KmsBackend { card, outputs });
                }
                Ok(_) => {
                    tracing::debug!("{path_str}: no active outputs");
                }
                Err(e) => {
                    tracing::debug!("{path_str}: probe failed: {e}");
                }
            }
        }

        Err("No DRI card with active outputs found. \
             Ensure /dev/dri/card* exists and the process has CAP_SYS_ADMIN \
             (try: sudo setcap cap_sys_admin+ep <binary>)"
            .into())
    }

    fn probe_outputs(card: &Card) -> Result<Vec<ActiveOutput>, Box<dyn std::error::Error>> {
        let res = card.resource_handles()?;
        let mut outputs = Vec::new();

        for &conn_h in res.connectors() {
            let conn = card.get_connector(conn_h, false)?;
            if conn.state() != connector::State::Connected {
                continue;
            }

            let enc_h = match conn.current_encoder() {
                Some(h) => h,
                None => continue,
            };
            let enc = card.get_encoder(enc_h)?;
            let crtc_h = match enc.crtc() {
                Some(h) => h,
                None => continue,
            };
            let crtc_info = card.get_crtc(crtc_h)?;
            let mode = match crtc_info.mode() {
                Some(m) => m,
                None => continue,
            };
            let fb_h = match crtc_info.framebuffer() {
                Some(h) => h,
                None => continue,
            };

            let (w, h) = mode.size();
            outputs.push(ActiveOutput {
                connector_name: format!("{}", conn),
                crtc_handle: crtc_h,
                width: w as u32,
                height: h as u32,
                fb_handle: fb_h,
            });
        }

        Ok(outputs)
    }

    pub fn capture_monitor(&self, monitor_id: Option<u32>) -> Result<RgbaImage, McpError> {
        let output = match monitor_id {
            Some(id) => self.outputs.get(id as usize).ok_or_else(|| {
                McpError::invalid_params(format!("Monitor index {id} out of range"), None)
            })?,
            None => self.outputs.first().ok_or_else(|| {
                McpError::internal_error("No active outputs", None)
            })?,
        };

        self.capture_fb(output)
    }

    pub fn list_monitors(&self) -> Result<Vec<MonitorInfo>, McpError> {
        Ok(self
            .outputs
            .iter()
            .enumerate()
            .map(|(i, o)| MonitorInfo {
                id: i as u32,
                name: o.connector_name.clone(),
                x: 0,
                y: 0,
                width: o.width,
                height: o.height,
                is_primary: i == 0,
            })
            .collect())
    }

    fn capture_fb(&self, output: &ActiveOutput) -> Result<RgbaImage, McpError> {
        // Refresh CRTC to get current framebuffer (may change due to page-flipping)
        let crtc_info = self.card.get_crtc(output.crtc_handle).map_err(|e| {
            McpError::internal_error(format!("Failed to get CRTC: {e}"), None)
        })?;
        let fb_handle = crtc_info.framebuffer().unwrap_or(output.fb_handle);

        // Try GET_FB2 first for pixel format info, fall back to GET_FB
        match self.capture_fb2(fb_handle, output.width, output.height) {
            Ok(img) => Ok(img),
            Err(fb2_err) => {
                tracing::debug!("GET_FB2 failed ({fb2_err}), trying GET_FB");
                self.capture_fb1(fb_handle, output.width, output.height)
            }
        }
    }

    fn capture_fb2(
        &self,
        fb_handle: framebuffer::Handle,
        width: u32,
        height: u32,
    ) -> Result<RgbaImage, McpError> {
        let info = self.card.get_planar_framebuffer(fb_handle).map_err(|e| {
            McpError::internal_error(format!("GET_FB2 failed: {e}"), None)
        })?;

        // Reject non-linear modifiers (tiled GPU buffers can't be mmap'd correctly)
        if let Some(modifier) = info.modifier() {
            if modifier != DrmModifier::Linear {
                return Err(McpError::internal_error(
                    format!(
                        "Framebuffer has non-linear modifier ({modifier:?}); \
                         tiled buffers cannot be read via mmap"
                    ),
                    None,
                ));
            }
        }

        let gem_handle = info.buffers()[0].ok_or_else(|| {
            McpError::internal_error("No buffer handle in framebuffer", None)
        })?;
        let pitch = info.pitches()[0];
        let format = info.pixel_format();

        let raw = self.mmap_gem_buffer(gem_handle, height, pitch)?;

        let rgba_data = pixel_format::convert_to_rgba(&raw, width, height, pitch, format)
            .map_err(|e| McpError::internal_error(e, None))?;

        // close_buffer releases our reference to the GEM handle returned by GET_FB2
        let _ = self.card.close_buffer(gem_handle);

        RgbaImage::from_raw(width, height, rgba_data).ok_or_else(|| {
            McpError::internal_error("Failed to create image from pixel data", None)
        })
    }

    fn capture_fb1(
        &self,
        fb_handle: framebuffer::Handle,
        width: u32,
        height: u32,
    ) -> Result<RgbaImage, McpError> {
        let info = self.card.get_framebuffer(fb_handle).map_err(|e| {
            McpError::internal_error(format!("GET_FB failed: {e}"), None)
        })?;

        let gem_handle = info.buffer().ok_or_else(|| {
            McpError::internal_error(
                "No buffer handle from GET_FB. \
                 CAP_SYS_ADMIN is required (try: sudo setcap cap_sys_admin+ep <binary>)",
                None,
            )
        })?;

        let pitch = info.pitch();
        let bpp = info.bpp();
        let depth = info.depth();

        // Map bpp/depth to a fourcc for our converter
        let format = match (bpp, depth) {
            (32, 24) => DrmFourcc::Xrgb8888,
            (32, 32) => DrmFourcc::Argb8888,
            (16, 16) => DrmFourcc::Rgb565,
            _ => {
                let _ = self.card.close_buffer(gem_handle);
                return Err(McpError::internal_error(
                    format!("Unsupported framebuffer format: {bpp}bpp depth={depth}"),
                    None,
                ));
            }
        };

        let raw = self.mmap_gem_buffer(gem_handle, height, pitch)?;

        let rgba_data = pixel_format::convert_to_rgba(&raw, width, height, pitch, format)
            .map_err(|e| {
                let _ = self.card.close_buffer(gem_handle);
                McpError::internal_error(e, None)
            })?;

        let _ = self.card.close_buffer(gem_handle);

        RgbaImage::from_raw(width, height, rgba_data).ok_or_else(|| {
            McpError::internal_error("Failed to create image from pixel data", None)
        })
    }

    /// Export GEM handle as PRIME fd, mmap it, read pixels, munmap, close fd.
    fn mmap_gem_buffer(
        &self,
        gem_handle: drm::buffer::Handle,
        height: u32,
        pitch: u32,
    ) -> Result<Vec<u8>, McpError> {
        let prime_fd: OwnedFd = self
            .card
            .buffer_to_prime_fd(gem_handle, drm::RDWR)
            .map_err(|e| {
                McpError::internal_error(format!("PRIME export failed: {e}"), None)
            })?;

        let size = (height as usize) * (pitch as usize);

        // SAFETY: we own the prime_fd, and the mapping size matches the buffer.
        // We read the pixels into a Vec and immediately munmap.
        let data = unsafe {
            let ptr = mm::mmap(
                ptr::null_mut(),
                size,
                ProtFlags::READ,
                MapFlags::SHARED,
                &prime_fd,
                0,
            )
            .map_err(|e| {
                McpError::internal_error(format!("mmap failed: {e}"), None)
            })?;

            let slice = std::slice::from_raw_parts(ptr.cast::<u8>(), size);
            let buf = slice.to_vec();

            let _ = mm::munmap(ptr, size);
            buf
        };

        Ok(data)
    }
}
