//! Choose automatic app-owned storage without moving existing project data.
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(crate) struct DataPaths {
    pub(crate) session_root: Option<PathBuf>,
    pub(crate) recovery_root: Option<PathBuf>,
    pub(crate) catalog_path: Option<PathBuf>,
}

impl DataPaths {
    pub(crate) fn resolve(
        portable: Option<PathBuf>,
        session_override: Option<PathBuf>,
        local_app_data: Option<PathBuf>,
        catalog_override: Option<PathBuf>,
    ) -> Result<Self, String> {
        if let Some(root) = portable {
            if session_override.is_some() {
                return Err("--portable-root and --session-root cannot be combined".into());
            }
            return Ok(Self {
                session_root: Some(root.join("Session")),
                recovery_root: Some(root.join("Recovery")),
                catalog_path: Some(catalog_path(&root)),
            });
        }

        let installed = local_app_data.map(|path| path.join("ExternalCAS"));
        Ok(Self {
            session_root: session_override,
            recovery_root: installed.as_ref().map(|root| root.join("Recovery")),
            catalog_path: catalog_override.or_else(|| installed.as_deref().map(catalog_path)),
        })
    }
}

fn catalog_path(root: &Path) -> PathBuf {
    root.join("Catalog").join("metadata-v1.sqlite")
}

const PORTABLE_MARKER: &str = "ExternalCAS.portable";
const PORTABLE_DATA_DIRECTORY: &str = "ExternalCAS-data";

/// Normal launches may opt in with a same-directory marker. Explicit options
/// and diagnostic launches keep their precedence without inspecting the marker.
pub(crate) fn launch_portable_root(
    explicit: Option<PathBuf>,
    use_marker: bool,
    executable: impl FnOnce() -> std::io::Result<PathBuf>,
) -> Result<Option<PathBuf>, String> {
    if explicit.is_some() || !use_marker {
        return Ok(explicit);
    }
    let executable =
        executable().map_err(|error| format!("Cannot locate the editor executable: {error}"))?;
    let directory = executable
        .parent()
        .filter(|path| path.is_absolute())
        .ok_or("The editor executable has no absolute parent folder")?;
    let marker = directory.join(PORTABLE_MARKER);
    let metadata = match std::fs::symlink_metadata(&marker) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Cannot inspect {PORTABLE_MARKER}: {error}")),
    };
    if !is_empty_ordinary_file(&metadata) {
        return Err(format!(
            "{PORTABLE_MARKER} must be an empty ordinary file next to the editor"
        ));
    }
    // A present but unreadable marker is not permission to use a different
    // storage root. Open read-only; no content or data directory is written.
    let opened = std::fs::File::open(&marker)
        .and_then(|file| file.metadata())
        .map_err(|error| format!("Cannot read {PORTABLE_MARKER}: {error}"))?;
    if !is_empty_ordinary_file(&opened) {
        return Err(format!("{PORTABLE_MARKER} changed while being inspected"));
    }
    Ok(Some(directory.join(PORTABLE_DATA_DIRECTORY)))
}

fn is_empty_ordinary_file(metadata: &std::fs::Metadata) -> bool {
    metadata.is_file() && !metadata_is_link(metadata) && metadata.len() == 0
}

fn metadata_is_link(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

/// Refuse existing links before creating the recovery path. The Tray writer
/// independently checks its destination and verifies the saved project.
fn reject_linked_recovery_path(path: &Path) -> Result<(), String> {
    for part in path.ancestors() {
        let metadata = match std::fs::symlink_metadata(part) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("Cannot inspect recovery directory: {error}")),
        };
        if metadata_is_link(&metadata) {
            return Err("Recovery directory cannot traverse a symlink or reparse point".into());
        }
    }
    Ok(())
}

pub(crate) fn save_draft_recovery(
    draft: &external_cas_tray::ProjectDraft,
    recovery_root: Option<&Path>,
) -> Result<PathBuf, String> {
    let root = recovery_root.ok_or("App recovery folder is unavailable; draft was not changed")?;
    // Like the session and catalog preflights, this does not bind future
    // operations to the same directory handles.
    reject_linked_recovery_path(root)?;
    std::fs::create_dir_all(root).map_err(|e| format!("Cannot create recovery folder: {e}"))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let destination = root.join(format!(
        "revision-{}-{}-{stamp}",
        draft.revision(),
        std::process::id()
    ));
    draft.save_as(&destination).map_err(|e| {
        format!("Recovery revision could not be verified; edit remains unapplied: {e}")
    })?;
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct OwnedMarkerFolder(PathBuf);
    impl OwnedMarkerFolder {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let name = format!(
                "external-cas-marker-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            let path = std::env::temp_dir().join(name);
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn resolve_marker(&self) -> Result<Option<PathBuf>, String> {
            launch_portable_root(None, true, || Ok(self.0.join("external-cas.exe")))
        }
    }
    impl Drop for OwnedMarkerFolder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn normal_launch_uses_marker_beside_the_executable_without_writing() {
        let folder = OwnedMarkerFolder::new();
        assert_eq!(folder.resolve_marker().unwrap(), None);
        let marker = folder.0.join(PORTABLE_MARKER);
        std::fs::write(&marker, []).unwrap();
        let portable = folder.0.join(PORTABLE_DATA_DIRECTORY);
        assert_eq!(folder.resolve_marker().unwrap(), Some(portable.clone()));
        assert!(
            !portable.exists(),
            "parsing the marker must not create data"
        );
        assert_eq!(std::fs::read(marker).unwrap(), b"");
    }

    #[test]
    fn marker_must_be_an_empty_ordinary_file() {
        let folder = OwnedMarkerFolder::new();
        let marker = folder.0.join(PORTABLE_MARKER);
        std::fs::create_dir(&marker).unwrap();
        assert!(folder.resolve_marker().is_err());
        std::fs::remove_dir(&marker).unwrap();
        std::fs::write(&marker, b"not an opt-in marker").unwrap();
        assert!(folder.resolve_marker().is_err());
        assert_eq!(std::fs::read(marker).unwrap(), b"not an opt-in marker");
    }

    #[cfg(unix)]
    #[test]
    fn linked_marker_is_not_followed() {
        let folder = OwnedMarkerFolder::new();
        let target = folder.0.join("unrelated");
        std::fs::write(&target, []).unwrap();
        std::os::unix::fs::symlink(&target, folder.0.join(PORTABLE_MARKER)).unwrap();
        assert!(folder.resolve_marker().is_err());
        assert_eq!(std::fs::read(target).unwrap(), b"");
    }

    #[test]
    fn explicit_and_noninteractive_launches_do_not_probe_the_marker() {
        let explicit = PathBuf::from("chosen");
        assert_eq!(
            launch_portable_root(Some(explicit.clone()), true, || panic!("unexpected probe"))
                .unwrap(),
            Some(explicit)
        );
        assert_eq!(
            launch_portable_root(None, false, || panic!("unexpected probe")).unwrap(),
            None
        );
    }

    #[test]
    fn portable_storage_overrides_installed_and_catalog_paths() {
        let selected = PathBuf::from("portable-fixture");
        let paths = DataPaths::resolve(
            Some(selected.clone()),
            None,
            Some(PathBuf::from("installed-fixture")),
            Some(PathBuf::from("external-catalog.sqlite")),
        )
        .unwrap();
        assert_eq!(paths.session_root, Some(selected.join("Session")));
        assert_eq!(paths.recovery_root, Some(selected.join("Recovery")));
        assert_eq!(
            paths.catalog_path,
            Some(selected.join("Catalog").join("metadata-v1.sqlite"))
        );
    }

    #[test]
    fn installed_mode_keeps_independent_legacy_overrides() {
        let installed = PathBuf::from("installed-fixture").join("ExternalCAS");
        let legacy_session = PathBuf::from("legacy-session");
        let catalog_override = PathBuf::from("external-catalog.sqlite");
        let paths = DataPaths::resolve(
            None,
            Some(legacy_session.clone()),
            installed.parent().map(Path::to_path_buf),
            Some(catalog_override.clone()),
        )
        .unwrap();
        assert_eq!(paths.session_root, Some(legacy_session));
        assert_eq!(paths.recovery_root, Some(installed.join("Recovery")));
        assert_eq!(paths.catalog_path, Some(catalog_override));
        let paths = DataPaths::resolve(None, None, installed.parent().map(Path::to_path_buf), None)
            .unwrap();
        assert_eq!(paths.session_root, None);
        assert_eq!(paths.catalog_path, Some(catalog_path(&installed)));
    }

    #[test]
    fn explicit_portable_and_session_override_conflict() {
        assert!(
            DataPaths::resolve(
                Some(PathBuf::from("portable")),
                Some(PathBuf::from("session")),
                None,
                None
            )
            .is_err()
        );
    }
}
