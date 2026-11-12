//! Pure numerical diagnostic seam; contains no asset discovery or game calls.
use std::{io::Read,sync::atomic::AtomicBool};
fn main()->Result<(),Box<dyn std::error::Error>>{
    let mut text=String::new();std::io::stdin().read_to_string(&mut text)?;let inputs:Vec<u32>=serde_json::from_str(&text)?;
    let output=inputs.into_iter().map(|input|external_cas_material::native_lookup::generate_native_skin_detail_lookup(input,&AtomicBool::new(false)).map(|value|serde_json::json!({"input":input,"bgra":value.bgra}))).collect::<Result<Vec<_>,_>>()?;println!("{}",serde_json::to_string(&output)?);Ok(())
}
