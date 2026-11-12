//! Append-only, hash-verified local editor context. No game files are opened.
//! Referenced asset/project paths are private metadata, never followed here.
use serde::{Deserialize, Serialize};
use serde_json::{Value, value::RawValue};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub const SESSION_SCHEMA: u32 = 1;
pub const MAX_PAYLOAD_BYTES: usize = 4 * 1024 * 1024;
const MAX_SNAPSHOT_BYTES: usize = MAX_PAYLOAD_BYTES + 4096;
const MAX_MARKER_BYTES: usize = 4096;
const FORMAT: &str = "external-cas-session";
const COMMIT_FORMAT: &str = "external-cas-session-commit";
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub enum SessionError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Invalid(String),
    FutureSchema { versions: Vec<u32> },
}
impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Json(e) => write!(f, "{e}"),
            Self::Invalid(e) => write!(f, "{e}"),
            Self::FutureSchema { versions } => write!(
                f,
                "Session contains unsupported future schemas {versions:?}; preserved snapshots prevent automatic downgrade writes"
            ),
        }
    }
}
impl std::error::Error for SessionError {}
impl From<std::io::Error> for SessionError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for SessionError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}
pub type Result<T> = std::result::Result<T, SessionError>;
fn invalid(reason: impl Into<String>) -> SessionError {
    SessionError::Invalid(reason.into())
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceSelection {
    /// Full TYPE:GROUP:INSTANCE, never an instance-only or wildcard selection.
    pub key: String,
    pub provider_path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_index_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_sha256: Option<String>,
    #[serde(default, flatten)]
    pub extra: BTreeMap<String, Value>,
}
impl ResourceSelection {
    pub fn new(key: impl Into<String>, provider_path: impl Into<PathBuf>) -> Self {
        Self {
            key: key.into(),
            provider_path: provider_path.into(),
            source_index_sha256: None,
            payload_sha256: None,
            extra: BTreeMap::new(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PinnedFile {
    pub path: PathBuf,
    /// Whole-file hash for this explicitly selected file (for example RIG).
    pub sha256: String,
    #[serde(default, flatten)]
    pub extra: BTreeMap<String, Value>,
}
impl PinnedFile {
    pub fn new(path: impl Into<PathBuf>, sha256: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            sha256: sha256.into(),
            extra: BTreeMap::new(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClipSelection {
    pub key: String,
    pub provider_path: PathBuf,
    /// Actual package index digest, when observed. Never a whole-package claim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_index_sha256: Option<String>,
    pub payload_sha256: String,
    #[serde(default, flatten)]
    pub extra: BTreeMap<String, Value>,
}
impl ClipSelection {
    pub fn new(
        key: impl Into<String>,
        provider_path: impl Into<PathBuf>,
        payload_sha256: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            provider_path: provider_path.into(),
            source_index_sha256: None,
            payload_sha256: payload_sha256.into(),
            extra: BTreeMap::new(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct CameraState {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: [f32; 3],
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
impl Default for CameraState {
    fn default() -> Self {
        Self {
            yaw: 0.,
            pitch: 0.08,
            distance: 3.6,
            target: [0., 0.9, 0.],
            extra: BTreeMap::new(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SessionState {
    pub roots: Vec<PathBuf>,
    pub project_path: Option<PathBuf>,
    pub recovery_project_path: Option<PathBuf>,
    pub selection: Option<ResourceSelection>,
    pub rig: Option<PinnedFile>,
    pub clip: Option<ClipSelection>,
    pub tick: f32,
    pub camera: CameraState,
    pub ui_tab: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}
impl Default for SessionState {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            project_path: None,
            recovery_project_path: None,
            selection: None,
            rig: None,
            clip: None,
            tick: 0.,
            camera: CameraState::default(),
            ui_tab: "wardrobe".into(),
            extra: BTreeMap::new(),
        }
    }
}
impl SessionState {
    /// Structural validation only. Caller must revalidate referenced assets and
    /// project receipts before restoring them; paths are not opened by this crate.
    pub fn validate(&self) -> Result<()> {
        for path in self
            .roots
            .iter()
            .chain(self.project_path.iter())
            .chain(self.recovery_project_path.iter())
        {
            absolute(path)?;
        }
        if let Some(value) = &self.selection {
            key(&value.key)?;
            absolute(&value.provider_path)?;
            optional_hash(value.source_index_sha256.as_deref())?;
            optional_hash(value.payload_sha256.as_deref())?;
        }
        if let Some(value) = &self.rig {
            absolute(&value.path)?;
            digest(&value.sha256)?;
        }
        if let Some(value) = &self.clip {
            key(&value.key)?;
            absolute(&value.provider_path)?;
            optional_hash(value.source_index_sha256.as_deref())?;
            digest(&value.payload_sha256)?;
        }
        if !self.tick.is_finite() || !(0.0..=65535.0).contains(&self.tick) {
            return Err(invalid("Invalid session pose tick"));
        }
        if !self.camera.yaw.is_finite()
            || !self.camera.pitch.is_finite()
            || !self.camera.distance.is_finite()
            || self.camera.distance <= 0.
            || self.camera.target.iter().any(|x| !x.is_finite())
        {
            return Err(invalid("Invalid session camera"));
        }
        if self.ui_tab.is_empty() || self.ui_tab.len() > 256 {
            return Err(invalid("Invalid session tab identifier"));
        }
        Ok(())
    }
}
fn absolute(path: &Path) -> Result<()> {
    if !path.is_absolute() {
        return Err(invalid("Session asset/project paths must be absolute"));
    }
    Ok(())
}
fn key(value: &str) -> Result<()> {
    let parts = value.split(':').collect::<Vec<_>>();
    if parts.len() != 3
        || !parts
            .iter()
            .zip([8, 8, 16])
            .all(|(p, n)| p.len() == n && p.bytes().all(|c| c.is_ascii_hexdigit()))
    {
        return Err(invalid(
            "Session resource key must be full TYPE:GROUP:INSTANCE",
        ));
    }
    Ok(())
}
fn digest(value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(invalid("Invalid observed SHA-256"));
    }
    Ok(())
}
fn optional_hash(value: Option<&str>) -> Result<()> {
    if let Some(value) = value {
        digest(value)?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueKind {
    Incomplete,
    Invalid,
    FutureSchema(u32),
    UnsupportedSchema(u32),
    Unreadable,
}
#[derive(Debug, Clone)]
pub struct SnapshotIssue {
    pub path: PathBuf,
    pub kind: IssueKind,
    pub message: String,
}
#[derive(Debug, Default)]
pub struct LoadReport {
    pub state: Option<SessionState>,
    pub snapshot: Option<PathBuf>,
    pub issues: Vec<SnapshotIssue>,
    pub recovered_from_previous: bool,
    pub future_schemas: Vec<u32>,
    pub snapshot_sha256: Option<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Durability {
    FileContentsSynced,
    FileContentsAndDirectorySynced,
}
#[derive(Debug)]
pub struct SnapshotReceipt {
    pub path: PathBuf,
    pub marker_path: PathBuf,
    pub sha256: String,
    pub durability: Durability,
}
#[derive(Debug, Clone)]
pub struct SessionStore {
    root: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    format: String,
    schema: u32,
    created_unix_ns: String,
    payload_sha256: String,
    payload: Box<RawValue>,
}
#[derive(Serialize, Deserialize)]
struct CommitMarker {
    format: String,
    schema: u32,
    file_name: String,
    snapshot_sha256: String,
    byte_length: u64,
}

impl SessionStore {
    pub fn open_default() -> Result<Self> {
        let local = std::env::var_os("LOCALAPPDATA")
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                invalid("LOCALAPPDATA is unavailable; provide an explicit session root")
            })?;
        Self::at(PathBuf::from(local).join("ExternalCAS"))
    }
    /// Explicit private root for tests or a separately authorized app location.
    /// Does not create files until `save` is called.
    pub fn at(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        absolute(&root)?;
        Ok(Self { root })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn load(&self) -> Result<LoadReport> {
        reject_links(&self.root)?;
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(LoadReport::default()),
            Err(e) => return Err(e.into()),
        };
        let mut candidates = Vec::new();
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("session-") && name.ends_with(".json") {
                candidates.push(entry.path());
            }
        }
        candidates.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
        let mut report = LoadReport::default();
        let mut newer_bad = false;
        for snapshot in candidates {
            let result = self.read_snapshot(&snapshot);
            match result {
                Ok(ReadSnapshot::Known(state, digest)) => {
                    if report.state.is_none() {
                        report.state = Some(state);
                        report.snapshot = Some(snapshot);
                        report.snapshot_sha256 = Some(digest);
                        report.recovered_from_previous = newer_bad;
                    }
                }
                Ok(ReadSnapshot::Unknown(schema)) => {
                    let kind = if schema > SESSION_SCHEMA {
                        report.future_schemas.push(schema);
                        IssueKind::FutureSchema(schema)
                    } else {
                        IssueKind::UnsupportedSchema(schema)
                    };
                    report.issues.push(SnapshotIssue {
                        path: snapshot,
                        kind,
                        message: format!(
                            "Unsupported session schema {schema}; original snapshot preserved"
                        ),
                    });
                    if report.state.is_none() {
                        newer_bad = true;
                    }
                }
                Err((kind, message)) => {
                    report.issues.push(SnapshotIssue {
                        path: snapshot,
                        kind,
                        message,
                    });
                    if report.state.is_none() {
                        newer_bad = true;
                    }
                }
            }
        }
        report.future_schemas.sort_unstable();
        report.future_schemas.dedup();
        Ok(report)
    }
    pub fn save(&self, state: &SessionState) -> Result<SnapshotReceipt> {
        state.validate()?;
        let payload = serde_json::to_vec(state)?;
        if payload.len() > MAX_PAYLOAD_BYTES {
            return Err(invalid(format!(
                "Session payload exceeds {MAX_PAYLOAD_BYTES} bytes; nothing was truncated"
            )));
        }
        let existing = self.load()?;
        if !existing.future_schemas.is_empty() {
            return Err(SessionError::FutureSchema {
                versions: existing.future_schemas,
            });
        }
        reject_links(&self.root)?;
        fs::create_dir_all(&self.root)?;
        reject_links(&self.root)?;
        let created = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| invalid("System clock precedes Unix epoch"))?
            .as_nanos();
        let raw = RawValue::from_string(
            String::from_utf8(payload.clone())
                .map_err(|_| invalid("Invalid serialized session UTF-8"))?,
        )?;
        let envelope = Envelope {
            format: FORMAT.into(),
            schema: SESSION_SCHEMA,
            created_unix_ns: created.to_string(),
            payload_sha256: hash(&payload),
            payload: raw,
        };
        let bytes = serde_json::to_vec(&envelope)?;
        if bytes.len() > MAX_SNAPSHOT_BYTES {
            return Err(invalid("Session envelope exceeds byte budget"));
        }
        // Each writer owns an exclusive filename. No existing file is replaced,
        // renamed over, deleted or truncated, including incomplete snapshots.
        let (path, mut file) = loop {
            let counter = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let name = format!(
                "session-{created:039}-{:010}-{counter:020}.json",
                std::process::id()
            );
            let path = self.root.join(name);
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => break (path, file),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        };
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        let snapshot_sha256 = hash(&bytes);
        let marker_path = path.with_extension("complete");
        let marker = CommitMarker {
            format: COMMIT_FORMAT.into(),
            schema: SESSION_SCHEMA,
            file_name: path.file_name().unwrap().to_string_lossy().into_owned(),
            snapshot_sha256: snapshot_sha256.clone(),
            byte_length: bytes.len() as u64,
        };
        let marker_bytes = serde_json::to_vec(&marker)?;
        let mut marker_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker_path)?;
        marker_file.write_all(&marker_bytes)?;
        marker_file.sync_all()?;
        drop(marker_file);
        let durability = sync_directory(&self.root)?;
        Ok(SnapshotReceipt {
            path,
            marker_path,
            sha256: snapshot_sha256,
            durability,
        })
    }
    fn read_snapshot(&self, path: &Path) -> std::result::Result<ReadSnapshot, (IssueKind, String)> {
        let marker_path = path.with_extension("complete");
        let marker_bytes=read_limited(&marker_path,MAX_MARKER_BYTES).map_err(|e|{
            let kind=if matches!(&e,SessionError::Io(io) if io.kind()==std::io::ErrorKind::NotFound){IssueKind::Incomplete}else{IssueKind::Invalid};(kind,e.to_string())
        })?;
        let marker: CommitMarker = serde_json::from_slice(&marker_bytes)
            .map_err(|e| (IssueKind::Invalid, e.to_string()))?;
        if marker.format != COMMIT_FORMAT
            || marker.file_name != path.file_name().unwrap_or_default().to_string_lossy()
            || digest(&marker.snapshot_sha256).is_err()
        {
            return Err((
                IssueKind::Invalid,
                "Invalid session completion marker".into(),
            ));
        }
        if marker.schema > SESSION_SCHEMA {
            return Ok(ReadSnapshot::Unknown(marker.schema));
        }
        let bytes = read_limited(path, MAX_SNAPSHOT_BYTES)
            .map_err(|e| (IssueKind::Invalid, e.to_string()))?;
        if bytes.len() as u64 != marker.byte_length || hash(&bytes) != marker.snapshot_sha256 {
            return Err((
                IssueKind::Invalid,
                "Session snapshot hash or size differs from its completion marker".into(),
            ));
        }
        let envelope: Envelope =
            serde_json::from_slice(&bytes).map_err(|e| (IssueKind::Invalid, e.to_string()))?;
        if envelope.format != FORMAT {
            return Err((IssueKind::Invalid, "Unrecognized session format".into()));
        }
        if envelope.schema != SESSION_SCHEMA {
            return Ok(ReadSnapshot::Unknown(envelope.schema));
        }
        if marker.schema != SESSION_SCHEMA
            || digest(&envelope.payload_sha256).is_err()
            || hash(envelope.payload.get().as_bytes()) != envelope.payload_sha256
        {
            return Err((
                IssueKind::Invalid,
                "Session payload hash/schema mismatch".into(),
            ));
        }
        let state: SessionState = serde_json::from_str(envelope.payload.get())
            .map_err(|e| (IssueKind::Invalid, e.to_string()))?;
        state
            .validate()
            .map_err(|e| (IssueKind::Invalid, e.to_string()))?;
        Ok(ReadSnapshot::Known(state, marker.snapshot_sha256))
    }
}
enum ReadSnapshot {
    Known(SessionState, String),
    Unknown(u32),
}

fn reject_links(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => {
                #[cfg(windows)]
                let linked = {
                    use std::os::windows::fs::MetadataExt;
                    metadata.file_attributes() & 0x400 != 0
                };
                #[cfg(not(windows))]
                let linked = metadata.file_type().is_symlink();
                if linked {
                    return Err(invalid(
                        "Session storage cannot traverse a symlink or reparse point",
                    ));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
fn read_limited(path: &Path, limit: usize) -> Result<Vec<u8>> {
    reject_links(path)?;
    let file = File::open(path)?;
    let before = file.metadata()?;
    if !before.is_file() || before.len() > limit as u64 {
        return Err(invalid("Session file is not a regular bounded file"));
    }
    let mut bytes = Vec::with_capacity(before.len() as usize);
    (&file).take(limit as u64 + 1).read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    if bytes.len() > limit
        || before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || bytes.len() as u64 != before.len()
    {
        return Err(invalid("Session file changed or exceeded its byte budget"));
    }
    Ok(bytes)
}
#[cfg(not(windows))]
fn sync_directory(path: &Path) -> Result<Durability> {
    File::open(path)?.sync_all()?;
    Ok(Durability::FileContentsAndDirectorySynced)
}
#[cfg(windows)]
fn sync_directory(_path: &Path) -> Result<Durability> {
    // Rust does not provide a portable Windows directory-entry flush. Both file
    // contents are flushed; power-loss durability still needs filesystem testing.
    Ok(Durability::FileContentsSynced)
}
