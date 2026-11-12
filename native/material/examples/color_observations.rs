//! Private read-only oracle seam. Prints derived source colors; redirect output
//! to an ignored local file and never commit the output or input manifests.
use external_cas_assets::{PackageIndex,ReadLimits,ResourceKey};
use external_cas_material::{color::{PackedColorShift,shift_reference_rgb},texture};
use sha2::{Digest,Sha256};
fn main()->Result<(),Box<dyn std::error::Error>>{
    let mut args=std::env::args().skip(1);
    let config:serde_json::Value=serde_json::from_slice(&std::fs::read(args.next().ok_or("Private source config required")?)?)?;
    if config["private"]!=true{return Err("Explicit private source config required".into());}
    let evidence:serde_json::Value=serde_json::from_slice(&std::fs::read(args.next().ok_or("Pinned texture evidence required")?)?)?;
    let mut observations=Vec::new();let mut resources=0;
    let shifts=[0x4000100000000000,0x4000E00000000000,0x40000000E0000000,0x4000000010000000,0x4000000000001000,0x400000000000E000,0x20001000F0000800,0x4000200020002000];
    for sample in evidence["samples"].as_array().ok_or("Texture sample list")?{
        let category=sample["category"].as_str().ok_or("Category")?;
        let package=PackageIndex::open(config["samples"][category].as_str().ok_or("Private sample path")?)?;
        let mut reader=package.reader()?;
        for record in sample["texturePixels"].as_array().ok_or("Texture records")?{
            let key:ResourceKey=record["key"].as_str().ok_or("Texture key")?.parse()?;
            let entries=package.matching(key).collect::<Vec<_>>();if entries.len()!=1{return Err("Ambiguous source texture".into());}
            let raw=reader.read_resource(entries[0],ReadLimits::default())?;
            if format!("{:x}",Sha256::digest(&raw))!=record["payloadHash"].as_str().ok_or("Payload pin")?{return Err("Source texture changed".into());}
            let rgba=if raw.starts_with(b"LRLE"){external_cas_assets::lrle::decode_lrle(&raw,ReadLimits::default())?.rgba}else{texture::decode(&raw).map_err(|e|format!("{key}: {e}"))?.rgba};
            resources+=1;
            for pixel in rgba.chunks_exact(4).step_by((rgba.len()/4/256).max(1)){
                if pixel[3]==0{continue;}
                let input=[pixel[0],pixel[1],pixel[2]];
                for raw in shifts{let shift=PackedColorShift(raw);let output=shift_reference_rgb(input,shift)?;
                    observations.push(serde_json::json!({"input":input,"shift":format!("{raw:016X}"),"output":output}));}
            }
        }
        package.revalidate(&std::sync::atomic::AtomicBool::new(false))?;
    }
    println!("{}",serde_json::to_string(&serde_json::json!({"private":true,"resources":resources,"observations":observations,"native_game_accepted":false}))?);Ok(())
}
