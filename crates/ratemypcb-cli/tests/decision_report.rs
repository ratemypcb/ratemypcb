use serde_json::{Value, json};
use sha2::Digest;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const FIXTURE: &str = "tests/fixtures/narrow-board.kicad_pcb";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "ratemypcb-decision-report-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ratemypcb"))
        .args(args)
        .current_dir(repository_root())
        .output()
        .unwrap()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn review(temp: &TempDir) -> (PathBuf, Vec<u8>, Value) {
    let report_path = temp.join("report.json");
    let report_arg = report_path.to_string_lossy().into_owned();
    let output = cli(&[
        "review",
        FIXTURE,
        "--native",
        "off",
        "--format",
        "json",
        "--output",
        &report_arg,
    ]);
    assert_success(&output);
    let bytes = fs::read(&report_path).unwrap();
    let report: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(report["input"]["path"], FIXTURE);
    assert!(!String::from_utf8_lossy(&bytes).contains(&repository_root().to_string_lossy()[..]));
    (report_path, bytes, report)
}

fn digest(report_path: &Path) -> String {
    let report_arg = report_path.to_string_lossy().into_owned();
    let output = cli(&["digest", &report_arg]);
    assert_success(&output);
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn assessment(report: &Value, digest: &str) -> Value {
    let finding = report["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["kind"] == "finding")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let coverage = report["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["kind"] == "coverage")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    json!({
        "assessmentSchemaVersion": "2.0",
        "reportDigest": digest,
        "rating": 4,
        "disposition": "blocked",
        "verdict": "Blocked pending fabrication evidence",
        "verdictEvidenceRefs": [coverage],
        "rationale": "Required manufacturing checks did not run.",
        "categorySummaries": [{
            "categoryId": "fabrication",
            "summary": "Fabrication evidence is incomplete.",
            "evidenceRefs": [coverage]
        }],
        "actions": [{
            "priority": 1,
            "title": "Run the required fabrication checks",
            "rationale": "Supply the missing release evidence before approval.",
            "evidenceRefs": [finding, coverage]
        }],
        "questions": [{
            "question": "Which fabrication profile applies?",
            "evidenceRefs": [coverage]
        }]
    })
}

fn write_assessment(temp: &TempDir, value: &Value, name: &str) -> PathBuf {
    let path = temp.join(name);
    fs::write(&path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    path
}

fn render(report: &Path, assessment: &Path, output: &Path) -> Output {
    let report = report.to_string_lossy();
    let assessment = assessment.to_string_lossy();
    let output = output.to_string_lossy();
    cli(&[
        "render",
        "--report",
        &report,
        "--assessment",
        &assessment,
        "--output",
        &output,
    ])
}

#[test]
fn fail_on_ignores_evidence_only_schematic_mismatches() {
    let temp = TempDir::new();
    let project = temp.join("mismatch-project");
    fs::create_dir(&project).unwrap();
    for name in ["root.kicad_pro", "root.kicad_sch", "root.kicad_pcb"] {
        fs::copy(
            repository_root()
                .join("tests/fixtures/kicad/mismatch")
                .join(name),
            project.join(name),
        )
        .unwrap();
    }
    let board = project.join("root.kicad_pcb");
    let source = fs::read_to_string(&board).unwrap();
    fs::write(
        &board,
        source.replace(
            "(property \"Value\" \"10k\")",
            "(property \"Value\" \"20k\")",
        ),
    )
    .unwrap();
    let output_path = temp.join("mismatch.json");
    let output = cli(&[
        "review",
        &project.to_string_lossy(),
        "--native",
        "off",
        "--format",
        "json",
        "--output",
        &output_path.to_string_lossy(),
        "--fail-on",
        "medium",
    ]);
    assert_success(&output);
    let report: Value = serde_json::from_slice(&fs::read(output_path).unwrap()).unwrap();
    assert_eq!(report["input"]["path"], "mismatch-project");
    assert!(
        report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|finding| {
                finding["source"] == "schematic-reconciliation"
                    && finding["gateImpact"] == "evidence_only"
                    && finding["severity"] == "medium"
            })
    );
}

#[test]
fn required_native_failure_exits_three() {
    let output = Command::new(env!("CARGO_BIN_EXE_ratemypcb"))
        .args([
            "review", FIXTURE, "--native", "required", "--format", "json",
        ])
        .current_dir(repository_root())
        .env("PATH", "")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Cannot run kicad-cli"));
}

#[test]
fn tracer_carries_exact_report_digest_and_blocked_decision_to_html() {
    let temp = TempDir::new();
    let (report_path, original_bytes, report) = review(&temp);
    let report_digest = digest(&report_path);
    assert_eq!(fs::read(&report_path).unwrap(), original_bytes);

    let assessment = assessment(&report, &report_digest);
    let assessment_path = write_assessment(&temp, &assessment, "assessment.json");
    let html_path = temp.join("report.html");
    let output = render(&report_path, &assessment_path, &html_path);
    assert_success(&output);
    assert_eq!(fs::read(&report_path).unwrap(), original_bytes);

    let html = fs::read_to_string(html_path).unwrap();
    let decision = html
        .find("data-report-landmark=\"release-decision\"")
        .unwrap();
    let completeness = html.find("data-report-landmark=\"completeness\"").unwrap();
    let scores = html.find("data-report-landmark=\"scores\"").unwrap();
    assert!(decision < completeness && completeness < scores);
    assert_eq!(
        html.matches("data-report-landmark=\"release-decision\"")
            .count(),
        1
    );
    for text in [
        "id=\"scope\"",
        "id=\"source\"",
        "id=\"evidence-time\"",
        "Required evidence completeness &amp; freshness",
        "Run the required fabrication checks",
        "not_run",
        "BLOCKED — Blocked pending fabrication evidence",
    ] {
        assert!(html.contains(text), "missing tracer value: {text}");
    }
    assert!(!html.contains("https://cdn."));
    assert!(!html.contains("src=\"http"));
    assert!(!html.contains("href=\"/local-viewer.css\""));
    assert!(!html.contains("src=\"/local-viewer.js\""));
    assert!(html.contains("globalThis.RATEMYPCB_PAYLOAD="));
}

#[test]
fn assessment_refs_share_one_anchor_mapping_and_invalid_inputs_are_rejected() {
    let temp = TempDir::new();
    let (report_path, original_bytes, report) = review(&temp);
    let report_digest = digest(&report_path);
    let valid = assessment(&report, &report_digest);
    let assessment_path = write_assessment(&temp, &valid, "assessment.json");
    let html_path = temp.join("report.html");
    assert_success(&render(&report_path, &assessment_path, &html_path));

    let evidence: BTreeSet<_> = report["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    let refs = valid["verdictEvidenceRefs"]
        .as_array()
        .unwrap()
        .iter()
        .chain(
            valid["categorySummaries"][0]["evidenceRefs"]
                .as_array()
                .unwrap(),
        )
        .chain(valid["actions"][0]["evidenceRefs"].as_array().unwrap())
        .chain(valid["questions"][0]["evidenceRefs"].as_array().unwrap());
    for evidence_ref in refs {
        assert!(evidence.contains(evidence_ref.as_str().unwrap()));
    }

    let html = fs::read_to_string(&html_path).unwrap();
    assert!(html.contains("function evidenceAnchor(publicId)"));
    assert!(html.contains("encodeURIComponent(publicId)"));
    assert!(html.contains("link.href = `#${evidenceAnchor(publicId)}`"));
    assert!(html.contains("node.id = evidenceAnchor(record.id)"));
    for evidence_ref in evidence {
        assert!(html.contains(evidence_ref));
    }

    fs::write(&report_path, [&original_bytes[..], b" "].concat()).unwrap();
    let mismatch = render(&report_path, &assessment_path, &temp.join("mismatch.html"));
    assert_eq!(mismatch.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&mismatch.stderr).contains("reportDigest does not match"));
    fs::write(&report_path, &original_bytes).unwrap();

    let mut invalid_report = report.clone();
    invalid_report["evidence"][1]["id"] = invalid_report["evidence"][0]["id"].clone();
    let invalid_report_path = temp.join("invalid-report.json");
    fs::write(
        &invalid_report_path,
        serde_json::to_vec(&invalid_report).unwrap(),
    )
    .unwrap();
    let invalid = cli(&[
        "render",
        "--report",
        &invalid_report_path.to_string_lossy(),
        "--output",
        &temp.join("invalid.html").to_string_lossy(),
    ]);
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("invalid report"));

    let mut broken = valid;
    broken["questions"][0]["evidenceRefs"][0] = json!(format!("ev-{}", "0".repeat(64)));
    let broken_path = write_assessment(&temp, &broken, "broken-assessment.json");
    let broken_output = render(&report_path, &broken_path, &temp.join("broken.html"));
    assert_eq!(broken_output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&broken_output.stderr).contains("unknown evidence ID"));
}

#[test]
fn schematic_selector_resolves_only_ambiguous_bounded_roots() {
    let temp = TempDir::new();
    let report_path = temp.join("schematic.json");
    let output = cli(&[
        "review",
        "tests/fixtures/kicad/hierarchy/ambiguous-roots",
        "--schematic",
        "beta.kicad_sch",
        "--native",
        "off",
        "--format",
        "json",
        "--output",
        &report_path.to_string_lossy(),
    ]);
    assert_success(&output);
    let report: Value = serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
    assert_eq!(report["schematic"]["rootPath"], "beta.kicad_sch");
    assert!(
        report["requiredEvidence"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| !item["checkId"].as_str().unwrap().starts_with("schematic"))
    );

    for selector in ["../beta.kicad_sch", "/beta.kicad_sch", "missing.kicad_sch"] {
        let invalid = cli(&[
            "review",
            "tests/fixtures/kicad/hierarchy/ambiguous-roots",
            "--schematic",
            selector,
            "--native",
            "off",
            "--format",
            "json",
        ]);
        assert_eq!(invalid.status.code(), Some(2), "{selector}");
    }
    let unnecessary = cli(&[
        "review",
        "tests/fixtures/kicad/hierarchy/reused-child",
        "--schematic",
        "root.kicad_sch",
        "--native",
        "off",
        "--format",
        "json",
    ]);
    assert_eq!(unnecessary.status.code(), Some(2));
}

#[test]
fn schematic_doctor_and_snapshot_expose_capabilities_without_client_policy() {
    let doctor = cli(&["doctor", "--json"]);
    assert_success(&doctor);
    let doctor: Value = serde_json::from_slice(&doctor.stdout).unwrap();
    assert_eq!(doctor["kicadCli"]["supportedMajors"], json!([8, 9, 10]));
    for capability in ["pcbDrc", "schematicErc", "coherentProjectParity"] {
        assert!(
            doctor["capabilities"][capability].is_object(),
            "{capability}"
        );
    }
    assert_eq!(
        doctor["capabilities"]["limitations"]["zipNativeChecks"],
        false
    );
    assert_eq!(
        doctor["capabilities"]["limitations"]["altiumNativeChecks"],
        false
    );
    assert_eq!(
        doctor["capabilities"]["limitations"]["genericNetlistNativeChecks"],
        false
    );

    let help = cli(&["--help"]);
    assert_success(&help);
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(help.contains("--schematic PATH"));
    assert!(help.contains("resolves only ambiguous automatic roots"));

    let temp = TempDir::new();
    let report_path = temp.join("report.json");
    let output = cli(&[
        "review",
        "tests/fixtures/kicad/mismatch",
        "--native",
        "off",
        "--format",
        "json",
        "--output",
        &report_path.to_string_lossy(),
    ]);
    assert_success(&output);
    let bytes = fs::read(&report_path).unwrap();
    let report: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        report["schematic"]["sourcePair"]["projectIdentity"],
        "root.kicad_pro"
    );
    assert_eq!(
        digest(&report_path),
        format!("{:x}", sha2::Sha256::digest(&bytes))
    );
    assert_eq!(fs::read(&report_path).unwrap(), bytes);

    let schematic_evidence = report["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["checkId"] == "schematic-evidence")
        .unwrap()["id"]
        .clone();
    let mut assessment = assessment(&report, &digest(&report_path));
    assessment["verdictEvidenceRefs"] = json!([schematic_evidence.clone()]);
    assessment["categorySummaries"][0]["evidenceRefs"] = json!([schematic_evidence.clone()]);
    assessment["actions"][0]["evidenceRefs"] = json!([schematic_evidence.clone()]);
    assessment["questions"][0]["evidenceRefs"] = json!([schematic_evidence.clone()]);
    let assessment_path = write_assessment(&temp, &assessment, "assessment.json");
    let html_path = temp.join("report.html");
    assert_success(&render(&report_path, &assessment_path, &html_path));
    let html = fs::read_to_string(html_path).unwrap();
    for text in [
        "function renderSchematicEvidence(report)",
        "Project source pair",
        "schematic.sourcePair",
        "marker.structuralLocation",
        "marker.excluded ?? \"unknown\"",
        "mismatch.gateImpact",
    ] {
        assert!(html.contains(text), "missing {text}");
    }
    assert!(html.contains(schematic_evidence.as_str().unwrap()));
    assert!(!html.contains("<script>alert(1)</script>"));
}
