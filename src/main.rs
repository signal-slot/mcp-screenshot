use std::io::Cursor;

use base64::Engine;
use image::{DynamicImage, ImageFormat};
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
};
use serde::{Deserialize, Serialize};

// -- Request structs for tool parameters --

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct TakeScreenshotRequest {
    #[schemars(description = "Monitor ID to capture (omit for primary monitor)")]
    monitor_id: Option<u32>,
    #[schemars(description = "File path to save the screenshot PNG")]
    save_path: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct TakeScreenshotRegionRequest {
    #[schemars(description = "X coordinate of the top-left corner")]
    x: i32,
    #[schemars(description = "Y coordinate of the top-left corner")]
    y: i32,
    #[schemars(description = "Width of the region in pixels")]
    width: u32,
    #[schemars(description = "Height of the region in pixels")]
    height: u32,
    #[schemars(description = "Monitor ID to capture from (omit for primary monitor)")]
    monitor_id: Option<u32>,
    #[schemars(description = "File path to save the screenshot PNG")]
    save_path: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct TakeScreenshotWindowRequest {
    #[schemars(description = "Window ID to capture (use list_windows to find IDs)")]
    window_id: u32,
    #[schemars(description = "File path to save the screenshot PNG")]
    save_path: Option<String>,
}

// -- Response structs for JSON output --

#[derive(Serialize)]
struct WindowInfo {
    id: u32,
    title: String,
    app_name: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    is_minimized: bool,
    is_maximized: bool,
}

#[derive(Serialize)]
struct MonitorInfo {
    id: u32,
    name: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    is_primary: bool,
}

// -- Helper functions --

fn encode_png_base64(img: &DynamicImage) -> Result<String, McpError> {
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
        .map_err(|e| McpError::internal_error(format!("Failed to encode PNG: {e}"), None))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&buf))
}

fn save_image(img: &DynamicImage, path: &str) -> Result<(), McpError> {
    img.save(path)
        .map_err(|e| {
            McpError::internal_error(format!("Failed to save image to {path}: {e}"), None)
        })?;
    Ok(())
}

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

fn screenshot_result(
    img: &DynamicImage,
    save_path: Option<&str>,
) -> Result<CallToolResult, McpError> {
    if let Some(path) = save_path {
        save_image(img, path)?;
    }
    let b64 = encode_png_base64(img)?;
    let mut content = vec![Content::image(b64, "image/png")];
    if let Some(path) = save_path {
        content.push(Content::text(format!("Screenshot saved to {path}")));
    }
    Ok(CallToolResult::success(content))
}

// -- MCP Server --

#[derive(Clone)]
struct ScreenshotServer {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl ScreenshotServer {
    fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Take a full-screen screenshot. Returns a base64-encoded PNG image. Optionally specify a monitor and/or a file path to save.")]
    async fn take_screenshot(
        &self,
        Parameters(req): Parameters<TakeScreenshotRequest>,
    ) -> Result<CallToolResult, McpError> {
        let monitor = find_monitor(req.monitor_id)?;
        let rgba = monitor
            .capture_image()
            .map_err(|e| McpError::internal_error(format!("Failed to capture screen: {e}"), None))?;
        let img = DynamicImage::ImageRgba8(rgba);
        screenshot_result(&img, req.save_path.as_deref())
    }

    #[tool(description = "Take a screenshot of a specific screen region. Captures the full screen then crops to the specified rectangle. Returns a base64-encoded PNG image.")]
    async fn take_screenshot_region(
        &self,
        Parameters(req): Parameters<TakeScreenshotRegionRequest>,
    ) -> Result<CallToolResult, McpError> {
        let monitor = find_monitor(req.monitor_id)?;
        let rgba = monitor
            .capture_image()
            .map_err(|e| McpError::internal_error(format!("Failed to capture screen: {e}"), None))?;
        let img = DynamicImage::ImageRgba8(rgba);

        let (img_w, img_h) = (img.width(), img.height());
        let crop_x = req.x.max(0) as u32;
        let crop_y = req.y.max(0) as u32;
        if crop_x >= img_w || crop_y >= img_h {
            return Err(McpError::invalid_params(
                "Region is outside screen bounds",
                None,
            ));
        }
        let crop_w = req.width.min(img_w - crop_x);
        let crop_h = req.height.min(img_h - crop_y);
        let cropped = img.crop_imm(crop_x, crop_y, crop_w, crop_h);

        screenshot_result(&cropped, req.save_path.as_deref())
    }

    #[tool(description = "Take a screenshot of a specific window by its ID. Use list_windows to find window IDs. Returns a base64-encoded PNG image.")]
    async fn take_screenshot_window(
        &self,
        Parameters(req): Parameters<TakeScreenshotWindowRequest>,
    ) -> Result<CallToolResult, McpError> {
        let windows = xcap::Window::all()
            .map_err(|e| McpError::internal_error(format!("Failed to list windows: {e}"), None))?;
        let window = windows
            .into_iter()
            .find(|w| w.id().ok() == Some(req.window_id))
            .ok_or_else(|| {
                McpError::invalid_params(
                    format!("Window with ID {} not found", req.window_id),
                    None,
                )
            })?;
        let rgba = window
            .capture_image()
            .map_err(|e| {
                McpError::internal_error(format!("Failed to capture window: {e}"), None)
            })?;
        let img = DynamicImage::ImageRgba8(rgba);
        screenshot_result(&img, req.save_path.as_deref())
    }

    #[tool(description = "List all visible windows with their ID, title, app name, position, size, and minimized/maximized state.")]
    async fn list_windows(&self) -> Result<CallToolResult, McpError> {
        let windows = xcap::Window::all()
            .map_err(|e| McpError::internal_error(format!("Failed to list windows: {e}"), None))?;
        let infos: Vec<WindowInfo> = windows
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
            .collect();
        let json = serde_json::to_string_pretty(&infos)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize: {e}"), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "List all monitors with their ID, name, position, resolution, and whether they are the primary monitor.")]
    async fn list_monitors(&self) -> Result<CallToolResult, McpError> {
        let monitors = xcap::Monitor::all()
            .map_err(|e| McpError::internal_error(format!("Failed to list monitors: {e}"), None))?;
        let infos: Vec<MonitorInfo> = monitors
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
            .collect();
        let json = serde_json::to_string_pretty(&infos)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize: {e}"), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

#[tool_handler]
impl ServerHandler for ScreenshotServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "MCP server for taking screenshots, listing windows and monitors.".to_string(),
            ),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();
    tracing::info!("Starting MCP Screenshot Server");

    let service = ScreenshotServer::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
