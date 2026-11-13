//! Synthetic numeric diagnostic only; accepts no game resource paths.
use external_cas_material::{native_cpu_bc1::{decode_native_cpu_detail_green_block,decode_native_cpu_skin_bgra_block,compose_native_cpu_skin_bc1_block},native_cpu_skin::generate_native_cpu_skin_lookup};
use std::{io::{Read,Write},sync::atomic::AtomicBool};
#[derive(serde::Deserialize)]struct Input{detail:[u8;8],skin:[u8;8],packed:u32,opacity:u32}
fn main()->Result<(),Box<dyn std::error::Error>>{
    let mut input=String::new();std::io::stdin().read_to_string(&mut input)?;let inputs:Vec<Input>=serde_json::from_str(&input)?;let cancel=AtomicBool::new(false);let mut output=vec![];
    for input in inputs{let lookup=generate_native_cpu_skin_lookup(input.packed,&cancel)?;let detail=decode_native_cpu_detail_green_block(&input.detail,&cancel)?;let skin=decode_native_cpu_skin_bgra_block(&input.skin,&cancel)?;let composed=compose_native_cpu_skin_bc1_block(&input.detail,&input.skin,&lookup,input.opacity,&cancel)?;output.push(serde_json::json!({"detail":detail,"skin":skin,"composed":composed}));}
    std::io::stdout().write_all(&serde_json::to_vec(&output)?)?;Ok(())
}
