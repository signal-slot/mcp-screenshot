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

## Tools

| Tool | Description | Parameters |
|------|-------------|------------|
| `take_screenshot` | Full-screen screenshot | `monitor_id?: u32`, `save_path?: string` |
| `take_screenshot_region` | Region screenshot | `x: i32`, `y: i32`, `width: u32`, `height: u32`, `monitor_id?: u32`, `save_path?: string` |
| `take_screenshot_window` | Window screenshot | `window_id: u32`, `save_path?: string` |
| `list_windows` | List all windows | (none) |
| `list_monitors` | List all monitors | (none) |

## Platform Support

Uses [xcap](https://crates.io/crates/xcap) for cross-platform screen capture:

| Platform | Screen Capture | Window Capture |
|----------|:-:|:-:|
| X11 | Yes | Yes |
| Wayland | Partial (portal) | Partial (portal) |
| macOS | Yes | Yes |
| Windows | Yes | Yes |

## Build

```sh
cargo build --release
```

The binary will be at `target/release/mcp-screenshot`.

### Linux Build Dependencies

On Linux, you may need:

```sh
# Debian/Ubuntu
apt install libxcb-dev libxrandr-dev libdbus-1-dev libpipewire-0.3-dev libwayland-dev libgbm-dev

# Gentoo
emerge -a x11-libs/libxcb x11-libs/libXrandr sys-apps/dbus media-video/pipewire dev-libs/wayland x11-libs/gbm
```

## Usage

### Claude Desktop

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

### Claude Code

Add to Claude Code MCP settings:

```sh
claude mcp add screenshot /path/to/mcp-screenshot
```

## Tech Stack

- [rmcp](https://crates.io/crates/rmcp) 0.13 - Official Rust MCP SDK (stdio transport)
- [xcap](https://crates.io/crates/xcap) 0.8 - Cross-platform screen capture
- [image](https://crates.io/crates/image) 0.25 - Image processing
- [tokio](https://crates.io/crates/tokio) - Async runtime

## License

MIT
