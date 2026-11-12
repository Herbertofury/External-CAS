//! Read-only source inspection and new disposable snapshot validation.
//! Usage: cargo run -p external-cas-tray --example validate_local -- SOURCE NEW_PROJECT
use external_cas_tray::*;
use serde_json::json;
use std::path::PathBuf;

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if args.len() != 2 {
        return Err("Expected source Tray directory and new external project directory".into());
    }
    let library = scan_library(&args[0])?;
    let group = library
        .groups
        .iter()
        .find(|g| g.household_path.is_some())
        .ok_or("No safely grouped household")?;
    let source_pins = group
        .paths()
        .iter()
        .map(|p| {
            read_document(p)
                .map(|d| json!({"filename":d.filename,"sha256":d.sha256,"bytes":d.bytes().len()}))
        })
        .collect::<Result<Vec<_>>>()?;
    let project = create_project(group, &args[1])?;
    let reopened = reopen_project(&project.directory)?;
    let source_conflicts = reopened.check_source_conflicts();
    if !source_conflicts.is_empty() {
        return Err("Source changed while validating snapshot".into());
    }
    let household = reopened.household().ok_or("Household did not reopen")?;
    let byte_identical = reopened.documents.iter().all(|d| {
        source_pins
            .iter()
            .any(|p| p["filename"] == d.filename && p["sha256"] == d.sha256)
    });
    if !byte_identical {
        return Err("Native file byte identity changed".into());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "schema":1,"source_library":args[0],"project":project.directory,
            "files_observed":library.files_observed,"group_count":library.groups.len(),"issues":library.issues,
            "source_files":source_pins,"household_version":household.version,"household":household,
            "copy_count":reopened.documents.len(),"native_files_byte_identical":byte_identical,
            "source_sha256_unchanged":source_conflicts.is_empty(),"project_reopened":true,
            "original_files_written":false,"game_contacted":false,"native_game_acceptance_tested":false,
            "native_editing_qualified":false
        }))?
    );
    Ok(())
}
