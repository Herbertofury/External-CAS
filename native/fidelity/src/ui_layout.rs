// SPDX-License-Identifier: GPL-3.0-or-later
//! Original widgetlist XML and source layout inputs, not authored UI geometry.
//! Original bytes and unrecognized XML remain retained. Runtime source scripts
//! still own control states, text metrics, animation and native-command behavior.
use crate::{fnv1_resource_name, installed::SourcePin};
use external_cas_assets::{PackageIndex, ReadLimits, ResourceKey};
use quick_xml::{Reader, XmlVersion, events::Event};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, ops::Range};
pub const UI_XML_TYPE: u32 = 0x0333406c;
#[derive(Debug, Clone, Serialize)]
pub struct UiXmlNode {
    pub tag: String,
    pub attrs: BTreeMap<String, String>,
    pub text: String,
    pub children: Vec<UiXmlNode>,
    pub source_extent: Range<usize>,
}
impl UiXmlNode {
    fn child(&self, name: &str) -> Result<Option<&Self>, String> {
        let found = self
            .children
            .iter()
            .filter(|n| n.tag == name)
            .collect::<Vec<_>>();
        if found.len() > 1 {
            return Err(format!("duplicate source UI element {name}"));
        }
        Ok(found.first().copied())
    }
    fn attr(&self, name: &str) -> Result<&str, String> {
        self.attrs
            .get(name)
            .map(String::as_str)
            .ok_or_else(|| format!("source UI attribute {name} absent from {}", self.tag))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiPlatform {
    Pc,
    Console,
}
impl UiPlatform {
    fn accepts(self, node: &UiXmlNode) -> Result<bool, String> {
        match node.attrs.get("platform").map(String::as_str) {
            None => Ok(true),
            Some("PC") => Ok(self == Self::Pc),
            Some("CONSOLE") => Ok(self == Self::Console),
            Some(p) => Err(format!("unknown original UI platform {p}")),
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct UiPlacement {
    /// Horizontal/vertical normalized fractions, not percent points.
    pub position: [f64; 2],
    pub origin: [f64; 2],
    /// Original top,right,bottom,left margins in logical stage pixels.
    pub margins: [f64; 4],
    pub state_override: bool,
}
impl UiPlacement {
    /// Source LayoutManager placement. The helper rectangle must come from the
    /// actual widget's mcPositionHelper, in the same logical stage pixel space.
    /// No fitted helper bounds or UI scale is guessed here.
    pub fn place(
        &self,
        stage_pixels: [f64; 2],
        helper_pixels: [f64; 4],
    ) -> Result<[f64; 2], String> {
        if stage_pixels
            .iter()
            .chain(helper_pixels.iter())
            .any(|v| !v.is_finite())
            || stage_pixels.iter().any(|v| *v <= 0.)
            || helper_pixels[2..].iter().any(|v| *v < 0.)
        {
            return Err("invalid original UI placement inputs".into());
        }
        // The original implementation assigns remaining stage extents to int.
        let available = [
            (stage_pixels[0] - self.margins[1]).trunc(),
            (stage_pixels[1] - self.margins[2]).trunc(),
        ];
        Ok([
            available[0] * self.position[0] - helper_pixels[2] * self.origin[0] - helper_pixels[0]
                + self.margins[3],
            available[1] * self.position[1] - helper_pixels[3] * self.origin[1] - helper_pixels[1]
                + self.margins[0],
        ])
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct UiWidgetState {
    pub name: String,
    pub telemetry_name: Option<String>,
    pub layer: String,
    pub on_demand: bool,
    pub initially_visible: bool,
    pub placement: UiPlacement,
    pub source_widget: UiXmlNode,
    pub source_state: UiXmlNode,
}
#[derive(Debug, Clone, Serialize)]
pub struct UiLayoutManifest {
    pub source_tree: UiXmlNode,
    pub widgets: Vec<UiXmlNode>,
    pub game_states: Vec<UiXmlNode>,
}
#[derive(Debug)]
pub struct LoadedUiManifest {
    pub pin: SourcePin,
    pub source_bytes: Vec<u8>,
    pub manifest: UiLayoutManifest,
}
fn scalar(n: &UiXmlNode, name: &str) -> Result<f64, String> {
    let text = n.attr(name)?;
    let number = text
        .parse::<f64>()
        .map_err(|_| format!("invalid source UI numeric {name}"))?;
    if !number.is_finite() {
        return Err(format!("nonfinite source UI numeric {name}"));
    }
    Ok(number)
}
fn fractions(n: &UiXmlNode) -> Result<[f64; 2], String> {
    Ok([scalar(n, "hPerc")?, scalar(n, "vPerc")?])
}
impl UiLayoutManifest {
    pub fn widgets_for_state(
        &self,
        state: &str,
        platform: UiPlatform,
    ) -> Result<Vec<UiWidgetState>, String> {
        let states = self
            .game_states
            .iter()
            .filter(|n| n.attrs.get("stateName").map(String::as_str) == Some(state))
            .collect::<Vec<_>>();
        if states.len() != 1 {
            return Err(format!(
                "original UI game state {state} has {} definitions",
                states.len()
            ));
        }
        let mut result = Vec::new();
        for selected in &states[0].children {
            if !platform.accepts(selected)? {
                continue;
            }
            let name = selected.attr("name")?;
            let mut candidates = Vec::new();
            for widget in &self.widgets {
                if widget.attrs.get("name").map(String::as_str) == Some(name)
                    && platform.accepts(widget)?
                {
                    candidates.push(widget);
                }
            }
            if candidates.len() != 1 {
                return Err(format!(
                    "original UI widget {name} matches {} platform definitions",
                    candidates.len()
                ));
            }
            let widget = candidates[0];
            let layer = widget
                .child("layerProps")?
                .ok_or("source widget has no layerProps")?
                .attr("layer")?
                .to_owned();
            let position_override = selected.child("position")?;
            let overridden = position_override.is_some();
            let (position, origin) = if let Some(p) = position_override {
                (
                    fractions(p)?,
                    fractions(
                        selected
                            .child("origin")?
                            .ok_or("source UI position override lacks origin")?,
                    )?,
                )
            } else {
                (
                    fractions(
                        widget
                            .child("position")?
                            .ok_or("source UI position absent")?,
                    )?,
                    fractions(widget.child("origin")?.ok_or("source UI origin absent")?)?,
                )
            };
            let margins = widget
                .child("margin")?
                .map(|n| {
                    Ok::<_, String>([
                        scalar(n, "top")?,
                        scalar(n, "right")?,
                        scalar(n, "bottom")?,
                        scalar(n, "left")?,
                    ])
                })
                .transpose()?
                .unwrap_or([0.; 4]);
            result.push(UiWidgetState {
                name: name.into(),
                telemetry_name: widget.attrs.get("telemetryName").cloned(),
                layer,
                on_demand: widget.attrs.get("onDemandLoad").map(String::as_str) == Some("true"),
                initially_visible: selected.attrs.get("visible").map(String::as_str)
                    != Some("false"),
                placement: UiPlacement {
                    position,
                    origin,
                    margins,
                    state_override: overridden,
                },
                source_widget: widget.clone(),
                source_state: selected.clone(),
            });
        }
        Ok(result)
    }
}
/// Shared bounded source-XML tree reader. It retains unknown fields and never
/// resolves DTDs/external entities; specialized adapters validate their own root.
pub fn parse_xml_tree(raw: &[u8]) -> Result<UiXmlNode, String> {
    if raw.len() > 16 * 1024 * 1024 {
        return Err("UI manifest byte budget".into());
    }
    let mut reader = Reader::from_reader(raw);
    let mut stack = Vec::<UiXmlNode>::new();
    let mut root = None;
    let mut count = 0usize;
    loop {
        let start = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(|e| e.to_string())?;
        match event {
            Event::Start(e) | Event::Empty(e) => {
                count += 1;
                if count > raw.len().saturating_div(3) + 1 || stack.len() > 128 {
                    return Err("source UI XML structural budget".into());
                }
                let empty = raw
                    .get(start..reader.buffer_position() as usize)
                    .is_some_and(|b| b.ends_with(b"/>"));
                let mut attrs = BTreeMap::new();
                for a in e.attributes() {
                    let a = a.map_err(|e| e.to_string())?;
                    let key = a.key.as_ref().to_string();
                    let value = a
                        .normalized_value(XmlVersion::Implicit1_0)
                        .map_err(|e| e.to_string())?
                        .into_owned();
                    if attrs.insert(key, value).is_some() {
                        return Err("duplicate source UI XML attribute".into());
                    }
                }
                let node = UiXmlNode {
                    tag: e.name().as_ref().to_string(),
                    attrs,
                    text: String::new(),
                    children: Vec::new(),
                    source_extent: start..reader.buffer_position() as usize,
                };
                if empty {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(node);
                    } else if root.replace(node).is_some() {
                        return Err("multiple source UI roots".into());
                    }
                } else {
                    stack.push(node);
                }
            }
            Event::End(_) => {
                let mut node = stack.pop().ok_or("source UI unbalanced closing element")?;
                node.source_extent.end = reader.buffer_position() as usize;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else if root.replace(node).is_some() {
                    return Err("multiple source UI roots".into());
                }
            }
            Event::Text(e) => {
                let text = e.xml10_content();
                if let Some(node) = stack.last_mut() {
                    node.text.push_str(&text);
                } else if !text.trim().is_empty() {
                    return Err("source UI text outside root".into());
                }
            }
            Event::CData(e) => {
                stack
                    .last_mut()
                    .ok_or("source UI CDATA outside root")?
                    .text
                    .push_str(&e.xml10_content());
            }
            Event::GeneralRef(e) => {
                let encoded = format!("&{};", e.as_ref());
                let value = quick_xml::escape::unescape(&encoded).map_err(|e| e.to_string())?;
                stack
                    .last_mut()
                    .ok_or("source UI entity outside root")?
                    .text
                    .push_str(&value);
            }
            Event::DocType(_) => {
                return Err("source UI external/entity declarations are unsupported".into());
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err("truncated source UI manifest".into());
    }
    root.ok_or_else(|| "source UI root absent".into())
}
pub fn parse_manifest(raw: &[u8]) -> Result<UiLayoutManifest, String> {
    let root = parse_xml_tree(raw)?;
    if root.tag != "widgetList" {
        return Err("unsupported source UI manifest root".into());
    }
    let widgets = root
        .child("widgets")?
        .ok_or("source UI widgets absent")?
        .children
        .clone();
    let game_states = root
        .child("gameStates")?
        .ok_or("source UI gameStates absent")?
        .children
        .clone();
    Ok(UiLayoutManifest {
        source_tree: root,
        widgets,
        game_states,
    })
}
pub fn load_manifest(index: &PackageIndex) -> Result<LoadedUiManifest, String> {
    let key = ResourceKey::new(UI_XML_TYPE, 0, fnv1_resource_name("widgetlist"));
    let entries = index
        .matching(key)
        .filter(|e| !e.deleted())
        .collect::<Vec<_>>();
    if entries.len() != 1 {
        return Err(format!(
            "original widgetlist provider has {} matching rows",
            entries.len()
        ));
    }
    let e = entries[0];
    let raw = index
        .read_resource(
            e,
            ReadLimits {
                max_resource_bytes: 16 * 1024 * 1024,
            },
        )
        .map_err(|e| e.to_string())?;
    let pin = SourcePin {
        provider_path: index.path.clone(),
        key,
        ordinal: e.ordinal,
        index_sha256: index
            .index_sha256
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        payload_sha256: format!("{:x}", Sha256::digest(&raw)),
    };
    let manifest = parse_manifest(&raw)?;
    Ok(LoadedUiManifest {
        pin,
        source_bytes: raw,
        manifest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = "<widgetList><widgets><widget name='A'><layerProps layer='WIDGETS'/><position hPerc='0.5' vPerc='1'/><origin hPerc='0.5' vPerc='1'/><unknown alpha='1'>A&amp;B</unknown></widget></widgets><gameStates><gameState stateName='cas'><widget name='A' visible='false'/></gameState></gameStates></widgetList>";
    #[test]
    fn original_fraction_units_helper_origin_unknown_fields_and_visibility() {
        let m = parse_manifest(SOURCE.as_bytes()).unwrap();
        let w = m.widgets_for_state("cas", UiPlatform::Pc).unwrap();
        assert_eq!(w.len(), 1);
        assert!(!w[0].initially_visible);
        assert_eq!(w[0].source_widget.children[3].text, "A&B");
        assert_eq!(
            w[0].placement
                .place([1000., 800.], [10., 20., 200., 300.])
                .unwrap(),
            [390., 480.]
        );
        assert!(w[0].placement.place([f64::NAN, 800.], [0.; 4]).is_err());
    }
    #[test]
    fn truncated_duplicate_platform_unknown_entity_and_external_dtd_fail() {
        for end in 0..SOURCE.len() {
            assert!(parse_manifest(&SOURCE.as_bytes()[..end]).is_err());
        }
        assert!(
            parse_manifest(SOURCE.replace("name='A'", "name='A' name='B'").as_bytes()).is_err()
        );
        assert!(parse_manifest(SOURCE.replace("A&amp;B", "&notDefined;").as_bytes()).is_err());
        assert!(
            parse_manifest(
                format!("<!DOCTYPE widgetList [<!ENTITY x SYSTEM 'file:///any'>]>{SOURCE}")
                    .as_bytes()
            )
            .is_err()
        );
        let m = parse_manifest(
            SOURCE
                .replace(
                    "<widget name='A' visible=",
                    "<widget name='A' platform='UNKNOWN' visible=",
                )
                .as_bytes(),
        )
        .unwrap();
        assert!(m.widgets_for_state("cas", UiPlatform::Pc).is_err());
    }
}
