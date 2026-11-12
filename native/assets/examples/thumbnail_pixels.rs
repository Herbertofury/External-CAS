// SPDX-License-Identifier: GPL-3.0-or-later
//! Test-oracle pipe only. No source paths or pixels are persisted.
use external_cas_assets::{ReadLimits, thumbnail_decode::decode_thumbnail};
use std::io::{Read, Write};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = std::io::stdin().lock();
    let mut out = std::io::stdout().lock();
    let mut word = [0; 4];
    input.read_exact(&mut word)?;
    let count = u32::from_le_bytes(word);
    if count > 4096 {
        return Err("oracle batch bound".into());
    }
    out.write_all(&word)?;
    let cancel = std::sync::atomic::AtomicBool::new(false);
    for _ in 0..count {
        input.read_exact(&mut word)?;
        let n = u32::from_le_bytes(word) as usize;
        if n > 64 * 1024 * 1024 {
            return Err("oracle input bound".into());
        }
        let mut bytes = vec![0; n];
        input.read_exact(&mut bytes)?;
        let decoded = decode_thumbnail(
            &bytes,
            ReadLimits {
                max_resource_bytes: 64 * 1024 * 1024,
            },
            &cancel,
        )?;
        out.write_all(&decoded.width.to_le_bytes())?;
        out.write_all(&decoded.height.to_le_bytes())?;
        out.write_all(&(decoded.rgba.len() as u32).to_le_bytes())?;
        out.write_all(&decoded.rgba)?;
    }
    out.flush()?;
    Ok(())
}
