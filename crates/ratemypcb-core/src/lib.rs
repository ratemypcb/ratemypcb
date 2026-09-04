#![recursion_limit = "256"]

mod dfm;
pub mod fabrication;
mod schematic;
pub mod stackup;
mod supply;

pub use dfm::DfmDeclarations;
pub use schematic::{
    NativeMarker as NativeViolation, NativeReport as NativeDrc, SchematicCapability,
    SchematicComparisonSource, SchematicFact, SchematicFootprintComparison, SchematicMismatch,
    SchematicOccurrence, SchematicReview, SchematicSourcePair,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use zip::ZipArchive;

pub const SCHEMA_VERSION: &str = "2.0";
pub const ASSESSMENT_SCHEMA_VERSION: &str = "2.0";
pub const DISCLAIMER: &str = "RateMyPCB is a manufacturing preflight, not a compliance certificate. Confirm results with your fabricator and a qualified engineer.";
const MAX_BOARD_BYTES: u64 = 8 * 1024 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 90 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 256 * 1024 * 1024;
const MAX_ENTRIES: usize = 2_000;
static NATIVE_STAGE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn archive_compressed_size_valid(size: u64) -> bool {
    (1..=MAX_ARCHIVE_BYTES).contains(&size)
}

fn archive_entry_count_valid(entries: usize) -> bool {
    entries <= MAX_ENTRIES
}

fn add_archive_expanded_bytes(total: u64, size: u64) -> Result<u64, Error> {
    total
        .checked_add(size)
        .filter(|expanded| *expanded <= MAX_EXPANDED_BYTES)
        .ok_or_else(|| Error::Invalid("Fabrication ZIP expands beyond 256 MB.".into()))
}

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
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GateImpact {
    #[default]
    Blocking,
    EvidenceOnly,
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
    #[serde(default)]
    pub gate_impact: GateImpact,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoverageStatus {
    Passed,
    Attention,
    NotRun,
    NotProvided,
    Failed,
    Unsupported,
    Stale,
    Unknown,
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
pub struct Score {
    pub value: f32,
    pub raw: u8,
    pub verdict: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceExecution {
    Completed,
    NotRun,
    NotProvided,
    Failed,
    Unsupported,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceResult {
    Pass,
    Attention,
    Fail,
    Unknown,
    NotApplicable,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceFreshness {
    Current,
    Stale,
    #[default]
    Unknown,
    NotApplicable,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EvidenceConfidence {
    Low,
    Medium,
    High,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceProducer {
    pub kind: String,
    pub name: String,
    pub version: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceProvenance {
    pub artifact_id: String,
    pub artifact_digest: String,
    pub producer: EvidenceProducer,
    pub location: BTreeMap<String, String>,
    pub evidence_class: String,
    pub confidence: EvidenceConfidence,
    pub freshness: EvidenceFreshness,
    pub observed_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceRecord {
    pub id: String,
    pub check_id: String,
    pub kind: String,
    pub provenance: EvidenceProvenance,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RequiredEvidence {
    pub check_id: String,
    pub evidence_id: String,
    pub execution: EvidenceExecution,
    pub result: EvidenceResult,
    pub freshness: EvidenceFreshness,
    pub confidence: EvidenceConfidence,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ObservedRisk {
    pub score_raw: u8,
    pub highest_severity: Option<Severity>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub schema_version: String,
    pub tool: ToolInfo,
    pub input: InputInfo,
    pub artifacts: Vec<Artifact>,
    pub score: Score,
    #[serde(default)]
    pub observed_risk: ObservedRisk,
    pub confidence: String,
    #[serde(default)]
    pub evidence_confidence: EvidenceConfidence,
    #[serde(default)]
    pub freshness: EvidenceFreshness,
    #[serde(default)]
    pub required_evidence: Vec<RequiredEvidence>,
    #[serde(default)]
    pub evidence: Vec<EvidenceRecord>,
    pub coverage: Vec<Coverage>,
    pub findings: Vec<Finding>,
    pub native_drc: NativeDrc,
    pub profile_drc: Option<NativeDrc>,
    #[serde(default)]
    pub schematic: SchematicReview,
    #[serde(default)]
    pub fabrication: fabrication::FabricationReview,
    pub review_scope: ReviewScope,
    pub categories: Vec<CategorySummary>,
    pub approval_eligible: bool,
    pub profile: Option<ProfileInfo>,
    #[serde(default)]
    pub bom: BomReport,
    #[serde(default)]
    pub stackup: Option<crate::stackup::Stackup>,
    pub limitations: Vec<String>,
    #[serde(default)]
    pub limitation_evidence_refs: Vec<Vec<String>>,
    pub disclaimer: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BomReport {
    pub status: String,
    pub line_count: usize,
    pub lines: Vec<BomLineReview>,
    #[serde(default)]
    pub supply_legal_expires_at_unix: Option<u64>,
}

impl Default for BomReport {
    fn default() -> Self {
        Self {
            status: "not-provided".into(),
            line_count: 0,
            lines: vec![],
            supply_legal_expires_at_unix: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BomLineReview {
    pub line_number: usize,
    pub references: Vec<String>,
    pub quantity: Option<usize>,
    pub value: Option<String>,
    pub footprint: Option<String>,
    pub manufacturer: Option<String>,
    pub mpn: Option<String>,
    pub identity: BomJudgment,
    pub lifecycle: BomJudgment,
    pub sourceability: BomJudgment,
    pub pricing: BomJudgment,
    pub alternatives: BomJudgment,
    #[serde(default = "not_checked_release_impact")]
    pub release_impact: BomJudgment,
    pub stock: Option<u64>,
    pub moq: Option<u64>,
    pub unit_price: Option<f64>,
    #[serde(default)]
    pub unit_price_decimal: Option<String>,
    pub currency: Option<String>,
    #[serde(default)]
    pub price_estimate: bool,
    pub distributors: Vec<String>,
    pub alternate_mpns: Vec<String>,
    #[serde(default)]
    pub required_quantity: Option<u64>,
    #[serde(default)]
    pub provider_checks: Vec<ProviderCheckReview>,
    #[serde(default)]
    pub offers: Vec<SupplyOfferReview>,
    #[serde(default)]
    pub lifecycle_conflict: bool,
    #[serde(default)]
    pub lifecycle_assertions: Vec<LifecycleReview>,
    #[serde(default)]
    pub alternate_candidates: Vec<AlternateCandidateReview>,
    #[serde(default)]
    pub approved_alternates: Vec<ApprovedAlternateReview>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlternateCandidateReview {
    pub manufacturer: String,
    pub mpn: String,
    pub source: String,
    pub evidence_id: String,
    pub provenance: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovedAlternateReview {
    pub manufacturer: String,
    pub mpn: String,
    pub authority_kind: String,
    pub authority: String,
    pub approved_at_unix: u64,
    pub evidence_refs: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleReview {
    pub provider: String,
    pub raw: String,
    pub normalized: String,
    pub observed_at_unix: u64,
    pub provenance: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCheckReview {
    pub provider: String,
    pub status: String,
    pub error_kind: Option<String>,
    pub retrieved_at_unix: Option<u64>,
    pub upstream_at_unix: Option<u64>,
    pub provenance: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplyOfferReview {
    pub observation_id: String,
    pub provider: String,
    pub seller: String,
    pub seller_original: String,
    pub authorization: String,
    pub sku: String,
    pub packaging: String,
    pub region: String,
    pub stock_status: String,
    pub stock: Option<u64>,
    pub moq: Option<u64>,
    pub order_multiple: Option<u64>,
    pub factory_lead_time_days: Option<u64>,
    pub purchasable_quantity: Option<u64>,
    pub applicable_unit_price: Option<String>,
    pub currency: Option<String>,
    pub retrieved_at_unix: u64,
    pub upstream_at_unix: Option<u64>,
    pub legal_expires_at_unix: u64,
    pub usable: bool,
    pub provenance: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BomJudgment {
    pub status: String,
    pub detail: String,
}

fn not_checked_release_impact() -> BomJudgment {
    BomJudgment {
        status: "not-checked".into(),
        detail: "No authoritative release-impact state was supplied.".into(),
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewScope {
    Design,
    Fabrication,
    Assembly,
    #[default]
    Full,
}

impl ReviewScope {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "design" => Some(Self::Design),
            "fabrication" => Some(Self::Fabrication),
            "assembly" => Some(Self::Assembly),
            "full" => Some(Self::Full),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorySummary {
    pub id: String,
    pub label: String,
    pub status: String,
    pub coverage_ids: Vec<String>,
    pub finding_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInfo {
    pub id: String,
    pub name: String,
    pub source_url: String,
    pub source_retrieved: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Assessment {
    pub assessment_schema_version: String,
    pub report_digest: String,
    pub rating: u8,
    pub disposition: String,
    pub verdict: String,
    #[serde(default)]
    pub verdict_evidence_refs: Vec<String>,
    pub rationale: String,
    #[serde(default)]
    pub category_summaries: Vec<AssessmentCategory>,
    #[serde(default)]
    pub actions: Vec<AssessmentAction>,
    #[serde(default)]
    pub questions: Vec<AssessmentQuestion>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentQuestion {
    pub question: String,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentCategory {
    pub category_id: String,
    pub summary: String,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentAction {
    pub priority: u8,
    pub title: String,
    pub rationale: String,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
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

    pub fn profile(name: &str) -> Option<(Self, ProfileInfo)> {
        let (id, display, source_url, preset) = match name.to_ascii_lowercase().as_str() {
            "eurocircuits" => (
                "eurocircuits",
                "Eurocircuits 6C",
                "https://www.eurocircuits.com/pcb-classification-drill-class/",
                Self {
                    track: 0.15,
                    via: 0.50,
                    drill: 0.25,
                    annular: 0.125,
                },
            ),
            "aisler" => (
                "aisler",
                "AISLER 2-layer Simple",
                "https://github.com/AislerHQ/aisler-support/tree/master/kicad/aisler-2-layer-simple-drc",
                Self {
                    track: 0.20,
                    via: 0.70,
                    drill: 0.30,
                    annular: 0.20,
                },
            ),
            "jlcpcb" => (
                "jlcpcb",
                "JLCPCB 2-layer, 1 oz",
                "https://jlcpcb.com/capabilities/Capabilities?type=1",
                Self {
                    track: 0.10,
                    via: 0.45,
                    drill: 0.20,
                    annular: 0.075,
                },
            ),
            "pcbway" => (
                "pcbway",
                "PCBWay Standard, 1 oz",
                "https://www.pcbway.com/pcb_prototype/",
                Self {
                    track: 0.102,
                    via: 0.40,
                    drill: 0.20,
                    annular: 0.10,
                },
            ),
            _ => return None,
        };
        Some((
            preset,
            ProfileInfo {
                id: id.into(),
                name: display.into(),
                source_url: source_url.into(),
                source_retrieved: "2026-08-19".into(),
            },
        ))
    }
}

#[derive(Clone, Debug)]
pub struct ReviewOptions {
    pub board: Option<String>,
    pub schematic: Option<String>,
    pub bom: Option<PathBuf>,
    pub placement: Option<PathBuf>,
    pub supply_snapshot: Option<PathBuf>,
    pub dfm_declarations: Option<DfmDeclarations>,
    pub preset: Preset,
    pub native: NativeMode,
    pub tool_version: String,
    pub scope: ReviewScope,
    pub profile: Option<String>,
}

#[derive(Default)]
struct BoardFacts {
    format_version: Option<u32>,
    tracks: Vec<f64>,
    vias: Vec<(f64, f64)>,
    nets: BTreeMap<u32, String>,
    pad_nets: BTreeMap<u32, usize>,
    routed_nets: BTreeSet<u32>,
    zone_nets: BTreeSet<u32>,
    edge_forms: usize,
    components: usize,
    pads: usize,
    references: BTreeSet<String>,
    placement_references: BTreeSet<String>,
    mask_issue_refs: BTreeSet<String>,
    paste_issue_refs: BTreeSet<String>,
}

pub(crate) fn forms<'a>(source: &'a str, name: &str) -> Vec<&'a str> {
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
fn pair(form: &str, field: &str) -> Option<(f64, f64)> {
    let start = form.find(&format!("({field} "))? + field.len() + 2;
    let mut values = form[start..].split_whitespace();
    Some((
        values.next()?.parse().ok()?,
        values.next()?.trim_end_matches(')').parse().ok()?,
    ))
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

fn has_layer(form: &str, name: &str) -> bool {
    form.split(|character: char| character.is_whitespace() || matches!(character, '(' | ')' | '"'))
        .any(|token| token == name)
}

fn property_value(form: &str, property: &str) -> Option<String> {
    let needle = format!("(property \"{property}\" \"");
    let start = form.find(&needle)? + needle.len();
    let end = form[start..].find('"')?;
    Some(form[start..start + end].to_string())
}

fn reference_value(form: &str) -> Option<String> {
    property_value(form, "Reference").or_else(|| {
        let needle = "(fp_text reference \"";
        let start = form.find(needle)? + needle.len();
        let end = form[start..].find('"')?;
        Some(form[start..start + end].to_string())
    })
}

fn assembly_reference(value: &str) -> Option<String> {
    let reference = value.trim().to_ascii_uppercase();
    if reference.is_empty() || matches!(reference.as_str(), "." | "?") || reference.contains('*') {
        return None;
    }
    let prefix = reference
        .chars()
        .take_while(|character| !character.is_ascii_digit())
        .collect::<String>();
    if matches!(
        prefix.as_str(),
        "TP" | "H" | "MH" | "HOLE" | "MTG" | "FID" | "FD" | "TOOL"
    ) {
        None
    } else {
        Some(reference)
    }
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
    let mut facts = BoardFacts {
        format_version: integer(source, "version"),
        ..BoardFacts::default()
    };
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
    let setup = forms(source, "setup")
        .into_iter()
        .next()
        .unwrap_or_default();
    let board_mask_margin = scalar(setup, "pad_to_mask_clearance").unwrap_or(0.0);
    let board_paste_margin = scalar(setup, "pad_to_paste_clearance").unwrap_or(0.0);
    for footprint in footprints {
        let settings = footprint.split("(pad").next().unwrap_or(footprint);
        let footprint_mask_margin =
            scalar(settings, "solder_mask_margin").unwrap_or(board_mask_margin);
        let footprint_paste_margin =
            scalar(settings, "solder_paste_margin").unwrap_or(board_paste_margin);
        let reference = reference_value(footprint)
            .map(|reference| reference.trim().to_ascii_uppercase())
            .filter(|reference| {
                !reference.is_empty()
                    && !matches!(reference.as_str(), "." | "?")
                    && !reference.contains('*')
            })
            .unwrap_or_else(|| "unreferenced footprint".into());
        if reference != "unreferenced footprint" {
            facts.references.insert(reference.clone());
            if !settings.contains("exclude_from_pos_files") && !settings.contains(" dnp") {
                if let Some(reference) = assembly_reference(&reference) {
                    facts.placement_references.insert(reference);
                }
            }
        }
        for pad in forms(footprint, "pad") {
            facts.pads += 1;
            let smd = pad.split_whitespace().nth(2) == Some("smd");
            let through_hole = pad.split_whitespace().nth(2) == Some("thru_hole");
            let front_copper = has_layer(pad, "F.Cu");
            let back_copper = has_layer(pad, "B.Cu");
            let front_mask = has_layer(pad, "F.Mask") || has_layer(pad, "*.Mask");
            let back_mask = has_layer(pad, "B.Mask") || has_layer(pad, "*.Mask");
            let front_paste = has_layer(pad, "F.Paste") || has_layer(pad, "*.Paste");
            let back_paste = has_layer(pad, "B.Paste") || has_layer(pad, "*.Paste");
            let size = pair(pad, "size");
            let collapses = |margin: f64| {
                size.is_some_and(|(x, y)| x + 2.0 * margin <= 0.0 || y + 2.0 * margin <= 0.0)
            };
            if smd
                && ((front_copper && !front_mask)
                    || (back_copper && !back_mask)
                    || collapses(
                        scalar(pad, "solder_mask_margin").unwrap_or(footprint_mask_margin),
                    ))
            {
                facts.mask_issue_refs.insert(reference.clone());
            }
            if (smd && ((front_copper && !front_paste) || (back_copper && !back_paste)))
                || (through_hole && (front_paste || back_paste))
                || ((front_paste || back_paste)
                    && collapses(
                        scalar(pad, "solder_paste_margin").unwrap_or(footprint_paste_margin),
                    ))
            {
                facts.paste_issue_refs.insert(reference.clone());
            }
        }
    }
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
        gate_impact: GateImpact::Blocking,
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
        .filter(|(size, drill)| (*size - *drill) / 2.0 + f64::EPSILON < preset.annular)
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
    if !facts.mask_issue_refs.is_empty() {
        out.push(finding(
            "solder-mask-configuration",
            Severity::Low,
            "Solder mask",
            "Some SMD pads have suspicious solder-mask apertures",
            format!(
                "Affected footprint reference(s): {}.",
                facts
                    .mask_issue_refs
                    .iter()
                    .take(12)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "Verify intentional tenting or maskless pads, and correct any non-positive aperture dimensions.",
            "Footprints",
            "static",
        ));
    }
    if !facts.paste_issue_refs.is_empty() {
        out.push(finding(
            "solder-paste-configuration",
            Severity::Low,
            "Solder paste",
            "Some pads have suspicious solder-paste apertures",
            format!(
                "Affected footprint reference(s): {}.",
                facts
                    .paste_issue_refs
                    .iter()
                    .take(12)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "Verify intentional paste omissions or pin-in-paste use and inspect the plotted stencil apertures.",
            "Footprints",
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
    } else if lower.ends_with(".kicad_sch") {
        Some(("schematic", "kicad"))
    } else if lower.ends_with(".schdoc") {
        Some(("schematic", "altium-inventory"))
    } else if lower.ends_with(".kicad_pro") {
        Some(("settings", "kicad"))
    } else if lower.ends_with(".kicad_prl") {
        Some(("settings", "kicad-private"))
    } else if lower.ends_with(".kicad_sym") || lower.ends_with("sym-lib-table") {
        Some(("library", "kicad"))
    } else if lower.ends_with(".net") {
        Some(("netlist", "generic"))
    } else if lower.ends_with(".xml") {
        Some(("netlist", "xml"))
    } else if lower.ends_with(".kicad_dru") {
        Some(("rules", "kicad"))
    } else if lower.ends_with(".gbrjob") {
        Some(("gerber-job", "gerber-job-2023.06"))
    } else if [
        ".gtl", ".gbl", ".gbr", ".ger", ".gko", ".gm1", ".gts", ".gbs", ".gto", ".gbo", ".gtp",
        ".gbp",
    ]
    .iter()
    .any(|e| lower.ends_with(e))
    {
        Some(("gerber", "rs-274x"))
    } else if [".drl", ".xln", ".exc", ".xnc"]
        .iter()
        .any(|e| lower.ends_with(e))
    {
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
    if name.ends_with(".gtl")
        || name.contains("f_cu")
        || name.contains("front-cu")
        || name.contains("cutop")
    {
        Some("top-copper")
    } else if name.ends_with(".gbl")
        || name.contains("b_cu")
        || name.contains("back-cu")
        || name.contains("cubottom")
    {
        Some("bottom-copper")
    } else if name.ends_with(".gko")
        || name.ends_with(".gm1")
        || name.contains("edge-cuts")
        || name.contains("edge_cuts")
        || name.contains("edgecuts")
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
        && path.len() <= fabrication::MANUFACTURING_LIMITS.normalized_path_bytes
        && path.split('/').count().saturating_sub(1)
            <= usize::from(fabrication::MANUFACTURING_LIMITS.directory_depth)
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

fn collect_hidden_manufacturing(
    root: &Path,
    output: &mut Vec<PathBuf>,
    depth: usize,
) -> Result<(), Error> {
    if depth > usize::from(fabrication::MANUFACTURING_LIMITS.directory_depth) {
        return Err(Error::Invalid(format!(
            "Project path exceeds the {} directory-level limit: {}",
            fabrication::MANUFACTURING_LIMITS.directory_depth,
            root.display()
        )));
    }
    for entry in fs::read_dir(root)
        .map_err(|error| Error::Invalid(format!("Cannot read {}: {error}", root.display())))?
    {
        let entry = entry.map_err(|error| Error::Invalid(error.to_string()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| Error::Invalid(error.to_string()))?;
        if file_type.is_dir() {
            collect_hidden_manufacturing(&path, output, depth + 1)?;
        } else if file_type.is_file()
            && classify(&path.to_string_lossy())
                .is_some_and(|(kind, _)| manufacturing_kind(kind).is_some())
        {
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

fn collect_dir(root: &Path, output: &mut Vec<PathBuf>, depth: usize) -> Result<(), Error> {
    if depth > usize::from(fabrication::MANUFACTURING_LIMITS.directory_depth) {
        return Err(Error::Invalid(format!(
            "Project path exceeds the {} directory-level limit: {}",
            fabrication::MANUFACTURING_LIMITS.directory_depth,
            root.display()
        )));
    }
    let entries = fs::read_dir(root)
        .map_err(|e| Error::Invalid(format!("Cannot read {}: {e}", root.display())))?;
    for entry in entries {
        let entry = entry.map_err(|e| Error::Invalid(e.to_string()))?;
        let path = entry.path();
        let name = entry.file_name();
        if matches!(name.to_str(), Some("node_modules" | "target" | "vendor")) {
            continue;
        }
        let ty = entry
            .file_type()
            .map_err(|e| Error::Invalid(e.to_string()))?;
        if ty.is_dir() && name.to_string_lossy().starts_with('.') {
            collect_hidden_manufacturing(&path, output, depth + 1)?;
        } else if ty.is_dir() {
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
    project_root: Option<PathBuf>,
    board_name: Option<String>,
    board_source: Option<String>,
    schematics: BTreeMap<String, String>,
    schematic_root_hint: Option<String>,
    projects: BTreeSet<String>,
    project_variables: BTreeMap<String, String>,
    altium_schematics: Vec<String>,
    netlists: BTreeMap<String, String>,
    artifacts: Vec<Artifact>,
    package_findings: Vec<Finding>,
    package_coverage: Vec<Coverage>,
    rules: Option<(String, String)>,
    bom: Option<(String, String)>,
    placement: Option<(String, String)>,
    manufacturing: fabrication::ManufacturingInventory,
    manufacturing_deadline: fabrication::ManufacturingDeadline,
}

fn manufacturing_kind(kind: &str) -> Option<fabrication::ManufacturingKindCandidate> {
    match kind {
        "gerber" => Some(fabrication::ManufacturingKindCandidate::Gerber),
        "drill" => Some(fabrication::ManufacturingKindCandidate::Excellon),
        "gerber-job" => Some(fabrication::ManufacturingKindCandidate::GerberJob),
        _ => None,
    }
}

fn manufacturing_limit_reason(
    recognized_index: usize,
    size: u64,
    retained_bytes: u64,
) -> Option<fabrication::ManufacturingLoadReason> {
    if recognized_index >= fabrication::MANUFACTURING_LIMITS.recognized_files {
        Some(fabrication::ManufacturingLoadReason::RecognizedFileLimit)
    } else if size > fabrication::MANUFACTURING_LIMITS.raw_bytes_per_file {
        Some(fabrication::ManufacturingLoadReason::PerFileByteLimit)
    } else if retained_bytes
        .checked_add(size)
        .is_none_or(|total| total > fabrication::MANUFACTURING_LIMITS.raw_bytes_aggregate)
    {
        Some(fabrication::ManufacturingLoadReason::AggregateByteLimit)
    } else {
        None
    }
}

fn read_manufacturing_bytes(
    reader: &mut impl Read,
    name: &str,
    deadline: fabrication::ManufacturingDeadline,
) -> Result<(Vec<u8>, String), Error> {
    let limit = fabrication::MANUFACTURING_LIMITS.raw_bytes_per_file;
    let mut bytes = Vec::new();
    let mut hasher = Sha256::new();
    let mut chunk = [0_u8; 8192];
    loop {
        deadline.check("manufacturing-input-read").map_err(|_| {
            Error::Invalid(format!(
                "Manufacturing input exceeded the read deadline: {name}"
            ))
        })?;
        let retained = u64::try_from(bytes.len())
            .map_err(|_| Error::Invalid(format!("{name} size overflowed.")))?;
        let remaining = limit.saturating_add(1).saturating_sub(retained);
        if remaining == 0 {
            break;
        }
        let capacity = chunk
            .len()
            .min(usize::try_from(remaining).unwrap_or(chunk.len()));
        let read = reader.read(&mut chunk[..capacity]).map_err(|error| {
            Error::Invalid(format!("Cannot read manufacturing bytes {name}: {error}"))
        })?;
        if read == 0 {
            break;
        }
        deadline.check("manufacturing-input-read").map_err(|_| {
            Error::Invalid(format!(
                "Manufacturing input exceeded the read deadline: {name}"
            ))
        })?;
        hasher.update(&chunk[..read]);
        bytes.extend_from_slice(&chunk[..read]);
    }
    if bytes.len() as u64 > limit {
        return Err(Error::Invalid(format!(
            "Manufacturing input changed beyond its declared bounded size: {name}"
        )));
    }
    deadline.check("manufacturing-input-hash").map_err(|_| {
        Error::Invalid(format!(
            "Manufacturing input exceeded the read deadline: {name}"
        ))
    })?;
    Ok((bytes, format!("{:x}", hasher.finalize())))
}

fn manufacturing_outcome(
    virtual_path: String,
    kind_candidate: fabrication::ManufacturingKindCandidate,
    size: u64,
    artifact_digest: Option<String>,
    reason: Option<fabrication::ManufacturingLoadReason>,
) -> fabrication::ManufacturingInputOutcome {
    let state = if reason.is_some() {
        fabrication::ManufacturingLoadState::Omitted
    } else {
        fabrication::ManufacturingLoadState::Retained
    };
    fabrication::ManufacturingInputOutcome {
        id: fabrication::input_outcome_id(
            &virtual_path,
            artifact_digest.as_deref(),
            kind_candidate,
        ),
        virtual_path,
        artifact_digest,
        kind_candidate,
        size,
        state,
        reason,
    }
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

fn split_designator(value: &str) -> Option<(&str, u32)> {
    let number = value.find(|character: char| character.is_ascii_digit())?;
    let (prefix, suffix) = value.split_at(number);
    if prefix.is_empty() || !suffix.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }
    Some((prefix, suffix.parse().ok()?))
}

fn expand_reference_token(value: &str) -> Vec<String> {
    let normalized = value
        .trim_matches(|character: char| matches!(character, '"' | '\'' | '(' | ')'))
        .replace(['–', '—'], "-")
        .replace("...", "-")
        .to_ascii_uppercase();
    let Some((start, end)) = normalized.split_once('-') else {
        return (!normalized.is_empty())
            .then_some(normalized)
            .into_iter()
            .collect();
    };
    let (Some((start_prefix, start_number)), Some((end_prefix, end_number))) =
        (split_designator(start), split_designator(end))
    else {
        return vec![normalized];
    };
    if start_prefix != end_prefix || start_number > end_number || end_number - start_number > 10_000
    {
        return vec![normalized];
    }
    (start_number..=end_number)
        .map(|number| format!("{start_prefix}{number}"))
        .collect()
}

fn optional_field(fields: &[String], index: Option<usize>) -> Option<String> {
    index
        .and_then(|index| fields.get(index))
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn parse_bom_lines(source: &str) -> Vec<BomLineReview> {
    let mut lines = source
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty());
    let Some((_, header)) = lines.next() else {
        return vec![];
    };
    let delimiter = delimiter_for(header);
    let headers = normalized_headers(header, delimiter);
    let index = |names: &[&str]| {
        headers
            .iter()
            .position(|header| names.contains(&header.as_str()))
    };
    let Some(reference_index) = index(&[
        "reference",
        "references",
        "ref",
        "refs",
        "designator",
        "designators",
    ]) else {
        return vec![];
    };
    let quantity_index = index(&["quantity", "qty"]);
    let value_index = index(&["value", "comment", "description"]);
    let footprint_index = index(&["footprint", "package", "pcbfootprint"]);
    let manufacturer_index = index(&["manufacturer", "mfr", "brand"]);
    let mpn_index = index(&["mpn", "manufacturerpartnumber", "partnumber"]);
    let alternate_index = index(&[
        "alternate",
        "alternates",
        "alternatempn",
        "alternatempns",
        "alternatemanufacturerpartnumber",
    ]);
    lines
        .filter_map(|(line_number, line)| {
            let fields = split_delimited(line, delimiter);
            if fields.iter().all(|field| field.is_empty()) {
                return None;
            }
            let references = fields
                .get(reference_index)
                .into_iter()
                .flat_map(|value| {
                    value.split(|character: char| {
                        character.is_whitespace() || matches!(character, ',' | ';')
                    })
                })
                .flat_map(expand_reference_token)
                .collect::<Vec<_>>();
            let quantity = if quantity_index.is_some() {
                optional_field(&fields, quantity_index)
                    .and_then(|quantity| quantity.parse::<usize>().ok())
            } else {
                (!references.is_empty()).then_some(references.len())
            };
            let manufacturer = optional_field(&fields, manufacturer_index);
            let mpn = optional_field(&fields, mpn_index);
            let alternate_mpns = optional_field(&fields, alternate_index)
                .map(|value| {
                    value
                        .split(|character: char| matches!(character, ',' | ';' | '|') || character.is_whitespace())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let identity = match (&manufacturer, &mpn) {
                (Some(_), Some(_)) => BomJudgment { status: "pass".into(), detail: "Exact manufacturer and part number are present in the BOM.".into() },
                (None, Some(_)) => BomJudgment { status: "attention".into(), detail: "A part number is present, but its manufacturer is not identified.".into() },
                (_, None) => BomJudgment { status: "attention".into(), detail: "No exact manufacturer part number is present.".into() },
            };
            let release_impact = if identity.status == "attention" {
                BomJudgment { status: "attention".into(), detail: "Part identity needs release attention.".into() }
            } else {
                BomJudgment { status: "not-checked".into(), detail: "Lifecycle, availability, commercial, and alternate evidence is incomplete.".into() }
            };
            Some(BomLineReview {
                line_number: line_number + 1,
                references,
                quantity,
                value: optional_field(&fields, value_index),
                footprint: optional_field(&fields, footprint_index),
                manufacturer,
                mpn,
                identity,
                lifecycle: BomJudgment { status: "not-checked".into(), detail: "No exact lifecycle evidence was joined to this BOM line.".into() },
                sourceability: BomJudgment { status: "not-checked".into(), detail: "No exact stock evidence was joined to this BOM line.".into() },
                pricing: BomJudgment { status: "not-checked".into(), detail: "No pricing evidence was supplied.".into() },
                alternatives: BomJudgment {
                    status: "not-checked".into(),
                    detail: if alternate_mpns.is_empty() {
                        "No approved alternate evidence was supplied.".into()
                    } else {
                        "BOM-declared MPN suggestions lack exact manufacturer identity, engineering authority, and evidence.".into()
                    },
                },
                release_impact,
                stock: None,
                moq: None,
                unit_price: None,
                unit_price_decimal: None,
                currency: None,
                price_estimate: false,
                distributors: vec![],
                alternate_mpns,
                required_quantity: None,
                provider_checks: vec![],
                offers: vec![],
                lifecycle_conflict: false,
                lifecycle_assertions: vec![],
                alternate_candidates: vec![],
                approved_alternates: vec![],
            })
        })
        .collect()
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
    let manufacturer_index = headers
        .iter()
        .position(|header| matches!(header.as_str(), "manufacturer" | "mfr" | "brand"));
    let Some(_reference_index) = reference_index else {
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
    let mut rows_without_manufacturer = 0;
    let mut quantity_mismatches = 0;
    let mut duplicates = BTreeSet::new();
    let parsed_lines = parse_bom_lines(source);
    for line in &parsed_lines {
        rows += 1;
        if line.mpn.is_none() {
            rows_without_mpn += 1;
        }
        if line.manufacturer.is_none() {
            rows_without_manufacturer += 1;
        }
        for reference in &line.references {
            if !references.insert(reference.clone()) {
                duplicates.insert(reference.clone());
            }
        }
        if line.quantity != Some(line.references.len()) {
            quantity_mismatches += 1;
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
    if manufacturer_index.is_none() || rows_without_manufacturer > 0 {
        findings.push(finding("bom-manufacturer-coverage", Severity::Medium, "BOM", "BOM manufacturer identity is incomplete", format!("{rows_without_manufacturer} of {rows} parsed row(s) have no recognized manufacturer."), "Add the exact manufacturer for every fitted purchased component.", "BOM", "bom"));
    }
    if quantity_mismatches > 0 {
        findings.push(finding("bom-quantity-mismatch", Severity::High, "BOM", "BOM quantities do not match grouped references", format!("{quantity_mismatches} row(s) have a quantity different from their parsed designator count."), "Regenerate grouped quantities from the matching design revision.", "BOM", "bom"));
    }
    if !duplicates.is_empty() {
        findings.push(finding(
            "bom-duplicate-references",
            Severity::High,
            "BOM",
            "BOM contains duplicate designators",
            duplicates
                .into_iter()
                .take(12)
                .collect::<Vec<_>>()
                .join(", "),
            "Assign every fitted designator to exactly one BOM row.",
            "BOM",
            "bom",
        ));
    }
    if let Some(board) = board {
        let board_refs: BTreeSet<_> = board
            .placement_references
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

fn normalized_headers(line: &str, delimiter: char) -> Vec<String> {
    split_delimited(line, delimiter)
        .into_iter()
        .map(|header| header.to_ascii_lowercase().replace([' ', '_', '-'], ""))
        .collect()
}

fn delimiter_for(line: &str) -> char {
    ['\t', ',', ';']
        .into_iter()
        .max_by_key(|delimiter| line.matches(*delimiter).count())
        .unwrap_or(',')
}

fn placement_review(source: &str, board: Option<&BoardFacts>) -> (Vec<Finding>, Coverage) {
    let mut rows = source.lines().filter(|line| !line.trim().is_empty());
    let Some(header) = rows.next() else {
        return (
            vec![finding(
                "placement-empty",
                Severity::Medium,
                "Placement",
                "Placement file is empty",
                "No placement rows were found.".into(),
                "Export fitted component placement data for this board revision.",
                "Placement",
                "placement",
            )],
            Coverage {
                id: "placement-structure".into(),
                label: "Placement structure and board correlation".into(),
                status: CoverageStatus::Attention,
                evidence: "The selected placement file was empty.".into(),
            },
        );
    };
    let delimiter = delimiter_for(header);
    let headers = normalized_headers(header, delimiter);
    let index = |names: &[&str]| {
        headers
            .iter()
            .position(|item| names.contains(&item.as_str()))
    };
    let reference_index = index(&["reference", "ref", "designator"]);
    let required = [
        reference_index,
        index(&["posx", "x", "midx"]),
        index(&["posy", "y", "midy"]),
        index(&["rotation", "rot"]),
        index(&["side", "layer"]),
    ];
    let mut findings = Vec::new();
    if required.iter().any(Option::is_none) {
        findings.push(finding(
            "placement-columns-missing",
            Severity::Medium,
            "Placement",
            "Placement columns are incomplete",
            "Reference, X, Y, rotation, and side columns are required.".into(),
            "Export a standard centroid/position file with all required fields.",
            "Placement",
            "placement",
        ));
    }
    let mut refs = BTreeSet::new();
    if let Some(reference_index) = reference_index {
        for row in rows.take(10_000) {
            let fields = split_delimited(row, delimiter);
            if let Some(reference) = fields
                .get(reference_index)
                .and_then(|value| assembly_reference(value))
            {
                refs.insert(reference);
            }
        }
    }
    if let Some(board) = board {
        let missing: Vec<_> = board
            .placement_references
            .difference(&refs)
            .take(12)
            .cloned()
            .collect();
        let unknown: Vec<_> = refs
            .difference(&board.placement_references)
            .take(12)
            .cloned()
            .collect();
        if !missing.is_empty() || !unknown.is_empty() {
            findings.push(finding(
                "placement-board-mismatch",
                Severity::Medium,
                "Placement",
                "Placement references do not match the board",
                format!("Missing: {}; unknown: {}.", missing.join(", "), unknown.join(", ")),
                "Regenerate placement data from the matching board and confirm excluded/DNP components.",
                "Placement",
                "placement",
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
            id: "placement-structure".into(),
            label: "Placement structure and board correlation".into(),
            status,
            evidence: format!("{} unique placement reference(s) were parsed.", refs.len()),
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

fn coherent_bom(board: &str, candidates: impl Iterator<Item = String>) -> Option<String> {
    let board_path = Path::new(board);
    let board_parent = board_path.parent().unwrap_or_else(|| Path::new(""));
    let board_stem = board_path
        .file_stem()?
        .to_string_lossy()
        .to_ascii_lowercase();
    let candidates: Vec<_> = candidates.collect();
    let same_directory: Vec<_> = candidates
        .iter()
        .filter(|candidate| {
            Path::new(candidate)
                .parent()
                .unwrap_or_else(|| Path::new(""))
                == board_parent
        })
        .cloned()
        .collect();
    let exact: Vec<_> = same_directory
        .iter()
        .filter(|candidate| {
            let stem = Path::new(candidate)
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            let normalized = stem
                .strip_suffix("-bom")
                .or_else(|| stem.strip_suffix("_bom"))
                .or_else(|| stem.strip_suffix(".bom"))
                .unwrap_or(&stem);
            normalized == board_stem
        })
        .cloned()
        .collect();
    if exact.len() == 1 {
        exact.into_iter().next()
    } else if same_directory.len() == 1 {
        same_directory.into_iter().next()
    } else if candidates.len() == 1 {
        candidates.into_iter().next()
    } else {
        None
    }
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
        if candidates
            .iter()
            .any(|candidate| candidate.replace('\\', "/") == normalized)
        {
            return Ok(Some(normalized));
        }
        let matches: Vec<_> = candidates
            .iter()
            .filter(|p| {
                Path::new(p).file_name().and_then(|n| n.to_str())
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
    let manufacturing_started = Instant::now();
    let manufacturing_deadline =
        fabrication::ManufacturingDeadline::from_aggregate_start(manufacturing_started);
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
        let bom_name = selected
            .as_deref()
            .and_then(|board| coherent_bom(board, bom_names.iter().cloned()))
            .or_else(|| (selected.is_none() && bom_names.len() == 1).then(|| bom_names[0].clone()));
        let bom = if let Some(name) = &bom_name {
            Some((
                name.clone(),
                fs::read_to_string(path.join(name))
                    .map_err(|error| Error::Invalid(format!("Cannot read {name}: {error}")))?,
            ))
        } else {
            None
        };
        let placement_names: Vec<_> = names
            .iter()
            .filter(|name| classify(name).is_some_and(|(kind, _)| kind == "placement"))
            .cloned()
            .collect();
        let placement_name = selected
            .as_deref()
            .and_then(|board| coherent_bom(board, placement_names.iter().cloned()))
            .or_else(|| {
                (selected.is_none() && placement_names.len() == 1)
                    .then(|| placement_names[0].clone())
            });
        let placement = placement_name
            .as_ref()
            .map(|name| {
                fs::read_to_string(path.join(name))
                    .map(|source| (name.clone(), source))
                    .map_err(|error| Error::Invalid(format!("Cannot read {name}: {error}")))
            })
            .transpose()?;
        let project_count = names
            .iter()
            .filter(|name| name.to_ascii_lowercase().ends_with(".kicad_pro"))
            .count();
        let mut schematics = BTreeMap::new();
        let mut projects = BTreeSet::new();
        let mut project_variables = BTreeMap::new();
        let mut altium_schematics = Vec::new();
        let mut netlists = BTreeMap::new();
        let mut schematic_bytes = 0_u64;
        for (file, name) in files.iter().zip(names.iter()) {
            let Some((kind, format)) = classify(name) else {
                continue;
            };
            if kind == "settings" && format == "kicad" {
                projects.insert(name.clone());
                let selected_context =
                    rules_name.as_ref() == Some(name) || (selected.is_none() && project_count == 1);
                if selected_context
                    && let Ok(value) =
                        serde_json::from_slice::<Value>(&fs::read(file).unwrap_or_default())
                {
                    if let Some(variables) = value.get("text_variables").and_then(Value::as_object)
                    {
                        for (key, value) in variables {
                            if let Some(value) = value.as_str().filter(|value| value.len() <= 4096)
                            {
                                project_variables.insert(key.clone(), value.into());
                            }
                        }
                    }
                }
            } else if kind == "schematic" && format == "altium-inventory" {
                altium_schematics.push(name.clone());
            } else if kind == "schematic" || kind == "netlist" {
                let size = fs::metadata(file)
                    .map_err(|error| Error::Invalid(error.to_string()))?
                    .len();
                if size == 0 || size > 2 * 1024 * 1024 || schematic_bytes + size > 8 * 1024 * 1024 {
                    if kind == "schematic" {
                        schematics.insert(name.clone(), String::new());
                    } else {
                        netlists.insert(name.clone(), String::new());
                    }
                    continue;
                }
                let source = fs::read_to_string(file).map_err(|_| {
                    Error::Invalid(format!("{name} is not bounded UTF-8 EDA text."))
                })?;
                schematic_bytes += size;
                if kind == "schematic" {
                    schematics.insert(name.clone(), source);
                } else {
                    netlists.insert(name.clone(), source);
                }
            }
        }
        let mut manufacturing = fabrication::ManufacturingInventory {
            aggregate_started: Some(manufacturing_started),
            ..fabrication::ManufacturingInventory::default()
        };
        let mut retained_manufacturing_bytes = 0_u64;
        for (file, name) in files.iter().zip(names.iter()) {
            if manufacturing_deadline
                .check("manufacturing-input-read")
                .is_err()
            {
                return Err(Error::Invalid(
                    "Manufacturing inputs exceeded the aggregate read deadline.".into(),
                ));
            }
            let Some((kind, _)) = classify(name) else {
                continue;
            };
            let Some(kind_candidate) = manufacturing_kind(kind) else {
                continue;
            };
            if !safe_archive_path(name) {
                return Err(Error::Invalid(format!(
                    "Manufacturing input has an unsafe or over-limit virtual path: {name}"
                )));
            }
            let declared_size = fs::metadata(file)
                .map_err(|error| Error::Invalid(error.to_string()))?
                .len();
            let reason = manufacturing_limit_reason(
                manufacturing.outcomes.len(),
                declared_size,
                retained_manufacturing_bytes,
            );
            let file_started = Instant::now();
            let (artifact_digest, actual_size, bytes) = if reason.is_none() {
                let mut file = File::open(file)
                    .map_err(|error| Error::Invalid(format!("Cannot read {name}: {error}")))?;
                let (bytes, digest) = read_manufacturing_bytes(
                    &mut file,
                    name,
                    manufacturing_deadline.for_file_started(file_started),
                )?;
                let actual_size = u64::try_from(bytes.len())
                    .map_err(|_| Error::Invalid(format!("{name} size overflowed.")))?;
                (Some(digest), actual_size, Some(bytes))
            } else {
                (None, declared_size, None)
            };
            if actual_size != declared_size {
                return Err(Error::Invalid(format!(
                    "Manufacturing input changed while being read: {name}"
                )));
            }
            let outcome = manufacturing_outcome(
                name.clone(),
                kind_candidate,
                actual_size,
                artifact_digest.clone(),
                reason,
            );
            if let Some(original_bytes) = bytes {
                retained_manufacturing_bytes += actual_size;
                manufacturing.inputs.push(fabrication::ManufacturingInput {
                    virtual_path: name.clone(),
                    artifact_digest: artifact_digest
                        .expect("retained manufacturing input has a digest"),
                    kind_candidate,
                    size: actual_size,
                    original_bytes,
                    file_started: Some(file_started),
                });
            }
            manufacturing.outcomes.push(outcome);
        }
        manufacturing
            .validate_with_deadline(manufacturing_deadline)
            .map_err(|error| Error::Invalid(format!("Invalid manufacturing inventory: {error}")))?;
        let artifacts = names
            .into_iter()
            .filter_map(|name| {
                classify(&name).map(|(kind, format)| Artifact {
                    selected: selected.as_ref() == Some(&name)
                        || rules_name.as_ref() == Some(&name)
                        || bom_name.as_ref() == Some(&name)
                        || placement_name.as_ref() == Some(&name),
                    path: name,
                    kind: kind.into(),
                    format: format.into(),
                })
            })
            .collect();
        return Ok(Loaded {
            input_kind: "directory".into(),
            project_root: Some(path.to_path_buf()),
            board_name: selected,
            board_source: source,
            schematics,
            schematic_root_hint: None,
            projects,
            project_variables,
            altium_schematics,
            netlists,
            artifacts,
            package_findings: vec![],
            package_coverage: vec![],
            rules,
            bom,
            placement,
            manufacturing,
            manufacturing_deadline,
        });
    }
    if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
    {
        return load_zip(
            path,
            selector,
            manufacturing_started,
            manufacturing_deadline,
        );
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("board.kicad_pcb")
        .to_string();
    let Some((kind, format)) = classify(&name) else {
        return Err(Error::Invalid(
            "Review a directory, supported KiCad board/schematic, .SchDoc, generic netlist, or fabrication .zip.".into(),
        ));
    };
    if !matches!(kind, "board" | "schematic" | "netlist") {
        return Err(Error::Invalid(
            "The standalone EDA artifact is inventory-only and cannot be selected.".into(),
        ));
    }
    let source = if format == "altium-inventory" {
        None
    } else {
        Some(
            fs::read_to_string(path)
                .map_err(|e| Error::Invalid(format!("Cannot read {}: {e}", path.display())))?,
        )
    };
    let mut schematics = BTreeMap::new();
    let mut netlists = BTreeMap::new();
    let mut altium_schematics = Vec::new();
    if kind == "schematic" && format == "kicad" {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let mut sibling_files = Vec::new();
        collect_dir(parent, &mut sibling_files, 0)?;
        let mut aggregate = 0_u64;
        for sibling in sibling_files {
            let relative = sibling
                .strip_prefix(parent)
                .unwrap_or(&sibling)
                .to_string_lossy()
                .replace('\\', "/");
            if classify(&relative) == Some(("schematic", "kicad")) {
                let size = fs::metadata(&sibling)
                    .map_err(|error| Error::Invalid(error.to_string()))?
                    .len();
                if size > 2 * 1024 * 1024 || aggregate + size > 8 * 1024 * 1024 {
                    schematics.insert(relative, String::new());
                    continue;
                }
                aggregate += size;
                schematics.insert(
                    relative,
                    fs::read_to_string(&sibling).map_err(|_| {
                        Error::Invalid(format!(
                            "{} is not UTF-8 schematic source.",
                            sibling.display()
                        ))
                    })?,
                );
            }
        }
        schematics
            .entry(name.clone())
            .or_insert_with(|| source.clone().unwrap());
    } else if kind == "schematic" {
        altium_schematics.push(name.clone());
    } else if kind == "netlist" {
        netlists.insert(name.clone(), source.clone().unwrap());
    }
    Ok(Loaded {
        input_kind: format!("standalone-{kind}"),
        project_root: path.parent().map(Path::to_path_buf),
        board_name: (kind == "board").then(|| name.clone()),
        board_source: (kind == "board").then(|| source.clone()).flatten(),
        schematics,
        schematic_root_hint: (kind == "schematic" && format == "kicad").then(|| name.clone()),
        projects: BTreeSet::new(),
        project_variables: BTreeMap::new(),
        altium_schematics,
        netlists,
        artifacts: vec![Artifact {
            path: name,
            kind: kind.into(),
            format: format.into(),
            selected: true,
        }],
        package_findings: vec![],
        package_coverage: vec![],
        rules: None,
        bom: None,
        placement: None,
        manufacturing: fabrication::ManufacturingInventory {
            aggregate_started: Some(manufacturing_started),
            ..fabrication::ManufacturingInventory::default()
        },
        manufacturing_deadline,
    })
}

fn display_path(path: &Path) -> String {
    if path.is_absolute() {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("input")
            .to_owned()
    } else {
        path.to_string_lossy().replace('\\', "/")
    }
}

fn load_explicit_bom(path: &Path) -> Result<(String, String), Error> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !matches!(
        extension.to_ascii_lowercase().as_str(),
        "csv" | "tsv" | "txt"
    ) {
        return Err(Error::Invalid(
            "Explicit BOMs must be UTF-8 .csv, .tsv, or delimited .txt files.".into(),
        ));
    }
    let size = fs::metadata(path)
        .map_err(|error| Error::Invalid(format!("Cannot read BOM {}: {error}", path.display())))?
        .len();
    if size == 0 || size > 256 * 1024 {
        return Err(Error::Invalid(
            "Explicit BOMs must be between 1 byte and 256 KB.".into(),
        ));
    }
    let source = fs::read_to_string(path)
        .map_err(|error| Error::Invalid(format!("Cannot read BOM {}: {error}", path.display())))?;
    Ok((display_path(path), source))
}

fn load_text_artifact(
    path: &Path,
    label: &str,
    extensions: &[&str],
    limit: u64,
) -> Result<(String, String), Error> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !extensions.contains(&extension.as_str()) {
        return Err(Error::Invalid(format!(
            "{label} must use one of these extensions: {}.",
            extensions.join(", ")
        )));
    }
    let size = fs::metadata(path)
        .map_err(|error| {
            Error::Invalid(format!("Cannot read {label} {}: {error}", path.display()))
        })?
        .len();
    if size == 0 || size > limit {
        return Err(Error::Invalid(format!(
            "{label} must be between 1 byte and {limit} bytes."
        )));
    }
    let source = fs::read_to_string(path).map_err(|error| {
        Error::Invalid(format!("Cannot read {label} {}: {error}", path.display()))
    })?;
    Ok((display_path(path), source))
}

fn load_zip(
    path: &Path,
    selector: Option<&str>,
    manufacturing_started: Instant,
    manufacturing_deadline: fabrication::ManufacturingDeadline,
) -> Result<Loaded, Error> {
    let size = fs::metadata(path)
        .map_err(|e| Error::Invalid(e.to_string()))?
        .len();
    if !archive_compressed_size_valid(size) {
        return Err(Error::Invalid(
            "Fabrication ZIP must be between 1 byte and 90 MB.".into(),
        ));
    }
    let file = File::open(path).map_err(|e| Error::Invalid(e.to_string()))?;
    let mut zip = ZipArchive::new(file)
        .map_err(|_| Error::Invalid("Fabrication ZIP is invalid or unsupported.".into()))?;
    if !archive_entry_count_valid(zip.len()) {
        return Err(Error::Invalid(
            "Fabrication ZIP has more than 2,000 entries.".into(),
        ));
    }
    let mut artifacts = Vec::new();
    let mut boards = Vec::new();
    let mut sources = BTreeMap::new();
    let mut sidecars = BTreeMap::new();
    let mut boms = BTreeMap::new();
    let mut schematics = BTreeMap::new();
    let mut projects = BTreeSet::new();
    let mut project_variables = BTreeMap::new();
    let mut altium_schematics = Vec::new();
    let mut netlists = BTreeMap::new();
    let mut schematic_bytes = 0_u64;
    let mut manufacturing = fabrication::ManufacturingInventory {
        aggregate_started: Some(manufacturing_started),
        ..fabrication::ManufacturingInventory::default()
    };
    let mut retained_manufacturing_bytes = 0_u64;
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
        expanded = add_archive_expanded_bytes(expanded, item.size())?;
        if let Some((kind, format)) = classify(&name) {
            artifacts.push(Artifact {
                path: name.clone(),
                kind: kind.into(),
                format: format.into(),
                selected: false,
            });
            if format == "altium-inventory" && kind == "schematic" {
                altium_schematics.push(name);
            } else if matches!(kind, "schematic" | "netlist") {
                if item.size() > 2 * 1024 * 1024 || schematic_bytes + item.size() > 8 * 1024 * 1024
                {
                    if kind == "schematic" {
                        schematics.insert(name, String::new());
                    } else {
                        netlists.insert(name, String::new());
                    }
                    continue;
                }
                let size = item.size();
                let mut source = String::new();
                item.read_to_string(&mut source).map_err(|_| {
                    Error::Invalid(format!("{name} is not bounded UTF-8 EDA text."))
                })?;
                schematic_bytes += size;
                if kind == "schematic" {
                    schematics.insert(name, source);
                } else {
                    netlists.insert(name, source);
                }
            } else if format == "kicad" && matches!(kind, "board" | "rules" | "settings") {
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
                    if kind == "settings" && format == "kicad" {
                        projects.insert(name.clone());
                        if let Ok(value) = serde_json::from_str::<Value>(&source) {
                            if let Some(variables) =
                                value.get("text_variables").and_then(Value::as_object)
                            {
                                for (key, value) in variables {
                                    if let Some(value) =
                                        value.as_str().filter(|value| value.len() <= 4096)
                                    {
                                        project_variables.insert(key.clone(), value.into());
                                    }
                                }
                            }
                        }
                    }
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
            } else if let Some(kind_candidate) = manufacturing_kind(kind) {
                if manufacturing_deadline
                    .check("manufacturing-input-read")
                    .is_err()
                {
                    return Err(Error::Invalid(
                        "Manufacturing inputs exceeded the aggregate read deadline.".into(),
                    ));
                }
                let declared_size = item.size();
                let reason = manufacturing_limit_reason(
                    manufacturing.outcomes.len(),
                    declared_size,
                    retained_manufacturing_bytes,
                );
                let file_started = Instant::now();
                let original_bytes = if reason.is_none() {
                    let (bytes, digest) = read_manufacturing_bytes(
                        &mut item,
                        &name,
                        manufacturing_deadline.for_file_started(file_started),
                    )?;
                    if bytes.len() as u64 != declared_size {
                        return Err(Error::Invalid(format!(
                            "Manufacturing ZIP entry size disagrees with its declaration: {name}"
                        )));
                    }
                    Some((bytes, digest))
                } else {
                    None
                };
                let artifact_digest = original_bytes.as_ref().map(|(_, digest)| digest.clone());
                let outcome = manufacturing_outcome(
                    name.clone(),
                    kind_candidate,
                    declared_size,
                    artifact_digest.clone(),
                    reason,
                );
                if let Some((original_bytes, _)) = original_bytes {
                    retained_manufacturing_bytes += declared_size;
                    manufacturing.inputs.push(fabrication::ManufacturingInput {
                        virtual_path: name,
                        artifact_digest: artifact_digest
                            .expect("retained manufacturing input has a digest"),
                        kind_candidate,
                        size: declared_size,
                        original_bytes,
                        file_started: Some(file_started),
                    });
                }
                manufacturing.outcomes.push(outcome);
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
    if projects.len() > 1 {
        project_variables.clear();
    }
    let rules_name = selected
        .as_ref()
        .and_then(|board| coherent_sidecar(board, sidecars.keys().cloned()));
    let bom_name = selected
        .as_deref()
        .and_then(|board| coherent_bom(board, boms.keys().cloned()))
        .or_else(|| {
            (selected.is_none() && boms.len() == 1).then(|| boms.keys().next().unwrap().clone())
        });
    for item in &mut artifacts {
        item.selected = selected.as_ref() == Some(&item.path)
            || rules_name.as_ref() == Some(&item.path)
            || bom_name.as_ref() == Some(&item.path);
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
    if let Some(finding) = findings
        .iter_mut()
        .find(|finding| finding.id == "package-copper-incomplete")
    {
        finding.severity = Severity::Info;
        finding.gate_impact = GateImpact::EvidenceOnly;
    }
    manufacturing
        .validate_with_deadline(manufacturing_deadline)
        .map_err(|error| Error::Invalid(format!("Invalid manufacturing inventory: {error}")))?;
    let coverage = vec![Coverage {
        id: "package-inventory".into(),
        label: "Archive safety and inventory".into(),
        status: CoverageStatus::Passed,
        evidence: format!(
            "{} recognized files across {} ZIP entries.",
            artifacts.len(),
            zip.len()
        ),
    }];
    let board_source = selected.as_ref().and_then(|s| sources.remove(s));
    let rules = rules_name.and_then(|name| sidecars.remove(&name).map(|source| (name, source)));
    let bom = bom_name.and_then(|name| boms.remove(&name).map(|source| (name, source)));
    Ok(Loaded {
        input_kind: "fabrication-zip".into(),
        project_root: None,
        board_name: selected,
        board_source,
        schematics,
        schematic_root_hint: None,
        projects,
        project_variables,
        altium_schematics,
        netlists,
        artifacts,
        package_findings: findings,
        package_coverage: coverage,
        rules,
        bom,
        placement: None,
        manufacturing,
        manufacturing_deadline,
    })
}

fn kicad_major(version: &str) -> Option<u32> {
    version
        .split(|character: char| !character.is_ascii_digit())
        .find(|part| !part.is_empty())
        .and_then(|part| part.parse::<u32>().ok())
}

fn board_format_major(format: u32) -> u32 {
    match format {
        20250000.. => 10,
        20241200.. => 9,
        20231100.. => 8,
        20221000.. => 7,
        20210000.. => 6,
        _ => 5,
    }
}

fn kicad_context_limitations(
    format: u32,
    installed: Option<&str>,
) -> Vec<(String, &'static [&'static str])> {
    let inferred_major = board_format_major(format);
    let mut limitations = vec![(
        format!(
            "KiCad source format {format} was detected (approximately KiCad {inferred_major})."
        ),
        &["source-structure"] as &'static [&'static str],
    )];
    if let Some((installed, _)) = installed
        .and_then(|version| kicad_major(version).map(|major| (version, major)))
        .filter(|(_, installed_major)| *installed_major >= inferred_major + 2)
    {
        limitations.push((
            format!(
                "Migration warning: native DRC used KiCad {installed} on an approximately KiCad {inferred_major} board format; new-version markers may describe migration work rather than historical manufacturing defects."
            ),
            &["native-drc"],
        ));
    }
    limitations
}

fn native_violation_severity(violation: &NativeViolation) -> Severity {
    let kind = violation.violation_type.to_ascii_lowercase();
    if kind.starts_with("lib_footprint_")
        || kind.starts_with("text_")
        || kind.contains("silkscreen")
        || kind.starts_with("silk_")
    {
        return Severity::Low;
    }
    if kind == "solder_mask_bridge"
        || kind == "footprint_type_mismatch"
        || kind.contains("courtyard")
    {
        return Severity::Medium;
    }
    let serious = violation.group == "unconnected_items"
        || kind.contains("unconnected")
        || kind.contains("short")
        || kind.contains("clearance");
    match (serious, violation.severity.as_str()) {
        (true, "error") => Severity::High,
        (true, _) => Severity::Medium,
        (false, "error") => Severity::Medium,
        (false, "warning") => Severity::Low,
        _ => Severity::Low,
    }
}

fn native_finding_summaries(violations: &[NativeViolation]) -> Vec<Finding> {
    let mut grouped: BTreeMap<(String, String, Severity), (usize, BTreeSet<String>)> =
        BTreeMap::new();
    for violation in violations.iter().filter(|violation| {
        violation.excluded == Some(false) && violation.group != "schematic_parity"
    }) {
        let severity = native_violation_severity(violation);
        let entry = grouped
            .entry((
                violation.group.clone(),
                violation.violation_type.clone(),
                severity,
            ))
            .or_default();
        entry.0 += 1;
        entry.1.insert(violation.structural_location.clone());
    }
    grouped
        .into_iter()
        .map(|((group, violation_type, severity), (count, locations))| {
            finding(
                &format!(
                    "kicad-native-{}-{}",
                    group.replace('_', "-"),
                    violation_type.replace('_', "-")
                ),
                severity,
                "Native DRC",
                &format!("KiCad {}", violation_type.replace('_', " ")),
                format!("{count} active KiCad {group} marker(s) of type {violation_type}."),
                "Open the matching markers in KiCad, correct them, and rerun DRC.",
                &locations.into_iter().collect::<Vec<_>>().join("|"),
                "kicad-cli",
            )
        })
        .collect()
}

fn replaced_native_courtyard_check_ids(
    review: &fabrication::FabricationReview,
    assembly_findings: &[Finding],
    assembly_coverage: &[Coverage],
) -> BTreeSet<&'static str> {
    let completed = assembly_coverage.iter().any(|coverage| {
        coverage.id == "assembly.courtyard-native.v1"
            && matches!(
                coverage.status,
                CoverageStatus::Passed | CoverageStatus::Attention | CoverageStatus::Unknown
            )
    });
    let Some(courtyard) = completed
        .then_some(review.assembly.native_courtyard.as_ref())
        .flatten()
        .filter(|courtyard| courtyard.state == fabrication::NativeCourtyardRunState::Complete)
    else {
        return BTreeSet::new();
    };
    [
        (
            fabrication::NativeCourtyardKind::Overlap,
            "kicad-native-violations-courtyards-overlap",
        ),
        (
            fabrication::NativeCourtyardKind::Malformed,
            "kicad-native-violations-malformed-courtyard",
        ),
        (
            fabrication::NativeCourtyardKind::Missing,
            "kicad-native-violations-missing-courtyard",
        ),
    ]
    .into_iter()
    .filter_map(|(kind, native_id)| {
        let active = courtyard.observations.iter().filter(|observation| {
            observation.kind == kind
                && observation.exclusion == fabrication::NativeExclusionState::Active
        });
        let mut count = 0_usize;
        let all_replaced = active.fold(true, |replaced, observation| {
            count += 1;
            replaced
                && assembly_findings.iter().any(|finding| {
                    finding.id == format!("assembly.courtyard-native.v1/{}", observation.id)
                })
        });
        (count > 0 && all_replaced).then_some(native_id)
    })
    .collect()
}

fn checks_score(findings: &[Finding]) -> u8 {
    let score_key = |finding: &Finding| {
        let title = finding.title.to_ascii_lowercase();
        if finding.source == "kicad-cli-profile" {
            if title.contains("track width") {
                return "track-width".to_string();
            }
            if title.contains("via diameter") {
                return "via-diameter".to_string();
            }
            if title.contains("hole size") || title.contains("drill") {
                return "via-drill".to_string();
            }
            if title.contains("annular width") {
                return "annular-width".to_string();
            }
        }
        finding.id.clone()
    };
    let mut strongest = BTreeMap::new();
    for finding in findings.iter().filter(|finding| {
        !matches!(
            finding.id.as_str(),
            "package-gerbers-missing" | "package-source-drc-unavailable"
        )
    }) {
        let severity = strongest
            .entry(score_key(finding))
            .or_insert(finding.severity);
        *severity = (*severity).max(finding.severity);
    }
    let mut counts = BTreeMap::new();
    for severity in strongest.into_values() {
        *counts.entry(severity).or_insert(0_u16) += 1;
    }
    let curve = |severity, first: u16, additional: u16, cap: u16| {
        let count = counts.get(&severity).copied().unwrap_or(0);
        if count == 0 {
            0
        } else {
            (first + additional * (count - 1)).min(cap)
        }
    };
    let penalty = curve(Severity::Critical, 25, 10, 60)
        + curve(Severity::High, 10, 6, 45)
        + curve(Severity::Medium, 4, 2, 20)
        + curve(Severity::Low, 1, 1, 8);
    100_u16.saturating_sub(penalty).min(100) as u8
}

fn run_native_drc(board_path: &Path, mode: NativeMode) -> Result<(NativeDrc, Vec<Finding>), Error> {
    let report = schematic::run_native(
        board_path,
        schematic::NativeKind::Drc {
            schematic_parity: false,
        },
        mode,
    )?;
    let findings = native_finding_summaries(&report.violations);
    Ok((report, findings))
}

fn copy_project_tree(
    source: &Path,
    destination: &Path,
    source_root: &Path,
    depth: usize,
    entries: &mut usize,
    bytes: &mut u64,
    skipped_external_links: &mut usize,
) -> Result<(), Error> {
    if depth > 32 {
        return Err(Error::Native(
            "Project staging exceeds 32 directory levels.".into(),
        ));
    }
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| Error::Native(format!("Cannot inspect {}: {error}", source.display())))?;
    if metadata.file_type().is_symlink() {
        let target = fs::canonicalize(source).map_err(|error| {
            Error::Native(format!(
                "Cannot resolve project link {}: {error}",
                source.display()
            ))
        })?;
        if !target.starts_with(source_root) {
            *skipped_external_links += 1;
            return Ok(());
        }
        return copy_project_tree(
            &target,
            destination,
            source_root,
            depth + 1,
            entries,
            bytes,
            skipped_external_links,
        );
    }
    *entries += 1;
    if *entries > 20_000 {
        return Err(Error::Native(
            "Project staging exceeds 20,000 files and directories.".into(),
        ));
    }
    if metadata.is_dir() {
        fs::create_dir_all(destination).map_err(|error| {
            Error::Native(format!("Cannot create {}: {error}", destination.display()))
        })?;
        for entry in fs::read_dir(source)
            .map_err(|error| Error::Native(format!("Cannot read {}: {error}", source.display())))?
        {
            let entry = entry.map_err(|error| Error::Native(error.to_string()))?;
            let name = entry.file_name();
            if matches!(
                name.to_str(),
                Some(".git" | "target" | "node_modules" | "vendor")
            ) {
                continue;
            }
            copy_project_tree(
                &entry.path(),
                &destination.join(name),
                source_root,
                depth + 1,
                entries,
                bytes,
                skipped_external_links,
            )?;
        }
    } else if metadata.is_file() {
        *bytes = bytes.saturating_add(metadata.len());
        if *bytes > MAX_EXPANDED_BYTES {
            return Err(Error::Native(format!(
                "Project staging exceeds {} MiB.",
                MAX_EXPANDED_BYTES / 1024 / 1024
            )));
        }
        fs::copy(source, destination).map_err(|error| {
            Error::Native(format!(
                "Cannot copy {} to {}: {error}",
                source.display(),
                destination.display()
            ))
        })?;
    }
    Ok(())
}

fn stage_project(
    project_root: &Path,
    board_path: &Path,
    destination: &Path,
) -> Result<(PathBuf, usize), Error> {
    let canonical_root = fs::canonicalize(project_root).map_err(|error| {
        Error::Native(format!(
            "Cannot resolve project root {}: {error}",
            project_root.display()
        ))
    })?;
    let canonical_board = fs::canonicalize(board_path).map_err(|error| {
        Error::Native(format!(
            "Cannot resolve board {}: {error}",
            board_path.display()
        ))
    })?;
    let canonical_destination = fs::canonicalize(destination).map_err(|error| {
        Error::Native(format!(
            "Cannot resolve staging directory {}: {error}",
            destination.display()
        ))
    })?;
    if canonical_destination.starts_with(&canonical_root) {
        return Err(Error::Native(
            "Profile staging directory must be outside the source project.".into(),
        ));
    }
    let relative_board = canonical_board.strip_prefix(&canonical_root).map_err(|_| {
        Error::Native(format!(
            "Board {} is outside project root {}.",
            board_path.display(),
            project_root.display()
        ))
    })?;
    let mut entries = 0;
    let mut bytes = 0;
    let mut skipped_external_links = 0;
    copy_project_tree(
        &canonical_root,
        destination,
        &canonical_root,
        0,
        &mut entries,
        &mut bytes,
        &mut skipped_external_links,
    )?;
    Ok((destination.join(relative_board), skipped_external_links))
}

fn native_drc(
    project_root: &Path,
    board_path: &Path,
    mode: NativeMode,
) -> Result<(NativeDrc, Vec<Finding>), Error> {
    if mode == NativeMode::Off {
        return run_native_drc(board_path, mode);
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_nanos();
    let sequence = NATIVE_STAGE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "ratemypcb-native-stage-{}-{nonce}-{sequence}",
        std::process::id()
    ));
    let result = (|| {
        fs::create_dir(&root)
            .map_err(|error| Error::Native(format!("Cannot stage native DRC: {error}")))?;
        let (staged_board, _) = stage_project(project_root, board_path, &root)?;
        run_native_drc(&staged_board, mode)
    })();
    let _ = fs::remove_dir_all(&root);
    match result {
        Err(error) if mode == NativeMode::Auto => Ok((
            NativeDrc::not_run(
                None,
                format!(
                    "Native analysis was not run; project staging failed closed: {}",
                    error.to_string().chars().take(2048).collect::<String>()
                ),
            ),
            vec![],
        )),
        result => result,
    }
}

fn append_profile_rules(rules_path: &Path, preset: Preset) -> Result<(), Error> {
    let mut rules = if rules_path.exists() {
        fs::read_to_string(rules_path)
            .map_err(|error| Error::Native(format!("Cannot read staged profile rules: {error}")))?
    } else {
        "(version 1)\n".into()
    };
    if !rules.ends_with('\n') {
        rules.push('\n');
    }
    rules.push_str(&format!(
        "(rule \"RateMyPCB profile track width\" (constraint track_width (min {:.6}mm)))\n(rule \"RateMyPCB profile via diameter\" (constraint via_diameter (min {:.6}mm)))\n(rule \"RateMyPCB profile drill size\" (constraint hole_size (min {:.6}mm)))\n(rule \"RateMyPCB profile annular width\" (constraint annular_width (min {:.6}mm)))\n",
        preset.track, preset.via, preset.drill, preset.annular
    ));
    fs::write(rules_path, rules)
        .map_err(|error| Error::Native(format!("Cannot stage profile rules: {error}")))
}

fn violation_key(violation: &NativeViolation) -> String {
    let mut items: Vec<_> = violation
        .items
        .iter()
        .map(|item| serde_json::to_string(item).unwrap_or_default())
        .collect();
    items.sort();
    serde_json::to_string(&(
        &violation.group,
        &violation.violation_type,
        &violation.severity,
        &violation.description,
        violation.excluded,
        &violation.comment,
        &violation.sheet_path,
        &violation.sheet_uuid_path,
        &violation.structural_location,
        items,
    ))
    .unwrap_or_default()
}

fn added_profile_violations(
    baseline: &[NativeViolation],
    profile: Vec<NativeViolation>,
) -> Vec<NativeViolation> {
    let mut baseline_counts = BTreeMap::new();
    for violation in baseline {
        *baseline_counts
            .entry(violation_key(violation))
            .or_insert(0_usize) += 1;
    }
    profile
        .into_iter()
        .filter(|violation| {
            let count = baseline_counts.entry(violation_key(violation)).or_default();
            if *count == 0 {
                true
            } else {
                *count -= 1;
                false
            }
        })
        .collect()
}

fn profile_native_drc(
    project_root: &Path,
    board_path: &Path,
    preset: Preset,
    mode: NativeMode,
) -> Result<(NativeDrc, Vec<Finding>), Error> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_nanos();
    let root = std::env::temp_dir().join(format!("ratemypcb-profile-{nonce}"));
    let result = (|| {
        fs::create_dir(&root)
            .map_err(|error| Error::Native(format!("Cannot stage profile DRC: {error}")))?;
        let (staged_board, skipped_external_links) =
            stage_project(project_root, board_path, &root)?;
        let (baseline, _) = run_native_drc(&staged_board, mode)?;
        if baseline.status != "completed" {
            let mut drc = baseline;
            drc.tool = "kicad-cli/profile".into();
            drc.note = format!(
                "Profile comparison was not run because the staged baseline did not complete. {}",
                drc.note
            );
            return Ok((drc, vec![]));
        }
        append_profile_rules(&staged_board.with_extension("kicad_dru"), preset)?;
        let (mut drc, _) = run_native_drc(&staged_board, mode)?;
        if drc.status != "completed" {
            drc.tool = "kicad-cli/profile".into();
            drc.note = format!("Profile comparison did not complete. {}", drc.note);
            return Ok((drc, vec![]));
        }
        drc.violations = added_profile_violations(&baseline.violations, drc.violations);
        drc.finding_count = drc
            .violations
            .iter()
            .filter(|marker| marker.excluded == Some(false) && marker.group != "schematic_parity")
            .count();
        drc.excluded_count = drc
            .violations
            .iter()
            .filter(|marker| marker.excluded == Some(true))
            .count();
        drc.unknown_exclusion_count = drc
            .violations
            .iter()
            .filter(|marker| marker.excluded.is_none())
            .count();
        let mut findings = native_finding_summaries(&drc.violations);
        drc.tool = "kicad-cli/profile".into();
        drc.note = format!(
            "Profile delta on a complete staged project copy: {} active added marker(s); {} active baseline marker(s) removed from the delta.{}",
            drc.finding_count,
            baseline.finding_count,
            if skipped_external_links == 0 {
                String::new()
            } else {
                format!(" {skipped_external_links} external symbolic link(s) were not followed.")
            }
        );
        for violation in &mut drc.violations {
            violation.id = violation.id.replacen("kicad-native-", "kicad-profile-", 1);
        }
        for finding in &mut findings {
            finding.id = finding.id.replacen("kicad-native-", "kicad-profile-", 1);
            finding.category = "Fabricator DRC".into();
            finding.source = "kicad-cli-profile".into();
        }
        Ok((drc, findings))
    })();
    let _ = fs::remove_dir_all(&root);
    result
}

fn coverage_category(id: &str) -> &'static str {
    match id {
        "project-rules" | "source-structure" | "native-drc" => "design-integrity",
        "global-minimums"
        | "pad-fabrication-layers"
        | "package-gerbers"
        | "gerber-syntax"
        | "package-inventory"
        | "profile"
        | "profile-drc"
        | "drill-data" => "fabrication",
        "bom-structure" => "bom",
        "placement-structure" => "assembly",
        "supply-snapshot" => "supply-chain",
        _ => "evidence-coverage",
    }
}

fn finding_category(finding: &Finding, evidence: &[EvidenceRecord]) -> &'static str {
    let check_id = evidence_check_id(&finding.id, evidence);
    if check_id.starts_with("kicad-native")
        || matches!(check_id, "ground-zone" | "unrouted-candidates")
    {
        return "design-integrity";
    }
    if check_id.starts_with("kicad-profile")
        || matches!(
            check_id,
            "track-width"
                | "via-diameter"
                | "via-drill"
                | "annular-width"
                | "board-outline"
                | "solder-mask-configuration"
                | "gerber-parse"
                | "gerber-mask-pair"
                | "excellon-parse"
        )
        || check_id.starts_with("package-")
    {
        return "fabrication";
    }
    if check_id.starts_with("bom-")
        || check_id.starts_with("placement-")
        || check_id == "solder-paste-configuration"
    {
        return if check_id.starts_with("bom-") {
            "bom"
        } else {
            "assembly"
        };
    }
    if check_id.starts_with("supply-") {
        return "supply-chain";
    }
    let category = finding.category.to_ascii_lowercase();
    if category.contains("bom") {
        "bom"
    } else if category.contains("placement") || category.contains("paste") {
        "assembly"
    } else if category.contains("supply") || category.contains("lifecycle") {
        "supply-chain"
    } else if category.contains("native")
        || category.contains("connect")
        || category.contains("ground")
    {
        "design-integrity"
    } else if category.contains("package")
        || category.contains("mask")
        || category.contains("geometry")
        || category.contains("via")
        || category.contains("outline")
    {
        "fabrication"
    } else {
        "evidence-coverage"
    }
}

fn required_coverage(scope: ReviewScope) -> &'static [&'static str] {
    match scope {
        ReviewScope::Design => &["source-structure", "native-drc"],
        ReviewScope::Fabrication => &[
            "source-structure",
            "native-drc",
            "profile",
            "profile-drc",
            "package-gerbers",
            "gerber-syntax",
            "drill-data",
        ],
        ReviewScope::Assembly => &[
            "source-structure",
            "native-drc",
            "profile",
            "profile-drc",
            "package-gerbers",
            "gerber-syntax",
            "drill-data",
            "bom-structure",
            "placement-structure",
        ],
        ReviewScope::Full => &[
            "source-structure",
            "native-drc",
            "profile",
            "profile-drc",
            "package-gerbers",
            "gerber-syntax",
            "drill-data",
            "bom-structure",
            "placement-structure",
            "supply-snapshot",
        ],
    }
}

fn ensure_required_coverage_occurrences(scope: ReviewScope, coverage: &mut Vec<Coverage>) {
    for check_id in required_coverage(scope) {
        if !coverage.iter().any(|item| item.id == *check_id) {
            coverage.push(Coverage {
                id: (*check_id).into(),
                label: format!("Required evidence: {check_id}"),
                status: CoverageStatus::Unknown,
                evidence:
                    "The required coverage occurrence was not produced; release remains blocked."
                        .into(),
            });
        }
    }
}

fn category_summaries(
    scope: ReviewScope,
    coverage: &[Coverage],
    findings: &[Finding],
    evidence: &[EvidenceRecord],
) -> Vec<CategorySummary> {
    let required = required_coverage(scope);
    [
        ("design-integrity", "Design Integrity"),
        ("fabrication", "Fabrication"),
        ("bom", "BOM"),
        ("assembly", "Assembly"),
        ("supply-chain", "Supply Chain"),
        ("evidence-coverage", "Evidence & Coverage"),
    ]
    .into_iter()
    .map(|(id, label)| {
        let coverage_ids: Vec<_> = coverage
            .iter()
            .filter(|item| {
                if id == "evidence-coverage" {
                    required.contains(&evidence_check_id(&item.id, evidence))
                        && !matches!(item.status, CoverageStatus::Passed)
                } else {
                    coverage_category(evidence_check_id(&item.id, evidence)) == id
                }
            })
            .map(|item| item.id.clone())
            .collect();
        let finding_ids: Vec<_> = findings
            .iter()
            .filter(|item| finding_category(item, evidence) == id)
            .map(|item| item.id.clone())
            .collect();
        let status = if findings
            .iter()
            .any(|item| finding_category(item, evidence) == id && item.severity >= Severity::High)
        {
            "fail"
        } else if coverage.iter().any(|item| {
            coverage_ids.contains(&item.id)
                && required.contains(&evidence_check_id(&item.id, evidence))
                && !matches!(item.status, CoverageStatus::Passed)
        }) {
            "not-run"
        } else if !finding_ids.is_empty()
            || coverage.iter().any(|item| {
                coverage_ids.contains(&item.id) && matches!(item.status, CoverageStatus::Attention)
            })
        {
            "warning"
        } else {
            "pass"
        };
        CategorySummary {
            id: id.into(),
            label: label.into(),
            status: status.into(),
            coverage_ids,
            finding_ids,
        }
    })
    .collect()
}

fn sha256(value: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(value.as_ref()))
}

fn evidence_id(
    artifact_digest: &str,
    check_id: &str,
    location: &BTreeMap<String, String>,
) -> String {
    let canonical = serde_json::to_vec(&(artifact_digest, check_id, location)).unwrap_or_default();
    format!("ev-{}", sha256(canonical))
}

struct EvidenceVersions<'a> {
    native: Option<&'a str>,
    profile_native: Option<&'a str>,
    schematic_erc: Option<&'a str>,
    schematic_parity: Option<&'a str>,
}

fn finalize_evidence(
    findings: &mut [Finding],
    coverage: &mut [Coverage],
    artifact_digests: &BTreeMap<String, String>,
    default_digest: &str,
    tool_version: &str,
    versions: EvidenceVersions<'_>,
) -> Vec<EvidenceRecord> {
    let digest_for = |source: &str| {
        artifact_digests
            .get(source)
            .or_else(|| artifact_digests.get("board"))
            .map(String::as_str)
            .unwrap_or(default_digest)
    };
    let provenance = |check_id: &str,
                      source: &str,
                      location: BTreeMap<String, String>,
                      confidence,
                      freshness| {
        let artifact_digest = if dfm::is_assembly_model_family_check(check_id) {
            digest_for("fabrication")
        } else {
            digest_for(source)
        }
        .to_string();
        let id = evidence_id(&artifact_digest, check_id, &location);
        let producer_name = if source.starts_with("kicad-cli")
            || matches!(source, "schematic-erc" | "schematic-parity")
        {
            "kicad-cli"
        } else {
            "ratemypcb"
        };
        let producer_version = if producer_name == "ratemypcb" {
            tool_version
        } else if source == "kicad-cli-profile" {
            versions.profile_native.unwrap_or("not-observed")
        } else if source == "schematic-erc" {
            versions.schematic_erc.unwrap_or("not-observed")
        } else if source == "schematic-parity" {
            versions.schematic_parity.unwrap_or("not-observed")
        } else {
            versions.native.unwrap_or("not-observed")
        };
        (
            id,
            EvidenceProvenance {
                artifact_id: format!("artifact-{}", &artifact_digest[..16]),
                artifact_digest,
                producer: EvidenceProducer {
                    kind: if source == "supply" {
                        "provider"
                    } else {
                        "tool"
                    }
                    .into(),
                    name: producer_name.into(),
                    version: producer_version.into(),
                },
                location,
                evidence_class: source.to_string(),
                confidence,
                freshness,
                observed_at: (source == "supply").then(|| "declared-in-supply-snapshot".into()),
            },
        )
    };
    let mut records = Vec::with_capacity(findings.len() + coverage.len());
    for finding in findings {
        let check_id = finding.id.clone();
        let location = BTreeMap::from([
            ("kind".into(), "finding".into()),
            ("value".into(), finding.location.clone()),
        ]);
        let (id, provenance) = provenance(
            &check_id,
            &finding.source,
            location,
            EvidenceConfidence::Medium,
            if finding.source == "supply" {
                EvidenceFreshness::Unknown
            } else {
                EvidenceFreshness::NotApplicable
            },
        );
        finding.id = id.clone();
        records.push(EvidenceRecord {
            id,
            check_id,
            kind: "finding".into(),
            provenance,
        });
    }
    for item in coverage {
        let check_id = item.id.clone();
        let source = match check_id.as_str() {
            id if id.starts_with("bom-") => "bom",
            id if id.starts_with("placement-") => "placement",
            id if id.starts_with("supply-") => "supply",
            "native-drc" | "profile-drc" => "kicad-cli",
            "schematic-evidence" => "schematic-evidence",
            "schematic-erc" => "schematic-erc",
            "schematic-parity" => "schematic-parity",
            dfm::POPULATION_PARITY_FAMILY => "schematic-reconciliation",
            id if dfm::is_footprint_string_family_check(id) => "schematic-reconciliation",
            id if id == "dfm-declarations" || id.starts_with("dfm-declaration-gap/") => {
                "dfm-declarations"
            }
            id if dfm::is_fabrication_family_check(id)
                || dfm::is_assembly_model_family_check(id) =>
            {
                "fabrication"
            }
            "gerber-syntax" | "package-gerbers" | "drill-data" => "fabrication",
            _ => "board",
        };
        let freshness = match item.status {
            CoverageStatus::Stale => EvidenceFreshness::Stale,
            CoverageStatus::Passed | CoverageStatus::Attention if source == "supply" => {
                EvidenceFreshness::Current
            }
            CoverageStatus::Unknown => EvidenceFreshness::Unknown,
            _ if source == "supply" => EvidenceFreshness::Unknown,
            _ => EvidenceFreshness::NotApplicable,
        };
        let location = BTreeMap::from([
            ("kind".into(), "coverage".into()),
            ("value".into(), check_id.clone()),
        ]);
        let (id, provenance) = provenance(
            &check_id,
            source,
            location,
            EvidenceConfidence::Medium,
            freshness,
        );
        item.id = id.clone();
        records.push(EvidenceRecord {
            id,
            check_id,
            kind: "coverage".into(),
            provenance,
        });
    }
    records
}

fn evidence_check_id<'a>(id: &'a str, evidence: &'a [EvidenceRecord]) -> &'a str {
    evidence
        .iter()
        .find(|item| item.id == id)
        .map(|item| item.check_id.as_str())
        .unwrap_or(id)
}

fn required_evidence_summary(
    scope: ReviewScope,
    coverage: &[Coverage],
    evidence: &[EvidenceRecord],
) -> Vec<RequiredEvidence> {
    required_coverage(scope)
        .iter()
        .map(|check_id| {
            let item = coverage
                .iter()
                .find(|item| evidence_check_id(&item.id, evidence) == *check_id);
            let (evidence_id, status, confidence, freshness) = item
                .and_then(|item| {
                    let record = evidence.iter().find(|record| record.id == item.id)?;
                    Some((
                        item.id.clone(),
                        item.status.clone(),
                        record.provenance.confidence.clone(),
                        record.provenance.freshness.clone(),
                    ))
                })
                .unwrap_or_else(|| {
                    (
                        String::new(),
                        CoverageStatus::Unknown,
                        EvidenceConfidence::Unknown,
                        EvidenceFreshness::Unknown,
                    )
                });
            let (execution, result) = match status {
                CoverageStatus::Passed => (EvidenceExecution::Completed, EvidenceResult::Pass),
                CoverageStatus::Attention => {
                    (EvidenceExecution::Completed, EvidenceResult::Attention)
                }
                CoverageStatus::NotRun => (EvidenceExecution::NotRun, EvidenceResult::Unknown),
                CoverageStatus::NotProvided => {
                    (EvidenceExecution::NotProvided, EvidenceResult::Unknown)
                }
                CoverageStatus::Failed => (EvidenceExecution::Failed, EvidenceResult::Fail),
                CoverageStatus::Unsupported => {
                    (EvidenceExecution::Unsupported, EvidenceResult::Unknown)
                }
                CoverageStatus::Stale => (EvidenceExecution::Completed, EvidenceResult::Attention),
                CoverageStatus::Unknown => (EvidenceExecution::Unknown, EvidenceResult::Unknown),
            };
            RequiredEvidence {
                check_id: (*check_id).into(),
                evidence_id,
                execution,
                result,
                freshness,
                confidence,
            }
        })
        .collect()
}

fn approval_eligible(required: &[RequiredEvidence], findings: &[Finding]) -> bool {
    required.iter().all(|item| {
        item.execution == EvidenceExecution::Completed
            && item.result == EvidenceResult::Pass
            && matches!(
                item.freshness,
                EvidenceFreshness::Current | EvidenceFreshness::NotApplicable
            )
    }) && !findings
        .iter()
        .any(|item| item.gate_impact == GateImpact::Blocking && item.severity >= Severity::Medium)
}

fn bounded_report_text(value: &str) -> bool {
    value.len() <= 512 && !value.chars().any(char::is_control)
}

fn validate_bom_bounds(bom: &BomReport) -> Result<(), Error> {
    if bom.lines.len() > 10_000 || bom.line_count != bom.lines.len() {
        return Err(Error::Invalid("Report BOM line bounds are invalid.".into()));
    }
    for line in &bom.lines {
        let texts = line
            .references
            .iter()
            .chain(line.distributors.iter())
            .chain(line.alternate_mpns.iter());
        if line.references.len() > 10_000
            || line.provider_checks.len() > 3
            || line.offers.len() > 256
            || line.lifecycle_assertions.len() > 32
            || line.alternate_candidates.len() > 64
            || line.approved_alternates.len() > 32
            || line.distributors.len() > 256
            || line.alternate_mpns.len() > 64
            || texts.into_iter().any(|value| !bounded_report_text(value))
            || [
                &line.identity,
                &line.lifecycle,
                &line.sourceability,
                &line.pricing,
                &line.alternatives,
                &line.release_impact,
            ]
            .into_iter()
            .any(|item| !bounded_report_text(&item.status) || !bounded_report_text(&item.detail))
            || line.provider_checks.iter().any(|check| {
                !matches!(check.provider.as_str(), "mouser" | "digikey" | "lcsc")
                    || !bounded_report_text(&check.status)
                    || check
                        .error_kind
                        .as_deref()
                        .is_some_and(|value| !bounded_report_text(value))
                    || check
                        .provenance
                        .as_deref()
                        .is_some_and(|value| !bounded_report_text(value))
            })
            || line.offers.iter().any(|offer| {
                [
                    &offer.observation_id,
                    &offer.provider,
                    &offer.seller,
                    &offer.seller_original,
                    &offer.authorization,
                    &offer.sku,
                    &offer.packaging,
                    &offer.region,
                    &offer.stock_status,
                    &offer.provenance,
                ]
                .into_iter()
                .any(|value| !bounded_report_text(value))
            })
            || line.lifecycle_assertions.iter().any(|assertion| {
                [
                    &assertion.provider,
                    &assertion.raw,
                    &assertion.normalized,
                    &assertion.provenance,
                ]
                .into_iter()
                .any(|value| !bounded_report_text(value))
            })
            || line.alternate_candidates.iter().any(|candidate| {
                [
                    &candidate.manufacturer,
                    &candidate.mpn,
                    &candidate.source,
                    &candidate.evidence_id,
                    &candidate.provenance,
                ]
                .into_iter()
                .any(|value| !bounded_report_text(value))
            })
            || line.approved_alternates.iter().any(|approved| {
                approved.evidence_refs.len() > 32
                    || [
                        &approved.manufacturer,
                        &approved.mpn,
                        &approved.authority,
                        &approved.authority_kind,
                    ]
                    .into_iter()
                    .any(|value| !bounded_report_text(value))
                    || approved
                        .evidence_refs
                        .iter()
                        .any(|value| !bounded_report_text(value))
            })
        {
            return Err(Error::Invalid(
                "Report BOM supply structures exceed their authoritative bounds.".into(),
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub(crate) enum NativeReportChannel {
    Erc,
    Drc,
    SchematicParity,
}

fn validate_native_report(report: &NativeDrc, channel: NativeReportChannel) -> Result<(), Error> {
    let serialized_bytes = serde_json::to_vec(report)
        .map_err(|error| Error::Invalid(format!("Cannot serialize native report: {error}")))?
        .len() as u64;
    let channel_markers = report.violations.iter().filter(|marker| match channel {
        NativeReportChannel::Erc => marker.group == "erc",
        NativeReportChannel::Drc => {
            matches!(marker.group.as_str(), "violations" | "unconnected_items")
        }
        NativeReportChannel::SchematicParity => marker.group == "schematic_parity",
    });
    let active = channel_markers
        .clone()
        .filter(|marker| marker.excluded == Some(false))
        .count();
    let excluded = channel_markers
        .clone()
        .filter(|marker| marker.excluded == Some(true))
        .count();
    let unknown = channel_markers
        .filter(|marker| marker.excluded.is_none())
        .count();
    if serialized_bytes > schematic::MAX_NATIVE_REPORT_BYTES
        || report.violations.len() > 250
        || report.finding_count != active
        || report.excluded_count != excluded
        || report.unknown_exclusion_count != unknown
        || report.included_severities.len() > 256
        || report.ignored_checks.len() > 256
        || report.tool.len() > 512
        || report.note.len() > 4096
        || report
            .version
            .as_deref()
            .is_some_and(|value| value.len() > 512)
        || report
            .report_version
            .as_deref()
            .is_some_and(|value| value.len() > 512)
        || report
            .source
            .as_deref()
            .is_some_and(|value| value.len() > 4096)
        || report
            .date
            .as_deref()
            .is_some_and(|value| value.len() > 4096)
        || !matches!(report.status.as_str(), "completed" | "not_run" | "disabled")
        || (report.status == "completed"
            && (report
                .version
                .as_deref()
                .and_then(schematic::KiCadMajor::parse)
                .is_none()
                || report
                    .report_version
                    .as_deref()
                    .and_then(schematic::KiCadMajor::parse)
                    != report
                        .version
                        .as_deref()
                        .and_then(schematic::KiCadMajor::parse)))
        || report.violations.iter().any(|marker| {
            marker.items.len() > 64
                || !matches!(
                    marker.group.as_str(),
                    "violations" | "unconnected_items" | "schematic_parity" | "erc"
                )
                || marker.id.len() > 512
                || marker.structural_location.is_empty()
                || marker.structural_location.len() > 4096
                || [
                    marker.violation_type.as_str(),
                    marker.severity.as_str(),
                    marker.description.as_str(),
                ]
                .into_iter()
                .any(|value| value.len() > 4096)
                || [
                    marker.comment.as_deref(),
                    marker.sheet_path.as_deref(),
                    marker.sheet_uuid_path.as_deref(),
                ]
                .into_iter()
                .flatten()
                .any(|value| value.len() > 4096)
        })
    {
        return Err(Error::Invalid(
            "Native report marker bounds or counts are invalid.".into(),
        ));
    }
    Ok(())
}

fn lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn validate_schematic_report(report: &SchematicReview) -> Result<(), Error> {
    if report.occurrence_count != report.occurrences.len()
        || report.occurrences.len() > 20_000
        || report.capabilities.len() > 1024
        || report.mismatches.len() > 20_000
        || report.footprint_comparisons.len() > 60_000
        || report.limitations.len() > 1024
        || report.artifact_digests.len() > 2_000
        || report.declared_revisions.len() > 2_000
        || report
            .artifact_digests
            .values()
            .any(|digest| !lowercase_sha256(digest))
        || report.source_pair.as_ref().is_some_and(|pair| {
            report.project_identity.as_deref() != Some(&pair.project_identity)
                || report.root_path.as_deref() != Some(&pair.schematic_path)
                || report.root_digest.as_deref() != Some(&pair.schematic_digest)
                || report.board_path.as_deref() != Some(&pair.board_path)
                || report.board_digest.as_deref() != Some(&pair.board_digest)
        })
        || report.mismatches.iter().any(|mismatch| {
            mismatch.gate_impact != GateImpact::EvidenceOnly
                || !matches!(
                    mismatch.join.as_str(),
                    "occurrence-uuid"
                        | "board-uuid-path"
                        | "reference-fallback"
                        | "unmatched"
                        | "artifact-revision"
                )
                || mismatch.location.len() > 4096
        })
        || report.occurrences.iter().any(|occurrence| {
            !lowercase_sha256(&occurrence.key)
                || !lowercase_sha256(&occurrence.root_digest)
                || occurrence.facts.len() > 64
                || occurrence.sheet_uuid_path.len() > 4096
                || occurrence.source_path.len() > 512
                || occurrence.facts.iter().any(|fact| {
                    fact.name.len() > 512
                        || fact.value.len() > 4096
                        || fact.producer.len() > 512
                        || fact.evidence_class.len() > 512
                        || fact.source_path.len() > 512
                        || fact.confidence.len() > 512
                        || !matches!(
                            fact.confidence.as_str(),
                            "high" | "medium" | "low" | "unknown"
                        )
                        || match fact.evidence_class.as_str() {
                            "explicit-source-fact" => {
                                fact.producer != "kicad-source"
                                    || fact.source_path != occurrence.source_path
                            }
                            "explicit-export-facts" => {
                                !matches!(
                                    fact.source_path.as_str(),
                                    "native:bom.csv"
                                        | "native:netlist.net"
                                        | "native:positions.csv"
                                ) || fact.producer.strip_prefix("kicad-cli ").is_none_or(
                                    |version| {
                                        version.is_empty()
                                            || schematic::KiCadMajor::parse(version).is_none()
                                    },
                                )
                            }
                            _ => true,
                        }
                })
        })
    {
        return Err(Error::Invalid(
            "Schematic hierarchy, reconciliation, or digest bounds are invalid.".into(),
        ));
    }
    if report
        .root_digest
        .as_deref()
        .is_some_and(|digest| !lowercase_sha256(digest))
        || report
            .board_digest
            .as_deref()
            .is_some_and(|digest| !lowercase_sha256(digest))
        || report.source_pair.as_ref().is_some_and(|pair| {
            !lowercase_sha256(&pair.schematic_digest) || !lowercase_sha256(&pair.board_digest)
        })
    {
        return Err(Error::Invalid(
            "Schematic source-pair digests must be lowercase SHA-256 values.".into(),
        ));
    }
    if report.root_digest.is_some() && !report.artifact_digests.contains_key("schematic:composite")
    {
        return Err(Error::Invalid(
            "Schematic artifactDigests must include the reconciliation composite.".into(),
        ));
    }
    let expected_native_facts =
        schematic::canonical_native_export_facts_digest(&report.occurrences);
    let recorded_native_facts = report
        .artifact_digests
        .get(schematic::NATIVE_FACTS_DIGEST_KEY);
    if expected_native_facts.as_ref() != recorded_native_facts {
        return Err(Error::Invalid(
            "Schematic native export fact digest is missing, extra, or does not match occurrences."
                .into(),
        ));
    }
    if let Some(composite) = report.artifact_digests.get("schematic:composite") {
        let expected = schematic::schematic_composite_digest(&report.artifact_digests);
        if composite != &expected {
            return Err(Error::Invalid(
                "Schematic composite digest does not match artifactDigests.".into(),
            ));
        }
    }
    let mut occurrence_keys = BTreeSet::new();
    if report
        .occurrences
        .iter()
        .any(|occurrence| !occurrence_keys.insert(&occurrence.key))
    {
        return Err(Error::Invalid(
            "Schematic occurrence identities must be unique.".into(),
        ));
    }
    let occurrences = report
        .occurrences
        .iter()
        .map(|occurrence| (occurrence.key.as_str(), occurrence))
        .collect::<BTreeMap<_, _>>();
    let mut comparison_keys = BTreeSet::new();
    for comparison in &report.footprint_comparisons {
        let occurrence = occurrences
            .get(comparison.occurrence_key.as_str())
            .copied()
            .ok_or_else(|| {
                Error::Invalid("Schematic footprint comparison occurrence is dangling.".into())
            })?;
        let expected_location = format!(
            "sheet={};item={};source={}",
            occurrence.sheet_uuid_path, occurrence.item_uuid, occurrence.source_path
        );
        let join_valid = match comparison.source {
            SchematicComparisonSource::Board | SchematicComparisonSource::Netlist => matches!(
                comparison.join.as_str(),
                "occurrence-uuid" | "reference-fallback"
            ),
            SchematicComparisonSource::Bom => comparison.join == "reference-fallback",
        };
        let source_pair_valid = comparison.source != SchematicComparisonSource::Board
            || report.source_pair.as_ref().is_some_and(|pair| {
                comparison.actual_source_path == pair.board_path
                    && comparison.actual_source_digest == pair.board_digest
            });
        if comparison.field != "footprint"
            || comparison.expected.is_empty()
            || comparison.actual.is_empty()
            || comparison.expected.len() > 4096
            || comparison.actual.len() > 4096
            || comparison.expected_source_path != occurrence.source_path
            || comparison.expected_source_path.len() > 512
            || comparison.actual_source_path.is_empty()
            || comparison.actual_source_path.len() > 512
            || comparison.actual_source_path == comparison.expected_source_path
            || comparison.location != expected_location
            || comparison.location.len() > 4096
            || !lowercase_sha256(&comparison.expected_source_digest)
            || !lowercase_sha256(&comparison.actual_source_digest)
            || report
                .artifact_digests
                .get(&comparison.expected_source_path)
                != Some(&comparison.expected_source_digest)
            || report.artifact_digests.get(&comparison.actual_source_path)
                != Some(&comparison.actual_source_digest)
            || !join_valid
            || comparison.confidence
                != if comparison.join == "reference-fallback" {
                    "low"
                } else {
                    "high"
                }
            || !source_pair_valid
            || !comparison_keys.insert((
                comparison.occurrence_key.as_str(),
                comparison.field.as_str(),
                comparison.source,
            ))
        {
            return Err(Error::Invalid(
                "Schematic footprint comparison authority is invalid or duplicated.".into(),
            ));
        }
        let mismatch_field = match comparison.source {
            SchematicComparisonSource::Board => "footprint",
            SchematicComparisonSource::Bom => "bom-footprint",
            SchematicComparisonSource::Netlist => "netlist-footprint",
        };
        let matching_mismatches = report
            .mismatches
            .iter()
            .filter(|mismatch| {
                mismatch.field == mismatch_field
                    && mismatch.expected == comparison.expected
                    && mismatch.actual == comparison.actual
                    && mismatch.join == comparison.join
                    && mismatch.confidence == comparison.confidence
                    && mismatch.location == comparison.location
            })
            .count();
        if (comparison.matched && matching_mismatches != 0)
            || (!comparison.matched && matching_mismatches != 1)
        {
            return Err(Error::Invalid(
                "Schematic footprint comparison result disagrees with typed mismatches.".into(),
            ));
        }
    }
    if let Some(native) = &report.native_erc {
        validate_native_report(native, NativeReportChannel::Erc)?;
    }
    if let Some(native) = &report.native_parity {
        validate_native_report(native, NativeReportChannel::SchematicParity)?;
    }
    Ok(())
}

pub fn validate_report_supply_retention(report: &Report, now: u64) -> Result<(), Error> {
    let contains_durable_supply = report.bom.lines.iter().any(|line| {
        !line.provider_checks.is_empty()
            || !line.offers.is_empty()
            || !line.lifecycle_assertions.is_empty()
            || !line.alternate_candidates.is_empty()
            || !line.approved_alternates.is_empty()
    });
    if contains_durable_supply
        && report
            .bom
            .supply_legal_expires_at_unix
            .is_none_or(|expires| expires < now)
    {
        return Err(Error::Invalid(
            "Report supply records are legally expired and cannot be rendered or shared.".into(),
        ));
    }
    Ok(())
}

pub fn validate_report(report: &Report) -> Result<(), Error> {
    validate_report_with_fabrication_deadline(report, None)
}

fn validate_report_with_fabrication_deadline(
    report: &Report,
    deadline: Option<fabrication::ManufacturingDeadline>,
) -> Result<(), Error> {
    if report.schema_version != SCHEMA_VERSION {
        return Err(Error::Invalid("Unsupported report schema version.".into()));
    }
    if Path::new(&report.input.path).is_absolute() {
        return Err(Error::Invalid(
            "Report input.path must not contain an absolute operational path.".into(),
        ));
    }
    validate_bom_bounds(&report.bom)?;
    validate_schematic_report(&report.schematic)?;
    let population_inputs_complete = dfm::population_inputs_complete(
        &report.schematic,
        report.coverage.iter().map(|coverage| {
            (
                evidence_check_id(&coverage.id, &report.evidence),
                &coverage.status,
            )
        }),
        report.artifacts.iter().map(|artifact| {
            (
                artifact.path.as_str(),
                artifact.kind.as_str(),
                artifact.selected,
            )
        }),
    );
    dfm::validate_population_parity(
        &report.schematic,
        population_inputs_complete,
        &report.findings,
        &report.coverage,
        &report.evidence,
    )
    .map_err(|error| Error::Invalid(format!("Invalid population parity evidence: {error}")))?;
    match deadline {
        Some(deadline) => report.fabrication.validate_with_deadline(deadline),
        None => report.fabrication.validate(),
    }
    .map_err(|error| Error::Invalid(format!("Invalid fabrication model: {error}")))?;
    let declaration_document = dfm::declaration_document(&report.fabrication)?;
    let declaration_artifacts = report
        .artifacts
        .iter()
        .filter(|artifact| artifact.kind == "dfm-declarations")
        .collect::<Vec<_>>();
    if declaration_document.is_some_and(|document| {
        declaration_artifacts.len() != 1
            || !declaration_artifacts[0].selected
            || declaration_artifacts[0].path != document.virtual_path
    }) || (declaration_document.is_none() && !declaration_artifacts.is_empty())
    {
        return Err(Error::Invalid(
            "DFM declaration artifact inventory does not match canonical evidence.".into(),
        ));
    }
    if !report.fabrication.assembly.declared_placements.is_empty() {
        let selected_placement_paths = report
            .artifacts
            .iter()
            .filter(|artifact| artifact.kind == "placement" && artifact.selected)
            .map(|artifact| artifact.path.as_str())
            .collect::<BTreeSet<_>>();
        let declared_paths = report
            .fabrication
            .assembly
            .declared_placements
            .iter()
            .map(|placement| placement.source_path.as_str())
            .collect::<BTreeSet<_>>();
        let Some(path) = declared_paths.iter().copied().next() else {
            return Err(Error::Invalid(
                "Declared assembly rows have no selected placement artifact.".into(),
            ));
        };
        if declared_paths.len() != 1
            || !selected_placement_paths.contains(path)
            || !report
                .fabrication
                .assembly
                .declared_placements
                .iter()
                .all(|placement| {
                    report.schematic.artifact_digests.get(path) == Some(&placement.artifact_digest)
                })
        {
            return Err(Error::Invalid(
                "Declared assembly rows do not match the selected placement artifact.".into(),
            ));
        }
    }
    for requirements in fabrication::STABLE_FABRICATION_ANALYZERS {
        let passed = report.coverage.iter().any(|coverage| {
            evidence_check_id(&coverage.id, &report.evidence) == requirements.check_family
                && coverage.status == CoverageStatus::Passed
        });
        if passed
            && fabrication::dispatch_analyzer(
                requirements,
                &report.fabrication.capabilities,
                Some(fabrication::SemanticAnalyzerResult::Pass),
            )
            .status
                != fabrication::AnalyzerDispatchStatus::Pass
        {
            return Err(Error::Invalid(format!(
                "Fabrication check {} cannot pass with incomplete capability prerequisites.",
                requirements.check_family
            )));
        }
    }
    validate_native_report(&report.native_drc, NativeReportChannel::Drc)?;
    let normalized_courtyard = fabrication::normalize_native_courtyard_report(&report.native_drc)
        .map_err(|error| {
        Error::Invalid(format!("Invalid native courtyard normalization: {error}"))
    })?;
    let retained_courtyard = report.fabrication.assembly.native_courtyard.as_ref();
    let decision_bearing_courtyard = retained_courtyard.is_some_and(|courtyard| {
        matches!(
            courtyard.state,
            fabrication::NativeCourtyardRunState::Complete
                | fabrication::NativeCourtyardRunState::Partial
        ) || !courtyard.observations.is_empty()
    }) || !normalized_courtyard.observations.is_empty();
    if decision_bearing_courtyard && retained_courtyard != Some(&normalized_courtyard) {
        return Err(Error::Invalid(
            "Canonical courtyard observations do not match the native DRC channel.".into(),
        ));
    }
    if let Some(profile) = &report.profile_drc {
        validate_native_report(profile, NativeReportChannel::Drc)?;
    }
    let mut ids = BTreeSet::new();
    for record in &report.evidence {
        let provenance = &record.provenance;
        if !ids.insert(record.id.as_str()) {
            return Err(Error::Invalid(format!(
                "Duplicate global evidence ID: {}",
                record.id
            )));
        }
        if record.check_id.trim().is_empty()
            || record.kind.trim().is_empty()
            || provenance.artifact_id.trim().is_empty()
            || !lowercase_sha256(&provenance.artifact_digest)
            || provenance.producer.kind.trim().is_empty()
            || provenance.producer.name.trim().is_empty()
            || provenance.producer.version.trim().is_empty()
            || provenance.location.is_empty()
            || provenance.evidence_class.trim().is_empty()
        {
            return Err(Error::Invalid(format!(
                "Incomplete required provenance for evidence ID: {}",
                record.id
            )));
        }
        if evidence_id(
            &provenance.artifact_digest,
            &record.check_id,
            &provenance.location,
        ) != record.id
        {
            return Err(Error::Invalid(format!(
                "Non-canonical evidence ID: {}",
                record.id
            )));
        }
        let expected_semantic_digest = if record.check_id == "schematic-erc" {
            report.schematic.root_digest.as_deref()
        } else if record.check_id == "schematic-evidence"
            || record.check_id == "schematic-parity"
            || record.check_id == dfm::POPULATION_PARITY_FAMILY
            || dfm::is_population_finding_check(&record.check_id)
            || dfm::is_footprint_string_family_check(&record.check_id)
            || record.check_id.starts_with("schematic-reconcile-")
        {
            report
                .schematic
                .artifact_digests
                .get("schematic:composite")
                .map(String::as_str)
        } else if record.check_id == "dfm-declarations"
            || record.check_id.starts_with("dfm-declaration-gap/")
        {
            declaration_document.map(|document| document.artifact_digest.as_str())
        } else if dfm::is_fabrication_family_check(&record.check_id)
            || dfm::is_assembly_model_family_check(&record.check_id)
            || matches!(
                record.check_id.as_str(),
                "package-gerbers" | "gerber-syntax" | "drill-data"
            )
        {
            Some(report.fabrication.model_digest.as_str())
        } else {
            None
        };
        if expected_semantic_digest.is_some_and(|digest| digest != provenance.artifact_digest) {
            let family = if record.check_id.starts_with("schematic") {
                "Schematic"
            } else {
                "Fabrication"
            };
            return Err(Error::Invalid(format!(
                "{family} evidence is not bound to its declared source digest: {}",
                record.id
            )));
        }
    }
    let occurrences: Vec<_> = report
        .findings
        .iter()
        .map(|item| item.id.as_str())
        .chain(report.coverage.iter().map(|item| item.id.as_str()))
        .collect();
    if occurrences.len() != report.evidence.len() || occurrences.iter().any(|id| !ids.contains(id))
    {
        return Err(Error::Invalid(
            "Every finding and coverage occurrence must have exactly one provenance record.".into(),
        ));
    }
    if report.findings.iter().any(|finding| {
        report
            .evidence
            .iter()
            .find(|record| record.id == finding.id)
            .is_none_or(|record| record.kind != "finding")
    }) || report.coverage.iter().any(|coverage| {
        report
            .evidence
            .iter()
            .find(|record| record.id == coverage.id)
            .is_none_or(|record| record.kind != "coverage")
    }) {
        return Err(Error::Invalid(
            "Evidence kinds must match their finding or coverage occurrence.".into(),
        ));
    }
    dfm::validate_fabrication_families(
        &report.fabrication,
        &report.schematic,
        &report.findings,
        &report.coverage,
        &report.evidence,
        deadline,
    )
    .map_err(|error| Error::Invalid(format!("Invalid fabrication DFM evidence: {error}")))?;
    dfm::validate_assembly_families(
        &report.fabrication,
        &report.schematic,
        &report.native_drc,
        &report.findings,
        &report.coverage,
        &report.evidence,
        deadline,
    )
    .map_err(|error| Error::Invalid(format!("Invalid assembly evidence: {error}")))?;
    dfm::validate_gate_impacts(&report.findings, &report.evidence)
        .map_err(|error| Error::Invalid(format!("Invalid DFM GateImpact: {error}")))?;
    let expected_required =
        required_evidence_summary(report.review_scope, &report.coverage, &report.evidence);
    if expected_required.iter().any(|item| {
        item.evidence_id.is_empty()
            || report.evidence.iter().all(|record| {
                record.id != item.evidence_id
                    || record.kind != "coverage"
                    || record.check_id != item.check_id
            })
    }) {
        return Err(Error::Invalid(
            "Every required check needs canonical coverage evidence.".into(),
        ));
    }
    if report.required_evidence != expected_required {
        return Err(Error::Invalid(
            "Required evidence summary does not match authoritative coverage.".into(),
        ));
    }
    if !report.limitation_evidence_refs.is_empty()
        && (report.limitations.len() != report.limitation_evidence_refs.len()
            || report
                .limitation_evidence_refs
                .iter()
                .any(|refs| refs.is_empty() || refs.iter().any(|id| !ids.contains(id.as_str()))))
    {
        return Err(Error::Invalid(
            "Every visible limitation requires valid evidence references.".into(),
        ));
    }
    if report.approval_eligible != approval_eligible(&report.required_evidence, &report.findings) {
        return Err(Error::Invalid(
            "Report approval eligibility conflicts with required evidence or observed findings."
                .into(),
        ));
    }
    Ok(())
}

fn validate_refs<'a>(
    label: &str,
    refs: impl IntoIterator<Item = &'a String>,
    evidence: &BTreeSet<&str>,
) -> Result<(), Error> {
    let mut unique = BTreeSet::new();
    let refs: Vec<_> = refs.into_iter().collect();
    if refs.is_empty() {
        return Err(Error::Invalid(format!(
            "Assessment {label} requires evidence references."
        )));
    }
    for reference in refs {
        if !unique.insert(reference.as_str()) {
            return Err(Error::Invalid(format!(
                "Assessment {label} contains duplicate evidence reference: {reference}"
            )));
        }
        if !evidence.contains(reference.as_str()) {
            return Err(Error::Invalid(format!(
                "Assessment references unknown evidence ID: {reference}"
            )));
        }
    }
    Ok(())
}

pub fn validate_assessment(report: &Report, assessment: &Assessment) -> Result<(), Error> {
    validate_report(report)?;
    if assessment.assessment_schema_version != ASSESSMENT_SCHEMA_VERSION {
        return Err(Error::Invalid(
            "Unsupported assessment schema version.".into(),
        ));
    }
    if assessment.rating > 10 {
        return Err(Error::Invalid(
            "Assessment rating must be between 0 and 10.".into(),
        ));
    }
    if assessment.verdict.trim().is_empty() || assessment.verdict.chars().count() > 60 {
        return Err(Error::Invalid(
            "Assessment verdict must contain 1 to 60 characters.".into(),
        ));
    }
    if assessment.rationale.trim().is_empty() {
        return Err(Error::Invalid(
            "Assessment rationale must be nonempty.".into(),
        ));
    }
    if assessment.actions.len() > 3 {
        return Err(Error::Invalid(
            "Assessment must contain at most three actions.".into(),
        ));
    }
    if !matches!(
        assessment.disposition.as_str(),
        "approve" | "revise" | "blocked"
    ) {
        return Err(Error::Invalid(
            "Assessment disposition must be approve, revise, or blocked.".into(),
        ));
    }
    if assessment.disposition == "approve" && !report.approval_eligible {
        return Err(Error::Invalid("Assessment cannot approve a report with incomplete required evidence or observed blockers.".into()));
    }
    let evidence: BTreeSet<_> = report
        .evidence
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    validate_refs("verdict", &assessment.verdict_evidence_refs, &evidence)?;
    let mut categories = BTreeSet::new();
    for item in &assessment.category_summaries {
        if item.category_id.trim().is_empty()
            || item.summary.trim().is_empty()
            || !categories.insert(item.category_id.as_str())
        {
            return Err(Error::Invalid(
                "Assessment categories must be nonempty and unique.".into(),
            ));
        }
        validate_refs("category", &item.evidence_refs, &evidence)?;
    }
    let mut priorities = BTreeSet::new();
    for item in &assessment.actions {
        if item.title.trim().is_empty()
            || item.rationale.trim().is_empty()
            || !priorities.insert(item.priority)
        {
            return Err(Error::Invalid(
                "Assessment actions must be nonempty with unique priorities.".into(),
            ));
        }
        validate_refs("action", &item.evidence_refs, &evidence)?;
    }
    let mut questions = BTreeSet::new();
    for item in &assessment.questions {
        if item.question.trim().is_empty() || !questions.insert(item.question.as_str()) {
            return Err(Error::Invalid(
                "Assessment questions must be nonempty and unique.".into(),
            ));
        }
        validate_refs("question", &item.evidence_refs, &evidence)?;
    }
    if assessment.disposition != "approve" {
        let top = dfm::top_unblock_evidence_refs(
            required_coverage(report.review_scope),
            &report.required_evidence,
            &report.findings,
            &report.evidence,
        )
        .map_err(|error| Error::Invalid(format!("Cannot rank release unblock: {error}")))?;
        let priority_one = assessment
            .actions
            .iter()
            .find(|action| action.priority == 1)
            .ok_or_else(|| {
                Error::Invalid("A non-approve assessment requires a priority 1 action.".into())
            })?;
        if top.is_empty()
            || !priority_one
                .evidence_refs
                .iter()
                .any(|reference| top.contains(reference))
        {
            return Err(Error::Invalid(
                "Assessment priority 1 must reference the top release-unblock evidence.".into(),
            ));
        }
    }
    Ok(())
}

fn set_fabrication_capability(
    review: &mut fabrication::FabricationReview,
    id: fabrication::CapabilityId,
    state: fabrication::CapabilityState,
    detail: &str,
) {
    review.capabilities.records.retain(|record| record.id != id);
    review
        .capabilities
        .records
        .push(fabrication::CapabilityRecord {
            id,
            state,
            authority: fabrication::Authority::NativeSource,
            document_ids: Vec::new(),
            provenance: Vec::new(),
            detail: detail.into(),
        });
    review.capabilities.records.sort_by_key(|record| record.id);
}

fn manufacturing_review(
    loaded: &Loaded,
    manufacturing_deadline: fabrication::ManufacturingDeadline,
) -> Result<(fabrication::FabricationReview, Vec<Finding>, Vec<Coverage>), Error> {
    let (mut fabrication, mut semantic_failures) =
        match fabrication::analyze_manufacturing_inventory_with_deadline(
            &loaded.manufacturing,
            manufacturing_deadline,
        ) {
            Ok(fabrication) => (fabrication, Vec::new()),
            Err(error) => {
                let mut fallback = fabrication::legacy_inventory_review_with_deadline(
                    &loaded.manufacturing,
                    manufacturing_deadline,
                )
                .map_err(|fallback_error| {
                    Error::Invalid(format!(
                        "Invalid fabrication evidence: {error}; fallback: {fallback_error}"
                    ))
                })?;
                fallback.status = fabrication::FabricationStatus::Failed;
                fallback.warnings.push(fabrication::ManufacturingWarning {
                    code: "manufacturing-semantic-parse-failed".into(),
                    message: error.to_string(),
                    provenance: None,
                });
                (fallback, vec![error.to_string()])
            }
        };
    fabrication
        .refresh_digests_with_deadline(manufacturing_deadline)
        .map_err(|error| {
            Error::Invalid(format!("Invalid package fabrication evidence: {error}"))
        })?;
    fabrication
        .validate_with_deadline(manufacturing_deadline)
        .map_err(|error| {
            Error::Invalid(format!("Invalid package fabrication evidence: {error}"))
        })?;

    if let Some(source) = loaded.board_source.as_deref() {
        let virtual_path = loaded.board_name.as_deref().unwrap_or("selected.kicad_pcb");
        match fabrication::parse_native_kicad_manufacturing_with_deadline(
            virtual_path,
            source.as_bytes(),
            manufacturing_deadline.with_file_limit(),
        ) {
            Ok(native) if fabrication.documents.is_empty() => {
                fabrication = native.review;
                set_fabrication_capability(
                    &mut fabrication,
                    fabrication::CapabilityId::PackageReconciliation,
                    fabrication::CapabilityState::NotProvided,
                    "No release package was provided for symmetric reconciliation.",
                );
            }
            Ok(native) => {
                let native_assembly = native.review.clone();
                match fabrication::reconcile_native_package_with_deadline(
                    fabrication.clone(),
                    native,
                    manufacturing_deadline,
                ) {
                    Ok(reconciled) => fabrication = reconciled,
                    Err(error) => {
                        semantic_failures.push(format!("native/package reconciliation: {error}"));
                        fabrication::retain_native_assembly_only(
                            &mut fabrication,
                            &native_assembly,
                            manufacturing_deadline,
                        )
                        .map_err(|assembly_error| {
                            Error::Invalid(format!(
                                "Invalid retained native assembly evidence: {assembly_error}"
                            ))
                        })?;
                        fabrication.status = fabrication::FabricationStatus::Partial;
                        set_fabrication_capability(
                            &mut fabrication,
                            fabrication::CapabilityId::NativeKicadFacts,
                            fabrication::CapabilityState::Failed,
                            "Native facts could not be retained through failed reconciliation.",
                        );
                        set_fabrication_capability(
                            &mut fabrication,
                            fabrication::CapabilityId::PackageReconciliation,
                            fabrication::CapabilityState::Failed,
                            "Native/package reconciliation failed closed.",
                        );
                        fabrication.integration_outcome = Some(
                            fabrication::IntegratedReconciliationOutcome::new(
                                fabrication::IntegratedReconciliationState::Failed,
                                Some(virtual_path.into()),
                                Some(
                                    fabrication::sha256_with_deadline(
                                        source.as_bytes(),
                                        manufacturing_deadline,
                                        "native-input-hash",
                                    )
                                    .map_err(|error| Error::Invalid(error.to_string()))?,
                                ),
                                "native-package-reconciliation-failed",
                            )
                            .map_err(|error| Error::Invalid(error.to_string()))?,
                        );
                        fabrication
                            .warnings
                            .push(fabrication::ManufacturingWarning {
                                code: "manufacturing-reconciliation-failed".into(),
                                message: error.to_string(),
                                provenance: None,
                            });
                    }
                }
            }
            Err(error) => {
                semantic_failures.push(format!("native KiCad manufacturing: {error}"));
                fabrication.status = if fabrication.documents.is_empty() {
                    fabrication::FabricationStatus::Failed
                } else {
                    fabrication::FabricationStatus::Partial
                };
                set_fabrication_capability(
                    &mut fabrication,
                    fabrication::CapabilityId::NativeKicadFacts,
                    fabrication::CapabilityState::Failed,
                    "Selected native KiCad source parsing failed closed.",
                );
                set_fabrication_capability(
                    &mut fabrication,
                    fabrication::CapabilityId::PackageReconciliation,
                    fabrication::CapabilityState::Failed,
                    "Native/package reconciliation cannot run without valid native facts.",
                );
                fabrication.integration_outcome = Some(
                    fabrication::IntegratedReconciliationOutcome::new(
                        fabrication::IntegratedReconciliationState::Failed,
                        Some(virtual_path.into()),
                        Some(
                            fabrication::sha256_with_deadline(
                                source.as_bytes(),
                                manufacturing_deadline,
                                "native-input-hash",
                            )
                            .map_err(|error| Error::Invalid(error.to_string()))?,
                        ),
                        "native-manufacturing-parse-failed",
                    )
                    .map_err(|canonical| Error::Invalid(canonical.to_string()))?,
                );
                fabrication
                    .warnings
                    .push(fabrication::ManufacturingWarning {
                        code: "native-manufacturing-parse-failed".into(),
                        message: error.to_string(),
                        provenance: None,
                    });
            }
        }
    } else {
        if fabrication.status == fabrication::FabricationStatus::Complete {
            fabrication.status = fabrication::FabricationStatus::Partial;
        }
        set_fabrication_capability(
            &mut fabrication,
            fabrication::CapabilityId::NativeKicadFacts,
            fabrication::CapabilityState::NotProvided,
            "No selected KiCad board source was available.",
        );
        set_fabrication_capability(
            &mut fabrication,
            fabrication::CapabilityId::PackageReconciliation,
            fabrication::CapabilityState::NotProvided,
            "No selected native source was available for symmetric reconciliation.",
        );
        fabrication.integration_outcome = Some(
            fabrication::IntegratedReconciliationOutcome::new(
                fabrication::IntegratedReconciliationState::NotProvided,
                None,
                None,
                "selected-native-source-not-provided",
            )
            .map_err(|error| Error::Invalid(error.to_string()))?,
        );
    }
    fabrication
        .refresh_digests_with_deadline(manufacturing_deadline)
        .map_err(|error| {
            Error::Invalid(format!("Invalid integrated fabrication evidence: {error}"))
        })?;
    fabrication
        .validate_with_deadline(manufacturing_deadline)
        .map_err(|error| {
            Error::Invalid(format!("Invalid integrated fabrication evidence: {error}"))
        })?;

    let mut findings = Vec::new();
    let omitted = fabrication
        .input_outcomes
        .iter()
        .filter(|outcome| outcome.state != fabrication::ManufacturingLoadState::Retained)
        .count();
    if omitted > 0 {
        findings.push(finding(
            "manufacturing-input-bounds",
            Severity::Medium,
            "Package",
            "Manufacturing inputs exceeded declared bounds",
            format!("{omitted} recognized manufacturing input(s) have explicit omitted outcomes."),
            "Reduce the package to the declared file and byte bounds before semantic analysis.",
            "Manufacturing inventory",
            "package",
        ));
    }

    let gerber_outcomes = fabrication
        .input_outcomes
        .iter()
        .filter(|outcome| outcome.kind_candidate == fabrication::ManufacturingKindCandidate::Gerber)
        .collect::<Vec<_>>();
    let drill_outcomes = fabrication
        .input_outcomes
        .iter()
        .filter(|outcome| {
            outcome.kind_candidate == fabrication::ManufacturingKindCandidate::Excellon
        })
        .collect::<Vec<_>>();
    let coverage = fabrication::STABLE_FABRICATION_ANALYZERS
        .into_iter()
        .map(|requirements| {
            let dispatch = fabrication::dispatch_analyzer(
                requirements,
                &fabrication.capabilities,
                Some(fabrication::SemanticAnalyzerResult::Pass),
            );
            let relevant = if requirements.check_family == "drill-data" {
                &drill_outcomes
            } else {
                &gerber_outcomes
            };
            let status = if relevant.is_empty() {
                CoverageStatus::NotProvided
            } else if relevant.iter().any(|outcome| {
                outcome.state != fabrication::ManufacturingLoadState::Retained
            }) {
                CoverageStatus::Failed
            } else {
                match dispatch.status {
                    fabrication::AnalyzerDispatchStatus::Pass => CoverageStatus::Passed,
                    fabrication::AnalyzerDispatchStatus::Attention
                    | fabrication::AnalyzerDispatchStatus::Fail
                    | fabrication::AnalyzerDispatchStatus::NotChecked => CoverageStatus::Attention,
                }
            };
            let label = match requirements.check_family {
                "package-gerbers" => "Gerber/X2 package completeness",
                "gerber-syntax" => "RS-274X semantic syntax parse",
                "drill-data" => "Excellon drill and route semantics",
                _ => unreachable!("stable fabrication analyzer family"),
            };
            Coverage {
                id: requirements.check_family.into(),
                label: label.into(),
                status,
                evidence: if relevant.is_empty() {
                    "No matching manufacturing bytes were provided.".into()
                } else if !semantic_failures.is_empty() {
                    format!(
                        "Production semantic parsing failed closed ({}); incomplete prerequisites: {:?}.",
                        semantic_failures.join("; "),
                        dispatch.incomplete_prerequisites
                    )
                } else {
                    format!(
                        "{} input(s) parsed with production semantics; incomplete prerequisites: {:?}.",
                        relevant.len(), dispatch.incomplete_prerequisites
                    )
                },
            }
        })
        .collect();
    Ok((fabrication, findings, coverage))
}

pub fn review(path: &Path, options: ReviewOptions) -> Result<Report, Error> {
    let mut loaded = load_path(path, options.board.as_deref())?;
    if let Some(declarations) = options.dfm_declarations.as_ref() {
        loaded.artifacts.push(Artifact {
            path: declarations.source_path().into(),
            kind: "dfm-declarations".into(),
            format: "ratemypcb-dfm-1".into(),
            selected: true,
        });
    }
    if let Some(path) = options.bom.as_deref() {
        let bom = load_explicit_bom(path)?;
        loaded.artifacts.push(Artifact {
            path: bom.0.clone(),
            kind: "bom".into(),
            format: "delimited".into(),
            selected: true,
        });
        loaded.bom = Some(bom);
    }
    let explicit_placement = options
        .placement
        .as_deref()
        .map(|path| {
            load_text_artifact(
                path,
                "Placement file",
                &["csv", "tsv", "txt", "pos"],
                2 * 1024 * 1024,
            )
        })
        .transpose()?;
    if let Some((name, _)) = &explicit_placement {
        loaded.artifacts.push(Artifact {
            path: name.clone(),
            kind: "placement".into(),
            format: "centroid".into(),
            selected: true,
        });
    }
    let placement = explicit_placement.or_else(|| loaded.placement.clone());
    let supply = options
        .supply_snapshot
        .as_deref()
        .map(|path| load_text_artifact(path, "Supply snapshot", &["json"], 2 * 1024 * 1024))
        .transpose()?;
    if let Some((name, source)) = &supply {
        let version = serde_json::from_str::<Value>(source)
            .ok()
            .and_then(|value| value.get("schemaVersion")?.as_str().map(str::to_owned))
            .unwrap_or_else(|| "unknown".into());
        loaded.artifacts.push(Artifact {
            path: name.clone(),
            kind: "supply".into(),
            format: format!("ratemypcb-supply-{version}"),
            selected: true,
        });
    }
    let requested_profile = options
        .profile
        .as_deref()
        .map(|name| {
            Preset::profile(name)
                .ok_or_else(|| Error::Invalid(format!("Unsupported profile: {name}")))
        })
        .transpose()?;
    let base_preset = requested_profile
        .as_ref()
        .map(|(preset, _)| *preset)
        .unwrap_or(options.preset);
    let (project_preset, imported_rules, rules_name) = loaded
        .rules
        .as_ref()
        .map(|(name, source)| {
            let (preset, count) = resolve_project_rules(base_preset, name, source);
            (preset, count, Some(name.clone()))
        })
        .unwrap_or((base_preset, 0, None));
    let active_preset = requested_profile
        .as_ref()
        .map(|(preset, _)| *preset)
        .unwrap_or(project_preset);
    let manufacturing_deadline = loaded.manufacturing_deadline;
    let (mut fabrication_review, manufacturing_findings, manufacturing_coverage) =
        manufacturing_review(&loaded, manufacturing_deadline)?;
    dfm::apply_declared_assembly_placements(
        &mut fabrication_review,
        placement
            .as_ref()
            .map(|(name, source)| (name.as_str(), source.as_str())),
    )?;
    let declaration_coverage =
        dfm::apply_declarations(&mut fabrication_review, options.dfm_declarations.as_ref())?;
    let declaration_gaps = dfm::normalized_declaration_gaps(&declaration_coverage, None)
        .map_err(|error| Error::Invalid(format!("Invalid DFM declaration gaps: {error}")))?;
    let mut findings = loaded.package_findings;
    findings.extend(manufacturing_findings);
    let mut coverage = loaded.package_coverage;
    coverage.extend(manufacturing_coverage);
    coverage.extend(declaration_coverage);
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
    coverage.push(Coverage {
        id: "profile".into(),
        label: "Named fabricator profile".into(),
        status: if requested_profile.is_some() {
            CoverageStatus::Passed
        } else {
            CoverageStatus::NotProvided
        },
        evidence: requested_profile
            .as_ref()
            .map(|(_, profile)| {
                format!(
                    "{} rules were applied from {}.",
                    profile.name, profile.source_url
                )
            })
            .unwrap_or_else(|| "No named fabricator profile was selected.".into()),
    });
    let mut facts = None;
    if let Some(source) = &loaded.board_source {
        let parsed = parse_board(source)?;
        findings.extend(static_findings(&parsed, active_preset));
        coverage.extend([
            Coverage { id: "source-structure".into(), label: "KiCad source structure".into(), status: CoverageStatus::Passed, evidence: format!("{} components, {} nets, {} tracks, and {} vias parsed.", parsed.components, parsed.nets.len(), parsed.tracks.len(), parsed.vias.len()) },
            Coverage { id: "global-minimums".into(), label: "Global manufacturing minimums".into(), status: if findings.iter().any(|f| matches!(f.id.as_str(), "track-width" | "via-diameter" | "via-drill" | "annular-width")) { CoverageStatus::Attention } else { CoverageStatus::Passed }, evidence: "Track width, via diameter, drill, and annular ring were compared with the active preset.".into() },
            Coverage { id: "pad-fabrication-layers".into(), label: "Footprint solder mask and paste layers".into(), status: if findings.iter().any(|f| matches!(f.id.as_str(), "solder-mask-configuration" | "solder-paste-configuration")) { CoverageStatus::Attention } else { CoverageStatus::Passed }, evidence: format!("{} footprint pad(s) were checked for layer consistency and non-positive mask/paste aperture dimensions.", parsed.pads) },
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
    let mut bom_report = BomReport::default();
    if let Some((_name, source)) = &loaded.bom {
        let (bom_findings, bom_coverage) = bom_review(source, facts.as_ref());
        bom_report.lines = parse_bom_lines(source);
        bom_report.line_count = bom_report.lines.len();
        bom_report.status = if matches!(bom_coverage.status, CoverageStatus::Passed) {
            "pass"
        } else {
            "attention"
        }
        .into();
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
    if let Some((_name, source)) = &placement {
        let (placement_findings, placement_coverage) = placement_review(source, facts.as_ref());
        findings.extend(placement_findings);
        coverage.push(placement_coverage);
    }
    let (schematic_review, schematic_findings, schematic_coverage) =
        schematic::review_project(schematic::ProjectEvidenceInput {
            input_kind: &loaded.input_kind,
            project_root: loaded.project_root.as_deref(),
            board_name: loaded.board_name.as_deref(),
            board_source: loaded.board_source.as_deref(),
            schematics: &loaded.schematics,
            root_hint: loaded.schematic_root_hint.as_deref(),
            root_selector: options.schematic.as_deref(),
            projects: &loaded.projects,
            project_variables: &loaded.project_variables,
            altium_schematics: &loaded.altium_schematics,
            netlists: &loaded.netlists,
            bom: loaded
                .bom
                .as_ref()
                .map(|(name, source)| (name.as_str(), source.as_str())),
            placement: placement
                .as_ref()
                .map(|(name, source)| (name.as_str(), source.as_str())),
            native_mode: options.native,
        })?;
    coverage.extend(schematic_coverage);
    let (fabrication_findings, fabrication_coverage) = dfm::fabrication_families_with_gaps(
        &fabrication_review,
        Some(&schematic_review),
        &declaration_gaps,
        manufacturing_deadline,
    );
    findings.extend(fabrication_findings);
    coverage.extend(fabrication_coverage);
    let population_inputs_complete = dfm::population_inputs_complete(
        &schematic_review,
        coverage
            .iter()
            .map(|coverage| (coverage.id.as_str(), &coverage.status)),
        loaded.artifacts.iter().map(|artifact| {
            (
                artifact.path.as_str(),
                artifact.kind.as_str(),
                artifact.selected,
            )
        }),
    );
    let (population_findings, population_coverage) =
        dfm::population_parity(&schematic_review, population_inputs_complete);
    let population_checked = population_coverage.status != CoverageStatus::NotRun;
    findings.extend(population_findings);
    coverage.push(population_coverage);
    if let Some((_name, source)) = &supply {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        let summary = supply::evaluate(source, &mut bom_report.lines, now)?;
        bom_report.supply_legal_expires_at_unix = Some(summary.legal_expires_at_unix);
        if !summary.fresh {
            findings.push(finding(
                "supply-snapshot-stale",
                Severity::Medium,
                "Supply Chain",
                "Supply evidence is stale or legally expired",
                "The retrieval or legal retention expiry has elapsed.".into(),
                "Refresh only through a provider use explicitly approved for query, retention, embedding, and sharing.",
                "Supply snapshot",
                "supply",
            ));
        }
        if !summary.attention.is_empty() {
            findings.push(finding(
                "supply-demand-risk",
                Severity::Medium,
                "Supply Chain",
                "Demand-aware supply evidence needs attention",
                summary.attention.iter().take(12).cloned().collect::<Vec<_>>().join(", "),
                "Resolve exact identity and every independent provider/commercial state before release.",
                "Supply snapshot",
                "supply",
            ));
        }
        coverage.push(Coverage {
            id: "supply-snapshot".into(),
            label: "Exact identity and demand-aware supply snapshot".into(),
            status: if !summary.fresh { CoverageStatus::Stale } else if summary.attention.is_empty() { CoverageStatus::Passed } else { CoverageStatus::Attention },
            evidence: format!(
                "{} exact manufacturer+MPN record(s); source {}; named providers remain independent.",
                summary.exact,
                if summary.imported_v1 { "conservative v1 import" } else { "validated v2" }
            ),
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
    let native_project_root = native_board.as_ref().map(|board| {
        if path.is_dir() {
            path
        } else {
            board
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."))
        }
    });
    let (native, native_findings) = if let (Some(board), Some(project_root)) =
        (&native_board, native_project_root)
    {
        native_drc(project_root, board, options.native)?
    } else if options.native == NativeMode::Required {
        return Err(Error::Native(
            "Native DRC requires a directly accessible KiCad board; extract the ZIP first.".into(),
        ));
    } else {
        (
            NativeDrc::not_run(
                None,
                "Native DRC cannot run without a directly accessible KiCad source board.",
            ),
            vec![],
        )
    };
    coverage.push(Coverage {
        id: "native-drc".into(),
        label: "Exact clearance and connectivity".into(),
        status: if native.status == "completed" {
            if native.finding_count > 0 || native.unknown_exclusion_count > 0 {
                CoverageStatus::Attention
            } else {
                CoverageStatus::Passed
            }
        } else {
            CoverageStatus::NotRun
        },
        evidence: native.note.clone(),
    });
    fabrication_review.assembly.native_courtyard = Some(
        fabrication::normalize_native_courtyard_report(&native).map_err(|error| {
            Error::Invalid(format!("Invalid native courtyard evidence: {error}"))
        })?,
    );
    fabrication_review
        .refresh_digests_with_deadline(manufacturing_deadline)
        .and_then(|_| fabrication_review.validate_with_deadline(manufacturing_deadline))
        .map_err(|error| {
            Error::Invalid(format!(
                "Invalid normalized native courtyard evidence: {error}"
            ))
        })?;
    let (assembly_findings, assembly_coverage) = dfm::assembly_families(
        &fabrication_review,
        &schematic_review,
        manufacturing_deadline,
    );
    let replaced_native_courtyard = replaced_native_courtyard_check_ids(
        &fabrication_review,
        &assembly_findings,
        &assembly_coverage,
    );
    let footprint_checked = assembly_coverage.iter().any(|item| {
        item.id == "assembly.footprint-string-parity.v1" && item.status != CoverageStatus::NotRun
    });
    findings.extend(schematic_findings.into_iter().filter(|finding| {
        (!population_checked || !dfm::is_population_reconciliation_check(&finding.id))
            && (!footprint_checked || !dfm::is_footprint_reconciliation_check(&finding.id))
    }));
    findings.extend(
        native_findings
            .into_iter()
            .filter(|finding| !replaced_native_courtyard.contains(finding.id.as_str())),
    );
    findings.extend(assembly_findings);
    coverage.extend(assembly_coverage);
    dfm::reconcile_native_creepage(&mut findings, &mut coverage, &native);
    let profile_drc = if let Some((preset, profile)) = requested_profile.as_ref() {
        if let Some(board) = &native_board {
            let project_root = native_project_root.expect("native board has a project root");
            let (drc, profile_findings) =
                profile_native_drc(project_root, board, *preset, options.native)?;
            findings.extend(profile_findings);
            coverage.push(Coverage {
                id: "profile-drc".into(),
                label: "Native fabricator-profile DRC".into(),
                status: if drc.status == "completed" {
                    if drc.finding_count > 0 || drc.unknown_exclusion_count > 0 {
                        CoverageStatus::Attention
                    } else {
                        CoverageStatus::Passed
                    }
                } else {
                    CoverageStatus::NotRun
                },
                evidence: format!("{}: {}", profile.name, drc.note),
            });
            Some(drc)
        } else {
            coverage.push(Coverage {
                id: "profile-drc".into(),
                label: "Native fabricator-profile DRC".into(),
                status: CoverageStatus::NotRun,
                evidence: "Profile DRC requires a directly accessible KiCad source board.".into(),
            });
            None
        }
    } else {
        coverage.push(Coverage {
            id: "profile-drc".into(),
            label: "Native fabricator-profile DRC".into(),
            status: CoverageStatus::NotProvided,
            evidence: "No named fabricator profile was selected.".into(),
        });
        None
    };
    let has = |id: &str| coverage.iter().any(|item| item.id == id);
    if !has("package-gerbers") {
        let gerbers: Vec<_> = loaded
            .artifacts
            .iter()
            .filter(|artifact| artifact.kind == "gerber")
            .collect();
        let roles: BTreeSet<_> = gerbers
            .iter()
            .filter_map(|artifact| gerber_role(&artifact.path))
            .collect();
        let complete = roles.contains("top-copper")
            && roles.contains("bottom-copper")
            && roles.contains("profile");
        coverage.push(Coverage {
            id: "package-gerbers".into(),
            label: "Gerber layer set".into(),
            status: if complete {
                CoverageStatus::Passed
            } else if !gerbers.is_empty() {
                CoverageStatus::Attention
            } else {
                CoverageStatus::NotProvided
            },
            evidence: if complete {
                format!(
                    "{} Gerber file(s) were inventoried; top/bottom copper and profile identified.",
                    gerbers.len()
                )
            } else if !gerbers.is_empty() {
                format!(
                    "{} Gerber file(s) were inventoried; required roles are incomplete.",
                    gerbers.len()
                )
            } else {
                "No Gerber layer set was provided.".into()
            },
        });
    }
    if !coverage.iter().any(|item| item.id == "drill-data") {
        coverage.push(Coverage {
            id: "drill-data".into(),
            label: "Excellon drill and route validation".into(),
            status: CoverageStatus::NotProvided,
            evidence: "No Excellon drill data was provided.".into(),
        });
    }
    if !coverage.iter().any(|item| item.id == "placement-structure") {
        coverage.push(Coverage {
            id: "placement-structure".into(),
            label: "Placement structure and board correlation".into(),
            status: CoverageStatus::NotProvided,
            evidence: "No explicit placement file was selected.".into(),
        });
    }
    if !coverage.iter().any(|item| item.id == "supply-snapshot") {
        coverage.push(Coverage {
            id: "supply-snapshot".into(),
            label: "Current lifecycle and availability snapshot".into(),
            status: CoverageStatus::NotProvided,
            evidence: "No current provider snapshot was supplied.".into(),
        });
    }
    ensure_required_coverage_occurrences(options.scope, &mut coverage);
    findings.sort_by(|a, b| b.severity.cmp(&a.severity).then_with(|| a.id.cmp(&b.id)));
    let raw = checks_score(&findings);
    let value = f32::from(raw) / 10.0;
    let default_digest = sha256(
        loaded
            .artifacts
            .iter()
            .map(|item| {
                format!(
                    "{}\0{}\0{}\0{}",
                    item.path, item.kind, item.format, item.selected
                )
            })
            .collect::<Vec<_>>()
            .join("\0"),
    );
    let mut artifact_digests = BTreeMap::from([
        (
            "board".into(),
            loaded
                .board_source
                .as_deref()
                .map(sha256)
                .unwrap_or_else(|| default_digest.clone()),
        ),
        (
            "static".into(),
            loaded
                .board_source
                .as_deref()
                .map(sha256)
                .unwrap_or_else(|| default_digest.clone()),
        ),
        ("package".into(), default_digest.clone()),
        (
            "kicad-cli".into(),
            loaded
                .board_source
                .as_deref()
                .map(sha256)
                .unwrap_or_else(|| default_digest.clone()),
        ),
        (
            "kicad-cli-profile".into(),
            loaded
                .board_source
                .as_deref()
                .map(sha256)
                .unwrap_or_else(|| default_digest.clone()),
        ),
        (
            "fabrication".into(),
            fabrication_review.model_digest.clone(),
        ),
    ]);
    if let Some((_, source)) = &loaded.bom {
        artifact_digests.insert("bom".into(), sha256(source));
    }
    if let Some((_, source)) = &placement {
        artifact_digests.insert("placement".into(), sha256(source));
    }
    if let Some((_, source)) = &supply {
        artifact_digests.insert("supply".into(), sha256(source));
    }
    if let Some(document) = dfm::declaration_document(&fabrication_review)? {
        artifact_digests.insert("dfm-declarations".into(), document.artifact_digest.clone());
    }
    artifact_digests.extend(schematic_review.artifact_digests.clone());
    if let Some(composite) = schematic_review
        .artifact_digests
        .get("schematic:composite")
        .cloned()
    {
        artifact_digests.insert("schematic-reconciliation".into(), composite.clone());
        artifact_digests.insert("schematic-evidence".into(), composite.clone());
        artifact_digests.insert("schematic-parity".into(), composite);
    }
    if let Some(root_digest) = &schematic_review.root_digest {
        artifact_digests.insert("schematic-erc".into(), root_digest.clone());
    }
    let evidence = finalize_evidence(
        &mut findings,
        &mut coverage,
        &artifact_digests,
        &default_digest,
        &options.tool_version,
        EvidenceVersions {
            native: native.version.as_deref(),
            profile_native: profile_drc
                .as_ref()
                .and_then(|report| report.version.as_deref()),
            schematic_erc: schematic_review
                .native_erc
                .as_ref()
                .and_then(|report| report.version.as_deref()),
            schematic_parity: schematic_review
                .native_parity
                .as_ref()
                .and_then(|report| report.version.as_deref()),
        },
    );
    let required_evidence = required_evidence_summary(options.scope, &coverage, &evidence);
    let approval_eligible = approval_eligible(&required_evidence, &findings);
    let complete = required_evidence.iter().all(|item| {
        item.execution == EvidenceExecution::Completed
            && matches!(
                item.result,
                EvidenceResult::Pass | EvidenceResult::Attention
            )
            && !matches!(
                item.freshness,
                EvidenceFreshness::Stale | EvidenceFreshness::Unknown
            )
    });
    let verdict = if !complete {
        "Observed checks only — required evidence incomplete"
    } else if !approval_eligible {
        "Observed checks need review — assessment disposition required"
    } else {
        "Observed checks complete — assessment disposition required"
    };
    let evidence_confidence = if complete {
        EvidenceConfidence::High
    } else if facts.is_some() {
        EvidenceConfidence::Medium
    } else {
        EvidenceConfidence::Low
    };
    let confidence = match evidence_confidence {
        EvidenceConfidence::High => "high",
        EvidenceConfidence::Medium => "medium",
        EvidenceConfidence::Low | EvidenceConfidence::Unknown => "low",
    };
    let freshness = if required_evidence
        .iter()
        .any(|item| item.freshness == EvidenceFreshness::Stale)
    {
        EvidenceFreshness::Stale
    } else if required_evidence
        .iter()
        .any(|item| item.freshness == EvidenceFreshness::Unknown)
    {
        EvidenceFreshness::Unknown
    } else {
        EvidenceFreshness::NotApplicable
    };
    let categories = category_summaries(options.scope, &coverage, &findings, &evidence);
    let mut limitations = vec![
        ("Static source checks do not replace native KiCad clearance, connectivity, custom-rule, or zone-fill DRC; consult the native-drc coverage item.".into(), &["native-drc"] as &'static [&'static str]),
        ("Gerber/X2, Gerber Job, and strict or named-legacy XNC are parsed by bounded core adapters. Package approval still requires complete semantic capabilities and symmetric native-source reconciliation; filenames and browser rendering never supply authority.".into(), &["gerber-syntax", "package-gerbers", "drill-data"]),
        ("Named profile minimums cover the stated baseline only; copper weight, layer count, material, finish, impedance, and special processes still require order-specific confirmation.".into(), &["profile", "project-rules"]),
        ("Altium .PcbDoc source-aware DRC is not supported; exported manufacturing artifacts are inventoried only.".into(), &["source-structure", "package-gerbers"]),
    ];
    if let Some(format) = facts.as_ref().and_then(|facts| facts.format_version) {
        limitations.extend(kicad_context_limitations(format, native.version.as_deref()));
    }
    let limitation_evidence_refs = limitations
        .iter()
        .map(|(_, check_ids)| {
            evidence
                .iter()
                .filter(|record| check_ids.contains(&record.check_id.as_str()))
                .map(|record| record.id.clone())
                .collect()
        })
        .collect();
    let limitations = limitations.into_iter().map(|(text, _)| text).collect();
    let stackup = loaded
        .board_source
        .as_deref()
        .and_then(|source| crate::stackup::Stackup::from_kicad_source(source).ok());
    let report = Report {
        schema_version: SCHEMA_VERSION.into(),
        tool: ToolInfo {
            name: "ratemypcb".into(),
            version: options.tool_version,
        },
        input: InputInfo {
            path: display_path(path),
            kind: loaded.input_kind,
            selected_board: loaded.board_name,
        },
        artifacts: loaded.artifacts,
        score: Score {
            value,
            raw,
            verdict: verdict.into(),
        },
        observed_risk: ObservedRisk {
            score_raw: raw,
            highest_severity: findings.iter().map(|item| item.severity).max(),
        },
        confidence: confidence.into(),
        evidence_confidence,
        freshness,
        required_evidence,
        evidence,
        coverage,
        findings,
        native_drc: native,
        profile_drc,
        schematic: schematic_review,
        fabrication: fabrication_review,
        review_scope: options.scope,
        categories,
        approval_eligible,
        profile: requested_profile.map(|(_, profile)| profile),
        bom: bom_report,
        stackup,
        limitations,
        limitation_evidence_refs,
        disclaimer: DISCLAIMER.into(),
    };
    validate_report_with_fabrication_deadline(&report, Some(manufacturing_deadline))?;
    Ok(report)
}

pub fn report_schema() -> Value {
    let mut schema = serde_json::json!({
      "$schema": "https://json-schema.org/draft/2020-12/schema",
      "$id": "https://ratemypcb.com/schemas/report-2.0.json",
      "title": "RateMyPCB decision-grade report",
      "type": "object",
      "additionalProperties": false,
      "required": ["schemaVersion", "tool", "input", "artifacts", "score", "observedRisk", "confidence", "evidenceConfidence", "freshness", "requiredEvidence", "evidence", "coverage", "findings", "nativeDrc", "profileDrc", "schematic", "fabrication", "reviewScope", "categories", "approvalEligible", "profile", "bom", "stackup", "limitations", "disclaimer"],
      "properties": {
        "schemaVersion": { "const": SCHEMA_VERSION },
        "tool": { "type": "object" }, "input": { "type": "object" }, "artifacts": { "type": "array" },
        "score": { "type": "object", "description": "Secondary score metadata; never a release disposition." },
        "observedRisk": { "type": "object", "required": ["scoreRaw", "highestSeverity"] },
        "confidence": { "enum": ["low", "medium", "high"] },
        "evidenceConfidence": { "enum": ["low", "medium", "high", "unknown"] },
        "freshness": { "enum": ["current", "stale", "unknown", "not_applicable"] },
        "requiredEvidence": { "type": "array", "items": { "$ref": "#/$defs/requiredEvidence" } },
        "evidence": { "type": "array", "items": { "$ref": "#/$defs/evidenceRecord" } },
        "coverage": { "type": "array" }, "findings": { "type": "array", "items": { "$ref": "#/$defs/finding" } },
        "nativeDrc": { "$ref": "#/$defs/nativeReport" }, "profileDrc": { "oneOf": [{ "$ref": "#/$defs/nativeReport" }, { "type": "null" }] },
        "schematic": { "$ref": "#/$defs/schematicReview" },
        "fabrication": { "$ref": "#/$defs/fabricationReview" },
        "reviewScope": { "enum": ["design", "fabrication", "assembly", "full"] },
        "categories": { "type": "array" }, "approvalEligible": { "type": "boolean" },
        "profile": { "type": ["object", "null"] }, "bom": { "$ref": "#/$defs/bomReport" },
        "stackup": { "type": ["object", "null"] },
        "limitations": { "type": "array", "items": { "type": "string" } },
        "limitationEvidenceRefs": { "type": "array", "items": { "type": "array", "minItems": 1, "uniqueItems": true, "items": { "type": "string", "pattern": "^ev-[0-9a-f]{64}$" } } },
        "disclaimer": { "type": "string" }
      },
      "$defs": {
        "finding": {
          "type": "object", "additionalProperties": false,
          "required": ["id", "severity", "category", "title", "evidence", "recommendation", "location", "source", "gateImpact"],
          "properties": {
            "id": { "type": "string", "maxLength": 512 }, "severity": { "enum": ["info", "low", "medium", "high", "critical"] }, "category": { "type": "string", "maxLength": 512 },
            "title": { "type": "string", "maxLength": 4096 }, "evidence": { "type": "string", "maxLength": 4096 }, "recommendation": { "type": "string", "maxLength": 4096 }, "location": { "type": "string", "maxLength": 4096 }, "source": { "type": "string", "maxLength": 512 },
            "gateImpact": { "enum": ["blocking", "evidence_only"], "default": "blocking", "description": "Authoritative release-gate disposition. Phase 4 schematic families are evidence_only. Existing findings default to blocking when importing older JSON." }
          }
        },
        "schematicFact": {
          "type": "object", "additionalProperties": false,
          "required": ["name", "value", "producer", "evidenceClass", "sourcePath", "confidence"],
          "properties": {
            "name": { "type": "string", "maxLength": 512 }, "value": { "type": "string", "maxLength": 4096 }, "producer": { "type": "string", "maxLength": 512 },
            "evidenceClass": { "enum": ["explicit-source-fact", "explicit-export-facts"] }, "sourcePath": { "type": "string", "maxLength": 512 }, "confidence": { "enum": ["high", "medium", "low", "unknown"] }
          }
        },
        "schematicOccurrence": {
          "type": "object", "additionalProperties": false,
          "required": ["key", "projectIdentity", "rootDigest", "sheetUuidPath", "itemUuid", "sourcePath", "reference", "unit", "facts"],
          "properties": {
            "key": { "type": "string", "pattern": "^[0-9a-f]{64}$" }, "projectIdentity": { "type": "string", "maxLength": 512 }, "rootDigest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
            "sheetUuidPath": { "type": "string", "maxLength": 4096 }, "itemUuid": { "type": "string", "maxLength": 512 }, "sourcePath": { "type": "string", "maxLength": 512 },
            "reference": { "type": ["string", "null"], "maxLength": 512 }, "unit": { "type": ["string", "null"], "maxLength": 512 }, "facts": { "type": "array", "maxItems": 64, "items": { "$ref": "#/$defs/schematicFact" } }
          }
        },
        "schematicSourcePair": {
          "type": "object", "additionalProperties": false,
          "required": ["projectIdentity", "schematicPath", "schematicDigest", "boardPath", "boardDigest"],
          "properties": {
            "projectIdentity": { "type": "string", "maxLength": 512 },
            "schematicPath": { "type": "string", "maxLength": 512 }, "schematicDigest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
            "boardPath": { "type": "string", "maxLength": 512 }, "boardDigest": { "type": "string", "pattern": "^[0-9a-f]{64}$" }
          }
        },
        "schematicCapability": {
          "type": "object", "additionalProperties": false,
          "required": ["id", "status", "producer", "evidenceClass", "detail"],
          "properties": { "id": { "type": "string", "maxLength": 512 }, "status": { "type": "string", "maxLength": 512 }, "producer": { "type": "string", "maxLength": 512 }, "evidenceClass": { "type": "string", "maxLength": 512 }, "detail": { "type": "string", "maxLength": 4096 } }
        },
        "schematicMismatch": {
          "type": "object", "additionalProperties": false,
          "required": ["checkId", "field", "expected", "actual", "join", "confidence", "gateImpact", "location"],
          "properties": { "checkId": { "type": "string", "maxLength": 512 }, "field": { "type": "string", "maxLength": 512 }, "expected": { "type": "string", "maxLength": 4096 }, "actual": { "type": "string", "maxLength": 4096 }, "join": { "enum": ["occurrence-uuid", "board-uuid-path", "reference-fallback", "unmatched", "artifact-revision"] }, "confidence": { "enum": ["high", "medium", "low", "unknown"] }, "gateImpact": { "const": "evidence_only" }, "location": { "type": "string", "maxLength": 4096 } }
        },
        "schematicFootprintComparison": {
          "type": "object", "additionalProperties": false,
          "required": ["occurrenceKey", "field", "source", "expected", "actual", "join", "confidence", "expectedSourcePath", "expectedSourceDigest", "actualSourcePath", "actualSourceDigest", "location", "matched"],
          "properties": {
            "occurrenceKey": { "type": "string", "pattern": "^[0-9a-f]{64}$" }, "field": { "const": "footprint" }, "source": { "enum": ["board", "bom", "netlist"] },
            "expected": { "type": "string", "minLength": 1, "maxLength": 4096 }, "actual": { "type": "string", "minLength": 1, "maxLength": 4096 },
            "join": { "enum": ["occurrence-uuid", "reference-fallback"] }, "confidence": { "enum": ["high", "low"] },
            "expectedSourcePath": { "type": "string", "minLength": 1, "maxLength": 512 }, "expectedSourceDigest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
            "actualSourcePath": { "type": "string", "minLength": 1, "maxLength": 512 }, "actualSourceDigest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
            "location": { "type": "string", "minLength": 1, "maxLength": 4096 }, "matched": { "type": "boolean" }
          }
        },
        "schematicReview": {
          "type": "object", "additionalProperties": false,
          "required": ["status", "projectIdentity", "rootPath", "rootDigest", "boardPath", "boardDigest", "sourcePair", "artifactDigests", "declaredRevisions", "occurrenceCount", "occurrences", "capabilities", "mismatches", "footprintComparisons", "nativeErc", "nativeParity", "limitations"],
          "properties": {
            "status": { "type": "string", "maxLength": 512 }, "projectIdentity": { "type": ["string", "null"], "maxLength": 512 }, "rootPath": { "type": ["string", "null"], "maxLength": 512 }, "rootDigest": { "type": ["string", "null"], "pattern": "^[0-9a-f]{64}$" },
            "boardPath": { "type": ["string", "null"], "maxLength": 512 }, "boardDigest": { "type": ["string", "null"], "pattern": "^[0-9a-f]{64}$" },
            "sourcePair": { "oneOf": [{ "$ref": "#/$defs/schematicSourcePair" }, { "type": "null" }] },
            "artifactDigests": { "type": "object", "maxProperties": 2000, "additionalProperties": { "type": "string", "pattern": "^[0-9a-f]{64}$" } }, "declaredRevisions": { "type": "object", "maxProperties": 2000, "additionalProperties": { "type": "string", "maxLength": 512 } },
            "occurrenceCount": { "type": "integer", "minimum": 0, "maximum": 20000 },
            "occurrences": { "type": "array", "maxItems": 20000, "items": { "$ref": "#/$defs/schematicOccurrence" } }, "capabilities": { "type": "array", "maxItems": 1024, "items": { "$ref": "#/$defs/schematicCapability" } },
            "mismatches": { "type": "array", "maxItems": 20000, "items": { "$ref": "#/$defs/schematicMismatch" } }, "footprintComparisons": { "type": "array", "maxItems": 60000, "items": { "$ref": "#/$defs/schematicFootprintComparison" } }, "nativeErc": { "oneOf": [{ "$ref": "#/$defs/nativeReport" }, { "type": "null" }] }, "nativeParity": { "oneOf": [{ "$ref": "#/$defs/nativeReport" }, { "type": "null" }] },
            "limitations": { "type": "array", "maxItems": 1024, "items": { "type": "string", "maxLength": 4096 } }
          }
        },
        "nativeMarker": {
          "type": "object", "additionalProperties": false,
          "required": ["id", "group", "violationType", "severity", "description", "items", "excluded", "comment", "sheetPath", "sheetUuidPath", "structuralLocation"],
          "properties": {
            "id": { "type": "string", "minLength": 1, "maxLength": 512 },
            "group": { "enum": ["violations", "unconnected_items", "schematic_parity", "erc"] },
            "violationType": { "type": "string", "maxLength": 4096 }, "severity": { "type": "string", "maxLength": 4096 }, "description": { "type": "string", "maxLength": 4096 },
            "items": { "type": "array", "maxItems": 64 }, "excluded": { "type": ["boolean", "null"] }, "comment": { "type": ["string", "null"], "maxLength": 4096 },
            "sheetPath": { "type": ["string", "null"], "maxLength": 4096 }, "sheetUuidPath": { "type": ["string", "null"], "maxLength": 4096 }, "structuralLocation": { "type": "string", "minLength": 1, "maxLength": 4096 }
          }
        },
        "nativeReport": {
          "type": "object", "additionalProperties": false,
          "required": ["status", "tool", "version", "reportVersion", "findingCount", "excludedCount", "unknownExclusionCount", "note", "source", "date", "includedSeverities", "ignoredChecks", "violations"],
          "properties": {
            "status": { "enum": ["completed", "not_run", "disabled"] }, "tool": { "type": "string", "minLength": 1, "maxLength": 512 }, "version": { "type": ["string", "null"], "maxLength": 512 }, "reportVersion": { "type": ["string", "null"], "maxLength": 512 },
            "findingCount": { "type": "integer", "minimum": 0, "maximum": 250 }, "excludedCount": { "type": "integer", "minimum": 0, "maximum": 250 }, "unknownExclusionCount": { "type": "integer", "minimum": 0, "maximum": 250 },
            "note": { "type": "string", "maxLength": 4096 }, "source": { "type": ["string", "null"], "maxLength": 4096 }, "date": { "type": ["string", "null"], "maxLength": 4096 },
            "includedSeverities": { "type": "array", "maxItems": 256 }, "ignoredChecks": { "type": "array", "maxItems": 256 },
            "violations": { "type": "array", "maxItems": 250, "items": { "$ref": "#/$defs/nativeMarker" } }
          }
        },
        "requiredEvidence": {
          "type": "object", "additionalProperties": false,
          "required": ["checkId", "evidenceId", "execution", "result", "freshness", "confidence"],
          "properties": {
            "checkId": { "type": "string", "minLength": 1 }, "evidenceId": { "type": "string", "pattern": "^ev-[0-9a-f]{64}$" },
            "execution": { "enum": ["completed", "not_run", "not_provided", "failed", "unsupported", "unknown"] },
            "result": { "enum": ["pass", "attention", "fail", "unknown", "not_applicable"] },
            "freshness": { "enum": ["current", "stale", "unknown", "not_applicable"] },
            "confidence": { "enum": ["low", "medium", "high", "unknown"] }
          }
        },
        "evidenceRecord": {
          "type": "object", "additionalProperties": false,
          "required": ["id", "checkId", "kind", "provenance"],
          "properties": {
            "id": { "type": "string", "pattern": "^ev-[0-9a-f]{64}$" }, "checkId": { "type": "string", "minLength": 1 },
            "kind": { "enum": ["finding", "coverage"] },
            "provenance": { "type": "object", "required": ["artifactId", "artifactDigest", "producer", "location", "evidenceClass", "confidence", "freshness", "observedAt"] }
          }
        },
        "bomJudgment": {
          "type": "object", "additionalProperties": false, "required": ["status", "detail"],
          "properties": { "status": { "enum": ["pass", "attention", "not-checked"] }, "detail": { "type": "string", "minLength": 1, "maxLength": 512 } }
        },
        "providerCheck": {
          "type": "object", "additionalProperties": false,
          "required": ["provider", "status", "errorKind", "retrievedAtUnix", "upstreamAtUnix", "provenance"],
          "properties": {
            "provider": { "enum": ["mouser", "digikey", "lcsc"] }, "status": { "enum": ["checked", "not-found", "error", "not-checked"] },
            "errorKind": { "type": ["string", "null"] }, "retrievedAtUnix": { "type": ["integer", "null"], "minimum": 0 },
            "upstreamAtUnix": { "type": ["integer", "null"], "minimum": 0 }, "provenance": { "type": ["string", "null"] }
          }
        },
        "lifecycleReview": {
          "type": "object", "additionalProperties": false, "required": ["provider", "raw", "normalized", "observedAtUnix", "provenance"],
          "properties": { "provider": { "enum": ["mouser", "digikey", "lcsc"] }, "raw": { "type": "string", "maxLength": 512 }, "normalized": { "enum": ["active", "new", "nrnd", "last-time-buy", "eol", "obsolete", "unknown"] }, "observedAtUnix": { "type": "integer", "minimum": 0 }, "provenance": { "type": "string" } }
        },
        "supplyOffer": {
          "type": "object", "additionalProperties": false,
          "required": ["observationId", "provider", "seller", "sellerOriginal", "authorization", "sku", "packaging", "region", "stockStatus", "stock", "moq", "orderMultiple", "factoryLeadTimeDays", "purchasableQuantity", "applicableUnitPrice", "currency", "retrievedAtUnix", "upstreamAtUnix", "legalExpiresAtUnix", "usable", "provenance"],
          "properties": {
            "observationId": { "type": "string", "maxLength": 512 }, "provider": { "enum": ["mouser", "digikey", "lcsc"] }, "seller": { "type": "string", "maxLength": 512 }, "sellerOriginal": { "type": "string", "maxLength": 512 },
            "authorization": { "enum": ["authorized", "unauthorized", "unknown"] }, "sku": { "type": "string", "maxLength": 512 }, "packaging": { "type": "string", "maxLength": 512 }, "region": { "type": "string", "maxLength": 512 },
            "stockStatus": { "enum": ["in-stock", "out-of-stock", "unknown"] }, "stock": { "type": ["integer", "null"], "minimum": 0 }, "moq": { "type": ["integer", "null"], "minimum": 1 },
            "orderMultiple": { "type": ["integer", "null"], "minimum": 1 }, "factoryLeadTimeDays": { "type": ["integer", "null"], "minimum": 0 }, "purchasableQuantity": { "type": ["integer", "null"], "minimum": 0 },
            "applicableUnitPrice": { "type": ["string", "null"], "pattern": "^(0|[1-9][0-9]*)(\\.[0-9]{1,12})?$" }, "currency": { "type": ["string", "null"], "pattern": "^[A-Z]{3}$" },
            "retrievedAtUnix": { "type": "integer", "minimum": 0 }, "upstreamAtUnix": { "type": ["integer", "null"], "minimum": 0 }, "legalExpiresAtUnix": { "type": "integer", "minimum": 0 }, "usable": { "type": "boolean" }, "provenance": { "type": "string" }
          }
        },
        "bomLine": {
          "type": "object", "additionalProperties": false,
          "required": ["lineNumber", "references", "quantity", "value", "footprint", "manufacturer", "mpn", "identity", "lifecycle", "sourceability", "pricing", "alternatives", "releaseImpact", "stock", "moq", "unitPrice", "unitPriceDecimal", "currency", "priceEstimate", "distributors", "alternateMpns", "requiredQuantity", "providerChecks", "offers", "lifecycleConflict", "lifecycleAssertions", "alternateCandidates", "approvedAlternates"],
          "properties": {
            "lineNumber": { "type": "integer", "minimum": 1 }, "references": { "type": "array", "maxItems": 10000, "items": { "type": "string", "maxLength": 512 } }, "quantity": { "type": ["integer", "null"], "minimum": 0 },
            "value": { "type": ["string", "null"] }, "footprint": { "type": ["string", "null"] }, "manufacturer": { "type": ["string", "null"] }, "mpn": { "type": ["string", "null"] },
            "identity": { "$ref": "#/$defs/bomJudgment" }, "lifecycle": { "$ref": "#/$defs/bomJudgment" }, "sourceability": { "$ref": "#/$defs/bomJudgment" }, "pricing": { "$ref": "#/$defs/bomJudgment" }, "alternatives": { "$ref": "#/$defs/bomJudgment" }, "releaseImpact": { "$ref": "#/$defs/bomJudgment" },
            "stock": { "type": ["integer", "null"], "minimum": 0 }, "moq": { "type": ["integer", "null"], "minimum": 0 }, "unitPrice": { "type": ["number", "null"] }, "unitPriceDecimal": { "type": ["string", "null"], "pattern": "^(0|[1-9][0-9]*)(\\.[0-9]{1,12})?$" }, "currency": { "type": ["string", "null"] }, "priceEstimate": { "type": "boolean" },
            "distributors": { "type": "array", "maxItems": 256, "items": { "type": "string", "maxLength": 512 } }, "alternateMpns": { "type": "array", "maxItems": 64, "items": { "type": "string", "maxLength": 512 } }, "requiredQuantity": { "type": ["integer", "null"], "minimum": 0 },
            "providerChecks": { "type": "array", "maxItems": 3, "items": { "$ref": "#/$defs/providerCheck" } }, "offers": { "type": "array", "maxItems": 256, "items": { "$ref": "#/$defs/supplyOffer" } }, "lifecycleConflict": { "type": "boolean" }, "lifecycleAssertions": { "type": "array", "maxItems": 32, "items": { "$ref": "#/$defs/lifecycleReview" } },
            "alternateCandidates": { "type": "array", "maxItems": 64, "items": { "$ref": "#/$defs/alternateCandidate" } }, "approvedAlternates": { "type": "array", "maxItems": 32, "items": { "$ref": "#/$defs/approvedAlternate" } }
          }
        },
        "alternateCandidate": {
          "type": "object", "additionalProperties": false, "required": ["manufacturer", "mpn", "source", "evidenceId", "provenance"],
          "properties": { "manufacturer": { "type": "string", "maxLength": 512 }, "mpn": { "type": "string", "maxLength": 512 }, "source": { "type": "string", "maxLength": 512 }, "evidenceId": { "type": "string", "maxLength": 512 }, "provenance": { "type": "string", "maxLength": 512 } }
        },
        "approvedAlternate": {
          "type": "object", "additionalProperties": false, "required": ["manufacturer", "mpn", "authorityKind", "authority", "approvedAtUnix", "evidenceRefs"],
          "properties": { "manufacturer": { "type": "string", "maxLength": 512 }, "mpn": { "type": "string", "maxLength": 512 }, "authorityKind": { "enum": ["engineering", "user"] }, "authority": { "type": "string", "maxLength": 512 }, "approvedAtUnix": { "type": "integer", "minimum": 0 }, "evidenceRefs": { "type": "array", "minItems": 1, "maxItems": 32, "uniqueItems": true, "items": { "type": "string", "maxLength": 512 } } }
        },
        "bomReport": {
          "type": "object", "additionalProperties": false, "required": ["status", "lineCount", "lines", "supplyLegalExpiresAtUnix"],
          "properties": { "status": { "enum": ["not-provided", "pass", "attention"] }, "lineCount": { "type": "integer", "minimum": 0, "maximum": 10000 }, "lines": { "type": "array", "maxItems": 10000, "items": { "$ref": "#/$defs/bomLine" } }, "supplyLegalExpiresAtUnix": { "type": ["integer", "null"], "minimum": 0 } }
        }
      }
    });
    schema["$defs"]["fabricationReview"] = fabrication::schema();
    for (name, definition) in fabrication::schema_defs() {
        schema["$defs"][name] = definition;
    }
    schema
}

pub fn assessment_schema() -> Value {
    serde_json::json!({
      "$schema": "https://json-schema.org/draft/2020-12/schema",
      "$id": "https://ratemypcb.com/schemas/assessment-2.0.json",
      "title": "RateMyPCB engineering assessment",
      "type": "object", "additionalProperties": false,
      "required": ["assessmentSchemaVersion", "reportDigest", "rating", "disposition", "verdict", "verdictEvidenceRefs", "rationale", "categorySummaries", "actions", "questions"],
      "properties": {
        "assessmentSchemaVersion": { "const": ASSESSMENT_SCHEMA_VERSION },
        "reportDigest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
        "rating": { "type": "integer", "minimum": 0, "maximum": 10, "description": "Secondary rating; disposition is authoritative." },
        "disposition": { "enum": ["approve", "revise", "blocked"] },
        "verdict": { "type": "string", "minLength": 1, "maxLength": 60 },
        "verdictEvidenceRefs": { "$ref": "#/$defs/evidenceRefs" },
        "rationale": { "type": "string", "minLength": 1 },
        "categorySummaries": { "type": "array", "items": { "type": "object", "required": ["categoryId", "summary", "evidenceRefs"] } },
        "actions": { "type": "array", "items": { "type": "object", "required": ["priority", "title", "rationale", "evidenceRefs"] } },
        "questions": { "type": "array", "items": { "type": "object", "required": ["question", "evidenceRefs"] } }
      },
      "$defs": { "evidenceRefs": { "type": "array", "minItems": 1, "uniqueItems": true, "items": { "type": "string", "pattern": "^ev-[0-9a-f]{64}$" } } }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    const CLEAN: &str = r#"(kicad_pcb (version 20240108) (layers (0 "F.Cu" signal) (31 "B.Cu" signal) (44 "Edge.Cuts" user)) (net 1 "GND") (footprint "R" (layer "F.Cu") (property "Reference" "R1") (pad "1" smd rect (size 1 1) (layers "F.Cu" "F.Paste" "F.Mask") (net 1 "GND"))) (segment (start 1 1) (end 2 2) (width 0.25) (layer "F.Cu") (net 1)) (zone (net 1) (layer "F.Cu")) (gr_rect (start 0 0) (end 10 10) (layer "Edge.Cuts")))"#;
    #[test]
    fn parses_clean_board() {
        let facts = parse_board(CLEAN).unwrap();
        assert_eq!(facts.tracks, vec![0.25]);
        assert_eq!(facts.edge_forms, 1);
        assert!(static_findings(&facts, Preset::named("standard").unwrap()).is_empty());
    }
    #[test]
    fn parses_legacy_references_and_placement_exclusions() {
        let source = CLEAN
            .replace(
                "(property \"Reference\" \"R1\")",
                "(fp_text reference \"R1\")",
            )
            .replace("(pad \"1\"", "(attr exclude_from_pos_files) (pad \"1\"");
        let facts = parse_board(&source).unwrap();
        assert!(facts.references.contains("R1"));
        assert!(facts.placement_references.is_empty());
    }
    #[test]
    fn placement_ignores_non_assembly_references_and_dnp_footprints() {
        let extras = r#"
          (footprint "TP" (layer "F.Cu") (property "Reference" "TP1"))
          (footprint "Hole" (layer "F.Cu") (property "Reference" "H1"))
          (footprint "Generated" (layer "F.Cu") (property "Reference" "G***"))
          (footprint "C" (layer "F.Cu") (property "Reference" "C1") (attr smd dnp))
          (footprint "R" (layer "F.Cu") (property "Reference" "R2") (attr exclude_from_pos_files))
        "#;
        let source = CLEAN.replace("(segment", &format!("{extras}(segment"));
        let facts = parse_board(&source).unwrap();
        assert_eq!(facts.placement_references, BTreeSet::from(["R1".into()]));
        let (findings, coverage) = placement_review(
            "Reference,PosX,PosY,Rotation,Side\nR1,1,2,0,Top\nTP1,1,2,0,Top\nH1,1,2,0,Top\nG***,1,2,0,Top\n.,1,2,0,Top\n,1,2,0,Top\n",
            Some(&facts),
        );
        assert!(findings.is_empty());
        assert!(matches!(coverage.status, CoverageStatus::Passed));
    }

    #[test]
    fn reports_legacy_kicad_migration_context_without_a_fabrication_finding() {
        let limitations = kicad_context_limitations(20211014, Some("10.0.5"));
        assert!(limitations.iter().any(|(item, families)| {
            item.contains("KiCad 6") && *families == ["source-structure"]
        }));
        assert!(limitations.iter().any(|(item, families)| {
            item.starts_with("Migration warning:") && *families == ["native-drc"]
        }));
        assert_eq!(kicad_context_limitations(20240108, Some("9.0.0")).len(), 1);
    }

    #[test]
    fn bom_sidecar_selection_prefers_the_selected_board_context() {
        let candidates = [
            "aggregate/project-bom.csv".to_string(),
            "boards/main/main-bom.csv".to_string(),
            "boards/main/unrelated.csv".to_string(),
        ];
        assert_eq!(
            coherent_bom("boards/main/main.kicad_pcb", candidates.into_iter()),
            Some("boards/main/main-bom.csv".into())
        );
        assert_eq!(
            coherent_bom(
                "boards/main/main.kicad_pcb",
                [
                    "boards/main/first.csv".to_string(),
                    "boards/main/second.csv".to_string(),
                    "aggregate/project-bom.csv".to_string(),
                ]
                .into_iter(),
            ),
            None
        );
    }

    #[test]
    fn exact_board_path_wins_over_duplicate_filenames() {
        let candidates = [
            "project/main.kicad_pcb".to_string(),
            "project/backup/main.kicad_pcb".to_string(),
        ];
        assert_eq!(
            select_board(&candidates, Some("project/main.kicad_pcb")).unwrap(),
            Some("project/main.kicad_pcb".into())
        );
        assert!(select_board(&candidates, Some("main.kicad_pcb")).is_err());
    }
    #[test]
    fn native_markers_are_grouped_by_rule() {
        let violation = NativeViolation {
            id: "marker-1".into(),
            group: "violations".into(),
            violation_type: "clearance".into(),
            severity: "error".into(),
            description: "Clearance violation".into(),
            items: vec![],
            excluded: Some(false),
            comment: None,
            sheet_path: None,
            sheet_uuid_path: None,
            structural_location: "channel=violations;sheet=root;items=none;index=0".into(),
        };
        let mut second = violation.clone();
        second.description = "Clearance violation at another location".into();
        let findings = native_finding_summaries(&[violation, second]);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].evidence.starts_with("2 active KiCad"));
    }

    #[test]
    fn partial_courtyard_run_preserves_active_native_finding_until_replaced() {
        let marker = NativeViolation {
            id: "overlap-marker".into(),
            group: "violations".into(),
            violation_type: "courtyards_overlap".into(),
            severity: "error".into(),
            description: "overlap".into(),
            items: vec![],
            excluded: Some(false),
            comment: None,
            sheet_path: None,
            sheet_uuid_path: None,
            structural_location: "channel=violations;sheet=root;items=footprints;index=0".into(),
        };
        let mut native = NativeDrc {
            status: "completed".into(),
            tool: "kicad-cli".into(),
            version: Some("10.0.5".into()),
            report_version: Some("10.0.5".into()),
            finding_count: 1,
            excluded_count: 0,
            unknown_exclusion_count: 0,
            note: "partial courtyard fixture".into(),
            source: Some("board.kicad_pcb".into()),
            date: Some("2026-09-01T00:00:00Z".into()),
            included_severities: vec!["error".into(), "warning".into(), "exclusion".into()],
            ignored_checks: vec![serde_json::json!({"key": "missing_courtyard"})],
            violations: vec![marker.clone()],
        };
        let native_findings = native_finding_summaries(&native.violations);
        assert_eq!(native_findings.len(), 1);
        let mut review = fabrication::FabricationReview::default();
        review.assembly.native_courtyard =
            Some(fabrication::normalize_native_courtyard_report(&native).unwrap());
        assert_eq!(
            review.assembly.native_courtyard.as_ref().unwrap().state,
            fabrication::NativeCourtyardRunState::Partial
        );
        let not_checked = vec![Coverage {
            id: "assembly.courtyard-native.v1".into(),
            label: "courtyard".into(),
            status: CoverageStatus::NotRun,
            evidence: "not_checked: ignored courtyard check".into(),
        }];
        let replaced = replaced_native_courtyard_check_ids(&review, &[], &not_checked);
        assert!(replaced.is_empty());
        assert!(
            native_findings
                .iter()
                .any(|finding| !replaced.contains(finding.id.as_str()))
        );

        native.ignored_checks.clear();
        review.assembly.native_courtyard =
            Some(fabrication::normalize_native_courtyard_report(&native).unwrap());
        let observation = &review
            .assembly
            .native_courtyard
            .as_ref()
            .unwrap()
            .observations[0];
        let assembly_findings = vec![finding(
            &format!("assembly.courtyard-native.v1/{}", observation.id),
            Severity::Medium,
            "Assembly",
            "Native courtyard overlap",
            "active overlap".into(),
            "Resolve it",
            &observation.location,
            "kicad-cli",
        )];
        let completed = vec![Coverage {
            id: "assembly.courtyard-native.v1".into(),
            label: "courtyard".into(),
            status: CoverageStatus::Attention,
            evidence: "completed".into(),
        }];
        let replaced = replaced_native_courtyard_check_ids(&review, &assembly_findings, &completed);
        assert_eq!(
            replaced,
            BTreeSet::from(["kicad-native-violations-courtyards-overlap"])
        );
    }

    #[test]
    fn native_provenance_uses_observed_version_exact_digest_and_location() {
        let digest = sha256("exact-board");
        let mut findings = vec![finding(
            "kicad-native-violations-clearance",
            Severity::High,
            "Native DRC",
            "KiCad clearance",
            "One active marker".into(),
            "Fix it",
            "channel=violations;sheet=root;items=item-1;index=0",
            "kicad-cli",
        )];
        let mut coverage = vec![];
        let evidence = finalize_evidence(
            &mut findings,
            &mut coverage,
            &BTreeMap::from([("kicad-cli".into(), digest.clone())]),
            &sha256("fallback"),
            "0.2.0",
            EvidenceVersions {
                native: Some("10.0.5"),
                profile_native: None,
                schematic_erc: None,
                schematic_parity: None,
            },
        );
        assert_eq!(evidence[0].provenance.artifact_digest, digest);
        assert_eq!(evidence[0].provenance.producer.version, "10.0.5");
        assert_eq!(
            evidence[0].provenance.location["value"],
            "channel=violations;sheet=root;items=item-1;index=0"
        );
    }

    #[test]
    fn schematic_evidence_only_findings_do_not_change_approval_gate() {
        let mut finding = finding(
            "schematic-reconcile-value",
            Severity::Critical,
            "Schematic reconciliation",
            "Mismatch",
            "Synthetic mismatch".into(),
            "Regenerate exports.",
            "sheet=/root;item=one",
            "schematic-reconciliation",
        );
        finding.gate_impact = GateImpact::EvidenceOnly;
        assert!(approval_eligible(&[], &[finding]));
    }

    #[test]
    fn native_rule_policy_distinguishes_blockers_from_review_items() {
        let violation = |kind: &str, severity: &str| NativeViolation {
            id: kind.into(),
            group: "violations".into(),
            violation_type: kind.into(),
            severity: severity.into(),
            description: kind.into(),
            items: vec![],
            excluded: Some(false),
            comment: None,
            sheet_path: None,
            sheet_uuid_path: None,
            structural_location: "channel=violations;sheet=root;items=none;index=0".into(),
        };
        assert_eq!(
            native_violation_severity(&violation("clearance", "error")),
            Severity::High
        );
        assert_eq!(
            native_violation_severity(&violation("lib_footprint_mismatch", "error")),
            Severity::Low
        );
        assert_eq!(
            native_violation_severity(&violation("solder_mask_bridge", "error")),
            Severity::Medium
        );
        assert_eq!(
            native_violation_severity(&violation("malformed_courtyard", "error")),
            Severity::Medium
        );
    }

    #[test]
    fn cosmetic_warning_sets_remain_distinguishable_without_flooring() {
        let violations: Vec<_> = [
            "lib_footprint_issues",
            "lib_footprint_mismatch",
            "text_height",
            "text_thickness",
        ]
        .into_iter()
        .map(|kind| NativeViolation {
            id: kind.into(),
            group: "violations".into(),
            violation_type: kind.into(),
            severity: "warning".into(),
            description: kind.into(),
            items: vec![],
            excluded: Some(false),
            comment: None,
            sheet_path: None,
            sheet_uuid_path: None,
            structural_location: "channel=violations;sheet=root;items=none;index=0".into(),
        })
        .collect();
        let one = native_finding_summaries(&violations[..1]);
        let four = native_finding_summaries(&violations);
        assert_eq!(checks_score(&one), 99);
        assert_eq!(checks_score(&four), 96);

        let missing_gerbers = finding(
            "package-gerbers-missing",
            Severity::High,
            "Coverage",
            "Missing evidence",
            "No Gerbers".into(),
            "Provide Gerbers",
            "Package",
            "package",
        );
        assert_eq!(checks_score(&[missing_gerbers]), 100);

        let static_via = finding(
            "via-diameter",
            Severity::High,
            "Drills",
            "Via diameters fall below the active rule",
            "252 vias".into(),
            "Confirm the process",
            "Board-wide",
            "static",
        );
        let profile_via = finding(
            "kicad-profile-summary-1",
            Severity::Medium,
            "Fabricator DRC",
            "KiCad via diameter",
            "199 markers".into(),
            "Confirm the process",
            "KiCad DRC report",
            "kicad-cli-profile",
        );
        assert_eq!(checks_score(std::slice::from_ref(&static_via)), 90);
        assert_eq!(checks_score(&[static_via, profile_via]), 90);
    }

    #[test]
    fn profile_delta_removes_only_matching_baseline_markers() {
        let baseline = NativeViolation {
            id: "baseline".into(),
            group: "violations".into(),
            violation_type: "clearance".into(),
            severity: "error".into(),
            description: "Clearance violation".into(),
            items: vec![serde_json::json!({"pos": {"x": 1, "y": 2}})],
            excluded: Some(false),
            comment: None,
            sheet_path: None,
            sheet_uuid_path: None,
            structural_location: "channel=violations;sheet=root;items=none;index=0".into(),
        };
        let mut added = baseline.clone();
        added.id = "profile".into();
        added.items = vec![serde_json::json!({"pos": {"x": 3, "y": 4}})];
        let delta = added_profile_violations(
            std::slice::from_ref(&baseline),
            vec![baseline.clone(), baseline.clone(), added.clone()],
        );
        assert_eq!(delta.len(), 2);
        assert!(delta.iter().any(|violation| violation.items == added.items));
    }

    #[test]
    fn profile_staging_preserves_project_context_and_rules() {
        let source = temp_test_dir("profile-source");
        let destination = temp_test_dir("profile-stage");
        fs::create_dir_all(source.join("boards/local.pretty")).unwrap();
        let board = source.join("boards/main.kicad_pcb");
        fs::write(&board, CLEAN).unwrap();
        fs::write(source.join("boards/main.kicad_pro"), "{}").unwrap();
        fs::write(
            source.join("boards/main.kicad_dru"),
            "(version 1)\n(rule \"Existing\" (constraint clearance (min 0.2mm)))\n",
        )
        .unwrap();
        fs::write(
            source.join("boards/fp-lib-table"),
            "(fp_lib_table (lib (name local)))",
        )
        .unwrap();
        fs::write(
            source.join("boards/local.pretty/local.kicad_mod"),
            "(footprint \"local\")",
        )
        .unwrap();

        let (staged_board, skipped) = stage_project(&source, &board, &destination).unwrap();
        assert_eq!(skipped, 0);
        assert!(staged_board.exists());
        assert!(destination.join("boards/main.kicad_pro").exists());
        assert!(destination.join("boards/fp-lib-table").exists());
        assert!(
            destination
                .join("boards/local.pretty/local.kicad_mod")
                .exists()
        );

        let rules_path = staged_board.with_extension("kicad_dru");
        append_profile_rules(&rules_path, Preset::named("standard").unwrap()).unwrap();
        let rules = fs::read_to_string(rules_path).unwrap();
        assert!(rules.contains("rule \"Existing\""));
        assert!(rules.contains("rule \"RateMyPCB profile track width\""));

        fs::remove_dir_all(source).unwrap();
        fs::remove_dir_all(destination).unwrap();
    }
    #[test]
    fn evidence_category_collects_missing_coverage() {
        let coverage = vec![
            Coverage {
                id: "source-structure".into(),
                label: "Source".into(),
                status: CoverageStatus::Passed,
                evidence: "parsed".into(),
            },
            Coverage {
                id: "supply-snapshot".into(),
                label: "Supply".into(),
                status: CoverageStatus::NotProvided,
                evidence: "missing".into(),
            },
        ];
        let categories = category_summaries(ReviewScope::Full, &coverage, &[], &[]);
        let evidence = categories
            .iter()
            .find(|category| category.id == "evidence-coverage")
            .unwrap();
        assert_eq!(evidence.status, "not-run");
        assert_eq!(evidence.coverage_ids, ["supply-snapshot"]);
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
    fn jlcpcb_profile_uses_the_via_ring_rule() {
        let source = CLEAN.replace(
            "(zone (net 1)",
            "(via (at 3 3) (size 0.45) (drill 0.30) (layers \"F.Cu\" \"B.Cu\") (net 1)) (zone (net 1)",
        );
        let facts = parse_board(&source).unwrap();
        let (preset, _) = Preset::profile("jlcpcb").unwrap();
        assert_eq!(preset.annular, 0.075);
        assert!(
            !static_findings(&facts, preset)
                .iter()
                .any(|finding| finding.id == "annular-width")
        );
    }
    #[test]
    fn flags_suspicious_mask_and_paste_layers() {
        let source = CLEAN.replace(
            "(layers \"F.Cu\" \"F.Paste\" \"F.Mask\")",
            "(layers \"F.Cu\" \"F.Paste\") (solder_paste_margin -1)",
        );
        let facts = parse_board(&source).unwrap();
        let ids: BTreeSet<_> = static_findings(&facts, Preset::named("standard").unwrap())
            .into_iter()
            .map(|finding| finding.id)
            .collect();
        assert!(ids.contains("solder-mask-configuration"));
        assert!(ids.contains("solder-paste-configuration"));
    }
    #[test]
    fn rejects_unsafe_archive_paths() {
        assert!(!safe_archive_path("../secret"));
        assert!(!safe_archive_path("/absolute"));
        assert!(safe_archive_path("fab/top.gtl"));
    }

    #[test]
    fn fabrication_archive_and_normalized_path_limits_are_exact_and_one_over() {
        assert!(archive_compressed_size_valid(MAX_ARCHIVE_BYTES));
        assert!(!archive_compressed_size_valid(MAX_ARCHIVE_BYTES + 1));
        assert!(!archive_compressed_size_valid(0));
        assert_eq!(
            add_archive_expanded_bytes(0, MAX_EXPANDED_BYTES).unwrap(),
            MAX_EXPANDED_BYTES
        );
        assert!(add_archive_expanded_bytes(0, MAX_EXPANDED_BYTES + 1).is_err());
        assert!(archive_entry_count_valid(MAX_ENTRIES));
        assert!(!archive_entry_count_valid(MAX_ENTRIES + 1));

        let exact_path = "x".repeat(fabrication::MANUFACTURING_LIMITS.normalized_path_bytes);
        assert!(safe_archive_path(&exact_path));
        assert!(!safe_archive_path(&format!("{exact_path}x")));
        let exact_depth =
            vec!["d"; usize::from(fabrication::MANUFACTURING_LIMITS.directory_depth) + 1].join("/");
        assert!(safe_archive_path(&exact_depth));
        let over_depth =
            vec!["d"; usize::from(fabrication::MANUFACTURING_LIMITS.directory_depth) + 2].join("/");
        assert!(!safe_archive_path(&over_depth));
    }

    #[test]
    fn classifies_core_gerber_roles() {
        assert_eq!(gerber_role("fab/board-F_Cu.gtl"), Some("top-copper"));
        assert_eq!(gerber_role("fab/board-B_Cu.gbl"), Some("bottom-copper"));
        assert_eq!(gerber_role("fab/board-Edge_Cuts.gko"), Some("profile"));
        assert_eq!(gerber_role("fab/board-Edge_Cuts.gm1"), Some("profile"));
        assert_eq!(
            classify("fab/board-Edge_Cuts.gm1"),
            Some(("gerber", "rs-274x"))
        );
        assert_eq!(gerber_role("fab/board-CuTop.gbr"), Some("top-copper"));
        assert_eq!(gerber_role("fab/board-CuBottom.gbr"), Some("bottom-copper"));
        assert_eq!(gerber_role("fab/board-EdgeCuts.gbr"), Some("profile"));
    }
    #[test]
    fn schema_is_versioned() {
        assert_eq!(
            report_schema()["properties"]["schemaVersion"]["const"],
            SCHEMA_VERSION
        );
        let four_layer = crate::stackup::Stackup::from_kicad_source(
            "(kicad_pcb (version 20241229) (stackup (layer \"F.Cu\" (type copper) (thickness 0.035)) (layer \"dielectric 1\" (type dielectric) (material \"FR4\") (thickness 0.5)) (layer \"In1.Cu\" (type copper) (thickness 0.0175)) (layer \"dielectric 2\" (type core) (material \"FR4\") (thickness 0.9)) (layer \"In2.Cu\" (type copper) (thickness 0.0175)) (layer \"dielectric 3\" (type dielectric) (material \"FR4\") (thickness 0.5)) (layer \"B.Cu\" (type copper) (thickness 0.035))))",
        )
        .expect("stackup parses");
        assert_eq!(four_layer.layer_count, 4);
        let serialized = serde_json::to_value(&four_layer).unwrap();
        assert_eq!(serialized["source"], "kicad");
        assert_eq!(serialized["layerCount"], 4);
        assert_eq!(serialized["layers"].as_array().unwrap().len(), 7);
        assert_eq!(
            serialized["layers"][0],
            serde_json::json!({ "name": "F.Cu", "kind": "copper", "thicknessMm": 0.035, "material": null })
        );
        assert!((serialized["thicknessMm"].as_f64().unwrap() - 2.005).abs() < 1e-9);
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
        let (clean, coverage) = bom_review(
            "Reference,Manufacturer,MPN\nR1,Yageo,RC0603FR-0710KL\n",
            Some(&facts),
        );
        assert!(clean.is_empty());
        assert!(matches!(coverage.status, CoverageStatus::Passed));

        let (findings, _) = bom_review("Designator,Value\nR2,10k\n", Some(&facts));
        let ids: BTreeSet<_> = findings.iter().map(|item| item.id.as_str()).collect();
        assert!(ids.contains("bom-mpn-coverage"));
        assert!(ids.contains("bom-missing-references"));
        assert!(ids.contains("bom-unknown-references"));
    }

    #[test]
    fn expands_grouped_bom_reference_ranges() {
        let lines = parse_bom_lines(
            "References,Qty,Value,Footprint,Manufacturer,MPN\nR2-R4,3,10k,0603,Yageo,RC0603FR-0710KL\n",
        );
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].references, ["R2", "R3", "R4"]);
        assert_eq!(lines[0].quantity, Some(3));
        let (findings, _) = bom_review(
            "References,Qty,Value,Footprint,Manufacturer,MPN\nR2-R4,3,10k,0603,Yageo,RC0603FR-0710KL\n",
            None,
        );
        assert!(
            !findings
                .iter()
                .any(|finding| finding.id == "bom-quantity-mismatch")
        );
        assert!(
            category_summaries(ReviewScope::Assembly, &[], &findings, &[])
                .iter()
                .any(|category| category.id == "bom")
        );
    }

    fn supply_fixture() -> Value {
        serde_json::from_str(include_str!(
            "../../../tests/fixtures/supply/synthetic-demand-v2.json"
        ))
        .unwrap()
    }

    fn supply_test_lines() -> Vec<BomLineReview> {
        parse_bom_lines("Reference,Manufacturer,MPN,Quantity\nU1,Acme Semiconductor,ABC-123,2\n")
    }

    #[test]
    fn supply_v2_uses_exact_identity_demand_and_independent_provider_states() {
        let mut lines = parse_bom_lines(
            "References,Qty,Manufacturer,MPN\nU1,2,  Acme   Semiconductor ,abc-123\nU2,1,Acme Semiconductor,NOT-MATCHED\n",
        );
        let summary = supply::evaluate_trusted_fixture(
            include_str!("../../../tests/fixtures/supply/synthetic-demand-v2.json"),
            &mut lines,
            2,
        )
        .unwrap();
        assert!(summary.fresh);
        assert_eq!(lines[0].required_quantity, Some(23));
        assert_eq!(lines[0].offers[0].purchasable_quantity, Some(30));
        assert_eq!(lines[0].unit_price_decimal.as_deref(), Some("0.7500"));
        assert_eq!(lines[0].provider_checks[0].status, "checked");
        assert_eq!(
            lines[0].provider_checks[1].error_kind.as_deref(),
            Some("quota")
        );
        assert_eq!(lines[0].provider_checks[2].status, "not-checked");
        assert_eq!(lines[0].alternatives.status, "not-checked");
        assert_eq!(
            lines[0].alternate_candidates[0].evidence_id,
            "synthetic-alternate-1"
        );
        assert!(lines[0].approved_alternates.is_empty());
        assert_eq!(lines[1].identity.status, "attention");
    }

    #[test]
    fn reference_fixture_uses_diminishing_score() {
        let board = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/narrow-board.kicad_pcb");
        let report = review(
            &board,
            ReviewOptions {
                board: None,
                schematic: None,
                bom: None,
                placement: None,
                supply_snapshot: None,
                dfm_declarations: None,
                preset: Preset::named("standard").unwrap(),
                native: NativeMode::Off,
                tool_version: "test".into(),
                scope: ReviewScope::Full,
                profile: None,
            },
        )
        .unwrap();
        let ids: BTreeSet<_> = report
            .findings
            .iter()
            .map(|item| evidence_check_id(&item.id, &report.evidence))
            .collect();
        assert_eq!(report.score.raw, 60);
        assert_eq!(ids.len(), 14);
        for check_id in [
            "annular-width",
            "dfm.finish-profile.v1/gap/castellation",
            "dfm.finish-profile.v1/gap/edge-plating",
            "dfm.finish-profile.v1/gap/finish",
            "dfm.finish-profile.v1/gap/profile",
            "dfm.impedance-special-process.v1/gap/impedance",
            "dfm.impedance-special-process.v1/gap/special-process",
            "dfm.stackup-order-confirmation.v1/gap/stackup-order",
            "dfm.total-thickness-material.v1/gap/thickness-material",
            "ground-zone",
            "track-width",
            "via-diameter",
            "via-drill",
        ] {
            assert!(ids.contains(check_id), "{check_id}");
        }
        assert_eq!(
            ids.iter()
                .filter(|check_id| check_id.starts_with("dfm.drill-span-plating.v1/gap/tool/"))
                .count(),
            1
        );
    }

    #[test]
    fn explicit_bom_is_checked_with_a_standalone_board() {
        let root = temp_test_dir("explicit-bom");
        let board = root.join("board.kicad_pcb");
        let bom = root.join("assembly.csv");
        fs::write(&board, CLEAN).unwrap();
        fs::write(
            &bom,
            "Reference,Manufacturer,MPN,Quantity\nR1,Yageo,RC0603FR-0710KL,1\n",
        )
        .unwrap();
        let report = review(
            &board,
            ReviewOptions {
                board: None,
                schematic: None,
                bom: Some(bom),
                placement: None,
                supply_snapshot: None,
                dfm_declarations: None,
                preset: Preset::named("standard").unwrap(),
                native: NativeMode::Off,
                tool_version: "test".into(),
                scope: ReviewScope::Full,
                profile: None,
            },
        )
        .unwrap();
        assert!(report.coverage.iter().any(|item| {
            evidence_check_id(&item.id, &report.evidence) == "bom-structure"
                && matches!(item.status, CoverageStatus::Passed)
        }));
        assert!(
            report
                .artifacts
                .iter()
                .any(|artifact| artifact.kind == "bom" && artifact.selected)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn placement_and_conservative_v1_supply_checks_are_structured() {
        let facts = parse_board(CLEAN).unwrap();
        let (placement_findings, placement) = placement_review(
            "Reference,PosX,PosY,Rotation,Side\nR1,1.0,2.0,90,Top\n",
            Some(&facts),
        );
        assert!(placement_findings.is_empty());
        assert!(matches!(placement.status, CoverageStatus::Passed));

        let mut lines =
            parse_bom_lines("Reference,Manufacturer,MPN,Quantity\nR1,Yageo,RC0603FR-0710KL,1\n");
        let source = r#"{"schemaVersion":"1.0","provider":"legacy","generatedAtUnix":1,"parts":[{"manufacturer":"Yageo","mpn":"RC0603FR-0710KL","stock":100,"unitPrice":0.01,"alternates":["UNAPPROVED"]}]}"#;
        let summary = supply::evaluate(source, &mut lines, 2).unwrap();
        assert!(summary.imported_v1);
        assert!(
            lines[0]
                .provider_checks
                .iter()
                .all(|check| check.status == "not-checked")
        );
        assert_eq!(lines[0].stock, None);
        assert_eq!(lines[0].pricing.status, "not-checked");
        assert_eq!(lines[0].alternatives.status, "not-checked");
    }

    #[test]
    fn supply_v2_rejects_float_money_and_duplicate_price_breaks() {
        let mut value: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/supply/synthetic-demand-v2.json"
        ))
        .unwrap();
        value["parts"][0]["offers"][0]["priceBreaks"][0]["unitPrice"] = serde_json::json!(1.0);
        let mut lines = parse_bom_lines(
            "Reference,Manufacturer,MPN,Quantity\nU1,Acme Semiconductor,ABC-123,1\n",
        );
        assert!(supply::evaluate_trusted_fixture(&value.to_string(), &mut lines, 2).is_err());

        let mut value: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/supply/synthetic-demand-v2.json"
        ))
        .unwrap();
        value["parts"][0]["offers"][0]["priceBreaks"][1]["quantity"] = serde_json::json!(1);
        assert!(supply::evaluate_trusted_fixture(&value.to_string(), &mut lines, 2).is_err());

        let mut value: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/supply/synthetic-demand-v2.json"
        ))
        .unwrap();
        value["terms"].as_array_mut().unwrap().pop();
        assert!(supply::evaluate_trusted_fixture(&value.to_string(), &mut lines, 2).is_err());

        let mut value: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/supply/synthetic-demand-v2.json"
        ))
        .unwrap();
        value["parts"][0]["offers"][0]["provenance"]["synthetic"] = serde_json::json!(false);
        value["terms"][0]["decision"] = serde_json::json!("approved");
        assert!(supply::evaluate_trusted_fixture(&value.to_string(), &mut lines, 2).is_err());

        let mut value: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/supply/synthetic-demand-v2.json"
        ))
        .unwrap();
        value["parts"][0]["approvedAlternates"] = serde_json::json!([{
            "identity": {"manufacturer": "Acme Semiconductor", "mpn": "ABC-124"},
            "authorityKind": "engineering",
            "authority": "Release engineering",
            "approvedAtUnix": 1,
            "evidenceRefs": ["synthetic-alternate-1"]
        }]);
        supply::evaluate_trusted_fixture(&value.to_string(), &mut lines, 2).unwrap();
        value["parts"][0]["approvedAlternates"][0]["evidenceRefs"] =
            serde_json::json!(["unresolved"]);
        assert!(supply::evaluate_trusted_fixture(&value.to_string(), &mut lines, 2).is_err());
    }

    #[test]
    fn supply_v2_rejects_stale_future_contradictory_and_unbounded_records() {
        type Mutation = Box<dyn Fn(&mut Value)>;
        let mutations: Vec<Mutation> = vec![
            Box::new(|value| value["generatedAtUnix"] = serde_json::json!(1000)),
            Box::new(|value| {
                value["parts"][0]["providerChecks"][0]["retrievedAtUnix"] = serde_json::json!(1000)
            }),
            Box::new(|value| {
                value["parts"][0]["lifecycleAssertions"][0]["observedAtUnix"] =
                    serde_json::json!(1000)
            }),
            Box::new(|value| {
                value["parts"][0]["providerChecks"][0]["status"] = serde_json::json!("not-found")
            }),
            Box::new(|value| value["terms"][0]["provider"] = serde_json::json!("Mouser")),
            Box::new(|value| {
                let extra = value["parts"][0]["providerChecks"][2].clone();
                value["parts"][0]["providerChecks"]
                    .as_array_mut()
                    .unwrap()
                    .push(extra);
            }),
            Box::new(|value| {
                let price = value["parts"][0]["offers"][0]["priceBreaks"][0].clone();
                value["parts"][0]["offers"][0]["priceBreaks"] = Value::Array(
                    (0..65)
                        .map(|index| {
                            let mut price = price.clone();
                            price["quantity"] = serde_json::json!(index + 1);
                            price
                        })
                        .collect(),
                );
            }),
        ];
        for mutate in mutations {
            let mut value = supply_fixture();
            mutate(&mut value);
            assert!(
                supply::evaluate_trusted_fixture(&value.to_string(), &mut supply_test_lines(), 2)
                    .is_err()
            );
        }
        assert!(
            supply::evaluate_trusted_fixture(
                &supply_fixture().to_string(),
                &mut supply_test_lines(),
                86_402
            )
            .is_err(),
            "expired snapshots are rejected"
        );
        assert!(
            supply::evaluate(&supply_fixture().to_string(), &mut supply_test_lines(), 2).is_err(),
            "untrusted snapshots cannot self-assert synthetic authority"
        );
    }

    #[test]
    fn supply_v2_price_applicability_requires_region_package_quantity_and_currency() {
        for (field, replacement) in [
            ("region", serde_json::json!("EU")),
            ("packaging", serde_json::json!("tray")),
        ] {
            let mut value = supply_fixture();
            value["parts"][0]["offers"][0][field] = replacement;
            let mut lines = supply_test_lines();
            supply::evaluate_trusted_fixture(&value.to_string(), &mut lines, 2).unwrap();
            assert_eq!(lines[0].offers[0].applicable_unit_price, None);
            assert!(!lines[0].offers[0].usable);
        }
        let mut value = supply_fixture();
        value["demand"]["currency"] = serde_json::json!("EUR");
        let mut lines = supply_test_lines();
        supply::evaluate_trusted_fixture(&value.to_string(), &mut lines, 2).unwrap();
        assert_eq!(lines[0].offers[0].applicable_unit_price, None);

        let mut value = supply_fixture();
        value["parts"][0]["offers"][0]["orderMultiple"] = Value::Null;
        let mut lines = supply_test_lines();
        supply::evaluate_trusted_fixture(&value.to_string(), &mut lines, 2).unwrap();
        assert_eq!(lines[0].offers[0].applicable_unit_price, None);
    }

    #[test]
    fn supply_v2_survives_review_to_report_without_hiding_unknowns() {
        let root = temp_test_dir("supply-v2-report");
        let board = root.join("board.kicad_pcb");
        let bom = root.join("bom.csv");
        let snapshot = root.join("supply.json");
        fs::write(&board, CLEAN).unwrap();
        fs::write(
            &bom,
            "Reference,Manufacturer,MPN,Quantity\nR1,Acme Semiconductor,ABC-123,2\n",
        )
        .unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut snapshot_value: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/supply/synthetic-demand-v2.json"
        ))
        .unwrap();
        snapshot_value["generatedAtUnix"] = serde_json::json!(now);
        snapshot_value["expiresAtUnix"] = serde_json::json!(now + 86_400);
        snapshot_value["legalExpiresAtUnix"] = serde_json::json!(now + 86_400);
        snapshot_value["parts"][0]["lifecycleAssertions"] = serde_json::json!([]);
        snapshot_value["parts"][0]["offers"] = serde_json::json!([]);
        snapshot_value["parts"][0]["alternateCandidates"] = serde_json::json!([]);
        for check in snapshot_value["parts"][0]["providerChecks"]
            .as_array_mut()
            .unwrap()
        {
            check["status"] = serde_json::json!("not-checked");
            check["errorKind"] = Value::Null;
            check["retrievedAtUnix"] = Value::Null;
            check["upstreamAtUnix"] = Value::Null;
            check["provenance"] = Value::Null;
        }
        fs::write(&snapshot, snapshot_value.to_string()).unwrap();
        let report = review(
            &board,
            ReviewOptions {
                board: None,
                schematic: None,
                bom: Some(bom),
                placement: None,
                supply_snapshot: Some(snapshot),
                dfm_declarations: None,
                preset: Preset::named("standard").unwrap(),
                native: NativeMode::Off,
                tool_version: "test".into(),
                scope: ReviewScope::Full,
                profile: None,
            },
        )
        .unwrap();
        assert!(
            report
                .artifacts
                .iter()
                .any(|artifact| artifact.format == "ratemypcb-supply-2.0")
        );
        assert_eq!(report.bom.lines[0].required_quantity, Some(23));
        assert_eq!(report.bom.lines[0].provider_checks[2].status, "not-checked");
        assert_eq!(report.bom.lines[0].release_impact.status, "not-checked");
        assert!(!report.approval_eligible);
        validate_report(&report).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn supply_v1_duplicate_identity_is_ambiguous_not_first_selected() {
        let source = r#"{"schemaVersion":"1.0","generatedAtUnix":1,"parts":[{"manufacturer":"Acme","mpn":"ABC","stock":100},{"manufacturer":" acme ","mpn":"abc","stock":0}]}"#;
        let mut lines = parse_bom_lines("Reference,Manufacturer,MPN,Quantity\nU1,Acme,ABC,1\n");
        supply::evaluate(source, &mut lines, 2).unwrap();
        assert_eq!(lines[0].identity.status, "attention");
        assert!(lines[0].identity.detail.contains("Ambiguous"));
        assert_eq!(lines[0].stock, None);
    }

    fn decision_fixture() -> Report {
        review(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/narrow-board.kicad_pcb"),
            ReviewOptions {
                board: None,
                schematic: None,
                bom: None,
                placement: None,
                supply_snapshot: None,
                dfm_declarations: None,
                preset: Preset::named("standard").unwrap(),
                native: NativeMode::Off,
                tool_version: "test".into(),
                scope: ReviewScope::Full,
                profile: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn decision_contract_tracer_carries_a_blocked_release() {
        let report = decision_fixture();
        validate_report(&report).unwrap();
        assert_eq!(report.schema_version, "2.0");
        assert_eq!(report.observed_risk.score_raw, report.score.raw);
        assert!(!report.required_evidence.is_empty());
        assert!(!report.approval_eligible);
        assert!(matches!(report.freshness, EvidenceFreshness::Unknown));
        let evidence_id = report.findings[0].id.clone();
        let top_unblock = report
            .required_evidence
            .iter()
            .find(|item| {
                item.execution != EvidenceExecution::Completed
                    || item.result != EvidenceResult::Pass
                    || !matches!(
                        item.freshness,
                        EvidenceFreshness::Current | EvidenceFreshness::NotApplicable
                    )
            })
            .unwrap()
            .evidence_id
            .clone();
        let assessment = Assessment {
            assessment_schema_version: ASSESSMENT_SCHEMA_VERSION.into(),
            report_digest: "0".repeat(64),
            rating: 4,
            disposition: "blocked".into(),
            verdict: "Do not release".into(),
            verdict_evidence_refs: vec![evidence_id.clone()],
            rationale: "Observed blockers require revision.".into(),
            category_summaries: vec![AssessmentCategory {
                category_id: "fabrication".into(),
                summary: "Blocked".into(),
                evidence_refs: vec![evidence_id.clone()],
            }],
            actions: vec![AssessmentAction {
                priority: 1,
                title: "Supply the top required evidence".into(),
                rationale: "Required before release".into(),
                evidence_refs: vec![top_unblock],
            }],
            questions: vec![AssessmentQuestion {
                question: "Has the blocker been corrected?".into(),
                evidence_refs: vec![evidence_id],
            }],
        };
        validate_assessment(&report, &assessment).unwrap();
    }

    #[test]
    fn decision_contract_required_states_fail_closed_without_changing_risk() {
        let baseline = decision_fixture();
        let risk = baseline.observed_risk.clone();
        for status in [
            CoverageStatus::Attention,
            CoverageStatus::NotRun,
            CoverageStatus::NotProvided,
            CoverageStatus::Failed,
            CoverageStatus::Unsupported,
            CoverageStatus::Stale,
            CoverageStatus::Unknown,
        ] {
            let mut report = baseline.clone();
            let coverage = report
                .coverage
                .iter_mut()
                .find(|item| evidence_check_id(&item.id, &report.evidence) == "source-structure")
                .unwrap();
            coverage.status = status.clone();
            let record = report
                .evidence
                .iter_mut()
                .find(|item| item.id == coverage.id)
                .unwrap();
            record.provenance.freshness = if status == CoverageStatus::Stale {
                EvidenceFreshness::Stale
            } else {
                EvidenceFreshness::NotApplicable
            };
            report.required_evidence =
                required_evidence_summary(report.review_scope, &report.coverage, &report.evidence);
            if status == CoverageStatus::NotProvided {
                assert!(report.required_evidence.iter().any(|item| {
                    item.check_id == "source-structure"
                        && item.execution == EvidenceExecution::NotProvided
                }));
            }
            report.approval_eligible =
                approval_eligible(&report.required_evidence, &report.findings);
            assert!(!report.approval_eligible, "{status:?} must close approval");
            assert_eq!(report.observed_risk, risk);
            validate_report(&report).unwrap();
        }
    }

    #[test]
    fn decision_contract_rejects_tampered_required_evidence() {
        let mut report = decision_fixture();
        report.required_evidence.clear();
        report.approval_eligible = true;
        assert!(
            validate_report(&report)
                .unwrap_err()
                .to_string()
                .contains("authoritative coverage")
        );
    }

    #[test]
    fn assessment_missing_required_coverage_gets_evidence_bearing_occurrences() {
        let mut coverage = Vec::new();
        ensure_required_coverage_occurrences(ReviewScope::Design, &mut coverage);
        assert_eq!(
            coverage
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec!["source-structure", "native-drc"]
        );
        let digest = "a".repeat(64);
        let evidence = finalize_evidence(
            &mut [],
            &mut coverage,
            &BTreeMap::from([("board".into(), digest.clone())]),
            &digest,
            "test",
            EvidenceVersions {
                native: None,
                profile_native: None,
                schematic_erc: None,
                schematic_parity: None,
            },
        );
        let required = required_evidence_summary(ReviewScope::Design, &coverage, &evidence);
        assert!(required.iter().all(|item| !item.evidence_id.is_empty()));
    }

    #[test]
    fn decision_contract_ids_are_stable_and_sensitive_only_to_canonical_identity() {
        let digest = sha256("artifact");
        let location = BTreeMap::from([
            ("kind".into(), "board".into()),
            ("value".into(), "segment:1".into()),
        ]);
        let id = evidence_id(&digest, "track-width", &location);
        assert_eq!(id, evidence_id(&digest, "track-width", &location));
        assert_ne!(
            id,
            evidence_id(&sha256("changed"), "track-width", &location)
        );
        assert_ne!(id, evidence_id(&digest, "via-width", &location));
        let changed_location = BTreeMap::from([
            ("kind".into(), "board".into()),
            ("value".into(), "segment:2".into()),
        ]);
        assert_ne!(id, evidence_id(&digest, "track-width", &changed_location));

        let first = decision_fixture();
        let second = decision_fixture();
        assert_eq!(
            first
                .findings
                .iter()
                .map(|item| &item.id)
                .collect::<Vec<_>>(),
            second
                .findings
                .iter()
                .map(|item| &item.id)
                .collect::<Vec<_>>()
        );
        let mut cosmetic = first.findings[0].clone();
        cosmetic.title = "Different prose".into();
        cosmetic.evidence = "Different evidence prose".into();
        cosmetic.recommendation = "Different recommendation".into();
        cosmetic.severity = Severity::Info;
        assert_eq!(first.findings[0].id, cosmetic.id);
    }

    #[test]
    fn decision_contract_rejects_more_than_three_actions() {
        let report = decision_fixture();
        let evidence_id = report.findings[0].id.clone();
        let action = AssessmentAction {
            priority: 1,
            title: "Fix blocker".into(),
            rationale: "Required before release".into(),
            evidence_refs: vec![evidence_id.clone()],
        };
        let mut actions = vec![action; 4];
        for (index, action) in actions.iter_mut().enumerate() {
            action.priority = index as u8 + 1;
        }
        let assessment = Assessment {
            assessment_schema_version: ASSESSMENT_SCHEMA_VERSION.into(),
            report_digest: "0".repeat(64),
            rating: 4,
            disposition: "blocked".into(),
            verdict: "Do not release".into(),
            verdict_evidence_refs: vec![evidence_id],
            rationale: "Observed blockers require revision.".into(),
            category_summaries: vec![],
            actions,
            questions: vec![],
        };
        assert!(
            validate_assessment(&report, &assessment)
                .unwrap_err()
                .to_string()
                .contains("at most three actions")
        );
    }

    #[test]
    fn decision_contract_rejects_duplicates_incomplete_provenance_and_broken_questions() {
        let report = decision_fixture();
        let mut duplicate = report.clone();
        duplicate.evidence[1].id = duplicate.evidence[0].id.clone();
        assert!(
            validate_report(&duplicate)
                .unwrap_err()
                .to_string()
                .contains("Duplicate global evidence ID")
        );

        let mut incomplete = report.clone();
        incomplete.evidence[0].provenance.producer.version.clear();
        assert!(
            validate_report(&incomplete)
                .unwrap_err()
                .to_string()
                .contains("Incomplete required provenance")
        );

        let mut unlinked_limitation = report.clone();
        unlinked_limitation.limitation_evidence_refs[0].clear();
        assert!(
            validate_report(&unlinked_limitation)
                .unwrap_err()
                .to_string()
                .contains("visible limitation")
        );

        let evidence_id = report.findings[0].id.clone();
        let assessment = Assessment {
            assessment_schema_version: ASSESSMENT_SCHEMA_VERSION.into(),
            report_digest: "0".repeat(64),
            rating: 1,
            disposition: "blocked".into(),
            verdict: "Blocked".into(),
            verdict_evidence_refs: vec![evidence_id],
            rationale: "Blocked".into(),
            category_summaries: vec![],
            actions: vec![],
            questions: vec![AssessmentQuestion {
                question: "Why?".into(),
                evidence_refs: vec!["ev-unknown".into()],
            }],
        };
        assert!(
            validate_assessment(&report, &assessment)
                .unwrap_err()
                .to_string()
                .contains("unknown evidence ID")
        );
    }

    #[test]
    fn decision_contract_generated_schemas_match_checked_in_json() {
        let checked_report: Value =
            serde_json::from_str(include_str!("../../../schemas/report-2.0.json")).unwrap();
        let checked_assessment: Value =
            serde_json::from_str(include_str!("../../../schemas/assessment-2.0.json")).unwrap();
        assert_eq!(report_schema(), checked_report);
        assert_eq!(assessment_schema(), checked_assessment);
    }

    #[test]
    fn assessment_cannot_approve_incomplete_evidence() {
        let board = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/narrow-board.kicad_pcb");
        let report = review(
            &board,
            ReviewOptions {
                board: None,
                schematic: None,
                bom: None,
                placement: None,
                supply_snapshot: None,
                dfm_declarations: None,
                preset: Preset::named("standard").unwrap(),
                native: NativeMode::Off,
                tool_version: "test".into(),
                scope: ReviewScope::Full,
                profile: None,
            },
        )
        .unwrap();
        let assessment = Assessment {
            assessment_schema_version: ASSESSMENT_SCHEMA_VERSION.into(),
            report_digest: "0".repeat(64),
            rating: 10,
            disposition: "approve".into(),
            verdict: "Would manufacture".into(),
            verdict_evidence_refs: vec!["native-drc".into()],
            rationale: "Test".into(),
            category_summaries: vec![],
            actions: vec![],
            questions: vec![],
        };
        assert!(validate_assessment(&report, &assessment).is_err());
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

    #[test]
    fn manufacturing_input_read_and_hash_deadline_is_cooperative_and_exact() {
        struct CountingReader {
            remaining: usize,
            reads: usize,
            delay: bool,
        }

        impl Read for CountingReader {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                if self.remaining == 0 {
                    return Ok(0);
                }
                if self.delay {
                    std::thread::sleep(Duration::from_micros(100));
                }
                let count = self.remaining.min(buffer.len()).min(1);
                buffer[..count].fill(b'x');
                self.remaining -= count;
                self.reads += 1;
                Ok(count)
            }
        }

        let mut exact = CountingReader {
            remaining: 100_000,
            reads: 0,
            delay: false,
        };
        let started = Instant::now();
        let (bytes, digest) = read_manufacturing_bytes(
            &mut exact,
            "exact.gbr",
            fabrication::ManufacturingDeadline::from_starts(started, started),
        )
        .unwrap();
        assert_eq!(bytes.len(), 100_000);
        assert_eq!(exact.reads, 100_000);
        assert_eq!(digest, sha256(&bytes));

        let mut expiring = CountingReader {
            remaining: 100_000,
            reads: 0,
            delay: true,
        };
        let aggregate_started = Instant::now();
        let file_started = aggregate_started
            .checked_sub(Duration::from_millis(
                fabrication::MANUFACTURING_LIMITS.file_timeout_ms - 20,
            ))
            .unwrap();
        assert!(
            read_manufacturing_bytes(
                &mut expiring,
                "expiring.gbr",
                fabrication::ManufacturingDeadline::from_starts(file_started, aggregate_started,),
            )
            .is_err()
        );
        assert!(expiring.reads > 0 && expiring.reads < 100_000);
    }
}
