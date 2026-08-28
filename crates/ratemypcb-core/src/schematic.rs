use crate::{Coverage, CoverageStatus, Error, Finding, GateImpact, NativeMode, Severity};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(crate) const MAX_NATIVE_REPORT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_NATIVE_DIAGNOSTIC_BYTES: u64 = 16 * 1024;
const MAX_NATIVE_MARKERS: usize = 250;
const MAX_NATIVE_ITEMS: usize = 64;
const MAX_NATIVE_METADATA_ITEMS: usize = 256;
const MAX_NATIVE_TEXT_BYTES: usize = 4096;
const NATIVE_TIMEOUT: Duration = Duration::from_secs(120);
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

const MAX_SCHEMATIC_BYTES: usize = 2 * 1024 * 1024;
const MAX_SCHEMATIC_AGGREGATE_BYTES: usize = 8 * 1024 * 1024;
const MAX_SCHEMATIC_TOKENS: usize = 200_000;
const MAX_SCHEMATIC_DEPTH: usize = 64;
const MAX_SCHEMATIC_TEXT: usize = 4096;
const MAX_SCHEMATIC_CHILDREN: usize = 512;
const MAX_SCHEMATIC_OCCURRENCES: usize = 20_000;
const MAX_GENERIC_RECORDS: usize = 20_000;
pub(crate) const NATIVE_FACTS_DIGEST_KEY: &str = "schematic:native-export-facts";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchematicReview {
    pub status: String,
    pub project_identity: Option<String>,
    pub root_path: Option<String>,
    pub root_digest: Option<String>,
    pub board_path: Option<String>,
    pub board_digest: Option<String>,
    pub source_pair: Option<SchematicSourcePair>,
    pub artifact_digests: BTreeMap<String, String>,
    pub declared_revisions: BTreeMap<String, String>,
    pub occurrence_count: usize,
    pub occurrences: Vec<SchematicOccurrence>,
    pub capabilities: Vec<SchematicCapability>,
    pub mismatches: Vec<SchematicMismatch>,
    pub native_erc: Option<NativeReport>,
    pub native_parity: Option<NativeReport>,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchematicSourcePair {
    pub project_identity: String,
    pub schematic_path: String,
    pub schematic_digest: String,
    pub board_path: String,
    pub board_digest: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchematicCapability {
    pub id: String,
    pub status: String,
    pub producer: String,
    pub evidence_class: String,
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchematicOccurrence {
    pub key: String,
    pub project_identity: String,
    pub root_digest: String,
    pub sheet_uuid_path: String,
    pub item_uuid: String,
    pub source_path: String,
    pub reference: Option<String>,
    pub unit: Option<String>,
    pub facts: Vec<SchematicFact>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchematicFact {
    pub name: String,
    pub value: String,
    pub producer: String,
    pub evidence_class: String,
    pub source_path: String,
    pub confidence: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchematicMismatch {
    pub check_id: String,
    pub field: String,
    pub expected: String,
    pub actual: String,
    pub join: String,
    pub confidence: String,
    pub gate_impact: GateImpact,
    pub location: String,
}

pub(crate) struct ProjectEvidenceInput<'a> {
    pub input_kind: &'a str,
    pub project_root: Option<&'a Path>,
    pub board_name: Option<&'a str>,
    pub board_source: Option<&'a str>,
    pub schematics: &'a BTreeMap<String, String>,
    pub root_hint: Option<&'a str>,
    pub root_selector: Option<&'a str>,
    pub projects: &'a BTreeSet<String>,
    pub project_variables: &'a BTreeMap<String, String>,
    pub altium_schematics: &'a [String],
    pub netlists: &'a BTreeMap<String, String>,
    pub bom: Option<(&'a str, &'a str)>,
    pub placement: Option<(&'a str, &'a str)>,
    pub native_mode: NativeMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KiCadMajor {
    V8,
    V9,
    V10,
}

impl KiCadMajor {
    pub(crate) fn parse(version: &str) -> Option<Self> {
        match version
            .split(|character: char| !character.is_ascii_digit())
            .find(|part| !part.is_empty())?
            .parse::<u32>()
            .ok()?
        {
            8 => Some(Self::V8),
            9 => Some(Self::V9),
            10 => Some(Self::V10),
            _ => None,
        }
    }

    fn number(self) -> u32 {
        match self {
            Self::V8 => 8,
            Self::V9 => 9,
            Self::V10 => 10,
        }
    }
}

#[allow(dead_code)] // ERC dispatch is established here and wired to selected roots in plan 04-02.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeKind {
    Erc,
    Drc { schematic_parity: bool },
}

impl NativeKind {
    fn args(self, output: &Path, input: &Path) -> Vec<String> {
        let mut args = match self {
            Self::Erc => vec!["sch", "erc"],
            Self::Drc { .. } => vec!["pcb", "drc"],
        }
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
        args.extend(
            ["--format", "json", "--severity-all"]
                .into_iter()
                .map(str::to_owned),
        );
        if matches!(
            self,
            Self::Drc {
                schematic_parity: true
            }
        ) {
            args.push("--schematic-parity".into());
        }
        args.extend(
            ["--exit-code-violations", "--output"]
                .into_iter()
                .map(str::to_owned),
        );
        args.push(output.to_string_lossy().into_owned());
        args.push(input.to_string_lossy().into_owned());
        args
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeReport {
    pub status: String,
    pub tool: String,
    pub version: Option<String>,
    #[serde(default)]
    pub report_version: Option<String>,
    pub finding_count: usize,
    #[serde(default)]
    pub excluded_count: usize,
    #[serde(default)]
    pub unknown_exclusion_count: usize,
    pub note: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub included_severities: Vec<Value>,
    #[serde(default)]
    pub ignored_checks: Vec<Value>,
    #[serde(default)]
    pub violations: Vec<NativeMarker>,
}

impl NativeReport {
    pub(crate) fn not_run(version: Option<String>, note: impl Into<String>) -> Self {
        Self {
            status: "not_run".into(),
            tool: "kicad-cli".into(),
            version,
            report_version: None,
            finding_count: 0,
            excluded_count: 0,
            unknown_exclusion_count: 0,
            note: note.into(),
            source: None,
            date: None,
            included_severities: vec![],
            ignored_checks: vec![],
            violations: vec![],
        }
    }

    pub(crate) fn disabled() -> Self {
        let mut report = Self::not_run(None, "Native DRC was disabled by --native off.");
        report.status = "disabled".into();
        report
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeMarker {
    pub id: String,
    pub group: String,
    pub violation_type: String,
    pub severity: String,
    pub description: String,
    #[serde(default)]
    pub items: Vec<Value>,
    pub excluded: Option<bool>,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub sheet_path: Option<String>,
    #[serde(default)]
    pub sheet_uuid_path: Option<String>,
    pub structural_location: String,
}

#[derive(Debug)]
struct NativeFailure {
    version: Option<String>,
    message: String,
}

struct TempDir(PathBuf);

impl TempDir {
    fn create() -> Result<Self, NativeFailure> {
        for _ in 0..16 {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_nanos();
            let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "ratemypcb-native-{}-{nonce}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(NativeFailure {
                        version: None,
                        message: format!("Cannot create native report directory: {error}"),
                    });
                }
            }
        }
        Err(NativeFailure {
            version: None,
            message: "Cannot allocate a fresh native report directory.".into(),
        })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn bounded_file(path: &Path, limit: u64, label: &str) -> Result<Vec<u8>, NativeFailure> {
    let metadata = fs::metadata(path).map_err(|error| NativeFailure {
        version: None,
        message: format!("Cannot read {label}: {error}"),
    })?;
    if metadata.len() > limit {
        return Err(NativeFailure {
            version: None,
            message: format!("{label} exceeds the {limit}-byte limit."),
        });
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)
        .and_then(|file| file.take(limit + 1).read_to_end(&mut bytes))
        .map_err(|error| NativeFailure {
            version: None,
            message: format!("Cannot read {label}: {error}"),
        })?;
    Ok(bytes)
}

fn wait_with_timeout(
    child: &mut std::process::Child,
    timeout: Duration,
    watched: &[(&Path, u64)],
) -> Result<i32, NativeFailure> {
    let started = std::time::Instant::now();
    loop {
        if let Some((path, limit)) = watched
            .iter()
            .find(|(path, limit)| fs::metadata(path).is_ok_and(|metadata| metadata.len() > *limit))
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(NativeFailure {
                version: None,
                message: format!(
                    "kicad-cli output {} exceeds the {limit}-byte limit.",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ),
            });
        }
        if let Some(status) = child.try_wait().map_err(|error| NativeFailure {
            version: None,
            message: format!("Cannot wait for kicad-cli: {error}"),
        })? {
            return Ok(status.code().unwrap_or(3));
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(NativeFailure {
                version: None,
                message: format!("kicad-cli timed out after {} seconds.", timeout.as_secs()),
            });
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn run_command(
    executable: &Path,
    args: &[String],
    cwd: &Path,
    stdout_path: &Path,
    stderr_path: &Path,
    timeout: Duration,
    additional_watched: &[(&Path, u64)],
) -> Result<i32, NativeFailure> {
    let stdout = File::create(stdout_path).map_err(|error| NativeFailure {
        version: None,
        message: format!("Cannot create bounded native stdout: {error}"),
    })?;
    let stderr = File::create(stderr_path).map_err(|error| NativeFailure {
        version: None,
        message: format!("Cannot create bounded native stderr: {error}"),
    })?;
    let mut child = Command::new(executable)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .map_err(|error| NativeFailure {
            version: None,
            message: format!("Cannot run kicad-cli: {error}"),
        })?;
    let mut watched = vec![
        (stdout_path, MAX_NATIVE_DIAGNOSTIC_BYTES),
        (stderr_path, MAX_NATIVE_DIAGNOSTIC_BYTES),
    ];
    watched.extend_from_slice(additional_watched);
    wait_with_timeout(&mut child, timeout, &watched)
}

fn bounded_cause(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(2048)
        .collect()
}

fn bounded_diagnostic(path: &Path) -> String {
    bounded_file(path, MAX_NATIVE_DIAGNOSTIC_BYTES, "kicad-cli diagnostic")
        .map(|bytes| {
            bounded_cause(
                &String::from_utf8_lossy(&bytes)
                    .split_whitespace()
                    .take(64)
                    .collect::<Vec<_>>()
                    .join(" "),
            )
        })
        .unwrap_or_else(|error| bounded_cause(&error.message))
}

fn executable_version(
    executable: &Path,
    temp: &TempDir,
    timeout: Duration,
) -> Result<String, NativeFailure> {
    let stdout = temp.0.join("version.out");
    let stderr = temp.0.join("version.err");
    let cwd = std::env::current_dir().map_err(|error| NativeFailure {
        version: None,
        message: format!("Cannot resolve current directory: {error}"),
    })?;
    let exit = run_command(
        executable,
        &["version".into()],
        &cwd,
        &stdout,
        &stderr,
        timeout,
        &[],
    )?;
    if exit != 0 {
        return Err(NativeFailure {
            version: None,
            message: format!(
                "kicad-cli version exited {exit}: {}",
                bounded_diagnostic(&stderr)
            ),
        });
    }
    let bytes = bounded_file(&stdout, 512, "kicad-cli version output")?;
    let version = String::from_utf8(bytes)
        .map_err(|_| NativeFailure {
            version: None,
            message: "kicad-cli version output is not UTF-8.".into(),
        })?
        .trim()
        .to_owned();
    if version.is_empty() {
        return Err(NativeFailure {
            version: None,
            message: "kicad-cli version output is empty.".into(),
        });
    }
    Ok(version)
}

fn bounded_string(value: Option<&Value>, field: &str) -> Result<Option<String>, NativeFailure> {
    let Some(value) = value else { return Ok(None) };
    let value = value.as_str().ok_or_else(|| NativeFailure {
        version: None,
        message: format!("Native report field {field} must be a string."),
    })?;
    if value.len() > MAX_NATIVE_TEXT_BYTES {
        return Err(NativeFailure {
            version: None,
            message: format!("Native report field {field} exceeds its text limit."),
        });
    }
    Ok(Some(value.to_owned()))
}

fn value_array(root: &Value, field: &str) -> Result<Vec<Value>, NativeFailure> {
    let Some(value) = root.get(field) else {
        return Ok(vec![]);
    };
    let values = value.as_array().ok_or_else(|| NativeFailure {
        version: None,
        message: format!("Native report field {field} must be an array."),
    })?;
    if values.len() > MAX_NATIVE_METADATA_ITEMS {
        return Err(NativeFailure {
            version: None,
            message: format!("Native report field {field} exceeds its item limit."),
        });
    }
    Ok(values.clone())
}

fn marker_identity(marker: &NativeMarker) -> String {
    let mut item_ids = marker
        .items
        .iter()
        .filter_map(|item| item.get("uuid").and_then(Value::as_str))
        .take(MAX_NATIVE_ITEMS)
        .collect::<Vec<_>>();
    item_ids.sort_unstable();
    format!(
        "channel={};sheet={};type={};items={}",
        marker.group,
        marker.sheet_uuid_path.as_deref().unwrap_or("root"),
        marker.violation_type,
        if item_ids.is_empty() {
            "none".into()
        } else {
            item_ids.join(",")
        }
    )
}

fn stabilize_markers(markers: &mut [NativeMarker]) {
    let mut groups = BTreeMap::<String, Vec<usize>>::new();
    for (index, marker) in markers.iter().enumerate() {
        groups
            .entry(marker_identity(marker))
            .or_default()
            .push(index);
    }
    for (identity, mut indices) in groups {
        indices.sort_by_key(|index| {
            serde_json::to_string(&markers[*index]).unwrap_or_else(|_| identity.clone())
        });
        for (ordinal, index) in indices.into_iter().enumerate() {
            let suffix = if ordinal == 0 {
                String::new()
            } else {
                format!(";duplicate={ordinal}")
            };
            markers[index].structural_location = format!("{identity}{suffix}");
            markers[index].id = format!(
                "kicad-native-{}-{}{}",
                markers[index].group.replace('_', "-"),
                &crate::sha256(identity.as_bytes())[..16],
                if ordinal == 0 {
                    String::new()
                } else {
                    format!("-{ordinal}")
                }
            );
        }
    }
}

fn parse_marker(
    value: &Value,
    group: &str,
    sheet_path: Option<&str>,
    sheet_uuid_path: Option<&str>,
) -> Result<NativeMarker, NativeFailure> {
    let object = value.as_object().ok_or_else(|| NativeFailure {
        version: None,
        message: format!("Native {group} marker must be an object."),
    })?;
    let required = |field: &str| -> Result<String, NativeFailure> {
        bounded_string(object.get(field), field)?.ok_or_else(|| NativeFailure {
            version: None,
            message: format!("Native marker is missing {field}."),
        })
    };
    let items = object
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| NativeFailure {
            version: None,
            message: "Native marker items must be an array.".into(),
        })?;
    if items.len() > MAX_NATIVE_ITEMS {
        return Err(NativeFailure {
            version: None,
            message: "Native marker exceeds its item limit.".into(),
        });
    }
    let excluded = match object.get("excluded") {
        Some(value) => Some(value.as_bool().ok_or_else(|| NativeFailure {
            version: None,
            message: "Native marker excluded field must be boolean.".into(),
        })?),
        None => None,
    };
    Ok(NativeMarker {
        id: String::new(),
        group: group.into(),
        violation_type: required("type")?,
        severity: required("severity")?,
        description: required("description")?,
        items,
        excluded,
        comment: bounded_string(object.get("comment"), "comment")?,
        sheet_path: sheet_path.map(str::to_owned),
        sheet_uuid_path: sheet_uuid_path.map(str::to_owned),
        structural_location: String::new(),
    })
}

fn marker_array(
    root: &Value,
    field: &str,
    markers: &mut Vec<NativeMarker>,
) -> Result<(), NativeFailure> {
    let values = root
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| NativeFailure {
            version: None,
            message: format!("Native DRC report is missing {field} array."),
        })?;
    if markers.len().saturating_add(values.len()) > MAX_NATIVE_MARKERS {
        return Err(NativeFailure {
            version: None,
            message: "Native report exceeds its marker limit.".into(),
        });
    }
    for value in values {
        markers.push(parse_marker(value, field, None, None)?);
    }
    Ok(())
}

#[allow(dead_code)] // Released-major adapters are consumed by hierarchy integration in plan 04-02.
pub(crate) fn parse_native_report(
    bytes: &[u8],
    expected_major: KiCadMajor,
    kind: NativeKind,
) -> Result<NativeReport, String> {
    parse_native_report_inner(bytes, expected_major, kind).map_err(|error| error.message)
}

fn parse_native_report_inner(
    bytes: &[u8],
    expected_major: KiCadMajor,
    kind: NativeKind,
) -> Result<NativeReport, NativeFailure> {
    if bytes.len() as u64 > MAX_NATIVE_REPORT_BYTES {
        return Err(NativeFailure {
            version: None,
            message: "Native report exceeds its byte limit.".into(),
        });
    }
    let root: Value = serde_json::from_slice(bytes).map_err(|error| NativeFailure {
        version: None,
        message: format!("Invalid native JSON report: {error}"),
    })?;
    if !root.is_object() {
        return Err(NativeFailure {
            version: None,
            message: "Native JSON report root must be an object.".into(),
        });
    }
    let report_version =
        bounded_string(root.get("kicad_version"), "kicad_version")?.ok_or_else(|| {
            NativeFailure {
                version: None,
                message: "Native report is missing kicad_version.".into(),
            }
        })?;
    if KiCadMajor::parse(&report_version) != Some(expected_major) {
        return Err(NativeFailure {
            version: Some(report_version),
            message: format!(
                "Native report major does not match executable KiCad {}.",
                expected_major.number()
            ),
        });
    }
    let source = Some(
        bounded_string(root.get("source"), "source")?.ok_or_else(|| NativeFailure {
            version: None,
            message: "Native report is missing source.".into(),
        })?,
    );
    let date = Some(
        bounded_string(root.get("date"), "date")?.ok_or_else(|| NativeFailure {
            version: None,
            message: "Native report is missing date.".into(),
        })?,
    );
    let included_severities = value_array(&root, "included_severities")?;
    let ignored_checks = value_array(&root, "ignored_checks")?;
    let mut markers = Vec::new();
    match kind {
        NativeKind::Drc { .. } => {
            for field in ["violations", "unconnected_items", "schematic_parity"] {
                marker_array(&root, field, &mut markers)?;
            }
        }
        NativeKind::Erc => {
            let sheets = root
                .get("sheets")
                .and_then(Value::as_array)
                .ok_or_else(|| NativeFailure {
                    version: None,
                    message: "Native ERC report is missing sheets array.".into(),
                })?;
            if sheets.len() > MAX_NATIVE_MARKERS {
                return Err(NativeFailure {
                    version: None,
                    message: "Native ERC report exceeds its sheet limit.".into(),
                });
            }
            for sheet in sheets {
                let path = bounded_string(sheet.get("path"), "sheet.path")?.ok_or_else(|| {
                    NativeFailure {
                        version: None,
                        message: "Native ERC sheet is missing path.".into(),
                    }
                })?;
                let uuid_path = bounded_string(sheet.get("uuid_path"), "sheet.uuid_path")?
                    .ok_or_else(|| NativeFailure {
                        version: None,
                        message: "Native ERC sheet is missing uuid_path.".into(),
                    })?;
                let violations = sheet
                    .get("violations")
                    .and_then(Value::as_array)
                    .ok_or_else(|| NativeFailure {
                        version: None,
                        message: "Native ERC sheet is missing violations array.".into(),
                    })?;
                if markers.len().saturating_add(violations.len()) > MAX_NATIVE_MARKERS {
                    return Err(NativeFailure {
                        version: None,
                        message: "Native report exceeds its marker limit.".into(),
                    });
                }
                for value in violations {
                    markers.push(parse_marker(value, "erc", Some(&path), Some(&uuid_path))?);
                }
            }
        }
    }
    stabilize_markers(&mut markers);
    let channel_markers = markers.iter().filter(|marker| match kind {
        NativeKind::Erc => marker.group == "erc",
        NativeKind::Drc {
            schematic_parity: false,
        } => matches!(marker.group.as_str(), "violations" | "unconnected_items"),
        NativeKind::Drc {
            schematic_parity: true,
        } => marker.group == "schematic_parity",
    });
    let finding_count = channel_markers
        .clone()
        .filter(|marker| marker.excluded == Some(false))
        .count();
    let excluded_count = channel_markers
        .clone()
        .filter(|marker| marker.excluded == Some(true))
        .count();
    let unknown_exclusion_count = channel_markers
        .filter(|marker| marker.excluded.is_none())
        .count();
    Ok(NativeReport {
        status: "completed".into(),
        tool: "kicad-cli".into(),
        version: Some(report_version.clone()),
        report_version: Some(report_version),
        finding_count,
        excluded_count,
        unknown_exclusion_count,
        note: format!(
            "KiCad native analysis completed without mutating the source; {excluded_count} excluded and {unknown_exclusion_count} unknown-exclusion marker(s) were retained."
        ),
        source,
        date,
        included_severities,
        ignored_checks,
        violations: markers,
    })
}

fn completed_exit(exit: i32) -> bool {
    matches!(exit, 0 | 5)
}

#[derive(Clone, Copy)]
enum ExportKind {
    Bom,
    Netlist,
    Position,
}

impl ExportKind {
    fn args(self, output: &Path, input: &Path) -> Vec<String> {
        let mut args = match self {
            Self::Bom => vec!["sch", "export", "bom"],
            Self::Netlist => vec!["sch", "export", "netlist", "--format", "kicadsexpr"],
            Self::Position => vec![
                "pcb", "export", "pos", "--format", "csv", "--units", "mm", "--side", "both",
            ],
        }
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
        args.extend(["--output".into(), output.to_string_lossy().into_owned()]);
        args.push(input.to_string_lossy().into_owned());
        args
    }

    fn label(self) -> &'static str {
        match self {
            Self::Bom => "native BOM export",
            Self::Netlist => "native netlist export",
            Self::Position => "native position export",
        }
    }
}

fn run_export_inner(
    executable: &Path,
    input: &Path,
    kind: ExportKind,
) -> Result<(String, String), NativeFailure> {
    let temp = TempDir::create()?;
    let version = executable_version(executable, &temp, NATIVE_TIMEOUT)?;
    if KiCadMajor::parse(&version).is_none() {
        return Err(NativeFailure {
            version: Some(version.clone()),
            message: format!("kicad-cli {version} is unsupported for native export."),
        });
    }
    let input = fs::canonicalize(input).map_err(|error| NativeFailure {
        version: Some(version.clone()),
        message: format!("Cannot resolve native export input: {error}"),
    })?;
    let output = temp.0.join(match kind {
        ExportKind::Bom => "bom.csv",
        ExportKind::Netlist => "netlist.net",
        ExportKind::Position => "positions.csv",
    });
    let stdout = temp.0.join("export.out");
    let stderr = temp.0.join("export.err");
    let args = kind.args(&output, &input);
    let exit = run_command(
        executable,
        &args,
        input.parent().unwrap_or_else(|| Path::new(".")),
        &stdout,
        &stderr,
        NATIVE_TIMEOUT,
        &[(&output, MAX_SCHEMATIC_BYTES as u64)],
    )?;
    if exit != 0 {
        return Err(NativeFailure {
            version: Some(version),
            message: format!(
                "{} exited {exit}: {}",
                kind.label(),
                bounded_diagnostic(&stderr)
            ),
        });
    }
    let source = String::from_utf8(bounded_file(
        &output,
        MAX_SCHEMATIC_BYTES as u64,
        kind.label(),
    )?)
    .map_err(|_| NativeFailure {
        version: Some(version.clone()),
        message: format!("{} is not UTF-8.", kind.label()),
    })?;
    let valid = match kind {
        ExportKind::Bom | ExportKind::Position => delimited_rows(&source).map(|_| ()),
        ExportKind::Netlist => netlist_components(&source).map(|_| ()),
    };
    valid.map_err(|error| NativeFailure {
        version: Some(version.clone()),
        message: format!("Malformed {}: {error}.", kind.label()),
    })?;
    Ok((source, version))
}

fn run_export(
    input: &Path,
    kind: ExportKind,
    mode: NativeMode,
) -> Result<Option<(String, String)>, Error> {
    if mode == NativeMode::Off {
        return Ok(None);
    }
    match run_export_inner(Path::new("kicad-cli"), input, kind) {
        Ok(export) => Ok(Some(export)),
        Err(_) if mode == NativeMode::Auto => Ok(None),
        Err(error) => Err(Error::Native(bounded_cause(&error.message))),
    }
}

fn run_native_inner(
    executable: &Path,
    input: &Path,
    kind: NativeKind,
    timeout: Duration,
) -> Result<NativeReport, NativeFailure> {
    let temp = TempDir::create()?;
    let version = executable_version(executable, &temp, timeout)?;
    let Some(major) = KiCadMajor::parse(&version) else {
        return Err(NativeFailure {
            version: Some(version.clone()),
            message: format!(
                "kicad-cli {version} is unsupported; exactly majors 8, 9, and 10 are supported."
            ),
        });
    };
    let input = fs::canonicalize(input).map_err(|error| NativeFailure {
        version: Some(version.clone()),
        message: format!("Cannot resolve native input {}: {error}", input.display()),
    })?;
    let report_path = temp.0.join(match kind {
        NativeKind::Erc => "report.erc.json",
        NativeKind::Drc { .. } => "report.drc.json",
    });
    let stdout_path = temp.0.join("analysis.out");
    let stderr_path = temp.0.join("analysis.err");
    let cwd = input.parent().unwrap_or_else(|| Path::new("."));
    let args = kind.args(&report_path, &input);
    let exit = run_command(
        executable,
        &args,
        cwd,
        &stdout_path,
        &stderr_path,
        timeout,
        &[(&report_path, MAX_NATIVE_REPORT_BYTES)],
    )
    .map_err(|mut error| {
        error.version = Some(version.clone());
        error
    })?;
    if !completed_exit(exit) {
        return Err(NativeFailure {
            version: Some(version),
            message: format!(
                "kicad-cli exited {exit}: {}",
                bounded_diagnostic(&stderr_path)
            ),
        });
    }
    let bytes = bounded_file(&report_path, MAX_NATIVE_REPORT_BYTES, "native JSON report").map_err(
        |mut error| {
            error.version = Some(version.clone());
            error
        },
    )?;
    let mut report = parse_native_report_inner(&bytes, major, kind).map_err(|mut error| {
        error.version = Some(version.clone());
        error
    })?;
    report.version = Some(version);
    Ok(report)
}

fn finish_native_run(
    result: Result<NativeReport, NativeFailure>,
    mode: NativeMode,
) -> Result<NativeReport, Error> {
    match result {
        Ok(report) => Ok(report),
        Err(error) if mode == NativeMode::Auto => Ok(NativeReport::not_run(
            error.version,
            format!(
                "Native analysis was not run; standalone evidence was preserved: {}",
                bounded_cause(&error.message)
            ),
        )),
        Err(error) => Err(Error::Native(bounded_cause(&error.message))),
    }
}

pub(crate) fn run_native(
    input: &Path,
    kind: NativeKind,
    mode: NativeMode,
) -> Result<NativeReport, Error> {
    if mode == NativeMode::Off {
        return Ok(NativeReport::disabled());
    }
    finish_native_run(
        run_native_inner(Path::new("kicad-cli"), input, kind, NATIVE_TIMEOUT),
        mode,
    )
}

#[derive(Clone, Debug)]
enum SExpr {
    Atom(String),
    List(Vec<SExpr>),
}

impl SExpr {
    fn atom(&self) -> Option<&str> {
        match self {
            Self::Atom(value) => Some(value),
            Self::List(_) => None,
        }
    }

    fn items(&self) -> Option<&[SExpr]> {
        match self {
            Self::List(items) => Some(items),
            Self::Atom(_) => None,
        }
    }

    fn head(&self) -> Option<&str> {
        self.items()?.first()?.atom()
    }

    fn children<'a>(&'a self, head: &str) -> impl Iterator<Item = &'a SExpr> {
        let head = head.to_owned();
        self.items()
            .into_iter()
            .flatten()
            .filter(move |item| item.head() == Some(head.as_str()))
    }

    fn child_value(&self, head: &str) -> Option<&str> {
        self.children(head).next()?.items()?.get(1)?.atom()
    }

    fn descendants<'a>(&'a self, head: &'a str, output: &mut Vec<&'a SExpr>) {
        if self.head() == Some(head) {
            output.push(self);
        }
        if let Some(items) = self.items() {
            for item in items {
                item.descendants(head, output);
            }
        }
    }
}

fn parse_sexpr(source: &str, expected_root: &[&str]) -> Result<SExpr, String> {
    if source.is_empty() || source.len() > MAX_SCHEMATIC_BYTES {
        return Err(format!(
            "source must be between 1 and {MAX_SCHEMATIC_BYTES} bytes"
        ));
    }
    let bytes = source.as_bytes();
    let mut stack: Vec<Vec<SExpr>> = Vec::new();
    let mut roots = Vec::new();
    let mut cursor = 0;
    let mut tokens = 0;
    while cursor < bytes.len() {
        match bytes[cursor] {
            byte if byte.is_ascii_whitespace() => cursor += 1,
            b';' => {
                cursor += 1;
                while cursor < bytes.len() && bytes[cursor] != b'\n' {
                    cursor += 1;
                }
            }
            b'(' => {
                if stack.len() >= MAX_SCHEMATIC_DEPTH {
                    return Err("s-expression nesting limit exceeded".into());
                }
                tokens += 1;
                stack.push(Vec::new());
                cursor += 1;
            }
            b')' => {
                tokens += 1;
                let list = SExpr::List(
                    stack
                        .pop()
                        .ok_or_else(|| "unexpected closing parenthesis".to_string())?,
                );
                if let Some(parent) = stack.last_mut() {
                    parent.push(list);
                } else {
                    roots.push(list);
                }
                cursor += 1;
            }
            b'"' => {
                cursor += 1;
                let mut value = String::new();
                let mut escaped = false;
                let mut closed = false;
                while cursor < bytes.len() {
                    let byte = bytes[cursor];
                    cursor += 1;
                    if escaped {
                        value.push(match byte {
                            b'n' => '\n',
                            b'r' => '\r',
                            b't' => '\t',
                            other => other as char,
                        });
                        escaped = false;
                    } else if byte == b'\\' {
                        escaped = true;
                    } else if byte == b'"' {
                        closed = true;
                        break;
                    } else if byte.is_ascii() {
                        value.push(byte as char);
                    } else {
                        let start = cursor - 1;
                        let character = source[start..]
                            .chars()
                            .next()
                            .ok_or_else(|| "invalid UTF-8 quoted token".to_string())?;
                        value.push(character);
                        cursor = start + character.len_utf8();
                    }
                    if value.len() > MAX_SCHEMATIC_TEXT {
                        return Err("s-expression string limit exceeded".into());
                    }
                }
                if !closed {
                    return Err("unterminated quoted token".into());
                }
                tokens += 1;
                stack
                    .last_mut()
                    .ok_or_else(|| "quoted token outside a list".to_string())?
                    .push(SExpr::Atom(value));
            }
            _ => {
                let start = cursor;
                while cursor < bytes.len()
                    && !bytes[cursor].is_ascii_whitespace()
                    && !matches!(bytes[cursor], b'(' | b')' | b';')
                {
                    cursor += 1;
                }
                let token = std::str::from_utf8(&bytes[start..cursor])
                    .map_err(|_| "source is not UTF-8".to_string())?;
                if token.len() > MAX_SCHEMATIC_TEXT {
                    return Err("s-expression token limit exceeded".into());
                }
                tokens += 1;
                stack
                    .last_mut()
                    .ok_or_else(|| "token outside a list".to_string())?
                    .push(SExpr::Atom(token.into()));
            }
        }
        if tokens > MAX_SCHEMATIC_TOKENS {
            return Err("s-expression token count limit exceeded".into());
        }
    }
    if !stack.is_empty() || roots.len() != 1 {
        return Err("s-expression must contain one complete root".into());
    }
    let root = roots.pop().expect("one root checked");
    if !expected_root.contains(&root.head().unwrap_or_default()) {
        return Err(format!(
            "unsupported root {}; expected {}",
            root.head().unwrap_or("unknown"),
            expected_root.join(" or ")
        ));
    }
    Ok(root)
}

#[derive(Clone)]
struct ParsedSchematic {
    root_uuid: String,
    sheets: Vec<ParsedSheet>,
    symbols: Vec<ParsedSymbol>,
}

#[derive(Clone)]
struct ParsedSheet {
    uuid: String,
    file: String,
    instance_paths: Vec<String>,
}

#[derive(Clone)]
struct ParsedSymbol {
    uuid: String,
    reference: Option<String>,
    unit: Option<String>,
    instances: BTreeMap<String, (Option<String>, Option<String>)>,
    facts: BTreeMap<String, String>,
}

fn direct_property(form: &SExpr, name: &str) -> Option<String> {
    form.children("property").find_map(|property| {
        let items = property.items()?;
        (items.get(1)?.atom()? == name)
            .then(|| items.get(2)?.atom().map(str::to_owned))
            .flatten()
    })
}

fn yes_no(value: &str) -> Option<String> {
    match value {
        "yes" | "true" | "1" => Some("true".into()),
        "no" | "false" | "0" => Some("false".into()),
        _ => None,
    }
}

fn parse_schematic_source(source: &str) -> Result<ParsedSchematic, String> {
    let root = parse_sexpr(source, &["kicad_sch"])?;
    let root_uuid = root
        .child_value("uuid")
        .ok_or_else(|| "schematic root UUID is missing".to_string())?
        .to_owned();
    let mut sheets = Vec::new();
    for sheet in root.children("sheet") {
        if sheets.len() >= MAX_SCHEMATIC_CHILDREN {
            return Err("schematic child count limit exceeded".into());
        }
        let uuid = sheet
            .child_value("uuid")
            .ok_or_else(|| "sheet UUID is missing".to_string())?
            .to_owned();
        let file = direct_property(sheet, "Sheetfile")
            .ok_or_else(|| "sheet file property is missing".to_string())?;
        let mut paths = Vec::new();
        sheet.descendants("path", &mut paths);
        let instance_paths = paths
            .into_iter()
            .filter_map(|path| path.items()?.get(1)?.atom().map(str::to_owned))
            .filter(|path| path.starts_with('/'))
            .collect();
        sheets.push(ParsedSheet {
            uuid,
            file,
            instance_paths,
        });
    }
    let mut symbol_instances = BTreeMap::new();
    for instances in root.children("symbol_instances") {
        for path in instances.children("path") {
            if let Some(uuid_path) = path
                .items()
                .and_then(|items| items.get(1))
                .and_then(SExpr::atom)
            {
                symbol_instances.insert(
                    uuid_path.to_owned(),
                    (
                        path.child_value("reference").map(str::to_owned),
                        path.child_value("unit").map(str::to_owned),
                    ),
                );
            }
        }
    }
    let mut symbols = Vec::new();
    for symbol in root.children("symbol") {
        let Some(uuid) = symbol.child_value("uuid") else {
            continue;
        };
        let reference = direct_property(symbol, "Reference");
        let mut facts = BTreeMap::new();
        let library_id = symbol.child_value("lib_id").map(str::to_owned);
        for (name, value) in [
            ("value", direct_property(symbol, "Value")),
            ("footprint", direct_property(symbol, "Footprint")),
            ("in_bom", symbol.child_value("in_bom").and_then(yes_no)),
            ("on_board", symbol.child_value("on_board").and_then(yes_no)),
            ("dnp", symbol.child_value("dnp").and_then(yes_no)),
            ("library_id", library_id.clone()),
        ] {
            if let Some(value) = value.filter(|value| !value.is_empty()) {
                facts.insert(name.into(), value);
            }
        }
        if let Some(reference) = &reference {
            facts.insert("reference".into(), reference.clone());
        }
        if let Some(library_id) = library_id {
            if library_id.starts_with("power:") {
                facts.insert("power_symbol".into(), "true".into());
            }
        }
        if facts.get("value").is_some_and(|value| value == "PWR_FLAG") {
            facts.insert("power_flag".into(), "true".into());
        }
        let unit = symbol.child_value("unit").map(str::to_owned);
        if let Some(unit) = &unit {
            facts.insert("unit".into(), unit.clone());
        }
        for pin in symbol.children("pin") {
            let number = pin
                .items()
                .and_then(|items| items.get(1))
                .and_then(SExpr::atom);
            let net = pin.child_value("net");
            if let (Some(number), Some(net)) = (number, net) {
                facts.insert(format!("pin:{number}"), net.into());
            }
            if let (Some(number), Some(electrical_type)) =
                (number, pin.child_value("electrical_type"))
            {
                facts.insert(
                    format!("pin_electrical_type:{number}"),
                    electrical_type.into(),
                );
            }
        }
        let instances = symbol_instances
            .iter()
            .filter(|(path, _)| path.rsplit('/').next() == Some(uuid))
            .map(|(path, instance)| (path.clone(), instance.clone()))
            .collect();
        symbols.push(ParsedSymbol {
            uuid: uuid.into(),
            reference,
            unit,
            instances,
            facts,
        });
    }
    Ok(ParsedSchematic {
        root_uuid,
        sheets,
        symbols,
    })
}

fn normalized_relative(
    base: &str,
    child: &str,
    variables: &BTreeMap<String, String>,
) -> Result<String, String> {
    let mut resolved = child.to_owned();
    while let Some(start) = resolved.find("${") {
        let end = resolved[start + 2..]
            .find('}')
            .map(|end| start + 2 + end)
            .ok_or_else(|| "unresolved-variable".to_string())?;
        let key = &resolved[start + 2..end];
        let value = variables
            .get(key)
            .ok_or_else(|| "unresolved-variable".to_string())?;
        resolved.replace_range(start..=end, value);
        if resolved.len() > MAX_SCHEMATIC_TEXT {
            return Err("child-path-limit".into());
        }
    }
    let path = Path::new(&resolved);
    if path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err("external-child".into());
    }
    let parent = Path::new(base).parent().unwrap_or_else(|| Path::new(""));
    let joined = parent.join(path);
    let mut parts = Vec::new();
    for part in joined.components() {
        match part {
            Component::Normal(value) => parts.push(value.to_string_lossy().into_owned()),
            _ => return Err("external-child".into()),
        }
    }
    Ok(parts.join("/"))
}

fn select_ambiguous_root(candidates: &[String], selector: &str) -> Result<String, Error> {
    if candidates.len() < 2 {
        return Err(Error::Invalid(
            "--schematic only resolves ambiguous automatic schematic roots.".into(),
        ));
    }
    let normalized = selector.replace('\\', "/");
    let path = Path::new(&normalized);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(Error::Invalid(
            "--schematic must be a normalized project-relative path.".into(),
        ));
    }
    let exact = candidates
        .iter()
        .filter(|candidate| candidate.replace('\\', "/") == normalized)
        .collect::<Vec<_>>();
    if exact.len() == 1 {
        return Ok(exact[0].to_string());
    }
    let by_name = candidates
        .iter()
        .filter(|candidate| Path::new(candidate).file_name() == path.file_name())
        .collect::<Vec<_>>();
    if by_name.len() == 1 {
        Ok(by_name[0].to_string())
    } else {
        Err(Error::Invalid(format!(
            "--schematic did not uniquely match an automatic root. Candidates: {}",
            candidates.join(", ")
        )))
    }
}

fn parent_and_stem(path: &str) -> Option<(String, String)> {
    let path = Path::new(path);
    Some((
        path.parent()
            .unwrap_or_else(|| Path::new(""))
            .to_string_lossy()
            .replace('\\', "/"),
        path.file_stem()?.to_str()?.to_owned(),
    ))
}

fn coherent_path(left: &str, right: &str) -> bool {
    parent_and_stem(left) == parent_and_stem(right)
}

fn candidate_roots(
    schematics: &BTreeMap<String, String>,
    parsed: &BTreeMap<String, ParsedSchematic>,
    root_hint: Option<&str>,
    board_name: Option<&str>,
    projects: &BTreeSet<String>,
    variables: &BTreeMap<String, String>,
) -> Vec<String> {
    if let Some(root) = root_hint.filter(|root| parsed.contains_key(*root)) {
        return vec![root.into()];
    }
    if let Some(board) = board_name {
        let coherent = schematics
            .keys()
            .filter(|schematic| {
                coherent_path(board, schematic)
                    && projects.iter().any(|project| {
                        coherent_path(board, project) && coherent_path(schematic, project)
                    })
            })
            .cloned()
            .collect::<Vec<_>>();
        if !coherent.is_empty() {
            return coherent;
        }
        let colocated = schematics
            .keys()
            .filter(|schematic| coherent_path(board, schematic))
            .cloned()
            .collect::<Vec<_>>();
        return colocated;
    }
    let mut children = BTreeSet::new();
    for (name, schematic) in parsed {
        for sheet in &schematic.sheets {
            if let Ok(child) = normalized_relative(name, &sheet.file, variables) {
                children.insert(child);
            }
        }
    }
    let roots = parsed
        .keys()
        .filter(|name| !children.contains(*name))
        .cloned()
        .collect::<Vec<_>>();
    if roots.is_empty() {
        parsed
            .keys()
            .filter(|name| {
                Path::new(name).file_name().and_then(|value| value.to_str())
                    == Some("root.kicad_sch")
            })
            .cloned()
            .collect()
    } else {
        roots
    }
}

struct HierarchyBuild<'a> {
    parsed: &'a BTreeMap<String, ParsedSchematic>,
    project_identity: &'a str,
    root_digest: &'a str,
    variables: &'a BTreeMap<String, String>,
    occurrences: Vec<SchematicOccurrence>,
    states: BTreeSet<String>,
    seen_instance_paths: BTreeSet<String>,
    seen_occurrence_keys: BTreeSet<String>,
}

impl HierarchyBuild<'_> {
    fn visit(&mut self, source_path: &str, sheet_uuid_path: &str, stack: &mut Vec<String>) {
        if self.occurrences.len() >= MAX_SCHEMATIC_OCCURRENCES {
            self.states.insert("occurrence-limit".into());
            return;
        }
        if stack.iter().any(|item| item == source_path) {
            self.states.insert("cycle".into());
            return;
        }
        let Some(source) = self.parsed.get(source_path) else {
            self.states.insert("missing-child".into());
            return;
        };
        stack.push(source_path.into());
        for symbol in &source.symbols {
            if self.occurrences.len() >= MAX_SCHEMATIC_OCCURRENCES {
                self.states.insert("occurrence-limit".into());
                break;
            }
            let location = format!(
                "project={};root={};sheet={};item={};source={}",
                self.project_identity, self.root_digest, sheet_uuid_path, symbol.uuid, source_path
            );
            let key = crate::sha256(&location);
            if !self.seen_occurrence_keys.insert(key.clone()) {
                self.states.insert("duplicate-item-identity".into());
                continue;
            }
            let instance_path = format!("{sheet_uuid_path}/{}", symbol.uuid);
            let (reference, unit) = symbol
                .instances
                .get(&instance_path)
                .cloned()
                .unwrap_or_else(|| (symbol.reference.clone(), symbol.unit.clone()));
            let mut explicit_facts = symbol.facts.clone();
            if let Some(reference) = &reference {
                explicit_facts.insert("reference".into(), reference.clone());
            }
            if let Some(unit) = &unit {
                explicit_facts.insert("unit".into(), unit.clone());
            }
            let facts = explicit_facts
                .into_iter()
                .map(|(name, value)| SchematicFact {
                    name,
                    value,
                    producer: "kicad-source".into(),
                    evidence_class: "explicit-source-fact".into(),
                    source_path: source_path.into(),
                    confidence: "high".into(),
                })
                .collect();
            self.occurrences.push(SchematicOccurrence {
                key,
                project_identity: self.project_identity.into(),
                root_digest: self.root_digest.into(),
                sheet_uuid_path: sheet_uuid_path.into(),
                item_uuid: symbol.uuid.clone(),
                source_path: source_path.into(),
                reference,
                unit,
                facts,
            });
        }
        for sheet in &source.sheets {
            let expected_path = format!("{sheet_uuid_path}/{}", sheet.uuid);
            if sheet.instance_paths.is_empty()
                || !sheet
                    .instance_paths
                    .iter()
                    .any(|path| path == &expected_path)
            {
                self.states.insert("broken-instance-path".into());
            }
            for path in &sheet.instance_paths {
                if !self.seen_instance_paths.insert(path.clone()) {
                    self.states.insert("duplicate-instance-path".into());
                }
            }
            match normalized_relative(source_path, &sheet.file, self.variables) {
                Ok(child) => self.visit(&child, &expected_path, stack),
                Err(state) => {
                    self.states.insert(state);
                }
            }
        }
        stack.pop();
    }
}

#[derive(Clone, Default)]
struct ArtifactComponent {
    path: Option<String>,
    item_uuid: Option<String>,
    reference: Option<String>,
    value: Option<String>,
    footprint: Option<String>,
    dnp: Option<bool>,
    pads: BTreeMap<String, String>,
}

fn first_atom(form: &SExpr, head: &str) -> Option<String> {
    form.child_value(head).map(str::to_owned)
}

fn bool_field(value: Option<&str>) -> Option<bool> {
    match value?.to_ascii_lowercase().as_str() {
        "yes" | "true" | "1" => Some(true),
        "no" | "false" | "0" => Some(false),
        _ => None,
    }
}

fn board_components(source: &str) -> Result<Vec<ArtifactComponent>, String> {
    let root = parse_sexpr(source, &["kicad_pcb"])?;
    let mut output = Vec::new();
    for footprint in root.children("footprint").chain(root.children("module")) {
        if output.len() >= MAX_GENERIC_RECORDS {
            return Err("board footprint count limit exceeded".into());
        }
        let reference = direct_property(footprint, "Reference").or_else(|| {
            footprint.children("fp_text").find_map(|item| {
                let values = item.items()?;
                (values.get(1)?.atom()? == "reference")
                    .then(|| values.get(2)?.atom().map(str::to_owned))
                    .flatten()
            })
        });
        let mut pads = BTreeMap::new();
        for pad in footprint.children("pad") {
            let number = pad
                .items()
                .and_then(|items| items.get(1))
                .and_then(SExpr::atom);
            if let Some(number) = number {
                let net = pad
                    .children("net")
                    .next()
                    .and_then(SExpr::items)
                    .and_then(|items| items.get(2))
                    .and_then(SExpr::atom)
                    .unwrap_or("");
                pads.insert(number.into(), net.into());
            }
        }
        let mut attributes = Vec::new();
        footprint.descendants("attr", &mut attributes);
        let dnp = footprint
            .items()
            .is_some_and(|items| items.iter().any(|item| item.atom() == Some("dnp")))
            || attributes.iter().any(|attr| {
                attr.items()
                    .is_some_and(|items| items.iter().any(|item| item.atom() == Some("dnp")))
            });
        let linkage = first_atom(footprint, "path");
        let (path, item_uuid) = linkage
            .as_deref()
            .and_then(|value| value.rsplit_once('/'))
            .filter(|(path, item)| !path.is_empty() && !item.is_empty())
            .map(|(path, item)| (Some(path.into()), Some(item.into())))
            .unwrap_or_else(|| {
                (
                    linkage,
                    first_atom(footprint, "uuid").or_else(|| first_atom(footprint, "tstamp")),
                )
            });
        output.push(ArtifactComponent {
            path,
            item_uuid,
            reference,
            value: direct_property(footprint, "Value"),
            footprint: footprint
                .items()
                .and_then(|items| items.get(1))
                .and_then(SExpr::atom)
                .map(str::to_owned),
            dnp: Some(dnp),
            pads,
        });
    }
    Ok(output)
}

fn netlist_components(source: &str) -> Result<(Vec<ArtifactComponent>, usize), String> {
    let root = parse_sexpr(source, &["export"])?;
    let components = root
        .children("components")
        .next()
        .map(|components| components.children("comp"))
        .into_iter()
        .flatten();
    let mut output = Vec::new();
    for component in components {
        if output.len() >= MAX_GENERIC_RECORDS {
            return Err("netlist component count limit exceeded".into());
        }
        let occurrence = first_atom(component, "tstamps").or_else(|| first_atom(component, "path"));
        let (path, item_uuid) = occurrence
            .as_deref()
            .and_then(|value| value.rsplit_once('/'))
            .map(|(path, item)| (Some(path.into()), Some(item.into())))
            .unwrap_or((None, None));
        output.push(ArtifactComponent {
            path,
            item_uuid,
            reference: first_atom(component, "ref"),
            value: first_atom(component, "value"),
            footprint: first_atom(component, "footprint"),
            dnp: component.children("property").find_map(|property| {
                let name = property
                    .children("name")
                    .next()
                    .and_then(SExpr::items)
                    .and_then(|items| items.get(1))
                    .and_then(SExpr::atom)?;
                (name.eq_ignore_ascii_case("dnp"))
                    .then(|| bool_field(property.child_value("value")))
                    .flatten()
            }),
            pads: BTreeMap::new(),
        });
    }
    let net_forms = root.children("nets").next();
    let nets = net_forms
        .as_ref()
        .map(|nets| nets.children("net").count())
        .unwrap_or(0);
    if let Some(net_forms) = net_forms {
        for net in net_forms.children("net") {
            let name = first_atom(net, "name").unwrap_or_default();
            for node in net.children("node") {
                let reference = first_atom(node, "ref");
                let pin = first_atom(node, "pin");
                if let (Some(reference), Some(pin)) = (reference, pin) {
                    if let Some(component) = output
                        .iter_mut()
                        .find(|component| component.reference.as_deref() == Some(&reference))
                    {
                        component.pads.insert(pin, name.clone());
                    }
                }
            }
        }
    }
    Ok((output, nets))
}

fn xml_counts(source: &str) -> Result<(usize, usize), String> {
    if source.len() > MAX_SCHEMATIC_BYTES {
        return Err("XML netlist byte limit exceeded".into());
    }
    let trimmed = source.trim_start_matches('\u{feff}').trim_start();
    if !(trimmed.starts_with("<?xml") || trimmed.starts_with("<export"))
        || !source.contains("<components")
        || !source.contains("<nets")
    {
        return Err("unrecognized XML netlist root".into());
    }
    let components = source.matches("<comp ").count() + source.matches("<comp>").count();
    let nets = source.matches("<net ").count() + source.matches("<net>").count();
    if components + nets > MAX_GENERIC_RECORDS {
        return Err("XML netlist record limit exceeded".into());
    }
    Ok((components, nets))
}

fn delimited_rows(source: &str) -> Result<Vec<BTreeMap<String, String>>, String> {
    let mut lines = source.lines().filter(|line| !line.trim().is_empty());
    let header = lines
        .next()
        .ok_or_else(|| "delimited export is empty".to_string())?;
    let delimiter = if header.matches('\t').count() > header.matches(',').count() {
        '\t'
    } else {
        ','
    };
    let headers = crate::split_delimited(header, delimiter)
        .into_iter()
        .map(|header| header.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    if headers.is_empty() || headers.iter().any(String::is_empty) {
        return Err("delimited export has an invalid header".into());
    }
    let mut rows = Vec::new();
    for line in lines {
        if rows.len() >= MAX_GENERIC_RECORDS {
            return Err(format!(
                "delimited export exceeds the {MAX_GENERIC_RECORDS}-record limit"
            ));
        }
        let values = crate::split_delimited(line, delimiter);
        if values.len() != headers.len() {
            return Err("delimited export row does not match its header".into());
        }
        rows.push(headers.iter().cloned().zip(values).collect());
    }
    Ok(rows)
}

fn unique_reference_map<'a, T>(
    values: &'a [T],
    reference: impl Fn(&'a T) -> Option<&'a str>,
) -> BTreeMap<String, Option<usize>> {
    let mut output = BTreeMap::new();
    for (index, value) in values.iter().enumerate() {
        if let Some(reference) = reference(value).map(|value| value.trim().to_ascii_uppercase()) {
            output
                .entry(reference)
                .and_modify(|entry| *entry = None)
                .or_insert(Some(index));
        }
    }
    output
}

fn occurrence_fact<'a>(occurrence: &'a SchematicOccurrence, name: &str) -> Option<&'a str> {
    occurrence
        .facts
        .iter()
        .find(|fact| fact.name == name)
        .map(|fact| fact.value.as_str())
}

fn apply_native_facts(
    occurrences: &mut [SchematicOccurrence],
    components: &[ArtifactComponent],
    version: &str,
    source_path: &str,
) {
    let component_refs = unique_reference_map(components, |item| item.reference.as_deref());
    let occurrence_refs = unique_reference_map(occurrences, |item| item.reference.as_deref());
    for occurrence in occurrences {
        let exact = components.iter().find(|item| {
            item.path.as_deref() == Some(occurrence.sheet_uuid_path.as_str())
                && item.item_uuid.as_deref() == Some(occurrence.item_uuid.as_str())
        });
        let by_reference = occurrence.reference.as_ref().and_then(|reference| {
            let reference = reference.trim().to_ascii_uppercase();
            occurrence_refs
                .get(&reference)
                .copied()
                .flatten()
                .and_then(|_| component_refs.get(&reference).copied().flatten())
                .map(|index| &components[index])
        });
        let Some(component) = exact.or(by_reference) else {
            continue;
        };
        let mut values = BTreeMap::new();
        for (name, value) in [
            ("reference", component.reference.as_ref()),
            ("value", component.value.as_ref()),
            ("footprint", component.footprint.as_ref()),
        ] {
            if let Some(value) = value {
                values.insert(name.to_owned(), value.clone());
            }
        }
        if let Some(dnp) = component.dnp {
            values.insert("dnp".into(), dnp.to_string());
        }
        for (pin, net) in &component.pads {
            values.insert(format!("pin:{pin}"), net.clone());
        }
        for (name, value) in values {
            occurrence.facts.retain(|fact| fact.name != name);
            if occurrence.facts.len() < 64 {
                occurrence.facts.push(SchematicFact {
                    name,
                    value,
                    producer: format!("kicad-cli {version}"),
                    evidence_class: "explicit-export-facts".into(),
                    source_path: source_path.into(),
                    confidence: "high".into(),
                });
            }
        }
    }
}

pub(crate) fn canonical_native_export_facts_digest(
    occurrences: &[SchematicOccurrence],
) -> Option<String> {
    let mut tuples = occurrences
        .iter()
        .flat_map(|occurrence| {
            occurrence
                .facts
                .iter()
                .filter(|fact| fact.evidence_class == "explicit-export-facts")
                .map(|fact| {
                    (
                        occurrence.key.as_str(),
                        fact.name.as_str(),
                        fact.value.as_str(),
                        fact.producer.as_str(),
                        fact.evidence_class.as_str(),
                        fact.source_path.as_str(),
                        fact.confidence.as_str(),
                    )
                })
        })
        .collect::<Vec<_>>();
    if tuples.is_empty() {
        return None;
    }
    tuples.sort_unstable();
    Some(crate::sha256(
        serde_json::to_vec(&tuples).expect("serializing schematic fact strings cannot fail"),
    ))
}

pub(crate) fn schematic_composite_digest(artifact_digests: &BTreeMap<String, String>) -> String {
    let inputs = artifact_digests
        .iter()
        // Raw native exports contain volatile KiCad timestamps and operational paths.
        .filter(|(name, _)| name.as_str() != "schematic:composite" && !name.starts_with("native:"))
        .collect::<BTreeMap<_, _>>();
    crate::sha256(
        serde_json::to_vec(&inputs).expect("serializing schematic digest strings cannot fail"),
    )
}

fn delimited_components(source: &str) -> Result<Vec<ArtifactComponent>, String> {
    let mut output = Vec::new();
    for row in delimited_rows(source)? {
        let references = row
            .get("reference")
            .or_else(|| row.get("references"))
            .or_else(|| row.get("ref"))
            .or_else(|| row.get("designator"));
        for reference in references
            .into_iter()
            .flat_map(|value| value.split([',', ';', ' ']))
            .filter(|value| !value.is_empty())
        {
            output.push(ArtifactComponent {
                reference: Some(reference.into()),
                value: row.get("value").or_else(|| row.get("val")).cloned(),
                footprint: row.get("footprint").cloned(),
                dnp: row
                    .get("dnp")
                    .and_then(|value| bool_field(Some(value)))
                    .or_else(|| {
                        row.get("population")
                            .map(|value| value.eq_ignore_ascii_case("dnp"))
                    }),
                ..ArtifactComponent::default()
            });
        }
    }
    Ok(output)
}

fn push_mismatch(
    output: &mut Vec<SchematicMismatch>,
    occurrence: &SchematicOccurrence,
    field: &str,
    expected: &str,
    actual: &str,
    join: &str,
) {
    output.push(SchematicMismatch {
        check_id: format!("schematic-reconcile-{}", field.replace('_', "-")),
        field: field.into(),
        expected: expected.into(),
        actual: actual.into(),
        join: join.into(),
        confidence: if join == "reference-fallback" {
            "low"
        } else {
            "high"
        }
        .into(),
        gate_impact: GateImpact::EvidenceOnly,
        location: format!(
            "sheet={};item={};source={}",
            occurrence.sheet_uuid_path, occurrence.item_uuid, occurrence.source_path
        ),
    });
}

fn reconcile(
    occurrences: &[SchematicOccurrence],
    board: &[ArtifactComponent],
    netlist: &[ArtifactComponent],
    bom: Option<&str>,
    placement: Option<&str>,
) -> Result<Vec<SchematicMismatch>, String> {
    let board_refs = unique_reference_map(board, |item| item.reference.as_deref());
    let mut board_occurrences = BTreeMap::new();
    for (index, item) in board.iter().enumerate() {
        if let (Some(path), Some(uuid)) = (&item.path, &item.item_uuid) {
            board_occurrences
                .entry(format!("{path}/{uuid}").to_ascii_uppercase())
                .and_modify(|entry| *entry = None)
                .or_insert(Some(index));
        }
    }
    let netlist_refs = unique_reference_map(netlist, |item| item.reference.as_deref());
    let occurrence_refs = unique_reference_map(occurrences, |item| item.reference.as_deref());
    let bom_rows = bom.map(delimited_rows).transpose()?.unwrap_or_default();
    let placement_rows = placement
        .map(delimited_rows)
        .transpose()?
        .unwrap_or_default();
    let bom_refs = bom_rows
        .iter()
        .flat_map(|row| {
            row.get("reference")
                .or_else(|| row.get("references"))
                .or_else(|| row.get("designator"))
                .into_iter()
                .flat_map(|value| value.split([',', ';', ' ']))
        })
        .filter(|value| !value.is_empty())
        .map(|value| value.trim().to_ascii_uppercase())
        .collect::<BTreeSet<_>>();
    let placement_refs = placement_rows
        .iter()
        .filter_map(|row| {
            row.get("ref")
                .or_else(|| row.get("reference"))
                .or_else(|| row.get("designator"))
        })
        .map(|value| value.trim().to_ascii_uppercase())
        .collect::<BTreeSet<_>>();
    let mut output = Vec::new();
    for occurrence in occurrences {
        let occurrence_path = format!("{}/{}", occurrence.sheet_uuid_path, occurrence.item_uuid);
        let exact_board = board_occurrences
            .get(&occurrence_path.to_ascii_uppercase())
            .copied()
            .flatten()
            .map(|index| &board[index]);
        let reference = occurrence
            .reference
            .as_ref()
            .map(|value| value.to_ascii_uppercase());
        let weak_board = reference.as_ref().and_then(|reference| {
            (occurrence_refs.get(reference).copied().flatten().is_some())
                .then(|| board_refs.get(reference).copied().flatten())
                .flatten()
                .map(|index| &board[index])
        });
        let (joined_board, join) = if let Some(item) = exact_board {
            (Some(item), "occurrence-uuid")
        } else {
            (weak_board, "reference-fallback")
        };
        if let Some(board) = joined_board {
            if let (Some(expected), Some(actual)) =
                (occurrence.reference.as_deref(), board.reference.as_deref())
            {
                if !expected.eq_ignore_ascii_case(actual) {
                    push_mismatch(&mut output, occurrence, "reference", expected, actual, join);
                }
            }
            if join == "reference-fallback"
                && board
                    .item_uuid
                    .as_deref()
                    .is_some_and(|actual| actual != occurrence.item_uuid)
            {
                push_mismatch(
                    &mut output,
                    occurrence,
                    "uuid",
                    &occurrence.item_uuid,
                    board.item_uuid.as_deref().unwrap_or("missing"),
                    join,
                );
            }
            for field in ["value", "footprint"] {
                let expected = occurrence_fact(occurrence, field);
                let actual = match field {
                    "value" => board.value.as_deref(),
                    "footprint" => board.footprint.as_deref(),
                    _ => None,
                };
                if let (Some(expected), Some(actual)) = (expected, actual) {
                    let footprint_matches = field != "footprint"
                        || expected == actual
                        || expected.rsplit(':').next() == actual.rsplit(':').next();
                    if !footprint_matches || (field != "footprint" && expected != actual) {
                        push_mismatch(&mut output, occurrence, field, expected, actual, join);
                    }
                }
            }
            for fact in occurrence
                .facts
                .iter()
                .filter(|fact| fact.name.starts_with("pin:"))
            {
                let pin = fact.name.trim_start_matches("pin:");
                match board.pads.get(pin) {
                    None => push_mismatch(&mut output, occurrence, "pin-pad", pin, "missing", join),
                    Some(actual) if actual != &fact.value => {
                        push_mismatch(&mut output, occurrence, "net", &fact.value, actual, join)
                    }
                    _ => {}
                }
            }
            if let (Some(expected), Some(actual)) = (
                occurrence_fact(occurrence, "dnp").and_then(|value| value.parse::<bool>().ok()),
                board.dnp,
            ) {
                if expected != actual {
                    push_mismatch(
                        &mut output,
                        occurrence,
                        "dnp",
                        &expected.to_string(),
                        &actual.to_string(),
                        join,
                    );
                }
            }
        } else if occurrence_fact(occurrence, "on_board") == Some("true") {
            push_mismatch(
                &mut output,
                occurrence,
                "board-population",
                "present",
                "missing",
                "unmatched",
            );
        }
        let weak_netlist = reference.as_ref().and_then(|reference| {
            (occurrence_refs.get(reference).copied().flatten().is_some())
                .then(|| netlist_refs.get(reference).copied().flatten())
                .flatten()
                .map(|index| &netlist[index])
        });
        let joined_netlist = netlist
            .iter()
            .find(|item| {
                item.path.as_deref() == Some(occurrence.sheet_uuid_path.as_str())
                    && item.item_uuid.as_deref() == Some(occurrence.item_uuid.as_str())
            })
            .or(weak_netlist);
        if let Some(netlist) = joined_netlist {
            let netlist_join = if netlist.path.is_some() {
                "occurrence-uuid"
            } else {
                "reference-fallback"
            };
            for field in ["value", "footprint"] {
                if let (Some(expected), Some(actual)) = (
                    occurrence_fact(occurrence, field),
                    match field {
                        "value" => netlist.value.as_deref(),
                        "footprint" => netlist.footprint.as_deref(),
                        _ => None,
                    },
                ) {
                    if expected != actual {
                        push_mismatch(
                            &mut output,
                            occurrence,
                            &format!("netlist-{field}"),
                            expected,
                            actual,
                            netlist_join,
                        );
                    }
                }
            }
            for fact in occurrence
                .facts
                .iter()
                .filter(|fact| fact.name.starts_with("pin:"))
            {
                let pin = fact.name.trim_start_matches("pin:");
                match netlist.pads.get(pin) {
                    None => push_mismatch(
                        &mut output,
                        occurrence,
                        "netlist-pin",
                        pin,
                        "missing",
                        netlist_join,
                    ),
                    Some(actual) if actual != &fact.value => push_mismatch(
                        &mut output,
                        occurrence,
                        "netlist-net",
                        &fact.value,
                        actual,
                        netlist_join,
                    ),
                    _ => {}
                }
            }
        }
        if let Some(reference) = reference {
            let bom_row = bom_rows.iter().find(|row| {
                row.get("reference")
                    .or_else(|| row.get("references"))
                    .or_else(|| row.get("designator"))
                    .is_some_and(|value| {
                        value
                            .split([',', ';', ' '])
                            .any(|item| item.eq_ignore_ascii_case(&reference))
                    })
            });
            if occurrence_fact(occurrence, "in_bom") == Some("true")
                && bom.is_some()
                && !bom_refs.contains(&reference)
            {
                push_mismatch(
                    &mut output,
                    occurrence,
                    "bom-population",
                    "present",
                    "missing",
                    "reference-fallback",
                );
            }
            if let Some(row) = bom_row {
                for field in ["value", "footprint"] {
                    if let (Some(expected), Some(actual)) =
                        (occurrence_fact(occurrence, field), row.get(field))
                    {
                        if expected != actual {
                            push_mismatch(
                                &mut output,
                                occurrence,
                                &format!("bom-{field}"),
                                expected,
                                actual,
                                "reference-fallback",
                            );
                        }
                    }
                }
                let references = row
                    .get("reference")
                    .or_else(|| row.get("references"))
                    .or_else(|| row.get("designator"));
                let explicit_quantity = row
                    .get("quantity")
                    .or_else(|| row.get("qty"))
                    .and_then(|value| value.parse::<usize>().ok());
                let grouped_quantity = references.map(|value| {
                    value
                        .split([',', ';', ' '])
                        .filter(|item| !item.is_empty())
                        .count()
                });
                if let (Some(expected), Some(actual)) = (grouped_quantity, explicit_quantity) {
                    if expected != actual {
                        push_mismatch(
                            &mut output,
                            occurrence,
                            "bom-quantity",
                            &expected.to_string(),
                            &actual.to_string(),
                            "reference-fallback",
                        );
                    }
                }
                if let Some(population) = row.get("population") {
                    let expected = if occurrence_fact(occurrence, "dnp") == Some("true") {
                        "dnp"
                    } else {
                        "fitted"
                    };
                    if !population.eq_ignore_ascii_case(expected) {
                        push_mismatch(
                            &mut output,
                            occurrence,
                            "bom-fitted",
                            expected,
                            population,
                            "reference-fallback",
                        );
                    }
                }
            }
            let fitted = occurrence_fact(occurrence, "dnp") == Some("false")
                && occurrence_fact(occurrence, "on_board") == Some("true");
            if fitted && placement.is_some() && !placement_refs.contains(&reference) {
                push_mismatch(
                    &mut output,
                    occurrence,
                    "placement-population",
                    "present",
                    "missing",
                    "reference-fallback",
                );
            }
            if let Some(row) = placement_rows.iter().find(|row| {
                row.get("ref")
                    .or_else(|| row.get("reference"))
                    .or_else(|| row.get("designator"))
                    .is_some_and(|value| value.eq_ignore_ascii_case(&reference))
            }) {
                if let (Some(expected), Some(actual)) = (
                    occurrence_fact(occurrence, "value"),
                    row.get("val").or_else(|| row.get("value")),
                ) {
                    if expected != actual {
                        push_mismatch(
                            &mut output,
                            occurrence,
                            "placement-value",
                            expected,
                            actual,
                            "reference-fallback",
                        );
                    }
                }
            }
        }
    }
    let bom_revision = bom_rows.iter().find_map(|row| row.get("revision"));
    let placement_revision = placement_rows.iter().find_map(|row| row.get("revision"));
    if let (Some(expected), Some(actual), Some(occurrence)) =
        (bom_revision, placement_revision, occurrences.first())
    {
        if expected != actual {
            push_mismatch(
                &mut output,
                occurrence,
                "revision",
                expected,
                actual,
                "artifact-revision",
            );
        }
    }
    Ok(output)
}

fn native_coverage(id: &str, label: &str, report: &NativeReport) -> Coverage {
    let status = if report.status != "completed" {
        CoverageStatus::NotRun
    } else if report.unknown_exclusion_count > 0 {
        CoverageStatus::Unknown
    } else if report.finding_count > 0 {
        CoverageStatus::Attention
    } else {
        CoverageStatus::Passed
    };
    Coverage {
        id: id.into(),
        label: label.into(),
        status,
        evidence: format!(
            "{}; {} active, {} excluded, {} unknown-exclusion marker(s).",
            report.status,
            report.finding_count,
            report.excluded_count,
            report.unknown_exclusion_count
        ),
    }
}

fn capability(
    id: &str,
    status: &str,
    producer: &str,
    evidence_class: &str,
    detail: impl Into<String>,
) -> SchematicCapability {
    SchematicCapability {
        id: id.into(),
        status: status.into(),
        producer: producer.into(),
        evidence_class: evidence_class.into(),
        detail: detail.into(),
    }
}

pub(crate) fn review_project(
    input: ProjectEvidenceInput<'_>,
) -> Result<(SchematicReview, Vec<Finding>, Vec<Coverage>), Error> {
    let mut review = SchematicReview {
        status: "not_provided".into(),
        board_path: input.board_name.map(str::to_owned),
        board_digest: input.board_source.map(crate::sha256),
        ..SchematicReview::default()
    };
    for (name, source) in input.schematics.iter().chain(input.netlists.iter()) {
        review
            .artifact_digests
            .insert(name.clone(), crate::sha256(source));
    }
    if let Some((name, source)) = input.bom {
        review
            .artifact_digests
            .insert(name.into(), crate::sha256(source));
        if let Some(revision) = delimited_rows(source)
            .map_err(|error| Error::Invalid(format!("Invalid BOM {name}: {error}.")))?
            .iter()
            .find_map(|row| row.get("revision").cloned())
        {
            review.declared_revisions.insert(name.into(), revision);
        }
    }
    if let Some((name, source)) = input.placement {
        review
            .artifact_digests
            .insert(name.into(), crate::sha256(source));
        if let Some(revision) = delimited_rows(source)
            .map_err(|error| Error::Invalid(format!("Invalid placement {name}: {error}.")))?
            .iter()
            .find_map(|row| row.get("revision").cloned())
        {
            review.declared_revisions.insert(name.into(), revision);
        }
    }
    if let (Some(name), Some(source)) = (input.board_name, input.board_source) {
        review
            .artifact_digests
            .insert(name.into(), crate::sha256(source));
    }
    let aggregate_bytes = input.schematics.values().map(String::len).sum::<usize>();
    let mut parsed = BTreeMap::new();
    if aggregate_bytes > MAX_SCHEMATIC_AGGREGATE_BYTES {
        review.status = "not_checked".into();
        review.capabilities.push(capability(
            "kicad-hierarchy",
            "limit_exceeded",
            "kicad-source",
            "inventory",
            "Schematic aggregate exceeds the bounded parser limit.",
        ));
    } else {
        for (name, source) in input.schematics {
            match parse_schematic_source(source) {
                Ok(value) => {
                    parsed.insert(name.clone(), value);
                }
                Err(error) => review.capabilities.push(capability(
                    "kicad-hierarchy",
                    "not_checked",
                    "kicad-source",
                    "inventory",
                    format!("{name}: {error}"),
                )),
            }
        }
    }
    let mut roots = candidate_roots(
        input.schematics,
        &parsed,
        input.root_hint,
        input.board_name,
        input.projects,
        input.project_variables,
    );
    if let Some(selector) = input.root_selector {
        roots = vec![select_ambiguous_root(&roots, selector)?];
    }
    if roots.is_empty() && input.board_name.is_some() && !input.schematics.is_empty() {
        review.status = "incoherent_project".into();
        review.capabilities.push(capability(
            "kicad-hierarchy",
            "not_checked",
            "kicad-source",
            "hierarchy-identity",
            "No schematic shares the selected board's normalized parent path and basename.",
        ));
    } else if roots.len() > 1 {
        review.status = "ambiguous_root".into();
        review.capabilities.push(capability(
            "kicad-hierarchy",
            "ambiguous",
            "kicad-source",
            "hierarchy-identity",
            format!("Multiple coherent roots remain: {}", roots.join(", ")),
        ));
    } else if let Some(root_path) = roots.first() {
        let source = &input.schematics[root_path];
        let root_digest = crate::sha256(source);
        let root = &parsed[root_path];
        let project_identity = input
            .projects
            .iter()
            .find(|project| coherent_path(project, root_path))
            .cloned()
            .unwrap_or_else(|| root_path.clone());
        review.status = "completed".into();
        review.project_identity = Some(project_identity.clone());
        review.root_path = Some(root_path.clone());
        review.root_digest = Some(root_digest.clone());
        let root_occurrence = format!("/{}", root.root_uuid);
        let mut build = HierarchyBuild {
            parsed: &parsed,
            project_identity: &project_identity,
            root_digest: &root_digest,
            variables: input.project_variables,
            occurrences: Vec::new(),
            states: BTreeSet::new(),
            seen_instance_paths: BTreeSet::new(),
            seen_occurrence_keys: BTreeSet::new(),
        };
        build.visit(root_path, &root_occurrence, &mut Vec::new());
        review.occurrences = build.occurrences;
        review.occurrence_count = review.occurrences.len();
        if build.states.is_empty() {
            review.capabilities.push(capability(
                "kicad-hierarchy",
                "completed",
                "kicad-source",
                "hierarchy-identity",
                format!(
                    "Root {root_path} produced {} bounded occurrence(s).",
                    review.occurrence_count
                ),
            ));
        } else {
            review.status = "attention".into();
            for state in build.states {
                review.capabilities.push(capability(
                    "kicad-hierarchy",
                    &state,
                    "kicad-source",
                    "hierarchy-identity",
                    format!("Hierarchy state: {state}."),
                ));
            }
        }
        let board = input
            .board_source
            .map(board_components)
            .transpose()
            .unwrap_or_else(|error| {
                review
                    .limitations
                    .push(format!("Board facts not checked: {error}"));
                None
            })
            .unwrap_or_default();
        let directly_accessible = input.input_kind != "fabrication-zip";
        let root_file = input.project_root.map(|root| root.join(root_path));
        let coherent_pair = input.board_name.is_some_and(|board| {
            coherent_path(board, root_path)
                && input.projects.iter().any(|project| {
                    coherent_path(board, project) && coherent_path(root_path, project)
                })
        });
        if coherent_pair {
            review.source_pair = Some(SchematicSourcePair {
                project_identity: project_identity.clone(),
                schematic_path: root_path.clone(),
                schematic_digest: root_digest.clone(),
                board_path: input.board_name.unwrap().into(),
                board_digest: review.board_digest.clone().unwrap(),
            });
        }
        let mut authoritative_bom = input.bom.map(|(_, source)| source.to_owned());
        let mut authoritative_placement = input.placement.map(|(_, source)| source.to_owned());
        let mut native_bom_version = None;
        let mut native_placement_version = None;
        let mut native_netlist = None;
        let mut native_netlist_version = None;
        if directly_accessible {
            if let Some(root_file) = root_file.as_deref().filter(|path| path.is_file()) {
                for (kind, slot) in [
                    (ExportKind::Bom, &mut authoritative_bom),
                    (ExportKind::Netlist, &mut native_netlist),
                ] {
                    let id = match kind {
                        ExportKind::Bom => "native-bom-export",
                        ExportKind::Netlist => "native-netlist-export",
                        ExportKind::Position => unreachable!(),
                    };
                    if let Some((source, version)) = run_export(root_file, kind, input.native_mode)?
                    {
                        review.artifact_digests.insert(
                            match kind {
                                ExportKind::Bom => "native:bom.csv",
                                ExportKind::Netlist => "native:netlist.net",
                                ExportKind::Position => unreachable!(),
                            }
                            .into(),
                            crate::sha256(&source),
                        );
                        *slot = Some(source);
                        match kind {
                            ExportKind::Bom => native_bom_version = Some(version.clone()),
                            ExportKind::Netlist => native_netlist_version = Some(version.clone()),
                            ExportKind::Position => unreachable!(),
                        }
                        review.capabilities.push(capability(
                            id,
                            "completed",
                            &format!("kicad-cli {version}"),
                            "explicit-export-facts",
                            format!(
                                "{} completed and is authoritative for fields it emits.",
                                kind.label()
                            ),
                        ));
                    } else if input.native_mode != NativeMode::Off {
                        review.capabilities.push(capability(
                            id,
                            "not_run",
                            "kicad-cli",
                            "native-export",
                            format!("{} was unavailable; explicit packaged/source facts remain labeled fallbacks.", kind.label()),
                        ));
                    }
                }
            }
            if coherent_pair {
                if let (Some(root), Some(board_name)) = (input.project_root, input.board_name) {
                    if let Some((source, version)) = run_export(
                        &root.join(board_name),
                        ExportKind::Position,
                        input.native_mode,
                    )? {
                        review
                            .artifact_digests
                            .insert("native:positions.csv".into(), crate::sha256(&source));
                        authoritative_placement = Some(source);
                        native_placement_version = Some(version.clone());
                        review.capabilities.push(capability(
                            "native-position-export",
                            "completed",
                            &format!("kicad-cli {version}"),
                            "explicit-export-facts",
                            "Native position export completed and is authoritative for population fields it emits.",
                        ));
                    } else if input.native_mode != NativeMode::Off {
                        review.capabilities.push(capability(
                            "native-position-export",
                            "not_run",
                            "kicad-cli",
                            "native-export",
                            "Native position export was unavailable; explicit packaged/source population facts remain labeled fallbacks.",
                        ));
                    }
                }
            }
        }
        let mut netlist_components_all = Vec::new();
        let netlist_inputs = if let Some(source) = native_netlist.as_ref() {
            vec![("native-kicadsexpr.net", source.as_str())]
        } else {
            input
                .netlists
                .iter()
                .map(|(name, source)| (name.as_str(), source.as_str()))
                .collect()
        };
        for (name, source) in netlist_inputs {
            if source.trim_start().starts_with("(export") {
                match netlist_components(source) {
                    Ok((components, nets)) => {
                        review.capabilities.push(capability(
                            "generic-netlist",
                            "explicit_fields_only",
                            "netlist-export",
                            "explicit-export-facts",
                            format!("{name}: {} component(s), {nets} net(s).", components.len()),
                        ));
                        netlist_components_all.extend(components);
                    }
                    Err(error) => review.capabilities.push(capability(
                        "generic-netlist",
                        "unsupported",
                        "netlist-export",
                        "inventory",
                        format!("{name}: {error}"),
                    )),
                }
            } else if source.trim_start().starts_with('<') {
                match xml_counts(source) {
                    Ok((components, nets)) => review.capabilities.push(capability(
                        "generic-netlist",
                        "explicit_fields_only",
                        "xml-netlist-export",
                        "explicit-export-facts",
                        format!("{name}: {components} component(s), {nets} net(s)."),
                    )),
                    Err(error) => review.capabilities.push(capability(
                        "generic-netlist",
                        "unsupported",
                        "xml-netlist-export",
                        "inventory",
                        format!("{name}: {error}"),
                    )),
                }
            } else {
                review.capabilities.push(capability(
                    "generic-netlist",
                    "unsupported",
                    "unknown-netlist",
                    "inventory",
                    format!("{name}: syntax was not recognized; no semantic capability inferred."),
                ));
            }
        }
        review.mismatches = reconcile(
            &review.occurrences,
            &board,
            &netlist_components_all,
            authoritative_bom.as_deref(),
            authoritative_placement.as_deref(),
        )
        .map_err(|error| Error::Invalid(format!("Schematic reconciliation input: {error}.")))?;
        if let (Some(source), Some(version)) =
            (native_netlist.as_deref(), native_netlist_version.as_deref())
        {
            let (components, _) = netlist_components(source)
                .map_err(|error| Error::Invalid(format!("Native netlist facts: {error}.")))?;
            apply_native_facts(
                &mut review.occurrences,
                &components,
                version,
                "native:netlist.net",
            );
        }
        if let (Some(source), Some(version)) =
            (authoritative_bom.as_deref(), native_bom_version.as_deref())
        {
            apply_native_facts(
                &mut review.occurrences,
                &delimited_components(source)
                    .map_err(|error| Error::Invalid(format!("Native BOM facts: {error}.")))?,
                version,
                "native:bom.csv",
            );
        }
        if let (Some(source), Some(version)) = (
            authoritative_placement.as_deref(),
            native_placement_version.as_deref(),
        ) {
            apply_native_facts(
                &mut review.occurrences,
                &delimited_components(source)
                    .map_err(|error| Error::Invalid(format!("Native position facts: {error}.")))?,
                version,
                "native:positions.csv",
            );
        }
        review.capabilities.push(capability(
            "schematic-reconciliation",
            if review.mismatches.is_empty() { "completed" } else { "attention" },
            "ratemypcb",
            "deterministic-cross-artifact",
            format!(
                "Occurrence-first reconciliation produced {} evidence-only mismatch(es); unique reference fallback is explicitly weaker.",
                review.mismatches.len()
            ),
        ));
        if directly_accessible {
            if let Some(root_file) = root_file.as_deref().filter(|path| path.is_file()) {
                review.native_erc =
                    Some(run_native(root_file, NativeKind::Erc, input.native_mode)?);
            }
        }
        if coherent_pair && directly_accessible {
            if let (Some(root), Some(board_name)) = (input.project_root, input.board_name) {
                review.native_parity = Some(run_native(
                    &root.join(board_name),
                    NativeKind::Drc {
                        schematic_parity: true,
                    },
                    input.native_mode,
                )?);
            }
        }
        if input.input_kind == "fabrication-zip" {
            review.native_erc = Some(NativeReport::not_run(
                None,
                "ZIP schematic sources are inventory-only and are never staged for native execution.",
            ));
            review.native_parity = Some(NativeReport::not_run(
                None,
                "ZIP board/schematic parity is not run because archive source trees are not staged.",
            ));
        } else if !coherent_pair {
            review.native_parity = Some(NativeReport::not_run(
                None,
                "Native parity requires one coherent board, schematic root, and project basename.",
            ));
        }
    }
    if !input.altium_schematics.is_empty() {
        review.capabilities.push(capability(
            "altium-schdoc",
            "inventory_only",
            "file-inventory",
            "inventory",
            format!(
                "{} .SchDoc file(s) inventoried; no Altium-native ERC, hierarchy, DNP, revision, or parity capability is claimed.",
                input.altium_schematics.len()
            ),
        ));
    }
    if input.schematics.is_empty()
        && input.altium_schematics.is_empty()
        && input.netlists.is_empty()
    {
        review.capabilities.push(capability(
            "schematic-context",
            "not_provided",
            "file-inventory",
            "inventory",
            "No schematic or generic netlist candidate was provided.",
        ));
    }
    if input.schematics.is_empty() {
        for (name, source) in input.netlists {
            let state = if source.trim_start().starts_with("(export") {
                netlist_components(source)
                    .map(|(components, nets)| {
                        format!("explicit_fields_only:{}:{nets}", components.len())
                    })
                    .unwrap_or_else(|_| "unsupported".into())
            } else if source.trim_start().starts_with('<') {
                xml_counts(source)
                    .map(|(components, nets)| format!("explicit_fields_only:{components}:{nets}"))
                    .unwrap_or_else(|_| "unsupported".into())
            } else {
                "unsupported".into()
            };
            review.capabilities.push(capability(
                "generic-netlist",
                if state.starts_with("explicit_fields_only") {
                    "explicit_fields_only"
                } else {
                    "unsupported"
                },
                "generic-netlist",
                if state.starts_with("explicit_fields_only") {
                    "explicit-export-facts"
                } else {
                    "inventory"
                },
                format!("{name}: {state}; native/source-aware capabilities remain unavailable."),
            ));
        }
    }
    if review.root_digest.is_some() {
        if let Some(digest) = canonical_native_export_facts_digest(&review.occurrences) {
            review
                .artifact_digests
                .insert(NATIVE_FACTS_DIGEST_KEY.into(), digest);
        }
        let composite = schematic_composite_digest(&review.artifact_digests);
        review
            .artifact_digests
            .insert("schematic:composite".into(), composite);
    }
    let mut findings = Vec::new();
    for mismatch in &review.mismatches {
        findings.push(Finding {
            id: mismatch.check_id.clone(),
            severity: Severity::Medium,
            category: "Schematic reconciliation".into(),
            title: format!("{} differs across project artifacts", mismatch.field),
            evidence: format!(
                "Expected {}; observed {}; join {} ({} confidence).",
                mismatch.expected, mismatch.actual, mismatch.join, mismatch.confidence
            ),
            recommendation: "Regenerate schematic, board, BOM, placement, and netlist exports from one project revision.".into(),
            location: mismatch.location.clone(),
            source: "schematic-reconciliation".into(),
            gate_impact: GateImpact::EvidenceOnly,
        });
    }
    let mut channel_coverage = Vec::new();
    if let Some(report) = &review.native_erc {
        channel_coverage.push(native_coverage(
            "schematic-erc",
            "Native schematic ERC",
            report,
        ));
    }
    if let Some(report) = &review.native_parity {
        channel_coverage.push(native_coverage(
            "schematic-parity",
            "Native board/schematic parity",
            report,
        ));
    }
    let native_complete = channel_coverage
        .iter()
        .all(|coverage| coverage.status == CoverageStatus::Passed);
    let status = if review.status == "completed" && review.mismatches.is_empty() && native_complete
    {
        CoverageStatus::Passed
    } else if review.status == "not_provided" {
        CoverageStatus::NotProvided
    } else if review.status == "ambiguous_root" {
        CoverageStatus::Unknown
    } else {
        CoverageStatus::Attention
    };
    let mut coverage = vec![Coverage {
        id: "schematic-evidence".into(),
        label: "Bounded schematic hierarchy and reconciliation".into(),
        status,
        evidence: format!(
            "{} occurrence(s), {} mismatch(es); all schematic families are evidence-only.",
            review.occurrence_count,
            review.mismatches.len()
        ),
    }];
    coverage.extend(channel_coverage);
    Ok((review, findings, coverage))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn marker(excluded: Option<bool>) -> Value {
        let mut value = serde_json::json!({
            "type": "clearance",
            "severity": "error",
            "description": "Synthetic marker",
            "items": [{"uuid": "item-1", "description": "Synthetic item"}]
        });
        if let Some(excluded) = excluded {
            value["excluded"] = Value::Bool(excluded);
        }
        value
    }

    fn drc(version: &str) -> Value {
        serde_json::json!({
            "source": "root.kicad_pcb",
            "date": "2026-08-27T00:00:00Z",
            "kicad_version": version,
            "coordinate_units": "mm",
            "violations": [marker(Some(false)), marker(Some(true)), marker(None)],
            "unconnected_items": [marker(Some(false))],
            "schematic_parity": [marker(Some(false)), marker(Some(true))]
        })
    }

    #[test]
    fn native_supported_majors_are_explicit() {
        assert_eq!(KiCadMajor::parse("KiCad 8.0.9"), Some(KiCadMajor::V8));
        assert_eq!(KiCadMajor::parse("9.0.6"), Some(KiCadMajor::V9));
        assert_eq!(KiCadMajor::parse("10.0.5"), Some(KiCadMajor::V10));
        assert_eq!(KiCadMajor::parse("7.0.11"), None);
        assert_eq!(KiCadMajor::parse("11.0.0"), None);
        assert_eq!(KiCadMajor::parse("unknown"), None);
    }

    #[test]
    fn native_exit_zero_and_five_complete_only() {
        assert!(completed_exit(0));
        assert!(completed_exit(5));
        assert!(!completed_exit(1));
        assert!(!completed_exit(3));
        assert!(!completed_exit(137));
    }

    #[test]
    fn native_command_vectors_are_fixed_and_non_mutating() {
        let output = Path::new("out.json");
        let input = Path::new("root.kicad_pcb");
        let ordinary = NativeKind::Drc {
            schematic_parity: false,
        }
        .args(output, input);
        assert_eq!(
            ordinary,
            [
                "pcb",
                "drc",
                "--format",
                "json",
                "--severity-all",
                "--exit-code-violations",
                "--output",
                "out.json",
                "root.kicad_pcb"
            ]
        );
        let parity = NativeKind::Drc {
            schematic_parity: true,
        }
        .args(output, input);
        assert!(parity.contains(&"--schematic-parity".into()));
        assert!(!parity.contains(&"--refill-zones".into()));
        assert!(!parity.contains(&"--save-board".into()));
        assert_eq!(
            NativeKind::Erc.args(Path::new("erc.json"), Path::new("root.kicad_sch")),
            [
                "sch",
                "erc",
                "--format",
                "json",
                "--severity-all",
                "--exit-code-violations",
                "--output",
                "erc.json",
                "root.kicad_sch"
            ]
        );
    }

    #[test]
    fn native_export_command_vectors_are_fixed_and_non_mutating() {
        assert_eq!(
            ExportKind::Bom.args(Path::new("bom.csv"), Path::new("root.kicad_sch")),
            [
                "sch",
                "export",
                "bom",
                "--output",
                "bom.csv",
                "root.kicad_sch"
            ]
        );
        assert_eq!(
            ExportKind::Netlist.args(Path::new("root.net"), Path::new("root.kicad_sch")),
            [
                "sch",
                "export",
                "netlist",
                "--format",
                "kicadsexpr",
                "--output",
                "root.net",
                "root.kicad_sch"
            ]
        );
        assert_eq!(
            ExportKind::Position.args(Path::new("positions.csv"), Path::new("root.kicad_pcb")),
            [
                "pcb",
                "export",
                "pos",
                "--format",
                "csv",
                "--units",
                "mm",
                "--side",
                "both",
                "--output",
                "positions.csv",
                "root.kicad_pcb"
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn native_export_runner_uses_fresh_bounded_output() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "ratemypcb-native-export-test-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let input = root.join("root.kicad_sch");
        fs::write(
            &input,
            "(kicad_sch (version 20250114) (uuid \"root-uuid\"))",
        )
        .unwrap();
        let script = root.join("kicad-cli");
        fs::write(
            &script,
            "#!/bin/sh\nif [ \"$1\" = version ]; then echo 10.0.5; exit 0; fi\nprevious=\nfor argument in \"$@\"; do\n  if [ \"$previous\" = --output ]; then output=$argument; fi\n  previous=$argument\ndone\nprintf '%s' '(export (version \"E\") (components) (nets))' > \"$output\"\n",
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        let (source, version) = run_export_inner(&script, &input, ExportKind::Netlist).unwrap();
        assert_eq!(version, "10.0.5");
        assert!(source.starts_with("(export"));
        fs::write(
            &script,
            "#!/bin/sh\nif [ \"$1\" = version ]; then echo 10.0.5; exit 0; fi\nprevious=\nfor argument in \"$@\"; do\n  if [ \"$previous\" = --output ]; then output=$argument; fi\n  previous=$argument\ndone\nprintf '%s' 'not a netlist' > \"$output\"\n",
        )
        .unwrap();
        assert!(
            run_export_inner(&script, &input, ExportKind::Netlist)
                .unwrap_err()
                .message
                .contains("Malformed native netlist export")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_drc_channels_and_exclusions_remain_distinct() {
        let bytes = serde_json::to_vec(&drc("10.0.5")).unwrap();
        let ordinary = parse_native_report(
            &bytes,
            KiCadMajor::V10,
            NativeKind::Drc {
                schematic_parity: false,
            },
        )
        .unwrap();
        let parity = parse_native_report(
            &bytes,
            KiCadMajor::V10,
            NativeKind::Drc {
                schematic_parity: true,
            },
        )
        .unwrap();
        assert_eq!(ordinary.violations.len(), 6);
        assert_eq!(parity.violations.len(), 6);
        assert_eq!(
            (
                ordinary.finding_count,
                ordinary.excluded_count,
                ordinary.unknown_exclusion_count
            ),
            (2, 1, 1)
        );
        assert_eq!(
            (
                parity.finding_count,
                parity.excluded_count,
                parity.unknown_exclusion_count
            ),
            (1, 1, 0)
        );
        assert_eq!(
            ordinary
                .violations
                .iter()
                .map(|marker| marker.group.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["schematic_parity", "unconnected_items", "violations"])
        );
        crate::validate_native_report(&ordinary, crate::NativeReportChannel::Drc).unwrap();
        crate::validate_native_report(&parity, crate::NativeReportChannel::SchematicParity)
            .unwrap();
        assert!(
            crate::validate_native_report(&ordinary, crate::NativeReportChannel::SchematicParity)
                .is_err()
        );
        assert!(crate::validate_native_report(&parity, crate::NativeReportChannel::Drc).is_err());
        let mut forged = parity;
        forged.finding_count = forged
            .violations
            .iter()
            .filter(|marker| marker.excluded == Some(false))
            .count();
        forged.excluded_count = forged
            .violations
            .iter()
            .filter(|marker| marker.excluded == Some(true))
            .count();
        forged.unknown_exclusion_count = forged
            .violations
            .iter()
            .filter(|marker| marker.excluded.is_none())
            .count();
        assert!(
            crate::validate_native_report(&forged, crate::NativeReportChannel::SchematicParity)
                .is_err()
        );
    }

    #[test]
    fn native_marker_identity_is_stable_under_report_reordering() {
        let first = marker(Some(false));
        let mut second = marker(Some(false));
        second["type"] = Value::String("unconnected".into());
        second["items"][0]["uuid"] = Value::String("item-2".into());
        let report = |markers: Vec<Value>| {
            let value = serde_json::json!({
                "source": "root.kicad_pcb",
                "date": "2026-08-27T00:00:00Z",
                "kicad_version": "10.0.5",
                "violations": markers,
                "unconnected_items": [],
                "schematic_parity": []
            });
            parse_native_report(
                &serde_json::to_vec(&value).unwrap(),
                KiCadMajor::V10,
                NativeKind::Drc {
                    schematic_parity: false,
                },
            )
            .unwrap()
        };
        let forward = report(vec![first.clone(), second.clone()]);
        let reversed = report(vec![second, first]);
        let identities = |report: NativeReport| {
            report
                .violations
                .into_iter()
                .map(|marker| (marker.id, marker.structural_location))
                .collect::<BTreeSet<_>>()
        };
        assert_eq!(identities(forward), identities(reversed));
    }

    #[test]
    fn native_erc_preserves_sheet_occurrence_paths() {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "source": "root.kicad_sch",
            "date": "2026-08-27T00:00:00Z",
            "kicad_version": "9.0.6",
            "sheets": [{
                "path": "/root/child",
                "uuid_path": "/root-uuid/child-occurrence-2",
                "violations": [marker(None)]
            }]
        }))
        .unwrap();
        let report = parse_native_report(&bytes, KiCadMajor::V9, NativeKind::Erc).unwrap();
        assert_eq!(report.violations[0].group, "erc");
        assert_eq!(
            report.violations[0].sheet_path.as_deref(),
            Some("/root/child")
        );
        assert_eq!(
            report.violations[0].sheet_uuid_path.as_deref(),
            Some("/root-uuid/child-occurrence-2")
        );
        assert_eq!(report.violations[0].excluded, None);
    }

    #[test]
    fn native_released_fixture_manifests_and_reports_match() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/kicad/supported");
        for (directory, major, locally_executed) in [
            ("8", KiCadMajor::V8, false),
            ("9", KiCadMajor::V9, false),
            ("10", KiCadMajor::V10, true),
        ] {
            let fixture = root.join(directory).join("hierarchical");
            let manifest: Value =
                serde_json::from_slice(&fs::read(fixture.join("manifest.json")).unwrap()).unwrap();
            assert_eq!(manifest["locallyExecuted"], locally_executed);
            if !locally_executed {
                assert!(
                    manifest["attestation"]
                        .as_str()
                        .unwrap()
                        .contains("no KiCad")
                );
            }
            for (name, expected) in manifest["sha256"].as_object().unwrap() {
                assert_eq!(
                    crate::sha256(fs::read(fixture.join(name)).unwrap()),
                    expected.as_str().unwrap()
                );
            }
            for name in ["erc.json", "drc.json"] {
                let bytes = fs::read(fixture.join(name)).unwrap();
                let kind = if name == "erc.json" {
                    NativeKind::Erc
                } else {
                    NativeKind::Drc {
                        schematic_parity: false,
                    }
                };
                assert!(parse_native_report(&bytes, major, kind).is_ok());
            }
        }
    }

    #[test]
    fn native_failure_fixtures_preserve_exclusion_unknown_and_reject_bad_json() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/kicad/native-failures");
        let manifest: Value =
            serde_json::from_slice(&fs::read(root.join("manifest.json")).unwrap()).unwrap();
        for (name, expected) in manifest["sha256"].as_object().unwrap() {
            assert_eq!(
                crate::sha256(fs::read(root.join(name)).unwrap()),
                expected.as_str().unwrap()
            );
        }
        let report = parse_native_report(
            &fs::read(root.join("erc-exclusions-9.json")).unwrap(),
            KiCadMajor::V9,
            NativeKind::Erc,
        )
        .unwrap();
        assert_eq!(report.violations[0].excluded, Some(true));
        assert_eq!(report.violations[1].excluded, None);
        for name in ["malformed.json", "truncated.json"] {
            assert!(
                parse_native_report(
                    &fs::read(root.join(name)).unwrap(),
                    KiCadMajor::V10,
                    NativeKind::Drc {
                        schematic_parity: false,
                    },
                )
                .is_err()
            );
        }
    }

    #[test]
    fn native_export_facts_replace_source_fallbacks_with_visible_provenance() {
        let mut occurrences = vec![SchematicOccurrence {
            key: crate::sha256("occurrence"),
            project_identity: "root.kicad_pro".into(),
            root_digest: crate::sha256("root"),
            sheet_uuid_path: "/root".into(),
            item_uuid: "item".into(),
            source_path: "root.kicad_sch".into(),
            reference: Some("R1".into()),
            unit: Some("1".into()),
            facts: vec![SchematicFact {
                name: "value".into(),
                value: "source-value".into(),
                producer: "kicad-source".into(),
                evidence_class: "explicit-source-fact".into(),
                source_path: "root.kicad_sch".into(),
                confidence: "high".into(),
            }],
        }];
        let components = vec![ArtifactComponent {
            path: Some("/root".into()),
            item_uuid: Some("item".into()),
            reference: Some("R1".into()),
            value: Some("native-value".into()),
            pads: BTreeMap::from([("1".into(), "GND".into())]),
            ..ArtifactComponent::default()
        }];
        apply_native_facts(
            &mut occurrences,
            &components,
            "10.0.5",
            "native:netlist.net",
        );
        let value = occurrences[0]
            .facts
            .iter()
            .find(|fact| fact.name == "value")
            .unwrap();
        assert_eq!(value.value, "native-value");
        assert_eq!(value.producer, "kicad-cli 10.0.5");
        assert_eq!(value.evidence_class, "explicit-export-facts");
        assert!(occurrences[0].facts.iter().any(|fact| fact.name == "pin:1"));
    }

    #[test]
    fn canonical_native_fact_identity_ignores_order_and_raw_export_volatility() {
        let native_fact = |name: &str, value: &str| SchematicFact {
            name: name.into(),
            value: value.into(),
            producer: "kicad-cli 10.0.5".into(),
            evidence_class: "explicit-export-facts".into(),
            source_path: "native:netlist.net".into(),
            confidence: "high".into(),
        };
        let occurrence = |key: &str, facts: Vec<SchematicFact>| SchematicOccurrence {
            key: crate::sha256(key),
            project_identity: "root.kicad_pro".into(),
            root_digest: crate::sha256("root"),
            sheet_uuid_path: format!("/{key}"),
            item_uuid: key.into(),
            source_path: "root.kicad_sch".into(),
            reference: None,
            unit: None,
            facts,
        };
        let forward = vec![
            occurrence(
                "a",
                vec![native_fact("value", "10k"), native_fact("pin:1", "GND")],
            ),
            occurrence("b", vec![native_fact("value", "1u")]),
        ];
        let mut reordered = forward.clone();
        reordered.reverse();
        reordered[1].facts.reverse();
        let canonical = canonical_native_export_facts_digest(&forward).unwrap();
        assert_eq!(
            canonical_native_export_facts_digest(&reordered).as_deref(),
            Some(canonical.as_str())
        );

        let composite = |raw: &str| {
            schematic_composite_digest(&BTreeMap::from([
                ("native:netlist.net".into(), crate::sha256(raw)),
                (NATIVE_FACTS_DIGEST_KEY.into(), canonical.clone()),
            ]))
        };
        assert_eq!(
            composite("(export (date one) (path /tmp/one))"),
            composite("(export (date two) (path /tmp/two))")
        );

        let mut changed = forward;
        changed[0].facts[0].value = "12k".into();
        assert_ne!(
            canonical_native_export_facts_digest(&changed).as_deref(),
            Some(canonical.as_str())
        );
    }

    #[test]
    fn canonical_native_fact_digest_cannot_be_forged_independently_of_facts() {
        let mut review = SchematicReview {
            status: "completed".into(),
            root_digest: Some(crate::sha256("root")),
            occurrence_count: 1,
            occurrences: vec![SchematicOccurrence {
                key: crate::sha256("occurrence"),
                project_identity: "root.kicad_pro".into(),
                root_digest: crate::sha256("root"),
                sheet_uuid_path: "/root".into(),
                item_uuid: "item".into(),
                source_path: "root.kicad_sch".into(),
                reference: Some("R1".into()),
                unit: Some("1".into()),
                facts: vec![SchematicFact {
                    name: "value".into(),
                    value: "10k".into(),
                    producer: "kicad-cli 10.0.5".into(),
                    evidence_class: "explicit-export-facts".into(),
                    source_path: "native:bom.csv".into(),
                    confidence: "high".into(),
                }],
            }],
            ..SchematicReview::default()
        };
        let canonical = canonical_native_export_facts_digest(&review.occurrences).unwrap();
        review
            .artifact_digests
            .insert(NATIVE_FACTS_DIGEST_KEY.into(), canonical);
        review.artifact_digests.insert(
            "schematic:composite".into(),
            schematic_composite_digest(&review.artifact_digests),
        );
        crate::validate_schematic_report(&review).unwrap();

        let rebind = |report: &mut SchematicReview| {
            if let Some(digest) = canonical_native_export_facts_digest(&report.occurrences) {
                report
                    .artifact_digests
                    .insert(NATIVE_FACTS_DIGEST_KEY.into(), digest);
            } else {
                report.artifact_digests.remove(NATIVE_FACTS_DIGEST_KEY);
            }
            report.artifact_digests.insert(
                "schematic:composite".into(),
                schematic_composite_digest(&report.artifact_digests),
            );
        };

        let mut changed_path = review.clone();
        changed_path.occurrences[0].facts[0].source_path = "root.kicad_sch".into();
        assert!(canonical_native_export_facts_digest(&changed_path.occurrences).is_some());
        assert!(crate::validate_schematic_report(&changed_path).is_err());

        let mut changed_class = review.clone();
        changed_class.occurrences[0].facts[0].evidence_class = "explicit-source-fact".into();
        assert!(canonical_native_export_facts_digest(&changed_class.occurrences).is_none());
        assert!(crate::validate_schematic_report(&changed_class).is_err());

        let mut changed_producer = review.clone();
        changed_producer.occurrences[0].facts[0].producer = "kicad-source".into();
        assert!(crate::validate_schematic_report(&changed_producer).is_err());

        let mut missing = review.clone();
        missing.artifact_digests.remove(NATIVE_FACTS_DIGEST_KEY);
        assert!(crate::validate_schematic_report(&missing).is_err());

        let mut self_consistent = review.clone();
        self_consistent.occurrences[0].facts[0].source_path = "root.kicad_sch".into();
        rebind(&mut self_consistent);
        assert!(crate::validate_schematic_report(&self_consistent).is_err());

        let mut unknown = review.clone();
        unknown.occurrences[0].facts[0].evidence_class = "unknown-fact-class".into();
        rebind(&mut unknown);
        assert!(crate::validate_schematic_report(&unknown).is_err());

        let mut oversized = review.clone();
        oversized.occurrences[0].facts[0].value = "x".repeat(4097);
        rebind(&mut oversized);
        assert!(crate::validate_schematic_report(&oversized).is_err());

        let mut source_only = review.clone();
        let occurrence_source = source_only.occurrences[0].source_path.clone();
        let source_fact = &mut source_only.occurrences[0].facts[0];
        source_fact.producer = "kicad-source".into();
        source_fact.evidence_class = "explicit-source-fact".into();
        source_fact.source_path = occurrence_source;
        rebind(&mut source_only);
        crate::validate_schematic_report(&source_only).unwrap();

        let mut extra = review;
        extra.occurrences[0].facts.clear();
        assert!(crate::validate_schematic_report(&extra).is_err());
    }

    #[test]
    fn delimited_exports_reject_empty_malformed_and_overflow_rows() {
        assert!(delimited_rows("").is_err());
        assert!(delimited_rows("reference,value\nR1").is_err());
        let overflow = std::iter::once("reference,value".to_owned())
            .chain((0..=MAX_GENERIC_RECORDS).map(|index| format!("R{index},10k")))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(delimited_rows(&overflow).is_err());
    }

    #[test]
    fn native_parser_rejects_wrong_kind_version_truncation_and_marker_overflow() {
        let erc = serde_json::json!({
            "source": "root.kicad_sch", "date": "now", "kicad_version": "10.0.5", "sheets": []
        });
        assert!(
            parse_native_report(
                &serde_json::to_vec(&erc).unwrap(),
                KiCadMajor::V10,
                NativeKind::Drc {
                    schematic_parity: false
                }
            )
            .is_err()
        );
        assert!(
            parse_native_report(
                &serde_json::to_vec(&drc("9.0.6")).unwrap(),
                KiCadMajor::V10,
                NativeKind::Drc {
                    schematic_parity: false
                }
            )
            .is_err()
        );
        assert!(
            parse_native_report(
                b"{\"kicad_version\":",
                KiCadMajor::V10,
                NativeKind::Drc {
                    schematic_parity: false
                }
            )
            .is_err()
        );
        let mut overflow = drc("10.0.5");
        overflow["violations"] = Value::Array(
            (0..=MAX_NATIVE_MARKERS)
                .map(|_| marker(Some(false)))
                .collect(),
        );
        assert!(
            parse_native_report(
                &serde_json::to_vec(&overflow).unwrap(),
                KiCadMajor::V10,
                NativeKind::Drc {
                    schematic_parity: false
                }
            )
            .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn native_runner_rejects_nonzero_missing_report_and_timeout() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "ratemypcb-native-runner-test-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let input = root.join("root.kicad_pcb");
        fs::write(&input, "(kicad_pcb (version 20240108))").unwrap();
        let script = root.join("kicad-cli");
        let run = |body: &str| {
            fs::write(
                &script,
                format!(
                    "#!/bin/sh\nif [ \"$1\" = version ]; then echo 10.0.5; exit 0; fi\n{body}\n"
                ),
            )
            .unwrap();
            fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
            run_native_inner(
                &script,
                &input,
                NativeKind::Drc {
                    schematic_parity: false,
                },
                Duration::from_secs(2),
            )
            .unwrap_err()
            .message
        };
        let nonzero = run("echo synthetic-failure >&2; exit 2");
        assert!(nonzero.contains("exited 2"), "{nonzero}");
        let missing = run("exit 0");
        assert!(
            missing.contains("Cannot read native JSON report"),
            "{missing}"
        );
        let timeout = run("sleep 3");
        assert!(timeout.contains("timed out"), "{timeout}");
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn native_runner_covers_exit_five_output_limit_and_major_mapping() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "ratemypcb-native-matrix-test-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let input = root.join("root.kicad_pcb");
        fs::write(&input, "(kicad_pcb (version 20240108))").unwrap();
        let script = root.join("kicad-cli");
        let write_script = |version: &str, body: &str| {
            fs::write(
                &script,
                format!(
                    "#!/bin/sh\nif [ \"$1\" = version ]; then echo {version}; exit 0; fi\nprevious=\nfor argument in \"$@\"; do\n  if [ \"$previous\" = --output ]; then output=$argument; fi\n  previous=$argument\ndone\n{body}\n"
                ),
            )
            .unwrap();
            fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
        };
        let valid = r#"printf '%s' '{"source":"root.kicad_pcb","date":"now","kicad_version":"10.0.5","violations":[],"unconnected_items":[],"schematic_parity":[]}' > "$output"; exit 5"#;
        write_script("10.0.5", valid);
        assert_eq!(
            run_native_inner(
                &script,
                &input,
                NativeKind::Drc {
                    schematic_parity: false,
                },
                Duration::from_secs(2),
            )
            .unwrap()
            .status,
            "completed"
        );

        write_script(
            "10.0.5",
            "dd if=/dev/zero of=\"$output\" bs=1048576 count=5 2>/dev/null; exit 0",
        );
        assert!(
            run_native_inner(
                &script,
                &input,
                NativeKind::Drc {
                    schematic_parity: false,
                },
                Duration::from_secs(2),
            )
            .unwrap_err()
            .message
            .contains("exceeds")
        );

        let wrong_major = r#"printf '%s' '{"source":"root.kicad_pcb","date":"now","kicad_version":"9.0.6","violations":[],"unconnected_items":[],"schematic_parity":[]}' > "$output"; exit 0"#;
        write_script("10.0.5", wrong_major);
        assert!(
            run_native_inner(
                &script,
                &input,
                NativeKind::Drc {
                    schematic_parity: false,
                },
                Duration::from_secs(2),
            )
            .unwrap_err()
            .message
            .contains("major")
        );

        write_script("11.0.0", "exit 0");
        let failure = run_native_inner(
            &script,
            &input,
            NativeKind::Drc {
                schematic_parity: false,
            },
            Duration::from_secs(2),
        )
        .unwrap_err();
        assert_eq!(
            finish_native_run(
                Err(NativeFailure {
                    version: failure.version.clone(),
                    message: failure.message.clone(),
                }),
                NativeMode::Auto,
            )
            .unwrap()
            .status,
            "not_run"
        );
        assert!(finish_native_run(Err(failure), NativeMode::Required).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_auto_failures_are_not_run_and_required_failures_error() {
        let missing = Path::new("/definitely/missing/kicad-cli");
        let input = Path::new("root.kicad_pcb");
        let kind = NativeKind::Drc {
            schematic_parity: false,
        };
        let failure =
            run_native_inner(missing, input, kind, Duration::from_millis(10)).unwrap_err();
        let auto = finish_native_run(
            Err(NativeFailure {
                version: failure.version.clone(),
                message: failure.message.clone(),
            }),
            NativeMode::Auto,
        )
        .unwrap();
        assert_eq!(auto.status, "not_run");
        assert!(auto.note.contains("Cannot run kicad-cli"));
        assert!(finish_native_run(Err(failure), NativeMode::Required).is_err());
    }
}
