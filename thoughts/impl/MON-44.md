# MON-44: Keybindings Settings Panel — Implementation Notes

## What was implemented

Central keybinding registry with a settings UI that lets users view all shortcuts and rebind editable ones. Inline capture flow (no fullscreen overlay). Two new navigation shortcuts: Ctrl+Tab (recent agent) and Ctrl+PageDown (next agent).

## Key decisions

- **Registry is a `.svelte.ts` module** with module-level `$state` for overrides — singleton store shared across all consumers. Components call `matchBinding(event, id)` instead of hardcoded key checks.
- **Tab history is a plain array, not `$state`** — using `$state` caused an infinite `$effect` loop since the effect both reads and writes the history. Since it's only read imperatively in `switchToRecentAgent()`, reactivity isn't needed.
- **Zoom stays hardcoded** — non-editable and has the `=`/`+` ambiguity that needs special handling. Registry includes zoom entries for display only.
- **Inline capture box** instead of fullscreen overlay — row highlights with accent background, focused div captures the next keypress. Blur cancels capture.
- **Platform-aware display** — `Ctrl` on Linux, `Cmd` on macOS. Matching logic uses `e.ctrlKey` on Linux, `e.metaKey` on macOS.

## Files touched

- `src/lib/keybindings.svelte.ts` — new, registry + matching + persistence + display formatting
- `src/lib/KeybindingsSettings.svelte` — new, settings tab UI with grouped list and inline capture
- `src/lib/SettingsDialog.svelte` — wired up KeybindingsSettings for the Keybindings tab
- `src/App.svelte` — migrated all keydown handlers to use registry, added tab history + new shortcuts
- `src/lib/SpawnDialog.svelte` — migrated Ctrl+Enter confirm to registry

## What was left out

- Conflict detection (two bindings sharing the same key combo)
- Import/export keybinding profiles
- Per-agent keybinding overrides
