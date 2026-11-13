// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only explicit-source qualification, output remains private.
use external_cas_source_ui::{
    HostValue as V,
    menus::{CasMenuSnapshot, CriterionContext},
};
use std::{collections::BTreeSet, sync::atomic::AtomicBool};
fn json(v: V) -> serde_json::Value {
    match v {
        V::Undefined => serde_json::json!({"unsupported":"Undefined"}),
        V::Null => serde_json::Value::Null,
        V::Bool(v) => v.into(),
        V::Number(v) => serde_json::json!(v),
        V::String(v) => v.into(),
        V::List(v) => serde_json::Value::Array(v.into_iter().map(json).collect()),
        V::Object(v) => {
            serde_json::Value::Object(v.into_iter().map(|(k, v)| (k, json(v))).collect())
        }
        V::Avm2Handle(_) => panic!("persistent VM identity refused"),
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err("usage: inspect_menu_data explicit-provider explicit-context.json".into());
    }
    let source = CasMenuSnapshot::load(std::path::Path::new(&args[0]), &AtomicBool::new(false))?;
    let input: serde_json::Value = serde_json::from_slice(&std::fs::read(&args[1])?)?;
    let u32field = |name: &str| -> Result<Option<u32>, String> {
        input
            .get(name)
            .map(|v| {
                v.as_u64()
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or_else(|| format!("{name} is not u32"))
            })
            .transpose()
    };
    let i32field = |name: &str| -> Result<Option<i32>, String> {
        input
            .get(name)
            .map(|v| {
                v.as_i64()
                    .and_then(|v| i32::try_from(v).ok())
                    .ok_or_else(|| format!("{name} is not i32"))
            })
            .transpose()
    };
    let mut context = CriterionContext {
        age_gender: u32field("age_gender")?,
        species: i32field("species")?,
        occult_flags: u32field("occult_flags")?,
        active_occult_form: i32field("active_occult_form")?,
        ..Default::default()
    };
    if let Some(v) = input.get("booleans") {
        for (k, v) in v.as_object().ok_or("booleans is not Object")? {
            context
                .booleans
                .insert(k.clone(), v.as_bool().ok_or("context Boolean invalid")?);
        }
    }
    if let Some(v) = input.get("available_packs") {
        context.available_packs = Some(
            v.as_array()
                .ok_or("available_packs is not Array")?
                .iter()
                .map(|v| v.as_i64().ok_or("pack id is not i64"))
                .collect::<Result<BTreeSet<_>, _>>()?,
        );
    }
    let project = |r: Result<V, String>| match r {
        Ok(v) => serde_json::json!({"status":"projected","value":json(v)}),
        Err(e) => serde_json::json!({"status":"unavailable","reason":e}),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"private":true,"explicit_qualification_context":input,"pin":source.pin,"copy":project(source.copy_filters(&context)),"randomization":project(source.randomization_filters(&context)),"add_sim":project(source.add_sim_menu(&context)),"original_bytes_retained":source.original_bytes.len(),"unknown_cells_retained":source.document.issues.len(),"complete_ui_qualified":false})
        )?
    );
    Ok(())
}
