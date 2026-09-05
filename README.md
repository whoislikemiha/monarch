# Monarch

Monarch is a desktop app for running and supervising multiple AI coding agents.

This is an experimental project created in order to explore topics like:
- Spec driven development and agent loops.
- Modern LLMs using a compiled type safe language like Rust to reduce code errors and bugs.
- Separating execution and conversation.
- Execution narration for better overview of what the agent is doing at all times.
- Deeper active context insights and manipulation.
- Persistent agent memory via local index where everything is local.
- Session ancestry and forking.  


![Monarch](docs/screenshot.png)


## Architecture

```
┌──────────────┐   Tauri commands    ┌────────────────┐   JSONL over stdio   ┌─────────────────┐
│   Svelte 5   │ ◄─────────────────► │  Rust (Tauri)  │ ◄──────────────────► │  Node sidecar   │
│   frontend   │                     │      core      │                      │  (Pi SDK host)  │
└──────────────┘                     └────────┬───────┘                      └─────────────────┘
                                              │
                                            SQLite 
```

- **Rust owns state.** SQLite is canonical; the sidecar is a stateless-on-restart execution engine, not a session authority.
- **The frontend only displays state**, reconciled via versioned snapshots pushed over Tauri events (with a WebSocket fallback for browser-mode dev).
- **The sidecar is a singleton** — one Node process hosts every agent's in-memory Pi SDK session, keyed by agent ID.


## Tech stack

| Layer | Tech |
|---|---|
| Desktop shell | [Tauri v2](https://v2.tauri.app/) |
| Frontend | Svelte 5 (runes), TypeScript, Vite |
| Backend | Rust, `tokio`, `rusqlite` |
| Agent runtime | Node.js sidecar embedding [Pi SDK](https://github.com/badlogic/pi-mono) |
| Storage | SQLite (WAL), local vector index for memory search |
| IPC | Tauri commands/events (native), WebSocket bridge (browser-mode dev) |

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

## Authentication / API keys

Monarch ships without credentials — you supply your own. On startup it reads provider auth from either source:

- **Pi subscription login** — `~/.pi/agent/auth.json`, created when you log in through Pi. Covers Anthropic (Claude) and OpenAI Codex.
- **Environment variables** — set any of:
  - `ANTHROPIC_API_KEY` — Claude models.
  - `OPENAI_API_KEY` — OpenAI models.
  - `OPENROUTER_API_KEY` — models routed via OpenRouter.
  - `LMSTUDIO_BASE_URL` — local models served by [LM Studio](https://lmstudio.ai/) (defaults to `http://127.0.0.1:1234`).

Keys must be present in the environment that **launches the app** — they are inherited by the Node sidecar that talks to the providers. Copy [`.env.example`](./.env.example) to `.env` and fill in what you use, or export the variables in your shell before starting Monarch.

## First run / prerequisites

- The first `cargo build` downloads ONNX Runtime binaries — **network access is required** for that initial build.
- The working-memory feature downloads the `bge-small-en-v1.5` embedding model from HuggingFace on first use.
- `node` must be on your `PATH` at runtime — the Rust core spawns the Node sidecar as a child process.

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

> **Linux / Wayland:** if the app window comes up blank, launch it with
> `WEBKIT_DISABLE_DMABUF_RENDERER=1 GDK_BACKEND=x11 npm run tauri dev`.

## Building

```bash
npm run build          # builds sidecar + web assets
npm run tauri build    # produces the packaged desktop app
```

## Testing & type-checking

```bash
npx svelte-check        # frontend types (from repo root)
cargo check              # backend types (from src-tauri/)
npm test                 # frontend unit tests (Vitest)
cargo test                # backend tests (from src-tauri/)
```
