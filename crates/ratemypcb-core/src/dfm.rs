use crate::fabrication::{
    AnalyzerDispatchStatus, AnalyzerRequirements, ApertureDefinition, ApertureShape,
    AssemblyBottomMirroring, AssemblyFittedState, AssemblyPlacementConvention,
    AssemblyPlacementOrigin, AssemblyRotationDirection, AssemblySideConvention, Authority,
    CanonicalArc, CanonicalContour, CanonicalLine, CanonicalPoint, CapabilityId, CapabilityRecord,
    CapabilityState, ConstraintKind, ConstructionLayer, ContourSegment, DeclaredAssemblyPlacement,
    DocumentFormat, DocumentMetrics, FabricationReview, FeatureMembership, Geometry,
    KICAD_MANUFACTURING_ADAPTER, KICAD_MANUFACTURING_ADAPTER_VERSION, LayerPolarity, LayerRole,
    LayerSide, ManufacturingConstraint, ManufacturingDeadline, ManufacturingDocument,
    ManufacturingFeature, ManufacturingProvenance, ManufacturingTool, NativeCourtyardKind,
    NativeCourtyardRunState, NativeExclusionState, ObjectSemantics, ParseStatus, Picometres,
    Plating, QuadrantMode, SemanticAnalyzerResult, SourceUnit, StructuralLocation, ToolKind,
    X2AttributeKind, X2AttributeScope, constraint_id, declared_assembly_placement_id,
    dispatch_analyzer, document_id, exact_circle_geometry, parse_decimal_microdegrees,
};
use crate::{
    Coverage, CoverageStatus, Error, EvidenceExecution, EvidenceFreshness, EvidenceRecord,
    EvidenceResult, Finding, GateImpact, RequiredEvidence, SchematicComparisonSource,
    SchematicFact, SchematicMismatch, SchematicOccurrence, SchematicReview, Severity,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};
use std::time::Duration;

pub(crate) const POPULATION_PARITY_FAMILY: &str = "assembly.population-parity.v1";
const POPULATION_FINDING_PREFIX: &str = "assembly.population-parity.v1/";
const POPULATION_FIELDS: &[&str] = &[
    "board-population",
    "bom-population",
    "bom-quantity",
    "bom-fitted",
    "dnp",
    "placement-population",
    "revision",
];
const SIDE_ROTATION_FAMILY: &str = "assembly.side-rotation.v1";
const ASSEMBLY_PASTE_FAMILY: &str = "assembly.paste-availability.v1";
const COURTYARD_NATIVE_FAMILY: &str = "assembly.courtyard-native.v1";
const FOOTPRINT_STRING_FAMILY: &str = "assembly.footprint-string-parity.v1";
const ASSEMBLY_ACCESS_FAMILY: &str = "assembly.access.v1";
const TESTPOINT_ACCESS_FAMILY: &str = "assembly.testpoint-access.v1";
const SIDE_ROTATION_REQUIREMENTS: AnalyzerRequirements = AnalyzerRequirements {
    check_family: SIDE_ROTATION_FAMILY,
    prerequisites: &[CapabilityId::Assembly, CapabilityId::NativeKicadFacts],
};
const ASSEMBLY_PASTE_REQUIREMENTS: AnalyzerRequirements = AnalyzerRequirements {
    check_family: ASSEMBLY_PASTE_FAMILY,
    prerequisites: &[
        CapabilityId::Assembly,
        CapabilityId::Components,
        CapabilityId::Pins,
        CapabilityId::LayerRoles,
        CapabilityId::Apertures,
        CapabilityId::X2ApertureAttributes,
        CapabilityId::GeometryFlashes,
        CapabilityId::GeometryExpanded,
        CapabilityId::Transforms,
        CapabilityId::Polarity,
    ],
};
const COURTYARD_NATIVE_REQUIREMENTS: AnalyzerRequirements = AnalyzerRequirements {
    check_family: COURTYARD_NATIVE_FAMILY,
    prerequisites: &[CapabilityId::NativeKicadFacts],
};
const FOOTPRINT_STRING_REQUIREMENTS: AnalyzerRequirements = AnalyzerRequirements {
    check_family: FOOTPRINT_STRING_FAMILY,
    prerequisites: &[CapabilityId::Components],
};
const ASSEMBLY_ACCESS_REQUIREMENTS: AnalyzerRequirements = AnalyzerRequirements {
    check_family: ASSEMBLY_ACCESS_FAMILY,
    prerequisites: &[
        CapabilityId::Assembly,
        CapabilityId::Profile,
        CapabilityId::Components,
        CapabilityId::LayerRoles,
        CapabilityId::Apertures,
        CapabilityId::GeometryFlashes,
        CapabilityId::GeometryRegions,
        CapabilityId::GeometryExpanded,
        CapabilityId::Transforms,
        CapabilityId::Polarity,
    ],
};
const TESTPOINT_ACCESS_REQUIREMENTS: AnalyzerRequirements = AnalyzerRequirements {
    check_family: TESTPOINT_ACCESS_FAMILY,
    prerequisites: &[
        CapabilityId::Connectivity,
        CapabilityId::Components,
        CapabilityId::Pins,
        CapabilityId::Assembly,
        CapabilityId::Profile,
        CapabilityId::LayerRoles,
        CapabilityId::Apertures,
        CapabilityId::GeometryFlashes,
        CapabilityId::GeometryRegions,
        CapabilityId::GeometryExpanded,
        CapabilityId::Transforms,
        CapabilityId::Polarity,
    ],
};
const DECLARATION_ADAPTER: &str = "ratemypcb-dfm-declarations";
const DECLARATION_SCHEMA: &str = "1";
const MAX_DECLARATION_BYTES: usize = 256 * 1024;
const MAX_DECLARATION_RECORDS: usize = 128;
const MAX_DECLARATION_TEXT: usize = 256;
const MAX_INFERENCE_TARGETS: usize = 16;
const MAX_INFERENCE_FIELDS: usize = 8;
const MAX_INFERENCE_COMPONENTS: usize = 512;
const MAX_INFERENCE_DISTANCE_PM: i64 = 1_000_000_000_000;
const OUTLINE_TOPOLOGY_FAMILY: &str = "dfm.outline-topology.v1";
const MINIMUM_FINISHED_DRILL_FAMILY: &str = "dfm.minimum-finished-drill.v1";
const DRILL_TOOL_INTEGRITY_FAMILY: &str = "dfm.drill-tool-integrity.v1";
const COPPER_EDGE_FAMILY: &str = "dfm.copper-edge.v1";
const COPPER_CLEARANCE_FAMILY: &str = "dfm.copper-clearance.v1";
const ANNULAR_RING_FAMILY: &str = "dfm.annular-ring.v1";
const MASK_SLIVER_FAMILY: &str = "dfm.mask-sliver.v1";
const PASTE_MASK_FAMILY: &str = "dfm.paste-mask-relationship.v1";
const STACKUP_ORDER_FAMILY: &str = "dfm.stackup-order-confirmation.v1";
const TOTAL_THICKNESS_MATERIAL_FAMILY: &str = "dfm.total-thickness-material.v1";
const DRILL_SPAN_PLATING_FAMILY: &str = "dfm.drill-span-plating.v1";
const FINISH_PROFILE_FAMILY: &str = "dfm.finish-profile.v1";
const IMPEDANCE_SPECIAL_PROCESS_FAMILY: &str = "dfm.impedance-special-process.v1";
const MINIMUM_DRILL_REQUIREMENTS: AnalyzerRequirements = AnalyzerRequirements {
    check_family: MINIMUM_FINISHED_DRILL_FAMILY,
    prerequisites: &[
        CapabilityId::UnitsAndFormat,
        CapabilityId::Tools,
        CapabilityId::Drills,
        CapabilityId::Constraints,
    ],
};
const DRILL_TOOL_REQUIREMENTS: AnalyzerRequirements = AnalyzerRequirements {
    check_family: DRILL_TOOL_INTEGRITY_FAMILY,
    prerequisites: &[CapabilityId::UnitsAndFormat, CapabilityId::Tools],
};
const DRILL_REQUIREMENT: AnalyzerRequirements = AnalyzerRequirements {
    check_family: DRILL_TOOL_INTEGRITY_FAMILY,
    prerequisites: &[CapabilityId::Drills],
};
const ROUTE_REQUIREMENT: AnalyzerRequirements = AnalyzerRequirements {
    check_family: DRILL_TOOL_INTEGRITY_FAMILY,
    prerequisites: &[CapabilityId::Routes],
};
const SLOT_REQUIREMENT: AnalyzerRequirements = AnalyzerRequirements {
    check_family: DRILL_TOOL_INTEGRITY_FAMILY,
    prerequisites: &[CapabilityId::Slots],
};
const PLATING_SPAN_REQUIREMENTS: AnalyzerRequirements = AnalyzerRequirements {
    check_family: DRILL_TOOL_INTEGRITY_FAMILY,
    prerequisites: &[CapabilityId::Plating, CapabilityId::LayerSpans],
};
const OUTLINE_REQUIREMENTS: AnalyzerRequirements = AnalyzerRequirements {
    check_family: OUTLINE_TOPOLOGY_FAMILY,
    prerequisites: &[
        CapabilityId::LayerRoles,
        CapabilityId::Profile,
        CapabilityId::GeometryLines,
        CapabilityId::GeometryArcs,
        CapabilityId::GeometryRegions,
        CapabilityId::GeometryExpanded,
        CapabilityId::Transforms,
        CapabilityId::Polarity,
    ],
};
const COPPER_EDGE_REQUIREMENTS: AnalyzerRequirements = AnalyzerRequirements {
    check_family: COPPER_EDGE_FAMILY,
    prerequisites: &[
        CapabilityId::LayerRoles,
        CapabilityId::Profile,
        CapabilityId::GeometryLines,
        CapabilityId::GeometryArcs,
        CapabilityId::GeometryRegions,
        CapabilityId::GeometryFlashes,
        CapabilityId::GeometryExpanded,
        CapabilityId::Transforms,
        CapabilityId::Polarity,
        CapabilityId::Constraints,
    ],
};
const COPPER_CLEARANCE_REQUIREMENTS: AnalyzerRequirements = AnalyzerRequirements {
    check_family: COPPER_CLEARANCE_FAMILY,
    prerequisites: &[
        CapabilityId::LayerRoles,
        CapabilityId::GeometryLines,
        CapabilityId::GeometryArcs,
        CapabilityId::GeometryRegions,
        CapabilityId::GeometryFlashes,
        CapabilityId::GeometryExpanded,
        CapabilityId::Transforms,
        CapabilityId::Polarity,
        CapabilityId::Connectivity,
        CapabilityId::Constraints,
    ],
};
const ANNULAR_RING_REQUIREMENTS: AnalyzerRequirements = AnalyzerRequirements {
    check_family: ANNULAR_RING_FAMILY,
    prerequisites: &[
        CapabilityId::LayerRoles,
        CapabilityId::LayerOrder,
        CapabilityId::Tools,
        CapabilityId::Drills,
        CapabilityId::Plating,
        CapabilityId::LayerSpans,
        CapabilityId::NativeKicadFacts,
        CapabilityId::Constraints,
    ],
};
const MASK_SLIVER_REQUIREMENTS: AnalyzerRequirements = AnalyzerRequirements {
    check_family: MASK_SLIVER_FAMILY,
    prerequisites: &[
        CapabilityId::LayerRoles,
        CapabilityId::GeometryLines,
        CapabilityId::GeometryArcs,
        CapabilityId::GeometryRegions,
        CapabilityId::GeometryFlashes,
        CapabilityId::GeometryExpanded,
        CapabilityId::Transforms,
        CapabilityId::Polarity,
        CapabilityId::Constraints,
    ],
};
const PASTE_MASK_REQUIREMENTS: AnalyzerRequirements = AnalyzerRequirements {
    check_family: PASTE_MASK_FAMILY,
    prerequisites: &[
        CapabilityId::LayerRoles,
        CapabilityId::Apertures,
        CapabilityId::X2ApertureAttributes,
        CapabilityId::GeometryLines,
        CapabilityId::GeometryArcs,
        CapabilityId::GeometryRegions,
        CapabilityId::GeometryFlashes,
        CapabilityId::GeometryExpanded,
        CapabilityId::Transforms,
        CapabilityId::Polarity,
        CapabilityId::Components,
        CapabilityId::Pins,
        CapabilityId::Assembly,
        CapabilityId::Constraints,
    ],
};
const STACKUP_ORDER_REQUIREMENTS: AnalyzerRequirements = AnalyzerRequirements {
    check_family: STACKUP_ORDER_FAMILY,
    prerequisites: &[CapabilityId::Construction, CapabilityId::LayerOrder],
};
const TOTAL_THICKNESS_MATERIAL_REQUIREMENTS: AnalyzerRequirements = AnalyzerRequirements {
    check_family: TOTAL_THICKNESS_MATERIAL_FAMILY,
    prerequisites: &[CapabilityId::Construction, CapabilityId::Constraints],
};
const DRILL_SPAN_PLATING_REQUIREMENTS: AnalyzerRequirements = AnalyzerRequirements {
    check_family: DRILL_SPAN_PLATING_FAMILY,
    prerequisites: &[
        CapabilityId::Tools,
        CapabilityId::Drills,
        CapabilityId::Plating,
        CapabilityId::LayerSpans,
        CapabilityId::LayerOrder,
    ],
};
const FINISH_PROFILE_REQUIREMENTS: AnalyzerRequirements = AnalyzerRequirements {
    check_family: FINISH_PROFILE_FAMILY,
    prerequisites: &[
        CapabilityId::Construction,
        CapabilityId::Profile,
        CapabilityId::Constraints,
    ],
};
const IMPEDANCE_SPECIAL_PROCESS_REQUIREMENTS: AnalyzerRequirements = AnalyzerRequirements {
    check_family: IMPEDANCE_SPECIAL_PROCESS_FAMILY,
    prerequisites: &[CapabilityId::Constraints, CapabilityId::Construction],
};
// ponytail: bounded O(n²) profile pairing; add exact spatial pruning only if qualified profiles exceed this ceiling.
const MAX_OUTLINE_SEGMENTS: usize = 1_414;
const MAX_OUTLINE_PAIR_CHECKS: usize = 1_000_000;
const MAX_DISTANCE_PRIMITIVES: usize = 100_000;
const MAX_DISTANCE_CANDIDATES: usize = 1_000_000;
const MAX_INEXACT_DISTANCE_CANDIDATES: usize = 100_000;
const PLACEMENT_DECLARATION_ADAPTER: &str = "ratemypcb-placement-declaration";
const PLACEMENT_DECLARATION_VERSION: &str = "1";

fn declared_placement_angle(value: &str) -> Result<i64, String> {
    let value = parse_decimal_microdegrees(value).map_err(|error| error.to_string())?;
    if value.unsigned_abs() > 360_000_000 {
        return Err("rotation is outside one explicit revolution".into());
    }
    Ok(value.rem_euclid(360_000_000))
}

fn parse_declared_placements(
    source_path: &str,
    source: &str,
) -> Result<Vec<DeclaredAssemblyPlacement>, String> {
    if source.is_empty()
        || source.len() > 2 * 1024 * 1024
        || source_path.is_empty()
        || source_path.len() > 512
        || source_path.contains(['\\', '\0'])
    {
        return Err("placement source is absent or outside its bounds".into());
    }
    let mut numbered = source
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty());
    let (header_line, header) = numbered
        .next()
        .ok_or("placement declaration has no header")?;
    let delimiter = crate::delimiter_for(header);
    let headers = crate::normalized_headers(header, delimiter);
    let index = |names: &[&str]| -> Result<usize, String> {
        let matches = headers
            .iter()
            .enumerate()
            .filter(|(_, header)| names.contains(&header.as_str()))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [index] => Ok(*index),
            _ => Err(format!(
                "placement declaration field {} is absent or ambiguous",
                names[0]
            )),
        }
    };
    let reference = index(&["reference", "ref", "designator"])?;
    let x = index(&["posx", "x", "midx"])?;
    let y = index(&["posy", "y", "midy"])?;
    let rotation = index(&["rotation", "rot"])?;
    let side = index(&["side", "layer"])?;
    let revision = index(&["revision"])?;
    let unit = index(&["unit", "units"])?;
    let origin = index(&["origin"])?;
    let side_convention = index(&["sideconvention"])?;
    let bottom_mirroring = index(&["bottommirroring"])?;
    let rotation_direction = index(&["rotationdirection"])?;
    let fitted = index(&["fitted", "fittedstate"])?;
    let digest = crate::sha256(source.as_bytes());
    let mut references = BTreeSet::new();
    let mut placements = Vec::new();
    for (line_index, row) in numbered {
        if placements.len() >= crate::fabrication::MANUFACTURING_LIMITS.geometry_features
            || row.matches('"').count() % 2 != 0
        {
            return Err("placement rows exceed their bound or contain an open quote".into());
        }
        let values = crate::split_delimited(row, delimiter);
        if values.len() != headers.len() {
            return Err("placement row width does not match its header".into());
        }
        let value = |index: usize| {
            values
                .get(index)
                .map(String::as_str)
                .filter(|value| {
                    !value.is_empty()
                        && value.len() <= crate::fabrication::MANUFACTURING_LIMITS.max_text_bytes
                        && !value.chars().any(char::is_control)
                })
                .ok_or("placement row contains an empty or unbounded value")
        };
        let reference_value = value(reference)?;
        if !references.insert(reference_value.to_owned()) {
            return Err("placement references are duplicated".into());
        }
        let source_unit = match value(unit)? {
            "mm" => SourceUnit::Millimetre,
            "in" => SourceUnit::Inch,
            _ => return Err("placement unit is unknown".into()),
        };
        let convention = AssemblyPlacementConvention {
            unit: Some(source_unit),
            origin: match value(origin)? {
                "kicad_board" => AssemblyPlacementOrigin::KicadBoard,
                _ => return Err("placement origin is unknown".into()),
            },
            side: match value(side_convention)? {
                "top_bottom" => AssemblySideConvention::TopBottom,
                _ => return Err("placement side convention is unknown".into()),
            },
            bottom_mirroring: match value(bottom_mirroring)? {
                "mirrored" => AssemblyBottomMirroring::Mirrored,
                "unmirrored" => AssemblyBottomMirroring::Unmirrored,
                _ => return Err("placement bottom mirroring is unknown".into()),
            },
            rotation_direction: match value(rotation_direction)? {
                "counter_clockwise" => AssemblyRotationDirection::CounterClockwise,
                "clockwise" => AssemblyRotationDirection::Clockwise,
                _ => return Err("placement rotation direction is unknown".into()),
            },
        };
        let side_value = match value(side)? {
            "top" => LayerSide::Top,
            "bottom" => LayerSide::Bottom,
            _ => return Err("placement side is unknown".into()),
        };
        let fitted_value = match value(fitted)? {
            "fitted" => AssemblyFittedState::Fitted,
            "not_fitted" => AssemblyFittedState::NotFitted,
            _ => return Err("placement fitted state is unknown".into()),
        };
        let line = u64::try_from(line_index + 1).map_err(|_| "placement line overflow")?;
        placements.push(DeclaredAssemblyPlacement {
            id: declared_assembly_placement_id(source_path, &digest, line, reference_value)
                .map_err(|error| error.to_string())?,
            reference: reference_value.into(),
            side: side_value,
            position: CanonicalPoint {
                x: Picometres::parse_decimal(value(x)?, source_unit)
                    .map_err(|error| error.to_string())?,
                y: Picometres::parse_decimal(value(y)?, source_unit)
                    .map_err(|error| error.to_string())?,
            },
            rotation_microdegrees: declared_placement_angle(value(rotation)?)?,
            fitted: fitted_value,
            revision: value(revision)?.into(),
            convention,
            source_path: source_path.into(),
            artifact_digest: digest.clone(),
            line,
        });
    }
    if header_line != 0 || placements.is_empty() {
        return Err("placement declaration must start with one header and contain rows".into());
    }
    Ok(placements)
}

pub(crate) fn apply_declared_assembly_placements(
    review: &mut FabricationReview,
    placement: Option<(&str, &str)>,
) -> Result<(), Error> {
    review.assembly.declared_placements = placement
        .and_then(|(path, source)| parse_declared_placements(path, source).ok())
        .unwrap_or_default();
    review
        .refresh_digests()
        .and_then(|_| review.validate())
        .map_err(|error| Error::Invalid(format!("Invalid declared assembly evidence: {error}")))
}

#[derive(Clone, Debug)]
pub struct DfmDeclarations {
    source_path: String,
    artifact_digest: String,
    producer: String,
    producer_version: String,
    raw_bytes: u64,
    max_line_bytes: usize,
    max_text_bytes: usize,
    max_numeric_bytes: usize,
    records: Vec<DeclarationRecord>,
    inference_records: Vec<InferenceDeclarationRecord>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum DeclarationGroup {
    Rule,
    Order,
    Inference,
}

#[derive(Clone, Debug)]
struct DeclarationRecord {
    group: DeclarationGroup,
    record: u64,
    id: String,
    value: Option<Picometres>,
    source_value: Option<String>,
    unit: Option<SourceUnit>,
    declared_value: Option<String>,
    applicability: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InferenceDeclarationRecord {
    record: u64,
    id: String,
    state: String,
    model: String,
    model_version: String,
    applicability: String,
    target_ids: Vec<String>,
    limits: Vec<InferenceLimit>,
    parameters: Vec<InferenceParameter>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InferenceLimit {
    id: String,
    value: String,
    unit: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InferenceParameter {
    id: String,
    value: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawDeclarations {
    schema_version: String,
    producer: String,
    producer_version: String,
    issued_at_unix: u64,
    expires_at_unix: u64,
    state: String,
    rules: Vec<RawDeclarationRecord>,
    order_acknowledgements: Vec<RawDeclarationRecord>,
    #[serde(default)]
    inference_records: Vec<InferenceDeclarationRecord>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawDeclarationRecord {
    record: u64,
    id: String,
    state: String,
    value: Option<String>,
    unit: Option<String>,
    declared_value: Option<String>,
    applicability: String,
}

fn parse_scaled_inference_value(value: &str, factor: i128, maximum: i128) -> Result<i128, Error> {
    if value.is_empty()
        || value.len() > crate::fabrication::MANUFACTURING_LIMITS.max_numeric_bytes
        || value.starts_with(['+', '-'])
        || value.matches('.').count() > 1
    {
        return declaration_error("inference numeric value is malformed or unbounded");
    }
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || fraction.len() > 9
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return declaration_error("inference numeric value is malformed or unbounded");
    }
    let denominator = 10_i128
        .checked_pow(fraction.len() as u32)
        .ok_or_else(|| Error::Invalid("DFM declarations: inference scale overflow".into()))?;
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<i128>()
            .map_err(|_| Error::Invalid("DFM declarations: invalid inference number".into()))?
    };
    let mantissa = whole
        .parse::<i128>()
        .ok()
        .and_then(|whole| whole.checked_mul(denominator))
        .and_then(|whole| whole.checked_add(fraction))
        .ok_or_else(|| Error::Invalid("DFM declarations: inference numeric overflow".into()))?;
    let numerator = mantissa
        .checked_mul(factor)
        .ok_or_else(|| Error::Invalid("DFM declarations: inference numeric overflow".into()))?;
    if numerator % denominator != 0 {
        return declaration_error("inference value is finer than its declared unit model");
    }
    let value = numerator / denominator;
    if value <= 0 || value > maximum {
        return declaration_error("inference value is outside its named model range");
    }
    Ok(value)
}

fn inference_limit_value(limit: &InferenceLimit) -> Result<i128, Error> {
    if limit.value.is_empty()
        || limit.value.starts_with(['+', '-'])
        || limit.value.matches('.').count() > 1
        || limit.value.ends_with('.')
        || limit.value.len() > crate::fabrication::MANUFACTURING_LIMITS.max_numeric_bytes
    {
        return declaration_error("inference numeric value is malformed or unbounded");
    }
    const DISTANCES: &[&str] = &[
        "copper_thickness",
        "maximum_boundary_distance",
        "maximum_discontinuity",
        "minimum_component_clearance",
        "minimum_copper_width",
        "minimum_creepage",
        "minimum_profile_clearance",
        "probe_diameter",
        "tool_diameter",
    ];
    if DISTANCES.contains(&limit.id.as_str()) {
        let unit = match limit.unit.as_str() {
            "mm" => SourceUnit::Millimetre,
            "in" => SourceUnit::Inch,
            _ => return declaration_error("inference distance unit must be mm or in"),
        };
        let value = Picometres::parse_decimal(&limit.value, unit)
            .map_err(|error| Error::Invalid(format!("DFM declarations: {error}")))?;
        if value.0 <= 0 || value.0 > MAX_INFERENCE_DISTANCE_PM {
            return declaration_error("inference distance is outside 1 pm..1 m");
        }
        return Ok(i128::from(value.0));
    }
    let (factor, maximum) = match (limit.id.as_str(), limit.unit.as_str()) {
        ("frequency", "hz") => (1, 1_000_000_000_000_000),
        ("frequency", "khz") => (1_000, 1_000_000_000_000_000),
        ("frequency", "mhz") => (1_000_000, 1_000_000_000_000_000),
        ("frequency", "ghz") => (1_000_000_000, 1_000_000_000_000_000),
        ("maximum_skew", "ps") => (1, 1_000_000_000_000),
        ("maximum_skew", "ns") => (1_000, 1_000_000_000_000),
        ("maximum_skew", "us") => (1_000_000, 1_000_000_000_000),
        ("edge_rate", "v_per_ns") => (1_000, 1_000_000_000_000),
        ("edge_rate", "v_per_us") => (1, 1_000_000_000_000),
        ("current", "ma") => (1, 1_000_000_000),
        ("current", "a") => (1_000, 1_000_000_000),
        ("allowed_temperature_rise", "c") => (1_000, 1_000_000),
        ("allowed_voltage_drop" | "maximum_voltage", "mv") => (1, 1_000_000_000),
        ("allowed_voltage_drop" | "maximum_voltage", "v") => (1_000, 1_000_000_000),
        ("impedance", "mohm") => (1, 1_000_000_000_000),
        ("impedance", "ohm") => (1_000, 1_000_000_000_000),
        ("impedance_tolerance", "percent") => (1_000, 100_000),
        ("power", "mw") => (1, 1_000_000_000_000),
        ("power", "w") => (1_000, 1_000_000_000_000),
        ("minimum_copper_area", "mm2") => (1_000_000, 1_000_000_000_000),
        ("minimum_via_count", "count") => (1, 1_000_000),
        _ => return declaration_error("inference limit ID or unit is unknown"),
    };
    parse_scaled_inference_value(&limit.value, factor, maximum)
}

fn canonical_target_kind(value: &str) -> Option<&'static str> {
    for (prefix, kind) in [
        ("assembly-placement-v1-", "placement"),
        ("feature-v1-", "feature"),
        ("layer-v1-", "layer"),
        ("net-v1-", "net"),
        ("pad-v1-", "pin"),
    ] {
        if value
            .strip_prefix(prefix)
            .is_some_and(crate::lowercase_sha256)
        {
            return Some(kind);
        }
    }
    None
}

fn exact_inference_ids<T>(values: &[T], ids: impl Fn(&T) -> &str, expected: &[&str]) -> bool {
    values.iter().map(ids).eq(expected.iter().copied())
}

fn validate_inference_record(record: &InferenceDeclarationRecord) -> Result<(), Error> {
    if record.state != "complete"
        || record.record == 0
        || record.record > MAX_DECLARATION_RECORDS as u64
        || record.applicability != "board"
        || !valid_declaration_atom(&record.id)
        || !valid_declaration_atom(&record.model)
        || !valid_declaration_atom(&record.model_version)
        || record.target_ids.len() > MAX_INFERENCE_TARGETS
        || record.limits.len() > MAX_INFERENCE_FIELDS
        || record.parameters.len() > MAX_INFERENCE_FIELDS
        || !record.target_ids.windows(2).all(|pair| pair[0] < pair[1])
        || !record.limits.windows(2).all(|pair| pair[0].id < pair[1].id)
        || !record
            .parameters
            .windows(2)
            .all(|pair| pair[0].id < pair[1].id)
        || record
            .target_ids
            .iter()
            .any(|target| canonical_target_kind(target).is_none())
        || record.limits.iter().any(|limit| {
            !valid_declaration_atom(&limit.id)
                || limit.value.len() > MAX_DECLARATION_TEXT
                || !valid_declaration_atom(&limit.unit)
        })
        || record.parameters.iter().any(|parameter| {
            !valid_declaration_atom(&parameter.id) || !valid_declaration_text(&parameter.value)
        })
    {
        return declaration_error(
            "inference record state, location, applicability, IDs, ordering, or bounds are invalid",
        );
    }
    for limit in &record.limits {
        inference_limit_value(limit)?;
    }
    let limit_ids = |expected: &[&str]| {
        exact_inference_ids(&record.limits, |limit| limit.id.as_str(), expected)
    };
    let parameter_ids = |expected: &[&str]| {
        exact_inference_ids(
            &record.parameters,
            |parameter| parameter.id.as_str(),
            expected,
        )
    };
    let all_targets = |kind: &str| {
        !record.target_ids.is_empty()
            && record
                .target_ids
                .iter()
                .all(|target| canonical_target_kind(target) == Some(kind))
    };
    let exact_model = |model: &str| record.model == model && record.model_version == "1";
    let valid = match record.id.as_str() {
        "assembly_process_envelope" => {
            exact_model("assembly.component-copper-envelope-2d")
                && record.target_ids.is_empty()
                && limit_ids(&[
                    "minimum_component_clearance",
                    "minimum_profile_clearance",
                    "tool_diameter",
                ])
                && parameter_ids(&["process", "process_version", "tool", "tool_version"])
        }
        "probe_envelope" => {
            exact_model("assembly.testpoint-probe-envelope-2d")
                && record.target_ids.is_empty()
                && limit_ids(&[
                    "minimum_component_clearance",
                    "minimum_profile_clearance",
                    "probe_diameter",
                ])
                && parameter_ids(&["probe", "probe_version", "process", "process_version"])
        }
        "target_net_authority" => {
            exact_model("canonical.connectivity-net-set")
                && all_targets("net")
                && record.limits.is_empty()
                && record.parameters.is_empty()
        }
        "signal_intent" => {
            let limits = record
                .limits
                .iter()
                .map(|limit| limit.id.as_str())
                .collect::<Vec<_>>();
            exact_model("pcb.signal-intent")
                && all_targets("net")
                && matches!(
                    limits.as_slice(),
                    ["edge_rate"] | ["frequency"] | ["edge_rate", "frequency"]
                )
                && parameter_ids(&["signal_class"])
        }
        "reference_plane_intent" => {
            exact_model("pcb.return-path-discontinuity-envelope")
                && record
                    .target_ids
                    .iter()
                    .any(|target| canonical_target_kind(target) == Some("net"))
                && record
                    .target_ids
                    .iter()
                    .any(|target| canonical_target_kind(target) == Some("layer"))
                && record
                    .target_ids
                    .iter()
                    .all(|target| matches!(canonical_target_kind(target), Some("net" | "layer")))
                && limit_ids(&["maximum_discontinuity"])
                && parameter_ids(&["reference_plane_role"])
        }
        "current_intent" => {
            exact_model("pcb.current-intent")
                && all_targets("net")
                && limit_ids(&[
                    "allowed_temperature_rise",
                    "allowed_voltage_drop",
                    "current",
                ])
                && parameter_ids(&["current_class"])
        }
        "process_envelope" => {
            exact_model("pcb.minimum-copper-process-envelope")
                && all_targets("net")
                && limit_ids(&["copper_thickness", "minimum_copper_width"])
                && parameter_ids(&["finish", "process", "process_version"])
        }
        "voltage_domains" => {
            exact_model("pcb.voltage-domain-pair")
                && record.target_ids.len() == 2
                && all_targets("net")
                && limit_ids(&["maximum_voltage"])
                && parameter_ids(&["domain_pair"])
        }
        "creepage_rule" => {
            exact_model("pcb.creepage-distance-rule")
                && record.target_ids.len() == 2
                && all_targets("net")
                && limit_ids(&["minimum_creepage"])
                && parameter_ids(&["rule", "rule_version"])
        }
        "material_environment" => {
            exact_model("pcb.material-environment-coating")
                && record.target_ids.len() == 2
                && all_targets("net")
                && record.limits.is_empty()
                && parameter_ids(&["coating", "environment", "material"])
        }
        "differential_pair_intent" => {
            exact_model("pcb.differential-pair-intent")
                && record.target_ids.len() == 2
                && all_targets("net")
                && record.limits.is_empty()
                && parameter_ids(&["pair"])
        }
        "impedance_skew_target" => {
            exact_model("pcb.impedance-skew-envelope")
                && record.target_ids.len() == 2
                && all_targets("net")
                && limit_ids(&["impedance", "impedance_tolerance", "maximum_skew"])
                && parameter_ids(&["topology"])
        }
        "power_intent" => {
            exact_model("pcb.power-intent")
                && !record.target_ids.is_empty()
                && record.target_ids.iter().all(|target| {
                    matches!(canonical_target_kind(target), Some("placement" | "feature"))
                })
                && limit_ids(&["power"])
                && parameter_ids(&["dissipation_basis"])
        }
        "thermal_boundary_conditions" => {
            exact_model("pcb.minimum-thermal-geometry-envelope")
                && !record.target_ids.is_empty()
                && record.target_ids.iter().all(|target| {
                    matches!(canonical_target_kind(target), Some("placement" | "feature"))
                })
                && limit_ids(&[
                    "maximum_boundary_distance",
                    "minimum_copper_area",
                    "minimum_via_count",
                ])
                && parameter_ids(&["boundary", "copper_model", "via_model"])
        }
        "interface_intent" => {
            let connector = record
                .parameters
                .iter()
                .find(|parameter| parameter.id == "connector")
                .map(|parameter| parameter.value.as_str());
            exact_model("pcb.interface-pin-constraints")
                && record
                    .target_ids
                    .iter()
                    .filter(|target| canonical_target_kind(target) == Some("placement"))
                    .count()
                    == 1
                && connector.is_some_and(|connector| {
                    canonical_target_kind(connector) == Some("placement")
                        && record.target_ids.iter().any(|target| target == connector)
                })
                && record
                    .target_ids
                    .iter()
                    .any(|target| canonical_target_kind(target) == Some("feature"))
                && record
                    .target_ids
                    .iter()
                    .any(|target| canonical_target_kind(target) == Some("net"))
                && record.target_ids.iter().all(|target| {
                    matches!(
                        canonical_target_kind(target),
                        Some("placement" | "feature" | "net" | "pin")
                    )
                })
                && record.limits.is_empty()
                && parameter_ids(&[
                    "connector",
                    "pin_constraints",
                    "protocol",
                    "protocol_version",
                ])
        }
        _ => false,
    };
    if !valid {
        return declaration_error(
            "inference record model, version, targets, limits, or parameters are unknown or partial",
        );
    }
    Ok(())
}

impl DfmDeclarations {
    pub fn from_json(source_path: &str, bytes: &[u8], now_unix: u64) -> Result<Self, Error> {
        if bytes.is_empty() || bytes.len() > MAX_DECLARATION_BYTES {
            return declaration_error("input is empty or exceeds 256 KiB");
        }
        if !valid_declaration_path(source_path) {
            return declaration_error("source path must be a bounded relative path");
        }
        let raw: RawDeclarations = serde_json::from_slice(bytes)
            .map_err(|error| Error::Invalid(format!("DFM declarations: invalid JSON: {error}")))?;
        if raw.schema_version != DECLARATION_SCHEMA
            || raw.state != "complete"
            || !valid_declaration_text(&raw.producer)
            || !valid_declaration_text(&raw.producer_version)
            || raw.issued_at_unix > now_unix
            || raw.expires_at_unix <= now_unix
            || raw.expires_at_unix <= raw.issued_at_unix
        {
            return declaration_error("metadata is unknown, incomplete, future-dated, or stale");
        }
        let count = raw
            .rules
            .len()
            .checked_add(raw.order_acknowledgements.len())
            .and_then(|count| count.checked_add(raw.inference_records.len()))
            .ok_or_else(|| Error::Invalid("DFM declarations: record count overflow".into()))?;
        if count == 0 || count > MAX_DECLARATION_RECORDS {
            return declaration_error("record count is zero or exceeds 128");
        }
        let mut identities = BTreeSet::new();
        let mut locations = BTreeSet::new();
        let mut records = Vec::with_capacity(count);
        for (group, values) in [
            (DeclarationGroup::Rule, raw.rules),
            (DeclarationGroup::Order, raw.order_acknowledgements),
        ] {
            for value in values {
                if value.record == 0
                    || value.record > count as u64
                    || !locations.insert(value.record)
                    || !identities.insert((group, value.id.clone(), value.applicability.clone()))
                {
                    return declaration_error(
                        "record locations and IDs must be unique and bounded",
                    );
                }
                records.push(parse_declaration_record(value, group)?);
            }
        }
        let mut inference_records = raw.inference_records;
        for record in &inference_records {
            if record.record > count as u64
                || !locations.insert(record.record)
                || !identities.insert((
                    DeclarationGroup::Inference,
                    record.id.clone(),
                    record.applicability.clone(),
                ))
            {
                return declaration_error("record locations and IDs must be unique and bounded");
            }
            validate_inference_record(record)?;
        }
        records.sort_by_key(|record| record.record);
        inference_records.sort_by_key(|record| record.record);
        let max_line_bytes = bytes
            .split(|byte| *byte == b'\n')
            .map(<[u8]>::len)
            .max()
            .unwrap_or(0);
        let max_text_bytes =
            records
                .iter()
                .flat_map(|record| {
                    [
                        record.id.len(),
                        record.applicability.len(),
                        record.declared_value.as_deref().map(str::len).unwrap_or(0),
                    ]
                })
                .chain(inference_records.iter().flat_map(|record| {
                    [
                        record.id.len(),
                        record.model.len(),
                        record.model_version.len(),
                        record.applicability.len(),
                    ]
                    .into_iter()
                    .chain(record.target_ids.iter().map(String::len))
                    .chain(
                        record
                            .limits
                            .iter()
                            .flat_map(|limit| [limit.id.len(), limit.unit.len()].into_iter()),
                    )
                    .chain(record.parameters.iter().flat_map(|parameter| {
                        [parameter.id.len(), parameter.value.len()].into_iter()
                    }))
                }))
                .max()
                .unwrap_or(0)
                .max(raw.producer.len())
                .max(raw.producer_version.len());
        let max_numeric_bytes = records
            .iter()
            .filter_map(|record| record.source_value.as_deref().map(str::len))
            .chain(
                inference_records
                    .iter()
                    .flat_map(|record| record.limits.iter().map(|limit| limit.value.len())),
            )
            .max()
            .unwrap_or(0);
        Ok(Self {
            source_path: source_path.into(),
            artifact_digest: crate::sha256(bytes),
            producer: raw.producer,
            producer_version: raw.producer_version,
            raw_bytes: bytes.len() as u64,
            max_line_bytes,
            max_text_bytes,
            max_numeric_bytes,
            records,
            inference_records,
        })
    }

    pub fn source_path(&self) -> &str {
        &self.source_path
    }

    pub fn artifact_digest(&self) -> &str {
        &self.artifact_digest
    }

    fn document(&self) -> Result<ManufacturingDocument, Error> {
        let id = document_id(&self.artifact_digest, DocumentFormat::Unknown)
            .map_err(|error| Error::Invalid(format!("DFM declarations: {error}")))?;
        Ok(ManufacturingDocument {
            id,
            virtual_path: self.source_path.clone(),
            artifact_digest: self.artifact_digest.clone(),
            format: DocumentFormat::Unknown,
            adapter: DECLARATION_ADAPTER.into(),
            adapter_version: DECLARATION_SCHEMA.into(),
            parse_status: ParseStatus::Complete,
            numeric_format: None,
            metrics: DocumentMetrics {
                raw_bytes: self.raw_bytes,
                records: (self.records.len() + self.inference_records.len()) as u64 + 1,
                lexical_tokens: (self.records.len() + self.inference_records.len()) as u64,
                metadata_bytes: self.raw_bytes,
                max_line_bytes: self.max_line_bytes,
                max_text_bytes: self.max_text_bytes,
                max_numeric_bytes: self.max_numeric_bytes,
                max_nesting: 2,
                max_aperture_nesting: 0,
            },
        })
    }

    fn provenance(
        &self,
        document: &ManufacturingDocument,
        record: &DeclarationRecord,
    ) -> ManufacturingProvenance {
        ManufacturingProvenance {
            document_id: document.id.clone(),
            artifact_digest: self.artifact_digest.clone(),
            producer: self.producer.clone(),
            producer_version: self.producer_version.clone(),
            location: StructuralLocation {
                record: record.record,
                subrecord: None,
                byte_start: 0,
                byte_end: self.raw_bytes.saturating_sub(1),
            },
            source_lexeme: Some(if record.applicability.starts_with("layer") {
                record.id.clone()
            } else {
                format!("{}@{}", record.id, record.applicability)
            }),
        }
    }

    fn inference_provenance(
        &self,
        document: &ManufacturingDocument,
        record: &InferenceDeclarationRecord,
    ) -> ManufacturingProvenance {
        ManufacturingProvenance {
            document_id: document.id.clone(),
            artifact_digest: self.artifact_digest.clone(),
            producer: self.producer.clone(),
            producer_version: self.producer_version.clone(),
            location: StructuralLocation {
                record: record.record,
                subrecord: None,
                byte_start: 0,
                byte_end: self.raw_bytes.saturating_sub(1),
            },
            source_lexeme: Some(format!("inference:{}@{}", record.id, record.applicability)),
        }
    }
}

fn declaration_error<T>(message: &str) -> Result<T, Error> {
    Err(Error::Invalid(format!("DFM declarations: {message}")))
}

fn valid_declaration_path(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 512
        && !value.contains(['\\', '\0'])
        && Path::new(value)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn valid_declaration_atom(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_DECLARATION_TEXT
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-' | b'.')
        })
}

fn valid_declaration_text(value: &str) -> bool {
    !value.is_empty()
        && value.trim() == value
        && value.len() <= MAX_DECLARATION_TEXT
        && !value.chars().any(char::is_control)
}

fn parse_declaration_record(
    raw: RawDeclarationRecord,
    group: DeclarationGroup,
) -> Result<DeclarationRecord, Error> {
    if raw.state != "complete"
        || !valid_declaration_atom(&raw.id)
        || raw.applicability.trim() != raw.applicability
        || raw.applicability.len() > MAX_DECLARATION_TEXT
        || !(raw.applicability == "board"
            || raw
                .applicability
                .strip_prefix("layer:")
                .or_else(|| raw.applicability.strip_prefix("layer-id:"))
                .is_some_and(|layer| {
                    !layer.is_empty()
                        && layer.trim() == layer
                        && layer.len() <= 128
                        && !layer.chars().any(char::is_control)
                }))
    {
        return declaration_error("record state, ID, or applicability is invalid");
    }
    let numeric = match (group, raw.id.as_str()) {
        (
            DeclarationGroup::Rule,
            "minimum_drill"
            | "minimum_clearance"
            | "minimum_annular_ring"
            | "dfm.copper-edge.v1"
            | "dfm.mask-sliver.v1"
            | "dfm.paste-mask-relationship.v1",
        ) => true,
        (DeclarationGroup::Order, "total_thickness" | "layer_thickness") => true,
        (
            DeclarationGroup::Order,
            "finish" | "impedance" | "material" | "special_process" | "layer_material"
            | "drill_span_plating" | "castellation" | "edge_plating" | "stackup_order" | "profile",
        ) => false,
        _ => return declaration_error("record ID is unknown or in the wrong section"),
    };
    let layer_record = matches!(raw.id.as_str(), "layer_material" | "layer_thickness");
    let layer_applicability = raw.applicability.starts_with("layer");
    let layer_rule = group == DeclarationGroup::Rule && raw.id != "minimum_drill";
    if (layer_record && !layer_applicability)
        || (layer_applicability && !layer_record && !layer_rule)
    {
        return declaration_error("record applicability does not match its represented scope");
    }
    let (value, source_value, unit, declared_value) = if numeric {
        let source = raw
            .value
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| Error::Invalid("DFM declarations: numeric value is missing".into()))?;
        let unit = match raw.unit.as_deref() {
            Some("mm") => SourceUnit::Millimetre,
            Some("in") => SourceUnit::Inch,
            _ => return declaration_error("numeric unit must be mm or in"),
        };
        if raw.declared_value.is_some() {
            return declaration_error("numeric records cannot also carry declaredValue");
        }
        let value = Picometres::parse_decimal(source, unit)
            .map_err(|error| Error::Invalid(format!("DFM declarations: {error}")))?;
        if value.0 <= 0 {
            return declaration_error("numeric values must be positive");
        }
        (Some(value), Some(source.into()), Some(unit), None)
    } else {
        let declared = raw
            .declared_value
            .as_deref()
            .filter(|value| valid_declaration_text(value))
            .ok_or_else(|| Error::Invalid("DFM declarations: declaredValue is missing".into()))?;
        if raw.value.is_some() || raw.unit.is_some() {
            return declaration_error("text records cannot carry numeric value or unit");
        }
        (None, None, None, Some(declared.into()))
    };
    Ok(DeclarationRecord {
        group,
        record: raw.record,
        id: raw.id,
        value,
        source_value,
        unit,
        declared_value,
        applicability: raw.applicability,
    })
}

fn unit_name(unit: SourceUnit) -> &'static str {
    match unit {
        SourceUnit::Millimetre => "mm",
        SourceUnit::Inch => "in",
    }
}

fn declared_constraint_value(record: &DeclarationRecord) -> String {
    match (&record.source_value, record.unit, &record.declared_value) {
        (Some(value), Some(unit), None) => format!(
            "{}={value} {};applies={}",
            record.id,
            unit_name(unit),
            record.applicability
        ),
        (None, None, Some(value)) => {
            format!("{}={value};applies={}", record.id, record.applicability)
        }
        _ => unreachable!("validated declaration record shape"),
    }
}

fn constraint_kind(record: &DeclarationRecord) -> Option<ConstraintKind> {
    Some(match record.id.as_str() {
        "minimum_drill" => ConstraintKind::MinimumDrill,
        "minimum_clearance" => ConstraintKind::MinimumClearance,
        "minimum_annular_ring" => ConstraintKind::MinimumAnnularRing,
        "dfm.copper-edge.v1" | "dfm.mask-sliver.v1" | "dfm.paste-mask-relationship.v1" => {
            ConstraintKind::Other
        }
        "total_thickness" => ConstraintKind::FinishedThickness,
        "finish" => ConstraintKind::Finish,
        "impedance" => ConstraintKind::Impedance,
        "material" => ConstraintKind::Material,
        "special_process" => ConstraintKind::SpecialProcess,
        _ => return None,
    })
}

fn is_confirmation_gap(id: &str) -> bool {
    matches!(
        id,
        "drill_span_plating" | "castellation" | "edge_plating" | "profile"
    )
}

fn conflicts_with_existing_constraint(
    existing: &ManufacturingConstraint,
    record: &DeclarationRecord,
    kind: ConstraintKind,
) -> bool {
    if record.group == DeclarationGroup::Order
        || existing.kind != kind
        || (kind == ConstraintKind::Other
            && existing
                .declared_value
                .as_deref()
                .is_none_or(|value| !value.starts_with(&format!("{}=", record.id))))
    {
        return false;
    }
    let existing_applicability = existing
        .declared_value
        .as_deref()
        .and_then(|value| value.rsplit_once(";applies=").map(|(_, applies)| applies));
    existing_applicability.is_none_or(|applies| {
        applies == "board" || record.applicability == "board" || applies == record.applicability
    })
}

fn declaration_layer_id(
    review: &FabricationReview,
    applicability: &str,
) -> Result<Option<String>, Error> {
    let Some(target) = applicability
        .strip_prefix("layer-id:")
        .map(|id| (id, true))
        .or_else(|| {
            applicability
                .strip_prefix("layer:")
                .map(|name| (name, false))
        })
    else {
        return Ok(None);
    };
    let mut layers = review.layers.iter().filter(|layer| {
        if target.1 {
            layer.id == target.0
        } else {
            layer.name.as_deref() == Some(target.0)
        }
    });
    let layer_id = layers
        .next()
        .map(|layer| layer.id.clone())
        .filter(|_| layers.next().is_none())
        .ok_or_else(|| {
            Error::Invalid(format!(
                "DFM declarations: layer applicability {applicability} is missing or ambiguous"
            ))
        })?;
    Ok(Some(layer_id))
}

fn stackup_order_lexeme(index: usize, token: &str) -> String {
    let literal = format!("stackup_order[{index}]={token}");
    if literal.len() <= crate::fabrication::MANUFACTURING_LIMITS.max_numeric_bytes {
        literal
    } else {
        crate::sha256(format!("stackup-order-token-v1\0{index}\0{token}").as_bytes())
    }
}

fn stackup_order_layers(
    review: &FabricationReview,
    record: &DeclarationRecord,
    provenance: &ManufacturingProvenance,
) -> Result<Option<Vec<ConstructionLayer>>, Error> {
    let declared = record
        .declared_value
        .as_deref()
        .ok_or_else(|| Error::Invalid("DFM declarations: stackup order is missing".into()))?;
    let names = declared.split(',').collect::<Vec<_>>();
    if names.is_empty()
        || names.iter().any(|name| {
            name.is_empty()
                || name.trim() != *name
                || name.len() > 128
                || name.chars().any(char::is_control)
        })
        || names.iter().copied().collect::<BTreeSet<_>>().len() != names.len()
    {
        return declaration_error(
            "stackup order must be a unique exact comma-delimited layer list",
        );
    }
    let mut represented = Vec::with_capacity(names.len());
    for (index, name) in names.into_iter().enumerate() {
        let mut matches = review
            .layers
            .iter()
            .filter(|layer| layer.id == name || layer.name.as_deref() == Some(name));
        let Some(layer) = matches.next() else {
            return Ok(None);
        };
        if matches.next().is_some() {
            return Ok(None);
        }
        let mut source = provenance.clone();
        source.location.subrecord = Some((index + 1) as u32);
        source.source_lexeme = Some(stackup_order_lexeme(index, name));
        represented.push(ConstructionLayer {
            layer_id: Some(layer.id.clone()),
            material: None,
            thickness: None,
            authority: Authority::Explicit,
            provenance: source,
        });
    }
    Ok(Some(represented))
}

fn declaration_gap_coverage(
    declarations: &DfmDeclarations,
    record: &DeclarationRecord,
) -> Coverage {
    Coverage {
        id: format!("dfm-declaration-gap/{}", record.id),
        label: format!("{} order/profile confirmation", record.id.replace('_', " ")),
        status: CoverageStatus::Attention,
        evidence: format!(
            "{} {} record {} declares {};applies={} from {} {} but is not represented by the canonical construction/constraint contract; confirmation remains required.",
            declarations.source_path,
            record.id,
            record.record,
            record.declared_value.as_deref().unwrap_or("unrepresented"),
            record.applicability,
            declarations.producer,
            declarations.producer_version
        ),
    }
}

fn merge_declaration_capability(
    review: &mut FabricationReview,
    id: CapabilityId,
    document: &ManufacturingDocument,
    provenance: Vec<ManufacturingProvenance>,
    create: bool,
    detail: &str,
) -> Result<(), Error> {
    if provenance.is_empty() {
        return Ok(());
    }
    let matches = review
        .capabilities
        .records
        .iter()
        .enumerate()
        .filter(|(_, capability)| capability.id == id)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return declaration_error("capability authority is duplicated");
    }
    if let Some(index) = matches.first().copied() {
        let capability = &mut review.capabilities.records[index];
        if capability.state == CapabilityState::Complete {
            if !capability.document_ids.contains(&document.id) {
                capability.document_ids.push(document.id.clone());
                capability.document_ids.sort();
            }
            capability.provenance.extend(provenance);
            capability.provenance.sort_by(|left, right| {
                left.document_id
                    .cmp(&right.document_id)
                    .then_with(|| left.location.cmp(&right.location))
            });
            capability.provenance.dedup();
            capability.detail = detail.into();
        }
    } else if create {
        review.capabilities.records.push(CapabilityRecord {
            id,
            state: CapabilityState::Complete,
            authority: Authority::Explicit,
            document_ids: vec![document.id.clone()],
            provenance,
            detail: detail.into(),
        });
    }
    Ok(())
}

fn inference_declared_value(record: &InferenceDeclarationRecord) -> Result<String, Error> {
    let encoded = serde_json::to_string(record)
        .map_err(|error| Error::Invalid(format!("DFM declarations: {error}")))?;
    let value = format!("inference:{}={encoded}", record.id);
    if value.len() > crate::fabrication::MANUFACTURING_LIMITS.max_text_bytes {
        return declaration_error("normalized inference record exceeds canonical text bounds");
    }
    Ok(value)
}

pub(crate) fn apply_declarations(
    review: &mut FabricationReview,
    declarations: Option<&DfmDeclarations>,
) -> Result<Vec<Coverage>, Error> {
    let Some(declarations) = declarations else {
        return Ok(vec![Coverage {
            id: "dfm-declarations".into(),
            label: "Source-bound DFM declarations".into(),
            status: CoverageStatus::NotProvided,
            evidence: "No source/version-bound DFM declaration file was supplied.".into(),
        }]);
    };
    let mut candidate = review.clone();
    let document = declarations.document()?;
    if candidate
        .documents
        .iter()
        .any(|existing| existing.id == document.id || existing.adapter == DECLARATION_ADAPTER)
    {
        return declaration_error("declaration document identity is duplicated");
    }
    candidate.documents.push(document.clone());
    let mut coverage = Vec::new();
    let mut gap_count = 0;
    for record in &declarations.records {
        let layer_id = declaration_layer_id(&candidate, &record.applicability)?;
        let provenance = declarations.provenance(&document, record);
        if let Some(kind) = constraint_kind(record) {
            if candidate
                .constraints
                .iter()
                .any(|existing| conflicts_with_existing_constraint(existing, record, kind))
            {
                return declaration_error("duplicate or conflicting threshold/order authority");
            }
            candidate.constraints.push(ManufacturingConstraint {
                id: constraint_id(&document.id, kind, &provenance.location),
                kind,
                value: record.value,
                declared_value: Some(declared_constraint_value(record)),
                authority: Authority::Explicit,
                provenance,
            });
        } else if matches!(record.id.as_str(), "layer_material" | "layer_thickness") {
            candidate.construction.layers.push(ConstructionLayer {
                layer_id,
                material: (record.id == "layer_material")
                    .then(|| record.declared_value.clone())
                    .flatten(),
                thickness: (record.id == "layer_thickness")
                    .then_some(record.value)
                    .flatten(),
                authority: Authority::Explicit,
                provenance,
            });
        } else if record.id == "stackup_order" {
            if let Some(layers) = stackup_order_layers(&candidate, record, &provenance)? {
                candidate.construction.layers.extend(layers);
            } else {
                gap_count += 1;
                coverage.push(declaration_gap_coverage(declarations, record));
            }
        } else if is_confirmation_gap(&record.id) {
            gap_count += 1;
            coverage.push(declaration_gap_coverage(declarations, record));
        } else {
            return declaration_error("validated record has no canonical representation");
        }
    }
    for record in &declarations.inference_records {
        let provenance = declarations.inference_provenance(&document, record);
        candidate.constraints.push(ManufacturingConstraint {
            id: constraint_id(&document.id, ConstraintKind::Other, &provenance.location),
            kind: ConstraintKind::Other,
            value: None,
            declared_value: Some(inference_declared_value(record)?),
            authority: Authority::Explicit,
            provenance,
        });
    }
    let declared_constraints = candidate
        .constraints
        .iter()
        .filter(|constraint| constraint.provenance.document_id == document.id)
        .map(|constraint| constraint.provenance.clone())
        .collect::<Vec<_>>();
    merge_declaration_capability(
        &mut candidate,
        CapabilityId::Constraints,
        &document,
        declared_constraints,
        true,
        "Complete canonical constraints include source/version-bound declarations.",
    )?;
    let declared_construction = candidate
        .construction
        .layers
        .iter()
        .filter(|layer| layer.provenance.document_id == document.id)
        .map(|layer| layer.provenance.clone())
        .collect::<Vec<_>>();
    merge_declaration_capability(
        &mut candidate,
        CapabilityId::Construction,
        &document,
        declared_construction,
        false,
        "Complete canonical construction includes source/version-bound order/profile declarations.",
    )?;
    candidate
        .capabilities
        .records
        .sort_by_key(|record| record.id);
    candidate
        .refresh_digests()
        .and_then(|_| candidate.validate())
        .map_err(|error| {
            Error::Invalid(format!(
                "DFM declarations: invalid canonical merge: {error}"
            ))
        })?;
    *review = candidate;
    coverage.insert(
        0,
        Coverage {
            id: "dfm-declarations".into(),
            label: "Source-bound DFM declarations".into(),
            status: if gap_count == 0 {
                CoverageStatus::Passed
            } else {
                CoverageStatus::Attention
            },
            evidence: format!(
                "{} complete declaration record(s) from {} {} at {}; {gap_count} unrepresented acknowledgement(s) remain confirmation gaps.",
                declarations.records.len() + declarations.inference_records.len(),
                declarations.producer,
                declarations.producer_version,
                declarations.source_path
            ),
        },
    );
    Ok(coverage)
}

pub(crate) fn normalized_declaration_gaps(
    coverage: &[Coverage],
    evidence: Option<&[EvidenceRecord]>,
) -> Result<BTreeMap<String, String>, String> {
    let mut gaps = BTreeMap::new();
    for item in coverage {
        let check_id = if let Some(evidence) = evidence {
            evidence
                .iter()
                .find(|record| record.id == item.id && record.kind == "coverage")
                .map(|record| record.check_id.as_str())
                .unwrap_or(item.id.as_str())
        } else {
            item.id.as_str()
        };
        let Some(id) = check_id.strip_prefix("dfm-declaration-gap/") else {
            continue;
        };
        if !matches!(
            id,
            "drill_span_plating" | "castellation" | "edge_plating" | "profile" | "stackup_order"
        ) || item.status != CoverageStatus::Attention
            || item.evidence.trim().is_empty()
            || gaps.insert(id.to_string(), item.evidence.clone()).is_some()
        {
            return Err("declaration confirmation-gap evidence is malformed or duplicated".into());
        }
    }
    Ok(gaps)
}

pub(crate) fn declaration_document(
    review: &FabricationReview,
) -> Result<Option<&ManufacturingDocument>, Error> {
    let mut documents = review
        .documents
        .iter()
        .filter(|document| document.adapter == DECLARATION_ADAPTER);
    let document = documents.next();
    if documents.next().is_some() {
        return declaration_error("multiple declaration documents are retained");
    }
    if document.is_some_and(|document| {
        document.adapter_version != DECLARATION_SCHEMA
            || document.format != DocumentFormat::Unknown
            || document.parse_status != ParseStatus::Complete
            || document.numeric_format.is_some()
    }) {
        return declaration_error("declaration adapter identity or parse contract is invalid");
    }
    Ok(document)
}

#[derive(Clone, Copy)]
struct QualificationEvidence {
    precision_bps: Option<u16>,
    recall_bps: Option<u16>,
    positive_present: bool,
    hard_negative_present: bool,
    mutations_green: bool,
    fixture_digest: Option<&'static str>,
    reviewed_family_version: Option<&'static str>,
    reviewer: Option<&'static str>,
    inference_approval: Option<&'static str>,
}

#[derive(Clone, Copy)]
struct FamilyPolicy {
    key: &'static str,
    inference: bool,
    evidence: QualificationEvidence,
}

const UNQUALIFIED: QualificationEvidence = QualificationEvidence {
    precision_bps: None,
    recall_bps: None,
    positive_present: false,
    hard_negative_present: false,
    mutations_green: false,
    fixture_digest: None,
    reviewed_family_version: None,
    reviewer: None,
    inference_approval: None,
};

const POPULATION_QUALIFICATION: QualificationEvidence = QualificationEvidence {
    precision_bps: Some(10_000),
    recall_bps: Some(10_000),
    positive_present: true,
    hard_negative_present: true,
    mutations_green: true,
    fixture_digest: Some("985e199bdaf7fd8a59c1a6ca7f63937e5d6f772794e0d597a0ab631edad674f9"),
    reviewed_family_version: None,
    reviewer: None,
    inference_approval: None,
};

const OUTLINE_QUALIFICATION: QualificationEvidence = QualificationEvidence {
    precision_bps: Some(10_000),
    recall_bps: Some(10_000),
    positive_present: true,
    hard_negative_present: true,
    mutations_green: true,
    fixture_digest: Some("1ccc6e6831aa72daf5698a79ba46083a7f6585a9742cfa87219e488e5579a94f"),
    reviewed_family_version: None,
    reviewer: None,
    inference_approval: None,
};

const MINIMUM_DRILL_QUALIFICATION: QualificationEvidence = QualificationEvidence {
    precision_bps: Some(10_000),
    recall_bps: Some(10_000),
    positive_present: true,
    hard_negative_present: true,
    mutations_green: true,
    fixture_digest: Some("5d1aca9c2296ed543a65fc23421b9622ec200446df5076e3087c45117ddac831"),
    reviewed_family_version: None,
    reviewer: None,
    inference_approval: None,
};

const DRILL_TOOL_QUALIFICATION: QualificationEvidence = QualificationEvidence {
    precision_bps: Some(10_000),
    recall_bps: Some(10_000),
    positive_present: true,
    hard_negative_present: true,
    mutations_green: true,
    fixture_digest: Some("b502b7f44605d37aef01660212bb914ef2e5285a2202c791c2d80ba8036f7839"),
    reviewed_family_version: None,
    reviewer: None,
    inference_approval: None,
};

const COPPER_EDGE_QUALIFICATION: QualificationEvidence = QualificationEvidence {
    precision_bps: Some(10_000),
    recall_bps: Some(10_000),
    positive_present: true,
    hard_negative_present: true,
    mutations_green: true,
    fixture_digest: Some("6e6da925b794970ba80ba4433071732094a61c12cee8a369f7ffcd281bd7d6c8"),
    reviewed_family_version: None,
    reviewer: None,
    inference_approval: None,
};

const COPPER_CLEARANCE_QUALIFICATION: QualificationEvidence = QualificationEvidence {
    precision_bps: Some(10_000),
    recall_bps: Some(10_000),
    positive_present: true,
    hard_negative_present: true,
    mutations_green: true,
    fixture_digest: Some("273bdfeacf60f6f4fa4ad65e01dc71a96f93c6e10b29cf3ff00f8e2b09ce251a"),
    reviewed_family_version: None,
    reviewer: None,
    inference_approval: None,
};

const ANNULAR_RING_QUALIFICATION: QualificationEvidence = QualificationEvidence {
    precision_bps: Some(10_000),
    recall_bps: Some(10_000),
    positive_present: true,
    hard_negative_present: true,
    mutations_green: true,
    fixture_digest: Some("dca43e3dbb44268fa3ed9023bde7b7c61d126d5ae2544c625cac838c97fe9889"),
    reviewed_family_version: None,
    reviewer: None,
    inference_approval: None,
};

const MASK_SLIVER_QUALIFICATION: QualificationEvidence = QualificationEvidence {
    precision_bps: Some(10_000),
    recall_bps: Some(10_000),
    positive_present: true,
    hard_negative_present: true,
    mutations_green: true,
    fixture_digest: Some("de9ca50f2abf0e82be8f8d9cc8bfe9c2399647563f5ae325145044e8ceefbc1f"),
    reviewed_family_version: None,
    reviewer: None,
    inference_approval: None,
};

const PASTE_MASK_QUALIFICATION: QualificationEvidence = QualificationEvidence {
    precision_bps: Some(10_000),
    recall_bps: Some(10_000),
    positive_present: true,
    hard_negative_present: true,
    mutations_green: true,
    fixture_digest: Some("a635af2ebcffbdaaf882b1c5b66de7e8cbca76a0cba2fbceb31450f5e66ef163"),
    reviewed_family_version: None,
    reviewer: None,
    inference_approval: None,
};

const STACKUP_ORDER_QUALIFICATION: QualificationEvidence = QualificationEvidence {
    precision_bps: Some(10_000),
    recall_bps: Some(10_000),
    positive_present: true,
    hard_negative_present: true,
    mutations_green: true,
    fixture_digest: Some("d1269c18cd29b96f930096c82c78bce48e1d169ee578e18ff937007a6c2561f7"),
    reviewed_family_version: None,
    reviewer: None,
    inference_approval: None,
};

const TOTAL_THICKNESS_MATERIAL_QUALIFICATION: QualificationEvidence = QualificationEvidence {
    precision_bps: Some(10_000),
    recall_bps: Some(10_000),
    positive_present: true,
    hard_negative_present: true,
    mutations_green: true,
    fixture_digest: Some("b42e8b5a5fd348341f28781983ae9c3e1aada1917e607552667d8f6f4d85b6ce"),
    reviewed_family_version: None,
    reviewer: None,
    inference_approval: None,
};

const DRILL_SPAN_PLATING_QUALIFICATION: QualificationEvidence = QualificationEvidence {
    precision_bps: Some(10_000),
    recall_bps: Some(10_000),
    positive_present: true,
    hard_negative_present: true,
    mutations_green: true,
    fixture_digest: Some("b5410b2c0d14a207df977ed98f5d92217e0a6e0970fcd45587024e7e0e5b4b7b"),
    reviewed_family_version: None,
    reviewer: None,
    inference_approval: None,
};

const FINISH_PROFILE_QUALIFICATION: QualificationEvidence = QualificationEvidence {
    precision_bps: Some(10_000),
    recall_bps: Some(10_000),
    positive_present: true,
    hard_negative_present: true,
    mutations_green: true,
    fixture_digest: Some("ba41d4c170bccaaf8b7e43cef369c4cd70d18d457980e7bea74a448f694b3bd2"),
    reviewed_family_version: None,
    reviewer: None,
    inference_approval: None,
};

const IMPEDANCE_SPECIAL_PROCESS_QUALIFICATION: QualificationEvidence = QualificationEvidence {
    precision_bps: Some(10_000),
    recall_bps: Some(10_000),
    positive_present: true,
    hard_negative_present: true,
    mutations_green: true,
    fixture_digest: Some("b13115a2b858f67de82afc794c18b9799048c093d3f91787f1d71e934e76362a"),
    reviewed_family_version: None,
    reviewer: None,
    inference_approval: None,
};

const SIDE_ROTATION_QUALIFICATION: QualificationEvidence = QualificationEvidence {
    precision_bps: Some(10_000),
    recall_bps: Some(10_000),
    positive_present: true,
    hard_negative_present: true,
    mutations_green: true,
    fixture_digest: Some("8cb05c2df287af00c1aef079109c1c9b9d711ca59fe9de66415c20b2886e9a0f"),
    reviewed_family_version: None,
    reviewer: None,
    inference_approval: None,
};

const ASSEMBLY_PASTE_QUALIFICATION: QualificationEvidence = QualificationEvidence {
    precision_bps: Some(10_000),
    recall_bps: Some(10_000),
    positive_present: true,
    hard_negative_present: true,
    mutations_green: true,
    fixture_digest: Some("1f56f6a2f1ca81ebd774def1f822075dfef98e6b39eefaa44ac77f88d9474aee"),
    reviewed_family_version: None,
    reviewer: None,
    inference_approval: None,
};

const COURTYARD_NATIVE_QUALIFICATION: QualificationEvidence = QualificationEvidence {
    precision_bps: Some(10_000),
    recall_bps: Some(10_000),
    positive_present: true,
    hard_negative_present: true,
    mutations_green: true,
    fixture_digest: Some("1c547f62117dc30800305b9deac789a0dbf7be796a1dccc38596479c3213545a"),
    reviewed_family_version: None,
    reviewer: None,
    inference_approval: None,
};

const FOOTPRINT_STRING_QUALIFICATION: QualificationEvidence = QualificationEvidence {
    precision_bps: Some(10_000),
    recall_bps: Some(10_000),
    positive_present: true,
    hard_negative_present: true,
    mutations_green: true,
    fixture_digest: Some("8f8d93bb018140287172ed717acb793d8789c76ac9abce800411598a4859394b"),
    reviewed_family_version: None,
    reviewer: None,
    inference_approval: None,
};

const ASSEMBLY_ACCESS_QUALIFICATION: QualificationEvidence = QualificationEvidence {
    precision_bps: Some(10_000),
    recall_bps: Some(10_000),
    positive_present: true,
    hard_negative_present: true,
    mutations_green: true,
    fixture_digest: Some("0cd840929c657fb9b7cdff7cef8a4887cd77a9fc217dbca77f616dc89d7ebfb5"),
    reviewed_family_version: None,
    reviewer: None,
    inference_approval: None,
};

const TESTPOINT_ACCESS_QUALIFICATION: QualificationEvidence = QualificationEvidence {
    precision_bps: Some(10_000),
    recall_bps: Some(10_000),
    positive_present: true,
    hard_negative_present: true,
    mutations_green: true,
    fixture_digest: Some("19345edcc110b73234530a35fc381aa213abb6737bb4d1b55a99ca677bece0f0"),
    reviewed_family_version: None,
    reviewer: None,
    inference_approval: None,
};

const FAMILY_POLICIES: &[FamilyPolicy] = &[
    FamilyPolicy {
        key: OUTLINE_TOPOLOGY_FAMILY,
        inference: false,
        evidence: OUTLINE_QUALIFICATION,
    },
    FamilyPolicy {
        key: "dfm.minimum-finished-drill.v1",
        inference: false,
        evidence: MINIMUM_DRILL_QUALIFICATION,
    },
    FamilyPolicy {
        key: COPPER_CLEARANCE_FAMILY,
        inference: false,
        evidence: COPPER_CLEARANCE_QUALIFICATION,
    },
    FamilyPolicy {
        key: ANNULAR_RING_FAMILY,
        inference: false,
        evidence: ANNULAR_RING_QUALIFICATION,
    },
    FamilyPolicy {
        key: COPPER_EDGE_FAMILY,
        inference: false,
        evidence: COPPER_EDGE_QUALIFICATION,
    },
    FamilyPolicy {
        key: MASK_SLIVER_FAMILY,
        inference: false,
        evidence: MASK_SLIVER_QUALIFICATION,
    },
    FamilyPolicy {
        key: PASTE_MASK_FAMILY,
        inference: false,
        evidence: PASTE_MASK_QUALIFICATION,
    },
    FamilyPolicy {
        key: "dfm.drill-tool-integrity.v1",
        inference: false,
        evidence: DRILL_TOOL_QUALIFICATION,
    },
    FamilyPolicy {
        key: STACKUP_ORDER_FAMILY,
        inference: false,
        evidence: STACKUP_ORDER_QUALIFICATION,
    },
    FamilyPolicy {
        key: TOTAL_THICKNESS_MATERIAL_FAMILY,
        inference: false,
        evidence: TOTAL_THICKNESS_MATERIAL_QUALIFICATION,
    },
    FamilyPolicy {
        key: DRILL_SPAN_PLATING_FAMILY,
        inference: false,
        evidence: DRILL_SPAN_PLATING_QUALIFICATION,
    },
    FamilyPolicy {
        key: FINISH_PROFILE_FAMILY,
        inference: false,
        evidence: FINISH_PROFILE_QUALIFICATION,
    },
    FamilyPolicy {
        key: IMPEDANCE_SPECIAL_PROCESS_FAMILY,
        inference: false,
        evidence: IMPEDANCE_SPECIAL_PROCESS_QUALIFICATION,
    },
    FamilyPolicy {
        key: POPULATION_PARITY_FAMILY,
        inference: false,
        evidence: POPULATION_QUALIFICATION,
    },
    FamilyPolicy {
        key: SIDE_ROTATION_FAMILY,
        inference: false,
        evidence: SIDE_ROTATION_QUALIFICATION,
    },
    FamilyPolicy {
        key: ASSEMBLY_PASTE_FAMILY,
        inference: false,
        evidence: ASSEMBLY_PASTE_QUALIFICATION,
    },
    FamilyPolicy {
        key: COURTYARD_NATIVE_FAMILY,
        inference: false,
        evidence: COURTYARD_NATIVE_QUALIFICATION,
    },
    FamilyPolicy {
        key: FOOTPRINT_STRING_FAMILY,
        inference: false,
        evidence: FOOTPRINT_STRING_QUALIFICATION,
    },
    FamilyPolicy {
        key: ASSEMBLY_ACCESS_FAMILY,
        inference: true,
        evidence: ASSEMBLY_ACCESS_QUALIFICATION,
    },
    FamilyPolicy {
        key: TESTPOINT_ACCESS_FAMILY,
        inference: true,
        evidence: TESTPOINT_ACCESS_QUALIFICATION,
    },
    FamilyPolicy {
        key: "inference.return-path.v1",
        inference: true,
        evidence: UNQUALIFIED,
    },
    FamilyPolicy {
        key: "inference.high-current.v1",
        inference: true,
        evidence: UNQUALIFIED,
    },
    FamilyPolicy {
        key: "inference.creepage.v1",
        inference: true,
        evidence: UNQUALIFIED,
    },
    FamilyPolicy {
        key: "inference.differential.v1",
        inference: true,
        evidence: UNQUALIFIED,
    },
    FamilyPolicy {
        key: "inference.thermal.v1",
        inference: true,
        evidence: UNQUALIFIED,
    },
    FamilyPolicy {
        key: "inference.interface.v1",
        inference: true,
        evidence: UNQUALIFIED,
    },
];

fn family_policy(check_id: &str) -> Option<FamilyPolicy> {
    let key = check_id.split_once('/').map_or(check_id, |(key, _)| key);
    let mut matches = FAMILY_POLICIES.iter().filter(|policy| policy.key == key);
    let policy = *matches.next()?;
    matches.next().is_none().then_some(policy)
}

fn qualification_eligible(policy: FamilyPolicy) -> bool {
    let evidence = policy.evidence;
    evidence
        .precision_bps
        .is_some_and(|precision| (9_500..=10_000).contains(&precision))
        && evidence.recall_bps.is_some_and(|recall| recall <= 10_000)
        && evidence.positive_present
        && evidence.hard_negative_present
        && evidence.mutations_green
        && evidence.fixture_digest.is_some_and(crate::lowercase_sha256)
        && evidence.reviewed_family_version == Some(policy.key)
        && evidence
            .reviewer
            .is_some_and(|reviewer| !reviewer.is_empty() && reviewer.trim() == reviewer)
        && (!policy.inference || evidence.inference_approval == Some(policy.key))
}

fn family_gate_impact(check_id: &str) -> GateImpact {
    family_policy(check_id)
        .filter(|policy| qualification_eligible(*policy))
        .map_or(GateImpact::EvidenceOnly, |_| GateImpact::Blocking)
}

pub(crate) fn validate_gate_impacts(
    findings: &[Finding],
    evidence: &[EvidenceRecord],
) -> Result<(), String> {
    let check_ids = evidence
        .iter()
        .map(|record| (record.id.as_str(), record.check_id.as_str()))
        .collect::<BTreeMap<_, _>>();
    for finding in findings {
        let Some(check_id) = check_ids.get(finding.id.as_str()).copied() else {
            continue;
        };
        let phase_seven = family_policy(check_id).is_some()
            || check_id.starts_with("dfm.")
            || check_id.starts_with("assembly.")
            || check_id.starts_with("inference.");
        let expected = if check_id.contains("/gap/") {
            GateImpact::EvidenceOnly
        } else {
            family_gate_impact(check_id)
        };
        if phase_seven && finding.gate_impact != expected {
            return Err(format!("forged GateImpact for {check_id}"));
        }
    }
    Ok(())
}

fn family_key(check_id: &str) -> &str {
    check_id
        .split_once('/')
        .map_or(check_id, |(family, _)| family)
}

fn same_corrective_action(left: (&Finding, &str), right: (&Finding, &str)) -> bool {
    !left.0.recommendation.trim().is_empty()
        && family_key(left.1) == family_key(right.1)
        && left.0.recommendation == right.0.recommendation
}

pub(crate) fn top_unblock_evidence_refs(
    required_order: &[&str],
    required: &[RequiredEvidence],
    findings: &[Finding],
    evidence: &[EvidenceRecord],
) -> Result<BTreeSet<String>, String> {
    let mut evidence_by_id = BTreeMap::new();
    for record in evidence {
        if evidence_by_id.insert(record.id.as_str(), record).is_some() {
            return Err("duplicate evidence identity while ranking release unblocks".into());
        }
    }
    let order = required_order
        .iter()
        .enumerate()
        .map(|(index, check_id)| (*check_id, index))
        .collect::<BTreeMap<_, _>>();
    let mut incomplete = required
        .iter()
        .filter(|item| {
            item.execution != EvidenceExecution::Completed
                || item.result != EvidenceResult::Pass
                || !matches!(
                    item.freshness,
                    EvidenceFreshness::Current | EvidenceFreshness::NotApplicable
                )
        })
        .collect::<Vec<_>>();
    incomplete.sort_by(|left, right| {
        order
            .get(left.check_id.as_str())
            .unwrap_or(&usize::MAX)
            .cmp(order.get(right.check_id.as_str()).unwrap_or(&usize::MAX))
            .then_with(|| left.check_id.cmp(&right.check_id))
    });
    if let Some(item) = incomplete.first() {
        let record = evidence_by_id
            .get(item.evidence_id.as_str())
            .filter(|record| record.kind == "coverage" && record.check_id == item.check_id)
            .ok_or("top required unblock lacks canonical coverage evidence")?;
        return Ok(BTreeSet::from([record.id.clone()]));
    }

    let mut candidates = Vec::with_capacity(findings.len());
    for finding in findings {
        let record = evidence_by_id
            .get(finding.id.as_str())
            .filter(|record| record.kind == "finding")
            .ok_or("release-unblock finding lacks canonical evidence")?;
        candidates.push((finding, record.check_id.as_str()));
    }
    let mut blockers = candidates
        .iter()
        .copied()
        .filter(|(finding, _)| {
            finding.gate_impact == GateImpact::Blocking && finding.severity >= Severity::Medium
        })
        .collect::<Vec<_>>();
    blockers.sort_by(|left, right| {
        right
            .0
            .severity
            .cmp(&left.0.severity)
            .then_with(|| family_key(left.1).cmp(family_key(right.1)))
            .then_with(|| left.0.location.cmp(&right.0.location))
            .then_with(|| left.0.id.cmp(&right.0.id))
    });
    if let Some(top) = blockers.first().copied() {
        return Ok(blockers
            .into_iter()
            .filter(|candidate| {
                candidate.0.id == top.0.id || same_corrective_action(*candidate, top)
            })
            .map(|(finding, _)| finding.id.clone())
            .collect());
    }

    let mut attention = candidates
        .into_iter()
        .filter(|(finding, _)| finding.gate_impact == GateImpact::EvidenceOnly)
        .collect::<Vec<_>>();
    attention.sort_by(|left, right| {
        family_key(left.1)
            .cmp(family_key(right.1))
            .then_with(|| left.0.location.cmp(&right.0.location))
            .then_with(|| left.0.id.cmp(&right.0.id))
    });
    let Some(top) = attention.first().copied() else {
        return Ok(BTreeSet::new());
    };
    Ok(attention
        .into_iter()
        .filter(|candidate| candidate.0.id == top.0.id || same_corrective_action(*candidate, top))
        .map(|(finding, _)| finding.id.clone())
        .collect())
}

fn not_checked(id: &str, label: &str, reason: impl std::fmt::Display) -> Coverage {
    Coverage {
        id: id.into(),
        label: label.into(),
        status: CoverageStatus::NotRun,
        evidence: format!("not_checked: {reason}"),
    }
}

fn dispatch_complete(
    review: &FabricationReview,
    requirements: AnalyzerRequirements,
) -> Result<(), String> {
    let outcome = dispatch_analyzer(
        requirements,
        &review.capabilities,
        Some(SemanticAnalyzerResult::Pass),
    );
    if outcome.status != AnalyzerDispatchStatus::Pass {
        return Err(format!(
            "incomplete or duplicate capabilities: {:?}",
            outcome.incomplete_prerequisites
        ));
    }
    if requirements.prerequisites.iter().any(|required| {
        review.capabilities.records.iter().any(|record| {
            record.id == *required
                && matches!(
                    record.authority,
                    Authority::FilenameInference | Authority::Unknown
                )
        })
    }) {
        return Err("inferred or unknown capability authority cannot qualify".into());
    }
    Ok(())
}

fn affected_capability_evidence(review: &FabricationReview, ids: &[CapabilityId]) -> bool {
    review.omissions.iter().any(|omission| {
        omission
            .affected_capabilities
            .iter()
            .any(|id| ids.contains(id))
    }) || review.conflicts.iter().any(|conflict| {
        conflict
            .affected_capabilities
            .iter()
            .any(|id| ids.contains(id))
    })
}

fn source_link(
    review: &FabricationReview,
    provenance: &ManufacturingProvenance,
) -> Result<String, String> {
    let mut documents = review
        .documents
        .iter()
        .filter(|document| document.id == provenance.document_id);
    let document = documents
        .next()
        .filter(|_| documents.next().is_none())
        .ok_or("source document is missing or duplicated")?;
    if document.parse_status != ParseStatus::Complete
        || provenance.artifact_digest != document.artifact_digest
        || provenance.producer.trim().is_empty()
        || provenance.producer_version.trim().is_empty()
    {
        return Err("source provenance is incomplete or stale".into());
    }
    Ok(format!(
        "{}#record={}{} digest={} producer={} {}",
        document.virtual_path,
        provenance.location.record,
        provenance
            .location
            .subrecord
            .map(|value| format!(".{}", value))
            .unwrap_or_default(),
        document.artifact_digest,
        provenance.producer,
        provenance.producer_version
    ))
}

fn capability_retains_provenance(
    review: &FabricationReview,
    id: CapabilityId,
    provenance: &ManufacturingProvenance,
) -> bool {
    let mut capabilities = review
        .capabilities
        .records
        .iter()
        .filter(|capability| capability.id == id);
    capabilities.next().is_some_and(|capability| {
        capability.state == CapabilityState::Complete
            && capability.provenance.contains(provenance)
            && capabilities.next().is_none()
    })
}

fn validate_declaration_source_identity(
    review: &FabricationReview,
    declaration: &ManufacturingDocument,
) -> Result<(), String> {
    let mut identity = None;
    let mut count = 0_usize;
    for provenance in review
        .constraints
        .iter()
        .map(|constraint| &constraint.provenance)
        .chain(
            review
                .construction
                .layers
                .iter()
                .map(|layer| &layer.provenance),
        )
        .filter(|provenance| provenance.document_id == declaration.id)
    {
        source_link(review, provenance)?;
        let observed = (
            provenance.producer.as_str(),
            provenance.producer_version.as_str(),
        );
        if identity.is_some_and(|expected| expected != observed) {
            return Err("declaration source/version identity is inconsistent".into());
        }
        identity = Some(observed);
        count = count
            .checked_add(1)
            .ok_or("declaration provenance count overflow")?;
    }
    if count == 0 {
        return Err("declaration has no represented source/version-bound authority".into());
    }
    Ok(())
}

fn declaration_link(review: &FabricationReview) -> String {
    match declaration_document(review) {
        Ok(Some(document)) => format!(
            "declaration_source={} digest={} adapter={} {}",
            document.virtual_path,
            document.artifact_digest,
            document.adapter,
            document.adapter_version
        ),
        Ok(None) => "declaration_source=not_provided".into(),
        Err(error) => format!("declaration_source=invalid ({error})"),
    }
}

fn confirmation_gap_finding(
    review: &FabricationReview,
    family: &str,
    concept: &str,
    reason: impl std::fmt::Display,
) -> Finding {
    Finding {
        id: format!("{family}/gap/{concept}"),
        severity: Severity::Low,
        category: "DFM".into(),
        title: format!("{} confirmation is required", concept.replace('-', " ")),
        evidence: format!(
            "outcome=confirmation_gap concept={concept} {reason}; {}",
            declaration_link(review)
        ),
        recommendation: format!(
            "Obtain one exact source/version-bound {concept} acknowledgement and retain both compared source locations."
        ),
        location: format!("concept={concept}"),
        source: "fabrication".into(),
        gate_impact: GateImpact::EvidenceOnly,
    }
}

fn customer_constraint<'a>(
    review: &'a FabricationReview,
    declaration: &ManufacturingDocument,
    kind: ConstraintKind,
    id: &str,
) -> Result<Option<&'a ManufacturingConstraint>, String> {
    let mut matches = review.constraints.iter().filter(|constraint| {
        constraint.kind == kind && constraint.provenance.document_id == declaration.id
    });
    let constraint = matches.next();
    if matches.next().is_some() {
        return Err(format!("{id} order authority is duplicated"));
    }
    let Some(constraint) = constraint else {
        return Ok(None);
    };
    validate_declaration_source_identity(review, declaration)?;
    let declared = constraint
        .declared_value
        .as_deref()
        .ok_or_else(|| format!("{id} declaration text is missing"))?;
    if constraint.authority != Authority::Explicit
        || constraint.provenance.artifact_digest != declaration.artifact_digest
        || constraint.provenance.source_lexeme.as_deref() != Some(&format!("{id}@board"))
        || !capability_retains_provenance(review, CapabilityId::Constraints, &constraint.provenance)
        || !declared.starts_with(&format!("{id}="))
        || !declared.ends_with(";applies=board")
    {
        return Err(format!(
            "{id} is not exact production-normalized order authority"
        ));
    }
    source_link(review, &constraint.provenance)?;
    Ok(Some(constraint))
}

fn design_constraint<'a>(
    review: &'a FabricationReview,
    declaration: &ManufacturingDocument,
    kind: ConstraintKind,
    concept: &str,
) -> Result<Option<&'a ManufacturingConstraint>, String> {
    let mut matches = review.constraints.iter().filter(|constraint| {
        constraint.kind == kind && constraint.provenance.document_id != declaration.id
    });
    let constraint = matches.next();
    if matches.next().is_some() {
        return Err(format!("design-side {concept} authority is duplicated"));
    }
    let Some(constraint) = constraint else {
        return Ok(None);
    };
    if matches!(
        constraint.authority,
        Authority::FilenameInference | Authority::Unknown
    ) || !capability_retains_provenance(
        review,
        CapabilityId::Constraints,
        &constraint.provenance,
    ) || constraint
        .declared_value
        .as_deref()
        .is_none_or(|value| !valid_declaration_text(value))
    {
        return Err(format!("design-side {concept} authority is incomplete"));
    }
    source_link(review, &constraint.provenance)?;
    Ok(Some(constraint))
}

fn customer_text<'a>(constraint: &'a ManufacturingConstraint, id: &str) -> Result<&'a str, String> {
    constraint
        .declared_value
        .as_deref()
        .and_then(|value| value.strip_prefix(&format!("{id}=")))
        .and_then(|value| value.strip_suffix(";applies=board"))
        .filter(|value| valid_declaration_text(value))
        .ok_or_else(|| format!("{id} declaration string is malformed"))
}

fn stackup_order_confirmation(
    review: &FabricationReview,
    deadline: ManufacturingDeadline,
) -> (Vec<Finding>, Coverage) {
    const LABEL: &str = "Source-authoritative stackup layer order confirmation";
    let result = (|| -> Result<(Vec<Finding>, Coverage), String> {
        dispatch_complete(review, STACKUP_ORDER_REQUIREMENTS)?;
        if affected_capability_evidence(
            review,
            &[CapabilityId::Construction, CapabilityId::LayerOrder],
        ) {
            return Err("affected omission or conflict prevents stackup confirmation".into());
        }
        let declaration = declaration_document(review)
            .map_err(|error| error.to_string())?
            .ok_or("source/version-bound stackup order authority is missing")?;
        validate_declaration_source_identity(review, declaration)?;
        let mut required = review
            .construction
            .layers
            .iter()
            .filter(|layer| {
                layer.provenance.document_id == declaration.id
                    && layer.provenance.location.subrecord.is_some()
                    && layer.material.is_none()
                    && layer.thickness.is_none()
            })
            .collect::<Vec<_>>();
        required.sort_by_key(|layer| layer.provenance.location.subrecord);
        if required.is_empty()
            || required.iter().enumerate().any(|(index, layer)| {
                layer.authority != Authority::Explicit
                    || layer.material.is_some()
                    || layer.thickness.is_some()
                    || layer.provenance.artifact_digest != declaration.artifact_digest
                    || !capability_retains_provenance(
                        review,
                        CapabilityId::Construction,
                        &layer.provenance,
                    )
                    || layer.provenance.location.subrecord != Some((index + 1) as u32)
                    || layer.layer_id.as_deref().is_none_or(str::is_empty)
            })
        {
            return Err("stackup order authority is missing, duplicated, or malformed".into());
        }
        let order_record = required[0].provenance.location.record;
        let canonical_ids = required
            .iter()
            .map(|layer| layer.layer_id.as_deref().unwrap())
            .collect::<BTreeSet<_>>();
        if canonical_ids.len() != required.len()
            || required.iter().enumerate().any(|(index, layer)| {
                let layer_id = layer.layer_id.as_deref().unwrap();
                let source_lexeme = layer
                    .provenance
                    .source_lexeme
                    .as_deref()
                    .unwrap_or_default();
                let mut token_matches = review.layers.iter().filter(|candidate| {
                    stackup_order_lexeme(index, &candidate.id) == source_lexeme
                        || candidate
                            .name
                            .as_deref()
                            .is_some_and(|name| stackup_order_lexeme(index, name) == source_lexeme)
                });
                let resolved = token_matches.next();
                layer.provenance.location.record != order_record
                    || resolved.is_none_or(|resolved| resolved.id != layer_id)
                    || token_matches.next().is_some()
            })
        {
            return Err("stackup order source record is inconsistent or duplicated".into());
        }
        for layer_id in &canonical_ids {
            let mut candidates = review
                .layers
                .iter()
                .filter(|candidate| candidate.id == **layer_id);
            let candidate = candidates.next();
            if candidate.is_none_or(|candidate| {
                candidate.order.is_none()
                    || matches!(
                        candidate.authority,
                        Authority::FilenameInference | Authority::Unknown
                    )
                    || !capability_retains_provenance(
                        review,
                        CapabilityId::LayerOrder,
                        &candidate.provenance,
                    )
            }) || candidates.next().is_some()
            {
                return Err(
                    "declared design layer order is missing, duplicated, or ambiguous".into(),
                );
            }
            source_link(review, &candidate.unwrap().provenance)?;
        }
        for layer in &required {
            source_link(review, &layer.provenance)?;
        }
        let mut design = review
            .layers
            .iter()
            .filter(|layer| layer.order.is_some())
            .collect::<Vec<_>>();
        design.sort_by(|left, right| {
            left.order
                .cmp(&right.order)
                .then_with(|| left.id.cmp(&right.id))
        });
        if design.is_empty()
            || design.iter().any(|layer| {
                matches!(
                    layer.authority,
                    Authority::FilenameInference | Authority::Unknown
                )
            })
            || design.iter().any(|layer| {
                !capability_retains_provenance(review, CapabilityId::LayerOrder, &layer.provenance)
            })
            || design
                .iter()
                .filter_map(|layer| layer.order)
                .collect::<BTreeSet<_>>()
                .len()
                != design.len()
        {
            return Err("design layer order is missing, inferred, or ambiguous".into());
        }
        for layer in &design {
            deadline
                .check("dfm-stackup-order-confirmation")
                .map_err(|error| error.to_string())?;
            source_link(review, &layer.provenance)?;
        }
        let requirement_ids = required
            .iter()
            .map(|layer| layer.layer_id.as_deref().unwrap())
            .collect::<Vec<_>>();
        let design_ids = design
            .iter()
            .map(|layer| layer.id.as_str())
            .collect::<Vec<_>>();
        let requirement_names = requirement_ids
            .iter()
            .map(|id| {
                review
                    .layers
                    .iter()
                    .find(|layer| layer.id == *id)
                    .and_then(|layer| layer.name.as_deref())
                    .unwrap_or(id)
            })
            .collect::<Vec<_>>();
        let design_names = design
            .iter()
            .map(|layer| layer.name.as_deref().unwrap_or(&layer.id))
            .collect::<Vec<_>>();
        let conflict = requirement_ids != design_ids;
        let evidence = format!(
            "outcome={} design_order={:?} requirement_order={:?} design_sources=[{}] requirement_sources=[{}]",
            if conflict { "conflict" } else { "match" },
            design_names,
            requirement_names,
            design
                .iter()
                .map(|layer| source_link(review, &layer.provenance))
                .collect::<Result<Vec<_>, _>>()?
                .join(", "),
            required
                .iter()
                .map(|layer| source_link(review, &layer.provenance))
                .collect::<Result<Vec<_>, _>>()?
                .join(", ")
        );
        let findings = conflict
            .then(|| Finding {
                id: format!("{STACKUP_ORDER_FAMILY}/conflict"),
                severity: Severity::Medium,
                category: "DFM".into(),
                title: "Design stackup order conflicts with the customer acknowledgement".into(),
                evidence: evidence.clone(),
                recommendation:
                    "Resolve the exact represented layer order and retain revised dual-source acknowledgement."
                        .into(),
                location: format!(
                    "design_document={};requirement_document={};requirement_record={}",
                    design[0].document_id,
                    declaration.id,
                    required[0].provenance.location.record
                ),
                source: "fabrication".into(),
                gate_impact: family_gate_impact(STACKUP_ORDER_FAMILY),
            })
            .into_iter()
            .collect();
        Ok((
            findings,
            Coverage {
                id: STACKUP_ORDER_FAMILY.into(),
                label: LABEL.into(),
                status: if conflict {
                    CoverageStatus::Attention
                } else {
                    CoverageStatus::Passed
                },
                evidence,
            },
        ))
    })();
    result.unwrap_or_else(|reason| {
        (
            vec![confirmation_gap_finding(
                review,
                STACKUP_ORDER_FAMILY,
                "stackup-order",
                &reason,
            )],
            not_checked(STACKUP_ORDER_FAMILY, LABEL, reason),
        )
    })
}

fn total_thickness_material(
    review: &FabricationReview,
    deadline: ManufacturingDeadline,
) -> (Vec<Finding>, Coverage) {
    const LABEL: &str = "Source-authoritative thickness and material confirmation";
    let result = (|| -> Result<(Vec<Finding>, Coverage), String> {
        dispatch_complete(review, TOTAL_THICKNESS_MATERIAL_REQUIREMENTS)?;
        if affected_capability_evidence(
            review,
            &[CapabilityId::Construction, CapabilityId::Constraints],
        ) {
            return Err("affected omission or conflict prevents construction confirmation".into());
        }
        let declaration = declaration_document(review)
            .map_err(|error| error.to_string())?
            .ok_or("source/version-bound thickness/material authority is missing")?;
        let required_total = customer_constraint(
            review,
            declaration,
            ConstraintKind::FinishedThickness,
            "total_thickness",
        )?
        .ok_or("customer total thickness acknowledgement is missing")?;
        let design_total = design_constraint(
            review,
            declaration,
            ConstraintKind::FinishedThickness,
            "total thickness",
        )?
        .ok_or("design total thickness authority is missing")?;
        let required_total_value = required_total
            .value
            .filter(|value| value.0 > 0)
            .ok_or("customer total thickness value is missing")?;
        let design_total_value = design_total
            .value
            .filter(|value| value.0 > 0)
            .ok_or("design total thickness value is missing")?;
        if review.construction.total_thickness != Some(design_total_value) {
            return Err(
                "design total thickness constraint and construction evidence disagree".into(),
            );
        }

        let required_material =
            customer_constraint(review, declaration, ConstraintKind::Material, "material")?
                .ok_or("customer material acknowledgement is missing")?;
        let required_material_value = customer_text(required_material, "material")?;
        let design_material =
            design_constraint(review, declaration, ConstraintKind::Material, "material")?
                .ok_or("design material authority is missing")?;
        let design_material_value = design_material
            .declared_value
            .as_deref()
            .ok_or("design material string is missing")?;

        let mut customer_layers = BTreeMap::<(String, &'static str), &ConstructionLayer>::new();
        for required in review.construction.layers.iter().filter(|layer| {
            layer.provenance.document_id == declaration.id
                && matches!(
                    layer.provenance.source_lexeme.as_deref(),
                    Some("layer_material" | "layer_thickness")
                )
        }) {
            deadline
                .check("dfm-total-thickness-material")
                .map_err(|error| error.to_string())?;
            if required.authority != Authority::Explicit
                || required.provenance.artifact_digest != declaration.artifact_digest
                || required.layer_id.as_deref().is_none_or(str::is_empty)
                || !capability_retains_provenance(
                    review,
                    CapabilityId::Construction,
                    &required.provenance,
                )
                || (required.material.is_some() == required.thickness.is_some())
            {
                return Err("per-layer customer authority is malformed".into());
            }
            source_link(review, &required.provenance)?;
            let kind = if required.material.is_some() {
                "material"
            } else {
                "thickness"
            };
            let key = (required.layer_id.clone().unwrap(), kind);
            if customer_layers.insert(key, required).is_some() {
                return Err("per-layer customer authority is duplicated".into());
            }
        }

        let mut design_layers = BTreeMap::<(String, &'static str), &ConstructionLayer>::new();
        for design in review
            .construction
            .layers
            .iter()
            .filter(|layer| layer.provenance.document_id != declaration.id)
        {
            if design.material.is_none() && design.thickness.is_none() {
                continue;
            }
            if design.layer_id.as_deref().is_none_or(str::is_empty)
                || matches!(
                    design.authority,
                    Authority::FilenameInference | Authority::Unknown
                )
                || !capability_retains_provenance(
                    review,
                    CapabilityId::Construction,
                    &design.provenance,
                )
            {
                return Err("per-layer design authority is missing, inferred, or unknown".into());
            }
            source_link(review, &design.provenance)?;
            for (kind, represented) in [
                ("material", design.material.is_some()),
                ("thickness", design.thickness.is_some()),
            ] {
                if represented
                    && design_layers
                        .insert((design.layer_id.clone().unwrap(), kind), design)
                        .is_some()
                {
                    return Err("per-layer design authority is duplicated".into());
                }
            }
        }
        if customer_layers.is_empty() && design_layers.is_empty() {
            return Err("per-layer material and thickness authority is missing".into());
        }

        let customer_keys = customer_layers.keys().cloned().collect::<BTreeSet<_>>();
        let design_keys = design_layers.keys().cloned().collect::<BTreeSet<_>>();
        let mut findings = Vec::new();
        let mut details = Vec::new();
        let mut has_gaps = false;
        for (layer_id, kind) in design_keys.difference(&customer_keys) {
            has_gaps = true;
            let design = design_layers
                .get(&(layer_id.clone(), *kind))
                .expect("key came from design map");
            let concept = format!("customer-layer-{kind}-{layer_id}");
            let reason = format!(
                "customer layer {kind} counterpart is missing for layer {layer_id}; design_source={}",
                source_link(review, &design.provenance)?
            );
            details.push(format!(
                "layer={layer_id} kind={kind} outcome=confirmation_gap missing=customer"
            ));
            findings.push(confirmation_gap_finding(
                review,
                TOTAL_THICKNESS_MATERIAL_FAMILY,
                &concept,
                reason,
            ));
        }
        for (layer_id, kind) in customer_keys.difference(&design_keys) {
            has_gaps = true;
            let required = customer_layers
                .get(&(layer_id.clone(), *kind))
                .expect("key came from customer map");
            let concept = format!("design-layer-{kind}-{layer_id}");
            let reason = format!(
                "design layer {kind} counterpart is missing for layer {layer_id}; requirement_source={}",
                source_link(review, &required.provenance)?
            );
            details.push(format!(
                "layer={layer_id} kind={kind} outcome=confirmation_gap missing=design"
            ));
            findings.push(confirmation_gap_finding(
                review,
                TOTAL_THICKNESS_MATERIAL_FAMILY,
                &concept,
                reason,
            ));
        }
        for (layer_id, kind) in design_keys.intersection(&customer_keys) {
            deadline
                .check("dfm-total-thickness-material")
                .map_err(|error| error.to_string())?;
            let design = design_layers
                .get(&(layer_id.clone(), *kind))
                .expect("key came from design map");
            let required = customer_layers
                .get(&(layer_id.clone(), *kind))
                .expect("key came from customer map");
            let design_source = source_link(review, &design.provenance)?;
            let requirement_source = source_link(review, &required.provenance)?;
            let (design_value, requirement_value, conflict) = if *kind == "material" {
                let design = design.material.as_deref().unwrap();
                let required = required.material.as_deref().unwrap();
                (design.to_string(), required.to_string(), design != required)
            } else {
                let design = design.thickness.unwrap().0;
                let required = required.thickness.unwrap().0;
                (
                    format!("{design}pm"),
                    format!("{required}pm"),
                    design != required,
                )
            };
            details.push(format!(
                "layer={layer_id} kind={kind} outcome={} design={design_value:?} requirement={requirement_value:?} design_source={design_source} requirement_source={requirement_source}",
                if conflict { "conflict" } else { "match" }
            ));
            if conflict {
                findings.push(Finding {
                    id: format!("{TOTAL_THICKNESS_MATERIAL_FAMILY}/conflict/{kind}/{layer_id}"),
                    severity: Severity::Medium,
                    category: "DFM".into(),
                    title: format!("Layer {kind} conflicts with the customer acknowledgement"),
                    evidence: details.last().unwrap().clone(),
                    recommendation: format!(
                        "Resolve the exact represented layer {kind} and retain revised dual-source acknowledgement."
                    ),
                    location: format!(
                        "layer={layer_id};design_record={};requirement_record={}",
                        design.provenance.location.record, required.provenance.location.record
                    ),
                    source: "fabrication".into(),
                    gate_impact: family_gate_impact(TOTAL_THICKNESS_MATERIAL_FAMILY),
                });
            }
        }

        let total_conflict = design_total_value != required_total_value;
        let total_evidence = format!(
            "concept=total_thickness outcome={} design={}pm requirement={}pm design_declared={:?} requirement_declared={:?} design_source={} requirement_source={}",
            if total_conflict { "conflict" } else { "match" },
            design_total_value.0,
            required_total_value.0,
            design_total.declared_value,
            required_total.declared_value,
            source_link(review, &design_total.provenance)?,
            source_link(review, &required_total.provenance)?
        );
        details.push(total_evidence.clone());
        if total_conflict {
            findings.push(Finding {
                id: format!("{TOTAL_THICKNESS_MATERIAL_FAMILY}/conflict/total-thickness"),
                severity: Severity::Medium,
                category: "DFM".into(),
                title: "Total thickness conflicts with the customer acknowledgement".into(),
                evidence: total_evidence,
                recommendation:
                    "Resolve the exact represented total thickness and retain revised dual-source acknowledgement."
                        .into(),
                location: format!(
                    "design_record={};requirement_record={}",
                    design_total.provenance.location.record,
                    required_total.provenance.location.record
                ),
                source: "fabrication".into(),
                gate_impact: family_gate_impact(TOTAL_THICKNESS_MATERIAL_FAMILY),
            });
        }
        let material_conflict = design_material_value != required_material_value;
        let material_evidence = format!(
            "concept=material outcome={} design={design_material_value:?} requirement={required_material_value:?} design_source={} requirement_source={}",
            if material_conflict {
                "conflict"
            } else {
                "match"
            },
            source_link(review, &design_material.provenance)?,
            source_link(review, &required_material.provenance)?
        );
        details.push(material_evidence.clone());
        if material_conflict {
            findings.push(Finding {
                id: format!("{TOTAL_THICKNESS_MATERIAL_FAMILY}/conflict/material"),
                severity: Severity::Medium,
                category: "DFM".into(),
                title: "Material conflicts with the customer acknowledgement".into(),
                evidence: material_evidence,
                recommendation:
                    "Resolve the exact declared material string and retain revised dual-source acknowledgement."
                        .into(),
                location: format!(
                    "design_record={};requirement_record={}",
                    design_material.provenance.location.record,
                    required_material.provenance.location.record
                ),
                source: "fabrication".into(),
                gate_impact: family_gate_impact(TOTAL_THICKNESS_MATERIAL_FAMILY),
            });
        }
        findings.sort_by(|left, right| left.id.cmp(&right.id));
        details.sort();
        let has_conflicts = findings
            .iter()
            .any(|finding| finding.id.contains("/conflict/"));
        let evidence = details.join("; ");
        Ok((
            findings,
            Coverage {
                id: TOTAL_THICKNESS_MATERIAL_FAMILY.into(),
                label: LABEL.into(),
                status: if has_gaps {
                    CoverageStatus::NotRun
                } else if has_conflicts {
                    CoverageStatus::Attention
                } else {
                    CoverageStatus::Passed
                },
                evidence: if has_gaps {
                    format!("not_checked: incomplete per-layer construction authority; {evidence}")
                } else {
                    evidence
                },
            },
        ))
    })();
    result.unwrap_or_else(|reason| {
        (
            vec![confirmation_gap_finding(
                review,
                TOTAL_THICKNESS_MATERIAL_FAMILY,
                "thickness-material",
                &reason,
            )],
            not_checked(TOTAL_THICKNESS_MATERIAL_FAMILY, LABEL, reason),
        )
    })
}

fn drill_span_plating(
    review: &FabricationReview,
    declaration_gaps: &BTreeMap<String, String>,
    deadline: ManufacturingDeadline,
) -> (Vec<Finding>, Coverage) {
    const LABEL: &str = "Drill span and plating customer confirmation";
    let result = (|| -> Result<(Vec<Finding>, Coverage), String> {
        dispatch_complete(review, DRILL_SPAN_PLATING_REQUIREMENTS)?;
        if affected_capability_evidence(
            review,
            &[
                CapabilityId::Tools,
                CapabilityId::Drills,
                CapabilityId::Plating,
                CapabilityId::LayerSpans,
                CapabilityId::LayerOrder,
            ],
        ) {
            return Err(
                "affected omission or conflict prevents drill span/plating evidence".into(),
            );
        }
        let acknowledgement = declaration_gaps
            .get("drill_span_plating")
            .cloned()
            .unwrap_or_else(|| {
                format!(
                    "customer drill-span/plating acknowledgement is absent; {}",
                    declaration_link(review)
                )
            });
        let mut hit_counts = BTreeMap::<String, usize>::new();
        for feature in &review.features {
            deadline
                .check("dfm-drill-span-plating")
                .map_err(|error| error.to_string())?;
            let Geometry::Drill(drill) = &feature.geometry else {
                continue;
            };
            if feature.tool_id.as_deref() != Some(drill.tool_id.as_str()) {
                return Err(format!("drill {} has ambiguous tool identity", feature.id));
            }
            let mut tools = review.tools.iter().filter(|tool| tool.id == drill.tool_id);
            let tool = tools
                .next()
                .filter(|_| tools.next().is_none())
                .ok_or_else(|| format!("drill {} tool is missing or duplicated", feature.id))?;
            if tool.kind != ToolKind::Drill
                || tool.document_id != feature.document_id
                || tool.diameter != Some(drill.diameter)
            {
                return Err(format!(
                    "drill {} and tool {} do not retain one exact round-drill identity",
                    feature.id, tool.id
                ));
            }
            source_link(review, &feature.provenance)?;
            source_link(review, &tool.provenance)?;
            *hit_counts.entry(tool.id.clone()).or_default() = hit_counts
                .get(&tool.id)
                .copied()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("drill hit count overflow")?;
        }
        if hit_counts.is_empty() {
            return Err("no represented round drill/tool evidence is available".into());
        }

        let mut findings = Vec::with_capacity(hit_counts.len());
        let mut summaries = Vec::with_capacity(hit_counts.len());
        for (tool_id, hit_count) in hit_counts {
            deadline
                .check("dfm-drill-span-plating")
                .map_err(|error| error.to_string())?;
            let mut tools = review.tools.iter().filter(|tool| tool.id == tool_id);
            let tool = tools
                .next()
                .filter(|_| tools.next().is_none())
                .ok_or_else(|| format!("tool {tool_id} is missing or duplicated"))?;
            let plating = match tool.plating {
                Plating::Plated => "plated",
                Plating::NonPlated => "non_plated",
                Plating::Mixed => "mixed",
                Plating::Unknown => "unknown",
            };
            let layer = |id: Option<&str>| -> Result<String, String> {
                let Some(id) = id.filter(|id| !id.is_empty()) else {
                    return Ok("unrepresented".into());
                };
                let mut layers = review.layers.iter().filter(|layer| layer.id == id);
                let Some(layer) = layers.next().filter(|_| layers.next().is_none()) else {
                    return Ok("unrepresented".into());
                };
                if layer.order.is_none()
                    || matches!(
                        layer.authority,
                        Authority::FilenameInference | Authority::Unknown
                    )
                {
                    return Ok("unrepresented".into());
                }
                source_link(review, &layer.provenance)?;
                Ok(layer.name.clone().unwrap_or_else(|| layer.id.clone()))
            };
            let (from_layer, to_layer) = match tool.span.as_ref() {
                Some(span) => (
                    layer(span.from_layer_id.as_deref())?,
                    layer(span.to_layer_id.as_deref())?,
                ),
                None => ("unrepresented".into(), "unrepresented".into()),
            };
            let design_complete = matches!(tool.plating, Plating::Plated | Plating::NonPlated)
                && from_layer != "unrepresented"
                && to_layer != "unrepresented";
            let evidence = format!(
                "confirmation_gap tool={} code={:?} diameter={}pm holes={hit_count} plating={plating} from_layer={from_layer} to_layer={to_layer} design_complete={design_complete} design_source={} customer_acknowledgement={acknowledgement:?}; customer comparison deferred because acknowledgement has no canonical representation",
                tool.id,
                tool.code,
                tool.diameter
                    .map_or("unrepresented".into(), |value| value.0.to_string()),
                source_link(review, &tool.provenance)?
            );
            summaries.push(evidence.clone());
            findings.push(Finding {
                id: format!("{DRILL_SPAN_PLATING_FAMILY}/gap/tool/{}", tool.id),
                severity: Severity::Low,
                category: "DFM".into(),
                title: "Drill span and plating require customer confirmation".into(),
                evidence,
                recommendation:
                    "Obtain a separately approved canonical customer drill-span/plating acknowledgement; do not assume a span or plating state."
                        .into(),
                location: format!(
                    "tool={};document={};record={}",
                    tool.id, tool.document_id, tool.provenance.location.record
                ),
                source: "fabrication".into(),
                gate_impact: GateImpact::EvidenceOnly,
            });
        }
        findings.sort_by(|left, right| left.id.cmp(&right.id));
        summaries.sort();
        Ok((
            findings,
            Coverage {
                id: DRILL_SPAN_PLATING_FAMILY.into(),
                label: LABEL.into(),
                status: CoverageStatus::NotRun,
                evidence: format!(
                    "not_checked: {} per-tool design record(s) retained; customer comparison is confirmation-gap only. {}",
                    summaries.len(),
                    summaries.join("; ")
                ),
            },
        ))
    })();
    result.unwrap_or_else(|reason| {
        (
            vec![confirmation_gap_finding(
                review,
                DRILL_SPAN_PLATING_FAMILY,
                "drill-span-plating",
                &reason,
            )],
            not_checked(DRILL_SPAN_PLATING_FAMILY, LABEL, reason),
        )
    })
}

struct TextComparison {
    conflict: bool,
    evidence: String,
    location: String,
}

fn represented_text_comparison(
    review: &FabricationReview,
    declaration: &ManufacturingDocument,
    kind: ConstraintKind,
    declaration_id: &str,
    concept: &str,
    construction_value: Option<&str>,
) -> Result<TextComparison, String> {
    let requirement = customer_constraint(review, declaration, kind, declaration_id)?
        .ok_or_else(|| format!("customer {concept} acknowledgement is missing"))?;
    let requirement_value = customer_text(requirement, declaration_id)?;
    let design = design_constraint(review, declaration, kind, concept)?
        .ok_or_else(|| format!("design {concept} authority is missing"))?;
    let design_value = design
        .declared_value
        .as_deref()
        .filter(|value| valid_declaration_text(value))
        .ok_or_else(|| format!("design {concept} string is missing"))?;
    if construction_value
        .is_some_and(|value| !valid_declaration_text(value) || value != design_value)
    {
        return Err(format!(
            "design {concept} construction and semantic constraint disagree"
        ));
    }
    let conflict = design_value != requirement_value;
    Ok(TextComparison {
        conflict,
        evidence: format!(
            "{concept} outcome={} design={design_value:?} requirement={requirement_value:?} design_source={} requirement_source={}",
            if conflict { "conflict" } else { "match" },
            source_link(review, &design.provenance)?,
            source_link(review, &requirement.provenance)?
        ),
        location: format!(
            "concept={concept};design_document={};design_record={};requirement_document={};requirement_record={}",
            design.provenance.document_id,
            design.provenance.location.record,
            requirement.provenance.document_id,
            requirement.provenance.location.record
        ),
    })
}

fn text_conflict_finding(family: &str, concept: &str, comparison: &TextComparison) -> Finding {
    Finding {
        id: format!("{family}/conflict/{concept}"),
        severity: Severity::Medium,
        category: "DFM".into(),
        title: format!(
            "{} conflicts with the customer acknowledgement",
            concept.replace('-', " ")
        ),
        evidence: comparison.evidence.clone(),
        recommendation: format!(
            "Resolve the exact declared {} and retain revised dual-source acknowledgement.",
            concept.replace('-', " ")
        ),
        location: comparison.location.clone(),
        source: "fabrication".into(),
        gate_impact: family_gate_impact(family),
    }
}

fn deferred_acknowledgement_finding(
    review: &FabricationReview,
    declaration_gaps: &BTreeMap<String, String>,
    declaration_id: &str,
    concept: &str,
) -> Finding {
    let represented = declaration_gaps.get(declaration_id);
    let acknowledgement = represented.cloned().unwrap_or_else(|| {
        format!(
            "customer {concept} acknowledgement is absent; {}",
            declaration_link(review)
        )
    });
    let mut finding = confirmation_gap_finding(
        review,
        FINISH_PROFILE_FAMILY,
        concept,
        format!(
            "customer_acknowledgement={acknowledgement:?}; canonical customer representation is unavailable, so customer comparison is deferred"
        ),
    );
    finding.recommendation = if represented.is_some() {
        format!(
            "Retain the source-linked {concept} acknowledgement as a confirmation gap until a separately approved canonical representation exists."
        )
    } else {
        format!(
            "Obtain a source-linked {concept} acknowledgement and keep it as a confirmation gap until a separately approved canonical representation exists."
        )
    };
    finding
}

fn finish_profile(
    review: &FabricationReview,
    declaration_gaps: &BTreeMap<String, String>,
    deadline: ManufacturingDeadline,
) -> (Vec<Finding>, Coverage) {
    const LABEL: &str = "Finish and profile customer confirmation";
    let deferred = || {
        let mut findings = [
            ("castellation", "castellation"),
            ("edge_plating", "edge-plating"),
            ("profile", "profile"),
        ]
        .into_iter()
        .map(|(id, concept)| {
            deferred_acknowledgement_finding(review, declaration_gaps, id, concept)
        })
        .collect::<Vec<_>>();
        findings.sort_by(|left, right| left.id.cmp(&right.id));
        findings
    };
    let result = (|| -> Result<TextComparison, String> {
        deadline
            .check("dfm-finish-profile")
            .map_err(|error| error.to_string())?;
        dispatch_complete(review, FINISH_PROFILE_REQUIREMENTS)?;
        if affected_capability_evidence(
            review,
            &[
                CapabilityId::Construction,
                CapabilityId::Profile,
                CapabilityId::Constraints,
            ],
        ) {
            return Err(
                "affected omission or conflict prevents finish/profile confirmation".into(),
            );
        }
        let declaration = declaration_document(review)
            .map_err(|error| error.to_string())?
            .ok_or("source/version-bound finish authority is missing")?;
        represented_text_comparison(
            review,
            declaration,
            ConstraintKind::Finish,
            "finish",
            "finish",
            review.construction.finish.as_deref(),
        )
    })();

    match result {
        Ok(comparison) => {
            let mut findings = deferred();
            if comparison.conflict {
                findings.push(text_conflict_finding(
                    FINISH_PROFILE_FAMILY,
                    "finish",
                    &comparison,
                ));
            }
            findings.sort_by(|left, right| left.id.cmp(&right.id));
            (
                findings,
                Coverage {
                    id: FINISH_PROFILE_FAMILY.into(),
                    label: LABEL.into(),
                    status: if comparison.conflict {
                        CoverageStatus::Attention
                    } else {
                        CoverageStatus::NotRun
                    },
                    evidence: format!(
                        "not_checked: profile, castellation, and edge-plating acknowledgements have no canonical comparison representation; {}",
                        comparison.evidence
                    ),
                },
            )
        }
        Err(reason) => {
            let mut findings = deferred();
            findings.push(confirmation_gap_finding(
                review,
                FINISH_PROFILE_FAMILY,
                "finish",
                &reason,
            ));
            findings.sort_by(|left, right| left.id.cmp(&right.id));
            (findings, not_checked(FINISH_PROFILE_FAMILY, LABEL, reason))
        }
    }
}

fn impedance_special_process(
    review: &FabricationReview,
    deadline: ManufacturingDeadline,
) -> (Vec<Finding>, Coverage) {
    const LABEL: &str = "Impedance and special-process customer confirmation";
    let prerequisite = (|| -> Result<&ManufacturingDocument, String> {
        deadline
            .check("dfm-impedance-special-process")
            .map_err(|error| error.to_string())?;
        dispatch_complete(review, IMPEDANCE_SPECIAL_PROCESS_REQUIREMENTS)?;
        if affected_capability_evidence(
            review,
            &[CapabilityId::Constraints, CapabilityId::Construction],
        ) {
            return Err(
                "affected omission or conflict prevents impedance/special-process confirmation"
                    .into(),
            );
        }
        declaration_document(review)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                "source/version-bound impedance/special-process authority is missing".into()
            })
    })();
    let declaration = match prerequisite {
        Ok(declaration) => declaration,
        Err(reason) => {
            let mut findings = ["impedance", "special-process"]
                .into_iter()
                .map(|concept| {
                    confirmation_gap_finding(
                        review,
                        IMPEDANCE_SPECIAL_PROCESS_FAMILY,
                        concept,
                        &reason,
                    )
                })
                .collect::<Vec<_>>();
            findings.sort_by(|left, right| left.id.cmp(&right.id));
            return (
                findings,
                not_checked(IMPEDANCE_SPECIAL_PROCESS_FAMILY, LABEL, reason),
            );
        }
    };

    let mut findings = Vec::new();
    let mut details = Vec::new();
    let mut gap_count = 0;
    for (kind, declaration_id, concept) in [
        (ConstraintKind::Impedance, "impedance", "impedance"),
        (
            ConstraintKind::SpecialProcess,
            "special_process",
            "special-process",
        ),
    ] {
        if let Err(error) = deadline.check("dfm-impedance-special-process") {
            let reason = error.to_string();
            findings.push(confirmation_gap_finding(
                review,
                IMPEDANCE_SPECIAL_PROCESS_FAMILY,
                concept,
                &reason,
            ));
            details.push(format!(
                "{concept} outcome=confirmation_gap reason={reason:?}"
            ));
            gap_count += 1;
            continue;
        }
        match represented_text_comparison(review, declaration, kind, declaration_id, concept, None)
        {
            Ok(comparison) => {
                details.push(comparison.evidence.clone());
                if comparison.conflict {
                    findings.push(text_conflict_finding(
                        IMPEDANCE_SPECIAL_PROCESS_FAMILY,
                        concept,
                        &comparison,
                    ));
                }
            }
            Err(reason) => {
                details.push(format!(
                    "{concept} outcome=confirmation_gap reason={reason:?}"
                ));
                findings.push(confirmation_gap_finding(
                    review,
                    IMPEDANCE_SPECIAL_PROCESS_FAMILY,
                    concept,
                    &reason,
                ));
                gap_count += 1;
            }
        }
    }
    findings.sort_by(|left, right| left.id.cmp(&right.id));
    details.sort();
    let conflict = findings
        .iter()
        .any(|finding| finding.id.contains("/conflict/"));
    (
        findings,
        Coverage {
            id: IMPEDANCE_SPECIAL_PROCESS_FAMILY.into(),
            label: LABEL.into(),
            status: if conflict {
                CoverageStatus::Attention
            } else if gap_count > 0 {
                CoverageStatus::NotRun
            } else {
                CoverageStatus::Passed
            },
            evidence: if gap_count > 0 {
                format!(
                    "not_checked: {gap_count} represented acknowledgement gap(s); {}",
                    details.join("; ")
                )
            } else {
                details.join("; ")
            },
        },
    )
}

fn validated_geometry_tool<'a>(
    review: &'a FabricationReview,
    feature: &ManufacturingFeature,
    geometry_tool_id: &str,
) -> Result<(&'a ManufacturingTool, Picometres), String> {
    if feature.tool_id.as_deref() != Some(geometry_tool_id) {
        return Err(format!(
            "feature {} has an ambiguous outer/geometry tool reference",
            feature.id
        ));
    }
    let mut tools = review
        .tools
        .iter()
        .filter(|tool| tool.id == geometry_tool_id);
    let tool = tools
        .next()
        .filter(|_| tools.next().is_none())
        .ok_or_else(|| format!("feature {} has a missing or duplicate tool", feature.id))?;
    if tool.document_id != feature.document_id
        || !matches!(tool.kind, ToolKind::Drill | ToolKind::Route)
        || tool.diameter.is_none()
        || !matches!(tool.plating, Plating::Plated | Plating::NonPlated)
        || tool.span.as_ref().is_none_or(|span| {
            span.from_layer_id.as_deref().is_none_or(str::is_empty)
                || span.to_layer_id.as_deref().is_none_or(str::is_empty)
        })
    {
        return Err(format!(
            "tool {} has unsupported kind, diameter, plating, or layer span",
            tool.id
        ));
    }
    let mut documents = review
        .documents
        .iter()
        .filter(|document| document.id == tool.document_id);
    let document = documents
        .next()
        .filter(|_| documents.next().is_none())
        .ok_or_else(|| format!("tool {} document is missing or ambiguous", tool.id))?;
    let resolution = document
        .numeric_format
        .as_ref()
        .filter(|_| {
            document.parse_status == ParseStatus::Complete
                && matches!(
                    document.format,
                    DocumentFormat::Excellon | DocumentFormat::KicadPcb
                )
        })
        .map(|format| format.resolution)
        .filter(|resolution| resolution.0 > 0)
        .ok_or_else(|| format!("tool {} source resolution is unavailable", tool.id))?;
    Ok((tool, resolution))
}

fn source_bound_minimum_drill(
    review: &FabricationReview,
) -> Result<(&ManufacturingConstraint, &ManufacturingDocument), String> {
    let document = declaration_document(review)
        .map_err(|error| error.to_string())?
        .ok_or("source/version-bound minimum drill authority is missing")?;
    let mut constraints = review
        .constraints
        .iter()
        .filter(|constraint| constraint.kind == ConstraintKind::MinimumDrill);
    let constraint = constraints
        .next()
        .filter(|_| constraints.next().is_none())
        .ok_or("minimum drill authority is missing or duplicated")?;
    let declared = constraint
        .declared_value
        .as_deref()
        .ok_or("minimum drill declaration text is missing")?;
    if constraint.authority != Authority::Explicit
        || constraint.provenance.document_id != document.id
        || constraint.provenance.artifact_digest != document.artifact_digest
        || constraint.value.is_none_or(|value| value.0 <= 0)
        || !declared.starts_with("minimum_drill=")
        || !declared.ends_with(";applies=board")
        || constraint.provenance.source_lexeme.as_deref() != Some("minimum_drill@board")
        || constraint.provenance.producer.trim().is_empty()
        || constraint.provenance.producer_version.trim().is_empty()
    {
        return Err("minimum drill authority is not the complete declaration record".into());
    }
    Ok((constraint, document))
}

fn minimum_finished_drill(
    review: &FabricationReview,
    deadline: ManufacturingDeadline,
) -> (Vec<Finding>, Coverage) {
    const LABEL: &str = "Source-authoritative minimum finished drill";
    let result = (|| -> Result<(Vec<Finding>, Coverage), String> {
        dispatch_complete(review, MINIMUM_DRILL_REQUIREMENTS)?;
        if affected_capability_evidence(
            review,
            &[
                CapabilityId::UnitsAndFormat,
                CapabilityId::Tools,
                CapabilityId::Drills,
                CapabilityId::Plating,
                CapabilityId::LayerSpans,
                CapabilityId::Constraints,
            ],
        ) {
            return Err("affected omission or conflict prevents a complete measurement".into());
        }
        dispatch_complete(review, PLATING_SPAN_REQUIREMENTS)?;
        let (threshold, authority_document) = source_bound_minimum_drill(review)?;
        let threshold_value = threshold
            .value
            .ok_or("validated minimum drill threshold lost its value")?;
        let mut best = None;
        let mut hits = 0_usize;
        for feature in &review.features {
            deadline
                .check("dfm-minimum-finished-drill")
                .map_err(|error| error.to_string())?;
            let Geometry::Drill(drill) = &feature.geometry else {
                continue;
            };
            hits = hits
                .checked_add(1)
                .ok_or("minimum drill hit count overflow")?;
            let (tool, resolution) = validated_geometry_tool(review, feature, &drill.tool_id)?;
            if tool.kind != ToolKind::Drill
                || tool.diameter != Some(drill.diameter)
                || drill.diameter.0 % resolution.0 != 0
                || threshold_value.0 % resolution.0 != 0
            {
                return Err(format!(
                    "drill {} diameter or threshold is inconsistent with its tool/source resolution",
                    feature.id
                ));
            }
            let candidate = (
                drill.diameter.0,
                feature.id.as_str(),
                feature,
                tool,
                resolution,
            );
            if best.as_ref().is_none_or(
                |current: &(
                    i64,
                    &str,
                    &ManufacturingFeature,
                    &ManufacturingTool,
                    Picometres,
                )| { (candidate.0, candidate.1) < (current.0, current.1) },
            ) {
                best = Some(candidate);
            }
        }
        let (observed, _, feature, tool, resolution) =
            best.ok_or("no represented round drill hit is available")?;
        let delta = threshold_value
            .0
            .checked_sub(observed)
            .ok_or("minimum drill delta overflow")?;
        let measurement = format!(
            "observed={observed}pm threshold={}pm delta={delta}pm resolution={}pm tool={} hit={} hits={hits} authority={} {} source={} threshold_record={} tool_record={} hit_record={}",
            threshold_value.0,
            resolution.0,
            tool.id,
            feature.id,
            threshold.provenance.producer,
            threshold.provenance.producer_version,
            authority_document.virtual_path,
            threshold.provenance.location.record,
            tool.provenance.location.record,
            feature.provenance.location.record,
        );
        let findings = if delta > 0 {
            vec![Finding {
                id: format!("{MINIMUM_FINISHED_DRILL_FAMILY}/{}", feature.id),
                severity: Severity::Medium,
                category: "DFM".into(),
                title: "Finished drill is below the declared minimum".into(),
                evidence: measurement.clone(),
                recommendation:
                    "Increase the finished round-hole diameter or obtain revised source-bound fabricator authority."
                        .into(),
                location: format!(
                    "document={};tool={};hit={};threshold={}",
                    feature.document_id, tool.id, feature.id, threshold.id
                ),
                source: "fabrication".into(),
                gate_impact: family_gate_impact(MINIMUM_FINISHED_DRILL_FAMILY),
            }]
        } else {
            vec![]
        };
        Ok((
            findings,
            Coverage {
                id: MINIMUM_FINISHED_DRILL_FAMILY.into(),
                label: LABEL.into(),
                status: if delta > 0 {
                    CoverageStatus::Attention
                } else {
                    CoverageStatus::Passed
                },
                evidence: measurement,
            },
        ))
    })();
    result.unwrap_or_else(|reason| {
        (
            vec![],
            not_checked(MINIMUM_FINISHED_DRILL_FAMILY, LABEL, reason),
        )
    })
}

fn object_tool_measurement(
    review: &FabricationReview,
    feature: &ManufacturingFeature,
    kind: &str,
    tool_id: &str,
    observed: Picometres,
) -> Result<Option<Finding>, String> {
    let (tool, resolution) = validated_geometry_tool(review, feature, tool_id)?;
    let compatible = match kind {
        "drill" => tool.kind == ToolKind::Drill,
        "route" | "slot" => matches!(tool.kind, ToolKind::Drill | ToolKind::Route),
        _ => false,
    };
    if !compatible {
        return Err(format!(
            "{kind} {} references incompatible {:?} tool {}",
            feature.id, tool.kind, tool.id
        ));
    }
    let expected = tool
        .diameter
        .ok_or_else(|| format!("{kind} {} tool diameter is missing", feature.id))?;
    if observed.0 % resolution.0 != 0 || expected.0 % resolution.0 != 0 {
        return Err(format!(
            "{kind} {} diameter is inconsistent with source resolution",
            feature.id
        ));
    }
    if observed == expected {
        return Ok(None);
    }
    Ok(Some(Finding {
        id: format!("{DRILL_TOOL_INTEGRITY_FAMILY}/{kind}/{}", feature.id),
        severity: Severity::Medium,
        category: "DFM".into(),
        title: format!("{kind} geometry disagrees with its declared tool"),
        evidence: format!(
            "kind={kind} observed={}pm tool_diameter={}pm resolution={}pm tool={} feature={} plating={:?} span={:?}",
            observed.0, expected.0, resolution.0, tool.id, feature.id, tool.plating, tool.span
        ),
        recommendation:
            "Regenerate drill/route output so every object retains one exact declared tool.".into(),
        location: format!(
            "document={};kind={kind};tool={};feature={}",
            feature.document_id, tool.id, feature.id
        ),
        source: "fabrication".into(),
        gate_impact: family_gate_impact(DRILL_TOOL_INTEGRITY_FAMILY),
    }))
}

fn drill_tool_integrity(
    review: &FabricationReview,
    deadline: ManufacturingDeadline,
) -> (Vec<Finding>, Coverage) {
    const LABEL: &str = "Drill, route, slot, and tool integrity";
    let result = (|| -> Result<(Vec<Finding>, Coverage), String> {
        dispatch_complete(review, DRILL_TOOL_REQUIREMENTS)?;
        dispatch_complete(review, PLATING_SPAN_REQUIREMENTS)?;
        if affected_capability_evidence(
            review,
            &[
                CapabilityId::UnitsAndFormat,
                CapabilityId::Tools,
                CapabilityId::Drills,
                CapabilityId::Routes,
                CapabilityId::Slots,
                CapabilityId::Plating,
                CapabilityId::LayerSpans,
            ],
        ) {
            return Err("affected omission or conflict prevents complete tool integrity".into());
        }
        let mut features = review
            .features
            .iter()
            .filter(|feature| {
                matches!(
                    feature.geometry,
                    Geometry::Drill(_) | Geometry::Route(_) | Geometry::Slot(_)
                )
            })
            .collect::<Vec<_>>();
        features.sort_by_key(|feature| feature.id.as_str());
        if features.is_empty() {
            return Err("no represented drill, route, or slot object is available".into());
        }
        let has_drills = features
            .iter()
            .any(|feature| matches!(feature.geometry, Geometry::Drill(_)));
        let has_routes = features
            .iter()
            .any(|feature| matches!(feature.geometry, Geometry::Route(_)));
        let has_slots = features
            .iter()
            .any(|feature| matches!(feature.geometry, Geometry::Slot(_)));
        for (present, requirements) in [
            (has_drills, DRILL_REQUIREMENT),
            (has_routes, ROUTE_REQUIREMENT),
            (has_slots, SLOT_REQUIREMENT),
        ] {
            if present {
                dispatch_complete(review, requirements)?;
            }
        }

        let (mut drills, mut routes, mut slots) = (0_usize, 0_usize, 0_usize);
        let mut findings = Vec::new();
        for feature in features {
            deadline
                .check("dfm-drill-tool-integrity")
                .map_err(|error| error.to_string())?;
            match &feature.geometry {
                Geometry::Drill(drill) => {
                    drills = drills.checked_add(1).ok_or("drill count overflow")?;
                    if let Some(finding) = object_tool_measurement(
                        review,
                        feature,
                        "drill",
                        &drill.tool_id,
                        drill.diameter,
                    )? {
                        findings.push(finding);
                    }
                }
                Geometry::Slot(slot) => {
                    slots = slots.checked_add(1).ok_or("slot count overflow")?;
                    if let Some(finding) =
                        object_tool_measurement(review, feature, "slot", &slot.tool_id, slot.width)?
                    {
                        findings.push(finding);
                    }
                }
                Geometry::Route(route) => {
                    routes = routes.checked_add(1).ok_or("route count overflow")?;
                    if route.segments.is_empty() {
                        return Err(format!("route {} has no represented segments", feature.id));
                    }
                    for (index, segment) in route.segments.iter().enumerate() {
                        deadline
                            .check("dfm-drill-tool-integrity")
                            .map_err(|error| error.to_string())?;
                        let width = match segment {
                            ContourSegment::Line(line) => line.width,
                            ContourSegment::Arc(arc) => arc.width,
                        }
                        .ok_or_else(|| {
                            format!("route {} segment {index} has no exact width", feature.id)
                        })?;
                        if let Some(mut finding) = object_tool_measurement(
                            review,
                            feature,
                            "route",
                            &route.tool_id,
                            width,
                        )? {
                            finding.id.push_str(&format!("/{index}"));
                            finding.location.push_str(&format!(";segment={index}"));
                            findings.push(finding);
                        }
                    }
                }
                _ => unreachable!("filtered geometry kind"),
            }
        }
        findings.sort_by(|left, right| left.id.cmp(&right.id));
        let has_findings = !findings.is_empty();
        let evidence = format!(
            "drills={drills} routes={routes} slots={slots} round_hits={drills} mismatches={} exact tool diameter, plating, span, object kind, and source resolution retained; routes/slots excluded from round hits",
            findings.len()
        );
        Ok((
            findings,
            Coverage {
                id: DRILL_TOOL_INTEGRITY_FAMILY.into(),
                label: LABEL.into(),
                status: if has_findings {
                    CoverageStatus::Attention
                } else {
                    CoverageStatus::Passed
                },
                evidence,
            },
        ))
    })();
    result.unwrap_or_else(|reason| {
        (
            vec![],
            not_checked(DRILL_TOOL_INTEGRITY_FAMILY, LABEL, reason),
        )
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum BoundaryKind {
    Exterior,
    Cutout,
    Routed,
}

impl BoundaryKind {
    fn name(self) -> &'static str {
        match self {
            Self::Exterior => "exterior",
            Self::Cutout => "cutout",
            Self::Routed => "routed",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct AxisPrimitive {
    start: CanonicalPoint,
    end: CanonicalPoint,
    radius: Picometres,
}

#[derive(Clone, Copy)]
struct LocatedPrimitive<'a> {
    owner_id: &'a str,
    segment: usize,
    layer_id: &'a str,
    primitive: AxisPrimitive,
    resolution: Picometres,
    provenance: &'a ManufacturingProvenance,
    boundary_kind: Option<BoundaryKind>,
}

#[derive(Clone, Copy)]
struct NearestPair<'a> {
    observed: Picometres,
    left: LocatedPrimitive<'a>,
    right: LocatedPrimitive<'a>,
    candidate_checks: usize,
}

fn exact_half(value: Picometres, label: &str) -> Result<Picometres, String> {
    if value.0 <= 0 || value.0 % 2 != 0 {
        return Err(format!(
            "{label} is not a positive exact even-picometre width"
        ));
    }
    Ok(Picometres(value.0 / 2))
}

fn axis_primitive(
    line: &CanonicalLine,
    radius: Picometres,
    label: &str,
) -> Result<AxisPrimitive, String> {
    if radius.0 < 0
        || line.start == line.end
        || (line.start.x != line.end.x && line.start.y != line.end.y)
    {
        return Err(format!(
            "{label} is not a nonzero exact axis-aligned represented segment"
        ));
    }
    Ok(AxisPrimitive {
        start: line.start,
        end: line.end,
        radius,
    })
}

fn primitive_bounds(primitive: AxisPrimitive) -> Result<(i128, i128, i128, i128), String> {
    let radius = i128::from(primitive.radius.0);
    let min_x = i128::from(primitive.start.x.0.min(primitive.end.x.0));
    let max_x = i128::from(primitive.start.x.0.max(primitive.end.x.0));
    let min_y = i128::from(primitive.start.y.0.min(primitive.end.y.0));
    let max_y = i128::from(primitive.start.y.0.max(primitive.end.y.0));
    Ok((
        min_x
            .checked_sub(radius)
            .ok_or("primitive x bound overflow")?,
        min_y
            .checked_sub(radius)
            .ok_or("primitive y bound overflow")?,
        max_x
            .checked_add(radius)
            .ok_or("primitive x bound overflow")?,
        max_y
            .checked_add(radius)
            .ok_or("primitive y bound overflow")?,
    ))
}

fn interval_gap(left_min: i64, left_max: i64, right_min: i64, right_max: i64) -> u128 {
    let left_min = i128::from(left_min);
    let left_max = i128::from(left_max);
    let right_min = i128::from(right_min);
    let right_max = i128::from(right_max);
    if left_max < right_min {
        (right_min - left_max) as u128
    } else if right_max < left_min {
        (left_min - right_max) as u128
    } else {
        0
    }
}

#[derive(Clone, Copy)]
struct AxisDistance {
    center_squared: u128,
    radii: i64,
}

fn axis_distance(left: AxisPrimitive, right: AxisPrimitive) -> Result<AxisDistance, String> {
    let dx = interval_gap(
        left.start.x.0.min(left.end.x.0),
        left.start.x.0.max(left.end.x.0),
        right.start.x.0.min(right.end.x.0),
        right.start.x.0.max(right.end.x.0),
    );
    let dy = interval_gap(
        left.start.y.0.min(left.end.y.0),
        left.start.y.0.max(left.end.y.0),
        right.start.y.0.min(right.end.y.0),
        right.start.y.0.max(right.end.y.0),
    );
    Ok(AxisDistance {
        center_squared: dx
            .checked_mul(dx)
            .and_then(|value| {
                dy.checked_mul(dy)
                    .and_then(|other| value.checked_add(other))
            })
            .ok_or("distance square overflow")?,
        radii: left
            .radius
            .0
            .checked_add(right.radius.0)
            .ok_or("distance radius overflow")?,
    })
}

fn exact_axis_measurement(distance: AxisDistance) -> Result<Option<Picometres>, String> {
    let radii = u128::try_from(distance.radii).map_err(|_| "negative distance radius")?;
    let radii_squared = radii
        .checked_mul(radii)
        .ok_or("distance radius square overflow")?;
    if distance.center_squared <= radii_squared {
        return Ok(Some(Picometres(0)));
    }
    Ok(exact_integer_sqrt(distance.center_squared)
        .map(|center| Picometres(center.saturating_sub(distance.radii))))
}

fn inexact_distance_can_beat(distance: AxisDistance, exact: Picometres) -> Result<bool, String> {
    let limit = i128::from(exact.0)
        .checked_add(i128::from(distance.radii))
        .ok_or("distance comparison overflow")?;
    let limit = u128::try_from(limit).map_err(|_| "negative distance comparison")?;
    let limit_squared = limit
        .checked_mul(limit)
        .ok_or("distance comparison square overflow")?;
    Ok(distance.center_squared < limit_squared)
}

fn exact_geometry_resolution(
    review: &FabricationReview,
    provenance: &ManufacturingProvenance,
) -> Result<Picometres, String> {
    let mut documents = review
        .documents
        .iter()
        .filter(|document| document.id == provenance.document_id);
    let document = documents
        .next()
        .filter(|_| documents.next().is_none())
        .ok_or("geometry source document is missing or ambiguous")?;
    document
        .numeric_format
        .as_ref()
        .filter(|format| document.parse_status == ParseStatus::Complete && format.resolution.0 > 0)
        .map(|format| format.resolution)
        .ok_or_else(|| "geometry source resolution is unavailable".into())
}

fn comparison_resolution(left: Picometres, right: Picometres) -> Result<Picometres, String> {
    let (fine, coarse) = if left <= right {
        (left, right)
    } else {
        (right, left)
    };
    if fine.0 <= 0 || coarse.0 % fine.0 != 0 {
        return Err("compared source resolutions do not share an exact fixed-point grid".into());
    }
    Ok(coarse)
}

fn primitive_on_resolution(primitive: AxisPrimitive, resolution: Picometres) -> bool {
    [
        primitive.start.x.0,
        primitive.start.y.0,
        primitive.end.x.0,
        primitive.end.y.0,
        primitive.radius.0,
    ]
    .into_iter()
    .all(|value| value % resolution.0 == 0)
}

fn nearest_axis_pair<'a>(
    mut left: Vec<LocatedPrimitive<'a>>,
    mut right: Vec<LocatedPrimitive<'a>>,
    deadline: ManufacturingDeadline,
    resource: &'static str,
) -> Result<NearestPair<'a>, String> {
    if left.is_empty() || right.is_empty() {
        return Err("no represented geometry pair is available".into());
    }
    if left.len() > MAX_DISTANCE_PRIMITIVES || right.len() > MAX_DISTANCE_PRIMITIVES {
        return Err(format!(
            "distance primitive limit {MAX_DISTANCE_PRIMITIVES} exceeded"
        ));
    }
    let compare = |left: &LocatedPrimitive<'_>, right: &LocatedPrimitive<'_>| {
        let left_bounds = primitive_bounds(left.primitive).unwrap_or((i128::MIN, 0, i128::MAX, 0));
        let right_bounds =
            primitive_bounds(right.primitive).unwrap_or((i128::MIN, 0, i128::MAX, 0));
        (
            left_bounds,
            left.layer_id,
            left.owner_id,
            left.segment,
            left.boundary_kind.map(BoundaryKind::name).unwrap_or(""),
        )
            .cmp(&(
                right_bounds,
                right.layer_id,
                right.owner_id,
                right.segment,
                right.boundary_kind.map(BoundaryKind::name).unwrap_or(""),
            ))
    };
    left.sort_by(compare);
    right.sort_by(compare);
    let mut best: Option<NearestPair<'a>> = None;
    let mut inexact = Vec::new();
    let mut candidate_checks = 0_usize;
    for left_item in left {
        deadline
            .check(resource)
            .map_err(|error| error.to_string())?;
        let left_bounds = primitive_bounds(left_item.primitive)?;
        for right_item in &right {
            deadline
                .check(resource)
                .map_err(|error| error.to_string())?;
            let right_bounds = primitive_bounds(right_item.primitive)?;
            if let Some(current) = best {
                let limit = i128::from(current.observed.0);
                if right_bounds.0 > left_bounds.2.saturating_add(limit) {
                    break;
                }
                if right_bounds.2 < left_bounds.0.saturating_sub(limit)
                    || right_bounds.1 > left_bounds.3.saturating_add(limit)
                    || right_bounds.3 < left_bounds.1.saturating_sub(limit)
                {
                    continue;
                }
            }
            candidate_checks = candidate_checks
                .checked_add(1)
                .ok_or("distance candidate count overflow")?;
            if candidate_checks > MAX_DISTANCE_CANDIDATES {
                return Err(format!(
                    "distance candidate limit {MAX_DISTANCE_CANDIDATES} exceeded"
                ));
            }
            let distance = axis_distance(left_item.primitive, right_item.primitive)?;
            let Some(observed) = exact_axis_measurement(distance)? else {
                if let Some(current) = best
                    && !inexact_distance_can_beat(distance, current.observed)?
                {
                    continue;
                }
                if inexact.len() >= MAX_INEXACT_DISTANCE_CANDIDATES {
                    return Err(format!(
                        "inexact distance candidate limit {MAX_INEXACT_DISTANCE_CANDIDATES} exceeded"
                    ));
                }
                inexact.push(distance);
                continue;
            };
            let candidate = NearestPair {
                observed,
                left: left_item,
                right: *right_item,
                candidate_checks,
            };
            let candidate_key = (
                candidate.observed,
                candidate.left.layer_id,
                candidate.left.owner_id,
                candidate.left.segment,
                candidate
                    .right
                    .boundary_kind
                    .map(BoundaryKind::name)
                    .unwrap_or(""),
                candidate.right.owner_id,
                candidate.right.segment,
            );
            if best.is_none_or(|current| {
                candidate_key
                    < (
                        current.observed,
                        current.left.layer_id,
                        current.left.owner_id,
                        current.left.segment,
                        current
                            .right
                            .boundary_kind
                            .map(BoundaryKind::name)
                            .unwrap_or(""),
                        current.right.owner_id,
                        current.right.segment,
                    )
            }) {
                best = Some(candidate);
            }
        }
    }
    let mut best = best.ok_or("bounded pruning found no exact represented geometry pair")?;
    if inexact.into_iter().try_fold(false, |closer, distance| {
        inexact_distance_can_beat(distance, best.observed).map(|value| closer || value)
    })? {
        return Err("an inexact represented distance may be nearer than the exact result".into());
    }
    best.candidate_checks = candidate_checks;
    Ok(best)
}

fn source_bound_threshold<'a>(
    review: &'a FabricationReview,
    kind: ConstraintKind,
    rule_id: &str,
) -> Result<(&'a ManufacturingConstraint, &'a ManufacturingDocument), String> {
    let document = declaration_document(review)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("source/version-bound {rule_id} authority is missing"))?;
    let prefix = format!("{rule_id}=");
    let mut constraints = review.constraints.iter().filter(|constraint| {
        constraint.kind == kind
            && (kind != ConstraintKind::Other
                || constraint
                    .declared_value
                    .as_deref()
                    .is_some_and(|value| value.starts_with(&prefix)))
    });
    let constraint = constraints
        .next()
        .filter(|_| constraints.next().is_none())
        .ok_or_else(|| format!("{rule_id} authority is missing or duplicated"))?;
    let declared = constraint
        .declared_value
        .as_deref()
        .ok_or_else(|| format!("{rule_id} declaration text is missing"))?;
    let expected_lexeme = format!("{rule_id}@board");
    if constraint.authority != Authority::Explicit
        || constraint.provenance.document_id != document.id
        || constraint.provenance.artifact_digest != document.artifact_digest
        || constraint.value.is_none_or(|value| value.0 <= 0)
        || !declared.starts_with(&prefix)
        || !declared.ends_with(";applies=board")
        || constraint.provenance.source_lexeme.as_deref() != Some(expected_lexeme.as_str())
        || constraint.provenance.producer.trim().is_empty()
        || constraint.provenance.producer_version.trim().is_empty()
    {
        return Err(format!(
            "{rule_id} authority is not one complete declaration record"
        ));
    }
    Ok((constraint, document))
}

struct InferenceAuthority<'a> {
    record: InferenceDeclarationRecord,
    constraint: &'a ManufacturingConstraint,
    document: &'a ManufacturingDocument,
}

impl InferenceAuthority<'_> {
    fn distance(&self, id: &str) -> Result<Picometres, String> {
        let mut limits = self.record.limits.iter().filter(|limit| limit.id == id);
        let limit = limits
            .next()
            .filter(|_| limits.next().is_none())
            .ok_or_else(|| format!("inference limit {id} is missing or duplicated"))?;
        let value = inference_limit_value(limit).map_err(|error| error.to_string())?;
        i64::try_from(value)
            .map(Picometres)
            .map_err(|_| format!("inference limit {id} exceeds exact distance range"))
    }

    fn parameter(&self, id: &str) -> Result<&str, String> {
        let mut parameters = self
            .record
            .parameters
            .iter()
            .filter(|parameter| parameter.id == id);
        parameters
            .next()
            .filter(|_| parameters.next().is_none())
            .map(|parameter| parameter.value.as_str())
            .ok_or_else(|| format!("inference parameter {id} is missing or duplicated"))
    }
}

fn source_bound_inference_record<'a>(
    review: &'a FabricationReview,
    id: &str,
) -> Result<InferenceAuthority<'a>, String> {
    let document = declaration_document(review)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("source/version-bound inference record {id} is missing"))?;
    let prefix = format!("inference:{id}=");
    let mut constraints = review.constraints.iter().filter(|constraint| {
        constraint.kind == ConstraintKind::Other
            && constraint
                .declared_value
                .as_deref()
                .is_some_and(|value| value.starts_with(&prefix))
    });
    let constraint = constraints
        .next()
        .filter(|_| constraints.next().is_none())
        .ok_or_else(|| format!("inference record {id} is missing or duplicated"))?;
    let encoded = constraint
        .declared_value
        .as_deref()
        .and_then(|value| value.strip_prefix(&prefix))
        .ok_or_else(|| format!("inference record {id} encoding is malformed"))?;
    let record: InferenceDeclarationRecord = serde_json::from_str(encoded)
        .map_err(|error| format!("inference record {id} encoding is malformed: {error}"))?;
    validate_inference_record(&record).map_err(|error| error.to_string())?;
    let canonical = inference_declared_value(&record).map_err(|error| error.to_string())?;
    let source_lexeme = format!("inference:{id}@board");
    if record.id != id
        || record.record != constraint.provenance.location.record
        || constraint.value.is_some()
        || constraint.authority != Authority::Explicit
        || constraint.provenance.document_id != document.id
        || constraint.provenance.artifact_digest != document.artifact_digest
        || constraint.provenance.source_lexeme.as_deref() != Some(source_lexeme.as_str())
        || constraint.declared_value.as_deref() != Some(canonical.as_str())
        || !capability_retains_provenance(review, CapabilityId::Constraints, &constraint.provenance)
    {
        return Err(format!(
            "inference record {id} is not one exact production-normalized authority"
        ));
    }
    validate_declaration_source_identity(review, document)?;
    source_link(review, &constraint.provenance)?;
    Ok(InferenceAuthority {
        record,
        constraint,
        document,
    })
}

fn copper_primitives<'a>(
    review: &'a FabricationReview,
    deadline: ManufacturingDeadline,
) -> Result<Vec<LocatedPrimitive<'a>>, String> {
    let layers = review
        .layers
        .iter()
        .map(|layer| (layer.id.as_str(), layer))
        .collect::<BTreeMap<_, _>>();
    let mut apertures = BTreeMap::new();
    for aperture in &review.apertures {
        if apertures.insert(aperture.id.as_str(), aperture).is_some() {
            return Err(format!("duplicate aperture identity {}", aperture.id));
        }
    }
    let mut output = Vec::new();
    for feature in &review.features {
        deadline
            .check("dfm-copper-geometry")
            .map_err(|error| error.to_string())?;
        let Some(layer) = layers.get(feature.layer_id.as_str()).copied() else {
            return Err(format!("feature {} has no physical layer", feature.id));
        };
        if layer.role != LayerRole::Copper
            || matches!(
                feature.geometry,
                Geometry::Drill(_) | Geometry::Route(_) | Geometry::Slot(_)
            )
        {
            continue;
        }
        if !matches!(
            feature.polarity,
            LayerPolarity::Dark | LayerPolarity::Positive
        ) || !feature.transforms.operations.is_empty()
            || feature.membership != FeatureMembership::TopLevel
            || review
                .repetitions
                .iter()
                .any(|repeat| repeat.feature_ids.contains(&feature.id))
        {
            return Err(format!(
                "copper feature {} has unresolved polarity, transform, or expansion",
                feature.id
            ));
        }
        let primitive = match &feature.geometry {
            Geometry::Line(line) => {
                let width = line
                    .width
                    .ok_or_else(|| format!("copper feature {} has no exact width", feature.id))?;
                axis_primitive(line, exact_half(width, "copper width")?, "copper feature")?
            }
            Geometry::Flash(flash) => {
                let aperture = apertures
                    .get(flash.aperture_id.as_str())
                    .copied()
                    .filter(|aperture| {
                        aperture.shape == ApertureShape::Circle
                            && aperture.dimensions.len() == 1
                            && aperture.macro_id.is_none()
                    })
                    .ok_or_else(|| {
                        format!(
                            "copper flash {} lacks one exact circular aperture",
                            feature.id
                        )
                    })?;
                AxisPrimitive {
                    start: flash.position,
                    end: flash.position,
                    radius: exact_half(aperture.dimensions[0], "copper flash diameter")?,
                }
            }
            _ => {
                return Err(format!(
                    "copper feature {} geometry is outside the exact represented line/round-flash subset",
                    feature.id
                ));
            }
        };
        let resolution = exact_geometry_resolution(review, &feature.provenance)?;
        if !primitive_on_resolution(primitive, resolution) {
            return Err(format!(
                "copper feature {} is inconsistent with source resolution",
                feature.id
            ));
        }
        output.push(LocatedPrimitive {
            owner_id: &feature.id,
            segment: 0,
            layer_id: &feature.layer_id,
            primitive,
            resolution,
            provenance: &feature.provenance,
            boundary_kind: None,
        });
    }
    if output.is_empty() {
        return Err("no exact represented copper geometry is available".into());
    }
    Ok(output)
}

fn profile_boundary_primitives<'a>(
    review: &'a FabricationReview,
    deadline: ManufacturingDeadline,
) -> Result<Vec<LocatedPrimitive<'a>>, String> {
    let profile = review
        .profile
        .as_ref()
        .ok_or("canonical profile is missing")?;
    if profile.contour_feature_ids.len() != 1 || profile.provenance.is_empty() {
        return Err("exterior profile identity or provenance is ambiguous".into());
    }
    let mut requested = profile
        .contour_feature_ids
        .iter()
        .map(|id| (BoundaryKind::Exterior, id.as_str()))
        .chain(
            profile
                .cutout_feature_ids
                .iter()
                .map(|id| (BoundaryKind::Cutout, id.as_str())),
        )
        .collect::<Vec<_>>();
    requested.sort();
    let mut seen = BTreeSet::new();
    let mut output = Vec::new();
    for (kind, id) in requested {
        deadline
            .check("dfm-copper-edge-profile")
            .map_err(|error| error.to_string())?;
        if !seen.insert(id) {
            return Err("profile boundary identity is duplicated".into());
        }
        let mut features = review.features.iter().filter(|feature| feature.id == id);
        let feature = features
            .next()
            .filter(|_| features.next().is_none())
            .ok_or_else(|| format!("profile feature {id} is missing or ambiguous"))?;
        let mut layers = review
            .layers
            .iter()
            .filter(|layer| layer.id == feature.layer_id);
        let layer = layers
            .next()
            .filter(|_| layers.next().is_none())
            .ok_or_else(|| format!("profile feature {id} layer is missing or ambiguous"))?;
        let polarity = match kind {
            BoundaryKind::Exterior => {
                matches!(
                    feature.polarity,
                    LayerPolarity::Dark | LayerPolarity::Positive
                )
            }
            BoundaryKind::Cutout => feature.polarity == LayerPolarity::Clear,
            BoundaryKind::Routed => unreachable!(),
        };
        if layer.role != LayerRole::Profile
            || !polarity
            || !feature.transforms.operations.is_empty()
            || feature.membership != FeatureMembership::TopLevel
        {
            return Err(format!(
                "profile feature {id} has unresolved role, polarity, transform, or expansion"
            ));
        }
        let contour = match &feature.geometry {
            Geometry::Contour(contour) => contour,
            Geometry::Region(region) if region.contours.len() == 1 => &region.contours[0],
            _ => return Err(format!("profile feature {id} is not one exact contour")),
        };
        if !contour.closed || contour.segments.is_empty() {
            return Err(format!("profile feature {id} is open or empty"));
        }
        let resolution = exact_geometry_resolution(review, &feature.provenance)?;
        for (segment, value) in contour.segments.iter().enumerate() {
            let ContourSegment::Line(line) = value else {
                return Err(format!(
                    "profile feature {id} contains an unsupported exact-distance arc"
                ));
            };
            if line.width.is_some() {
                return Err(format!(
                    "profile feature {id} has an ambiguous stroked boundary"
                ));
            }
            let primitive = axis_primitive(line, Picometres(0), "profile boundary")?;
            if !primitive_on_resolution(primitive, resolution) {
                return Err(format!(
                    "profile feature {id} is inconsistent with source resolution"
                ));
            }
            output.push(LocatedPrimitive {
                owner_id: id,
                segment,
                layer_id: &feature.layer_id,
                primitive,
                resolution,
                provenance: &feature.provenance,
                boundary_kind: Some(kind),
            });
        }
    }
    Ok(output)
}

fn routed_boundary_primitives<'a>(
    review: &'a FabricationReview,
    deadline: ManufacturingDeadline,
) -> Result<Vec<LocatedPrimitive<'a>>, String> {
    let has_routes = review
        .features
        .iter()
        .any(|feature| matches!(feature.geometry, Geometry::Route(_)));
    let has_slots = review
        .features
        .iter()
        .any(|feature| matches!(feature.geometry, Geometry::Slot(_)));
    if !has_routes && !has_slots {
        return Ok(Vec::new());
    }
    dispatch_complete(review, DRILL_TOOL_REQUIREMENTS)?;
    dispatch_complete(review, PLATING_SPAN_REQUIREMENTS)?;
    if has_routes {
        dispatch_complete(review, ROUTE_REQUIREMENT)?;
    }
    if has_slots {
        dispatch_complete(review, SLOT_REQUIREMENT)?;
    }
    if affected_capability_evidence(
        review,
        &[
            CapabilityId::UnitsAndFormat,
            CapabilityId::Tools,
            CapabilityId::Routes,
            CapabilityId::Slots,
            CapabilityId::Plating,
            CapabilityId::LayerSpans,
        ],
    ) {
        return Err("affected route/slot evidence prevents exact routed boundaries".into());
    }
    let mut output = Vec::new();
    for feature in &review.features {
        let tool_id = match &feature.geometry {
            Geometry::Route(route) => &route.tool_id,
            Geometry::Slot(slot) => &slot.tool_id,
            _ => continue,
        };
        deadline
            .check("dfm-copper-edge-routes")
            .map_err(|error| error.to_string())?;
        let (tool, resolution) = validated_geometry_tool(review, feature, tool_id)?;
        if tool.plating != Plating::NonPlated {
            continue;
        }
        let segments = match &feature.geometry {
            Geometry::Route(route) => route
                .segments
                .iter()
                .map(|segment| match segment {
                    ContourSegment::Line(line) => Ok(line.clone()),
                    ContourSegment::Arc(_) => Err(format!(
                        "routed boundary {} contains an unsupported arc",
                        feature.id
                    )),
                })
                .collect::<Result<Vec<_>, _>>()?,
            Geometry::Slot(slot) => vec![CanonicalLine {
                start: slot.start,
                end: slot.end,
                width: Some(slot.width),
            }],
            _ => unreachable!("route/slot filtered above"),
        };
        if !feature.transforms.operations.is_empty()
            || feature.membership != FeatureMembership::TopLevel
        {
            return Err(format!(
                "routed boundary {} has unresolved transform or expansion",
                feature.id
            ));
        }
        for (segment, line) in segments.iter().enumerate() {
            let width = line
                .width
                .filter(|width| Some(*width) == tool.diameter)
                .ok_or_else(|| format!("routed boundary {} width is ambiguous", feature.id))?;
            let primitive =
                axis_primitive(line, exact_half(width, "route width")?, "routed boundary")?;
            if !primitive_on_resolution(primitive, resolution) {
                return Err(format!(
                    "routed boundary {} is inconsistent with source resolution",
                    feature.id
                ));
            }
            output.push(LocatedPrimitive {
                owner_id: &feature.id,
                segment,
                layer_id: &feature.layer_id,
                primitive,
                resolution,
                provenance: &feature.provenance,
                boundary_kind: Some(BoundaryKind::Routed),
            });
        }
    }
    Ok(output)
}

fn copper_edge(
    review: &FabricationReview,
    deadline: ManufacturingDeadline,
) -> (Vec<Finding>, Coverage) {
    const LABEL: &str = "Exact copper-to-edge distance";
    let result = (|| -> Result<(Vec<Finding>, Coverage), String> {
        dispatch_complete(review, COPPER_EDGE_REQUIREMENTS)?;
        if affected_capability_evidence(review, COPPER_EDGE_REQUIREMENTS.prerequisites) {
            return Err(
                "affected omission or conflict prevents complete copper-edge measurement".into(),
            );
        }
        let (threshold, authority_document) =
            source_bound_threshold(review, ConstraintKind::Other, COPPER_EDGE_FAMILY)?;
        let threshold_value = threshold
            .value
            .ok_or("copper-edge threshold lost its value")?;
        let copper = copper_primitives(review, deadline)?;
        let mut boundaries = profile_boundary_primitives(review, deadline)?;
        boundaries.extend(routed_boundary_primitives(review, deadline)?);
        let nearest = nearest_axis_pair(copper, boundaries, deadline, "dfm-copper-edge")?;
        let resolution = comparison_resolution(nearest.left.resolution, nearest.right.resolution)?;
        if nearest.observed.0 % resolution.0 != 0 || threshold_value.0 % resolution.0 != 0 {
            return Err(
                "copper-edge measurement or threshold is off the compared source grid".into(),
            );
        }
        let delta = threshold_value
            .0
            .checked_sub(nearest.observed.0)
            .ok_or("copper-edge delta overflow")?;
        let boundary = nearest
            .right
            .boundary_kind
            .ok_or("copper-edge boundary classification is missing")?;
        let measurement = format!(
            "observed={}pm threshold={}pm delta={delta}pm resolution={}pm copper={} copper_segment={} copper_layer={} copper_source={}:{} boundary={} boundary_feature={} boundary_segment={} boundary_source={}:{} authority={} {} source={} threshold_record={} candidate_checks={}",
            nearest.observed.0,
            threshold_value.0,
            resolution.0,
            nearest.left.owner_id,
            nearest.left.segment,
            nearest.left.layer_id,
            nearest.left.provenance.document_id,
            nearest.left.provenance.location.record,
            boundary.name(),
            nearest.right.owner_id,
            nearest.right.segment,
            nearest.right.provenance.document_id,
            nearest.right.provenance.location.record,
            threshold.provenance.producer,
            threshold.provenance.producer_version,
            authority_document.virtual_path,
            threshold.provenance.location.record,
            nearest.candidate_checks,
        );
        let findings = if delta > 0 {
            vec![Finding {
                id: format!(
                    "{COPPER_EDGE_FAMILY}/{}/{}/{}/{}",
                    nearest.left.owner_id,
                    boundary.name(),
                    nearest.right.owner_id,
                    nearest.right.segment
                ),
                severity: Severity::Medium,
                category: "DFM".into(),
                title: "Copper is closer to an edge than declared".into(),
                evidence: measurement.clone(),
                recommendation:
                    "Move the represented copper or revise the source-bound copper-edge requirement."
                        .into(),
                location: format!(
                    "copper={}:{};boundary={}:{}:{}",
                    nearest.left.owner_id,
                    nearest.left.segment,
                    boundary.name(),
                    nearest.right.owner_id,
                    nearest.right.segment
                ),
                source: "fabrication".into(),
                gate_impact: family_gate_impact(COPPER_EDGE_FAMILY),
            }]
        } else {
            Vec::new()
        };
        Ok((
            findings,
            Coverage {
                id: COPPER_EDGE_FAMILY.into(),
                label: LABEL.into(),
                status: if delta > 0 {
                    CoverageStatus::Attention
                } else {
                    CoverageStatus::Passed
                },
                evidence: measurement,
            },
        ))
    })();
    result.unwrap_or_else(|reason| (vec![], not_checked(COPPER_EDGE_FAMILY, LABEL, reason)))
}

#[derive(Clone, Copy)]
struct ConnectedPrimitive<'a> {
    geometry: LocatedPrimitive<'a>,
    semantics: &'a ObjectSemantics,
}

#[derive(Clone, Copy)]
struct ClearanceNearest<'a> {
    observed: Picometres,
    left: ConnectedPrimitive<'a>,
    right: ConnectedPrimitive<'a>,
    pair_checks: usize,
    different_net_pairs: usize,
}

fn connected_copper_primitives<'a>(
    review: &'a FabricationReview,
    deadline: ManufacturingDeadline,
) -> Result<Vec<ConnectedPrimitive<'a>>, String> {
    let copper = copper_primitives(review, deadline)?;
    let mut semantics = BTreeMap::<&str, Vec<&ObjectSemantics>>::new();
    for item in &review.connectivity {
        deadline
            .check("dfm-copper-clearance-connectivity")
            .map_err(|error| error.to_string())?;
        semantics
            .entry(item.feature_id.as_str())
            .or_default()
            .push(item);
    }
    let mut output = Vec::with_capacity(copper.len());
    for geometry in copper {
        let matches = semantics
            .get(geometry.owner_id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let semantic = match matches {
            [semantic]
                if semantic
                    .net
                    .as_deref()
                    .is_some_and(|net| !net.trim().is_empty() && net.trim() == net) =>
            {
                *semantic
            }
            _ => {
                return Err(format!(
                    "copper feature {} lacks one complete explicit net identity",
                    geometry.owner_id
                ));
            }
        };
        output.push(ConnectedPrimitive {
            geometry,
            semantics: semantic,
        });
    }
    Ok(output)
}

fn nearest_clearance_pair<'a>(
    copper: Vec<ConnectedPrimitive<'a>>,
    deadline: ManufacturingDeadline,
) -> Result<Option<ClearanceNearest<'a>>, String> {
    if copper.len() > MAX_DISTANCE_PRIMITIVES {
        return Err(format!(
            "clearance primitive limit {MAX_DISTANCE_PRIMITIVES} exceeded"
        ));
    }
    deadline
        .check("dfm-copper-clearance-buckets")
        .map_err(|error| error.to_string())?;
    let mut layers = BTreeMap::<&str, BTreeMap<&str, Vec<ConnectedPrimitive<'a>>>>::new();
    for item in copper {
        deadline
            .check("dfm-copper-clearance-buckets")
            .map_err(|error| error.to_string())?;
        let net = item
            .semantics
            .net
            .as_deref()
            .ok_or("validated copper net disappeared")?;
        layers
            .entry(item.geometry.layer_id)
            .or_default()
            .entry(net)
            .or_default()
            .push(item);
    }
    let compare = |left: &ConnectedPrimitive<'_>, right: &ConnectedPrimitive<'_>| {
        let left_bounds =
            primitive_bounds(left.geometry.primitive).unwrap_or((i128::MIN, 0, i128::MAX, 0));
        let right_bounds =
            primitive_bounds(right.geometry.primitive).unwrap_or((i128::MIN, 0, i128::MAX, 0));
        (
            left_bounds,
            left.geometry.owner_id,
            left.geometry.segment,
            left.semantics.net.as_deref().unwrap_or(""),
        )
            .cmp(&(
                right_bounds,
                right.geometry.owner_id,
                right.geometry.segment,
                right.semantics.net.as_deref().unwrap_or(""),
            ))
    };
    let mut best: Option<ClearanceNearest<'a>> = None;
    let mut inexact = Vec::new();
    let mut pair_checks = 0_usize;
    let mut different_net_pairs = 0_usize;
    for nets in layers.values_mut() {
        deadline
            .check("dfm-copper-clearance-buckets")
            .map_err(|error| error.to_string())?;
        for values in nets.values_mut() {
            deadline
                .check("dfm-copper-clearance-sort")
                .map_err(|error| error.to_string())?;
            values.sort_by(compare);
            deadline
                .check("dfm-copper-clearance-sort")
                .map_err(|error| error.to_string())?;
        }
        let buckets = nets.values().collect::<Vec<_>>();
        for left_bucket in 0..buckets.len() {
            for right_bucket in (left_bucket + 1)..buckets.len() {
                for left in buckets[left_bucket].iter().copied() {
                    let left_bounds = primitive_bounds(left.geometry.primitive)?;
                    for right in buckets[right_bucket].iter().copied() {
                        deadline
                            .check("dfm-copper-clearance")
                            .map_err(|error| error.to_string())?;
                        pair_checks = pair_checks
                            .checked_add(1)
                            .ok_or("clearance pair count overflow")?;
                        different_net_pairs = different_net_pairs
                            .checked_add(1)
                            .ok_or("different-net pair count overflow")?;
                        if pair_checks > MAX_DISTANCE_CANDIDATES {
                            return Err(format!(
                                "clearance pair limit {MAX_DISTANCE_CANDIDATES} exceeded"
                            ));
                        }
                        let right_bounds = primitive_bounds(right.geometry.primitive)?;
                        if let Some(current) = best {
                            let limit = i128::from(current.observed.0);
                            if right_bounds.0 > left_bounds.2.saturating_add(limit) {
                                break;
                            }
                            if right_bounds.2 < left_bounds.0.saturating_sub(limit)
                                || right_bounds.1 > left_bounds.3.saturating_add(limit)
                                || right_bounds.3 < left_bounds.1.saturating_sub(limit)
                            {
                                continue;
                            }
                        }
                        let distance =
                            axis_distance(left.geometry.primitive, right.geometry.primitive)?;
                        let Some(observed) = exact_axis_measurement(distance)? else {
                            if let Some(current) = best
                                && !inexact_distance_can_beat(distance, current.observed)?
                            {
                                continue;
                            }
                            if inexact.len() >= MAX_INEXACT_DISTANCE_CANDIDATES {
                                return Err(format!(
                                    "inexact clearance candidate limit {MAX_INEXACT_DISTANCE_CANDIDATES} exceeded"
                                ));
                            }
                            inexact.push(distance);
                            continue;
                        };
                        let candidate = ClearanceNearest {
                            observed,
                            left,
                            right,
                            pair_checks,
                            different_net_pairs,
                        };
                        let candidate_key = (
                            candidate.observed,
                            candidate.left.geometry.layer_id,
                            candidate.left.geometry.owner_id,
                            candidate.left.geometry.segment,
                            candidate.right.geometry.owner_id,
                            candidate.right.geometry.segment,
                        );
                        if best.is_none_or(|current| {
                            candidate_key
                                < (
                                    current.observed,
                                    current.left.geometry.layer_id,
                                    current.left.geometry.owner_id,
                                    current.left.geometry.segment,
                                    current.right.geometry.owner_id,
                                    current.right.geometry.segment,
                                )
                        }) {
                            best = Some(candidate);
                        }
                    }
                }
            }
        }
    }
    deadline
        .check("dfm-copper-clearance-complete")
        .map_err(|error| error.to_string())?;
    if let Some(mut best) = best {
        if inexact.into_iter().try_fold(false, |closer, distance| {
            inexact_distance_can_beat(distance, best.observed).map(|value| closer || value)
        })? {
            return Err(
                "an inexact different-net distance may be nearer than the exact result".into(),
            );
        }
        best.pair_checks = pair_checks;
        best.different_net_pairs = different_net_pairs;
        Ok(Some(best))
    } else if different_net_pairs == 0 {
        Ok(None)
    } else {
        Err("no exact represented different-net distance is available".into())
    }
}

fn copper_clearance(
    review: &FabricationReview,
    deadline: ManufacturingDeadline,
) -> (Vec<Finding>, Coverage) {
    const LABEL: &str = "Exact same-layer different-net copper clearance";
    let result = (|| -> Result<(Vec<Finding>, Coverage), String> {
        dispatch_complete(review, COPPER_CLEARANCE_REQUIREMENTS)?;
        if affected_capability_evidence(review, COPPER_CLEARANCE_REQUIREMENTS.prerequisites) {
            return Err(
                "affected omission or conflict prevents complete copper-clearance measurement"
                    .into(),
            );
        }
        let (threshold, authority_document) = source_bound_threshold(
            review,
            ConstraintKind::MinimumClearance,
            "minimum_clearance",
        )?;
        let threshold_value = threshold
            .value
            .ok_or("copper-clearance threshold lost its value")?;
        let copper = connected_copper_primitives(review, deadline)?;
        let Some(nearest) = nearest_clearance_pair(copper, deadline)? else {
            return Ok((
                Vec::new(),
                Coverage {
                    id: COPPER_CLEARANCE_FAMILY.into(),
                    label: LABEL.into(),
                    status: CoverageStatus::Passed,
                    evidence: format!(
                        "different_net_pairs=0 threshold={}pm complete explicit connectivity contains no same-physical-layer proven-different-net pair; authority={} {} source={} threshold_record={}",
                        threshold_value.0,
                        threshold.provenance.producer,
                        threshold.provenance.producer_version,
                        authority_document.virtual_path,
                        threshold.provenance.location.record,
                    ),
                },
            ));
        };
        let resolution = comparison_resolution(
            nearest.left.geometry.resolution,
            nearest.right.geometry.resolution,
        )?;
        if nearest.observed.0 % resolution.0 != 0 || threshold_value.0 % resolution.0 != 0 {
            return Err(
                "copper-clearance measurement or threshold is off the compared source grid".into(),
            );
        }
        let delta = threshold_value
            .0
            .checked_sub(nearest.observed.0)
            .ok_or("copper-clearance delta overflow")?;
        let left_net = nearest
            .left
            .semantics
            .net
            .as_deref()
            .ok_or("validated left net disappeared")?;
        let right_net = nearest
            .right
            .semantics
            .net
            .as_deref()
            .ok_or("validated right net disappeared")?;
        let measurement = format!(
            "observed={}pm threshold={}pm delta={delta}pm resolution={}pm layer={} left={} left_segment={} left_net={} left_source={}:{} left_net_source={}:{} right={} right_segment={} right_net={} right_source={}:{} right_net_source={}:{} authority={} {} source={} threshold_record={} pair_checks={} different_net_pairs={}",
            nearest.observed.0,
            threshold_value.0,
            resolution.0,
            nearest.left.geometry.layer_id,
            nearest.left.geometry.owner_id,
            nearest.left.geometry.segment,
            left_net,
            nearest.left.geometry.provenance.document_id,
            nearest.left.geometry.provenance.location.record,
            nearest.left.semantics.provenance.document_id,
            nearest.left.semantics.provenance.location.record,
            nearest.right.geometry.owner_id,
            nearest.right.geometry.segment,
            right_net,
            nearest.right.geometry.provenance.document_id,
            nearest.right.geometry.provenance.location.record,
            nearest.right.semantics.provenance.document_id,
            nearest.right.semantics.provenance.location.record,
            threshold.provenance.producer,
            threshold.provenance.producer_version,
            authority_document.virtual_path,
            threshold.provenance.location.record,
            nearest.pair_checks,
            nearest.different_net_pairs,
        );
        let findings = if delta > 0 {
            vec![Finding {
                id: format!(
                    "{COPPER_CLEARANCE_FAMILY}/{}/{}/{}",
                    nearest.left.geometry.layer_id,
                    nearest.left.geometry.owner_id,
                    nearest.right.geometry.owner_id
                ),
                severity: Severity::Medium,
                category: "DFM".into(),
                title: "Different-net copper clearance is below the declared minimum".into(),
                evidence: measurement.clone(),
                recommendation:
                    "Separate the represented different-net copper or revise the source-bound clearance requirement."
                        .into(),
                location: format!(
                    "layer={};left={}:{};right={}:{}",
                    nearest.left.geometry.layer_id,
                    nearest.left.geometry.owner_id,
                    nearest.left.geometry.segment,
                    nearest.right.geometry.owner_id,
                    nearest.right.geometry.segment
                ),
                source: "fabrication".into(),
                gate_impact: family_gate_impact(COPPER_CLEARANCE_FAMILY),
            }]
        } else {
            Vec::new()
        };
        Ok((
            findings,
            Coverage {
                id: COPPER_CLEARANCE_FAMILY.into(),
                label: LABEL.into(),
                status: if delta > 0 {
                    CoverageStatus::Attention
                } else {
                    CoverageStatus::Passed
                },
                evidence: measurement,
            },
        ))
    })();
    result.unwrap_or_else(|reason| (vec![], not_checked(COPPER_CLEARANCE_FAMILY, LABEL, reason)))
}

fn authoritative_native_pad_holes(
    review: &FabricationReview,
    deadline: ManufacturingDeadline,
) -> Result<BTreeSet<&str>, String> {
    let native_documents = review
        .documents
        .iter()
        .filter(|document| {
            document.format == DocumentFormat::KicadPcb
                && document.adapter == KICAD_MANUFACTURING_ADAPTER
                && document.adapter_version == KICAD_MANUFACTURING_ADAPTER_VERSION
                && document.parse_status == ParseStatus::Complete
        })
        .map(|document| document.id.as_str())
        .collect::<BTreeSet<_>>();
    if native_documents.len() != 1 {
        return Err("one complete authoritative native KiCad document is required".into());
    }
    let features = review
        .features
        .iter()
        .map(|feature| (feature.id.as_str(), feature))
        .collect::<BTreeMap<_, _>>();
    let mut output = BTreeSet::new();
    for semantic in &review.connectivity {
        deadline
            .check("dfm-annular-ring-pad-authority")
            .map_err(|error| error.to_string())?;
        let Some(feature) = features.get(semantic.feature_id.as_str()).copied() else {
            return Err("pad connectivity has a dangling feature identity".into());
        };
        if !native_documents.contains(feature.document_id.as_str())
            || !matches!(feature.geometry, Geometry::Drill(_) | Geometry::Slot(_))
        {
            continue;
        }
        if feature.provenance != semantic.provenance
            || semantic
                .component
                .as_deref()
                .is_none_or(|value| value.is_empty())
            || semantic.pin.as_deref().is_none_or(|value| value.is_empty())
        {
            return Err(format!(
                "native pad-hole feature {} lacks same-object component/pin authority",
                feature.id
            ));
        }
        if !output.insert(feature.id.as_str()) {
            return Err(format!(
                "native pad-hole feature {} has duplicate semantic authority",
                feature.id
            ));
        }
    }
    if output.is_empty() {
        return Err("no authoritative native KiCad pad-hole object is available".into());
    }
    Ok(output)
}

fn annular_ring(
    review: &FabricationReview,
    deadline: ManufacturingDeadline,
) -> (Vec<Finding>, Coverage) {
    const LABEL: &str = "Exact authoritative native annular ring";
    let result = (|| -> Result<(Vec<Finding>, Coverage), String> {
        dispatch_complete(review, ANNULAR_RING_REQUIREMENTS)?;
        if affected_capability_evidence(review, ANNULAR_RING_REQUIREMENTS.prerequisites) {
            return Err(
                "affected omission or conflict prevents complete annular-ring measurement".into(),
            );
        }
        let (threshold, authority_document) = source_bound_threshold(
            review,
            ConstraintKind::MinimumAnnularRing,
            "minimum_annular_ring",
        )?;
        let threshold_value = threshold
            .value
            .ok_or("annular-ring threshold lost its value")?;
        let expected_holes = authoritative_native_pad_holes(review, deadline)?;
        let actual_holes = review
            .pad_hole_associations
            .iter()
            .map(|association| association.hole_id.as_str())
            .collect::<BTreeSet<_>>();
        if actual_holes != expected_holes
            || actual_holes.len() != review.pad_hole_associations.len()
        {
            return Err(
                "native pad-hole authority is absent, duplicate, unsupported, or incomplete".into(),
            );
        }
        if review.pad_hole_associations.len() > MAX_DISTANCE_PRIMITIVES {
            return Err(format!(
                "annular association limit {MAX_DISTANCE_PRIMITIVES} exceeded"
            ));
        }
        let layers = review
            .layers
            .iter()
            .map(|layer| (layer.id.as_str(), layer))
            .collect::<BTreeMap<_, _>>();
        let features = review
            .features
            .iter()
            .map(|feature| (feature.id.as_str(), feature))
            .collect::<BTreeMap<_, _>>();
        let tools = review
            .tools
            .iter()
            .map(|tool| (tool.id.as_str(), tool))
            .collect::<BTreeMap<_, _>>();
        let mut associations = review.pad_hole_associations.iter().collect::<Vec<_>>();
        associations.sort_by_key(|association| association.id.as_str());
        let mut findings = Vec::new();
        let mut details = Vec::new();
        let mut minimum: Option<(Picometres, &str, &str, Picometres)> = None;
        let mut layer_measurements = 0_usize;
        for association in associations {
            deadline
                .check("dfm-annular-ring")
                .map_err(|error| error.to_string())?;
            let source_hole = features
                .get(association.hole_id.as_str())
                .copied()
                .ok_or_else(|| format!("{} hole identity is dangling", association.id))?;
            let tool = tools
                .get(association.tool_id.as_str())
                .copied()
                .ok_or_else(|| format!("{} tool identity is dangling", association.id))?;
            let mut orders = Vec::new();
            let mut unique_layers = BTreeSet::new();
            for layer_id in &association.applicable_layer_ids {
                let layer = layers
                    .get(layer_id.as_str())
                    .copied()
                    .ok_or_else(|| format!("{} layer identity is dangling", association.id))?;
                if !unique_layers.insert(layer_id.as_str())
                    || layer.role != LayerRole::Copper
                    || layer.order.is_none()
                {
                    return Err(format!(
                        "{} applicable layers are ambiguous",
                        association.id
                    ));
                }
                orders.push(layer.order);
            }
            if association.applicable_layer_ids.is_empty()
                || orders.windows(2).any(|pair| pair[0] >= pair[1])
                || association.span.from_layer_id.as_ref()
                    != association.applicable_layer_ids.first()
                || association.span.to_layer_id.as_ref() != association.applicable_layer_ids.last()
                || tool.span.as_ref() != Some(&association.span)
                || tool.plating != Plating::Plated
                || tool.kind != ToolKind::Drill
                || source_hole.tool_id.as_deref() != Some(association.tool_id.as_str())
                || source_hole.provenance != association.hole_provenance
                || association.pad_provenance != association.hole_provenance
                || association.pad_provenance.producer != KICAD_MANUFACTURING_ADAPTER
                || association.pad_provenance.producer_version
                    != KICAD_MANUFACTURING_ADAPTER_VERSION
            {
                return Err(format!(
                    "{} plating, span, layer, or same-object authority is incomplete",
                    association.id
                ));
            }
            let (pad_center, pad_radius, pad_resolution) =
                exact_circle_geometry(&association.pad_geometry)
                    .map_err(|error| format!("{}: {error}", association.pad_id))?;
            let Geometry::Drill(hole) = &association.hole_geometry else {
                return Err(format!(
                    "{} hole geometry is unsupported",
                    association.hole_id
                ));
            };
            if hole.position != pad_center
                || hole.diameter.0 <= 0
                || hole.diameter.0 % 2 != 0
                || association.plating != Plating::Plated
            {
                return Err(format!(
                    "{} pad/hole geometry or plating is incomplete",
                    association.id
                ));
            }
            let source_resolution =
                exact_geometry_resolution(review, &association.hole_provenance)?;
            let resolution = comparison_resolution(pad_resolution, source_resolution)?;
            let hole_radius = Picometres(hole.diameter.0 / 2);
            let observed = Picometres(
                pad_radius
                    .0
                    .checked_sub(hole_radius.0)
                    .ok_or("annular-ring subtraction overflow")?,
            );
            if [
                pad_center.x.0,
                pad_center.y.0,
                pad_radius.0,
                hole_radius.0,
                observed.0,
                threshold_value.0,
            ]
            .into_iter()
            .any(|value| value % resolution.0 != 0)
            {
                return Err(format!(
                    "{} measurement or threshold is off the native source grid",
                    association.id
                ));
            }
            for layer_id in &association.applicable_layer_ids {
                deadline
                    .check("dfm-annular-ring-layers")
                    .map_err(|error| error.to_string())?;
                layer_measurements = layer_measurements
                    .checked_add(1)
                    .ok_or("annular-ring layer count overflow")?;
                let delta = threshold_value
                    .0
                    .checked_sub(observed.0)
                    .ok_or("annular-ring delta overflow")?;
                let measurement = format!(
                    "observed={}pm threshold={}pm delta={delta}pm resolution={}pm association={} pad={} hole={} tool={} layer={} span={:?}..{:?} pad_source={}:{} hole_source={}:{} authority={} {} source={} threshold_record={}",
                    observed.0,
                    threshold_value.0,
                    resolution.0,
                    association.id,
                    association.pad_id,
                    association.hole_id,
                    association.tool_id,
                    layer_id,
                    association.span.from_layer_id,
                    association.span.to_layer_id,
                    association.pad_provenance.document_id,
                    association.pad_provenance.location.record,
                    association.hole_provenance.document_id,
                    association.hole_provenance.location.record,
                    threshold.provenance.producer,
                    threshold.provenance.producer_version,
                    authority_document.virtual_path,
                    threshold.provenance.location.record,
                );
                details.push(measurement.clone());
                let candidate = (
                    observed,
                    association.id.as_str(),
                    layer_id.as_str(),
                    resolution,
                );
                if minimum.is_none_or(|current| candidate < current) {
                    minimum = Some(candidate);
                }
                if delta > 0 {
                    findings.push(Finding {
                        id: format!(
                            "{ANNULAR_RING_FAMILY}/{}/{}/{}",
                            association.id, association.hole_id, layer_id
                        ),
                        severity: Severity::Medium,
                        category: "DFM".into(),
                        title: "Annular ring is below the declared minimum".into(),
                        evidence: measurement,
                        recommendation:
                            "Increase the exact native pad copper around this hole or revise the source-bound annular-ring requirement."
                                .into(),
                        location: format!(
                            "association={};pad={};hole={};tool={};layer={layer_id}",
                            association.id,
                            association.pad_id,
                            association.hole_id,
                            association.tool_id,
                        ),
                        source: "fabrication".into(),
                        gate_impact: family_gate_impact(ANNULAR_RING_FAMILY),
                    });
                }
            }
        }
        findings.sort_by(|left, right| left.id.cmp(&right.id));
        details.sort();
        let (observed, _, _, resolution) =
            minimum.ok_or("no applicable native pad layer is available")?;
        let delta = threshold_value
            .0
            .checked_sub(observed.0)
            .ok_or("annular-ring minimum delta overflow")?;
        let evidence = format!(
            "observed={}pm threshold={}pm delta={delta}pm resolution={}pm associations={} layers={layer_measurements}; {}",
            observed.0,
            threshold_value.0,
            resolution.0,
            review.pad_hole_associations.len(),
            details.join(" | "),
        );
        Ok((
            findings,
            Coverage {
                id: ANNULAR_RING_FAMILY.into(),
                label: LABEL.into(),
                status: if delta > 0 {
                    CoverageStatus::Attention
                } else {
                    CoverageStatus::Passed
                },
                evidence,
            },
        ))
    })();
    result.unwrap_or_else(|reason| (vec![], not_checked(ANNULAR_RING_FAMILY, LABEL, reason)))
}

#[derive(Clone, Copy)]
struct PadIntent<'a> {
    component: &'a str,
    pin: &'a str,
    provenance: &'a ManufacturingProvenance,
}

type PadIntentIndex<'a> = BTreeMap<&'a str, Vec<PadIntent<'a>>>;
type RolePrimitives<'a> = (
    Vec<LocatedPrimitive<'a>>,
    PadIntentIndex<'a>,
    BTreeMap<&'a str, LayerSide>,
);

fn exact_apertures<'a>(
    review: &'a FabricationReview,
    deadline: ManufacturingDeadline,
    resource: &'static str,
) -> Result<BTreeMap<&'a str, &'a ApertureDefinition>, String> {
    let mut apertures = BTreeMap::new();
    for aperture in &review.apertures {
        deadline
            .check(resource)
            .map_err(|error| error.to_string())?;
        if apertures.insert(aperture.id.as_str(), aperture).is_some() {
            return Err(format!("duplicate aperture identity {}", aperture.id));
        }
    }
    Ok(apertures)
}

fn exact_role_primitive<'a>(
    review: &'a FabricationReview,
    apertures: &BTreeMap<&str, &'a ApertureDefinition>,
    repeated: &BTreeSet<&str>,
    feature: &'a ManufacturingFeature,
    label: &str,
) -> Result<LocatedPrimitive<'a>, String> {
    if feature.polarity != LayerPolarity::Dark
        || !feature.transforms.operations.is_empty()
        || feature.membership != FeatureMembership::TopLevel
        || repeated.contains(feature.id.as_str())
    {
        return Err(format!(
            "{label} {} has unresolved polarity, transform, or expansion",
            feature.id
        ));
    }
    let primitive = match &feature.geometry {
        Geometry::Line(line) => {
            let width = line
                .width
                .ok_or_else(|| format!("{label} {} has no exact width", feature.id))?;
            axis_primitive(line, exact_half(width, label)?, label)?
        }
        Geometry::Flash(flash) => {
            let aperture = apertures
                .get(flash.aperture_id.as_str())
                .copied()
                .filter(|aperture| {
                    aperture.document_id == feature.document_id
                        && aperture.shape == ApertureShape::Circle
                        && aperture.dimensions.len() == 1
                        && aperture.macro_id.is_none()
                })
                .ok_or_else(|| {
                    format!(
                        "{label} {} lacks one same-document exact circular aperture",
                        feature.id
                    )
                })?;
            AxisPrimitive {
                start: flash.position,
                end: flash.position,
                radius: exact_half(aperture.dimensions[0], label)?,
            }
        }
        _ => {
            return Err(format!(
                "{label} {} is outside the exact represented line/round-flash subset",
                feature.id
            ));
        }
    };
    let resolution = exact_geometry_resolution(review, &feature.provenance)?;
    if !primitive_on_resolution(primitive, resolution) {
        return Err(format!(
            "{label} {} is inconsistent with source resolution",
            feature.id
        ));
    }
    Ok(LocatedPrimitive {
        owner_id: feature.id.as_str(),
        segment: 0,
        layer_id: feature.layer_id.as_str(),
        primitive,
        resolution,
        provenance: &feature.provenance,
        boundary_kind: None,
    })
}

fn pad_intents<'a>(
    review: &'a FabricationReview,
    feature_ids: &BTreeSet<&'a str>,
    deadline: ManufacturingDeadline,
    resource: &'static str,
) -> Result<PadIntentIndex<'a>, String> {
    let mut output = BTreeMap::<&str, Vec<PadIntent<'a>>>::new();
    let mut seen = BTreeSet::new();
    for semantic in &review.connectivity {
        deadline
            .check(resource)
            .map_err(|error| error.to_string())?;
        if !feature_ids.contains(semantic.feature_id.as_str()) {
            continue;
        }
        let component = semantic
            .component
            .as_deref()
            .filter(|value| !value.is_empty() && value.trim() == *value)
            .ok_or_else(|| {
                format!(
                    "feature {} lacks represented component intent",
                    semantic.feature_id
                )
            })?;
        let pin = semantic
            .pin
            .as_deref()
            .filter(|value| !value.is_empty() && value.trim() == *value)
            .ok_or_else(|| {
                format!(
                    "feature {} lacks represented pad intent",
                    semantic.feature_id
                )
            })?;
        if !seen.insert((semantic.feature_id.as_str(), component, pin)) {
            return Err(format!(
                "feature {} has duplicate component/pad intent",
                semantic.feature_id
            ));
        }
        output
            .entry(semantic.feature_id.as_str())
            .or_default()
            .push(PadIntent {
                component,
                pin,
                provenance: &semantic.provenance,
            });
    }
    for id in feature_ids {
        let intents = output
            .get_mut(id)
            .filter(|intents| !intents.is_empty())
            .ok_or_else(|| format!("feature {id} lacks explicit opening/pad intent"))?;
        deadline
            .check(resource)
            .map_err(|error| error.to_string())?;
        intents.sort_by_key(|intent| {
            (
                intent.component,
                intent.pin,
                intent.provenance.document_id.as_str(),
                intent.provenance.location.record,
            )
        });
        deadline
            .check(resource)
            .map_err(|error| error.to_string())?;
    }
    Ok(output)
}

fn resolved_role_primitives<'a>(
    review: &'a FabricationReview,
    role: LayerRole,
    layer_polarity: LayerPolarity,
    deadline: ManufacturingDeadline,
    resource: &'static str,
) -> Result<RolePrimitives<'a>, String> {
    let mut layers = BTreeMap::new();
    let mut sides = BTreeMap::new();
    for layer in review.layers.iter().filter(|layer| layer.role == role) {
        deadline
            .check(resource)
            .map_err(|error| error.to_string())?;
        if layers.insert(layer.id.as_str(), layer).is_some()
            || !matches!(layer.side, LayerSide::Top | LayerSide::Bottom)
            || sides.insert(layer.side, layer.id.as_str()).is_some()
            || layer.polarity != layer_polarity
            || matches!(
                layer.authority,
                Authority::FilenameInference | Authority::Unknown
            )
        {
            return Err(format!(
                "{role:?} layer identity, side, authority, or polarity is unresolved"
            ));
        }
    }
    if layers.is_empty() {
        return Err(format!("no actual {role:?} layer is represented"));
    }
    if layers.len() > MAX_DISTANCE_PRIMITIVES {
        return Err(format!(
            "{role:?} layer limit {MAX_DISTANCE_PRIMITIVES} exceeded"
        ));
    }
    let apertures = exact_apertures(review, deadline, resource)?;
    let mut repeated = BTreeSet::new();
    for repeat in &review.repetitions {
        deadline
            .check(resource)
            .map_err(|error| error.to_string())?;
        for feature_id in &repeat.feature_ids {
            deadline
                .check(resource)
                .map_err(|error| error.to_string())?;
            repeated.insert(feature_id.as_str());
        }
    }
    let mut feature_ids = BTreeSet::new();
    for feature in &review.features {
        deadline
            .check(resource)
            .map_err(|error| error.to_string())?;
        if layers.contains_key(feature.layer_id.as_str()) {
            feature_ids.insert(feature.id.as_str());
        }
    }
    if feature_ids.is_empty() || feature_ids.len() > MAX_DISTANCE_PRIMITIVES {
        return Err(format!(
            "actual {role:?} feature count is empty or exceeds {MAX_DISTANCE_PRIMITIVES}"
        ));
    }
    let mut represented_layers = BTreeSet::new();
    let mut primitives = Vec::with_capacity(feature_ids.len());
    for feature in review
        .features
        .iter()
        .filter(|feature| feature_ids.contains(feature.id.as_str()))
    {
        deadline
            .check(resource)
            .map_err(|error| error.to_string())?;
        let layer = layers[feature.layer_id.as_str()];
        if feature.document_id != layer.document_id {
            return Err(format!(
                "{role:?} feature {} is not owned by its physical layer document",
                feature.id
            ));
        }
        represented_layers.insert(feature.layer_id.as_str());
        primitives.push(exact_role_primitive(
            review,
            &apertures,
            &repeated,
            feature,
            &format!("{role:?} opening"),
        )?);
    }
    if represented_layers.len() != layers.len() {
        return Err(format!(
            "{role:?} layer presence without actual resolved opening geometry"
        ));
    }
    deadline
        .check(resource)
        .map_err(|error| error.to_string())?;
    primitives.sort_by_key(|primitive| (primitive.layer_id, primitive.owner_id, primitive.segment));
    deadline
        .check(resource)
        .map_err(|error| error.to_string())?;
    let intents = pad_intents(review, &feature_ids, deadline, resource)?;
    let mut side_by_layer = BTreeMap::new();
    for (id, layer) in layers {
        deadline
            .check(resource)
            .map_err(|error| error.to_string())?;
        side_by_layer.insert(id, layer.side);
    }
    Ok((primitives, intents, side_by_layer))
}

fn pads_by_primitive<'a>(
    primitives: &[LocatedPrimitive<'a>],
    intents: &PadIntentIndex<'a>,
    one_pad_per_primitive: bool,
    deadline: ManufacturingDeadline,
    resource: &'static str,
) -> Result<BTreeMap<(&'a str, &'a str), LocatedPrimitive<'a>>, String> {
    let mut output = BTreeMap::new();
    for primitive in primitives {
        deadline
            .check(resource)
            .map_err(|error| error.to_string())?;
        let represented = intents
            .get(primitive.owner_id)
            .ok_or_else(|| format!("{} intent disappeared", primitive.owner_id))?;
        if one_pad_per_primitive && represented.len() != 1 {
            return Err(format!(
                "{} has merged/windowpane pad intent that this relationship cannot resolve",
                primitive.owner_id
            ));
        }
        for intent in represented {
            if output
                .insert((intent.component, intent.pin), *primitive)
                .is_some()
            {
                return Err(format!(
                    "component {} pad {} maps to multiple physical openings",
                    intent.component, intent.pin
                ));
            }
        }
    }
    Ok(output)
}

fn nearest_same_layer_pair<'a>(
    openings: Vec<LocatedPrimitive<'a>>,
    deadline: ManufacturingDeadline,
    resource: &'static str,
) -> Result<Option<NearestPair<'a>>, String> {
    if openings.len() > MAX_DISTANCE_PRIMITIVES {
        return Err(format!(
            "opening primitive limit {MAX_DISTANCE_PRIMITIVES} exceeded"
        ));
    }
    let mut layers = BTreeMap::<&str, Vec<LocatedPrimitive<'a>>>::new();
    for opening in openings {
        deadline
            .check(resource)
            .map_err(|error| error.to_string())?;
        layers.entry(opening.layer_id).or_default().push(opening);
    }
    let mut best: Option<NearestPair<'a>> = None;
    let mut inexact = Vec::new();
    let mut candidate_checks = 0_usize;
    for values in layers.values_mut() {
        deadline
            .check(resource)
            .map_err(|error| error.to_string())?;
        values.sort_by(|left, right| {
            primitive_bounds(left.primitive)
                .unwrap_or((i128::MIN, 0, i128::MAX, 0))
                .cmp(&primitive_bounds(right.primitive).unwrap_or((i128::MIN, 0, i128::MAX, 0)))
                .then_with(|| left.owner_id.cmp(right.owner_id))
        });
        deadline
            .check(resource)
            .map_err(|error| error.to_string())?;
        for left_index in 0..values.len() {
            let left = values[left_index];
            let left_bounds = primitive_bounds(left.primitive)?;
            for right in values.iter().skip(left_index + 1).copied() {
                deadline
                    .check(resource)
                    .map_err(|error| error.to_string())?;
                let right_bounds = primitive_bounds(right.primitive)?;
                if let Some(current) = best {
                    let limit = i128::from(current.observed.0);
                    if right_bounds.0 > left_bounds.2.saturating_add(limit) {
                        break;
                    }
                    if right_bounds.2 < left_bounds.0.saturating_sub(limit)
                        || right_bounds.1 > left_bounds.3.saturating_add(limit)
                        || right_bounds.3 < left_bounds.1.saturating_sub(limit)
                    {
                        continue;
                    }
                }
                candidate_checks = candidate_checks
                    .checked_add(1)
                    .ok_or("opening candidate count overflow")?;
                if candidate_checks > MAX_DISTANCE_CANDIDATES {
                    return Err(format!(
                        "opening candidate limit {MAX_DISTANCE_CANDIDATES} exceeded"
                    ));
                }
                let distance = axis_distance(left.primitive, right.primitive)?;
                let Some(observed) = exact_axis_measurement(distance)? else {
                    if let Some(current) = best
                        && !inexact_distance_can_beat(distance, current.observed)?
                    {
                        continue;
                    }
                    if inexact.len() >= MAX_INEXACT_DISTANCE_CANDIDATES {
                        return Err(format!(
                            "inexact opening candidate limit {MAX_INEXACT_DISTANCE_CANDIDATES} exceeded"
                        ));
                    }
                    inexact.push(distance);
                    continue;
                };
                let candidate = NearestPair {
                    observed,
                    left,
                    right,
                    candidate_checks,
                };
                let key = (
                    candidate.observed,
                    candidate.left.layer_id,
                    candidate.left.owner_id,
                    candidate.right.owner_id,
                );
                if best.is_none_or(|current| {
                    key < (
                        current.observed,
                        current.left.layer_id,
                        current.left.owner_id,
                        current.right.owner_id,
                    )
                }) {
                    best = Some(candidate);
                }
            }
        }
    }
    deadline
        .check(resource)
        .map_err(|error| error.to_string())?;
    let Some(mut best) = best else {
        return if inexact.is_empty() {
            Ok(None)
        } else {
            Err("no exact represented opening distance is available".into())
        };
    };
    if inexact.into_iter().try_fold(false, |closer, distance| {
        inexact_distance_can_beat(distance, best.observed).map(|value| closer || value)
    })? {
        return Err("an inexact opening distance may be nearer than the exact result".into());
    }
    best.candidate_checks = candidate_checks;
    Ok(Some(best))
}

fn intent_summary(intents: &PadIntentIndex<'_>, owner_id: &str) -> Result<String, String> {
    Ok(intents
        .get(owner_id)
        .ok_or_else(|| format!("{owner_id} intent disappeared"))?
        .iter()
        .map(|intent| {
            format!(
                "{}:{}@{}:{}",
                intent.component,
                intent.pin,
                intent.provenance.document_id,
                intent.provenance.location.record
            )
        })
        .collect::<Vec<_>>()
        .join(","))
}

fn mask_sliver(
    review: &FabricationReview,
    deadline: ManufacturingDeadline,
) -> (Vec<Finding>, Coverage) {
    const LABEL: &str = "Exact remaining negative-mask sliver";
    let result = (|| -> Result<(Vec<Finding>, Coverage), String> {
        dispatch_complete(review, MASK_SLIVER_REQUIREMENTS)?;
        if affected_capability_evidence(review, MASK_SLIVER_REQUIREMENTS.prerequisites) {
            return Err("affected omission or conflict prevents complete mask geometry".into());
        }
        let (threshold, authority_document) =
            source_bound_threshold(review, ConstraintKind::Other, MASK_SLIVER_FAMILY)?;
        let threshold_value = threshold
            .value
            .ok_or("mask-sliver threshold lost its value")?;
        let (openings, intents, _) = resolved_role_primitives(
            review,
            LayerRole::SolderMask,
            LayerPolarity::Negative,
            deadline,
            "dfm-mask-sliver-openings",
        )?;
        let _pads = pads_by_primitive(
            &openings,
            &intents,
            true,
            deadline,
            "dfm-mask-sliver-intent",
        )?;
        for opening in &openings {
            deadline
                .check("dfm-mask-sliver-intent")
                .map_err(|error| error.to_string())?;
            if threshold_value.0 % opening.resolution.0 != 0 {
                return Err("mask-sliver threshold is off a represented source grid".into());
            }
        }
        let Some(nearest) =
            nearest_same_layer_pair(openings.clone(), deadline, "dfm-mask-sliver-distance")?
        else {
            return Ok((
                Vec::new(),
                Coverage {
                    id: MASK_SLIVER_FAMILY.into(),
                    label: LABEL.into(),
                    status: CoverageStatus::Passed,
                    evidence: format!(
                        "openings={} pairs=0 threshold={}pm; every opening has actual negative-layer geometry and one explicit component/pad intent; authority={} {} source={} threshold_record={}",
                        openings.len(),
                        threshold_value.0,
                        threshold.provenance.producer,
                        threshold.provenance.producer_version,
                        authority_document.virtual_path,
                        threshold.provenance.location.record,
                    ),
                },
            ));
        };
        if nearest.observed.0 == 0 {
            return Err(
                "distinct overlapping openings lack represented merged/override intent".into(),
            );
        }
        let resolution = comparison_resolution(nearest.left.resolution, nearest.right.resolution)?;
        if nearest.observed.0 % resolution.0 != 0 || threshold_value.0 % resolution.0 != 0 {
            return Err("mask-sliver measurement is off the compared source grid".into());
        }
        let delta = threshold_value
            .0
            .checked_sub(nearest.observed.0)
            .ok_or("mask-sliver delta overflow")?;
        let measurement = format!(
            "observed={}pm threshold={}pm delta={delta}pm resolution={}pm layer={} left={} left_source={}:{} left_intent={} right={} right_source={}:{} right_intent={} openings={} candidate_checks={} authority={} {} source={} threshold_record={}",
            nearest.observed.0,
            threshold_value.0,
            resolution.0,
            nearest.left.layer_id,
            nearest.left.owner_id,
            nearest.left.provenance.document_id,
            nearest.left.provenance.location.record,
            intent_summary(&intents, nearest.left.owner_id)?,
            nearest.right.owner_id,
            nearest.right.provenance.document_id,
            nearest.right.provenance.location.record,
            intent_summary(&intents, nearest.right.owner_id)?,
            openings.len(),
            nearest.candidate_checks,
            threshold.provenance.producer,
            threshold.provenance.producer_version,
            authority_document.virtual_path,
            threshold.provenance.location.record,
        );
        let findings = if delta > 0 {
            vec![Finding {
                id: format!(
                    "{MASK_SLIVER_FAMILY}/{}/{}/{}",
                    nearest.left.layer_id, nearest.left.owner_id, nearest.right.owner_id
                ),
                severity: Severity::Medium,
                category: "DFM".into(),
                title: "Remaining solder-mask sliver is below the declared minimum".into(),
                evidence: measurement.clone(),
                recommendation:
                    "Separate or deliberately merge the represented mask openings, or revise the source-bound mask-sliver requirement."
                        .into(),
                location: format!(
                    "layer={};left={};right={}",
                    nearest.left.layer_id, nearest.left.owner_id, nearest.right.owner_id
                ),
                source: "fabrication".into(),
                gate_impact: family_gate_impact(MASK_SLIVER_FAMILY),
            }]
        } else {
            Vec::new()
        };
        Ok((
            findings,
            Coverage {
                id: MASK_SLIVER_FAMILY.into(),
                label: LABEL.into(),
                status: if delta > 0 {
                    CoverageStatus::Attention
                } else {
                    CoverageStatus::Passed
                },
                evidence: measurement,
            },
        ))
    })();
    result.unwrap_or_else(|reason| (vec![], not_checked(MASK_SLIVER_FAMILY, LABEL, reason)))
}

fn placement_index(
    review: &FabricationReview,
    deadline: ManufacturingDeadline,
) -> Result<BTreeMap<&str, &crate::fabrication::AssemblyPlacement>, String> {
    if review.assembly.placements.len() > MAX_DISTANCE_PRIMITIVES {
        return Err(format!(
            "assembly placement limit {MAX_DISTANCE_PRIMITIVES} exceeded"
        ));
    }
    let mut placements = BTreeMap::new();
    for placement in &review.assembly.placements {
        deadline
            .check("dfm-paste-mask-placements")
            .map_err(|error| error.to_string())?;
        if placement.reference.is_empty()
            || placement.reference.trim() != placement.reference
            || !matches!(placement.side, LayerSide::Top | LayerSide::Bottom)
            || placements
                .insert(placement.reference.as_str(), placement)
                .is_some()
        {
            return Err("component placement identity or side is missing or ambiguous".into());
        }
    }
    if placements.is_empty() {
        return Err("no explicit component placement side is represented".into());
    }
    Ok(placements)
}

#[derive(Clone, Copy)]
struct FittedEvidence<'a> {
    occurrence: &'a SchematicOccurrence,
    fact: &'a SchematicFact,
}

fn fitted_components<'a>(
    review: &'a SchematicReview,
    deadline: ManufacturingDeadline,
) -> Result<BTreeMap<&'a str, Option<FittedEvidence<'a>>>, String> {
    if review.status != "completed"
        || review.occurrence_count == 0
        || review.occurrence_count != review.occurrences.len()
        || review.occurrences.len() > MAX_DISTANCE_PRIMITIVES
    {
        return Err("typed schematic fitted-state evidence is incomplete".into());
    }
    if review.mismatches.iter().any(|mismatch| {
        matches!(
            mismatch.field.as_str(),
            "board-population" | "bom-population" | "bom-fitted" | "dnp" | "placement-population"
        )
    }) {
        return Err("typed fitted-state reconciliation is conflicted".into());
    }
    let mut output = BTreeMap::<&str, Option<FittedEvidence<'a>>>::new();
    for occurrence in &review.occurrences {
        deadline
            .check("dfm-paste-mask-fitted-state")
            .map_err(|error| error.to_string())?;
        let Some(reference) = occurrence
            .reference
            .as_deref()
            .filter(|value| !value.is_empty() && value.trim() == *value)
        else {
            continue;
        };
        let mut facts = occurrence.facts.iter().filter(|fact| fact.name == "dnp");
        let fact = facts.next().filter(|_| facts.next().is_none());
        let fitted = fact
            .filter(|fact| {
                fact.value == "false"
                    && fact.confidence == "high"
                    && matches!(
                        fact.evidence_class.as_str(),
                        "explicit-source-fact" | "explicit-export-facts"
                    )
                    && !fact.source_path.is_empty()
                    && !fact.producer.is_empty()
            })
            .map(|fact| FittedEvidence { occurrence, fact });
        output
            .entry(reference)
            .and_modify(|current| match (*current, fitted) {
                (Some(existing), Some(candidate)) => {
                    if candidate.occurrence.key < existing.occurrence.key {
                        *current = Some(candidate);
                    }
                }
                _ => *current = None,
            })
            .or_insert(fitted);
    }
    Ok(output)
}

fn smd_pad_apertures(
    review: &FabricationReview,
    deadline: ManufacturingDeadline,
) -> Result<BTreeSet<&str>, String> {
    let mut output = BTreeSet::new();
    for attribute in &review.x2_attributes {
        deadline
            .check("dfm-paste-mask-smd-authority")
            .map_err(|error| error.to_string())?;
        if attribute.scope != X2AttributeScope::Aperture
            || attribute.kind != X2AttributeKind::ApertureFunction
            || attribute.deletion
            || attribute.values.len() != 2
            || attribute.values[0] != "SMDPad"
            || !matches!(attribute.values[1].as_str(), "CuDef" | "SMDef")
        {
            continue;
        }
        for target in &attribute.target_ids {
            deadline
                .check("dfm-paste-mask-smd-authority")
                .map_err(|error| error.to_string())?;
            if !output.insert(target.as_str()) {
                return Err("SMD pad aperture authority is duplicated".into());
            }
        }
    }
    if output.is_empty() {
        return Err("positive non-through-hole SMD pad authority is missing".into());
    }
    Ok(output)
}

fn require_smd_pad_authority(
    review: &FabricationReview,
    primitives: &[LocatedPrimitive<'_>],
    deadline: ManufacturingDeadline,
) -> Result<(), String> {
    let smd_apertures = smd_pad_apertures(review, deadline)?;
    let mut features = BTreeMap::new();
    for feature in &review.features {
        deadline
            .check("dfm-paste-mask-smd-authority")
            .map_err(|error| error.to_string())?;
        if features.insert(feature.id.as_str(), feature).is_some() {
            return Err("feature identity is duplicated".into());
        }
    }
    for primitive in primitives {
        deadline
            .check("dfm-paste-mask-smd-authority")
            .map_err(|error| error.to_string())?;
        let feature = features
            .get(primitive.owner_id)
            .copied()
            .ok_or("SMD pad feature identity is dangling")?;
        let Geometry::Flash(flash) = &feature.geometry else {
            return Err(format!(
                "feature {} lacks positive round SMD pad authority",
                feature.id
            ));
        };
        if !smd_apertures.contains(flash.aperture_id.as_str()) {
            return Err(format!(
                "feature {} aperture lacks positive SMD pad authority",
                feature.id
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct PasteRequiredPad<'a> {
    feature_id: &'a str,
    side: LayerSide,
    provenance: &'a ManufacturingProvenance,
    intent_provenance: &'a ManufacturingProvenance,
}

fn paste_requiring_pads<'a>(
    review: &'a FabricationReview,
    deadline: ManufacturingDeadline,
) -> Result<BTreeMap<(&'a str, &'a str), PasteRequiredPad<'a>>, String> {
    let smd_apertures = smd_pad_apertures(review, deadline)?;
    let apertures = exact_apertures(review, deadline, "assembly-paste-copper-authority")?;
    let mut layers = BTreeMap::new();
    for layer in review
        .layers
        .iter()
        .filter(|layer| layer.role == LayerRole::Copper)
    {
        deadline
            .check("assembly-paste-copper-authority")
            .map_err(|error| error.to_string())?;
        if !matches!(layer.side, LayerSide::Top | LayerSide::Bottom) {
            continue;
        }
        if matches!(
            layer.authority,
            Authority::FilenameInference | Authority::Unknown
        ) || layers.insert(layer.id.as_str(), layer).is_some()
        {
            return Err("authoritative copper-pad layer identity or side is unresolved".into());
        }
    }
    if layers.is_empty() {
        return Err("authoritative copper-pad layers are absent".into());
    }
    let mut repeated = BTreeSet::new();
    for repetition in &review.repetitions {
        for feature_id in &repetition.feature_ids {
            deadline
                .check("assembly-paste-copper-authority")
                .map_err(|error| error.to_string())?;
            repeated.insert(feature_id.as_str());
        }
    }
    let mut feature_ids = BTreeSet::new();
    let mut pad_features = BTreeMap::new();
    for feature in &review.features {
        deadline
            .check("assembly-paste-copper-authority")
            .map_err(|error| error.to_string())?;
        let Some(layer) = layers.get(feature.layer_id.as_str()).copied() else {
            continue;
        };
        let Geometry::Flash(flash) = &feature.geometry else {
            continue;
        };
        if !smd_apertures.contains(flash.aperture_id.as_str()) {
            continue;
        }
        let aperture = apertures
            .get(flash.aperture_id.as_str())
            .copied()
            .filter(|aperture| aperture.document_id == feature.document_id)
            .ok_or("copper SMD-pad aperture identity is dangling")?;
        if feature.document_id != layer.document_id
            || feature.polarity != LayerPolarity::Dark
            || !feature.transforms.operations.is_empty()
            || feature.membership != FeatureMembership::TopLevel
            || repeated.contains(feature.id.as_str())
            || aperture.shape == ApertureShape::Unknown
        {
            return Err(format!(
                "copper SMD-pad feature {} has unresolved geometry authority",
                feature.id
            ));
        }
        feature_ids.insert(feature.id.as_str());
        pad_features.insert(feature.id.as_str(), (feature, layer.side));
    }
    if feature_ids.is_empty() || feature_ids.len() > MAX_DISTANCE_PRIMITIVES {
        return Err(
            "authoritative copper SMD-pad set is absent or exceeds its bounded size".into(),
        );
    }
    let intents = pad_intents(
        review,
        &feature_ids,
        deadline,
        "assembly-paste-copper-authority",
    )?;
    let mut required = BTreeMap::new();
    for feature_id in feature_ids {
        deadline
            .check("assembly-paste-copper-authority")
            .map_err(|error| error.to_string())?;
        let [intent] = intents[feature_id].as_slice() else {
            return Err(format!(
                "copper SMD-pad feature {feature_id} lacks one exact component/pad identity"
            ));
        };
        let (feature, side) = pad_features[feature_id];
        if required
            .insert(
                (intent.component, intent.pin),
                PasteRequiredPad {
                    feature_id,
                    side,
                    provenance: &feature.provenance,
                    intent_provenance: intent.provenance,
                },
            )
            .is_some()
        {
            return Err(format!(
                "component {} pad {} has duplicate copper SMD-pad authority",
                intent.component, intent.pin
            ));
        }
    }
    Ok(required)
}

fn plated_pad_keys(
    review: &FabricationReview,
    deadline: ManufacturingDeadline,
) -> Result<BTreeSet<(&str, &str)>, String> {
    if review.pad_hole_associations.len() > MAX_DISTANCE_PRIMITIVES {
        return Err(format!(
            "pad-hole association limit {MAX_DISTANCE_PRIMITIVES} exceeded"
        ));
    }
    let mut holes = BTreeSet::new();
    for association in &review.pad_hole_associations {
        deadline
            .check("dfm-paste-mask-pin-in-paste")
            .map_err(|error| error.to_string())?;
        holes.insert(association.hole_id.as_str());
    }
    let mut keys = BTreeSet::new();
    let mut seen_holes = BTreeSet::new();
    for semantic in &review.connectivity {
        deadline
            .check("dfm-paste-mask-pin-in-paste")
            .map_err(|error| error.to_string())?;
        if !holes.contains(semantic.feature_id.as_str()) {
            continue;
        }
        let component = semantic
            .component
            .as_deref()
            .filter(|value| !value.is_empty() && value.trim() == *value)
            .ok_or("pad-hole association lacks component identity")?;
        let pin = semantic
            .pin
            .as_deref()
            .filter(|value| !value.is_empty() && value.trim() == *value)
            .ok_or("pad-hole association lacks pin identity")?;
        if !seen_holes.insert(semantic.feature_id.as_str()) || !keys.insert((component, pin)) {
            return Err("pad-hole component/pin authority is duplicated".into());
        }
    }
    if seen_holes.len() != holes.len() {
        return Err("pad-hole association intent is incomplete".into());
    }
    Ok(keys)
}

fn side_name(side: LayerSide) -> &'static str {
    match side {
        LayerSide::Top => "top",
        LayerSide::Bottom => "bottom",
        _ => "unknown",
    }
}

fn paste_mask_relationship(
    review: &FabricationReview,
    schematic: Option<&SchematicReview>,
    deadline: ManufacturingDeadline,
) -> (Vec<Finding>, Coverage) {
    const LABEL: &str = "Exact fitted-pad paste/mask relationship";
    let result = (|| -> Result<(Vec<Finding>, Coverage), String> {
        dispatch_complete(review, PASTE_MASK_REQUIREMENTS)?;
        if affected_capability_evidence(review, PASTE_MASK_REQUIREMENTS.prerequisites) {
            return Err(
                "affected omission or conflict prevents complete paste/mask geometry".into(),
            );
        }
        let (threshold, authority_document) =
            source_bound_threshold(review, ConstraintKind::Other, PASTE_MASK_FAMILY)?;
        let threshold_value = threshold
            .value
            .ok_or("paste/mask relationship threshold lost its value")?;
        let (mask, mask_intents, mask_sides) = resolved_role_primitives(
            review,
            LayerRole::SolderMask,
            LayerPolarity::Negative,
            deadline,
            "dfm-paste-mask-mask",
        )?;
        let (paste, paste_intents, paste_sides) = resolved_role_primitives(
            review,
            LayerRole::Paste,
            LayerPolarity::Positive,
            deadline,
            "dfm-paste-mask-paste",
        )?;
        require_smd_pad_authority(review, &mask, deadline)?;
        require_smd_pad_authority(review, &paste, deadline)?;
        let mask_by_pad = pads_by_primitive(
            &mask,
            &mask_intents,
            true,
            deadline,
            "dfm-paste-mask-pad-index",
        )?;
        let paste_by_pad = pads_by_primitive(
            &paste,
            &paste_intents,
            true,
            deadline,
            "dfm-paste-mask-pad-index",
        )?;
        if mask_by_pad.len() != paste_by_pad.len() {
            return Err(
                "paste omission, mask override, or component/pad association is unresolved".into(),
            );
        }
        for key in mask_by_pad.keys() {
            deadline
                .check("dfm-paste-mask-pad-index")
                .map_err(|error| error.to_string())?;
            if !paste_by_pad.contains_key(key) {
                return Err(
                    "paste omission, mask override, or component/pad association is unresolved"
                        .into(),
                );
            }
        }
        let placements = placement_index(review, deadline)?;
        let fitted = fitted_components(
            schematic.ok_or("typed schematic fitted-state evidence is missing")?,
            deadline,
        )?;
        let plated = plated_pad_keys(review, deadline)?;
        let mut findings = Vec::new();
        let mut details = Vec::new();
        let mut maximum: Option<(Picometres, &str, &str)> = None;
        let mut comparisons = 0_usize;
        for (key @ (component, pin), mask_opening) in &mask_by_pad {
            deadline
                .check("dfm-paste-mask-relationship")
                .map_err(|error| error.to_string())?;
            comparisons = comparisons
                .checked_add(1)
                .ok_or("paste/mask comparison count overflow")?;
            if comparisons > MAX_DISTANCE_CANDIDATES {
                return Err(format!(
                    "paste/mask comparison limit {MAX_DISTANCE_CANDIDATES} exceeded"
                ));
            }
            if plated.contains(key) {
                return Err(format!(
                    "pin-in-paste intent for component {component} pad {pin} is unresolved"
                ));
            }
            let paste_opening = paste_by_pad[key];
            let placement = placements
                .get(component)
                .copied()
                .ok_or_else(|| format!("component {component} placement side is unknown"))?;
            let fitted = fitted
                .get(component)
                .copied()
                .flatten()
                .ok_or_else(|| format!("component {component} fitted/DNP state is unknown"))?;
            let mask_side = mask_sides[mask_opening.layer_id];
            let paste_side = paste_sides[paste_opening.layer_id];
            if mask_side != paste_side || placement.side != mask_side {
                return Err(format!(
                    "component {component} pad {pin} applicable side is unknown or inconsistent"
                ));
            }
            if mask_opening.primitive.start != mask_opening.primitive.end
                || paste_opening.primitive.start != paste_opening.primitive.end
                || mask_opening.primitive.start != paste_opening.primitive.start
            {
                return Err(format!(
                    "component {component} pad {pin} set relation is outside the exact concentric-round subset"
                ));
            }
            let resolution =
                comparison_resolution(mask_opening.resolution, paste_opening.resolution)?;
            let signed = paste_opening
                .primitive
                .radius
                .0
                .checked_sub(mask_opening.primitive.radius.0)
                .ok_or("paste/mask radial subtraction overflow")?;
            let observed = Picometres(
                signed
                    .checked_abs()
                    .ok_or("paste/mask absolute radial delta overflow")?,
            );
            if [
                mask_opening.primitive.start.x.0,
                mask_opening.primitive.start.y.0,
                mask_opening.primitive.radius.0,
                paste_opening.primitive.radius.0,
                observed.0,
                threshold_value.0,
            ]
            .into_iter()
            .any(|value| value % resolution.0 != 0)
            {
                return Err(format!(
                    "component {component} pad {pin} relationship is off the compared source grid"
                ));
            }
            let relation = match signed.cmp(&0) {
                std::cmp::Ordering::Less => "paste_subset_of_mask",
                std::cmp::Ordering::Equal => "equal",
                std::cmp::Ordering::Greater => "mask_subset_of_paste",
            };
            let expansion = signed.max(0);
            let reduction = if signed < 0 {
                signed
                    .checked_neg()
                    .ok_or("paste/mask radial reduction overflow")?
            } else {
                0
            };
            let delta = observed
                .0
                .checked_sub(threshold_value.0)
                .ok_or("paste/mask threshold delta overflow")?;
            let mask_intent = mask_intents[mask_opening.owner_id]
                .iter()
                .find(|intent| intent.component == *component && intent.pin == *pin)
                .ok_or("mask pad intent disappeared")?;
            let paste_intent = paste_intents[paste_opening.owner_id]
                .first()
                .ok_or("paste pad intent disappeared")?;
            let measurement = format!(
                "component={component} pad={pin} side={} relation={relation} observed={}pm threshold={}pm delta={delta}pm expansion={expansion}pm reduction={reduction}pm resolution={}pm mask={} mask_layer={} mask_source={}:{} mask_intent_source={}:{} paste={} paste_layer={} paste_source={}:{} paste_intent_source={}:{} placement_source={}:{} fitted_source={} fitted_occurrence={} fitted_producer={} authority={} {} source={} threshold_record={}",
                side_name(mask_side),
                observed.0,
                threshold_value.0,
                resolution.0,
                mask_opening.owner_id,
                mask_opening.layer_id,
                mask_opening.provenance.document_id,
                mask_opening.provenance.location.record,
                mask_intent.provenance.document_id,
                mask_intent.provenance.location.record,
                paste_opening.owner_id,
                paste_opening.layer_id,
                paste_opening.provenance.document_id,
                paste_opening.provenance.location.record,
                paste_intent.provenance.document_id,
                paste_intent.provenance.location.record,
                placement.provenance.document_id,
                placement.provenance.location.record,
                fitted.fact.source_path,
                fitted.occurrence.key,
                fitted.fact.producer,
                threshold.provenance.producer,
                threshold.provenance.producer_version,
                authority_document.virtual_path,
                threshold.provenance.location.record,
            );
            details.push(measurement.clone());
            let candidate = (observed, *component, *pin);
            if maximum.is_none_or(|current| candidate > current) {
                maximum = Some(candidate);
            }
            if delta > 0 {
                findings.push(Finding {
                    id: format!(
                        "{PASTE_MASK_FAMILY}/{}/{}",
                        mask_opening.owner_id, paste_opening.owner_id
                    ),
                    severity: Severity::Medium,
                    category: "DFM".into(),
                    title: "Paste/mask radial relationship exceeds the declared limit".into(),
                    evidence: measurement,
                    recommendation:
                        "Align the exact fitted-pad paste and mask openings or revise the source-bound relationship limit."
                            .into(),
                    location: format!(
                        "component={component};pad={pin};side={};mask={};paste={}",
                        side_name(mask_side),
                        mask_opening.owner_id,
                        paste_opening.owner_id
                    ),
                    source: "fabrication".into(),
                    gate_impact: family_gate_impact(PASTE_MASK_FAMILY),
                });
            }
        }
        deadline
            .check("dfm-paste-mask-complete")
            .map_err(|error| error.to_string())?;
        deadline
            .check("dfm-paste-mask-complete")
            .map_err(|error| error.to_string())?;
        findings.sort_by(|left, right| left.id.cmp(&right.id));
        details.sort();
        deadline
            .check("dfm-paste-mask-complete")
            .map_err(|error| error.to_string())?;
        let has_findings = !findings.is_empty();
        let maximum = maximum.ok_or("no fitted component/pad relationship is available")?;
        let evidence = format!(
            "maximum_observed={}pm maximum_component={} maximum_pad={} threshold={}pm comparisons={comparisons}; {}",
            maximum.0.0,
            maximum.1,
            maximum.2,
            threshold_value.0,
            details.join(" | "),
        );
        Ok((
            findings,
            Coverage {
                id: PASTE_MASK_FAMILY.into(),
                label: LABEL.into(),
                status: if has_findings {
                    CoverageStatus::Attention
                } else {
                    CoverageStatus::Passed
                },
                evidence,
            },
        ))
    })();
    result.unwrap_or_else(|reason| (vec![], not_checked(PASTE_MASK_FAMILY, LABEL, reason)))
}

fn side_rotation(
    review: &FabricationReview,
    deadline: ManufacturingDeadline,
) -> (Vec<Finding>, Coverage) {
    const LABEL: &str = "Exact declared side and rotation parity";
    let result = (|| -> Result<(Vec<Finding>, Coverage), String> {
        dispatch_complete(review, SIDE_ROTATION_REQUIREMENTS)?;
        if affected_capability_evidence(review, SIDE_ROTATION_REQUIREMENTS.prerequisites) {
            return Err(
                "affected omission or conflict prevents complete placement evidence".into(),
            );
        }
        let native = placement_index(review, deadline)?;
        if review.assembly.declared_placements.is_empty()
            || review.assembly.declared_placements.len() > MAX_DISTANCE_PRIMITIVES
        {
            return Err("declared placement rows or explicit conventions are absent".into());
        }
        let mut declared = BTreeMap::new();
        for placement in &review.assembly.declared_placements {
            deadline
                .check("assembly-side-rotation-declarations")
                .map_err(|error| error.to_string())?;
            if declared
                .insert(placement.reference.as_str(), placement)
                .is_some()
            {
                return Err("declared placement identity is duplicated".into());
            }
        }
        for placement in declared.values() {
            let native = native
                .get(placement.reference.as_str())
                .copied()
                .ok_or("declared placement has no exact native occurrence")?;
            if native.fitted != placement.fitted {
                return Err("native and declared fitted-state authority conflicts".into());
            }
        }
        let mut findings = Vec::new();
        let mut details = Vec::new();
        let mut compared = 0_usize;
        for (reference, native) in &native {
            deadline
                .check("assembly-side-rotation-comparison")
                .map_err(|error| error.to_string())?;
            if native.fitted == AssemblyFittedState::NotFitted {
                continue;
            }
            if native.fitted != AssemblyFittedState::Fitted {
                return Err(format!("component {reference} fitted state is unknown"));
            }
            let declared = declared
                .get(reference)
                .copied()
                .ok_or_else(|| format!("fitted component {reference} has no declared placement"))?;
            let native_revision = native
                .revision
                .as_deref()
                .ok_or_else(|| format!("component {reference} revision is unknown"))?;
            if native_revision != declared.revision
                || native.convention != declared.convention
                || native.convention != AssemblyPlacementConvention::native_kicad()
            {
                return Err(format!(
                    "component {reference} revision or placement convention is not explicitly equivalent"
                ));
            }
            compared = compared
                .checked_add(1)
                .ok_or("side/rotation comparison count overflow")?;
            let native_rotation = native.rotation_microdegrees.rem_euclid(360_000_000);
            let declared_rotation = declared.rotation_microdegrees.rem_euclid(360_000_000);
            let evidence = format!(
                "reference={reference} native_side={} declared_side={} native_rotation={}udeg declared_rotation={}udeg convention=mm/kicad_board/top_bottom/mirrored/counter_clockwise revision={} native_source={}:{}@{} declared_source={}@{}:{} adapter={} {}",
                side_name(native.side),
                side_name(declared.side),
                native_rotation,
                declared_rotation,
                native_revision,
                native.provenance.document_id,
                native.provenance.location.record,
                native.provenance.artifact_digest,
                declared.source_path,
                declared.artifact_digest,
                declared.line,
                PLACEMENT_DECLARATION_ADAPTER,
                PLACEMENT_DECLARATION_VERSION,
            );
            details.push(evidence.clone());
            if native.side != declared.side || native_rotation != declared_rotation {
                findings.push(Finding {
                    id: format!("{SIDE_ROTATION_FAMILY}/{reference}"),
                    severity: Severity::Medium,
                    category: "Assembly".into(),
                    title: "Declared placement side or rotation differs from native KiCad".into(),
                    evidence,
                    recommendation: "Regenerate the source-declared placement rows from the matching native board revision and explicit convention.".into(),
                    location: format!("reference={reference};native={};declared={}:{}", native.provenance.location.record, declared.source_path, declared.line),
                    source: "assembly-correlation".into(),
                    gate_impact: family_gate_impact(SIDE_ROTATION_FAMILY),
                });
            }
        }
        if compared == 0 {
            return Err("no fitted placement has an exact declared counterpart".into());
        }
        findings.sort_by(|left, right| left.id.cmp(&right.id));
        details.sort();
        let has_findings = !findings.is_empty();
        Ok((
            findings,
            Coverage {
                id: SIDE_ROTATION_FAMILY.into(),
                label: LABEL.into(),
                status: if has_findings {
                    CoverageStatus::Attention
                } else {
                    CoverageStatus::Passed
                },
                evidence: format!(
                    "compared={compared}; exact declared conventions and modulo-360 microdegree equivalence only; {}",
                    details.join(" | ")
                ),
            },
        ))
    })();
    result.unwrap_or_else(|reason| (vec![], not_checked(SIDE_ROTATION_FAMILY, LABEL, reason)))
}

fn assembly_paste_availability(
    review: &FabricationReview,
    deadline: ManufacturingDeadline,
) -> (Vec<Finding>, Coverage) {
    const LABEL: &str = "Actual fitted-pad paste availability";
    let result = (|| -> Result<(Vec<Finding>, Coverage), String> {
        dispatch_complete(review, ASSEMBLY_PASTE_REQUIREMENTS)?;
        if affected_capability_evidence(review, ASSEMBLY_PASTE_REQUIREMENTS.prerequisites) {
            return Err("affected omission or conflict prevents complete paste evidence".into());
        }
        let placements = placement_index(review, deadline)?;
        let required = paste_requiring_pads(review, deadline)?;
        let (paste, paste_intents, paste_sides) = resolved_role_primitives(
            review,
            LayerRole::Paste,
            LayerPolarity::Unknown,
            deadline,
            "assembly-paste-geometry",
        )?;
        require_smd_pad_authority(review, &paste, deadline)?;
        let paste_by_pad = pads_by_primitive(
            &paste,
            &paste_intents,
            true,
            deadline,
            "assembly-paste-pad-identity",
        )?;
        let mut findings = Vec::new();
        let mut details = Vec::new();
        let mut fitted_pads = 0_usize;
        let mut dnp_pads = 0_usize;
        for (key @ (component, pin), required_pad) in &required {
            deadline
                .check("assembly-paste-comparison")
                .map_err(|error| error.to_string())?;
            let placement = placements
                .get(component)
                .copied()
                .ok_or_else(|| format!("component {component} placement is absent"))?;
            match placement.fitted {
                AssemblyFittedState::NotFitted => {
                    dnp_pads += 1;
                    continue;
                }
                AssemblyFittedState::Unknown => {
                    return Err(format!("component {component} fitted state is unknown"));
                }
                AssemblyFittedState::Fitted => fitted_pads += 1,
            }
            let paste_opening = paste_by_pad.get(key).copied().ok_or_else(|| {
                format!("fitted component {component} pad {pin} lacks one exact paste geometry")
            })?;
            let paste_side = paste_sides[paste_opening.layer_id];
            let evidence = format!(
                "component={component} pad={pin} fitted=fitted placement_side={} required_side={} paste_side={} required_pad={} required_source={}:{} required_intent_source={}:{} paste={} paste_source={}:{} placement_source={}:{}",
                side_name(placement.side),
                side_name(required_pad.side),
                side_name(paste_side),
                required_pad.feature_id,
                required_pad.provenance.document_id,
                required_pad.provenance.location.record,
                required_pad.intent_provenance.document_id,
                required_pad.intent_provenance.location.record,
                paste_opening.owner_id,
                paste_opening.provenance.document_id,
                paste_opening.provenance.location.record,
                placement.provenance.document_id,
                placement.provenance.location.record,
            );
            details.push(evidence.clone());
            if required_pad.side != paste_side || placement.side != paste_side {
                findings.push(Finding {
                    id: format!(
                        "{ASSEMBLY_PASTE_FAMILY}/{}/{}",
                        paste_opening.owner_id, placement.id
                    ),
                    severity: Severity::Medium,
                    category: "Assembly".into(),
                    title: "Actual paste geometry is not on the fitted placement side".into(),
                    evidence,
                    recommendation: "Regenerate exact per-pad paste geometry for the fitted component side, or retain an explicit deliberate-omission review.".into(),
                    location: format!("component={component};pad={pin};paste={}", paste_opening.owner_id),
                    source: "fabrication".into(),
                    gate_impact: family_gate_impact(ASSEMBLY_PASTE_FAMILY),
                });
            }
        }
        findings.sort_by(|left, right| left.id.cmp(&right.id));
        details.sort();
        let has_findings = !findings.is_empty();
        Ok((
            findings,
            Coverage {
                id: ASSEMBLY_PASTE_FAMILY.into(),
                label: LABEL.into(),
                status: if has_findings {
                    CoverageStatus::Attention
                } else {
                    CoverageStatus::Passed
                },
                evidence: format!(
                    "required_pads={} fitted_pads={fitted_pads} dnp_pads={dnp_pads} actual_paste_openings={}; independent copper SMD-pad authority plus exact fitted-state, side, identity, and paste geometry only; {}",
                    required.len(),
                    paste_by_pad.len(),
                    details.join(" | ")
                ),
            },
        ))
    })();
    result.unwrap_or_else(|reason| (vec![], not_checked(ASSEMBLY_PASTE_FAMILY, LABEL, reason)))
}

fn courtyard_native(review: &FabricationReview) -> (Vec<Finding>, Coverage) {
    const LABEL: &str = "Completed native KiCad courtyard observations";
    let result = (|| -> Result<(Vec<Finding>, Coverage), String> {
        dispatch_complete(review, COURTYARD_NATIVE_REQUIREMENTS)?;
        if affected_capability_evidence(review, COURTYARD_NATIVE_REQUIREMENTS.prerequisites) {
            return Err("affected native omission or conflict prevents courtyard evidence".into());
        }
        let courtyard = review
            .assembly
            .native_courtyard
            .as_ref()
            .ok_or("native courtyard execution evidence is absent")?;
        if courtyard.state != NativeCourtyardRunState::Complete {
            return Err(format!(
                "native courtyard execution state is {:?}",
                courtyard.state
            ));
        }
        let version = courtyard
            .version
            .as_deref()
            .ok_or("native courtyard tool version is absent")?;
        let source = courtyard
            .source
            .as_deref()
            .ok_or("native courtyard source is absent")?;
        let mut active = 0_usize;
        let mut excluded = 0_usize;
        let mut unknown = 0_usize;
        let mut findings = Vec::new();
        for observation in &courtyard.observations {
            let kind = match observation.kind {
                NativeCourtyardKind::Overlap => "overlap",
                NativeCourtyardKind::Malformed => "malformed",
                NativeCourtyardKind::Missing => "missing",
            };
            match observation.exclusion {
                NativeExclusionState::Active => {
                    active += 1;
                    findings.push(Finding {
                        id: format!("{COURTYARD_NATIVE_FAMILY}/{}", observation.id),
                        severity: Severity::Medium,
                        category: "Assembly".into(),
                        title: format!("Native KiCad courtyard {kind} observation is active"),
                        evidence: format!(
                            "kind={kind} exclusion=active tool={} version={version} source={source} location={}",
                            courtyard.tool, observation.location
                        ),
                        recommendation: "Resolve the matching native KiCad courtyard marker and rerun the completed DRC channel.".into(),
                        location: observation.location.clone(),
                        source: "kicad-cli".into(),
                        gate_impact: family_gate_impact(COURTYARD_NATIVE_FAMILY),
                    });
                }
                NativeExclusionState::Excluded => excluded += 1,
                NativeExclusionState::Unknown => unknown += 1,
            }
        }
        findings.sort_by(|left, right| left.id.cmp(&right.id));
        Ok((
            findings,
            Coverage {
                id: COURTYARD_NATIVE_FAMILY.into(),
                label: LABEL.into(),
                status: if unknown > 0 {
                    CoverageStatus::Unknown
                } else if active > 0 {
                    CoverageStatus::Attention
                } else {
                    CoverageStatus::Passed
                },
                evidence: format!(
                    "tool={} version={version} source={source}; active={active} excluded={excluded} unknown_exclusion={unknown}; normalized completed native results only",
                    courtyard.tool
                ),
            },
        ))
    })();
    result.unwrap_or_else(|reason| (vec![], not_checked(COURTYARD_NATIVE_FAMILY, LABEL, reason)))
}

const FOOTPRINT_FIELDS: &[&str] = &["footprint", "netlist-footprint", "bom-footprint"];

fn footprint_string_parity(
    fabrication: &FabricationReview,
    review: &SchematicReview,
    deadline: ManufacturingDeadline,
) -> (Vec<Finding>, Coverage) {
    const LABEL: &str = "Typed footprint source-string parity";
    let result = (|| -> Result<(Vec<Finding>, Coverage), String> {
        dispatch_complete(fabrication, FOOTPRINT_STRING_REQUIREMENTS)?;
        if affected_capability_evidence(fabrication, FOOTPRINT_STRING_REQUIREMENTS.prerequisites) {
            return Err("affected component omission or conflict prevents footprint parity".into());
        }
        if review.status != "completed"
            || review.occurrence_count == 0
            || review.occurrence_count != review.occurrences.len()
            || review.occurrences.len() > MAX_DISTANCE_PRIMITIVES
        {
            return Err("typed schematic reconciliation is incomplete".into());
        }
        let mut capabilities = review
            .capabilities
            .iter()
            .filter(|capability| capability.id == "schematic-reconciliation");
        let capability = capabilities
            .next()
            .ok_or("schematic reconciliation capability is absent")?;
        if capabilities.next().is_some()
            || capability.status
                != if review.mismatches.is_empty() {
                    "completed"
                } else {
                    "attention"
                }
            || capability.producer != "ratemypcb"
            || capability.evidence_class != "deterministic-cross-artifact"
            || capability.detail.trim().is_empty()
        {
            return Err("schematic reconciliation capability is ambiguous".into());
        }
        let pair = review
            .source_pair
            .as_ref()
            .ok_or("coherent schematic/board source identity is absent")?;
        if review.project_identity.as_deref() != Some(pair.project_identity.as_str())
            || review.root_path.as_deref() != Some(pair.schematic_path.as_str())
            || review.root_digest.as_deref() != Some(pair.schematic_digest.as_str())
            || review.board_path.as_deref() != Some(pair.board_path.as_str())
            || review.board_digest.as_deref() != Some(pair.board_digest.as_str())
            || review
                .artifact_digests
                .get(&pair.schematic_path)
                .is_none_or(|digest| digest != &pair.schematic_digest)
            || review
                .artifact_digests
                .get(&pair.board_path)
                .is_none_or(|digest| digest != &pair.board_digest)
        {
            return Err("schematic/board source identity is inconsistent".into());
        }
        let composite = review
            .artifact_digests
            .get("schematic:composite")
            .filter(|digest| crate::lowercase_sha256(digest))
            .ok_or("schematic composite digest is absent")?;
        let mut occurrences = BTreeMap::new();
        let mut occurrences_by_key = BTreeMap::new();
        let mut reference_counts = BTreeMap::new();
        let mut represented_footprints = 0_usize;
        for occurrence in &review.occurrences {
            deadline
                .check("assembly-footprint-occurrences")
                .map_err(|error| error.to_string())?;
            if review
                .artifact_digests
                .get(&occurrence.source_path)
                .is_none_or(|digest| !crate::lowercase_sha256(digest))
                || occurrences
                    .insert(
                        (
                            occurrence.sheet_uuid_path.as_str(),
                            occurrence.item_uuid.as_str(),
                            occurrence.source_path.as_str(),
                        ),
                        occurrence,
                    )
                    .is_some()
                || occurrences_by_key
                    .insert(occurrence.key.as_str(), occurrence)
                    .is_some()
            {
                return Err("schematic occurrence provenance is incomplete or duplicated".into());
            }
            if let Some(reference) = occurrence.reference.as_deref() {
                *reference_counts
                    .entry(reference.to_ascii_uppercase())
                    .or_insert(0_usize) += 1;
            }
            represented_footprints += occurrence
                .facts
                .iter()
                .filter(|fact| {
                    fact.name == "footprint"
                        && !fact.value.is_empty()
                        && !fact.source_path.is_empty()
                        && !fact.producer.is_empty()
                })
                .count();
        }
        if represented_footprints == 0 {
            return Err("footprint occurrence identity is absent".into());
        }
        let mut target_footprints = BTreeMap::new();
        for occurrence in &review.occurrences {
            deadline
                .check("assembly-footprint-targets")
                .map_err(|error| error.to_string())?;
            let unique_fact = |name: &str| {
                let mut facts = occurrence.facts.iter().filter(|fact| fact.name == name);
                facts.next().filter(|_| facts.next().is_none())
            };
            if unique_fact("on_board").map(|fact| fact.value.as_str()) == Some("true")
                && unique_fact("in_bom").map(|fact| fact.value.as_str()) == Some("true")
            {
                let footprint = unique_fact("footprint")
                    .filter(|fact| {
                        !fact.value.is_empty()
                            && !fact.source_path.is_empty()
                            && !fact.producer.is_empty()
                            && fact.confidence == "high"
                            && review
                                .artifact_digests
                                .get(&fact.source_path)
                                .is_some_and(|digest| crate::lowercase_sha256(digest))
                    })
                    .ok_or("assembly occurrence footprint field authority is absent")?;
                if occurrence
                    .reference
                    .as_deref()
                    .is_none_or(|reference| reference.is_empty() || reference.trim() != reference)
                    || target_footprints
                        .insert(occurrence.key.as_str(), footprint)
                        .is_some()
                {
                    return Err(
                        "assembly footprint target identity is missing or duplicated".into(),
                    );
                }
            }
        }
        if target_footprints.is_empty() {
            return Err("no board-and-BOM assembly footprint target is represented".into());
        }
        represented_footprints = target_footprints.len();
        let mut comparisons = BTreeMap::new();
        let mut comparison_sources = BTreeMap::new();
        let mut unmatched_comparisons = 0_usize;
        for comparison in &review.footprint_comparisons {
            deadline
                .check("assembly-footprint-comparisons")
                .map_err(|error| error.to_string())?;
            let Some(footprint) = target_footprints
                .get(comparison.occurrence_key.as_str())
                .copied()
            else {
                continue;
            };
            let occurrence = occurrences_by_key
                .get(comparison.occurrence_key.as_str())
                .copied()
                .ok_or("typed footprint comparison occurrence disappeared")?;
            let join_valid = match comparison.source {
                SchematicComparisonSource::Board | SchematicComparisonSource::Netlist => matches!(
                    comparison.join.as_str(),
                    "occurrence-uuid" | "reference-fallback"
                ),
                SchematicComparisonSource::Bom => comparison.join == "reference-fallback",
            };
            let expected_confidence = if comparison.join == "reference-fallback" {
                "low"
            } else {
                "high"
            };
            if comparison.field != "footprint"
                || comparison.expected != footprint.value
                || comparison.expected_source_path != footprint.source_path
                || review
                    .artifact_digests
                    .get(&comparison.expected_source_path)
                    != Some(&comparison.expected_source_digest)
                || review.artifact_digests.get(&comparison.actual_source_path)
                    != Some(&comparison.actual_source_digest)
                || comparison.actual.is_empty()
                || !join_valid
                || comparison.confidence != expected_confidence
                || comparison.location
                    != format!(
                        "sheet={};item={};source={}",
                        occurrence.sheet_uuid_path, occurrence.item_uuid, occurrence.source_path
                    )
                || (comparison.source == SchematicComparisonSource::Board
                    && (comparison.actual_source_path != pair.board_path
                        || comparison.actual_source_digest != pair.board_digest))
                || (comparison.join == "reference-fallback"
                    && occurrence.reference.as_deref().is_none_or(|reference| {
                        reference_counts.get(&reference.to_ascii_uppercase()) != Some(&1)
                    }))
            {
                return Err(
                    "typed footprint comparison source, field, or join is incomplete".into(),
                );
            }
            if let Some(previous) = comparison_sources.insert(
                comparison.source,
                (
                    comparison.actual_source_path.as_str(),
                    comparison.actual_source_digest.as_str(),
                ),
            ) && previous
                != (
                    comparison.actual_source_path.as_str(),
                    comparison.actual_source_digest.as_str(),
                )
            {
                return Err("typed footprint comparison source identity is inconsistent".into());
            }
            if comparisons
                .insert(
                    (comparison.occurrence_key.as_str(), comparison.source),
                    comparison,
                )
                .is_some()
            {
                return Err("typed footprint comparison identity is duplicated".into());
            }
            unmatched_comparisons += usize::from(!comparison.matched);
        }
        let required_comparisons = target_footprints
            .len()
            .checked_mul(3)
            .ok_or("footprint comparison count overflow")?;
        if comparisons.len() != required_comparisons
            || [
                SchematicComparisonSource::Board,
                SchematicComparisonSource::Bom,
                SchematicComparisonSource::Netlist,
            ]
            .into_iter()
            .any(|source| !comparison_sources.contains_key(&source))
            || comparison_sources
                .values()
                .map(|(path, _)| *path)
                .collect::<BTreeSet<_>>()
                .len()
                != 3
        {
            return Err(
                "board, BOM, and netlist footprint comparison completeness is absent".into(),
            );
        }
        for mismatch in review
            .mismatches
            .iter()
            .filter(|mismatch| mismatch.field == "board-population")
        {
            let ambiguous = location_identity(&mismatch.location)
                .and_then(|identity| occurrences.get(&identity).copied())
                .and_then(|occurrence| occurrence.reference.as_deref())
                .is_some_and(|reference| {
                    reference_counts.get(&reference.to_ascii_uppercase()) != Some(&1)
                });
            if ambiguous {
                return Err("typed reconciliation retained an ambiguous footprint fallback".into());
            }
        }
        let mut identities = BTreeSet::new();
        let mut findings = Vec::new();
        for mismatch in review
            .mismatches
            .iter()
            .filter(|mismatch| FOOTPRINT_FIELDS.contains(&mismatch.field.as_str()))
        {
            deadline
                .check("assembly-footprint-mismatches")
                .map_err(|error| error.to_string())?;
            let expected_check_id =
                format!("schematic-reconcile-{}", mismatch.field.replace('_', "-"));
            let expected_confidence = if mismatch.join == "reference-fallback" {
                "low"
            } else {
                "high"
            };
            let occurrence = location_identity(&mismatch.location)
                .and_then(|identity| occurrences.get(&identity).copied())
                .ok_or("typed footprint mismatch has no source occurrence")?;
            let source = match mismatch.field.as_str() {
                "footprint" => SchematicComparisonSource::Board,
                "bom-footprint" => SchematicComparisonSource::Bom,
                "netlist-footprint" => SchematicComparisonSource::Netlist,
                _ => return Err("typed footprint mismatch source is unsupported".into()),
            };
            let comparison = comparisons
                .get(&(occurrence.key.as_str(), source))
                .copied()
                .ok_or("typed footprint mismatch has no complete source comparison")?;
            if mismatch.check_id != expected_check_id
                || mismatch.expected.trim().is_empty()
                || mismatch.actual.trim().is_empty()
                || mismatch.confidence != expected_confidence
                || mismatch.gate_impact != GateImpact::EvidenceOnly
                || comparison.matched
                || comparison.expected != mismatch.expected
                || comparison.actual != mismatch.actual
                || comparison.join != mismatch.join
                || comparison.confidence != mismatch.confidence
                || comparison.location != mismatch.location
                || !identities.insert((mismatch.check_id.as_str(), mismatch.location.as_str()))
                || (mismatch.join == "reference-fallback"
                    && occurrence.reference.as_deref().is_none_or(|reference| {
                        reference_counts.get(&reference.to_ascii_uppercase()) != Some(&1)
                    }))
            {
                return Err("typed footprint mismatch identity or provenance is invalid".into());
            }
            let expected_digest = &comparison.expected_source_digest;
            let actual_source = &comparison.actual_source_path;
            let actual_digest = &comparison.actual_source_digest;
            findings.push(Finding {
                id: format!("{FOOTPRINT_STRING_FAMILY}/{}", mismatch.field),
                severity: Severity::Medium,
                category: "Assembly".into(),
                title: format!("Footprint source string differs for {}", mismatch.field),
                evidence: format!(
                    "typed_field={} expected={} actual={} join={} confidence={} expected_source={}@{} actual_source={}@{}; existing typed exact/full-string and board library-suffix semantics only",
                    mismatch.field,
                    mismatch.expected,
                    mismatch.actual,
                    mismatch.join,
                    mismatch.confidence,
                    comparison.expected_source_path,
                    expected_digest,
                    actual_source,
                    actual_digest,
                ),
                recommendation: "Regenerate the typed schematic, board, BOM, and netlist footprint strings from one revision; assess physical package suitability separately.".into(),
                location: mismatch.location.clone(),
                source: "schematic-reconciliation".into(),
                gate_impact: family_gate_impact(FOOTPRINT_STRING_FAMILY),
            });
        }
        findings.sort_by(|left, right| {
            left.id
                .cmp(&right.id)
                .then_with(|| left.location.cmp(&right.location))
        });
        if findings.len() != unmatched_comparisons {
            return Err("typed footprint mismatch set is incomplete".into());
        }
        let count = findings.len();
        Ok((
            findings,
            Coverage {
                id: FOOTPRINT_STRING_FAMILY.into(),
                label: LABEL.into(),
                status: if count == 0 {
                    CoverageStatus::Passed
                } else {
                    CoverageStatus::Attention
                },
                evidence: format!(
                    "typed_footprint_mismatches={count} represented_occurrence_footprints={represented_footprints} typed_source_comparisons={} board_source={}@{} bom_source={}@{} netlist_source={}@{} schematic_composite={composite}; upstream comparison results and joins only; no comparison, fallback, packaging, name-similarity, or physical-package inference was rerun",
                    comparisons.len(),
                    comparison_sources[&SchematicComparisonSource::Board].0,
                    comparison_sources[&SchematicComparisonSource::Board].1,
                    comparison_sources[&SchematicComparisonSource::Bom].0,
                    comparison_sources[&SchematicComparisonSource::Bom].1,
                    comparison_sources[&SchematicComparisonSource::Netlist].0,
                    comparison_sources[&SchematicComparisonSource::Netlist].1,
                ),
            },
        ))
    })();
    result.unwrap_or_else(|reason| (vec![], not_checked(FOOTPRINT_STRING_FAMILY, LABEL, reason)))
}

#[derive(Clone, Copy)]
struct ComponentPrimitive<'a> {
    located: LocatedPrimitive<'a>,
    component: &'a str,
    pin: Option<&'a str>,
    net: Option<&'a str>,
    semantic_provenance: &'a ManufacturingProvenance,
}

struct CompleteAssemblyGeometry<'a> {
    components: BTreeMap<&'a str, Vec<ComponentPrimitive<'a>>>,
    placements: BTreeMap<&'a str, &'a crate::fabrication::AssemblyPlacement>,
    profile: Vec<LocatedPrimitive<'a>>,
}

fn complete_assembly_geometry<'a>(
    review: &'a FabricationReview,
    require_connectivity: bool,
    deadline: ManufacturingDeadline,
) -> Result<CompleteAssemblyGeometry<'a>, String> {
    let apertures = exact_apertures(review, deadline, "assembly-inference-geometry")?;
    let mut repeated = BTreeSet::new();
    for repetition in &review.repetitions {
        for feature_id in &repetition.feature_ids {
            deadline
                .check("assembly-inference-geometry")
                .map_err(|error| error.to_string())?;
            repeated.insert(feature_id.as_str());
        }
    }
    let mut features = BTreeMap::new();
    for feature in &review.features {
        deadline
            .check("assembly-inference-geometry")
            .map_err(|error| error.to_string())?;
        if features.insert(feature.id.as_str(), feature).is_some() {
            return Err("component geometry feature identity is duplicated".into());
        }
    }
    let mut layers = BTreeMap::new();
    for layer in review
        .layers
        .iter()
        .filter(|layer| layer.role == LayerRole::Copper)
    {
        deadline
            .check("assembly-inference-geometry")
            .map_err(|error| error.to_string())?;
        if !matches!(layer.side, LayerSide::Top | LayerSide::Bottom)
            || matches!(
                layer.authority,
                Authority::FilenameInference | Authority::Unknown
            )
            || layers.insert(layer.id.as_str(), layer).is_some()
        {
            return Err("component copper layer identity, side, or authority is unresolved".into());
        }
    }
    if layers.is_empty() {
        return Err("component copper geometry layers are absent".into());
    }

    let mut semantics = BTreeMap::new();
    for semantic in &review.connectivity {
        deadline
            .check("assembly-inference-geometry")
            .map_err(|error| error.to_string())?;
        let Some(component) = semantic
            .component
            .as_deref()
            .filter(|value| !value.is_empty() && value.trim() == *value)
        else {
            continue;
        };
        let feature = features
            .get(semantic.feature_id.as_str())
            .copied()
            .ok_or("component semantic feature identity is dangling")?;
        if !layers.contains_key(feature.layer_id.as_str()) {
            continue;
        }
        if semantics
            .insert(semantic.feature_id.as_str(), (semantic, component))
            .is_some()
        {
            return Err("component geometry ownership is duplicated".into());
        }
    }
    if semantics.is_empty() || semantics.len() > MAX_DISTANCE_PRIMITIVES {
        return Err("component geometry ownership is absent or unbounded".into());
    }

    let mut components = BTreeMap::<&str, Vec<ComponentPrimitive<'a>>>::new();
    for (feature_id, (semantic, component)) in semantics {
        deadline
            .check("assembly-inference-geometry")
            .map_err(|error| error.to_string())?;
        let feature = features
            .get(feature_id)
            .copied()
            .ok_or("component geometry feature identity is dangling")?;
        let layer = layers
            .get(feature.layer_id.as_str())
            .copied()
            .ok_or("component geometry is not on one explicit copper side")?;
        if feature.document_id != layer.document_id {
            return Err("component geometry and layer source identity differ".into());
        }
        let pin = semantic
            .pin
            .as_deref()
            .filter(|value| !value.is_empty() && value.trim() == *value);
        let net = semantic
            .net
            .as_deref()
            .filter(|value| !value.is_empty() && value.trim() == *value);
        if require_connectivity && (pin.is_none() || net.is_none()) {
            return Err("component pin or connectivity identity is incomplete".into());
        }
        source_link(review, &semantic.provenance)?;
        let located = exact_role_primitive(
            review,
            &apertures,
            &repeated,
            feature,
            "component copper geometry",
        )?;
        components
            .entry(component)
            .or_default()
            .push(ComponentPrimitive {
                located,
                component,
                pin,
                net,
                semantic_provenance: &semantic.provenance,
            });
    }

    let all_placements = placement_index(review, deadline)?;
    let profile = review
        .profile
        .as_ref()
        .ok_or("canonical profile is missing")?;
    let extents = profile
        .extents
        .as_ref()
        .filter(|extent| extent.min.x < extent.max.x && extent.min.y < extent.max.y)
        .ok_or("canonical profile extents are incomplete")?;
    let profile_primitives = profile_boundary_primitives(review, deadline)?;
    if profile_primitives.is_empty() {
        return Err("canonical profile geometry is empty".into());
    }
    for provenance in &profile.provenance {
        source_link(review, provenance)?;
    }

    let mut placements = BTreeMap::new();
    let mut fitted_components = BTreeMap::new();
    for (reference, placement) in all_placements {
        deadline
            .check("assembly-inference-placement")
            .map_err(|error| error.to_string())?;
        if placement.convention != AssemblyPlacementConvention::native_kicad()
            || !matches!(placement.side, LayerSide::Top | LayerSide::Bottom)
            || !matches!(
                placement.fitted,
                AssemblyFittedState::Fitted | AssemblyFittedState::NotFitted
            )
            || placement.position.x < extents.min.x
            || placement.position.x > extents.max.x
            || placement.position.y < extents.min.y
            || placement.position.y > extents.max.y
        {
            return Err(format!(
                "component {reference} placement convention, side, fitted state, or profile location is incomplete"
            ));
        }
        source_link(review, &placement.provenance)?;
        placements.insert(reference, placement);
        if placement.fitted == AssemblyFittedState::Fitted {
            let values = components
                .get_mut(reference)
                .filter(|values| !values.is_empty())
                .ok_or_else(|| {
                    format!("fitted component {reference} lacks complete component geometry")
                })?;
            values.sort_by_key(|primitive| primitive.located.owner_id);
            if values
                .iter()
                .any(|primitive| layers[primitive.located.layer_id].side != placement.side)
            {
                return Err(format!(
                    "component {reference} geometry and placement side differ"
                ));
            }
            fitted_components.insert(reference, values.clone());
        }
    }
    for component in components.keys() {
        if !placements.contains_key(component) {
            return Err(format!(
                "component geometry {component} lacks one exact placement"
            ));
        }
    }
    if fitted_components.is_empty() || fitted_components.len() > MAX_INFERENCE_COMPONENTS {
        return Err("fitted component geometry set is empty or unbounded".into());
    }
    let primitive_count = fitted_components
        .values()
        .try_fold(0_usize, |count, values| count.checked_add(values.len()))
        .ok_or("component primitive count overflow")?;
    if primitive_count == 0 || primitive_count > MAX_DISTANCE_PRIMITIVES {
        return Err("fitted component geometry is empty or unbounded".into());
    }
    Ok(CompleteAssemblyGeometry {
        components: fitted_components,
        placements,
        profile: profile_primitives,
    })
}

fn bounded_inference_product(left: usize, right: usize, label: &str) -> Result<(), String> {
    if left
        .checked_mul(right)
        .is_none_or(|value| value > MAX_DISTANCE_CANDIDATES)
    {
        return Err(format!("{label} comparison count is unbounded"));
    }
    Ok(())
}

fn inference_comparison(
    pair: NearestPair<'_>,
    required: Picometres,
) -> Result<(Picometres, i64), String> {
    let resolution = comparison_resolution(pair.left.resolution, pair.right.resolution)?;
    if pair.observed.0 % resolution.0 != 0 || required.0 % resolution.0 != 0 {
        return Err("inference envelope or geometry is off the compared source grid".into());
    }
    let margin = pair
        .observed
        .0
        .checked_sub(required.0)
        .ok_or("inference comparison margin overflow")?;
    Ok((resolution, margin))
}

fn inference_assumptions(
    review: &FabricationReview,
    authority: &InferenceAuthority<'_>,
) -> Result<String, String> {
    source_link(review, &authority.constraint.provenance)?;
    Ok(format!(
        "model={}@{} applicability={} declaration_source={} declaration_digest={} declaration_record={} declaration_producer={} {}",
        authority.record.model,
        authority.record.model_version,
        authority.record.applicability,
        authority.document.virtual_path,
        authority.document.artifact_digest,
        authority.constraint.provenance.location.record,
        authority.constraint.provenance.producer,
        authority.constraint.provenance.producer_version,
    ))
}

fn assembly_access(
    review: &FabricationReview,
    deadline: ManufacturingDeadline,
) -> (Vec<Finding>, Coverage) {
    const LABEL: &str = "Declared-envelope assembly access inference";
    let result = (|| -> Result<(Vec<Finding>, Coverage), String> {
        dispatch_complete(review, ASSEMBLY_ACCESS_REQUIREMENTS)?;
        if affected_capability_evidence(review, ASSEMBLY_ACCESS_REQUIREMENTS.prerequisites) {
            return Err("affected assembly, profile, component, or geometry evidence".into());
        }
        let authority = source_bound_inference_record(review, "assembly_process_envelope")?;
        let tool_diameter = authority.distance("tool_diameter")?;
        let tool_radius = exact_half(tool_diameter, "assembly tool diameter")?;
        let component_clearance = authority.distance("minimum_component_clearance")?;
        let profile_clearance = authority.distance("minimum_profile_clearance")?;
        let component_required = Picometres(
            tool_radius
                .0
                .checked_add(component_clearance.0)
                .ok_or("assembly component envelope overflow")?,
        );
        let profile_required = Picometres(
            tool_radius
                .0
                .checked_add(profile_clearance.0)
                .ok_or("assembly profile envelope overflow")?,
        );
        let process = authority.parameter("process")?;
        let process_version = authority.parameter("process_version")?;
        let tool = authority.parameter("tool")?;
        let tool_version = authority.parameter("tool_version")?;
        let assumptions = inference_assumptions(review, &authority)?;
        let geometry = complete_assembly_geometry(review, false, deadline)?;
        let primitive_count = geometry.components.values().map(Vec::len).sum::<usize>();
        bounded_inference_product(primitive_count, primitive_count, "component access")?;
        bounded_inference_product(
            primitive_count,
            geometry.profile.len(),
            "component/profile access",
        )?;

        let mut findings = Vec::new();
        let mut observations = Vec::new();
        let components = geometry.components.keys().copied().collect::<Vec<_>>();
        for (left_index, left_component) in components.iter().enumerate() {
            for right_component in components.iter().skip(left_index + 1) {
                deadline
                    .check("assembly-access-components")
                    .map_err(|error| error.to_string())?;
                let left = geometry.components[left_component]
                    .iter()
                    .map(|primitive| primitive.located)
                    .collect::<Vec<_>>();
                let right = geometry.components[right_component]
                    .iter()
                    .map(|primitive| primitive.located)
                    .collect::<Vec<_>>();
                let nearest =
                    nearest_axis_pair(left, right, deadline, "assembly-access-components")?;
                let (resolution, margin) = inference_comparison(nearest, component_required)?;
                let left_placement = geometry.placements[left_component];
                let right_placement = geometry.placements[right_component];
                let detail = format!(
                    "kind=component component_left={} placement_left={} component_right={} placement_right={} observed={}pm required={}pm margin={}pm resolution={}pm tool_radius={}pm minimum_component_clearance={}pm geometry_source={}:{} geometry_peer_source={}:{}",
                    left_component,
                    left_placement.id,
                    right_component,
                    right_placement.id,
                    nearest.observed.0,
                    component_required.0,
                    margin,
                    resolution.0,
                    tool_radius.0,
                    component_clearance.0,
                    nearest.left.provenance.document_id,
                    nearest.left.provenance.location.record,
                    nearest.right.provenance.document_id,
                    nearest.right.provenance.location.record,
                );
                observations.push(detail.clone());
                if margin < 0 {
                    findings.push(Finding {
                        id: format!(
                            "{ASSEMBLY_ACCESS_FAMILY}/component/{}/{}",
                            left_placement.id, right_placement.id
                        ),
                        severity: Severity::Medium,
                        category: "Assembly inference".into(),
                        title: "Declared assembly tool envelope is obstructed by component geometry".into(),
                        evidence: format!(
                            "inference=true process={process}@{process_version} tool={tool}@{tool_version} {assumptions}; {detail} declaration_source={}",
                            authority.document.virtual_path,
                        ),
                        recommendation: "Review the exact component spacing against the named assembly process and tool envelope; do not treat this inference as process simulation.".into(),
                        location: format!(
                            "left={};right={}",
                            left_placement.id, right_placement.id
                        ),
                        source: "bounded-assembly-inference".into(),
                        gate_impact: family_gate_impact(ASSEMBLY_ACCESS_FAMILY),
                    });
                }
            }
        }
        for (component, values) in &geometry.components {
            deadline
                .check("assembly-access-profile")
                .map_err(|error| error.to_string())?;
            let nearest = nearest_axis_pair(
                values.iter().map(|primitive| primitive.located).collect(),
                geometry.profile.clone(),
                deadline,
                "assembly-access-profile",
            )?;
            let (resolution, margin) = inference_comparison(nearest, profile_required)?;
            let placement = geometry.placements[component];
            let detail = format!(
                "kind=profile component={} placement={} observed={}pm required={}pm margin={}pm resolution={}pm tool_radius={}pm minimum_profile_clearance={}pm boundary={} boundary_segment={} geometry_source={}:{} profile_source={}:{}",
                component,
                placement.id,
                nearest.observed.0,
                profile_required.0,
                margin,
                resolution.0,
                tool_radius.0,
                profile_clearance.0,
                nearest.right.owner_id,
                nearest.right.segment,
                nearest.left.provenance.document_id,
                nearest.left.provenance.location.record,
                nearest.right.provenance.document_id,
                nearest.right.provenance.location.record,
            );
            observations.push(detail.clone());
            if margin < 0 {
                findings.push(Finding {
                    id: format!("{ASSEMBLY_ACCESS_FAMILY}/profile/{}", placement.id),
                    severity: Severity::Medium,
                    category: "Assembly inference".into(),
                    title: "Declared assembly tool envelope reaches the board profile".into(),
                    evidence: format!(
                        "inference=true process={process}@{process_version} tool={tool}@{tool_version} {assumptions}; {detail} declaration_source={}",
                        authority.document.virtual_path,
                    ),
                    recommendation: "Review the exact component-to-profile spacing against the named assembly process and tool envelope; do not treat this inference as process simulation.".into(),
                    location: format!(
                        "placement={};profile={}:{}",
                        placement.id, nearest.right.owner_id, nearest.right.segment
                    ),
                    source: "bounded-assembly-inference".into(),
                    gate_impact: family_gate_impact(ASSEMBLY_ACCESS_FAMILY),
                });
            }
        }
        findings.sort_by(|left, right| left.id.cmp(&right.id));
        observations.sort();
        let has_findings = !findings.is_empty();
        Ok((
            findings,
            Coverage {
                id: ASSEMBLY_ACCESS_FAMILY.into(),
                label: LABEL.into(),
                status: if has_findings {
                    CoverageStatus::Attention
                } else {
                    CoverageStatus::Passed
                },
                evidence: format!(
                    "inference=true model={}@{} process={process}@{process_version} tool={tool}@{tool_version} tool_diameter={}pm assumptions=complete_placement+profile+component_copper_union_2d observations={} {assumptions}; {}",
                    authority.record.model,
                    authority.record.model_version,
                    tool_diameter.0,
                    observations.len(),
                    observations.join(" | "),
                ),
            },
        ))
    })();
    result.unwrap_or_else(|reason| (vec![], not_checked(ASSEMBLY_ACCESS_FAMILY, LABEL, reason)))
}

fn canonical_net_id(mut feature_ids: Vec<&str>) -> Result<String, String> {
    let original_len = feature_ids.len();
    feature_ids.sort();
    feature_ids.dedup();
    if feature_ids.is_empty() || feature_ids.len() != original_len {
        return Err("canonical net feature identity is empty or duplicated".into());
    }
    let bytes = serde_json::to_vec(&("dfm-canonical-net-v1", feature_ids))
        .map_err(|error| error.to_string())?;
    Ok(format!("net-v1-{}", crate::sha256(&bytes)))
}

fn testpoint_access(
    review: &FabricationReview,
    deadline: ManufacturingDeadline,
) -> (Vec<Finding>, Coverage) {
    const LABEL: &str = "Explicit-target probe access inference";
    let result = (|| -> Result<(Vec<Finding>, Coverage), String> {
        dispatch_complete(review, TESTPOINT_ACCESS_REQUIREMENTS)?;
        if affected_capability_evidence(review, TESTPOINT_ACCESS_REQUIREMENTS.prerequisites) {
            return Err(
                "affected connectivity, component, pin, placement, profile, or geometry evidence"
                    .into(),
            );
        }
        let probe = source_bound_inference_record(review, "probe_envelope")?;
        let targets = source_bound_inference_record(review, "target_net_authority")?;
        let probe_diameter = probe.distance("probe_diameter")?;
        let probe_radius = exact_half(probe_diameter, "testpoint probe diameter")?;
        let component_clearance = probe.distance("minimum_component_clearance")?;
        let profile_clearance = probe.distance("minimum_profile_clearance")?;
        let component_required = Picometres(
            probe_radius
                .0
                .checked_add(component_clearance.0)
                .ok_or("testpoint component envelope overflow")?,
        );
        let profile_required = Picometres(
            probe_radius
                .0
                .checked_add(profile_clearance.0)
                .ok_or("testpoint profile envelope overflow")?,
        );
        let process = probe.parameter("process")?;
        let process_version = probe.parameter("process_version")?;
        let probe_name = probe.parameter("probe")?;
        let probe_version = probe.parameter("probe_version")?;
        let probe_assumptions = inference_assumptions(review, &probe)?;
        let target_assumptions = inference_assumptions(review, &targets)?;
        let geometry = complete_assembly_geometry(review, true, deadline)?;
        let all_primitives = geometry
            .components
            .values()
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        bounded_inference_product(
            all_primitives.len(),
            all_primitives
                .len()
                .checked_add(geometry.profile.len())
                .ok_or("testpoint geometry count overflow")?,
            "testpoint access",
        )?;

        let feature_ids = review
            .features
            .iter()
            .map(|feature| feature.id.as_str())
            .collect::<BTreeSet<_>>();
        let mut canonical_features = BTreeMap::<&str, Vec<&str>>::new();
        let mut connected_features = BTreeSet::new();
        for semantic in &review.connectivity {
            deadline
                .check("assembly-testpoint-connectivity")
                .map_err(|error| error.to_string())?;
            let net = semantic
                .net
                .as_deref()
                .filter(|net| !net.is_empty() && net.trim() == *net)
                .ok_or("complete connectivity contains an unnamed net")?;
            if !feature_ids.contains(semantic.feature_id.as_str())
                || !connected_features.insert(semantic.feature_id.as_str())
            {
                return Err(
                    "canonical connectivity feature identity is dangling or duplicated".into(),
                );
            }
            source_link(review, &semantic.provenance)?;
            canonical_features
                .entry(net)
                .or_default()
                .push(semantic.feature_id.as_str());
        }
        let mut canonical_ids = BTreeMap::new();
        for (net, features) in canonical_features {
            let id = canonical_net_id(features)?;
            if canonical_ids.insert(net, id).is_some() {
                return Err("canonical connectivity net identity is duplicated".into());
            }
        }
        if canonical_ids.is_empty() {
            return Err("canonical connectivity net set is empty".into());
        }
        let mut nets = BTreeMap::<String, Vec<ComponentPrimitive<'_>>>::new();
        for primitive in &all_primitives {
            let net = primitive
                .net
                .ok_or("component connectivity is incomplete")?;
            primitive
                .pin
                .ok_or("component pin identity is incomplete")?;
            let id = canonical_ids
                .get(net)
                .ok_or("component net is absent from canonical connectivity")?;
            nets.entry(id.clone()).or_default().push(*primitive);
        }
        for primitives in nets.values_mut() {
            primitives.sort_by_key(|primitive| primitive.located.owner_id);
        }
        if targets.record.target_ids.is_empty() {
            return Err("explicit canonical target-net authority is empty".into());
        }

        let mut findings = Vec::new();
        let mut observations = Vec::new();
        for target_id in &targets.record.target_ids {
            deadline
                .check("assembly-testpoint-targets")
                .map_err(|error| error.to_string())?;
            let candidates = nets
                .get(target_id)
                .filter(|values| !values.is_empty())
                .ok_or_else(|| format!("canonical target-net ID {target_id} is dangling"))?;
            let mut best: Option<(i64, &str, String)> = None;
            for candidate in candidates {
                deadline
                    .check("assembly-testpoint-candidates")
                    .map_err(|error| error.to_string())?;
                let profile_pair = nearest_axis_pair(
                    vec![candidate.located],
                    geometry.profile.clone(),
                    deadline,
                    "assembly-testpoint-profile",
                )?;
                let (profile_resolution, profile_margin) =
                    inference_comparison(profile_pair, profile_required)?;
                let blockers = all_primitives
                    .iter()
                    .filter(|other| other.component != candidate.component)
                    .map(|other| other.located)
                    .collect::<Vec<_>>();
                let (component_observed, component_resolution, component_margin, blocker) =
                    if blockers.is_empty() {
                        (None, None, i64::MAX, None)
                    } else {
                        let pair = nearest_axis_pair(
                            vec![candidate.located],
                            blockers,
                            deadline,
                            "assembly-testpoint-components",
                        )?;
                        let (resolution, margin) = inference_comparison(pair, component_required)?;
                        (
                            Some(pair.observed),
                            Some(resolution),
                            margin,
                            Some(pair.right),
                        )
                    };
                let margin = profile_margin.min(component_margin);
                let pin = candidate.pin.ok_or("candidate pin identity disappeared")?;
                let connectivity = source_link(review, candidate.semantic_provenance)?;
                let detail = format!(
                    "target_net_id={target_id} candidate={} component={} pin={} profile_observed={}pm profile_required={}pm profile_margin={}pm profile_resolution={}pm component_observed={} component_required={}pm component_margin={} component_resolution={} blocker={} probe_radius={}pm connectivity_source={} geometry_source={}:{} profile_source={}:{}",
                    candidate.located.owner_id,
                    candidate.component,
                    pin,
                    profile_pair.observed.0,
                    profile_required.0,
                    profile_margin,
                    profile_resolution.0,
                    component_observed
                        .map(|value| format!("{}pm", value.0))
                        .unwrap_or_else(|| "not_applicable".into()),
                    component_required.0,
                    if component_margin == i64::MAX {
                        "not_applicable".into()
                    } else {
                        format!("{component_margin}pm")
                    },
                    component_resolution
                        .map(|value| format!("{}pm", value.0))
                        .unwrap_or_else(|| "not_applicable".into()),
                    blocker
                        .map(|value| value.owner_id.to_owned())
                        .unwrap_or_else(|| "none".into()),
                    probe_radius.0,
                    connectivity,
                    candidate.located.provenance.document_id,
                    candidate.located.provenance.location.record,
                    profile_pair.right.provenance.document_id,
                    profile_pair.right.provenance.location.record,
                );
                if best.as_ref().is_none_or(|current| {
                    margin > current.0
                        || (margin == current.0 && candidate.located.owner_id < current.1)
                }) {
                    best = Some((margin, candidate.located.owner_id, detail));
                }
            }
            let (margin, feature_id, detail) =
                best.ok_or("declared target net has no fitted probe candidate")?;
            observations.push(detail.clone());
            if margin < 0 {
                findings.push(Finding {
                    id: format!("{TESTPOINT_ACCESS_FAMILY}/{target_id}/{feature_id}"),
                    severity: Severity::Medium,
                    category: "Assembly inference".into(),
                    title: "No explicit target pad meets the declared probe access envelope".into(),
                    evidence: format!(
                        "inference=true probe={probe_name}@{probe_version} process={process}@{process_version} {probe_assumptions}; target_authority_source={} target_authority_model={}@{} target_authority_record={} {target_assumptions}; {detail}",
                        targets.document.virtual_path,
                        targets.record.model,
                        targets.record.model_version,
                        targets.constraint.provenance.location.record,
                    ),
                    recommendation: "Review the exact declared target pad against the named probe and process envelope; do not infer test intent from references or net names.".into(),
                    location: format!("target_net_id={target_id};feature={feature_id}"),
                    source: "bounded-assembly-inference".into(),
                    gate_impact: family_gate_impact(TESTPOINT_ACCESS_FAMILY),
                });
            }
        }
        findings.sort_by(|left, right| left.id.cmp(&right.id));
        observations.sort();
        let has_findings = !findings.is_empty();
        Ok((
            findings,
            Coverage {
                id: TESTPOINT_ACCESS_FAMILY.into(),
                label: LABEL.into(),
                status: if has_findings {
                    CoverageStatus::Attention
                } else {
                    CoverageStatus::Passed
                },
                evidence: format!(
                    "inference=true model={}@{} probe={probe_name}@{probe_version} process={process}@{process_version} probe_diameter={}pm explicit_target_net_ids={} assumptions=complete_connectivity+component+pin+placement+profile+component_copper_union_2d observations={} {probe_assumptions}; {target_assumptions}; {}",
                    probe.record.model,
                    probe.record.model_version,
                    probe_diameter.0,
                    targets.record.target_ids.join(","),
                    observations.len(),
                    observations.join(" | "),
                ),
            },
        ))
    })();
    result.unwrap_or_else(|reason| (vec![], not_checked(TESTPOINT_ACCESS_FAMILY, LABEL, reason)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum OutlineClassification {
    Exterior,
    Cutout,
}

impl OutlineClassification {
    fn name(self) -> &'static str {
        match self {
            Self::Exterior => "exterior",
            Self::Cutout => "cutout",
        }
    }
}

fn segment_start(segment: &ContourSegment) -> CanonicalPoint {
    match segment {
        ContourSegment::Line(line) => line.start,
        ContourSegment::Arc(arc) => arc.start,
    }
}

fn segment_end(segment: &ContourSegment) -> CanonicalPoint {
    match segment {
        ContourSegment::Line(line) => line.end,
        ContourSegment::Arc(arc) => arc.end,
    }
}

fn outline_cross(a: CanonicalPoint, b: CanonicalPoint, c: CanonicalPoint) -> Result<i128, String> {
    let ab_x = i128::from(b.x.0)
        .checked_sub(i128::from(a.x.0))
        .ok_or("outline coordinate subtraction overflow")?;
    let ab_y = i128::from(b.y.0)
        .checked_sub(i128::from(a.y.0))
        .ok_or("outline coordinate subtraction overflow")?;
    let ac_x = i128::from(c.x.0)
        .checked_sub(i128::from(a.x.0))
        .ok_or("outline coordinate subtraction overflow")?;
    let ac_y = i128::from(c.y.0)
        .checked_sub(i128::from(a.y.0))
        .ok_or("outline coordinate subtraction overflow")?;
    ab_x.checked_mul(ac_y)
        .and_then(|left| {
            ab_y.checked_mul(ac_x)
                .and_then(|right| left.checked_sub(right))
        })
        .ok_or_else(|| "outline cross-product overflow".into())
}

fn point_on_outline_line(point: CanonicalPoint, line: &CanonicalLine) -> Result<bool, String> {
    Ok(outline_cross(line.start, line.end, point)? == 0
        && point.x.0 >= line.start.x.0.min(line.end.x.0)
        && point.x.0 <= line.start.x.0.max(line.end.x.0)
        && point.y.0 >= line.start.y.0.min(line.end.y.0)
        && point.y.0 <= line.start.y.0.max(line.end.y.0))
}

fn outline_lines_intersect(left: &CanonicalLine, right: &CanonicalLine) -> Result<bool, String> {
    let crosses = [
        outline_cross(left.start, left.end, right.start)?,
        outline_cross(left.start, left.end, right.end)?,
        outline_cross(right.start, right.end, left.start)?,
        outline_cross(right.start, right.end, left.end)?,
    ];
    Ok(
        (((crosses[0] > 0 && crosses[1] < 0) || (crosses[0] < 0 && crosses[1] > 0))
            && ((crosses[2] > 0 && crosses[3] < 0) || (crosses[2] < 0 && crosses[3] > 0)))
            || (crosses[0] == 0 && point_on_outline_line(right.start, left)?)
            || (crosses[1] == 0 && point_on_outline_line(right.end, left)?)
            || (crosses[2] == 0 && point_on_outline_line(left.start, right)?)
            || (crosses[3] == 0 && point_on_outline_line(left.end, right)?),
    )
}

fn outline_lines_overlap(left: &CanonicalLine, right: &CanonicalLine) -> Result<bool, String> {
    if outline_cross(left.start, left.end, right.start)? != 0
        || outline_cross(left.start, left.end, right.end)? != 0
    {
        return Ok(false);
    }
    let (left_min, left_max, right_min, right_max) =
        if left.start.x != left.end.x || right.start.x != right.end.x {
            (
                left.start.x.0.min(left.end.x.0),
                left.start.x.0.max(left.end.x.0),
                right.start.x.0.min(right.end.x.0),
                right.start.x.0.max(right.end.x.0),
            )
        } else {
            (
                left.start.y.0.min(left.end.y.0),
                left.start.y.0.max(left.end.y.0),
                right.start.y.0.min(right.end.y.0),
                right.start.y.0.max(right.end.y.0),
            )
        };
    Ok(left_min.max(right_min) < left_max.min(right_max))
}

fn exact_integer_sqrt(value: u128) -> Option<i64> {
    if value == 0 {
        return Some(0);
    }
    let mut current = value;
    let mut next = current.div_ceil(2);
    while next < current {
        current = next;
        next = (current + value / current) / 2;
    }
    (current.checked_mul(current) == Some(value))
        .then(|| i64::try_from(current).ok())
        .flatten()
}

fn arc_radius(arc: &CanonicalArc) -> Result<i64, String> {
    if arc.source_resolution.0 <= 0 || arc.quadrant == QuadrantMode::Unknown {
        return Err("arc source resolution or quadrant mode is unsupported".into());
    }
    let squared = |point: CanonicalPoint| {
        let x = i128::from(point.x.0) - i128::from(arc.center.x.0);
        let y = i128::from(point.y.0) - i128::from(arc.center.y.0);
        x.checked_mul(x)?.checked_add(y.checked_mul(y)?)
    };
    let start = squared(arc.start).ok_or("arc radius overflow")?;
    let end = squared(arc.end).ok_or("arc radius overflow")?;
    if start <= 0 || start != end {
        return Err("arc endpoints do not retain one exact radius".into());
    }
    exact_integer_sqrt(u128::try_from(start).map_err(|_| "negative arc radius")?)
        .filter(|radius| *radius > 0)
        .ok_or_else(|| "non-integral arc radius is outside the bounded topology slice".into())
}

fn segment_bounds(segment: &ContourSegment) -> Result<(i64, i64, i64, i64), String> {
    match segment {
        ContourSegment::Line(line) => Ok((
            line.start.x.0.min(line.end.x.0),
            line.start.y.0.min(line.end.y.0),
            line.start.x.0.max(line.end.x.0),
            line.start.y.0.max(line.end.y.0),
        )),
        ContourSegment::Arc(arc) => {
            let radius = arc_radius(arc)?;
            Ok((
                arc.center
                    .x
                    .0
                    .checked_sub(radius)
                    .ok_or("arc bounds overflow")?,
                arc.center
                    .y
                    .0
                    .checked_sub(radius)
                    .ok_or("arc bounds overflow")?,
                arc.center
                    .x
                    .0
                    .checked_add(radius)
                    .ok_or("arc bounds overflow")?,
                arc.center
                    .y
                    .0
                    .checked_add(radius)
                    .ok_or("arc bounds overflow")?,
            ))
        }
    }
}

fn bounds_disjoint(left: (i64, i64, i64, i64), right: (i64, i64, i64, i64)) -> bool {
    left.2 < right.0 || right.2 < left.0 || left.3 < right.1 || right.3 < left.1
}

fn line_has_second_circle_hit(
    line: &CanonicalLine,
    arc: &CanonicalArc,
    shared: CanonicalPoint,
) -> Result<bool, String> {
    let other = if line.start == shared {
        line.end
    } else if line.end == shared {
        line.start
    } else {
        return Err("adjacent line/arc segments do not share an endpoint".into());
    };
    let px = i128::from(shared.x.0) - i128::from(arc.center.x.0);
    let py = i128::from(shared.y.0) - i128::from(arc.center.y.0);
    let dx = i128::from(other.x.0) - i128::from(shared.x.0);
    let dy = i128::from(other.y.0) - i128::from(shared.y.0);
    let denominator = dx
        .checked_mul(dx)
        .and_then(|value| value.checked_add(dy.checked_mul(dy)?))
        .ok_or("line/arc predicate overflow")?;
    if denominator == 0 {
        return Err("zero-length outline line".into());
    }
    let numerator = px
        .checked_mul(dx)
        .and_then(|value| value.checked_add(py.checked_mul(dy)?))
        .and_then(|value| value.checked_mul(-2))
        .ok_or("line/arc predicate overflow")?;
    Ok(numerator > 0 && numerator <= denominator)
}

fn adjacent_segments_supported(
    left: &ContourSegment,
    right: &ContourSegment,
    shared: CanonicalPoint,
) -> Result<bool, String> {
    match (left, right) {
        (ContourSegment::Line(left), ContourSegment::Line(right)) => {
            outline_lines_overlap(left, right)
        }
        (ContourSegment::Line(line), ContourSegment::Arc(arc))
        | (ContourSegment::Arc(arc), ContourSegment::Line(line)) => {
            if line_has_second_circle_hit(line, arc, shared)? {
                Err("an adjacent line may intersect an arc away from its shared endpoint".into())
            } else {
                Ok(false)
            }
        }
        (ContourSegment::Arc(left), ContourSegment::Arc(right)) => {
            let left_radius = arc_radius(left)?;
            let right_radius = arc_radius(right)?;
            let dx = i128::from(right.center.x.0) - i128::from(left.center.x.0);
            let dy = i128::from(right.center.y.0) - i128::from(left.center.y.0);
            let distance_squared = dx
                .checked_mul(dx)
                .and_then(|value| value.checked_add(dy.checked_mul(dy)?))
                .ok_or("arc/arc predicate overflow")?;
            if left.center == right.center && left_radius == right_radius {
                return Err("coincident adjacent arcs have ambiguous overlap".into());
            }
            let sum = i128::from(left_radius)
                .checked_add(i128::from(right_radius))
                .ok_or("arc radius sum overflow")?;
            let difference = i128::from(left_radius)
                .checked_sub(i128::from(right_radius))
                .and_then(i128::checked_abs)
                .ok_or("arc radius difference overflow")?;
            let sum_squared = sum.checked_mul(sum).ok_or("arc radius sum overflow")?;
            let difference_squared = difference
                .checked_mul(difference)
                .ok_or("arc radius difference overflow")?;
            let tangent = distance_squared == sum_squared || distance_squared == difference_squared;
            if tangent {
                Ok(false)
            } else {
                Err("adjacent arcs may intersect away from their shared endpoint".into())
            }
        }
    }
}

fn outline_segments_intersect(
    left: &ContourSegment,
    right: &ContourSegment,
) -> Result<bool, String> {
    match (left, right) {
        (ContourSegment::Line(left), ContourSegment::Line(right)) => {
            outline_lines_intersect(left, right)
        }
        _ => {
            if [segment_start(left), segment_end(left)]
                .into_iter()
                .any(|point| point == segment_start(right) || point == segment_end(right))
            {
                return Ok(true);
            }
            if bounds_disjoint(segment_bounds(left)?, segment_bounds(right)?) {
                Ok(false)
            } else {
                Err("non-adjacent arc intersection is outside the bounded exact subset".into())
            }
        }
    }
}

fn line_contour_contains(
    contour: &CanonicalContour,
    point: CanonicalPoint,
) -> Result<bool, String> {
    if !contour.closed {
        return Err("open contour cannot establish cutout containment".into());
    }
    let mut winding = 0_i32;
    for segment in &contour.segments {
        let ContourSegment::Line(line) = segment else {
            return Err(
                "arc-bearing cutout containment is outside the bounded exact subset".into(),
            );
        };
        if point_on_outline_line(point, line)? {
            return Err("cutout classification touches another contour".into());
        }
        if line.start.y.0 <= point.y.0 {
            if line.end.y.0 > point.y.0 && outline_cross(line.start, line.end, point)? > 0 {
                winding += 1;
            }
        } else if line.end.y.0 <= point.y.0 && outline_cross(line.start, line.end, point)? < 0 {
            winding -= 1;
        }
    }
    Ok(winding != 0)
}

fn outline_topology(
    review: &FabricationReview,
    deadline: ManufacturingDeadline,
) -> (Vec<Finding>, Coverage) {
    const LABEL: &str = "Exact outline contour topology";
    let result = (|| -> Result<(Vec<Finding>, Coverage), String> {
        dispatch_complete(review, OUTLINE_REQUIREMENTS)?;
        if affected_capability_evidence(review, OUTLINE_REQUIREMENTS.prerequisites) {
            return Err("affected omission or conflict prevents complete outline topology".into());
        }
        let profile = review
            .profile
            .as_ref()
            .ok_or("canonical profile is missing")?;
        if profile.contour_feature_ids.len() != 1
            || profile
                .extents
                .as_ref()
                .is_none_or(|extent| extent.min.x >= extent.max.x || extent.min.y >= extent.max.y)
            || profile.provenance.is_empty()
        {
            return Err("exterior identity, extents, or profile provenance is ambiguous".into());
        }
        let mut identities = BTreeSet::new();
        let mut requested = Vec::new();
        for (classification, ids) in [
            (
                OutlineClassification::Exterior,
                &profile.contour_feature_ids,
            ),
            (OutlineClassification::Cutout, &profile.cutout_feature_ids),
        ] {
            for id in ids {
                deadline
                    .check("dfm-outline-topology")
                    .map_err(|error| error.to_string())?;
                if !identities.insert(id.as_str()) {
                    return Err(
                        "exterior/cutout classification is duplicated or overlapping".into(),
                    );
                }
                requested.push((classification, id.as_str()));
            }
        }
        requested.sort();

        let mut contours = Vec::new();
        let mut segment_count = 0_usize;
        for (classification, id) in requested {
            let mut features = review.features.iter().filter(|feature| feature.id == id);
            let feature = features
                .next()
                .filter(|_| features.next().is_none())
                .ok_or_else(|| format!("profile feature {id} is missing or ambiguous"))?;
            let mut layers = review
                .layers
                .iter()
                .filter(|layer| layer.id == feature.layer_id);
            let layer = layers
                .next()
                .filter(|_| layers.next().is_none())
                .ok_or_else(|| format!("profile feature {id} layer is missing or ambiguous"))?;
            let polarity_supported = match classification {
                OutlineClassification::Exterior => {
                    matches!(
                        feature.polarity,
                        LayerPolarity::Dark | LayerPolarity::Positive
                    )
                }
                OutlineClassification::Cutout => feature.polarity == LayerPolarity::Clear,
            };
            if layer.role != LayerRole::Profile
                || !polarity_supported
                || !feature.transforms.operations.is_empty()
                || feature.membership != FeatureMembership::TopLevel
                || review.repetitions.iter().any(|repetition| {
                    repetition
                        .feature_ids
                        .iter()
                        .any(|feature_id| feature_id == id)
                })
            {
                return Err(format!(
                    "profile feature {id} has unsupported layer, polarity, transform, or expansion"
                ));
            }
            let contour = match &feature.geometry {
                Geometry::Contour(contour) => contour,
                Geometry::Region(region) if region.contours.len() == 1 => &region.contours[0],
                _ => {
                    return Err(format!(
                        "profile feature {id} is not one represented contour"
                    ));
                }
            };
            if contour.segments.is_empty() {
                return Err(format!("profile feature {id} has no segments"));
            }
            segment_count = segment_count
                .checked_add(contour.segments.len())
                .ok_or("outline segment count overflow")?;
            if segment_count > MAX_OUTLINE_SEGMENTS {
                return Err(format!(
                    "outline segment limit {MAX_OUTLINE_SEGMENTS} exceeded"
                ));
            }
            for segment in &contour.segments {
                deadline
                    .check("dfm-outline-topology")
                    .map_err(|error| error.to_string())?;
                if segment_start(segment) == segment_end(segment) {
                    return Err(format!("profile feature {id} has a zero-length segment"));
                }
                if let ContourSegment::Arc(arc) = segment {
                    arc_radius(arc)?;
                }
            }
            contours.push((classification, feature, contour));
        }

        let mut open = Vec::new();
        let mut intersections = BTreeSet::new();
        let mut pair_checks = 0_usize;
        for (contour_index, (classification, feature, contour)) in contours.iter().enumerate() {
            let connected = contour
                .segments
                .windows(2)
                .all(|pair| segment_end(&pair[0]) == segment_start(&pair[1]));
            let last = contour
                .segments
                .last()
                .ok_or("validated outline contour became empty")?;
            let wraps = segment_end(last) == segment_start(&contour.segments[0]);
            if !contour.closed || !connected || !wraps {
                open.push((*classification, feature.id.as_str()));
            }
            for left in 0..contour.segments.len() {
                for right in (left + 1)..contour.segments.len() {
                    deadline
                        .check("dfm-outline-topology")
                        .map_err(|error| error.to_string())?;
                    pair_checks = pair_checks
                        .checked_add(1)
                        .ok_or("outline pair count overflow")?;
                    if pair_checks > MAX_OUTLINE_PAIR_CHECKS {
                        return Err(format!(
                            "outline pair limit {MAX_OUTLINE_PAIR_CHECKS} exceeded"
                        ));
                    }
                    let shared = if right == left + 1
                        && segment_end(&contour.segments[left])
                            == segment_start(&contour.segments[right])
                    {
                        Some(segment_end(&contour.segments[left]))
                    } else if contour.closed
                        && left == 0
                        && right + 1 == contour.segments.len()
                        && segment_end(&contour.segments[right])
                            == segment_start(&contour.segments[left])
                    {
                        Some(segment_end(&contour.segments[right]))
                    } else {
                        None
                    };
                    let intersects = if let Some(shared) = shared {
                        adjacent_segments_supported(
                            &contour.segments[left],
                            &contour.segments[right],
                            shared,
                        )?
                    } else {
                        outline_segments_intersect(
                            &contour.segments[left],
                            &contour.segments[right],
                        )?
                    };
                    if intersects {
                        intersections.insert((
                            feature.id.as_str(),
                            left,
                            feature.id.as_str(),
                            right,
                        ));
                    }
                }
            }
            for (other_classification, other_feature, other) in
                contours.iter().skip(contour_index + 1)
            {
                for (left, left_segment) in contour.segments.iter().enumerate() {
                    for (right, right_segment) in other.segments.iter().enumerate() {
                        deadline
                            .check("dfm-outline-topology")
                            .map_err(|error| error.to_string())?;
                        pair_checks = pair_checks
                            .checked_add(1)
                            .ok_or("outline pair count overflow")?;
                        if pair_checks > MAX_OUTLINE_PAIR_CHECKS {
                            return Err(format!(
                                "outline pair limit {MAX_OUTLINE_PAIR_CHECKS} exceeded"
                            ));
                        }
                        if outline_segments_intersect(left_segment, right_segment)? {
                            intersections.insert((
                                feature.id.as_str(),
                                left,
                                other_feature.id.as_str(),
                                right,
                            ));
                        }
                    }
                }
                let _ = (classification, other_classification);
            }
        }

        if open.is_empty() && intersections.is_empty() && contours.len() > 1 {
            let exterior = contours
                .iter()
                .find(|(classification, _, _)| *classification == OutlineClassification::Exterior)
                .ok_or("exterior contour is missing")?;
            let cutouts = contours
                .iter()
                .filter(|(classification, _, _)| *classification == OutlineClassification::Cutout)
                .collect::<Vec<_>>();
            for (_, feature, cutout) in &cutouts {
                let point = segment_start(&cutout.segments[0]);
                if !line_contour_contains(exterior.2, point)? {
                    return Err(format!(
                        "cutout {} is not exactly contained by the exterior",
                        feature.id
                    ));
                }
            }
            for left in 0..cutouts.len() {
                for right in (left + 1)..cutouts.len() {
                    let left_point = segment_start(&cutouts[left].2.segments[0]);
                    let right_point = segment_start(&cutouts[right].2.segments[0]);
                    if line_contour_contains(cutouts[left].2, right_point)?
                        || line_contour_contains(cutouts[right].2, left_point)?
                    {
                        return Err("nested cutouts have ambiguous topology".into());
                    }
                }
            }
        }

        let mut findings = Vec::new();
        for (classification, id) in &open {
            findings.push(Finding {
                id: format!("{OUTLINE_TOPOLOGY_FAMILY}/open/{id}"),
                severity: Severity::Medium,
                category: "DFM".into(),
                title: "Profile contour is open".into(),
                evidence: format!(
                    "classification={} contour={id} closed=false",
                    classification.name()
                ),
                recommendation:
                    "Close the exact profile contour without replacing arcs or source geometry."
                        .into(),
                location: format!("classification={};contour={id}", classification.name()),
                source: "fabrication".into(),
                gate_impact: family_gate_impact(OUTLINE_TOPOLOGY_FAMILY),
            });
        }
        for (left_id, left, right_id, right) in &intersections {
            findings.push(Finding {
                id: format!(
                    "{OUTLINE_TOPOLOGY_FAMILY}/intersection/{left_id}/{left}/{right_id}/{right}"
                ),
                severity: Severity::Medium,
                category: "DFM".into(),
                title: "Profile contours intersect".into(),
                evidence: format!(
                    "left_contour={left_id} left_segment={left} right_contour={right_id} right_segment={right} exact_intersection=true"
                ),
                recommendation:
                    "Remove the exact profile self-intersection or contour overlap at the source."
                        .into(),
                location: format!(
                    "left={left_id}:{left};right={right_id}:{right}"
                ),
                source: "fabrication".into(),
                gate_impact: family_gate_impact(OUTLINE_TOPOLOGY_FAMILY),
            });
        }
        findings.sort_by(|left, right| left.id.cmp(&right.id));
        let extent = profile
            .extents
            .as_ref()
            .ok_or("validated profile extents became unavailable")?;
        let details = contours
            .iter()
            .map(|(classification, feature, contour)| {
                format!(
                    "classification={} feature={} closed={} segments={} source={}:{} digest={}",
                    classification.name(),
                    feature.id,
                    contour.closed,
                    contour.segments.len(),
                    feature.provenance.document_id,
                    feature.provenance.location.record,
                    &feature.provenance.artifact_digest[..16],
                )
            })
            .collect::<Vec<_>>()
            .join(" | ");
        let evidence = format!(
            "contours={} exteriors=1 cutouts={} open={} intersections={} segments={segment_count} pair_checks={pair_checks} extents={},{}..{},{}pm; {details}",
            contours.len(),
            contours
                .iter()
                .filter(|(classification, _, _)| {
                    *classification == OutlineClassification::Cutout
                })
                .count(),
            open.len(),
            intersections.len(),
            extent.min.x.0,
            extent.min.y.0,
            extent.max.x.0,
            extent.max.y.0,
        );
        Ok((
            findings,
            Coverage {
                id: OUTLINE_TOPOLOGY_FAMILY.into(),
                label: LABEL.into(),
                status: if open.is_empty() && intersections.is_empty() {
                    CoverageStatus::Passed
                } else {
                    CoverageStatus::Attention
                },
                evidence,
            },
        ))
    })();
    result.unwrap_or_else(|reason| (vec![], not_checked(OUTLINE_TOPOLOGY_FAMILY, LABEL, reason)))
}

#[cfg(test)]
pub(crate) fn fabrication_families(
    review: &FabricationReview,
    schematic: Option<&SchematicReview>,
    deadline: ManufacturingDeadline,
) -> (Vec<Finding>, Vec<Coverage>) {
    fabrication_families_with_gaps(review, schematic, &BTreeMap::new(), deadline)
}

pub(crate) fn fabrication_families_with_gaps(
    review: &FabricationReview,
    schematic: Option<&SchematicReview>,
    declaration_gaps: &BTreeMap<String, String>,
    deadline: ManufacturingDeadline,
) -> (Vec<Finding>, Vec<Coverage>) {
    let (mut findings, minimum) = minimum_finished_drill(review, deadline);
    let (integrity_findings, integrity) = drill_tool_integrity(review, deadline);
    let (outline_findings, outline) = outline_topology(review, deadline);
    let (edge_findings, edge) = copper_edge(review, deadline);
    let (clearance_findings, clearance) = copper_clearance(review, deadline);
    let (annular_findings, annular) = annular_ring(review, deadline);
    let (mask_findings, mask) = mask_sliver(review, deadline);
    let (paste_mask_findings, paste_mask) = paste_mask_relationship(review, schematic, deadline);
    let (stackup_findings, stackup) = stackup_order_confirmation(review, deadline);
    let (thickness_findings, thickness) = total_thickness_material(review, deadline);
    let (drill_span_findings, drill_span) = drill_span_plating(review, declaration_gaps, deadline);
    let (finish_findings, finish) = finish_profile(review, declaration_gaps, deadline);
    let (process_findings, process) = impedance_special_process(review, deadline);
    findings.extend(integrity_findings);
    findings.extend(outline_findings);
    findings.extend(edge_findings);
    findings.extend(clearance_findings);
    findings.extend(annular_findings);
    findings.extend(mask_findings);
    findings.extend(paste_mask_findings);
    findings.extend(stackup_findings);
    findings.extend(thickness_findings);
    findings.extend(drill_span_findings);
    findings.extend(finish_findings);
    findings.extend(process_findings);
    findings.sort_by(|left, right| left.id.cmp(&right.id));
    (
        findings,
        vec![
            minimum, integrity, outline, edge, clearance, annular, mask, paste_mask, stackup,
            thickness, drill_span, finish, process,
        ],
    )
}

pub(crate) fn is_fabrication_family_check(check_id: &str) -> bool {
    [
        OUTLINE_TOPOLOGY_FAMILY,
        MINIMUM_FINISHED_DRILL_FAMILY,
        DRILL_TOOL_INTEGRITY_FAMILY,
        COPPER_EDGE_FAMILY,
        COPPER_CLEARANCE_FAMILY,
        ANNULAR_RING_FAMILY,
        MASK_SLIVER_FAMILY,
        PASTE_MASK_FAMILY,
        STACKUP_ORDER_FAMILY,
        TOTAL_THICKNESS_MATERIAL_FAMILY,
        DRILL_SPAN_PLATING_FAMILY,
        FINISH_PROFILE_FAMILY,
        IMPEDANCE_SPECIAL_PROCESS_FAMILY,
    ]
    .iter()
    .any(|family| {
        check_id == *family
            || check_id
                .strip_prefix(*family)
                .is_some_and(|suffix| suffix.starts_with('/'))
    })
}

pub(crate) fn validate_fabrication_families(
    review: &FabricationReview,
    schematic: &SchematicReview,
    findings: &[Finding],
    coverage: &[Coverage],
    evidence: &[EvidenceRecord],
    deadline: Option<ManufacturingDeadline>,
) -> Result<(), String> {
    let deadline =
        deadline.unwrap_or_else(|| ManufacturingDeadline::from_timeout(Duration::from_secs(30)));
    let declaration_gaps = normalized_declaration_gaps(coverage, Some(evidence))?;
    let (expected_findings, expected_coverage) =
        fabrication_families_with_gaps(review, Some(schematic), &declaration_gaps, deadline);
    let check_ids = evidence
        .iter()
        .map(|record| (record.id.as_str(), record.check_id.as_str()))
        .collect::<BTreeMap<_, _>>();
    let expected_coverage_ids = expected_coverage
        .iter()
        .map(|item| item.id.as_str())
        .collect::<BTreeSet<_>>();
    let actual_coverage_ids = coverage
        .iter()
        .filter_map(|item| check_ids.get(item.id.as_str()).copied())
        .filter(|check_id| is_fabrication_family_check(check_id))
        .collect::<Vec<_>>();
    if actual_coverage_ids.len() != expected_coverage.len()
        || actual_coverage_ids.iter().copied().collect::<BTreeSet<_>>() != expected_coverage_ids
    {
        return Err("fabrication family coverage set is incomplete or forged".into());
    }
    for expected in expected_coverage {
        let actual = coverage
            .iter()
            .filter(|item| check_ids.get(item.id.as_str()) == Some(&expected.id.as_str()))
            .collect::<Vec<_>>();
        if actual.len() != 1
            || actual[0].label != expected.label
            || actual[0].status != expected.status
            || actual[0].evidence != expected.evidence
        {
            return Err(format!(
                "{} coverage does not match canonical facts",
                expected.id
            ));
        }
    }
    let expected = expected_findings
        .iter()
        .map(|finding| (finding.id.as_str(), finding))
        .collect::<BTreeMap<_, _>>();
    let mut actual = BTreeMap::new();
    for finding in findings.iter().filter(|finding| {
        check_ids
            .get(finding.id.as_str())
            .is_some_and(|check_id| is_fabrication_family_check(check_id))
    }) {
        let check_id = check_ids[finding.id.as_str()];
        if actual.insert(check_id, finding).is_some() {
            return Err(format!("duplicate fabrication family finding {check_id}"));
        }
    }
    if expected.len() != actual.len()
        || expected.iter().any(|(check_id, expected)| {
            actual.get(check_id).is_none_or(|actual| {
                actual.severity != expected.severity
                    || actual.category != expected.category
                    || actual.title != expected.title
                    || actual.evidence != expected.evidence
                    || actual.recommendation != expected.recommendation
                    || actual.location != expected.location
                    || actual.source != expected.source
                    || actual.gate_impact
                        != if check_id.contains("/gap/") {
                            GateImpact::EvidenceOnly
                        } else {
                            family_gate_impact(check_id)
                        }
            })
        })
    {
        return Err("fabrication family findings do not match canonical facts".into());
    }
    Ok(())
}

pub(crate) fn assembly_families(
    review: &FabricationReview,
    schematic: &SchematicReview,
    deadline: ManufacturingDeadline,
) -> (Vec<Finding>, Vec<Coverage>) {
    let (mut findings, side_rotation) = side_rotation(review, deadline);
    let (paste_findings, paste) = assembly_paste_availability(review, deadline);
    let (courtyard_findings, courtyard) = courtyard_native(review);
    let (footprint_findings, footprint) = footprint_string_parity(review, schematic, deadline);
    let (access_findings, access) = assembly_access(review, deadline);
    let (testpoint_findings, testpoint) = testpoint_access(review, deadline);
    findings.extend(paste_findings);
    findings.extend(courtyard_findings);
    findings.extend(footprint_findings);
    findings.extend(access_findings);
    findings.extend(testpoint_findings);
    findings.sort_by(|left, right| {
        left.id
            .cmp(&right.id)
            .then_with(|| left.location.cmp(&right.location))
    });
    (
        findings,
        vec![
            side_rotation,
            paste,
            courtyard,
            footprint,
            access,
            testpoint,
        ],
    )
}

pub(crate) fn is_assembly_model_family_check(check_id: &str) -> bool {
    [
        SIDE_ROTATION_FAMILY,
        ASSEMBLY_PASTE_FAMILY,
        COURTYARD_NATIVE_FAMILY,
        ASSEMBLY_ACCESS_FAMILY,
        TESTPOINT_ACCESS_FAMILY,
    ]
    .iter()
    .any(|family| {
        check_id == *family
            || check_id
                .strip_prefix(*family)
                .is_some_and(|suffix| suffix.starts_with('/'))
    })
}

pub(crate) fn is_footprint_string_family_check(check_id: &str) -> bool {
    check_id == FOOTPRINT_STRING_FAMILY
        || check_id
            .strip_prefix(FOOTPRINT_STRING_FAMILY)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn is_assembly_family_check(check_id: &str) -> bool {
    is_assembly_model_family_check(check_id) || is_footprint_string_family_check(check_id)
}

pub(crate) fn is_footprint_reconciliation_check(check_id: &str) -> bool {
    check_id
        .strip_prefix("schematic-reconcile-")
        .is_some_and(|field| FOOTPRINT_FIELDS.contains(&field))
}

pub(crate) fn validate_assembly_families(
    review: &FabricationReview,
    schematic: &SchematicReview,
    findings: &[Finding],
    coverage: &[Coverage],
    evidence: &[EvidenceRecord],
    deadline: Option<ManufacturingDeadline>,
) -> Result<(), String> {
    let deadline =
        deadline.unwrap_or_else(|| ManufacturingDeadline::from_timeout(Duration::from_secs(30)));
    let (expected_findings, expected_coverage) = assembly_families(review, schematic, deadline);
    let check_ids = evidence
        .iter()
        .map(|record| (record.id.as_str(), record.check_id.as_str()))
        .collect::<BTreeMap<_, _>>();
    let actual_coverage = coverage
        .iter()
        .filter(|item| {
            check_ids
                .get(item.id.as_str())
                .is_some_and(|check_id| is_assembly_family_check(check_id))
        })
        .collect::<Vec<_>>();
    if actual_coverage.len() != expected_coverage.len() {
        return Err("assembly family coverage set is incomplete or forged".into());
    }
    for expected in expected_coverage {
        let actual = actual_coverage
            .iter()
            .filter(|item| check_ids.get(item.id.as_str()) == Some(&expected.id.as_str()))
            .collect::<Vec<_>>();
        if actual.len() != 1
            || actual[0].label != expected.label
            || actual[0].status != expected.status
            || actual[0].evidence != expected.evidence
        {
            return Err(format!(
                "{} coverage does not match canonical assembly facts",
                expected.id
            ));
        }
    }
    let expected = expected_findings
        .iter()
        .map(|finding| ((finding.id.as_str(), finding.location.as_str()), finding))
        .collect::<BTreeMap<_, _>>();
    let mut actual = BTreeMap::new();
    for finding in findings.iter().filter(|finding| {
        check_ids
            .get(finding.id.as_str())
            .is_some_and(|check_id| is_assembly_family_check(check_id))
    }) {
        let check_id = check_ids[finding.id.as_str()];
        if actual
            .insert((check_id, finding.location.as_str()), finding)
            .is_some()
        {
            return Err("assembly finding identity is duplicated".into());
        }
    }
    if expected.len() != actual.len()
        || expected.iter().any(|((check_id, location), expected)| {
            actual.get(&(*check_id, *location)).is_none_or(|actual| {
                actual.severity != expected.severity
                    || actual.category != expected.category
                    || actual.title != expected.title
                    || actual.evidence != expected.evidence
                    || actual.recommendation != expected.recommendation
                    || actual.source != expected.source
                    || actual.gate_impact != family_gate_impact(check_id)
            })
        })
    {
        return Err("assembly findings do not match canonical facts".into());
    }
    Ok(())
}

pub(crate) fn is_population_reconciliation_check(check_id: &str) -> bool {
    check_id
        .strip_prefix("schematic-reconcile-")
        .is_some_and(|field| POPULATION_FIELDS.contains(&field))
}

pub(crate) fn is_population_finding_check(check_id: &str) -> bool {
    check_id
        .strip_prefix(POPULATION_FINDING_PREFIX)
        .is_some_and(|field| POPULATION_FIELDS.contains(&field))
}

pub(crate) fn population_inputs_complete<'a, 'b>(
    review: &SchematicReview,
    coverage: impl Iterator<Item = (&'a str, &'a CoverageStatus)>,
    artifacts: impl Iterator<Item = (&'b str, &'b str, bool)>,
) -> bool {
    let (mut bom_coverage, mut bom_complete) = (0, false);
    let (mut placement_coverage, mut placement_complete) = (0, false);
    for (id, status) in coverage {
        let complete = matches!(status, CoverageStatus::Passed | CoverageStatus::Attention);
        match id {
            "bom-structure" => {
                bom_coverage += 1;
                bom_complete = complete;
            }
            "placement-structure" => {
                placement_coverage += 1;
                placement_complete = complete;
            }
            _ => {}
        }
    }
    let (mut bom_artifacts, mut placement_artifacts) = (0, 0);
    for (path, kind, _) in artifacts.filter(|(_, _, selected)| *selected) {
        let retained = review
            .artifact_digests
            .get(path)
            .is_some_and(|digest| crate::lowercase_sha256(digest));
        match kind {
            "bom" if retained => bom_artifacts += 1,
            "placement" if retained => placement_artifacts += 1,
            _ => {}
        }
    }
    let explicit = bom_coverage == 1
        && bom_complete
        && placement_coverage == 1
        && placement_complete
        && bom_artifacts == 1
        && placement_artifacts == 1;
    let native = [
        ("native-bom-export", "native:bom.csv"),
        ("native-position-export", "native:positions.csv"),
    ]
    .into_iter()
    .all(|(id, artifact)| {
        let mut capabilities = review
            .capabilities
            .iter()
            .filter(|capability| capability.id == id);
        capabilities.next().is_some_and(|capability| {
            capability.status == "completed"
                && capability.evidence_class == "explicit-export-facts"
                && capability
                    .producer
                    .strip_prefix("kicad-cli ")
                    .is_some_and(|version| {
                        !version.is_empty()
                            && crate::schematic::KiCadMajor::parse(version).is_some()
                    })
                && !capability.detail.trim().is_empty()
                && review
                    .artifact_digests
                    .get(artifact)
                    .is_some_and(|digest| crate::lowercase_sha256(digest))
        }) && capabilities.next().is_none()
    });
    explicit || native
}

pub(crate) fn population_parity(
    review: &SchematicReview,
    inputs_complete: bool,
) -> (Vec<Finding>, Coverage) {
    match population_mismatches(review, inputs_complete) {
        Ok(mismatches) => {
            let findings = mismatches
                .iter()
                .map(|mismatch| Finding {
                    id: format!("{POPULATION_FINDING_PREFIX}{}", mismatch.field),
                    severity: Severity::Medium,
                    category: "Assembly".into(),
                    title: format!("Population parity differs for {}", mismatch.field),
                    evidence: format!(
                        "Expected {}; observed {}; join {} ({} confidence).",
                        mismatch.expected, mismatch.actual, mismatch.join, mismatch.confidence
                    ),
                    recommendation: "Regenerate schematic, board, BOM, and placement artifacts from one project revision.".into(),
                    location: mismatch.location.clone(),
                    source: "schematic-reconciliation".into(),
                    gate_impact: family_gate_impact(POPULATION_PARITY_FAMILY),
                })
                .collect::<Vec<_>>();
            let count = findings.len();
            (
                findings,
                Coverage {
                    id: POPULATION_PARITY_FAMILY.into(),
                    label: "Typed population reconciliation".into(),
                    status: if count == 0 {
                        CoverageStatus::Passed
                    } else {
                        CoverageStatus::Attention
                    },
                    evidence: format!(
                        "Completed typed reconciliation produced {count} typed population mismatch(es); family remains evidence-only."
                    ),
                },
            )
        }
        Err(reason) => (
            vec![],
            Coverage {
                id: POPULATION_PARITY_FAMILY.into(),
                label: "Typed population reconciliation".into(),
                status: CoverageStatus::NotRun,
                evidence: format!("not_checked: {reason}"),
            },
        ),
    }
}

fn population_mismatches(
    review: &SchematicReview,
    inputs_complete: bool,
) -> Result<Vec<&SchematicMismatch>, String> {
    if !inputs_complete {
        return Err("BOM and placement population inputs are incomplete".into());
    }
    if review.status != "completed" {
        return Err(format!("schematic review status is {}", review.status));
    }
    if review.occurrence_count == 0 || review.occurrence_count != review.occurrences.len() {
        return Err("schematic occurrence identity is incomplete".into());
    }
    let pair = review
        .source_pair
        .as_ref()
        .ok_or("coherent schematic/board source identity is missing")?;
    if pair.project_identity.trim().is_empty()
        || pair.schematic_path.trim().is_empty()
        || pair.board_path.trim().is_empty()
        || review.project_identity.as_deref() != Some(pair.project_identity.as_str())
        || review.root_path.as_deref() != Some(pair.schematic_path.as_str())
        || review.root_digest.as_deref() != Some(pair.schematic_digest.as_str())
        || review.board_path.as_deref() != Some(pair.board_path.as_str())
        || review.board_digest.as_deref() != Some(pair.board_digest.as_str())
        || !crate::lowercase_sha256(&pair.schematic_digest)
        || !crate::lowercase_sha256(&pair.board_digest)
        || review
            .artifact_digests
            .get(&pair.schematic_path)
            .is_none_or(|digest| digest != &pair.schematic_digest)
        || review
            .artifact_digests
            .get(&pair.board_path)
            .is_none_or(|digest| digest != &pair.board_digest)
    {
        return Err("schematic/board source identity is inconsistent".into());
    }
    if review
        .artifact_digests
        .get("schematic:composite")
        .is_none_or(|digest| !crate::lowercase_sha256(digest))
    {
        return Err("schematic composite identity is missing or malformed".into());
    }

    let mut capabilities = review
        .capabilities
        .iter()
        .filter(|capability| capability.id == "schematic-reconciliation");
    let capability = capabilities
        .next()
        .ok_or("schematic reconciliation capability is missing")?;
    if capabilities.next().is_some()
        || capability.status
            != if review.mismatches.is_empty() {
                "completed"
            } else {
                "attention"
            }
        || capability.producer != "ratemypcb"
        || capability.evidence_class != "deterministic-cross-artifact"
        || capability.detail.trim().is_empty()
    {
        return Err("schematic reconciliation capability is incomplete or ambiguous".into());
    }

    let mut reference_counts = BTreeMap::new();
    let mut occurrences = BTreeMap::new();
    for occurrence in &review.occurrences {
        if review
            .artifact_digests
            .get(&occurrence.source_path)
            .is_none_or(|digest| !crate::lowercase_sha256(digest))
            || occurrences
                .insert(
                    (
                        occurrence.sheet_uuid_path.as_str(),
                        occurrence.item_uuid.as_str(),
                        occurrence.source_path.as_str(),
                    ),
                    occurrence,
                )
                .is_some()
        {
            return Err("schematic occurrence provenance is incomplete or duplicated".into());
        }
        if let Some(reference) = occurrence.reference.as_deref() {
            *reference_counts
                .entry(reference.to_ascii_uppercase())
                .or_insert(0) += 1;
        }
    }
    let mut identities = BTreeSet::new();
    let mut mismatches = Vec::new();
    for mismatch in review
        .mismatches
        .iter()
        .filter(|mismatch| POPULATION_FIELDS.contains(&mismatch.field.as_str()))
    {
        let expected_check_id = format!("schematic-reconcile-{}", mismatch.field.replace('_', "-"));
        let expected_confidence = if mismatch.join == "reference-fallback" {
            "low"
        } else {
            "high"
        };
        let occurrence = location_identity(&mismatch.location)
            .and_then(|identity| occurrences.get(&identity).copied());
        let identity_is_unique = mismatch.join == "occurrence-uuid"
            || mismatch.join == "artifact-revision"
            || occurrence
                .and_then(|occurrence| occurrence.reference.as_deref())
                .is_some_and(|reference| {
                    reference_counts.get(&reference.to_ascii_uppercase()) == Some(&1)
                });
        if mismatch.check_id != expected_check_id
            || mismatch.expected.trim().is_empty()
            || mismatch.actual.trim().is_empty()
            || mismatch.expected == mismatch.actual
            || mismatch.confidence != expected_confidence
            || mismatch.gate_impact != GateImpact::EvidenceOnly
            || occurrence.is_none()
            || !identity_is_unique
            || !identities.insert((mismatch.check_id.as_str(), mismatch.location.as_str()))
        {
            return Err("typed population mismatch identity or provenance is invalid".into());
        }
        mismatches.push(mismatch);
    }
    Ok(mismatches)
}

fn location_identity(location: &str) -> Option<(&str, &str, &str)> {
    let location = location.strip_prefix("sheet=")?;
    let (sheet, location) = location.split_once(";item=")?;
    let (item, source) = location.split_once(";source=")?;
    (!sheet.trim().is_empty() && !item.trim().is_empty() && !source.trim().is_empty())
        .then_some((sheet, item, source))
}

pub(crate) fn validate_population_parity(
    review: &SchematicReview,
    inputs_complete: bool,
    findings: &[Finding],
    coverage: &[Coverage],
    evidence: &[EvidenceRecord],
) -> Result<(), String> {
    let (expected_findings, expected_coverage) = population_parity(review, inputs_complete);
    let check_ids = evidence
        .iter()
        .map(|record| (record.id.as_str(), record.check_id.as_str()))
        .collect::<BTreeMap<_, _>>();
    if expected_coverage.status != CoverageStatus::NotRun
        && findings.iter().any(|finding| {
            check_ids
                .get(finding.id.as_str())
                .is_some_and(|check_id| is_population_reconciliation_check(check_id))
        })
    {
        return Err("population mismatch was emitted twice".into());
    }
    let actual_coverage = coverage
        .iter()
        .filter(|item| check_ids.get(item.id.as_str()) == Some(&POPULATION_PARITY_FAMILY))
        .collect::<Vec<_>>();
    if actual_coverage.len() != 1
        || actual_coverage[0].label != expected_coverage.label
        || actual_coverage[0].status != expected_coverage.status
        || actual_coverage[0].evidence != expected_coverage.evidence
    {
        return Err("population parity coverage does not match typed reconciliation".into());
    }

    let mut actual_findings = BTreeMap::new();
    for finding in findings.iter().filter(|finding| {
        check_ids
            .get(finding.id.as_str())
            .is_some_and(|check_id| is_population_finding_check(check_id))
    }) {
        let check_id = check_ids[finding.id.as_str()];
        if actual_findings
            .insert((check_id, finding.location.as_str()), finding)
            .is_some()
        {
            return Err("population parity finding identities are duplicated".into());
        }
    }
    if actual_findings.len() != expected_findings.len()
        || expected_findings.iter().any(|expected| {
            actual_findings
                .get(&(expected.id.as_str(), expected.location.as_str()))
                .is_none_or(|actual| {
                    actual.severity != expected.severity
                        || actual.category != expected.category
                        || actual.title != expected.title
                        || actual.evidence != expected.evidence
                        || actual.recommendation != expected.recommendation
                        || actual.source != expected.source
                        || actual.gate_impact != family_gate_impact(&expected.id)
                })
        })
    {
        return Err("population parity findings do not match typed reconciliation".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eligible_policy(inference: bool) -> FamilyPolicy {
        let key = if inference {
            "inference.interface.v1"
        } else {
            POPULATION_PARITY_FAMILY
        };
        FamilyPolicy {
            key,
            inference,
            evidence: QualificationEvidence {
                precision_bps: Some(9_500),
                recall_bps: Some(1),
                positive_present: true,
                hard_negative_present: true,
                mutations_green: true,
                fixture_digest: Some(
                    "985e199bdaf7fd8a59c1a6ca7f63937e5d6f772794e0d597a0ab631edad674f9",
                ),
                reviewed_family_version: Some(key),
                reviewer: Some("independent-reviewer"),
                inference_approval: inference.then_some(key),
            },
        }
    }

    #[test]
    fn qualification_positive_eligibility_is_exact_and_reviewed() {
        let deterministic = eligible_policy(false);
        assert!(qualification_eligible(deterministic));
        assert_eq!(
            family_gate_impact(POPULATION_PARITY_FAMILY),
            GateImpact::EvidenceOnly,
            "the shipped static policy has no reviewed promotion"
        );
        assert!(qualification_eligible(eligible_policy(true)));
    }

    #[test]
    fn qualification_missing_under_threshold_or_mismatched_facts_downgrade() {
        let policy = eligible_policy(false);
        for evidence in [
            QualificationEvidence {
                precision_bps: Some(9_499),
                ..policy.evidence
            },
            QualificationEvidence {
                precision_bps: Some(10_001),
                ..policy.evidence
            },
            QualificationEvidence {
                precision_bps: None,
                ..policy.evidence
            },
            QualificationEvidence {
                recall_bps: None,
                ..policy.evidence
            },
            QualificationEvidence {
                recall_bps: Some(10_001),
                ..policy.evidence
            },
            QualificationEvidence {
                positive_present: false,
                ..policy.evidence
            },
            QualificationEvidence {
                hard_negative_present: false,
                ..policy.evidence
            },
            QualificationEvidence {
                mutations_green: false,
                ..policy.evidence
            },
            QualificationEvidence {
                fixture_digest: None,
                ..policy.evidence
            },
            QualificationEvidence {
                reviewed_family_version: Some("assembly.population-parity.v2"),
                ..policy.evidence
            },
            QualificationEvidence {
                reviewer: None,
                ..policy.evidence
            },
        ] {
            assert!(!qualification_eligible(FamilyPolicy { evidence, ..policy }));
        }
        let inference = eligible_policy(true);
        assert!(!qualification_eligible(FamilyPolicy {
            evidence: QualificationEvidence {
                inference_approval: None,
                ..inference.evidence
            },
            ..inference
        }));
        assert_eq!(
            family_gate_impact("assembly.population-parity.v2/finding"),
            GateImpact::EvidenceOnly
        );
        assert_eq!(
            family_gate_impact("inference.interface.v1/finding"),
            GateImpact::EvidenceOnly
        );
    }

    #[test]
    fn authority_merge_rejects_existing_semantic_constraint() {
        let original = include_bytes!("../../../tests/fixtures/dfm/declarations.json");
        let incoming =
            DfmDeclarations::from_json("incoming.json", original, 2_000_000_000).unwrap();
        let mut existing_value: serde_json::Value = serde_json::from_slice(original).unwrap();
        existing_value["producer"] = serde_json::Value::String("existing-authority".into());
        let existing_bytes = serde_json::to_vec(&existing_value).unwrap();
        let existing =
            DfmDeclarations::from_json("existing.json", &existing_bytes, 2_000_000_000).unwrap();
        let mut document = existing.document().unwrap();
        document.adapter = "existing-authority".into();
        let record = existing
            .records
            .iter()
            .find(|record| record.id == "minimum_drill")
            .unwrap();
        let provenance = existing.provenance(&document, record);
        let mut review = FabricationReview::default();
        review.documents.push(document.clone());
        review.constraints.push(ManufacturingConstraint {
            id: constraint_id(
                &document.id,
                ConstraintKind::MinimumDrill,
                &provenance.location,
            ),
            kind: ConstraintKind::MinimumDrill,
            value: record.value,
            declared_value: Some(declared_constraint_value(record)),
            authority: Authority::Explicit,
            provenance,
        });
        review.refresh_digests().unwrap();
        review.validate().unwrap();

        assert!(apply_declarations(&mut review, Some(&incoming)).is_err());
    }

    fn drill_fixture_review(declarations: bool) -> FabricationReview {
        let root = std::env::temp_dir().join(format!(
            "ratemypcb-dfm-drill-unit-{}-{declarations}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir(&root).unwrap();
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/fabrication/xnc/strict.xnc"),
            root.join("strict.xnc"),
        )
        .unwrap();
        let authority = declarations.then(|| {
            DfmDeclarations::from_json(
                "dfm/declarations.json",
                include_bytes!("../../../tests/fixtures/dfm/declarations.json"),
                2_000_000_000,
            )
            .unwrap()
        });
        let fabrication = crate::review(
            &root,
            crate::ReviewOptions {
                board: None,
                schematic: None,
                bom: None,
                placement: None,
                supply_snapshot: None,
                dfm_declarations: authority,
                preset: crate::Preset::named("standard").unwrap(),
                native: crate::NativeMode::Off,
                tool_version: "dfm-drill-unit-test".into(),
                scope: crate::ReviewScope::Full,
                profile: None,
            },
        )
        .unwrap()
        .fabrication;
        std::fs::remove_dir_all(root).unwrap();
        fabrication
    }

    fn drill_family_status(review: &FabricationReview, family: &str) -> CoverageStatus {
        fabrication_families(
            review,
            None,
            ManufacturingDeadline::from_timeout(Duration::from_secs(5)),
        )
        .1
        .into_iter()
        .find(|coverage| coverage.id == family)
        .unwrap()
        .status
    }

    #[test]
    fn drill_mutations_fail_closed_and_distinct_objects_are_deterministic() {
        let valid = drill_fixture_review(true);
        assert_eq!(
            drill_family_status(&valid, MINIMUM_FINISHED_DRILL_FAMILY),
            CoverageStatus::Passed
        );
        assert_eq!(
            drill_family_status(&valid, DRILL_TOOL_INTEGRITY_FAMILY),
            CoverageStatus::Passed
        );

        let mut direct = drill_fixture_review(false);
        let document = direct.documents[0].clone();
        let provenance = direct.tools[0].provenance.clone();
        direct.constraints.push(ManufacturingConstraint {
            id: constraint_id(
                &document.id,
                ConstraintKind::MinimumDrill,
                &provenance.location,
            ),
            kind: ConstraintKind::MinimumDrill,
            value: Some(Picometres(600_000_000)),
            declared_value: Some("minimum_drill=0.600 mm;applies=board".into()),
            authority: Authority::Explicit,
            provenance: provenance.clone(),
        });
        direct.capabilities.records.push(CapabilityRecord {
            id: CapabilityId::Constraints,
            state: CapabilityState::Complete,
            authority: Authority::Explicit,
            document_ids: vec![document.id],
            provenance: vec![provenance],
            detail: "direct fixture injection is not production authority".into(),
        });
        assert_eq!(
            drill_family_status(&direct, MINIMUM_FINISHED_DRILL_FAMILY),
            CoverageStatus::NotRun
        );

        for state in [
            CapabilityState::Partial,
            CapabilityState::NotProvided,
            CapabilityState::Unsupported,
            CapabilityState::Failed,
            CapabilityState::Stale,
            CapabilityState::Omitted,
        ] {
            let mut mutated = valid.clone();
            mutated
                .capabilities
                .records
                .iter_mut()
                .find(|record| record.id == CapabilityId::Plating)
                .unwrap()
                .state = state;
            assert_eq!(
                drill_family_status(&mutated, MINIMUM_FINISHED_DRILL_FAMILY),
                CoverageStatus::NotRun,
                "{state:?}"
            );
            assert_eq!(
                drill_family_status(&mutated, DRILL_TOOL_INTEGRITY_FAMILY),
                CoverageStatus::NotRun,
                "{state:?}"
            );
        }

        let mut unknown_span = valid.clone();
        unknown_span.tools[0].span = None;
        assert_eq!(
            drill_family_status(&unknown_span, DRILL_TOOL_INTEGRITY_FAMILY),
            CoverageStatus::NotRun
        );

        let mut route_tool_as_round_hit = valid.clone();
        route_tool_as_round_hit.tools[0].kind = ToolKind::Route;
        assert_eq!(
            drill_family_status(&route_tool_as_round_hit, MINIMUM_FINISHED_DRILL_FAMILY),
            CoverageStatus::NotRun
        );
        assert_eq!(
            drill_family_status(&route_tool_as_round_hit, DRILL_TOOL_INTEGRITY_FAMILY),
            CoverageStatus::NotRun
        );

        let mut duplicate_tool = valid.clone();
        duplicate_tool.tools.push(duplicate_tool.tools[0].clone());
        assert_eq!(
            drill_family_status(&duplicate_tool, DRILL_TOOL_INTEGRITY_FAMILY),
            CoverageStatus::NotRun
        );

        let mut duplicate_capability = valid.clone();
        let tools = duplicate_capability
            .capabilities
            .records
            .iter()
            .find(|record| record.id == CapabilityId::Tools)
            .unwrap()
            .clone();
        duplicate_capability.capabilities.records.push(tools);
        assert_eq!(
            drill_family_status(&duplicate_capability, DRILL_TOOL_INTEGRITY_FAMILY),
            CoverageStatus::NotRun
        );

        let mut ambiguous = valid.clone();
        ambiguous
            .features
            .iter_mut()
            .find(|feature| matches!(feature.geometry, Geometry::Drill(_)))
            .unwrap()
            .tool_id = None;
        assert_eq!(
            drill_family_status(&ambiguous, DRILL_TOOL_INTEGRITY_FAMILY),
            CoverageStatus::NotRun
        );

        let mut no_round_hits = valid.clone();
        no_round_hits
            .features
            .retain(|feature| !matches!(feature.geometry, Geometry::Drill(_)));
        assert_eq!(
            drill_family_status(&no_round_hits, MINIMUM_FINISHED_DRILL_FAMILY),
            CoverageStatus::NotRun,
            "routes and slots must not masquerade as round hits"
        );

        let mut mismatch = valid.clone();
        let Geometry::Drill(drill) = &mut mismatch
            .features
            .iter_mut()
            .find(|feature| matches!(feature.geometry, Geometry::Drill(_)))
            .unwrap()
            .geometry
        else {
            unreachable!()
        };
        drill.diameter = Picometres(599_000_000);
        let (findings, coverage) = fabrication_families(
            &mismatch,
            None,
            ManufacturingDeadline::from_timeout(Duration::from_secs(5)),
        );
        assert_eq!(
            coverage
                .iter()
                .find(|coverage| coverage.id == DRILL_TOOL_INTEGRITY_FAMILY)
                .unwrap()
                .status,
            CoverageStatus::Attention
        );
        assert_eq!(
            findings
                .iter()
                .filter(|finding| finding.id.starts_with(DRILL_TOOL_INTEGRITY_FAMILY))
                .count(),
            1
        );
        assert!(findings.iter().all(|finding| {
            !finding.id.starts_with(DRILL_TOOL_INTEGRITY_FAMILY)
                || finding.gate_impact == GateImpact::EvidenceOnly
        }));

        let mut reordered = valid.clone();
        reordered.features.reverse();
        reordered.tools.reverse();
        assert_eq!(
            serde_json::to_value(fabrication_families(
                &valid,
                None,
                ManufacturingDeadline::from_timeout(Duration::from_secs(5)),
            ))
            .unwrap(),
            serde_json::to_value(fabrication_families(
                &reordered,
                None,
                ManufacturingDeadline::from_timeout(Duration::from_secs(5)),
            ))
            .unwrap()
        );
    }

    fn geometry_options(declarations: bool) -> crate::ReviewOptions {
        crate::ReviewOptions {
            board: None,
            schematic: None,
            bom: None,
            placement: None,
            supply_snapshot: None,
            dfm_declarations: declarations.then(|| {
                DfmDeclarations::from_json(
                    "dfm/declarations.json",
                    include_bytes!("../../../tests/fixtures/dfm/declarations.json"),
                    2_000_000_000,
                )
                .unwrap()
            }),
            preset: crate::Preset::named("standard").unwrap(),
            native: crate::NativeMode::Off,
            tool_version: "dfm-plan07-05-unit-test".into(),
            scope: crate::ReviewScope::Full,
            profile: None,
        }
    }

    fn unit_fixture_root(label: &str) -> std::path::PathBuf {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        std::env::temp_dir().join(format!(
            "ratemypcb-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ))
    }

    fn distance_fixture_review() -> FabricationReview {
        let root = unit_fixture_root("dfm-distance-unit");
        std::fs::create_dir(&root).unwrap();
        let copper = |function: &str, second_net: &str| {
            format!(
                "G04 Plan 07-05 distance unit fixture*\n%FSLAX46Y46*%\n%MOMM*%\n%TF.FileFunction,{function}*%\n%TA.AperFunction,Conductor*%\n%ADD10C,0.200*%\nD10*\n%TO.C,U1*%\n%TO.P,U1,1*%\n%TO.N,GND*%\nX1000000Y1000000D02*\nX2000000Y1000000D01*\n%TO.N,{second_net}*%\nX1000000Y1400000D02*\nX2000000Y1400000D01*\nM02*\n"
            )
        };
        std::fs::write(root.join("top.gbr"), copper("Copper,L1,Top", "VCC")).unwrap();
        std::fs::write(root.join("bottom.gbr"), copper("Copper,L2,Bot", "GND")).unwrap();
        std::fs::write(
            root.join("profile.gbr"),
            "G04 Plan 07-05 profile*\n%FSLAX46Y46*%\n%MOMM*%\n%TF.FileFunction,Profile,NP*%\n%ADD10C,0.200*%\nD10*\n%TO.N,GND*%\n%TO.C,U1*%\n%TO.P,U1,1*%\nG36*\nX000000Y000000D02*\nX10000000Y000000D01*\nX10000000Y10000000D01*\nX000000Y10000000D01*\nX000000Y000000D01*\nG37*\nM02*\n",
        )
        .unwrap();
        std::fs::write(
            root.join("holes.xnc"),
            "; Plan 07-05 drill\nM48\n; #@! TF.FileFunction,Plated,1,2,PTH\n; #@! TF.GenerationSoftware,Ucamco,UcamX,2021.11\nMETRIC\nT01C0.600\n%\nT01\nX5.000Y5.000\nM30\n",
        )
        .unwrap();
        std::fs::write(
            root.join("complete.gbrjob"),
            serde_json::to_vec(&serde_json::json!({
                "Header": {"GenerationSoftware": {"Vendor": "RateMyPCB", "Application": "fixture", "Version": "1"}},
                "GeneralSpecs": {"ProjectId": {"Name": "phase7-unit", "Revision": "r1", "PartNumber": "P7-005"}},
                "FilesAttributes": [
                    {"Path": "top.gbr", "FileFunction": "Copper,L1,Top"},
                    {"Path": "bottom.gbr", "FileFunction": "Copper,L2,Bot"},
                    {"Path": "profile.gbr", "FileFunction": "Profile,NP"},
                    {"Path": "holes.xnc", "FileFunction": "Plated,1,2,PTH"}
                ]
            }))
            .unwrap(),
        )
        .unwrap();
        let review = crate::review(&root, geometry_options(true))
            .unwrap()
            .fabrication;
        std::fs::remove_dir_all(root).unwrap();
        review
    }

    fn annular_fixture_review() -> FabricationReview {
        let root = unit_fixture_root("dfm-annular-unit");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(
            root.join("board.kicad_pcb"),
            "(kicad_pcb (version 20240108)\n  (generator ratemypcb-plan07-05)\n  (layers (0 \"F.Cu\" signal) (31 \"B.Cu\" signal) (44 \"Edge.Cuts\" user \"Edge.Cuts\"))\n  (net 0 \"\") (net 1 \"PTH\")\n  (footprint \"Connector:Test\" (layer \"F.Cu\") (at 0 0)\n    (property \"Reference\" \"J1\")\n    (pad \"1\" thru_hole circle (at 5.000 5.000) (size 1.000 1.000) (drill 0.600) (layers \"*.Cu\" \"*.Mask\") (net 1 \"PTH\")))\n  (gr_rect (start 0 0) (end 10 10) (layer \"Edge.Cuts\")))\n",
        )
        .unwrap();
        let review = crate::review(&root, geometry_options(true))
            .unwrap()
            .fabrication;
        std::fs::remove_dir_all(root).unwrap();
        review
    }

    fn mask_paste_fixture_review() -> FabricationReview {
        mask_paste_fixture_review_with_thresholds("0.08", "0.05")
    }

    fn mask_paste_fixture_review_with_thresholds(mask: &str, paste: &str) -> FabricationReview {
        let root = unit_fixture_root("dfm-mask-paste-unit");
        std::fs::create_dir(&root).unwrap();
        let gerber = |function: &str, diameter: &str| {
            format!(
                "G04 Plan 07-06 exact mask/paste unit fixture*\n%FSLAX46Y46*%\n%MOMM*%\n%TF.FileFunction,{function}*%\n%TA.AperFunction,SMDPad,CuDef*%\n%ADD10C,{diameter}*%\nD10*\n%TO.C,R1*%\n%TO.P,R1,1*%\nX1000000Y1000000D03*\n%TO.P,R1,2*%\nX2200000Y1000000D03*\nM02*\n"
            )
        };
        std::fs::write(root.join("mask.gbr"), gerber("Soldermask,Top", "1.000")).unwrap();
        std::fs::write(root.join("paste.gbr"), gerber("Paste,Top", "0.800")).unwrap();
        std::fs::write(
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
        let mut declaration: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../tests/fixtures/dfm/declarations.json"
        ))
        .unwrap();
        for rule in declaration["rules"].as_array_mut().unwrap() {
            match rule["id"].as_str().unwrap() {
                MASK_SLIVER_FAMILY => rule["value"] = mask.into(),
                PASTE_MASK_FAMILY => rule["value"] = paste.into(),
                _ => {}
            }
        }
        let declarations = DfmDeclarations::from_json(
            "dfm/declarations.json",
            &serde_json::to_vec(&declaration).unwrap(),
            2_000_000_000,
        )
        .unwrap();
        let mut options = geometry_options(false);
        options.dfm_declarations = Some(declarations);
        let mut review = crate::review(&root, options).unwrap().fabrication;
        std::fs::remove_dir_all(root).unwrap();
        for layer in &mut review.layers {
            layer.polarity = match layer.role {
                LayerRole::SolderMask => LayerPolarity::Negative,
                LayerRole::Paste => LayerPolarity::Positive,
                _ => layer.polarity,
            };
        }
        let provenance = review
            .features
            .iter()
            .find(|feature| {
                review.layers.iter().any(|layer| {
                    layer.id == feature.layer_id && layer.role == LayerRole::SolderMask
                })
            })
            .unwrap()
            .provenance
            .clone();
        review
            .assembly
            .placements
            .push(crate::fabrication::AssemblyPlacement {
                id: crate::fabrication::assembly_placement_id(
                    &provenance.document_id,
                    Some("dfm-mask-paste-r1"),
                    "R1",
                    &provenance.location,
                )
                .unwrap(),
                occurrence_id: Some("dfm-mask-paste-r1".into()),
                reference: "R1".into(),
                side: LayerSide::Top,
                position: CanonicalPoint::new(0, 0),
                rotation_microdegrees: 0,
                fitted: crate::fabrication::AssemblyFittedState::Fitted,
                revision: Some("r1".into()),
                convention: crate::fabrication::AssemblyPlacementConvention::native_kicad(),
                provenance: provenance.clone(),
            });
        review.capabilities.records.push(CapabilityRecord {
            id: CapabilityId::Assembly,
            state: CapabilityState::Complete,
            authority: Authority::X2,
            document_ids: vec![provenance.document_id.clone()],
            provenance: vec![provenance],
            detail: "Exact component placement-side evidence for Plan 07-06 unit analysis.".into(),
        });
        review.capabilities.records.sort_by_key(|record| record.id);
        review.refresh_digests().unwrap();
        review.validate().unwrap();
        review
    }

    fn fitted_schematic_review() -> SchematicReview {
        crate::review(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/kicad/mismatch"),
            geometry_options(false),
        )
        .unwrap()
        .schematic
    }

    fn role_feature_id(review: &FabricationReview, role: LayerRole, pin: &str) -> String {
        let layer_ids = review
            .layers
            .iter()
            .filter(|layer| layer.role == role)
            .map(|layer| layer.id.as_str())
            .collect::<BTreeSet<_>>();
        review
            .connectivity
            .iter()
            .find(|semantic| {
                semantic.pin.as_deref() == Some(pin)
                    && review.features.iter().any(|feature| {
                        feature.id == semantic.feature_id
                            && layer_ids.contains(feature.layer_id.as_str())
                    })
            })
            .unwrap()
            .feature_id
            .clone()
    }

    fn role_aperture_id(review: &FabricationReview, role: LayerRole) -> String {
        let feature_id = role_feature_id(review, role, "1");
        let Geometry::Flash(flash) = &review
            .features
            .iter()
            .find(|feature| feature.id == feature_id)
            .unwrap()
            .geometry
        else {
            unreachable!()
        };
        flash.aperture_id.clone()
    }

    fn family_result(review: &FabricationReview, family: &str) -> (Vec<Finding>, Coverage) {
        family_result_with_schematic(review, None, family)
    }

    fn paste_result(
        review: &FabricationReview,
        schematic: &SchematicReview,
    ) -> (Vec<Finding>, Coverage) {
        family_result_with_schematic(review, Some(schematic), PASTE_MASK_FAMILY)
    }

    fn family_result_with_schematic(
        review: &FabricationReview,
        schematic: Option<&SchematicReview>,
        family: &str,
    ) -> (Vec<Finding>, Coverage) {
        let (findings, coverage) = fabrication_families(
            review,
            schematic,
            ManufacturingDeadline::from_timeout(Duration::from_secs(30)),
        );
        (
            findings
                .into_iter()
                .filter(|finding| finding.id.starts_with(family))
                .collect(),
            coverage
                .into_iter()
                .find(|coverage| coverage.id == family)
                .unwrap(),
        )
    }

    #[test]
    fn copper_edge_cutout_mutations_order_arithmetic_and_resources_fail_closed() {
        let valid = distance_fixture_review();
        assert_eq!(
            family_result(&valid, COPPER_EDGE_FAMILY).1.status,
            CoverageStatus::Passed
        );

        let mut round_flash = valid.clone();
        let copper_index =
            round_flash
                .features
                .iter()
                .position(|feature| {
                    round_flash.layers.iter().any(|layer| {
                        layer.id == feature.layer_id && layer.role == LayerRole::Copper
                    }) && matches!(feature.geometry, Geometry::Line(_))
                })
                .unwrap();
        let aperture_id = round_flash
            .apertures
            .iter()
            .find(|aperture| {
                aperture.document_id == round_flash.features[copper_index].document_id
                    && aperture.shape == ApertureShape::Circle
            })
            .unwrap()
            .id
            .clone();
        let position = match &round_flash.features[copper_index].geometry {
            Geometry::Line(line) => line.start,
            _ => unreachable!(),
        };
        round_flash.features[copper_index].geometry =
            Geometry::Flash(crate::fabrication::CanonicalFlash {
                position,
                aperture_id,
            });
        assert_eq!(
            family_result(&round_flash, COPPER_EDGE_FAMILY).1.status,
            CoverageStatus::Passed
        );
        assert_eq!(
            family_result(&round_flash, COPPER_CLEARANCE_FAMILY)
                .1
                .status,
            CoverageStatus::Passed
        );

        let mut cutout = valid.clone();
        let exterior_id = cutout.profile.as_ref().unwrap().contour_feature_ids[0].clone();
        let exterior = cutout
            .features
            .iter()
            .find(|feature| feature.id == exterior_id)
            .unwrap()
            .clone();
        let mut feature = exterior;
        feature.id = "cutout-plan07-05".into();
        feature.polarity = LayerPolarity::Clear;
        feature.geometry = Geometry::Contour(outline_rectangle(4_000_000_000, 6_000_000_000));
        cutout
            .profile
            .as_mut()
            .unwrap()
            .cutout_feature_ids
            .push(feature.id.clone());
        cutout.features.push(feature);
        let copper_layer_ids = cutout
            .layers
            .iter()
            .filter(|layer| layer.role == LayerRole::Copper)
            .map(|layer| layer.id.as_str())
            .collect::<BTreeSet<_>>();
        let copper = cutout
            .features
            .iter_mut()
            .find(|feature| {
                copper_layer_ids.contains(feature.layer_id.as_str())
                    && matches!(feature.geometry, Geometry::Line(_))
            })
            .unwrap();
        let Geometry::Line(line) = &mut copper.geometry else {
            unreachable!()
        };
        line.start.x = Picometres(4_500_000_000);
        line.end.x = Picometres(5_500_000_000);
        line.start.y = Picometres(3_800_000_000);
        line.end.y = Picometres(3_800_000_000);
        let (findings, coverage) = family_result(&cutout, COPPER_EDGE_FAMILY);
        assert_eq!(coverage.status, CoverageStatus::Attention);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].evidence.contains("boundary=cutout"));

        let mut reordered = valid.clone();
        reordered.features.reverse();
        reordered.layers.reverse();
        assert_eq!(
            serde_json::to_value(family_result(&valid, COPPER_EDGE_FAMILY)).unwrap(),
            serde_json::to_value(family_result(&reordered, COPPER_EDGE_FAMILY)).unwrap()
        );

        for state in [
            CapabilityState::Partial,
            CapabilityState::NotProvided,
            CapabilityState::Unsupported,
            CapabilityState::Failed,
            CapabilityState::Stale,
            CapabilityState::Omitted,
        ] {
            let mut mutated = valid.clone();
            mutated
                .capabilities
                .records
                .iter_mut()
                .find(|record| record.id == CapabilityId::Profile)
                .unwrap()
                .state = state;
            assert_eq!(
                family_result(&mutated, COPPER_EDGE_FAMILY).1.status,
                CoverageStatus::NotRun,
                "{state:?}"
            );
        }
        let mut direct = valid.clone();
        direct
            .constraints
            .iter_mut()
            .find(|constraint| {
                constraint.kind == ConstraintKind::Other
                    && constraint
                        .declared_value
                        .as_deref()
                        .is_some_and(|value| value.starts_with("dfm.copper-edge.v1="))
            })
            .unwrap()
            .authority = Authority::FileContent;
        assert_eq!(
            family_result(&direct, COPPER_EDGE_FAMILY).1.status,
            CoverageStatus::NotRun
        );
        let mut forged_adapter = valid.clone();
        declaration_document(&forged_adapter).unwrap().unwrap();
        forged_adapter
            .documents
            .iter_mut()
            .find(|document| document.adapter == DECLARATION_ADAPTER)
            .unwrap()
            .adapter_version = "forged-declaration-adapter".into();
        assert_eq!(
            family_result(&forged_adapter, COPPER_EDGE_FAMILY).1.status,
            CoverageStatus::NotRun
        );
        let mut duplicate_capability = valid.clone();
        let profile = duplicate_capability
            .capabilities
            .records
            .iter()
            .find(|record| record.id == CapabilityId::Profile)
            .unwrap()
            .clone();
        duplicate_capability.capabilities.records.push(profile);
        assert_eq!(
            family_result(&duplicate_capability, COPPER_EDGE_FAMILY)
                .1
                .status,
            CoverageStatus::NotRun
        );
        let mut omitted = valid.clone();
        omitted.omissions.push(crate::fabrication::Omission {
            id: "edge-omission".into(),
            kind: crate::fabrication::OmissionKind::MissingSemanticRecord,
            affected_capabilities: vec![CapabilityId::Profile],
            provenance: omitted.features[0].provenance.clone(),
            detail: "edge geometry omitted".into(),
        });
        assert_eq!(
            family_result(&omitted, COPPER_EDGE_FAMILY).1.status,
            CoverageStatus::NotRun
        );
        let mut unsupported = valid.clone();
        let copper = unsupported
            .features
            .iter_mut()
            .find(|feature| {
                copper_layer_ids.contains(feature.layer_id.as_str())
                    && matches!(feature.geometry, Geometry::Line(_))
            })
            .unwrap();
        copper.geometry = Geometry::Point(CanonicalPoint::new(1_000_000_000, 1_000_000_000));
        assert_eq!(
            family_result(&unsupported, COPPER_EDGE_FAMILY).1.status,
            CoverageStatus::NotRun
        );

        let extreme = AxisPrimitive {
            start: CanonicalPoint::new(i64::MIN, i64::MIN),
            end: CanonicalPoint::new(i64::MIN + 1, i64::MIN),
            radius: Picometres(0),
        };
        let opposite = AxisPrimitive {
            start: CanonicalPoint::new(i64::MAX - 1, i64::MAX),
            end: CanonicalPoint::new(i64::MAX, i64::MAX),
            radius: Picometres(0),
        };
        assert!(axis_distance(extreme, opposite).is_err());

        let provenance = outline_test_provenance(1);
        let primitive = LocatedPrimitive {
            owner_id: "resource",
            segment: 0,
            layer_id: "layer",
            primitive: AxisPrimitive {
                start: CanonicalPoint::new(0, 0),
                end: CanonicalPoint::new(1, 0),
                radius: Picometres(0),
            },
            resolution: Picometres(1),
            provenance: &provenance,
            boundary_kind: Some(BoundaryKind::Exterior),
        };
        assert!(
            nearest_axis_pair(
                vec![primitive; 1_001],
                vec![primitive; 1_001],
                ManufacturingDeadline::from_timeout(Duration::from_secs(30)),
                "dfm-distance-resource-test",
            )
            .is_err()
        );
        assert!(
            nearest_axis_pair(
                vec![primitive],
                vec![primitive],
                ManufacturingDeadline::from_timeout(Duration::ZERO),
                "dfm-distance-deadline-test",
            )
            .is_err()
        );
    }

    #[test]
    fn copper_clearance_mutations_order_and_layer_local_resource_bound_fail_closed() {
        let valid = distance_fixture_review();
        let valid_result = family_result(&valid, COPPER_CLEARANCE_FAMILY);
        assert_eq!(
            valid_result.1.status,
            CoverageStatus::Passed,
            "{:?} connectivity={:?}",
            valid_result.1,
            valid
                .capabilities
                .records
                .iter()
                .filter(|record| record.id == CapabilityId::Connectivity)
                .collect::<Vec<_>>()
        );
        let mut reordered = valid.clone();
        reordered.features.reverse();
        reordered.connectivity.reverse();
        reordered.layers.reverse();
        assert_eq!(
            serde_json::to_value(family_result(&valid, COPPER_CLEARANCE_FAMILY)).unwrap(),
            serde_json::to_value(family_result(&reordered, COPPER_CLEARANCE_FAMILY)).unwrap()
        );

        let mut tie = valid.clone();
        let bottom_layer = tie
            .layers
            .iter()
            .find(|layer| layer.side == crate::fabrication::LayerSide::Bottom)
            .unwrap()
            .id
            .clone();
        let tied_feature = tie
            .features
            .iter()
            .find(|feature| {
                feature.layer_id == bottom_layer
                    && matches!(
                        &feature.geometry,
                        Geometry::Line(line) if line.start.y.0 == 1_400_000_000
                    )
            })
            .unwrap()
            .id
            .clone();
        tie.connectivity
            .iter_mut()
            .find(|semantic| semantic.feature_id == tied_feature)
            .unwrap()
            .net = Some("opaque-second-net".into());
        let tied = family_result(&tie, COPPER_CLEARANCE_FAMILY);
        let mut tie_reordered = tie.clone();
        tie_reordered.features.reverse();
        tie_reordered.connectivity.reverse();
        tie_reordered.layers.reverse();
        assert_eq!(
            serde_json::to_value(tied).unwrap(),
            serde_json::to_value(family_result(&tie_reordered, COPPER_CLEARANCE_FAMILY)).unwrap()
        );

        let mut direct = valid.clone();
        direct
            .constraints
            .iter_mut()
            .find(|constraint| constraint.kind == ConstraintKind::MinimumClearance)
            .unwrap()
            .authority = Authority::FileContent;
        assert_eq!(
            family_result(&direct, COPPER_CLEARANCE_FAMILY).1.status,
            CoverageStatus::NotRun
        );
        let mut duplicate_capability = valid.clone();
        let connectivity = duplicate_capability
            .capabilities
            .records
            .iter()
            .find(|record| record.id == CapabilityId::Connectivity)
            .unwrap()
            .clone();
        duplicate_capability.capabilities.records.push(connectivity);
        assert_eq!(
            family_result(&duplicate_capability, COPPER_CLEARANCE_FAMILY)
                .1
                .status,
            CoverageStatus::NotRun
        );
        let mut duplicate = valid.clone();
        let copper_id = duplicate
            .connectivity
            .iter()
            .find(|semantic| semantic.net.as_deref() == Some("VCC"))
            .unwrap()
            .feature_id
            .clone();
        let semantic = duplicate
            .connectivity
            .iter()
            .find(|semantic| semantic.feature_id == copper_id)
            .unwrap()
            .clone();
        duplicate.connectivity.push(semantic);
        assert_eq!(
            family_result(&duplicate, COPPER_CLEARANCE_FAMILY).1.status,
            CoverageStatus::NotRun
        );

        for state in [
            CapabilityState::Partial,
            CapabilityState::NotProvided,
            CapabilityState::Unsupported,
            CapabilityState::Failed,
            CapabilityState::Stale,
            CapabilityState::Omitted,
        ] {
            let mut mutated = valid.clone();
            mutated
                .capabilities
                .records
                .iter_mut()
                .find(|record| record.id == CapabilityId::Connectivity)
                .unwrap()
                .state = state;
            assert_eq!(
                family_result(&mutated, COPPER_CLEARANCE_FAMILY).1.status,
                CoverageStatus::NotRun,
                "{state:?}"
            );
        }

        let provenance = outline_test_provenance(1);
        let left_semantics = ObjectSemantics {
            feature_id: "left".into(),
            net: Some("opaque-a".into()),
            component: Some("U1".into()),
            pin: Some("1".into()),
            provenance: provenance.clone(),
        };
        let right_semantics = ObjectSemantics {
            feature_id: "right".into(),
            net: Some("opaque-b".into()),
            component: Some("U2".into()),
            pin: Some("1".into()),
            provenance: provenance.clone(),
        };
        let geometry = LocatedPrimitive {
            owner_id: "resource",
            segment: 0,
            layer_id: "same-physical-layer",
            primitive: AxisPrimitive {
                start: CanonicalPoint::new(0, 0),
                end: CanonicalPoint::new(1, 0),
                radius: Picometres(0),
            },
            resolution: Picometres(1),
            provenance: &provenance,
            boundary_kind: None,
        };
        assert!(
            nearest_clearance_pair(
                vec![ConnectedPrimitive {
                    geometry,
                    semantics: &left_semantics,
                }],
                ManufacturingDeadline::from_timeout(Duration::ZERO),
            )
            .is_err(),
            "an expired no-pair scan must not become a clean pass"
        );
        let mut dense = vec![
            ConnectedPrimitive {
                geometry,
                semantics: &left_semantics,
            };
            1_001
        ];
        dense.extend(vec![
            ConnectedPrimitive {
                geometry,
                semantics: &right_semantics,
            };
            1_001
        ]);
        assert!(
            nearest_clearance_pair(
                dense,
                ManufacturingDeadline::from_timeout(Duration::from_secs(30)),
            )
            .is_err()
        );
    }

    #[test]
    fn annular_association_mutations_order_and_deadline_fail_closed() {
        let valid = annular_fixture_review();
        assert_eq!(
            family_result(&valid, ANNULAR_RING_FAMILY).1.status,
            CoverageStatus::Passed
        );
        let mut reordered = valid.clone();
        reordered.features.reverse();
        reordered.layers.reverse();
        reordered.pad_hole_associations.reverse();
        assert_eq!(
            serde_json::to_value(family_result(&valid, ANNULAR_RING_FAMILY)).unwrap(),
            serde_json::to_value(family_result(&reordered, ANNULAR_RING_FAMILY)).unwrap()
        );

        let mut tie = valid.clone();
        let source_hole = tie
            .features
            .iter()
            .find(|feature| feature.id == tie.pad_hole_associations[0].hole_id)
            .unwrap()
            .clone();
        let mut second_hole = source_hole.clone();
        second_hole.id = "feature-v1-second-native-pad-hole".into();
        let mut second_semantic = tie
            .connectivity
            .iter()
            .find(|semantic| semantic.feature_id == source_hole.id)
            .unwrap()
            .clone();
        second_semantic.feature_id = second_hole.id.clone();
        let mut second_association = tie.pad_hole_associations[0].clone();
        second_association.id = "pad-hole-association-v1-second".into();
        second_association.pad_id = "pad-v1-second".into();
        second_association.hole_id = second_hole.id.clone();
        tie.features.push(second_hole);
        tie.connectivity.push(second_semantic);
        tie.pad_hole_associations.push(second_association);
        let threshold = tie
            .constraints
            .iter_mut()
            .find(|constraint| constraint.kind == ConstraintKind::MinimumAnnularRing)
            .unwrap();
        threshold.value = Some(Picometres(201_000_000));
        threshold.declared_value = Some("minimum_annular_ring=0.201 mm;applies=board".into());
        let tied = family_result(&tie, ANNULAR_RING_FAMILY);
        assert_eq!(tied.0.len(), 4);
        let mut tie_reordered = tie.clone();
        tie_reordered.features.reverse();
        tie_reordered.connectivity.reverse();
        tie_reordered.pad_hole_associations.reverse();
        assert_eq!(
            serde_json::to_value(tied).unwrap(),
            serde_json::to_value(family_result(&tie_reordered, ANNULAR_RING_FAMILY)).unwrap()
        );

        let mut forged_adapter = valid.clone();
        forged_adapter
            .documents
            .iter_mut()
            .find(|document| document.adapter == "ratemypcb-kicad-source")
            .unwrap()
            .adapter_version = "forged-native-adapter".into();
        assert_eq!(
            family_result(&forged_adapter, ANNULAR_RING_FAMILY).1.status,
            CoverageStatus::NotRun
        );
        let mut direct = valid.clone();
        direct
            .constraints
            .iter_mut()
            .find(|constraint| constraint.kind == ConstraintKind::MinimumAnnularRing)
            .unwrap()
            .authority = Authority::FileContent;
        assert_eq!(
            family_result(&direct, ANNULAR_RING_FAMILY).1.status,
            CoverageStatus::NotRun
        );
        let mut duplicate_capability = valid.clone();
        let drills = duplicate_capability
            .capabilities
            .records
            .iter()
            .find(|record| record.id == CapabilityId::Drills)
            .unwrap()
            .clone();
        duplicate_capability.capabilities.records.push(drills);
        assert_eq!(
            family_result(&duplicate_capability, ANNULAR_RING_FAMILY)
                .1
                .status,
            CoverageStatus::NotRun
        );
        let mut absent = valid.clone();
        absent.pad_hole_associations.clear();
        assert_eq!(
            family_result(&absent, ANNULAR_RING_FAMILY).1.status,
            CoverageStatus::NotRun
        );
        let mut duplicate = valid.clone();
        duplicate
            .pad_hole_associations
            .push(duplicate.pad_hole_associations[0].clone());
        assert_eq!(
            family_result(&duplicate, ANNULAR_RING_FAMILY).1.status,
            CoverageStatus::NotRun
        );
        let mut dangling = valid.clone();
        dangling.pad_hole_associations[0].hole_id = "feature-v1-dangling".into();
        assert_eq!(
            family_result(&dangling, ANNULAR_RING_FAMILY).1.status,
            CoverageStatus::NotRun
        );
        let mut plating = valid.clone();
        plating.pad_hole_associations[0].plating = Plating::NonPlated;
        assert_eq!(
            family_result(&plating, ANNULAR_RING_FAMILY).1.status,
            CoverageStatus::NotRun
        );
        let mut span = valid.clone();
        span.tools
            .iter_mut()
            .find(|tool| tool.id == span.pad_hole_associations[0].tool_id)
            .unwrap()
            .span = None;
        assert_eq!(
            family_result(&span, ANNULAR_RING_FAMILY).1.status,
            CoverageStatus::NotRun
        );
        let mut geometry = valid.clone();
        geometry.pad_hole_associations[0].pad_geometry = Geometry::Point(CanonicalPoint::new(0, 0));
        assert_eq!(
            family_result(&geometry, ANNULAR_RING_FAMILY).1.status,
            CoverageStatus::NotRun
        );
        assert_eq!(
            annular_ring(&valid, ManufacturingDeadline::from_timeout(Duration::ZERO))
                .1
                .status,
            CoverageStatus::NotRun
        );
        assert_eq!(
            family_result(&distance_fixture_review(), ANNULAR_RING_FAMILY)
                .1
                .status,
            CoverageStatus::NotRun,
            "Gerber/XNC package facts cannot create pad-hole authority"
        );
    }

    #[test]
    fn mask_sliver_exact_boundaries_intent_order_and_resources_fail_closed() {
        let valid = mask_paste_fixture_review();
        let result = family_result(&valid, MASK_SLIVER_FAMILY);
        assert_eq!(result.1.status, CoverageStatus::Passed, "{:?}", result.1);
        assert!(result.1.evidence.contains("observed=200000000pm"));
        assert!(result.1.evidence.contains("threshold=80000000pm"));

        let exact = mask_paste_fixture_review_with_thresholds("0.200", "0.05");
        assert_eq!(
            family_result(&exact, MASK_SLIVER_FAMILY).1.status,
            CoverageStatus::Passed
        );
        let violation = mask_paste_fixture_review_with_thresholds("0.201", "0.05");
        let violation = family_result(&violation, MASK_SLIVER_FAMILY);
        assert_eq!(violation.1.status, CoverageStatus::Attention);
        assert_eq!(violation.0.len(), 1);
        assert_eq!(violation.0[0].gate_impact, GateImpact::EvidenceOnly);
        assert!(violation.0[0].evidence.contains("delta=1000000pm"));
        assert!(violation.0[0].evidence.contains("left_source="));
        assert!(violation.0[0].evidence.contains("right_intent="));

        let mut reordered = valid.clone();
        reordered.features.reverse();
        reordered.layers.reverse();
        reordered.connectivity.reverse();
        assert_eq!(
            serde_json::to_value(family_result(&valid, MASK_SLIVER_FAMILY)).unwrap(),
            serde_json::to_value(family_result(&reordered, MASK_SLIVER_FAMILY)).unwrap()
        );

        let first = role_feature_id(&valid, LayerRole::SolderMask, "1");
        let second = role_feature_id(&valid, LayerRole::SolderMask, "2");
        let mut ambiguous_merge = valid.clone();
        ambiguous_merge
            .features
            .retain(|feature| feature.id != second);
        let mut second_intent = ambiguous_merge
            .connectivity
            .iter()
            .find(|semantic| semantic.feature_id == second)
            .unwrap()
            .clone();
        ambiguous_merge
            .connectivity
            .retain(|semantic| semantic.feature_id != second);
        second_intent.feature_id = first.clone();
        ambiguous_merge.connectivity.push(second_intent);
        assert_eq!(
            family_result(&ambiguous_merge, MASK_SLIVER_FAMILY).1.status,
            CoverageStatus::NotRun,
            "component/pin association alone is not deliberate merged-opening intent"
        );

        let mut single_opening = valid.clone();
        single_opening
            .features
            .retain(|feature| feature.id != second);
        single_opening
            .connectivity
            .retain(|semantic| semantic.feature_id != second);
        let single_opening = family_result(&single_opening, MASK_SLIVER_FAMILY);
        assert_eq!(single_opening.1.status, CoverageStatus::Passed);
        assert!(single_opening.1.evidence.contains("pairs=0"));

        let mut overlap = valid.clone();
        let first_position = match &overlap
            .features
            .iter()
            .find(|feature| feature.id == first)
            .unwrap()
            .geometry
        {
            Geometry::Flash(flash) => flash.position,
            _ => unreachable!(),
        };
        let Geometry::Flash(flash) = &mut overlap
            .features
            .iter_mut()
            .find(|feature| feature.id == second)
            .unwrap()
            .geometry
        else {
            unreachable!()
        };
        flash.position = first_position;
        assert_eq!(
            family_result(&overlap, MASK_SLIVER_FAMILY).1.status,
            CoverageStatus::NotRun
        );

        let mut missing_intent = valid.clone();
        missing_intent
            .connectivity
            .retain(|semantic| semantic.feature_id != first);
        assert_eq!(
            family_result(&missing_intent, MASK_SLIVER_FAMILY).1.status,
            CoverageStatus::NotRun
        );
        let mut polarity = valid.clone();
        polarity
            .layers
            .iter_mut()
            .find(|layer| layer.role == LayerRole::SolderMask)
            .unwrap()
            .polarity = LayerPolarity::Unknown;
        assert_eq!(
            family_result(&polarity, MASK_SLIVER_FAMILY).1.status,
            CoverageStatus::NotRun
        );
        let mut shape = valid.clone();
        shape
            .features
            .iter_mut()
            .find(|feature| feature.id == first)
            .unwrap()
            .geometry = Geometry::Point(CanonicalPoint::new(0, 0));
        assert_eq!(
            family_result(&shape, MASK_SLIVER_FAMILY).1.status,
            CoverageStatus::NotRun
        );

        let mut direct = valid.clone();
        direct
            .constraints
            .iter_mut()
            .find(|constraint| {
                constraint
                    .declared_value
                    .as_deref()
                    .is_some_and(|value| value.starts_with(MASK_SLIVER_FAMILY))
            })
            .unwrap()
            .authority = Authority::FileContent;
        assert_eq!(
            family_result(&direct, MASK_SLIVER_FAMILY).1.status,
            CoverageStatus::NotRun
        );
        let mut duplicate = valid.clone();
        let threshold = duplicate
            .constraints
            .iter()
            .find(|constraint| {
                constraint
                    .declared_value
                    .as_deref()
                    .is_some_and(|value| value.starts_with(MASK_SLIVER_FAMILY))
            })
            .unwrap()
            .clone();
        duplicate.constraints.push(threshold);
        assert_eq!(
            family_result(&duplicate, MASK_SLIVER_FAMILY).1.status,
            CoverageStatus::NotRun
        );
        let mut inapplicable = valid.clone();
        inapplicable
            .constraints
            .iter_mut()
            .find(|constraint| {
                constraint
                    .declared_value
                    .as_deref()
                    .is_some_and(|value| value.starts_with(MASK_SLIVER_FAMILY))
            })
            .unwrap()
            .declared_value = Some(format!(
            "{MASK_SLIVER_FAMILY}=0.080 mm;applies=layer:F.Mask"
        ));
        assert_eq!(
            family_result(&inapplicable, MASK_SLIVER_FAMILY).1.status,
            CoverageStatus::NotRun
        );
        for state in [
            CapabilityState::Partial,
            CapabilityState::NotProvided,
            CapabilityState::Unsupported,
            CapabilityState::Failed,
            CapabilityState::Stale,
            CapabilityState::Omitted,
        ] {
            let mut mutated = valid.clone();
            mutated
                .capabilities
                .records
                .iter_mut()
                .find(|record| record.id == CapabilityId::Polarity)
                .unwrap()
                .state = state;
            assert_eq!(
                family_result(&mutated, MASK_SLIVER_FAMILY).1.status,
                CoverageStatus::NotRun,
                "{state:?}"
            );
        }
        let (openings, _, _) = resolved_role_primitives(
            &valid,
            LayerRole::SolderMask,
            LayerPolarity::Negative,
            ManufacturingDeadline::from_timeout(Duration::from_secs(5)),
            "dfm-mask-sliver-test",
        )
        .unwrap();
        assert!(
            nearest_same_layer_pair(
                vec![openings[0]; MAX_DISTANCE_PRIMITIVES + 1],
                ManufacturingDeadline::from_timeout(Duration::from_secs(5)),
                "dfm-mask-sliver-resource-test",
            )
            .is_err()
        );
        assert_eq!(
            mask_sliver(&valid, ManufacturingDeadline::from_timeout(Duration::ZERO))
                .1
                .status,
            CoverageStatus::NotRun
        );
    }

    #[test]
    fn paste_mask_exact_set_relationship_boundaries_and_mutations_fail_closed() {
        let valid = mask_paste_fixture_review();
        let schematic = fitted_schematic_review();
        let result = paste_result(&valid, &schematic);
        assert_eq!(result.1.status, CoverageStatus::Attention, "{:?}", result.1);
        assert_eq!(result.0.len(), 2);
        assert!(result.0.iter().all(|finding| {
            finding.gate_impact == GateImpact::EvidenceOnly
                && finding.evidence.contains("relation=paste_subset_of_mask")
                && finding.evidence.contains("mask_source=")
                && finding.evidence.contains("paste_source=")
                && finding.evidence.contains("placement_source=")
                && finding.evidence.contains("fitted_source=")
        }));

        let exact = mask_paste_fixture_review_with_thresholds("0.08", "0.100");
        assert_eq!(
            paste_result(&exact, &schematic).1.status,
            CoverageStatus::Passed
        );
        let mut equal = exact.clone();
        let paste_aperture = role_aperture_id(&equal, LayerRole::Paste);
        equal
            .apertures
            .iter_mut()
            .find(|aperture| aperture.id == paste_aperture)
            .unwrap()
            .dimensions[0] = Picometres(1_000_000_000);
        let equal = paste_result(&equal, &schematic);
        assert_eq!(equal.1.status, CoverageStatus::Passed);
        assert!(equal.1.evidence.contains("relation=equal"));

        let mut expansion = exact.clone();
        let paste_aperture = role_aperture_id(&expansion, LayerRole::Paste);
        expansion
            .apertures
            .iter_mut()
            .find(|aperture| aperture.id == paste_aperture)
            .unwrap()
            .dimensions[0] = Picometres(1_200_000_000);
        let expansion_result = paste_result(&expansion, &schematic);
        assert_eq!(expansion_result.1.status, CoverageStatus::Passed);
        assert!(
            expansion_result
                .1
                .evidence
                .contains("relation=mask_subset_of_paste")
        );
        let mut expansion_violation = mask_paste_fixture_review_with_thresholds("0.08", "0.099");
        let paste_aperture = role_aperture_id(&expansion_violation, LayerRole::Paste);
        expansion_violation
            .apertures
            .iter_mut()
            .find(|aperture| aperture.id == paste_aperture)
            .unwrap()
            .dimensions[0] = Picometres(1_200_000_000);
        assert_eq!(
            paste_result(&expansion_violation, &schematic).1.status,
            CoverageStatus::Attention
        );

        let mut reordered = valid.clone();
        reordered.features.reverse();
        reordered.layers.reverse();
        reordered.connectivity.reverse();
        reordered.x2_attributes.reverse();
        reordered.assembly.placements.reverse();
        assert_eq!(
            serde_json::to_value(paste_result(&valid, &schematic)).unwrap(),
            serde_json::to_value(paste_result(&reordered, &schematic)).unwrap()
        );

        let mask_first = role_feature_id(&valid, LayerRole::SolderMask, "1");
        let paste_first = role_feature_id(&valid, LayerRole::Paste, "1");
        let paste_second = role_feature_id(&valid, LayerRole::Paste, "2");
        let mut no_placement = valid.clone();
        no_placement.assembly.placements.clear();
        assert_eq!(
            paste_result(&no_placement, &schematic).1.status,
            CoverageStatus::NotRun
        );
        let mut unknown_side = valid.clone();
        unknown_side.assembly.placements[0].side = LayerSide::Unknown;
        assert_eq!(
            paste_result(&unknown_side, &schematic).1.status,
            CoverageStatus::NotRun
        );

        let mut dnp = schematic.clone();
        dnp.occurrences
            .iter_mut()
            .find(|occurrence| occurrence.reference.as_deref() == Some("R1"))
            .unwrap()
            .facts
            .iter_mut()
            .find(|fact| fact.name == "dnp")
            .unwrap()
            .value = "true".into();
        assert_eq!(
            paste_result(&valid, &dnp).1.status,
            CoverageStatus::NotRun,
            "a placement is not fitted-state authority"
        );
        let mut unknown_fitted = schematic.clone();
        unknown_fitted
            .occurrences
            .iter_mut()
            .find(|occurrence| occurrence.reference.as_deref() == Some("R1"))
            .unwrap()
            .facts
            .retain(|fact| fact.name != "dnp");
        assert_eq!(
            paste_result(&valid, &unknown_fitted).1.status,
            CoverageStatus::NotRun
        );

        let mut no_smd_authority = valid.clone();
        for attribute in &mut no_smd_authority.x2_attributes {
            if attribute.kind == X2AttributeKind::ApertureFunction {
                attribute.values = vec!["Conductor".into()];
            }
        }
        assert!(no_smd_authority.pad_hole_associations.is_empty());
        assert_eq!(
            paste_result(&no_smd_authority, &schematic).1.status,
            CoverageStatus::NotRun,
            "absence from the pad-hole subset is not positive SMD authority"
        );

        let mut missing_association = valid.clone();
        missing_association
            .connectivity
            .retain(|semantic| semantic.feature_id != paste_first);
        assert_eq!(
            paste_result(&missing_association, &schematic).1.status,
            CoverageStatus::NotRun
        );
        let mut omission = valid.clone();
        omission
            .features
            .retain(|feature| feature.id != paste_second);
        omission
            .connectivity
            .retain(|semantic| semantic.feature_id != paste_second);
        assert_eq!(
            paste_result(&omission, &schematic).1.status,
            CoverageStatus::NotRun
        );
        let mut windowpane = valid.clone();
        let mut extra = windowpane
            .features
            .iter()
            .find(|feature| feature.id == paste_first)
            .unwrap()
            .clone();
        extra.id = "plan07-06-windowpane-extra".into();
        let mut intent = windowpane
            .connectivity
            .iter()
            .find(|semantic| semantic.feature_id == paste_first)
            .unwrap()
            .clone();
        intent.feature_id = extra.id.clone();
        windowpane.features.push(extra);
        windowpane.connectivity.push(intent);
        assert_eq!(
            paste_result(&windowpane, &schematic).1.status,
            CoverageStatus::NotRun
        );

        let mut pin_in_paste = valid.clone();
        let annular = annular_fixture_review();
        let association = annular.pad_hole_associations[0].clone();
        let mut semantic = annular
            .connectivity
            .iter()
            .find(|semantic| semantic.feature_id == association.hole_id)
            .unwrap()
            .clone();
        semantic.component = Some("R1".into());
        semantic.pin = Some("1".into());
        pin_in_paste.pad_hole_associations.push(association);
        pin_in_paste.connectivity.push(semantic);
        assert_eq!(
            paste_result(&pin_in_paste, &schematic).1.status,
            CoverageStatus::NotRun
        );

        let mut offset = valid.clone();
        let Geometry::Flash(flash) = &mut offset
            .features
            .iter_mut()
            .find(|feature| feature.id == paste_first)
            .unwrap()
            .geometry
        else {
            unreachable!()
        };
        flash.position.x.0 += 1_000_000;
        assert_eq!(
            paste_result(&offset, &schematic).1.status,
            CoverageStatus::NotRun
        );
        let mut unsupported = valid.clone();
        unsupported
            .features
            .iter_mut()
            .find(|feature| feature.id == mask_first)
            .unwrap()
            .geometry = Geometry::Point(CanonicalPoint::new(0, 0));
        assert_eq!(
            paste_result(&unsupported, &schematic).1.status,
            CoverageStatus::NotRun
        );

        let mut direct = valid.clone();
        direct
            .constraints
            .iter_mut()
            .find(|constraint| {
                constraint
                    .declared_value
                    .as_deref()
                    .is_some_and(|value| value.starts_with(PASTE_MASK_FAMILY))
            })
            .unwrap()
            .authority = Authority::FileContent;
        assert_eq!(
            paste_result(&direct, &schematic).1.status,
            CoverageStatus::NotRun
        );
        let mut duplicate = valid.clone();
        let threshold = duplicate
            .constraints
            .iter()
            .find(|constraint| {
                constraint
                    .declared_value
                    .as_deref()
                    .is_some_and(|value| value.starts_with(PASTE_MASK_FAMILY))
            })
            .unwrap()
            .clone();
        duplicate.constraints.push(threshold);
        assert_eq!(
            paste_result(&duplicate, &schematic).1.status,
            CoverageStatus::NotRun
        );
        let off_grid = mask_paste_fixture_review_with_thresholds("0.08", "0.1000005");
        assert_eq!(
            paste_result(&off_grid, &schematic).1.status,
            CoverageStatus::NotRun
        );
        for state in [
            CapabilityState::Partial,
            CapabilityState::NotProvided,
            CapabilityState::Unsupported,
            CapabilityState::Failed,
            CapabilityState::Stale,
            CapabilityState::Omitted,
        ] {
            let mut mutated = valid.clone();
            mutated
                .capabilities
                .records
                .iter_mut()
                .find(|record| record.id == CapabilityId::Assembly)
                .unwrap()
                .state = state;
            assert_eq!(
                paste_result(&mutated, &schematic).1.status,
                CoverageStatus::NotRun,
                "{state:?}"
            );
        }
        assert_eq!(
            paste_mask_relationship(
                &valid,
                Some(&schematic),
                ManufacturingDeadline::from_timeout(Duration::ZERO),
            )
            .1
            .status,
            CoverageStatus::NotRun
        );
    }

    #[test]
    fn courtyard_native_preserves_active_excluded_unknown_and_failure_semantics() {
        let marker = |excluded| crate::NativeViolation {
            id: "courtyard-marker".into(),
            group: "violations".into(),
            violation_type: "courtyards_overlap".into(),
            severity: "error".into(),
            description: "overlap".into(),
            items: vec![],
            excluded,
            comment: None,
            sheet_path: None,
            sheet_uuid_path: None,
            structural_location: "channel=violations;sheet=root;items=footprints;index=0".into(),
        };
        let base = crate::NativeDrc {
            status: "completed".into(),
            tool: "kicad-cli".into(),
            version: Some("10.0.5".into()),
            report_version: Some("10.0.5".into()),
            finding_count: 1,
            excluded_count: 0,
            unknown_exclusion_count: 0,
            note: "completed".into(),
            source: Some("board.kicad_pcb".into()),
            date: Some("2026-08-31T00:00:00Z".into()),
            included_severities: vec!["error".into(), "warning".into(), "exclusion".into()],
            ignored_checks: vec![],
            violations: vec![marker(Some(false))],
        };
        for (excluded, status, finding_count) in [
            (Some(false), CoverageStatus::Attention, 1),
            (Some(true), CoverageStatus::Passed, 0),
            (None, CoverageStatus::Unknown, 0),
        ] {
            let mut report = base.clone();
            report.violations = vec![marker(excluded)];
            let mut review = annular_fixture_review();
            review.assembly.native_courtyard =
                Some(crate::fabrication::normalize_native_courtyard_report(&report).unwrap());
            review.refresh_digests().unwrap();
            review.validate().unwrap();
            let (findings, coverage) = courtyard_native(&review);
            assert_eq!(coverage.status, status);
            assert_eq!(findings.len(), finding_count);
            assert!(findings.iter().all(|finding| {
                finding.gate_impact == GateImpact::EvidenceOnly
                    && finding.evidence.contains("version=10.0.5")
                    && finding.evidence.contains("location=")
            }));
        }
        let mut incomplete = base;
        incomplete.status = "not_run".into();
        let mut review = annular_fixture_review();
        review.assembly.native_courtyard =
            Some(crate::fabrication::normalize_native_courtyard_report(&incomplete).unwrap());
        review.refresh_digests().unwrap();
        assert_eq!(courtyard_native(&review).1.status, CoverageStatus::NotRun);
    }

    fn construction_design_review() -> FabricationReview {
        let artifact_digest = crate::sha256(b"plan-07-07-construction-design");
        let document_id = document_id(&artifact_digest, DocumentFormat::KicadPcb).unwrap();
        let document = ManufacturingDocument {
            id: document_id.clone(),
            virtual_path: "design/board.kicad_pcb".into(),
            artifact_digest: artifact_digest.clone(),
            format: DocumentFormat::KicadPcb,
            adapter: KICAD_MANUFACTURING_ADAPTER.into(),
            adapter_version: KICAD_MANUFACTURING_ADAPTER_VERSION.into(),
            parse_status: ParseStatus::Complete,
            numeric_format: None,
            metrics: DocumentMetrics {
                raw_bytes: 1_024,
                records: 64,
                lexical_tokens: 64,
                metadata_bytes: 128,
                max_line_bytes: 64,
                max_text_bytes: 64,
                max_numeric_bytes: 32,
                max_nesting: 4,
                max_aperture_nesting: 0,
            },
        };
        let provenance = |record| ManufacturingProvenance {
            document_id: document_id.clone(),
            artifact_digest: artifact_digest.clone(),
            producer: KICAD_MANUFACTURING_ADAPTER.into(),
            producer_version: KICAD_MANUFACTURING_ADAPTER_VERSION.into(),
            location: StructuralLocation {
                record,
                subrecord: None,
                byte_start: record,
                byte_end: record,
            },
            source_lexeme: None,
        };
        let layer = |name: &str,
                     side: LayerSide,
                     order: i32,
                     record: u64|
         -> crate::fabrication::ManufacturingLayer {
            let source = provenance(record);
            crate::fabrication::ManufacturingLayer {
                id: crate::fabrication::layer_id(
                    &document_id,
                    Some(name),
                    LayerRole::Copper,
                    side,
                    Some(order),
                    Authority::NativeSource,
                    &source.location,
                ),
                document_id: document_id.clone(),
                name: Some(name.into()),
                role: LayerRole::Copper,
                side,
                context: crate::fabrication::LayerContext::Board,
                polarity: LayerPolarity::Positive,
                order: Some(order),
                authority: Authority::NativeSource,
                provenance: source,
            }
        };
        let layers = vec![
            layer("L1", LayerSide::Top, 1, 1),
            layer("L2", LayerSide::Bottom, 2, 2),
        ];
        let construction_layers = layers
            .iter()
            .map(|layer| ConstructionLayer {
                layer_id: Some(layer.id.clone()),
                material: Some("FR-4".into()),
                thickness: Some(Picometres(35_000_000)),
                authority: Authority::NativeSource,
                provenance: layer.provenance.clone(),
            })
            .collect::<Vec<_>>();
        let constraint =
            |kind: ConstraintKind, value: Option<Picometres>, declared_value: &str, record: u64| {
                let source = provenance(record);
                ManufacturingConstraint {
                    id: constraint_id(&document_id, kind, &source.location),
                    kind,
                    value,
                    declared_value: Some(declared_value.into()),
                    authority: Authority::NativeSource,
                    provenance: source,
                }
            };
        let constraints = vec![
            constraint(
                ConstraintKind::FinishedThickness,
                Some(Picometres(1_600_000_000)),
                "1.60 mm",
                10,
            ),
            constraint(ConstraintKind::Material, None, "FR-4", 11),
            constraint(ConstraintKind::Finish, None, "ENIG", 12),
            constraint(ConstraintKind::Impedance, None, "50 ohm +/- 10%", 13),
            constraint(
                ConstraintKind::SpecialProcess,
                None,
                "controlled impedance",
                14,
            ),
        ];
        let mut review = FabricationReview {
            documents: vec![document],
            layers,
            construction: crate::fabrication::ConstructionEvidence {
                layers: construction_layers,
                total_thickness: Some(Picometres(1_600_000_000)),
                finish: Some("ENIG".into()),
            },
            profile: Some(crate::fabrication::BoardProfile {
                contour_feature_ids: vec![],
                cutout_feature_ids: vec![],
                extents: None,
                provenance: vec![provenance(15)],
            }),
            constraints,
            ..FabricationReview::default()
        };
        review.capabilities.records = vec![
            CapabilityRecord {
                id: CapabilityId::LayerOrder,
                state: CapabilityState::Complete,
                authority: Authority::NativeSource,
                document_ids: vec![document_id.clone()],
                provenance: review
                    .layers
                    .iter()
                    .map(|layer| layer.provenance.clone())
                    .collect(),
                detail: "Exact source layer order.".into(),
            },
            CapabilityRecord {
                id: CapabilityId::Construction,
                state: CapabilityState::Complete,
                authority: Authority::NativeSource,
                document_ids: vec![document_id.clone()],
                provenance: review
                    .construction
                    .layers
                    .iter()
                    .map(|layer| layer.provenance.clone())
                    .collect(),
                detail: "Exact source construction facts.".into(),
            },
            CapabilityRecord {
                id: CapabilityId::Constraints,
                state: CapabilityState::Complete,
                authority: Authority::NativeSource,
                document_ids: vec![document_id.clone()],
                provenance: review
                    .constraints
                    .iter()
                    .map(|constraint| constraint.provenance.clone())
                    .collect(),
                detail: "Exact source construction constraints.".into(),
            },
            CapabilityRecord {
                id: CapabilityId::Profile,
                state: CapabilityState::Complete,
                authority: Authority::NativeSource,
                document_ids: vec![document_id],
                provenance: review.profile.as_ref().unwrap().provenance.clone(),
                detail: "Exact source profile evidence.".into(),
            },
        ];
        review.capabilities.records.sort_by_key(|record| record.id);
        review.refresh_digests().unwrap();
        review.validate().unwrap();
        review
    }

    fn construction_declaration_value() -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": "1",
            "producer": "ratemypcb-project-authority",
            "producerVersion": "2026.08",
            "issuedAtUnix": 1_700_000_000_u64,
            "expiresAtUnix": 4_102_444_800_u64,
            "state": "complete",
            "rules": [],
            "orderAcknowledgements": [
                {"record": 1, "id": "stackup_order", "state": "complete", "value": null, "unit": null, "declaredValue": "L1,L2", "applicability": "board"},
                {"record": 2, "id": "total_thickness", "state": "complete", "value": "1.60", "unit": "mm", "declaredValue": null, "applicability": "board"},
                {"record": 3, "id": "material", "state": "complete", "value": null, "unit": null, "declaredValue": "FR-4", "applicability": "board"},
                {"record": 4, "id": "finish", "state": "complete", "value": null, "unit": null, "declaredValue": "ENIG", "applicability": "board"},
                {"record": 5, "id": "impedance", "state": "complete", "value": null, "unit": null, "declaredValue": "50 ohm +/- 10%", "applicability": "board"},
                {"record": 6, "id": "special_process", "state": "complete", "value": null, "unit": null, "declaredValue": "controlled impedance", "applicability": "board"},
                {"record": 7, "id": "drill_span_plating", "state": "complete", "value": null, "unit": null, "declaredValue": "confirm with fabricator", "applicability": "board"},
                {"record": 8, "id": "castellation", "state": "complete", "value": null, "unit": null, "declaredValue": "required", "applicability": "board"},
                {"record": 9, "id": "edge_plating", "state": "complete", "value": null, "unit": null, "declaredValue": "not required", "applicability": "board"},
                {"record": 10, "id": "profile", "state": "complete", "value": null, "unit": null, "declaredValue": "standard rectangular profile", "applicability": "board"},
                {"record": 11, "id": "layer_material", "state": "complete", "value": null, "unit": null, "declaredValue": "FR-4", "applicability": "layer:L1"},
                {"record": 12, "id": "layer_thickness", "state": "complete", "value": "0.035", "unit": "mm", "declaredValue": null, "applicability": "layer:L1"},
                {"record": 13, "id": "layer_material", "state": "complete", "value": null, "unit": null, "declaredValue": "FR-4", "applicability": "layer:L2"},
                {"record": 14, "id": "layer_thickness", "state": "complete", "value": "0.035", "unit": "mm", "declaredValue": null, "applicability": "layer:L2"}
            ]
        })
    }

    fn remove_construction_records(value: &mut serde_json::Value, records: &[u64]) {
        let acknowledgements = value["orderAcknowledgements"].as_array_mut().unwrap();
        acknowledgements.retain(|item| {
            item["record"]
                .as_u64()
                .is_none_or(|record| !records.contains(&record))
        });
        for (index, item) in acknowledgements.iter_mut().enumerate() {
            item["record"] = ((index + 1) as u64).into();
        }
    }

    fn construction_declarations_from(value: &serde_json::Value) -> DfmDeclarations {
        DfmDeclarations::from_json(
            "dfm/construction.json",
            &serde_json::to_vec(value).unwrap(),
            2_000_000_000,
        )
        .unwrap()
    }

    fn construction_review_from(value: &serde_json::Value) -> (FabricationReview, Vec<Coverage>) {
        let mut review = construction_design_review();
        let gaps =
            apply_declarations(&mut review, Some(&construction_declarations_from(value))).unwrap();
        (review, gaps)
    }

    fn construction_review() -> (FabricationReview, Vec<Coverage>) {
        construction_review_from(&construction_declaration_value())
    }

    #[test]
    fn construction_stackup_tokens_follow_the_normalizer_contract() {
        let design = construction_design_review();
        let l1 = design
            .layers
            .iter()
            .find(|layer| layer.name.as_deref() == Some("L1"))
            .unwrap()
            .id
            .clone();
        let l2 = design
            .layers
            .iter()
            .find(|layer| layer.name.as_deref() == Some("L2"))
            .unwrap()
            .id
            .clone();
        for (label, order) in [
            ("exact names", "L1,L2".to_string()),
            ("canonical ids", format!("{l1},{l2}")),
            ("mixed forms", format!("{l1},L2")),
        ] {
            let mut value = construction_declaration_value();
            value["orderAcknowledgements"][0]["declaredValue"] = order.into();
            let (review, _) = construction_review_from(&value);
            let (findings, coverage) = stackup_order_confirmation(
                &review,
                ManufacturingDeadline::from_timeout(Duration::from_secs(5)),
            );
            assert!(findings.is_empty(), "{label}: {findings:?}");
            assert_eq!(coverage.status, CoverageStatus::Passed, "{label}");
        }
    }

    #[test]
    fn construction_incomplete_layer_authority_is_a_stable_gap() {
        let (valid, _) = construction_review();
        let declaration_id = declaration_document(&valid).unwrap().unwrap().id.clone();
        let l1 = valid
            .layers
            .iter()
            .find(|layer| layer.name.as_deref() == Some("L1"))
            .unwrap()
            .id
            .clone();
        let l2 = valid
            .layers
            .iter()
            .find(|layer| layer.name.as_deref() == Some("L2"))
            .unwrap()
            .id
            .clone();
        let run = |review: &FabricationReview| {
            total_thickness_material(
                review,
                ManufacturingDeadline::from_timeout(Duration::from_secs(5)),
            )
        };

        for (record, kind) in [(13, "material"), (14, "thickness")] {
            let mut value = construction_declaration_value();
            remove_construction_records(&mut value, &[record]);
            let (partial, _) = construction_review_from(&value);
            let (findings, coverage) = run(&partial);
            assert_eq!(coverage.status, CoverageStatus::NotRun, "{kind}");
            assert_eq!(
                findings
                    .iter()
                    .filter(|finding| finding.id.contains("/gap/"))
                    .map(|finding| finding.id.as_str())
                    .collect::<Vec<_>>(),
                [format!(
                    "{TOTAL_THICKNESS_MATERIAL_FAMILY}/gap/customer-layer-{kind}-{l2}"
                )],
                "{kind}"
            );
            assert!(findings.iter().all(|finding| {
                !finding.id.contains("/conflict/")
                    && finding.gate_impact == GateImpact::EvidenceOnly
            }));
        }

        let mut partial_conflict_value = construction_declaration_value();
        partial_conflict_value["orderAcknowledgements"][10]["declaredValue"] = "FR4".into();
        remove_construction_records(&mut partial_conflict_value, &[13]);
        let (partial_conflict, _) = construction_review_from(&partial_conflict_value);
        let (findings, coverage) = run(&partial_conflict);
        assert_eq!(coverage.status, CoverageStatus::NotRun);
        assert!(findings.iter().any(|finding| {
            finding.id == format!("{TOTAL_THICKNESS_MATERIAL_FAMILY}/conflict/material/{l1}")
                && finding.evidence.contains("outcome=conflict")
        }));
        assert!(findings.iter().any(|finding| {
            finding.id
                == format!("{TOTAL_THICKNESS_MATERIAL_FAMILY}/gap/customer-layer-material-{l2}")
        }));

        let mut partial_value = construction_declaration_value();
        remove_construction_records(&mut partial_value, &[13, 14]);
        let (partial, _) = construction_review_from(&partial_value);
        let expected = [
            format!("{TOTAL_THICKNESS_MATERIAL_FAMILY}/gap/customer-layer-material-{l2}"),
            format!("{TOTAL_THICKNESS_MATERIAL_FAMILY}/gap/customer-layer-thickness-{l2}"),
        ];
        let (findings, coverage) = run(&partial);
        assert_eq!(coverage.status, CoverageStatus::NotRun);
        assert_eq!(
            findings
                .iter()
                .filter(|finding| finding.id.contains("/gap/"))
                .map(|finding| finding.id.as_str())
                .collect::<Vec<_>>(),
            expected
        );
        let mut reordered = partial.clone();
        reordered.layers.reverse();
        reordered.construction.layers.reverse();
        reordered.capabilities.records.reverse();
        for capability in &mut reordered.capabilities.records {
            capability.provenance.reverse();
        }
        assert_eq!(
            serde_json::to_value(run(&partial)).unwrap(),
            serde_json::to_value(run(&reordered)).unwrap()
        );

        let mut missing_design = valid.clone();
        missing_design
            .construction
            .layers
            .iter_mut()
            .find(|layer| {
                layer.provenance.document_id != declaration_id
                    && layer.layer_id.as_deref() == Some(&l2)
            })
            .unwrap()
            .material = None;
        let (findings, coverage) = run(&missing_design);
        assert_eq!(coverage.status, CoverageStatus::NotRun);
        assert!(findings.iter().any(|finding| {
            finding.id
                == format!("{TOTAL_THICKNESS_MATERIAL_FAMILY}/gap/design-layer-material-{l2}")
                && finding.gate_impact == GateImpact::EvidenceOnly
        }));
    }

    #[test]
    fn construction_incomplete_or_ambiguous_design_order_is_a_stable_gap() {
        let (valid, _) = construction_review();
        let run = |review: &FabricationReview| {
            stackup_order_confirmation(
                review,
                ManufacturingDeadline::from_timeout(Duration::from_secs(5)),
            )
        };
        let expected = format!("{STACKUP_ORDER_FAMILY}/gap/stackup-order");
        let mut variants = Vec::new();

        let mut partial = valid.clone();
        partial.layers[1].order = None;
        variants.push(("partial", partial));

        let mut duplicate = valid.clone();
        let mut repeated = duplicate.layers[0].clone();
        repeated.name = None;
        duplicate.layers.push(repeated);
        variants.push(("duplicate", duplicate));

        let mut ambiguous = valid.clone();
        ambiguous.layers[1].order = ambiguous.layers[0].order;
        variants.push(("ambiguous", ambiguous));

        for (label, review) in variants {
            let (findings, coverage) = run(&review);
            assert_eq!(coverage.status, CoverageStatus::NotRun, "{label}");
            assert_eq!(
                findings
                    .iter()
                    .map(|finding| finding.id.as_str())
                    .collect::<Vec<_>>(),
                [expected.as_str()],
                "{label}"
            );
            assert!(findings.iter().all(|finding| {
                !finding.id.contains("/conflict/")
                    && !finding.evidence.contains("outcome=conflict")
                    && finding.gate_impact == GateImpact::EvidenceOnly
            }));
        }
    }

    #[test]
    fn construction_stackup_order_material_and_thickness_are_exact_and_fail_closed() {
        const STACKUP: &str = "dfm.stackup-order-confirmation.v1";
        const THICKNESS: &str = "dfm.total-thickness-material.v1";
        let (valid, _) = construction_review();
        let run = |review: &FabricationReview| {
            fabrication_families(
                review,
                None,
                ManufacturingDeadline::from_timeout(Duration::from_secs(5)),
            )
        };
        let (findings, coverage) = run(&valid);
        assert!(findings.iter().all(|finding| {
            !finding.id.starts_with(STACKUP) && !finding.id.starts_with(THICKNESS)
        }));
        assert_eq!(
            coverage
                .iter()
                .find(|item| item.id == STACKUP)
                .unwrap()
                .status,
            CoverageStatus::Passed
        );
        let thickness = coverage.iter().find(|item| item.id == THICKNESS).unwrap();
        assert_eq!(thickness.status, CoverageStatus::Passed);
        assert!(thickness.evidence.contains("1600000000pm"));
        assert!(thickness.evidence.contains("FR-4"));
        assert!(thickness.evidence.contains("design/board.kicad_pcb"));
        assert!(thickness.evidence.contains("dfm/construction.json"));

        let mut order_value = construction_declaration_value();
        order_value["orderAcknowledgements"][0]["declaredValue"] = "L2,L1".into();
        let (order_conflict, _) = construction_review_from(&order_value);
        let (findings, coverage) = run(&order_conflict);
        assert_eq!(
            coverage
                .iter()
                .find(|item| item.id == STACKUP)
                .unwrap()
                .status,
            CoverageStatus::Attention
        );
        assert!(findings.iter().any(|finding| {
            finding.id.starts_with(STACKUP)
                && finding.evidence.contains("outcome=conflict")
                && finding.gate_impact == GateImpact::EvidenceOnly
        }));

        let mut thickness_value = construction_declaration_value();
        thickness_value["orderAcknowledgements"][1]["value"] = "1.600000001".into();
        let (thickness_conflict, _) = construction_review_from(&thickness_value);
        let (findings, coverage) = run(&thickness_conflict);
        assert_eq!(
            coverage
                .iter()
                .find(|item| item.id == THICKNESS)
                .unwrap()
                .status,
            CoverageStatus::Attention
        );
        assert!(findings.iter().any(|finding| {
            finding.id.starts_with(THICKNESS)
                && finding.evidence.contains("1600000001pm")
                && finding.evidence.contains("1600000000pm")
                && finding.gate_impact == GateImpact::EvidenceOnly
        }));

        let mut material_value = construction_declaration_value();
        material_value["orderAcknowledgements"][10]["declaredValue"] = "FR4".into();
        let (material_conflict, _) = construction_review_from(&material_value);
        let (findings, coverage) = run(&material_conflict);
        assert_eq!(
            coverage
                .iter()
                .find(|item| item.id == THICKNESS)
                .unwrap()
                .status,
            CoverageStatus::Attention
        );
        assert!(findings.iter().any(|finding| {
            finding.id.contains("/conflict/material/")
                && finding.evidence.contains("design=\"FR-4\"")
                && finding.evidence.contains("requirement=\"FR4\"")
        }));

        let direct = construction_design_review();
        let (_, coverage) = run(&direct);
        assert_eq!(
            coverage
                .iter()
                .find(|item| item.id == STACKUP)
                .unwrap()
                .status,
            CoverageStatus::NotRun
        );
        assert_eq!(
            coverage
                .iter()
                .find(|item| item.id == THICKNESS)
                .unwrap()
                .status,
            CoverageStatus::NotRun
        );

        let declaration_id = declaration_document(&valid).unwrap().unwrap().id.clone();
        let mut duplicate = valid.clone();
        let authority = duplicate
            .constraints
            .iter()
            .find(|constraint| {
                constraint.kind == ConstraintKind::FinishedThickness
                    && constraint.provenance.document_id == declaration_id
            })
            .unwrap()
            .clone();
        duplicate.constraints.push(authority);
        assert_eq!(
            run(&duplicate)
                .1
                .iter()
                .find(|item| item.id == THICKNESS)
                .unwrap()
                .status,
            CoverageStatus::NotRun
        );

        let mut stale = valid.clone();
        stale
            .capabilities
            .records
            .iter_mut()
            .find(|record| record.id == CapabilityId::Construction)
            .unwrap()
            .state = CapabilityState::Stale;
        assert_eq!(
            run(&stale)
                .1
                .iter()
                .find(|item| item.id == STACKUP)
                .unwrap()
                .status,
            CoverageStatus::NotRun
        );

        let mut reordered = valid.clone();
        reordered.layers.reverse();
        reordered.construction.layers.reverse();
        reordered.constraints.reverse();
        reordered.documents.reverse();
        reordered.capabilities.records.reverse();
        assert_eq!(
            serde_json::to_value(run(&valid)).unwrap(),
            serde_json::to_value(run(&reordered)).unwrap()
        );
    }

    fn construction_drill_review() -> (FabricationReview, BTreeMap<String, String>) {
        let (mut review, declaration_coverage) = construction_review();
        let design_document = review
            .documents
            .iter()
            .find(|document| document.adapter == KICAD_MANUFACTURING_ADAPTER)
            .unwrap()
            .clone();
        let provenance = |record| ManufacturingProvenance {
            document_id: design_document.id.clone(),
            artifact_digest: design_document.artifact_digest.clone(),
            producer: KICAD_MANUFACTURING_ADAPTER.into(),
            producer_version: KICAD_MANUFACTURING_ADAPTER_VERSION.into(),
            location: StructuralLocation {
                record,
                subrecord: None,
                byte_start: record,
                byte_end: record,
            },
            source_lexeme: None,
        };
        let tool_source = provenance(20);
        let tool_id =
            crate::fabrication::tool_id(&design_document.id, "Drill:T01", &tool_source.location);
        let span = crate::fabrication::LayerSpan {
            from_layer_id: Some(review.layers[0].id.clone()),
            to_layer_id: Some(review.layers[1].id.clone()),
        };
        review.tools.push(ManufacturingTool {
            id: tool_id.clone(),
            document_id: design_document.id.clone(),
            code: "T01".into(),
            kind: ToolKind::Drill,
            diameter: Some(Picometres(300_000_000)),
            plating: Plating::Plated,
            span: Some(span),
            provenance: tool_source.clone(),
        });
        let feature_source = provenance(21);
        review.features.push(ManufacturingFeature {
            id: crate::fabrication::feature_id(
                &design_document.id,
                &review.layers[0].id,
                "drill",
                &feature_source.location,
            ),
            document_id: design_document.id.clone(),
            layer_id: review.layers[0].id.clone(),
            tool_id: Some(tool_id.clone()),
            polarity: LayerPolarity::Dark,
            geometry: Geometry::Drill(crate::fabrication::DrillFeature {
                position: CanonicalPoint::new(1_000_000_000, 1_000_000_000),
                diameter: Picometres(300_000_000),
                tool_id,
            }),
            transforms: crate::fabrication::TransformChain::default(),
            membership: FeatureMembership::TopLevel,
            provenance: feature_source.clone(),
        });
        for id in [
            CapabilityId::Tools,
            CapabilityId::Drills,
            CapabilityId::Plating,
            CapabilityId::LayerSpans,
        ] {
            review.capabilities.records.push(CapabilityRecord {
                id,
                state: CapabilityState::Complete,
                authority: Authority::NativeSource,
                document_ids: vec![design_document.id.clone()],
                provenance: vec![tool_source.clone(), feature_source.clone()],
                detail: "Exact design drill, plating, and span evidence.".into(),
            });
        }
        review.capabilities.records.sort_by_key(|record| record.id);
        review.refresh_digests().unwrap();
        review.validate().unwrap();
        let gaps = normalized_declaration_gaps(&declaration_coverage, None).unwrap();
        (review, gaps)
    }

    #[test]
    fn construction_drill_span_and_plating_are_confirmation_gaps_only() {
        let (valid, gaps) = construction_drill_review();
        let run = |review: &FabricationReview, gaps: &BTreeMap<String, String>| {
            drill_span_plating(
                review,
                gaps,
                ManufacturingDeadline::from_timeout(Duration::from_secs(5)),
            )
        };
        let (findings, coverage) = run(&valid, &gaps);
        assert_eq!(coverage.status, CoverageStatus::NotRun);
        assert!(coverage.evidence.starts_with("not_checked:"));
        assert_eq!(findings.len(), 1);
        assert!(findings[0].id.contains("/gap/tool/"));
        assert!(findings[0].evidence.contains("plating=plated"));
        assert!(findings[0].evidence.contains("from_layer=L1"));
        assert!(findings[0].evidence.contains("to_layer=L2"));
        assert!(findings[0].evidence.contains("confirm with fabricator"));
        assert!(findings[0].evidence.contains("dfm/construction.json"));
        assert!(!findings[0].evidence.contains("through-hole"));
        assert_eq!(findings[0].gate_impact, GateImpact::EvidenceOnly);

        for plating in [Plating::Mixed, Plating::Unknown] {
            let mut mutated = valid.clone();
            mutated.tools[0].plating = plating;
            let (findings, coverage) = run(&mutated, &gaps);
            assert_eq!(coverage.status, CoverageStatus::NotRun, "{plating:?}");
            assert!(findings.iter().all(|finding| {
                finding.gate_impact == GateImpact::EvidenceOnly
                    && !finding.id.contains("match")
                    && !finding.id.contains("conflict")
            }));
        }
        let mut missing_span = valid.clone();
        missing_span.tools[0].span = None;
        assert_eq!(run(&missing_span, &gaps).1.status, CoverageStatus::NotRun);

        let mut duplicate = valid.clone();
        duplicate.tools.push(duplicate.tools[0].clone());
        assert_eq!(run(&duplicate, &gaps).1.status, CoverageStatus::NotRun);

        let mut stale = valid.clone();
        stale
            .capabilities
            .records
            .iter_mut()
            .find(|record| record.id == CapabilityId::Plating)
            .unwrap()
            .state = CapabilityState::Stale;
        assert_eq!(run(&stale, &gaps).1.status, CoverageStatus::NotRun);

        let direct = construction_design_review();
        let (findings, coverage) = run(&direct, &BTreeMap::new());
        assert_eq!(coverage.status, CoverageStatus::NotRun);
        assert!(findings.iter().all(|finding| {
            finding.gate_impact == GateImpact::EvidenceOnly
                && !finding.id.contains("match")
                && !finding.id.contains("conflict")
        }));

        let mut reordered = valid.clone();
        reordered.tools.reverse();
        reordered.features.reverse();
        reordered.layers.reverse();
        reordered.capabilities.records.reverse();
        assert_eq!(
            serde_json::to_value(run(&valid, &gaps)).unwrap(),
            serde_json::to_value(run(&reordered, &gaps)).unwrap()
        );
        assert_eq!(
            drill_span_plating(
                &valid,
                &gaps,
                ManufacturingDeadline::from_timeout(Duration::ZERO),
            )
            .1
            .status,
            CoverageStatus::NotRun
        );
    }

    #[test]
    fn construction_finish_profile_impedance_and_special_process_are_bounded() {
        let (valid, declaration_coverage) = construction_review();
        let gaps = normalized_declaration_gaps(&declaration_coverage, None).unwrap();
        let deadline = || ManufacturingDeadline::from_timeout(Duration::from_secs(5));

        let (finish_findings, finish_coverage) = finish_profile(&valid, &gaps, deadline());
        assert_eq!(finish_coverage.status, CoverageStatus::NotRun);
        assert!(finish_coverage.evidence.contains("finish outcome=match"));
        assert_eq!(finish_findings.len(), 3);
        for concept in ["profile", "castellation", "edge-plating"] {
            let finding = finish_findings
                .iter()
                .find(|finding| finding.id.ends_with(concept))
                .unwrap();
            assert!(finding.id.contains("/gap/"));
            assert!(finding.evidence.contains("outcome=confirmation_gap"));
            assert!(finding.evidence.contains("dfm/construction.json"));
            assert!(!finding.evidence.contains("outcome=match"));
            assert!(!finding.evidence.contains("outcome=conflict"));
            assert_eq!(finding.gate_impact, GateImpact::EvidenceOnly);
        }

        let (process_findings, process_coverage) = impedance_special_process(&valid, deadline());
        assert!(process_findings.is_empty());
        assert_eq!(process_coverage.status, CoverageStatus::Passed);
        assert!(
            process_coverage
                .evidence
                .contains("impedance outcome=match")
        );
        assert!(
            process_coverage
                .evidence
                .contains("special-process outcome=match")
        );
        assert!(process_coverage.evidence.contains("design/board.kicad_pcb"));
        assert!(process_coverage.evidence.contains("dfm/construction.json"));
        assert!(!process_coverage.evidence.contains("calculated"));

        let mut conflict_value = construction_declaration_value();
        conflict_value["orderAcknowledgements"][3]["declaredValue"] = "HASL".into();
        conflict_value["orderAcknowledgements"][4]["declaredValue"] = "75 ohm".into();
        conflict_value["orderAcknowledgements"][5]["declaredValue"] = "none".into();
        let (conflicts, conflict_coverage) = construction_review_from(&conflict_value);
        let conflict_gaps = normalized_declaration_gaps(&conflict_coverage, None).unwrap();
        let (finish_findings, finish_coverage) =
            finish_profile(&conflicts, &conflict_gaps, deadline());
        assert_eq!(finish_coverage.status, CoverageStatus::Attention);
        assert!(finish_findings.iter().any(|finding| {
            finding.id.contains("/conflict/finish")
                && finding.evidence.contains("design=\"ENIG\"")
                && finding.evidence.contains("requirement=\"HASL\"")
        }));
        let (process_findings, process_coverage) =
            impedance_special_process(&conflicts, deadline());
        assert_eq!(process_coverage.status, CoverageStatus::Attention);
        assert_eq!(process_findings.len(), 2);
        assert!(process_findings.iter().all(|finding| {
            finding.id.contains("/conflict/") && finding.gate_impact == GateImpact::EvidenceOnly
        }));

        let declaration_id = declaration_document(&valid).unwrap().unwrap().id.clone();
        let mut missing = valid.clone();
        missing.constraints.retain(|constraint| {
            constraint.kind != ConstraintKind::Impedance
                || constraint.provenance.document_id != declaration_id
        });
        let (findings, coverage) = impedance_special_process(&missing, deadline());
        assert_eq!(coverage.status, CoverageStatus::NotRun);
        assert!(findings.iter().any(|finding| {
            finding.id.ends_with("/gap/impedance")
                && finding.evidence.contains("outcome=confirmation_gap")
        }));
        assert!(
            !findings
                .iter()
                .any(|finding| finding.id.contains("/match/"))
        );

        for state in [
            CapabilityState::Partial,
            CapabilityState::NotProvided,
            CapabilityState::Unsupported,
            CapabilityState::Failed,
            CapabilityState::Stale,
            CapabilityState::Omitted,
        ] {
            let mut mutated = valid.clone();
            mutated
                .capabilities
                .records
                .iter_mut()
                .find(|record| record.id == CapabilityId::Constraints)
                .unwrap()
                .state = state;
            assert_eq!(
                finish_profile(&mutated, &gaps, deadline()).1.status,
                CoverageStatus::NotRun,
                "{state:?}"
            );
            assert_eq!(
                impedance_special_process(&mutated, deadline()).1.status,
                CoverageStatus::NotRun,
                "{state:?}"
            );
        }
        let direct = construction_design_review();
        assert_eq!(
            finish_profile(&direct, &BTreeMap::new(), deadline())
                .1
                .status,
            CoverageStatus::NotRun
        );
        assert_eq!(
            impedance_special_process(&direct, deadline()).1.status,
            CoverageStatus::NotRun
        );

        let mut unknown = valid.clone();
        unknown
            .capabilities
            .records
            .iter_mut()
            .find(|record| record.id == CapabilityId::Constraints)
            .unwrap()
            .authority = Authority::Unknown;
        assert_eq!(
            finish_profile(&unknown, &gaps, deadline()).1.status,
            CoverageStatus::NotRun
        );
        assert_eq!(
            impedance_special_process(&unknown, deadline()).1.status,
            CoverageStatus::NotRun
        );

        let mut stale_source = valid.clone();
        stale_source
            .constraints
            .iter_mut()
            .find(|constraint| {
                constraint.kind == ConstraintKind::Finish
                    && constraint.provenance.document_id == declaration_id
            })
            .unwrap()
            .provenance
            .producer_version = "stale-version".into();
        assert_eq!(
            finish_profile(&stale_source, &gaps, deadline()).1.status,
            CoverageStatus::NotRun
        );

        let mut unknown_design = valid.clone();
        unknown_design
            .constraints
            .iter_mut()
            .find(|constraint| {
                constraint.kind == ConstraintKind::Impedance
                    && constraint.provenance.document_id != declaration_id
            })
            .unwrap()
            .authority = Authority::Unknown;
        assert_eq!(
            impedance_special_process(&unknown_design, deadline())
                .1
                .status,
            CoverageStatus::NotRun
        );

        let mutation_provenance = valid.constraints[0].provenance.clone();
        let mutation_fact = crate::fabrication::ConflictFact {
            canonical_value: "mutated".into(),
            authority: Authority::Explicit,
            provenance: mutation_provenance.clone(),
        };
        let mut affected_conflict = valid.clone();
        affected_conflict
            .conflicts
            .push(crate::fabrication::Conflict {
                id: "construction-test-conflict".into(),
                kind: crate::fabrication::ConflictKind::Constraint,
                affected_capabilities: vec![CapabilityId::Constraints],
                left: mutation_fact.clone(),
                right: mutation_fact,
            });
        assert_eq!(
            finish_profile(&affected_conflict, &gaps, deadline())
                .1
                .status,
            CoverageStatus::NotRun
        );
        assert_eq!(
            impedance_special_process(&affected_conflict, deadline())
                .1
                .status,
            CoverageStatus::NotRun
        );

        let mut affected_omission = valid.clone();
        affected_omission
            .omissions
            .push(crate::fabrication::Omission {
                id: "construction-test-omission".into(),
                kind: crate::fabrication::OmissionKind::MissingSemanticRecord,
                affected_capabilities: vec![CapabilityId::Constraints],
                provenance: mutation_provenance,
                detail: "mutated construction authority".into(),
            });
        assert_eq!(
            finish_profile(&affected_omission, &gaps, deadline())
                .1
                .status,
            CoverageStatus::NotRun
        );
        assert_eq!(
            impedance_special_process(&affected_omission, deadline())
                .1
                .status,
            CoverageStatus::NotRun
        );

        let mut dangling = valid.clone();
        dangling
            .constraints
            .iter_mut()
            .find(|constraint| {
                constraint.kind == ConstraintKind::SpecialProcess
                    && constraint.provenance.document_id == declaration_id
            })
            .unwrap()
            .provenance
            .artifact_digest = "0".repeat(64);
        assert_eq!(
            impedance_special_process(&dangling, deadline()).1.status,
            CoverageStatus::NotRun
        );

        let mut malformed = valid.clone();
        malformed
            .constraints
            .iter_mut()
            .find(|constraint| {
                constraint.kind == ConstraintKind::Finish
                    && constraint.provenance.document_id == declaration_id
            })
            .unwrap()
            .provenance
            .source_lexeme = Some("finish@inferred-default".into());
        assert_eq!(
            finish_profile(&malformed, &gaps, deadline()).1.status,
            CoverageStatus::NotRun
        );

        let mut missing_prerequisite = valid.clone();
        missing_prerequisite
            .capabilities
            .records
            .retain(|record| record.id != CapabilityId::Construction);
        assert_eq!(
            finish_profile(&missing_prerequisite, &gaps, deadline())
                .1
                .status,
            CoverageStatus::NotRun
        );
        assert_eq!(
            impedance_special_process(&missing_prerequisite, deadline())
                .1
                .status,
            CoverageStatus::NotRun
        );

        let mut reordered = valid.clone();
        reordered.documents.reverse();
        reordered.layers.reverse();
        reordered.construction.layers.reverse();
        reordered.constraints.reverse();
        reordered.capabilities.records.reverse();
        assert_eq!(
            serde_json::to_value(finish_profile(&valid, &gaps, deadline())).unwrap(),
            serde_json::to_value(finish_profile(&reordered, &gaps, deadline())).unwrap()
        );
        assert_eq!(
            serde_json::to_value(impedance_special_process(&valid, deadline())).unwrap(),
            serde_json::to_value(impedance_special_process(&reordered, deadline())).unwrap()
        );

        let mut duplicate = valid.clone();
        duplicate.capabilities.records.push(
            duplicate
                .capabilities
                .records
                .iter()
                .find(|record| record.id == CapabilityId::Construction)
                .unwrap()
                .clone(),
        );
        assert_eq!(
            finish_profile(&duplicate, &gaps, deadline()).1.status,
            CoverageStatus::NotRun
        );
        assert_eq!(
            impedance_special_process(&duplicate, deadline()).1.status,
            CoverageStatus::NotRun
        );
        assert_eq!(
            finish_profile(
                &valid,
                &gaps,
                ManufacturingDeadline::from_timeout(Duration::ZERO),
            )
            .1
            .status,
            CoverageStatus::NotRun
        );
        assert_eq!(
            impedance_special_process(&valid, ManufacturingDeadline::from_timeout(Duration::ZERO),)
                .1
                .status,
            CoverageStatus::NotRun
        );
    }

    fn outline_test_provenance(record: u64) -> ManufacturingProvenance {
        ManufacturingProvenance {
            document_id: "outline-document".into(),
            artifact_digest: "a".repeat(64),
            producer: "outline-fixture".into(),
            producer_version: "1".into(),
            location: StructuralLocation {
                record,
                subrecord: None,
                byte_start: record,
                byte_end: record + 1,
            },
            source_lexeme: None,
        }
    }

    fn outline_line(start: (i64, i64), end: (i64, i64)) -> ContourSegment {
        ContourSegment::Line(CanonicalLine {
            start: CanonicalPoint::new(start.0, start.1),
            end: CanonicalPoint::new(end.0, end.1),
            width: None,
        })
    }

    fn outline_rectangle(min: i64, max: i64) -> CanonicalContour {
        CanonicalContour {
            segments: vec![
                outline_line((min, min), (max, min)),
                outline_line((max, min), (max, max)),
                outline_line((max, max), (min, max)),
                outline_line((min, max), (min, min)),
            ],
            closed: true,
        }
    }

    fn outline_fixture(
        values: Vec<(OutlineClassification, &str, CanonicalContour)>,
    ) -> FabricationReview {
        let document = ManufacturingDocument {
            id: "outline-document".into(),
            virtual_path: "fab/profile.gbr".into(),
            artifact_digest: "a".repeat(64),
            format: DocumentFormat::Gerber,
            adapter: "outline-fixture".into(),
            adapter_version: "1".into(),
            parse_status: ParseStatus::Complete,
            numeric_format: None,
            metrics: DocumentMetrics::default(),
        };
        let provenance = outline_test_provenance(1);
        let layer = crate::fabrication::ManufacturingLayer {
            id: "outline-layer".into(),
            document_id: document.id.clone(),
            name: Some("Profile".into()),
            role: LayerRole::Profile,
            side: crate::fabrication::LayerSide::NotApplicable,
            context: crate::fabrication::LayerContext::Board,
            polarity: LayerPolarity::Positive,
            order: None,
            authority: Authority::Explicit,
            provenance: provenance.clone(),
        };
        let mut exterior = Vec::new();
        let mut cutouts = Vec::new();
        let features = values
            .into_iter()
            .enumerate()
            .map(|(index, (classification, id, contour))| {
                match classification {
                    OutlineClassification::Exterior => exterior.push(id.to_owned()),
                    OutlineClassification::Cutout => cutouts.push(id.to_owned()),
                }
                ManufacturingFeature {
                    id: id.into(),
                    document_id: document.id.clone(),
                    layer_id: layer.id.clone(),
                    tool_id: None,
                    polarity: match classification {
                        OutlineClassification::Exterior => LayerPolarity::Dark,
                        OutlineClassification::Cutout => LayerPolarity::Clear,
                    },
                    geometry: Geometry::Contour(contour),
                    transforms: crate::fabrication::TransformChain::default(),
                    membership: FeatureMembership::TopLevel,
                    provenance: outline_test_provenance(index as u64 + 2),
                }
            })
            .collect();
        let capabilities = OUTLINE_REQUIREMENTS
            .prerequisites
            .iter()
            .map(|id| CapabilityRecord {
                id: *id,
                state: CapabilityState::Complete,
                authority: Authority::Explicit,
                document_ids: vec![document.id.clone()],
                provenance: vec![provenance.clone()],
                detail: "complete outline fixture capability".into(),
            })
            .collect();
        FabricationReview {
            documents: vec![document],
            layers: vec![layer],
            features,
            profile: Some(crate::fabrication::BoardProfile {
                contour_feature_ids: exterior,
                cutout_feature_ids: cutouts,
                extents: Some(crate::fabrication::Extent {
                    min: CanonicalPoint::new(-10_000, -10_000),
                    max: CanonicalPoint::new(10_000, 10_000),
                }),
                provenance: vec![provenance],
            }),
            capabilities: crate::fabrication::CapabilityLedger {
                records: capabilities,
            },
            ..FabricationReview::default()
        }
    }

    fn outline_result(review: &FabricationReview) -> (Vec<Finding>, Coverage) {
        outline_topology(
            review,
            ManufacturingDeadline::from_timeout(Duration::from_secs(30)),
        )
    }

    #[test]
    fn outline_open_and_intersecting_contours_are_exact_evidence_only_findings() {
        assert!(
            !outline_lines_intersect(
                &CanonicalLine {
                    start: CanonicalPoint::new(0, 0),
                    end: CanonicalPoint::new(10, 0),
                    width: None,
                },
                &CanonicalLine {
                    start: CanonicalPoint::new(20, 0),
                    end: CanonicalPoint::new(20, 10),
                    width: None,
                },
            )
            .unwrap()
        );
        let open = outline_fixture(vec![(
            OutlineClassification::Exterior,
            "open",
            CanonicalContour {
                segments: vec![
                    outline_line((0, 0), (100, 0)),
                    outline_line((100, 0), (100, 100)),
                    outline_line((100, 100), (0, 100)),
                ],
                closed: false,
            },
        )]);
        let (findings, coverage) = outline_result(&open);
        assert_eq!(coverage.status, CoverageStatus::Attention);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].id.contains("/open/"));
        assert_eq!(findings[0].gate_impact, GateImpact::EvidenceOnly);

        let intersecting = outline_fixture(vec![(
            OutlineClassification::Exterior,
            "bow-tie",
            CanonicalContour {
                segments: vec![
                    outline_line((0, 0), (100, 100)),
                    outline_line((100, 100), (0, 100)),
                    outline_line((0, 100), (100, 0)),
                    outline_line((100, 0), (0, 0)),
                ],
                closed: true,
            },
        )]);
        let (findings, coverage) = outline_result(&intersecting);
        assert_eq!(coverage.status, CoverageStatus::Attention);
        assert!(
            findings
                .iter()
                .any(|finding| finding.id.contains("/intersection/"))
        );
        assert!(
            findings
                .iter()
                .all(|finding| finding.gate_impact == GateImpact::EvidenceOnly)
        );
    }

    #[test]
    fn outline_closed_cutout_and_represented_arc_hard_negatives_are_clean() {
        let cutout = outline_fixture(vec![
            (
                OutlineClassification::Exterior,
                "exterior",
                outline_rectangle(0, 100),
            ),
            (
                OutlineClassification::Cutout,
                "cutout",
                outline_rectangle(25, 50),
            ),
        ]);
        let (findings, coverage) = outline_result(&cutout);
        assert_eq!(coverage.status, CoverageStatus::Passed);
        assert!(findings.is_empty());
        assert!(coverage.evidence.contains("cutouts=1"));
        assert!(coverage.evidence.contains("classification=cutout"));

        let arc = CanonicalArc {
            start: CanonicalPoint::new(-10, 0),
            end: CanonicalPoint::new(10, 0),
            center: CanonicalPoint::new(0, 0),
            direction: crate::fabrication::ArcDirection::Clockwise,
            quadrant: QuadrantMode::Multi,
            width: None,
            source_resolution: Picometres(1),
        };
        let arc_outline = outline_fixture(vec![(
            OutlineClassification::Exterior,
            "arc-exterior",
            CanonicalContour {
                segments: vec![
                    ContourSegment::Arc(arc),
                    outline_line((10, 0), (20, -20)),
                    outline_line((20, -20), (-20, -20)),
                    outline_line((-20, -20), (-10, 0)),
                ],
                closed: true,
            },
        )]);
        let (findings, coverage) = outline_result(&arc_outline);
        assert_eq!(coverage.status, CoverageStatus::Passed);
        assert!(findings.is_empty());
    }

    #[test]
    fn outline_prerequisite_polarity_transform_and_resource_mutations_fail_closed() {
        let valid = outline_fixture(vec![(
            OutlineClassification::Exterior,
            "exterior",
            outline_rectangle(0, 100),
        )]);
        for state in [
            CapabilityState::Partial,
            CapabilityState::NotProvided,
            CapabilityState::Unsupported,
            CapabilityState::Failed,
            CapabilityState::Stale,
            CapabilityState::Omitted,
        ] {
            let mut mutated = valid.clone();
            mutated
                .capabilities
                .records
                .iter_mut()
                .find(|record| record.id == CapabilityId::Profile)
                .unwrap()
                .state = state;
            assert_eq!(outline_result(&mutated).1.status, CoverageStatus::NotRun);
        }

        let mut duplicate = valid.clone();
        duplicate
            .capabilities
            .records
            .push(duplicate.capabilities.records[0].clone());
        assert_eq!(outline_result(&duplicate).1.status, CoverageStatus::NotRun);

        let mut polarity = valid.clone();
        polarity.features[0].polarity = LayerPolarity::Unknown;
        assert_eq!(outline_result(&polarity).1.status, CoverageStatus::NotRun);

        let mut transformed = valid.clone();
        transformed.features[0].transforms.operations.push(
            crate::fabrication::TransformOperation::Translate {
                x: Picometres(1),
                y: Picometres(0),
            },
        );
        assert_eq!(
            outline_result(&transformed).1.status,
            CoverageStatus::NotRun
        );

        let mut unexpanded = valid.clone();
        unexpanded.features[0].membership = FeatureMembership::ApertureBlock {
            block_id: "block".into(),
            aperture_id: "aperture".into(),
        };
        assert_eq!(outline_result(&unexpanded).1.status, CoverageStatus::NotRun);

        let mut ambiguous = valid.clone();
        ambiguous
            .profile
            .as_mut()
            .unwrap()
            .cutout_feature_ids
            .push("exterior".into());
        assert_eq!(outline_result(&ambiguous).1.status, CoverageStatus::NotRun);

        let nested = outline_fixture(vec![
            (
                OutlineClassification::Exterior,
                "exterior",
                outline_rectangle(0, 100),
            ),
            (
                OutlineClassification::Cutout,
                "cutout-outer",
                outline_rectangle(20, 80),
            ),
            (
                OutlineClassification::Cutout,
                "cutout-inner",
                outline_rectangle(30, 40),
            ),
        ]);
        assert_eq!(outline_result(&nested).1.status, CoverageStatus::NotRun);

        assert_eq!(
            outline_topology(&valid, ManufacturingDeadline::from_timeout(Duration::ZERO))
                .1
                .status,
            CoverageStatus::NotRun
        );

        let over = outline_fixture(vec![(
            OutlineClassification::Exterior,
            "over-limit",
            CanonicalContour {
                segments: (0..=MAX_OUTLINE_SEGMENTS)
                    .map(|index| outline_line((index as i64, 0), (index as i64 + 1, 0)))
                    .collect(),
                closed: false,
            },
        )]);
        assert_eq!(outline_result(&over).1.status, CoverageStatus::NotRun);
    }

    #[test]
    fn outline_extreme_arithmetic_fails_closed_without_panicking() {
        let extreme = outline_fixture(vec![(
            OutlineClassification::Exterior,
            "extreme",
            CanonicalContour {
                segments: vec![
                    outline_line((i64::MIN, i64::MIN), (i64::MAX, i64::MAX)),
                    outline_line((i64::MAX, i64::MAX), (i64::MIN, i64::MAX)),
                    outline_line((i64::MIN, i64::MAX), (i64::MAX, i64::MIN)),
                    outline_line((i64::MAX, i64::MIN), (i64::MIN, i64::MIN)),
                ],
                closed: true,
            },
        )]);
        let extreme_result = std::panic::catch_unwind(|| outline_result(&extreme));
        assert!(extreme_result.is_ok());
        assert_eq!(extreme_result.unwrap().1.status, CoverageStatus::NotRun);

        let radius = i64::MAX;
        let left = ContourSegment::Arc(CanonicalArc {
            start: CanonicalPoint::new(radius, radius),
            end: CanonicalPoint::new(0, 0),
            center: CanonicalPoint::new(radius, 0),
            direction: crate::fabrication::ArcDirection::Clockwise,
            quadrant: QuadrantMode::Multi,
            width: None,
            source_resolution: Picometres(1),
        });
        let right = ContourSegment::Arc(CanonicalArc {
            start: CanonicalPoint::new(0, 0),
            end: CanonicalPoint::new(radius, radius),
            center: CanonicalPoint::new(0, radius),
            direction: crate::fabrication::ArcDirection::Clockwise,
            quadrant: QuadrantMode::Multi,
            width: None,
            source_resolution: Picometres(1),
        });
        let arc_result = std::panic::catch_unwind(|| {
            adjacent_segments_supported(&left, &right, CanonicalPoint::new(0, 0))
        });
        assert!(arc_result.is_ok());
        assert!(arc_result.unwrap().is_err());
    }

    #[test]
    fn outline_order_and_exact_resource_boundary_are_deterministic() {
        let valid = outline_fixture(vec![
            (
                OutlineClassification::Exterior,
                "exterior",
                outline_rectangle(0, 100),
            ),
            (
                OutlineClassification::Cutout,
                "cutout-a",
                outline_rectangle(20, 30),
            ),
            (
                OutlineClassification::Cutout,
                "cutout-b",
                outline_rectangle(60, 70),
            ),
        ]);
        let mut reordered = valid.clone();
        reordered.features.reverse();
        reordered
            .profile
            .as_mut()
            .unwrap()
            .cutout_feature_ids
            .reverse();
        assert_eq!(
            serde_json::to_value(outline_result(&valid)).unwrap(),
            serde_json::to_value(outline_result(&reordered)).unwrap()
        );

        let exact = outline_fixture(vec![(
            OutlineClassification::Exterior,
            "exact-limit",
            CanonicalContour {
                segments: (0..MAX_OUTLINE_SEGMENTS)
                    .map(|index| outline_line((index as i64, 0), (index as i64 + 1, 0)))
                    .collect(),
                closed: false,
            },
        )]);
        let (_, coverage) = outline_result(&exact);
        assert_eq!(coverage.status, CoverageStatus::Attention);
        assert!(
            coverage
                .evidence
                .contains(&format!("segments={MAX_OUTLINE_SEGMENTS}"))
        );
        assert!(coverage.evidence.contains("pair_checks=998991"));
    }

    fn ranking_evidence(id: &str, check_id: &str, kind: &str) -> EvidenceRecord {
        EvidenceRecord {
            id: id.into(),
            check_id: check_id.into(),
            kind: kind.into(),
            provenance: crate::EvidenceProvenance {
                artifact_id: "artifact-test".into(),
                artifact_digest: "a".repeat(64),
                producer: crate::EvidenceProducer {
                    kind: "tool".into(),
                    name: "test".into(),
                    version: "1".into(),
                },
                location: BTreeMap::from([("value".into(), id.into())]),
                evidence_class: "test".into(),
                confidence: crate::EvidenceConfidence::Medium,
                freshness: EvidenceFreshness::NotApplicable,
                observed_at: None,
            },
        }
    }

    fn ranking_finding(
        id: &str,
        severity: Severity,
        impact: GateImpact,
        location: &str,
        recommendation: &str,
    ) -> Finding {
        Finding {
            id: id.into(),
            severity,
            category: "test".into(),
            title: "test".into(),
            evidence: "test".into(),
            recommendation: recommendation.into(),
            location: location.into(),
            source: "test".into(),
            gate_impact: impact,
        }
    }

    #[test]
    fn qualification_shipped_policy_is_unique_and_evidence_only() {
        assert_eq!(FAMILY_POLICIES.len(), 26);
        assert_eq!(
            FAMILY_POLICIES
                .iter()
                .map(|policy| policy.key)
                .collect::<BTreeSet<_>>()
                .len(),
            FAMILY_POLICIES.len()
        );
        assert!(
            FAMILY_POLICIES
                .iter()
                .all(|policy| family_gate_impact(policy.key) == GateImpact::EvidenceOnly)
        );
        for family in [ASSEMBLY_ACCESS_FAMILY, TESTPOINT_ACCESS_FAMILY] {
            let policy = family_policy(family).unwrap();
            assert_eq!(policy.evidence.precision_bps, Some(10_000));
            assert_eq!(policy.evidence.recall_bps, Some(10_000));
            assert!(policy.evidence.reviewed_family_version.is_none());
            assert!(policy.evidence.reviewer.is_none());
            assert!(policy.evidence.inference_approval.is_none());
            assert!(!qualification_eligible(policy));
            assert_eq!(family_gate_impact(family), GateImpact::EvidenceOnly);
        }
    }

    #[test]
    fn unblock_tiers_ties_and_corrective_action_dedupe_are_exact() {
        let mut required = vec![
            RequiredEvidence {
                check_id: "required-b".into(),
                evidence_id: "coverage-b".into(),
                execution: EvidenceExecution::NotRun,
                result: EvidenceResult::Unknown,
                freshness: EvidenceFreshness::Unknown,
                confidence: crate::EvidenceConfidence::Unknown,
            },
            RequiredEvidence {
                check_id: "required-a".into(),
                evidence_id: "coverage-a".into(),
                execution: EvidenceExecution::Completed,
                result: EvidenceResult::Attention,
                freshness: EvidenceFreshness::Current,
                confidence: crate::EvidenceConfidence::Medium,
            },
        ];
        let mut evidence = vec![
            ranking_evidence("coverage-a", "required-a", "coverage"),
            ranking_evidence("coverage-b", "required-b", "coverage"),
            ranking_evidence("ev-a", "family.alpha.v1/one", "finding"),
            ranking_evidence("ev-b", "family.alpha.v1/two", "finding"),
            ranking_evidence("ev-c", "family.alpha.v1/three", "finding"),
            ranking_evidence("ev-d", "family.beta.v1/one", "finding"),
        ];
        let mut findings = vec![
            ranking_finding(
                "ev-b",
                Severity::High,
                GateImpact::Blocking,
                "b",
                "fix alpha",
            ),
            ranking_finding(
                "ev-a",
                Severity::High,
                GateImpact::Blocking,
                "a",
                "fix alpha",
            ),
            ranking_finding(
                "ev-c",
                Severity::High,
                GateImpact::Blocking,
                "c",
                "different alpha fix",
            ),
            ranking_finding(
                "ev-d",
                Severity::Critical,
                GateImpact::Blocking,
                "z",
                "fix alpha",
            ),
        ];

        assert_eq!(
            top_unblock_evidence_refs(
                &["required-a", "required-b"],
                &required,
                &findings,
                &evidence,
            )
            .unwrap(),
            BTreeSet::from(["coverage-a".into()])
        );
        for item in &mut required {
            item.execution = EvidenceExecution::Completed;
            item.result = EvidenceResult::Pass;
            item.freshness = EvidenceFreshness::NotApplicable;
        }
        assert_eq!(
            top_unblock_evidence_refs(&[], &required, &findings, &evidence).unwrap(),
            BTreeSet::from(["ev-d".into()]),
            "severity descending precedes family and location"
        );
        findings[3].severity = Severity::Medium;
        assert_eq!(
            top_unblock_evidence_refs(&[], &required, &findings, &evidence).unwrap(),
            BTreeSet::from(["ev-a".into(), "ev-b".into()]),
            "same-family exact corrective action groups occurrences only"
        );
        for finding in &mut findings {
            finding.gate_impact = GateImpact::EvidenceOnly;
        }
        assert_eq!(
            top_unblock_evidence_refs(&[], &required, &findings, &evidence).unwrap(),
            BTreeSet::from(["ev-a".into(), "ev-b".into()]),
            "evidence-only attention uses family then location"
        );

        evidence.retain(|record| record.id != "ev-a");
        assert!(top_unblock_evidence_refs(&[], &required, &findings, &evidence).is_err());
    }
}
