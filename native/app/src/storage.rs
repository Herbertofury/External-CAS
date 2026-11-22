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

/// Refuse existing links before creating the recovery path. The Tray writer
/// independently checks its destination and verifies the saved project.
fn reject_linked_recovery_path(path: &Path) -> Result<(), String> {
    for part in path.ancestors() {
        let metadata = match std::fs::symlink_metadata(part) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("Cannot inspect recovery directory: {error}")),
        };
        #[cfg(windows)]
        let linked = {
            use std::os::windows::fs::MetadataExt;
            metadata.file_attributes() & 0x400 != 0
        };
        #[cfg(not(windows))]
        let linked = metadata.file_type().is_symlink();
        if linked {
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
