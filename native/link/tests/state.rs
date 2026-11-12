// SPDX-License-Identifier: GPL-3.0-or-later
//! Deliberate model fixtures, not real game synchronization acceptance.
use external_cas_link::*;
use std::{collections::BTreeMap, sync::Arc};
fn identity(sim: u64) -> SimIdentity {
    SimIdentity {
        save: SaveIdentity {
            slot_id: 4,
            world_id: 8,
            lineage: [3; 32],
        },
        household_id: 12,
        sim_id: sim,
        form: Some(1),
    }
}
fn snapshot(sim: u64, value: u8) -> Snapshot {
    Snapshot::new(
        identity(sim),
        BTreeMap::from([
            ("outfits".into(), Arc::from(vec![value])),
            ("unknown-future-data".into(), Arc::from(vec![99, 88, 77])),
        ]),
        Coverage::CompletePreservationVerified,
    )
    .unwrap()
}
fn connection(controller: &mut LinkController, nonce: u8, qualified: bool) {
    if qualified {
        controller
            .register_local_write_qualification(WriteQualification {
                game_build: "fixture-1".into(),
                adapter_build: "fixture-only".into(),
                evidence_sha256: [42; 32],
                fields: vec![FieldCapability {
                    field: "outfits".into(),
                    read: true,
                    cas_write: true,
                    live_write: true,
                    rollback: true,
                    preservation_verified: true,
                }],
            })
            .unwrap();
    }
    controller.request_connect().unwrap();
    controller
        .connected(
            RuntimeSession {
                nonce: [nonce; 16],
                game_build: "fixture-1".into(),
                adapter_build: "fixture-only".into(),
                save: identity(1).save,
                zone_id: 19,
            },
            Capabilities {
                game_build: "fixture-1".into(),
                adapter_build: "fixture-only".into(),
                real_game_qualified: qualified,
                fields: vec![FieldCapability {
                    field: "outfits".into(),
                    read: true,
                    cas_write: true,
                    live_write: true,
                    rollback: true,
                    preservation_verified: true,
                }],
                unsupported: vec![],
            },
        )
        .unwrap();
}
fn load(controller: &mut LinkController, sim: u64, value: u8) {
    controller
        .request_switch(SwitchTarget::Sim(identity(sim)), SwitchDecision::Stash)
        .unwrap();
    let ticket = controller.issue_snapshot_request(identity(sim)).unwrap();
    controller
        .accept_snapshot(&ticket, snapshot(sim, value), None)
        .unwrap();
}
fn linked(qualified: bool) -> LinkController {
    let mut controller = LinkController::new(LinkLimits::default());
    connection(&mut controller, 1, qualified);
    controller
        .request_switch(
            SwitchTarget::Mode(LinkMode::LiveLink),
            SwitchDecision::Stash,
        )
        .unwrap();
    load(&mut controller, 1, 10);
    controller
}
#[test]
fn game_launch_selection_and_dirty_switch_cannot_hijack_standalone() {
    let mut c = LinkController::new(LinkLimits::default());
    c.edit(
        &ContextId::Standalone,
        0,
        "opaque-project".into(),
        Some(vec![5, 4, 3]),
    )
    .unwrap();
    connection(&mut c, 1, false);
    assert_eq!(
        c.observe_selection([1; 16], 1, Some(identity(7))).unwrap(),
        SelectionNotice::ObservedOnly
    );
    assert_eq!(c.mode, LinkMode::Standalone);
    assert!(c.selected.is_none());
    assert_eq!(&*c.standalone.fields["opaque-project"], &[5, 4, 3]);
    assert!(
        c.request_switch(SwitchTarget::Sim(identity(7)), SwitchDecision::Stash)
            .is_err()
    );
    assert!(matches!(
        c.request_switch(SwitchTarget::Mode(LinkMode::LiveLink), SwitchDecision::Ask)
            .unwrap(),
        SwitchOutcome::NeedsDecision { .. }
    ));
    assert_eq!(
        c.request_switch(
            SwitchTarget::Mode(LinkMode::LiveLink),
            SwitchDecision::Cancel
        )
        .unwrap(),
        SwitchOutcome::Cancelled
    );
    assert_eq!(c.mode, LinkMode::Standalone);
    assert!(
        c.request_switch(
            SwitchTarget::Mode(LinkMode::LiveLink),
            SwitchDecision::SaveAcknowledged {
                draft_version: 0,
                checkpoint_sha256: [1; 32]
            }
        )
        .is_err()
    );
    assert_eq!(c.mode, LinkMode::Standalone);
    c.request_switch(
        SwitchTarget::Mode(LinkMode::LiveLink),
        SwitchDecision::Stash,
    )
    .unwrap();
    load(&mut c, 7, 11);
    c.request_switch(
        SwitchTarget::Mode(LinkMode::Standalone),
        SwitchDecision::Stash,
    )
    .unwrap();
    assert_eq!(c.standalone.version, 1);
    assert_eq!(c.standalone.undo.len(), 1);
}
#[test]
fn deleting_last_standalone_field_is_still_a_dirty_edit() {
    let mut c = LinkController::new(LinkLimits::default());
    c.edit(&ContextId::Standalone, 0, "old".into(), Some(vec![1]))
        .unwrap();
    c.standalone.saved_version = Some(1);
    c.edit(&ContextId::Standalone, 1, "old".into(), None)
        .unwrap();
    assert!(c.standalone.fields.is_empty());
    assert!(c.dirty(&ContextId::Standalone));
    assert!(matches!(
        c.request_switch(SwitchTarget::Mode(LinkMode::LiveLink), SwitchDecision::Ask)
            .unwrap(),
        SwitchOutcome::NeedsDecision { .. }
    ));
}
#[test]
fn rapid_same_name_household_switch_retains_each_draft_and_refuses_stale_async() {
    let mut c = linked(false);
    let members = (1..=128)
        .map(|sim| HouseholdMember {
            identity: identity(sim),
            name: "Same Name".into(),
            portrait_resource: None,
            available: true,
        })
        .collect();
    c.replace_household([1; 16], members).unwrap();
    let mut previous = None;
    for sim in 1..=128 {
        c.request_switch(SwitchTarget::Sim(identity(sim)), SwitchDecision::Stash)
            .unwrap();
        let ticket = c.issue_snapshot_request(identity(sim)).unwrap();
        if let Some(old) = previous {
            assert!(c.accept_snapshot(&old, snapshot(sim - 1, 2), None).is_err());
        }
        c.accept_snapshot(&ticket, snapshot(sim, 10), None).unwrap();
        let key = ContextId::Sim(identity(sim));
        c.edit(&key, 0, "outfits".into(), Some(vec![(sim + 20) as u8]))
            .unwrap();
        previous = Some(ticket);
    }
    for sim in 1..=128 {
        let key = ContextId::Sim(identity(sim));
        assert_eq!(
            &*c.draft(&key).unwrap().fields["outfits"],
            &[(sim + 20) as u8]
        );
        c.undo(&key, 1).unwrap();
        assert_eq!(&*c.draft(&key).unwrap().fields["outfits"], &[10]);
        c.redo(&key, 2).unwrap();
    }
    assert_eq!(c.sims.len(), 128);
    c.disconnect().unwrap();
    connection(&mut c, 2, false);
    assert!(
        c.observe_selection([1; 16], 999, Some(identity(1)))
            .is_err()
    );
    assert_eq!(c.sims.len(), 128);
}
#[test]
fn concurrent_changes_require_explicit_three_way_resolution_and_preserve_unknowns() {
    let mut c = linked(false);
    let key = ContextId::Sim(identity(1));
    c.edit(&key, 0, "outfits".into(), Some(vec![11])).unwrap();
    let ticket = c.issue_snapshot_request(identity(1)).unwrap();
    let mut remote = snapshot(1, 12);
    remote
        .fields
        .insert("unknown-future-data".into(), Arc::from(vec![66]));
    remote.revision = remote.calculate_revision().unwrap();
    let revision = remote.revision;
    c.accept_snapshot(&ticket, remote, None).unwrap();
    assert!(c.sims[&identity(1)].conflict.is_some());
    assert!(
        c.reconcile_sim(&identity(1), 1, revision, BTreeMap::new())
            .is_err()
    );
    assert_eq!(&*c.draft(&key).unwrap().fields["outfits"], &[11]);
    c.reconcile_sim(
        &identity(1),
        1,
        revision,
        BTreeMap::from([("outfits".into(), Resolution::KeepExternal)]),
    )
    .unwrap();
    assert_eq!(&*c.draft(&key).unwrap().fields["outfits"], &[11]);
    assert_eq!(
        &*c.draft(&key).unwrap().fields["unknown-future-data"],
        &[66]
    );
    assert!(c.sims[&identity(1)].conflict.is_none());
    assert_eq!(c.sims[&identity(1)].base.revision, revision);
}
#[test]
fn actual_unqualified_contract_and_unsupported_fields_refuse_writes() {
    let mut c = linked(false);
    let key = ContextId::Sim(identity(1));
    c.edit(&key, 0, "outfits".into(), Some(vec![20])).unwrap();
    assert!(matches!(
        c.prepare_apply(&identity(1), 1, [2; 16]),
        Err(LinkError::Unsupported(_))
    ));
    assert!(c.operations.is_empty());
    c.capabilities.real_game_qualified = true;
    assert!(matches!(
        c.prepare_apply(&identity(1), 1, [2; 16]),
        Err(LinkError::Unsupported(_))
    ));
    c.edit(&key, 1, "unknown-future-data".into(), Some(vec![1]))
        .unwrap();
    assert!(matches!(
        c.prepare_apply(&identity(1), 2, [2; 16]),
        Err(LinkError::Unsupported(_))
    ));
    assert!(c.operations.is_empty());
}
#[test]
fn apply_requires_review_and_exact_readback_preserves_later_edits_and_ignores_echo() {
    let mut c = linked(true);
    let key = ContextId::Sim(identity(1));
    c.edit(&key, 0, "outfits".into(), Some(vec![20])).unwrap();
    let review = c.prepare_apply(&identity(1), 1, [2; 16]).unwrap();
    assert!(c.send_apply([2; 16]).is_err());
    assert!(c.review_apply([2; 16], [0; 32]).is_err());
    c.review_apply([2; 16], review).unwrap();
    let request = c.send_apply([2; 16]).unwrap();
    assert!(c.send_apply([2; 16]).is_err());
    assert_eq!(c.operations[&[2; 16]].phase, ApplyPhase::Sent);
    let ack = ApplyAcknowledgment {
        operation_id: [2; 16],
        identity: identity(1),
        session_nonce: [1; 16],
        result: ApplyResult::Applied {
            observed_revision: request.expected_after_revision,
        },
    };
    c.acknowledge_apply(ack.clone()).unwrap();
    assert_eq!(c.sims[&identity(1)].base.revision, request.base_revision);
    c.edit(&key, 1, "outfits".into(), Some(vec![21])).unwrap();
    let after = (*c.operations[&[2; 16]].expected_after).clone();
    assert_eq!(
        c.readback_apply([2; 16], [1; 16], after.clone()).unwrap(),
        ApplyPhase::Committed
    );
    assert_eq!(&*c.draft(&key).unwrap().fields["outfits"], &[21]);
    assert_eq!(&*c.sims[&identity(1)].base.fields["outfits"], &[20]);
    c.acknowledge_apply(ack).unwrap();
    let ticket = c.issue_snapshot_request(identity(1)).unwrap();
    c.accept_snapshot(&ticket, after, Some([2; 16])).unwrap();
    assert!(c.sims[&identity(1)].conflict.is_none());
    assert_eq!(c.draft(&key).unwrap().undo.len(), 2);
}
#[test]
fn rollback_is_guarded_by_observed_revision_and_unknown_changes_require_manual_recovery() {
    for change_unknown in [false, true] {
        let mut c = linked(true);
        c.edit(
            &ContextId::Sim(identity(1)),
            0,
            "outfits".into(),
            Some(vec![20]),
        )
        .unwrap();
        let review = c.prepare_apply(&identity(1), 1, [2; 16]).unwrap();
        c.review_apply([2; 16], review).unwrap();
        let request = c.send_apply([2; 16]).unwrap();
        c.acknowledge_apply(ApplyAcknowledgment {
            operation_id: [2; 16],
            identity: identity(1),
            session_nonce: [1; 16],
            result: ApplyResult::Applied {
                observed_revision: request.expected_after_revision,
            },
        })
        .unwrap();
        let mut observed = snapshot(1, 19);
        if change_unknown {
            observed
                .fields
                .insert("unknown-future-data".into(), Arc::from(vec![123]));
            observed.revision = observed.calculate_revision().unwrap();
        }
        c.readback_apply([2; 16], [1; 16], observed.clone())
            .unwrap();
        assert!(c.rollback_request([2; 16], &snapshot(1, 18)).is_err());
        if change_unknown {
            assert!(c.rollback_request([2; 16], &observed).is_err());
            assert!(matches!(
                c.operations[&[2; 16]].phase,
                ApplyPhase::RecoveryRequired { .. }
            ));
        } else {
            let rollback = c.rollback_request([2; 16], &observed).unwrap();
            assert_eq!(rollback.expected_current_revision, observed.revision);
            assert_eq!(
                c.readback_rollback([2; 16], [1; 16], snapshot(1, 10))
                    .unwrap(),
                ApplyPhase::RolledBack
            );
        }
    }
}
#[test]
fn crash_reopen_and_timeouts_preserve_pending_preimage_and_never_duplicate_apply() {
    let mut c = linked(true);
    let key = ContextId::Sim(identity(1));
    c.edit(&key, 0, "outfits".into(), Some(vec![20])).unwrap();
    let review = c.prepare_apply(&identity(1), 1, [2; 16]).unwrap();
    c.review_apply([2; 16], review).unwrap();
    c.send_apply([2; 16]).unwrap();
    let raw = serde_json::to_vec(&c.export_contexts()).unwrap();
    let saved: PersistedContexts = serde_json::from_slice(&raw).unwrap();
    let reopened = LinkController::restore_contexts(saved, LinkLimits::default()).unwrap();
    assert!(matches!(
        reopened.operations[&[2; 16]].phase,
        ApplyPhase::Indeterminate { .. }
    ));
    assert_eq!(reopened.connection, ConnectionState::Disconnected);
    assert_eq!(&*reopened.draft(&key).unwrap().fields["outfits"], &[20]);
    assert_eq!(reopened.draft(&key).unwrap().undo.len(), 1);
    c.apply_timeout([2; 16], "socket timeout".into()).unwrap();
    assert!(c.send_apply([2; 16]).is_err());
    assert!(c.prepare_apply(&identity(1), 1, [3; 16]).is_err());
    let mut tampered = c.export_contexts();
    tampered.operations[0].review_sha256 = [7; 32];
    assert!(LinkController::restore_contexts(tampered, LinkLimits::default()).is_err());
}
#[test]
fn bounds_cancel_departure_and_wrong_identity_leave_existing_context_intact() {
    let mut c = linked(true);
    c.limits.max_contexts = 1;
    c.request_switch(SwitchTarget::Sim(identity(2)), SwitchDecision::Stash)
        .unwrap();
    let ticket = c.issue_snapshot_request(identity(2)).unwrap();
    assert!(matches!(
        c.accept_snapshot(&ticket, snapshot(2, 1), None),
        Err(LinkError::Budget(_))
    ));
    assert_eq!(c.sims.len(), 1);
    c.request_switch(SwitchTarget::Sim(identity(1)), SwitchDecision::Stash)
        .unwrap();
    c.edit(
        &ContextId::Sim(identity(1)),
        0,
        "outfits".into(),
        Some(vec![20]),
    )
    .unwrap();
    c.target_unavailable([1; 16], &identity(1), "Sim despawned".into())
        .unwrap();
    assert!(c.prepare_apply(&identity(1), 1, [2; 16]).is_err());
    assert_eq!(&*c.sims[&identity(1)].draft.fields["outfits"], &[20]);
    c.limits.max_retained_bytes = 1;
    let before = c.standalone.version;
    assert!(
        c.edit(
            &ContextId::Standalone,
            before,
            "new".into(),
            Some(vec![1, 2, 3])
        )
        .is_err()
    );
    assert_eq!(c.standalone.version, before);
}
