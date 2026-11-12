//! Schema-proven read-only native appearance, retaining complete raw messages.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NativeField {
    pub number: u32,
    pub wire: u8,
    pub encoded_range: Range<usize>,
    pub payload_range: Range<usize>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NativeMessage {
    pub bytes: Vec<u8>,
    pub sha256: String,
    pub fields: Vec<NativeField>,
}
impl NativeMessage {
    fn new(raw: &[u8]) -> Result<Self> {
        let base = raw.as_ptr() as usize;
        let fields = fields(raw)?
            .into_iter()
            .map(|f| {
                let encoded_start = f.raw.as_ptr() as usize - base;
                let payload_start = f.value.as_ptr() as usize - base;
                NativeField {
                    number: f.number,
                    wire: f.wire,
                    encoded_range: encoded_start..encoded_start + f.raw.len(),
                    payload_range: payload_start..payload_start + f.value.len(),
                }
            })
            .collect();
        Ok(Self {
            bytes: raw.to_vec(),
            sha256: sha256(raw),
            fields,
        })
    }
    pub fn encoded(&self, field: &NativeField) -> Option<&[u8]> {
        self.bytes.get(field.encoded_range.clone())
    }
    pub fn payload(&self, field: &NativeField) -> Option<&[u8]> {
        self.bytes.get(field.payload_range.clone())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppearanceModifier {
    pub key: Option<u64>,
    pub amount: Option<f32>,
    pub native: NativeMessage,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FacialCustomization {
    pub sculpts: Vec<u64>,
    pub face_modifiers: Vec<AppearanceModifier>,
    pub body_modifiers: Vec<AppearanceModifier>,
    pub aged_face_modifiers: Vec<AppearanceModifier>,
    pub aged_body_modifiers: Vec<AppearanceModifier>,
    pub native: NativeMessage,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeneticPart {
    pub id: u64,
    pub body_type: u32,
    pub color_shift: Option<u64>,
    pub object_id: Option<u64>,
    pub layer_id: Option<u32>,
    pub native: NativeMessage,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeneticAppearance {
    pub physique: Option<String>,
    pub facial: Option<FacialCustomization>,
    pub parts: Vec<GeneticPart>,
    pub growth_parts: Vec<GeneticPart>,
    pub native: NativeMessage,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutfitPartAppearance {
    pub part_index: usize,
    pub cas_part_id: u64,
    pub body_type: u32,
    pub color_shift: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OutfitAppearance {
    pub outfit_index: usize,
    pub outfit_id: u64,
    pub category: Option<u32>,
    pub category_index: usize,
    pub parts: Vec<OutfitPartAppearance>,
    pub native: NativeMessage,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppearanceForm {
    /// None is the base SimData message; no implicit species or form label.
    pub occult_form_index: Option<usize>,
    pub occult_type: Option<u32>,
    pub skin_tone: Option<u64>,
    pub skin_tone_value_shift: Option<f32>,
    pub physique: Option<String>,
    pub facial: Option<FacialCustomization>,
    pub genetics: Option<GeneticAppearance>,
    pub outfits: Vec<OutfitAppearance>,
    /// Computed only when both current category and category-index were stored.
    pub current_outfit_match: Option<usize>,
    pub native: NativeMessage,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SimAppearance {
    pub sim_index: usize,
    pub sim_id: u64,
    pub gender: Option<u32>,
    pub age: Option<u32>,
    pub extended_species: Option<u32>,
    pub current_outfit_category: Option<u32>,
    pub current_outfit_index: Option<u32>,
    pub custom_texture: Option<u64>,
    pub pelt_layers: Option<NativeMessage>,
    pub occult_types: Option<u32>,
    pub current_occult_types: Option<u32>,
    pub occult_form_available: Option<bool>,
    pub base: AppearanceForm,
    pub occult_forms: Vec<AppearanceForm>,
}
impl ProjectDraft {
    pub fn appearance(&self) -> Result<Vec<SimAppearance>> {
        let mut result = Vec::new();
        for doc in self.documents() {
            if !matches!(doc.summary, DocumentSummary::Household(_)) {
                continue;
            }
            for (sim_index, raw) in doc.native_sims()?.into_iter().enumerate() {
                result.push(parse_sim_appearance(sim_index, raw)?);
            }
        }
        Ok(result)
    }
}
fn u32_field(f: &[Field<'_>], n: u32) -> Result<Option<u32>> {
    number(f, n, 0)?
        .map(|v| u32::try_from(v).map_err(|_| invalid("Native uint32 appearance value overflows")))
        .transpose()
}
fn optional_text(f: &[Field<'_>], n: u32) -> Result<Option<String>> {
    unique(f, n)?
        .map(|v| {
            std::str::from_utf8(v.bytes()?)
                .map(str::to_string)
                .map_err(|_| invalid("Native appearance string is not UTF-8"))
        })
        .transpose()
}
fn float_field(f: &[Field<'_>], n: u32) -> Result<Option<f32>> {
    let Some(field) = unique(f, n)? else {
        return Ok(None);
    };
    if field.wire != 5 || field.value.len() != 4 {
        return Err(invalid("Native appearance float has changed wire type"));
    }
    let value = f32::from_le_bytes(field.value.try_into().unwrap());
    if !value.is_finite() {
        return Err(invalid(
            "Non-finite appearance float retained in native bytes but cannot be projected",
        ));
    }
    Ok(Some(value))
}
fn uints(f: &[Field<'_>], n: u32, maximum: u64, fixed64: bool) -> Result<Vec<u64>> {
    let mut result = Vec::new();
    for field in f.iter().filter(|f| f.number == n) {
        if fixed64 {
            if !(field.wire == 1 || field.wire == 2) || field.value.len() % 8 != 0 {
                return Err(invalid("Native repeated fixed64 changed wire type"));
            }
            for b in field.value.chunks_exact(8) {
                result.push(u64::from_le_bytes(b.try_into().unwrap()));
            }
        } else {
            match field.wire {
                0 => result.push(field.integer.unwrap()),
                2 => {
                    let mut at = 0;
                    while at < field.value.len() {
                        result.push(varint(field.value, &mut at)?);
                        if result.len() > MAX_FIELDS {
                            return Err(invalid("Native appearance array exceeds bound"));
                        }
                    }
                }
                _ => {
                    return Err(invalid(
                        "Native repeated unsigned integer changed wire type",
                    ))
                }
            }
        }
    }
    if result.len() > MAX_FIELDS || result.iter().any(|&v| v > maximum) {
        return Err(invalid("Native appearance array value/count exceeds bound"));
    }
    Ok(result)
}
fn modifiers(f: &[Field<'_>], n: u32) -> Result<Vec<AppearanceModifier>> {
    f.iter()
        .filter(|f| f.number == n)
        .map(|field| {
            let raw = field.bytes()?;
            let f = fields(raw)?;
            Ok(AppearanceModifier {
                key: number(&f, 1, 0)?,
                amount: float_field(&f, 2)?,
                native: NativeMessage::new(raw)?,
            })
        })
        .collect()
}
fn facial(raw: &[u8]) -> Result<FacialCustomization> {
    let f = fields(raw)?;
    Ok(FacialCustomization {
        sculpts: uints(&f, 1, u64::MAX, false)?,
        face_modifiers: modifiers(&f, 2)?,
        body_modifiers: modifiers(&f, 3)?,
        aged_face_modifiers: modifiers(&f, 4)?,
        aged_body_modifiers: modifiers(&f, 5)?,
        native: NativeMessage::new(raw)?,
    })
}
fn genetic_parts(f: &[Field<'_>], n: u32) -> Result<Vec<GeneticPart>> {
    let Some(owner) = unique(f, n)? else {
        return Ok(Vec::new());
    };
    fields(owner.bytes()?)?
        .iter()
        .filter(|f| f.number == 1)
        .map(|part| {
            let raw = part.bytes()?;
            let f = fields(raw)?;
            Ok(GeneticPart {
                id: number(&f, 1, 0)?
                    .ok_or_else(|| invalid("Required genetic PartData ID absent"))?,
                body_type: u32_field(&f, 2)?
                    .ok_or_else(|| invalid("Required genetic PartData body type absent"))?,
                color_shift: number(&f, 3, 0)?,
                object_id: number(&f, 4, 0)?,
                layer_id: u32_field(&f, 5)?,
                native: NativeMessage::new(raw)?,
            })
        })
        .collect()
}
fn genetics(raw: &[u8]) -> Result<GeneticAppearance> {
    let f = fields(raw)?;
    Ok(GeneticAppearance {
        physique: optional_text(&f, 2)?,
        facial: unique(&f, 1)?.map(|v| facial(v.bytes()?)).transpose()?,
        parts: genetic_parts(&f, 5)?,
        growth_parts: genetic_parts(&f, 6)?,
        native: NativeMessage::new(raw)?,
    })
}
fn outfits(f: &[Field<'_>]) -> Result<Vec<OutfitAppearance>> {
    let Some(owner) = unique(f, 21)? else {
        return Ok(Vec::new());
    };
    let list = fields(owner.bytes()?)?;
    let mut result = Vec::new();
    let mut categories = BTreeMap::new();
    for (outfit_index, owner) in list.iter().filter(|f| f.number == 1).enumerate() {
        let raw = owner.bytes()?;
        let f = fields(raw)?;
        let category = u32_field(&f, 2)?;
        let category_index = *categories.entry(category.unwrap_or(0)).or_insert(0usize);
        *categories.get_mut(&category.unwrap_or(0)).unwrap() += 1;
        let parts = uints(&fields(required_blob(&f, 5)?)?, 1, u64::MAX, true)?;
        let bodies = uints(&fields(required_blob(&f, 7)?)?, 1, u32::MAX as u64, false)?;
        let shifts = unique(&f, 12)?
            .map(|f| uints(&fields(f.bytes()?)?, 1, u64::MAX, false))
            .transpose()?;
        if parts.len() != bodies.len()
            || shifts
                .as_ref()
                .map(|s| s.len() != parts.len())
                .unwrap_or(false)
        {
            return Err(invalid(
                "Native outfit part/body/color arrays differ in length",
            ));
        }
        let parts = parts
            .into_iter()
            .zip(bodies)
            .enumerate()
            .map(|(part_index, (cas_part_id, body))| OutfitPartAppearance {
                part_index,
                cas_part_id,
                body_type: body as u32,
                color_shift: shifts.as_ref().map(|s| s[part_index]),
            })
            .collect();
        result.push(OutfitAppearance {
            outfit_index,
            outfit_id: number(&f, 1, 0)?.ok_or_else(|| invalid("Required outfit ID absent"))?,
            category,
            category_index,
            parts,
            native: NativeMessage::new(raw)?,
        });
    }
    Ok(result)
}
fn form(
    raw: &[u8],
    occult_form_index: Option<usize>,
    current_category: Option<u32>,
    current_index: Option<u32>,
) -> Result<AppearanceForm> {
    let f = fields(raw)?;
    let occult = occult_form_index.is_some();
    let outfits = outfits(&f)?;
    let current_outfit_match = match (current_category, current_index) {
        (Some(c), Some(i)) => outfits
            .iter()
            .position(|o| o.category.unwrap_or(0) == c && o.category_index == i as usize),
        _ => None,
    };
    Ok(AppearanceForm {
        occult_form_index,
        occult_type: if occult { u32_field(&f, 1)? } else { None },
        skin_tone: number(&f, if occult { 16 } else { 10 }, 0)?,
        skin_tone_value_shift: float_field(&f, if occult { 22 } else { 67 })?,
        physique: optional_text(&f, if occult { 11 } else { 12 })?,
        facial: unique(&f, if occult { 12 } else { 18 })?
            .map(|v| facial(v.bytes()?))
            .transpose()?,
        genetics: unique(&f, if occult { 17 } else { 28 })?
            .map(|v| genetics(v.bytes()?))
            .transpose()?,
        outfits,
        current_outfit_match,
        native: NativeMessage::new(raw)?,
    })
}
fn parse_sim_appearance(sim_index: usize, raw: &[u8]) -> Result<SimAppearance> {
    let f = fields(raw)?;
    let current_outfit_category = u32_field(&f, 38)?;
    let current_outfit_index = u32_field(&f, 39)?;
    let mut occult_types = None;
    let mut current_occult_types = None;
    let mut occult_form_available = None;
    let mut occult_forms = Vec::new();
    if let Some(attributes) = unique(&f, 30)? {
        let a = fields(attributes.bytes()?)?;
        if let Some(tracker) = unique(&a, 17)? {
            let t = fields(tracker.bytes()?)?;
            occult_types = u32_field(&t, 1)?;
            current_occult_types = u32_field(&t, 2)?;
            occult_form_available = number(&t, 5, 0)?
                .map(|v| match v {
                    0 => Ok(false),
                    1 => Ok(true),
                    _ => Err(invalid("Native occult availability is not boolean")),
                })
                .transpose()?;
            for (index, owner) in t.iter().filter(|f| f.number == 3).enumerate() {
                occult_forms.push(form(
                    owner.bytes()?,
                    Some(index),
                    current_outfit_category,
                    current_outfit_index,
                )?);
            }
        }
    }
    Ok(SimAppearance {
        sim_index,
        sim_id: number(&f, 1, 1)?.ok_or_else(|| invalid("Required Sim ID absent"))?,
        gender: u32_field(&f, 7)?,
        age: u32_field(&f, 8)?,
        extended_species: u32_field(&f, 60)?,
        current_outfit_category,
        current_outfit_index,
        custom_texture: number(&f, 62, 0)?,
        pelt_layers: unique(&f, 63)?
            .map(|v| NativeMessage::new(v.bytes()?))
            .transpose()?,
        occult_types,
        current_occult_types,
        occult_form_available,
        base: form(raw, None, current_outfit_category, current_outfit_index)?,
        occult_forms,
    })
}
