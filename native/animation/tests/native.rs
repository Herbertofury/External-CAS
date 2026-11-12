// SPDX-License-Identifier: GPL-3.0-or-later
use external_cas_animation::*;

fn u16v(out: &mut Vec<u8>, v: u16) {
    out.extend(v.to_le_bytes());
}
fn u32v(out: &mut Vec<u8>, v: u32) {
    out.extend(v.to_le_bytes());
}
fn i32v(out: &mut Vec<u8>, v: i32) {
    out.extend(v.to_le_bytes());
}
fn f32v(out: &mut Vec<u8>, v: f32) {
    out.extend(v.to_le_bytes());
}
fn text(out: &mut Vec<u8>, value: &str) {
    u32v(out, value.len() as u32);
    out.extend(value.as_bytes());
}
fn rig_bytes(parents: &[i32]) -> Vec<u8> {
    let mut out = Vec::new();
    u32v(&mut out, 3);
    u32v(&mut out, 1);
    u32v(&mut out, parents.len() as u32);
    for (index, &parent) in parents.iter().enumerate() {
        for v in [index as f32, 0., 0., 0., 0., 0., 1., 1., 1., 1.] {
            f32v(&mut out, v);
        }
        text(&mut out, &format!("bone{index}"));
        i32v(&mut out, index as i32);
        i32v(&mut out, parent);
        u32v(&mut out, 100 + index as u32);
        u32v(&mut out, 0x23);
    }
    u32v(&mut out, 1);
    u32v(&mut out, 1);
    i32v(&mut out, 0);
    i32v(&mut out, -1);
    i32v(&mut out, -1);
    i32v(&mut out, 0);
    out
}
#[derive(Clone)]
struct TestChannel {
    target: u32,
    kind: u8,
    component: u8,
    offset: f32,
    scale: f32,
    frames: Vec<(u16, u16, Vec<u8>)>,
}
fn translation(target: u32, frames: &[(u16, u16, u32)]) -> TestChannel {
    TestChannel {
        target,
        kind: 18,
        component: 1,
        offset: 0.,
        scale: 1.,
        frames: frames
            .iter()
            .map(|(tick, flags, packed)| (*tick, *flags, packed.to_le_bytes().to_vec()))
            .collect(),
    }
}
fn rotation(target: u32, frames: &[(u16, [u16; 4])]) -> TestChannel {
    TestChannel {
        target,
        kind: 20,
        component: 2,
        offset: 0.,
        scale: 1.,
        frames: frames
            .iter()
            .map(|(tick, q)| (*tick, 0, q.iter().flat_map(|v| v.to_le_bytes()).collect()))
            .collect(),
    }
}
fn clip_bytes(channels: &[TestChannel], ancillary: bool) -> Vec<u8> {
    let data_start = 48 + channels.len() * 20 + 4;
    let mut codec = Vec::new();
    codec.extend(b"_pilC3S_");
    u32v(&mut codec, 2);
    u32v(&mut codec, 0);
    f32v(&mut codec, 1. / 30.);
    u16v(&mut codec, 31);
    u16v(&mut codec, 0);
    u32v(&mut codec, channels.len() as u32);
    u32v(&mut codec, 0);
    u32v(&mut codec, 48);
    u32v(&mut codec, data_start as u32);
    u32v(&mut codec, (data_start - 4) as u32);
    u32v(&mut codec, (data_start - 2) as u32);
    let mut next = data_start;
    for c in channels {
        u32v(&mut codec, next as u32);
        u32v(&mut codec, c.target);
        f32v(&mut codec, c.offset);
        f32v(&mut codec, c.scale);
        u16v(&mut codec, c.frames.len() as u16);
        codec.extend([c.kind, c.component]);
        next += c.frames.iter().map(|f| 4 + f.2.len()).sum::<usize>();
    }
    codec.extend(b"a\0b\0");
    for c in channels {
        for (tick, flags, data) in &c.frames {
            u16v(&mut codec, *tick);
            u16v(&mut codec, *flags);
            codec.extend(data);
        }
    }
    let mut out = Vec::new();
    u32v(&mut out, 14);
    u32v(&mut out, 0);
    f32v(&mut out, 1.);
    for v in [0., 0., 0., 1., 0., 0., 0.] {
        f32v(&mut out, v);
    }
    for _ in 0..4 {
        u32v(&mut out, 0);
    }
    text(&mut out, "test clip");
    text(&mut out, "rig");
    u32v(&mut out, 0);
    u32v(&mut out, u32::from(ancillary));
    if ancillary {
        u16v(&mut out, 2);
        u16v(&mut out, 3);
        text(&mut out, "object");
        text(&mut out, "joint");
    }
    u32v(&mut out, u32::from(ancillary));
    if ancillary {
        u32v(&mut out, 44);
        u32v(&mut out, 12);
        out.extend([0xAB; 12]);
    }
    u32v(&mut out, codec.len() as u32);
    out.extend(codec);
    out
}
fn current_clip_bytes(ancillary:bool)->Vec<u8>{
    let mut bytes=clip_bytes(&[translation(100,&[(0,0,0),(30,0,1023)])],ancillary);
    let codec=bytes.windows(8).position(|w|w==b"_pilC3S_").unwrap();
    bytes[..4].copy_from_slice(&18u32.to_le_bytes());
    bytes.splice(codec-4..codec-4,0xd057fcc534c1bcbb_u64.to_le_bytes());bytes
}
#[test]
fn current_v18_retains_new_rig_reference_opaque_words_events_and_exact_codec_extent(){
    for ancillary in [false,true]{let mut raw=current_clip_bytes(ancillary);raw[12..16].copy_from_slice(&0x7fc0_1234u32.to_le_bytes());raw[40..44].copy_from_slice(&0xdead_beefu32.to_le_bytes());
        let parsed=Clip::parse(&raw,FrameLayout::Conservative).unwrap();let metadata=parsed.metadata.as_ref().unwrap();let current=metadata.current_header.as_ref().unwrap();
        assert_eq!(current.metadata_words[0],0x7fc0_1234);assert_eq!(current.metadata_words[7],0xdead_beef);assert_eq!(current.rig_instance,0xd057fcc534c1bcbb);assert_eq!(&raw[current.codec_range.start..current.codec_range.start+8],b"_pilC3S_");assert_eq!(current.codec_range.end,raw.len());assert_eq!(parsed.original,raw);assert!(parsed.codec.is_some());assert_eq!(metadata.events.len(),usize::from(ancillary));
        assert!(Clip::parse(&clip_bytes(&[],false),FrameLayout::Conservative).unwrap().metadata.unwrap().current_header.is_none());
    }
}
#[test]
fn every_current_v18_prefix_rejects_and_unknown_versions_remain_opaque(){
    let raw=current_clip_bytes(true);for n in 0..raw.len(){assert!(Clip::parse(&raw[..n],FrameLayout::Conservative).is_err(),"prefix {n}");}
    for version in [15u32,16,17,19]{let mut raw=raw.clone();raw[..4].copy_from_slice(&version.to_le_bytes());let parsed=Clip::parse(&raw,FrameLayout::Conservative).unwrap();assert_eq!(parsed.original,raw);assert!(parsed.metadata.is_none());assert!(parsed.codec.is_none());}
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 2e-6, "{a} != {b}");
}

#[test]
fn rig_parses_raw_transforms_chains_and_preserves_unknown_data() {
    let mut bytes = rig_bytes(&[-1, 0]);
    bytes.extend([7, 8, 9]);
    let rig = Rig::parse(&bytes).unwrap();
    assert_eq!(rig.original, bytes);
    assert_eq!(rig.trailing, [7, 8, 9]);
    assert_eq!(rig.bones[1].parent_index, 0);
    assert_eq!(rig.chains[0].pole, -1);
    assert_eq!(rig.bones[1].position, [1., 0., 0.]);
    let mut unsupported = bytes;
    unsupported[0] = 4;
    let parsed = Rig::parse(&unsupported).unwrap();
    assert_eq!(parsed.original, unsupported);
    assert!(parsed.bind_pose().is_err());
}
#[test]
fn rig_rejects_every_truncation_and_cycles_but_evaluates_non_topological_order() {
    let bytes = rig_bytes(&[-1, 0]);
    for n in 0..bytes.len() {
        assert!(Rig::parse(&bytes[..n]).is_err(), "accepted truncation {n}");
    }
    assert!(Rig::parse(&rig_bytes(&[1, 0])).is_err());
    let rig = Rig::parse(&rig_bytes(&[1, -1])).unwrap();
    assert_eq!(rig.evaluation_order().unwrap(), [1, 0]);
    close(
        rig.bind_pose().unwrap().global[0].transform_point([0.; 3])[0],
        1.,
    );
}
#[test]
fn inverse_bind_rest_pose_is_identity_for_rotation_translation_and_scale() {
    let mut rig = Rig::parse(&rig_bytes(&[-1, 0])).unwrap();
    rig.bones[0].orientation = [
        0.,
        0.,
        std::f32::consts::FRAC_1_SQRT_2,
        std::f32::consts::FRAC_1_SQRT_2,
    ];
    rig.bones[1].scale = [2., 3., 4.];
    let bind = rig.bind_pose().unwrap();
    for (world, inverse) in bind.global.iter().zip(&bind.inverse) {
        let m = world.multiply(*inverse);
        for (a, b) in m.0.iter().zip(Matrix4::IDENTITY.0) {
            close(*a, b);
        }
    }
    rig.bones[1].scale = [0., 1., 1.];
    assert!(rig.bind_pose().is_err());
}
#[test]
fn clip_preserves_events_ik_and_unknown_channels_without_silent_animation() {
    let mut unknown = translation(100, &[(0, 0, 0)]);
    unknown.kind = 250;
    let bytes = clip_bytes(&[unknown], true);
    let clip = Clip::parse(&bytes, FrameLayout::Conservative).unwrap();
    assert_eq!(clip.original, bytes);
    let meta = clip.metadata.as_ref().unwrap();
    assert_eq!(meta.events[0].payload, [0xAB; 12]);
    assert_eq!(meta.ik_slots[0].target_joint, "joint");
    assert_eq!(
        clip.codec.as_ref().unwrap().channels[0].support,
        ChannelSupport::UnsupportedEncoding
    );
    assert!(
        !clip.codec.as_ref().unwrap().channels[0]
            .original_region
            .is_empty()
    );
    let rig = Rig::parse(&rig_bytes(&[-1])).unwrap();
    assert!(clip.sample(&rig, 0., SampleOptions::default()).is_err());
    assert!(
        clip.sample(
            &rig,
            0.,
            SampleOptions {
                ancillary_policy: AncillaryPolicy::PreserveAndIgnore,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn clip_rejects_all_structural_truncations_and_impossible_extents() {
    let bytes = clip_bytes(
        &[
            translation(100, &[(0, 0, 1023)]),
            rotation(100, &[(0, [0, 0, 0, 4095])]),
        ],
        true,
    );
    for n in 0..bytes.len() {
        assert!(
            Clip::parse(&bytes[..n], FrameLayout::Conservative).is_err(),
            "accepted truncation {n}"
        );
    }
    let mut bad = bytes.clone();
    let codec = bad.windows(8).position(|w| w == b"_pilC3S_").unwrap();
    bad[codec + 24..codec + 28].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(Clip::parse(&bad, FrameLayout::Conservative).is_err());
}
#[test]
fn signed_quantized_vectors_and_quaternions_decode_independently_known_values() {
    let mut position = translation(100, &[(0, 1, 1023 | (512 << 10) | (256 << 20))]);
    position.offset = 0.25;
    position.scale = 2.;
    let bytes = clip_bytes(&[position, rotation(100, &[(0, [0, 0, 0, 4095])])], false);
    let clip = Clip::parse(&bytes, FrameLayout::Conservative).unwrap();
    let channels = &clip.codec.as_ref().unwrap().channels;
    let v = channels[0].frames[0].values.as_ref().unwrap();
    close(v[0] as f64, -1.75);
    close(v[1] as f64, 0.25 + 2. * 512. / 1023.);
    close(v[2] as f64, 0.25 + 2. * 256. / 1023.);
    assert_eq!(
        channels[1].frames[0].values.as_ref().unwrap(),
        &[0., 0., 0., 1.]
    );
}
#[test]
fn invalid_quantization_flags_reserved_bits_and_frame_order_remain_explicit() {
    for channel in [
        translation(100, &[(0, 8, 0)]),
        translation(100, &[(0, 0, 1 << 31)]),
        rotation(100, &[(0, [4096, 0, 0, 0])]),
    ] {
        let clip = Clip::parse(&clip_bytes(&[channel], false), FrameLayout::Conservative).unwrap();
        assert_eq!(
            clip.codec.unwrap().channels[0].support,
            ChannelSupport::PartialValues
        );
    }
    let unordered = translation(100, &[(4, 0, 0), (4, 0, 0)]);
    let clip = Clip::parse(&clip_bytes(&[unordered], false), FrameLayout::Conservative).unwrap();
    assert_eq!(
        clip.codec.unwrap().channels[0].support,
        ChannelSupport::InvalidFrameOrder
    );
}
#[test]
fn sampling_holds_components_and_composes_hierarchy_with_inverse_bind() {
    let rig = Rig::parse(&rig_bytes(&[-1, 0])).unwrap();
    let clip = Clip::parse(
        &clip_bytes(
            &[
                translation(100, &[(0, 0, 0), (10, 0, 1023)]),
                rotation(100, &[(0, [0, 0, 0, 4095])]),
            ],
            false,
        ),
        FrameLayout::Conservative,
    )
    .unwrap();
    for (tick, expected) in [(0., 0.), (9.999, 0.), (10., 1.), (100., 1.)] {
        let pose = clip.sample(&rig, tick, SampleOptions::default()).unwrap();
        assert_eq!(pose.palette.len(), 2);
        for matrix in pose.palette {
            close(matrix[12] as f64, expected);
            close(matrix[13] as f64, 0.);
            close(matrix[14] as f64, 0.);
        }
    }
}
#[test]
fn coordinate_conversion_conjugates_palette_instead_of_rotating_only_vertices() {
    let rig = Rig::parse(&rig_bytes(&[-1])).unwrap();
    let clip = Clip::parse(
        &clip_bytes(&[translation(100, &[(0, 0, 1023 << 10)])], false),
        FrameLayout::Conservative,
    )
    .unwrap();
    let options = SampleOptions {
        coordinate_basis: CoordinateBasis::YUpToZUp,
        ..Default::default()
    };
    let pose = clip.sample(&rig, 0., options).unwrap();
    close(pose.palette[0][12] as f64, 0.);
    close(pose.palette[0][13] as f64, 0.);
    close(pose.palette[0][14] as f64, 1.);
}
#[test]
fn duplicate_hashes_channels_and_nonfinite_ticks_fail_without_partial_palette() {
    let mut rig = Rig::parse(&rig_bytes(&[-1, 0])).unwrap();
    rig.bones[1].hash = 100;
    let channel = translation(100, &[(0, 0, 0)]);
    let clip = Clip::parse(
        &clip_bytes(&[channel.clone()], false),
        FrameLayout::Conservative,
    )
    .unwrap();
    assert!(clip.sample(&rig, 0., SampleOptions::default()).is_err());
    let rig = Rig::parse(&rig_bytes(&[-1])).unwrap();
    let duplicate = Clip::parse(
        &clip_bytes(&[channel.clone(), channel], false),
        FrameLayout::Conservative,
    )
    .unwrap();
    assert!(
        duplicate
            .sample(&rig, 0., SampleOptions::default())
            .is_err()
    );
    assert!(
        clip.sample(&rig, f32::NAN, SampleOptions::default())
            .is_err()
    );
}
#[test]
fn ancillary_ignore_is_explicit_counted_and_raw_bytes_stay_unchanged() {
    let rig = Rig::parse(&rig_bytes(&[-1])).unwrap();
    let bytes = clip_bytes(&[translation(100, &[(0, 0, 0)])], true);
    let clip = Clip::parse(&bytes, FrameLayout::Conservative).unwrap();
    assert!(clip.sample(&rig, 0., SampleOptions::default()).is_err());
    let pose = clip
        .sample(
            &rig,
            0.,
            SampleOptions {
                ancillary_policy: AncillaryPolicy::PreserveAndIgnore,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(pose.ignored_events, 1);
    assert_eq!(pose.ignored_ik_slots, 1);
    assert_eq!(clip.original, bytes);
}
#[test]
fn studio_convention_requires_declared_bind_and_keeps_root_rotation_policy_explicit() {
    let rig = Rig::parse(&rig_bytes(&[-1])).unwrap();
    let clip = Clip::parse(
        &clip_bytes(&[rotation(100, &[(0, [0, 0, 4095, 0])])], false),
        FrameLayout::Conservative,
    )
    .unwrap();
    let options = SampleOptions {
        convention: PoseConvention::StudioLoadedBindBlender43,
        ..Default::default()
    };
    assert!(clip.sample(&rig, 0., options).is_err());
    let bind = rig.bind_pose_from_globals(&[Matrix4::IDENTITY]).unwrap();
    let pose = clip.sample_with_bind(&rig, &bind, 0., options).unwrap();
    for (a, b) in pose.palette[0].iter().zip(Matrix4::IDENTITY.0) {
        close(*a as f64, b);
    }
    let native = clip.sample(&rig, 0., SampleOptions::default()).unwrap();
    close(native.palette[0][0] as f64, -1.);
}

#[test]
fn partial_preview_reports_non_bone_and_empty_tracks_but_never_skips_active_missing_bones() {
    let rig = Rig::parse(&rig_bytes(&[-1])).unwrap();
    let position = translation(100, &[(0, 0, 0)]);
    let mut control = translation(999, &[(0, 0, 0)]);
    control.kind = 5;
    control.component = 15;
    let mut inactive = rotation(100, &[]);
    inactive.kind = 12;
    let clip = Clip::parse(
        &clip_bytes(&[position.clone(), control, inactive], false),
        FrameLayout::Conservative,
    )
    .unwrap();
    assert!(clip.sample(&rig, 0., SampleOptions::default()).is_err());
    let options = SampleOptions {
        unsupported_channels: UnsupportedChannelPolicy::PreserveAndSkipNonBoneTracks,
        ..Default::default()
    };
    let pose = clip.sample(&rig, 0., options).unwrap();
    assert_eq!(pose.skipped_channels.len(), 2);
    assert_eq!(pose.edited_channels, 1);
    let missing = Clip::parse(
        &clip_bytes(&[translation(999, &[(0, 0, 0)])], false),
        FrameLayout::Conservative,
    )
    .unwrap();
    assert!(missing.sample(&rig, 0., options).is_err());
    let mut scale = position.clone();
    scale.component = 3;
    let scale = Clip::parse(
        &clip_bytes(&[position, scale], false),
        FrameLayout::Conservative,
    )
    .unwrap();
    assert!(scale.sample(&rig, 0., options).is_err());
}

#[test]
fn animated_scale_is_applied_and_reported_independently_of_channel_order() {
    let rig = Rig::parse(&rig_bytes(&[-1, 0])).unwrap();
    let mut scale = translation(100, &[(0, 0, 0)]);
    scale.component = 3;
    scale.offset = 2.;
    scale.scale = 0.;
    let rotation = rotation(100, &[(0, [0, 0, 0, 4095])]);
    let a = Clip::parse(
        &clip_bytes(&[scale.clone(), rotation.clone()], false),
        FrameLayout::Conservative,
    )
    .unwrap()
    .sample(&rig, 0., SampleOptions::default())
    .unwrap();
    let b = Clip::parse(
        &clip_bytes(&[rotation, scale], false),
        FrameLayout::Conservative,
    )
    .unwrap()
    .sample(&rig, 0., SampleOptions::default())
    .unwrap();
    assert_eq!(a.palette, b.palette);
    assert!(a.contains_nonrigid_transforms);
    for matrix in a.palette {
        for index in [0, 5, 10] {
            close(matrix[index] as f64, 2.);
        }
    }
}
