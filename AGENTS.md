# AGENTS.md

Instructions and guidelines for AI agents working in the Clipped repository.

## Project Overview

Clipped is an ultra-lightweight, pure native desktop clipboard manager built with **Rust 2021** and **Raw Win32 + Direct2D / DirectWrite** (zero Chromium / zero WebView2 / zero Slint).

- **RAM Footprint**: **~0.33 MB Working Set / ~1.8 MB Private Memory** (reduced from 89 MB WebView2 and 10 MB Slint).
- **Executable Size**: ~1.5 MB standalone release binary.
- **Global Hotkey**: `Ctrl+Alt+Shift+.` (configurable in settings).
- **Behavior**: Opens centered floating modal, tracks clipboard history via native Win32 `AddClipboardFormatListener`, smart-truncates oldest non-favorites (max 999), allows instant FTS5 SQLite searching and direct pasting into active apps via simulated keystrokes.

---

## Tooling & Package Management

- **Build Tool**: Pure `cargo`. No Node.js, npm, or pnpm dependencies.
- **Rust Toolchain**: 2021 edition.

### Common Commands

```bash
# Verify Rust compilation
cargo check

# Run tests
cargo test

# Run native desktop app in dev mode
cargo run

# Build optimized release binary (target/release/clipped.exe)
cargo build --release
```

---

## Technology Stack

### Native GUI & Rendering
- **Framework**: Pure Win32 API (`WS_POPUP`, DWM rounded corners, per-monitor V2 DPI awareness).
- **Renderer**: Direct2D + DirectWrite hardware-accelerated rendering (`src/d2d.rs`).
- **RAM Footprint**: Sub-2 MB private memory with aggressive `EmptyWorkingSet` on window hide.

### Backend (Pure Rust)
- `windows`: Pure Win32 window management, Direct2D/DirectWrite rendering, Shell tray icon, Global Hotkey registration, Windows Media OCR, and process working set management.
- `arboard`: Fast native clipboard read/write (`3.6`).
- `rusqlite`: SQLite storage with WAL mode, FTS5 full-text search, and LRU bumping.
- `uuid` / `xxhash-rust`: Fast hashing and unique clip ID generation.

---

## Key Architecture & Patterns

### 1. Direct2D & DirectWrite Native Window
- Window created via `CreateWindowExW` with `WS_EX_TOOLWINDOW | WS_EX_TOPMOST` and `WS_POPUP`.
- Styled using Windows DWM: `DWMWA_WINDOW_CORNER_PREFERENCE = DWMWCP_ROUND` and `DWMWA_USE_IMMERSIVE_DARK_MODE`.
- Direct2D render target scales by system DPI factor (`GetDpiForWindow(hwnd)`), keeping internal drawing layout in fixed 480x360 logical space.

### 2. Event-Driven Subsystems
- **Event-Driven Clipboard Listener**: Native `AddClipboardFormatListener(hwnd)` with `WM_CLIPBOARDUPDATE` (0% idle CPU polling).
- **System Tray**: Native `Shell_NotifyIconW` with popup menu (`CreatePopupMenu`).
- **Global Hotkey**: Native `RegisterHotKey` handled via `WM_HOTKEY`.
- **Working Set Purge**: Win32 `EmptyWorkingSet` called whenever the modal loses focus or hides, trimming resident memory to under 2 MB.

### 3. Modal & Keyboard Flow
- **Auto-hide on blur**: Handled via `WM_ACTIVATE` / `WM_KILLFOCUS`.
- **Search input toggle**:
  - Typing any printable character in single card view immediately transitions into search list view and filters SQLite history via FTS5.
  - Backspace when query is empty returns to single card view.
- **Escape Key**:
  - In Settings: closes settings.
  - In Search with text: clears query.
  - In Search without text: returns to single card view.
  - In Single Card view: hides window.
- **Enter Key**:
  - Hides window, restores focus to target window, and synthesizes `Ctrl+V`.

---

## Code Quality Rules

1. **Verify Rust build and tests**: Always run `cargo check` and `cargo test`.
2. **Keep RAM lean**: Maintain pure Win32 / Direct2D architecture; do not introduce heavy UI runtimes or WebViews.
3. **No npm/Node drift**: Project is pure Rust, do not generate `package.json` or `node_modules`.
