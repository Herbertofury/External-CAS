//! Surgical native uint64 color-shift writes. Unknown fields and unaffected
//! encodings stay byte exact; only changed values and necessary lengths change.
use super::*;
pub const COLOR_SHIFT_SCHEMA_EVIDENCE_ID:&str="simripper-862a32949e5156b371e2d2f83de4e37e0bb1afcc:sim21/outfit1/shift12/color1-uint64-varint;occult30/17/3/21";
const NEUTRAL: u64 = 0x4000000000000000;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MissingColorShiftPolicy {
    RequireExisting,
    MaterializeNeutralOthers,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorShiftStorage {
    Absent,
    EmptyList,
    UnpackedVarint,
    PackedVarint,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutfitColorShift {
    pub slot: OutfitSlot,
    pub value: Option<u64>,
    pub storage: ColorShiftStorage,
    pub color_list_sha256: Option<String>,
}
fn encode(mut v: u64) -> Vec<u8> {
    let mut b = vec![];
    loop {
        let mut q = (v & 127) as u8;
        v >>= 7;
        if v != 0 {
            q |= 128;
        }
        b.push(q);
        if v == 0 {
            return b;
        }
    }
}
fn append_blob(out: &mut Vec<u8>, number: u32, payload: &[u8]) {
    out.extend(encode(((number as u64) << 3) | 2));
    out.extend(encode(payload.len() as u64));
    out.extend(payload);
}
fn field_bytes(field: &Field<'_>, payload: &[u8]) -> Result<Vec<u8>> {
    if field.wire != 2 {
        return Err(invalid("Expected native length-delimited field"));
    }
    let mut out = vec![];
    if payload.len() == field.value.len() {
        let prefix = field.value.as_ptr() as usize - field.raw.as_ptr() as usize;
        out.extend(&field.raw[..prefix]);
    } else {
        let mut at = 0;
        varint(field.raw, &mut at)?;
        out.extend(&field.raw[..at]);
        out.extend(encode(payload.len() as u64));
    }
    out.extend(payload);
    Ok(out)
}
fn replace_blob(
    raw: &[u8],
    number: u32,
    index: usize,
    change: impl FnOnce(&[u8]) -> Result<Vec<u8>>,
) -> Result<Vec<u8>> {
    let all = fields(raw)?;
    let field = all
        .iter()
        .filter(|f| f.number == number)
        .nth(index)
        .ok_or_else(|| invalid("Native edit path is absent"))?;
    let payload = field.bytes()?;
    let new = change(payload)?;
    if new == payload {
        return Ok(raw.to_vec());
    }
    let replacement = field_bytes(field, &new)?;
    let start = field.raw.as_ptr() as usize - raw.as_ptr() as usize;
    let end = start + field.raw.len();
    let mut out = Vec::with_capacity(raw.len() - field.raw.len() + replacement.len());
    out.extend(&raw[..start]);
    out.extend(replacement);
    out.extend(&raw[end..]);
    Ok(out)
}
fn values(raw: &[u8]) -> Result<Vec<(u64, ColorShiftStorage)>> {
    let mut out = vec![];
    for f in fields(raw)?.iter().filter(|f| f.number == 1) {
        match f.wire {
            0 => out.push((f.integer.unwrap(), ColorShiftStorage::UnpackedVarint)),
            2 => {
                let mut at = 0;
                while at < f.value.len() {
                    out.push((varint(f.value, &mut at)?, ColorShiftStorage::PackedVarint));
                    if out.len() > MAX_FIELDS {
                        return Err(invalid("Native color-shift count exceeds limit"));
                    }
                }
            }
            _ => return Err(invalid("ColorShiftList.color_shift changed wire type")),
        }
    }
    Ok(out)
}
fn outfit_raw<'a>(doc: &'a TrayDocument, slot: &OutfitSlot) -> Result<&'a [u8]> {
    let sims = doc.native_sims()?;
    let sim = *sims
        .get(slot.sim_index)
        .ok_or_else(|| invalid("Native Sim absent"))?;
    let list = outfit_lists(sim)?
        .into_iter()
        .find(|(f, _)| *f == slot.occult_form_index)
        .ok_or_else(|| invalid("Native form outfit list absent"))?
        .1;
    let all = fields(list)?;
    all.iter()
        .filter(|f| f.number == 1)
        .nth(slot.outfit_index)
        .ok_or_else(|| invalid("Native outfit absent"))?
        .bytes()
}
fn read_shift(
    doc: &TrayDocument,
    slot: &OutfitSlot,
) -> Result<(Option<u64>, ColorShiftStorage, Option<String>)> {
    let raw = outfit_raw(doc, slot)?;
    let f = fields(raw)?;
    let Some(shifts) = unique(&f, 12)? else {
        return Ok((None, ColorShiftStorage::Absent, None));
    };
    let raw = shifts.bytes()?;
    let entries = values(raw)?;
    if entries.is_empty() {
        return Ok((None, ColorShiftStorage::EmptyList, Some(sha256(raw))));
    }
    let (v, storage) = entries
        .get(slot.part_index)
        .copied()
        .ok_or_else(|| invalid("Native color-shift list not aligned"))?;
    Ok((Some(v), storage, Some(sha256(raw))))
}
fn edit_list(raw: &[u8], part_index: usize, new_value: u64) -> Result<Vec<u8>> {
    let mut current = 0usize;
    let mut found = false;
    let mut out = vec![];
    for f in fields(raw)? {
        if f.number != 1 {
            out.extend(f.raw);
            continue;
        }
        match f.wire {
            0 => {
                if current == part_index {
                    let prefix = f.value.as_ptr() as usize - f.raw.as_ptr() as usize;
                    out.extend(&f.raw[..prefix]);
                    out.extend(encode(new_value));
                    found = true;
                } else {
                    out.extend(f.raw);
                }
                current += 1;
            }
            2 => {
                let mut at = 0;
                let mut payload = f.value.to_vec();
                while at < f.value.len() {
                    let start = at;
                    varint(f.value, &mut at)?;
                    if current == part_index {
                        payload.splice(start..at, encode(new_value));
                        found = true;
                    }
                    current += 1;
                }
                if payload.as_slice() == f.value {
                    out.extend(f.raw);
                } else {
                    out.extend(field_bytes(&f, &payload)?);
                }
            }
            _ => return Err(invalid("Native color-shift wire type changed")),
        }
    }
    if !found {
        return Err(invalid("Native color-shift slot absent"));
    }
    Ok(out)
}
fn edit_outfit(
    raw: &[u8],
    slot: &OutfitSlot,
    count: usize,
    new_value: u64,
    policy: MissingColorShiftPolicy,
) -> Result<Vec<u8>> {
    let f = fields(raw)?;
    if let Some(shifts) = unique(&f, 12)? {
        let len = values(shifts.bytes()?)?.len();
        if len == 0 {
            if policy != MissingColorShiftPolicy::MaterializeNeutralOthers {
                return Err(invalid(
                    "Native color-shift list is empty; explicit materialization is required",
                ));
            }
            return replace_blob(raw, 12, 0, |payload| {
                let mut out = payload.to_vec();
                for i in 0..count {
                    out.push(8);
                    out.extend(encode(if i == slot.part_index {
                        new_value
                    } else {
                        NEUTRAL
                    }));
                }
                Ok(out)
            });
        }
        if len != count {
            return Err(invalid("Native color-shift array not aligned"));
        }
        replace_blob(raw, 12, 0, |payload| {
            edit_list(payload, slot.part_index, new_value)
        })
    } else {
        if policy != MissingColorShiftPolicy::MaterializeNeutralOthers {
            return Err(invalid(
                "Native color-shift array absent; explicit materialization is required",
            ));
        }
        let mut values = vec![];
        for i in 0..count {
            values.push(8);
            values.extend(encode(if i == slot.part_index {
                new_value
            } else {
                NEUTRAL
            }));
        }
        let mut out = raw.to_vec();
        append_blob(&mut out, 12, &values);
        Ok(out)
    }
}
fn edit_sim(
    raw: &[u8],
    slot: &OutfitSlot,
    count: usize,
    value: u64,
    policy: MissingColorShiftPolicy,
) -> Result<Vec<u8>> {
    let outfit = |raw: &[u8]| {
        replace_blob(raw, 21, 0, |list| {
            replace_blob(list, 1, slot.outfit_index, |o| {
                edit_outfit(o, slot, count, value, policy)
            })
        })
    };
    if let Some(form) = slot.occult_form_index {
        replace_blob(raw, 30, 0, |attrs| {
            replace_blob(attrs, 17, 0, |tracker| {
                replace_blob(tracker, 3, form, outfit)
            })
        })
    } else {
        outfit(raw)
    }
}
fn edit_document(
    doc: &TrayDocument,
    slot: &OutfitSlot,
    count: usize,
    value: u64,
    policy: MissingColorShiftPolicy,
) -> Result<Vec<u8>> {
    let DocumentSummary::Household(h) = &doc.summary else {
        return Err(invalid("Color-shift document is not a native household"));
    };
    let payload = &doc.bytes()[h.payload_range.clone()];
    let edited = replace_blob(payload, 1, 0, |account| {
        replace_blob(account, 6, slot.sim_index, |sim| {
            edit_sim(sim, slot, count, value, policy)
        })
    })?;
    if edited.len() != payload.len()
        && h.version == 2
        && h.embedded_images.is_none()
        && !h.suffix_range.is_empty()
    {
        return Err(invalid(
            "Cannot move an unsupported opaque household suffix during a variable-length edit",
        ));
    }
    let mut out = doc.bytes()[..h.payload_range.start].to_vec();
    out.extend(&edited);
    out.extend(&doc.bytes()[h.suffix_range.clone()]);
    let size =
        u32::try_from(out.len() - 4).map_err(|_| invalid("Native household size overflow"))?;
    out[4..8].copy_from_slice(&size.to_le_bytes());
    if h.version == 2 {
        let len =
            u32::try_from(edited.len()).map_err(|_| invalid("Native payload size overflow"))?;
        out[12..16].copy_from_slice(&len.to_le_bytes());
    }
    Ok(out)
}
impl ProjectDraft {
    pub fn color_shifts(&self) -> Result<Vec<OutfitColorShift>> {
        let mut out = vec![];
        for located in located_slots(&self.documents)? {
            let (value, storage, color_list_sha256) =
                read_shift(&self.documents[located.document_index], &located.slot)?;
            out.push(OutfitColorShift {
                slot: located.slot,
                value,
                storage,
                color_list_sha256,
            });
        }
        Ok(out)
    }
    /// Raw uint64 only. Shader/slider conversion is a separate explicitly proven
    /// boundary. This operation never normalizes, clamps or alters other lanes.
    pub fn apply_color_shift(
        &mut self,
        expected_revision: u64,
        slot: &OutfitSlot,
        expected_shift: Option<u64>,
        new_shift: u64,
        missing: MissingColorShiftPolicy,
    ) -> Result<()> {
        self.require_revision(expected_revision)?;
        let slots = located_slots(&self.documents)?;
        let located = slots.iter().find(|s| &s.slot == slot).ok_or_else(|| {
            TrayError::Conflict(
                "Outfit identity/native Sim hash changed; refresh before color edit".into(),
            )
        })?;
        let doc = &self.documents[located.document_index];
        let (current, _, _) = read_shift(doc, slot)?;
        if current != expected_shift {
            return Err(TrayError::Conflict(
                "Native color-shift value changed".into(),
            ));
        }
        if current == Some(new_shift) {
            return Ok(());
        }
        let count = slots
            .iter()
            .filter(|s| {
                s.document_index == located.document_index
                    && s.slot.sim_index == slot.sim_index
                    && s.slot.occult_form_index == slot.occult_form_index
                    && s.slot.outfit_index == slot.outfit_index
            })
            .count();
        let updated = edit_document(doc, slot, count, new_shift, missing)?;
        let before = doc.bytes();
        let mut prefix = 0;
        while prefix < before.len().min(updated.len()) && before[prefix] == updated[prefix] {
            prefix += 1;
        }
        let mut suffix = 0;
        while suffix < before.len() - prefix
            && suffix < updated.len() - prefix
            && before[before.len() - 1 - suffix] == updated[updated.len() - 1 - suffix]
        {
            suffix += 1;
        }
        let change = Change {
            document_index: located.document_index,
            offset: prefix,
            before: before[prefix..before.len() - suffix].to_vec(),
            after: updated[prefix..updated.len() - suffix].to_vec(),
            schema_evidence_id: COLOR_SHIFT_SCHEMA_EVIDENCE_ID,
        };
        let history_bytes = self
            .undo
            .iter()
            .map(|c| c.before.len() + c.after.len())
            .sum::<usize>()
            .checked_add(change.before.len() + change.after.len())
            .ok_or_else(|| invalid("Draft history byte overflow"))?;
        if history_bytes > 128 * 1024 * 1024 {
            return Err(invalid(
                "Draft undo budget reached; save-as a new project revision before further edits",
            ));
        }
        self.replace(&change, false)?;
        self.undo.push(change);
        self.redo.clear();
        self.advance_revision()
    }
}
