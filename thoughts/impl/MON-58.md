# MON-58: Avatar Placement — Sidebar and Agent Header

## What was implemented

ShadowAvatar component integrated into two UI locations: sidebar agent list (32px) and agent detail header (40px). Each avatar loads the .riv file, auto-detects the first available state machine, and drives inputs from agent state.

## Key decisions

- **@rive-app/canvas over webgl2** — WebGL2 creates one context per canvas. With 13+ agents in the sidebar, browsers hit the ~16 context limit and nothing renders. Canvas 2D has no such limit and uses the same Rive API.
- **40px header avatar instead of 96px** — The header is a compact bar. 96px would dominate it. 40px fits naturally next to name/title.
- **Auto-detect state machine name** — Instead of hardcoding, reads `stateMachineNames[0]` on load. Resilient to .riv renames.
- **publicDir: "static"** — Vite defaults to `public/` but our assets are in `static/`. Added to vite.config.ts.
- **Avatars invisible until .riv has artboard content** — The integration is complete but the current .riv has no visible content on its artboard. Once rebuilt with content, avatars will render automatically.

## Files touched

- `src/lib/avatar/ShadowAvatar.svelte` — switched to canvas renderer, auto-detect SM, removed useOffscreenRenderer prop
- `src/lib/Sidebar.svelte` — added 32px ShadowAvatar per agent
- `src/lib/AgentHeader.svelte` — added 40px ShadowAvatar in header
- `vite.config.ts` — added publicDir: "static"
- `package.json` — added @rive-app/canvas dependency

## What was left out

- .riv content (ball not on artboard — MON-57 needs rebuild)
- Interactive hover/click (MON-59)
- War Room (MON-61)
- Visual progression (MON-60)
