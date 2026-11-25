// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit, read-only checks of one user-selected package. No installation.

use crate::cc_evidence::{Candidate, Digest};
use external_cas_assets::{PackageIndex, ReadLimits, ResourceKey};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuppliedPackage {
    UnresolvedClaim,
    MissingResource,
    AmbiguousResource,
    DeletedResource,
    DifferentBytes,
    /// The explicit file matched the expected package digest and contained one
    /// decodable target resource. Not provenance, installation, dependency
    /// closure, effective loader priority, or consent to write.
    BytesMatch {
        key: ResourceKey,
        package_sha256: Digest,
    },
}

/// Compare a single, explicitly chosen local file against independently
/// collected expectations. Do not attach provider IDs to the local inventory
/// on the basis of this result. Call again when advancing past review.
pub fn check_supplied_package(
    path: &Path,
    expected: &Candidate<'_>,
    limits: ReadLimits,
    cancel: &AtomicBool,
) -> external_cas_assets::Result<SuppliedPackage> {
    if cancel.load(Ordering::Relaxed) {
        return Err(external_cas_assets::Error::Cancelled);
    }
    if !expected.file.complete() || expected.key.is_empty() {
        return Ok(SuppliedPackage::UnresolvedClaim);
    }
    let index = PackageIndex::open_cancellable(path, cancel)?;
    let mut matches = index.matching(expected.key);
    let Some(entry) = matches.next() else {
        return Ok(SuppliedPackage::MissingResource);
    };
    if matches.next().is_some() {
        return Ok(SuppliedPackage::AmbiguousResource);
    }
    if entry.deleted() {
        return Ok(SuppliedPackage::DeletedResource);
    }
    let mut reader = index.reader_cancellable(cancel)?;
    let package_sha256 = reader.package_sha256(cancel)?;
    if package_sha256 != expected.digest {
        return Ok(SuppliedPackage::DifferentBytes);
    }
    reader.read_resource_cancellable(entry, limits, cancel)?;
    Ok(SuppliedPackage::BytesMatch {
        key: expected.key,
        package_sha256,
    })
}
