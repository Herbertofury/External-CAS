// SPDX-License-Identifier: GPL-3.0-or-later
//! Static source-symbol timeline adapter. No ActionScript executes. The initial
//! renderer supports source bitmap-filled rectangles used by CAS navigation;
//! visible masks, strokes, filters, gradients or nonrectangular paths fail closed.
use crate::{Bits, GfxMovie, Reader, Tag};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct Matrix {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub tx: f64,
    pub ty: f64,
}
impl Matrix {
    pub const IDENTITY: Self = Self {
        a: 1.,
        b: 0.,
        c: 0.,
        d: 1.,
        tx: 0.,
        ty: 0.,
    };
    pub fn transform(self, p: [f64; 2]) -> [f64; 2] {
        [
            self.a * p[0] + self.c * p[1] + self.tx,
            self.b * p[0] + self.d * p[1] + self.ty,
        ]
    }
    pub fn then(self, child: Self) -> Self {
        Self {
            a: self.a * child.a + self.c * child.b,
            b: self.b * child.a + self.d * child.b,
            c: self.a * child.c + self.c * child.d,
            d: self.b * child.c + self.d * child.d,
            tx: self.a * child.tx + self.c * child.ty + self.tx,
            ty: self.b * child.tx + self.d * child.ty + self.ty,
        }
    }
    pub fn inverse(self) -> Result<Self, String> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-12 {
            return Err("singular source transform".into());
        }
        Ok(Self {
            a: self.d / det,
            b: -self.b / det,
            c: -self.c / det,
            d: self.a / det,
            tx: (self.c * self.ty - self.d * self.tx) / det,
            ty: (self.b * self.tx - self.a * self.ty) / det,
        })
    }
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct ColorTransform {
    pub multiplier: [f64; 4],
    pub add: [f64; 4],
}
impl ColorTransform {
    pub const IDENTITY: Self = Self {
        multiplier: [1.; 4],
        add: [0.; 4],
    };
    pub fn then(self, child: Self) -> Self {
        let mut result = Self::IDENTITY;
        for i in 0..4 {
            result.multiplier[i] = self.multiplier[i] * child.multiplier[i];
            result.add[i] = self.multiplier[i] * child.add[i] + self.add[i];
        }
        result
    }
    pub fn apply(self, rgba: [u8; 4]) -> [f64; 4] {
        std::array::from_fn(|i| {
            (f64::from(rgba[i]) * self.multiplier[i] + self.add[i]).clamp(0., 255.) / 255.
        })
    }
    fn invisible(self) -> bool {
        self.multiplier[3] == 0. && self.add[3] <= 0.
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct BitmapLayer {
    pub shape_id: u16,
    /// xmin, ymin, xmax, ymax in source twips.
    pub rect: [f64; 4],
    pub bitmap_id: u16,
    /// Source bitmap pixels -> shape twips.
    pub bitmap_matrix: Matrix,
    /// Shape twips -> root symbol twips, preserving mirroring/skew.
    pub matrix: Matrix,
    pub color: ColorTransform,
    pub smoothed: bool,
    pub repeating: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct SymbolFrame {
    pub symbol: String,
    pub root_id: u16,
    pub frame_index: u16,
    pub selected_label: Option<String>,
    pub layers: Vec<BitmapLayer>,
    pub preserved_script_tags: usize,
    pub skipped_transparent_objects: usize,
    pub static_only: bool,
}
impl SymbolFrame {
    pub fn bounds(&self) -> Result<[f64; 4], String> {
        let mut bounds = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for layer in &self.layers {
            for p in [
                [layer.rect[0], layer.rect[1]],
                [layer.rect[2], layer.rect[1]],
                [layer.rect[0], layer.rect[3]],
                [layer.rect[2], layer.rect[3]],
            ] {
                let p = layer.matrix.transform(p);
                bounds[0] = bounds[0].min(p[0]);
                bounds[1] = bounds[1].min(p[1]);
                bounds[2] = bounds[2].max(p[0]);
                bounds[3] = bounds[3].max(p[1]);
            }
        }
        if !bounds.iter().all(|x| x.is_finite()) {
            return Err("source symbol has no supported visible bitmap layers".into());
        }
        Ok(bounds)
    }
}

pub struct SymbolLibrary<'a> {
    movie: &'a GfxMovie<'a>,
    characters: BTreeMap<u16, &'a Tag>,
}
impl<'a> SymbolLibrary<'a> {
    pub fn new(movie: &'a GfxMovie<'a>) -> Result<Self, String> {
        let mut characters = BTreeMap::new();
        for tag in &movie.tags {
            if matches!(tag.code, 2 | 22 | 32 | 83 | 39 | 1008) {
                let mut r = Reader::new(movie.payload(tag), tag.payload.start);
                let id = r.u16().map_err(|e| e.to_string())?;
                if characters.insert(id, tag).is_some() {
                    return Err(format!("duplicate source character {id}"));
                }
            }
        }
        Ok(Self { movie, characters })
    }
    pub fn labels(&self, symbol: &str) -> Result<Vec<(String, u16)>, String> {
        let id = self.resolve(symbol)?;
        let tag = self
            .characters
            .get(&id)
            .ok_or_else(|| format!("source symbol {symbol} is externally assigned"))?;
        if tag.code != 39 {
            return Ok(Vec::new());
        }
        let data = self.movie.payload(tag);
        let mut r = Reader::new(data, tag.payload.start);
        r.take(4).map_err(|e| e.to_string())?;
        let mut frame = 0;
        let mut labels = Vec::new();
        while r.remaining() > 0 {
            let (code, mut body) = next_tag(&mut r)?;
            match code {
                0 => break,
                1 => frame += 1,
                43 => labels.push((body.c_string().map_err(|e| e.to_string())?, frame)),
                _ => {}
            }
        }
        Ok(labels)
    }
    pub fn frame(
        &self,
        symbol: &str,
        label: Option<&str>,
        frame_index: u16,
    ) -> Result<SymbolFrame, String> {
        let id = self.resolve(symbol)?;
        let frame_index = if let Some(label) = label {
            self.labels(symbol)?
                .iter()
                .find(|(name, _)| name == label)
                .map(|(_, frame)| *frame)
                .ok_or_else(|| format!("source symbol {symbol} has no state {label}"))?
        } else {
            frame_index
        };
        let mut result = SymbolFrame {
            symbol: symbol.into(),
            root_id: id,
            frame_index,
            selected_label: label.map(str::to_owned),
            layers: Vec::new(),
            preserved_script_tags: 0,
            skipped_transparent_objects: 0,
            static_only: true,
        };
        self.collect(
            id,
            frame_index,
            Matrix::IDENTITY,
            ColorTransform::IDENTITY,
            &mut BTreeSet::new(),
            &mut result,
        )?;
        Ok(result)
    }
    fn resolve(&self, symbol: &str) -> Result<u16, String> {
        let ids: BTreeSet<_> = self
            .movie
            .exports
            .iter()
            .chain(&self.movie.symbol_classes)
            .filter(|item| item.name == symbol)
            .map(|item| item.id)
            .collect();
        if ids.len() != 1 {
            return Err(format!(
                "source symbol {symbol} has {} matching ids",
                ids.len()
            ));
        }
        Ok(*ids.first().unwrap())
    }
    fn collect(
        &self,
        id: u16,
        frame: u16,
        matrix: Matrix,
        color: ColorTransform,
        stack: &mut BTreeSet<u16>,
        out: &mut SymbolFrame,
    ) -> Result<(), String> {
        if color.invisible() {
            out.skipped_transparent_objects += 1;
            return Ok(());
        }
        if stack.len() >= 64 || !stack.insert(id) {
            return Err(format!("cyclic/deep source symbol at {id}"));
        }
        let tag = self.characters.get(&id).ok_or_else(|| {
            format!("source character {id} requires an external/runtime resource")
        })?;
        if tag.code == 39 {
            let snapshot = sprite_snapshot(self.movie.payload(tag), tag.payload.start, frame)?;
            out.preserved_script_tags += snapshot.scripts;
            for object in snapshot.objects.values() {
                if let Some(reason) = &object.unsupported {
                    return Err(format!(
                        "source character {id} depth {}: {reason}",
                        object.depth
                    ));
                }
                self.collect(
                    object.character,
                    0,
                    matrix.then(object.matrix),
                    color.then(object.color),
                    stack,
                    out,
                )?;
            }
        } else if matches!(tag.code, 2 | 22 | 32 | 83) {
            match bitmap_quad(self.movie.payload(tag), tag.payload.start, tag.code)? {
                Some(quad) => {
                    if out.layers.len() >= 4096 {
                        return Err("source symbol layer budget exceeded".into());
                    }
                    out.layers.push(BitmapLayer {
                        shape_id: id,
                        rect: quad.rect,
                        bitmap_id: quad.bitmap,
                        bitmap_matrix: quad.matrix,
                        matrix,
                        color,
                        smoothed: quad.smoothed,
                        repeating: quad.repeating,
                    });
                }
                None => {
                    if color.add[3] > 0. {
                        return Err("source alpha addition makes a solid fill visible; vector adapter required".into());
                    }
                    out.skipped_transparent_objects += 1;
                }
            }
        } else {
            return Err(format!(
                "source character {id} tag {} cannot be drawn as a symbol",
                tag.code
            ));
        }
        stack.remove(&id);
        Ok(())
    }
}

#[derive(Clone)]
struct Object {
    depth: u16,
    character: u16,
    matrix: Matrix,
    color: ColorTransform,
    unsupported: Option<String>,
}
struct Snapshot {
    objects: BTreeMap<u16, Object>,
    scripts: usize,
}
fn sprite_snapshot(data: &[u8], base: usize, target: u16) -> Result<Snapshot, String> {
    let mut r = Reader::new(data, base);
    let _id = r.u16().map_err(|e| e.to_string())?;
    let count = r.u16().map_err(|e| e.to_string())?;
    if target >= count {
        return Err(format!("source sprite frame {target} outside {count}"));
    }
    let mut frame = 0;
    let mut out = Snapshot {
        objects: BTreeMap::new(),
        scripts: 0,
    };
    while r.remaining() > 0 {
        let (code, mut body) = next_tag(&mut r)?;
        match code {
            0 => return Err("source sprite ends before requested ShowFrame".into()),
            1 => {
                if frame == target {
                    return Ok(out);
                }
                frame += 1;
            }
            26 | 70 => {
                let flags = body.u8().map_err(|e| e.to_string())?;
                let flags2 = if code == 70 {
                    body.u8().map_err(|e| e.to_string())?
                } else {
                    0
                };
                let depth = body.u16().map_err(|e| e.to_string())?;
                if flags2 & 0x18 != 0 {
                    return Err("PlaceObject3 class/image binding is unsupported".into());
                }
                let character = if flags & 2 != 0 {
                    Some(body.u16().map_err(|e| e.to_string())?)
                } else {
                    None
                };
                let transform = if flags & 4 != 0 {
                    Some(read_matrix(&mut body)?)
                } else {
                    None
                };
                let color = if flags & 8 != 0 {
                    Some(read_color(&mut body)?)
                } else {
                    None
                };
                if flags & 16 != 0 {
                    body.u16().map_err(|e| e.to_string())?;
                }
                if flags & 32 != 0 {
                    body.c_string().map_err(|e| e.to_string())?;
                }
                let mut unsupported = None;
                if flags & 64 != 0 {
                    body.u16().map_err(|e| e.to_string())?;
                    unsupported = Some("clip-depth mask is unsupported".into());
                }
                if flags2 & 1 != 0 {
                    unsupported = Some("source filter list is unsupported".into());
                } else {
                    if flags2 & 2 != 0 {
                        let blend = body.u8().map_err(|e| e.to_string())?;
                        if blend > 1 {
                            unsupported = Some(format!("source blend mode {blend} is unsupported"));
                        }
                    }
                    if flags2 & 4 != 0 {
                        body.u8().map_err(|e| e.to_string())?;
                    }
                }
                if flags & 128 != 0 {
                    out.scripts += 1;
                }
                let moving = flags & 1 != 0;
                if !moving && character.is_none() {
                    return Err("source placement has neither move nor character".into());
                }
                let mut object = if moving {
                    out.objects
                        .get(&depth)
                        .cloned()
                        .ok_or_else(|| format!("source move has no object at depth {depth}"))?
                } else {
                    Object {
                        depth,
                        character: character.unwrap(),
                        matrix: Matrix::IDENTITY,
                        color: ColorTransform::IDENTITY,
                        unsupported: None,
                    }
                };
                if let Some(character) = character {
                    object.character = character;
                }
                if let Some(matrix) = transform {
                    object.matrix = matrix;
                }
                if let Some(color) = color {
                    object.color = color;
                }
                if unsupported.is_some() {
                    object.unsupported = unsupported;
                }
                out.objects.insert(depth, object);
            }
            28 => {
                let depth = body.u16().map_err(|e| e.to_string())?;
                out.objects.remove(&depth);
            }
            12 | 59 | 82 => out.scripts += 1,
            43 | 86 => {} // labels and scene metadata are inspected separately
            _ => {
                return Err(format!(
                    "source timeline tag {code} is preserved but unsupported"
                ))
            }
        }
    }
    Err("truncated source sprite timeline".into())
}

fn next_tag<'a>(r: &mut Reader<'a>) -> Result<(u16, Reader<'a>), String> {
    let head = r.u16().map_err(|e| e.to_string())?;
    let short = (head & 63) as usize;
    let count = if short == 63 {
        r.u32().map_err(|e| e.to_string())? as usize
    } else {
        short
    };
    let base = r.absolute();
    let data = r.take(count).map_err(|e| e.to_string())?;
    Ok((head >> 6, Reader::new(data, base)))
}
fn read_matrix(r: &mut Reader<'_>) -> Result<Matrix, String> {
    let mut bits = Bits {
        raw: r.data,
        at: r.at * 8,
    };
    let mut m = Matrix::IDENTITY;
    if bits.unsigned(1).map_err(|e| e.to_string())? != 0 {
        let n = bits.unsigned(5).map_err(|e| e.to_string())? as u8;
        if n == 0 {
            return Err("zero source scale bit width".into());
        }
        m.a = f64::from(bits.signed(n).map_err(|e| e.to_string())?) / 65536.;
        m.d = f64::from(bits.signed(n).map_err(|e| e.to_string())?) / 65536.;
    }
    if bits.unsigned(1).map_err(|e| e.to_string())? != 0 {
        let n = bits.unsigned(5).map_err(|e| e.to_string())? as u8;
        if n == 0 {
            return Err("zero source rotate bit width".into());
        }
        m.b = f64::from(bits.signed(n).map_err(|e| e.to_string())?) / 65536.;
        m.c = f64::from(bits.signed(n).map_err(|e| e.to_string())?) / 65536.;
    }
    let n = bits.unsigned(5).map_err(|e| e.to_string())? as u8;
    if n > 0 {
        m.tx = f64::from(bits.signed(n).map_err(|e| e.to_string())?);
        m.ty = f64::from(bits.signed(n).map_err(|e| e.to_string())?);
    }
    r.at = bits.at.div_ceil(8);
    Ok(m)
}
fn read_color(r: &mut Reader<'_>) -> Result<ColorTransform, String> {
    let mut bits = Bits {
        raw: r.data,
        at: r.at * 8,
    };
    let add = bits.unsigned(1).map_err(|e| e.to_string())? != 0;
    let mult = bits.unsigned(1).map_err(|e| e.to_string())? != 0;
    let n = bits.unsigned(4).map_err(|e| e.to_string())? as u8;
    let mut color = ColorTransform::IDENTITY;
    if n == 0 && (add || mult) {
        return Err("zero source color bit width".into());
    }
    if mult {
        for v in &mut color.multiplier {
            *v = f64::from(bits.signed(n).map_err(|e| e.to_string())?) / 256.;
        }
    }
    if add {
        for v in &mut color.add {
            *v = f64::from(bits.signed(n).map_err(|e| e.to_string())?);
        }
    }
    r.at = bits.at.div_ceil(8);
    Ok(color)
}
fn read_rect(r: &mut Reader<'_>) -> Result<[i32; 4], String> {
    let mut bits = Bits {
        raw: r.data,
        at: r.at * 8,
    };
    let n = bits.unsigned(5).map_err(|e| e.to_string())? as u8;
    if n == 0 {
        return Err("zero source rectangle width".into());
    }
    let p = [
        bits.signed(n),
        bits.signed(n),
        bits.signed(n),
        bits.signed(n),
    ];
    r.at = bits.at.div_ceil(8);
    let mut v = [0; 4];
    for (i, item) in p.into_iter().enumerate() {
        v[i] = item.map_err(|e| e.to_string())?;
    }
    Ok([v[0], v[2], v[1], v[3]])
}
enum Fill {
    Bitmap {
        bitmap: u16,
        matrix: Matrix,
        kind: u8,
    },
    Solid([u8; 4]),
    Unsupported,
}
struct Quad {
    rect: [f64; 4],
    bitmap: u16,
    matrix: Matrix,
    smoothed: bool,
    repeating: bool,
}
fn bitmap_quad(data: &[u8], base: usize, tag: u16) -> Result<Option<Quad>, String> {
    let mut r = Reader::new(data, base);
    r.u16().map_err(|e| e.to_string())?;
    let _bounds = read_rect(&mut r)?;
    let version = match tag {
        2 => 1,
        22 => 2,
        32 => 3,
        83 => 4,
        _ => return Err("not a source shape".into()),
    };
    if version == 4 {
        read_rect(&mut r)?;
        r.u8().map_err(|e| e.to_string())?;
    }
    let mut count = r.u8().map_err(|e| e.to_string())? as usize;
    if count == 255 && version >= 2 {
        count = r.u16().map_err(|e| e.to_string())? as usize;
    }
    if count > r.remaining() / 4 {
        return Err("source fill count exceeds shape".into());
    }
    let mut fills = Vec::new();
    for _ in 0..count {
        let kind = r.u8().map_err(|e| e.to_string())?;
        let fill = match kind {
            0 => {
                let rgb = r
                    .take(if version >= 3 { 4 } else { 3 })
                    .map_err(|e| e.to_string())?;
                Fill::Solid([
                    rgb[0],
                    rgb[1],
                    rgb[2],
                    if version >= 3 { rgb[3] } else { 255 },
                ])
            }
            0x40..=0x43 => Fill::Bitmap {
                bitmap: r.u16().map_err(|e| e.to_string())?,
                matrix: read_matrix(&mut r)?,
                kind,
            },
            0x10 | 0x12 | 0x13 => {
                read_matrix(&mut r)?;
                let packed = r.u8().map_err(|e| e.to_string())?;
                let count = (packed & 15) as usize;
                r.take(count * (if version >= 3 { 5 } else { 4 }))
                    .map_err(|e| e.to_string())?;
                if kind == 0x13 {
                    r.u16().map_err(|e| e.to_string())?;
                }
                Fill::Unsupported
            }
            _ => return Err(format!("source fill type {kind} is unsupported")),
        };
        fills.push(fill);
    }
    let mut lines = r.u8().map_err(|e| e.to_string())? as usize;
    if lines == 255 && version >= 2 {
        lines = r.u16().map_err(|e| e.to_string())? as usize;
    }
    if lines != 0 {
        return Err("visible source stroked shapes are unsupported".into());
    }
    let mut bits = Bits {
        raw: r.data,
        at: r.at * 8,
    };
    let fill_bits = bits.unsigned(4).map_err(|e| e.to_string())? as u8;
    let line_bits = bits.unsigned(4).map_err(|e| e.to_string())? as u8;
    let mut pos = [0i32; 2];
    let mut path = Vec::new();
    let mut fill0 = 0;
    let mut fill1 = 0;
    let mut used = BTreeSet::new();
    loop {
        if bits.unsigned(1).map_err(|e| e.to_string())? != 0 {
            let straight = bits.unsigned(1).map_err(|e| e.to_string())? != 0;
            let n = bits.unsigned(4).map_err(|e| e.to_string())? as u8 + 2;
            if !straight {
                return Err(
                    "visible nonrectangular source curves require the vector adapter".into(),
                );
            }
            let mut delta = [0; 2];
            if bits.unsigned(1).map_err(|e| e.to_string())? != 0 {
                delta = [
                    bits.signed(n).map_err(|e| e.to_string())?,
                    bits.signed(n).map_err(|e| e.to_string())?,
                ];
            } else {
                let vertical = bits.unsigned(1).map_err(|e| e.to_string())? != 0;
                delta[usize::from(vertical)] = bits.signed(n).map_err(|e| e.to_string())?;
            }
            if path.is_empty() {
                path.push(pos);
            }
            for i in 0..2 {
                pos[i] = pos[i]
                    .checked_add(delta[i])
                    .ok_or("source path coordinate overflow")?;
            }
            path.push(pos);
            if fill0 != 0 {
                used.insert(fill0);
            }
            if fill1 != 0 {
                used.insert(fill1);
            }
            if path.len() > 5 {
                return Err("source bitmap path is not a single rectangle".into());
            }
        } else {
            let flags = bits.unsigned(5).map_err(|e| e.to_string())?;
            if flags == 0 {
                break;
            }
            if flags & 16 != 0 {
                return Err("source shape introduces additional style arrays".into());
            }
            if flags & 1 != 0 {
                if !path.is_empty() {
                    return Err("source shape has multiple contours".into());
                }
                let n = bits.unsigned(5).map_err(|e| e.to_string())? as u8;
                if n > 0 {
                    pos = [
                        bits.signed(n).map_err(|e| e.to_string())?,
                        bits.signed(n).map_err(|e| e.to_string())?,
                    ];
                }
            }
            if flags & 2 != 0 {
                fill0 = bits.unsigned(fill_bits).map_err(|e| e.to_string())?;
            }
            if flags & 4 != 0 {
                fill1 = bits.unsigned(fill_bits).map_err(|e| e.to_string())?;
            }
            if flags & 8 != 0 && bits.unsigned(line_bits).map_err(|e| e.to_string())? != 0 {
                return Err("source shape selects a stroke".into());
            }
        }
    }
    if path.len() != 5 || path.first() != path.last() || used.len() != 1 {
        return Err("source shape is not one closed single-fill bitmap rectangle".into());
    }
    let mut rect = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
    for p in &path[..4] {
        rect[0] = rect[0].min(p[0]);
        rect[1] = rect[1].min(p[1]);
        rect[2] = rect[2].max(p[0]);
        rect[3] = rect[3].max(p[1]);
    }
    let corners: BTreeSet<_> = path[..4].iter().copied().collect();
    let expected = BTreeSet::from([
        [rect[0], rect[1]],
        [rect[2], rect[1]],
        [rect[0], rect[3]],
        [rect[2], rect[3]],
    ]);
    if corners != expected
        || path
            .windows(2)
            .any(|p| p[0][0] != p[1][0] && p[0][1] != p[1][1])
    {
        return Err("source bitmap path does not follow rectangle edges".into());
    }
    let fill = *used.first().unwrap() as usize;
    let selected = fills
        .get(fill.checked_sub(1).ok_or("invalid zero fill")?)
        .ok_or("source fill index outside array")?;
    match selected {
        Fill::Bitmap {
            bitmap,
            matrix,
            kind,
        } => Ok(Some(Quad {
            rect: rect.map(f64::from),
            bitmap: *bitmap,
            matrix: *matrix,
            smoothed: *kind < 0x42,
            repeating: *kind & 1 == 0,
        })),
        Fill::Solid(color) if color[3] == 0 => Ok(None),
        _ => Err("visible source non-bitmap fill requires the vector adapter".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn affine_inverse_and_parent_child_order() {
        let parent = Matrix {
            a: -1.,
            d: 2.,
            tx: 90.,
            ty: 5.,
            ..Matrix::IDENTITY
        };
        let child = Matrix {
            tx: 10.,
            ty: 20.,
            ..Matrix::IDENTITY
        };
        let p = parent.then(child).transform([3., 4.]);
        assert_eq!(p, [77., 53.]);
        let q = parent.then(child).inverse().unwrap().transform(p);
        assert!((q[0] - 3.).abs() < 1e-8 && (q[1] - 4.).abs() < 1e-8);
    }
    #[test]
    fn color_composition_and_literal_alpha() {
        let parent = ColorTransform {
            multiplier: [0.5, 1., 1., 0.5],
            add: [10., 0., 0., 0.],
        };
        let child = ColorTransform {
            multiplier: [0.5, 1., 1., 1.],
            add: [20., 0., 0., 0.],
        };
        let actual = parent.then(child).apply([200, 10, 20, 128]);
        assert!((actual[0] - 70. / 255.).abs() < 1e-8);
        assert!((actual[3] - 64. / 255.).abs() < 1e-8);
    }
    struct Writer {
        bytes: Vec<u8>,
        at: usize,
    }
    impl Writer {
        fn new() -> Self {
            Self {
                bytes: Vec::new(),
                at: 0,
            }
        }
        fn put(&mut self, value: i32, width: u8) {
            for bit in (0..width).rev() {
                if self.at % 8 == 0 {
                    self.bytes.push(0);
                }
                self.bytes[self.at / 8] |= (((value >> bit) & 1) as u8) << (7 - self.at % 8);
                self.at += 1;
            }
        }
    }
    fn matrix(tx: i32) -> Vec<u8> {
        let mut w = Writer::new();
        w.put(0, 1);
        w.put(0, 1);
        w.put(9, 5);
        w.put(tx, 9);
        w.put(0, 9);
        w.bytes
    }
    fn tag(out: &mut Vec<u8>, code: u16, body: &[u8]) {
        out.extend_from_slice(&((code << 6) | 63).to_le_bytes());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(body);
    }
    fn quad() -> Vec<u8> {
        let mut out = 1u16.to_le_bytes().to_vec();
        let mut r = Writer::new();
        r.put(9, 5);
        for value in [0, 200, 0, 100] {
            r.put(value, 9);
        }
        out.extend(r.bytes);
        out.extend_from_slice(&[1, 65, 7, 0]);
        out.extend_from_slice(&matrix(0));
        out.push(0);
        let mut w = Writer::new();
        w.put(1, 4);
        w.put(0, 4);
        w.put(0, 1);
        w.put(5, 5);
        w.put(0, 5);
        w.put(1, 1);
        for (delta, vertical) in [(200, false), (100, true), (-200, false), (-100, true)] {
            w.put(1, 1);
            w.put(1, 1);
            w.put(7, 4);
            w.put(0, 1);
            w.put(i32::from(vertical), 1);
            w.put(delta, 9);
        }
        w.put(0, 1);
        w.put(0, 5);
        out.extend(w.bytes);
        out
    }
    #[test]
    fn actual_rectangle_path_and_each_truncation_are_validated() {
        let bytes = quad();
        let q = bitmap_quad(&bytes, 0, 2).unwrap().unwrap();
        assert_eq!(q.rect, [0., 0., 200., 100.]);
        assert_eq!(q.bitmap, 7);
        for cut in 0..bytes.len() {
            assert!(bitmap_quad(&bytes[..cut], 0, 2).is_err(), "cut={cut}");
        }
        let mut invalid = bytes;
        invalid[0] = 3; // character id may differ without changing source shape meaning
        assert_eq!(
            bitmap_quad(&invalid, 0, 2).unwrap().unwrap().rect,
            [0., 0., 200., 100.]
        );
    }
    #[test]
    fn move_only_inherits_character_and_remove_clears_depth() {
        let mut bytes = 1u16.to_le_bytes().to_vec();
        bytes.extend_from_slice(&2u16.to_le_bytes());
        let mut place = vec![6, 1, 0, 2, 0];
        place.extend(matrix(0));
        tag(&mut bytes, 26, &place);
        tag(&mut bytes, 1, &[]);
        let mut moved = vec![5, 1, 0];
        moved.extend(matrix(-20));
        tag(&mut bytes, 26, &moved);
        tag(&mut bytes, 1, &[]);
        tag(&mut bytes, 0, &[]);
        let state = sprite_snapshot(&bytes, 0, 1).unwrap();
        assert_eq!(state.objects[&1].character, 2);
        assert_eq!(state.objects[&1].matrix.tx, -20.);
        let mut removed = bytes[..bytes.len() - 6].to_vec(); // remove final long-form End tag before appending a third frame
        removed[2..4].copy_from_slice(&3u16.to_le_bytes());
        tag(&mut removed, 28, &1u16.to_le_bytes());
        tag(&mut removed, 1, &[]);
        tag(&mut removed, 0, &[]);
        assert!(sprite_snapshot(&removed, 0, 2).unwrap().objects.is_empty());
    }
    #[test]
    fn move_without_prior_object_and_unknown_timeline_tag_fail() {
        let mut bytes = vec![1, 0, 1, 0];
        tag(&mut bytes, 26, &[1, 1, 0]);
        tag(&mut bytes, 1, &[]);
        assert!(sprite_snapshot(&bytes, 0, 0).is_err());
        let mut bytes = vec![1, 0, 1, 0];
        tag(&mut bytes, 999, &[]);
        tag(&mut bytes, 1, &[]);
        assert!(sprite_snapshot(&bytes, 0, 0).is_err());
    }
}
