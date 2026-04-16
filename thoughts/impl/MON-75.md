# MON-75 — Image pasting in chat

## What was implemented

First-class image input for the chat box, plus full persistence so the
LLM retains image context across reloads.

- **Three input paths:** Ctrl+V paste (with a Tauri clipboard-manager
  fallback that decodes RGBA → PNG via a canvas, required on Linux /
  WebKitGTK where `ClipboardEvent.clipboardData` doesn't carry images),
  drag-and-drop anywhere on the agent panel (overlay gated on
  `!isStreaming`), and a paperclip file-picker button.
- **Pending thumbnail strip** above the textarea with a per-thumb ×
  button, a click-to-preview handler, and a 5-image / 5 MB guard.
- **Multimodal send** through a widened sidecar protocol: the
  `PromptCommand.message` field now accepts either a plain string or an
  array of `PromptContentPart`s (`text` | `image`). Rust passes it
  through transparently as `serde_json::Value`; the sidecar splits the
  parts and calls `session.prompt(text, { images })` /
  `session.followUp(text, images)` on the Pi SDK.
- **Chat-history rendering:** sent images appear inside the user bubble
  both live (ephemeral in-flight cache) and on reload (DB-driven).
- **Shared `ImageLightbox`** (backdrop / × / Esc to close, Tab trap,
  focus restore on unmount).
- **Persistence:** new `message_attachments` table references
  `~/.config/monarch/attachments/{uuid}.{ext}` blobs. User `MessageEnd`
  events have their image blocks stripped from the stored `content`,
  written to disk, and linked via a foreign-key row. Restoring a
  session re-reads the bytes and splices image blocks back into the
  content JSON before shipping `LoadSession` to the sidecar, so the
  agent remembers what it was looking at.

## Key decisions

- **Base64 stays in memory only briefly.** On send the frontend hands a
  base64 data URL to Rust via the sidecar command, Rust persists the
  bytes to disk immediately, then strips the image from `content`. The
  `messages` table stays lean — only text lands there.
- **Filesystem under a UUID, flat layout.** Matches the MON-73 avatar
  pattern. Attachments are write-once, so a global by-path data-URL
  cache on the frontend is safely unbounded for any realistic session.
  No garbage collection of orphaned files yet — left for a follow-up.
- **Attachment write failures don't roll back the message save.** A
  disk-full condition should not lose the user's text. Worst case: one
  thumbnail gap in the bubble, plus a stderr log line.
- **Live vs. restored rendering.** The snapshot emitted from live
  sidecar events has `attachments: []` for user items — during the
  in-flight window between send and the first DB-driven snapshot, the
  frontend uses its ephemeral `sentImages` map keyed by user-message
  index. Once the DB round-trip lands, `attachments` takes over and the
  map becomes irrelevant for that message. Both paths feed the same
  lightbox callback.
- **Image-only sends work but pass empty text to Pi.** The Pi SDK wraps
  user turns as `[{type:"text",text}, ...images]` unconditionally.
  Empty text is technically invalid on some providers; not observed in
  testing with Anthropic. Left alone — fixing requires opinionated
  placeholder text that we don't need yet.
- **Standby-wake edge case left as-is.** Sending an image to a stopped
  agent that has parent-session ancestry can briefly mis-key the
  ephemeral thumbnail until the DB snapshot replaces it; persistence
  makes this self-healing within one round-trip.

## Files touched

### Frontend
- `src/lib/ChatInput.svelte` — paste handler with Tauri fallback,
  thumbnail strip, attach button, send-clears-images.
- `src/lib/AgentView.svelte` — panel drag-and-drop (with streaming
  guard), ephemeral `sentImages` map, lightbox host.
- `src/lib/MessageList.svelte` — user bubble renders persisted
  attachments (via `AttachmentThumb`) or falls back to ephemeral map.
- `src/lib/ImageLightbox.svelte` *(new)* — shared fullscreen preview
  with focus trap.
- `src/lib/AttachmentThumb.svelte` *(new)* — resolves an attachment
  path to a data URL with a global cache.
- `src/lib/types.ts` — `DisplayItem.user.attachments`,
  `MessageAttachment` alias.
- `src/lib/bindings.ts` — regenerated via `cargo run -- --export-bindings`.

### Sidecar
- `sidecar/src/protocol.ts` — `PromptContentPart` type, widened
  `PromptCommand.message`.
- `sidecar/src/runtime-manager.ts` — multimodal prompt/followUp branch.

### Rust
- `src-tauri/src/sidecar_protocol.rs` — `PromptCommand.message` widened
  to `serde_json::Value`; `DisplayItem::User` gains `attachments`.
- `src-tauri/src/db.rs` — `message_attachments` schema,
  `MessageAttachmentRow`, `save_message_attachment_internal`,
  `get_messages_with_ancestry` hydration, `MessageRow.attachments`.
- `src-tauri/src/persistence.rs` — `attachments_dir`,
  `write_attachment_bytes`, `read_attachment_bytes_base64`,
  `read_attachment_data_url` (Tauri command).
- `src-tauri/src/agent/persist.rs` — `PendingAttachment`,
  `extract_image_attachments`, write + link in `apply`.
- `src-tauri/src/agent/manager.rs` — `rehydrate_user_content` used by
  `load_session_context`.
- `src-tauri/src/agent_state.rs` — `DisplayItem::User.attachments`
  populated in `display_items_from_messages`.
- `src-tauri/src/lib.rs` — register `read_attachment_data_url`.
- `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`,
  `src-tauri/capabilities/default.json` — `tauri-plugin-clipboard-manager`.

## What was left out

- **Non-image file attachments.** Out of scope per the updated AC.
- **Size/format validation beyond existing 5 MB / 5 per-send limits.**
- **Orphaned attachment cleanup.** Deleted sessions leave their files
  on disk; a future maintenance job or `CASCADE`-driven cleanup
  (currently the FK `ON DELETE CASCADE` only removes DB rows, not the
  files) should pick these up. Not blocking for this feature.
- **Image-only send placeholder.** If the Pi SDK starts rejecting
  empty-text multimodal turns in the wild we'll revisit.
- **Gallery / attachment management view.** No UI for browsing all
  attachments across sessions.
