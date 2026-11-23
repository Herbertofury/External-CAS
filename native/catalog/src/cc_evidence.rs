// SPDX-License-Identifier: GPL-3.0-or-later
//! Pure reconciliation of already collected CC evidence. Does not locate,
//! acquire, install, or substitute any resource, or certify in-game behavior.

use external_cas_assets::ResourceKey;

pub type Digest = [u8; 32];

/// Opaque provider IDs, not display strings. All five components must be present.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId<'a> {
    pub provider: &'a str,
    pub creator: &'a str,
    pub project: &'a str,
    pub release: &'a str,
    pub file: &'a str,
}

impl FileId<'_> {
    pub(crate) fn complete(self) -> bool {
        [
            self.provider,
            self.creator,
            self.project,
            self.release,
            self.file,
        ]
        .into_iter()
        .all(|part| !part.trim().is_empty())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Public,
    Restricted,
    Unavailable,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessEvidence {
    /// Current authoritative evidence for one exact provider file.
    Confirmed(Access),
    /// A transport or authentication failure is not evidence of deletion.
    Unverified,
    Offline,
}

#[derive(Debug, Clone, Copy)]
pub enum Requirement<'a> {
    Exact {
        file: FileId<'a>,
        key: ResourceKey,
        digest: Digest,
    },
    /// For example, an association with a named creator in a Sim bundle, but
    /// no verified file ID. Evaluate only on the exact file holding the report.
    Unresolved,
}

#[derive(Debug)]
pub struct Candidate<'a> {
    pub file: FileId<'a>,
    pub key: ResourceKey,
    pub digest: Digest,
    pub requirements: &'a [Requirement<'a>],
    pub access: AccessEvidence,
}

#[derive(Debug, Clone, Copy)]
pub struct InventoryResource<'a> {
    /// None when independent evidence has not mapped the bytes to a provider ID.
    pub file: Option<FileId<'a>>,
    pub key: ResourceKey,
    pub digest: Digest,
    /// The caller supplies current game/loader precedence, larger wins.
    /// Ties stay ambiguous unless every winning receipt agrees on ID and bytes.
    pub precedence: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryState {
    UnresolvedIdentity,
    MissingResource,
    AmbiguousOverride,
    DifferentSource,
    ChangedBytes,
    /// Local byte and effective resource identity match. Not a runtime claim.
    LocalEvidenceMatches,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assessment {
    pub access: Access,
    pub offline: bool,
    pub inventory: InventoryState,
    /// One entry for every declared requirement, in declaration order.
    pub requirements: Vec<InventoryState>,
}

fn check_resource(
    file: FileId<'_>,
    key: ResourceKey,
    digest: Digest,
    inventory: &[InventoryResource<'_>],
) -> InventoryState {
    if !file.complete() {
        return InventoryState::UnresolvedIdentity;
    }
    let mut winner: Option<&InventoryResource<'_>> = None;
    let mut ambiguous = false;
    for resource in inventory.iter().filter(|resource| resource.key == key) {
        match winner {
            Some(existing) if existing.precedence > resource.precedence => {}
            Some(existing) if existing.precedence == resource.precedence => {
                ambiguous |= existing.file != resource.file
                    || existing.digest != resource.digest
                    || existing.file.is_none();
            }
            _ => {
                winner = Some(resource);
                ambiguous = false;
            }
        }
    }
    let Some(winner) = winner else {
        return InventoryState::MissingResource;
    };
    if ambiguous {
        return InventoryState::AmbiguousOverride;
    }
    match winner.file {
        None => InventoryState::UnresolvedIdentity,
        Some(other) if other != file => InventoryState::DifferentSource,
        Some(_) if winner.digest != digest => InventoryState::ChangedBytes,
        Some(_) => InventoryState::LocalEvidenceMatches,
    }
}

pub fn assess(candidate: &Candidate<'_>, inventory: &[InventoryResource<'_>]) -> Assessment {
    let requirements = candidate
        .requirements
        .iter()
        .map(|requirement| match requirement {
            Requirement::Unresolved => InventoryState::UnresolvedIdentity,
            Requirement::Exact { file, key, digest } => {
                check_resource(*file, *key, *digest, inventory)
            }
        })
        .collect();
    Assessment {
        access: match candidate.access {
            AccessEvidence::Confirmed(access) if candidate.file.complete() => access,
            _ => Access::Unknown,
        },
        offline: candidate.access == AccessEvidence::Offline,
        inventory: check_resource(candidate.file, candidate.key, candidate.digest, inventory),
        requirements,
    }
}

impl Assessment {
    pub fn local_evidence_matches(&self) -> bool {
        self.inventory == InventoryState::LocalEvidenceMatches
            && self
                .requirements
                .iter()
                .all(|state| *state == InventoryState::LocalEvidenceMatches)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Choice<'a> {
    Cancel,
    Skip,
    SupplyLocalFile,
    ChooseInstalledAlternative(&'a Candidate<'a>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Review<'a> {
    Cancelled,
    Skipped,
    NeedsLocalRescan,
    AlternativeNotVerified,
    /// A review outcome only; the caller still needs consent at the write boundary.
    ExplicitAlternative {
        file: FileId<'a>,
        key: ResourceKey,
        digest: Digest,
    },
}

pub fn review<'a>(
    original: FileId<'_>,
    choice: Choice<'a>,
    current_inventory: &[InventoryResource<'_>],
) -> Review<'a> {
    match choice {
        Choice::Cancel => Review::Cancelled,
        Choice::Skip => Review::Skipped,
        Choice::SupplyLocalFile => Review::NeedsLocalRescan,
        Choice::ChooseInstalledAlternative(candidate)
            if candidate.file != original
                && assess(candidate, current_inventory).local_evidence_matches() =>
        {
            Review::ExplicitAlternative {
                file: candidate.file,
                key: candidate.key,
                digest: candidate.digest,
            }
        }
        Choice::ChooseInstalledAlternative(_) => Review::AlternativeNotVerified,
    }
}
