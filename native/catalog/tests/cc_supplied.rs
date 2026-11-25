// SPDX-License-Identifier: GPL-3.0-or-later
use external_cas_assets::{Error, PackageIndex, ReadLimits, ResourceKey, dbpf::CASP};
use external_cas_catalog_cache::{cc_evidence::*, cc_supplied::*};
use sha2::{Digest as _, Sha256};
use std::{fs, path::Path, sync::atomic::AtomicBool};

const KEY: ResourceKey = ResourceKey::new(CASP, 2, 3);
const FILE: FileId<'static> = FileId {
    provider: "test-provider",
    creator: "test-creator",
    project: "p1",
    release: "r2",
    file: "f3",
};

fn package(payload: &[u8; 4], rows: &[(ResourceKey, u16)]) -> Vec<u8> {
    let mut bytes = vec![0; 96];
    bytes[..4].copy_from_slice(b"DBPF");
    for (at, val) in [
        (4, 2),
        (8, 1),
        (36, rows.len() as u32),
        (44, 4 + 32 * rows.len() as u32),
        (64, 100),
    ] {
        bytes[at..at + 4].copy_from_slice(&val.to_le_bytes());
    }
    bytes.extend(payload);
    bytes.extend(0u32.to_le_bytes());
    for &(key, marker) in rows {
        for val in [
            key.type_id,
            key.group_id,
            (key.instance_id >> 32) as u32,
            key.instance_id as u32,
            96,
            0x80000004,
            4,
        ] {
            bytes.extend(val.to_le_bytes());
        }
        bytes.extend(marker.to_le_bytes());
        bytes.extend(1u16.to_le_bytes());
    }
    bytes
}

fn expected(bytes: &[u8]) -> Candidate<'static> {
    Candidate {
        file: FILE,
        key: KEY,
        digest: Sha256::digest(bytes).into(),
        requirements: &[Requirement::Unresolved],
        access: AccessEvidence::Offline,
    }
}

fn check(path: &Path, c: &Candidate<'_>) -> external_cas_assets::Result<SuppliedPackage> {
    check_supplied_package(path, c, ReadLimits::default(), &AtomicBool::new(false))
}

#[test]
fn explicitly_supplied_file_matching_bytes_still_is_not_installed_or_attributed() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("same-name.package");
    let bytes = package(b"abcd", &[(KEY, 0)]);
    fs::write(&path, &bytes).unwrap();
    let c = expected(&bytes);
    assert_eq!(
        review(FILE, Choice::SupplyLocalFile, &[]),
        Review::NeedsLocalRescan
    );
    assert_eq!(
        check(&path, &c).unwrap(),
        SuppliedPackage::BytesMatch {
            key: KEY,
            package_sha256: c.digest,
        }
    );
    assert_eq!(assess(&c, &[]).inventory, InventoryState::MissingResource);
    assert_eq!(
        assess(&c, &[]).requirements,
        [InventoryState::UnresolvedIdentity]
    );
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn rechecking_same_name_and_same_index_does_not_reuse_old_success() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("same-name.package");
    let a = package(b"abcd", &[(KEY, 0)]);
    let b = package(b"dcba", &[(KEY, 0)]);
    let c = expected(&a);
    fs::write(&path, a).unwrap();
    let first = PackageIndex::open(&path).unwrap();
    assert!(matches!(
        check(&path, &c).unwrap(),
        SuppliedPackage::BytesMatch { .. }
    ));
    fs::write(&path, b).unwrap();
    let second = PackageIndex::open(&path).unwrap();
    assert_eq!(first.file_size, second.file_size);
    assert_eq!(first.index_sha256, second.index_sha256);
    assert_eq!(check(&path, &c).unwrap(), SuppliedPackage::DifferentBytes);
}

#[test]
fn matching_digest_does_not_promote_missing_deleted_duplicate_or_unsupported_resources() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("selected.package");
    for (rows, verdict) in [
        (
            vec![(ResourceKey::new(CASP, 2, 4), 0)],
            SuppliedPackage::MissingResource,
        ),
        (vec![(KEY, 0xffe0)], SuppliedPackage::DeletedResource),
        (vec![(KEY, 0), (KEY, 0)], SuppliedPackage::AmbiguousResource),
    ] {
        let bytes = package(b"abcd", &rows);
        fs::write(&path, &bytes).unwrap();
        assert_eq!(check(&path, &expected(&bytes)).unwrap(), verdict);
    }
    let bytes = package(b"abcd", &[(KEY, 0xbeef)]);
    fs::write(&path, &bytes).unwrap();
    assert!(matches!(
        check(&path, &expected(&bytes)),
        Err(Error::Unsupported(_))
    ));
}

#[test]
fn cancellation_and_incomplete_expectations_do_not_open_the_file() {
    let root = tempfile::tempdir().unwrap();
    let nonexistent = root.path().join("absent.package");
    let mut c = expected(b"bytes");
    assert!(matches!(
        check_supplied_package(
            &nonexistent,
            &c,
            ReadLimits::default(),
            &AtomicBool::new(true)
        ),
        Err(Error::Cancelled)
    ));
    c.file.file = " ";
    assert_eq!(
        check(&nonexistent, &c).unwrap(),
        SuppliedPackage::UnresolvedClaim
    );
}
