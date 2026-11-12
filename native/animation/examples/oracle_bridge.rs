// SPDX-License-Identifier: GPL-3.0-or-later
// Private in-memory comparison seam. Its stdout contains derived asset data;
// callers must consume it privately and print only aggregate comparisons.
use external_cas_animation::*;
use serde_json::{Value, json};
use std::io::{Read, Write};

fn bytes(value: &Value) -> Result<Vec<u8>> {
    value
        .as_array()
        .ok_or_else(|| AnimationError::Unsupported("expected byte array".into()))?
        .iter()
        .map(|x| {
            x.as_u64()
                .filter(|v| *v <= 255)
                .map(|v| v as u8)
                .ok_or_else(|| AnimationError::Unsupported("invalid byte".into()))
        })
        .collect()
}
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut input = Vec::new();
    std::io::stdin()
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut input)?;
    if input.len() > 32 * 1024 * 1024 {
        return Err("comparison request exceeds budget".into());
    }
    let req: Value = serde_json::from_slice(&input)?;
    let rig = Rig::parse(&bytes(&req["rig"])?)?;
    let clip = Clip::parse(
        &bytes(&req["clip"])?,
        if req["studioLayout"] == true {
            FrameLayout::StudioExplicit
        } else {
            FrameLayout::Conservative
        },
    )?;
    let bind = if let Some(rows) = req["bindRows"].as_array() {
        let mut globals = Vec::new();
        for matrix in rows {
            let values = matrix.as_array().ok_or("invalid bind rows")?;
            if values.len() != 16 {
                return Err("invalid bind extent".into());
            }
            let mut rows = [0.; 16];
            for (dst, value) in rows.iter_mut().zip(values) {
                *dst = value.as_f64().ok_or("invalid bind value")?;
            }
            globals.push(Matrix4::from_rows_f64(rows));
        }
        rig.bind_pose_from_globals(&globals)?
    } else {
        rig.bind_pose()?
    };
    let options = SampleOptions {
        ancillary_policy: AncillaryPolicy::PreserveAndIgnore,
        unsupported_channels: if req["skipNonBone"] == true {
            UnsupportedChannelPolicy::PreserveAndSkipNonBoneTracks
        } else {
            UnsupportedChannelPolicy::Reject
        },
        convention: if req["studioConvention"] == true {
            PoseConvention::StudioLoadedBindBlender43
        } else {
            PoseConvention::SourceLocal
        },
        ..Default::default()
    };
    let samples=req["ticks"].as_array().ok_or("missing ticks")?.iter().map(|v| {
        let tick=v.as_f64().unwrap_or(f64::NAN) as f32;
        match clip.sample_with_bind(&rig,&bind,tick,options) {
            Ok(pose)=>json!({"tick":tick,"palette":pose.palette,"ignoredEvents":pose.ignored_events,"ignoredIk":pose.ignored_ik_slots,"nonRigid":pose.contains_nonrigid_transforms,"skippedChannels":pose.skipped_channels.iter().map(|ch|json!({"target":ch.target_hash,"subTarget":ch.sub_target,"type":ch.type_id,"reason":ch.reason})).collect::<Vec<_>>(),"quaternionAdjustment":pose.max_quaternion_norm_adjustment}),
            Err(err)=>json!({"tick":tick,"error":err.to_string()}),
        }
    }).collect::<Vec<_>>();
    let output = json!({
        "rig":{"bones":rig.bones.iter().map(|b|json!({"hash":b.hash,"name":b.name,"parentIndex":b.parent_index,"position":b.position,"orientation":b.orientation,"scale":b.scale})).collect::<Vec<_>>(),"chains":rig.chains.len(),"bindRows":bind.global.iter().map(|m|m.to_rows_f64()).collect::<Vec<_>>()},
        "clip":{"name":clip.name(),"maxTick":clip.max_tick(),"channels":clip.codec.as_ref().map(|c|c.channels.iter().map(|ch|json!({"target":ch.target_hash,"type":ch.type_id,"subTarget":ch.sub_target,"support":format!("{:?}",ch.support),"frames":ch.frames.iter().map(|f|json!({"tick":f.tick,"flags":f.flags,"values":f.values})).collect::<Vec<_>>()})).collect::<Vec<_>>())},
        "samples":samples,
    });
    serde_json::to_writer(std::io::stdout().lock(), &output)?;
    std::io::stdout().flush()?;
    Ok(())
}
