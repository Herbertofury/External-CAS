//! Explicit private PRECOMP input only. No shader bytes are exported.
use external_cas_shader::*;
use std::{path::PathBuf, sync::atomic::AtomicBool};
fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(
        args.next()
            .ok_or("Explicit absolute PRECOMP path required")?,
    );
    let shader_ref: u32 = args
        .next()
        .ok_or("Explicit one-based PS reference required")?
        .parse::<u32>()
        .map_err(|e| e.to_string())?;
    let cancel = AtomicBool::new(false);
    let library = ShaderLibrary::open(&path, Limits::default(), &cancel)?;
    let shader = library.shader(Stage::Pixel, shader_ref, &cancel)?;
    let mut cas = Vec::new();
    for i in 0..library.effect_count() {
        let effect = library.effect(i)?;
        if effect.shader_hash == 0xdf0863b8 {
            let techniques = library.techniques(&effect, &cancel)?;
            if techniques
                .iter()
                .flat_map(|t| &t.passes)
                .any(|p| p.pixel_ref == shader_ref)
            {
                cas.push(serde_json::json!({"effect":effect,"techniques":techniques}));
            }
        }
    }
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({"private":true,"pin":library.pin,"payload_version":library.payload_version,"payload_offset":library.payload_offset,"effects":library.effect_count(),"vertex_count":library.stage_count(Stage::Vertex),"pixel_count":library.stage_count(Stage::Pixel),"compute_count":library.stage_count(Stage::Compute),"selected_pixel_reference":shader.reference,"selected_dxbc_bytes":shader.dxbc.len(),"selected_dxbc_sha256":shader.sha256,"source_record":shader.source_record,"source_record_words":shader.raw_record,"pixel_bindings":library.pixel_bindings(shader.reference,&cancel)?,"cas_effect_candidates":cas,"runtime_permutation_qualified":false})).map_err(|e|e.to_string())?);
    Ok(())
}
