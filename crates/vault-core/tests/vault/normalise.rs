//! Turning two keepassxc-cli exports into a form where any remaining difference
//! is a lost or altered field.
//!
//! What is normalised away, and why, is written down in
//! `tests/fixtures/README.md`. Nothing here removes content: it canonicalises
//! how a value is spelled, and sorts collections the format stores as maps.

use std::fmt::Write as _;

use base64::Engine as _;
use chrono::{Duration, NaiveDate, NaiveDateTime};
use quick_xml::Reader;
use quick_xml::events::Event;

/// Elements holding a KeePass timestamp. KDBX 3 spells them ISO 8601 and KDBX 4
/// spells them base64 seconds, and reading one format and writing the other has
/// to compare.
const TIMESTAMPS: &[&str] = &[
    "CreationTime",
    "LastModificationTime",
    "LastAccessTime",
    "ExpiryTime",
    "LocationChanged",
    "DeletionTime",
    "DatabaseNameChanged",
    "DatabaseDescriptionChanged",
    "DefaultUserNameChanged",
    "MasterKeyChanged",
    "RecycleBinChanged",
    "EntryTemplatesGroupChanged",
    "SettingsChanged",
];

/// Collections the `keepass` crate holds in a `HashMap`, so it writes them in
/// whatever order the hasher gives. The child element that identifies a member
/// decides the order here.
const UNORDERED: &[(&str, &str)] = &[
    ("CustomData", "Key"),
    ("CustomIcons", "UUID"),
    ("DeletedObjects", "UUID"),
];

/// Elements that exist because of the format version rather than because of
/// anything in the database. Only dropped when a fixture is read in one format
/// and written in another.
const FORMAT_ARTEFACTS: &[&str] = &["Binaries", "SettingsChanged"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    name: String,
    attributes: Vec<(String, String)>,
    text: String,
    children: Vec<Node>,
}

/// Reads an export and puts it in canonical form: one line per element, the
/// element's path and then a tab and its text.
///
/// `cross_version` drops the handful of elements that only differ because the
/// database went in as KDBX 3 and came out as KDBX 4.1.
pub fn canonical(xml: &str, cross_version: bool) -> Vec<String> {
    let mut root = parse(xml);
    walk(&mut root, cross_version);

    let mut lines = Vec::new();
    render(&root, &mut Vec::new(), &mut lines);
    lines
}

/// Describes how two canonical documents differ, naming elements and never
/// quoting a value.
///
/// `keepassxc-cli export` writes every protected value in cleartext, so a
/// failure report that printed the differing lines would put the whole database
/// in the test output. What a reader needs is which element moved or changed,
/// and that is what this says.
pub fn differences(before: &[String], after: &[String]) -> Vec<String> {
    let mut found = Vec::new();

    for (index, (left, right)) in before.iter().zip(after.iter()).enumerate() {
        if left == right {
            continue;
        }

        let (left_path, left_value) = split(left);
        let (right_path, right_value) = split(right);

        if left_path != right_path {
            found.push(format!(
                "line {index}: element {left_path} became {right_path}"
            ));
        } else {
            found.push(format!(
                "line {index}: {left_path} changed value ({} bytes became {} bytes)",
                left_value.len(),
                right_value.len()
            ));
        }

        if found.len() == 20 {
            found.push("...".to_owned());
            break;
        }
    }

    if before.len() != after.len() {
        found.push(format!(
            "the documents have different lengths: {} elements became {}",
            before.len(),
            after.len()
        ));
    }

    found
}

fn split(line: &str) -> (&str, &str) {
    line.split_once('\t').unwrap_or((line, ""))
}

fn parse(xml: &str) -> Node {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut stack: Vec<Node> = vec![Node {
        name: String::new(),
        attributes: Vec::new(),
        text: String::new(),
        children: Vec::new(),
    }];

    loop {
        match reader.read_event().expect("the export is well-formed XML") {
            Event::Eof => break,
            Event::Start(start) => stack.push(node_of(start.name().as_ref(), start.attributes())),
            Event::Empty(empty) => {
                let node = node_of(empty.name().as_ref(), empty.attributes());
                push(&mut stack, node);
            }
            Event::End(_) => {
                if let Some(node) = stack.pop() {
                    push(&mut stack, node);
                }
            }
            Event::Text(text) => {
                let decoded = text.decode().expect("the export is UTF-8").into_owned();
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&decoded);
                }
            }
            _ => {}
        }
    }

    stack.pop().unwrap_or_else(|| Node {
        name: String::new(),
        attributes: Vec::new(),
        text: String::new(),
        children: Vec::new(),
    })
}

fn node_of(name: &[u8], attributes: quick_xml::events::attributes::Attributes<'_>) -> Node {
    let mut collected: Vec<(String, String)> = attributes
        .flatten()
        .map(|attribute| {
            (
                String::from_utf8_lossy(attribute.key.as_ref()).into_owned(),
                String::from_utf8_lossy(&attribute.value).into_owned(),
            )
        })
        .collect();
    collected.sort();

    Node {
        name: String::from_utf8_lossy(name).into_owned(),
        attributes: collected,
        text: String::new(),
        children: Vec::new(),
    }
}

fn push(stack: &mut [Node], node: Node) {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(node);
    }
}

fn walk(node: &mut Node, cross_version: bool) {
    if TIMESTAMPS.contains(&node.name.as_str()) {
        node.text = canonical_time(&node.text);
    }

    if node.name == "CustomData" {
        for item in &mut node.children {
            if item.children.iter().any(is_export_stamp) {
                for child in &mut item.children {
                    if child.name == "Value" {
                        child.text = "<normalised>".to_owned();
                    }
                }
            }
        }
    }

    if cross_version {
        node.children
            .retain(|child| !FORMAT_ARTEFACTS.contains(&child.name.as_str()));
    }

    for child in &mut node.children {
        walk(child, cross_version);
    }

    if let Some((_, key)) = UNORDERED.iter().find(|(name, _)| *name == node.name) {
        node.children.sort_by_key(|child| identity(child, key));
    }
}

/// KeePassXC writes this into `Meta/CustomData` at export time, so two exports
/// of the same untouched file already differ here.
fn is_export_stamp(child: &Node) -> bool {
    child.name == "Key" && child.text == "_LAST_MODIFIED"
}

fn identity(node: &Node, key: &str) -> String {
    node.children
        .iter()
        .find(|child| child.name == key)
        .map(|child| child.text.clone())
        .unwrap_or_default()
}

/// Renders a KeePass timestamp as ISO 8601 whichever way it was written.
fn canonical_time(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.contains('-') && trimmed.ends_with('Z') {
        return trimmed.to_owned();
    }

    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(trimmed) else {
        return trimmed.to_owned();
    };
    let Ok(bytes) = <[u8; 8]>::try_from(bytes.as_slice()) else {
        return trimmed.to_owned();
    };

    let seconds = i64::from_le_bytes(bytes);
    let Some(year_one) = NaiveDate::from_ymd_opt(1, 1, 1) else {
        return trimmed.to_owned();
    };
    let base: NaiveDateTime = year_one.and_time(chrono::NaiveTime::MIN);

    match base.checked_add_signed(Duration::seconds(seconds)) {
        Some(moment) => moment.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        None => trimmed.to_owned(),
    }
}

fn render(node: &Node, path: &mut Vec<String>, out: &mut Vec<String>) {
    if !node.name.is_empty() {
        let mut here = node.name.clone();
        for (key, value) in &node.attributes {
            let _ = write!(here, "[{key}={value}]");
        }
        path.push(here);

        out.push(format!("{}\t{}", path.join("/"), node.text.trim()));
    }

    for child in &node.children {
        render(child, path, out);
    }

    if !node.name.is_empty() {
        path.pop();
    }
}
