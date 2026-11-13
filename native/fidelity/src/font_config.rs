// SPDX-License-Identifier: GPL-3.0-or-later
//! Original installed font configuration. Metadata does not imply that a host
//! renderer already implements its fallback, locale or font-metric semantics.
use crate::{
    fnv1_resource_name,
    installed::SourcePin,
    ui_layout::{UI_XML_TYPE, UiXmlNode, parse_xml_tree},
};
use external_cas_assets::{PackageIndex, ReadLimits, ResourceKey};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize)]
pub struct FontAlias {
    pub name: String,
    pub ordered_faces: Vec<String>,
    /// Original lexical metrics; scaling/units are not invented by this reader.
    pub metrics: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize)]
pub struct FontConfig {
    pub source_tree: UiXmlNode,
    pub aliases: BTreeMap<String, FontAlias>,
    pub remaps: BTreeMap<String, String>,
    pub library_names: Vec<String>,
    pub font_nodes: Vec<UiXmlNode>,
    pub locale_scale_nodes: Vec<UiXmlNode>,
}
#[derive(Clone, Debug)]
pub struct InstalledFontConfig {
    pub pin: SourcePin,
    pub source_bytes: Vec<u8>,
    pub config: FontConfig,
}
fn attribute<'a>(n: &'a UiXmlNode, name: &str) -> Result<&'a str, String> {
    n.attrs
        .get(name)
        .map(String::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("fontconfig {} lacks {name}", n.tag))
}
impl FontConfig {
    pub fn parse(raw: &[u8]) -> Result<Self, String> {
        let root = parse_xml_tree(raw)?;
        if root.tag != "fontconfig" {
            return Err("unsupported installed fontconfig root".into());
        }
        let mut result = Self {
            source_tree: root.clone(),
            aliases: BTreeMap::new(),
            remaps: BTreeMap::new(),
            library_names: Vec::new(),
            font_nodes: Vec::new(),
            locale_scale_nodes: Vec::new(),
        };
        for node in &root.children {
            match node.tag.as_str() {
                "library" => result.library_names.push(attribute(node, "name")?.into()),
                "font" => result.font_nodes.push(node.clone()),
                "fontscale" => result.locale_scale_nodes.push(node.clone()),
                "alias" => {
                    let name = attribute(node, "name")?.to_owned();
                    let mut faces = Vec::new();
                    for face in node.children.iter().filter(|n| n.tag == "use") {
                        faces.push(attribute(face, "font")?.to_owned());
                    }
                    if faces.is_empty() {
                        return Err(format!("source font alias {name} has no faces"));
                    }
                    let metrics = node
                        .attrs
                        .iter()
                        .filter(|(k, _)| k.as_str() != "name")
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    if result
                        .aliases
                        .insert(
                            name.clone(),
                            FontAlias {
                                name,
                                ordered_faces: faces,
                                metrics,
                            },
                        )
                        .is_some()
                    {
                        return Err("duplicate source font alias".into());
                    }
                }
                "remap" => {
                    if result
                        .remaps
                        .insert(
                            attribute(node, "from")?.into(),
                            attribute(node, "to")?.into(),
                        )
                        .is_some()
                    {
                        return Err("duplicate source font remap".into());
                    }
                }
                _ => {} // Complete unknown source node remains in source_tree.
            }
        }
        for from in result.remaps.keys() {
            result.canonical_alias(from)?;
        }
        Ok(result)
    }
    /// Apply only the source's explicit remap chain. No style/name similarity or
    /// system-font substitution is used. Terminal targets must be declared aliases.
    pub fn canonical_alias<'a>(&'a self, name: &'a str) -> Result<&'a str, String> {
        let mut current = name;
        let mut seen = BTreeSet::new();
        while let Some(next) = self.remaps.get(current) {
            if !seen.insert(current) {
                return Err("cyclic source font remap".into());
            }
            current = next;
        }
        if !self.aliases.contains_key(current) {
            return Err(format!("source font alias {current} is not declared"));
        }
        Ok(current)
    }
    pub fn alias(&self, name: &str) -> Result<&FontAlias, String> {
        let resolved = self.canonical_alias(name)?;
        self.aliases
            .get(resolved)
            .ok_or_else(|| "source alias absent".into())
    }
}
pub fn load_font_config(index: &PackageIndex) -> Result<InstalledFontConfig, String> {
    let key = ResourceKey::new(UI_XML_TYPE, 0, fnv1_resource_name("fontconfig"));
    let rows = index
        .matching(key)
        .filter(|e| !e.deleted())
        .collect::<Vec<_>>();
    if rows.len() != 1 {
        return Err(format!(
            "source fontconfig has {} matching provider ordinals",
            rows.len()
        ));
    }
    let e = rows[0];
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
    let config = FontConfig::parse(&raw)?;
    Ok(InstalledFontConfig {
        pin,
        source_bytes: raw,
        config,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remaps_chain_to_source_faces_and_preserve_metrics_unknowns() {
        let c=FontConfig::parse(br#"<fontconfig><alias name="$Body" ascent="944"><use font="Original A"/><use font="Original B"/><unknown/></alias><remap from="$Old" to="$Middle"/><remap from="$Middle" to="$Body"/><unknown bytes="keep"/></fontconfig>"#).unwrap();
        assert_eq!(c.canonical_alias("$Old").unwrap(), "$Body");
        assert_eq!(
            c.alias("$Old").unwrap().ordered_faces,
            ["Original A", "Original B"]
        );
        assert_eq!(c.alias("$Old").unwrap().metrics["ascent"], "944");
        assert_eq!(c.source_tree.children.last().unwrap().tag, "unknown");
        assert!(c.alias("uninstalled").is_err());
    }
    #[test]
    fn malformed_duplicates_cycles_and_external_entities_refuse() {
        for b in [br#"<fontconfig><remap from="a" to="b"/><remap from="b" to="a"/></fontconfig>"#.as_slice(),br#"<fontconfig><alias name="a"><use font="X"/></alias><alias name="a"><use font="Y"/></alias></fontconfig>"#,br#"<!DOCTYPE fontconfig SYSTEM "remote"><fontconfig/>"#,br#"<fontconfig><alias name="a"/></fontconfig>"#]{assert!(FontConfig::parse(b).is_err());}
        let b = br#"<fontconfig><alias name="a"><use font="X"/></alias></fontconfig>"#;
        for cut in 0..b.len() {
            assert!(FontConfig::parse(&b[..cut]).is_err(), "{cut}");
        }
    }
}
