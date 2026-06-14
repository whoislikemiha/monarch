/// MON-75: reassemble a user message's content JSON with its persisted
/// image attachments spliced back in as `{type:"image"}` blocks, so the
/// sidecar (and the LLM behind it) see the same multimodal payload they
/// saw when the message was first sent. The `content` argument is the
/// raw JSON string stored in `messages.content` — after MON-75 persist,
/// this is always text-only (image blocks were stripped and written to
/// disk). If a block fails to read from disk we drop it silently and
/// keep going, rather than aborting the whole replay over one missing
/// file; a gap is better than a broken restore.
pub(super) async fn rehydrate_user_content(
    content: &str,
    attachments: &[crate::db::MessageAttachmentRow],
) -> String {
    // Parse the stored content as JSON. Strings and arrays are both
    // valid shapes depending on how the turn was captured; treat
    // anything else defensively as empty text.
    let parsed: serde_json::Value =
        serde_json::from_str(content).unwrap_or_else(|_| serde_json::Value::Array(Vec::new()));
    let mut blocks: Vec<serde_json::Value> = match parsed {
        serde_json::Value::Array(arr) => arr,
        serde_json::Value::String(s) => {
            if s.is_empty() {
                Vec::new()
            } else {
                vec![serde_json::json!({ "type": "text", "text": s })]
            }
        }
        other => vec![other],
    };

    for att in attachments {
        let bytes_b64 = match crate::persistence::read_attachment_bytes_base64(&att.path).await {
            Ok(b) => b,
            Err(e) => {
                eprintln!(
                    "[monarch] skipping attachment {} during replay: {}",
                    att.path, e
                );
                continue;
            }
        };
        blocks.push(serde_json::json!({
            "type": "image",
            "data": bytes_b64,
            "mimeType": att.mime_type,
        }));
    }

    serde_json::to_string(&serde_json::Value::Array(blocks)).unwrap_or_else(|_| String::new())
}

/// MON-100: pull plain text out of a stored `messages.content` value, which
/// may be a JSON-encoded array of content blocks (assistant), a plain string
/// (user, free-form), or a tool-result JSON blob. The Keeper benefits from
/// reading text fluently; image data and binary blobs are skipped.
pub(super) fn extract_text_from_stored_content(stored: &str) -> String {
    let trimmed = stored.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if !(trimmed.starts_with('[') || trimmed.starts_with('{') || trimmed.starts_with('"')) {
        return stored.to_string();
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(stored) else {
        return stored.to_string();
    };
    match value {
        serde_json::Value::String(s) => s,
        serde_json::Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| match b.get("type").and_then(|t| t.as_str()) {
                Some("text") => b.get("text").and_then(|t| t.as_str()).map(String::from),
                Some("thinking") => b.get("thinking").and_then(|t| t.as_str()).map(String::from),
                Some("toolCall") => {
                    let name = b.get("name").and_then(|n| n.as_str()).unwrap_or("tool");
                    let args = b
                        .get("arguments")
                        .map(|a| serde_json::to_string(a).unwrap_or_default())
                        .unwrap_or_default();
                    Some(format!("<toolCall name=\"{}\">{}</toolCall>", name, args))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        serde_json::Value::Object(obj) => {
            // Tool-result JSON: surface `result` + `toolName` so the Keeper
            // can claim "tool X returned Y" without tripping on the JSON
            // wrapper.
            let name = obj
                .get("toolName")
                .and_then(|n| n.as_str())
                .unwrap_or("tool");
            let result = obj
                .get("result")
                .map(|r| {
                    if let Some(s) = r.as_str() {
                        s.to_string()
                    } else {
                        serde_json::to_string(r).unwrap_or_default()
                    }
                })
                .unwrap_or_default();
            format!("<toolResult name=\"{}\">{}</toolResult>", name, result)
        }
        _ => stored.to_string(),
    }
}
