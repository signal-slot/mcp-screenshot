# mcp-screenshot

An MCP (Model Context Protocol) server for taking screenshots, listing windows, and listing monitors. Built with Rust.

## Features

- Full-screen screenshot capture
- Region-based screenshot capture
- Window-specific screenshot capture
- Window listing with position, size, and state
- Monitor listing with resolution and layout info
- Screenshots returned as base64-encoded PNG via MCP image content
- Optional file saving

## Backends

| Backend | Platforms | Window Support |
|---------|-----------|:-:|
| **desktop** (default) | X11, Wayland, macOS, Windows | Yes |
| **kms** | Embedded Linux (DRM/KMS, no display server) | No |

Backend is auto-detected at startup:

1. `MCP_SCREENSHOT_BACKEND` env var override (`desktop` or `kms`)
2. `DISPLAY` / `WAYLAND_DISPLAY` present → desktop
3. `/dev/dri/card*` with active outputs → KMS
4. Fallback to desktop

## Tools

| Tool | Description | desktop | kms |
|------|-------------|:----:|:---:|
| `take_screenshot` | Full-screen screenshot | Yes | Yes |
| `take_screenshot_region` | Region screenshot | Yes | Yes |
| `take_screenshot_window` | Window screenshot | Yes | - |
| `list_windows` | List all windows | Yes | - |
| `list_monitors` | List all monitors | Yes | Yes |

On the KMS backend, window tools are removed from the MCP tool list entirely — clients never see them.

### Parameters

| Tool | Parameters |
|------|------------|
| `take_screenshot` | `monitor_id?: u32`, `save_path?: string` |
| `take_screenshot_region` | `x: i32`, `y: i32`, `width: u32`, `height: u32`, `monitor_id?: u32`, `save_path?: string` |
| `take_screenshot_window` | `window_id: u32`, `save_path?: string` |
| `list_windows` | (none) |
| `list_monitors` | (none) |

## Install

```sh
# Default (desktop backend, stdio transport)
cargo install mcp-screenshot

# HTTP + KMS (headless server)
cargo install mcp-screenshot --no-default-features --features http,kms
```

See [BUILD.md](BUILD.md) for building from source.

## Transport

By default, the server uses **stdio** transport. With the `http` feature enabled, you can switch to **HTTP Streamable** transport — ideal for headless servers running the KMS backend.

### HTTP Transport

Start in HTTP mode via CLI flag or environment variable:

```sh
# CLI flag (default port 8080)
mcp-screenshot --http

# Custom port
mcp-screenshot --http --port 3000

# Environment variables
MCP_SCREENSHOT_TRANSPORT=http MCP_SCREENSHOT_PORT=3000 mcp-screenshot
```

The server listens on `http://0.0.0.0:<port>/mcp`.

## Usage

### Claude Desktop (stdio)

Add to `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "screenshot": {
      "command": "/path/to/mcp-screenshot"
    }
  }
}
```

### Claude Desktop (HTTP)

Start the server, then add to `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "screenshot": {
      "url": "http://localhost:8080/mcp"
    }
  }
}
```

### Claude Code (stdio)

```sh
cargo install mcp-screenshot
claude mcp add screenshot mcp-screenshot
```

### Claude Code (HTTP)

```sh
cargo install mcp-screenshot --no-default-features --features http
claude mcp add --transport http screenshot http://localhost:8080/mcp
```

## Tech Stack

- [rmcp](https://crates.io/crates/rmcp) 0.13 - Official Rust MCP SDK (stdio & HTTP Streamable transport)
- [xcap](https://crates.io/crates/xcap) 0.8 - Cross-platform screen capture
- [drm](https://crates.io/crates/drm) 0.14 - DRM/KMS bindings (KMS backend)
- [image](https://crates.io/crates/image) 0.25 - Image processing
- [tokio](https://crates.io/crates/tokio) - Async runtime

## License

MIT
