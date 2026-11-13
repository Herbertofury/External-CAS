//! Pure native CPU skin numeric comparison seam, without asset discovery.
use std::{io::Read,sync::atomic::AtomicBool};
#[derive(serde::Deserialize)]struct Input{packed:u32,opacity:u32,detail:[u8;16],skin:[[u8;4];16]}
fn main()->Result<(),Box<dyn std::error::Error>>{
    let mut text=String::new();std::io::stdin().read_to_string(&mut text)?;let inputs:Vec<Input>=serde_json::from_str(&text)?;let cancel=AtomicBool::new(false);let mut output=vec![];
    for input in inputs{let lookup=external_cas_material::native_cpu_skin::generate_native_cpu_skin_lookup(input.packed,&cancel)?;let pixels=external_cas_material::native_cpu_skin::compose_native_cpu_skin_block(&input.detail,&input.skin,&lookup,input.opacity,&cancel)?;output.push(serde_json::json!({"bgra":pixels,"weight":external_cas_material::native_cpu_skin::native_cpu_skin_weight(input.opacity)}));}println!("{}",serde_json::to_string(&output)?);Ok(())
}
