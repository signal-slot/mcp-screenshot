#[cfg(feature = "desktop")]
mod xcap;
#[cfg(feature = "kms")]
mod kms;
#[cfg(feature = "kms")]
mod pixel_format;

#[cfg(feature = "desktop")]
pub use self::xcap::XcapBackend;
#[cfg(feature = "kms")]
pub use self::kms::KmsBackend;

use image::{DynamicImage, RgbaImage};
use rmcp::ErrorData as McpError;
use serde::Serialize;

// -- Shared data types --

#[derive(Serialize)]
pub struct MonitorInfo {
    pub id: u32,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}

#[derive(Serialize)]
pub struct WindowInfo {
    pub id: u32,
    pub title: String,
    pub app_name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_minimized: bool,
    pub is_maximized: bool,
}

// -- Backend capabilities --

pub struct BackendCapabilities {
    pub supports_windows: bool,
}

// -- Backend enum --

pub enum Backend {
    #[cfg(feature = "desktop")]
    Xcap(XcapBackend),
    #[cfg(feature = "kms")]
    Kms(KmsBackend),
}

impl Backend {
    pub fn capabilities(&self) -> BackendCapabilities {
        match self {
            #[cfg(feature = "desktop")]
            Backend::Xcap(_) => BackendCapabilities {
                supports_windows: true,
            },
            #[cfg(feature = "kms")]
            Backend::Kms(_) => BackendCapabilities {
                supports_windows: false,
            },
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            #[cfg(feature = "desktop")]
            Backend::Xcap(_) => "xcap",
            #[cfg(feature = "kms")]
            Backend::Kms(_) => "kms",
        }
    }

    pub fn capture_monitor(&self, monitor_id: Option<u32>) -> Result<RgbaImage, McpError> {
        match self {
            #[cfg(feature = "desktop")]
            Backend::Xcap(b) => b.capture_monitor(monitor_id),
            #[cfg(feature = "kms")]
            Backend::Kms(b) => b.capture_monitor(monitor_id),
        }
    }

    pub fn capture_region(
        &self,
        monitor_id: Option<u32>,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> Result<DynamicImage, McpError> {
        let rgba = self.capture_monitor(monitor_id)?;
        let img = DynamicImage::ImageRgba8(rgba);

        let (img_w, img_h) = (img.width(), img.height());
        let crop_x = x.max(0) as u32;
        let crop_y = y.max(0) as u32;
        if crop_x >= img_w || crop_y >= img_h {
            return Err(McpError::invalid_params(
                "Region is outside screen bounds",
                None,
            ));
        }
        let crop_w = width.min(img_w - crop_x);
        let crop_h = height.min(img_h - crop_y);
        Ok(img.crop_imm(crop_x, crop_y, crop_w, crop_h))
    }

    #[allow(unused_variables)]
    pub fn capture_window(&self, window_id: u32) -> Result<RgbaImage, McpError> {
        match self {
            #[cfg(feature = "desktop")]
            Backend::Xcap(b) => b.capture_window(window_id),
            #[cfg(feature = "kms")]
            Backend::Kms(_) => Err(McpError::internal_error(
                "Window capture is not supported on KMS backend",
                None,
            )),
        }
    }

    pub fn list_windows(&self) -> Result<Vec<WindowInfo>, McpError> {
        match self {
            #[cfg(feature = "desktop")]
            Backend::Xcap(b) => b.list_windows(),
            #[cfg(feature = "kms")]
            Backend::Kms(_) => Err(McpError::internal_error(
                "Window listing is not supported on KMS backend",
                None,
            )),
        }
    }

    pub fn list_monitors(&self) -> Result<Vec<MonitorInfo>, McpError> {
        match self {
            #[cfg(feature = "desktop")]
            Backend::Xcap(b) => b.list_monitors(),
            #[cfg(feature = "kms")]
            Backend::Kms(b) => b.list_monitors(),
        }
    }
}

// -- Backend detection --

pub fn detect() -> Result<Backend, Box<dyn std::error::Error>> {
    // 1. Check env override
    if let Ok(val) = std::env::var("MCP_SCREENSHOT_BACKEND") {
        match val.as_str() {
            #[cfg(feature = "desktop")]
            "xcap" => {
                tracing::info!("Using xcap backend (env override)");
                return Ok(Backend::Xcap(XcapBackend));
            }
            #[cfg(feature = "kms")]
            "kms" => {
                tracing::info!("Using KMS backend (env override)");
                let b = KmsBackend::open()?;
                return Ok(Backend::Kms(b));
            }
            other => {
                return Err(format!("Unknown backend '{other}' in MCP_SCREENSHOT_BACKEND").into());
            }
        }
    }

    // 2. Auto-detect: display server present -> xcap
    #[cfg(feature = "desktop")]
    {
        if std::env::var_os("DISPLAY").is_some()
            || std::env::var_os("WAYLAND_DISPLAY").is_some()
        {
            tracing::info!("Display server detected, using xcap backend");
            return Ok(Backend::Xcap(XcapBackend));
        }
    }

    // 3. Try KMS
    #[cfg(feature = "kms")]
    {
        match KmsBackend::open() {
            Ok(b) => {
                tracing::info!("Using KMS backend (no display server found)");
                return Ok(Backend::Kms(b));
            }
            Err(e) => {
                tracing::debug!("KMS probe failed: {e}");
            }
        }
    }

    // 4. Fallback to xcap even without display env vars
    #[cfg(feature = "desktop")]
    {
        tracing::info!("Falling back to xcap backend");
        return Ok(Backend::Xcap(XcapBackend));
    }

    #[allow(unreachable_code)]
    Err("No usable backend found. Enable the 'desktop' or 'kms' feature.".into())
}
