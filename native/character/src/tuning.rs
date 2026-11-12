// SPDX-License-Identifier: GPL-3.0-or-later
//! Selective combined XML/shared-reference observation. Scales are observed
//! asset values; native context/precedence qualification is never inferred.
use crate::*;
use quick_xml::{events::Event, Reader, XmlVersion};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Default)]
pub struct TuningContext {
    pub ages: Option<Vec<String>>,
    pub genders: Option<Vec<String>>,
    pub species: Option<Vec<String>>,
    pub occult: Option<Vec<String>>,
}
#[derive(Clone, Debug)]
pub struct TuningRecord {
    pub instance: u64,
    pub resource_namespace: String,
    pub context: TuningContext,
    pub modifier_names: BTreeMap<u64, String>,
    pub scales: BTreeMap<u64, f32>,
    pub resolved_xml: Option<Vec<u8>>,
    pub issues: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct CombinedXmlInspection {
    pub records: Vec<TuningRecord>,
    pub retained_nodes: usize,
    pub shared_entries: usize,
    pub issues: Vec<String>,
}
#[derive(Clone, Debug)]
struct Node {
    tag: String,
    attrs: BTreeMap<String, String>,
    text: String,
    children: Vec<usize>,
}
#[derive(Clone, Debug)]
struct Expanded {
    tag: String,
    attrs: BTreeMap<String, String>,
    text: String,
    children: Vec<Expanded>,
}
fn invalid(s: impl Into<String>) -> CharacterError {
    CharacterError::Invalid(s.into())
}
fn attrs(e: &quick_xml::events::BytesStart<'_>) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for a in e.attributes() {
        let a = a.map_err(|e| invalid(e.to_string()))?;
        let name = a.key.as_ref().to_string();
        let value = a
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|e| invalid(e.to_string()))?
            .into_owned();
        if out.insert(name, value).is_some() {
            return Err(invalid("Duplicate XML attribute"));
        }
    }
    Ok(out)
}
fn expand(
    id: usize,
    nodes: &[Node],
    shared: &BTreeMap<String, Vec<usize>>,
    path: &mut BTreeSet<usize>,
    visits: &mut usize,
    depth: usize,
) -> Result<Expanded> {
    if depth > 64 || *visits >= 100000 {
        return Err(invalid("Tuning expansion budget"));
    }
    *visits += 1;
    if !path.insert(id) {
        return Err(invalid("Tuning shared/node cycle"));
    }
    let node = nodes.get(id).ok_or_else(|| invalid("Tuning node index"))?;
    let result = if node.tag == "r" {
        let key = node
            .attrs
            .get("x")
            .ok_or_else(|| invalid("Shared reference lacks x"))?;
        let targets = shared
            .get(key)
            .ok_or_else(|| invalid("Shared reference target absent"))?;
        if targets.len() != 1 {
            return Err(invalid("Duplicate shared reference ID"));
        }
        let mut target = expand(targets[0], nodes, shared, path, visits, depth + 1)?;
        if let Some(name) = node.attrs.get("n") {
            target.attrs.entry("n".into()).or_insert(name.clone());
        }
        target
    } else {
        if !node.text.trim().is_empty() && !node.children.is_empty() {
            return Err(invalid("Mixed tuning text/children is unqualified"));
        }
        Expanded {
            tag: node.tag.clone(),
            attrs: node.attrs.clone(),
            text: node.text.clone(),
            children: node
                .children
                .iter()
                .map(|i| expand(*i, nodes, shared, path, visits, depth + 1))
                .collect::<Result<Vec<_>>>()?,
        }
    };
    path.remove(&id);
    Ok(result)
}
fn texts(node: &Expanded, out: &mut Vec<String>) -> Result<()> {
    if !["L", "E", "T", "U", "V"].contains(&node.tag.as_str()) {
        return Err(invalid(format!(
            "Unsupported tuning value tag {}",
            node.tag
        )));
    }
    if !node.text.trim().is_empty() {
        out.push(node.text.trim().into());
    }
    for child in &node.children {
        texts(child, out)?;
    }
    if out.len() > 128 {
        return Err(invalid("Context token budget"));
    }
    Ok(())
}
fn write_xml(n: &Expanded, out: &mut String) {
    out.push('<');
    out.push_str(&n.tag);
    for (k, v) in &n.attrs {
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        out.push_str(&quick_xml::escape::escape(v));
        out.push('"');
    }
    out.push('>');
    out.push_str(&quick_xml::escape::escape(&n.text));
    for c in &n.children {
        write_xml(c, out);
    }
    out.push_str("</");
    out.push_str(&n.tag);
    out.push('>');
}
fn parse_record(node: &Expanded, namespace: String) -> Result<TuningRecord> {
    let instance = node
        .attrs
        .get("s")
        .ok_or_else(|| invalid("Tuning instance absent"))?
        .parse::<u64>()
        .map_err(|_| invalid("Tuning instance is not decimal uint64"))?;
    let mut record = TuningRecord {
        instance,
        resource_namespace: namespace,
        context: TuningContext::default(),
        modifier_names: BTreeMap::new(),
        scales: BTreeMap::new(),
        resolved_xml: None,
        issues: vec![],
    };
    let mut current = None;
    let mut seen_context = BTreeSet::new();
    fn visit(
        n: &Expanded,
        r: &mut TuningRecord,
        current: &mut Option<u64>,
        seen: &mut BTreeSet<String>,
    ) -> Result<()> {
        if let Some(name) = n.attrs.get("n") {
            if ["Ages", "Genders", "Species", "OccultType"].contains(&name.as_str()) {
                if !seen.insert(name.clone()) {
                    return Err(invalid("Duplicate tuning context field"));
                }
                let mut values = vec![];
                texts(n, &mut values)?;
                let known = match name.as_str() {
                    "Ages" => vec![
                        "INFANT",
                        "TODDLER",
                        "CHILD",
                        "TEEN",
                        "YOUNGADULT",
                        "ADULT",
                        "ELDER",
                    ],
                    "Genders" => vec!["MALE", "FEMALE"],
                    "Species" => vec!["HUMAN", "SMALLDOG", "DOG", "CAT", "FOX", "HORSE"],
                    _ => vec![
                        "HUMAN", "VAMPIRE", "ALIEN", "MERMAID", "WITCH", "WEREWOLF", "FAIRY",
                    ],
                };
                if values.iter().any(|v| !known.contains(&v.as_str())) {
                    return Err(invalid("Unknown tuning context token"));
                }
                match name.as_str() {
                    "Ages" => r.context.ages = Some(values),
                    "Genders" => r.context.genders = Some(values),
                    "Species" => r.context.species = Some(values),
                    _ => r.context.occult = Some(values),
                }
            } else if name == "ModifierName" {
                if !n.children.is_empty() || n.text.is_empty() || n.text != n.text.trim() {
                    return Err(invalid("Unsupported modifier name text"));
                }
                let key = external_cas_scene::fnv64(&n.text).map_err(invalid)?;
                if r.modifier_names.insert(key, n.text.clone()).is_some() {
                    return Err(invalid("Duplicate modifier-name hash"));
                }
                *current = Some(key);
            } else if name == "Scale" {
                if !n.children.is_empty() {
                    return Err(invalid("Scale contains child structure"));
                }
                let key = current.ok_or_else(|| invalid("Scale has no preceding modifier name"))?;
                let scale = n
                    .text
                    .trim()
                    .parse::<f32>()
                    .map_err(|_| invalid("Scale number"))?;
                if !scale.is_finite() || r.scales.insert(key, scale).is_some() {
                    return Err(invalid("Nonfinite/duplicate tuning scale"));
                }
            }
        }
        for c in &n.children {
            visit(c, r, current, seen)?;
        }
        Ok(())
    }
    visit(node, &mut record, &mut current, &mut seen_context)?;
    let mut xml = String::new();
    write_xml(node, &mut xml);
    record.resolved_xml = Some(xml.into_bytes());
    Ok(record)
}
/// Bounded selective parser. Only shared-group nodes and the exact modifier
/// class are retained; unrelated combined records are streamed past.
pub fn inspect_combined_xml(bytes: &[u8], cancel: &AtomicBool) -> Result<CombinedXmlInspection> {
    if bytes.len() > 32 * 1024 * 1024 {
        return Err(CharacterError::Unsupported(
            "Combined XML byte budget".into(),
        ));
    }
    let mut reader = Reader::from_reader(bytes);
    let mut stack: Vec<(String, Option<usize>, Option<String>)> = vec![];
    let mut nodes: Vec<Node> = vec![];
    let mut roots = vec![];
    let mut groups = vec![];
    let mut events = 0usize;
    let mut text_bytes = 0usize;
    let mut top = None;
    loop {
        events += 1;
        if events > 6_000_000 {
            return Err(invalid("Combined XML event budget"));
        }
        if events % 1024 == 0 {
            check(cancel)?;
        }
        let event = reader.read_event().map_err(|e| invalid(e.to_string()))?;
        let empty = matches!(&event, Event::Empty(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let tag = e.name().as_ref().to_string();
                let attributes = attrs(&e)?;
                if stack.is_empty() {
                    if top.is_some() {
                        return Err(invalid("Multiple combined XML roots"));
                    }
                    top = Some(tag.clone());
                }
                let parent = stack.last().and_then(|v| v.1);
                let direct_group =
                    tag == "g" && stack.last().map(|v| v.0.as_str()) == Some("combined");
                let record = tag == "I"
                    && attributes.get("c").map(String::as_str) == Some("Client_CASModifierTuning")
                    && stack.last().map(|v| v.0.as_str()) == Some("R");
                let capture = parent.is_some() || direct_group || record;
                let id = if capture {
                    if nodes.len() >= 500_000 {
                        return Err(invalid("Selective combined node budget"));
                    }
                    let id = nodes.len();
                    nodes.push(Node {
                        tag: tag.clone(),
                        attrs: attributes.clone(),
                        text: String::new(),
                        children: vec![],
                    });
                    if let Some(parent) = parent {
                        nodes[parent].children.push(id);
                    }
                    if direct_group {
                        groups.push(id);
                    }
                    if record {
                        roots.push((
                            id,
                            stack.last().and_then(|v| v.2.clone()).unwrap_or_default(),
                        ));
                    }
                    Some(id)
                } else {
                    None
                };
                let namespace = if tag == "R" {
                    attributes.get("n").cloned()
                } else {
                    None
                };
                if !empty {
                    if stack.len() >= 64 {
                        return Err(invalid("Combined XML depth budget"));
                    }
                    stack.push((tag, id, namespace));
                }
            }
            Event::End(e) => {
                let tag = e.name().as_ref().to_string();
                let (last, _, _) = stack
                    .pop()
                    .ok_or_else(|| invalid("Unexpected XML closing element"))?;
                if tag != last {
                    return Err(invalid("Mismatched XML closing element"));
                }
            }
            Event::Text(t) => {
                if let Some(id) = stack.last().and_then(|v| v.1) {
                    let text = t.xml10_content();
                    text_bytes += text.len();
                    if text_bytes > 64 * 1024 * 1024 || nodes[id].text.len() + text.len() > 65536 {
                        return Err(invalid("Tuning text budget"));
                    }
                    nodes[id].text.push_str(&text);
                }
            }
            Event::CData(t) => {
                if let Some(id) = stack.last().and_then(|v| v.1) {
                    let text = t.xml10_content();
                    text_bytes += text.len();
                    if text_bytes > 64 * 1024 * 1024 {
                        return Err(invalid("Tuning text budget"));
                    }
                    nodes[id].text.push_str(&text);
                }
            }
            Event::GeneralRef(r) => {
                if let Some(id) = stack.last().and_then(|v| v.1) {
                    let text = format!("&{};", r.as_ref());
                    let value =
                        quick_xml::escape::unescape(&text).map_err(|e| invalid(e.to_string()))?;
                    nodes[id].text.push_str(&value);
                }
            }
            Event::DocType(_) => {
                return Err(CharacterError::Unsupported(
                    "Combined XML DTD/entity declarations".into(),
                ))
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() || top.as_deref() != Some("combined") {
        return Err(invalid("Expected complete combined XML root"));
    }
    let mut out = CombinedXmlInspection {
        records: vec![],
        retained_nodes: nodes.len(),
        shared_entries: 0,
        issues: vec![],
    };
    let mut shared: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    if groups.len() != 1 {
        out.issues
            .push("Combined XML requires exactly one shared group".into());
    } else {
        for child in &nodes[groups[0]].children {
            if let Some(x) = nodes[*child].attrs.get("x") {
                shared.entry(x.clone()).or_default().push(*child);
            } else {
                out.issues.push("Shared child without x ID".into());
            }
        }
    }
    out.shared_entries = shared.len();
    for (id, namespace) in roots {
        check(cancel)?;
        let result = expand(id, &nodes, &shared, &mut BTreeSet::new(), &mut 0, 0)
            .and_then(|n| parse_record(&n, namespace));
        match result {
            Ok(r) => out.records.push(r),
            Err(e) => out
                .issues
                .push(format!("Modifier tuning record unresolved: {e}")),
        }
    }
    Ok(out)
}
