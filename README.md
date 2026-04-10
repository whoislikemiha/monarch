# Monarch

Multi-agent desktop command center for running a fleet of AI coding agents (shadows). Built on Tauri v2, Svelte 5, Rust, and SQLite, with a long-lived Node sidecar that hosts in-memory Pi SDK sessions. Monarch owns agent identity, persistence, and session history; Pi is the execution engine.

See [VISION.md](./VISION.md) for the broader concept.

## Running in dev

Prerequisites: Node.js, pnpm or npm, Rust toolchain, and the Tauri v2 [system dependencies](https://v2.tauri.app/start/prerequisites/).

```bash
# install frontend deps
npm install

# install sidecar deps
npm install --prefix sidecar

# build the Node sidecar (Tauri dev expects dist/ to exist)
npm run build:sidecar

# run the desktop app
npm run tauri dev
```

## Building

```bash
npm run build          # builds sidecar + web assets
npm run tauri build    # produces the packaged desktop app
```
