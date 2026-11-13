//! Source shader feature-program semantics recovered from the installed
//! build1.129.39.1030 wrapper01372260/evaluator013724B0. This evaluates explicit
//! caller lists; it does not discover the live renderer's lists or select a pass.
use crate::{Result, check};
use std::{collections::HashSet, ops::Range, sync::atomic::AtomicBool};

#[derive(Clone, Copy, Debug)]
pub struct FeatureLimits {
    pub program_words: usize,
    pub input_values: usize,
    pub instructions: usize,
}
impl Default for FeatureLimits {
    fn default() -> Self {
        Self {
            program_words: 65_536,
            input_values: 65_536,
            instructions: 65_536,
        }
    }
}
/// These are explicit engine-domain values, not guessed texture flags. The
/// wildcard corresponds only to the source caller flag plus an EMPTY ID list.
#[derive(Clone, Copy, Debug)]
pub struct FeatureInputs<'a> {
    pub renderer_ids: &'a [u32],
    pub material_hashes: &'a [u32],
    pub renderer_wildcard_if_empty: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct FeatureEvaluation {
    pub mask: u32,
    pub field_values: [u32; 32],
    pub instructions: usize,
    pub remaining_boolean_depth: usize,
}
pub struct FeatureProgram<'a> {
    words: &'a [u32],
    renderer: Range<usize>,
    material: Range<usize>,
    code: usize,
    limits: FeatureLimits,
}
impl<'a> FeatureProgram<'a> {
    /// Original words stay borrowed and unchanged, including unknown opcodes.
    /// A header is [renderer-count, renderer-IDs, material-count, hashes, code].
    /// There is no extra instruction-count word within this header.
    pub fn parse(words: &'a [u32], limits: FeatureLimits, cancel: &AtomicBool) -> Result<Self> {
        check(cancel)?;
        if words.len() > limits.program_words {
            return Err("Feature program exceeds word budget".into());
        }
        if words.is_empty() {
            return Ok(Self {
                words,
                renderer: 0..0,
                material: 0..0,
                code: 0,
                limits,
            });
        }
        let renderer_end = 1usize
            .checked_add(words[0] as usize)
            .ok_or("Feature header overflow")?;
        let material_count = *words
            .get(renderer_end)
            .ok_or("Truncated feature renderer header")? as usize;
        let material_start = renderer_end
            .checked_add(1)
            .ok_or("Feature header overflow")?;
        let code = material_start
            .checked_add(material_count)
            .ok_or("Feature header overflow")?;
        if code >= words.len() {
            return Err("Truncated feature material header/code".into());
        }
        Ok(Self {
            words,
            renderer: 1..renderer_end,
            material: material_start..code,
            code,
            limits,
        })
    }
    pub fn raw_words(&self) -> &'a [u32] {
        self.words
    }
    pub fn declared_renderer_ids(&self) -> &'a [u32] {
        &self.words[self.renderer.clone()]
    }
    pub fn declared_material_hashes(&self) -> &'a [u32] {
        &self.words[self.material.clone()]
    }
    pub fn code_words(&self) -> &'a [u32] {
        &self.words[self.code..]
    }

    pub fn evaluate(
        &self,
        inputs: FeatureInputs<'_>,
        cancel: &AtomicBool,
    ) -> Result<FeatureEvaluation> {
        check(cancel)?;
        if inputs
            .renderer_ids
            .len()
            .checked_add(inputs.material_hashes.len())
            .ok_or("Feature input extent overflow")?
            > self.limits.input_values
        {
            return Err("Feature inputs exceed value budget".into());
        }
        let mut renderer = HashSet::new();
        let mut material = HashSet::new();
        for (i, value) in inputs.renderer_ids.iter().enumerate() {
            if i & 255 == 0 {
                check(cancel)?;
            }
            renderer.insert(*value);
        }
        for (i, value) in inputs.material_hashes.iter().enumerate() {
            if i & 255 == 0 {
                check(cancel)?;
            }
            material.insert(*value);
        }
        // The native wrapper rotates a32-bit bit. More than32 declared material
        // hashes therefore intentionally alias membership bits; retain this.
        let mut membership = 0u32;
        for (i, hash) in self.declared_material_hashes().iter().enumerate() {
            if i & 255 == 0 {
                check(cancel)?;
            }
            if material.contains(hash) {
                membership |= 1u32.rotate_left((i & 31) as u32);
            }
        }
        let code = self.code_words();
        let mut fields = [0u32; 32];
        if code.is_empty() {
            return Ok(FeatureEvaluation {
                mask: 0,
                field_values: fields,
                instructions: 0,
                remaining_boolean_depth: 0,
            });
        }
        // Prevent forward branches into the immediate word of set/add. Unknown
        // single-word opcodes remain intact and fail explicitly if reached.
        let mut boundaries = vec![false; code.len()];
        let mut pos = 0;
        while pos < code.len() {
            if pos & 255 == 0 {
                check(cancel)?;
            }
            boundaries[pos] = true;
            pos += if matches!(code[pos] & 0xffff, 9 | 10) {
                2
            } else {
                1
            };
            if pos > code.len() {
                return Err("Truncated feature field immediate".into());
            }
        }
        let mut pc = 0usize;
        let mut bits = 0u32;
        let mut depth = 0usize;
        let mut executed = 0usize;
        loop {
            if executed & 63 == 0 {
                check(cancel)?;
            }
            if executed >= self.limits.instructions {
                return Err("Feature instruction budget exhausted".into());
            }
            let word = *code
                .get(pc)
                .ok_or("Feature program has no bounded terminator")?;
            let at = pc;
            pc += 1;
            executed += 1;
            if word == 0 {
                check(cancel)?;
                return Ok(FeatureEvaluation {
                    mask: fields.iter().fold(0u32, |sum, n| sum.wrapping_add(*n)),
                    field_values: fields,
                    instructions: executed,
                    remaining_boolean_depth: depth,
                });
            }
            let op = word & 0xffff;
            let arg = (word >> 16) as usize;
            match op {
                1 | 2 => {
                    if depth < 2 {
                        return Err(format!("Feature boolean stack underflow at word{at}"));
                    }
                    bits = if op == 1 {
                        (bits >> 1) & (bits | 0xffff_fffe)
                    } else {
                        (bits >> 1) | (bits & 1)
                    };
                    depth -= 1;
                }
                3 => {
                    if depth == 0 {
                        return Err(format!("Feature boolean stack underflow at word{at}"));
                    }
                    bits ^= 1;
                }
                4 | 5 => {
                    if depth == 0 {
                        return Err(format!("Feature boolean stack underflow at word{at}"));
                    }
                    let taken = (bits & 1 != 0) == (op == 5);
                    bits >>= 1;
                    depth -= 1;
                    if taken {
                        pc = pc.checked_add(arg).ok_or("Feature branch overflow")?;
                    }
                }
                6 | 7 => {
                    if depth >= 32 {
                        return Err(format!("Feature boolean stack overflow at word{at}"));
                    }
                    let present = if op == 6 {
                        membership & (1 << (arg & 31)) != 0
                    } else {
                        renderer.contains(&(arg as u32))
                            || inputs.renderer_wildcard_if_empty && renderer.is_empty()
                    };
                    bits = bits.wrapping_mul(2) | u32::from(present);
                    depth += 1;
                }
                9 | 10 => {
                    let field = fields
                        .get_mut(arg)
                        .ok_or("Feature field index exceeds native32-field extent")?;
                    let value = *code.get(pc).ok_or("Truncated feature field immediate")?;
                    pc += 1;
                    *field = if op == 9 {
                        value
                    } else {
                        field.wrapping_add(value)
                    };
                }
                11 => {
                    pc = pc.checked_add(arg).ok_or("Feature branch overflow")?;
                }
                // Source opcode8 takes the same failure branch as unknown codes.
                _ => {
                    return Err(format!(
                        "Source feature program rejected opcode{op} at word{at}"
                    ));
                }
            }
            if pc >= boundaries.len() || !boundaries[pc] {
                return Err("Feature branch/fallthrough outside instruction boundary".into());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn eval(code: &[u32], renderer: &[u32], material: &[u32]) -> Result<FeatureEvaluation> {
        FeatureProgram::parse(code, FeatureLimits::default(), &AtomicBool::new(false))?.evaluate(
            FeatureInputs {
                renderer_ids: renderer,
                material_hashes: material,
                renderer_wildcard_if_empty: false,
            },
            &AtomicBool::new(false),
        )
    }
    #[test]
    fn exact_boolean_material_renderer_fields_wrapping_and_forward_skip() {
        // IF material0 AND renderer7: field0=0xffff_ffff; field1+=2.
        let words = [
            1,
            7,
            1,
            0xabcdef01,
            6,
            (7 << 16) | 7,
            1,
            (4 << 16) | 4,
            9,
            u32::MAX,
            (1 << 16) | 10,
            2,
            0,
        ];
        assert_eq!(eval(&words, &[7], &[0xabcdef01]).unwrap().mask, 1);
        assert_eq!(eval(&words, &[], &[0xabcdef01]).unwrap().mask, 0);
        assert_eq!(eval(&words, &[7], &[]).unwrap().mask, 0);
        assert_eq!(
            eval(&[0, 0, 9, 5, (2 << 16) | 11, 9, 999, 0], &[], &[])
                .unwrap()
                .mask,
            5
        );
    }
    #[test]
    fn malformed_unknown_cancel_and_explicit_budgets_are_refused() {
        for words in [
            vec![99],
            vec![0, 2, 1],
            vec![0, 0, 8, 0],
            vec![0, 0, 1, 0],
            vec![0, 0, (32 << 16) | 9, 1, 0],
            vec![0, 0, 11 | (1 << 16), 9, 42, 0],
            vec![0, 0, 9],
            vec![0, 0, 11 | (65535 << 16), 0],
        ] {
            assert!(eval(&words, &[], &[]).is_err(), "{words:?}");
        }
        assert!(
            FeatureProgram::parse(&[0, 0, 0], FeatureLimits::default(), &AtomicBool::new(true))
                .is_err()
        );
        let limits = FeatureLimits {
            program_words: 3,
            input_values: 0,
            instructions: 0,
        };
        assert!(
            FeatureProgram::parse(&[0, 0, 0], limits, &AtomicBool::new(false))
                .unwrap()
                .evaluate(
                    FeatureInputs {
                        renderer_ids: &[],
                        material_hashes: &[],
                        renderer_wildcard_if_empty: false
                    },
                    &AtomicBool::new(false)
                )
                .is_err()
        );
    }
    #[test]
    fn source_membership_rotation_and_wildcard_are_explicit() {
        let mut words = vec![0, 33];
        words.extend(0..33);
        words.extend([6, (2 << 16) | 4, 9, 77, 0]);
        assert_eq!(eval(&words, &[], &[32]).unwrap().mask, 77);
        let program = [0, 0, (9 << 16) | 7, (2 << 16) | 4, 9, 99, 0];
        let parsed =
            FeatureProgram::parse(&program, FeatureLimits::default(), &AtomicBool::new(false))
                .unwrap();
        let input = FeatureInputs {
            renderer_ids: &[],
            material_hashes: &[],
            renderer_wildcard_if_empty: true,
        };
        assert_eq!(
            parsed
                .evaluate(input, &AtomicBool::new(false))
                .unwrap()
                .mask,
            99
        );
        assert_eq!(
            parsed
                .evaluate(
                    FeatureInputs {
                        renderer_ids: &[1],
                        ..input
                    },
                    &AtomicBool::new(false)
                )
                .unwrap()
                .mask,
            0
        );
    }
    /// Private receipt contains original installed program words. Nothing from
    /// that receipt is copied into the crate or distributed test fixtures.
    #[test]
    #[ignore = "Requires explicit installed PRECOMP and private isolated-original-VM receipt"]
    fn installed_programs_match_isolated_original_machine_vm() {
        #[derive(serde::Deserialize)]
        struct Case {
            name: String,
            program_words: Vec<u32>,
            renderer_ids: Vec<u32>,
            material_hashes: Vec<u32>,
            renderer_wildcard_if_empty: bool,
            mask: u32,
        }
        #[derive(serde::Deserialize)]
        struct Receipt {
            client_sha256: String,
            analysis_section_sha256: String,
            precomp_sha256: String,
            cases: Vec<Case>,
            native_io: bool,
            game_contact: bool,
        }
        let cancel = AtomicBool::new(false);
        let receipt_path = std::env::var_os("EXTERNAL_CAS_FEATURE_VM_RECEIPT")
            .expect("Explicit private VM receipt path required");
        let metadata = std::fs::metadata(&receipt_path).unwrap();
        assert!(metadata.len() <= 32 * 1024 * 1024);
        let receipt: Receipt =
            serde_json::from_slice(&std::fs::read(receipt_path).unwrap()).unwrap();
        assert_eq!(
            receipt.client_sha256,
            "95e40b75f76711c66b9f36a974ec04d43304a6f591d409d3d4019818f366237b"
        );
        assert_eq!(
            receipt.analysis_section_sha256,
            "829377024c3e7970214cd1af1765994364402a28ed96761a7afb568f8e3c98fa"
        );
        assert!(!receipt.native_io && !receipt.game_contact);
        let path = std::path::PathBuf::from(
            std::env::var_os("EXTERNAL_CAS_SHADER_SOURCE")
                .expect("Explicit installed shader path required"),
        );
        let library = crate::ShaderLibrary::open(&path, crate::Limits::default(), &cancel).unwrap();
        assert_eq!(library.pin.sha256, receipt.precomp_sha256);
        let mut installed = HashSet::new();
        for ordinal in 0..library.named_set_count().unwrap() {
            let set = library.named_set(ordinal, &cancel).unwrap();
            assert_eq!(set.source_file_sha256, library.pin.sha256);
            assert!(set.variant_fields.iter().all(|f| f.name_hash_matches));
            installed.insert(set.program_words);
        }
        let mut masks = 0;
        let mut rejected = 0;
        for case in &receipt.cases {
            assert!(
                installed.contains(&case.program_words),
                "Unpinned source program {}",
                case.name
            );
            let actual =
                FeatureProgram::parse(&case.program_words, FeatureLimits::default(), &cancel)
                    .unwrap()
                    .evaluate(
                        FeatureInputs {
                            renderer_ids: &case.renderer_ids,
                            material_hashes: &case.material_hashes,
                            renderer_wildcard_if_empty: case.renderer_wildcard_if_empty,
                        },
                        &cancel,
                    );
            if case.mask == u32::MAX {
                assert!(actual.is_err(), "Expected native failure for {}", case.name);
                rejected += 1;
            } else {
                assert_eq!(actual.unwrap().mask, case.mask, "{}", case.name);
                masks += 1;
            }
        }
        eprintln!(
            "Original installed feature VM parity: {} cases; {masks} masks; {rejected} explicit refusals; {} unique source programs",
            receipt.cases.len(),
            installed.len()
        );
    }
}
