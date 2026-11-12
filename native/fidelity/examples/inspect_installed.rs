// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit local-only read-only validation. Emits private provenance to stdout;
//! callers should redirect output to an ignored local evidence directory.
use external_cas_assets::PackageIndex;
use external_cas_fidelity::font::CompactedFont;
use external_cas_fidelity::installed::{load_atlas, load_movie};
use serde_json::json;
use sha2::{Digest, Sha256};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let provider = args
        .next()
        .ok_or("usage: inspect_installed UI.package [bare library names...]")?;
    let index = PackageIndex::open(provider)?;
    let mut names = Vec::new();
    let mut evidence = None;
    while let Some(arg) = args.next() {
        if arg == "--private-output-dir" {
            evidence = Some(std::path::PathBuf::from(
                args.next()
                    .ok_or("missing explicit private output directory")?,
            ));
        } else {
            names.push(arg);
        }
    }
    if let Some(path) = &evidence {
        std::fs::create_dir_all(path)?;
    }
    if names.is_empty() {
        names = vec![
            "CASLibrary".into(),
            "ComponentLibrary".into(),
            "gfxfontlib".into(),
        ];
    }
    let mut results = Vec::new();
    for name in names {
        let loaded = load_movie(&index, &name)?;
        if let Some(path) = &evidence {
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path.join(format!("{name}.gfx")))
            {
                use std::io::Write;
                file.write_all(&loaded.source_bytes)?;
            }
        }
        let movie = loaded.inspect()?;
        let atlases: Vec<_> = movie.external_images.iter().map(|image| match load_atlas(&index,image) {
            Ok(atlas) => {
                if let Some(path)=&evidence{for (suffix,bytes) in [("dds",atlas.source_bytes.as_slice()),("rg.rgba",atlas.rgba.as_slice())]{let file=std::fs::OpenOptions::new().write(true).create_new(true).open(path.join(format!("{}.{suffix}",atlas.resolved_name)));if let Ok(mut file)=file{use std::io::Write;file.write_all(bytes).expect("private evidence write");}}}
                json!({"image":image,"status":"decoded","pin":atlas.pin,"resolved_name":atlas.resolved_name,"dimensions":atlas.dimensions,"pixel_encoding":atlas.pixel_encoding,"rgba_sha256":format!("{:x}",Sha256::digest(&atlas.rgba)),"source_bytes":atlas.source_bytes.len(),"decoded_bytes":atlas.rgba.len()})
            },
            Err(error) => json!({"image":image,"status":"unsupported-or-unresolved","reason":error}),
        }).collect();
        let fonts:Vec<_>=movie.compacted_font_headers.iter().map(|header|{
            match CompactedFont::open(&movie,header){
                Ok(font)=>{
                    let converted=match external_cas_fidelity::truetype::convert(&font){
                        Ok(converted)=>{
                            if let Some(path)=&evidence{if let Ok(mut file)=std::fs::OpenOptions::new().write(true).create_new(true).open(path.join(format!("font-{}.ttf",header.id))){use std::io::Write;file.write_all(&converted.bytes).expect("private font evidence write");}}
                            json!({"status":"converted-in-memory","bytes":converted.bytes.len(),"glyph_count":converted.glyph_count,"kerning_pairs":converted.kerning_pairs,"sha256":format!("{:x}",Sha256::digest(&converted.bytes))})
                        },
                        Err(error)=>json!({"status":"unsupported","error":error}),
                    };
                    let codes=if header.first_font_name=="Sims4 Symbols" {font.glyphs.iter().map(|g|g.code).collect()}else{vec![32,48,65,97]};
                    let glyphs:Vec<_>=codes.into_iter().map(|code|match font.glyph(code,100_000){
                        Ok(glyph)=>json!({"code":code,"glyph":glyph}),
                        Err(error)=>json!({"code":code,"error":error.to_string()}),
                    }).collect();
                    json!({"name":header.first_font_name,"status":"native-outlines","glyph_count":font.glyphs.len(),"kerning_pairs":font.kerning.len(),"unparsed":font.unparsed,"sampled_glyphs":glyphs,"truetype":converted})
                },
                Err(error)=>json!({"name":header.first_font_name,"status":"unsupported","error":error.to_string()}),
            }
        }).collect();
        results.push(json!({"name":name,"pin":loaded.pin,"stage_pixels":movie.stage_pixels(),"metadata":movie,"atlases":atlases,"fonts":fonts}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"private":true,"metadata_only":true,"libraries":results})
        )?
    );
    Ok(())
}
