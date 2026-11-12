//! existing-project new-project new-private-report schema-oracle.dll
use external_cas_tray::*;
use serde_json::json;
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};
fn household_bytes(d: &ProjectDraft) -> &[u8] {
    d.documents()
        .iter()
        .find(|d| matches!(d.summary, DocumentSummary::Household(_)))
        .unwrap()
        .bytes()
}
fn oracle(d: &ProjectDraft, path: &PathBuf) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut child = Command::new("dotnet")
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child.stdin.take().unwrap().write_all(household_bytes(d))?;
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(format!(
            "Schema oracle error {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let reference: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let records = reference["records"]
        .as_array()
        .ok_or("Schema records absent")?;
    for entry in d.color_shifts()? {
        let slot = &entry.slot;
        let r = records
            .iter()
            .find(|r| {
                r["sim_index"].as_u64() == Some(slot.sim_index as u64)
                    && r["occult_form_index"].as_u64() == slot.occult_form_index.map(|v| v as u64)
                    && r["outfit_index"].as_u64() == Some(slot.outfit_index as u64)
            })
            .ok_or("Schema outfit absent")?;
        assert_eq!(r["sim_id"].as_u64(), Some(slot.sim_id));
        assert_eq!(r["outfit_id"].as_u64(), Some(slot.outfit_id));
        assert_eq!(r["parts"][slot.part_index].as_u64(), Some(slot.part_id));
        assert_eq!(
            r["body_types"][slot.part_index].as_u64(),
            Some(slot.body_type as u64)
        );
        assert_eq!(r["color_shifts"][slot.part_index].as_u64(), entry.value);
    }
    Ok(())
}
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if args.len() != 4 {
        return Err("existing-project new-project report schema-oracle.dll".into());
    }
    let project = reopen_project(&args[0])?;
    let mut draft = ProjectDraft::from_project(&project)?;
    let original = household_bytes(&draft).to_vec();
    let appearance = draft.appearance()?;
    let current = appearance
        .first()
        .and_then(|s| s.base.current_outfit_match)
        .ok_or("Native current outfit absent")?;
    let choose = |d: &ProjectDraft| -> external_cas_tray::Result<OutfitColorShift> {
        d.color_shifts()?
            .into_iter()
            .find(|e| {
                e.slot.sim_index == 0
                    && e.slot.occult_form_index.is_none()
                    && e.slot.outfit_index == current
                    && e.slot.body_type == 6
            })
            .ok_or_else(|| TrayError::Invalid("Native current top slot absent".into()))
    };
    let entry = choose(&draft)?;
    let old = entry.value.ok_or("Real color array absent")?;
    oracle(&draft, &args[3])?;
    // Exercise real native varint shrink/growth without retaining that test edit.
    draft.apply_color_shift(
        draft.revision(),
        &entry.slot,
        Some(old),
        0,
        MissingColorShiftPolicy::RequireExisting,
    )?;
    let shrink_bytes = household_bytes(&draft).len();
    oracle(&draft, &args[3])?;
    draft.undo(draft.revision())?;
    assert_eq!(household_bytes(&draft), original);
    let entry = choose(&draft)?;
    let new = (old & !(0xffffu64 << 32)) | (0x1000u64 << 32);
    draft.apply_color_shift(
        draft.revision(),
        &entry.slot,
        Some(old),
        new,
        MissingColorShiftPolicy::RequireExisting,
    )?;
    let edited = household_bytes(&draft).to_vec();
    oracle(&draft, &args[3])?;
    draft.undo(draft.revision())?;
    assert_eq!(household_bytes(&draft), original);
    draft.redo(draft.revision())?;
    assert_eq!(household_bytes(&draft), edited);
    let saved = draft.save_as(&args[1])?;
    let reopened = reopen_project(&saved.directory)?;
    let mut readback = ProjectDraft::from_project(&reopened)?;
    assert_eq!(household_bytes(&readback), edited);
    assert_eq!(choose(&readback)?.value, Some(new));
    oracle(&readback, &args[3])?;
    for doc in &reopened.documents {
        if !matches!(doc.summary, DocumentSummary::Household(_)) {
            assert_eq!(
                doc.bytes(),
                project
                    .documents
                    .iter()
                    .find(|d| d.filename == doc.filename)
                    .unwrap()
                    .bytes()
            );
        }
    }
    let selected = choose(&readback)?;
    readback.apply_color_shift(
        readback.revision(),
        &selected.slot,
        Some(new),
        old,
        MissingColorShiftPolicy::RequireExisting,
    )?;
    assert_eq!(household_bytes(&readback), original);
    assert!(project.check_source_conflicts().is_empty());
    assert!(reopened.check_source_conflicts().is_empty());
    let changed = original
        .iter()
        .zip(&edited)
        .enumerate()
        .filter_map(|(i, (a, b))| (a != b).then_some(i))
        .collect::<Vec<_>>();
    let report = json!({"schema":1,"files":project.documents.len(),"native_color_entries":draft.color_shifts()?.len(),"base_and_occult_schema_reader":"unchanged public SimRipper schemas with protobuf-net3.2.30","independent_schema_readbacks":4,"original_household_bytes":original.len(),"zero_shift_household_bytes":shrink_bytes,"edited_household_bytes":edited.len(),"changed_byte_count_for_hue_lane":changed.len(),"changed_byte_offsets_for_hue_lane":changed,"nonselected_48_bits_preserved":(old&!(0xffffu64<<32))==(new&!(0xffffu64<<32)),"undo_redo_byte_exact":true,"save_reopen_byte_exact":true,"reopened_revert_restores_original_bytes":true,"nonhousehold_bytes_unchanged":true,"original_source_conflicts":project.check_source_conflicts().len(),"originals_written":false,"game_contacted":false,"native_tray_import_qualified":false});
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[2])?;
    file.write_all(&serde_json::to_vec_pretty(&report)?)?;
    file.sync_all()?;
    println!("{}", report);
    Ok(())
}
