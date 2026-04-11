# MON-43: Theme System Research Plan

## Summary

Monarch has a single dark-purple color scheme. CSS variables in `global.css` define ~20 base tokens (backgrounds, borders, text, accents, semantic status colors), but ~19 additional hex values and 60+ `rgba()` calls are hardcoded directly in component styles across all 21 components. The Settings dialog already has an `Appearance` tab placeholder with no content, and the `ui_state` key-value table in SQLite can persist the selected theme without schema changes.

The goal is to introduce a proper theme system where all colors flow through CSS custom properties, themes are defined as standalone palette objects, and switching themes swaps the full variable set on `:root`. Four presets ship initially: Purple (current default), Dark Grey, Dark Blue, and Light. The system must be extensible — adding a theme means adding a palette definition, not touching component code.

## Relevant Files and Areas

### Core styling
- **`src/global.css`** — `:root` CSS variable definitions (lines 1-28). This is where theme variables are currently declared. Will become the anchor point for theme application.
- **`src/index.html`** — Imports `global.css` (line 7). May need a `data-theme` attribute on `<html>` or `<body>` for theme class scoping.

### Settings infrastructure
- **`src/lib/SettingsDialog.svelte`** — Settings dialog with four category tabs. Appearance tab exists (line 10) but renders "No settings configured yet" (line 52). Theme selector UI goes here.
- **`src/App.svelte`** — Uses `db_get_ui_state`/`db_set_ui_state` for persisting UI preferences (lines 188-215). Theme restore-on-launch logic goes here.

### Persistence
- **`src-tauri/src/db.rs`** — `ui_state` table (line 234-238), `db_get_ui_state` command (line 1092), `db_set_ui_state` command (line 1108). No schema changes needed — theme preference stores as `{ key: "theme", value: "purple" }`.

### IPC layer
- **`src/lib/api.ts`** — All `invoke`/`listen` calls route through here. Already wraps `db_get_ui_state` and `db_set_ui_state`.

### Components with hardcoded colors (need CSS variable migration)
- **`src/lib/Sidebar.svelte`** — `#e2d4ff` highlight colors (lines 255, 291, 329, 333, 391)
- **`src/lib/ChatInput.svelte`** — `#140d22`, `#d5bbff` button colors (lines 96, 109)
- **`src/lib/AgentControls.svelte`** — Multiple `rgba()` overlays with hardcoded base colors (lines 237-313)
- **`src/lib/ToolCallCard.svelte`** — Inline color map for status colors (lines 90-92), hardcoded hex (lines 237, 256)
- **`src/lib/ToolGroup.svelte`** — `#ffb4b4`, `#ff8a8a` error highlights
- **`src/lib/ExtensionDialog.svelte`** — `#190f24`, `#d5bbff` button colors (lines 259, 268)
- **`src/lib/PromptEditor.svelte`** — Similar button color pattern (line 292)
- **`src/lib/ProjectEditor.svelte`** — Orange/error highlights (lines 316-317, 343)
- **`src/lib/TabBar.svelte`** — Background and border hardcodes
- **`src/lib/AgentView.svelte`** — Multiple background tiers hardcoded
- **`src/lib/MessageList.svelte`** — `#33b1ff` accent usage
- **`src/lib/HistoryPanel.svelte`** — Panel backgrounds
- **`src/lib/SpawnDialog.svelte`** — Dialog styling
- **`src/lib/PlaceholderTool.svelte`** — Error color hardcodes
- **`src/lib/AgentStatusDot.svelte`** — Status indicator colors
- **All modal/dialog components** — `rgba(0, 0, 0, 0.6)` backdrop

## What Needs to Change

### 1. Theme definition module (`src/lib/themes.ts` — new file)
Define a `Theme` interface that enumerates every CSS variable the app uses. Each preset theme is an object conforming to this interface. A single `applyTheme(themeName)` function sets all variables on `document.documentElement.style`. This is the only place palette values live — components never reference raw colors.

The variable set should be expanded beyond the current ~20 to cover all the hardcoded colors discovered: button text on accent backgrounds, hover/active state variations, overlay opacities, modal backdrops, status color variants at different opacity levels, etc.

### 2. CSS variable expansion (`src/global.css`)
Expand the `:root` variable set to cover every color currently hardcoded in components. Group into semantic categories:
- **Surfaces**: app background, sidebar, panels (3 tiers), cards, modal backdrop
- **Borders**: subtle, strong, focus ring
- **Text**: primary, secondary, muted, on-accent (text on colored backgrounds)
- **Accent**: primary accent, hover, active, muted (low opacity)
- **Status**: success, warning, error — each with base, text, and background-tint variants
- **Interactive**: button bg, button hover, button text, input bg, input border

The default values in `global.css` become the Purple theme fallback.

### 3. Hardcoded color migration (all 21 components)
Systematically replace every hardcoded `#hex` and `rgba()` color in component `<style>` blocks with the corresponding CSS variable. This is the bulk of the work. Each component needs an audit-and-replace pass. Inline styles in `<script>` logic (like the ToolCallCard color map) need to reference CSS variable values or use utility classes.

### 4. Appearance tab UI (`src/lib/SettingsDialog.svelte`)
Build a theme picker in the Appearance category: a grid of theme cards showing a small preview swatch for each preset. Clicking a card applies the theme immediately (live preview) and persists the selection. Highlight the active theme.

### 5. Theme persistence and restore (`src/App.svelte`)
On app launch, read `ui_state.theme` and call `applyTheme()` before the first paint to avoid flash-of-wrong-theme. On theme change in Settings, write to `ui_state` via `db_set_ui_state`.

### 6. Theme palette design (4 presets)
Design the actual color palettes:
- **Purple** (current) — existing colors, just moved into the theme object
- **Obsidian** (dark grey) — neutral grey palette, no purple tint, subtle blue-grey accents
- **Midnight** (dark blue) — deep navy backgrounds, steel blue accents
- **Light** — light backgrounds, dark text, adjusted contrast for all semantic colors

## Open Questions

1. **Flash prevention strategy**: Should the theme be applied via a blocking `<script>` in `index.html` that reads from localStorage (fastest, avoids FOUC) or should it go through the Tauri `db_get_ui_state` path (consistent with existing pattern but async, may flash)? A hybrid approach — mirror the theme key to localStorage for instant apply, persist canonical value in SQLite — might be best.

2. **`rgba()` handling**: Many components use `rgba(r, g, b, alpha)` with hardcoded RGB values. CSS variables don't compose well with `rgba()` unless the variable stores just the RGB triplet (e.g., `--accent-purple-rgb: 190, 149, 255` then `rgba(var(--accent-purple-rgb), 0.12)`). Alternatively, define explicit opacity-variant variables for each needed level (`--accent-purple-12`, `--accent-purple-40`). Which approach do you prefer? RGB triplets are more flexible but slightly less readable; explicit variants are clearer but mean more variables.

3. **Inline style colors in JS**: `ToolCallCard.svelte` has a JS-side color map (line ~90) that picks colors based on tool status. Should this use `getComputedStyle` to read CSS variables at runtime, or should it reference a theme-aware utility/store?

4. **Light theme scope**: The light theme requires inverting nearly everything — text goes dark, backgrounds go light, borders change direction (lighter borders on light bg), shadows may need to appear/change. Some components may need minor structural adjustments (e.g., shadows that are invisible on dark but needed on light). Is a "good enough" light theme acceptable for v1, or does it need to be polished?

## Out of Scope
- Custom user-defined themes or color picker
- Per-agent theming
- Font size, typography, or spacing settings
- Accent color customization independent of theme preset
- System theme auto-detection (OS dark/light follow)
- Theme import/export
