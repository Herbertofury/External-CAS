//! Exact fixed64 CASP slot replacement in independently saved local drafts.
//! Field identities follow the pinned schema in PROVENANCE.md. No eligibility,
//! game precedence, portrait regeneration or native acceptance is implied.

use super::*;

pub const DRAFT_SCHEMA_EVIDENCE_ID: &str = "simripper-862a32949e5156b371e2d2f83de4e37e0bb1afcc:sim21/outfit1/parts5/id1-fixed64/body7/shift12";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutfitSlot {
    pub sim_index: usize,
    pub sim_id: u64,
    pub outfit_index: usize,
    pub outfit_id: u64,
    pub category: u64,
    pub part_index: usize,
    pub body_type: u32,
    pub part_id: u64,
    pub native_sim_sha256: String,
}
#[derive(Clone, Debug)]
struct LocatedSlot {
    slot: OutfitSlot,
    document_index: usize,
    offset: usize,
}
#[derive(Clone, Debug)]
struct Change {
    document_index: usize,
    offset: usize,
    before: [u8; 8],
    after: [u8; 8],
}

#[derive(Clone, Debug)]
pub struct ProjectDraft {
    source: TrayProject,
    documents: Vec<TrayDocument>,
    revision: u64,
    undo: Vec<Change>,
    redo: Vec<Change>,
    parent_manifest_sha256: String,
}
impl ProjectDraft {
    pub fn from_project(project: &TrayProject) -> Result<Self> {
        let reopened = reopen_project(&project.directory)?;
        if reopened.documents.len() != project.documents.len()
            || reopened.manifest.files.iter().any(|f| {
                !project
                    .documents
                    .iter()
                    .any(|d| d.filename == f.filename && d.sha256 == f.sha256)
            })
        {
            return Err(TrayError::Conflict(
                "Project changed before draft creation".into(),
            ));
        }
        if reopened
            .documents
            .iter()
            .filter(|d| matches!(d.summary, DocumentSummary::Household(_)))
            .count()
            != 1
        {
            return Err(invalid("Select exactly one native household to edit"));
        }
        let parent_manifest_sha256 =
            sha256(&read_stable(&reopened.directory.join(PROJECT_MANIFEST))?);
        let revision = reopened
            .manifest
            .draft
            .as_ref()
            .map(|d| d.revision)
            .unwrap_or(0);
        // Resolve every supported outfit before exposing an editable draft.
        located_slots(&reopened.documents)?;
        Ok(Self {
            documents: reopened.documents.clone(),
            source: reopened,
            revision,
            undo: Vec::new(),
            redo: Vec::new(),
            parent_manifest_sha256,
        })
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn slots(&self) -> Result<Vec<OutfitSlot>> {
        Ok(located_slots(&self.documents)?
            .into_iter()
            .map(|s| s.slot)
            .collect())
    }
    pub fn documents(&self) -> &[TrayDocument] {
        &self.documents
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn is_changed(&self) -> bool {
        self.documents.iter().any(|d| {
            self.source
                .documents
                .iter()
                .any(|s| s.filename == d.filename && s.sha256 != d.sha256)
        })
    }
    pub fn apply_slot(
        &mut self,
        expected_revision: u64,
        slot: &OutfitSlot,
        new_part_id: u64,
        selected_body_type: u32,
    ) -> Result<()> {
        self.require_revision(expected_revision)?;
        if new_part_id == 0 || selected_body_type != slot.body_type {
            return Err(invalid(
                "Selected CASP must have a nonzero ID and the exact existing body type",
            ));
        }
        let found = located_slots(&self.documents)?
            .into_iter()
            .find(|s| &s.slot == slot)
            .ok_or_else(|| {
                TrayError::Conflict(
                    "Outfit slot or source Sim hash changed; refresh before applying".into(),
                )
            })?;
        if new_part_id == slot.part_id {
            return Ok(());
        }
        let change = Change {
            document_index: found.document_index,
            offset: found.offset,
            before: slot.part_id.to_le_bytes(),
            after: new_part_id.to_le_bytes(),
        };
        self.replace(&change, false)?;
        self.undo.push(change);
        self.redo.clear();
        self.advance_revision()
    }
    pub fn undo(&mut self, expected_revision: u64) -> Result<()> {
        self.require_revision(expected_revision)?;
        let change = self
            .undo
            .last()
            .cloned()
            .ok_or_else(|| invalid("Nothing to undo"))?;
        self.replace(&change, true)?;
        self.undo.pop();
        self.redo.push(change);
        self.advance_revision()
    }
    pub fn redo(&mut self, expected_revision: u64) -> Result<()> {
        self.require_revision(expected_revision)?;
        let change = self
            .redo
            .last()
            .cloned()
            .ok_or_else(|| invalid("Nothing to redo"))?;
        self.replace(&change, false)?;
        self.redo.pop();
        self.undo.push(change);
        self.advance_revision()
    }
    fn require_revision(&self, expected: u64) -> Result<()> {
        if expected != self.revision {
            return Err(TrayError::Conflict(
                "Draft revision changed; refresh the selection".into(),
            ));
        }
        if self.revision == u64::MAX {
            return Err(invalid("Draft revision exhausted"));
        }
        Ok(())
    }
    fn advance_revision(&mut self) -> Result<()> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("Draft revision exhausted"))?;
        Ok(())
    }
    fn replace(&mut self, change: &Change, undo: bool) -> Result<()> {
        let doc = self
            .documents
            .get(change.document_index)
            .ok_or_else(|| invalid("Draft document absent"))?;
        let (expected, replacement) = if undo {
            (&change.after, &change.before)
        } else {
            (&change.before, &change.after)
        };
        if doc.bytes().get(change.offset..change.offset + 8) != Some(expected.as_slice()) {
            return Err(TrayError::Conflict(
                "Selected fixed64 bytes no longer match their expected value".into(),
            ));
        }
        let mut bytes = doc.bytes().to_vec();
        bytes[change.offset..change.offset + 8].copy_from_slice(replacement);
        let updated = TrayDocument::from_bytes(&doc.filename, bytes)?;
        // Check all known parallel arrays still align before accepting mutation.
        let mut candidates = self.documents.clone();
        candidates[change.document_index] = updated;
        located_slots(&candidates)?;
        self.documents = candidates;
        Ok(())
    }
    fn verify_sources(&self) -> Result<()> {
        if sha256(&read_stable(&self.source.directory.join(PROJECT_MANIFEST))?)
            != self.parent_manifest_sha256
        {
            return Err(TrayError::Conflict(
                "Parent project manifest changed".into(),
            ));
        }
        let reopened = reopen_project(&self.source.directory)?;
        if reopened.documents.iter().any(|d| {
            !self
                .source
                .documents
                .iter()
                .any(|s| s.filename == d.filename && s.sha256 == d.sha256)
        }) {
            return Err(TrayError::Conflict(
                "Parent project native files changed".into(),
            ));
        }
        if !self.source.check_source_conflicts().is_empty() {
            return Err(TrayError::Conflict(
                "Original Tray source changed; save-as requires conflict review".into(),
            ));
        }
        Ok(())
    }
    pub fn save_as(&self, destination: &Path) -> Result<TrayProject> {
        self.verify_sources()?;
        let destination = external_destination(destination)?;
        if destination.starts_with(&self.source.directory) {
            return Err(invalid("Save the draft in a new sibling project directory"));
        }
        for f in &self.source.manifest.files {
            if destination.starts_with(f.original_path.parent().unwrap_or(&f.original_path)) {
                return Err(invalid(
                    "Draft destination must be outside original source library",
                ));
            }
        }
        let mut manifest = self.source.manifest.clone();
        manifest.schema = 2;
        manifest.draft = Some(DraftProvenance {
            schema_evidence_id: DRAFT_SCHEMA_EVIDENCE_ID.into(),
            revision: self.revision,
            parent_manifest_sha256: self.parent_manifest_sha256.clone(),
            native_game_acceptance_verified: false,
        });
        let mut total = 0;
        for f in &mut manifest.files {
            let doc = self
                .documents
                .iter()
                .find(|d| d.filename == f.filename)
                .ok_or_else(|| invalid("Draft file inventory differs"))?;
            f.original_sha256 = Some(f.original_sha256.as_ref().unwrap_or(&f.sha256).clone());
            f.original_size = Some(f.original_size.unwrap_or(f.size));
            f.sha256 = doc.sha256.clone();
            f.size = doc.bytes().len() as u64;
            total += f.size;
        }
        if total > MAX_PROJECT_BYTES {
            return Err(invalid("Draft exceeds snapshot byte limit"));
        }
        fs::create_dir(&destination)?;
        write_new(
            &destination.join("SNAPSHOT_INCOMPLETE"),
            b"Draft save-as is incomplete. Original and parent files are untouched.\n",
        )?;
        fs::create_dir(destination.join("native"))?;
        for doc in &self.documents {
            write_new(&destination.join("native").join(&doc.filename), doc.bytes())?;
        }
        self.verify_sources()?;
        write_new(
            &destination.join(PROJECT_MANIFEST),
            &serde_json::to_vec_pretty(&manifest).map_err(|e| invalid(e.to_string()))?,
        )?;
        let reopened = load_project(&destination, true)?;
        self.verify_sources()?;
        fs::remove_file(destination.join("SNAPSHOT_INCOMPLETE"))?;
        Ok(reopened)
    }
}

fn repeated_uint(raw: &[u8], maximum: u64) -> Result<Vec<u64>> {
    let mut result = Vec::new();
    for f in fields(raw)?.iter().filter(|f| f.number == 1) {
        match f.wire {
            0 => result.push(f.integer.unwrap()),
            2 => {
                let mut at = 0;
                while at < f.value.len() {
                    result.push(varint(f.value, &mut at)?);
                    if result.len() > MAX_FIELDS {
                        return Err(invalid("Outfit parallel array exceeds limit"));
                    }
                }
            }
            _ => return Err(invalid("Outfit parallel array changed wire type")),
        }
    }
    if result.iter().any(|&n| n > maximum) {
        return Err(invalid("Outfit parallel array value exceeds schema range"));
    }
    Ok(result)
}
fn located_slots(documents: &[TrayDocument]) -> Result<Vec<LocatedSlot>> {
    let mut result = Vec::new();
    for (document_index, doc) in documents.iter().enumerate() {
        let DocumentSummary::Household(_) = &doc.summary else {
            continue;
        };
        for (sim_index, sim_raw) in doc.native_sims()?.into_iter().enumerate() {
            let sim_fields = fields(sim_raw)?;
            let sim_id =
                number(&sim_fields, 1, 1)?.ok_or_else(|| invalid("Native Sim ID missing"))?;
            let Some(outfit_owner) = unique(&sim_fields, 21)? else {
                continue;
            };
            let listing = fields(outfit_owner.bytes()?)?;
            for (outfit_index, outfit_owner) in listing.iter().filter(|f| f.number == 1).enumerate()
            {
                let outfit = fields(outfit_owner.bytes()?)?;
                let outfit_id =
                    number(&outfit, 1, 0)?.ok_or_else(|| invalid("Required outfit ID missing"))?;
                let category = number(&outfit, 2, 0)?.unwrap_or(0);
                let parts = required_blob(&outfit, 5)?;
                let body_types = repeated_uint(required_blob(&outfit, 7)?, u32::MAX as u64)?;
                let mut entries = Vec::new();
                for f in fields(parts)?.iter().filter(|f| f.number == 1) {
                    if !(f.wire == 1 || f.wire == 2) || f.value.len() % 8 != 0 {
                        return Err(invalid("Outfit IdList is not packed/unpacked fixed64"));
                    }
                    let base = (f.value.as_ptr() as usize)
                        .checked_sub(doc.bytes().as_ptr() as usize)
                        .ok_or_else(|| invalid("Outfit source extent differs"))?;
                    for (i, chunk) in f.value.chunks_exact(8).enumerate() {
                        entries.push((u64::from_le_bytes(chunk.try_into().unwrap()), base + i * 8));
                    }
                }
                if entries.len() != body_types.len() {
                    return Err(invalid("Outfit CASP and body-type arrays are not aligned"));
                }
                if let Some(shifts) = unique(&outfit, 12)? {
                    if repeated_uint(shifts.bytes()?, u64::MAX)?.len() != entries.len() {
                        return Err(invalid("Outfit color-shift array is not aligned"));
                    }
                }
                for (part_index, ((part_id, offset), body)) in
                    entries.into_iter().zip(body_types).enumerate()
                {
                    if offset
                        .checked_add(8)
                        .filter(|&end| end <= doc.bytes().len())
                        .is_none()
                    {
                        return Err(invalid("Outfit slot is outside native document"));
                    }
                    result.push(LocatedSlot {
                        document_index,
                        offset,
                        slot: OutfitSlot {
                            sim_index,
                            sim_id,
                            outfit_index,
                            outfit_id,
                            category,
                            part_index,
                            body_type: body as u32,
                            part_id,
                            native_sim_sha256: sha256(sim_raw),
                        },
                    });
                }
            }
        }
    }
    Ok(result)
}
