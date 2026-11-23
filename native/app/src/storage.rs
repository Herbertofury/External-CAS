//! Choose automatic app-owned storage without moving existing project data.
use external_cas_session::SessionState;
use external_cas_tray::TrayProject;
use std::path::{Path, PathBuf};

const PORTABLE_RECOVERY: &str = "portable_recovery_v1";

#[derive(Default)]
pub(crate) struct DataPaths {
    pub(crate) portable: bool,
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
                portable: true,
                session_root: Some(root.join("Session")),
                recovery_root: Some(root.join("Recovery")),
                catalog_path: Some(catalog_path(&root)),
            });
        }

        let installed = local_app_data.map(|path| path.join("ExternalCAS"));
        Ok(Self {
            portable: false,
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

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecoveryLocator {
    revision_id: String,
    manifest_sha256: String,
}

impl RecoveryLocator {
    fn validate(&self) -> Result<(), String> {
        let mut fields = self.revision_id.split('-');
        if fields.next() != Some("revision")
            || !canonical_number::<u64>(fields.next())
            || !canonical_number::<u32>(fields.next())
            || !canonical_number::<u128>(fields.next())
            || fields.next().is_some()
            || self.manifest_sha256.len() != 64
            || !self
                .manifest_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err("Saved portable recovery identifier or checksum is invalid".into());
        }
        Ok(())
    }
}

fn canonical_number<T: std::str::FromStr + ToString>(field: Option<&str>) -> bool {
    field.is_some_and(|value| value.parse::<T>().is_ok_and(|num| num.to_string() == value))
}

pub(crate) struct SavedRecovery {
    pub(crate) path: PathBuf,
    locator: RecoveryLocator,
}

impl SavedRecovery {
    pub(crate) fn remember(self, state: &mut SessionState, portable: bool) -> PathBuf {
        if portable {
            state.recovery_project_path = None;
            state
                .extra
                .insert(PORTABLE_RECOVERY.into(), serde_json::json!(self.locator));
        } else {
            state.extra.remove(PORTABLE_RECOVERY);
            state.recovery_project_path = Some(self.path.clone());
        }
        self.path
    }
}

pub(crate) fn remember_project(state: &mut SessionState, path: PathBuf) {
    state.project_path = Some(path);
    state.recovery_project_path = None;
    state.extra.remove(PORTABLE_RECOVERY);
}

pub(crate) enum ProjectOpen {
    Explicit(PathBuf),
    AbsoluteRecovery(PathBuf),
    PortableRecovery {
        root: PathBuf,
        locator: RecoveryLocator,
    },
}

impl ProjectOpen {
    pub(crate) fn saved(state: &SessionState, paths: &DataPaths) -> Result<Option<Self>, String> {
        if let Some(raw) = state.extra.get(PORTABLE_RECOVERY) {
            let locator: RecoveryLocator = serde_json::from_value(raw.clone())
                .map_err(|_| "Saved portable recovery record is invalid".to_owned())?;
            locator.validate()?;
            let root = paths
                .recovery_root
                .as_ref()
                .filter(|root| paths.portable && root.is_absolute())
                .ok_or("Saved portable recovery requires the current portable folder")?;
            return Ok(Some(Self::PortableRecovery {
                root: root.clone(),
                locator,
            }));
        }
        // An older absolute recovery path was not recorded as app-owned portable
        // data. Keep its original meaning; never guess a new path from its name.
        Ok(state
            .recovery_project_path
            .clone()
            .map(Self::AbsoluteRecovery)
            .or_else(|| state.project_path.clone().map(Self::Explicit)))
    }

    pub(crate) fn is_recovery(&self) -> bool {
        !matches!(self, Self::Explicit(_))
    }

    pub(crate) fn reopen(self) -> Result<TrayProject, String> {
        match self {
            Self::Explicit(path) | Self::AbsoluteRecovery(path) => {
                external_cas_tray::reopen_project(&path).map_err(|e| e.to_string())
            }
            Self::PortableRecovery { root, locator } => {
                locator.validate()?;
                let draft = external_cas_tray::reopen_project(&root.join(&locator.revision_id))
                    .map_err(|e| format!("Saved portable recovery could not be opened: {e}"))?;
                if draft.manifest_sha256() != locator.manifest_sha256
                    || draft.manifest.draft.is_none()
                {
                    return Err(
                        "Saved portable recovery does not match its recorded revision".into(),
                    );
                }
                Ok(draft)
            }
        }
    }
}

pub(crate) fn save_draft_recovery(
    draft: &external_cas_tray::ProjectDraft,
    recovery_root: Option<&Path>,
) -> Result<SavedRecovery, String> {
    let root = recovery_root.ok_or("App recovery folder is unavailable; draft was not changed")?;
    // Like the session and catalog preflights, this does not bind future
    // operations to the same directory handles.
    reject_linked_recovery_path(root)?;
    std::fs::create_dir_all(root).map_err(|e| format!("Cannot create recovery folder: {e}"))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let revision_id = format!(
        "revision-{}-{}-{stamp}",
        draft.revision(),
        std::process::id()
    );
    let destination = root.join(&revision_id);
    let verified = draft.save_as(&destination).map_err(|e| {
        format!("Recovery revision could not be verified; edit remains unapplied: {e}")
    })?;
    Ok(SavedRecovery {
        path: destination,
        locator: RecoveryLocator {
            revision_id,
            manifest_sha256: verified.manifest_sha256().to_owned(),
        },
    })
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
    fn portable_recovery_uses_only_current_root_and_does_not_fall_back() {
        let old = OwnedMarkerFolder::new();
        let new = OwnedMarkerFolder::new();
        let paths = DataPaths::resolve(Some(new.0.clone()), None, None, None).unwrap();
        let absolute = old.0.join("Recovery").join("revision-2-1-100");
        let mut state = SessionState {
            project_path: Some(old.0.join("source-project")),
            ..SessionState::default()
        };
        SavedRecovery {
            path: absolute.clone(),
            locator: RecoveryLocator {
                revision_id: "revision-2-1-100".into(),
                manifest_sha256: "ab".repeat(32),
            },
        }
        .remember(&mut state, true);
        assert!(state.recovery_project_path.is_none());
        let saved = ProjectOpen::saved(&state, &paths).unwrap().unwrap();
        assert!(saved.is_recovery());
        let ProjectOpen::PortableRecovery { ref root, .. } = saved else {
            panic!("new records must not open an absolute project");
        };
        assert_eq!(root, &new.0.join("Recovery"));
        assert!(
            saved
                .reopen()
                .unwrap_err()
                .contains("Saved portable recovery")
        );
        assert!(ProjectOpen::saved(&state, &DataPaths::default()).is_err());
        assert!(state.extra.contains_key(PORTABLE_RECOVERY));
        assert_eq!(state.project_path, Some(old.0.join("source-project")));
        assert!(!new.0.join("Recovery").exists());

        let mut legacy = state.clone();
        legacy.extra.clear();
        legacy.recovery_project_path = Some(absolute.clone());
        assert!(matches!(
            ProjectOpen::saved(&legacy, &paths).unwrap(),
            Some(ProjectOpen::AbsoluteRecovery(path)) if path == absolute
        ));
        remember_project(&mut state, new.0.join("explicit"));
        assert!(matches!(
            ProjectOpen::saved(&state, &paths).unwrap(),
            Some(ProjectOpen::Explicit(path)) if path == new.0.join("explicit")
        ));
    }

    #[test]
    fn edited_portable_draft_reopens_from_copied_or_moved_session_storage() {
        use external_cas_session::SessionStore;
        use external_cas_tray::{ProjectDraft, create_project, scan_library};
        use std::fs;

        fn copy_owned_directory(from: &Path, to: &Path) {
            fs::create_dir(to).unwrap();
            for item in fs::read_dir(from).unwrap() {
                let item = item.unwrap();
                let next = to.join(item.file_name());
                if item.file_type().unwrap().is_dir() {
                    copy_owned_directory(&item.path(), &next);
                } else {
                    fs::copy(item.path(), next).unwrap();
                }
            }
        }

        let owned = OwnedMarkerFolder::new();
        let source = owned.0.join("tray");
        fs::create_dir(&source).unwrap();
        // Original minimal v1 Tray fixture: ID 77, one Sim, one outfit, one CASP.
        let metadata = [0, 0, 0, 0, 2, 0, 0, 0, 0x08, 77];
        let household = [
            1, 0, 0, 0, 41, 0, 0, 0, 10, 35, 8, 42, 50, 31, 9, 1, 0, 0, 0, 0, 0, 0, 0, 170, 1, 19,
            10, 17, 8, 1, 42, 9, 9, 11, 0, 0, 0, 0, 0, 0, 0, 58, 2, 8, 8,
        ];
        let source_house = source.join("0x00000000!0x000000000000004d.householdbinary");
        fs::write(
            source.join("0x00000000!0x000000000000004d.trayitem"),
            metadata,
        )
        .unwrap();
        fs::write(&source_house, household).unwrap();
        let group = scan_library(&source).unwrap().groups.remove(0);
        let project = create_project(&group, &owned.0.join("project")).unwrap();
        let mut draft = ProjectDraft::from_project(&project).unwrap();
        let slot = draft.slots().unwrap().remove(0);
        draft.apply_slot(0, &slot, 29, slot.body_type).unwrap();

        let old = owned.0.join("old portable");
        let paths = DataPaths::resolve(Some(old.clone()), None, None, None).unwrap();
        let recovery = save_draft_recovery(&draft, paths.recovery_root.as_deref()).unwrap();
        let mut state = SessionState {
            project_path: Some(project.directory.clone()),
            ..SessionState::default()
        };
        let old_revision = recovery.remember(&mut state, true);
        let store = SessionStore::at(paths.session_root.unwrap()).unwrap();
        let receipt = store.save(&state).unwrap();

        let new = owned.0.join("new portable");
        copy_owned_directory(&old, &new);
        let paths = DataPaths::resolve(Some(new.clone()), None, None, None).unwrap();
        let store = SessionStore::at(paths.session_root.clone().unwrap()).unwrap();
        let restored = store.load().unwrap();
        assert_eq!(restored.snapshot_sha256, Some(receipt.sha256));
        assert_eq!(restored.state.as_ref(), Some(&state));
        let reopen = || {
            ProjectOpen::saved(&state, &paths)
                .unwrap()
                .unwrap()
                .reopen()
                .unwrap()
        };
        let expected =
            fs::canonicalize(new.join("Recovery").join(old_revision.file_name().unwrap())).unwrap();
        for original_present in [true, false] {
            assert_eq!(old_revision.exists(), original_present);
            let recovered = reopen();
            assert_eq!(recovered.directory, expected);
            assert_eq!(
                ProjectDraft::from_project(&recovered)
                    .unwrap()
                    .slots()
                    .unwrap()[0]
                    .part_id,
                29
            );
            if original_present {
                fs::rename(&old, owned.0.join("removed old name")).unwrap();
            }
        }
        assert_eq!(fs::read(source_house).unwrap(), household);
        assert_eq!(store.load().unwrap().state, Some(state));
    }

    #[test]
    fn malformed_portable_recovery_never_chooses_legacy_path() {
        let folder = OwnedMarkerFolder::new();
        let paths = DataPaths::resolve(Some(folder.0.clone()), None, None, None).unwrap();
        let mut state = SessionState {
            project_path: Some(folder.0.join("source")),
            recovery_project_path: Some(folder.0.join("old")),
            ..SessionState::default()
        };
        for name in [
            "..",
            "../revision-2-1-1",
            "revision-2-1-1/.",
            r"revision-2-1-1\old",
            "revision-2-1-1:alternate",
            "revision-02-1-1",
            "revision-2-+1-1",
        ] {
            state.extra.insert(
                PORTABLE_RECOVERY.into(),
                serde_json::json!({"revision_id": name, "manifest_sha256": "ab".repeat(32)}),
            );
            assert!(ProjectOpen::saved(&state, &paths).is_err(), "{name}");
        }
        state.extra.insert(
            PORTABLE_RECOVERY.into(),
            serde_json::json!({
                "revision_id": "revision-2-1-1", "manifest_sha256": "AB".repeat(32)
            }),
        );
        assert!(ProjectOpen::saved(&state, &paths).is_err());
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
