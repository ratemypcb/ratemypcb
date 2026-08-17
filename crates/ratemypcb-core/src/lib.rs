use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use zip::ZipArchive;

pub const SCHEMA_VERSION: &str = "1.0";
pub const DISCLAIMER: &str = "RateMyPCB is a manufacturing preflight, not a compliance certificate. Confirm results with your fabricator and a qualified engineer.";
const MAX_BOARD_BYTES: u64 = 8 * 1024 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 90 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 256 * 1024 * 1024;
const MAX_ENTRIES: usize = 2_000;

#[derive(Debug)]
pub enum Error {
    Invalid(String),
    Ambiguous(String),
    Native(String),
    Internal(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(s) | Self::Ambiguous(s) | Self::Native(s) | Self::Internal(s) => {
                f.write_str(s)
            }
        }
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "info" => Some(Self::Info),
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            "critical" => Some(Self::Critical),
            _ => None,
        }
    }
    fn penalty(self) -> u8 {
        match self {
            Self::Critical => 22,
            Self::High => 13,
            Self::Medium => 7,
            Self::Low => 3,
            Self::Info => 0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub id: String,
    pub severity: Severity,
    pub category: String,
    pub title: String,
    pub evidence: String,
    pub recommendation: String,
    pub location: String,
    pub source: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageStatus {
    Passed,
    Attention,
    NotRun,
    NotProvided,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub id: String,
    pub label: String,
    pub status: CoverageStatus,
    pub evidence: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub path: String,
    pub kind: String,
    pub format: String,
    pub selected: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeDrc {
    pub status: String,
    pub tool: String,
    pub version: Option<String>,
    pub finding_count: usize,
    pub note: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Score {
    pub value: f32,
    pub raw: u8,
    pub verdict: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub schema_version: String,
    pub tool: ToolInfo,
    pub input: InputInfo,
    pub artifacts: Vec<Artifact>,
    pub score: Score,
    pub confidence: String,
    pub coverage: Vec<Coverage>,
    pub findings: Vec<Finding>,
    pub native_drc: NativeDrc,
    pub limitations: Vec<String>,
    pub disclaimer: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub version: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputInfo {
    pub path: String,
    pub kind: String,
    pub selected_board: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeMode {
    Auto,
    Off,
    Required,
}

#[derive(Clone, Copy, Debug)]
pub struct Preset {
    pub track: f64,
    pub via: f64,
    pub drill: f64,
    pub annular: f64,
}
impl Preset {
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "standard" => Some(Self {
                track: 0.2,
                via: 0.6,
                drill: 0.3,
                annular: 0.15,
            }),
            "compact" => Some(Self {
                track: 0.125,
                via: 0.45,
                drill: 0.2,
                annular: 0.1,
            }),
            "relaxed" => Some(Self {
                track: 0.3,
                via: 0.8,
                drill: 0.4,
                annular: 0.2,
            }),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ReviewOptions {
    pub board: Option<String>,
    pub preset: Preset,
    pub native: NativeMode,
    pub tool_version: String,
}

#[derive(Default)]
struct BoardFacts {
    tracks: Vec<f64>,
    vias: Vec<(f64, f64)>,
    nets: BTreeMap<u32, String>,
    pad_nets: BTreeMap<u32, usize>,
    routed_nets: BTreeSet<u32>,
    zone_nets: BTreeSet<u32>,
    edge_forms: usize,
    components: usize,
    references: BTreeSet<String>,
}

fn forms<'a>(source: &'a str, name: &str) -> Vec<&'a str> {
    let needle = format!("({name}");
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = source[cursor..].find(&needle) {
        let start = cursor + relative;
        let boundary = bytes.get(start + needle.len()).copied().unwrap_or(b')');
        if !boundary.is_ascii_whitespace() && boundary != b')' {
            cursor = start + needle.len();
            continue;
        }
        let (mut depth, mut quoted, mut escaped) = (0_i32, false, false);
        let mut closed = None;
        for (offset, byte) in bytes[start..].iter().copied().enumerate() {
            if quoted {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    quoted = false;
                }
            } else if byte == b'"' {
                quoted = true;
            } else if byte == b'(' {
                depth += 1;
            } else if byte == b')' {
                depth -= 1;
                if depth == 0 {
                    closed = Some(start + offset + 1);
                    break;
                }
            }
        }
        if let Some(end) = closed {
            out.push(&source[start..end]);
            cursor = end;
        } else {
            break;
        }
    }
    out
}

fn scalar(form: &str, field: &str) -> Option<f64> {
    let start = form.find(&format!("({field} "))? + field.len() + 2;
    let token = form[start..]
        .split(|c: char| c.is_whitespace() || c == ')')
        .next()?;
    let numeric: String = token
        .chars()
        .take_while(|character| character.is_ascii_digit() || matches!(character, '-' | '+' | '.'))
        .collect();
    numeric.parse().ok()
}
fn integer(form: &str, field: &str) -> Option<u32> {
    scalar(form, field).map(|v| v as u32)
}
fn quoted(form: &str, field: &str) -> Option<String> {
    let start = form.find(&format!("({field} \""))? + field.len() + 3;
    let end = form[start..].find('"')?;
    Some(form[start..start + end].to_string())
}
fn layer(form: &str) -> String {
    quoted(form, "layer").unwrap_or_default()
}

fn property_value(form: &str, property: &str) -> Option<String> {
    let needle = format!("(property \"{property}\" \"");
    let start = form.find(&needle)? + needle.len();
    let end = form[start..].find('"')?;
    Some(form[start..start + end].to_string())
}

fn parse_board(source: &str) -> Result<BoardFacts, Error> {
    if source.len() as u64 > MAX_BOARD_BYTES
        || !source
            .trim_start_matches('\u{feff}')
            .starts_with("(kicad_pcb")
    {
        return Err(Error::Invalid(
            "Board must be a valid KiCad .kicad_pcb file no larger than 8 MB.".into(),
        ));
    }
    let mut facts = BoardFacts::default();
    for form in forms(source, "net") {
        if let (Some(id), Some(name)) = (integer(form, "net"), form.split('"').nth(1)) {
            if !name.is_empty() {
                facts.nets.insert(id, name.into());
            }
        }
    }
    for form in forms(source, "segment") {
        if let Some(width) = scalar(form, "width") {
            facts.tracks.push(width);
        }
        if let Some(net) = integer(form, "net") {
            if net > 0 {
                facts.routed_nets.insert(net);
            }
        }
    }
    for form in forms(source, "via") {
        facts.vias.push((
            scalar(form, "size").unwrap_or(0.0),
            scalar(form, "drill").unwrap_or(0.0),
        ));
        if let Some(net) = integer(form, "net") {
            if net > 0 {
                facts.routed_nets.insert(net);
            }
        }
    }
    for form in forms(source, "pad") {
        if let Some(net) = integer(form, "net") {
            if net > 0 {
                *facts.pad_nets.entry(net).or_insert(0) += 1;
            }
        }
    }
    for form in forms(source, "zone") {
        if let Some(net) = integer(form, "net") {
            if net > 0 {
                facts.zone_nets.insert(net);
            }
        }
    }
    facts.edge_forms = ["gr_line", "gr_arc", "gr_rect", "gr_curve"]
        .into_iter()
        .flat_map(|n| forms(source, n))
        .filter(|f| layer(f) == "Edge.Cuts")
        .count();
    let footprints: Vec<_> = forms(source, "footprint")
        .into_iter()
        .chain(forms(source, "module"))
        .collect();
    facts.components = footprints.len();
    facts.references = footprints
        .into_iter()
        .filter_map(|form| property_value(form, "Reference"))
        .filter(|reference| !reference.is_empty() && reference != "?")
        .collect();
    Ok(facts)
}

#[allow(clippy::too_many_arguments)] // Mirrors the stable, flat public finding contract.
fn finding(
    id: &str,
    severity: Severity,
    category: &str,
    title: &str,
    evidence: String,
    recommendation: &str,
    location: &str,
    source: &str,
) -> Finding {
    Finding {
        id: id.into(),
        severity,
        category: category.into(),
        title: title.into(),
        evidence,
        recommendation: recommendation.into(),
        location: location.into(),
        source: source.into(),
    }
}

fn static_findings(facts: &BoardFacts, preset: Preset) -> Vec<Finding> {
    let mut out = Vec::new();
    if facts.edge_forms == 0 {
        out.push(finding(
            "outline-missing",
            Severity::High,
            "Fabrication",
            "Board outline is missing",
            "No Edge.Cuts geometry was found.".into(),
            "Add one closed board outline on Edge.Cuts.",
            "Edge.Cuts",
            "static",
        ));
    }
    let narrow: Vec<_> = facts.tracks.iter().filter(|v| **v < preset.track).collect();
    if !narrow.is_empty() {
        out.push(finding(
            "track-width",
            Severity::High,
            "Copper",
            "Tracks fall below the active width rule",
            format!(
                "{} of {} segments are below {:.3} mm; smallest is {:.3} mm.",
                narrow.len(),
                facts.tracks.len(),
                preset.track,
                narrow.into_iter().copied().fold(f64::INFINITY, f64::min)
            ),
            "Widen the tracks or confirm the process with your fabricator.",
            "Board-wide",
            "static",
        ));
    }
    let small_vias = facts
        .vias
        .iter()
        .filter(|(size, _)| *size < preset.via)
        .count();
    if small_vias > 0 {
        out.push(finding(
            "via-diameter",
            Severity::High,
            "Drills",
            "Via diameters fall below the active rule",
            format!(
                "{small_vias} of {} vias are below {:.3} mm.",
                facts.vias.len(),
                preset.via
            ),
            "Increase via diameter or confirm an HDI-capable process.",
            "Board-wide",
            "static",
        ));
    }
    let small_drills = facts
        .vias
        .iter()
        .filter(|(_, drill)| *drill < preset.drill)
        .count();
    if small_drills > 0 {
        out.push(finding(
            "via-drill",
            Severity::High,
            "Drills",
            "Via drills fall below the active rule",
            format!(
                "{small_drills} of {} drills are below {:.3} mm.",
                facts.vias.len(),
                preset.drill
            ),
            "Increase drill size or confirm the process.",
            "Board-wide",
            "static",
        ));
    }
    let thin = facts
        .vias
        .iter()
        .filter(|(size, drill)| (*size - *drill) / 2.0 < preset.annular)
        .count();
    if thin > 0 {
        out.push(finding(
            "annular-width",
            Severity::High,
            "Drills",
            "Via annular rings are too thin",
            format!(
                "{thin} vias have less than {:.3} mm radial copper.",
                preset.annular
            ),
            "Increase pad diameter or reduce drill size within process limits.",
            "Board-wide",
            "static",
        ));
    }
    let ground_ids: BTreeSet<_> = facts
        .nets
        .iter()
        .filter(|(_, n)| {
            matches!(
                n.to_ascii_uppercase().as_str(),
                "GND" | "AGND" | "DGND" | "PGND"
            )
        })
        .map(|(id, _)| *id)
        .collect();
    if !ground_ids.is_empty() && ground_ids.is_disjoint(&facts.zone_nets) {
        out.push(finding(
            "ground-zone",
            Severity::Medium,
            "Return paths",
            "No ground copper zone is identifiable",
            format!(
                "{} ground net(s) were found, but none is assigned to a zone.",
                ground_ids.len()
            ),
            "Verify reference-plane coverage and return paths.",
            "Board-wide",
            "static",
        ));
    }
    let unrouted: Vec<_> = facts
        .pad_nets
        .iter()
        .filter(|(id, count)| {
            **count > 1 && !facts.routed_nets.contains(id) && !facts.zone_nets.contains(id)
        })
        .collect();
    if !unrouted.is_empty() {
        out.push(finding(
            "unrouted-candidates",
            Severity::Medium,
            "Connectivity",
            "Some multi-pad nets have no routed copper",
            format!(
                "{} candidate net(s) have multiple pads but no track, via, or zone evidence.",
                unrouted.len()
            ),
            "Inspect the ratsnest and run native connectivity DRC.",
            "Board-wide",
            "static",
        ));
    }
    out
}

fn classify(path: &str) -> Option<(&'static str, &'static str)> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".kicad_pcb") {
        Some(("board", "kicad"))
    } else if lower.ends_with(".pcbdoc") {
        Some(("board", "altium"))
    } else if lower.ends_with(".kicad_pro") {
        Some(("settings", "kicad"))
    } else if lower.ends_with(".kicad_dru") {
        Some(("rules", "kicad"))
    } else if [
        ".gtl", ".gbl", ".gbr", ".ger", ".gko", ".gts", ".gbs", ".gto", ".gbo", ".gtp", ".gbp",
    ]
    .iter()
    .any(|e| lower.ends_with(e))
    {
        Some(("gerber", "rs-274x"))
    } else if [".drl", ".xln", ".exc"].iter().any(|e| lower.ends_with(e)) {
        Some(("drill", "excellon"))
    } else if [".csv", ".tsv"].iter().any(|e| lower.ends_with(e)) && lower.contains("bom") {
        Some(("bom", "delimited"))
    } else if lower.contains("position") || lower.contains("pick") || lower.ends_with(".pos") {
        Some(("placement", "centroid"))
    } else {
        None
    }
}

fn gerber_role(path: &str) -> Option<&'static str> {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    if name.ends_with(".gtl") || name.contains("f_cu") || name.contains("front-cu") {
        Some("top-copper")
    } else if name.ends_with(".gbl") || name.contains("b_cu") || name.contains("back-cu") {
        Some("bottom-copper")
    } else if name.ends_with(".gko")
        || name.contains("edge-cuts")
        || name.contains("outline")
        || name.contains("profile")
    {
        Some("profile")
    } else {
        None
    }
}

fn safe_archive_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && path.len() <= 512
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

fn collect_dir(root: &Path, output: &mut Vec<PathBuf>, depth: usize) -> Result<(), Error> {
    if depth > 12 {
        return Ok(());
    }
    let entries = fs::read_dir(root)
        .map_err(|e| Error::Invalid(format!("Cannot read {}: {e}", root.display())))?;
    for entry in entries {
        let entry = entry.map_err(|e| Error::Invalid(e.to_string()))?;
        let path = entry.path();
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.')
            || matches!(name.to_str(), Some("node_modules" | "target" | "vendor"))
        {
            continue;
        }
        let ty = entry
            .file_type()
            .map_err(|e| Error::Invalid(e.to_string()))?;
        if ty.is_dir() {
            collect_dir(&path, output, depth + 1)?;
        } else if ty.is_file() && classify(&path.to_string_lossy()).is_some() {
            output.push(path);
        }
        if output.len() > MAX_ENTRIES {
            return Err(Error::Invalid(
                "Project contains more than 2,000 recognized artifacts.".into(),
            ));
        }
    }
    Ok(())
}

struct Loaded {
    input_kind: String,
    board_name: Option<String>,
    board_source: Option<String>,
    artifacts: Vec<Artifact>,
    package_findings: Vec<Finding>,
    package_coverage: Vec<Coverage>,
    rules: Option<(String, String)>,
    bom: Option<(String, String)>,
}

fn split_delimited(line: &str, delimiter: char) -> Vec<String> {
    let mut output = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '"' {
            if quoted && chars.peek() == Some(&'"') {
                current.push('"');
                chars.next();
            } else {
                quoted = !quoted;
            }
        } else if character == delimiter && !quoted {
            output.push(current.trim().to_string());
            current.clear();
        } else {
            current.push(character);
        }
    }
    output.push(current.trim().to_string());
    output
}

fn bom_review(source: &str, board: Option<&BoardFacts>) -> (Vec<Finding>, Coverage) {
    let lines: Vec<_> = source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(502)
        .collect();
    if lines.is_empty() {
        return (
            vec![finding(
                "bom-empty",
                Severity::Medium,
                "BOM",
                "BOM is empty",
                "No non-empty rows were found.".into(),
                "Export the fitted bill of materials for this board revision.",
                "BOM",
                "bom",
            )],
            Coverage {
                id: "bom-structure".into(),
                label: "BOM structure and board correlation".into(),
                status: CoverageStatus::Attention,
                evidence: "The selected BOM was empty.".into(),
            },
        );
    }
    let delimiter = ['\t', ',', ';']
        .into_iter()
        .max_by_key(|delimiter| lines[0].matches(*delimiter).count())
        .unwrap_or(',');
    let headers: Vec<_> = split_delimited(lines[0], delimiter)
        .into_iter()
        .map(|header| header.to_ascii_lowercase().replace([' ', '_', '-'], ""))
        .collect();
    let reference_index = headers.iter().position(|header| {
        matches!(
            header.as_str(),
            "reference" | "references" | "ref" | "refs" | "designator" | "designators"
        )
    });
    let mpn_index = headers.iter().position(|header| {
        matches!(
            header.as_str(),
            "mpn" | "manufacturerpartnumber" | "partnumber"
        )
    });
    let Some(reference_index) = reference_index else {
        return (
            vec![finding(
                "bom-references-missing",
                Severity::High,
                "BOM",
                "BOM has no reference-designator column",
                format!("Headers found: {}.", headers.join(", ")),
                "Export a BOM with Reference or Designator identifiers.",
                "BOM header",
                "bom",
            )],
            Coverage {
                id: "bom-structure".into(),
                label: "BOM structure and board correlation".into(),
                status: CoverageStatus::Attention,
                evidence: "No reference-designator column was recognized.".into(),
            },
        );
    };
    let mut references = BTreeSet::new();
    let mut rows = 0;
    let mut rows_without_mpn = 0;
    for line in lines.iter().skip(1).take(500) {
        let fields = split_delimited(line, delimiter);
        if fields.iter().all(|field| field.is_empty()) {
            continue;
        }
        rows += 1;
        if mpn_index
            .and_then(|index| fields.get(index))
            .is_none_or(|mpn| mpn.trim().is_empty())
        {
            rows_without_mpn += 1;
        }
        if let Some(value) = fields.get(reference_index) {
            for reference in value.split(|character: char| {
                character.is_whitespace() || matches!(character, ',' | ';')
            }) {
                let reference = reference.trim();
                if !reference.is_empty() {
                    references.insert(reference.to_ascii_uppercase());
                }
            }
        }
    }
    let mut findings = Vec::new();
    if rows == 0 {
        findings.push(finding(
            "bom-empty",
            Severity::Medium,
            "BOM",
            "BOM has no component rows",
            "A header was present, but no component rows were parsed.".into(),
            "Export the fitted bill of materials.",
            "BOM",
            "bom",
        ));
    }
    if mpn_index.is_none() || rows_without_mpn > 0 {
        findings.push(finding("bom-mpn-coverage", Severity::Medium, "BOM", "BOM manufacturer identity is incomplete", format!("{rows_without_mpn} of {rows} parsed row(s) have no recognized manufacturer part number."), "Add exact manufacturer part numbers for fitted purchased components.", "BOM", "bom"));
    }
    if let Some(board) = board {
        let board_refs: BTreeSet<_> = board
            .references
            .iter()
            .map(|reference| reference.to_ascii_uppercase())
            .collect();
        let missing: Vec<_> = board_refs
            .difference(&references)
            .take(12)
            .cloned()
            .collect();
        let unknown: Vec<_> = references
            .difference(&board_refs)
            .take(12)
            .cloned()
            .collect();
        if !missing.is_empty() {
            findings.push(finding(
                "bom-missing-references",
                Severity::Medium,
                "BOM",
                "Board references are absent from the BOM",
                missing.join(", "),
                "Regenerate the BOM from the matching board revision and confirm DNP policy.",
                "BOM",
                "bom",
            ));
        }
        if !unknown.is_empty() {
            findings.push(finding(
                "bom-unknown-references",
                Severity::Medium,
                "BOM",
                "BOM references are absent from the board",
                unknown.join(", "),
                "Remove stale BOM rows or use the matching board revision.",
                "BOM",
                "bom",
            ));
        }
    }
    let status = if findings.is_empty() {
        CoverageStatus::Passed
    } else {
        CoverageStatus::Attention
    };
    (
        findings,
        Coverage {
            id: "bom-structure".into(),
            label: "BOM structure and board correlation".into(),
            status,
            evidence: format!(
                "{rows} row(s) and {} unique reference(s) were parsed.",
                references.len()
            ),
        },
    )
}

fn coherent_sidecar(board: &str, candidates: impl Iterator<Item = String>) -> Option<String> {
    let board_path = Path::new(board);
    let board_parent = board_path.parent().unwrap_or_else(|| Path::new(""));
    let board_stem = board_path.file_stem()?.to_string_lossy();
    let mut matches: Vec<_> = candidates
        .filter(|candidate| {
            let path = Path::new(candidate);
            path.parent().unwrap_or_else(|| Path::new("")) == board_parent
                && path
                    .file_stem()
                    .is_some_and(|stem| stem == board_stem.as_ref())
        })
        .collect();
    matches.sort_by_key(|candidate| {
        if candidate.to_ascii_lowercase().ends_with(".kicad_dru") {
            0
        } else {
            1
        }
    });
    matches.into_iter().next()
}

fn resolve_project_rules(base: Preset, name: &str, source: &str) -> (Preset, usize) {
    let mut result = base;
    let mut imported = 0;
    let mut assign = |field: &str, value: Option<f64>| {
        if let Some(value) = value.filter(|value| *value > 0.0) {
            match field {
                "track" => result.track = value,
                "via" => result.via = value,
                "drill" => result.drill = value,
                "annular" => result.annular = value,
                _ => return,
            }
            imported += 1;
        }
    };
    if name.to_ascii_lowercase().ends_with(".kicad_pro") {
        if let Ok(project) = serde_json::from_str::<Value>(source.trim_start_matches('\u{feff}')) {
            let rules = &project["board"]["design_settings"]["rules"];
            assign("track", rules["min_track_width"].as_f64());
            assign("via", rules["min_via_diameter"].as_f64());
            assign("drill", rules["min_through_hole_diameter"].as_f64());
            assign("annular", rules["min_via_annular_width"].as_f64());
        }
    } else {
        for constraint in forms(source, "constraint") {
            let lower = constraint.to_ascii_lowercase();
            if lower.starts_with("(constraint track_width") {
                assign("track", scalar(constraint, "min"));
            } else if lower.starts_with("(constraint via_diameter") {
                assign("via", scalar(constraint, "min"));
            } else if lower.starts_with("(constraint hole_size") {
                assign("drill", scalar(constraint, "min"));
            } else if lower.starts_with("(constraint annular_width") {
                assign("annular", scalar(constraint, "min"));
            }
        }
    }
    (result, imported)
}

fn select_board(candidates: &[String], selector: Option<&str>) -> Result<Option<String>, Error> {
    if candidates.is_empty() {
        return Ok(None);
    }
    if let Some(selector) = selector {
        let normalized = selector.replace('\\', "/");
        let matches: Vec<_> = candidates
            .iter()
            .filter(|p| {
                p.replace('\\', "/") == normalized
                    || Path::new(p).file_name().and_then(|n| n.to_str())
                        == Path::new(&normalized).file_name().and_then(|n| n.to_str())
            })
            .collect();
        return if matches.len() == 1 {
            Ok(Some(matches[0].clone()))
        } else {
            Err(Error::Invalid(format!(
                "--board did not uniquely match a board. Candidates: {}",
                candidates.join(", ")
            )))
        };
    }
    if candidates.len() == 1 {
        Ok(Some(candidates[0].clone()))
    } else {
        Err(Error::Ambiguous(format!(
            "Multiple KiCad boards found; rerun with --board PATH. Candidates: {}",
            candidates.join(", ")
        )))
    }
}

fn load_path(path: &Path, selector: Option<&str>) -> Result<Loaded, Error> {
    if path.is_dir() {
        let mut files = Vec::new();
        collect_dir(path, &mut files, 0)?;
        files.sort();
        let names: Vec<String> = files
            .iter()
            .map(|p| {
                p.strip_prefix(path)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        let boards: Vec<String> = names
            .iter()
            .filter(|p| p.to_ascii_lowercase().ends_with(".kicad_pcb"))
            .cloned()
            .collect();
        let selected = select_board(&boards, selector)?;
        let source = selected
            .as_ref()
            .map(|name| {
                fs::read_to_string(path.join(name))
                    .map_err(|e| Error::Invalid(format!("Cannot read {name}: {e}")))
            })
            .transpose()?;
        let rules_name = selected.as_ref().and_then(|board| {
            coherent_sidecar(
                board,
                names
                    .iter()
                    .filter(|name| {
                        let lower = name.to_ascii_lowercase();
                        lower.ends_with(".kicad_dru") || lower.ends_with(".kicad_pro")
                    })
                    .cloned(),
            )
        });
        let rules = rules_name
            .as_ref()
            .map(|name| {
                fs::read_to_string(path.join(name))
                    .map(|source| (name.clone(), source))
                    .map_err(|error| Error::Invalid(format!("Cannot read {name}: {error}")))
            })
            .transpose()?;
        let bom_names: Vec<_> = names
            .iter()
            .filter(|name| classify(name).is_some_and(|(kind, _)| kind == "bom"))
            .cloned()
            .collect();
        let bom = if bom_names.len() == 1 {
            let name = &bom_names[0];
            Some((
                name.clone(),
                fs::read_to_string(path.join(name))
                    .map_err(|error| Error::Invalid(format!("Cannot read {name}: {error}")))?,
            ))
        } else {
            None
        };
        let artifacts = names
            .into_iter()
            .filter_map(|name| {
                classify(&name).map(|(kind, format)| Artifact {
                    selected: selected.as_ref() == Some(&name)
                        || rules_name.as_ref() == Some(&name),
                    path: name,
                    kind: kind.into(),
                    format: format.into(),
                })
            })
            .collect();
        return Ok(Loaded {
            input_kind: "directory".into(),
            board_name: selected,
            board_source: source,
            artifacts,
            package_findings: vec![],
            package_coverage: vec![],
            rules,
            bom,
        });
    }
    if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
    {
        return load_zip(path, selector);
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("board.kicad_pcb")
        .to_string();
    if !name.to_ascii_lowercase().ends_with(".kicad_pcb") {
        return Err(Error::Invalid(
            "Review a directory, .kicad_pcb file, or fabrication .zip.".into(),
        ));
    }
    let source = fs::read_to_string(path)
        .map_err(|e| Error::Invalid(format!("Cannot read {}: {e}", path.display())))?;
    Ok(Loaded {
        input_kind: "kicad-board".into(),
        board_name: Some(name.clone()),
        board_source: Some(source),
        artifacts: vec![Artifact {
            path: name,
            kind: "board".into(),
            format: "kicad".into(),
            selected: true,
        }],
        package_findings: vec![],
        package_coverage: vec![],
        rules: None,
        bom: None,
    })
}

fn load_zip(path: &Path, selector: Option<&str>) -> Result<Loaded, Error> {
    let size = fs::metadata(path)
        .map_err(|e| Error::Invalid(e.to_string()))?
        .len();
    if size == 0 || size > MAX_ARCHIVE_BYTES {
        return Err(Error::Invalid(
            "Fabrication ZIP must be between 1 byte and 90 MB.".into(),
        ));
    }
    let file = File::open(path).map_err(|e| Error::Invalid(e.to_string()))?;
    let mut zip = ZipArchive::new(file)
        .map_err(|_| Error::Invalid("Fabrication ZIP is invalid or unsupported.".into()))?;
    if zip.len() > MAX_ENTRIES {
        return Err(Error::Invalid(
            "Fabrication ZIP has more than 2,000 entries.".into(),
        ));
    }
    let mut artifacts = Vec::new();
    let mut boards = Vec::new();
    let mut sources = BTreeMap::new();
    let mut sidecars = BTreeMap::new();
    let mut boms = BTreeMap::new();
    let mut expanded = 0_u64;
    let mut seen = BTreeSet::new();
    for index in 0..zip.len() {
        let mut item = zip
            .by_index(index)
            .map_err(|_| Error::Invalid("Cannot read ZIP entry.".into()))?;
        let name = item.name().to_string();
        if item.is_dir() {
            continue;
        }
        if !safe_archive_path(&name) {
            return Err(Error::Invalid(format!(
                "Fabrication ZIP contains an unsafe path: {name}"
            )));
        }
        let key = name.to_ascii_lowercase();
        if !seen.insert(key) {
            return Err(Error::Invalid(
                "Fabrication ZIP contains duplicate normalized paths.".into(),
            ));
        }
        expanded = expanded.saturating_add(item.size());
        if expanded > MAX_EXPANDED_BYTES {
            return Err(Error::Invalid(
                "Fabrication ZIP expands beyond 256 MB.".into(),
            ));
        }
        if let Some((kind, format)) = classify(&name) {
            artifacts.push(Artifact {
                path: name.clone(),
                kind: kind.into(),
                format: format.into(),
                selected: false,
            });
            if format == "kicad" && matches!(kind, "board" | "rules" | "settings") {
                let limit = if kind == "board" {
                    MAX_BOARD_BYTES
                } else {
                    512 * 1024
                };
                if item.size() > limit {
                    return Err(Error::Invalid(format!(
                        "{name} exceeds its inspection limit."
                    )));
                }
                let mut source = String::new();
                item.read_to_string(&mut source)
                    .map_err(|_| Error::Invalid(format!("{name} is not UTF-8 KiCad source.")))?;
                if kind == "board" {
                    boards.push(name.clone());
                    sources.insert(name, source);
                } else {
                    sidecars.insert(name, source);
                }
            } else if kind == "bom" {
                if item.size() > 256 * 1024 {
                    return Err(Error::Invalid(format!(
                        "{name} exceeds the 256 KB BOM limit."
                    )));
                }
                let mut source = String::new();
                item.read_to_string(&mut source)
                    .map_err(|_| Error::Invalid(format!("{name} is not a UTF-8 BOM.")))?;
                boms.insert(name, source);
            }
        }
    }
    if artifacts.is_empty() {
        return Err(Error::Invalid(
            "Fabrication ZIP has no recognized PCB artifacts.".into(),
        ));
    }
    boards.sort();
    let selected = select_board(&boards, selector)?;
    let rules_name = selected
        .as_ref()
        .and_then(|board| coherent_sidecar(board, sidecars.keys().cloned()));
    for item in &mut artifacts {
        item.selected =
            selected.as_ref() == Some(&item.path) || rules_name.as_ref() == Some(&item.path);
    }
    let counts = |kind: &str| artifacts.iter().filter(|a| a.kind == kind).count();
    let gerber_roles: BTreeSet<_> = artifacts
        .iter()
        .filter(|artifact| artifact.kind == "gerber")
        .filter_map(|artifact| gerber_role(&artifact.path))
        .collect();
    let gerber_complete = gerber_roles.contains("top-copper")
        && gerber_roles.contains("bottom-copper")
        && gerber_roles.contains("profile");
    let mut findings = Vec::new();
    if counts("gerber") == 0 {
        findings.push(finding(
            "package-gerbers-missing",
            Severity::High,
            "Package",
            "No Gerber layer set was recognized",
            "The ZIP contains no recognized RS-274X Gerber files.".into(),
            "Include the authoritative fabrication layer set.",
            "ZIP inventory",
            "package",
        ));
    }
    if selected.is_none() {
        findings.push(finding(
            "package-source-drc-unavailable",
            Severity::Info,
            "DRC coverage",
            "Net-aware source review is unavailable",
            "No supported KiCad source board is packaged.".into(),
            "Include matching KiCad source when source-aware review is required.",
            "ZIP inventory",
            "package",
        ));
    }
    if counts("gerber") > 0 && !gerber_complete {
        findings.push(finding(
            "package-copper-incomplete",
            Severity::Medium,
            "Package",
            "Gerber layer set needs confirmation",
            format!(
                "{} Gerber file(s) were found; top copper {}, bottom copper {}, and profile {}.",
                counts("gerber"),
                if gerber_roles.contains("top-copper") { "identified" } else { "missing" },
                if gerber_roles.contains("bottom-copper") { "identified" } else { "missing" },
                if gerber_roles.contains("profile") { "identified" } else { "missing" }
            ),
            "Include and clearly identify the authoritative copper layers and board profile, or confirm an intentional single-sided build.",
            "ZIP inventory",
            "package",
        ));
    }
    let coverage = vec![
        Coverage {
            id: "package-inventory".into(),
            label: "Archive safety and inventory".into(),
            status: CoverageStatus::Passed,
            evidence: format!(
                "{} recognized files across {} ZIP entries.",
                artifacts.len(),
                zip.len()
            ),
        },
        Coverage {
            id: "package-gerbers".into(),
            label: "Gerber layer set".into(),
            status: if gerber_complete {
                CoverageStatus::Passed
            } else if counts("gerber") > 0 {
                CoverageStatus::Attention
            } else {
                CoverageStatus::NotProvided
            },
            evidence: format!(
                "{} Gerber, {} drill, {} BOM, and {} placement artifacts found.",
                counts("gerber"),
                counts("drill"),
                counts("bom"),
                counts("placement")
            ),
        },
    ];
    let board_source = selected.as_ref().and_then(|s| sources.remove(s));
    let rules = rules_name.and_then(|name| sidecars.remove(&name).map(|source| (name, source)));
    let bom = if boms.len() == 1 {
        boms.pop_first()
    } else {
        None
    };
    Ok(Loaded {
        input_kind: "fabrication-zip".into(),
        board_name: selected,
        board_source,
        artifacts,
        package_findings: findings,
        package_coverage: coverage,
        rules,
        bom,
    })
}

fn kicad_version() -> Option<String> {
    Command::new("kicad-cli")
        .arg("version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|version| !version.is_empty())
}

fn supported_kicad_version(version: &str) -> bool {
    version
        .split(|character: char| !character.is_ascii_digit())
        .find(|part| !part.is_empty())
        .and_then(|part| part.parse::<u32>().ok())
        .is_some_and(|major| major >= 8)
}

fn wait_with_timeout(child: &mut std::process::Child, timeout: Duration) -> Result<i32, Error> {
    let started = std::time::Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| Error::Native(format!("Cannot wait for kicad-cli: {error}")))?
        {
            return Ok(status.code().unwrap_or(3));
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Native(
                "kicad-cli DRC timed out after 120 seconds.".into(),
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn native_drc(board_path: &Path, mode: NativeMode) -> Result<(NativeDrc, Vec<Finding>), Error> {
    if mode == NativeMode::Off {
        return Ok((
            NativeDrc {
                status: "disabled".into(),
                tool: "kicad-cli".into(),
                version: None,
                finding_count: 0,
                note: "Native DRC was disabled by --native off.".into(),
            },
            vec![],
        ));
    }
    let version = kicad_version();
    if version.is_none() {
        if mode == NativeMode::Required {
            return Err(Error::Native(
                "--native required but kicad-cli is unavailable.".into(),
            ));
        }
        return Ok((
            NativeDrc {
                status: "not_run".into(),
                tool: "kicad-cli".into(),
                version: None,
                finding_count: 0,
                note: "kicad-cli is not installed; standalone preflight completed.".into(),
            },
            vec![],
        ));
    }
    if !version.as_deref().is_some_and(supported_kicad_version) {
        if mode == NativeMode::Required {
            return Err(Error::Native(format!(
                "--native required but kicad-cli {} is unsupported; version 8 or newer is required.",
                version.as_deref().unwrap_or("unknown")
            )));
        }
        return Ok((
            NativeDrc {
                status: "not_run".into(),
                tool: "kicad-cli".into(),
                version,
                finding_count: 0,
                note: "Installed kicad-cli is too old for compatible JSON DRC; standalone preflight completed.".into(),
            },
            vec![],
        ));
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_nanos();
    let report_path = std::env::temp_dir().join(format!("ratemypcb-{nonce}.drc.json"));
    let mut child = Command::new("kicad-cli")
        .args([
            "pcb",
            "drc",
            "--format",
            "json",
            "--severity-all",
            "--refill-zones",
            "--exit-code-violations",
            "--output",
        ])
        .arg(&report_path)
        .arg(board_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Error::Native(format!("Cannot run kicad-cli: {e}")))?;
    let exit_code = wait_with_timeout(&mut child, Duration::from_secs(120))?;
    if !matches!(exit_code, 0 | 5) {
        let _ = fs::remove_file(&report_path);
        return Err(Error::Native(format!("kicad-cli exited {exit_code}.")));
    }
    let bytes = fs::read(&report_path)
        .map_err(|e| Error::Native(format!("Cannot read native DRC report: {e}")))?;
    let _ = fs::remove_file(&report_path);
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|e| Error::Native(format!("Invalid native DRC JSON: {e}")))?;
    let mut findings = Vec::new();
    for (group, default) in [
        ("violations", Severity::Medium),
        ("unconnected_items", Severity::High),
        ("schematic_parity", Severity::Medium),
    ] {
        for item in value
            .get(group)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .take(250 - findings.len())
        {
            let raw = item.get("severity").and_then(Value::as_str).unwrap_or("");
            if matches!(raw, "exclusion" | "excluded") {
                continue;
            }
            let severity = match raw {
                "error" => Severity::High,
                "warning" => Severity::Medium,
                _ => default,
            };
            let title = item
                .get("description")
                .or_else(|| item.get("type"))
                .and_then(Value::as_str)
                .unwrap_or("KiCad DRC finding");
            findings.push(finding(
                &format!(
                    "kicad-native-{}-{}",
                    group.replace('_', "-"),
                    findings.len() + 1
                ),
                severity,
                "Native DRC",
                title,
                format!("Reported by KiCad's {group} check."),
                "Open the matching marker in KiCad, correct it, and rerun DRC.",
                "KiCad DRC report",
                "kicad-cli",
            ));
        }
    }
    Ok((
        NativeDrc {
            status: "completed".into(),
            tool: "kicad-cli".into(),
            version,
            finding_count: findings.len(),
            note: "KiCad native DRC completed without modifying the source board.".into(),
        },
        findings,
    ))
}

pub fn review(path: &Path, options: ReviewOptions) -> Result<Report, Error> {
    let loaded = load_path(path, options.board.as_deref())?;
    let (active_preset, imported_rules, rules_name) = loaded
        .rules
        .as_ref()
        .map(|(name, source)| {
            let (preset, count) = resolve_project_rules(options.preset, name, source);
            (preset, count, Some(name.clone()))
        })
        .unwrap_or((options.preset, 0, None));
    let mut findings = loaded.package_findings;
    let mut coverage = loaded.package_coverage;
    coverage.push(Coverage {
        id: "project-rules".into(),
        label: "Project manufacturing rules".into(),
        status: if imported_rules > 0 {
            CoverageStatus::Passed
        } else if rules_name.is_some() {
            CoverageStatus::Attention
        } else {
            CoverageStatus::NotProvided
        },
        evidence: if let Some(name) = rules_name {
            format!("Imported {imported_rules} supported global minimum(s) from {name}.")
        } else {
            "No coherent .kicad_dru or .kicad_pro sidecar was selected; the requested preset was used."
                .into()
        },
    });
    let mut facts = None;
    if let Some(source) = &loaded.board_source {
        let parsed = parse_board(source)?;
        findings.extend(static_findings(&parsed, active_preset));
        coverage.extend([
            Coverage { id: "source-structure".into(), label: "KiCad source structure".into(), status: CoverageStatus::Passed, evidence: format!("{} components, {} nets, {} tracks, and {} vias parsed.", parsed.components, parsed.nets.len(), parsed.tracks.len(), parsed.vias.len()) },
            Coverage { id: "global-minimums".into(), label: "Global manufacturing minimums".into(), status: if findings.iter().any(|f| matches!(f.id.as_str(), "track-width" | "via-diameter" | "via-drill" | "annular-width")) { CoverageStatus::Attention } else { CoverageStatus::Passed }, evidence: "Track width, via diameter, drill, and annular ring were compared with the active preset.".into() },
        ]);
        facts = Some(parsed);
    } else {
        coverage.push(Coverage {
            id: "source-structure".into(),
            label: "KiCad source structure".into(),
            status: CoverageStatus::NotProvided,
            evidence: "No supported KiCad source was selected.".into(),
        });
    }
    if let Some((_name, source)) = &loaded.bom {
        let (bom_findings, bom_coverage) = bom_review(source, facts.as_ref());
        findings.extend(bom_findings);
        coverage.push(bom_coverage);
    } else {
        coverage.push(Coverage {
            id: "bom-structure".into(),
            label: "BOM structure and board correlation".into(),
            status: CoverageStatus::NotProvided,
            evidence: "No unambiguous BOM was selected.".into(),
        });
    }
    let native_board = if path.is_file()
        && path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("kicad_pcb"))
    {
        Some(path.to_path_buf())
    } else if path.is_dir() {
        loaded.board_name.as_ref().map(|b| path.join(b))
    } else {
        None
    };
    let (native, native_findings) = if let Some(board) = native_board {
        native_drc(&board, options.native)?
    } else if options.native == NativeMode::Required {
        return Err(Error::Native(
            "Native DRC requires a directly accessible KiCad board; extract the ZIP first.".into(),
        ));
    } else {
        (
            NativeDrc {
                status: "not_run".into(),
                tool: "kicad-cli".into(),
                version: None,
                finding_count: 0,
                note: "Native DRC cannot run without a directly accessible KiCad source board."
                    .into(),
            },
            vec![],
        )
    };
    findings.extend(native_findings);
    coverage.push(Coverage {
        id: "native-drc".into(),
        label: "Exact clearance and connectivity".into(),
        status: if native.status == "completed" {
            if native.finding_count > 0 {
                CoverageStatus::Attention
            } else {
                CoverageStatus::Passed
            }
        } else {
            CoverageStatus::NotRun
        },
        evidence: native.note.clone(),
    });
    findings.sort_by(|a, b| b.severity.cmp(&a.severity).then_with(|| a.id.cmp(&b.id)));
    let penalty: u16 = findings
        .iter()
        .map(|f| u16::from(f.severity.penalty()))
        .sum();
    let raw = (98_i16 - penalty as i16).clamp(12, 98) as u8;
    let value = f32::from(raw) / 10.0;
    let verdict = if raw >= 90 {
        "Would DFM this — verify the coverage ledger"
    } else if raw >= 75 {
        "Promising board — clear the flagged items"
    } else if raw >= 55 {
        "Revision recommended before ordering"
    } else {
        "Hold fabrication and fix the blockers"
    };
    let confidence = if native.status == "completed" {
        "high"
    } else if facts.is_some() {
        "medium"
    } else {
        "low"
    };
    Ok(Report {
        schema_version: SCHEMA_VERSION.into(), tool: ToolInfo { name: "ratemypcb".into(), version: options.tool_version },
        input: InputInfo { path: path.to_string_lossy().to_string(), kind: loaded.input_kind, selected_board: loaded.board_name }, artifacts: loaded.artifacts,
        score: Score { value, raw, verdict: verdict.into() }, confidence: confidence.into(), coverage, findings, native_drc: native,
        limitations: vec!["Standalone checks do not prove exact copper clearance, connectivity, custom-rule behavior, zone fill, schematic intent, or fabricator stack-up.".into(), "Gerber and drill presence does not prove CAM manufacturability or registration.".into(), "Altium .PcbDoc source-aware DRC is not supported; exported manufacturing artifacts are inventoried only.".into()], disclaimer: DISCLAIMER.into(),
    })
}

pub fn report_schema() -> Value {
    serde_json::json!({
      "$schema": "https://json-schema.org/draft/2020-12/schema", "$id": "https://ratemypcb.com/schemas/report-1.0.json", "title": "RateMyPCB report", "type": "object",
      "required": ["schemaVersion", "tool", "input", "artifacts", "score", "confidence", "coverage", "findings", "nativeDrc", "limitations", "disclaimer"],
      "properties": {
        "schemaVersion": { "const": "1.0" }, "tool": { "type": "object" }, "input": { "type": "object" }, "artifacts": { "type": "array" },
        "score": { "type": "object", "required": ["value", "raw", "verdict"], "properties": { "value": { "type": "number", "minimum": 0, "maximum": 10 }, "raw": { "type": "integer", "minimum": 0, "maximum": 100 }, "verdict": { "type": "string" } } },
        "confidence": { "enum": ["low", "medium", "high"] }, "coverage": { "type": "array" }, "findings": { "type": "array" }, "nativeDrc": { "type": "object" }, "limitations": { "type": "array", "items": { "type": "string" } }, "disclaimer": { "type": "string" }
      }, "additionalProperties": false
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    const CLEAN: &str = r#"(kicad_pcb (version 20240108) (layers (0 "F.Cu" signal) (31 "B.Cu" signal) (44 "Edge.Cuts" user)) (net 1 "GND") (footprint "R" (layer "F.Cu") (property "Reference" "R1") (pad "1" smd rect (net 1 "GND"))) (segment (start 1 1) (end 2 2) (width 0.25) (layer "F.Cu") (net 1)) (zone (net 1) (layer "F.Cu")) (gr_rect (start 0 0) (end 10 10) (layer "Edge.Cuts")))"#;
    #[test]
    fn parses_clean_board() {
        let facts = parse_board(CLEAN).unwrap();
        assert_eq!(facts.tracks, vec![0.25]);
        assert_eq!(facts.edge_forms, 1);
        assert!(static_findings(&facts, Preset::named("standard").unwrap()).is_empty());
    }
    #[test]
    fn flags_narrow_geometry() {
        let facts = parse_board(&CLEAN.replace("0.25", "0.1")).unwrap();
        assert!(
            static_findings(&facts, Preset::named("standard").unwrap())
                .iter()
                .any(|f| f.id == "track-width")
        );
    }
    #[test]
    fn rejects_unsafe_archive_paths() {
        assert!(!safe_archive_path("../secret"));
        assert!(!safe_archive_path("/absolute"));
        assert!(safe_archive_path("fab/top.gtl"));
    }

    #[test]
    fn classifies_core_gerber_roles() {
        assert_eq!(gerber_role("fab/board-F_Cu.gtl"), Some("top-copper"));
        assert_eq!(gerber_role("fab/board-B_Cu.gbl"), Some("bottom-copper"));
        assert_eq!(gerber_role("fab/board-Edge_Cuts.gko"), Some("profile"));
    }
    #[test]
    fn schema_is_versioned() {
        assert_eq!(
            report_schema()["properties"]["schemaVersion"]["const"],
            "1.0"
        );
    }

    #[test]
    fn recognizes_supported_native_versions() {
        assert!(supported_kicad_version("10.0.3"));
        assert!(supported_kicad_version("KiCad 8.0.9"));
        assert!(!supported_kicad_version("7.0.11"));
        assert!(!supported_kicad_version("unknown"));
    }

    #[test]
    fn imports_supported_project_rule_minimums() {
        let base = Preset::named("standard").unwrap();
        let (dru, count) = resolve_project_rules(
            base,
            "board.kicad_dru",
            "(version 1) (rule \"fab\" (constraint track_width (min 0.31mm)) (constraint hole_size (min 0.41mm)))",
        );
        assert_eq!(count, 2);
        assert_eq!(dru.track, 0.31);
        assert_eq!(dru.drill, 0.41);

        let (project, count) = resolve_project_rules(
            base,
            "board.kicad_pro",
            r#"{"board":{"design_settings":{"rules":{"min_track_width":0.25,"min_via_diameter":0.7}}}}"#,
        );
        assert_eq!(count, 2);
        assert_eq!(project.track, 0.25);
        assert_eq!(project.via, 0.7);
    }

    #[test]
    fn parses_and_correlates_bom_rows() {
        let facts = parse_board(CLEAN).unwrap();
        let (clean, coverage) = bom_review("Reference,MPN\nR1,RC0603FR-0710KL\n", Some(&facts));
        assert!(clean.is_empty());
        assert!(matches!(coverage.status, CoverageStatus::Passed));

        let (findings, _) = bom_review("Designator,Value\nR2,10k\n", Some(&facts));
        let ids: BTreeSet<_> = findings.iter().map(|item| item.id.as_str()).collect();
        assert!(ids.contains("bom-mpn-coverage"));
        assert!(ids.contains("bom-missing-references"));
        assert!(ids.contains("bom-unknown-references"));
    }

    #[test]
    fn reference_fixture_matches_javascript_baseline() {
        let board = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/narrow-board.kicad_pcb");
        let report = review(
            &board,
            ReviewOptions {
                board: None,
                preset: Preset::named("standard").unwrap(),
                native: NativeMode::Off,
                tool_version: "test".into(),
            },
        )
        .unwrap();
        let ids: BTreeSet<_> = report
            .findings
            .iter()
            .map(|item| item.id.as_str())
            .collect();
        assert_eq!(report.score.raw, 39);
        assert_eq!(
            ids,
            BTreeSet::from([
                "annular-width",
                "ground-zone",
                "track-width",
                "via-diameter",
                "via-drill"
            ])
        );
    }

    fn temp_test_dir(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("ratemypcb-test-{label}-{nonce}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn directory_requires_explicit_board_when_ambiguous() {
        let root = temp_test_dir("ambiguous");
        fs::write(root.join("a.kicad_pcb"), CLEAN).unwrap();
        fs::write(root.join("b.kicad_pcb"), CLEAN).unwrap();
        let error = load_path(&root, None).err().unwrap();
        assert!(matches!(error, Error::Ambiguous(_)));
        let selected = load_path(&root, Some("b.kicad_pcb")).unwrap();
        assert_eq!(selected.board_name.as_deref(), Some("b.kicad_pcb"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fabrication_zip_is_inventoried_without_extracting() {
        let root = temp_test_dir("zip");
        let archive = root.join("fab.zip");
        let mut writer = zip::ZipWriter::new(File::create(&archive).unwrap());
        writer
            .start_file("fab/board.kicad_pcb", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(CLEAN.as_bytes()).unwrap();
        writer
            .start_file("fab/board-F_Cu.gtl", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"G04 fixture*").unwrap();
        writer.finish().unwrap();
        let loaded = load_path(&archive, None).unwrap();
        assert_eq!(loaded.input_kind, "fabrication-zip");
        assert_eq!(loaded.board_name.as_deref(), Some("fab/board.kicad_pcb"));
        assert_eq!(loaded.artifacts.len(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fabrication_zip_rejects_traversal_paths() {
        let root = temp_test_dir("unsafe-zip");
        let archive = root.join("fab.zip");
        let mut writer = zip::ZipWriter::new(File::create(&archive).unwrap());
        writer
            .start_file("../board.kicad_pcb", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(CLEAN.as_bytes()).unwrap();
        writer.finish().unwrap();
        assert!(matches!(load_path(&archive, None), Err(Error::Invalid(_))));
        fs::remove_dir_all(root).unwrap();
    }
}
