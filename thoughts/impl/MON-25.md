# MON-25: Implementation Notes — Zoom Support

## What was built

App-wide zoom via Tauri's native `WebviewWindow::set_zoom()` API, controlled by keyboard shortcuts, scroll wheel, and a settings UI control.

## Architecture decisions

- **Native webview zoom over CSS zoom** — `set_zoom()` scales everything uniformly including scrollbars. CSS `zoom` or `transform: scale()` would've required layout workarounds.
- **No new Rust command for reading zoom** — reused existing `db_get_ui_state`/`db_set_ui_state` for persistence. The Rust `set_zoom` command only wraps the webview API and returns the clamped value; persistence is handled by the frontend after a successful call.
- **Browser mode is a no-op** — `applyZoom()` catches the invoke error silently. Browser has its own zoom.
- **Unified 5% step** — keyboard, scroll, and settings buttons all use 0.05 increments. Originally keyboard was 10% and scroll was 5%, but mismatched steps meant the settings buttons couldn't land on scroll-produced values.

## Files changed

| File | Change |
|------|--------|
| `src-tauri/src/zoom.rs` | New module — `set_zoom` command wrapping `WebviewWindow::set_zoom()` with 50%–200% clamping |
| `src-tauri/src/lib.rs` | Registered `zoom` module and command |
| `src-tauri/capabilities/default.json` | Added `core:webview:allow-set-webview-zoom` permission |
| `src/App.svelte` | Zoom state, keyboard handlers (Ctrl+=/-/0), wheel handler, restore on mount, props to SettingsDialog |
| `src/lib/SettingsDialog.svelte` | Zoom control in Appearance tab (below theme picker from MON-43) |
| `src/lib/toolbox/tools/PlaceholderTool.svelte` | Removed font-size mismatch (`.value.mono` was 10px vs row's 11px) |
| `src/lib/bindings.ts` | Regenerated with `setZoom` binding |

## Resolved open questions

1. **Native vs CSS** → Native. User preference for explicit Rust control.
2. **Browser fallback** → No-op. Browser has its own zoom.
3. **Tauri default hotkeys** → Disabled by default (`zoomHotkeysEnabled` is false). No conflicts.
