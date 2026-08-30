use serde_json::{Value, json};
use sha2::Digest;
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
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

fn x2_layer(function: &str, profile: bool) -> Vec<u8> {
    let geometry = if profile {
        "G36*\nX000000Y000000D02*\nX10000000Y000000D01*\nX10000000Y10000000D01*\nX000000Y10000000D01*\nX000000Y000000D01*\nG37*\n"
    } else {
        "X1000000Y1000000D02*\nX2000000Y1000000D01*\n"
    };
    format!(
        "G04 RateMyPCB CLI tracer fixture*\n%FSLAX46Y46*%\n%MOMM*%\n%TF.FileFunction,{function}*%\n%TA.AperFunction,Conductor*%\n%ADD10C,0.200*%\nD10*\n%TO.N,GND*%\n%TO.C,U1*%\n%TO.P,U1,1*%\n{geometry}M02*\n"
    )
    .into_bytes()
}

fn fabrication_files(board_end: &str, hostile: bool) -> Vec<(String, Vec<u8>)> {
    let board = format!(
        "(kicad_pcb (version 20240108) (generator ratemypcb-fixture)\n  (title_block (title \"phase5-board\") (rev \"r1\"))\n  (layers (0 \"F.Cu\" signal) (31 \"B.Cu\" signal) (44 \"Edge.Cuts\" user))\n  (net 0 \"\") (net 1 \"GND\")\n  (footprint \"Fixture:Connector\" (layer \"F.Cu\") (at 0 0)\n    (property \"Reference\" \"U1\")\n    (pad \"1\" thru_hole circle (at 1 1) (size 1 1) (drill 0.6) (layers \"*.Cu\" \"*.Mask\") (net 1 \"GND\"))\n    (pad \"1\" thru_hole oval (at 5.5 5) (size 2 1) (drill oval 1.6 0.6) (layers \"*.Cu\" \"*.Mask\") (net 1 \"GND\")))\n  (gr_rect (start 0 0) (end {board_end}) (layer \"Edge.Cuts\")))"
    );
    let mut top = x2_layer("Copper,L1,Top", false);
    if hostile {
        top.truncate(top.len() - "M02*\n".len());
    }
    vec![
        ("board.kicad_pcb".into(), board.into_bytes()),
        ("top.gbr".into(), top),
        ("bottom.gbr".into(), x2_layer("Copper,L2,Bot", false)),
        ("profile.gbr".into(), x2_layer("Profile,NP", true)),
        (
            "holes.xnc".into(),
            fs::read(repository_root().join("tests/fixtures/fabrication/xnc/strict.xnc")).unwrap(),
        ),
        (
            "complete.gbrjob".into(),
            fs::read(repository_root().join("tests/fixtures/fabrication/job/complete.gbrjob"))
                .unwrap(),
        ),
    ]
}

fn write_fabrication_project(root: &Path, board_end: &str, hostile: bool) {
    fs::create_dir(root).unwrap();
    for (name, bytes) in fabrication_files(board_end, hostile) {
        fs::write(root.join(name), bytes).unwrap();
    }
}

fn write_fabrication_zip(path: &Path) {
    let mut archive = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, bytes) in fabrication_files("10 10", false) {
        archive
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(&bytes).unwrap();
    }
    archive.finish().unwrap();
}

fn fabrication_review(path: &Path, output: &Path) -> Value {
    let result = cli(&[
        "review",
        &path.to_string_lossy(),
        "--native",
        "off",
        "--scope",
        "fabrication",
        "--format",
        "json",
        "--output",
        &output.to_string_lossy(),
    ]);
    assert_success(&result);
    serde_json::from_slice(&fs::read(output).unwrap()).unwrap()
}

#[test]
fn fabrication_directory_zip_digest_render_and_fail_closed_tracer() {
    let temp = TempDir::new();
    let clean = temp.join("clean");
    write_fabrication_project(&clean, "10 10", false);
    let clean_report_path = temp.join("clean.json");
    let clean_report = fabrication_review(&clean, &clean_report_path);
    assert_eq!(
        clean_report["fabrication"]["status"],
        "complete",
        "{}",
        serde_json::to_string_pretty(&clean_report["fabrication"]).unwrap()
    );
    assert_eq!(
        clean_report["fabrication"]["reconciliations"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    assert!(
        clean_report["fabrication"]["reconciliations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["status"] == "match")
    );
    assert!(
        clean_report["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .any(|record| {
                record["checkId"] == "package-gerbers"
                    && clean_report["coverage"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|coverage| {
                            coverage["id"] == record["id"] && coverage["status"] == "passed"
                        })
            })
    );

    let archive = temp.join("package.zip");
    write_fabrication_zip(&archive);
    let zip_report = fabrication_review(&archive, &temp.join("zip.json"));
    assert_eq!(zip_report["fabrication"]["status"], "complete");
    assert!(zip_report["fabrication"]["sourcePair"].is_object());

    let mismatch = temp.join("mismatch");
    write_fabrication_project(&mismatch, "11 10", false);
    let mismatch_path = temp.join("mismatch.json");
    let mismatch_report = fabrication_review(&mismatch, &mismatch_path);
    assert_eq!(mismatch_report["fabrication"]["status"], "partial");
    assert_eq!(mismatch_report["approvalEligible"], false);
    assert!(
        mismatch_report["fabrication"]["reconciliations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["status"] == "mismatch")
    );

    let report_digest = digest(&mismatch_path);
    let assessment = assessment(&mismatch_report, &report_digest);
    let assessment_path = write_assessment(&temp, &assessment, "fabrication-assessment.json");
    let html_path = temp.join("fabrication.html");
    assert_success(&render(&mismatch_path, &assessment_path, &html_path));
    let html = fs::read_to_string(html_path).unwrap();
    for value in [
        "function renderFabricationEvidence(report)",
        "nativeArtifactDigest",
        "smallestEvidenceAction",
        "resolution_bounded",
        "mismatch",
    ] {
        assert!(html.contains(value), "missing fabrication tracer: {value}");
    }

    let hostile = temp.join("hostile");
    write_fabrication_project(&hostile, "10 10", true);
    let hostile_report = fabrication_review(&hostile, &temp.join("hostile.json"));
    assert_ne!(hostile_report["fabrication"]["status"], "complete");
    assert!(
        hostile_report["fabrication"]["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning["code"] == "manufacturing-semantic-parse-failed")
    );
    assert_eq!(hostile_report["approvalEligible"], false);
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
    assert_eq!(
        doctor["capabilities"]["manufacturing"]["gerberX2"]["semantic"],
        true
    );
    assert_eq!(
        doctor["capabilities"]["manufacturing"]["gerberJob"]["subset"],
        "2023.06"
    );
    assert_eq!(
        doctor["capabilities"]["manufacturing"]["xnc"]["profiles"],
        json!([
            "strict-xnc",
            "kicad-legacy-excellon",
            "librepcb-legacy-excellon"
        ])
    );
    assert_eq!(
        doctor["capabilities"]["manufacturing"]["browserGerberEvidence"],
        false
    );
    assert_eq!(
        doctor["capabilities"]["manufacturing"]["unsupportedFormats"],
        json!(["ODB++", "IPC-2581"])
    );

    let help = cli(&["--help"]);
    assert_success(&help);
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(help.contains("--schematic PATH"));
    assert!(help.contains("resolves only ambiguous automatic roots"));
    for value in [
        "Gerber/X2",
        "strict XNC",
        "ODB++",
        "IPC-2581",
        "presentation-only",
    ] {
        assert!(help.contains(value));
    }

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
