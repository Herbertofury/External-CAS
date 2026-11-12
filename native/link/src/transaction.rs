// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit compare-and-swap apply journal. Acknowledgment is never readback.
use crate::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, sync::Arc};
pub type OperationId = [u8; 16];
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldPatch {
    pub field: String,
    pub before: Option<Arc<[u8]>>,
    pub after: Option<Arc<[u8]>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApplyPhase {
    Prepared,
    Reviewed,
    Sent,
    AwaitingReadback,
    Committed,
    Rejected { reason: String },
    RollbackRequired { reason: String },
    RollbackSent,
    RolledBack,
    RecoveryRequired { reason: String },
    Indeterminate { reason: String },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyTransaction {
    pub operation_id: OperationId,
    pub identity: SimIdentity,
    pub session_nonce: [u8; 16],
    pub mode: LinkMode,
    pub draft_version: u64,
    pub preimage: Arc<Snapshot>,
    pub expected_after: Arc<Snapshot>,
    pub patches: Vec<FieldPatch>,
    pub review_sha256: Digest32,
    pub phase: ApplyPhase,
    pub observed_after: Option<Arc<Snapshot>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApplyRequest {
    pub operation_id: OperationId,
    pub identity: SimIdentity,
    pub session_nonce: [u8; 16],
    pub mode: LinkMode,
    pub base_revision: Digest32,
    pub expected_after_revision: Digest32,
    pub review_sha256: Digest32,
    pub patches: Vec<FieldPatch>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ApplyResult {
    Applied {
        observed_revision: Digest32,
    },
    Rejected {
        unchanged_verified: bool,
        reason: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApplyAcknowledgment {
    pub operation_id: OperationId,
    pub identity: SimIdentity,
    pub session_nonce: [u8; 16],
    pub result: ApplyResult,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RollbackRequest {
    pub operation_id: OperationId,
    pub identity: SimIdentity,
    pub session_nonce: [u8; 16],
    pub expected_current_revision: Digest32,
    pub patches: Vec<FieldPatch>,
    pub expected_restored_revision: Digest32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OperationReadbackRequest {
    pub operation_id: OperationId,
    pub identity: SimIdentity,
    pub session_nonce: [u8; 16],
}
impl ApplyTransaction {
    pub fn validate(&self) -> Result<()> {
        self.preimage.validate()?;
        self.expected_after.validate()?;
        if self.operation_id == [0; 16]
            || self.session_nonce == [0; 16]
            || self.mode == LinkMode::Standalone
            || self.preimage.identity != self.identity
            || self.expected_after.identity != self.identity
            || self.preimage.coverage != self.expected_after.coverage
        {
            return Err(LinkError::Invalid(
                "Persisted operation identity or mode differs".into(),
            ));
        }
        let fields: BTreeSet<_> = self
            .preimage
            .fields
            .keys()
            .chain(self.expected_after.fields.keys())
            .cloned()
            .collect();
        let expected = fields
            .into_iter()
            .filter_map(|field| {
                let before = self.preimage.fields.get(&field).cloned();
                let after = self.expected_after.fields.get(&field).cloned();
                (before != after).then_some(FieldPatch {
                    field,
                    before,
                    after,
                })
            })
            .collect::<Vec<_>>();
        let hash: Digest32 = Sha256::digest(serde_json::to_vec(&(
            self.operation_id,
            &self.identity,
            self.session_nonce,
            self.mode,
            self.preimage.revision,
            self.expected_after.revision,
            &self.patches,
        ))?)
        .into();
        if expected.is_empty() || expected != self.patches || hash != self.review_sha256 {
            return Err(LinkError::Invalid(
                "Persisted reviewed diff/preimages differ".into(),
            ));
        }
        if let Some(observed) = &self.observed_after {
            observed.validate()?;
            if observed.identity != self.identity {
                return Err(LinkError::Invalid(
                    "Persisted operation readback identity differs".into(),
                ));
            }
        }
        Ok(())
    }
}
impl LinkController {
    pub fn operation_readback_request(&self, id: OperationId) -> Result<OperationReadbackRequest> {
        let transaction = self
            .operations
            .get(&id)
            .ok_or_else(|| LinkError::Invalid("Unknown operation".into()))?;
        if self.session()?.nonce != transaction.session_nonce
            || !matches!(
                transaction.phase,
                ApplyPhase::Sent
                    | ApplyPhase::AwaitingReadback
                    | ApplyPhase::RollbackRequired { .. }
                    | ApplyPhase::RollbackSent
                    | ApplyPhase::Indeterminate { .. }
            )
        {
            return Err(LinkError::Stale(
                "Operation readback phase/session differs".into(),
            ));
        }
        Ok(OperationReadbackRequest {
            operation_id: id,
            identity: transaction.identity.clone(),
            session_nonce: transaction.session_nonce,
        })
    }
    pub fn prepare_apply(
        &mut self,
        identity: &SimIdentity,
        expected_draft_version: u64,
        operation_id: OperationId,
    ) -> Result<Digest32> {
        if self.operations.len() >= self.limits.max_history {
            return Err(LinkError::Budget(
                "Operation journal; preserve unresolved operations before starting more".into(),
            ));
        }
        if operation_id == [0; 16] || self.operations.contains_key(&operation_id) {
            return Err(LinkError::Invalid(
                "Fresh operation ID required; query status of existing operations".into(),
            ));
        }
        if self.mode == LinkMode::Standalone || self.selected.as_ref() != Some(identity) {
            return Err(LinkError::Unsupported(
                "Explicit linked mode and exact selected Sim required".into(),
            ));
        }
        let session = self.session()?;
        if session.save != identity.save
            || !self.capabilities.real_game_qualified
            || self.capabilities.game_build != session.game_build
            || self.capabilities.adapter_build != session.adapter_build
        {
            return Err(LinkError::Unsupported(
                "No real-game-qualified write contract for this exact build/session".into(),
            ));
        }
        let context = self
            .sims
            .get(identity)
            .ok_or_else(|| LinkError::Invalid("Target Sim snapshot unavailable".into()))?;
        if context.departed
            || context.conflict.is_some()
            || context.remote.is_some()
            || context.draft.version != expected_draft_version
        {
            return Err(LinkError::Stale(
                "Target/draft has pending divergence or departure".into(),
            ));
        }
        if context.base.coverage != Coverage::CompletePreservationVerified {
            return Err(LinkError::Unsupported(
                "Partial snapshots cannot replace a complete Sim".into(),
            ));
        }
        let mut patches = Vec::new();
        let fields: BTreeSet<_> = context
            .base
            .fields
            .keys()
            .chain(context.draft.fields.keys())
            .cloned()
            .collect();
        for field in fields {
            let before = context.base.fields.get(&field).cloned();
            let after = context.draft.fields.get(&field).cloned();
            if before == after {
                continue;
            }
            let qualified = self
                .capabilities
                .fields
                .iter()
                .find(|c| c.field == field)
                .is_some_and(|c| {
                    c.read
                        && c.rollback
                        && c.preservation_verified
                        && match self.mode {
                            LinkMode::CasSync => c.cas_write,
                            LinkMode::LiveLink => c.live_write,
                            LinkMode::Standalone => false,
                        }
                });
            if !qualified || !self.locally_qualified(&field, self.mode) {
                return Err(LinkError::Unsupported(format!(
                    "Field {field} has no qualified {:?} write+rollback capability",
                    self.mode
                )));
            }
            patches.push(FieldPatch {
                field,
                before,
                after,
            });
        }
        if patches.is_empty() {
            return Err(LinkError::Invalid("No external changes to apply".into()));
        }
        if self.operations.values().any(|o| {
            o.identity == *identity
                && !matches!(
                    o.phase,
                    ApplyPhase::Committed | ApplyPhase::Rejected { .. } | ApplyPhase::RolledBack
                )
        }) {
            return Err(LinkError::Stale(
                "Existing operation for this Sim requires status/readback/recovery".into(),
            ));
        }
        let after = Arc::new(Snapshot::new(
            identity.clone(),
            context.draft.fields.clone(),
            context.base.coverage.clone(),
        )?);
        let review_sha256 = Sha256::digest(serde_json::to_vec(&(
            operation_id,
            identity,
            session.nonce,
            self.mode,
            context.base.revision,
            after.revision,
            &patches,
        ))?)
        .into();
        let transaction = ApplyTransaction {
            operation_id,
            identity: identity.clone(),
            session_nonce: session.nonce,
            mode: self.mode,
            draft_version: expected_draft_version,
            preimage: context.base.clone(),
            expected_after: after,
            patches,
            review_sha256,
            phase: ApplyPhase::Prepared,
            observed_after: None,
        };
        self.operations.insert(operation_id, transaction);
        if self.retained_bytes() > self.limits.max_retained_bytes {
            self.operations.remove(&operation_id);
            return Err(LinkError::Budget(
                "Apply preimage/journal; draft retained".into(),
            ));
        }
        Ok(review_sha256)
    }
    pub fn review_apply(&mut self, id: OperationId, review_sha256: Digest32) -> Result<()> {
        let transaction = self
            .operations
            .get_mut(&id)
            .ok_or_else(|| LinkError::Invalid("Unknown operation".into()))?;
        if transaction.phase != ApplyPhase::Prepared || transaction.review_sha256 != review_sha256 {
            return Err(LinkError::Stale(
                "Reviewed diff does not match prepared transaction".into(),
            ));
        }
        transaction.phase = ApplyPhase::Reviewed;
        Ok(())
    }
    fn validate_transaction_target(
        &self,
        transaction: &ApplyTransaction,
        require_original_draft: bool,
    ) -> Result<()> {
        let session = self.session()?;
        if !self.capabilities.real_game_qualified
            || self.capabilities.game_build != session.game_build
            || self.capabilities.adapter_build != session.adapter_build
            || transaction.patches.iter().any(|p| {
                !self.capabilities.fields.iter().any(|c| {
                    c.field == p.field
                        && c.read
                        && c.rollback
                        && c.preservation_verified
                        && match transaction.mode {
                            LinkMode::LiveLink => c.live_write,
                            LinkMode::CasSync => c.cas_write,
                            LinkMode::Standalone => false,
                        }
                })
            })
        {
            return Err(LinkError::Unsupported(
                "Write/rollback capabilities changed after review".into(),
            ));
        }
        if transaction
            .patches
            .iter()
            .any(|patch| !self.locally_qualified(&patch.field, transaction.mode))
        {
            return Err(LinkError::Unsupported(
                "Local qualification contract changed or is unavailable".into(),
            ));
        }
        let context = self
            .sims
            .get(&transaction.identity)
            .ok_or_else(|| LinkError::Stale("Apply target departed from context".into()))?;
        if session.nonce != transaction.session_nonce
            || session.save != transaction.identity.save
            || self.mode != transaction.mode
            || self.selected.as_ref() != Some(&transaction.identity)
            || context.departed
            || context.conflict.is_some()
            || context.remote.is_some()
            || context.base.revision != transaction.preimage.revision
            || require_original_draft && context.draft.version != transaction.draft_version
        {
            return Err(LinkError::Stale(
                "Apply target/session/mode/revision changed".into(),
            ));
        }
        Ok(())
    }
    /// Mark sent BEFORE I/O and persist the controller checkpoint. If I/O then
    /// fails, query operation status; never send another Apply blindly.
    pub fn send_apply(&mut self, id: OperationId) -> Result<ApplyRequest> {
        let transaction = self
            .operations
            .get(&id)
            .ok_or_else(|| LinkError::Invalid("Unknown operation".into()))?;
        if transaction.phase != ApplyPhase::Reviewed {
            return Err(LinkError::Stale(
                "Apply has not been reviewed or was already sent".into(),
            ));
        }
        self.validate_transaction_target(transaction, true)?;
        let request = ApplyRequest {
            operation_id: id,
            identity: transaction.identity.clone(),
            session_nonce: transaction.session_nonce,
            mode: transaction.mode,
            base_revision: transaction.preimage.revision,
            expected_after_revision: transaction.expected_after.revision,
            review_sha256: transaction.review_sha256,
            patches: transaction.patches.clone(),
        };
        self.operations.get_mut(&id).unwrap().phase = ApplyPhase::Sent;
        Ok(request)
    }
    pub fn acknowledge_apply(&mut self, ack: ApplyAcknowledgment) -> Result<()> {
        let transaction = self
            .operations
            .get_mut(&ack.operation_id)
            .ok_or_else(|| LinkError::Invalid("Unknown operation acknowledgement".into()))?;
        if ack.identity != transaction.identity || ack.session_nonce != transaction.session_nonce {
            return Err(LinkError::Stale(
                "Acknowledgement target/session differs".into(),
            ));
        }
        if matches!(
            transaction.phase,
            ApplyPhase::Committed | ApplyPhase::RolledBack
        ) {
            return match ack.result {
                ApplyResult::Applied { observed_revision }
                    if transaction.phase == ApplyPhase::Committed
                        && observed_revision == transaction.expected_after.revision =>
                {
                    Ok(())
                }
                _ => Err(LinkError::Stale(
                    "Repeated acknowledgement differs from completed transaction".into(),
                )),
            };
        }
        if !matches!(
            transaction.phase,
            ApplyPhase::Sent | ApplyPhase::AwaitingReadback | ApplyPhase::Indeterminate { .. }
        ) {
            return Err(LinkError::Stale(
                "Acknowledgement is out of transaction order".into(),
            ));
        }
        transaction.phase=match ack.result{ApplyResult::Applied{observed_revision}if observed_revision==transaction.expected_after.revision=>ApplyPhase::AwaitingReadback,ApplyResult::Applied{..}=>ApplyPhase::RollbackRequired{reason:"Adapter reports a different post-apply revision; readback required before recovery".into()},ApplyResult::Rejected{unchanged_verified:true,reason}=>ApplyPhase::Rejected{reason},ApplyResult::Rejected{reason,..}=>ApplyPhase::RollbackRequired{reason}};
        Ok(())
    }
    pub fn readback_apply(
        &mut self,
        id: OperationId,
        session_nonce: [u8; 16],
        snapshot: Snapshot,
    ) -> Result<ApplyPhase> {
        snapshot.validate()?;
        self.check_snapshot(&snapshot)?;
        let transaction = self
            .operations
            .get(&id)
            .ok_or_else(|| LinkError::Invalid("Unknown operation readback".into()))?;
        if session_nonce != transaction.session_nonce
            || snapshot.identity != transaction.identity
            || self.session()?.nonce != session_nonce
        {
            return Err(LinkError::Stale(
                "Readback belongs to another target/session".into(),
            ));
        }
        if !matches!(
            transaction.phase,
            ApplyPhase::AwaitingReadback
                | ApplyPhase::RollbackRequired { .. }
                | ApplyPhase::Indeterminate { .. }
        ) {
            return Err(LinkError::Stale(
                "Readback arrived before acknowledged apply".into(),
            ));
        }
        let exact = snapshot == *transaction.expected_after;
        let transaction = self.operations.get_mut(&id).unwrap();
        transaction.observed_after = Some(Arc::new(snapshot.clone()));
        if !exact {
            transaction.phase=ApplyPhase::RollbackRequired{reason:"Readback differs, including unedited opaque data; retain preimage and reconcile before rollback".into()};
            return Ok(transaction.phase.clone());
        }
        let context = self
            .sims
            .get_mut(&transaction.identity)
            .ok_or_else(|| LinkError::Stale("Target context missing at readback".into()))?;
        // Draft edits made while the game processed Apply remain untouched.
        context.base = Arc::new(snapshot);
        context.last_operation = Some(id);
        context.remote = None;
        context.conflict = None;
        transaction.phase = ApplyPhase::Committed;
        Ok(transaction.phase.clone())
    }
    pub fn apply_timeout(&mut self, id: OperationId, reason: String) -> Result<()> {
        let transaction = self
            .operations
            .get_mut(&id)
            .ok_or_else(|| LinkError::Invalid("Unknown operation timeout".into()))?;
        if matches!(
            transaction.phase,
            ApplyPhase::Sent | ApplyPhase::AwaitingReadback | ApplyPhase::RollbackSent
        ) {
            transaction.phase = ApplyPhase::Indeterminate { reason };
        }
        Ok(())
    }
    pub fn rollback_request(
        &mut self,
        id: OperationId,
        current: &Snapshot,
    ) -> Result<RollbackRequest> {
        current.validate()?;
        let transaction = self
            .operations
            .get(&id)
            .ok_or_else(|| LinkError::Invalid("Unknown rollback operation".into()))?;
        self.validate_transaction_target(transaction, false)?;
        if !matches!(transaction.phase, ApplyPhase::RollbackRequired { .. })
            || self.session()?.nonce != transaction.session_nonce
            || self.mode != transaction.mode
            || self.selected.as_ref() != Some(&transaction.identity)
            || current.identity != transaction.identity
            || transaction.observed_after.as_deref() != Some(current)
        {
            return Err(LinkError::Stale("Rollback needs exact observed target/revision; never overwrite a later game change".into()));
        }
        let mut restored = current.fields.clone();
        let mut patches = Vec::new();
        for patch in &transaction.patches {
            let before = current.fields.get(&patch.field).cloned();
            let after = patch.before.clone();
            if let Some(after) = &after {
                restored.insert(patch.field.clone(), after.clone());
            } else {
                restored.remove(&patch.field);
            }
            patches.push(FieldPatch {
                field: patch.field.clone(),
                before,
                after,
            });
        }
        let expected = Snapshot::new(current.identity.clone(), restored, current.coverage.clone())?;
        if expected != *transaction.preimage {
            self.operations.get_mut(&id).unwrap().phase=ApplyPhase::RecoveryRequired{reason:"Unrelated or unknown fields changed; selective rollback cannot prove restoration of the complete preimage".into()};
            return Err(LinkError::Stale(
                "Unknown/unrelated changes need manual recovery; nothing was rolled back".into(),
            ));
        }
        let request = RollbackRequest {
            operation_id: id,
            identity: current.identity.clone(),
            session_nonce: transaction.session_nonce,
            expected_current_revision: current.revision,
            patches,
            expected_restored_revision: expected.revision,
        };
        self.operations.get_mut(&id).unwrap().phase = ApplyPhase::RollbackSent;
        Ok(request)
    }
    pub fn readback_rollback(
        &mut self,
        id: OperationId,
        session_nonce: [u8; 16],
        snapshot: Snapshot,
    ) -> Result<ApplyPhase> {
        snapshot.validate()?;
        if self.session()?.nonce != session_nonce {
            return Err(LinkError::Stale(
                "Rollback readback belongs to previous session".into(),
            ));
        }
        let transaction = self
            .operations
            .get_mut(&id)
            .ok_or_else(|| LinkError::Invalid("Unknown rollback readback".into()))?;
        if transaction.phase != ApplyPhase::RollbackSent
            || transaction.identity != snapshot.identity
            || transaction.session_nonce != session_nonce
        {
            return Err(LinkError::Stale(
                "Rollback readback phase/identity differs".into(),
            ));
        }
        transaction.phase = if snapshot == *transaction.preimage {
            ApplyPhase::RolledBack
        } else {
            ApplyPhase::RecoveryRequired {
                reason: "Rollback readback did not restore the exact complete preimage".into(),
            }
        };
        Ok(transaction.phase.clone())
    }
}
