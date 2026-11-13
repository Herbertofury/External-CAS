use super::*;
use external_cas_shader::SamplerRecord;

/// Explicit native renderer inputs. The source producer/setting names for
/// knob A/B and the anisotropy floor still need independent qualification.
#[derive(Clone, Copy, Debug)]
pub struct NativeSamplerContext {
    pub knob_a: bool,
    pub knob_b: bool,
    pub anisotropy_floor: u32,
}
/// Functional port of the installed native descriptor translator. Unknown flag
/// bits remain in the source record; this does not select context settings.
pub fn native_sampler_description(
    record: &SamplerRecord,
    context: NativeSamplerContext,
) -> D3D11_SAMPLER_DESC {
    let [border, bias, flags] = record.raw_words;
    let address = |shift: u32| {
        let code = (flags >> shift) & 7u32;
        D3D11_TEXTURE_ADDRESS_MODE(if (1..=4).contains(&code) {
            code as i32 + 1
        } else {
            1
        })
    };
    let mut field9 = (flags >> 9) & 7;
    let field12 = (flags >> 12) & 7;
    let mut field15 = (flags >> 15) & 7;
    if context.knob_a && flags & (1 << 30) != 0 {
        field15 = 1;
    }
    if context.knob_b && flags & (1 << 31) != 0 {
        field9 = 2;
    }
    let mut filter = if [field9, field12, field15].contains(&2) {
        0x55
    } else {
        (if field12 != 0 { 4 } else { 0 })
            | (if field9 != 0 { 16 } else { 0 })
            | (if field15 != 0 { 1 } else { 0 })
    };
    if flags & (1 << 22) != 0 {
        filter |= 0x80;
    }
    let comparison = (flags >> 18) & 15;
    D3D11_SAMPLER_DESC {
        Filter: D3D11_FILTER(filter),
        AddressU: address(0),
        AddressV: address(3),
        AddressW: address(6),
        MipLODBias: f32::from_bits(bias),
        MaxAnisotropy: (1u32 << ((flags >> 23) & 7)).max(context.anisotropy_floor),
        ComparisonFunc: D3D11_COMPARISON_FUNC(if comparison <= 6 {
            comparison as i32 + 1
        } else {
            8
        }),
        BorderColor: std::array::from_fn(|i| {
            ((border >> (i * 8)) & 255) as f32 * f32::from_bits(0x3b808081)
        }),
        MinLOD: 0.,
        MaxLOD: f32::MAX,
    }
}
impl OriginalShaderBridge {
    pub fn create_source_sampler(
        &self,
        record: &SamplerRecord,
        context: NativeSamplerContext,
    ) -> Result<OriginalSampler> {
        self.create_sampler(&native_sampler_description(record, context))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Requires explicit private isolated-original sampler descriptor receipt"]
    fn descriptor_matches_original_native_producer_bits() {
        let path = std::env::var_os("EXTERNAL_CAS_SAMPLER_RECEIPT")
            .expect("Explicit sampler receipt required");
        assert!(std::fs::metadata(&path).unwrap().len() <= 8 * 1024 * 1024);
        let receipt: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(
            receipt["client_sha256"],
            "95e40b75f76711c66b9f36a974ec04d43304a6f591d409d3d4019818f366237b"
        );
        assert_eq!(receipt["game_contact"], false);
        assert_eq!(receipt["native_io"], false);
        let cases = receipt["cases"].as_array().unwrap();
        assert_eq!(
            receipt["intercepted_calls"].as_u64().unwrap(),
            cases.len() as u64
        );
        for (index, case) in cases.iter().enumerate() {
            let raw_words = std::array::from_fn(|i| {
                u32::try_from(case["raw_words"][i].as_u64().unwrap()).unwrap()
            });
            let record = SamplerRecord {
                ordinal: 0,
                source_file_sha256: String::new(),
                raw_words,
            };
            let d = native_sampler_description(
                &record,
                NativeSamplerContext {
                    knob_a: case["knob_a"].as_bool().unwrap(),
                    knob_b: case["knob_b"].as_bool().unwrap(),
                    anisotropy_floor: u32::try_from(case["anisotropy_floor"].as_u64().unwrap())
                        .unwrap(),
                },
            );
            let words = [
                d.Filter.0 as u32,
                d.AddressU.0 as u32,
                d.AddressV.0 as u32,
                d.AddressW.0 as u32,
                d.MipLODBias.to_bits(),
                d.MaxAnisotropy,
                d.ComparisonFunc.0 as u32,
                d.BorderColor[0].to_bits(),
                d.BorderColor[1].to_bits(),
                d.BorderColor[2].to_bits(),
                d.BorderColor[3].to_bits(),
                d.MinLOD.to_bits(),
                d.MaxLOD.to_bits(),
            ];
            let hex: String = words
                .into_iter()
                .flat_map(|w| w.to_le_bytes())
                .map(|b| format!("{b:02x}"))
                .collect();
            assert_eq!(hex, case["descriptor_hex"].as_str().unwrap(), "case{index}");
        }
        eprintln!(
            "Original native sampler producer parity: {} exact52-byte descriptors",
            cases.len()
        );
    }
}
