// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{LinkError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};
pub type Digest32 = [u8; 32];
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SaveIdentity {
    pub slot_id: u64,
    pub world_id: u64,
    pub lineage: Digest32,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SimIdentity {
    pub save: SaveIdentity,
    pub household_id: u64,
    pub sim_id: u64,
    pub form: Option<u32>,
}
impl SimIdentity {
    pub fn validate(&self) -> Result<()> {
        if self.sim_id == 0 || self.household_id == 0 || self.save.lineage == [0; 32] {
            return Err(LinkError::Invalid(
                "Exact save lineage, household and Sim identity required".into(),
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkMode {
    #[default]
    Standalone,
    CasSync,
    LiveLink,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeSession {
    pub nonce: [u8; 16],
    pub game_build: String,
    pub adapter_build: String,
    pub save: SaveIdentity,
    pub zone_id: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected(RuntimeSession),
    Paused {
        session: RuntimeSession,
        reason: String,
    },
    Loading {
        session: RuntimeSession,
    },
}
impl Default for ConnectionState {
    fn default() -> Self {
        Self::Disconnected
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldCapability {
    pub field: String,
    pub read: bool,
    pub cas_write: bool,
    pub live_write: bool,
    pub rollback: bool,
    pub preservation_verified: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub game_build: String,
    pub adapter_build: String,
    pub real_game_qualified: bool,
    pub fields: Vec<FieldCapability>,
    pub unsupported: Vec<String>,
}
/// Local qualification evidence; never accepted from the peer or restored
/// from an editor checkpoint. Production has no approved write profiles yet.
#[derive(Clone, Debug)]
pub struct WriteQualification {
    pub game_build: String,
    pub adapter_build: String,
    pub evidence_sha256: Digest32,
    pub fields: Vec<FieldCapability>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Coverage {
    Partial { missing: Vec<String> },
    CompletePreservationVerified,
}
/// Opaque values include all unsupported data delivered by the adapter. A
/// partial observation is never eligible to replace a complete Sim.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub identity: SimIdentity,
    pub fields: BTreeMap<String, Arc<[u8]>>,
    pub coverage: Coverage,
    pub revision: Digest32,
}
impl Snapshot {
    pub fn new(
        identity: SimIdentity,
        fields: BTreeMap<String, Arc<[u8]>>,
        coverage: Coverage,
    ) -> Result<Self> {
        identity.validate()?;
        let mut value = Self {
            identity,
            fields,
            coverage,
            revision: [0; 32],
        };
        value.revision = value.calculate_revision()?;
        Ok(value)
    }
    pub fn calculate_revision(&self) -> Result<Digest32> {
        let raw = serde_json::to_vec(&(&self.identity, &self.fields, &self.coverage))?;
        Ok(Sha256::digest(raw).into())
    }
    pub fn validate(&self) -> Result<()> {
        self.identity.validate()?;
        if self.revision != self.calculate_revision()? {
            return Err(LinkError::Invalid(
                "Snapshot revision does not bind every opaque field".into(),
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContextId {
    Standalone,
    Sim(SimIdentity),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edit {
    pub field: String,
    pub before: Option<Arc<[u8]>>,
    pub after: Option<Arc<[u8]>>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftState {
    pub fields: BTreeMap<String, Arc<[u8]>>,
    pub version: u64,
    pub undo: Vec<Edit>,
    pub redo: Vec<Edit>,
    pub saved_version: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimContext {
    pub base: Arc<Snapshot>,
    pub draft: DraftState,
    pub remote: Option<Arc<Snapshot>>,
    pub conflict: Option<String>,
    pub departed: bool,
    pub last_operation: Option<[u8; 16]>,
}
impl SimContext {
    pub fn dirty(&self) -> bool {
        self.draft.fields != self.base.fields
    }
    pub fn confirmed_revision(&self) -> Digest32 {
        self.base.revision
    }
}
#[derive(Clone, Debug)]
pub struct LinkLimits {
    pub max_contexts: usize,
    pub max_household_sims: usize,
    pub max_retained_bytes: u64,
    pub max_snapshot_bytes: u64,
    pub max_history: usize,
}
impl Default for LinkLimits {
    fn default() -> Self {
        Self {
            max_contexts: 4096,
            max_household_sims: 4096,
            max_retained_bytes: 512 * 1024 * 1024,
            max_snapshot_bytes: 32 * 1024 * 1024,
            max_history: 4096,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HouseholdMember {
    pub identity: SimIdentity,
    pub name: String,
    pub portrait_resource: Option<String>,
    pub available: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionTicket {
    pub session_nonce: [u8; 16],
    pub navigation_epoch: u64,
    pub request_id: u64,
    pub identity: SimIdentity,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SwitchTarget {
    Mode(LinkMode),
    Sim(SimIdentity),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SwitchDecision {
    Ask,
    Stash,
    SaveAcknowledged {
        draft_version: u64,
        checkpoint_sha256: Digest32,
    },
    Resolve,
    Cancel,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SwitchOutcome {
    Switched,
    Cancelled,
    NeedsDecision {
        context: ContextId,
        draft_version: u64,
    },
    NeedsResolution {
        context: ContextId,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectionNotice {
    ObservedOnly,
    TargetUnchanged,
    SwitchProposed(SimIdentity),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolution {
    KeepExternal,
    TakeGame,
}

/// Serialize through the app's existing append-only session journal. This is
/// editor context, not a game save; reconnect still needs fresh observations.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistedContexts {
    pub schema: u32,
    pub mode: LinkMode,
    pub selected: Option<SimIdentity>,
    pub standalone: DraftState,
    pub sims: Vec<(SimIdentity, SimContext)>,
    pub operations: Vec<crate::ApplyTransaction>,
}
pub struct LinkController {
    pub mode: LinkMode,
    pub connection: ConnectionState,
    pub capabilities: Capabilities,
    pub standalone: DraftState,
    pub sims: HashMap<SimIdentity, SimContext>,
    pub selected: Option<SimIdentity>,
    pub observed_selection: Option<SimIdentity>,
    pub household: Vec<HouseholdMember>,
    pub limits: LinkLimits,
    navigation_epoch: u64,
    next_request: u64,
    last_selection_sequence: u64,
    pending: Option<SelectionTicket>,
    pub operations: HashMap<crate::OperationId, crate::ApplyTransaction>,
    write_qualifications: Vec<WriteQualification>,
}
impl LinkController {
    pub fn new(limits: LinkLimits) -> Self {
        Self {
            mode: LinkMode::Standalone,
            connection: ConnectionState::Disconnected,
            capabilities: Capabilities::default(),
            standalone: DraftState::default(),
            sims: HashMap::new(),
            selected: None,
            observed_selection: None,
            household: Vec::new(),
            limits,
            navigation_epoch: 0,
            next_request: 0,
            last_selection_sequence: 0,
            pending: None,
            operations: HashMap::new(),
            write_qualifications: Vec::new(),
        }
    }
    pub fn active_context(&self) -> ContextId {
        if self.mode == LinkMode::Standalone {
            ContextId::Standalone
        } else {
            self.selected
                .clone()
                .map(ContextId::Sim)
                .unwrap_or(ContextId::Standalone)
        }
    }
    pub fn register_local_write_qualification(
        &mut self,
        profile: WriteQualification,
    ) -> Result<()> {
        if profile.game_build.is_empty()
            || profile.adapter_build.is_empty()
            || profile.evidence_sha256 == [0; 32]
            || profile.fields.is_empty()
        {
            return Err(LinkError::Invalid(
                "Local versioned qualification evidence required".into(),
            ));
        }
        let mut seen = HashSet::new();
        for field in &profile.fields {
            if field.field.is_empty()
                || field.field.len() > 256
                || !seen.insert(field.field.clone())
                || !field.read
                || !field.rollback
                || !field.preservation_verified
                || !(field.cas_write || field.live_write)
            {
                return Err(LinkError::Invalid(
                    "Qualification needs unique, preservation-tested write/rollback fields".into(),
                ));
            }
        }
        self.write_qualifications.push(profile);
        Ok(())
    }
    pub(crate) fn locally_qualified(&self, field: &str, mode: LinkMode) -> bool {
        self.write_qualifications.iter().any(|profile| {
            profile.game_build == self.capabilities.game_build
                && profile.adapter_build == self.capabilities.adapter_build
                && profile.fields.iter().any(|capability| {
                    capability.field == field
                        && match mode {
                            LinkMode::CasSync => capability.cas_write,
                            LinkMode::LiveLink => capability.live_write,
                            LinkMode::Standalone => false,
                        }
                })
        })
    }
    pub fn draft(&self, id: &ContextId) -> Result<&DraftState> {
        match id {
            ContextId::Standalone => Ok(&self.standalone),
            ContextId::Sim(id) => self
                .sims
                .get(id)
                .map(|c| &c.draft)
                .ok_or_else(|| LinkError::Invalid("Sim draft has not been loaded".into())),
        }
    }
    fn draft_mut(&mut self, id: &ContextId) -> Result<&mut DraftState> {
        match id {
            ContextId::Standalone => Ok(&mut self.standalone),
            ContextId::Sim(id) => self
                .sims
                .get_mut(id)
                .map(|c| &mut c.draft)
                .ok_or_else(|| LinkError::Invalid("Sim draft has not been loaded".into())),
        }
    }
    pub fn dirty(&self, id: &ContextId) -> bool {
        match id {
            ContextId::Standalone => {
                self.standalone.saved_version != Some(self.standalone.version)
                    && (self.standalone.version != 0 || !self.standalone.fields.is_empty())
            }
            ContextId::Sim(id) => self.sims.get(id).is_some_and(SimContext::dirty),
        }
    }
    pub fn request_connect(&mut self) -> Result<()> {
        if !matches!(
            self.connection,
            ConnectionState::Disconnected
                | ConnectionState::Paused { .. }
                | ConnectionState::Loading { .. }
        ) {
            return Err(LinkError::Invalid(
                "Disconnect or resolve current connection first".into(),
            ));
        }
        self.connection = ConnectionState::Connecting;
        self.capabilities = Capabilities::default();
        self.invalidate_navigation()?;
        Ok(())
    }
    pub fn connected(&mut self, session: RuntimeSession, capabilities: Capabilities) -> Result<()> {
        if !matches!(self.connection, ConnectionState::Connecting) {
            return Err(LinkError::Invalid(
                "Connection was not explicitly requested".into(),
            ));
        }
        if session.nonce == [0; 16]
            || session.game_build.is_empty()
            || session.game_build != capabilities.game_build
            || session.adapter_build != capabilities.adapter_build
        {
            return Err(LinkError::Invalid(
                "Adapter version/session capability mismatch".into(),
            ));
        }
        self.connection = ConnectionState::Connected(session);
        self.capabilities = capabilities;
        self.last_selection_sequence = 0;
        self.invalidate_navigation()?;
        Ok(())
    }
    pub fn disconnect(&mut self) -> Result<()> {
        self.connection = ConnectionState::Disconnected;
        self.capabilities = Capabilities::default();
        self.observed_selection = None;
        self.household.clear();
        self.invalidate_navigation()?;
        self.interrupt_operations(
            "Connection lost; query operation status/readback before retrying",
        );
        for context in self.sims.values_mut() {
            context.conflict.get_or_insert_with(|| {
                "Disconnected; revalidate exact identity/revision before applying".into()
            });
        }
        Ok(())
    }
    pub fn pause(&mut self, reason: String) -> Result<()> {
        let session = self.session()?.clone();
        self.connection = ConnectionState::Paused { session, reason };
        self.invalidate_navigation()
    }
    pub fn loading(&mut self) -> Result<()> {
        let session = self.session()?.clone();
        self.connection = ConnectionState::Loading { session };
        self.invalidate_navigation()?;
        self.interrupt_operations(
            "Travel/loading during operation; revalidate status and identity",
        );
        for context in self.sims.values_mut() {
            context
                .conflict
                .get_or_insert_with(|| "Travel/loading requires a fresh target snapshot".into());
        }
        Ok(())
    }
    fn interrupt_operations(&mut self, reason: &str) {
        for operation in self.operations.values_mut() {
            if matches!(
                operation.phase,
                crate::ApplyPhase::Sent
                    | crate::ApplyPhase::AwaitingReadback
                    | crate::ApplyPhase::RollbackSent
            ) {
                operation.phase = crate::ApplyPhase::Indeterminate {
                    reason: reason.into(),
                };
            }
        }
    }
    pub fn session(&self) -> Result<&RuntimeSession> {
        match &self.connection {
            ConnectionState::Connected(s) => Ok(s),
            _ => Err(LinkError::Stale(
                "No active, unpaused game connection".into(),
            )),
        }
    }
    fn invalidate_navigation(&mut self) -> Result<()> {
        self.navigation_epoch = self
            .navigation_epoch
            .checked_add(1)
            .ok_or_else(|| LinkError::Invalid("Navigation epoch exhausted".into()))?;
        self.pending = None;
        Ok(())
    }
    pub fn request_switch(
        &mut self,
        target: SwitchTarget,
        decision: SwitchDecision,
    ) -> Result<SwitchOutcome> {
        if let SwitchTarget::Sim(id) = &target {
            id.validate()?;
            if self.mode == LinkMode::Standalone {
                return Err(LinkError::Invalid(
                    "Standalone editing cannot be hijacked by a linked Sim selection".into(),
                ));
            }
            if &self.session()?.save != &id.save {
                return Err(LinkError::Stale(
                    "Selected Sim belongs to another save/world".into(),
                ));
            }
        }
        if decision == SwitchDecision::Cancel {
            return Ok(SwitchOutcome::Cancelled);
        }
        let active = if self.mode == LinkMode::Standalone {
            Some(ContextId::Standalone)
        } else {
            self.selected.clone().map(ContextId::Sim)
        };
        if let Some(active) = active.filter(|id| self.dirty(id)) {
            match decision {
                SwitchDecision::Ask => {
                    return Ok(SwitchOutcome::NeedsDecision {
                        context: active.clone(),
                        draft_version: self.draft(&active)?.version,
                    });
                }
                SwitchDecision::Resolve => {
                    return Ok(SwitchOutcome::NeedsResolution { context: active });
                }
                SwitchDecision::SaveAcknowledged {
                    draft_version,
                    checkpoint_sha256,
                } => {
                    if self.draft(&active)?.version != draft_version || checkpoint_sha256 == [0; 32]
                    {
                        return Err(LinkError::Stale(
                            "Save acknowledgement does not match current draft".into(),
                        ));
                    }
                    self.draft_mut(&active)?.saved_version = Some(draft_version);
                }
                SwitchDecision::Stash => {}
                SwitchDecision::Cancel => unreachable!(),
            }
        }
        match target {
            SwitchTarget::Mode(mode) => self.mode = mode,
            SwitchTarget::Sim(id) => self.selected = Some(id),
        }
        self.invalidate_navigation()?;
        Ok(SwitchOutcome::Switched)
    }
    pub fn observe_selection(
        &mut self,
        session_nonce: [u8; 16],
        sequence: u64,
        identity: Option<SimIdentity>,
    ) -> Result<SelectionNotice> {
        let session = self.session()?;
        if session.nonce != session_nonce || sequence <= self.last_selection_sequence {
            return Err(LinkError::Stale(
                "Selection event belongs to an earlier session/sequence".into(),
            ));
        }
        if let Some(id) = &identity {
            id.validate()?;
            if id.save != session.save {
                return Err(LinkError::Stale(
                    "Selection event save/world differs".into(),
                ));
            }
        }
        self.last_selection_sequence = sequence;
        self.observed_selection = identity.clone();
        if self.mode == LinkMode::Standalone {
            return Ok(SelectionNotice::ObservedOnly);
        }
        if identity == self.selected {
            return Ok(SelectionNotice::TargetUnchanged);
        }
        Ok(identity
            .map(SelectionNotice::SwitchProposed)
            .unwrap_or(SelectionNotice::ObservedOnly))
    }
    pub fn replace_household(
        &mut self,
        session_nonce: [u8; 16],
        members: Vec<HouseholdMember>,
    ) -> Result<()> {
        let session = self.session()?;
        if session.nonce != session_nonce {
            return Err(LinkError::Stale(
                "Household belongs to previous session".into(),
            ));
        }
        if members.len() > self.limits.max_household_sims {
            return Err(LinkError::Budget(
                "Household snapshot; previous state retained".into(),
            ));
        }
        let mut ids = HashSet::new();
        for member in &members {
            member.identity.validate()?;
            if member.identity.save != session.save
                || member.name.len() > 1024
                || !ids.insert(member.identity.clone())
            {
                return Err(LinkError::Invalid(
                    "Household identity/duplicate/name bound".into(),
                ));
            }
        }
        self.household = members;
        Ok(())
    }
    pub fn target_unavailable(
        &mut self,
        session_nonce: [u8; 16],
        identity: &SimIdentity,
        reason: String,
    ) -> Result<()> {
        if self.session()?.nonce != session_nonce {
            return Err(LinkError::Stale(
                "Departure belongs to previous session".into(),
            ));
        }
        if let Some(context) = self.sims.get_mut(identity) {
            context.departed = true;
            context.conflict = Some(reason);
        }
        if self
            .pending
            .as_ref()
            .is_some_and(|t| &t.identity == identity)
        {
            self.invalidate_navigation()?;
        }
        Ok(())
    }
    pub fn issue_snapshot_request(&mut self, identity: SimIdentity) -> Result<SelectionTicket> {
        if self.mode == LinkMode::Standalone || self.selected.as_ref() != Some(&identity) {
            return Err(LinkError::Stale(
                "Snapshot must target the explicitly selected linked Sim".into(),
            ));
        }
        let session_nonce = self.session()?.nonce;
        self.next_request = self
            .next_request
            .checked_add(1)
            .ok_or_else(|| LinkError::Invalid("Request IDs exhausted".into()))?;
        let ticket = SelectionTicket {
            session_nonce,
            navigation_epoch: self.navigation_epoch,
            request_id: self.next_request,
            identity,
        };
        self.pending = Some(ticket.clone());
        Ok(ticket)
    }
    pub fn accept_snapshot(
        &mut self,
        ticket: &SelectionTicket,
        snapshot: Snapshot,
        origin_operation: Option<[u8; 16]>,
    ) -> Result<()> {
        if self.pending.as_ref() != Some(ticket)
            || self.session()?.nonce != ticket.session_nonce
            || self.navigation_epoch != ticket.navigation_epoch
            || self.selected.as_ref() != Some(&ticket.identity)
            || snapshot.identity != ticket.identity
        {
            return Err(LinkError::Stale(
                "Obsolete asynchronous Sim snapshot".into(),
            ));
        }
        snapshot.validate()?;
        self.check_snapshot(&snapshot)?;
        let identity = snapshot.identity.clone();
        let snapshot = Arc::new(snapshot);
        let candidate = if let Some(previous) = self.sims.get(&identity) {
            let mut next = previous.clone();
            next.departed = false;
            if origin_operation.is_some()
                && origin_operation == next.last_operation
                && snapshot.revision == next.base.revision
            {
                next.remote = None;
            } else if snapshot.revision == next.base.revision {
                next.remote = None;
                next.conflict = None;
            } else if next.dirty() || !next.draft.undo.is_empty() {
                next.remote = Some(snapshot);
                next.conflict = Some(
                    "Game and retained external context differ; preview/reconcile before writing"
                        .into(),
                );
            } else {
                next.base = snapshot.clone();
                next.draft.fields = snapshot.fields.clone();
                next.remote = None;
                next.conflict = None;
            }
            next
        } else {
            if self.sims.len() >= self.limits.max_contexts {
                return Err(LinkError::Budget(
                    "Sim context count; no draft was evicted".into(),
                ));
            }
            SimContext {
                base: snapshot.clone(),
                draft: DraftState {
                    fields: snapshot.fields.clone(),
                    ..Default::default()
                },
                remote: None,
                conflict: None,
                departed: false,
                last_operation: None,
            }
        };
        let previous = self.sims.insert(identity.clone(), candidate);
        if self.retained_bytes() > self.limits.max_retained_bytes {
            if let Some(previous) = previous {
                self.sims.insert(identity, previous);
            } else {
                self.sims.remove(&identity);
            }
            return Err(LinkError::Budget(
                "Retained contexts; no draft was evicted".into(),
            ));
        }
        self.pending = None;
        Ok(())
    }
    pub(crate) fn check_snapshot(&self, snapshot: &Snapshot) -> Result<()> {
        if snapshot.fields.len() > 4096
            || snapshot
                .fields
                .keys()
                .any(|k| k.is_empty() || k.len() > 256)
            || snapshot
                .fields
                .values()
                .map(|v| v.len() as u64)
                .sum::<u64>()
                > self.limits.max_snapshot_bytes
        {
            return Err(LinkError::Budget("Snapshot fields/bytes".into()));
        }
        Ok(())
    }
    pub fn edit(
        &mut self,
        id: &ContextId,
        expected_version: u64,
        field: String,
        after: Option<Vec<u8>>,
    ) -> Result<u64> {
        if field.is_empty() || field.len() > 256 {
            return Err(LinkError::Invalid(
                "Bounded opaque field name required".into(),
            ));
        }
        let current = self.draft(id)?;
        if current.version != expected_version {
            return Err(LinkError::Stale("Draft changed before edit".into()));
        }
        if current.undo.len() >= self.limits.max_history {
            return Err(LinkError::Budget(
                "Undo history; save/stash the context before adding more edits".into(),
            ));
        }
        let before = current.fields.get(&field).cloned();
        let after = after.map(Arc::from);
        if before == after {
            return Ok(current.version);
        }
        let mut next = current.clone();
        next.redo.clear();
        set_field(&mut next.fields, &field, after.clone());
        next.undo.push(Edit {
            field,
            before,
            after,
        });
        next.version = next
            .version
            .checked_add(1)
            .ok_or_else(|| LinkError::Invalid("Draft version exhausted".into()))?;
        self.replace_draft_checked(id, next)
    }
    fn replace_draft_checked(&mut self, id: &ContextId, next: DraftState) -> Result<u64> {
        self.check_draft(&next)?;
        let previous = std::mem::replace(self.draft_mut(id)?, next);
        if self.retained_bytes() > self.limits.max_retained_bytes {
            *self.draft_mut(id)? = previous;
            return Err(LinkError::Budget(
                "Draft history bytes; prior draft retained".into(),
            ));
        }
        Ok(self.draft(id)?.version)
    }
    fn check_draft(&self, draft: &DraftState) -> Result<()> {
        if draft.fields.len() > 4096
            || draft.fields.keys().any(|k| k.is_empty() || k.len() > 256)
            || draft.fields.values().map(|v| v.len() as u64).sum::<u64>()
                > self.limits.max_snapshot_bytes
            || draft.undo.len() > self.limits.max_history
            || draft.redo.len() > self.limits.max_history
        {
            return Err(LinkError::Budget("Draft fields/history extent".into()));
        }
        for redo in [false, true] {
            let mut fields = draft.fields.clone();
            for edit in (if redo { &draft.redo } else { &draft.undo }).iter().rev() {
                let (expected, replacement) = if redo {
                    (&edit.before, &edit.after)
                } else {
                    (&edit.after, &edit.before)
                };
                if fields.get(&edit.field) != expected.as_ref() {
                    return Err(LinkError::Invalid(
                        "Persisted history preimage differs".into(),
                    ));
                }
                set_field(&mut fields, &edit.field, replacement.clone());
            }
        }
        Ok(())
    }
    pub fn undo(&mut self, id: &ContextId, expected_version: u64) -> Result<u64> {
        self.history(id, expected_version, false)
    }
    /// Three-way merge: retain nonconflicting edits from each side, require
    /// an explicit choice for every concurrently changed field. Unknown blobs
    /// participate byte-for-byte; they are never defaulted or dropped.
    pub fn reconcile_sim(
        &mut self,
        identity: &SimIdentity,
        expected_draft_version: u64,
        remote_revision: Digest32,
        choices: BTreeMap<String, Resolution>,
    ) -> Result<u64> {
        let previous = self
            .sims
            .get(identity)
            .ok_or_else(|| LinkError::Invalid("Missing Sim context".into()))?;
        if previous.draft.version != expected_draft_version {
            return Err(LinkError::Stale(
                "Draft changed during conflict review".into(),
            ));
        }
        let remote = previous
            .remote
            .as_ref()
            .filter(|r| r.revision == remote_revision)
            .ok_or_else(|| LinkError::Stale("Reviewed remote revision changed".into()))?
            .clone();
        let mut next = previous.clone();
        let mut merged = next.draft.fields.clone();
        let fields: std::collections::BTreeSet<_> = previous
            .base
            .fields
            .keys()
            .chain(previous.draft.fields.keys())
            .chain(remote.fields.keys())
            .cloned()
            .collect();
        let mut conflicts = HashSet::new();
        for field in fields {
            let base = previous.base.fields.get(&field);
            let external = previous.draft.fields.get(&field);
            let game = remote.fields.get(&field);
            let chosen = if external == game || game == base {
                external.cloned()
            } else if external == base {
                game.cloned()
            } else {
                conflicts.insert(field.clone());
                match choices.get(&field) {
                    Some(Resolution::KeepExternal) => external.cloned(),
                    Some(Resolution::TakeGame) => game.cloned(),
                    None => {
                        return Err(LinkError::Stale(format!(
                            "Resolve concurrent field {field} explicitly"
                        )));
                    }
                }
            };
            set_field(&mut merged, &field, chosen);
        }
        if choices.keys().any(|k| !conflicts.contains(k)) {
            return Err(LinkError::Invalid(
                "Resolution includes a field outside the current conflict set".into(),
            ));
        }
        next.draft.redo.clear();
        for field in previous
            .draft
            .fields
            .keys()
            .chain(merged.keys())
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
        {
            let before = previous.draft.fields.get(&field).cloned();
            let after = merged.get(&field).cloned();
            if before != after {
                next.draft.undo.push(Edit {
                    field,
                    before,
                    after,
                });
            }
        }
        next.draft.fields = merged;
        next.draft.version = next
            .draft
            .version
            .checked_add(1)
            .ok_or_else(|| LinkError::Invalid("Draft version exhausted".into()))?;
        self.check_draft(&next.draft)?;
        next.base = remote;
        next.remote = None;
        next.conflict = None;
        let previous = self.sims.insert(identity.clone(), next).unwrap();
        if self.retained_bytes() > self.limits.max_retained_bytes {
            self.sims.insert(identity.clone(), previous);
            return Err(LinkError::Budget(
                "Conflict resolution history; previous context retained".into(),
            ));
        }
        Ok(self.sims[identity].draft.version)
    }
    pub fn redo(&mut self, id: &ContextId, expected_version: u64) -> Result<u64> {
        self.history(id, expected_version, true)
    }
    fn history(&mut self, id: &ContextId, expected_version: u64, redo: bool) -> Result<u64> {
        let mut next = self.draft(id)?.clone();
        if next.version != expected_version {
            return Err(LinkError::Stale("Draft version differs".into()));
        }
        let Some(edit) = (if redo { &mut next.redo } else { &mut next.undo }).pop() else {
            return Ok(expected_version);
        };
        let (expected, replacement) = if redo {
            (&edit.before, &edit.after)
        } else {
            (&edit.after, &edit.before)
        };
        if next.fields.get(&edit.field) != expected.as_ref() {
            return Err(LinkError::Stale("Undo/redo preimage differs".into()));
        }
        set_field(&mut next.fields, &edit.field, replacement.clone());
        (if redo { &mut next.undo } else { &mut next.redo }).push(edit);
        next.version = next
            .version
            .checked_add(1)
            .ok_or_else(|| LinkError::Invalid("Draft version exhausted".into()))?;
        self.replace_draft_checked(id, next)
    }
    pub fn retained_bytes(&self) -> u64 {
        let mut seen = HashSet::new();
        let mut total = 0u64;
        let mut add = |value: &Arc<[u8]>| {
            if seen.insert((Arc::as_ptr(value) as *const u8 as usize, value.len())) {
                total = total.saturating_add(value.len() as u64);
            }
        };
        let mut draft = |d: &DraftState| {
            for value in d.fields.values() {
                add(value);
            }
            for e in d.undo.iter().chain(&d.redo) {
                for value in e.before.iter().chain(&e.after) {
                    add(value);
                }
            }
        };
        draft(&self.standalone);
        for c in self.sims.values() {
            draft(&c.draft);
        }
        drop(draft);
        for c in self.sims.values() {
            for value in c.base.fields.values() {
                add(value);
            }
            if let Some(remote) = &c.remote {
                for value in remote.fields.values() {
                    add(value);
                }
            }
        }
        for o in self.operations.values() {
            for value in o
                .preimage
                .fields
                .values()
                .chain(o.expected_after.fields.values())
            {
                add(value);
            }
            if let Some(after) = &o.observed_after {
                for value in after.fields.values() {
                    add(value);
                }
            }
            for patch in &o.patches {
                for value in patch.before.iter().chain(&patch.after) {
                    add(value);
                }
            }
        }
        total
    }
    pub fn export_contexts(&self) -> PersistedContexts {
        PersistedContexts {
            schema: 1,
            mode: self.mode,
            selected: self.selected.clone(),
            standalone: self.standalone.clone(),
            sims: self
                .sims
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            operations: self.operations.values().cloned().collect(),
        }
    }
    pub fn restore_contexts(saved: PersistedContexts, limits: LinkLimits) -> Result<Self> {
        if saved.schema != 1
            || saved.sims.len() > limits.max_contexts
            || saved.operations.len() > limits.max_history
        {
            return Err(LinkError::Unsupported(
                "Context schema or configured context/operation count".into(),
            ));
        }
        let mut result = Self::new(limits);
        result.mode = saved.mode;
        result.selected = saved.selected;
        result.standalone = saved.standalone;
        result.check_draft(&result.standalone)?;
        if let Some(selected) = &result.selected {
            selected.validate()?;
        }
        for (key, mut context) in saved.sims {
            context.base.validate()?;
            result.check_snapshot(&context.base)?;
            result.check_draft(&context.draft)?;
            if context.base.identity != key || result.sims.contains_key(&key) {
                return Err(LinkError::Invalid(
                    "Persisted Sim context identity/duplicate".into(),
                ));
            }
            if let Some(remote) = &context.remote {
                remote.validate()?;
                result.check_snapshot(remote)?;
                if remote.identity != key {
                    return Err(LinkError::Invalid(
                        "Persisted remote snapshot identity".into(),
                    ));
                }
            }
            context.conflict =
                Some("Reopened external context; reconnect and reconcile before applying".into());
            result.sims.insert(key, context);
        }
        for mut operation in saved.operations {
            operation.validate()?;
            result.check_snapshot(&operation.preimage)?;
            result.check_snapshot(&operation.expected_after)?;
            if let Some(observed) = &operation.observed_after {
                result.check_snapshot(observed)?;
            }
            if matches!(
                operation.phase,
                crate::ApplyPhase::Sent
                    | crate::ApplyPhase::AwaitingReadback
                    | crate::ApplyPhase::RollbackSent
            ) {
                operation.phase=crate::ApplyPhase::Indeterminate{reason:"App reopened with an unresolved operation; query status/readback, never reapply automatically".into()};
            }
            if result
                .operations
                .insert(operation.operation_id, operation)
                .is_some()
            {
                return Err(LinkError::Invalid("Duplicate persisted operation".into()));
            }
        }
        if result.retained_bytes() > result.limits.max_retained_bytes {
            return Err(LinkError::Budget(
                "Restored context bytes; original journal must be retained".into(),
            ));
        }
        Ok(result)
    }
}
fn set_field(fields: &mut BTreeMap<String, Arc<[u8]>>, key: &str, value: Option<Arc<[u8]>>) {
    if let Some(value) = value {
        fields.insert(key.into(), value);
    } else {
        fields.remove(key);
    }
}
