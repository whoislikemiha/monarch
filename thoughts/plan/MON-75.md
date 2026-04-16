# MON-75: Enable image pasting in chat (drag, paste, file picker)

## Summary

The chat input in Monarch is currently text-only. This plan threads image support through all four layers needed to deliver it: the `ChatInput.svelte` UI (paste/drag/pick + thumbnail strip), `AgentView.svelte` (updated `sendPrompt` signature), the sidecar protocol (expanding `PromptCommand.message` from a plain string to a union that can carry image payloads), and the sidecar runtime manager (building a proper `AgentMessage` from text + images before calling `session.prompt()`). The Pi SDK already supports multimodal input natively — the `prompt()` method accepts `ImageContent` objects with base64 data — so no SDK changes are needed. Images are intentionally not persisted to SQLite in this scope; they are sent to the agent and then discarded.

---

## Relevant files and areas

| File | Why it's relevant |
|------|-------------------|
| `src/lib/ChatInput.svelte` | The input component; needs drag/drop zone, paste handler, file picker button, and thumbnail strip. Currently only emits a plain string via `onsend`. |
| `src/lib/AgentView.svelte` (~line 261–268) | Houses `sendPrompt`, which calls `sendPiCommand({ type: "prompt", message })`. Must be updated to accept images and build a structured content payload. |
| `src/lib/api.ts` (~line 164–176) | The IPC abstraction; already passes generic JSON — no changes needed. |
| `sidecar/src/protocol.ts` (~line 34–38) | Defines `PromptCommand`. `message` is currently `string`. Must be widened to accept an array of text/image content items. |
| `src-tauri/src/sidecar_protocol.rs` | Rust mirror of `PromptCommand`. Must match the TypeScript protocol change so serde can serialize and forward the richer message to the sidecar. |
| `sidecar/src/runtime-manager.ts` (~line 317–333) | The `prompt()` handler. Currently calls `session.prompt(message)` with the raw string. Must detect whether the payload contains images and build an `AgentMessage` with a `content` array before forwarding to the Pi SDK. |
| `node_modules/@mariozechner/pi-ai/dist/types.d.ts` | Pi SDK type definitions. Confirms `ImageContent { type, data, mimeType }` is already supported and that `prompt()` accepts `AgentMessage[]`. No changes needed here — just the types to program against. |
| `src-tauri/src/db.rs` | SQLite schema. `messages.content` is plain text. Not touched in this scope (no image persistence). |

---

## What needs to change

### 1. `sidecar/src/protocol.ts` — widen `PromptCommand`
`PromptCommand.message` changes from `string` to a union: either a plain string (for backward compatibility) or an array of content items where each item is either `{ type: "text", text: string }` or `{ type: "image", data: string, mimeType: string }`. The runtime manager already narrows on the type, so this is additive.

### 2. `src-tauri/src/sidecar_protocol.rs` — mirror the protocol change
The `Prompt` variant's `message` field must be widened to a type that serde can serialize as either a plain string or an array of content objects. A Rust enum (`PromptMessage::Text(String)` / `PromptMessage::Parts(Vec<ContentPart>)`) with `#[serde(untagged)]` achieves this while keeping the wire format readable.

### 3. `sidecar/src/runtime-manager.ts` — handle image payloads
The `prompt()` method inspects the incoming `message`. If it's a string, current behavior is unchanged. If it's an array of parts, it maps them to `TextContent | ImageContent` objects and builds an `AgentMessage` before calling `session.prompt()` or `session.followUp()`. The Pi SDK already accepts this shape.

### 4. `src/lib/ChatInput.svelte` — image input UI
Three entry points all funnel to the same internal image state (`$state<PendingImage[]>`):
- **Ctrl+V** — intercept `paste` event, read `ClipboardEvent.clipboardData.items`, filter for `image/*`, convert the `Blob` to a base64 data URL.
- **Drag & drop** — attach `dragover` + `drop` handlers to the chat container, read `DataTransfer.files`, convert each accepted image file to base64.
- **File picker** — a hidden `<input type="file" accept="image/*" multiple>` triggered by a small attachment button.

Each accepted image is appended to the pending queue. The thumbnail strip renders below the text area: a small `<img>` of the base64 data with an X button that splices the item out. On send, the images are included in the `onsend` callback payload and then the queue is cleared.

### 5. `src/lib/AgentView.svelte` — updated `sendPrompt`
`sendPrompt` gains a second parameter: the array of `PendingImage` objects from `ChatInput`. It assembles the sidecar payload: if there are no images, sends a plain string (existing behavior); if there are images, builds an array of content parts. `ChatInput`'s `onsend` callback type is updated to match.

---

## Decisions (resolved)

1. **Image size limit** — 5 MB per image. If the decoded file exceeds this, silently reject it (no attachment added). A small error indicator or toast can surface the rejection reason.

2. **Drag target scope** — the full agent view panel, not just the text area. A visual drag-overlay (e.g., a dimmed border glow) should appear on `dragover` to make the drop zone discoverable.

3. **Clipboard approach** — use `ClipboardEvent.clipboardData` on the `paste` event, not `navigator.clipboard.read()`. The `clipboardData` path requires no permissions and works in Tauri's WebView on all platforms. Requires the textarea to be focused, which is already the expected state.

4. **Maximum images per message** — cap at 5. If the queue is full, additional paste/drag/pick attempts are silently ignored (or the button is disabled).

---

## Out of scope reminders

- No image persistence to SQLite (this includes no display of images when loading past messages from history).
- No image compression or resizing.
- No format validation beyond what the browser/clipboard natively provides.
- No changes to the WebSocket fallback path beyond what the generic `send_command` already handles.
