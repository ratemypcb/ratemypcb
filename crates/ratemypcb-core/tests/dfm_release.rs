use ratemypcb_core::fabrication::{Authority, ConstraintKind, Picometres};
use ratemypcb_core::{
    Assessment, AssessmentAction, Coverage, CoverageStatus, DfmDeclarations, EvidenceConfidence,
    EvidenceExecution, EvidenceFreshness, EvidenceResult, Finding, GateImpact, NativeMode, Preset,
    Report, ReviewOptions, ReviewScope, Severity, review, validate_assessment, validate_report,
};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MANIFEST_JSON: &str = include_str!("../../../tests/fixtures/dfm/manifest.json");
const POPULATION_TARGETS_JSON: &str =
    include_str!("../../../tests/fixtures/dfm/population-targets.json");
const PREREQUISITE_MUTATIONS_JSON: &str =
    include_str!("../../../tests/fixtures/dfm/prerequisite-mutations.json");
const GEOMETRY_TARGETS_JSON: &str =
    include_str!("../../../tests/fixtures/dfm/geometry-targets.json");
const CONSTRUCTION_TARGETS_JSON: &str =
    include_str!("../../../tests/fixtures/dfm/construction-targets.json");
const ASSEMBLY_TARGETS_JSON: &str =
    include_str!("../../../tests/fixtures/dfm/assembly-targets.json");

const TARGET_FIXTURE_DIGEST: &str =
    "985e199bdaf7fd8a59c1a6ca7f63937e5d6f772794e0d597a0ab631edad674f9";
const MUTATION_FIXTURE_DIGEST: &str =
    "145cb25ffba66e0fa2967a775b969ff32c83c23c7fcfa88db09670e01f621cc3";
const POPULATION_FAMILY: &str = "assembly.population-parity.v1";
static NEXT_POPULATION_FIXTURE: AtomicU64 = AtomicU64::new(0);

const CANONICAL_AUTHORITY: &[&str] = &["canonical_model"];
const THRESHOLD_AUTHORITY: &[&str] = &["canonical_model", "source_version_bound_threshold"];
const ORDER_PROFILE_AUTHORITY: &[&str] = &["canonical_model", "source_version_bound_order_profile"];
const SCHEMATIC_AUTHORITY: &[&str] = &["typed_schematic_reconciliation"];
const ASSEMBLY_AUTHORITY: &[&str] = &["source_linked_assembly_facts"];
const CANONICAL_ASSEMBLY_AUTHORITY: &[&str] = &["canonical_model", "source_linked_assembly_facts"];
const NATIVE_DRC_AUTHORITY: &[&str] = &["completed_native_drc"];
const INTENT_AUTHORITY: &[&str] = &["canonical_model", "source_linked_intent_declaration"];
type FamilyContract = (
    &'static str,
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
    &'static [&'static str],
);
const FAMILY_CONTRACTS: &[FamilyContract] = &[
    (
        "dfm.outline-topology.v1",
        "DFM-01",
        &[
            "layer_roles",
            "profile",
            "geometry_lines",
            "geometry_arcs",
            "geometry_regions",
            "geometry_expanded",
            "transforms",
            "polarity",
        ],
        &[],
        CANONICAL_AUTHORITY,
    ),
    (
        "dfm.minimum-finished-drill.v1",
        "DFM-01",
        &["units_and_format", "tools", "drills", "constraints"],
        &[],
        THRESHOLD_AUTHORITY,
    ),
    (
        "dfm.copper-clearance.v1",
        "DFM-01",
        &[
            "layer_roles",
            "geometry_lines",
            "geometry_arcs",
            "geometry_regions",
            "geometry_flashes",
            "geometry_expanded",
            "transforms",
            "polarity",
            "connectivity",
            "constraints",
        ],
        &[],
        THRESHOLD_AUTHORITY,
    ),
    (
        "dfm.annular-ring.v1",
        "DFM-01",
        &[
            "layer_roles",
            "layer_order",
            "geometry_flashes",
            "geometry_expanded",
            "transforms",
            "polarity",
            "tools",
            "drills",
            "plating",
            "layer_spans",
            "constraints",
        ],
        &["pad_hole_association"],
        THRESHOLD_AUTHORITY,
    ),
    (
        "dfm.copper-edge.v1",
        "DFM-01",
        &[
            "layer_roles",
            "profile",
            "geometry_lines",
            "geometry_arcs",
            "geometry_regions",
            "geometry_flashes",
            "geometry_expanded",
            "transforms",
            "polarity",
            "constraints",
        ],
        &[],
        THRESHOLD_AUTHORITY,
    ),
    (
        "dfm.mask-sliver.v1",
        "DFM-01",
        &[
            "layer_roles",
            "geometry_lines",
            "geometry_arcs",
            "geometry_regions",
            "geometry_flashes",
            "geometry_expanded",
            "transforms",
            "polarity",
            "constraints",
        ],
        &["mask_opening_intent"],
        THRESHOLD_AUTHORITY,
    ),
    (
        "dfm.paste-mask-relationship.v1",
        "DFM-01",
        &[
            "layer_roles",
            "apertures",
            "x2_aperture_attributes",
            "geometry_lines",
            "geometry_arcs",
            "geometry_regions",
            "geometry_flashes",
            "geometry_expanded",
            "transforms",
            "polarity",
            "components",
            "pins",
            "assembly",
            "constraints",
        ],
        &["fitted_state", "smd_pad_authority"],
        THRESHOLD_AUTHORITY,
    ),
    (
        "dfm.drill-tool-integrity.v1",
        "DFM-01",
        &["units_and_format", "tools"],
        &["relevant_drill_route_slot_capabilities"],
        CANONICAL_AUTHORITY,
    ),
    (
        "dfm.stackup-order-confirmation.v1",
        "DFM-02",
        &["construction", "layer_order"],
        &["order_profile_authority"],
        ORDER_PROFILE_AUTHORITY,
    ),
    (
        "dfm.total-thickness-material.v1",
        "DFM-02",
        &["construction", "constraints"],
        &["order_profile_authority"],
        ORDER_PROFILE_AUTHORITY,
    ),
    (
        "dfm.drill-span-plating.v1",
        "DFM-02",
        &["tools", "drills", "plating", "layer_spans", "layer_order"],
        &["order_profile_authority"],
        ORDER_PROFILE_AUTHORITY,
    ),
    (
        "dfm.finish-profile.v1",
        "DFM-02",
        &["construction", "profile", "constraints"],
        &["order_profile_authority"],
        ORDER_PROFILE_AUTHORITY,
    ),
    (
        "dfm.impedance-special-process.v1",
        "DFM-02",
        &["constraints", "construction"],
        &["order_profile_authority"],
        ORDER_PROFILE_AUTHORITY,
    ),
    (
        "assembly.population-parity.v1",
        "DFM-03",
        &[],
        &[
            "schematic_reconciliation_complete",
            "schematic_artifact_identity",
        ],
        SCHEMATIC_AUTHORITY,
    ),
    (
        "assembly.side-rotation.v1",
        "DFM-03",
        &["assembly", "native_kicad_facts"],
        &["placement_convention"],
        ASSEMBLY_AUTHORITY,
    ),
    (
        "assembly.paste-availability.v1",
        "DFM-03",
        &[
            "assembly",
            "components",
            "pins",
            "layer_roles",
            "apertures",
            "x2_aperture_attributes",
            "geometry_flashes",
            "geometry_expanded",
            "transforms",
            "polarity",
        ],
        &["fitted_state", "paste_requiring_pad_authority"],
        CANONICAL_ASSEMBLY_AUTHORITY,
    ),
    (
        "assembly.courtyard-native.v1",
        "DFM-03",
        &["native_kicad_facts"],
        &["native_drc_complete"],
        NATIVE_DRC_AUTHORITY,
    ),
    (
        "assembly.footprint-string-parity.v1",
        "DFM-03",
        &["components"],
        &[
            "schematic_reconciliation_complete",
            "per_source_footprint_comparisons",
        ],
        SCHEMATIC_AUTHORITY,
    ),
    (
        "assembly.access.v1",
        "DFM-03",
        &[
            "assembly",
            "profile",
            "components",
            "layer_roles",
            "apertures",
            "geometry_flashes",
            "geometry_regions",
            "geometry_expanded",
            "transforms",
            "polarity",
        ],
        &[
            "assembly_process_envelope",
            "complete_component_geometry",
            "complete_placement_geometry",
            "complete_profile_geometry",
        ],
        INTENT_AUTHORITY,
    ),
    (
        "assembly.testpoint-access.v1",
        "DFM-03",
        &[
            "connectivity",
            "components",
            "pins",
            "assembly",
            "profile",
            "layer_roles",
            "apertures",
            "geometry_flashes",
            "geometry_regions",
            "geometry_expanded",
            "transforms",
            "polarity",
        ],
        &[
            "probe_envelope",
            "target_net_authority",
            "complete_connectivity_geometry",
            "complete_component_geometry",
            "complete_pin_geometry",
            "complete_placement_geometry",
            "complete_profile_geometry",
        ],
        INTENT_AUTHORITY,
    ),
    (
        "inference.return-path.v1",
        "DFM-04",
        &[
            "layer_order",
            "connectivity",
            "geometry_lines",
            "geometry_arcs",
        ],
        &["signal_intent", "reference_plane_intent"],
        INTENT_AUTHORITY,
    ),
    (
        "inference.high-current.v1",
        "DFM-04",
        &[
            "geometry_lines",
            "geometry_arcs",
            "construction",
            "constraints",
        ],
        &["current_intent", "process_envelope"],
        INTENT_AUTHORITY,
    ),
    (
        "inference.creepage.v1",
        "DFM-04",
        &[
            "connectivity",
            "profile",
            "geometry_lines",
            "geometry_arcs",
            "geometry_regions",
        ],
        &["voltage_domains", "creepage_rule", "material_environment"],
        INTENT_AUTHORITY,
    ),
    (
        "inference.differential.v1",
        "DFM-04",
        &[
            "connectivity",
            "layer_order",
            "geometry_lines",
            "geometry_arcs",
            "construction",
        ],
        &["differential_pair_intent", "impedance_skew_target"],
        INTENT_AUTHORITY,
    ),
    (
        "inference.thermal.v1",
        "DFM-04",
        &[
            "construction",
            "geometry_lines",
            "geometry_regions",
            "drills",
        ],
        &["power_intent", "thermal_boundary_conditions"],
        INTENT_AUTHORITY,
    ),
    (
        "inference.interface.v1",
        "DFM-04",
        &["connectivity", "components", "pins", "constraints"],
        &["interface_intent"],
        INTENT_AUTHORITY,
    ),
];
const INFERENCE_FAMILIES: &[&str] = &[
    "assembly.access.v1",
    "assembly.testpoint-access.v1",
    "inference.creepage.v1",
    "inference.differential.v1",
    "inference.high-current.v1",
    "inference.interface.v1",
    "inference.return-path.v1",
    "inference.thermal.v1",
];
const VALID_CAPABILITIES: &[&str] = &[
    "apertures",
    "assembly",
    "components",
    "connectivity",
    "constraints",
    "construction",
    "document_syntax",
    "drills",
    "extents",
    "geometry_arcs",
    "geometry_expanded",
    "geometry_flashes",
    "geometry_lines",
    "geometry_points",
    "geometry_regions",
    "layer_order",
    "layer_roles",
    "layer_spans",
    "macros",
    "native_kicad_facts",
    "package_completeness",
    "package_reconciliation",
    "pins",
    "plating",
    "polarity",
    "product_identity",
    "profile",
    "repetition",
    "routes",
    "slots",
    "tools",
    "transforms",
    "units_and_format",
    "x2_aperture_attributes",
    "x2_file_attributes",
    "x2_object_attributes",
];
const VALID_FACTS: &[&str] = &[
    "assembly_process_envelope",
    "complete_component_geometry",
    "complete_connectivity_geometry",
    "complete_pin_geometry",
    "complete_placement_geometry",
    "complete_profile_geometry",
    "creepage_rule",
    "current_intent",
    "differential_pair_intent",
    "fitted_state",
    "impedance_skew_target",
    "interface_intent",
    "mask_opening_intent",
    "material_environment",
    "native_drc_complete",
    "order_profile_authority",
    "pad_hole_association",
    "paste_requiring_pad_authority",
    "per_source_footprint_comparisons",
    "placement_convention",
    "power_intent",
    "probe_envelope",
    "process_envelope",
    "reference_plane_intent",
    "relevant_drill_route_slot_capabilities",
    "schematic_artifact_identity",
    "schematic_reconciliation_complete",
    "signal_intent",
    "smd_pad_authority",
    "target_net_authority",
    "thermal_boundary_conditions",
    "voltage_domains",
];
const VALID_AUTHORITIES: &[&str] = &[
    "canonical_model",
    "completed_native_drc",
    "source_linked_assembly_facts",
    "source_linked_intent_declaration",
    "source_version_bound_order_profile",
    "source_version_bound_threshold",
    "typed_schematic_reconciliation",
];
const REQUIRED_MUTATIONS: &[&str] = &[
    "affected_conflict",
    "affected_omission",
    "dangling_identity",
    "duplicate_capability",
    "missing_prerequisite",
    "reordered_facts",
    "resolution_changed",
    "state_failed",
    "state_not_provided",
    "state_omitted",
    "state_partial",
    "state_stale",
    "state_unsupported",
    "unit_changed",
];

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    origin: String,
    license: String,
    target_key_fields: Vec<String>,
    required_fixture_classes: Vec<String>,
    qualification_policy: QualificationPolicy,
    forbidden_sources: Vec<ForbiddenSource>,
    families: Vec<Family>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct QualificationPolicy {
    default_gate_impact: String,
    minimum_blocking_precision_bps: u16,
    recall_threshold: Option<u16>,
    mutation_status: String,
    unsupported_status: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ForbiddenSource {
    id: String,
    claim: String,
    action: String,
    reason: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Family {
    family_id: String,
    family_version: String,
    requirement: String,
    family_class: String,
    capability_prerequisites: Vec<String>,
    fact_prerequisites: Vec<String>,
    authority: Vec<String>,
    promotion_state: String,
    human_approval_required: bool,
    corpus_ref: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GeometryCorpus {
    schema_version: u32,
    origin: String,
    license: String,
    families: Vec<GeometryFamilyTargets>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GeometryFamilyTargets {
    family_id: String,
    family_version: String,
    fixture_digest: String,
    targets: Vec<Target>,
    unsupported_targets: Vec<UnsupportedTarget>,
    mutations: Vec<Mutation>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PopulationTargets {
    schema_version: u32,
    origin: String,
    family_id: String,
    family_version: String,
    fixture_digest: String,
    targets: Vec<Target>,
    unsupported_targets: Vec<UnsupportedTarget>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Target {
    case_id: String,
    case_class: String,
    canonical_target_ids: Vec<String>,
    expected_label: String,
    #[serde(default)]
    actual_label: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UnsupportedTarget {
    case_id: String,
    canonical_target_ids: Vec<String>,
    expected_status: String,
    actual_status: String,
    reason: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MutationCorpus {
    schema_version: u32,
    origin: String,
    family_id: String,
    family_version: String,
    fixture_digest: String,
    mutations: Vec<Mutation>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Mutation {
    id: String,
    kind: String,
    expected_status: String,
    contributes_to_confusion_matrix: bool,
    detail: String,
}

#[derive(Clone, Debug)]
struct MutationMeasurement {
    case_id: String,
    kind: String,
    input_changed: bool,
    status: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Metrics {
    tp: usize,
    fp: usize,
    fn_count: usize,
    tn: usize,
    executable_targets: usize,
    not_checked_mutations: usize,
    unsupported_targets: usize,
    precision: Option<f64>,
    recall: Option<f64>,
}

fn family_key(family_id: &str, family_version: &str) -> String {
    format!("{family_id}.{family_version}")
}

fn require_unique_nonempty(values: &[String], label: &str) -> Result<(), String> {
    if values.is_empty() {
        return Err(format!("{label} must not be empty"));
    }
    let unique = values.iter().collect::<BTreeSet<_>>();
    if unique.len() != values.len() || values.iter().any(|value| value.trim().is_empty()) {
        return Err(format!("{label} must contain unique nonempty values"));
    }
    Ok(())
}

fn is_exact_set(values: &[String], expected: &[&str]) -> bool {
    values.len() == expected.len()
        && values.iter().map(String::as_str).collect::<BTreeSet<_>>()
            == expected.iter().copied().collect()
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn validate_manifest(manifest: &Manifest) -> Result<(), String> {
    if manifest.schema_version != 1
        || manifest.origin != "project-authored"
        || manifest.license != "MIT OR Apache-2.0"
    {
        return Err("manifest metadata is not the locked project-authored contract".into());
    }
    if manifest.target_key_fields
        != [
            "familyId",
            "familyVersion",
            "fixtureDigest",
            "canonicalTargetIds",
        ]
    {
        return Err("target-key fields changed".into());
    }
    if manifest.required_fixture_classes != ["positive", "hard_negative", "mutation"] {
        return Err("positive/hard-negative/mutation classes are required".into());
    }
    let policy = &manifest.qualification_policy;
    if policy.default_gate_impact != "evidence_only"
        || policy.minimum_blocking_precision_bps != 9500
        || policy.recall_threshold.is_some()
        || policy.mutation_status != "not_checked"
        || policy.unsupported_status != "not_checked"
    {
        return Err("qualification policy changed or invented a recall threshold".into());
    }

    let expected_forbidden = BTreeMap::from([
        (
            "common_practice_construction_default",
            "construction_or_order_compliance",
        ),
        (
            "distributor_order_packaging",
            "physical_package_compatibility",
        ),
        ("net_name_intent", "electrical_or_interface_intent"),
        ("paste_layer_presence", "component_paste_coverage"),
    ]);
    let mut forbidden = BTreeMap::new();
    for source in &manifest.forbidden_sources {
        if source.action != "reject" || source.reason.trim().is_empty() {
            return Err(format!(
                "forbidden source {} is not explicitly rejected",
                source.id
            ));
        }
        if forbidden
            .insert(source.id.as_str(), source.claim.as_str())
            .is_some()
        {
            return Err(format!("duplicate forbidden source {}", source.id));
        }
    }
    if forbidden != expected_forbidden {
        return Err("forbidden-source rejection set changed".into());
    }

    let expected_families = FAMILY_CONTRACTS
        .iter()
        .map(|contract| contract.0.to_owned())
        .collect::<BTreeSet<_>>();
    let mut observed_families = BTreeSet::new();
    for family in &manifest.families {
        let key = family_key(&family.family_id, &family.family_version);
        if !observed_families.insert(key.clone()) {
            return Err(format!("duplicate family/version {key}"));
        }
        let expected = FAMILY_CONTRACTS
            .iter()
            .find(|contract| contract.0 == key)
            .ok_or_else(|| format!("unknown or missing family/version {key}"))?;
        if family.requirement != expected.1 {
            return Err(format!("{key} mapped to the wrong requirement"));
        }
        let inference = INFERENCE_FAMILIES.contains(&key.as_str());
        let expected_class = if inference {
            "inference"
        } else {
            "deterministic"
        };
        let expected_corpus = match key.as_str() {
            "dfm.minimum-finished-drill.v1"
            | "dfm.drill-tool-integrity.v1"
            | "dfm.outline-topology.v1"
            | "dfm.copper-edge.v1"
            | "dfm.copper-clearance.v1"
            | "dfm.annular-ring.v1"
            | "dfm.mask-sliver.v1"
            | "dfm.paste-mask-relationship.v1" => Some(format!("geometry-targets.json#{key}")),
            "dfm.stackup-order-confirmation.v1"
            | "dfm.total-thickness-material.v1"
            | "dfm.drill-span-plating.v1"
            | "dfm.finish-profile.v1"
            | "dfm.impedance-special-process.v1" => {
                Some(format!("construction-targets.json#{key}"))
            }
            "assembly.side-rotation.v1"
            | "assembly.paste-availability.v1"
            | "assembly.courtyard-native.v1"
            | "assembly.footprint-string-parity.v1"
            | "assembly.access.v1"
            | "assembly.testpoint-access.v1" => Some(format!("assembly-targets.json#{key}")),
            _ => None,
        };
        if family.family_class != expected_class
            || family.human_approval_required != inference
            || family.promotion_state != "evidence_only"
            || family.corpus_ref != expected_corpus
        {
            return Err(format!(
                "{key} class, corpus, approval gate, or EvidenceOnly default changed"
            ));
        }
        if family.capability_prerequisites.is_empty() && family.fact_prerequisites.is_empty() {
            return Err(format!("{key} has no prerequisites"));
        }
        let capability_count = family
            .capability_prerequisites
            .iter()
            .collect::<BTreeSet<_>>()
            .len();
        if capability_count != family.capability_prerequisites.len()
            || family.capability_prerequisites.iter().any(|capability| {
                !VALID_CAPABILITIES.contains(&capability.as_str())
                    || capability.starts_with("legacy_")
            })
        {
            return Err(format!(
                "{key} has duplicate, unknown, or legacy capability prerequisites"
            ));
        }
        let fact_count = family
            .fact_prerequisites
            .iter()
            .collect::<BTreeSet<_>>()
            .len();
        if fact_count != family.fact_prerequisites.len()
            || family
                .fact_prerequisites
                .iter()
                .any(|fact| !VALID_FACTS.contains(&fact.as_str()))
        {
            return Err(format!("{key} has duplicate or unknown fact prerequisites"));
        }
        require_unique_nonempty(&family.authority, &format!("{key} authority"))?;
        if family
            .authority
            .iter()
            .any(|authority| !VALID_AUTHORITIES.contains(&authority.as_str()))
        {
            return Err(format!("{key} has unknown authority"));
        }
        if !is_exact_set(&family.capability_prerequisites, expected.2)
            || !is_exact_set(&family.fact_prerequisites, expected.3)
            || !is_exact_set(&family.authority, expected.4)
        {
            return Err(format!("{key} prerequisites or authority changed"));
        }
    }
    if observed_families != expected_families {
        return Err("manifest does not cover every DFM-01..DFM-04 family/version".into());
    }
    Ok(())
}

fn validate_target_ids(ids: &[String], label: &str) -> Result<(), String> {
    require_unique_nonempty(ids, label)?;
    let mut sorted = ids.to_vec();
    sorted.sort();
    if sorted != ids {
        return Err(format!("{label} must be canonical and sorted"));
    }
    Ok(())
}

fn target_key(family_id: &str, family_version: &str, digest: &str, ids: &[String]) -> String {
    serde_json::to_string(&(family_id, family_version, digest, ids)).unwrap()
}

fn validate_family_targets(
    targets: &PopulationTargets,
    measured_labels: Option<&BTreeMap<String, String>>,
) -> Result<Metrics, String> {
    if targets.schema_version != 1
        || targets.origin != "project-authored"
        || !is_sha256(&targets.fixture_digest)
    {
        return Err("target metadata or digest changed".into());
    }
    let mut case_ids = BTreeSet::new();
    let mut target_keys = BTreeSet::new();
    let mut metrics = Metrics::default();
    let mut classes = BTreeSet::new();
    for target in &targets.targets {
        if target.case_id.trim().is_empty() || !case_ids.insert(target.case_id.as_str()) {
            return Err(format!("duplicate target case {}", target.case_id));
        }
        validate_target_ids(&target.canonical_target_ids, &target.case_id)?;
        if !target_keys.insert(target_key(
            &targets.family_id,
            &targets.family_version,
            &targets.fixture_digest,
            &target.canonical_target_ids,
        )) {
            return Err(format!("duplicate target key in {}", target.case_id));
        }
        match target.case_class.as_str() {
            "positive" if target.expected_label == "violation" => {}
            "hard_negative" if target.expected_label == "clean" => {}
            _ => {
                return Err(format!(
                    "{} has inconsistent class/expected label",
                    target.case_id
                ));
            }
        }
        let actual_label = match measured_labels {
            Some(labels) => labels.get(&target.case_id).map(String::as_str),
            None => target.actual_label.as_deref(),
        }
        .ok_or_else(|| format!("{} has no measured actual label", target.case_id))?;
        if !matches!(actual_label, "finding" | "no_finding") {
            return Err(format!("{} has unknown actual label", target.case_id));
        }
        classes.insert(target.case_class.as_str());
        match (target.expected_label.as_str(), actual_label) {
            ("violation", "finding") => metrics.tp += 1,
            ("clean", "finding") => metrics.fp += 1,
            ("violation", "no_finding") => metrics.fn_count += 1,
            ("clean", "no_finding") => metrics.tn += 1,
            _ => return Err(format!("{} has unknown confusion labels", target.case_id)),
        }
    }
    if classes != BTreeSet::from(["hard_negative", "positive"]) {
        return Err("population targets require positive and hard-negative cases".into());
    }
    for target in &targets.unsupported_targets {
        if target.case_id.trim().is_empty() || !case_ids.insert(target.case_id.as_str()) {
            return Err(format!("duplicate unsupported case {}", target.case_id));
        }
        validate_target_ids(&target.canonical_target_ids, &target.case_id)?;
        if !target_keys.insert(target_key(
            &targets.family_id,
            &targets.family_version,
            &targets.fixture_digest,
            &target.canonical_target_ids,
        )) || target.expected_status != "not_checked"
            || target.actual_status != "not_checked"
            || target.reason.trim().is_empty()
        {
            return Err(format!(
                "unsupported case {} did not fail closed",
                target.case_id
            ));
        }
    }
    metrics.executable_targets = targets.targets.len();
    metrics.unsupported_targets = targets.unsupported_targets.len();
    let precision_denominator = metrics.tp + metrics.fp;
    metrics.precision =
        (precision_denominator != 0).then(|| metrics.tp as f64 / precision_denominator as f64);
    let recall_denominator = metrics.tp + metrics.fn_count;
    metrics.recall =
        (recall_denominator != 0).then(|| metrics.tp as f64 / recall_denominator as f64);
    Ok(metrics)
}

fn validate_targets(targets: &PopulationTargets) -> Result<Metrics, String> {
    if targets.family_id != "assembly.population-parity"
        || targets.family_version != "v1"
        || targets.fixture_digest != TARGET_FIXTURE_DIGEST
    {
        return Err("population target metadata or digest changed".into());
    }
    validate_family_targets(targets, None)
}

fn validate_mutation_rows(
    mutations: &[Mutation],
    measurements: Option<&[MutationMeasurement]>,
) -> Result<usize, String> {
    let mut ids = BTreeSet::new();
    let mut kinds = BTreeSet::new();
    if measurements.is_some_and(|measurements| measurements.len() != mutations.len()) {
        return Err("mutation measurements changed row count".into());
    }
    for (index, mutation) in mutations.iter().enumerate() {
        let measurement = measurements.map(|measurements| &measurements[index]);
        let actual_status = match measurement {
            Some(measurement) => Some(measurement.status.as_str()),
            None => Some("not_checked"),
        };
        let expected_status = "not_checked";
        if mutation.id.trim().is_empty()
            || !ids.insert(mutation.id.as_str())
            || !kinds.insert(mutation.kind.as_str())
            || measurement.is_some_and(|measurement| {
                measurement.case_id != mutation.id
                    || measurement.kind != mutation.kind
                    || !measurement.input_changed
            })
            || mutation.expected_status != expected_status
            || actual_status != Some(expected_status)
            || mutation.contributes_to_confusion_matrix
            || mutation.detail.trim().is_empty()
        {
            return Err(format!(
                "mutation {} is duplicated, unmeasured, or not fail-closed: expected={expected_status} actual={actual_status:?}",
                mutation.id
            ));
        }
    }
    if kinds != REQUIRED_MUTATIONS.iter().copied().collect() {
        return Err("mutation corpus does not cover every required fail-closed class".into());
    }
    Ok(mutations.len())
}

fn validate_mutations(mutations: &MutationCorpus) -> Result<usize, String> {
    if mutations.schema_version != 1
        || mutations.origin != "project-authored"
        || mutations.family_id != "assembly.population-parity"
        || mutations.family_version != "v1"
        || mutations.fixture_digest != MUTATION_FIXTURE_DIGEST
        || !is_sha256(&mutations.fixture_digest)
    {
        return Err("mutation metadata or digest changed".into());
    }
    validate_mutation_rows(&mutations.mutations, None)
}

fn validate_geometry_corpus(corpus: &GeometryCorpus) -> Result<BTreeMap<String, Metrics>, String> {
    if corpus.schema_version != 1
        || corpus.origin != "project-authored"
        || corpus.license != "MIT OR Apache-2.0"
    {
        return Err("geometry corpus metadata changed".into());
    }
    let expected = BTreeMap::from([
        (
            "dfm.drill-tool-integrity.v1",
            "b502b7f44605d37aef01660212bb914ef2e5285a2202c791c2d80ba8036f7839",
        ),
        (
            "dfm.minimum-finished-drill.v1",
            "5d1aca9c2296ed543a65fc23421b9622ec200446df5076e3087c45117ddac831",
        ),
        (
            "dfm.outline-topology.v1",
            "1ccc6e6831aa72daf5698a79ba46083a7f6585a9742cfa87219e488e5579a94f",
        ),
        (
            "dfm.copper-edge.v1",
            "6e6da925b794970ba80ba4433071732094a61c12cee8a369f7ffcd281bd7d6c8",
        ),
        (
            "dfm.copper-clearance.v1",
            "273bdfeacf60f6f4fa4ad65e01dc71a96f93c6e10b29cf3ff00f8e2b09ce251a",
        ),
        (
            "dfm.annular-ring.v1",
            "dca43e3dbb44268fa3ed9023bde7b7c61d126d5ae2544c625cac838c97fe9889",
        ),
        (
            "dfm.mask-sliver.v1",
            "de9ca50f2abf0e82be8f8d9cc8bfe9c2399647563f5ae325145044e8ceefbc1f",
        ),
        (
            "dfm.paste-mask-relationship.v1",
            "a635af2ebcffbdaaf882b1c5b66de7e8cbca76a0cba2fbceb31450f5e66ef163",
        ),
    ]);
    let mut metrics_by_family = BTreeMap::new();
    for family in &corpus.families {
        let key = family_key(&family.family_id, &family.family_version);
        if expected.get(key.as_str()).copied() != Some(family.fixture_digest.as_str())
            || !is_sha256(&family.fixture_digest)
            || metrics_by_family.contains_key(&key)
        {
            return Err(format!(
                "unknown, duplicate, or malformed geometry family {key}"
            ));
        }
        let targets = PopulationTargets {
            schema_version: 1,
            origin: "project-authored".into(),
            family_id: family.family_id.clone(),
            family_version: family.family_version.clone(),
            fixture_digest: family.fixture_digest.clone(),
            targets: family.targets.clone(),
            unsupported_targets: family.unsupported_targets.clone(),
        };
        let mut metrics = validate_family_targets(&targets, None)?;
        metrics.not_checked_mutations = validate_mutation_rows(&family.mutations, None)?;
        if !meets_metric_policy(metrics, 9500) {
            return Err(format!("geometry family {key} does not meet metric policy"));
        }
        metrics_by_family.insert(key, metrics);
    }
    if metrics_by_family
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        != expected.keys().copied().collect()
    {
        return Err("geometry corpus family set changed".into());
    }
    Ok(metrics_by_family)
}

fn validate_confirmation_gap_targets(targets: &PopulationTargets) -> Result<Metrics, String> {
    if targets.schema_version != 1
        || targets.origin != "project-authored"
        || !is_sha256(&targets.fixture_digest)
    {
        return Err("confirmation-gap target metadata changed".into());
    }
    let mut case_ids = BTreeSet::new();
    let mut target_keys = BTreeSet::new();
    let mut classes = BTreeSet::new();
    let mut metrics = Metrics::default();
    for target in &targets.targets {
        if target.case_id.trim().is_empty() || !case_ids.insert(target.case_id.as_str()) {
            return Err(format!(
                "duplicate confirmation-gap case {}",
                target.case_id
            ));
        }
        validate_target_ids(&target.canonical_target_ids, &target.case_id)?;
        if !target_keys.insert(target_key(
            &targets.family_id,
            &targets.family_version,
            &targets.fixture_digest,
            &target.canonical_target_ids,
        )) {
            return Err(format!(
                "duplicate confirmation-gap target key {}",
                target.case_id
            ));
        }
        match (
            target.case_class.as_str(),
            target.expected_label.as_str(),
            target.actual_label.as_deref(),
        ) {
            ("positive", "confirmation_gap", Some("confirmation_gap")) => metrics.tp += 1,
            ("hard_negative", "no_match_or_conflict", Some("no_match_or_conflict")) => {
                metrics.tn += 1
            }
            _ => {
                return Err(format!(
                    "{} overclaims a represented result",
                    target.case_id
                ));
            }
        }
        classes.insert(target.case_class.as_str());
    }
    if classes != BTreeSet::from(["hard_negative", "positive"]) {
        return Err("confirmation-gap targets require positive and hard-negative cases".into());
    }
    for target in &targets.unsupported_targets {
        if target.case_id.trim().is_empty() || !case_ids.insert(target.case_id.as_str()) {
            return Err(format!("duplicate unsupported case {}", target.case_id));
        }
        validate_target_ids(&target.canonical_target_ids, &target.case_id)?;
        if !target_keys.insert(target_key(
            &targets.family_id,
            &targets.family_version,
            &targets.fixture_digest,
            &target.canonical_target_ids,
        )) || target.expected_status != "not_checked"
            || target.actual_status != "not_checked"
            || target.reason.trim().is_empty()
        {
            return Err(format!(
                "unsupported case {} did not fail closed",
                target.case_id
            ));
        }
    }
    metrics.executable_targets = targets.targets.len();
    metrics.unsupported_targets = targets.unsupported_targets.len();
    metrics.precision = Some(1.0);
    metrics.recall = Some(1.0);
    Ok(metrics)
}

fn validate_construction_corpus(
    corpus: &GeometryCorpus,
) -> Result<BTreeMap<String, Metrics>, String> {
    if corpus.schema_version != 1
        || corpus.origin != "project-authored"
        || corpus.license != "MIT OR Apache-2.0"
    {
        return Err("construction corpus metadata changed".into());
    }
    let expected = BTreeMap::from([
        (
            "dfm.stackup-order-confirmation.v1",
            "d1269c18cd29b96f930096c82c78bce48e1d169ee578e18ff937007a6c2561f7",
        ),
        (
            "dfm.total-thickness-material.v1",
            "b42e8b5a5fd348341f28781983ae9c3e1aada1917e607552667d8f6f4d85b6ce",
        ),
        (
            "dfm.drill-span-plating.v1",
            "b5410b2c0d14a207df977ed98f5d92217e0a6e0970fcd45587024e7e0e5b4b7b",
        ),
        (
            "dfm.finish-profile.v1",
            "ba41d4c170bccaaf8b7e43cef369c4cd70d18d457980e7bea74a448f694b3bd2",
        ),
        (
            "dfm.impedance-special-process.v1",
            "b13115a2b858f67de82afc794c18b9799048c093d3f91787f1d71e934e76362a",
        ),
    ]);
    let mut metrics_by_family = BTreeMap::new();
    for family in &corpus.families {
        let key = family_key(&family.family_id, &family.family_version);
        if expected.get(key.as_str()).copied() != Some(family.fixture_digest.as_str())
            || !is_sha256(&family.fixture_digest)
            || metrics_by_family.contains_key(&key)
        {
            return Err(format!(
                "unknown, duplicate, or malformed construction family {key}"
            ));
        }
        let targets = PopulationTargets {
            schema_version: 1,
            origin: "project-authored".into(),
            family_id: family.family_id.clone(),
            family_version: family.family_version.clone(),
            fixture_digest: family.fixture_digest.clone(),
            targets: family.targets.clone(),
            unsupported_targets: family.unsupported_targets.clone(),
        };
        let mut metrics = if key == "dfm.drill-span-plating.v1" {
            validate_confirmation_gap_targets(&targets)?
        } else {
            validate_family_targets(&targets, None)?
        };
        metrics.not_checked_mutations = validate_mutation_rows(&family.mutations, None)?;
        if !meets_metric_policy(metrics, 9500) {
            return Err(format!(
                "construction family {key} does not meet metric policy"
            ));
        }
        metrics_by_family.insert(key, metrics);
    }
    if metrics_by_family
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        != expected.keys().copied().collect()
    {
        return Err("construction corpus family set changed".into());
    }
    Ok(metrics_by_family)
}

fn validate_assembly_corpus(
    corpus: &GeometryCorpus,
    measured_labels: &BTreeMap<String, BTreeMap<String, String>>,
    measured_mutations: &BTreeMap<String, Vec<MutationMeasurement>>,
) -> Result<BTreeMap<String, Metrics>, String> {
    if corpus.schema_version != 1
        || corpus.origin != "project-authored"
        || corpus.license != "MIT OR Apache-2.0"
    {
        return Err("assembly corpus metadata changed".into());
    }
    let expected = BTreeMap::from([
        (
            "assembly.side-rotation.v1",
            "8cb05c2df287af00c1aef079109c1c9b9d711ca59fe9de66415c20b2886e9a0f",
        ),
        (
            "assembly.paste-availability.v1",
            "1f56f6a2f1ca81ebd774def1f822075dfef98e6b39eefaa44ac77f88d9474aee",
        ),
        (
            "assembly.courtyard-native.v1",
            "1c547f62117dc30800305b9deac789a0dbf7be796a1dccc38596479c3213545a",
        ),
        (
            "assembly.footprint-string-parity.v1",
            "8f8d93bb018140287172ed717acb793d8789c76ac9abce800411598a4859394b",
        ),
        (
            "assembly.access.v1",
            "0cd840929c657fb9b7cdff7cef8a4887cd77a9fc217dbca77f616dc89d7ebfb5",
        ),
        (
            "assembly.testpoint-access.v1",
            "19345edcc110b73234530a35fc381aa213abb6737bb4d1b55a99ca677bece0f0",
        ),
    ]);
    let mut metrics_by_family = BTreeMap::new();
    for family in &corpus.families {
        let key = family_key(&family.family_id, &family.family_version);
        if expected.get(key.as_str()).copied() != Some(family.fixture_digest.as_str())
            || !is_sha256(&family.fixture_digest)
            || metrics_by_family.contains_key(&key)
        {
            return Err(format!(
                "unknown, duplicate, or malformed assembly family {key}"
            ));
        }
        let targets = PopulationTargets {
            schema_version: 1,
            origin: "project-authored".into(),
            family_id: family.family_id.clone(),
            family_version: family.family_version.clone(),
            fixture_digest: family.fixture_digest.clone(),
            targets: family.targets.clone(),
            unsupported_targets: family.unsupported_targets.clone(),
        };
        let labels = measured_labels.get(&key);
        let mutation_statuses = measured_mutations.get(&key).map(Vec::as_slice);
        if INFERENCE_FAMILIES.contains(&key.as_str())
            && (labels.is_none() || mutation_statuses.is_none())
        {
            return Err(format!(
                "assembly family {key} has no production analyzer measurements"
            ));
        }
        let mut metrics = validate_family_targets(&targets, labels)?;
        metrics.not_checked_mutations =
            validate_mutation_rows(&family.mutations, mutation_statuses)?;
        if !meets_metric_policy(metrics, 9500) {
            return Err(format!("assembly family {key} does not meet metric policy"));
        }
        metrics_by_family.insert(key, metrics);
    }
    if metrics_by_family
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        != expected.keys().copied().collect()
    {
        return Err("assembly corpus family set changed".into());
    }
    Ok(metrics_by_family)
}

fn validate_contract(
    manifest: &Manifest,
    targets: &PopulationTargets,
    mutations: &MutationCorpus,
) -> Result<Metrics, String> {
    validate_manifest(manifest)?;
    let population_key = family_key(&targets.family_id, &targets.family_version);
    let mutation_key = family_key(&mutations.family_id, &mutations.family_version);
    if population_key != "assembly.population-parity.v1" || mutation_key != population_key {
        return Err("target and mutation family/version do not match the manifest".into());
    }
    let mut metrics = validate_targets(targets)?;
    metrics.not_checked_mutations = validate_mutations(mutations)?;
    Ok(metrics)
}

fn contract_from_values(
    manifest: &Value,
    targets: &Value,
    mutations: &Value,
) -> Result<Metrics, String> {
    let manifest = serde_json::from_value::<Manifest>(manifest.clone())
        .map_err(|error| format!("manifest JSON: {error}"))?;
    let targets = serde_json::from_value::<PopulationTargets>(targets.clone())
        .map_err(|error| format!("population target JSON: {error}"))?;
    let mutations = serde_json::from_value::<MutationCorpus>(mutations.clone())
        .map_err(|error| format!("mutation JSON: {error}"))?;
    validate_contract(&manifest, &targets, &mutations)
}

fn manifest_family_mut<'a>(manifest: &'a mut Value, family_id: &str) -> &'a mut Value {
    manifest["families"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|family| family["familyId"].as_str() == Some(family_id))
        .unwrap()
}

fn meets_metric_policy(metrics: Metrics, minimum_precision_bps: u16) -> bool {
    metrics
        .precision
        .is_some_and(|precision| precision >= f64::from(minimum_precision_bps) / 10_000.0)
        && metrics.recall.is_some()
        && metrics.tp > 0
        && metrics.tn > 0
        && metrics.not_checked_mutations == REQUIRED_MUTATIONS.len()
}

#[test]
fn inert_dfm_contract_is_semantically_valid_and_fail_closed() {
    let manifest_value = serde_json::from_str::<Value>(MANIFEST_JSON).unwrap();
    let targets_value = serde_json::from_str::<Value>(POPULATION_TARGETS_JSON).unwrap();
    let mutations_value = serde_json::from_str::<Value>(PREREQUISITE_MUTATIONS_JSON).unwrap();

    let metrics = contract_from_values(&manifest_value, &targets_value, &mutations_value).unwrap();
    assert_eq!(
        metrics,
        Metrics {
            tp: 2,
            fp: 0,
            fn_count: 0,
            tn: 2,
            executable_targets: 4,
            not_checked_mutations: 14,
            unsupported_targets: 1,
            precision: Some(1.0),
            recall: Some(1.0),
        }
    );
    assert!(meets_metric_policy(metrics, 9500));
    println!(
        "DFM QUALIFICATION family=assembly.population-parity.v1 tp={} fp={} fn={} tn={} precision={:.3} recall={:.3} executable={} not_checked_mutations={} unsupported={} gate=evidence_only",
        metrics.tp,
        metrics.fp,
        metrics.fn_count,
        metrics.tn,
        metrics.precision.unwrap(),
        metrics.recall.unwrap(),
        metrics.executable_targets,
        metrics.not_checked_mutations,
        metrics.unsupported_targets,
    );

    let mut duplicate_family = manifest_value.clone();
    let first_family = duplicate_family["families"][0].clone();
    duplicate_family["families"]
        .as_array_mut()
        .unwrap()
        .push(first_family);
    assert!(contract_from_values(&duplicate_family, &targets_value, &mutations_value).is_err());

    let mut missing_family = manifest_value.clone();
    missing_family["families"].as_array_mut().unwrap().remove(0);
    assert!(contract_from_values(&missing_family, &targets_value, &mutations_value).is_err());

    let mut wrong_version = manifest_value.clone();
    wrong_version["families"][0]["familyVersion"] = Value::String("v2".into());
    assert!(contract_from_values(&wrong_version, &targets_value, &mutations_value).is_err());

    let mut empty_prerequisites = manifest_value.clone();
    empty_prerequisites["families"][0]["capabilityPrerequisites"] = Value::Array(vec![]);
    assert!(contract_from_values(&empty_prerequisites, &targets_value, &mutations_value).is_err());

    let mut wrong_prerequisite = manifest_value.clone();
    wrong_prerequisite["families"][0]["capabilityPrerequisites"][0] =
        Value::String("legacy_token_screening".into());
    assert!(contract_from_values(&wrong_prerequisite, &targets_value, &mutations_value).is_err());

    let mut valid_but_wrong_prerequisite = manifest_value.clone();
    manifest_family_mut(&mut valid_but_wrong_prerequisite, "dfm.outline-topology")["capabilityPrerequisites"]
        [1] = Value::String("tools".into());
    assert!(
        contract_from_values(
            &valid_but_wrong_prerequisite,
            &targets_value,
            &mutations_value
        )
        .is_err()
    );

    let mut valid_but_wrong_fact = manifest_value.clone();
    manifest_family_mut(&mut valid_but_wrong_fact, "assembly.population-parity")["factPrerequisites"]
        [1] = Value::String("fitted_state".into());
    assert!(contract_from_values(&valid_but_wrong_fact, &targets_value, &mutations_value).is_err());

    let mut valid_but_wrong_authority = manifest_value.clone();
    manifest_family_mut(&mut valid_but_wrong_authority, "assembly.courtyard-native")["authority"]
        [0] = Value::String("canonical_model".into());
    assert!(
        contract_from_values(&valid_but_wrong_authority, &targets_value, &mutations_value).is_err()
    );

    let mut missing_forbidden_source = manifest_value.clone();
    missing_forbidden_source["forbiddenSources"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    assert!(
        contract_from_values(&missing_forbidden_source, &targets_value, &mutations_value).is_err()
    );

    let mut forged_promotion = manifest_value.clone();
    forged_promotion["families"][0]["promotionState"] = Value::String("blocking".into());
    assert!(contract_from_values(&forged_promotion, &targets_value, &mutations_value).is_err());

    let mut duplicate_target = targets_value.clone();
    let first_target = duplicate_target["targets"][0].clone();
    duplicate_target["targets"]
        .as_array_mut()
        .unwrap()
        .push(first_target);
    assert!(contract_from_values(&manifest_value, &duplicate_target, &mutations_value).is_err());

    let mut malformed_digest = targets_value.clone();
    malformed_digest["fixtureDigest"] = Value::String("not-a-sha256".into());
    assert!(contract_from_values(&manifest_value, &malformed_digest, &mutations_value).is_err());

    let mut blank_case_id = targets_value.clone();
    blank_case_id["targets"][0]["caseId"] = Value::String(" \t ".into());
    assert!(contract_from_values(&manifest_value, &blank_case_id, &mutations_value).is_err());

    let mut blank_target_id = targets_value.clone();
    blank_target_id["targets"][0]["canonicalTargetIds"][0] = Value::String("   ".into());
    assert!(contract_from_values(&manifest_value, &blank_target_id, &mutations_value).is_err());

    let mut blank_mutation_id = mutations_value.clone();
    blank_mutation_id["mutations"][0]["id"] = Value::String(" \t ".into());
    assert!(contract_from_values(&manifest_value, &targets_value, &blank_mutation_id).is_err());

    let mut false_positive = targets_value.clone();
    false_positive["targets"][2]["actualLabel"] = Value::String("finding".into());
    let false_positive_metrics =
        contract_from_values(&manifest_value, &false_positive, &mutations_value).unwrap();
    assert!(false_positive_metrics.precision.unwrap() < 0.95);
    assert!(!meets_metric_policy(false_positive_metrics, 9500));

    let mut no_predicted_findings = targets_value.clone();
    for target in no_predicted_findings["targets"].as_array_mut().unwrap() {
        target["actualLabel"] = Value::String("no_finding".into());
    }
    let undefined =
        contract_from_values(&manifest_value, &no_predicted_findings, &mutations_value).unwrap();
    assert_eq!(undefined.precision, None);
    assert_eq!(undefined.recall, Some(0.0));
    assert!(!meets_metric_policy(undefined, 9500));

    let mut mutation_as_tn = mutations_value.clone();
    mutation_as_tn["mutations"][0]["contributesToConfusionMatrix"] = Value::Bool(true);
    assert!(contract_from_values(&manifest_value, &targets_value, &mutation_as_tn).is_err());

    let mut missing_mutation = mutations_value.clone();
    missing_mutation["mutations"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    assert!(contract_from_values(&manifest_value, &targets_value, &missing_mutation).is_err());
    assert_eq!(metrics.tn, 2, "not_checked mutations must never inflate TN");
}

fn fixture(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(path)
}

fn population_options() -> ReviewOptions {
    ReviewOptions {
        board: None,
        schematic: None,
        bom: None,
        placement: None,
        supply_snapshot: None,
        dfm_declarations: None,
        preset: Preset::named("standard").unwrap(),
        native: NativeMode::Off,
        tool_version: "dfm-release-test".into(),
        scope: ReviewScope::Full,
        profile: None,
    }
}

fn copy_population_fixture(reverse_creation_order: bool) -> PathBuf {
    let destination = std::env::temp_dir().join(format!(
        "ratemypcb-dfm-population-{}-{}",
        std::process::id(),
        NEXT_POPULATION_FIXTURE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&destination).unwrap();
    let mut names = [
        "root.kicad_pro",
        "root.kicad_sch",
        "root.kicad_pcb",
        "root.net",
        "root-bom.csv",
        "root-positions.csv",
    ];
    if reverse_creation_order {
        names.reverse();
    }
    for name in names {
        fs::copy(fixture("kicad/mismatch").join(name), destination.join(name)).unwrap();
    }
    destination
}

fn replace_once(path: &Path, from: &str, to: &str) {
    let source = fs::read_to_string(path).unwrap();
    let changed = source.replacen(from, to, 1);
    assert_ne!(source, changed, "fixture mutation did not apply");
    fs::write(path, changed).unwrap();
}

fn assembly_project(placement: &str) -> PathBuf {
    let root = copy_population_fixture(false);
    replace_once(
        &root.join("root.kicad_pcb"),
        "(generator ratemypcb-fixture)",
        "(generator ratemypcb-fixture) (title_block (title \"phase7-assembly\") (rev \"A\"))",
    );
    replace_once(
        &root.join("root.kicad_pcb"),
        "(footprint \"Resistor:R_0603\" (layer \"F.Cu\")",
        "(footprint \"Resistor:R_0603\" (layer \"F.Cu\") (at 1 1 0)",
    );
    fs::write(root.join("root-positions.csv"), placement).unwrap();
    root
}

fn assembly_options(root: &Path) -> ReviewOptions {
    let mut options = population_options();
    options.placement = Some(root.join("root-positions.csv"));
    options
}

fn declared_placement(rotation: &str, side: &str, direction: &str) -> String {
    format!(
        "Ref,PosX,PosY,Rot,Side,Revision,Unit,Origin,SideConvention,BottomMirroring,RotationDirection,Fitted\nR1,1.000000000,1.000000000,{rotation},{side},A,mm,kicad_board,top_bottom,mirrored,{direction},fitted\n"
    )
}

#[test]
fn side_rotation_compares_only_explicit_conventions_and_equivalent_angles() {
    let root = assembly_project(&declared_placement("360", "top", "counter_clockwise"));
    let report = review(&root, assembly_options(&root)).unwrap();
    validate_report(&report).unwrap();
    assert_eq!(
        coverage_for(&report, "assembly.side-rotation.v1").status,
        CoverageStatus::Passed
    );
    assert!(findings_for(&report, "assembly.side-rotation.v1").is_empty());
    assert!(report.required_evidence.iter().all(|required| {
        !matches!(
            required.check_id.as_str(),
            "assembly.side-rotation.v1"
                | "assembly.paste-availability.v1"
                | "assembly.courtyard-native.v1"
                | "assembly.footprint-string-parity.v1"
        )
    }));

    let bottom = assembly_project(&declared_placement("270", "bottom", "counter_clockwise"));
    replace_once(
        &bottom.join("root.kicad_pcb"),
        "(layer \"F.Cu\") (at 1 1 0)",
        "(layer \"B.Cu\") (at 1 1 -90)",
    );
    let bottom_report = review(&bottom, assembly_options(&bottom)).unwrap();
    assert_eq!(
        coverage_for(&bottom_report, "assembly.side-rotation.v1").status,
        CoverageStatus::Passed
    );
    fs::remove_dir_all(bottom).unwrap();

    fs::write(
        root.join("root-positions.csv"),
        declared_placement("90", "top", "counter_clockwise"),
    )
    .unwrap();
    let mismatch = review(&root, assembly_options(&root)).unwrap();
    let findings = findings_for(&mismatch, "assembly.side-rotation.v1");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].gate_impact, GateImpact::EvidenceOnly);
    assert!(findings[0].evidence.contains("native_source="));
    assert!(findings[0].evidence.contains("declared_source="));
    let finding_id = findings[0].id.clone();
    let mut forged = mismatch.clone();
    forged
        .findings
        .iter_mut()
        .find(|finding| finding.id == finding_id)
        .unwrap()
        .gate_impact = GateImpact::Blocking;
    assert!(validate_report(&forged).is_err());

    fs::write(
        root.join("root-positions.csv"),
        declared_placement("0", "top", "unknown"),
    )
    .unwrap();
    let unknown = review(&root, assembly_options(&root)).unwrap();
    assert_eq!(
        coverage_for(&unknown, "assembly.side-rotation.v1").status,
        CoverageStatus::NotRun
    );
    assert!(findings_for(&unknown, "assembly.side-rotation.v1").is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn footprint_string_parity_maps_typed_exact_suffix_and_mismatch_semantics_only() {
    let exact_root = assembly_project(&declared_placement("0", "top", "counter_clockwise"));
    let exact = review(&exact_root, assembly_options(&exact_root)).unwrap();
    assert_eq!(
        coverage_for(&exact, "assembly.footprint-string-parity.v1").status,
        CoverageStatus::Passed
    );
    assert!(findings_for(&exact, "assembly.footprint-string-parity.v1").is_empty());
    assert_eq!(exact.schematic.footprint_comparisons.len(), 3);
    assert!(
        exact
            .schematic
            .footprint_comparisons
            .iter()
            .all(|comparison| {
                comparison.field == "footprint"
                    && comparison.matched
                    && !comparison.expected_source_digest.is_empty()
                    && !comparison.actual_source_digest.is_empty()
            })
    );
    fs::remove_dir_all(exact_root).unwrap();

    let suffix_root = assembly_project(&declared_placement("0", "top", "counter_clockwise"));
    replace_once(
        &suffix_root.join("root.kicad_pcb"),
        "Resistor:R_0603",
        "OtherLibrary:R_0603",
    );
    let suffix = review(&suffix_root, assembly_options(&suffix_root)).unwrap();
    assert_eq!(
        coverage_for(&suffix, "assembly.footprint-string-parity.v1").status,
        CoverageStatus::Passed
    );
    assert!(findings_for(&suffix, "assembly.footprint-string-parity.v1").is_empty());
    fs::remove_dir_all(suffix_root).unwrap();

    let mismatch_root = assembly_project(&declared_placement("0", "top", "counter_clockwise"));
    replace_once(
        &mismatch_root.join("root.kicad_pcb"),
        "Resistor:R_0603",
        "OtherLibrary:C_0603",
    );
    let mismatch = review(&mismatch_root, assembly_options(&mismatch_root)).unwrap();
    validate_report(&mismatch).unwrap();
    let findings = findings_for(&mismatch, "assembly.footprint-string-parity.v1");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].gate_impact, GateImpact::EvidenceOnly);
    assert!(findings[0].evidence.contains("typed_field=footprint"));
    assert!(findings[0].evidence.contains("expected_source="));
    assert!(findings[0].evidence.contains("actual_source="));
    assert!(!findings[0].evidence.contains("compatible"));
    fs::remove_dir_all(mismatch_root).unwrap();

    for unsupported in ["no-bom", "no-netlist", "missing-footprint-field"] {
        let root = assembly_project(&declared_placement("0", "top", "counter_clockwise"));
        match unsupported {
            "no-bom" => fs::remove_file(root.join("root-bom.csv")).unwrap(),
            "no-netlist" => fs::remove_file(root.join("root.net")).unwrap(),
            "missing-footprint-field" => replace_once(
                &root.join("root.kicad_sch"),
                " (property \"Footprint\" \"Resistor:R_0603\")",
                "",
            ),
            _ => unreachable!(),
        }
        let report = review(&root, assembly_options(&root)).unwrap();
        assert_eq!(
            coverage_for(&report, "assembly.footprint-string-parity.v1").status,
            CoverageStatus::NotRun,
            "{unsupported}: {}",
            coverage_for(&report, "assembly.footprint-string-parity.v1").evidence,
        );
        assert!(findings_for(&report, "assembly.footprint-string-parity.v1").is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    let ambiguous_root = assembly_project(&declared_placement("0", "top", "counter_clockwise"));
    let schematic_path = ambiguous_root.join("root.kicad_sch");
    let mut schematic = fs::read_to_string(&schematic_path)
        .unwrap()
        .trim_end()
        .to_owned();
    assert_eq!(schematic.pop(), Some(')'));
    schematic.push_str(" (symbol (uuid \"cccccccc-0000-4000-8000-000000000002\") (property \"Reference\" \"R1\") (property \"Value\" \"10k\") (property \"Footprint\" \"Resistor:R_0603\") (unit 1) (in_bom yes) (on_board yes) (dnp no)))");
    fs::write(&schematic_path, schematic).unwrap();
    let ambiguous = review(&ambiguous_root, assembly_options(&ambiguous_root)).unwrap();
    assert_eq!(
        coverage_for(&ambiguous, "assembly.footprint-string-parity.v1").status,
        CoverageStatus::NotRun
    );
    assert!(findings_for(&ambiguous, "assembly.footprint-string-parity.v1").is_empty());
    fs::remove_dir_all(ambiguous_root).unwrap();
}

#[test]
fn courtyard_native_absent_or_failed_execution_never_becomes_clean() {
    let root = assembly_project(&declared_placement("0", "top", "counter_clockwise"));
    let report = review(&root, assembly_options(&root)).unwrap();
    assert_eq!(
        report
            .fabrication
            .assembly
            .native_courtyard
            .as_ref()
            .unwrap()
            .state,
        ratemypcb_core::fabrication::NativeCourtyardRunState::Disabled
    );
    assert_eq!(
        coverage_for(&report, "assembly.courtyard-native.v1").status,
        CoverageStatus::NotRun
    );
    assert!(findings_for(&report, "assembly.courtyard-native.v1").is_empty());
    fs::remove_dir_all(root).unwrap();
}

fn occurrence_check_id<'a>(report: &'a Report, occurrence_id: &str) -> Option<&'a str> {
    report
        .evidence
        .iter()
        .find(|record| record.id == occurrence_id)
        .map(|record| record.check_id.as_str())
}

fn population_coverage(report: &Report) -> &Coverage {
    report
        .coverage
        .iter()
        .find(|coverage| occurrence_check_id(report, &coverage.id) == Some(POPULATION_FAMILY))
        .unwrap()
}

fn is_population_finding(report: &Report, finding_id: &str) -> bool {
    occurrence_check_id(report, finding_id)
        .and_then(|check_id| check_id.strip_prefix(POPULATION_FAMILY))
        .is_some_and(|suffix| suffix.starts_with('/'))
}

fn population_finding_ids(report: &Report) -> BTreeSet<&str> {
    report
        .findings
        .iter()
        .filter(|finding| is_population_finding(report, &finding.id))
        .map(|finding| finding.id.as_str())
        .collect()
}

#[test]
fn population_tracer_maps_typed_reconciliation_and_is_stable() {
    let root = copy_population_fixture(false);
    replace_once(&root.join("root-bom.csv"), "R1,1,", "R1,2,");
    let report = review(&root, population_options()).unwrap();
    validate_report(&report).unwrap();

    let mismatch = report
        .schematic
        .mismatches
        .iter()
        .find(|mismatch| mismatch.field == "bom-quantity")
        .unwrap();
    let finding = report
        .findings
        .iter()
        .find(|finding| is_population_finding(&report, &finding.id))
        .unwrap();
    assert_eq!(finding.gate_impact, GateImpact::EvidenceOnly);
    assert_eq!(finding.source, "schematic-reconciliation");
    assert_eq!(finding.location, mismatch.location);
    assert!(!report.findings.iter().any(|finding| {
        occurrence_check_id(&report, &finding.id) == Some(mismatch.check_id.as_str())
    }));
    assert!(
        finding.evidence.contains(&mismatch.expected)
            && finding.evidence.contains(&mismatch.actual)
            && finding.evidence.contains(&mismatch.join)
            && finding.evidence.contains(&mismatch.confidence)
    );
    assert_eq!(
        population_coverage(&report).status,
        CoverageStatus::Attention
    );
    let evidence = report
        .evidence
        .iter()
        .find(|record| record.id == finding.id)
        .unwrap();
    assert_eq!(
        evidence.provenance.artifact_digest,
        report.schematic.artifact_digests["schematic:composite"]
    );

    let missing_placement_root = copy_population_fixture(false);
    replace_once(
        &missing_placement_root.join("root-positions.csv"),
        "R1,10k,R_0603,1,1,0,top,A",
        "",
    );
    let missing_placement = review(&missing_placement_root, population_options()).unwrap();
    assert!(
        missing_placement
            .schematic
            .mismatches
            .iter()
            .any(|mismatch| mismatch.field == "placement-population")
    );
    assert_eq!(population_finding_ids(&missing_placement).len(), 1);
    validate_report(&missing_placement).unwrap();

    let reordered_root = copy_population_fixture(true);
    replace_once(&reordered_root.join("root-bom.csv"), "R1,1,", "R1,2,");
    let reordered = review(&reordered_root, population_options()).unwrap();
    assert_eq!(
        population_finding_ids(&report),
        population_finding_ids(&reordered)
    );
    validate_report(&reordered).unwrap();

    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(missing_placement_root).unwrap();
    fs::remove_dir_all(reordered_root).unwrap();
}

#[test]
fn population_tracer_accepts_distinct_findings_at_one_source_location() {
    let root = copy_population_fixture(false);
    replace_once(&root.join("root-bom.csv"), "R1,1,", "R1,2,");
    replace_once(&root.join("root-positions.csv"), ",top,A", ",top,B");
    let report = review(&root, population_options()).unwrap();

    let population = report
        .findings
        .iter()
        .filter(|finding| is_population_finding(&report, &finding.id))
        .collect::<Vec<_>>();
    assert_eq!(population.len(), 2);
    assert_eq!(
        population
            .iter()
            .map(|finding| finding.location.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        1
    );
    assert_eq!(
        population
            .iter()
            .filter_map(|finding| occurrence_check_id(&report, &finding.id))
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "assembly.population-parity.v1/bom-quantity",
            "assembly.population-parity.v1/revision",
        ])
    );
    validate_report(&report).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn population_tracer_clean_control_is_semantic_pass() {
    let report = review(&fixture("kicad/mismatch"), population_options()).unwrap();
    assert!(population_finding_ids(&report).is_empty());
    assert_eq!(population_coverage(&report).status, CoverageStatus::Passed);
    assert!(
        population_coverage(&report)
            .evidence
            .contains("0 typed population mismatch")
    );
    validate_report(&report).unwrap();
}

#[test]
fn population_tracer_fails_closed_on_authority_and_provenance_mutations() {
    let root = copy_population_fixture(false);
    replace_once(&root.join("root-bom.csv"), "R1,1,", "R1,2,");
    let report = review(&root, population_options()).unwrap();
    let capability_index = report
        .schematic
        .capabilities
        .iter()
        .position(|capability| capability.id == "schematic-reconciliation")
        .unwrap();

    for status in [
        "partial",
        "failed",
        "stale",
        "not_provided",
        "unsupported",
        "omitted",
    ] {
        let mut mutated = report.clone();
        mutated.schematic.capabilities[capability_index].status = status.into();
        assert!(validate_report(&mutated).is_err(), "{status}");
    }

    let mut missing = report.clone();
    missing.schematic.capabilities.remove(capability_index);
    assert!(validate_report(&missing).is_err());

    let mut duplicate = report.clone();
    let capability = duplicate.schematic.capabilities[capability_index].clone();
    duplicate.schematic.capabilities.push(capability);
    assert!(validate_report(&duplicate).is_err());

    let mut duplicate_reference = report.clone();
    let mut occurrence = duplicate_reference.schematic.occurrences[0].clone();
    occurrence.key = "0".repeat(64);
    occurrence.item_uuid = "duplicate-item".into();
    duplicate_reference.schematic.occurrences.push(occurrence);
    duplicate_reference.schematic.occurrence_count += 1;
    assert!(validate_report(&duplicate_reference).is_err());

    let mut missing_artifact = report.clone();
    missing_artifact
        .artifacts
        .retain(|artifact| artifact.kind != "bom");
    assert!(validate_report(&missing_artifact).is_err());

    let mut dangling = report.clone();
    dangling
        .schematic
        .mismatches
        .iter_mut()
        .find(|mismatch| mismatch.field == "bom-quantity")
        .unwrap()
        .location = "sheet=/missing;item=missing;source=missing.kicad_sch".into();
    assert!(validate_report(&dangling).is_err());

    let mut missing_digest = report.clone();
    missing_digest
        .schematic
        .artifact_digests
        .remove("schematic:composite");
    assert!(validate_report(&missing_digest).is_err());

    let mut blocking = report.clone();
    let finding_id = blocking
        .findings
        .iter()
        .find(|finding| is_population_finding(&blocking, &finding.id))
        .unwrap()
        .id
        .clone();
    blocking
        .findings
        .iter_mut()
        .find(|finding| finding.id == finding_id)
        .unwrap()
        .gate_impact = GateImpact::Blocking;
    assert!(validate_report(&blocking).is_err());

    let mut reordered = report.clone();
    reordered.schematic.mismatches.reverse();
    validate_report(&reordered).unwrap();
    assert_eq!(
        population_finding_ids(&report),
        population_finding_ids(&reordered)
    );

    let missing_population_input = copy_population_fixture(false);
    fs::remove_file(missing_population_input.join("root-bom.csv")).unwrap();
    let incomplete = review(&missing_population_input, population_options()).unwrap();
    assert_eq!(
        population_coverage(&incomplete).status,
        CoverageStatus::NotRun
    );
    assert!(
        population_coverage(&incomplete)
            .evidence
            .starts_with("not_checked:")
    );
    assert!(population_finding_ids(&incomplete).is_empty());
    validate_report(&incomplete).unwrap();

    let no_schematic = copy_population_fixture(false);
    fs::remove_file(no_schematic.join("root.kicad_sch")).unwrap();
    let unchecked = review(&no_schematic, population_options()).unwrap();
    assert_eq!(
        population_coverage(&unchecked).status,
        CoverageStatus::NotRun
    );
    assert!(
        population_coverage(&unchecked)
            .evidence
            .starts_with("not_checked:")
    );
    assert!(population_finding_ids(&unchecked).is_empty());
    validate_report(&unchecked).unwrap();

    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(missing_population_input).unwrap();
    fs::remove_dir_all(no_schematic).unwrap();
}

fn declaration_value() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/dfm/declarations.json"
    ))
    .unwrap()
}

fn declarations_from(value: &Value) -> Result<DfmDeclarations, ratemypcb_core::Error> {
    DfmDeclarations::from_json(
        "dfm/declarations.json",
        &serde_json::to_vec(value).unwrap(),
        2_000_000_000,
    )
}

fn authority_options(declarations: Option<DfmDeclarations>) -> ReviewOptions {
    let mut options = population_options();
    options.dfm_declarations = declarations;
    options
}

#[test]
fn authority_normalizes_exact_rules_and_represented_order_facts() {
    let declarations = declarations_from(&declaration_value()).unwrap();
    let digest = declarations.artifact_digest().to_owned();
    let baseline = review(&fixture("kicad/mismatch"), authority_options(None)).unwrap();
    assert!(
        baseline
            .fabrication
            .documents
            .iter()
            .all(|document| document.adapter != "ratemypcb-dfm-declarations")
    );
    assert_eq!(
        baseline
            .coverage
            .iter()
            .find(|coverage| {
                occurrence_check_id(&baseline, &coverage.id) == Some("dfm-declarations")
            })
            .unwrap()
            .status,
        CoverageStatus::NotProvided
    );
    let report = review(
        &fixture("kicad/mismatch"),
        authority_options(Some(declarations)),
    )
    .unwrap();
    validate_report(&report).unwrap();

    let document = report
        .fabrication
        .documents
        .iter()
        .find(|document| document.adapter == "ratemypcb-dfm-declarations")
        .unwrap();
    assert_eq!(document.artifact_digest, digest);
    assert_eq!(document.virtual_path, "dfm/declarations.json");
    let constraints = report
        .fabrication
        .constraints
        .iter()
        .filter(|constraint| constraint.provenance.document_id == document.id)
        .collect::<Vec<_>>();
    assert_eq!(constraints.len(), 26);
    assert_eq!(
        constraints
            .iter()
            .find(|constraint| constraint.kind == ConstraintKind::MinimumDrill)
            .unwrap()
            .value,
        Some(Picometres(200_000_000))
    );
    assert_eq!(
        constraints
            .iter()
            .find(|constraint| constraint.kind == ConstraintKind::FinishedThickness)
            .unwrap()
            .value,
        Some(Picometres(1_600_000_000))
    );
    assert_eq!(
        constraints
            .iter()
            .filter(|constraint| constraint.kind == ConstraintKind::Other)
            .count(),
        18
    );
    assert_eq!(
        constraints
            .iter()
            .filter(|constraint| {
                constraint
                    .declared_value
                    .as_deref()
                    .is_some_and(|value| value.starts_with("inference:"))
            })
            .count(),
        15
    );
    assert!(constraints.iter().all(|constraint| {
        constraint.authority == Authority::Explicit
            && constraint.provenance.artifact_digest == digest
            && constraint.provenance.producer == "ratemypcb-project-authority"
            && constraint.provenance.producer_version == "2026.08"
            && constraint
                .provenance
                .source_lexeme
                .as_deref()
                .is_some_and(|source| source.contains("@board"))
    }));
    assert!(
        report.fabrication.constraints.len()
            >= baseline.fabrication.constraints.len() + constraints.len()
    );
    assert_eq!(
        report
            .coverage
            .iter()
            .find(|coverage| {
                occurrence_check_id(&report, &coverage.id) == Some("dfm-declarations")
            })
            .unwrap()
            .status,
        CoverageStatus::Attention
    );
    assert_eq!(
        report
            .evidence
            .iter()
            .filter(|record| record.check_id.starts_with("dfm-declaration-gap/"))
            .count(),
        5
    );
    let gap = report
        .coverage
        .iter()
        .find(|coverage| {
            occurrence_check_id(&report, &coverage.id)
                == Some("dfm-declaration-gap/drill_span_plating")
        })
        .unwrap();
    assert!(gap.evidence.contains("ratemypcb-project-authority 2026.08"));
    assert!(gap.evidence.contains("confirm with fabricator"));
    assert!(gap.evidence.contains("applies=board"));
    assert_eq!(report.approval_eligible, baseline.approval_eligible);
    assert!(report.findings.iter().all(|finding| {
        !is_population_finding(&report, &finding.id)
            || finding.gate_impact == GateImpact::EvidenceOnly
    }));
}

#[test]
fn authority_rejects_invalid_stale_duplicate_unknown_and_unbounded_inputs() {
    let original = declaration_value();
    let mut cases = Vec::new();

    let mut unknown = original.clone();
    unknown["rules"][0]["id"] = Value::String("unknown-rule".into());
    cases.push(("unknown", unknown));

    let mut duplicate = original.clone();
    let first = duplicate["rules"][0].clone();
    duplicate["rules"].as_array_mut().unwrap().push(first);
    cases.push(("duplicate", duplicate));

    let mut stale = original.clone();
    stale["expiresAtUnix"] = Value::from(1);
    cases.push(("stale", stale));

    let mut partial = original.clone();
    partial["rules"][0]["state"] = Value::String("partial".into());
    cases.push(("partial", partial));

    let mut unit = original.clone();
    unit["rules"][0]["unit"] = Value::String("mil".into());
    cases.push(("unit", unit));

    let mut inexact = original.clone();
    inexact["rules"][0]["value"] = Value::String("0.0000000001".into());
    cases.push(("inexact", inexact));

    let mut range = original.clone();
    range["rules"][0]["value"] = Value::String("999999999999999999".into());
    cases.push(("range", range));

    let mut unknown_field = original.clone();
    unknown_field["unexpected"] = Value::Bool(true);
    cases.push(("unknown-field", unknown_field));

    let mut padded_producer = original.clone();
    padded_producer["producer"] = Value::String(" padded-authority ".into());
    cases.push(("padded-producer", padded_producer));

    let mut padded_fact = original.clone();
    padded_fact["orderAcknowledgements"][1]["declaredValue"] = Value::String(" ENIG ".into());
    cases.push(("padded-fact", padded_fact));

    let mut unbounded = original.clone();
    let rule = unbounded["rules"][0].clone();
    unbounded["rules"] = Value::Array(vec![rule; 129]);
    cases.push(("unbounded", unbounded));

    for (name, value) in cases {
        assert!(declarations_from(&value).is_err(), "{name}");
    }

    let mut dangling = original.clone();
    let mut layer = serde_json::json!({
        "record": 32,
        "id": "layer_material",
        "state": "complete",
        "value": null,
        "unit": null,
        "declaredValue": "FR-4",
        "applicability": "layer:missing"
    });
    dangling["orderAcknowledgements"]
        .as_array_mut()
        .unwrap()
        .push(layer.take());
    let parsed = declarations_from(&dangling).unwrap();
    assert!(review(&fixture("kicad/mismatch"), authority_options(Some(parsed))).is_err());
}

#[test]
fn declaration_inference_extension_rejects_unknown_partial_duplicate_and_unbounded_records() {
    let original = declaration_value();
    let parsed = declarations_from(&original).unwrap();
    let report = review(
        &fixture("narrow-board.kicad_pcb"),
        authority_options(Some(parsed)),
    )
    .unwrap();
    validate_report(&report).unwrap();
    let document = report
        .fabrication
        .documents
        .iter()
        .find(|document| document.adapter == "ratemypcb-dfm-declarations")
        .unwrap();
    let inference = report
        .fabrication
        .constraints
        .iter()
        .filter(|constraint| {
            constraint.provenance.document_id == document.id
                && constraint
                    .declared_value
                    .as_deref()
                    .is_some_and(|value| value.starts_with("inference:"))
        })
        .collect::<Vec<_>>();
    assert_eq!(inference.len(), 15);
    assert_eq!(
        inference
            .iter()
            .map(|constraint| constraint.provenance.location.record)
            .collect::<BTreeSet<_>>(),
        (17..=31).collect()
    );
    assert!(inference.iter().all(|constraint| {
        constraint.kind == ConstraintKind::Other
            && constraint.value.is_none()
            && constraint.authority == Authority::Explicit
            && constraint.provenance.artifact_digest == document.artifact_digest
            && constraint.provenance.producer == "ratemypcb-project-authority"
            && constraint.provenance.producer_version == "2026.08"
    }));

    let mut cases = Vec::new();
    let mut unknown_model = original.clone();
    unknown_model["inferenceRecords"][0]["model"] = Value::String("unknown".into());
    cases.push(("unknown-model", unknown_model));

    let mut unknown_version = original.clone();
    unknown_version["inferenceRecords"][0]["modelVersion"] = Value::String("2".into());
    cases.push(("unknown-version", unknown_version));

    let mut missing_unit = original.clone();
    missing_unit["inferenceRecords"][0]["limits"][0]["unit"] = Value::Null;
    cases.push(("missing-unit", missing_unit));

    let mut out_of_range = original.clone();
    out_of_range["inferenceRecords"][0]["limits"][2]["value"] = Value::String("1000000".into());
    cases.push(("out-of-range", out_of_range));

    let mut non_finite = original.clone();
    non_finite["inferenceRecords"][0]["limits"][2]["value"] = Value::String("NaN".into());
    cases.push(("non-finite", non_finite));

    let mut names_only = original.clone();
    names_only["inferenceRecords"][2]["targetIds"] = serde_json::json!(["TEST"]);
    cases.push(("names-only-target", names_only));

    let mut names_only_connector = original.clone();
    names_only_connector["inferenceRecords"][14]["parameters"][0]["value"] =
        Value::String("J1".into());
    cases.push(("names-only-connector", names_only_connector));

    let mut partial = original.clone();
    partial["inferenceRecords"][0]["state"] = Value::String("partial".into());
    cases.push(("partial", partial));

    let mut missing_limit = original.clone();
    missing_limit["inferenceRecords"][0]["limits"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    cases.push(("missing-limit", missing_limit));

    let mut duplicate_limit = original.clone();
    let limit = duplicate_limit["inferenceRecords"][0]["limits"][0].clone();
    duplicate_limit["inferenceRecords"][0]["limits"]
        .as_array_mut()
        .unwrap()
        .push(limit);
    cases.push(("duplicate-limit", duplicate_limit));

    let mut duplicate_authority = original.clone();
    let mut duplicate = duplicate_authority["inferenceRecords"][0].clone();
    duplicate["record"] = Value::from(32);
    duplicate_authority["inferenceRecords"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    cases.push(("duplicate-authority", duplicate_authority));

    let mut unbounded_targets = original.clone();
    unbounded_targets["inferenceRecords"][2]["targetIds"] = Value::Array(
        (0_u64..17)
            .map(|index| Value::String(format!("net-v1-{index:064x}")))
            .collect(),
    );
    cases.push(("unbounded-targets", unbounded_targets));

    let mut unknown_field = original.clone();
    unknown_field["inferenceRecords"][0]["unexpected"] = Value::Bool(true);
    cases.push(("unknown-field", unknown_field));

    for (name, value) in cases {
        assert!(declarations_from(&value).is_err(), "{name}");
    }
}

#[test]
fn authority_converts_inches_and_retains_unique_layer_provenance() {
    let root = std::env::temp_dir().join(format!(
        "ratemypcb-dfm-layer-authority-{}-{}",
        std::process::id(),
        NEXT_POPULATION_FIXTURE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
    fs::copy(
        fixture("fabrication/gerber/simple-x2.gbr"),
        root.join("top.gbr"),
    )
    .unwrap();
    let baseline = review(&root, authority_options(None)).unwrap();
    let layer_id = baseline.fabrication.layers[0].id.clone();

    let mut value = declaration_value();
    value["rules"][0]["value"] = Value::String("0.01".into());
    value["rules"][0]["unit"] = Value::String("in".into());
    value["orderAcknowledgements"]
        .as_array_mut()
        .unwrap()
        .extend([
            serde_json::json!({
                "record": 32,
                "id": "layer_material",
                "state": "complete",
                "value": null,
                "unit": null,
                "declaredValue": "FR-4",
                "applicability": format!("layer-id:{layer_id}")
            }),
            serde_json::json!({
                "record": 33,
                "id": "layer_thickness",
                "state": "complete",
                "value": "0.035",
                "unit": "mm",
                "declaredValue": null,
                "applicability": format!("layer-id:{layer_id}")
            }),
        ]);
    let report = review(
        &root,
        authority_options(Some(declarations_from(&value).unwrap())),
    )
    .unwrap();
    validate_report(&report).unwrap();
    let document = report
        .fabrication
        .documents
        .iter()
        .find(|document| document.adapter == "ratemypcb-dfm-declarations")
        .unwrap();
    assert_eq!(
        report
            .fabrication
            .constraints
            .iter()
            .find(|constraint| {
                constraint.provenance.document_id == document.id
                    && constraint.kind == ConstraintKind::MinimumDrill
            })
            .unwrap()
            .value,
        Some(Picometres(254_000_000))
    );
    let layers = report
        .fabrication
        .construction
        .layers
        .iter()
        .filter(|layer| layer.provenance.document_id == document.id)
        .collect::<Vec<_>>();
    assert_eq!(layers.len(), 2);
    assert!(layers.iter().all(|layer| layer.layer_id.is_some()));
    assert_eq!(
        layers
            .iter()
            .map(|layer| layer.provenance.location.record)
            .collect::<BTreeSet<_>>()
            .len(),
        2
    );
    assert!(
        layers
            .iter()
            .any(|layer| layer.material.as_deref() == Some("FR-4"))
    );
    assert!(
        layers
            .iter()
            .any(|layer| layer.thickness == Some(Picometres(35_000_000)))
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn authority_distinguishes_same_fact_for_distinct_layer_applicability() {
    let root = std::env::temp_dir().join(format!(
        "ratemypcb-dfm-multilayer-authority-{}-{}",
        std::process::id(),
        NEXT_POPULATION_FIXTURE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
    let top = fs::read_to_string(fixture("fabrication/gerber/simple-x2.gbr")).unwrap();
    fs::write(root.join("top.gbr"), &top).unwrap();
    fs::write(
        root.join("bottom.gbr"),
        top.replace("Copper,L1,Top", "Copper,L2,Bot"),
    )
    .unwrap();
    let baseline = review(&root, authority_options(None)).unwrap();
    assert_eq!(baseline.fabrication.layers.len(), 2);
    let layer_ids = baseline
        .fabrication
        .layers
        .iter()
        .map(|layer| layer.id.clone())
        .collect::<Vec<_>>();

    let mut value = declaration_value();
    value["rules"][1]["applicability"] = Value::String(format!("layer-id:{}", layer_ids[0]));
    let mut second_clearance = value["rules"][1].clone();
    second_clearance["record"] = Value::from(32);
    second_clearance["applicability"] = Value::String(format!("layer-id:{}", layer_ids[1]));
    value["rules"]
        .as_array_mut()
        .unwrap()
        .push(second_clearance);
    value["orderAcknowledgements"]
        .as_array_mut()
        .unwrap()
        .extend(layer_ids.iter().enumerate().map(|(index, layer_id)| {
            serde_json::json!({
                "record": 33 + index,
                "id": "layer_material",
                "state": "complete",
                "value": null,
                "unit": null,
                "declaredValue": "FR-4",
                "applicability": format!("layer-id:{layer_id}")
            })
        }));
    let report = review(
        &root,
        authority_options(Some(declarations_from(&value).unwrap())),
    )
    .unwrap();
    validate_report(&report).unwrap();
    let document = report
        .fabrication
        .documents
        .iter()
        .find(|document| document.adapter == "ratemypcb-dfm-declarations")
        .unwrap();
    let declared_clearance = report
        .fabrication
        .constraints
        .iter()
        .filter(|constraint| {
            constraint.provenance.document_id == document.id
                && constraint.kind == ConstraintKind::MinimumClearance
        })
        .collect::<Vec<_>>();
    assert_eq!(declared_clearance.len(), 2);
    assert!(layer_ids.iter().all(|layer_id| {
        declared_clearance.iter().any(|constraint| {
            constraint
                .declared_value
                .as_deref()
                .is_some_and(|value| value.contains(&format!("applies=layer-id:{layer_id}")))
        })
    }));
    assert_eq!(
        report
            .fabrication
            .construction
            .layers
            .iter()
            .filter(|layer| {
                layer.provenance.document_id == document.id
                    && layer.material.as_deref() == Some("FR-4")
            })
            .count(),
        2
    );
    fs::remove_dir_all(root).unwrap();
}

fn geometry_x2_layer(function: &str, profile: bool) -> String {
    let geometry = if profile {
        "G36*\nX000000Y000000D02*\nX10000000Y000000D01*\nX10000000Y10000000D01*\nX000000Y10000000D01*\nX000000Y000000D01*\nG37*\n"
    } else {
        "X1000000Y1000000D02*\nX2000000Y1000000D01*\n"
    };
    format!(
        "G04 RateMyPCB Plan 07-04 project-authored geometry fixture*\n%FSLAX46Y46*%\n%MOMM*%\n%TF.FileFunction,{function}*%\n%TA.AperFunction,Conductor*%\n%ADD10C,0.200*%\nD10*\n%TO.N,GND*%\n%TO.C,U1*%\n%TO.P,U1,1*%\n{geometry}M02*\n"
    )
}

fn geometry_package() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "ratemypcb-dfm-geometry-{}-{}",
        std::process::id(),
        NEXT_POPULATION_FIXTURE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
    fs::write(
        root.join("top.gbr"),
        geometry_x2_layer("Copper,L1,Top", false),
    )
    .unwrap();
    fs::write(
        root.join("bottom.gbr"),
        geometry_x2_layer("Copper,L2,Bot", false),
    )
    .unwrap();
    fs::write(
        root.join("profile.gbr"),
        geometry_x2_layer("Profile,NP", true),
    )
    .unwrap();
    fs::copy(
        fixture("fabrication/xnc/strict.xnc"),
        root.join("holes.xnc"),
    )
    .unwrap();
    fs::copy(
        fixture("fabrication/job/complete.gbrjob"),
        root.join("complete.gbrjob"),
    )
    .unwrap();
    root
}

fn geometry_declarations(minimum_drill: &str) -> DfmDeclarations {
    let mut value = declaration_value();
    value["rules"][0]["value"] = Value::String(minimum_drill.into());
    declarations_from(&value).unwrap()
}

fn distance_copper_layer() -> String {
    "G04 RateMyPCB Plan 07-05 project-authored distance fixture*\n%FSLAX46Y46*%\n%MOMM*%\n%TF.FileFunction,Copper,L1,Top*%\n%TA.AperFunction,Conductor*%\n%ADD10C,0.200*%\nD10*\n%TO.N,GND*%\nX1000000Y1000000D02*\nX2000000Y1000000D01*\n%TO.N,VCC*%\nX1000000Y1400000D02*\nX2000000Y1400000D01*\nM02*\n".into()
}

fn distance_package(route: bool) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "ratemypcb-dfm-distance-{}-{}",
        std::process::id(),
        NEXT_POPULATION_FIXTURE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
    fs::write(root.join("top.gbr"), distance_copper_layer()).unwrap();
    fs::write(
        root.join("bottom.gbr"),
        geometry_x2_layer("Copper,L2,Bot", false),
    )
    .unwrap();
    fs::write(
        root.join("profile.gbr"),
        geometry_x2_layer("Profile,NP", true),
    )
    .unwrap();
    fs::write(
        root.join("holes.xnc"),
        "; RateMyPCB Plan 07-05 drill fixture\nM48\n; #@! TF.FileFunction,Plated,1,2,PTH\n; #@! TF.GenerationSoftware,Ucamco,UcamX,2021.11\nMETRIC\nT01C0.600\n%\nT01\nX5.000Y5.000\nM30\n",
    )
    .unwrap();
    let mut files = vec![
        serde_json::json!({"Path": "top.gbr", "FileFunction": "Copper,L1,Top"}),
        serde_json::json!({"Path": "bottom.gbr", "FileFunction": "Copper,L2,Bot"}),
        serde_json::json!({"Path": "profile.gbr", "FileFunction": "Profile,NP"}),
        serde_json::json!({"Path": "holes.xnc", "FileFunction": "Plated,1,2,PTH"}),
    ];
    if route {
        fs::write(
            root.join("route.xnc"),
            "; RateMyPCB Plan 07-05 routed-boundary fixture\nM48\n; #@! TF.FileFunction,NonPlated,1,2,NPTH\n; #@! TF.GenerationSoftware,Ucamco,UcamX,2021.11\nMETRIC\nT01C0.200\n%\nT01\nG00X1.000Y2.000\nM15\nG01X2.000Y2.000\nM16\nM30\n",
        )
        .unwrap();
        files.push(serde_json::json!({
            "Path": "route.xnc",
            "FileFunction": "NonPlated,1,2,NPTH"
        }));
    }
    fs::write(
        root.join("complete.gbrjob"),
        serde_json::to_vec(&serde_json::json!({
            "Header": {"GenerationSoftware": {"Vendor": "RateMyPCB", "Application": "fixture", "Version": "1"}},
            "GeneralSpecs": {"ProjectId": {"Name": "phase7-distance", "Revision": "r1", "PartNumber": "P7-005"}},
            "FilesAttributes": files
        }))
        .unwrap(),
    )
    .unwrap();
    root
}

fn distance_declarations(edge: &str, clearance: &str) -> DfmDeclarations {
    let mut value = declaration_value();
    for rule in value["rules"].as_array_mut().unwrap() {
        match rule["id"].as_str().unwrap() {
            "dfm.copper-edge.v1" => rule["value"] = Value::String(edge.into()),
            "minimum_clearance" => rule["value"] = Value::String(clearance.into()),
            _ => {}
        }
    }
    declarations_from(&value).unwrap()
}

fn distance_report(route: bool, edge: Option<&str>, clearance: &str) -> (PathBuf, Report) {
    let root = distance_package(route);
    let declarations = edge.map(|edge| distance_declarations(edge, clearance));
    let report = review(&root, authority_options(declarations)).unwrap();
    (root, report)
}

fn annular_declarations(value: &str) -> DfmDeclarations {
    let mut declarations = declaration_value();
    let rule = declarations["rules"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|rule| rule["id"] == "minimum_annular_ring")
        .unwrap();
    rule["value"] = Value::String(value.into());
    declarations_from(&declarations).unwrap()
}

fn native_annular_package(kind: &str, shape: &str, size: &str, drill: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "ratemypcb-dfm-annular-{}-{}",
        std::process::id(),
        NEXT_POPULATION_FIXTURE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
    fs::write(
        root.join("board.kicad_pcb"),
        format!(
            "(kicad_pcb (version 20240108)\n  (generator ratemypcb-plan07-05)\n  (general (thickness 1.6))\n  (layers\n    (0 \"F.Cu\" signal)\n    (31 \"B.Cu\" signal)\n    (44 \"Edge.Cuts\" user \"Edge.Cuts\")\n  )\n  (net 0 \"\")\n  (net 1 \"PTH\")\n  (footprint \"Connector:Test\" (layer \"F.Cu\") (at 0 0)\n    (property \"Reference\" \"J1\")\n    (pad \"1\" {kind} {shape} (at 5 5) (size {size}) (drill {drill}) (layers \"*.Cu\" \"*.Mask\") (net 1 \"PTH\"))\n  )\n  (gr_rect (start 0 0) (end 10 10) (stroke (width 0.05) (type default)) (fill none) (layer \"Edge.Cuts\"))\n)\n"
        ),
    )
    .unwrap();
    root
}

fn annular_report(
    kind: &str,
    shape: &str,
    size: &str,
    drill: &str,
    threshold: Option<&str>,
) -> (PathBuf, Report) {
    let root = native_annular_package(kind, shape, size, drill);
    let report = review(
        &root,
        authority_options(threshold.map(annular_declarations)),
    )
    .unwrap();
    (root, report)
}

fn mask_paste_declarations(mask: &str, paste: &str) -> DfmDeclarations {
    let mut value = declaration_value();
    for rule in value["rules"].as_array_mut().unwrap() {
        match rule["id"].as_str().unwrap() {
            "dfm.mask-sliver.v1" => rule["value"] = Value::String(mask.into()),
            "dfm.paste-mask-relationship.v1" => rule["value"] = Value::String(paste.into()),
            _ => {}
        }
    }
    declarations_from(&value).unwrap()
}

fn mask_paste_gerber(function: &str, diameter: &str) -> String {
    format!(
        "G04 RateMyPCB Plan 07-06 project-authored mask/paste fixture*\n%FSLAX46Y46*%\n%MOMM*%\n%TF.FileFunction,{function}*%\n%TA.AperFunction,SMDPad,CuDef*%\n%ADD10C,{diameter}*%\nD10*\n%TO.C,R1*%\n%TO.P,R1,1*%\nX1000000Y1000000D03*\n%TO.P,R1,2*%\nX2200000Y1000000D03*\nM02*\n"
    )
}

fn mask_paste_package() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "ratemypcb-dfm-mask-paste-{}-{}",
        std::process::id(),
        NEXT_POPULATION_FIXTURE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
    fs::write(
        root.join("mask.gbr"),
        mask_paste_gerber("Soldermask,Top", "1.000"),
    )
    .unwrap();
    fs::write(
        root.join("paste.gbr"),
        mask_paste_gerber("Paste,Top", "0.800"),
    )
    .unwrap();
    fs::write(
        root.join("complete.gbrjob"),
        serde_json::to_vec(&serde_json::json!({
            "Header": {"GenerationSoftware": {"Vendor": "RateMyPCB", "Application": "fixture", "Version": "1"}},
            "GeneralSpecs": {"ProjectId": {"Name": "phase7-mask-paste", "Revision": "r1", "PartNumber": "P7-006"}},
            "FilesAttributes": [
                {"Path": "mask.gbr", "FileFunction": "Soldermask,Top"},
                {"Path": "paste.gbr", "FileFunction": "Paste,Top"}
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    root
}

fn mask_paste_report(with_declarations: bool) -> (PathBuf, Report) {
    let root = mask_paste_package();
    let report = review(
        &root,
        authority_options(with_declarations.then(|| mask_paste_declarations("0.200", "0.100"))),
    )
    .unwrap();
    (root, report)
}

fn assembly_mask_paste_project(side: &str, dnp: bool) -> PathBuf {
    let root = mask_paste_package();
    fs::write(
        root.join("copper.gbr"),
        mask_paste_gerber("Copper,L1,Top", "1.000"),
    )
    .unwrap();
    let job_path = root.join("complete.gbrjob");
    let mut job: Value = serde_json::from_slice(&fs::read(&job_path).unwrap()).unwrap();
    job["FilesAttributes"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "Path": "copper.gbr",
            "FileFunction": "Copper,L1,Top"
        }));
    fs::write(&job_path, serde_json::to_vec(&job).unwrap()).unwrap();
    let (copper, paste, mask) = if side == "F.Cu" {
        ("F.Cu", "F.Paste", "F.Mask")
    } else {
        ("B.Cu", "B.Paste", "B.Mask")
    };
    let dnp = if dnp { " dnp" } else { "" };
    fs::write(
        root.join("board.kicad_pcb"),
        format!(
            "(kicad_pcb (version 20240108)\n  (generator ratemypcb-plan07-08)\n  (title_block (title \"phase7-mask-paste\") (rev \"r1\"))\n  (layers (0 \"F.Cu\" signal) (31 \"B.Cu\" signal) (44 \"Edge.Cuts\" user \"Edge.Cuts\"))\n  (net 0 \"\") (net 1 \"GND\")\n  (footprint \"Resistor:R_0603\" (layer \"{side}\") (at 1 1 0)\n    (uuid \"33333333-3333-4333-8333-333333333333\")\n    (property \"Reference\" \"R1\")\n    (attr smd{dnp})\n    (pad \"1\" smd circle (at 0 0) (size 1 1) (layers \"{copper}\" \"{paste}\" \"{mask}\") (net 1 \"GND\"))\n    (pad \"2\" smd circle (at 1.2 0) (size 1 1) (layers \"{copper}\" \"{paste}\" \"{mask}\") (net 1 \"GND\")))\n  (gr_rect (start 0 0) (end 10 10) (layer \"Edge.Cuts\")))\n"
        ),
    )
    .unwrap();
    root
}

#[test]
fn assembly_paste_requires_fitted_side_pad_identity_and_actual_geometry() {
    let root = assembly_mask_paste_project("F.Cu", false);
    let report = review(&root, authority_options(None)).unwrap();
    validate_report(&report).unwrap();
    let paste_coverage = coverage_for(&report, "assembly.paste-availability.v1");
    assert_eq!(
        paste_coverage.status,
        CoverageStatus::Passed,
        "{}",
        paste_coverage.evidence
    );
    assert!(findings_for(&report, "assembly.paste-availability.v1").is_empty());
    fs::remove_dir_all(root).unwrap();

    let wrong_side = assembly_mask_paste_project("B.Cu", false);
    let report = review(&wrong_side, authority_options(None)).unwrap();
    let findings = findings_for(&report, "assembly.paste-availability.v1");
    assert_eq!(findings.len(), 2);
    assert!(findings.iter().all(|finding| {
        finding.gate_impact == GateImpact::EvidenceOnly
            && finding.evidence.contains("paste_source=")
            && finding.evidence.contains("placement_source=")
    }));
    fs::remove_dir_all(wrong_side).unwrap();

    let dnp = assembly_mask_paste_project("F.Cu", true);
    let report = review(&dnp, authority_options(None)).unwrap();
    assert_eq!(
        coverage_for(&report, "assembly.paste-availability.v1").status,
        CoverageStatus::Passed
    );
    assert!(findings_for(&report, "assembly.paste-availability.v1").is_empty());
    fs::remove_dir_all(dnp).unwrap();

    for mutation in ["omission", "both-omit", "windowpane"] {
        let root = assembly_mask_paste_project("F.Cu", false);
        let paste = root.join("paste.gbr");
        match mutation {
            "omission" => {
                replace_once(&paste, "%TO.P,R1,2*%\nX2200000Y1000000D03*\n", "");
            }
            "both-omit" => {
                replace_once(&paste, "%TO.P,R1,2*%\nX2200000Y1000000D03*\n", "");
                replace_once(
                    &root.join("mask.gbr"),
                    "%TO.P,R1,2*%\nX2200000Y1000000D03*\n",
                    "",
                );
            }
            "windowpane" => {
                replace_once(
                    &paste,
                    "%TO.P,R1,1*%\nX1000000Y1000000D03*\n",
                    "%TO.P,R1,1*%\nX1000000Y1000000D03*\nX1100000Y1000000D03*\n",
                );
            }
            _ => unreachable!(),
        }
        let report = review(&root, authority_options(None)).unwrap();
        assert_eq!(
            coverage_for(&report, "assembly.paste-availability.v1").status,
            CoverageStatus::NotRun,
            "{mutation}: {}",
            coverage_for(&report, "assembly.paste-availability.v1").evidence,
        );
        assert!(
            findings_for(&report, "assembly.paste-availability.v1").is_empty(),
            "{mutation}"
        );
        fs::remove_dir_all(root).unwrap();
    }

    for unsupported in ["missing-copper", "non-smd-copper"] {
        let root = assembly_mask_paste_project("F.Cu", false);
        if unsupported == "missing-copper" {
            fs::remove_file(root.join("copper.gbr")).unwrap();
            let job_path = root.join("complete.gbrjob");
            let mut job: Value = serde_json::from_slice(&fs::read(&job_path).unwrap()).unwrap();
            job["FilesAttributes"]
                .as_array_mut()
                .unwrap()
                .retain(|entry| entry["Path"] != "copper.gbr");
            fs::write(&job_path, serde_json::to_vec(&job).unwrap()).unwrap();
        } else {
            replace_once(
                &root.join("copper.gbr"),
                "%TA.AperFunction,SMDPad,CuDef*%",
                "%TA.AperFunction,Conductor*%",
            );
        }
        let report = review(&root, authority_options(None)).unwrap();
        assert_eq!(
            coverage_for(&report, "assembly.paste-availability.v1").status,
            CoverageStatus::NotRun,
            "{unsupported}: {}",
            coverage_for(&report, "assembly.paste-availability.v1").evidence,
        );
        assert!(findings_for(&report, "assembly.paste-availability.v1").is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}

fn access_copper_layer(second_x_mm: &str) -> String {
    format!(
        "G04 RateMyPCB Plan 07-09 project-authored access fixture*\n%FSLAX46Y46*%\n%MOMM*%\n%TF.FileFunction,Copper,L1,Top*%\n%TA.AperFunction,SMDPad,CuDef*%\n%ADD10C,1.000*%\nD10*\n%TO.N,TP_TEST*%\n%TO.C,TP1*%\n%TO.P,TP1,1*%\nX2000000Y5000000D03*\n%TD.P*%\n%TD.C*%\n%TD.N*%\n%TO.N,OTHER*%\n%TO.C,U2*%\n%TO.P,U2,1*%\nX{second_x_mm}000000Y5000000D03*\nM02*\n"
    )
}

fn access_profile_layer() -> String {
    "G04 RateMyPCB Plan 07-09 exact profile fixture*\n%FSLAX46Y46*%\n%MOMM*%\n%TF.FileFunction,Profile,NP*%\n%TA.AperFunction,Conductor*%\n%ADD10C,0.100*%\nD10*\n%TO.N,PROFILE*%\n%TO.C,BOARD_PROFILE*%\n%TO.P,BOARD_PROFILE,1*%\nG36*\nX000000Y000000D02*\nX10000000Y000000D01*\nX10000000Y10000000D01*\nX000000Y10000000D01*\nX000000Y000000D01*\nG37*\nM02*\n"
        .into()
}

fn access_project(second_x_mm: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "ratemypcb-dfm-access-{}-{}",
        std::process::id(),
        NEXT_POPULATION_FIXTURE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
    fs::write(root.join("copper.gbr"), access_copper_layer(second_x_mm)).unwrap();
    fs::write(root.join("profile.gbr"), access_profile_layer()).unwrap();
    fs::write(
        root.join("complete.gbrjob"),
        serde_json::to_vec(&serde_json::json!({
            "Header": {"GenerationSoftware": {"Vendor": "RateMyPCB", "Application": "fixture", "Version": "1"}},
            "GeneralSpecs": {"ProjectId": {"Name": "phase7-access", "Revision": "r1", "PartNumber": "P7-009"}},
            "FilesAttributes": [
                {"Path": "copper.gbr", "FileFunction": "Copper,L1,Top"},
                {"Path": "profile.gbr", "FileFunction": "Profile,NP"}
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        root.join("board.kicad_pcb"),
        format!(
            "(kicad_pcb (version 20240108)\n  (generator ratemypcb-plan07-09)\n  (title_block (title \"phase7-access\") (rev \"r1\"))\n  (layers (0 \"F.Cu\" signal) (31 \"B.Cu\" signal) (44 \"Edge.Cuts\" user \"Edge.Cuts\"))\n  (net 0 \"\") (net 1 \"TP_TEST\") (net 2 \"OTHER\")\n  (footprint \"Fixture:TP\" (layer \"F.Cu\") (at 2 5 0)\n    (uuid \"11111111-1111-4111-8111-111111111111\")\n    (property \"Reference\" \"TP1\")\n    (attr smd)\n    (pad \"1\" smd circle (at 0 0) (size 1 1) (layers \"F.Cu\") (net 1 \"TP_TEST\")))\n  (footprint \"Fixture:U\" (layer \"F.Cu\") (at {second_x_mm} 5 0)\n    (uuid \"22222222-2222-4222-8222-222222222222\")\n    (property \"Reference\" \"U2\")\n    (attr smd)\n    (pad \"1\" smd circle (at 0 0) (size 1 1) (layers \"F.Cu\") (net 2 \"OTHER\")))\n  (gr_rect (start 0 0) (end 10 10) (layer \"Edge.Cuts\")))\n"
        ),
    )
    .unwrap();
    root
}

fn opposite_side_access_project(target_side: &str) -> PathBuf {
    let root = access_project("2");
    let (target_role, blocker_role, target_layer, blocker_layer) = match target_side {
        "top" => ("Copper,L1,Top", "Copper,L2,Bot", "F.Cu", "B.Cu"),
        "bottom" => ("Copper,L2,Bot", "Copper,L1,Top", "B.Cu", "F.Cu"),
        _ => unreachable!(),
    };
    let target = format!(
        "G04 RateMyPCB Plan 07-09 project-authored access fixture*\n%FSLAX46Y46*%\n%MOMM*%\n%TF.FileFunction,{target_role}*%\n%TA.AperFunction,SMDPad,CuDef*%\n%ADD10C,1.000*%\nD10*\n%TO.N,TP_TEST*%\n%TO.C,TP1*%\n%TO.P,TP1,1*%\nX2000000Y5000000D03*\n%TD.P*%\n%TD.C*%\n%TD.N*%\nM02*\n"
    );
    let blocker = format!(
        "G04 RateMyPCB Plan 07-09 project-authored access fixture*\n%FSLAX46Y46*%\n%MOMM*%\n%TF.FileFunction,{blocker_role}*%\n%TA.AperFunction,SMDPad,CuDef*%\n%ADD10C,1.000*%\nD10*\n%TO.N,OTHER*%\n%TO.C,U2*%\n%TO.P,U2,1*%\nX2000000Y5000000D03*\n%TD.P*%\n%TD.C*%\n%TD.N*%\nM02*\n"
    );
    fs::write(root.join("copper.gbr"), target).unwrap();
    fs::write(root.join("bottom.gbr"), blocker).unwrap();
    fs::write(
        root.join("complete.gbrjob"),
        serde_json::to_vec(&serde_json::json!({
            "Header": {"GenerationSoftware": {"Vendor": "RateMyPCB", "Application": "fixture", "Version": "1"}},
            "GeneralSpecs": {"ProjectId": {"Name": "phase7-access", "Revision": "r1", "PartNumber": "P7-009"}},
            "FilesAttributes": [
                {"Path": "copper.gbr", "FileFunction": target_role},
                {"Path": "bottom.gbr", "FileFunction": blocker_role},
                {"Path": "profile.gbr", "FileFunction": "Profile,NP"}
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    if target_layer != "F.Cu" {
        replace_once(
            &root.join("board.kicad_pcb"),
            "(footprint \"Fixture:TP\" (layer \"F.Cu\")",
            &format!("(footprint \"Fixture:TP\" (layer \"{target_layer}\")"),
        );
        replace_once(
            &root.join("board.kicad_pcb"),
            "(layers \"F.Cu\") (net 1 \"TP_TEST\")",
            &format!("(layers \"{target_layer}\") (net 1 \"TP_TEST\")"),
        );
    }
    if blocker_layer != "F.Cu" {
        replace_once(
            &root.join("board.kicad_pcb"),
            "(footprint \"Fixture:U\" (layer \"F.Cu\")",
            &format!("(footprint \"Fixture:U\" (layer \"{blocker_layer}\")"),
        );
        replace_once(
            &root.join("board.kicad_pcb"),
            "(layers \"F.Cu\") (net 2 \"OTHER\")",
            &format!("(layers \"{blocker_layer}\") (net 2 \"OTHER\")"),
        );
    }
    root
}

fn many_component_access_project(count: usize) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "ratemypcb-dfm-access-many-{}-{}",
        std::process::id(),
        NEXT_POPULATION_FIXTURE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
    let mut copper = "G04 bounded access fixture*\n%FSLAX46Y46*%\n%MOMM*%\n%TF.FileFunction,Copper,L1,Top*%\n%TA.AperFunction,SMDPad,CuDef*%\n%ADD10C,1.000*%\nD10*\n".to_owned();
    let mut board = "(kicad_pcb (version 20240108)\n  (generator ratemypcb-plan07-09)\n  (title_block (title \"phase7-access\") (rev \"r1\"))\n  (layers (0 \"F.Cu\" signal) (31 \"B.Cu\" signal) (44 \"Edge.Cuts\" user \"Edge.Cuts\"))\n  (net 0 \"\")\n".to_owned();
    for index in 0..count {
        copper.push_str(&format!(
            "%TO.N,N{index}*%\n%TO.C,U{index}*%\n%TO.P,U{index},1*%\nX5000000Y5000000D03*\n%TD.P*%\n%TD.C*%\n%TD.N*%\n"
        ));
        board.push_str(&format!(
            "  (net {} \"N{index}\")\n  (footprint \"Fixture:U\" (layer \"F.Cu\") (at 5 5 0)\n    (uuid \"00000000-0000-4000-8000-{index:012x}\")\n    (property \"Reference\" \"U{index}\")\n    (attr smd)\n    (pad \"1\" smd circle (at 0 0) (size 1 1) (layers \"F.Cu\") (net {} \"N{index}\")))\n",
            index + 1,
            index + 1,
        ));
    }
    copper.push_str("M02*\n");
    board.push_str("  (gr_rect (start 0 0) (end 10 10) (layer \"Edge.Cuts\")))\n");
    fs::write(root.join("copper.gbr"), copper).unwrap();
    fs::write(root.join("profile.gbr"), access_profile_layer()).unwrap();
    fs::write(root.join("board.kicad_pcb"), board).unwrap();
    fs::write(
        root.join("complete.gbrjob"),
        serde_json::to_vec(&serde_json::json!({
            "Header": {"GenerationSoftware": {"Vendor": "RateMyPCB", "Application": "fixture", "Version": "1"}},
            "GeneralSpecs": {"ProjectId": {"Name": "phase7-access", "Revision": "r1", "PartNumber": "P7-009"}},
            "FilesAttributes": [
                {"Path": "copper.gbr", "FileFunction": "Copper,L1,Top"},
                {"Path": "profile.gbr", "FileFunction": "Profile,NP"}
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    root
}

fn inference_declaration_value(mut records: Vec<Value>) -> Value {
    for (index, record) in records.iter_mut().enumerate() {
        record["record"] = Value::from(index + 1);
    }
    serde_json::json!({
        "schemaVersion": "1",
        "producer": "ratemypcb-project-authority",
        "producerVersion": "2026.08",
        "issuedAtUnix": 1700000000_u64,
        "expiresAtUnix": 4102444800_u64,
        "state": "complete",
        "rules": [],
        "orderAcknowledgements": [],
        "inferenceRecords": records
    })
}

fn inference_declarations(records: Vec<Value>) -> DfmDeclarations {
    declarations_from(&inference_declaration_value(records)).unwrap()
}

fn inference_limit(id: &str, value: &str, unit: &str) -> Value {
    serde_json::json!({"id": id, "value": value, "unit": unit})
}

fn process_envelope(tool: &str, component: &str, profile: &str) -> Value {
    serde_json::json!({
        "record": 1,
        "id": "assembly_process_envelope",
        "state": "complete",
        "model": "assembly.component-copper-envelope-2d",
        "modelVersion": "1",
        "applicability": "board",
        "targetIds": [],
        "limits": [
            inference_limit("minimum_component_clearance", component, "mm"),
            inference_limit("minimum_profile_clearance", profile, "mm"),
            inference_limit("tool_diameter", tool, "mm")
        ],
        "parameters": [
            {"id": "process", "value": "pick_and_place"},
            {"id": "process_version", "value": "2026.1"},
            {"id": "tool", "value": "nozzle_n1"},
            {"id": "tool_version", "value": "1"}
        ]
    })
}

fn probe_envelope(probe: &str, component: &str, profile: &str) -> Value {
    serde_json::json!({
        "record": 1,
        "id": "probe_envelope",
        "state": "complete",
        "model": "assembly.testpoint-probe-envelope-2d",
        "modelVersion": "1",
        "applicability": "board",
        "targetIds": [],
        "limits": [
            inference_limit("minimum_component_clearance", component, "mm"),
            inference_limit("minimum_profile_clearance", profile, "mm"),
            inference_limit("probe_diameter", probe, "mm")
        ],
        "parameters": [
            {"id": "probe", "value": "spring_probe_p80"},
            {"id": "probe_version", "value": "1"},
            {"id": "process", "value": "flying_probe"},
            {"id": "process_version", "value": "2026.1"}
        ]
    })
}

fn canonical_net_id(report: &Report, source_name: &str) -> String {
    let mut features = report
        .fabrication
        .connectivity
        .iter()
        .filter(|semantic| semantic.net.as_deref() == Some(source_name))
        .map(|semantic| semantic.feature_id.as_str())
        .collect::<Vec<_>>();
    features.sort();
    features.dedup();
    assert!(!features.is_empty());
    let bytes = serde_json::to_vec(&("dfm-canonical-net-v1", features)).unwrap();
    format!("net-v1-{:x}", Sha256::digest(bytes))
}

fn target_net_authority(targets: Vec<String>) -> Value {
    serde_json::json!({
        "record": 1,
        "id": "target_net_authority",
        "state": "complete",
        "model": "canonical.connectivity-net-set",
        "modelVersion": "1",
        "applicability": "board",
        "targetIds": targets,
        "limits": [],
        "parameters": []
    })
}

#[test]
fn assembly_access_compares_complete_geometry_to_named_process_tool_envelopes_only() {
    let root = access_project("5");
    let absent = review(&root, authority_options(None)).unwrap();
    assert_eq!(
        coverage_for(&absent, "assembly.access.v1").status,
        CoverageStatus::NotRun
    );

    let clean = review(
        &root,
        authority_options(Some(inference_declarations(vec![process_envelope(
            "1.00", "1.50", "0.50",
        )]))),
    )
    .unwrap();
    validate_report(&clean).unwrap();
    assert_eq!(
        coverage_for(&clean, "assembly.access.v1").status,
        CoverageStatus::Passed,
        "{}",
        coverage_for(&clean, "assembly.access.v1").evidence,
    );
    assert!(findings_for(&clean, "assembly.access.v1").is_empty());
    assert!(
        coverage_for(&clean, "assembly.access.v1")
            .evidence
            .contains("model=assembly.component-copper-envelope-2d@1")
    );

    let attention = review(
        &root,
        authority_options(Some(inference_declarations(vec![process_envelope(
            "1.00", "1.60", "1.10",
        )]))),
    )
    .unwrap();
    validate_report(&attention).unwrap();
    let findings = findings_for(&attention, "assembly.access.v1");
    assert_eq!(findings.len(), 2);
    assert!(findings.iter().all(|finding| {
        finding.gate_impact == GateImpact::EvidenceOnly
            && finding.evidence.contains("inference=true")
            && finding.evidence.contains("process=pick_and_place@2026.1")
            && finding.evidence.contains("tool=nozzle_n1@1")
            && finding.evidence.contains("declaration_source=")
            && finding.evidence.contains("geometry_source=")
    }));
    let repeated = review(
        &root,
        authority_options(Some(inference_declarations(vec![process_envelope(
            "1.00", "1.60", "1.10",
        )]))),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(coverage_for(&attention, "assembly.access.v1")).unwrap(),
        serde_json::to_value(coverage_for(&repeated, "assembly.access.v1")).unwrap()
    );
    assert_eq!(
        findings_for(&attention, "assembly.access.v1")
            .into_iter()
            .map(|finding| serde_json::to_value(finding).unwrap())
            .collect::<Vec<_>>(),
        findings_for(&repeated, "assembly.access.v1")
            .into_iter()
            .map(|finding| serde_json::to_value(finding).unwrap())
            .collect::<Vec<_>>()
    );
    assert!(attention.required_evidence.iter().all(|required| {
        required.check_id != "assembly.access.v1"
            && required.check_id != "assembly.testpoint-access.v1"
    }));
    let mut forged = attention.clone();
    forged
        .findings
        .iter_mut()
        .find(|finding| {
            occurrence_check_id(&attention, &finding.id)
                .is_some_and(|check_id| check_id.starts_with("assembly.access.v1/"))
        })
        .unwrap()
        .gate_impact = GateImpact::Blocking;
    assert!(validate_report(&forged).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn assembly_access_and_testpoint_access_ignore_opposite_side_coincident_geometry() {
    for target_side in ["top", "bottom"] {
        let root = opposite_side_access_project(target_side);
        let baseline = review(&root, authority_options(None)).unwrap();
        let target = canonical_net_id(&baseline, "TP_TEST");
        let report = review(
            &root,
            authority_options(Some(inference_declarations(vec![
                process_envelope("1.00", "0.10", "0.10"),
                probe_envelope("0.40", "0.10", "0.10"),
                target_net_authority(vec![target]),
            ]))),
        )
        .unwrap();
        for family in ["assembly.access.v1", "assembly.testpoint-access.v1"] {
            assert_eq!(
                coverage_for(&report, family).status,
                CoverageStatus::Passed,
                "{target_side} {family}: {}",
                coverage_for(&report, family).evidence
            );
            assert!(findings_for(&report, family).is_empty());
        }
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn assembly_access_profile_membership_fails_closed_for_concave_exterior_cutout_and_outside() {
    let cases = [
        (
            "concave",
            "G04 concave profile*\n%FSLAX46Y46*%\n%MOMM*%\n%TF.FileFunction,Profile,NP*%\n%TA.AperFunction,Conductor*%\n%ADD10C,0.100*%\nD10*\n%TO.N,PROFILE*%\n%TO.C,BOARD_PROFILE*%\n%TO.P,BOARD_PROFILE,1*%\nG36*\nX000000Y000000D02*\nX10000000Y000000D01*\nX10000000Y4000000D01*\nX4000000Y4000000D01*\nX4000000Y10000000D01*\nX000000Y10000000D01*\nX000000Y000000D01*\nG37*\nM02*\n",
        ),
        (
            "cutout",
            "G04 cutout profile*\n%FSLAX46Y46*%\n%MOMM*%\n%TF.FileFunction,Profile,NP*%\n%TA.AperFunction,Conductor*%\n%ADD10C,0.100*%\nD10*\n%TO.N,PROFILE*%\n%TO.C,BOARD_PROFILE*%\n%TO.P,BOARD_PROFILE,1*%\nG36*\nX000000Y000000D02*\nX10000000Y000000D01*\nX10000000Y10000000D01*\nX000000Y10000000D01*\nX000000Y000000D01*\nG37*\n%LPC*%\nG36*\nX4500000Y4500000D02*\nX5500000Y4500000D01*\nX5500000Y5500000D01*\nX4500000Y5500000D01*\nX4500000Y4500000D01*\nG37*\nM02*\n",
        ),
    ];
    for (case, profile) in cases {
        let root = access_project("5");
        let baseline = review(&root, authority_options(None)).unwrap();
        let target = canonical_net_id(&baseline, "TP_TEST");
        fs::write(root.join("profile.gbr"), profile).unwrap();
        let report = review(
            &root,
            authority_options(Some(inference_declarations(vec![
                process_envelope("1.00", "0.10", "0.10"),
                probe_envelope("0.40", "0.10", "0.10"),
                target_net_authority(vec![target]),
            ]))),
        )
        .unwrap();
        for family in ["assembly.access.v1", "assembly.testpoint-access.v1"] {
            assert_eq!(
                coverage_for(&report, family).status,
                CoverageStatus::NotRun,
                "{case} {family}: {}",
                coverage_for(&report, family).evidence
            );
            assert!(findings_for(&report, family).is_empty());
        }
        fs::remove_dir_all(root).unwrap();
    }

    let root = access_project("11");
    let baseline = review(&root, authority_options(None)).unwrap();
    let target = canonical_net_id(&baseline, "TP_TEST");
    let report = review(
        &root,
        authority_options(Some(inference_declarations(vec![
            process_envelope("1.00", "0.10", "0.10"),
            probe_envelope("0.40", "0.10", "0.10"),
            target_net_authority(vec![target]),
        ]))),
    )
    .unwrap();
    for family in ["assembly.access.v1", "assembly.testpoint-access.v1"] {
        assert_eq!(
            coverage_for(&report, family).status,
            CoverageStatus::NotRun,
            "outside {family}: {}",
            coverage_for(&report, family).evidence
        );
        assert!(findings_for(&report, family).is_empty());
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn assembly_access_generated_output_limits_are_atomic_and_near_limit() {
    let near_root = many_component_access_project(15);
    let near = review(
        &near_root,
        authority_options(Some(inference_declarations(vec![process_envelope(
            "1.00", "0.10", "0.10",
        )]))),
    )
    .unwrap();
    assert_eq!(
        coverage_for(&near, "assembly.access.v1").status,
        CoverageStatus::Attention,
        "{}",
        coverage_for(&near, "assembly.access.v1").evidence
    );
    assert_eq!(findings_for(&near, "assembly.access.v1").len(), 105);
    assert!(
        coverage_for(&near, "assembly.access.v1")
            .evidence
            .contains("observations=120")
    );

    let over_root = many_component_access_project(16);
    let over = review(
        &over_root,
        authority_options(Some(inference_declarations(vec![process_envelope(
            "1.00", "0.10", "0.10",
        )]))),
    )
    .unwrap();
    assert_eq!(
        coverage_for(&over, "assembly.access.v1").status,
        CoverageStatus::NotRun
    );
    assert!(
        coverage_for(&over, "assembly.access.v1")
            .evidence
            .contains("inference observation limit 128 exceeded")
    );
    assert!(findings_for(&over, "assembly.access.v1").is_empty());
    fs::remove_dir_all(near_root).unwrap();
    fs::remove_dir_all(over_root).unwrap();
}

#[test]
fn assembly_access_missing_profile_placement_or_component_geometry_is_not_checked() {
    for mutation in ["profile", "placement", "component-geometry"] {
        let root = access_project("5");
        match mutation {
            "profile" => fs::remove_file(root.join("profile.gbr")).unwrap(),
            "placement" => replace_once(
                &root.join("board.kicad_pcb"),
                "    (property \"Reference\" \"U2\")",
                "    (property \"Reference\" \"\")",
            ),
            "component-geometry" => replace_once(
                &root.join("copper.gbr"),
                "X5000000Y5000000D03*",
                "X5000000Y5000000D02*",
            ),
            _ => unreachable!(),
        }
        let report = review(
            &root,
            authority_options(Some(inference_declarations(vec![process_envelope(
                "1.00", "1.50", "0.50",
            )]))),
        )
        .unwrap();
        assert_eq!(
            coverage_for(&report, "assembly.access.v1").status,
            CoverageStatus::NotRun,
            "{mutation}: {}",
            coverage_for(&report, "assembly.access.v1").evidence
        );
        assert!(findings_for(&report, "assembly.access.v1").is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn testpoint_access_requires_canonical_target_ids_and_probe_process_envelope() {
    let root = access_project("5");
    let baseline = review(&root, authority_options(None)).unwrap();
    let target = canonical_net_id(&baseline, "TP_TEST");

    let clean = review(
        &root,
        authority_options(Some(inference_declarations(vec![
            probe_envelope("0.40", "1.50", "0.50"),
            target_net_authority(vec![target.clone()]),
        ]))),
    )
    .unwrap();
    validate_report(&clean).unwrap();
    assert_eq!(
        coverage_for(&clean, "assembly.testpoint-access.v1").status,
        CoverageStatus::Passed
    );
    assert!(findings_for(&clean, "assembly.testpoint-access.v1").is_empty());
    let evidence = &coverage_for(&clean, "assembly.testpoint-access.v1").evidence;
    assert!(evidence.contains(&target));
    assert!(evidence.contains("probe=spring_probe_p80@1"));
    assert!(evidence.contains("process=flying_probe@2026.1"));

    let attention = review(
        &root,
        authority_options(Some(inference_declarations(vec![
            probe_envelope("0.40", "1.90", "1.40"),
            target_net_authority(vec![target.clone()]),
        ]))),
    )
    .unwrap();
    validate_report(&attention).unwrap();
    let findings = findings_for(&attention, "assembly.testpoint-access.v1");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].gate_impact, GateImpact::EvidenceOnly);
    assert!(findings[0].evidence.contains("target_authority_source="));
    assert!(findings[0].evidence.contains("connectivity_source="));
    let repeated = review(
        &root,
        authority_options(Some(inference_declarations(vec![
            probe_envelope("0.40", "1.90", "1.40"),
            target_net_authority(vec![target.clone()]),
        ]))),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(coverage_for(&attention, "assembly.testpoint-access.v1")).unwrap(),
        serde_json::to_value(coverage_for(&repeated, "assembly.testpoint-access.v1")).unwrap()
    );
    assert_eq!(
        findings_for(&attention, "assembly.testpoint-access.v1")
            .into_iter()
            .map(|finding| serde_json::to_value(finding).unwrap())
            .collect::<Vec<_>>(),
        findings_for(&repeated, "assembly.testpoint-access.v1")
            .into_iter()
            .map(|finding| serde_json::to_value(finding).unwrap())
            .collect::<Vec<_>>()
    );
    let finding_id = findings[0].id.clone();
    let mut forged = attention.clone();
    forged
        .findings
        .iter_mut()
        .find(|finding| finding.id == finding_id)
        .unwrap()
        .gate_impact = GateImpact::Blocking;
    assert!(validate_report(&forged).is_err());

    for records in [
        vec![probe_envelope("0.40", "1.50", "0.50")],
        vec![target_net_authority(vec![target.clone()])],
        vec![
            probe_envelope("0.40", "1.50", "0.50"),
            target_net_authority(vec![format!("net-v1-{}", "0".repeat(64))]),
        ],
    ] {
        let report = review(
            &root,
            authority_options(Some(inference_declarations(records))),
        )
        .unwrap();
        assert_eq!(
            coverage_for(&report, "assembly.testpoint-access.v1").status,
            CoverageStatus::NotRun
        );
        assert!(findings_for(&report, "assembly.testpoint-access.v1").is_empty());
    }

    let names_only = review(&root, authority_options(None)).unwrap();
    assert_eq!(
        coverage_for(&names_only, "assembly.testpoint-access.v1").status,
        CoverageStatus::NotRun
    );
    assert!(findings_for(&names_only, "assembly.testpoint-access.v1").is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn testpoint_access_incomplete_connectivity_component_pin_placement_or_profile_is_not_checked() {
    let baseline_root = access_project("5");
    let baseline = review(&baseline_root, authority_options(None)).unwrap();
    let target = canonical_net_id(&baseline, "TP_TEST");
    fs::remove_dir_all(baseline_root).unwrap();

    for mutation in ["connectivity", "component", "pin", "placement", "profile"] {
        let root = access_project("5");
        match mutation {
            "connectivity" => replace_once(&root.join("copper.gbr"), "%TO.N,TP_TEST*%\n", ""),
            "component" => {
                replace_once(&root.join("copper.gbr"), "%TO.C,TP1*%\n", "");
                replace_once(&root.join("copper.gbr"), "%TO.P,TP1,1*%\n", "");
            }
            "pin" => replace_once(&root.join("copper.gbr"), "%TO.P,TP1,1*%\n", ""),
            "placement" => replace_once(
                &root.join("board.kicad_pcb"),
                "    (property \"Reference\" \"TP1\")",
                "    (property \"Reference\" \"\")",
            ),
            "profile" => fs::remove_file(root.join("profile.gbr")).unwrap(),
            _ => unreachable!(),
        }
        let report = review(
            &root,
            authority_options(Some(inference_declarations(vec![
                probe_envelope("0.40", "1.50", "0.50"),
                target_net_authority(vec![target.clone()]),
            ]))),
        )
        .unwrap();
        let coverage = coverage_for(&report, "assembly.testpoint-access.v1");
        assert_eq!(
            coverage.status,
            CoverageStatus::NotRun,
            "{mutation}: {}",
            coverage.evidence
        );
        assert!(findings_for(&report, "assembly.testpoint-access.v1").is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn dfm03_family_matrix_covers_population_and_all_bounded_assembly_families() {
    let manifest: Manifest = serde_json::from_str(MANIFEST_JSON).unwrap();
    let dfm03 = manifest
        .families
        .iter()
        .filter(|family| family.requirement == "DFM-03")
        .map(|family| family_key(&family.family_id, &family.family_version))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        dfm03,
        BTreeSet::from([
            "assembly.access.v1".into(),
            "assembly.courtyard-native.v1".into(),
            "assembly.footprint-string-parity.v1".into(),
            "assembly.paste-availability.v1".into(),
            "assembly.population-parity.v1".into(),
            "assembly.side-rotation.v1".into(),
            "assembly.testpoint-access.v1".into(),
        ])
    );

    let root = access_project("5");
    let report = review(&root, authority_options(None)).unwrap();
    let observed = report
        .coverage
        .iter()
        .filter_map(|coverage| occurrence_check_id(&report, &coverage.id))
        .filter(|check_id| dfm03.contains(*check_id))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        observed,
        dfm03.iter().map(String::as_str).collect::<BTreeSet<_>>()
    );
    fs::remove_dir_all(root).unwrap();
}

fn geometry_report(minimum_drill: Option<&str>) -> (PathBuf, Report) {
    let root = geometry_package();
    let report = review(
        &root,
        authority_options(minimum_drill.map(geometry_declarations)),
    )
    .unwrap();
    (root, report)
}

fn coverage_for<'a>(report: &'a Report, check_id: &str) -> &'a Coverage {
    report
        .coverage
        .iter()
        .find(|coverage| occurrence_check_id(report, &coverage.id) == Some(check_id))
        .unwrap()
}

fn findings_for<'a>(report: &'a Report, family: &str) -> Vec<&'a Finding> {
    report
        .findings
        .iter()
        .filter(|finding| {
            occurrence_check_id(report, &finding.id)
                .is_some_and(|check_id| check_id.starts_with(family))
        })
        .collect()
}

struct InferenceMeasurements {
    labels: BTreeMap<String, BTreeMap<String, String>>,
    mutations: BTreeMap<String, Vec<MutationMeasurement>>,
}

fn measured_inference_corpus(corpus: &GeometryCorpus) -> InferenceMeasurements {
    fn label(report: &Report, family: &str, kind: Option<&str>) -> String {
        let finding = findings_for(report, family).iter().any(|finding| {
            kind.is_none_or(|kind| {
                occurrence_check_id(report, &finding.id)
                    .is_some_and(|check_id| check_id.contains(kind))
            })
        });
        if finding { "finding" } else { "no_finding" }.into()
    }

    fn report(
        second_x_mm: &str,
        process: Option<Value>,
        probe: Option<Value>,
    ) -> (PathBuf, Report) {
        let root = access_project(second_x_mm);
        let baseline = review(&root, authority_options(None)).unwrap();
        let target = canonical_net_id(&baseline, "TP_TEST");
        let mut records = Vec::new();
        if let Some(process) = process {
            records.push(process);
        }
        if let Some(probe) = probe {
            records.push(probe);
            records.push(target_net_authority(vec![target]));
        }
        let report = review(
            &root,
            authority_options(Some(inference_declarations(records))),
        )
        .unwrap();
        validate_report(&report).unwrap();
        (root, report)
    }

    let mut labels = BTreeMap::new();
    let mut access = BTreeMap::new();
    for (case, second_x, component, profile, kind) in [
        (
            "component-envelope-obstruction",
            "3",
            "0.60",
            "0.10",
            Some("/component/"),
        ),
        (
            "profile-envelope-obstruction",
            "5",
            "0.10",
            "1.10",
            Some("/profile/"),
        ),
        ("exact-component-envelope", "4", "0.50", "0.10", None),
        ("safe-profile-envelope", "5", "0.10", "1.00", None),
    ] {
        let (root, measured) = report(
            second_x,
            Some(process_envelope("1.00", component, profile)),
            None,
        );
        access.insert(case.into(), label(&measured, "assembly.access.v1", kind));
        fs::remove_dir_all(root).unwrap();
    }
    labels.insert("assembly.access.v1".into(), access);

    let mut testpoint = BTreeMap::new();
    for (case, second_x, component, profile) in [
        ("component-obstructed-target", "3", "0.30", "0.10"),
        ("profile-obstructed-target", "5", "0.10", "1.40"),
        ("explicit-accessible-target", "5", "0.10", "0.10"),
        ("tp-like-names-do-not-add-targets", "5", "0.10", "0.10"),
    ] {
        let (root, measured) = report(
            second_x,
            None,
            Some(probe_envelope("0.40", component, profile)),
        );
        testpoint.insert(
            case.into(),
            label(&measured, "assembly.testpoint-access.v1", None),
        );
        fs::remove_dir_all(root).unwrap();
    }
    labels.insert("assembly.testpoint-access.v1".into(), testpoint);

    let mut mutations = BTreeMap::new();
    for family in corpus.families.iter().filter(|family| {
        INFERENCE_FAMILIES.contains(&family_key(&family.family_id, &family.family_version).as_str())
    }) {
        let key = family_key(&family.family_id, &family.family_version);
        let root = access_project("5");
        let baseline = review(&root, authority_options(None)).unwrap();
        let target = canonical_net_id(&baseline, "TP_TEST");
        let records = vec![
            process_envelope("1.00", "0.10", "0.10"),
            probe_envelope("0.40", "0.10", "0.10"),
            target_net_authority(vec![target]),
        ];
        let baseline_input = inference_declaration_value(records.clone());
        let baseline_report = review(
            &root,
            authority_options(Some(declarations_from(&baseline_input).unwrap())),
        )
        .unwrap();
        validate_report(&baseline_report).unwrap();
        let snapshot = |report: &Report| {
            let coverage = coverage_for(report, &key);
            let findings = findings_for(report, &key)
                .into_iter()
                .map(|finding| {
                    (
                        occurrence_check_id(report, &finding.id).unwrap().to_owned(),
                        finding.severity,
                        finding.title.clone(),
                        finding.location.clone(),
                        finding.gate_impact.clone(),
                    )
                })
                .collect::<Vec<_>>();
            (coverage.status.clone(), findings)
        };
        let record_id = match key.as_str() {
            "assembly.access.v1" => "assembly_process_envelope",
            "assembly.testpoint-access.v1" => "probe_envelope",
            _ => unreachable!(),
        };
        let mut measurements = Vec::new();
        for mutation in &family.mutations {
            let mut transformed_records = records.clone();
            let record_index = transformed_records
                .iter()
                .position(|record| record["id"] == record_id)
                .unwrap();
            match mutation.kind.as_str() {
                "affected_conflict" => {
                    let mut conflicting = transformed_records[record_index].clone();
                    conflicting["limits"][0]["value"] = Value::String("9.00".into());
                    transformed_records.push(conflicting);
                }
                "affected_omission" => {
                    transformed_records[record_index]["limits"]
                        .as_array_mut()
                        .unwrap()
                        .remove(0);
                }
                "dangling_identity" => {
                    let index = if key == "assembly.testpoint-access.v1" {
                        transformed_records
                            .iter()
                            .position(|record| record["id"] == "target_net_authority")
                            .unwrap()
                    } else {
                        record_index
                    };
                    transformed_records[index]["targetIds"] =
                        serde_json::json!([format!("net-v1-{}", "0".repeat(64))]);
                }
                "duplicate_capability" => {
                    transformed_records.push(transformed_records[record_index].clone());
                }
                "missing_prerequisite" => {
                    transformed_records.remove(record_index);
                }
                "reordered_facts" => transformed_records.reverse(),
                "resolution_changed" => {
                    transformed_records[record_index]["limits"][0]["value"] =
                        Value::String("0.0000001".into());
                }
                "state_failed" => {
                    transformed_records[record_index]["state"] = Value::String("failed".into());
                }
                "state_not_provided" => {
                    transformed_records[record_index]["state"] =
                        Value::String("not_provided".into());
                }
                "state_omitted" => {
                    transformed_records[record_index]["state"] = Value::String("omitted".into());
                }
                "state_partial" => {
                    transformed_records[record_index]["state"] = Value::String("partial".into());
                }
                "state_stale" => {
                    transformed_records[record_index]["state"] = Value::String("stale".into());
                }
                "state_unsupported" => {
                    transformed_records[record_index]["state"] =
                        Value::String("unsupported".into());
                }
                "unit_changed" => {
                    transformed_records[record_index]["limits"][0]["unit"] =
                        Value::String("mil".into());
                }
                _ => unreachable!(),
            }
            let transformed_input = inference_declaration_value(transformed_records);
            let input_changed = transformed_input != baseline_input;
            let status = match declarations_from(&transformed_input) {
                Ok(declarations) => {
                    let report = review(&root, authority_options(Some(declarations))).unwrap();
                    validate_report(&report).unwrap();
                    if mutation.kind == "reordered_facts" {
                        if snapshot(&baseline_report) == snapshot(&report) {
                            "not_checked"
                        } else {
                            "changed"
                        }
                    } else if coverage_for(&report, &key).status == CoverageStatus::NotRun
                        && findings_for(&report, &key).is_empty()
                    {
                        "not_checked"
                    } else {
                        "partial_pass"
                    }
                }
                Err(_) => "not_checked",
            };
            measurements.push(MutationMeasurement {
                case_id: mutation.id.clone(),
                kind: mutation.kind.clone(),
                input_changed,
                status: status.into(),
            });
        }
        fs::remove_dir_all(root).unwrap();
        mutations.insert(key, measurements);
    }
    InferenceMeasurements { labels, mutations }
}

#[test]
fn assembly_corpus_metrics_use_production_measurements() {
    let mut corpus: GeometryCorpus = serde_json::from_str(ASSEMBLY_TARGETS_JSON).unwrap();
    for family in &mut corpus.families {
        let key = family_key(&family.family_id, &family.family_version);
        if INFERENCE_FAMILIES.contains(&key.as_str()) {
            for target in &mut family.targets {
                target.actual_label = Some("forged".into());
            }
        }
    }
    let mut measured = measured_inference_corpus(&corpus);
    let metrics = validate_assembly_corpus(&corpus, &measured.labels, &measured.mutations).unwrap();
    assert_eq!(metrics["assembly.access.v1"].tp, 2);

    measured
        .labels
        .get_mut("assembly.access.v1")
        .unwrap()
        .insert("component-envelope-obstruction".into(), "no_finding".into());
    let regressed =
        validate_assembly_corpus(&corpus, &measured.labels, &measured.mutations).unwrap();
    assert_eq!(regressed["assembly.access.v1"].fn_count, 1);

    measured
        .mutations
        .get_mut("assembly.access.v1")
        .unwrap()
        .iter_mut()
        .find(|measurement| measurement.kind == "reordered_facts")
        .unwrap()
        .status = "changed".into();
    assert!(validate_assembly_corpus(&corpus, &measured.labels, &measured.mutations).is_err());
}

#[test]
fn inference_mutation_measurements_are_sensitive_to_non_reordering_transformations() {
    let corpus: GeometryCorpus = serde_json::from_str(ASSEMBLY_TARGETS_JSON).unwrap();
    let mut measured = measured_inference_corpus(&corpus);
    let state_failed_index = measured.mutations["assembly.access.v1"]
        .iter()
        .position(|measurement| measurement.kind == "state_failed")
        .unwrap();
    assert!(measured.mutations["assembly.access.v1"][state_failed_index].input_changed);
    assert_eq!(
        measured.mutations["assembly.access.v1"][state_failed_index].status,
        "not_checked"
    );

    measured.mutations.get_mut("assembly.access.v1").unwrap()[state_failed_index].input_changed =
        false;
    assert!(validate_assembly_corpus(&corpus, &measured.labels, &measured.mutations).is_err());

    let state_failed =
        &mut measured.mutations.get_mut("assembly.access.v1").unwrap()[state_failed_index];
    state_failed.input_changed = true;
    state_failed.kind = "state_partial".into();
    assert!(validate_assembly_corpus(&corpus, &measured.labels, &measured.mutations).is_err());

    let state_failed =
        &mut measured.mutations.get_mut("assembly.access.v1").unwrap()[state_failed_index];
    state_failed.kind = "state_failed".into();
    state_failed.status = "partial_pass".into();
    assert!(validate_assembly_corpus(&corpus, &measured.labels, &measured.mutations).is_err());
}

#[test]
fn native_assembly_corpus_and_mutations_are_qualified_but_evidence_only() {
    let corpus: GeometryCorpus = serde_json::from_str(ASSEMBLY_TARGETS_JSON).unwrap();
    let measured = measured_inference_corpus(&corpus);
    let metrics = validate_assembly_corpus(&corpus, &measured.labels, &measured.mutations).unwrap();
    assert_eq!(metrics.len(), 6);
    for (family, expected) in [
        ("assembly.side-rotation.v1", (2, 2)),
        ("assembly.paste-availability.v1", (1, 2)),
        ("assembly.courtyard-native.v1", (3, 2)),
        ("assembly.footprint-string-parity.v1", (2, 2)),
        ("assembly.access.v1", (2, 2)),
        ("assembly.testpoint-access.v1", (2, 2)),
    ] {
        let metrics = metrics[family];
        assert_eq!((metrics.tp, metrics.tn), expected, "{family}");
        assert_eq!((metrics.fp, metrics.fn_count), (0, 0), "{family}");
        assert_eq!(metrics.precision, Some(1.0), "{family}");
        assert_eq!(metrics.recall, Some(1.0), "{family}");
        assert_eq!(metrics.not_checked_mutations, REQUIRED_MUTATIONS.len());
        println!(
            "DFM QUALIFICATION family={family} tp={} fp={} fn={} tn={} precision=1.000 recall=1.000 not_checked_mutations={} gate=evidence_only review=pending",
            metrics.tp, metrics.fp, metrics.fn_count, metrics.tn, metrics.not_checked_mutations,
        );
    }
}

#[test]
fn construction_stackup_corpus_and_production_gap_path_are_fail_closed() {
    let corpus: GeometryCorpus = serde_json::from_str(CONSTRUCTION_TARGETS_JSON).unwrap();
    let metrics = validate_construction_corpus(&corpus).unwrap();
    assert_eq!(metrics.len(), 5);
    for (family, expected) in [
        ("dfm.stackup-order-confirmation.v1", (1, 1)),
        ("dfm.total-thickness-material.v1", (2, 1)),
    ] {
        let metrics = metrics[family];
        assert_eq!((metrics.tp, metrics.tn), expected, "{family}");
        assert_eq!((metrics.fp, metrics.fn_count), (0, 0), "{family}");
        assert_eq!(metrics.precision, Some(1.0), "{family}");
        assert_eq!(metrics.recall, Some(1.0), "{family}");
        assert_eq!(metrics.not_checked_mutations, REQUIRED_MUTATIONS.len());
    }

    let absent = review(&fixture("kicad/mismatch"), authority_options(None)).unwrap();
    let declared = review(
        &fixture("kicad/mismatch"),
        authority_options(Some(declarations_from(&declaration_value()).unwrap())),
    )
    .unwrap();
    for report in [&absent, &declared] {
        validate_report(report).unwrap();
        for family in [
            "dfm.stackup-order-confirmation.v1",
            "dfm.total-thickness-material.v1",
        ] {
            let coverage = coverage_for(report, family);
            assert_eq!(coverage.status, CoverageStatus::NotRun, "{family}");
            assert!(coverage.evidence.starts_with("not_checked:"), "{family}");
            let findings = findings_for(report, family);
            assert_eq!(findings.len(), 1, "{family}");
            assert!(findings[0].evidence.contains("outcome=confirmation_gap"));
            assert_eq!(findings[0].gate_impact, GateImpact::EvidenceOnly);
        }
    }
    assert!(
        findings_for(&absent, "dfm.stackup-order-confirmation.v1")[0]
            .evidence
            .contains("declaration_source=not_provided")
    );
    assert!(
        findings_for(&declared, "dfm.stackup-order-confirmation.v1")[0]
            .evidence
            .contains("dfm/declarations.json")
    );
    for report in [&absent, &declared] {
        assert!(report.required_evidence.iter().all(|required| {
            !required
                .check_id
                .starts_with("dfm.stackup-order-confirmation")
                && !required
                    .check_id
                    .starts_with("dfm.total-thickness-material")
        }));
    }
}

#[test]
fn construction_drill_span_acknowledgement_is_per_tool_confirmation_gap_only() {
    let corpus: GeometryCorpus = serde_json::from_str(CONSTRUCTION_TARGETS_JSON).unwrap();
    let metrics = validate_construction_corpus(&corpus).unwrap();
    let metrics = metrics["dfm.drill-span-plating.v1"];
    assert_eq!((metrics.tp, metrics.tn), (1, 1));
    assert_eq!((metrics.fp, metrics.fn_count), (0, 0));
    assert_eq!(metrics.precision, Some(1.0));
    assert_eq!(metrics.recall, Some(1.0));
    assert_eq!(metrics.not_checked_mutations, REQUIRED_MUTATIONS.len());

    let (declared_root, declared) = geometry_report(Some("0.600"));
    let (absent_root, absent) = geometry_report(None);
    for report in [&declared, &absent] {
        validate_report(report).unwrap();
        let coverage = coverage_for(report, "dfm.drill-span-plating.v1");
        assert_eq!(coverage.status, CoverageStatus::NotRun);
        assert!(coverage.evidence.starts_with("not_checked:"));
        let findings = findings_for(report, "dfm.drill-span-plating.v1");
        assert!(!findings.is_empty());
        assert!(findings.iter().all(|finding| {
            finding.gate_impact == GateImpact::EvidenceOnly
                && finding.id != "match"
                && finding.id != "conflict"
                && !finding.title.contains("matches")
                && !finding.title.contains("conflicts")
        }));
        assert!(
            report
                .required_evidence
                .iter()
                .all(|required| { !required.check_id.starts_with("dfm.drill-span-plating") })
        );
    }
    assert!(
        findings_for(&declared, "dfm.drill-span-plating.v1")
            .iter()
            .any(|finding| finding.evidence.contains("confirm with fabricator"))
    );
    assert!(
        findings_for(&absent, "dfm.drill-span-plating.v1")
            .iter()
            .any(|finding| finding.evidence.contains("acknowledgement is absent"))
    );
    fs::remove_dir_all(declared_root).unwrap();
    fs::remove_dir_all(absent_root).unwrap();
}

#[test]
fn construction_dfm02_matrix_distinguishes_comparisons_from_deferred_gaps() {
    let corpus: GeometryCorpus = serde_json::from_str(CONSTRUCTION_TARGETS_JSON).unwrap();
    let metrics = validate_construction_corpus(&corpus).unwrap();
    for (family, expected) in [
        ("dfm.stackup-order-confirmation.v1", (1, 1)),
        ("dfm.total-thickness-material.v1", (2, 1)),
        ("dfm.drill-span-plating.v1", (1, 1)),
        ("dfm.finish-profile.v1", (1, 1)),
        ("dfm.impedance-special-process.v1", (2, 1)),
    ] {
        let metric = metrics[family];
        assert_eq!((metric.tp, metric.tn), expected, "{family}");
        assert_eq!((metric.fp, metric.fn_count), (0, 0), "{family}");
        assert_eq!(metric.precision, Some(1.0), "{family}");
        assert_eq!(metric.recall, Some(1.0), "{family}");
        assert_eq!(metric.not_checked_mutations, REQUIRED_MUTATIONS.len());
    }

    let manifest: Manifest = serde_json::from_str(MANIFEST_JSON).unwrap();
    validate_manifest(&manifest).unwrap();
    for family in metrics.keys() {
        let contract = manifest
            .families
            .iter()
            .find(|candidate| {
                family_key(&candidate.family_id, &candidate.family_version) == *family
            })
            .unwrap();
        assert_eq!(contract.promotion_state, "evidence_only");
        assert_eq!(
            contract.corpus_ref.as_deref(),
            Some(format!("construction-targets.json#{family}").as_str())
        );
    }

    let report = review(
        &fixture("kicad/mismatch"),
        authority_options(Some(declarations_from(&declaration_value()).unwrap())),
    )
    .unwrap();
    validate_report(&report).unwrap();
    let declaration_document = report
        .fabrication
        .documents
        .iter()
        .find(|document| document.adapter == "ratemypcb-dfm-declarations")
        .unwrap();
    for kind in [
        ConstraintKind::Finish,
        ConstraintKind::Impedance,
        ConstraintKind::SpecialProcess,
    ] {
        assert_eq!(
            report
                .fabrication
                .constraints
                .iter()
                .filter(|constraint| {
                    constraint.kind == kind
                        && constraint.provenance.document_id == declaration_document.id
                })
                .count(),
            1,
            "{kind:?}"
        );
    }
    assert!(report.fabrication.constraints.iter().all(|constraint| {
        constraint.provenance.document_id != declaration_document.id
            || constraint
                .provenance
                .source_lexeme
                .as_deref()
                .is_none_or(|source| {
                    ![
                        "drill_span_plating@board",
                        "castellation@board",
                        "edge_plating@board",
                        "profile@board",
                    ]
                    .contains(&source)
                })
    }));
    for family in [
        "dfm.stackup-order-confirmation.v1",
        "dfm.total-thickness-material.v1",
        "dfm.drill-span-plating.v1",
        "dfm.finish-profile.v1",
        "dfm.impedance-special-process.v1",
    ] {
        assert!(
            report
                .required_evidence
                .iter()
                .all(|required| !required.check_id.starts_with(family))
        );
        assert!(
            findings_for(&report, family)
                .iter()
                .all(|finding| finding.gate_impact == GateImpact::EvidenceOnly)
        );
    }
    for concept in ["profile", "castellation", "edge-plating"] {
        let finding = findings_for(&report, "dfm.finish-profile.v1")
            .into_iter()
            .find(|finding| {
                occurrence_check_id(&report, &finding.id)
                    .is_some_and(|check_id| check_id.ends_with(concept))
            })
            .unwrap();
        let check_id = occurrence_check_id(&report, &finding.id).unwrap();
        assert!(check_id.contains("/gap/"));
        assert!(finding.evidence.contains("outcome=confirmation_gap"));
        assert!(!check_id.contains("/match/"));
        assert!(!check_id.contains("/conflict/"));
    }
    assert!(
        findings_for(&report, "dfm.drill-span-plating.v1")
            .iter()
            .all(|finding| occurrence_check_id(&report, &finding.id)
                .is_some_and(|check_id| check_id.contains("/gap/")))
    );

    let mut forged_pass = report.clone();
    let coverage_id = coverage_for(&forged_pass, "dfm.drill-span-plating.v1")
        .id
        .clone();
    forged_pass
        .coverage
        .iter_mut()
        .find(|coverage| coverage.id == coverage_id)
        .unwrap()
        .status = CoverageStatus::Passed;
    assert!(validate_report(&forged_pass).is_err());

    let mut forged_blocker = report.clone();
    let gap_id = findings_for(&forged_blocker, "dfm.finish-profile.v1")[0]
        .id
        .clone();
    forged_blocker
        .findings
        .iter_mut()
        .find(|finding| finding.id == gap_id)
        .unwrap()
        .gate_impact = GateImpact::Blocking;
    assert!(validate_report(&forged_blocker).is_err());
}

#[test]
fn drill_outline_geometry_corpus_is_exact_bounded_and_evidence_only() {
    let corpus: GeometryCorpus = serde_json::from_str(GEOMETRY_TARGETS_JSON).unwrap();
    let metrics = validate_geometry_corpus(&corpus).unwrap();
    assert_eq!(metrics.len(), 8);
    for (family, expected) in [
        ("dfm.minimum-finished-drill.v1", (1, 1)),
        ("dfm.drill-tool-integrity.v1", (1, 1)),
        ("dfm.outline-topology.v1", (2, 2)),
        ("dfm.copper-edge.v1", (3, 2)),
        ("dfm.copper-clearance.v1", (2, 3)),
        ("dfm.annular-ring.v1", (2, 1)),
        ("dfm.mask-sliver.v1", (2, 3)),
        ("dfm.paste-mask-relationship.v1", (2, 3)),
    ] {
        let metrics = metrics[family];
        assert_eq!((metrics.tp, metrics.tn), expected, "{family}");
        assert_eq!(metrics.fp, 0, "{family}");
        assert_eq!(metrics.fn_count, 0, "{family}");
        assert_eq!(metrics.not_checked_mutations, REQUIRED_MUTATIONS.len());
        assert_eq!(metrics.precision, Some(1.0));
        assert_eq!(metrics.recall, Some(1.0));
    }

    let manifest: Manifest = serde_json::from_str(MANIFEST_JSON).unwrap();
    validate_manifest(&manifest).unwrap();
    for key in metrics.keys() {
        let family = manifest
            .families
            .iter()
            .find(|family| family_key(&family.family_id, &family.family_version) == *key)
            .unwrap();
        assert_eq!(family.promotion_state, "evidence_only");
        assert_eq!(
            family.corpus_ref.as_deref(),
            Some(format!("geometry-targets.json#{key}").as_str())
        );
    }
}

#[test]
fn drill_families_use_exact_declaration_authority_and_keep_objects_distinct() {
    let (root, report) = geometry_report(Some("0.600"));
    validate_report(&report).unwrap();

    let minimum = coverage_for(&report, "dfm.minimum-finished-drill.v1");
    assert_eq!(minimum.status, CoverageStatus::Passed);
    assert!(minimum.evidence.contains("observed=600000000pm"));
    assert!(minimum.evidence.contains("threshold=600000000pm"));
    assert!(minimum.evidence.contains("delta=0pm"));
    assert!(minimum.evidence.contains("resolution=1000000pm"));
    assert!(
        minimum
            .evidence
            .contains("ratemypcb-project-authority 2026.08")
    );

    let integrity = coverage_for(&report, "dfm.drill-tool-integrity.v1");
    assert_eq!(integrity.status, CoverageStatus::Passed);
    assert!(integrity.evidence.contains("drills=1"));
    assert!(integrity.evidence.contains("routes=1"));
    assert!(integrity.evidence.contains("slots=1"));
    assert!(integrity.evidence.contains("round_hits=1"));
    assert!(findings_for(&report, "dfm.minimum-finished-drill.v1").is_empty());
    assert!(findings_for(&report, "dfm.drill-tool-integrity.v1").is_empty());

    for coverage in [minimum, integrity] {
        let evidence = report
            .evidence
            .iter()
            .find(|record| record.id == coverage.id)
            .unwrap();
        assert_eq!(
            evidence.provenance.artifact_digest,
            report.fabrication.model_digest
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn drill_one_resolution_violation_is_measured_and_evidence_only() {
    let (root, report) = geometry_report(Some("0.601"));
    validate_report(&report).unwrap();

    assert_eq!(
        coverage_for(&report, "dfm.minimum-finished-drill.v1").status,
        CoverageStatus::Attention
    );
    let findings = findings_for(&report, "dfm.minimum-finished-drill.v1");
    assert_eq!(findings.len(), 1);
    let finding = findings[0];
    assert_eq!(finding.gate_impact, GateImpact::EvidenceOnly);
    assert!(finding.evidence.contains("observed=600000000pm"));
    assert!(finding.evidence.contains("threshold=601000000pm"));
    assert!(finding.evidence.contains("delta=1000000pm"));
    assert!(finding.evidence.contains("resolution=1000000pm"));
    assert!(finding.location.contains("tool="));
    assert!(finding.location.contains("hit="));
    assert_eq!(
        coverage_for(&report, "dfm.drill-tool-integrity.v1").status,
        CoverageStatus::Passed
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn drill_missing_stale_and_duplicate_authority_fail_closed_without_preset_fallback() {
    let (root, report) = geometry_report(None);
    validate_report(&report).unwrap();
    let minimum = coverage_for(&report, "dfm.minimum-finished-drill.v1");
    assert_eq!(minimum.status, CoverageStatus::NotRun);
    assert!(minimum.evidence.starts_with("not_checked:"));
    assert!(findings_for(&report, "dfm.minimum-finished-drill.v1").is_empty());
    fs::remove_dir_all(root).unwrap();

    let original = declaration_value();
    let mut stale = original.clone();
    stale["expiresAtUnix"] = Value::from(1);
    assert!(declarations_from(&stale).is_err());

    let mut duplicate = original;
    let first = duplicate["rules"][0].clone();
    duplicate["rules"].as_array_mut().unwrap().push(first);
    assert!(declarations_from(&duplicate).is_err());
}

#[test]
fn copper_edge_uses_production_authority_for_exterior_and_routed_boundaries() {
    let (root, boundary) = distance_report(false, Some("0.900"), "0.200");
    validate_report(&boundary).unwrap();
    let coverage = coverage_for(&boundary, "dfm.copper-edge.v1");
    assert_eq!(coverage.status, CoverageStatus::Passed, "{coverage:?}");
    assert!(coverage.evidence.contains("observed=900000000pm"));
    assert!(coverage.evidence.contains("threshold=900000000pm"));
    assert!(coverage.evidence.contains("boundary=exterior"));
    fs::remove_dir_all(root).unwrap();

    let (root, violation) = distance_report(false, Some("0.900001"), "0.200");
    validate_report(&violation).unwrap();
    assert_eq!(
        coverage_for(&violation, "dfm.copper-edge.v1").status,
        CoverageStatus::Attention
    );
    let findings = findings_for(&violation, "dfm.copper-edge.v1");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].gate_impact, GateImpact::EvidenceOnly);
    assert!(findings[0].evidence.contains("delta=1000pm"));
    fs::remove_dir_all(root).unwrap();

    let (root, routed) = distance_report(true, Some("0.401"), "0.200");
    validate_report(&routed).unwrap();
    let findings = findings_for(&routed, "dfm.copper-edge.v1");
    assert_eq!(findings.len(), 1);
    assert!(findings[0].evidence.contains("observed=400000000pm"));
    assert!(findings[0].evidence.contains("boundary=routed"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn copper_edge_missing_or_anonymous_authority_is_not_checked() {
    let (root, report) = distance_report(false, None, "0.200");
    validate_report(&report).unwrap();
    let coverage = coverage_for(&report, "dfm.copper-edge.v1");
    assert_eq!(coverage.status, CoverageStatus::NotRun);
    assert!(coverage.evidence.starts_with("not_checked:"));
    assert!(findings_for(&report, "dfm.copper-edge.v1").is_empty());
    fs::remove_dir_all(root).unwrap();

    let (root, off_grid) = distance_report(false, Some("0.9000005"), "0.200");
    validate_report(&off_grid).unwrap();
    assert_eq!(
        coverage_for(&off_grid, "dfm.copper-edge.v1").status,
        CoverageStatus::NotRun
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn copper_clearance_uses_only_same_layer_proven_different_nets() {
    let (root, boundary) = distance_report(false, Some("0.900"), "0.200");
    validate_report(&boundary).unwrap();
    let coverage = coverage_for(&boundary, "dfm.copper-clearance.v1");
    assert_eq!(coverage.status, CoverageStatus::Passed, "{coverage:?}");
    assert!(coverage.evidence.contains("observed=200000000pm"));
    assert!(coverage.evidence.contains("threshold=200000000pm"));
    assert!(coverage.evidence.contains("different_net_pairs="));
    fs::remove_dir_all(root).unwrap();

    let (root, violation) = distance_report(false, Some("0.900"), "0.200001");
    validate_report(&violation).unwrap();
    assert_eq!(
        coverage_for(&violation, "dfm.copper-clearance.v1").status,
        CoverageStatus::Attention
    );
    let findings = findings_for(&violation, "dfm.copper-clearance.v1");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].gate_impact, GateImpact::EvidenceOnly);
    assert!(findings[0].evidence.contains("delta=1000pm"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn copper_clearance_same_net_adjacent_layer_and_missing_authority_are_hard_negatives() {
    let root = distance_package(false);
    replace_once(&root.join("top.gbr"), "%TO.N,VCC*%", "%TO.N,GND*%");
    replace_once(&root.join("bottom.gbr"), "%TO.N,GND*%", "%TO.N,VCC*%");
    let report = review(
        &root,
        authority_options(Some(distance_declarations("0.900", "0.200"))),
    )
    .unwrap();
    validate_report(&report).unwrap();
    let coverage = coverage_for(&report, "dfm.copper-clearance.v1");
    assert_eq!(coverage.status, CoverageStatus::Passed, "{coverage:?}");
    assert!(coverage.evidence.contains("different_net_pairs=0"));
    assert!(findings_for(&report, "dfm.copper-clearance.v1").is_empty());
    fs::remove_dir_all(root).unwrap();

    let (root, missing) = distance_report(false, None, "0.200");
    validate_report(&missing).unwrap();
    assert_eq!(
        coverage_for(&missing, "dfm.copper-clearance.v1").status,
        CoverageStatus::NotRun
    );
    assert!(findings_for(&missing, "dfm.copper-clearance.v1").is_empty());
    fs::remove_dir_all(root).unwrap();

    let (root, off_grid) = distance_report(false, Some("0.900"), "0.2000005");
    validate_report(&off_grid).unwrap();
    assert_eq!(
        coverage_for(&off_grid, "dfm.copper-clearance.v1").status,
        CoverageStatus::NotRun
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn annular_ring_native_same_pad_association_is_exact_per_layer() {
    let (root, boundary) =
        annular_report("thru_hole", "circle", "1.000 1.000", "0.600", Some("0.200"));
    validate_report(&boundary).unwrap();
    let model = serde_json::to_value(&boundary.fabrication).unwrap();
    let associations = model["padHoleAssociations"].as_array().unwrap();
    assert_eq!(associations.len(), 1);
    assert_eq!(associations[0]["plating"], "plated");
    assert_eq!(
        associations[0]["applicableLayerIds"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(associations[0]["padGeometry"]["kind"], "contour");
    assert_eq!(associations[0]["holeGeometry"]["kind"], "drill");
    assert_eq!(
        associations[0]["padProvenance"]["documentId"],
        associations[0]["holeProvenance"]["documentId"]
    );
    let coverage = coverage_for(&boundary, "dfm.annular-ring.v1");
    assert_eq!(coverage.status, CoverageStatus::Passed, "{coverage:?}");
    assert!(coverage.evidence.contains("observed=200000000pm"));
    assert!(coverage.evidence.contains("threshold=200000000pm"));
    assert!(coverage.evidence.contains("layers=2"));
    fs::remove_dir_all(root).unwrap();

    let (root, violation) =
        annular_report("thru_hole", "circle", "1.000 1.000", "0.600", Some("0.201"));
    validate_report(&violation).unwrap();
    let coverage = coverage_for(&violation, "dfm.annular-ring.v1");
    assert_eq!(coverage.status, CoverageStatus::Attention, "{coverage:?}");
    let findings = findings_for(&violation, "dfm.annular-ring.v1");
    assert_eq!(findings.len(), 2);
    assert!(findings.iter().all(|finding| {
        finding.gate_impact == GateImpact::EvidenceOnly
            && finding.evidence.contains("delta=1000000pm")
    }));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn annular_ring_npth_slot_unsupported_and_missing_authority_are_not_checked() {
    for (kind, shape, size, drill) in [
        ("np_thru_hole", "circle", "1.000 1.000", "0.600"),
        ("thru_hole", "circle", "1.200 1.200", "oval 0.800 0.600"),
        ("thru_hole", "rect", "1.000 1.000", "0.600"),
    ] {
        let (root, report) = annular_report(kind, shape, size, drill, Some("0.200"));
        validate_report(&report).unwrap();
        let model = serde_json::to_value(&report.fabrication).unwrap();
        assert!(model["padHoleAssociations"].as_array().unwrap().is_empty());
        assert_eq!(
            coverage_for(&report, "dfm.annular-ring.v1").status,
            CoverageStatus::NotRun
        );
        assert!(findings_for(&report, "dfm.annular-ring.v1").is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    let (root, report) = annular_report("thru_hole", "circle", "1.000 1.000", "0.600", None);
    validate_report(&report).unwrap();
    assert_eq!(
        coverage_for(&report, "dfm.annular-ring.v1").status,
        CoverageStatus::NotRun
    );
    fs::remove_dir_all(root).unwrap();

    let (root, off_grid) = annular_report(
        "thru_hole",
        "circle",
        "1.000 1.000",
        "0.600",
        Some("0.2005"),
    );
    validate_report(&off_grid).unwrap();
    assert_eq!(
        coverage_for(&off_grid, "dfm.annular-ring.v1").status,
        CoverageStatus::NotRun
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn mask_sliver_layer_presence_without_resolved_polarity_or_intent_is_not_checked() {
    for with_declarations in [true, false] {
        let (root, report) = mask_paste_report(with_declarations);
        validate_report(&report).unwrap();
        let coverage = coverage_for(&report, "dfm.mask-sliver.v1");
        assert_eq!(coverage.status, CoverageStatus::NotRun);
        assert!(coverage.evidence.starts_with("not_checked:"));
        assert!(findings_for(&report, "dfm.mask-sliver.v1").is_empty());
        assert!(!report.fabrication.assembly.mask_layer_ids.is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn paste_mask_layer_presence_without_fitted_pad_authority_is_not_checked() {
    for with_declarations in [true, false] {
        let (root, report) = mask_paste_report(with_declarations);
        validate_report(&report).unwrap();
        let coverage = coverage_for(&report, "dfm.paste-mask-relationship.v1");
        assert_eq!(coverage.status, CoverageStatus::NotRun);
        assert!(coverage.evidence.starts_with("not_checked:"));
        assert!(findings_for(&report, "dfm.paste-mask-relationship.v1").is_empty());
        assert!(!report.fabrication.assembly.mask_layer_ids.is_empty());
        assert!(!report.fabrication.assembly.paste_layer_ids.is_empty());
        assert!(report.fabrication.assembly.placements.is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn dfm01_family_matrix_is_complete_qualified_and_not_required() {
    let manifest: Manifest = serde_json::from_str(MANIFEST_JSON).unwrap();
    validate_manifest(&manifest).unwrap();
    let corpus: GeometryCorpus = serde_json::from_str(GEOMETRY_TARGETS_JSON).unwrap();
    let metrics = validate_geometry_corpus(&corpus).unwrap();
    let expected = BTreeSet::from([
        "dfm.annular-ring.v1",
        "dfm.copper-clearance.v1",
        "dfm.copper-edge.v1",
        "dfm.drill-tool-integrity.v1",
        "dfm.mask-sliver.v1",
        "dfm.minimum-finished-drill.v1",
        "dfm.outline-topology.v1",
        "dfm.paste-mask-relationship.v1",
    ]);
    assert_eq!(
        metrics.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        expected
    );
    assert!(
        manifest
            .families
            .iter()
            .filter(|family| family.requirement == "DFM-01")
            .all(|family| family.promotion_state == "evidence_only" && family.corpus_ref.is_some())
    );

    let (root, report) = mask_paste_report(true);
    validate_report(&report).unwrap();
    for family in expected {
        assert!(
            report
                .coverage
                .iter()
                .any(|coverage| occurrence_check_id(&report, &coverage.id) == Some(family)),
            "missing production coverage for {family}"
        );
    }
    assert_eq!(
        report
            .required_evidence
            .iter()
            .map(|required| required.check_id.as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "bom-structure",
            "drill-data",
            "gerber-syntax",
            "native-drc",
            "package-gerbers",
            "placement-structure",
            "profile",
            "profile-drc",
            "source-structure",
            "supply-snapshot",
        ])
    );
    assert!(report.required_evidence.iter().all(|required| {
        !required.check_id.starts_with("dfm.")
            && !required.check_id.starts_with("assembly.")
            && !required.check_id.starts_with("inference.")
    }));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn outline_complete_profile_reports_exact_stable_topology() {
    let (root, report) = geometry_report(None);
    validate_report(&report).unwrap();
    let outline = coverage_for(&report, "dfm.outline-topology.v1");
    assert_eq!(outline.status, CoverageStatus::Passed);
    assert!(outline.evidence.contains("contours=1"));
    assert!(outline.evidence.contains("exteriors=1"));
    assert!(outline.evidence.contains("cutouts=0"));
    assert!(outline.evidence.contains("open=0"));
    assert!(outline.evidence.contains("intersections=0"));
    assert!(
        outline
            .evidence
            .contains("extents=0,0..10000000000,10000000000pm")
    );
    assert!(outline.evidence.contains("classification=exterior"));
    assert!(outline.evidence.contains("source="));
    assert!(findings_for(&report, "dfm.outline-topology.v1").is_empty());
    let evidence = report
        .evidence
        .iter()
        .find(|record| record.id == outline.id)
        .unwrap();
    assert_eq!(
        evidence.provenance.artifact_digest,
        report.fabrication.model_digest
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn outline_forged_family_prefixed_coverage_is_rejected() {
    let (root, mut report) = geometry_report(None);
    let check_id = "dfm.outline-topology.v1/forged";
    let location = BTreeMap::from([
        ("kind".into(), "coverage".into()),
        ("value".into(), check_id.into()),
    ]);
    let id = canonical_evidence_id(&report.fabrication.model_digest, check_id, &location);
    report.coverage.push(Coverage {
        id: id.clone(),
        label: "Forged outline pass".into(),
        status: CoverageStatus::Passed,
        evidence: "Fabricated family-prefixed coverage".into(),
    });
    let mut evidence = report
        .evidence
        .iter()
        .find(|record| record.check_id == "dfm.outline-topology.v1")
        .unwrap()
        .clone();
    evidence.id = id;
    evidence.check_id = check_id.into();
    evidence.provenance.location = location;
    report.evidence.push(evidence);
    assert!(validate_report(&report).is_err());
    fs::remove_dir_all(root).unwrap();
}

fn canonical_evidence_id(
    artifact_digest: &str,
    check_id: &str,
    location: &BTreeMap<String, String>,
) -> String {
    let canonical = serde_json::to_vec(&(artifact_digest, check_id, location)).unwrap();
    format!("ev-{:x}", Sha256::digest(canonical))
}

fn design_unblock_report() -> Report {
    let mut options = population_options();
    options.scope = ReviewScope::Design;
    review(&fixture("narrow-board.kicad_pcb"), options).unwrap()
}

fn assessment_with_p1(evidence_refs: Vec<String>) -> Assessment {
    Assessment {
        assessment_schema_version: "2.0".into(),
        report_digest: "0".repeat(64),
        rating: 4,
        disposition: "blocked".into(),
        verdict: "Release remains blocked".into(),
        verdict_evidence_refs: evidence_refs.clone(),
        rationale: "The highest-priority release unblock remains unresolved.".into(),
        category_summaries: vec![],
        actions: vec![AssessmentAction {
            priority: 1,
            title: "Resolve the top release unblock".into(),
            rationale: "Address the core-ranked evidence before release.".into(),
            evidence_refs,
        }],
        questions: vec![],
    }
}

fn complete_design_required_evidence(report: &mut Report) {
    let native = report
        .required_evidence
        .iter_mut()
        .find(|item| item.check_id == "native-drc")
        .unwrap();
    let native_id = native.evidence_id.clone();
    report
        .coverage
        .iter_mut()
        .find(|item| item.id == native_id)
        .unwrap()
        .status = CoverageStatus::Passed;
    let record = report
        .evidence
        .iter_mut()
        .find(|item| item.id == native_id)
        .unwrap();
    record.provenance.freshness = EvidenceFreshness::NotApplicable;
    record.provenance.producer.version = "10.0.5".into();
    native.execution = EvidenceExecution::Completed;
    native.result = EvidenceResult::Pass;
    native.freshness = EvidenceFreshness::NotApplicable;
    report.native_drc.status = "completed".into();
    report.native_drc.version = Some("10.0.5".into());
    report.native_drc.report_version = Some("10.0".into());
    report.native_drc.note = "Synthetic completed native evidence for ranking policy.".into();
    report.approval_eligible = false;
    validate_report(report).unwrap();
}

#[test]
fn qualification_validation_recomputes_unknown_and_inference_family_impact() {
    let mut report = design_unblock_report();
    let mut record = report
        .evidence
        .iter()
        .find(|record| record.kind == "finding")
        .unwrap()
        .clone();
    let check_id = "inference.interface.v1/synthetic";
    record.check_id = check_id.into();
    record.provenance.location = BTreeMap::from([
        ("kind".into(), "finding".into()),
        ("value".into(), "synthetic-interface".into()),
    ]);
    record.id = canonical_evidence_id(
        &record.provenance.artifact_digest,
        check_id,
        &record.provenance.location,
    );
    let id = record.id.clone();
    report.evidence.push(record);
    report.findings.push(Finding {
        id: id.clone(),
        severity: Severity::High,
        category: "Inference".into(),
        title: "Synthetic inference attention".into(),
        evidence: "Synthetic family policy mutation.".into(),
        recommendation: "Review the source-linked intent declaration.".into(),
        location: "synthetic-interface".into(),
        source: "dfm-qualification-test".into(),
        gate_impact: GateImpact::EvidenceOnly,
    });
    validate_report(&report).unwrap();

    report
        .findings
        .iter_mut()
        .find(|finding| finding.id == id)
        .unwrap()
        .gate_impact = GateImpact::Blocking;
    assert!(
        validate_report(&report)
            .unwrap_err()
            .to_string()
            .contains("GateImpact")
    );
}

#[test]
fn unblock_required_evidence_precedes_findings_and_requires_priority_one() {
    let report = design_unblock_report();
    let top = report
        .required_evidence
        .iter()
        .find(|item| item.check_id == "native-drc")
        .unwrap()
        .evidence_id
        .clone();
    let wrong = report.findings[0].id.clone();

    assert!(validate_assessment(&report, &assessment_with_p1(vec![top])).is_ok());
    assert!(
        validate_assessment(&report, &assessment_with_p1(vec![wrong]))
            .unwrap_err()
            .to_string()
            .contains("priority 1")
    );
    let mut missing = assessment_with_p1(vec![report.required_evidence[0].evidence_id.clone()]);
    missing.actions.clear();
    assert!(
        validate_assessment(&report, &missing)
            .unwrap_err()
            .to_string()
            .contains("priority 1")
    );
}

#[test]
fn unblock_blocker_and_attention_ties_ignore_score() {
    let mut report = design_unblock_report();
    complete_design_required_evidence(&mut report);
    let check_ids = report
        .evidence
        .iter()
        .map(|record| (record.id.clone(), record.check_id.clone()))
        .collect::<BTreeMap<_, _>>();
    let check_id = |finding: &Finding| check_ids.get(&finding.id).unwrap().as_str();
    let mut blockers = report
        .findings
        .iter()
        .filter(|finding| {
            finding.gate_impact == GateImpact::Blocking && finding.severity >= Severity::Medium
        })
        .collect::<Vec<_>>();
    blockers.sort_by(|left, right| {
        right
            .severity
            .cmp(&left.severity)
            .then_with(|| check_id(left).cmp(check_id(right)))
            .then_with(|| left.location.cmp(&right.location))
            .then_with(|| left.id.cmp(&right.id))
    });
    let top = blockers[0].id.clone();
    let wrong = blockers[1].id.clone();
    assert!(validate_assessment(&report, &assessment_with_p1(vec![top.clone()])).is_ok());
    assert!(validate_assessment(&report, &assessment_with_p1(vec![wrong])).is_err());

    let mut rescored = report.clone();
    rescored.score.raw = 0;
    rescored.score.value = 0.0;
    rescored.score.verdict = "Mutated score must not affect action order".into();
    assert!(validate_assessment(&rescored, &assessment_with_p1(vec![top])).is_ok());

    for finding in &mut report.findings {
        finding.gate_impact = GateImpact::EvidenceOnly;
    }
    report.approval_eligible = true;
    validate_report(&report).unwrap();
    let mut attention = report.findings.iter().collect::<Vec<_>>();
    attention.sort_by(|left, right| {
        check_id(left)
            .cmp(check_id(right))
            .then_with(|| left.location.cmp(&right.location))
            .then_with(|| left.id.cmp(&right.id))
    });
    assert!(
        validate_assessment(&report, &assessment_with_p1(vec![attention[0].id.clone()])).is_ok()
    );
    assert!(
        validate_assessment(&report, &assessment_with_p1(vec![attention[1].id.clone()])).is_err()
    );
}

#[test]
fn unblock_empty_required_evidence_id_is_rejected() {
    let mut report = design_unblock_report();
    let source_id = report
        .required_evidence
        .iter()
        .find(|item| item.check_id == "source-structure")
        .unwrap()
        .evidence_id
        .clone();
    let native = report
        .required_evidence
        .iter_mut()
        .find(|item| item.check_id == "native-drc")
        .unwrap();
    let native_id = std::mem::take(&mut native.evidence_id);
    native.execution = EvidenceExecution::Unknown;
    native.result = EvidenceResult::Unknown;
    native.freshness = EvidenceFreshness::Unknown;
    native.confidence = EvidenceConfidence::Unknown;
    report.coverage.retain(|item| item.id != native_id);
    report.evidence.retain(|item| item.id != native_id);
    for refs in &mut report.limitation_evidence_refs {
        for evidence_ref in refs {
            if *evidence_ref == native_id {
                *evidence_ref = source_id.clone();
            }
        }
    }
    assert!(
        validate_report(&report)
            .unwrap_err()
            .to_string()
            .contains("canonical coverage evidence")
    );
}
