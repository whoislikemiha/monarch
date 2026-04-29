//! MON-31: domain error type for the Tauri command boundary.
//!
//! Every `#[tauri::command]` in the crate returns `Result<T, MonarchError>`.
//! The enum carries source chains where available (rusqlite, reqwest, io,
//! serde_json) and serializes via a flat `ErrorDto { kind, message, details }`
//! shape. `kind` is the stable contract the frontend can branch on.

use serde::{Serialize, Serializer};
use std::fmt;
use std::sync::PoisonError;

/// Sub-kinds of sidecar-interaction failures. Flattened into the DTO's
/// top-level `kind` field so the frontend type-narrows without a nested match.
#[derive(Debug)]
pub enum SidecarErrorKind {
    ProcessDown,
    StdinWrite(String),
    ReplyError(String),
    Parse(String),
}

/// Monarch domain error. Every Tauri command returns this; `From` impls make
/// `?` work for the common source types.
#[derive(Debug)]
pub enum MonarchError {
    Sidecar(SidecarErrorKind),
    Db(rusqlite::Error),
    Persistence(String),
    Http(reqwest::Error),
    InvalidInput(String),
    NotFound(String),
    Lock(&'static str),
    Io(std::io::Error),
    Serde(serde_json::Error),
}

impl MonarchError {
    pub fn not_found(what: impl Into<String>) -> Self {
        Self::NotFound(what.into())
    }

    pub fn invalid_input(msg: impl Into<String>) -> Self {
        Self::InvalidInput(msg.into())
    }

    pub fn lock(what: &'static str) -> Self {
        Self::Lock(what)
    }

    pub fn persistence(msg: impl Into<String>) -> Self {
        Self::Persistence(msg.into())
    }

    pub fn sidecar_process_down() -> Self {
        Self::Sidecar(SidecarErrorKind::ProcessDown)
    }

    pub fn sidecar_stdin_write(msg: impl Into<String>) -> Self {
        Self::Sidecar(SidecarErrorKind::StdinWrite(msg.into()))
    }

    pub fn sidecar_reply(msg: impl Into<String>) -> Self {
        Self::Sidecar(SidecarErrorKind::ReplyError(msg.into()))
    }

    pub fn sidecar_parse(raw: impl Into<String>) -> Self {
        Self::Sidecar(SidecarErrorKind::Parse(raw.into()))
    }

    /// Stable camelCase `kind` for the serialized DTO. The frontend narrows on
    /// this string; sidecar sub-kinds flatten to the top level so callers can
    /// type-narrow without a nested match.
    fn kind_str(&self) -> &'static str {
        match self {
            Self::Sidecar(k) => match k {
                SidecarErrorKind::ProcessDown => "sidecarProcessDown",
                SidecarErrorKind::StdinWrite(_) => "sidecarStdinWrite",
                SidecarErrorKind::ReplyError(_) => "sidecarReplyError",
                SidecarErrorKind::Parse(_) => "sidecarParse",
            },
            Self::Db(_) => "db",
            Self::Persistence(_) => "persistence",
            Self::Http(_) => "http",
            Self::InvalidInput(_) => "invalidInput",
            Self::NotFound(_) => "notFound",
            Self::Lock(_) => "lock",
            Self::Io(_) => "io",
            Self::Serde(_) => "serde",
        }
    }

    fn details(&self) -> Option<String> {
        match self {
            Self::Sidecar(k) => match k {
                SidecarErrorKind::ProcessDown => None,
                SidecarErrorKind::StdinWrite(m) | SidecarErrorKind::ReplyError(m) => {
                    Some(m.clone())
                }
                SidecarErrorKind::Parse(raw) => Some(raw.clone()),
            },
            Self::Db(e) => Some(e.to_string()),
            Self::Http(e) => Some(e.to_string()),
            Self::Io(e) => Some(e.to_string()),
            Self::Serde(e) => Some(e.to_string()),
            Self::Persistence(_) | Self::InvalidInput(_) | Self::NotFound(_) | Self::Lock(_) => {
                None
            }
        }
    }
}

impl fmt::Display for MonarchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sidecar(k) => match k {
                SidecarErrorKind::ProcessDown => write!(f, "Sidecar process is not running"),
                SidecarErrorKind::StdinWrite(m) => write!(f, "Sidecar stdin write failed: {m}"),
                SidecarErrorKind::ReplyError(m) => write!(f, "Sidecar reply error: {m}"),
                SidecarErrorKind::Parse(_) => write!(f, "Failed to parse sidecar payload"),
            },
            Self::Db(e) => write!(f, "Database error: {e}"),
            Self::Persistence(m) => write!(f, "Persistence error: {m}"),
            Self::Http(e) => write!(f, "HTTP error: {e}"),
            Self::InvalidInput(m) => write!(f, "Invalid input: {m}"),
            Self::NotFound(m) => write!(f, "Not found: {m}"),
            Self::Lock(what) => write!(f, "Lock poisoned: {what}"),
            Self::Io(e) => write!(f, "IO error: {e}"),
            Self::Serde(e) => write!(f, "Serialization error: {e}"),
        }
    }
}

impl std::error::Error for MonarchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Db(e) => Some(e),
            Self::Http(e) => Some(e),
            Self::Io(e) => Some(e),
            Self::Serde(e) => Some(e),
            _ => None,
        }
    }
}

// ---- From impls ----

impl From<rusqlite::Error> for MonarchError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(e)
    }
}

impl From<tokio_rusqlite::Error> for MonarchError {
    fn from(e: tokio_rusqlite::Error) -> Self {
        match e {
            tokio_rusqlite::Error::Error(inner) => Self::Db(inner),
            other => Self::Persistence(other.to_string()),
        }
    }
}

impl From<std::io::Error> for MonarchError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for MonarchError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serde(e)
    }
}

impl From<reqwest::Error> for MonarchError {
    fn from(e: reqwest::Error) -> Self {
        Self::Http(e)
    }
}

impl From<tauri::Error> for MonarchError {
    fn from(e: tauri::Error) -> Self {
        Self::Persistence(e.to_string())
    }
}

/// Build a `map_err` closure for `Mutex::lock` / `RwLock` that tags the
/// poisoned lock with a stable label. Use as:
/// `.lock().map_err(lock_poisoned("agents"))?`
#[allow(dead_code)]
pub fn lock_poisoned<T>(what: &'static str) -> impl FnOnce(PoisonError<T>) -> MonarchError {
    move |_| MonarchError::Lock(what)
}

// ---- Serialized DTO shape ----
//
// ErrorDto is the stable contract exposed to the frontend. MonarchError
// projects to this via a custom Serialize impl so the enum can keep source
// chains (rusqlite::Error, reqwest::Error, …) while the wire shape stays flat.

#[derive(Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ErrorDto {
    pub kind: String,
    pub message: String,
    pub details: Option<String>,
}

impl Serialize for MonarchError {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let dto = ErrorDto {
            kind: self.kind_str().to_string(),
            message: self.to_string(),
            details: self.details(),
        };
        dto.serialize(serializer)
    }
}

impl specta::Type for MonarchError {
    fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
        <ErrorDto as specta::Type>::definition(types)
    }
}

/// Crate-wide result alias. Files that don't clash with `std::result::Result`
/// (e.g. no `.collect::<Result<Vec<_>, _>>()` over rusqlite rows) can
/// `use crate::error::Result;` and write `Result<T>` in signatures. Files
/// that do clash stick with the fully-qualified `Result<T, MonarchError>`.
#[allow(dead_code)]
pub type Result<T> = std::result::Result<T, MonarchError>;
