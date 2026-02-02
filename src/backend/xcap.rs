use image::RgbaImage;
use rmcp::ErrorData as McpError;

use super::{MonitorInfo, WindowInfo};

pub struct XcapBackend;

impl XcapBackend {
    fn find_monitor(monitor_id: Option<u32>) -> Result<xcap::Monitor, McpError> {
        let monitors = xcap::Monitor::all()
            .map_err(|e| McpError::internal_error(format!("Failed to list monitors: {e}"), None))?;

        match monitor_id {
            Some(id) => monitors
                .into_iter()
                .find(|m| m.id().ok() == Some(id))
                .ok_or_else(|| {
                    McpError::invalid_params(format!("Monitor with ID {id} not found"), None)
                }),
            None => monitors
                .into_iter()
                .find(|m| m.is_primary().unwrap_or(false))
                .or_else(|| xcap::Monitor::all().ok()?.into_iter().next())
                .ok_or_else(|| McpError::internal_error("No monitors found", None)),
        }
    }

    pub fn capture_monitor(&self, monitor_id: Option<u32>) -> Result<RgbaImage, McpError> {
        let monitor = Self::find_monitor(monitor_id)?;
        monitor
            .capture_image()
            .map_err(|e| McpError::internal_error(format!("Failed to capture screen: {e}"), None))
    }

    pub fn capture_window(&self, window_id: u32) -> Result<RgbaImage, McpError> {
        let windows = xcap::Window::all()
            .map_err(|e| McpError::internal_error(format!("Failed to list windows: {e}"), None))?;
        let window = windows
            .into_iter()
            .find(|w| w.id().ok() == Some(window_id))
            .ok_or_else(|| {
                McpError::invalid_params(
                    format!("Window with ID {window_id} not found"),
                    None,
                )
            })?;
        window
            .capture_image()
            .map_err(|e| McpError::internal_error(format!("Failed to capture window: {e}"), None))
    }

    pub fn list_windows(&self) -> Result<Vec<WindowInfo>, McpError> {
        let windows = xcap::Window::all()
            .map_err(|e| McpError::internal_error(format!("Failed to list windows: {e}"), None))?;
        Ok(windows
            .iter()
            .filter_map(|w| {
                Some(WindowInfo {
                    id: w.id().ok()?,
                    title: w.title().unwrap_or_default(),
                    app_name: w.app_name().unwrap_or_default(),
                    x: w.x().unwrap_or(0),
                    y: w.y().unwrap_or(0),
                    width: w.width().unwrap_or(0),
                    height: w.height().unwrap_or(0),
                    is_minimized: w.is_minimized().unwrap_or(false),
                    is_maximized: w.is_maximized().unwrap_or(false),
                })
            })
            .collect())
    }

    pub fn list_monitors(&self) -> Result<Vec<MonitorInfo>, McpError> {
        let monitors = xcap::Monitor::all()
            .map_err(|e| McpError::internal_error(format!("Failed to list monitors: {e}"), None))?;
        Ok(monitors
            .iter()
            .filter_map(|m| {
                Some(MonitorInfo {
                    id: m.id().ok()?,
                    name: m.name().ok()?.to_string(),
                    x: m.x().unwrap_or(0),
                    y: m.y().unwrap_or(0),
                    width: m.width().unwrap_or(0),
                    height: m.height().unwrap_or(0),
                    is_primary: m.is_primary().unwrap_or(false),
                })
            })
            .collect())
    }
}
