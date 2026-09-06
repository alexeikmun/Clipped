# AGENTS.md

Instructions and guidelines for AI agents working in the Clipped repository.

## Project Overview

Clipped is an ultra-lightweight, pure native desktop clipboard manager built with **Rust 2021** and **Slint 1.9** (zero Chromium / zero WebView2).

- **RAM Footprint**: ~10 MB private memory (reduced from ~138 MB).
- **Global Hotkey**: `Ctrl+Alt+Shift+.` (configurable in settings).
- **Behavior**: Opens centered floating modal, tracks clipboard history in background thread, smart-truncates oldest non-favorites (max 999), allows searching and direct pasting into active apps via simulated keystrokes.

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

### Native GUI
- **Framework**: Slint 1.9 (`ui/app.slint` compiled via `build.rs`).
- **Renderer**: `winit-software` / `femtovg` (ultra-low ~10 MB RAM footprint).

### Backend (Pure Rust)
- `global-hotkey`: Standalone native global hotkey registration (`0.6`).
- `tray-icon` + `muda`: Native Windows system tray icon and context menu.
- `arboard`: Fast native clipboard read/write (`3.6`).
- `enigo`: Simulates keystrokes (`Ctrl+V`) for target app pasting (`0.6`).
- `rusqlite`: SQLite storage with WAL mode, FTS5 full-text search, and LRU bumping.
- `windows`: Windows Media OCR and Win32 window management.
- `uuid` / `xxhash-rust`: Fast hashing and unique clip ID generation.

---

## Key Architecture & Patterns

### 1. Slint Declarative UI & Event Loop
- GUI defined in `ui/app.slint` and compiled at build-time via `build.rs` into native Rust code.
- Data binding via `slint::ModelRc<ClipData>` and `slint::VecModel`.
- Thread synchronization: Background threads use `window_weak.upgrade_in_event_loop(...)` to safely update UI state.
- Main timer loop (40ms) drains `GlobalHotKeyEvent`, `TrayIconEvent`, and `MenuEvent`, and checks foreground window focus for auto-hide.
### 2. Modal & Keyboard Flow
- **Auto-hide on blur**: Handled via Win32 `GetForegroundWindow()` polling check in 40ms timer loop (clicking outside hides modal).
- **Search input toggle**:
  - Starts hidden (`is_search_visible: false`).
  - Typing any printable character sets `is_search_visible: true`, enters character, and focuses `search_input`.
  - Deleting all text sets `is_search_visible: false` and returns to single card view.
- **Escape Key**:
  - 1st press: Clears search query and hides search input.
  - 2nd press: Hides window.
- **Enter Key**:
  - Hides window, pastes selected item via `enigo`.

---

## Code Quality Rules

1. **Verify Rust build and tests**: Always run `cargo check` and `cargo test`.
2. **Keep RAM lean**: Ensure `winit-software` renderer stays default to keep private memory ~10 MB.
3. **No npm/Node drift**: Project is pure Rust, do not generate `package.json` or `node_modules`.
