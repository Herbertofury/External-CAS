// SPDX-License-Identifier: GPL-3.0-or-later
use external_cas_assets::PackageIndex;
fn main()->Result<(),Box<dyn std::error::Error>>{let index=PackageIndex::open(std::env::args().nth(1).ok_or("explicit UI.package required")?)?;let c=external_cas_fidelity::font_config::load_font_config(&index)?;println!("{}",serde_json::to_string(&serde_json::json!({"private":true,"pin":c.pin,"config":c.config}))?);Ok(())}
