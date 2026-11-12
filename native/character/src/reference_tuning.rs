// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit source-tool predicate, separate from native-game qualification.
use crate::*;
use std::collections::BTreeMap;
#[derive(Clone, Debug, Default)]
pub struct ReferenceTuningSelection {
    pub matching_records: Vec<(usize, usize)>,
    pub scales: Option<BTreeMap<u64, f32>>,
    /// Form1 uses1 only when the selected reference dictionary lacks an ID.
    /// This rule is source evidence; it does not qualify game-native semantics.
    pub reference_default_scale: Option<f32>,
    pub issues: Vec<String>,
    pub native_qualified: bool,
}
fn flags(tokens: &Option<Vec<String>>, field: &str) -> std::result::Result<u32, String> {
    let mut bits = 0;
    for token in tokens.iter().flatten() {
        bits |= match (field, token.as_str()) {
            ("age", "INFANT") => 128,
            ("age", "TODDLER") => 2,
            ("age", "CHILD") => 4,
            ("age", "TEEN") => 8,
            ("age", "YOUNGADULT") => 16,
            ("age", "ADULT") => 32,
            ("age", "ELDER") => 64,
            ("gender", "MALE") => 4096,
            ("gender", "FEMALE") => 8192,
            ("species", "HUMAN") => 1,
            ("species", "DOG") => 2,
            ("species", "CAT") => 3,
            ("species", "SMALLDOG") => 4,
            ("species", "FOX") => 5,
            ("species", "HORSE") => 6,
            ("occult", "HUMAN") => 1,
            ("occult", "ALIEN") => 2,
            ("occult", "VAMPIRE") => 4,
            ("occult", "MERMAID") => 8,
            ("occult", "WITCH") => 16,
            ("occult", "WEREWOLF") => 32,
            ("occult", "FAIRY") => 64,
            _ => {
                return Err(format!(
                    "Unknown {field} token {token}; no native interpretation"
                ))
            }
        };
    }
    Ok(bits)
}
pub fn reference_context_matches(
    context: &TuningContext,
    species: u32,
    occult: u32,
    age: u32,
    gender: u32,
) -> std::result::Result<bool, String> {
    let mut s = flags(&context.species, "species")?;
    let mut o = flags(&context.occult, "occult")?;
    let mut g = flags(&context.genders, "gender")?;
    let a = flags(&context.ages, "age")?;
    // These defaults and exact age/species/occult equality are the pinned
    // ParseTuning/CASModifierScales behavior, not inferred flag intersection.
    if s == 0 {
        s = 1
    }
    if o == 0 {
        o = 1
    }
    if g == 0 {
        g = 12288
    }
    Ok(s == species && o == occult && a == age && (g & gender) > 0)
}
pub fn select_reference_tuning(plan: &CharacterPlan) -> ReferenceTuningSelection {
    let mut out = ReferenceTuningSelection::default();
    let Some(age) = plan.native.age else {
        out.issues.push("Native age absent".into());
        return out;
    };
    let Some(gender) = plan.native.gender else {
        out.issues.push("Native gender absent".into());
        return out;
    };
    let species = plan
        .native
        .extended_species
        .filter(|s| *s != 0)
        .unwrap_or(1);
    let occult = match plan.scene.resolved_form {
        Some(FormSelection::Base)
            if plan.native.current_occult_types.is_none()
                || plan.native.current_occult_types == Some(1) =>
        {
            1
        }
        Some(FormSelection::OccultIndex(i)) => {
            match plan.native.occult_forms.get(i).and_then(|f| f.occult_type) {
                Some(t) => t,
                None => {
                    out.issues
                        .push("Selected native occult form type absent".into());
                    return out;
                }
            }
        }
        _ => {
            out.issues
                .push("Base-form versus stored occult tuning context is unqualified".into());
            return out;
        }
    };
    if plan.tuning.is_empty() {
        out.issues
            .push("No combined tuning sources inspected; no default scale inferred".into());
        return out;
    }
    for (si, source) in plan.tuning.iter().enumerate() {
        let Some(inspection) = &source.inspection else {
            out.issues
                .push(format!("Tuning source {} unresolved", source.source.key));
            continue;
        };
        if !source.issues.is_empty() {
            out.issues.push(format!(
                "Tuning source {} has unresolved records",
                source.source.key
            ));
        }
        for (ri, r) in inspection.records.iter().enumerate() {
            match reference_context_matches(&r.context, species, occult, age, gender) {
                Ok(true) => out.matching_records.push((si, ri)),
                Ok(false) => {}
                Err(e) => out.issues.push(format!("Record {:016X}: {e}", r.instance)),
            }
        }
    }
    let mut first: Option<&TuningRecord> = None;
    let mut different = false;
    for (si, ri) in &out.matching_records {
        let r = &plan.tuning[*si].inspection.as_ref().unwrap().records[*ri];
        if let Some(old) = first {
            if old.scales.len() != r.scales.len()
                || old
                    .scales
                    .iter()
                    .any(|(k, v)| r.scales.get(k).is_none_or(|n| n.to_bits() != v.to_bits()))
            {
                different = true;
            }
        } else {
            first = Some(r)
        }
    }
    if different {
        out.issues.push("Differing context-matching modifier dictionaries require embedded-record precedence; no arbitrary first source selected".into());
    } else if let Some(r) = first {
        out.scales = Some(r.scales.clone());
        out.reference_default_scale = Some(1.0);
    } else {
        out.issues.push(
            "No supported context-matching dictionary; native/default scale remains unresolved"
                .into(),
        );
    }
    out.issues.push("Source-tool predicate/default-scale rules are reference inspection; native context, full scope and embedded-record precedence remain unqualified".into());
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_source_context_equality_defaults_and_unknown_tokens() {
        let mut c = TuningContext {
            ages: Some(vec!["YOUNGADULT".into()]),
            ..Default::default()
        };
        assert!(reference_context_matches(&c, 1, 1, 16, 4096).unwrap());
        assert!(!reference_context_matches(&c, 1, 1, 32, 4096).unwrap());
        c.ages.as_mut().unwrap().push("ADULT".into());
        assert!(!reference_context_matches(&c, 1, 1, 16, 4096).unwrap());
        assert!(reference_context_matches(&c, 1, 1, 48, 4096).unwrap());
        c.ages = Some(vec!["UNUSED_FLAG".into()]);
        assert!(reference_context_matches(&c, 1, 1, 0, 4096).is_err());
    }
}
