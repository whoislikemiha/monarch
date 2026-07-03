# MON-56: Rive Runtime Integration + ShadowAvatar Svelte Component

## What was implemented

Rive animation runtime (`@rive-app/webgl2`) integrated into the Monarch frontend with a reusable `ShadowAvatar.svelte` component and a decoupled state mapping layer. This is the foundation for the entire avatar system — subsequent tickets (MON-57 through MON-63) build on this.

## Key decisions

- **WebGL2 over Canvas** — went with `@rive-app/webgl2` for maximum rendering quality. Prior WebGL experience on the project confirms WebKitGTK handles it fine.
- **CDN WASM loading** — Rive loads its WASM renderer from unpkg by default. Works for both dev and production. Can switch to local bundle via `RuntimeLoader.setWasmUrl()` if offline support is ever needed.
- **Mutually exclusive activity states** — The state mapper enforces exactly one boolean true at a time via priority ordering: error > tool running > coding > thinking > idle. No ambiguous states possible.
- **Read vs Tool distinction** — Tools like `Read`, `Grep`, `Glob` trigger `isReading` instead of generic `isUsingTool`, giving richer animation variety.
- **"Waiting" not auto-derived** — Would need a timer-based heuristic (tool running > N seconds), which doesn't belong in a pure mapper. Left for future enhancement.
- **No placeholder .riv binary** — Can't generate valid `.riv` from CLI. Directory structure is ready; MON-57 delivers the real art.

## Files touched

- `package.json` / `package-lock.json` — added `@rive-app/webgl2@2.37.1`
- `src/lib/avatar/stateMapper.ts` — `deriveAnimationState()` and `detectTriggers()` pure functions
- `src/lib/avatar/ShadowAvatar.svelte` — canvas component with Rive lifecycle, reactive $effect() binding
- `src/lib/avatar/index.ts` — barrel export
- `static/avatars/.gitkeep` — asset directory for .riv files

## What was left out

- Placeholder `.riv` file (can't create binary from CLI — needs Rive editor)
- Vite WASM config (not needed — Rive loads from CDN)
- `isWaiting` state derivation (needs timer, out of scope for pure mapper)
- Grade/experience data wiring (MON-63 provides the stats backend)
