mod backend;

use std::io::Cursor;
use std::sync::Arc;

use base64::Engine;
use image::{DynamicImage, ImageFormat};
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
};
use serde::Deserialize;

use backend::Backend;

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
    backend: Arc<Backend>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl ScreenshotServer {
    fn new(backend: Backend) -> Self {
        let caps = backend.capabilities();
        let mut router = Self::tool_router();

        if !caps.supports_windows {
            router.remove_route("take_screenshot_window");
            router.remove_route("list_windows");
            tracing::info!("Window tools removed (not supported by {} backend)", backend.name());
        }

        Self {
            backend: Arc::new(backend),
            tool_router: router,
        }
    }

    #[tool(description = "Take a full-screen screenshot. Returns a base64-encoded PNG image. Optionally specify a monitor and/or a file path to save.")]
    async fn take_screenshot(
        &self,
        Parameters(req): Parameters<TakeScreenshotRequest>,
    ) -> Result<CallToolResult, McpError> {
        let rgba = self.backend.capture_monitor(req.monitor_id)?;
        let img = DynamicImage::ImageRgba8(rgba);
        screenshot_result(&img, req.save_path.as_deref())
    }

    #[tool(description = "Take a screenshot of a specific screen region. Captures the full screen then crops to the specified rectangle. Returns a base64-encoded PNG image.")]
    async fn take_screenshot_region(
        &self,
        Parameters(req): Parameters<TakeScreenshotRegionRequest>,
    ) -> Result<CallToolResult, McpError> {
        let cropped =
            self.backend
                .capture_region(req.monitor_id, req.x, req.y, req.width, req.height)?;
        screenshot_result(&cropped, req.save_path.as_deref())
    }

    #[tool(description = "Take a screenshot of a specific window by its ID. Use list_windows to find window IDs. Returns a base64-encoded PNG image.")]
    async fn take_screenshot_window(
        &self,
        Parameters(req): Parameters<TakeScreenshotWindowRequest>,
    ) -> Result<CallToolResult, McpError> {
        let rgba = self.backend.capture_window(req.window_id)?;
        let img = DynamicImage::ImageRgba8(rgba);
        screenshot_result(&img, req.save_path.as_deref())
    }

    #[tool(description = "List all visible windows with their ID, title, app name, position, size, and minimized/maximized state.")]
    async fn list_windows(&self) -> Result<CallToolResult, McpError> {
        let infos = self.backend.list_windows()?;
        let json = serde_json::to_string_pretty(&infos)
            .map_err(|e| McpError::internal_error(format!("Failed to serialize: {e}"), None))?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "List all monitors with their ID, name, position, resolution, and whether they are the primary monitor.")]
    async fn list_monitors(&self) -> Result<CallToolResult, McpError> {
        let infos = self.backend.list_monitors()?;
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

    let backend = backend::detect()?;
    tracing::info!("Backend: {}", backend.name());

    let service = ScreenshotServer::new(backend).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
