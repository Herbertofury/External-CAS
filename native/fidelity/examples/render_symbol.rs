// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit source-symbol validation artifact. Keep generated source art private.
use external_cas_assets::PackageIndex;
use external_cas_fidelity::{
    installed::{load_atlas, load_movie},
    symbol_raster::{rasterize, AtlasBinding, AtlasPresentation},
    timeline::SymbolLibrary,
};
use serde_json::json;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().skip(1).collect();
    if a.len() != 5 {
        return Err(
            "usage: render_symbol UI.package movie symbol state private-output-directory".into(),
        );
    }
    let index = PackageIndex::open(&a[0])?;
    let loaded = load_movie(&index, &a[1])?;
    let movie = loaded.inspect()?;
    let library = SymbolLibrary::new(&movie)?;
    let frame = library.frame(&a[2], Some(&a[3]), 0)?;
    let atlases: Vec<_> = movie
        .external_images
        .iter()
        .map(|image| Ok((image.id, load_atlas(&index, image)?)))
        .collect::<Result<_, String>>()?;
    let bindings: Vec<_> = atlases
        .iter()
        .map(|(image_id, atlas)| AtlasBinding {
            image_id: *image_id,
            atlas,
        })
        .collect();
    let raster = rasterize(
        &frame,
        &movie,
        &bindings,
        1.,
        None,
        AtlasPresentation::RedLuminanceGreenAlpha,
    )?;
    let out = std::path::Path::new(&a[4]);
    std::fs::create_dir_all(out)?;
    image::save_buffer_with_format(
        out.join("native.png"),
        &raster.rgba,
        raster.dimensions[0],
        raster.dimensions[1],
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )?;
    std::fs::write(
        out.join("native-plan.json"),
        serde_json::to_vec_pretty(
            &json!({"private":true,"pin":loaded.pin,"plan":frame,"bounds":raster.bounds_twips,"dimensions":raster.dimensions,"labels":library.labels(&a[2])?}),
        )?,
    )?;
    println!(
        "{}",
        serde_json::to_string(
            &json!({"dimensions":raster.dimensions,"bounds":raster.bounds_twips,"layers":frame.layers.len()})
        )?
    );
    Ok(())
}
