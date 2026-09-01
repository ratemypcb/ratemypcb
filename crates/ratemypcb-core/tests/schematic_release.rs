use ratemypcb_core::{
    GateImpact, NativeMode, Preset, ReviewOptions, ReviewScope, review, validate_report,
};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use zip::write::SimpleFileOptions;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

fn fixtures(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(path)
}

fn options() -> ReviewOptions {
    ReviewOptions {
        board: None,
        schematic: None,
        bom: None,
        placement: None,
        supply_snapshot: None,
        dfm_declarations: None,
        preset: Preset::named("standard").unwrap(),
        native: NativeMode::Off,
        tool_version: "schematic-release-test".into(),
        scope: ReviewScope::Full,
        profile: None,
    }
}

#[test]
fn hierarchy_adversarial_states_are_typed() {
    for (directory, expected) in [
        ("missing-child", "missing-child"),
        ("unresolved-variable", "unresolved-variable"),
        ("ambiguous-roots", "ambiguous"),
        ("broken-instance-path", "broken-instance-path"),
        ("duplicate-instance-path", "duplicate-instance-path"),
        ("cycle", "cycle"),
    ] {
        let report = review(
            &fixtures(&format!("kicad/hierarchy/{directory}")),
            options(),
        )
        .unwrap();
        assert!(
            report
                .schematic
                .capabilities
                .iter()
                .any(|capability| capability.status == expected),
            "{directory}: {:?}",
            report
                .schematic
                .capabilities
                .iter()
                .map(|capability| &capability.status)
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn hierarchy_selector_only_resolves_bounded_automatic_root_ambiguity() {
    let mut selected = options();
    selected.schematic = Some("beta.kicad_sch".into());
    let report = review(&fixtures("kicad/hierarchy/ambiguous-roots"), selected).unwrap();
    assert_eq!(
        report.schematic.root_path.as_deref(),
        Some("beta.kicad_sch")
    );

    for selector in ["../beta.kicad_sch", "/beta.kicad_sch", "missing.kicad_sch"] {
        let mut invalid = options();
        invalid.schematic = Some(selector.into());
        assert!(review(&fixtures("kicad/hierarchy/ambiguous-roots"), invalid).is_err());
    }

    let mut unnecessary = options();
    unnecessary.schematic = Some("root.kicad_sch".into());
    assert!(review(&fixtures("kicad/hierarchy/reused-child"), unnecessary).is_err());

    let duplicate = std::env::temp_dir().join(format!(
        "ratemypcb-schematic-selector-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(duplicate.join("a")).unwrap();
    fs::create_dir_all(duplicate.join("b")).unwrap();
    fs::write(
        duplicate.join("a/root.kicad_sch"),
        "(kicad_sch (version 20250114) (uuid \"aaaaaaaa-0000-4000-8000-000000000001\"))",
    )
    .unwrap();
    fs::write(
        duplicate.join("b/root.kicad_sch"),
        "(kicad_sch (version 20250114) (uuid \"bbbbbbbb-0000-4000-8000-000000000001\"))",
    )
    .unwrap();
    let mut duplicate_name = options();
    duplicate_name.schematic = Some("root.kicad_sch".into());
    assert!(review(&duplicate, duplicate_name).is_err());
    fs::remove_dir_all(duplicate).unwrap();
}

#[test]
fn selected_board_pairs_only_with_colocated_schematic_and_project() {
    let root = std::env::temp_dir().join(format!(
        "ratemypcb-schematic-coherence-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    for directory in ["a", "z"] {
        fs::create_dir_all(root.join(directory)).unwrap();
    }
    fs::write(
        root.join("a/root.kicad_sch"),
        "(kicad_sch (version 20250114) (uuid \"aaaaaaaa-0000-4000-8000-000000000001\"))",
    )
    .unwrap();
    fs::write(
        root.join("z/root.kicad_sch"),
        "(kicad_sch (version 20250114) (uuid \"zzzzzzzz-0000-4000-8000-000000000001\"))",
    )
    .unwrap();
    fs::write(root.join("z/root.kicad_pro"), "{}").unwrap();
    fs::write(
        root.join("z/root.kicad_pcb"),
        "(kicad_pcb (version 20240108) (layers (0 \"F.Cu\" signal) (31 \"B.Cu\" signal) (44 \"Edge.Cuts\" user)))",
    )
    .unwrap();
    let report = review(&root, options()).unwrap();
    assert_eq!(
        report.schematic.root_path.as_deref(),
        Some("z/root.kicad_sch")
    );
    assert_eq!(
        report.schematic.project_identity.as_deref(),
        Some("z/root.kicad_pro")
    );
    assert_eq!(
        report.schematic.source_pair.as_ref().unwrap().board_path,
        "z/root.kicad_pcb"
    );

    fs::remove_file(root.join("z/root.kicad_sch")).unwrap();
    let incoherent = review(&root, options()).unwrap();
    assert_eq!(incoherent.schematic.status, "incoherent_project");
    assert!(incoherent.schematic.root_path.is_none());
    assert!(incoherent.schematic.source_pair.is_none());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn hierarchy_project_variables_resolve_only_explicit_bounded_child_paths() {
    let report = review(&fixtures("kicad/hierarchy/resolved-variable"), options()).unwrap();
    assert_eq!(report.schematic.status, "completed");
    assert_eq!(report.schematic.occurrences.len(), 1);
    assert_eq!(
        report.schematic.occurrences[0].source_path,
        "channel/child.kicad_sch"
    );
}

#[test]
fn hierarchy_reused_child_occurrences_keep_uuid_paths_distinct() {
    let report = review(&fixtures("kicad/hierarchy/reused-child"), options()).unwrap();
    assert_eq!(report.schematic.occurrences.len(), 2);
    let first = &report.schematic.occurrences[0];
    let second = &report.schematic.occurrences[1];
    assert_eq!(first.item_uuid, second.item_uuid);
    assert_eq!(first.source_path, second.source_path);
    assert_ne!(first.sheet_uuid_path, second.sheet_uuid_path);
    assert_ne!(first.key, second.key);
    assert!(first.key.len() == 64 && second.key.len() == 64);
}

#[test]
fn hierarchy_standalone_root_loads_bounded_relative_children_without_parity() {
    let root = fixtures("kicad/hierarchy/reused-child/root.kicad_sch");
    let report = review(&root, options()).unwrap();
    assert_eq!(
        report.schematic.root_path.as_deref(),
        Some("root.kicad_sch")
    );
    assert_eq!(report.schematic.occurrences.len(), 2);
    assert_eq!(
        report.schematic.native_parity.as_ref().unwrap().status,
        "not_run"
    );
}

#[test]
fn hierarchy_root_digest_and_project_are_part_of_occurrence_identity() {
    let report = review(&fixtures("kicad/mismatch"), options()).unwrap();
    let occurrence = &report.schematic.occurrences[0];
    assert_eq!(occurrence.project_identity, "root.kicad_pro");
    assert_eq!(
        occurrence.root_digest,
        report.schematic.root_digest.as_deref().unwrap()
    );
    assert!(occurrence.sheet_uuid_path.contains("aaaaaaaa"));
    assert!(occurrence.key.len() == 64);
}

fn copy_mismatch_fixture() -> PathBuf {
    let destination = std::env::temp_dir().join(format!(
        "ratemypcb-schematic-release-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&destination).unwrap();
    for name in [
        "root.kicad_pro",
        "root.kicad_sch",
        "root.kicad_pcb",
        "root.net",
        "root-bom.csv",
        "root-positions.csv",
    ] {
        fs::copy(
            fixtures("kicad/mismatch").join(name),
            destination.join(name),
        )
        .unwrap();
    }
    destination
}

fn replace(path: &Path, from: &str, to: &str) {
    let source = fs::read_to_string(path).unwrap();
    assert!(
        source.contains(from),
        "{} did not contain {from}",
        path.display()
    );
    fs::write(path, source.replacen(from, to, 1)).unwrap();
}

#[test]
fn reconciliation_reused_occurrences_join_distinct_full_board_paths() {
    let root = std::env::temp_dir().join(format!(
        "ratemypcb-reused-board-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
    fs::write(root.join("root.kicad_pro"), "{}").unwrap();
    fs::write(
        root.join("root.kicad_sch"),
        "(kicad_sch (version 20250114) (uuid \"root-uuid\") (sheet (uuid \"sheet-a\") (property \"Sheetfile\" \"child.kicad_sch\") (instances (project \"root\" (path \"/root-uuid/sheet-a\")))) (sheet (uuid \"sheet-b\") (property \"Sheetfile\" \"child.kicad_sch\") (instances (project \"root\" (path \"/root-uuid/sheet-b\")))))",
    )
    .unwrap();
    fs::write(
        root.join("child.kicad_sch"),
        "(kicad_sch (version 20250114) (uuid \"child-uuid\") (symbol (uuid \"item-x\") (property \"Reference\" \"R?\") (property \"Value\" \"10k\") (property \"Footprint\" \"Resistor:R_0603\") (on_board yes)) (symbol_instances (path \"/root-uuid/sheet-a/item-x\" (reference \"R1\") (unit 1)) (path \"/root-uuid/sheet-b/item-x\" (reference \"R2\") (unit 1))))",
    )
    .unwrap();
    fs::write(
        root.join("root.kicad_pcb"),
        "(kicad_pcb (version 20240108) (layers (0 \"F.Cu\" signal) (31 \"B.Cu\" signal) (44 \"Edge.Cuts\" user)) (footprint \"Resistor:R_0603\" (path \"/root-uuid/sheet-a/item-x\") (uuid \"footprint-a\") (property \"Reference\" \"R1\") (property \"Value\" \"10k\")) (footprint \"Resistor:R_0603\" (path \"/root-uuid/sheet-b/item-x\") (uuid \"footprint-b\") (property \"Reference\" \"R2\") (property \"Value\" \"20k\")))",
    )
    .unwrap();
    let report = review(&root, options()).unwrap();
    assert_eq!(report.schematic.occurrences.len(), 2);
    assert_eq!(
        report.schematic.occurrences[0].reference.as_deref(),
        Some("R1")
    );
    assert_eq!(
        report.schematic.occurrences[1].reference.as_deref(),
        Some("R2")
    );
    let mismatches = &report.schematic.mismatches;
    assert_eq!(mismatches.len(), 1, "{mismatches:?}");
    assert_eq!(mismatches[0].field, "value");
    assert!(mismatches[0].location.contains("sheet-b"));
    assert_eq!(mismatches[0].join, "occurrence-uuid");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn reconciliation_clean_control_uses_exact_occurrence_identity() {
    let report = review(&fixtures("kicad/mismatch"), options()).unwrap();
    assert!(
        report.schematic.mismatches.is_empty(),
        "{:?}",
        report
            .schematic
            .mismatches
            .iter()
            .map(|item| &item.check_id)
            .collect::<Vec<_>>()
    );
    assert!(
        report
            .required_evidence
            .iter()
            .all(|item| !item.check_id.starts_with("schematic"))
    );
    assert!(report.schematic.capabilities.iter().any(|capability| {
        capability.id == "schematic-reconciliation"
            && capability
                .detail
                .contains("reference fallback is explicitly weaker")
    }));
    let pair = report.schematic.source_pair.as_ref().unwrap();
    assert_eq!(pair.project_identity, "root.kicad_pro");
    assert_eq!(pair.schematic_path, "root.kicad_sch");
    assert_eq!(pair.board_path, "root.kicad_pcb");
    assert_eq!(
        pair.schematic_digest,
        report.schematic.root_digest.as_deref().unwrap()
    );
    assert_eq!(
        pair.board_digest,
        report.schematic.board_digest.as_deref().unwrap()
    );
    validate_report(&report).unwrap();
}

#[test]
fn schematic_runtime_validation_matches_lowercase_digest_schema_and_provenance() {
    let baseline = review(&fixtures("kicad/mismatch"), options()).unwrap();

    let mut uppercase_artifact = baseline.clone();
    let digest = uppercase_artifact
        .schematic
        .artifact_digests
        .values_mut()
        .next()
        .unwrap();
    *digest = digest.to_ascii_uppercase();
    assert!(validate_report(&uppercase_artifact).is_err());

    let mut uppercase_key = baseline.clone();
    uppercase_key.schematic.occurrences[0].key = uppercase_key.schematic.occurrences[0]
        .key
        .to_ascii_uppercase();
    assert!(validate_report(&uppercase_key).is_err());

    let mut missing_composite = baseline.clone();
    missing_composite
        .schematic
        .artifact_digests
        .remove("schematic:composite");
    assert!(validate_report(&missing_composite).is_err());

    let mut absolute_input = baseline.clone();
    absolute_input.input.path = "/private/project/root".into();
    assert!(validate_report(&absolute_input).is_err());

    let mut wrong_provenance = baseline;
    let evidence = wrong_provenance
        .evidence
        .iter_mut()
        .find(|record| record.check_id == "schematic-evidence")
        .unwrap();
    let old_id = evidence.id.clone();
    evidence.provenance.artifact_digest = "0".repeat(64);
    let canonical = serde_json::to_vec(&(
        &evidence.provenance.artifact_digest,
        &evidence.check_id,
        &evidence.provenance.location,
    ))
    .unwrap();
    use sha2::Digest;
    evidence.id = format!("ev-{:x}", sha2::Sha256::digest(canonical));
    let new_id = evidence.id.clone();
    wrong_provenance
        .coverage
        .iter_mut()
        .find(|coverage| coverage.id == old_id)
        .unwrap()
        .id = new_id;
    assert!(validate_report(&wrong_provenance).is_err());
}

#[test]
fn reconciliation_gate_mutation_cannot_create_a_blocking_schematic_contract() {
    let root = copy_mismatch_fixture();
    replace(
        &root.join("root.kicad_pcb"),
        "(property \"Value\" \"10k\")",
        "(property \"Value\" \"20k\")",
    );
    let mut report = review(&root, options()).unwrap();
    assert!(!report.schematic.mismatches.is_empty());
    report.schematic.mismatches[0].gate_impact = GateImpact::Blocking;
    assert!(validate_report(&report).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn reconciliation_mutation_matrix_is_occurrence_linked_and_evidence_only() {
    for (mutation, file, from, to, expected) in [
        (
            "reference-uuid",
            "root.kicad_pcb",
            "bbbbbbbb-0000-4000-8000-000000000001",
            "cccccccc-0000-4000-8000-000000000001",
            "schematic-reconcile-uuid",
        ),
        (
            "value",
            "root.kicad_pcb",
            "(property \"Value\" \"10k\")",
            "(property \"Value\" \"20k\")",
            "schematic-reconcile-value",
        ),
        (
            "footprint",
            "root.kicad_pcb",
            "(footprint \"Resistor:R_0603\"",
            "(footprint \"Resistor:R_0805\"",
            "schematic-reconcile-footprint",
        ),
        (
            "fitted-dnp",
            "root.kicad_pcb",
            "(property \"Reference\" \"R1\")",
            "(attr dnp) (property \"Reference\" \"R1\")",
            "schematic-reconcile-dnp",
        ),
        (
            "pin-pad",
            "root.kicad_pcb",
            "(pad \"1\"",
            "(pad \"2\"",
            "schematic-reconcile-pin-pad",
        ),
        (
            "net",
            "root.kicad_pcb",
            "(net 1 \"GND\"))) (segment",
            "(net 1 \"VCC\"))) (segment",
            "schematic-reconcile-net",
        ),
        (
            "quantity",
            "root-bom.csv",
            "R1,1,",
            "R1,2,",
            "schematic-reconcile-bom-quantity",
        ),
        (
            "placement",
            "root-positions.csv",
            "R1,10k,R_0603,1,1,0,top,A",
            "",
            "schematic-reconcile-placement-population",
        ),
        (
            "revision",
            "root-positions.csv",
            ",top,A",
            ",top,B",
            "schematic-reconcile-revision",
        ),
    ] {
        let root = copy_mismatch_fixture();
        replace(&root.join(file), from, to);
        let report = review(&root, options()).unwrap();
        let mismatch = report
            .schematic
            .mismatches
            .iter()
            .find(|mismatch| mismatch.check_id == expected)
            .unwrap_or_else(|| {
                panic!(
                    "{mutation}: {:?}",
                    report
                        .schematic
                        .mismatches
                        .iter()
                        .map(|item| &item.check_id)
                        .collect::<Vec<_>>()
                )
            });
        assert!(mismatch.location.contains("sheet=") && mismatch.location.contains("item="));
        assert_eq!(mismatch.gate_impact, GateImpact::EvidenceOnly);
        let finding = report.findings.iter().find(|finding| {
            finding.gate_impact == GateImpact::EvidenceOnly && finding.location == mismatch.location
        });
        assert!(finding.is_some(), "{mutation}");
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn reconciliation_reference_fallback_is_unique_only_and_labeled_weak() {
    let root = copy_mismatch_fixture();
    replace(
        &root.join("root.kicad_pcb"),
        "bbbbbbbb-0000-4000-8000-000000000001",
        "cccccccc-0000-4000-8000-000000000001",
    );
    let report = review(&root, options()).unwrap();
    let mismatch = report
        .schematic
        .mismatches
        .iter()
        .find(|mismatch| mismatch.check_id == "schematic-reconcile-uuid")
        .unwrap();
    assert_eq!(mismatch.join, "reference-fallback");
    assert_eq!(mismatch.confidence, "low");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn bounded_eda_capabilities_do_not_inflate_native_or_source_aware_claims() {
    let report = review(&fixtures("bounded-eda"), options()).unwrap();
    assert!(report.schematic.capabilities.iter().any(|capability| {
        capability.id == "altium-schdoc" && capability.status == "inventory_only"
    }));
    assert!(report.schematic.capabilities.iter().any(|capability| {
        capability.id == "generic-netlist" && capability.status == "explicit_fields_only"
    }));
    assert!(report.schematic.capabilities.iter().any(|capability| {
        capability.id == "generic-netlist" && capability.status == "unsupported"
    }));
    assert!(report.schematic.native_erc.is_none());
    assert!(report.schematic.native_parity.is_none());
    assert!(report.schematic.occurrences.is_empty());
}

#[test]
fn bounded_eda_zip_hierarchy_is_inventory_only_for_native_execution() {
    let path = std::env::temp_dir().join(format!(
        "ratemypcb-schematic-zip-{}-{}.zip",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let file = fs::File::create(&path).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    archive
        .start_file("root.kicad_sch", SimpleFileOptions::default())
        .unwrap();
    archive
        .write_all(
            b"(kicad_sch (version 20250114) (uuid \"dddddddd-0000-4000-8000-000000000001\"))",
        )
        .unwrap();
    archive.finish().unwrap();
    let report = review(&path, options()).unwrap();
    assert_eq!(
        report.schematic.root_path.as_deref(),
        Some("root.kicad_sch")
    );
    assert_eq!(
        report.schematic.native_erc.as_ref().unwrap().status,
        "not_run"
    );
    assert!(
        report
            .schematic
            .native_erc
            .as_ref()
            .unwrap()
            .note
            .contains("never staged")
    );
    assert_eq!(
        report.schematic.native_parity.as_ref().unwrap().status,
        "not_run"
    );
    fs::remove_file(path).unwrap();
}
