# MON-25: Add zoom in/out support (Ctrl+Plus, Ctrl+Minus, Ctrl+Scroll)

## Summary

App-wide UI zoom scaling, mirroring the VS Code pattern: Ctrl+Plus zooms in, Ctrl+Minus zooms out, Ctrl+0 resets, and Ctrl+Scroll zooms incrementally. The zoom level persists across app restarts via the existing `ui_state` key-value table. Two implementation paths are possible — Tauri v2's native `WebviewWindow.setZoom()` (cleanest, scales everything including scrollbars) or CSS `zoom` property on the root element (simpler, frontend-only, but doesn't scale native scrollbars). The native path is recommended.

## Relevant files and areas

| File | What lives there | Why it matters |
|------|------------------|----------------|
| `src/App.svelte` (L477–546) | Centralized `handleKeydown()` on `<svelte:window>` | All keyboard shortcuts live here — zoom shortcuts (Ctrl+Plus/Minus/0) get added to this handler, before the `inInput` guard since zoom should always work |
| `src/App.svelte` (L96–170) | `onMount` — restore flow, UI state loading | Zoom level restore on startup goes here, alongside existing `ui_state` restores (sidebar width, open tabs) |
| `src/lib/api.ts` | Unified IPC wrapper (Tauri vs browser fallback) | Zoom commands route through `invoke()` from here. Browser fallback needs consideration — CSS zoom fallback when not in Tauri? |
| `src-tauri/src/db.rs` (L1088–1115) | `db_get_ui_state` / `db_set_ui_state` commands | Already exists, no schema changes needed. Store zoom as `ui_state` key `"zoomLevel"` with string value like `"1.25"` |
| `src-tauri/src/lib.rs` | `tauri::generate_handler![]` command registration | New zoom command gets registered here |
| `src-tauri/tauri.conf.json` | Window and webview configuration | May need to check if `zoomHotkeysEnabled` or similar defaults interfere |
| `src/lib/SettingsDialog.svelte` | Settings dialog with "Appearance" category (placeholder) | Out of scope per ticket, but the Appearance category is already there for future zoom UI |

## What needs to change

### 1. Rust: zoom command (small)
Add a Tauri command — something like `set_zoom_level` — that calls `webview_window.set_zoom(scale_factor)` on the current webview. This is a thin wrapper. Alternatively, check if Tauri v2's JS API (`@tauri-apps/api/webviewWindow`) exposes `setZoom()` directly — if so, no Rust command needed at all and the entire feature stays in the frontend.

### 2. Frontend: keyboard + scroll handlers (core of the change)
Extend `handleKeydown()` in `App.svelte` to intercept:
- `Ctrl+=` / `Ctrl+Shift+=` (zoom in — note: `+` is `Shift+=` on most keyboards, so match on `=` too)
- `Ctrl+-` (zoom out)
- `Ctrl+0` (reset to 1.0)

These go **before** the `inInput || inDialog` guard — zoom should work regardless of focus.

Add a wheel event listener (`<svelte:window>`) for `Ctrl+Scroll` — deltaY controls direction.

### 3. Frontend: zoom state + persistence
- Track current zoom level as `$state` (default `1.0`)
- On zoom change: call the zoom API (Tauri native or CSS fallback), then persist to `ui_state` via `db_set_ui_state("zoomLevel", ...)`
- On mount: read `db_get_ui_state("zoomLevel")` and apply it. This fits alongside existing `onMount` restores

### 4. Bounds + step size
- Define min (0.5) and max (2.0) zoom bounds
- Keyboard step: 0.1 per press (matching VS Code's behavior)
- Scroll step: 0.05 per tick (finer control)
- Clamp to bounds on every change

### 5. Browser fallback (if needed)
When running in browser mode (WebSocket fallback, no Tauri runtime), `WebviewWindow.setZoom()` won't exist. Two options:
- Apply CSS `zoom` on `document.documentElement` as a fallback
- Or simply skip zoom in browser mode (it's a desktop app feature)

## Open questions

1. **Native API vs CSS zoom?** Tauri v2's `WebviewWindow` has `set_zoom(scale_factor)` in Rust. Does `@tauri-apps/api/webviewWindow` expose it on the JS side too? If yes, no Rust command needed — the entire feature is frontend-only. Need to verify the JS API surface.

2. **Browser fallback behavior?** When running in browser mode (dev without Tauri), should zoom work via CSS `zoom` property, or should it be a no-op? CSS zoom is easy to add but the ticket says "app-wide" which implies the desktop app context.

3. **Default zoom from Tauri config?** Tauri may have its own default zoom handling or hotkey interception. Need to check if Tauri v2 already handles Ctrl+Plus/Minus at the webview level and whether we need to disable that to avoid double-zoom.

## Out of scope

- Zoom level slider/control in the Settings dialog (explicitly excluded in the ticket)
- Per-panel zoom — this is app-wide only
- Zoom level indicator in the status bar or elsewhere
