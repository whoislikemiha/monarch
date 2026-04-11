# MON-46: Fix theme flash (FOUC) on app startup

## What was implemented

Eliminated the purple flash that appeared on startup for users with a non-purple theme. The root cause was that `global.css` `:root` has purple theme CSS vars hardcoded, and the saved theme wasn't applied until `onMount` → `loadUiState()` → `applyTheme()` ran asynchronously.

## Key decisions

- **localStorage cache, not embedded theme map**: Rather than duplicating all theme definitions in a blocking `<script>`, `applyTheme()` caches the resolved CSS var map to localStorage. The inline script reads this cache — zero maintenance when themes are added or changed.
- **Kept `:root` purple defaults in `global.css`**: Acts as fallback for first-ever launch (no cache yet). Purple is the default theme, so first-time users see the correct theme with no flash.

## Files touched

- `index.html` — added blocking `<script>` that reads `monarch-theme-cache` from localStorage and sets CSS vars before first paint
- `src/lib/themes/index.ts` — `applyTheme()` now writes resolved CSS vars to localStorage after applying them

## Also in this PR

- Added `/linear-to-impl` slash command (`.claude/commands/linear-to-impl.md`) — streamlined `/linear-to-plan` variant that skips the research plan for straightforward changes.

## What was left out

- No migration for existing users — they'll see one final flash on the first load after this update, then the cache kicks in for subsequent launches.
