# Clipped

Ultra-lightweight, native Windows clipboard manager written in **pure Rust** with **Raw Win32 + Direct2D / DirectWrite**.

## Highlights

- **Sub-2 MB RAM Footprint**: Idles at ~0.33 MB Working Set / ~1.8 MB Private Memory (down from 89 MB WebView2 and 10 MB Slint).
- **Zero Web Runtimes**: Zero Chromium, zero WebView2, zero Node.js/npm, zero heavyweight UI runtime.
- **Hardware-Accelerated UI**: Rendered with native Direct2D & DirectWrite with DWM rounded corners and smooth dark theme.
- **Event-Driven**: Win32 `AddClipboardFormatListener` eliminates CPU polling loops entirely.
- **Instant Search**: Full-text search (FTS5) backed by SQLite in WAL mode.
- **Global Hotkey**: Toggle floating modal anywhere with `Ctrl+Alt+Shift+.`.
- **Auto-Paste**: Restores foreground focus and simulates `Ctrl+V` into target application.
- **System Tray**: Native Windows notification area icon and context menu.

## Keyboard Controls

| Key | Action |
|---|---|
| `Ctrl+Alt+Shift+.` | Open / cycle modal |
| `Typing (a-z, 0-9)` | Instant search filter |
| `↑` / `↓` / `←` / `→` | Navigate clips |
| `1` - `9` | Select clip directly |
| `Tab` | Toggle starred view |
| `Enter` | Paste selected clip |
| `Delete` | Remove selected clip |
| `Escape` | Clear search / close modal |

## Build & Run

```bash
# Check compilation
cargo check

# Run tests
cargo test

# Run native app in dev mode
cargo run

# Build optimized release binary (~1.5 MB)
cargo build --release

# Build installer package (creates releases/download/clipped_0.5.0_x64-setup.exe)
powershell -ExecutionPolicy Bypass -File .\scripts\build-installer.ps1
# or manually:
makensis installer.nsi
```
