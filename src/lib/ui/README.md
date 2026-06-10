# Monarch UI — the design system

**Build every new surface with this. Do not copy legacy components** — most of the app predates this
system (ad-hoc scoped styles, legacy token names, the odd shadow). Legacy styles get deleted as surfaces
are rebuilt, not propagated.

Live reference (renders every token + atom, theme-switchable): run dev → **`http://localhost:1420/?catalog`**
(source: `Catalog.svelte`).

## Layers

| Layer | Where | Loaded |
|-------|-------|--------|
| **Tokens** (colors, elevation, grade ramp, spacing, radius) | `src/global.css` + `src/lib/themes/*.ts` | **Global, always.** Just use the `var(--…)`. |
| **Atoms** (component CSS classes) | `src/lib/ui/styles/atoms.css` | **Opt-in.** Import where used, or wrap in a primitive. |
| **Primitives** (typed Svelte components) | `src/lib/ui/*.svelte` | Imported per use. **Preferred over raw classes.** Built on demand. |

## How to build a surface

1. **Use tokens directly** for layout/color — they're global, no import:
   ```svelte
   <style>
     .panel { background: var(--bg-panel); border: 1px solid var(--border);
              border-radius: var(--r-lg); padding: var(--s4); gap: var(--s3); }
   </style>
   ```
2. **Use a primitive** if one exists (`import Button from "../ui/Button.svelte"` — the codebase uses
   relative imports, not the `$lib` alias).
3. **No primitive yet?** Either import the atom class…
   ```svelte
   <script>import "../ui/styles/atoms.css";</script>
   <button class="btn btn-primary">Dispatch</button>
   ```
   …or, if you'll reuse it, **add a primitive** here (wrap the atom class with typed props) and use that.

## Token cheat-sheet

- **Elevation** (depth = stacking, never shadow): `--bg-sink` < `--bg-base` < `--bg-panel` < `--bg-raised` < `--bg-overlay`
- **Text**: `--text-primary` / `--text-secondary` / `--text-muted`
- **Accent**: `--accent`, `--accent-hover`, `--accent-ink` (text on accent fill), `--accent-2` (secondary)
- **Status** (pair with shape + label, never color alone): `--status-success` / `-warning` / `-error` / `-info`
- **Borders**: `--border-subtle` < `--border` < `--border-strong`; focus ring `--focus`; modal dim `--scrim`
- **Grade ramp** (E→S rarity): `--grade-e` `--grade-d` `--grade-c` `--grade-b` `--grade-a` `--grade-s`
- **Spacing** (4px base): `--s1`(4) `--s2`(8) `--s3`(12) `--s4`(16) `--s5`(24) `--s6`(32) `--s7`(48) `--s8`(64)
- **Radius**: `--r-sm`(2) `--r-md`(4) `--r-lg`(6); `--r-full` (circle — dots/avatars only)
- **Density**: wrap narrow inspector panels in `[data-density="compact"]` to tighten row rhythm

## Atom classes (in `styles/atoms.css`)

`.panel`/`.panel-head`/`.panel-body` · `.btn`(`.btn-primary`/`-ghost`/`-danger`/`-icon`) · `.field`/`.input`/`.textarea`/`.select` ·
`.badge`(`.b-success`/`-warning`/`-error`/`-info`) · `.chip`/`.chip-scope` · `.gchip` (grade) · `.sdot`(`.idle`/`.success`/`.running`/`.warning`/`.error`) ·
`.avatar`(`.ring` + `--gc`) · `.shadow-row` · `.empty` · `.meter` · `.gprog` · `.drow`/`.drow-group` (data rows) ·
`.tree`/`.tnode`/`.trow`/`.tkids` (disclosure) · `.popover`/`.tooltip` · `.codeblock`/`.showmore` · `.evt`/`.ei` (timeline event icons) · `.caret` (streaming)

## House rules (non-negotiable)

- **No shadows / glows / blurs.** Depth = elevation + 1px border + space.
- **Restrained radius** (`--r-sm/-md/-lg`); circle only for dots/avatars. No pills/blobby cards.
- **Inter for everything a human reads as language. JetBrains Mono (`.mono`) ONLY for data** — ids, metrics, paths, timestamps, code.
- **Status never by color alone** — shape + label too.
- **Themeable only** — token `var(--…)`; never hardcode hex. Verify in the catalog across all four themes.

## Adding a token

Themed value → add to `src/lib/themes/types.ts` + all four theme files (it auto-maps `camelCase → --kebab`).
Theme-invariant / an alias onto an existing token → add to `src/global.css`. Then show it in `Catalog.svelte`.
