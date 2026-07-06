# Monarch

![Monarch](docs/screenshot.png)
<!-- TODO(owner): add a real screenshot or GIF of the running app at docs/screenshot.png -->

**A desktop command center for running a fleet of AI coding agents — not another terminal with tabs.**

Monarch runs several AI coding agents side by side and treats each one as something worth keeping: a clean dialogue-only chat, a separate execution timeline the agent narrates itself, a live view into its context window, long-term memory that's curated and stored locally, and session history with real ancestry. Agents can run on Claude, OpenAI Codex, OpenRouter, or fully-local models via LM Studio — mixed freely in one roster. Under the hood it's a real multi-process desktop app: a Rust/Tauri core that owns all state in SQLite, a Svelte 5 frontend that renders it, and a long-lived Node sidecar hosting the agent runtime ([Pi SDK](https://github.com/badlogic/pi-mono)).

> **Status — parked.** Monarch is an experimental project exploring multi-agent orchestration: agent memory, persistent work structure, and session ancestry. A large amount was built here; rather than leave it hidden, it's published as a reference and showcase. It is **not actively maintained and is not accepting contributions** — fork and explore freely. See [VISION.md](./VISION.md) for the original direction, [ONBOARDING.md](./ONBOARDING.md) for the architecture and data-model walkthrough, and [`thoughts/`](./thoughts/) for the design log (research plans, implementation notes, and design docs written as it evolved).

## What's different

Most agent UIs are a terminal emulator with extra steps: one scrollback per agent, tool spam interleaved with conversation, and no memory of anything once the process dies. Monarch makes a few opinionated bets instead:

### Chat and execution are separate surfaces

The chat pane is **dialogue-only**. Tool calls, thinking blocks, and grinding never render there — while an agent works, chat shows a single pulsing line with what it's currently doing, and the full mechanical record lives in the **timeline**: a chronological feed of narrated actions with their grouped tool calls, decisions, and spawned sub-chats. You read the conversation like a conversation and audit the work like a log — without either polluting the other.

The timeline isn't a harvested log dump, either. Agents **narrate their own work** through a dedicated tool ("one line of intent before each chunk of work"), and the harness — not the prompt — enforces the cadence, nudging the agent when it goes too long without narrating. Un-narrated tool calls still show up as bare rows: the honest floor, never invented headlines. Previews are truncated at record time; clicking any tool row fetches the full args/result on demand.

The timeline is interactive, too: you can open a **scoped chat on any specific action** — "why did you do it this way?" — and question the agent about that piece of work directly, without derailing the main conversation. The exchange is recorded under the action it's about.

### You can see inside the context window

A live **context inspector** breaks down exactly what's occupying an agent's context right now — system prompt, project instructions, user turns, assistant turns, thinking, tool calls, tool results — with per-category token counts and a health meter against the model's window. No more guessing why an agent got dumber at 150k tokens.

### Memory is local and curated

Agents accumulate long-term memory across sessions in a **local vector index** (embeddings run on-device via ONNX — nothing leaves your machine), searchable by the agent through its own memory tools. A periodic **Curator** pass distills, merges, and prunes what accumulates, so memory improves with use instead of silting up. All of it sits in the same SQLite file as everything else.

### One roster, any provider

Each agent picks its own model independently: **Claude** (via subscription login or API key), **OpenAI Codex**, anything on **OpenRouter**, or fully-local models through **LM Studio** — side by side in the same fleet. Per-model thinking-level defaults and curated catalogs are handled centrally; a Haiku-powered classifier can run next to a local Qwen next to an Opus workhorse.

### Sessions have real ancestry

Fork, continue-in-place, and fresh-start are **three distinct, tracked moves** — not one "new chat" button. Continuing a conversation creates a new session row that points at its parent, so "what does this agent remember" is an explicit chain you can inspect in the session browser, not an accident of what stayed in RAM.

### And the rest

- **Objectives orthogonal to sessions** — goal tracking that spans sessions (and sessions that span goals), so "what is this agent working on" is queryable, not archaeological.
- **Per-turn complexity classification** — a cheap advisory classifier tags every user turn (visible as a pill in chat), groundwork for routing and automation.
- **Extensible inspector dock** — session browser, context inspector, memory inspector, stats, classifier settings; adding a panel is one component + one registry entry.
- **Everything on disk, everything yours** — SQLite as the single source of truth, prompts as editable markdown files, attachments and avatars as plain files under `~/.config/monarch/`.
- **Token-driven design system** — flat, no-shadow visual language with light/dark theming, browsable live at `/?catalog`.

## Architecture

```
┌──────────────┐   Tauri commands / events   ┌────────────────┐   JSONL over stdio   ┌─────────────────┐
│   Svelte 5   │ ◄─────────────────────────► │  Rust (Tauri)  │ ◄──────────────────► │  Node sidecar   │
│   frontend   │                             │      core      │                      │  (Pi SDK host)  │
└──────────────┘                             └────────┬───────┘                      └─────────────────┘
                                                      │
                                              SQLite (source of truth:
                                              agents, sessions, messages,
                                              objectives, memories, ...)
```

- **Rust owns state.** SQLite is canonical; the sidecar is a stateless-on-restart execution engine, not a session authority.
- **The frontend only displays state**, reconciled via versioned snapshots pushed over Tauri events (with a WebSocket fallback for browser-mode dev).
- **The sidecar is a singleton** — one Node process hosts every agent's in-memory Pi SDK session, keyed by agent ID.

See [ONBOARDING.md](./ONBOARDING.md) for the full data model, lifecycle walkthroughs, and protocol reference.

## Tech stack

| Layer | Tech |
|---|---|
| Desktop shell | [Tauri v2](https://v2.tauri.app/) |
| Frontend | Svelte 5 (runes), TypeScript, Vite |
| Backend | Rust, `tokio`, `rusqlite` / `tokio-rusqlite` |
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

## Docs

- [VISION.md](./VISION.md) — the north star this is building toward.
- [ONBOARDING.md](./ONBOARDING.md) — architecture, data model, lifecycle, and protocol reference.
- [CLAUDE.md](./CLAUDE.md) — conventions and code patterns used throughout the codebase.
- [`thoughts/`](./thoughts/) — research plans and implementation notes, written per feature as it's built.

## License

[MIT](./LICENSE)
