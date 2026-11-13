// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit read-only local resource observer; output contains private pins.
use external_cas_assets::{PackageIndex,ReadLimits,ResourceKey};
use external_cas_room::parse_simdata;
use sha2::{Digest,Sha256};
use std::sync::atomic::AtomicBool;
fn main()->Result<(),Box<dyn std::error::Error>> {
 let args=std::env::args().skip(1).collect::<Vec<_>>();if args.len()!=2 {return Err("usage: inspect_static_data explicit-provider full-TGI".into());}
 let index=PackageIndex::open(&args[0])?;let key:ResourceKey=args[1].parse()?;
 let rows=index.matching(key).filter(|e|!e.deleted()).collect::<Vec<_>>();if rows.len()!=1{return Err("source resource absent/ambiguous".into());}
 let raw=index.read_resource(rows[0],ReadLimits{max_resource_bytes:16*1024*1024})?;
 let doc=parse_simdata(&raw,&AtomicBool::new(false))?;
 println!("{}",serde_json::to_string_pretty(&serde_json::json!({"private":true,"provider":index.path,"key":key.to_string(),"ordinal":rows[0].ordinal,"index_sha256":index.index_sha256.iter().map(|b|format!("{b:02x}")).collect::<String>(),"payload_sha256":format!("{:x}",Sha256::digest(&raw)),"document":doc}))?);Ok(())
}
