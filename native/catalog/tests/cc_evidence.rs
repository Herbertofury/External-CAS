// SPDX-License-Identifier: GPL-3.0-or-later
use external_cas_assets::ResourceKey;
use external_cas_catalog_cache::cc_evidence::*;

const SKIN: ResourceKey = ResourceKey::new(1, 0, 1);
const MESH: ResourceKey = ResourceKey::new(2, 0, 7);
const FILE: FileId<'static> = FileId {
    provider: "provider-a",
    creator: "creator-3",
    project: "1001",
    release: "4",
    file: "25",
};
const MIRROR: FileId<'static> = FileId {
    provider: "provider-b",
    ..FILE
};
const UNKNOWN_FILE: FileId<'static> = FileId { file: "", ..FILE };
const MESH_FILE: FileId<'static> = FileId {
    creator: "creator-13",
    file: "150",
    ..FILE
};
const GOOD: Digest = [1; 32];
const CHANGED: Digest = [2; 32];

fn candidate<'a>(requirements: &'a [Requirement<'a>]) -> Candidate<'a> {
    Candidate {
        file: FILE,
        key: SKIN,
        digest: GOOD,
        requirements,
        access: AccessEvidence::Unverified,
    }
}

fn receipt(file: FileId<'static>, key: ResourceKey, precedence: u32) -> InventoryResource<'static> {
    InventoryResource {
        file: Some(file),
        key,
        digest: GOOD,
        precedence,
    }
}

#[test]
fn same_file_number_and_bytes_are_not_a_match_on_another_provider() {
    let c = candidate(&[]);
    for other in [
        MIRROR,
        FileId {
            creator: "creator-13",
            ..FILE
        },
        FileId {
            release: "5",
            ..FILE
        },
    ] {
        let local = [receipt(other, SKIN, 4)];
        assert_eq!(
            assess(&c, &local).inventory,
            InventoryState::DifferentSource
        );
    }
    let local = [InventoryResource {
        file: None,
        ..receipt(FILE, SKIN, 4)
    }];
    assert_eq!(
        assess(&c, &local).inventory,
        InventoryState::UnresolvedIdentity
    );
}

#[test]
fn override_priority_and_equal_precedence_are_order_independent() {
    let c = candidate(&[]);
    let expected = receipt(FILE, SKIN, 4);
    let conflict = receipt(MIRROR, SKIN, 4);
    let dominates = receipt(FILE, SKIN, 5);
    let lower = receipt(MIRROR, SKIN, 2);
    for same_priority in [[expected, conflict, lower], [lower, conflict, expected]] {
        assert_eq!(
            assess(&c, &same_priority).inventory,
            InventoryState::AmbiguousOverride
        );
    }
    for winning in [
        [dominates, expected, conflict],
        [expected, conflict, dominates],
    ] {
        assert!(assess(&c, &winning).local_evidence_matches());
    }
    let override_wins = [expected, receipt(MIRROR, SKIN, 6)];
    assert_eq!(
        assess(&c, &override_wins).inventory,
        InventoryState::DifferentSource
    );
}

#[test]
fn changed_bytes_and_each_missing_or_unresolved_requirement_stay_visible() {
    let declared = [
        Requirement::Exact {
            file: MESH_FILE,
            key: MESH,
            digest: GOOD,
        },
        Requirement::Unresolved,
    ];
    let c = candidate(&declared);
    let skin = receipt(FILE, SKIN, 5);
    let mesh = receipt(MESH_FILE, MESH, 5);
    let changed = InventoryResource {
        digest: CHANGED,
        ..skin
    };
    let missing = assess(&c, &[changed]);
    assert_eq!(missing.inventory, InventoryState::ChangedBytes);
    assert_eq!(
        missing.requirements,
        [
            InventoryState::MissingResource,
            InventoryState::UnresolvedIdentity
        ]
    );
    let matched = assess(&c, &[skin, mesh]);
    assert_eq!(
        matched.requirements,
        [
            InventoryState::LocalEvidenceMatches,
            InventoryState::UnresolvedIdentity
        ]
    );
    assert!(!matched.local_evidence_matches());
    let only_exact = candidate(&declared[..1]);
    assert!(assess(&only_exact, &[skin, mesh]).local_evidence_matches());
}

#[test]
fn bundle_requirement_is_not_inherited_by_a_sibling_overlay() {
    let bundle = candidate(&[Requirement::Unresolved]);
    let overlay = Candidate {
        file: FileId { file: "26", ..FILE },
        ..candidate(&[])
    };
    assert!(!assess(&bundle, &[receipt(FILE, SKIN, 1)]).local_evidence_matches());
    assert!(assess(&overlay, &[receipt(overlay.file, SKIN, 1)]).local_evidence_matches());
}

#[test]
fn current_exact_access_is_not_installed_status_and_offline_is_not_removed() {
    let mut c = candidate(&[]);
    for access in [
        Access::Public,
        Access::Restricted,
        Access::Unavailable,
        Access::Unknown,
    ] {
        c.access = AccessEvidence::Confirmed(access);
        let assessment = assess(&c, &[]);
        assert_eq!(assessment.access, access);
        assert_eq!(assessment.inventory, InventoryState::MissingResource);
        assert!(!assessment.offline);
    }
    c.access = AccessEvidence::Offline;
    let offline = assess(&c, &[receipt(FILE, SKIN, 1)]);
    assert_eq!(offline.access, Access::Unknown);
    assert!(offline.offline && offline.local_evidence_matches());
    c.file = UNKNOWN_FILE;
    c.access = AccessEvidence::Confirmed(Access::Public);
    assert_eq!(assess(&c, &[]).access, Access::Unknown);
}

#[test]
fn review_is_explicit_and_rescans_supplied_or_changed_evidence() {
    let alternative = Candidate {
        file: MIRROR,
        ..candidate(&[])
    };
    let select = Choice::ChooseInstalledAlternative(&alternative);
    let local = receipt(MIRROR, SKIN, 1);
    assert_eq!(review(FILE, Choice::Cancel, &[local]), Review::Cancelled);
    assert_eq!(review(FILE, Choice::Skip, &[local]), Review::Skipped);
    assert_eq!(
        review(FILE, Choice::SupplyLocalFile, &[local]),
        Review::NeedsLocalRescan
    );
    assert_eq!(review(FILE, select, &[]), Review::AlternativeNotVerified);
    assert_eq!(
        review(FILE, select, &[local]),
        Review::ExplicitAlternative {
            file: MIRROR,
            key: SKIN,
            digest: GOOD
        }
    );
    assert_eq!(
        review(MIRROR, select, &[local]),
        Review::AlternativeNotVerified
    );
    let changed = InventoryResource {
        digest: CHANGED,
        ..local
    };
    assert_eq!(
        review(FILE, select, &[changed]),
        Review::AlternativeNotVerified
    );
}
