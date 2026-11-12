//! Read an existing disposable project and emit private exact appearance data.
use external_cas_tray::*;
use serde_json::json;
use std::path::PathBuf;
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("Expected project directory")?;
    let project = reopen_project(&path)?;
    let draft = ProjectDraft::from_project(&project)?;
    let appearance = draft.appearance()?;
    let total_outfits = appearance
        .iter()
        .map(|s| {
            s.base.outfits.len()
                + s.occult_forms
                    .iter()
                    .map(|f| f.outfits.len())
                    .sum::<usize>()
        })
        .sum::<usize>();
    let total_parts = appearance
        .iter()
        .map(|s| {
            s.base.outfits.iter().map(|o| o.parts.len()).sum::<usize>()
                + s.occult_forms
                    .iter()
                    .flat_map(|f| f.outfits.iter())
                    .map(|o| o.parts.len())
                    .sum::<usize>()
        })
        .sum::<usize>();
    let raw_field_bytes_identical = appearance.iter().all(|s| {
        s.base
            .native
            .fields
            .iter()
            .flat_map(|f| s.base.native.encoded(f).unwrap())
            .copied()
            .collect::<Vec<_>>()
            == s.base.native.bytes
    });
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"schema":1,"sim_count":appearance.len(),"occult_form_count":appearance.iter().map(|s|s.occult_forms.len()).sum::<usize>(),"outfit_count":total_outfits,"outfit_part_count":total_parts,"raw_field_bytes_identical":raw_field_bytes_identical,"original_files_written":false,"game_contacted":false,"source_conflict_count":project.check_source_conflicts().len(),"appearance":appearance})
        )?
    );
    Ok(())
}
