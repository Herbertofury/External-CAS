// SPDX-License-Identifier: GPL-3.0-or-later
// CLIP v14 / S3CLIP v2 layout and quantization ported from recovered s4pi-
// derived clip-{metadata,frames,values,sample}.mjs. See README provenance.
use crate::{BindPose, Matrix4, Result, Rig, fingerprint, invalid, reader::Reader, unsupported};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrameLayout {
    #[default]
    Conservative,
    /// Explicit Studio interpretation of disputed low-three-bit encodings.
    StudioExplicit,
}
#[derive(Debug, Clone)]
pub struct ClipEvent {
    pub type_id: u32,
    pub payload: Vec<u8>,
}
#[derive(Debug, Clone)]
pub struct IkSlot {
    pub chain: u16,
    pub slot: u16,
    pub target_object: String,
    pub target_joint: String,
}
#[derive(Debug, Clone)]
pub struct ClipMetadata {
    pub version: u32,
    pub flags: u32,
    pub duration: f32,
    pub initial_quaternion: [f32; 4],
    pub initial_translation: [f32; 3],
    pub namespace_hashes: [u32; 4],
    pub name: String,
    pub rig_namespace: String,
    pub namespaces: Vec<String>,
    pub ik_slots: Vec<IkSlot>,
    pub events: Vec<ClipEvent>,
}
#[derive(Debug, Clone)]
pub struct Frame {
    pub tick: u16,
    pub flags: u16,
    pub encoded: Vec<u8>,
    pub values: Option<Vec<f32>>,
    pub unsupported_reason: Option<&'static str>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelSupport {
    Decoded,
    Empty,
    AmbiguousLayout,
    UnsupportedEncoding,
    PartialValues,
    InvalidFrameOrder,
}
#[derive(Debug, Clone)]
pub struct Channel {
    pub data_offset: u32,
    pub target_hash: u32,
    pub offset: f32,
    pub scale: f32,
    pub frame_count: u16,
    pub type_id: u8,
    pub sub_target: u8,
    pub frames: Vec<Frame>,
    pub support: ChannelSupport,
    /// Byte range in `Clip::original`, retained without per-channel duplication.
    pub original_region: std::ops::Range<usize>,
}
#[derive(Debug, Clone)]
pub struct ClipCodec {
    pub version: u32,
    pub flags: u32,
    pub tick_length: f32,
    pub ticks: u16,
    pub padding: u16,
    pub name: String,
    pub source_asset_name: String,
    pub palette: Vec<f32>,
    pub channels: Vec<Channel>,
    pub layout: FrameLayout,
}
#[derive(Debug, Clone)]
pub struct Clip {
    pub version: u32,
    pub metadata: Option<ClipMetadata>,
    pub codec: Option<ClipCodec>,
    pub original: Vec<u8>,
    pub sha256: [u8; 32],
    pub trailing: Vec<u8>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AncillaryPolicy {
    #[default]
    Reject,
    PreserveAndIgnore,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PoseConvention {
    /// Absolute local translation/rotation channels in the supplied rig basis.
    #[default]
    SourceLocal,
    /// Recovered Studio 4.3.2 matrix-assignment convention using a caller bind.
    /// Root rotation is ignored exactly as in the recovered source-tool route.
    StudioLoadedBindBlender43,
}
#[derive(Debug, Clone, Copy, Default)]
pub enum CoordinateBasis {
    #[default]
    Source,
    /// Caller declares source Y-up and requests (x,-z,y).
    YUpToZUp,
    /// Caller declares source Z-up and requests (x,z,-y).
    ZUpToYUp,
    Explicit(Matrix4),
}
impl CoordinateBasis {
    pub fn matrix(self) -> Matrix4 {
        match self {
            Self::Source => Matrix4::IDENTITY,
            Self::YUpToZUp => Matrix4::from_rows_f64([
                1., 0., 0., 0., 0., 0., -1., 0., 0., 1., 0., 0., 0., 0., 0., 1.,
            ]),
            Self::ZUpToYUp => Matrix4::from_rows_f64([
                1., 0., 0., 0., 0., 0., 1., 0., 0., -1., 0., 0., 0., 0., 0., 1.,
            ]),
            Self::Explicit(matrix) => matrix,
        }
    }
}
#[derive(Debug, Clone, Copy, Default)]
pub struct SampleOptions {
    pub ancillary_policy: AncillaryPolicy,
    pub coordinate_basis: CoordinateBasis,
    pub convention: PoseConvention,
    pub unsupported_channels: UnsupportedChannelPolicy,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnsupportedChannelPolicy {
    #[default]
    Reject,
    /// Explicit partial preview: retain/report empty and non-bone control tracks.
    /// Active bone transforms and scale tracks are never skipped.
    PreserveAndSkipNonBoneTracks,
}
#[derive(Debug, Clone)]
pub struct SkippedChannel {
    pub target_hash: u32,
    pub sub_target: u8,
    pub type_id: u8,
    pub reason: &'static str,
}
#[derive(Debug, Clone)]
pub struct SampledPose {
    /// Column-major float32 matrices in file bone order, <=256 entries.
    /// Vertex positions must use the same explicitly selected coordinate basis.
    pub palette: Vec<[f32; 16]>,
    pub global: Vec<Matrix4>,
    pub tick: f32,
    pub edited_channels: usize,
    pub ignored_events: usize,
    pub ignored_ik_slots: usize,
    pub skipped_channels: Vec<SkippedChannel>,
    pub max_quaternion_norm_adjustment: f64,
    /// Nonuniform/animated scales require inverse-transpose normal transforms.
    pub contains_nonrigid_transforms: bool,
    pub limitations: Vec<&'static str>,
}

fn ztext(bytes: &[u8], at: usize) -> Result<String> {
    if at < 48 || at >= bytes.len() {
        return Err(invalid(at, "invalid codec string offset"));
    }
    let tail = &bytes[at..];
    let n = tail
        .iter()
        .position(|b| *b == 0)
        .ok_or_else(|| invalid(at, "unterminated codec string"))?;
    if n > 4096 {
        return Err(invalid(at, "codec string exceeds budget"));
    }
    String::from_utf8(tail[..n].to_vec()).map_err(|_| invalid(at, "invalid codec UTF-8"))
}

impl Clip {
    pub fn parse(bytes: &[u8], layout: FrameLayout) -> Result<Self> {
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(invalid(0, "clip exceeds 16 MiB input budget"));
        }
        let mut r = Reader::new(bytes);
        let version = r.u32()?;
        let mut out = Self {
            version,
            metadata: None,
            codec: None,
            original: bytes.to_vec(),
            sha256: fingerprint(bytes),
            trailing: Vec::new(),
            limitations: Vec::new(),
        };
        if version != 14 {
            out.limitations.push(format!(
                "unsupported CLIP version {version}; original bytes retained"
            ));
            return Ok(out);
        }
        let flags = r.u32()?;
        let duration = r.f32()?;
        if duration < 0. {
            return Err(invalid(r.at - 4, "negative duration"));
        }
        let initial_quaternion = r.vector()?;
        let initial_translation = r.vector()?;
        let mut namespace_hashes = [0u32; 4];
        for hash in &mut namespace_hashes {
            *hash = r.u32()?;
        }
        let name = r.text()?;
        let rig_namespace = r.text()?;
        let namespace_count = r.u32()? as usize;
        if namespace_count > 1024 {
            return Err(invalid(r.at - 4, "namespace count exceeds budget"));
        }
        let mut namespaces = Vec::with_capacity(namespace_count);
        for _ in 0..namespace_count {
            namespaces.push(r.text()?);
        }
        let slot_count = r.u32()? as usize;
        if slot_count > 1024 {
            return Err(invalid(r.at - 4, "IK slot count exceeds budget"));
        }
        let mut ik_slots = Vec::with_capacity(slot_count);
        for _ in 0..slot_count {
            ik_slots.push(IkSlot {
                chain: r.u16()?,
                slot: r.u16()?,
                target_object: r.text()?,
                target_joint: r.text()?,
            });
        }
        let event_count = r.u32()? as usize;
        if event_count > 4096 {
            return Err(invalid(r.at - 4, "event count exceeds budget"));
        }
        let mut events = Vec::with_capacity(event_count);
        for _ in 0..event_count {
            let type_id = r.u32()?;
            let size = r.u32()? as usize;
            if !(12..=16 * 1024 * 1024).contains(&size) {
                return Err(invalid(r.at - 4, "invalid event extent"));
            }
            events.push(ClipEvent {
                type_id,
                payload: r.take(size)?.to_vec(),
            });
        }
        out.metadata = Some(ClipMetadata {
            version,
            flags,
            duration,
            initial_quaternion,
            initial_translation,
            namespace_hashes,
            name,
            rig_namespace,
            namespaces,
            ik_slots,
            events,
        });
        let codec_size = r.u32()? as usize;
        if codec_size > 16 * 1024 * 1024 {
            return Err(invalid(r.at - 4, "codec exceeds budget"));
        }
        let codec_base = r.at;
        let bytes = r.take(codec_size)?;
        out.trailing = r.data[r.at..].to_vec();
        if !out.trailing.is_empty() {
            out.limitations
                .push("unknown trailing CLIP bytes retained; mutation unavailable".into());
        }
        if bytes.len() < 48 || &bytes[..8] != b"_pilC3S_" {
            out.limitations
                .push("unsupported codec; original bytes retained".into());
            return Ok(out);
        }
        let mut c = Reader::new(bytes);
        c.take(8)?;
        let version = c.u32()?;
        if version != 2 {
            out.limitations.push(format!(
                "unsupported S3CLIP codec version {version}; original bytes retained"
            ));
            return Ok(out);
        }
        let flags = c.u32()?;
        let tick_length = c.f32()?;
        let ticks = c.u16()?;
        let padding = c.u16()?;
        let channel_count = c.u32()? as usize;
        let palette_count = c.u32()? as usize;
        let channel_offset = c.u32()? as usize;
        let palette_offset = c.u32()? as usize;
        let name_offset = c.u32()? as usize;
        let asset_offset = c.u32()? as usize;
        if tick_length <= 0.
            || channel_count > 4096
            || palette_count > 65536
            || channel_offset < 48
            || palette_offset < 48
        {
            return Err(invalid(c.at, "invalid codec layout"));
        }
        let name = ztext(bytes, name_offset)?;
        let source_asset_name = ztext(bytes, asset_offset)?;
        let mut p = Reader::new(bytes);
        p.at = palette_offset;
        p.take(palette_count * 4)?;
        p.at = palette_offset;
        let mut palette = Vec::with_capacity(palette_count);
        for _ in 0..palette_count {
            palette.push(p.f32()?);
        }
        c.at = channel_offset;
        c.take(channel_count * 20)?;
        c.at = channel_offset;
        let mut channels = Vec::with_capacity(channel_count);
        let mut frame_budget = 0usize;
        for _ in 0..channel_count {
            let data_offset = c.u32()?;
            let target_hash = c.u32()?;
            let offset = c.f32()?;
            let scale = c.f32()?;
            let frame_count = c.u16()?;
            let type_id = c.u8()?;
            let sub_target = c.u8()?;
            if data_offset < 48 || data_offset as usize > bytes.len() {
                return Err(invalid(c.at - 20, "invalid channel data offset"));
            }
            frame_budget += frame_count as usize;
            if frame_budget > 250_000 {
                return Err(invalid(c.at, "frame count exceeds budget"));
            }
            channels.push(Channel {
                data_offset,
                target_hash,
                offset,
                scale,
                frame_count,
                type_id,
                sub_target,
                frames: Vec::new(),
                support: ChannelSupport::UnsupportedEncoding,
                original_region: 0..0,
            });
        }
        // Retain opaque channel data without inferring unknown record widths.
        let mut boundaries = channels
            .iter()
            .map(|ch| ch.data_offset as usize)
            .collect::<Vec<_>>();
        boundaries.extend([
            channel_offset,
            palette_offset,
            name_offset,
            asset_offset,
            bytes.len(),
        ]);
        boundaries.sort_unstable();
        boundaries.dedup();
        for channel in &mut channels {
            let begin = channel.data_offset as usize;
            let end = boundaries
                .iter()
                .copied()
                .find(|v| *v > begin)
                .unwrap_or(bytes.len());
            channel.original_region = codec_base + begin..codec_base + end;
            decode_channel(channel, bytes, &palette, layout)?;
        }
        out.codec = Some(ClipCodec {
            version,
            flags,
            tick_length,
            ticks,
            padding,
            name,
            source_asset_name,
            palette,
            channels,
            layout,
        });
        Ok(out)
    }
    pub fn name(&self) -> &str {
        self.metadata
            .as_ref()
            .map(|m| m.name.as_str())
            .unwrap_or("Unsupported CLIP")
    }
    pub fn max_tick(&self) -> Option<u16> {
        self.codec
            .as_ref()?
            .channels
            .iter()
            .flat_map(|c| c.frames.iter().map(|f| f.tick))
            .max()
    }
    pub fn sample(&self, rig: &Rig, tick: f32, options: SampleOptions) -> Result<SampledPose> {
        self.sample_with_bind(rig, &rig.bind_pose()?, tick, options)
    }
    pub fn sample_with_bind(
        &self,
        rig: &Rig,
        bind: &BindPose,
        tick: f32,
        options: SampleOptions,
    ) -> Result<SampledPose> {
        if !tick.is_finite() || !(0.0..=65535.0).contains(&tick) {
            return Err(unsupported("tick must be finite and between 0 and 65535"));
        }
        let meta = self
            .metadata
            .as_ref()
            .ok_or_else(|| unsupported("CLIP metadata unavailable"))?;
        let codec = self
            .codec
            .as_ref()
            .ok_or_else(|| unsupported("CLIP codec unavailable"))?;
        if rig.bones.is_empty() || rig.bones.len() > 256 {
            return Err(unsupported("viewport supports 1 to 256 bones"));
        }
        if bind.rig_sha256 != rig.sha256
            || bind.local.len() != rig.bones.len()
            || bind.global.len() != rig.bones.len()
            || bind.inverse.len() != rig.bones.len()
        {
            return Err(unsupported("bind/rig identity mismatch"));
        }
        if options.ancillary_policy == AncillaryPolicy::Reject
            && (!meta.events.is_empty() || !meta.ik_slots.is_empty())
        {
            return Err(unsupported(format!(
                "clip contains {} events and {} IK slots; explicit preview ignore policy required",
                meta.events.len(),
                meta.ik_slots.len()
            )));
        }
        if options.convention == PoseConvention::StudioLoadedBindBlender43
            && !bind.caller_supplied_globals
        {
            return Err(unsupported(
                "Studio convention requires explicitly supplied loaded-bind globals",
            ));
        }
        // Revalidate potentially caller-mutated public structures before indexing.
        let order = rig.evaluation_order()?;
        let mut locals = bind.local.clone();
        if rig.bones.iter().any(|b| b.scale.iter().any(|v| *v <= 0.)) {
            return Err(unsupported(
                "negative or zero source bind scale is not supported for animation",
            ));
        }
        let mut scales = locals
            .iter()
            .map(|m| m.column_scales())
            .collect::<Result<Vec<_>>>()?;
        let mut seen = HashSet::new();
        let mut max_adjustment = 0.0f64;
        let mut skipped_channels = Vec::new();
        for channel in &codec.channels {
            if options.unsupported_channels
                == UnsupportedChannelPolicy::PreserveAndSkipNonBoneTracks
            {
                let reason = if channel.frame_count == 0 && channel.frames.is_empty() {
                    Some("zero-frame source-tool track retained; bind component held")
                } else if channel.sub_target >= 4 {
                    Some(
                        "non-bone control track retained; scalar/expression evaluation unavailable",
                    )
                } else {
                    None
                };
                if let Some(reason) = reason {
                    skipped_channels.push(SkippedChannel {
                        target_hash: channel.target_hash,
                        sub_target: channel.sub_target,
                        type_id: channel.type_id,
                        reason,
                    });
                    continue;
                }
            }
            if channel.support != ChannelSupport::Decoded {
                return Err(unsupported(format!(
                    "channel {:08X}/{} type {} has {:?}",
                    channel.target_hash, channel.sub_target, channel.type_id, channel.support
                )));
            }
            let index = rig.bone_index(channel.target_hash)?;
            if !seen.insert((index, channel.sub_target)) {
                return Err(unsupported("duplicate bone component channel"));
            }
            if channel.frames.is_empty()
                || channel.frames.windows(2).any(|f| f[0].tick >= f[1].tick)
            {
                return Err(unsupported("empty or unordered channel frames"));
            }
            let partition = channel.frames.partition_point(|f| f.tick as f32 <= tick);
            let values = channel.frames[partition.saturating_sub(1)]
                .values
                .as_ref()
                .ok_or_else(|| unsupported("undecoded frame values"))?;
            if values.iter().any(|v| !v.is_finite()) {
                return Err(unsupported("non-finite decoded frame values"));
            }
            match channel.sub_target {
                1 if values.len() == 3 => locals[index].replace_translation(values),
                2 if values.len() == 4 => {
                    let mut q = [
                        values[0] as f64,
                        values[1] as f64,
                        values[2] as f64,
                        values[3] as f64,
                    ];
                    let norm = q.iter().map(|v| v * v).sum::<f64>().sqrt();
                    let adjustment = (norm - 1.).abs();
                    max_adjustment = max_adjustment.max(adjustment);
                    if !norm.is_finite() || norm < 1e-8 || adjustment > 0.001 {
                        return Err(unsupported(format!(
                            "bone {index} quaternion is outside supported quantization tolerance"
                        )));
                    }
                    let rotation =
                        if options.convention == PoseConvention::StudioLoadedBindBlender43 {
                            if index == 0 {
                                q = [0., 0., 0., 1.];
                            }
                            Matrix4::studio_rotation(bind.local[index], q)?
                        } else {
                            for value in &mut q {
                                *value /= norm;
                            }
                            Matrix4::rotation_unchecked(q)
                        };
                    locals[index].replace_rotation(rotation);
                }
                3 if values.len() == 3 => {
                    let scale = [values[0] as f64, values[1] as f64, values[2] as f64];
                    if scale.iter().any(|v| *v < 1e-8 || *v > 1e4) {
                        return Err(unsupported("zero, negative or excessive animated scale"));
                    }
                    scales[index] = scale;
                }
                _ => {
                    return Err(unsupported(format!(
                        "unsupported channel component {} with {} values",
                        channel.sub_target,
                        values.len()
                    )));
                }
            }
        }
        if seen.is_empty() {
            return Err(unsupported("clip has no usable bone channels"));
        }
        // Apply scales after rotations so channel-table order cannot change the pose.
        for (local, scale) in locals.iter_mut().zip(scales) {
            local.replace_scale(scale)?;
        }
        let basis = options.coordinate_basis.matrix();
        let reverse = basis.inverse_affine()?;
        let mut global = vec![Matrix4::IDENTITY; rig.bones.len()];
        for index in order {
            let parent = rig.bones[index].parent_index;
            global[index] = if parent < 0 {
                locals[index]
            } else {
                global[parent as usize].multiply(locals[index])
            };
            global[index].validate_affine()?;
        }
        let matrices = global
            .iter()
            .zip(&bind.inverse)
            .map(|(posed, inverse)| basis.multiply(posed.multiply(*inverse)).multiply(reverse))
            .collect::<Vec<_>>();
        let contains_nonrigid_transforms = matrices.iter().any(|matrix| !matrix.is_rigid());
        let palette = matrices
            .iter()
            .map(|matrix| matrix.to_cols_f32())
            .collect::<Result<Vec<_>>>()?;
        let mut limitations = vec![
            "source-tool constant-component sampling; game interpolation unverified",
            "game-selected rig and native pose fidelity unverified",
            "events and IK preserved but not executed",
            "initial object transform retained but not applied",
        ];
        if codec.layout == FrameLayout::StudioExplicit {
            limitations.push("caller-selected disputed Studio frame layout");
        }
        if max_adjustment > 0. {
            limitations.push("near-unit quantized quaternion conversion applied");
        }
        if !skipped_channels.is_empty() {
            limitations.push("partial source-tool pose preview: empty/non-bone tracks retained but not evaluated");
        }
        if contains_nonrigid_transforms {
            limitations
                .push("scaled skin matrices require inverse-transpose normal transformation");
        }
        Ok(SampledPose {
            palette,
            global,
            tick,
            edited_channels: seen.len(),
            ignored_events: meta.events.len(),
            ignored_ik_slots: meta.ik_slots.len(),
            skipped_channels,
            max_quaternion_norm_adjustment: max_adjustment,
            contains_nonrigid_transforms,
            limitations,
        })
    }
}

fn decode_channel(
    channel: &mut Channel,
    bytes: &[u8],
    palette: &[f32],
    layout: FrameLayout,
) -> Result<()> {
    let type_id = channel.type_id;
    let kind = if layout == FrameLayout::StudioExplicit {
        type_id & 7
    } else {
        match type_id {
            18 => 2,
            20 => 4,
            _ => 0,
        }
    };
    let payload = if layout == FrameLayout::StudioExplicit {
        match kind {
            2 => Some(4),
            3 => Some(6),
            4 => Some(8),
            5 => Some(2),
            _ => None,
        }
    } else {
        match type_id {
            3 | 5 => {
                channel.support = ChannelSupport::AmbiguousLayout;
                return Ok(());
            }
            1..=4 => Some((type_id as usize).div_ceil(2) * 4),
            6..=8 => Some(4),
            9..=17 => Some(0),
            18 | 19 | 21 => Some(4),
            20 => Some(8),
            _ => None,
        }
    };
    let Some(payload) = payload else {
        channel.support = ChannelSupport::UnsupportedEncoding;
        return Ok(());
    };
    let stride = 4 + payload;
    let begin = channel.data_offset as usize;
    let mut r = Reader::new(bytes);
    r.at = begin;
    r.take(channel.frame_count as usize * stride)?;
    r.at = begin;
    let mut failed = false;
    for _ in 0..channel.frame_count {
        let tick = r.u16()?;
        let flags = r.u16()?;
        let encoded = r.take(payload)?.to_vec();
        let mut frame = Frame {
            tick,
            flags,
            encoded,
            values: None,
            unsupported_reason: None,
        };
        if ![2, 3, 4, 5].contains(&kind) {
            frame.unsupported_reason = Some("unsupported value encoding");
        } else {
            decode_frame(&mut frame, kind, channel.offset, channel.scale, palette);
        }
        failed |= frame.values.is_none();
        channel.frames.push(frame);
    }
    channel.support = if channel.frames.is_empty() {
        ChannelSupport::Empty
    } else if channel
        .frames
        .windows(2)
        .any(|frames| frames[0].tick >= frames[1].tick)
    {
        ChannelSupport::InvalidFrameOrder
    } else if failed {
        ChannelSupport::PartialValues
    } else {
        ChannelSupport::Decoded
    };
    Ok(())
}
fn decode_frame(frame: &mut Frame, kind: u8, offset: f32, scale: f32, palette: &[f32]) {
    let count = match kind {
        2 | 3 => 3,
        4 => 4,
        5 => 1,
        _ => return,
    };
    if (count == 3 && frame.flags & 8 != 0) || (count == 1 && frame.flags & 14 != 0) {
        frame.unsupported_reason = Some("unsupported frame sign bits");
        return;
    }
    let mut raw = Vec::with_capacity(count);
    let divisor;
    match kind {
        2 => {
            let packed = u32::from_le_bytes(frame.encoded[..4].try_into().unwrap());
            if packed >> 30 != 0 {
                frame.unsupported_reason = Some("reserved packed-vector bits set");
                return;
            }
            raw.extend([
                (packed & 1023) as f64,
                ((packed >> 10) & 1023) as f64,
                ((packed >> 20) & 1023) as f64,
            ]);
            divisor = 1023.;
        }
        3 => {
            for pair in frame.encoded.chunks_exact(2) {
                let index = u16::from_le_bytes(pair.try_into().unwrap()) as usize;
                let Some(value) = palette.get(index) else {
                    frame.unsupported_reason = Some("palette index out of range");
                    return;
                };
                raw.push(*value as f64);
            }
            divisor = 1.;
        }
        4 | 5 => {
            for pair in frame.encoded.chunks_exact(2) {
                let value = u16::from_le_bytes(pair.try_into().unwrap());
                if kind == 4 && value > 4095 {
                    frame.unsupported_reason = Some("quaternion quantization exceeds 12 bits");
                    return;
                }
                raw.push(value as f64);
            }
            divisor = if kind == 4 { 4095. } else { 65535. };
        }
        _ => return,
    }
    let values = raw
        .iter()
        .enumerate()
        .map(|(component, value)| {
            (offset as f64
                + scale as f64
                    * (if frame.flags & (1 << component) != 0 {
                        -1.
                    } else {
                        1.
                    })
                    * value
                    / divisor) as f32
        })
        .collect::<Vec<_>>();
    if values.iter().any(|v| !v.is_finite()) {
        frame.unsupported_reason = Some("decoded value exceeds float32 range");
        return;
    }
    frame.values = Some(values);
}
