# MON-43: Theme System Research Plan

## Summary

Monarch has a single dark-purple color scheme. CSS variables in `global.css` define ~20 base tokens, but ~19 additional hex values and 60+ `rgba()` calls are hardcoded in component styles across all 21 components. The Settings dialog has an Appearance tab placeholder with no content, and the `ui_state` key-value table in SQLite can persist the selected theme.

The goal: a typed theme system where each theme is a standalone TS file exporting an object that conforms to a shared `Theme` interface. An `applyTheme()` function maps the object to CSS custom properties on `:root`. Components only reference `var(--token)` — they never import themes or know colors. Adding a theme = create a new file, conform to the interface, register it. This architecture also supports user-created themes in the future (same interface, constructed at runtime).

Four presets ship initially: Purple (current default), Obsidian (dark grey), Midnight (dark blue), and Light.

## Resolved Design Decisions

1. **Architecture: Typed TS objects → CSS variables.** Each theme is a TS file exporting a typed object. `applyTheme()` loops the object and sets CSS variables on `:root`. Components consume `var(--token)` in their `<style>` blocks. TypeScript enforces that every theme defines every token — missing colors are compile-time errors.

2. **One file per theme.** Themes live in `src/lib/themes/` — `purple.ts`, `obsidian.ts`, `midnight.ts`, `light.ts`, plus `index.ts` (interface, registry, apply function). Clean to find, clean to edit.

3. **No flash on startup.** Read theme from `ui_state` DB on launch, apply before mount. The default theme's values also live in `global.css` `:root` as fallback so there's never a frameless moment — the CSS file loads synchronously with the HTML.

4. **rgba() handling.** Define explicit semantic tokens for every opacity variant needed (e.g., `--accent-bg-subtle`, `--accent-bg-hover`, `--status-error-bg`). Each theme defines the full resolved color including opacity. More tokens, but each is self-documenting and type-checked. No RGB triplet hacks.

5. **JS-side color access.** Components that need colors in JS (like ToolCallCard's status color map) import the active theme object directly from the theme module — no `getComputedStyle` needed. The theme module exports both the apply function and the current theme reference.

## Relevant Files and Areas

### Core styling
- **`src/global.css`** — Current `:root` CSS variable definitions (lines 1-28). Default theme fallback values stay here.
- **`src/index.html`** — Imports `global.css` (line 7).

### Settings infrastructure
- **`src/lib/SettingsDialog.svelte`** — Appearance tab exists (line 10) but empty. Theme picker UI goes here.
- **`src/App.svelte`** — Uses `db_get_ui_state`/`db_set_ui_state` for UI preferences (lines 188-215). Theme restore-on-launch goes here.

### Persistence
- **`src-tauri/src/db.rs`** — `ui_state` table (line 234-238). No schema changes — stores `{ key: "theme", value: "purple" }`.

### IPC layer
- **`src/lib/api.ts`** — Already wraps `db_get_ui_state` and `db_set_ui_state`.

### Components with hardcoded colors (need CSS variable migration)
- **`src/lib/Sidebar.svelte`** — `#e2d4ff` highlight colors
- **`src/lib/ChatInput.svelte`** — `#140d22`, `#d5bbff` button colors
- **`src/lib/AgentControls.svelte`** — Multiple `rgba()` overlays
- **`src/lib/ToolCallCard.svelte`** — Inline JS color map for status, hardcoded hex
- **`src/lib/ToolGroup.svelte`** — `#ffb4b4`, `#ff8a8a` error highlights
- **`src/lib/ExtensionDialog.svelte`** — Button colors
- **`src/lib/PromptEditor.svelte`** — Button color pattern
- **`src/lib/ProjectEditor.svelte`** — Error highlights
- **`src/lib/TabBar.svelte`** — Background and border hardcodes
- **`src/lib/AgentView.svelte`** — Multiple background tiers
- **`src/lib/MessageList.svelte`** — Accent usage
- **`src/lib/HistoryPanel.svelte`** — Panel backgrounds
- **`src/lib/SpawnDialog.svelte`** — Dialog styling
- **`src/lib/PlaceholderTool.svelte`** — Error color hardcodes
- **`src/lib/AgentStatusDot.svelte`** — Status indicator colors
- **All modal/dialog components** — `rgba(0, 0, 0, 0.6)` backdrop

## What Needs to Change

### 1. Theme infrastructure (`src/lib/themes/` — new directory)
- **`types.ts`** — `Theme` interface with every semantic token grouped by category (surfaces, borders, text, accent, status, interactive, overlays).
- **`purple.ts`** — Current colors extracted into a Theme object (default).
- **`obsidian.ts`** — Neutral dark grey palette.
- **`midnight.ts`** — Deep navy/steel blue palette.
- **`light.ts`** — Light backgrounds, dark text, full inversion.
- **`index.ts`** — Theme registry (name → theme object map), `applyTheme(name)` function that loops the object and sets CSS variables on `:root`, exports current active theme for JS-side access.

### 2. CSS variable expansion (`src/global.css`)
Expand `:root` to cover every semantic token from the Theme interface. Group into:
- **Surfaces**: app bg, app glow, sidebar, panels (3 tiers), card, modal backdrop
- **Borders**: subtle, strong, focus
- **Text**: primary, secondary, muted, on-accent
- **Accent**: base, hover, active, subtle bg, subtle hover
- **Status**: success/warning/error — each with base, text, and bg-tint
- **Interactive**: button bg, button hover, button text, input bg, input border
- **Overlays**: backdrop, hover overlay, active overlay

Default values = Purple theme (fallback before JS runs).

### 3. Hardcoded color migration (all 21 components)
Replace every hardcoded `#hex` and `rgba()` in component `<style>` blocks with `var(--token)`. Inline JS color maps (ToolCallCard) switch to importing from the theme module. This is the bulk of the work.

### 4. Appearance tab UI (`src/lib/SettingsDialog.svelte`)
Theme picker grid in the Appearance category — cards showing preview swatches for each preset. Click applies immediately and persists. Active theme highlighted.

### 5. Theme persistence and restore (`src/App.svelte`)
On launch: read `ui_state.theme`, call `applyTheme()` before component mount. On change: write via `db_set_ui_state`.

## Out of Scope
- Custom user-defined themes or color picker (architecture supports it for later)
- Per-agent theming
- Font size, typography, or spacing settings
- Accent color customization independent of theme preset
- System theme auto-detection (OS dark/light follow)
- Theme import/export
