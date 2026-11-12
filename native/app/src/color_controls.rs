//! Real native preview controls. Per-item/per-Sim contexts retain exact packed
//! values. Saving to native outfit state is a separate verified draft operation.
use eframe::egui;
use external_cas_fidelity::color_ui::{self, SliderRange, SwatchColorInput};
use external_cas_material::color::{Axis, PackedColorShift};
use std::collections::BTreeMap;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ColorDraft {
    pub base: PackedColorShift,
    pub current: PackedColorShift,
    #[serde(default)]
    pub conflict: bool,
    #[serde(default)]
    pub presets: Vec<PackedColorShift>,
    #[serde(default)]
    pub selected_preset: Option<usize>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct ColorEditor {
    pub contexts: BTreeMap<String, ColorDraft>,
    #[serde(skip)]
    pub active: Option<String>,
    #[serde(skip)]
    pub expanded_by_default: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}
impl ColorEditor {
    pub fn activate(&mut self, target: String, base: PackedColorShift) {
        let draft = self.contexts.entry(target.clone()).or_insert(ColorDraft {
            base,
            current: base,
            conflict: false,
            presets: Vec::new(),
            selected_preset: None,
            extra: BTreeMap::new(),
        });
        if draft.base != base {
            if draft.current == draft.base || draft.current == base {
                draft.current = base;
                draft.base = base;
                draft.conflict = false;
            } else {
                draft.conflict = true;
            }
        }
        self.active = Some(target);
    }
    pub fn value(&self) -> Option<PackedColorShift> {
        self.active
            .as_ref()
            .and_then(|key| self.contexts.get(key))
            .map(|draft| draft.current)
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        enabled: bool,
        metadata: Option<external_cas_assets::casp::CaspColorSliders>,
        fully_transparent: Option<bool>,
    ) -> bool {
        let Some(draft) = self
            .active
            .as_ref()
            .and_then(|key| self.contexts.get_mut(key))
        else {
            return false;
        };
        let before = draft.current;
        let before_presets = (draft.presets.len(), draft.selected_preset);
        let [h, s, v, o] = draft.current.hue_saturation_value_opacity();
        let input = SwatchColorInput {
            ranges_hosv: metadata
                .map(|m| {
                    m.ranges_hosv().map(|r| {
                        Some(SliderRange {
                            minimum: r.minimum,
                            maximum: r.maximum,
                            step: r.step,
                        })
                    })
                })
                .unwrap_or([None; 4]),
            stored_hosv: [h, o, s, v].map(Some),
            fully_transparent,
            ..Default::default()
        };
        let resolved = color_ui::resolve(&input);
        egui::CollapsingHeader::new("Color sliders").id_salt("native_item_color_sliders").default_open(self.expanded_by_default).show(ui,|ui|{
            ui.label(egui::RichText::new("Hue · opacity · saturation · brightness").size(10.));
            ui.add_enabled_ui(enabled,|ui|{
                for(axis,control)in [Axis::Hue,Axis::Opacity,Axis::Saturation,Axis::Brightness].into_iter().zip(&resolved.controls_hosv){
                    if !control.visible_in_reference{continue;}
                    let Some(range)=control.range else{continue;};
                    let mut value=draft.current.value(axis);
                    let response=ui.add(egui::Slider::new(&mut value,range.minimum..=range.maximum).clamping(egui::SliderClamping::Never).text(control.label).step_by(range.step as f64));
                    if response.changed(){if let Ok(changed)=draft.current.replace_value(axis,value){draft.current=changed;draft.selected_preset=None;}}
                    if control.stored_value_outside_range{ui.label(egui::RichText::new("Stored value is outside this swatch's UI range; preserved until explicitly edited.").size(9.));}
                }
                ui.horizontal(|ui|{
                    if ui.button("Original").clicked(){draft.current=draft.base;draft.conflict=false;draft.selected_preset=None;}
                    if ui.button("Neutral").clicked(){draft.current=PackedColorShift::NEUTRAL;draft.selected_preset=None;}
                });
                let hosv=|value:PackedColorShift|{let[h,s,v,o]=value.hue_saturation_value_opacity();[h,o,s,v]};
                let actions=color_ui::custom_actions(hosv(draft.current),hosv(draft.base),draft.presets.len(),Some(draft.selected_preset.is_some()));
                ui.horizontal_wrapped(|ui|{
                    for(index,&preset)in draft.presets.iter().enumerate(){if ui.selectable_label(draft.selected_preset==Some(index),format!("{}",index+1)).on_hover_text(format!("Local color preset · {:016X}",preset.0)).clicked(){draft.current=preset;draft.selected_preset=Some(index);}}
                });
                ui.horizontal(|ui|{
                    if ui.add_enabled(actions.can_create,egui::Button::new("Save color preset")).clicked(){draft.presets.push(draft.current);draft.selected_preset=Some(draft.presets.len()-1);}
                    if ui.add_enabled(actions.can_delete,egui::Button::new("Delete preset")).clicked(){if let Some(index)=draft.selected_preset.take(){if index<draft.presets.len(){draft.presets.remove(index);}}}
                });
                ui.label(egui::RichText::new(format!("{} / 10 local presets · original source swatches stay intact",draft.presets.len())).size(9.));
            });
            if draft.conflict{ui.colored_label(egui::Color32::DARK_RED,"This item's saved colors changed while its preview had edits. Resolve before applying.");}
            if !draft.current.preview_supported(){ui.label("Stored modifier values are preserved; this range is not qualified for preview.");}
            if resolved.controls_hosv.iter().any(|c|c.fallback_used){ui.label(egui::RichText::new("Source metadata unavailable for some controls; tyjokr fallback ranges shown.").size(9.));}
            ui.label(egui::RichText::new("Native HSV/opacity preview · full source shader behavior remains under qualification").size(9.));
        });
        draft.current != before || before_presets != (draft.presets.len(), draft.selected_preset)
    }
    pub fn snapshot(&self) -> Self {
        Self {
            contexts: self
                .contexts
                .iter()
                .filter(|(_, draft)| {
                    draft.current != draft.base || draft.conflict || !draft.presets.is_empty()
                })
                .map(|(key, draft)| (key.clone(), draft.clone()))
                .collect(),
            active: None,
            expanded_by_default: false,
            extra: self.extra.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opening_controls_does_not_clamp_raw_zero_or_unsupported_lanes() {
        for raw in [0, 0xffff_ffff_ffff_ffff, 0x0000_7000_8000_0000] {
            let mut editor = ColorEditor::default();
            editor.expanded_by_default = true;
            editor.activate("test".into(), PackedColorShift(raw));
            let ctx = egui::Context::default();
            for _ in 0..3 {
                let mut output = ctx.run_ui(Default::default(), |root_ui| {
                    egui::CentralPanel::default().show(root_ui, |ui| {
                        assert!(!editor.show(ui, true, None, Some(false)));
                    });
                });
                output.textures_delta.clear();
            }
            assert_eq!(editor.value(), Some(PackedColorShift(raw)));
        }
    }
    #[test]
    fn per_sim_drafts_presets_and_unknown_context_fields_survive_snapshot() {
        let mut editor = ColorEditor::default();
        editor.activate("sim-one".into(), PackedColorShift::NEUTRAL);
        let first = editor.contexts.get_mut("sim-one").unwrap();
        first.current = PackedColorShift(0x2000_1000_0000_0000);
        first.presets.push(first.current);
        first
            .extra
            .insert("future-material".into(), serde_json::json!({"x":7}));
        editor.activate("sim-two".into(), PackedColorShift::NEUTRAL);
        let snapshot = editor.snapshot();
        assert!(!snapshot.contexts.contains_key("sim-two"));
        let bytes = serde_json::to_vec(&snapshot).unwrap();
        let mut restored: ColorEditor = serde_json::from_slice(&bytes).unwrap();
        restored.activate("sim-one".into(), PackedColorShift::NEUTRAL);
        assert_eq!(
            restored.value(),
            Some(PackedColorShift(0x2000_1000_0000_0000))
        );
        assert_eq!(restored.contexts["sim-one"].presets.len(), 1);
        assert!(
            restored.contexts["sim-one"]
                .extra
                .contains_key("future-material")
        );
    }
}
