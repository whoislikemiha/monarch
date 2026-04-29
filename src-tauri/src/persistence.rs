use std::path::PathBuf;

use crate::error::MonarchError;

async fn monarch_dir() -> Result<PathBuf, MonarchError> {
    let dir = dirs::config_dir()
        .ok_or_else(|| MonarchError::persistence("config_dir unavailable"))?
        .join("monarch");
    tokio::fs::create_dir_all(&dir).await?;
    Ok(dir)
}

async fn prompts_dir() -> Result<PathBuf, MonarchError> {
    let dir = monarch_dir().await?.join("prompts");
    tokio::fs::create_dir_all(&dir).await?;
    Ok(dir)
}

/// MON-73: directory for user-uploaded agent avatar images.
async fn avatars_dir() -> Result<PathBuf, MonarchError> {
    let dir = monarch_dir().await?.join("avatars");
    tokio::fs::create_dir_all(&dir).await?;
    Ok(dir)
}

/// MON-75: directory for persisted chat-message image attachments.
async fn attachments_dir() -> Result<PathBuf, MonarchError> {
    let dir = monarch_dir().await?.join("attachments");
    tokio::fs::create_dir_all(&dir).await?;
    Ok(dir)
}

/// MON-75: decode a base64 image blob that came in on a user prompt and write
/// it to the attachments directory under a fresh UUID. Returns the absolute
/// on-disk path, ready to be stored in `message_attachments.path`.
pub async fn write_attachment_bytes(
    data_base64: &str,
    mime_type: &str,
) -> Result<PathBuf, MonarchError> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_base64)
        .map_err(|e| MonarchError::persistence(format!("Invalid base64 image data: {}", e)))?;
    let ext = match mime_type {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        _ => "bin",
    };
    let filename = format!("{}.{}", uuid::Uuid::new_v4(), ext);
    let dest = attachments_dir().await?.join(filename);
    tokio::fs::write(&dest, &bytes)
        .await
        .map_err(|e| MonarchError::persistence(format!("Failed to write attachment: {}", e)))?;
    Ok(dest)
}

/// MON-75: read a persisted attachment file and return it as a base64
/// `data:` URL. Used by both the webview (for displaying thumbnails in
/// restored chat bubbles) and by session replay (for hydrating image
/// content blocks back into the prompt the sidecar re-feeds to the LLM).
pub async fn read_attachment_bytes_base64(path: &str) -> Result<String, MonarchError> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| MonarchError::persistence(format!("Failed to read attachment: {}", e)))?;
    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD.encode(&bytes))
}

pub async fn read_agent_prompt_file(agent_id: &str) -> Result<Option<String>, MonarchError> {
    let path = prompts_dir().await?.join(format!("{}.md", agent_id));
    match tokio::fs::read_to_string(&path).await {
        Ok(contents) => Ok(Some(contents)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub async fn write_agent_prompt_file(agent_id: &str, prompt: &str) -> Result<(), MonarchError> {
    let path = prompts_dir().await?.join(format!("{}.md", agent_id));
    tokio::fs::write(&path, prompt).await?;
    Ok(())
}

pub async fn prompts_dir_string() -> Result<String, MonarchError> {
    Ok(prompts_dir().await?.to_string_lossy().to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_agent_prompt(agent_id: String) -> Result<Option<String>, MonarchError> {
    read_agent_prompt_file(&agent_id).await
}

#[tauri::command]
#[specta::specta]
pub async fn save_agent_prompt(agent_id: String, prompt: String) -> Result<(), MonarchError> {
    write_agent_prompt_file(&agent_id, &prompt).await
}

#[tauri::command]
#[specta::specta]
pub async fn get_prompts_dir() -> Result<String, MonarchError> {
    prompts_dir_string().await
}

/// MON-73: Read a saved avatar image and return it as a base64 data URL.
/// Lets the webview display local filesystem images without needing the Tauri
/// asset protocol to be scoped, which requires additional capability config.
#[tauri::command]
#[specta::specta]
pub async fn read_avatar_data_url(path: String) -> Result<String, MonarchError> {
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| MonarchError::persistence(format!("Failed to read avatar image: {}", e)))?;
    let ext = std::path::Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_lowercase();
    let mime = match ext.as_str() {
        "svg" => "image/svg+xml",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "image/png",
    };
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{};base64,{}", mime, b64))
}

/// MON-75: Read a saved chat-message attachment and return it as a base64
/// data URL. Mirrors `read_avatar_data_url` but is keyed on an arbitrary
/// path (attachments live under stable UUIDs, not an entity id).
#[tauri::command]
#[specta::specta]
pub async fn read_attachment_data_url(path: String) -> Result<String, MonarchError> {
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| MonarchError::persistence(format!("Failed to read attachment: {}", e)))?;
    let ext = std::path::Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_lowercase();
    let mime = match ext.as_str() {
        "svg" => "image/svg+xml",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "image/png",
    };
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{};base64,{}", mime, b64))
}

/// MON-73: Copy a user-selected image file into the Monarch avatars directory,
/// naming it after the agent. Returns the stored absolute path.
#[tauri::command]
#[specta::specta]
pub async fn save_avatar_image(agent_id: String, src_path: String) -> Result<String, MonarchError> {
    let src = std::path::Path::new(&src_path);
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_lowercase();
    let dest = avatars_dir().await?.join(format!("{}.{}", agent_id, ext));
    tokio::fs::copy(&src, &dest)
        .await
        .map_err(|e| MonarchError::persistence(format!("Failed to copy avatar image: {}", e)))?;
    Ok(dest.to_string_lossy().to_string())
}
