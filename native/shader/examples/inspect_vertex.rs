//! Source metadata/hash report only; never exports installed shader programs.
use external_cas_shader::*;
use std::{path::PathBuf, sync::atomic::AtomicBool};
fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(
        args.next()
            .ok_or("Explicit absolute PRECOMP path required")?,
    );
    let reference: u32 = args
        .next()
        .ok_or("One-based VS reference required")?
        .parse::<u32>()
        .map_err(|e| e.to_string())?;
    let cancel = AtomicBool::new(false);
    let library = ShaderLibrary::open(&path, Limits::default(), &cancel)?;
    let code = library.shader(Stage::Vertex, reference, &cancel)?;
    let bindings = library.vertex_bindings(reference, &cancel)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "private": true, "pin": library.pin,
            "vertex_ref": reference, "dxbc_sha256": code.sha256,
            "dxbc_bytes": code.dxbc.len(), "bindings": bindings,
            "runtime_permutation_qualified": false,
        }))
        .map_err(|e| e.to_string())?
    );
    Ok(())
}
