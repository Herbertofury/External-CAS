//! Validate a structural draft on an existing private disposable project.
//! Replacement is deliberately a test ID, not an eligibility-approved asset.
use external_cas_tray::*;
use serde_json::json;
use std::path::PathBuf;

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if args.len() != 2 {
        return Err("Expected existing private project and new draft project".into());
    }
    let original = reopen_project(&args[0])?;
    let mut draft = ProjectDraft::from_project(&original)?;
    let slot = draft
        .slots()?
        .into_iter()
        .next()
        .ok_or("Real household has no supported outfit slot")?;
    let test_id = slot.part_id ^ u64::MAX;
    if test_id == 0 {
        return Err("Cannot use zero replacement".into());
    }
    let before = original
        .documents
        .iter()
        .find(|d| matches!(d.summary, DocumentSummary::Household(_)))
        .ok_or("Household absent")?;
    draft.apply_slot(draft.revision(), &slot, test_id, slot.body_type)?;
    let after = draft
        .documents()
        .iter()
        .find(|d| d.filename == before.filename)
        .unwrap()
        .bytes()
        .to_vec();
    let changes = before
        .bytes()
        .iter()
        .zip(&after)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    if before.bytes().len() != after.len() || changes.len() != 8 || changes[7] - changes[0] != 7 {
        return Err("Changes extend beyond one exact fixed64 slot".into());
    }
    draft.undo(draft.revision())?;
    if draft
        .documents()
        .iter()
        .find(|d| d.filename == before.filename)
        .unwrap()
        .bytes()
        != before.bytes()
    {
        return Err("Undo was not byte-exact".into());
    }
    draft.redo(draft.revision())?;
    let saved = draft.save_as(&args[1])?;
    let reopened = reopen_project(&saved.directory)?;
    if reopened
        .documents
        .iter()
        .find(|d| d.filename == before.filename)
        .unwrap()
        .bytes()
        != after
    {
        return Err("Reopened draft differs".into());
    }
    let other_files_unchanged = reopened
        .documents
        .iter()
        .filter(|d| d.filename != before.filename)
        .all(|d| {
            original
                .documents
                .iter()
                .any(|o| o.filename == d.filename && o.sha256 == d.sha256)
        });
    if !other_files_unchanged || !reopened.check_source_conflicts().is_empty() {
        return Err("Other files or original sources changed".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"schema":1,"source_project":original.directory,"draft_project":saved.directory,"slot":slot,"replacement_test_id":test_id,"changed_byte_count":changes.len(),"changed_range":changes[0]..changes[7]+1,"all_other_native_bytes_unchanged":true,"metadata_and_portraits_unchanged":other_files_unchanged,"undo_byte_exact":true,"redo_reopen_byte_exact":true,"source_sha256_unchanged":true,"original_files_written":false,"game_contacted":false,"replacement_asset_eligibility_verified":false,"native_game_acceptance_verified":false,"baseline_source_pins_separate":reopened.manifest.files.iter().all(|f|f.original_sha256.is_some())})
        )?
    );
    Ok(())
}
