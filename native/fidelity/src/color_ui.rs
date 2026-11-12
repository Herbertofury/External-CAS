// SPDX-License-Identifier: GPL-3.0-or-later
//! Typed color UI metadata boundary. It consumes actual selected-swatch CASP
//! ranges and caller-observed provenance/availability; it does not execute or
//! embed proprietary game/mod scripts. Source UI findings are documented in
//! docs/fidelity/COLOR_SLIDERS_SOURCE_20261112.md. Shader parity is separate.
use serde::Serialize;

pub const CUSTOM_SWATCH_LIMIT: usize = 10;
pub const VALUE_COMPARISON_TOLERANCE: f32 = 0.001;
pub const NEUTRAL_HOSV: [f32; 4] = [0., 1., 0., 0.];
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub enum Availability {
    Available,
    Unavailable,
    #[default]
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
pub enum ColorUiMode {
    #[default]
    Diffuse,
    SkinSpecularity,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct SliderRange {
    pub minimum: f32,
    pub maximum: f32,
    pub step: f32,
}
impl SliderRange {
    pub fn valid(self) -> bool {
        self.minimum.is_finite()
            && self.maximum.is_finite()
            && self.step.is_finite()
            && self.maximum > self.minimum
            && self.step > 0.
    }
    pub fn tyjokr_fallback_hosv(index: usize) -> Option<Self> {
        match index {
            0 | 2 | 3 => Some(Self {
                minimum: -0.5,
                maximum: 0.5,
                step: 0.05,
            }),
            1 => Some(Self {
                minimum: 0.2,
                maximum: 1.,
                step: 0.05,
            }),
            _ => None,
        }
    }
    pub fn contains(self, value: f32) -> bool {
        value.is_finite() && value >= self.minimum && value <= self.maximum
    }
}
#[derive(Debug, Clone, Default)]
pub struct SwatchColorInput {
    /// Actual selected CASP ranges, H,O,S,V. None remains explicit unknown.
    pub ranges_hosv: [Option<SliderRange>; 4],
    /// Caller owns the preserved packed modifier. These values are observed UI
    /// inputs only; resolve() never mutates, clamps or saves that packed value.
    pub stored_hosv: [Option<f32>; 4],
    pub modded: Option<bool>,
    /// True only when an actual source channel table lookup has succeeded.
    pub table_known: bool,
    pub availability_hosv: [Availability; 4],
    pub mode: ColorUiMode,
    /// Actual decoded texture observation, never inferred from body type alone.
    pub fully_transparent: Option<bool>,
}
impl SwatchColorInput {
    pub fn from_casp(
        part: &external_cas_assets::CasPart,
        stored_hosv: [f32; 4],
        mode: ColorUiMode,
    ) -> Self {
        let ranges_hosv = part
            .color_sliders
            .as_ref()
            .map(|source| {
                source.ranges_hosv().map(|range| {
                    Some(SliderRange {
                        minimum: range.minimum,
                        maximum: range.maximum,
                        step: range.step,
                    })
                })
            })
            .unwrap_or([None; 4]);
        Self {
            ranges_hosv,
            stored_hosv: stored_hosv.map(Some),
            mode,
            ..Default::default()
        }
    }
    /// The source table's string order is H,S,V,O. Invalid/missing states stay
    /// Unknown; an invalid string must not be represented as a known table hit.
    pub fn set_source_table_hsvo(&mut self, channels: Option<&str>) {
        self.table_known = false;
        self.availability_hosv = [Availability::Unknown; 4];
        let Some(channels) = channels else {
            return;
        };
        if channels.len() != 4 || !channels.bytes().all(|b| matches!(b, b'A' | b'N')) {
            return;
        }
        let bytes = channels.as_bytes();
        let decode = |b| {
            if b == b'A' {
                Availability::Available
            } else {
                Availability::Unavailable
            }
        };
        self.availability_hosv = [
            decode(bytes[0]),
            decode(bytes[3]),
            decode(bytes[1]),
            decode(bytes[2]),
        ];
        self.table_known = true;
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ChannelControl {
    pub index_hosv: usize,
    pub label: &'static str,
    pub source_binding: &'static str,
    /// Effective UI range only. Raw source float bits stay in CasPart.
    pub range: Option<SliderRange>,
    pub fallback_used: bool,
    pub availability: Availability,
    pub enabled_in_reference: bool,
    pub visible_in_reference: bool,
    pub neutral: f32,
    /// Exact observed value bits, including unsupported/nonfinite values.
    pub preserved_value_bits: Option<u32>,
    /// Reference initial control value. It must never replace an unsupported
    /// preserved packed value merely because the UI resolved a fallback.
    pub initial_display_value: f32,
    pub stored_value_outside_range: bool,
    pub source_range_invalid_or_missing: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PreviewBoundary {
    TextureUnknown,
    TransparentTextureNoEffect,
    DiffuseTransformRequiresQualification,
    SpecularMaterialAdapterRequired,
}
#[derive(Debug, Clone, Serialize)]
pub struct ColorUiState {
    pub mode: ColorUiMode,
    pub controls_hosv: [ChannelControl; 4],
    pub custom_swatch_limit: usize,
    pub preview_boundary: PreviewBoundary,
    pub notes: Vec<&'static str>,
}
pub fn resolve(input: &SwatchColorInput) -> ColorUiState {
    let modded = input.modded == Some(true);
    let fallback_allowed = modded || !input.table_known;
    let availability = if modded {
        [Availability::Available; 4]
    } else {
        input.availability_hosv
    };
    let bindings = match input.mode {
        ColorUiMode::Diffuse => ["slHue", "slOpacity", "slSaturation", "slValue"],
        ColorUiMode::SkinSpecularity => [
            "slSpecHue",
            "slSpecOpacity",
            "slSpecSaturation",
            "slSpecGloss",
        ],
    };
    let labels = match input.mode {
        ColorUiMode::Diffuse => ["Hue", "Opacity", "Saturation", "Brightness"],
        ColorUiMode::SkinSpecularity => ["Hue", "Opacity", "Saturation", "Gloss"],
    };
    let controls_hosv = std::array::from_fn(|index| {
        // A zero maximum is the source UI's disabled sentinel, even when a
        // mathematical interval such as [-0.5, 0] would otherwise be valid.
        let source = input.ranges_hosv[index].filter(|range| {
            range.valid()
                && range.maximum != 0.
                && (index != 1 || (range.minimum >= 0. && range.maximum <= 1.))
        });
        let fallback_used = source.is_none() && fallback_allowed;
        let range = if fallback_used {
            SliderRange::tyjokr_fallback_hosv(index)
        } else {
            source
        };
        let stored = input.stored_hosv[index];
        let contained = stored
            .zip(range)
            .map(|(value, range)| range.contains(value))
            .unwrap_or(false);
        let enabled = range.is_some();
        ChannelControl {
            index_hosv: index,
            label: labels[index],
            source_binding: bindings[index],
            range,
            fallback_used,
            availability: availability[index],
            enabled_in_reference: enabled,
            visible_in_reference: enabled && availability[index] != Availability::Unavailable,
            neutral: NEUTRAL_HOSV[index],
            preserved_value_bits: stored.map(f32::to_bits),
            initial_display_value: if contained {
                stored.unwrap()
            } else {
                NEUTRAL_HOSV[index]
            },
            stored_value_outside_range: stored.is_some() && !contained,
            source_range_invalid_or_missing: source.is_none(),
        }
    });
    let preview_boundary = if input.fully_transparent == Some(true) {
        PreviewBoundary::TransparentTextureNoEffect
    } else if input.mode == ColorUiMode::SkinSpecularity {
        PreviewBoundary::SpecularMaterialAdapterRequired
    } else if input.fully_transparent.is_none() {
        PreviewBoundary::TextureUnknown
    } else {
        PreviewBoundary::DiffuseTransformRequiresQualification
    };
    let mut notes = Vec::new();
    if !input.table_known && !modded {
        notes.push("Per-item source availability table is unknown; fallback is not proof that each channel changes the selected material.");
    }
    if input.mode == ColorUiMode::SkinSpecularity {
        notes.push("The source binds four separate specular-state controls. Gloss must not be silently implemented as diffuse brightness.");
    }
    if input.fully_transparent == Some(true) {
        notes.push("The supplied update skips fully transparent textures; preserved modifiers remain data and may have no visible effect.");
    }
    if controls_hosv
        .iter()
        .any(|control| control.stored_value_outside_range)
    {
        notes.push("A stored modifier is outside the effective UI range; retain its exact value until an explicit edit or reset.");
    }
    ColorUiState {
        mode: input.mode,
        controls_hosv,
        custom_swatch_limit: CUSTOM_SWATCH_LIMIT,
        preview_boundary,
        notes,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CreateSwatchStatus {
    Allowed,
    LimitReached,
    Neutral,
    MatchesOriginal,
    InvalidModifier,
}
#[derive(Debug, Clone, Serialize)]
pub struct CustomSwatchActions {
    pub create: CreateSwatchStatus,
    pub can_create: bool,
    pub can_delete: bool,
    pub selected_custom_status_known: bool,
}
pub fn custom_actions(
    current: [f32; 4],
    original: [f32; 4],
    count: usize,
    selected_is_custom: Option<bool>,
) -> CustomSwatchActions {
    let same = |other: [f32; 4]| {
        current.iter().zip(other).all(|(a, b)| {
            a.is_finite() && b.is_finite() && (*a - b).abs() < VALUE_COMPARISON_TOLERANCE
        })
    };
    let create = if current.iter().any(|value| !value.is_finite()) {
        CreateSwatchStatus::InvalidModifier
    } else if count >= CUSTOM_SWATCH_LIMIT {
        CreateSwatchStatus::LimitReached
    } else if same(NEUTRAL_HOSV) {
        CreateSwatchStatus::Neutral
    } else if same(original) {
        CreateSwatchStatus::MatchesOriginal
    } else {
        CreateSwatchStatus::Allowed
    };
    CustomSwatchActions {
        create,
        can_create: create == CreateSwatchStatus::Allowed,
        can_delete: selected_is_custom == Some(true),
        selected_custom_status_known: selected_is_custom.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_selected_ranges_take_priority_over_fallback() {
        let mut input = SwatchColorInput::default();
        input.ranges_hosv[0] = Some(SliderRange {
            minimum: -0.25,
            maximum: 0.4,
            step: 0.01,
        });
        input.stored_hosv[0] = Some(0.123);
        let state = resolve(&input);
        assert!(!state.controls_hosv[0].fallback_used);
        assert_eq!(state.controls_hosv[0].range, input.ranges_hosv[0]);
        assert_eq!(state.controls_hosv[0].initial_display_value, 0.123);
        assert!(state.controls_hosv[1].fallback_used);
    }
    #[test]
    fn source_table_order_modded_override_and_unknown_are_distinct() {
        let mut input = SwatchColorInput::default();
        input.set_source_table_hsvo(Some("ANAN"));
        assert_eq!(
            input.availability_hosv,
            [
                Availability::Available,
                Availability::Unavailable,
                Availability::Unavailable,
                Availability::Available
            ]
        );
        let state = resolve(&input);
        assert!(state
            .controls_hosv
            .iter()
            .all(|control| control.range.is_none()));
        input.modded = Some(true);
        let state = resolve(&input);
        assert!(state
            .controls_hosv
            .iter()
            .all(|control| control.availability == Availability::Available
                && control.visible_in_reference));
        input.set_source_table_hsvo(Some("AAAAx"));
        assert!(!input.table_known);
    }
    #[test]
    fn zero_maximum_is_source_disabled_sentinel() {
        let mut input = SwatchColorInput::default();
        input.ranges_hosv[0] = Some(SliderRange {
            minimum: -0.5,
            maximum: 0.,
            step: 0.05,
        });
        assert!(resolve(&input).controls_hosv[0].fallback_used);
        input.set_source_table_hsvo(Some("AAAA"));
        assert!(resolve(&input).controls_hosv[0].range.is_none());
    }
    #[test]
    fn invalid_source_and_outside_values_are_retained_without_clamping() {
        let mut input = SwatchColorInput::default();
        input.ranges_hosv[0] = Some(SliderRange {
            minimum: f32::NAN,
            maximum: 1.,
            step: 0.,
        });
        let value = f32::from_bits(0x7fc00123);
        input.stored_hosv[0] = Some(value);
        input.stored_hosv[2] = Some(1.75);
        let state = resolve(&input);
        assert_eq!(
            state.controls_hosv[0].preserved_value_bits,
            Some(0x7fc00123)
        );
        assert!(state.controls_hosv[2].stored_value_outside_range);
        assert_eq!(
            state.controls_hosv[2].preserved_value_bits,
            Some(1.75f32.to_bits())
        );
        assert_eq!(state.controls_hosv[2].initial_display_value, 0.);
    }
    #[test]
    fn specular_state_is_not_diffuse_brightness_and_transparency_is_explicit() {
        let mut input = SwatchColorInput {
            mode: ColorUiMode::SkinSpecularity,
            ..Default::default()
        };
        let state = resolve(&input);
        assert_eq!(state.controls_hosv[3].label, "Gloss");
        assert_eq!(state.controls_hosv[3].source_binding, "slSpecGloss");
        assert_eq!(
            state.preview_boundary,
            PreviewBoundary::SpecularMaterialAdapterRequired
        );
        input.fully_transparent = Some(true);
        assert_eq!(
            resolve(&input).preview_boundary,
            PreviewBoundary::TransparentTextureNoEffect
        );
    }
    #[test]
    fn custom_create_delete_and_comparison_threshold() {
        let original = [0.1, 1., 0., 0.];
        assert_eq!(
            custom_actions(NEUTRAL_HOSV, original, 0, Some(false)).create,
            CreateSwatchStatus::Neutral
        );
        assert_eq!(
            custom_actions(original, original, 0, None).create,
            CreateSwatchStatus::MatchesOriginal
        );
        let changed = [0.2, 1., 0., 0.];
        assert!(custom_actions(changed, original, 9, Some(true)).can_create);
        assert!(custom_actions(changed, original, 9, Some(true)).can_delete);
        assert_eq!(
            custom_actions(changed, original, 10, Some(true)).create,
            CreateSwatchStatus::LimitReached
        );
        assert!(!custom_actions(changed, original, 0, None).selected_custom_status_known);
    }
}
