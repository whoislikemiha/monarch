# MON-44: Keybindings Settings Panel — Research Plan

## Summary

Monarch has 20+ keybindings defined inline across ~8 components (App.svelte, ChatInput, SpawnDialog, PromptEditor, ProjectEditor, ExtensionDialog, TabBar, Sidebar). There is no central registry — each component has its own `onkeydown` handler with hardcoded key combos. The Settings dialog already has a "Keybindings" tab (placeholder only). This plan designs a keybinding registry that all components read from, a settings UI that displays/edits bindings, and a persistence layer using the existing `ui_state` DB table.

## Relevant files and areas

| File | What lives here | Why it matters |
|------|----------------|---------------|
| `src/lib/SettingsDialog.svelte` | Settings dialog with 4 tabs, Keybindings tab is placeholder (line 114) | UI target — the keybindings panel goes here |
| `src/App.svelte:513-599` | Global keydown handler: Ctrl+N, Ctrl+,, Ctrl+B, Ctrl+=/-, Ctrl+0, Ctrl+1-9, /, i, Esc, Ctrl+C | Biggest source of changeable keybindings |
| `src/lib/ChatInput.svelte:19-24` | Enter to send, Shift+Enter for newline | Context-specific bindings |
| `src/lib/SpawnDialog.svelte:241-332` | Ctrl+Enter to confirm, Arrow keys for model dropdown, Escape | Dialog-scoped bindings |
| `src/lib/PromptEditor.svelte:135-144` | Ctrl+S save, Escape close | Dialog-scoped bindings |
| `src/lib/ProjectEditor.svelte:118-127` | Ctrl+S save, Escape close, Enter confirm rename | Dialog-scoped bindings |
| `src/lib/ExtensionDialog.svelte:38-44` | Escape cancel, Enter submit | Dialog-scoped bindings |
| `src/lib/TabBar.svelte:43` | Enter to select tab | Navigation bindings |
| `src/lib/Sidebar.svelte:138` | Enter to select item | Navigation bindings |
| `src-tauri/src/db.rs:234-238, 1092-1115` | `ui_state` table + `db_get_ui_state`/`db_set_ui_state` commands | Persistence layer for custom bindings |
| `src/lib/api.ts` | IPC abstraction | All invoke calls go through here |

## What needs to change

### 1. Keybinding registry module (new file: `src/lib/keybindings.ts`)

A central registry that defines every app-specific shortcut. Each entry has:
- `id` — stable string key (e.g. `"global.spawn-agent"`)
- `label` — human-readable action name
- `group` — display group (Global, Navigation, Chat)
- `defaultKeys` — the default key combo (e.g. `"Ctrl+N"`)
- `editable` — whether the user can rebind it (zoom shortcuts are `false`)
- `hint` — optional context note (e.g. `"when not in input"`)

**Omit truly universal bindings** — Enter to send, Shift+Enter for newline, Escape to close dialogs, arrow keys in dropdowns. These are so standard they'd just be noise.

**Bindings to include in the registry:**
- Ctrl+N (spawn agent) — editable
- Ctrl+, (settings) — editable
- Ctrl+B (toggle sidebar) — editable
- Ctrl+=/- and Ctrl+0 (zoom — non-editable, display only)
- Ctrl+Scroll (zoom — non-editable, display only)
- Ctrl+1-9 (switch to tab N — each individually rebindable)
- Ctrl+Tab (switch to recent agent — NEW, editable)
- Next agent tab shortcut (NEW, editable)
- / and i (focus chat input — editable, "when not in input" hint)
- Ctrl+C when no selection (abort agent) — editable

The registry exports a reactive map of `id → currentKeys` that components read from. On app startup, it loads overrides from `ui_state` (key: `"keybindings"`, value: JSON map of `id → customKeys`). Components check the registry instead of hardcoding keys.

### 2. Keybinding matching utility

A helper function that takes a `KeyboardEvent` and a binding string (e.g. `"Ctrl+Shift+N"`) and returns whether they match. This replaces the scattered `e.ctrlKey && e.key === "n"` checks across components. Needs to normalize platform differences (Ctrl vs Meta on macOS — though Monarch is Linux-first, worth getting right early).

### 3. Migrate components to use the registry

Each component's keydown handler needs to switch from hardcoded checks to calling the registry's match function. For example, `App.svelte` line 520's `if (e.ctrlKey && (e.key === "n" || e.key === "N"))` becomes `if (matchBinding(e, bindings.get("global.spawn-agent")))`.

This is the biggest surface area change — touches all 8+ components.

### 4. Keybindings tab UI in SettingsDialog

Replace the placeholder with a scrollable list grouped by context. Each row shows:
- Action label
- Current key combo (as styled `<kbd>` badges)
- Edit button (for editable bindings) or a lock/info icon (for non-editable ones)

Clicking edit enters a "capture mode" — listens for the next keypress combo and writes it back to the registry + persists to DB.

A "Reset all to defaults" button at the bottom clears all overrides.

### 5. Persistence via `ui_state`

Store all custom overrides as a single JSON object under `ui_state` key `"keybindings"`. Shape: `{ "global.spawn-agent": "Ctrl+Shift+N", ... }`. Only stores overrides — missing keys use defaults. On load, merge overrides onto defaults. On reset, delete the key.

### 6. Non-editable bindings display

Zoom bindings (Ctrl+Plus, Ctrl+Minus, Ctrl+0, Ctrl+Scroll) are marked `editable: false` in the registry. The UI shows them with a distinct style (dimmed edit area, lock icon or "System" badge) and no capture flow. They still appear in the list so users know they exist.

## Resolved decisions

1. **Omit only truly universal bindings:** Enter-to-send, Esc-to-close, Shift+Enter-for-newline, arrow keys in dropdowns — too obvious, just noise. Everything else (including `/` and `i` for focus chat) appears in the panel. `/` and `i` are editable — they're app-specific and not discoverable without the panel.

2. **Tab switching = individually rebindable:** Ctrl+1 through Ctrl+9 are each their own binding. Additionally, add two new shortcuts:
   - **Ctrl+Tab** — switch to most recent agent (Alt+Tab behavior for agent windows)
   - **Ctrl+Shift+Tab** (or similar) — next agent tab

3. **Context hints:** Show `/` and `i` as Global shortcuts with a "(when not in input)" hint in the UI.

4. **Platform-aware key display:** Show `Ctrl` on Linux/Windows, `Cmd` on macOS.

## Open questions

_(None — ready for implementation)_

## Out of scope

- Conflict detection when two actions share the same key combo
- Import/export keybinding profiles
- Per-agent keybinding overrides
- Tauri-level global shortcuts / system tray hotkeys (all current shortcuts are web-layer `onkeydown`)
- macOS `Cmd` key mapping (can be a follow-up if Monarch ships cross-platform)
