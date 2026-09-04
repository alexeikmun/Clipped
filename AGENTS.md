# AGENTS.md

Instructions and guidelines for AI agents working in the Clipped repository.

## Project Overview

Clipped is a lightweight, cross-platform desktop clipboard manager built with **Tauri 2.0** (Rust) and **React 19 + TypeScript** (Vite).

- **Global Hotkey**: `Ctrl+Alt+Shift+.` (configurable in settings).
- **Behavior**: Opens centered floating modal, tracks clipboard history in background thread, smart-truncates oldest non-favorites (max 999), allows searching and direct pasting into active apps via simulated keystrokes.

---

## Tooling & Package Management

- **Package Manager**: Always use `pnpm`. Never use `npm` or `yarn`.
- **Node Version**: 18+ LTS.
- **Rust Toolchain**: 2021 edition.

### Common Commands

```bash
# Install frontend dependencies
pnpm install

# Run frontend development server
pnpm run dev

# Run Tauri desktop app in dev mode
pnpm tauri dev

# Typecheck and build frontend
pnpm run build

# Verify Rust backend compilation
cargo check --manifest-path src-tauri/Cargo.toml

# Build release installer (bundles NSIS installer and copies to releases/download/)
pnpm run build:release

# Update Rust dependencies
cargo update --manifest-path src-tauri/Cargo.toml
```

---

## Technology Stack

### Frontend
- **Framework**: React 19, TypeScript 7.x, Vite 8.x.
- **Styling**: Vanilla CSS (`src/App.css`) with CSS custom properties (`--bg-app`, `--accent`, etc.). No Tailwind or CSS-in-JS.
- **State Management**: Native React hooks (`useState`, `useEffect`, `useRef`, `useMemo`). No Redux or Zustand.
- **Syntax Highlighting**: `react-syntax-highlighter/dist/esm/prism-light` (`PrismLight`). Only register required languages (`json`, `bash`, `typescript`) to avoid bundle bloat.

### Backend (Rust / Tauri 2.0)
- `tauri` (v2, feature: `tray-icon`)
- `tauri-plugin-global-shortcut`: Global hotkey registration.
- `tauri-plugin-autostart`: Launch on system boot.
- `tauri-plugin-opener`: Open URLs / external paths.
- `tauri-plugin-clipboard-manager`: Tauri clipboard APIs.
- `arboard`: Fast native clipboard read/write.
- `enigo`: Simulates `Ctrl+V` (or `Cmd+V` on macOS) to paste text into target windows.
- `serde` / `serde_json`: Serialization for settings and history.
- `uuid`: Clip ID generation.

---

## Key Architecture & Patterns

### 1. Tauri IPC & Capabilities
- Rust commands are declared in `src-tauri/src/lib.rs` and registered in `tauri::generate_handler![]`.
- All exposed commands and capabilities must be declared in `src-tauri/capabilities/default.json`.
- Event flow:
  - Rust emits: `window.emit("event-name", payload)`
  - React listens: `const unlisten = await listen("event-name", handler)`
  - Always clean up event listeners on unmount (`unlisten()`).

### 2. State Synchronization & Stale Closures
- To prevent stale closures in global `window` event listeners and Tauri event callbacks, maintain `stateRef`:
  ```tsx
  const stateRef = useRef({ ...stateValues });
  stateRef.current = { ...stateValues };
  ```
- Listeners should read mutable values directly from `stateRef.current`.

### 3. Modal & Keyboard Flow
- **Auto-hide on blur**: Handled in Rust via `WindowEvent::Focused(false)` (clicking outside hides the modal).
- **Search input toggle**:
  - Starts hidden (`isSearchVisible: false`).
  - Typing any printable character sets `isSearchVisible: true`, enters the character, and focuses the input via `useEffect([isSearchVisible])`.
  - Deleting all text via Backspace/Delete sets `isSearchVisible: false` and returns to single card view.
- **Escape Key**:
  - 1st press: Clears search query and hides search input.
  - 2nd press: Hides window via `hide_app` invoke.
- **Enter Key**:
  - Invokes `paste_item`, hides window, pastes via `enigo`.

### 4. Build & Release Scripts
- `scripts/copy-release.js` copies newly generated `.exe` installers from `src-tauri/target/release/bundle/nsis/` to `releases/download/`.
- It filters strictly by `_${currentVersion}_` read from `package.json` to avoid copying stale builds.
- `pnpm-workspace.yaml` approves `esbuild` build scripts for pnpm 12.

---

## Code Quality Rules

1. **Verify both sides after changes**: Always run `pnpm run build` and `cargo check --manifest-path src-tauri/Cargo.toml`.
2. **Keep bundle lean**: Do not import full syntax highlighter packages or unnecessary npm libraries. Keep bundle under 500 kB.
3. **No lockfile drift**: Never run `npm install` or generate `package-lock.json`.
