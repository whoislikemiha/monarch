# Monarch

Multi-agent desktop command center for running a fleet of AI coding agents (shadows). Built on Tauri v2, Svelte 5, Rust, and SQLite, with a long-lived Node sidecar that hosts in-memory Pi SDK sessions. Monarch owns agent identity, persistence, and session history; Pi is the execution engine.

See [VISION.md](./VISION.md) for the broader concept.

## Prerequisites

- **Node.js** (v18+) and npm.
- **Rust toolchain** — install via [rustup](https://rustup.rs/):
  - Linux/macOS: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
  - Windows: download and run [`rustup-init.exe`](https://win.rustup.rs/).
  - After install, restart your shell and verify with `rustc --version`.
- **Tauri v2 system dependencies** — see the [official prerequisites guide](https://v2.tauri.app/start/prerequisites/):
  - Linux: WebKitGTK, libappindicator, etc. (`apt`/`dnf`/`pacman` packages).
  - macOS: Xcode Command Line Tools (`xcode-select --install`).
  - Windows: Microsoft C++ Build Tools and the WebView2 runtime.

## Running in dev

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
