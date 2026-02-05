# Building from source

```sh
# Default (desktop backend, stdio transport)
cargo build --release

# HTTP + KMS (headless server)
cargo build --release --no-default-features --features http,kms

# All features
cargo build --release --features desktop,kms,http
```

The binary will be at `target/release/mcp-screenshot`.

## Linux Build Dependencies

### desktop backend

```sh
# Debian/Ubuntu
apt install libxcb1-dev libxrandr-dev libdbus-1-dev libpipewire-0.3-dev libwayland-dev libgbm-dev

# Gentoo
emerge -a x11-libs/libxcb x11-libs/libXrandr sys-apps/dbus media-video/pipewire dev-libs/wayland x11-libs/gbm
```

### KMS backend

No additional build dependencies — the `drm`, `drm-fourcc`, and `rustix` crates are pure Rust.

## KMS Runtime Requirements

The KMS backend requires `CAP_SYS_ADMIN` capability to read framebuffer contents via DRM ioctls (GET_FB/GET_FB2):

```sh
sudo setcap cap_sys_admin+ep target/release/mcp-screenshot
```

Or run as root. Without this capability, the KMS backend will fail to open with a clear error message.

**Supported pixel formats:** XRGB8888, ARGB8888, XBGR8888, ABGR8888, RGB565.

**Limitation:** Framebuffers with non-linear modifiers (tiled GPU buffers) cannot be read via mmap and are rejected.
