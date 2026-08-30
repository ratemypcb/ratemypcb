use gerber_parser::{ContentError, parse as parse_gerber};
use ratemypcb_core::fabrication::*;
use ratemypcb_core::{
    NativeMode, Preset, ReviewOptions, ReviewScope, report_schema, review, validate_report,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::{BufReader, Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn location(record: u64) -> StructuralLocation {
    StructuralLocation {
        record,
        subrecord: None,
        byte_start: record * 10,
        byte_end: record * 10 + 9,
    }
}

fn fixture_stable_id(kind: &str, fields: &impl serde::Serialize) -> String {
    let canonical = serde_json::to_vec(&("fabrication-identity-v1", kind, fields)).unwrap();
    format!("{kind}-v1-{:x}", Sha256::digest(canonical))
}

fn record_id(kind: &str, document_id: &str, record: u64) -> String {
    let location = location(record);
    let canonical =
        serde_json::to_vec(&("fabrication-identity-v1", kind, (document_id, &location))).unwrap();
    format!("{kind}-v1-{:x}", Sha256::digest(canonical))
}

fn provenance(document_id: &str, digest: &str, record: u64) -> ManufacturingProvenance {
    ManufacturingProvenance {
        document_id: document_id.into(),
        artifact_digest: digest.into(),
        producer: "fixture-adapter".into(),
        producer_version: "1".into(),
        location: location(record),
        source_lexeme: None,
    }
}

fn retained_inventory(
    path: &str,
    kind_candidate: ManufacturingKindCandidate,
    bytes: &[u8],
) -> ManufacturingInventory {
    let artifact_digest = format!("{:x}", Sha256::digest(bytes));
    let size = bytes.len() as u64;
    ManufacturingInventory {
        inputs: vec![ManufacturingInput {
            virtual_path: path.into(),
            artifact_digest: artifact_digest.clone(),
            kind_candidate,
            size,
            original_bytes: bytes.to_vec(),
            file_started: None,
        }],
        outcomes: vec![ManufacturingInputOutcome {
            id: input_outcome_id(path, Some(&artifact_digest), kind_candidate),
            virtual_path: path.into(),
            artifact_digest: Some(artifact_digest),
            kind_candidate,
            size,
            state: ManufacturingLoadState::Retained,
            reason: None,
        }],
        aggregate_started: None,
    }
}

fn repetition(
    model: &FabricationReview,
    feature_ids: Vec<String>,
    x_count: u32,
    y_count: u32,
    x_step: i64,
    y_step: i64,
    record: u64,
) -> StepRepeat {
    let document = &model.documents[0];
    StepRepeat {
        id: record_id("repeat", &document.id, record),
        document_id: document.id.clone(),
        feature_ids,
        x_count,
        y_count,
        x_step: Picometres(x_step),
        y_step: Picometres(y_step),
        provenance: provenance(&document.id, &document.artifact_digest, record),
    }
}

fn review_options(scope: ReviewScope) -> ReviewOptions {
    ReviewOptions {
        board: None,
        schematic: None,
        bom: None,
        placement: None,
        supply_snapshot: None,
        preset: Preset::named("standard").unwrap(),
        native: NativeMode::Off,
        tool_version: "test".into(),
        scope,
        profile: None,
    }
}

fn temp_dir(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("ratemypcb-fabrication-{label}-{nonce}"));
    fs::create_dir_all(&path).unwrap();
    path
}

fn coverage_status(
    report: &ratemypcb_core::Report,
    check_id: &str,
) -> ratemypcb_core::CoverageStatus {
    let evidence_id = report
        .evidence
        .iter()
        .find(|record| record.check_id == check_id && record.kind == "coverage")
        .unwrap()
        .id
        .as_str();
    report
        .coverage
        .iter()
        .find(|coverage| coverage.id == evidence_id)
        .unwrap()
        .status
        .clone()
}

fn sample_model() -> FabricationReview {
    let digest = "1".repeat(64);
    let document_id = document_id(&digest, DocumentFormat::Gerber).unwrap();
    let second_digest = "3".repeat(64);
    let second_document_id =
        ratemypcb_core::fabrication::document_id(&second_digest, DocumentFormat::Excellon).unwrap();
    let layer_provenance = provenance(&document_id, &digest, 2);
    let layer_id = layer_id(
        &document_id,
        Some("F.Cu"),
        LayerRole::Copper,
        LayerSide::Top,
        Some(1),
        Authority::X2,
        &layer_provenance.location,
    );
    let feature_provenance = provenance(&document_id, &digest, 3);
    let feature_id = feature_id(
        &document_id,
        &layer_id,
        "line",
        &feature_provenance.location,
    );
    let tool_provenance = provenance(&document_id, &digest, 4);
    let tool_id = tool_id(&document_id, "Aperture:D10", &tool_provenance.location);
    let aperture_id = aperture_id(
        &document_id,
        ApertureShape::Circle,
        &tool_provenance.location,
    );

    let mut model = FabricationReview {
        status: FabricationStatus::Partial,
        product: Some(ProductIdentity {
            name: Some("project-a".into()),
            revision: Some("r1".into()),
            part_number: None,
            authority: Authority::Explicit,
            provenance: vec![provenance(&document_id, &digest, 1)],
        }),
        documents: vec![
            ManufacturingDocument {
                id: document_id.clone(),
                virtual_path: "fab/board.gtl".into(),
                artifact_digest: digest.clone(),
                format: DocumentFormat::Gerber,
                adapter: "fixture-adapter".into(),
                adapter_version: "1".into(),
                parse_status: ParseStatus::Partial,
                numeric_format: Some(
                    SourceNumericFormat::new(SourceUnit::Millimetre, 3, 6).unwrap(),
                ),
                metrics: DocumentMetrics {
                    raw_bytes: 200,
                    records: 20,
                    lexical_tokens: 20,
                    metadata_bytes: 20,
                    max_line_bytes: 40,
                    max_text_bytes: 12,
                    max_numeric_bytes: 8,
                    max_nesting: 1,
                    max_aperture_nesting: 0,
                },
            },
            ManufacturingDocument {
                id: second_document_id,
                virtual_path: "fab/board.drl".into(),
                artifact_digest: second_digest,
                format: DocumentFormat::Excellon,
                adapter: "fixture-adapter".into(),
                adapter_version: "1".into(),
                parse_status: ParseStatus::Partial,
                numeric_format: Some(
                    SourceNumericFormat::new(SourceUnit::Millimetre, 3, 6).unwrap(),
                ),
                metrics: DocumentMetrics {
                    raw_bytes: 100,
                    records: 5,
                    lexical_tokens: 10,
                    metadata_bytes: 10,
                    max_line_bytes: 20,
                    max_text_bytes: 10,
                    max_numeric_bytes: 8,
                    max_nesting: 1,
                    max_aperture_nesting: 0,
                },
            },
        ],
        layers: vec![ManufacturingLayer {
            id: layer_id.clone(),
            document_id: document_id.clone(),
            name: Some("F.Cu".into()),
            role: LayerRole::Copper,
            side: LayerSide::Top,
            context: LayerContext::Board,
            polarity: LayerPolarity::Positive,
            order: Some(1),
            authority: Authority::X2,
            provenance: layer_provenance,
        }],
        tools: vec![ManufacturingTool {
            id: tool_id.clone(),
            document_id: document_id.clone(),
            code: "D10".into(),
            kind: ToolKind::Aperture,
            diameter: Some(Picometres(250_000_000)),
            plating: Plating::Unknown,
            span: None,
            provenance: tool_provenance,
        }],
        apertures: vec![ApertureDefinition {
            id: aperture_id,
            document_id: document_id.clone(),
            shape: ApertureShape::Circle,
            dimensions: vec![Picometres(250_000_000)],
            polygon_vertices: None,
            polygon_rotation_microdegrees: None,
            macro_id: None,
            macro_arguments: vec![],
            provenance: provenance(&document_id, &digest, 4),
        }],
        features: vec![
            ManufacturingFeature {
                id: feature_id.clone(),
                document_id: document_id.clone(),
                layer_id: layer_id.clone(),
                tool_id: Some(tool_id),
                polarity: LayerPolarity::Positive,
                geometry: Geometry::Line(CanonicalLine {
                    start: CanonicalPoint::new(0, 0),
                    end: CanonicalPoint::new(1_000_000_000, 2_000_000_000),
                    width: Some(Picometres(250_000_000)),
                }),
                transforms: TransformChain::default(),
                membership: FeatureMembership::TopLevel,
                provenance: feature_provenance,
            },
            ManufacturingFeature {
                id: ratemypcb_core::fabrication::feature_id(
                    &document_id,
                    &layer_id,
                    "point",
                    &location(11),
                ),
                document_id: document_id.clone(),
                layer_id: layer_id.clone(),
                tool_id: None,
                polarity: LayerPolarity::Positive,
                geometry: Geometry::Point(CanonicalPoint::new(5, 6)),
                transforms: TransformChain::default(),
                membership: FeatureMembership::TopLevel,
                provenance: provenance(&document_id, &digest, 11),
            },
        ],
        profile: Some(BoardProfile {
            contour_feature_ids: vec![feature_id.clone()],
            cutout_feature_ids: vec![],
            extents: Some(Extent {
                min: CanonicalPoint::new(0, 0),
                max: CanonicalPoint::new(1_000_000_000, 2_000_000_000),
            }),
            provenance: vec![provenance(&document_id, &digest, 5)],
        }),
        connectivity: vec![ObjectSemantics {
            feature_id: feature_id.clone(),
            net: Some("GND".into()),
            component: Some("U1".into()),
            pin: Some("1".into()),
            provenance: provenance(&document_id, &digest, 6),
        }],
        assembly: AssemblyEvidence {
            placements: vec![AssemblyPlacement {
                reference: "U1".into(),
                side: LayerSide::Top,
                position: CanonicalPoint::new(100, 200),
                rotation_microdegrees: 90_000_000,
                provenance: provenance(&document_id, &digest, 7),
            }],
            mask_layer_ids: vec![layer_id.clone()],
            paste_layer_ids: vec![],
        },
        construction: ConstructionEvidence {
            layers: vec![ConstructionLayer {
                layer_id: Some(layer_id),
                material: Some("copper".into()),
                thickness: Some(Picometres(35_000_000)),
                authority: Authority::NativeSource,
                provenance: provenance(&document_id, &digest, 8),
            }],
            total_thickness: None,
            finish: None,
        },
        constraints: vec![ManufacturingConstraint {
            id: constraint_id(
                &document_id,
                ConstraintKind::MinimumTrackWidth,
                &location(9),
            ),
            kind: ConstraintKind::MinimumTrackWidth,
            value: Some(Picometres(100_000_000)),
            declared_value: None,
            authority: Authority::Explicit,
            provenance: provenance(&document_id, &digest, 9),
        }],
        capabilities: CapabilityLedger {
            records: vec![
                CapabilityRecord {
                    id: CapabilityId::GeometryLines,
                    state: CapabilityState::Complete,
                    authority: Authority::X2,
                    document_ids: vec![document_id.clone()],
                    provenance: vec![provenance(&document_id, &digest, 3)],
                    detail: "all supported lines retained".into(),
                },
                CapabilityRecord {
                    id: CapabilityId::Connectivity,
                    state: CapabilityState::Partial,
                    authority: Authority::X2,
                    document_ids: vec![document_id.clone()],
                    provenance: vec![provenance(&document_id, &digest, 6)],
                    detail: "one object has attributes".into(),
                },
            ],
        },
        omissions: vec![Omission {
            id: "omission-v1-connectivity".into(),
            kind: OmissionKind::MissingSemanticRecord,
            affected_capabilities: vec![CapabilityId::Connectivity],
            provenance: provenance(&document_id, &digest, 10),
            detail: "other objects did not provide attributes".into(),
        }],
        conflicts: vec![],
        warnings: vec![
            ManufacturingWarning {
                code: "fixture-warning".into(),
                message: "human diagnostic".into(),
                provenance: Some(provenance(&document_id, &digest, 10)),
            },
            ManufacturingWarning {
                code: "second-warning".into(),
                message: "second human diagnostic".into(),
                provenance: None,
            },
        ],
        ..FabricationReview::default()
    };
    model.refresh_digests().unwrap();
    model
}

#[test]
fn model_fixed_point_decimal_parsing_is_exact_and_checked() {
    assert_eq!(
        Picometres::parse_decimal("1.234567891", SourceUnit::Millimetre).unwrap(),
        Picometres(1_234_567_891)
    );
    assert_eq!(
        Picometres::parse_decimal("-0.000000001", SourceUnit::Millimetre).unwrap(),
        Picometres(-1)
    );
    assert_eq!(
        Picometres::parse_decimal("1.25", SourceUnit::Inch).unwrap(),
        Picometres(31_750_000_000)
    );
    assert_eq!(
        Picometres::parse_decimal("393.700787", SourceUnit::Inch).unwrap(),
        Picometres(9_999_999_989_800)
    );
    assert_eq!(
        Picometres::parse_decimal("10000", SourceUnit::Millimetre).unwrap(),
        Picometres(MAX_COORDINATE_PM)
    );
    assert!(matches!(
        Picometres::parse_decimal("0.0000000001", SourceUnit::Millimetre),
        Err(FabricationError::TooManyDecimalPlaces { .. })
    ));
    assert!(matches!(
        Picometres::parse_decimal("0.000000001", SourceUnit::Inch),
        Err(FabricationError::FinerThanPicometre)
    ));
    assert!(matches!(
        Picometres::parse_decimal("10000.000000001", SourceUnit::Millimetre),
        Err(FabricationError::CoordinateOutOfRange)
    ));
    assert!(matches!(
        Picometres::parse_decimal(
            "9999999999999999999999999999999999999999999999999999999999999999",
            SourceUnit::Millimetre
        ),
        Err(FabricationError::ArithmeticOverflow)
    ));
    let format = SourceNumericFormat::new(SourceUnit::Millimetre, 3, 6).unwrap();
    assert_eq!(format.resolution, Picometres(1_000));
}

#[test]
fn model_transforms_keep_order_and_report_quantization() {
    let exact = TransformChain {
        operations: vec![
            TransformOperation::Mirror { x: true, y: false },
            TransformOperation::Rotate {
                microdegrees: 90_000_000,
            },
            TransformOperation::Translate {
                x: Picometres(5),
                y: Picometres(-2),
            },
        ],
    }
    .materialize(CanonicalPoint::new(3, 7))
    .unwrap();
    assert_eq!(exact.point, CanonicalPoint::new(-2, -5));
    assert!(exact.quantization.is_empty());

    let rotated = TransformChain {
        operations: vec![TransformOperation::Rotate {
            microdegrees: 45_000_000,
        }],
    }
    .materialize(CanonicalPoint::new(1_000_000, 0))
    .unwrap();
    assert!((rotated.point.x.0 - 707_107).abs() <= 3);
    assert!((rotated.point.y.0 - 707_107).abs() <= 3);
    assert_eq!(rotated.quantization.len(), 1);
    assert_eq!(rotated.quantization[0].routine, "cordic-microdegree-v1");
    assert!(rotated.quantization[0].max_error_pm > 0);

    assert!(matches!(
        TransformChain {
            operations: vec![TransformOperation::Scale {
                numerator: 1,
                denominator: 0,
            }],
        }
        .materialize(CanonicalPoint::new(1, 1)),
        Err(FabricationError::InvalidScale)
    ));
}

#[test]
fn model_validation_rejects_every_geometry_point_transformed_outside_coordinate_bounds() {
    let maximum = CanonicalPoint::new(MAX_COORDINATE_PM, 0);
    let origin = CanonicalPoint::new(0, 0);
    let base = sample_model();
    let tool_id = base.tools[0].id.clone();
    let aperture_id = base.apertures[0].id.clone();
    let arc = || CanonicalArc {
        start: origin,
        end: origin,
        center: maximum,
        direction: ArcDirection::Clockwise,
        quadrant: QuadrantMode::Multi,
        width: None,
        source_resolution: Picometres(1),
    };
    for (kind, geometry) in [
        ("point", Geometry::Point(maximum)),
        (
            "line",
            Geometry::Line(CanonicalLine {
                start: origin,
                end: maximum,
                width: None,
            }),
        ),
        ("arc", Geometry::Arc(arc())),
        (
            "contour",
            Geometry::Contour(CanonicalContour {
                segments: vec![ContourSegment::Arc(arc())],
                closed: false,
            }),
        ),
        (
            "region",
            Geometry::Region(CanonicalRegion {
                contours: vec![CanonicalContour {
                    segments: vec![ContourSegment::Arc(arc())],
                    closed: false,
                }],
            }),
        ),
        (
            "flash",
            Geometry::Flash(CanonicalFlash {
                position: maximum,
                aperture_id,
            }),
        ),
        (
            "drill",
            Geometry::Drill(DrillFeature {
                position: maximum,
                diameter: Picometres(1),
                tool_id: tool_id.clone(),
            }),
        ),
        (
            "route",
            Geometry::Route(RouteFeature {
                segments: vec![ContourSegment::Arc(arc())],
                tool_id: tool_id.clone(),
            }),
        ),
        (
            "slot",
            Geometry::Slot(SlotFeature {
                start: origin,
                end: maximum,
                width: Picometres(1),
                tool_id,
            }),
        ),
    ] {
        let mut model = base.clone();
        model.features[1].geometry = geometry;
        model.features[1].id = feature_id(
            &model.features[1].document_id,
            &model.features[1].layer_id,
            kind,
            &model.features[1].provenance.location,
        );
        model.features[1].transforms.operations = vec![TransformOperation::Scale {
            numerator: 2,
            denominator: 1,
        }];
        model.refresh_digests().unwrap();
        assert!(
            matches!(
                model.validate(),
                Err(FabricationError::CoordinateOutOfRange)
            ),
            "{kind} transformed out of range"
        );
    }
}

#[test]
fn model_identity_and_digest_are_stable_but_semantically_sensitive() {
    let model = sample_model();
    model.validate().unwrap();

    let mut reordered = model.clone();
    reordered.documents.reverse();
    reordered.features.reverse();
    reordered.warnings.reverse();
    reordered.warnings[0].message = "changed diagnostic prose".into();
    reordered.capabilities.records[0].detail = "changed capability prose".into();
    reordered.refresh_digests().unwrap();
    assert_eq!(model.package_id, reordered.package_id);
    assert_eq!(model.model_digest, reordered.model_digest);

    let mut changed = model.clone();
    if let Geometry::Line(line) = &mut changed.features[0].geometry {
        line.end.x = Picometres(line.end.x.0 + 1);
    }
    changed.refresh_digests().unwrap();
    assert_ne!(model.model_digest, changed.model_digest);
    assert_eq!(model.features[0].id, changed.features[0].id);

    let changed_digest = "2".repeat(64);
    assert_ne!(
        document_id(&"1".repeat(64), DocumentFormat::Gerber).unwrap(),
        document_id(&changed_digest, DocumentFormat::Gerber).unwrap()
    );
    assert_ne!(
        feature_id(
            &model.documents[0].id,
            &model.layers[0].id,
            "line",
            &location(3)
        ),
        feature_id(
            &model.documents[0].id,
            &model.layers[0].id,
            "arc",
            &location(3)
        )
    );
}

#[test]
fn model_quantized_transforms_cannot_claim_complete_expanded_geometry() {
    let mut model = sample_model();
    model.features[0].transforms.operations = vec![TransformOperation::Rotate {
        microdegrees: 45_000_000,
    }];
    model.capabilities.records.push(CapabilityRecord {
        id: CapabilityId::GeometryExpanded,
        state: CapabilityState::Complete,
        authority: Authority::FileContent,
        document_ids: vec![model.documents[0].id.clone()],
        provenance: vec![model.features[0].provenance.clone()],
        detail: "expanded geometry is exact".into(),
    });
    model.refresh_digests().unwrap();
    assert!(model.validate().is_err());

    model.capabilities.records.last_mut().unwrap().state = CapabilityState::Partial;
    model.refresh_digests().unwrap();
    model.validate().unwrap();
}

#[test]
fn model_inventory_requires_a_unique_exact_retained_input_bijection() {
    let mut inventory =
        retained_inventory("fab/board.gtl", ManufacturingKindCandidate::Gerber, b"M02*");
    inventory.inputs.push(inventory.inputs[0].clone());
    assert!(inventory.validate().is_err());

    let mut missing =
        retained_inventory("fab/board.gtl", ManufacturingKindCandidate::Gerber, b"M02*");
    missing.inputs.clear();
    assert!(missing.validate().is_err());
}

#[test]
fn model_load_outcome_states_require_exact_digest_reason_combinations() {
    let valid = retained_inventory("fab/board.gtl", ManufacturingKindCandidate::Gerber, b"M02*");
    for (state, digest, reason) in [
        (ManufacturingLoadState::Retained, None, None),
        (
            ManufacturingLoadState::Retained,
            valid.outcomes[0].artifact_digest.clone(),
            Some(ManufacturingLoadReason::ReadFailure),
        ),
        (ManufacturingLoadState::Omitted, None, None),
        (
            ManufacturingLoadState::Omitted,
            valid.outcomes[0].artifact_digest.clone(),
            Some(ManufacturingLoadReason::PerFileByteLimit),
        ),
        (ManufacturingLoadState::Failed, None, None),
        (
            ManufacturingLoadState::Failed,
            valid.outcomes[0].artifact_digest.clone(),
            Some(ManufacturingLoadReason::ReadFailure),
        ),
    ] {
        let mut forged = valid.clone();
        forged.inputs.clear();
        let outcome = &mut forged.outcomes[0];
        outcome.state = state;
        outcome.artifact_digest = digest;
        outcome.reason = reason;
        outcome.id = input_outcome_id(
            &outcome.virtual_path,
            outcome.artifact_digest.as_deref(),
            outcome.kind_candidate,
        );
        assert!(forged.validate().is_err(), "accepted {state:?}");

        let mut review = FabricationReview {
            input_outcomes: forged.outcomes,
            ..FabricationReview::default()
        };
        review.refresh_digests().unwrap();
        assert!(review.validate().is_err(), "report accepted {state:?}");
    }

    for state in [
        ManufacturingLoadState::Omitted,
        ManufacturingLoadState::Failed,
    ] {
        let mut outcome = valid.outcomes[0].clone();
        outcome.state = state;
        outcome.artifact_digest = None;
        outcome.reason = Some(ManufacturingLoadReason::ReadFailure);
        outcome.id = input_outcome_id(&outcome.virtual_path, None, outcome.kind_candidate);
        let inventory = ManufacturingInventory {
            inputs: vec![],
            outcomes: vec![outcome.clone()],
            aggregate_started: None,
        };
        inventory.validate().unwrap();
        let mut review = FabricationReview {
            input_outcomes: vec![outcome],
            ..FabricationReview::default()
        };
        review.refresh_digests().unwrap();
        review.validate().unwrap();
    }
}

#[test]
fn model_report_rejects_forged_load_outcome_state() {
    let root = temp_dir("forged-load-outcome");
    fs::write(root.join("board.gtl"), b"M02*").unwrap();
    let mut report = review(&root, review_options(ReviewScope::Fabrication)).unwrap();
    let outcome = &mut report.fabrication.input_outcomes[0];
    outcome.state = ManufacturingLoadState::Omitted;
    outcome.reason = Some(ManufacturingLoadReason::PerFileByteLimit);
    outcome.id = input_outcome_id(
        &outcome.virtual_path,
        outcome.artifact_digest.as_deref(),
        outcome.kind_candidate,
    );
    report.fabrication.refresh_digests().unwrap();
    assert!(validate_report(&report).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn model_tool_spans_and_provenance_locations_are_document_bounded() {
    let mut bad_span = sample_model();
    bad_span.tools[0].span = Some(LayerSpan {
        from_layer_id: Some("layer-v1-missing".into()),
        to_layer_id: None,
    });
    bad_span.refresh_digests().unwrap();
    assert!(matches!(
        bad_span.validate(),
        Err(FabricationError::DanglingReference(_))
    ));

    let mut bad_byte = sample_model();
    bad_byte.features[0].provenance.location.byte_end = bad_byte.documents[0].metrics.raw_bytes;
    bad_byte.features[0].id = feature_id(
        &bad_byte.features[0].document_id,
        &bad_byte.features[0].layer_id,
        "line",
        &bad_byte.features[0].provenance.location,
    );
    bad_byte.refresh_digests().unwrap();
    assert!(matches!(
        bad_byte.validate(),
        Err(FabricationError::InvalidProvenance(_))
    ));

    let mut bad_record = sample_model();
    bad_record.features[0].provenance.location.record = bad_record.documents[0].metrics.records;
    bad_record.features[0].id = feature_id(
        &bad_record.features[0].document_id,
        &bad_record.features[0].layer_id,
        "line",
        &bad_record.features[0].provenance.location,
    );
    bad_record.refresh_digests().unwrap();
    assert!(matches!(
        bad_record.validate(),
        Err(FabricationError::InvalidProvenance(_))
    ));
}

#[test]
fn model_conflicts_require_affected_capabilities_to_be_non_complete() {
    let mut model = sample_model();
    model.capabilities.records.push(CapabilityRecord {
        id: CapabilityId::LayerRoles,
        state: CapabilityState::Complete,
        authority: Authority::X2,
        document_ids: vec![model.documents[0].id.clone()],
        provenance: vec![provenance(
            &model.documents[0].id,
            &model.documents[0].artifact_digest,
            2,
        )],
        detail: "complete".into(),
    });
    model.conflicts.push(Conflict {
        id: "conflict-v1-complete-layer-role".into(),
        kind: ConflictKind::LayerRole,
        affected_capabilities: vec![CapabilityId::LayerRoles],
        left: ConflictFact {
            canonical_value: "copper".into(),
            authority: Authority::X2,
            provenance: provenance(
                &model.documents[0].id,
                &model.documents[0].artifact_digest,
                2,
            ),
        },
        right: ConflictFact {
            canonical_value: "profile".into(),
            authority: Authority::FilenameInference,
            provenance: provenance(
                &model.documents[0].id,
                &model.documents[0].artifact_digest,
                3,
            ),
        },
    });
    model.refresh_digests().unwrap();
    assert!(matches!(
        model.validate(),
        Err(FabricationError::InvalidConflict(_))
    ));
}

#[test]
fn model_runtime_validation_rejects_forgery_duplicates_and_bad_links() {
    let model = sample_model();
    model.validate().unwrap();

    let mut forged = model.clone();
    forged.model_digest = "0".repeat(64);
    assert!(matches!(
        forged.validate(),
        Err(FabricationError::DigestMismatch)
    ));

    let mut duplicate = model.clone();
    duplicate.features.push(duplicate.features[0].clone());
    duplicate.refresh_digests().unwrap();
    assert!(matches!(
        duplicate.validate(),
        Err(FabricationError::DuplicateId(_))
    ));

    let mut dangling = model.clone();
    dangling.features[0].layer_id = "layer-v1-missing".into();
    dangling.refresh_digests().unwrap();
    assert!(matches!(
        dangling.validate(),
        Err(FabricationError::DanglingReference(_))
    ));

    let mut bad_digest = model.clone();
    bad_digest.documents[0].artifact_digest = "A".repeat(64);
    bad_digest.refresh_digests().unwrap();
    assert!(matches!(
        bad_digest.validate(),
        Err(FabricationError::InvalidDigest(_))
    ));

    let mut bad_omission = model.clone();
    bad_omission.capabilities.records[1].state = CapabilityState::Complete;
    bad_omission.refresh_digests().unwrap();
    assert!(matches!(
        bad_omission.validate(),
        Err(FabricationError::InvalidOmission(_))
    ));

    let mut bad_conflict = model.clone();
    let provenance = bad_conflict.features[0].provenance.clone();
    bad_conflict.conflicts.push(Conflict {
        id: "conflict-v1-role".into(),
        kind: ConflictKind::LayerRole,
        affected_capabilities: vec![CapabilityId::LayerRoles],
        left: ConflictFact {
            canonical_value: "copper".into(),
            authority: Authority::X2,
            provenance: provenance.clone(),
        },
        right: ConflictFact {
            canonical_value: "profile".into(),
            authority: Authority::FilenameInference,
            provenance,
        },
    });
    bad_conflict.refresh_digests().unwrap();
    assert!(matches!(
        bad_conflict.validate(),
        Err(FabricationError::InvalidConflict(_))
    ));
}

#[test]
fn model_step_repeat_exact_boundary_counts_original_instances_once() {
    let mut model = sample_model();
    model.features.truncate(1);
    model.refresh_digests().unwrap();
    let base_allocation = model.estimated_allocation_bytes;
    model.repetitions = vec![repetition(
        &model,
        vec![model.features[0].id.clone()],
        475,
        883,
        0,
        0,
        12,
    )];
    model.refresh_digests().unwrap();
    assert!(
        model.estimated_allocation_bytes
            >= base_allocation + (MANUFACTURING_LIMITS.geometry_features as u64 - 1) * 8
    );
    model.validate().unwrap();
}

#[test]
fn model_step_repeat_multiplies_feature_count_and_aggregates_repeats() {
    let mut two_features = sample_model();
    two_features.repetitions = vec![repetition(
        &two_features,
        two_features
            .features
            .iter()
            .map(|feature| feature.id.clone())
            .collect(),
        475,
        883,
        0,
        0,
        12,
    )];
    two_features.refresh_digests().unwrap();
    assert!(matches!(
        two_features.validate(),
        Err(FabricationError::LimitExceeded { .. })
    ));

    let mut aggregate = sample_model();
    aggregate.repetitions = vec![
        repetition(
            &aggregate,
            vec![aggregate.features[0].id.clone()],
            137,
            730,
            0,
            0,
            12,
        ),
        repetition(
            &aggregate,
            vec![aggregate.features[1].id.clone()],
            331,
            965,
            0,
            0,
            13,
        ),
    ];
    aggregate.refresh_digests().unwrap();
    aggregate.validate().unwrap();

    aggregate.repetitions[1].x_count = 332;
    aggregate.refresh_digests().unwrap();
    assert!(matches!(
        aggregate.validate(),
        Err(FabricationError::LimitExceeded { .. })
    ));
}

#[test]
fn model_step_repeat_rejects_step_and_repeated_coordinate_overflow() {
    let mut invalid_step = sample_model();
    invalid_step.repetitions = vec![repetition(
        &invalid_step,
        vec![invalid_step.features[0].id.clone()],
        2,
        1,
        MAX_COORDINATE_PM + 1,
        0,
        12,
    )];
    invalid_step.refresh_digests().unwrap();
    assert!(matches!(
        invalid_step.validate(),
        Err(FabricationError::CoordinateOutOfRange)
    ));

    let mut multiplication_overflow = sample_model();
    multiplication_overflow.repetitions = vec![repetition(
        &multiplication_overflow,
        vec![multiplication_overflow.features[0].id.clone()],
        u32::MAX,
        1,
        MAX_COORDINATE_PM,
        0,
        12,
    )];
    multiplication_overflow.refresh_digests().unwrap();
    assert!(matches!(
        multiplication_overflow.validate(),
        Err(FabricationError::LimitExceeded { .. } | FabricationError::ArithmeticOverflow)
    ));

    let mut invalid_offset = sample_model();
    invalid_offset.repetitions = vec![repetition(
        &invalid_offset,
        vec![invalid_offset.features[0].id.clone()],
        2,
        1,
        MAX_COORDINATE_PM,
        0,
        12,
    )];
    invalid_offset.refresh_digests().unwrap();
    assert!(matches!(
        invalid_offset.validate(),
        Err(FabricationError::CoordinateOutOfRange)
    ));
}

#[test]
fn model_limits_are_exact_serialized_and_fail_closed() {
    assert_eq!(MANUFACTURING_LIMITS.recognized_files, 256);
    assert_eq!(MANUFACTURING_LIMITS.raw_bytes_per_file, 4 * 1024 * 1024);
    assert_eq!(MANUFACTURING_LIMITS.raw_bytes_aggregate, 20 * 1024 * 1024);
    assert_eq!(MANUFACTURING_LIMITS.records_per_file, 400_000);
    assert_eq!(MANUFACTURING_LIMITS.records_aggregate, 1_000_000);
    assert_eq!(MANUFACTURING_LIMITS.lexical_tokens_per_file, 1_000_000);
    assert_eq!(MANUFACTURING_LIMITS.lexical_tokens_aggregate, 2_000_000);
    assert_eq!(MANUFACTURING_LIMITS.max_line_bytes, 16 * 1024);
    assert_eq!(MANUFACTURING_LIMITS.max_text_bytes, 4 * 1024);
    assert_eq!(MANUFACTURING_LIMITS.metadata_bytes_per_file, 64 * 1024);
    assert_eq!(MANUFACTURING_LIMITS.max_numeric_bytes, 64);
    assert_eq!(MANUFACTURING_LIMITS.max_decimal_places, 9);
    assert_eq!(MANUFACTURING_LIMITS.max_coordinate_pm, 10_000_000_000_000);
    assert_eq!(MANUFACTURING_LIMITS.max_nesting, 32);
    assert_eq!(MANUFACTURING_LIMITS.max_aperture_nesting, 16);
    assert_eq!(MANUFACTURING_LIMITS.apertures, 10_000);
    assert_eq!(MANUFACTURING_LIMITS.macros, 1_024);
    assert_eq!(MANUFACTURING_LIMITS.macro_variables, 1_024);
    assert_eq!(MANUFACTURING_LIMITS.operations_per_macro, 4_096);
    assert_eq!(MANUFACTURING_LIMITS.strict_tool_max, 99);
    assert_eq!(MANUFACTURING_LIMITS.geometry_features, 419_425);
    assert_eq!(MANUFACTURING_LIMITS.contour_vertices, 1_000_000);
    assert_eq!(MANUFACTURING_LIMITS.drill_route_features, 100_000);
    assert_eq!(MANUFACTURING_LIMITS.repeat_factor, 1_000);
    assert_eq!(
        MANUFACTURING_LIMITS.canonical_allocation_bytes,
        256 * 1024 * 1024
    );
    assert_eq!(MANUFACTURING_LIMITS.file_timeout_ms, 5_000);
    assert_eq!(MANUFACTURING_LIMITS.aggregate_timeout_ms, 30_000);

    let mut excessive = sample_model();
    excessive.documents[0].metrics.max_nesting = 33;
    excessive.refresh_digests().unwrap();
    assert!(matches!(
        excessive.validate(),
        Err(FabricationError::LimitExceeded { .. })
    ));

    let mut records = sample_model();
    records.documents[0].metrics.records = MANUFACTURING_LIMITS.records_per_file + 1;
    records.refresh_digests().unwrap();
    assert!(matches!(
        records.validate(),
        Err(FabricationError::LimitExceeded { .. })
    ));

    let mut text = sample_model();
    text.warnings[0].message = "x".repeat(MANUFACTURING_LIMITS.max_text_bytes + 1);
    text.refresh_digests().unwrap();
    assert!(matches!(
        text.validate(),
        Err(FabricationError::LimitExceeded { .. })
    ));

    let mut allocation = sample_model();
    allocation.estimated_allocation_bytes = MANUFACTURING_LIMITS.canonical_allocation_bytes + 1;
    assert!(matches!(
        allocation.validate(),
        Err(FabricationError::AllocationEstimateMismatch | FabricationError::LimitExceeded { .. })
    ));
}

#[test]
fn model_geometry_contract_preserves_arcs_regions_drills_routes_and_slots() {
    let point = CanonicalPoint::new(10, 20);
    let line = CanonicalLine {
        start: point,
        end: CanonicalPoint::new(30, 40),
        width: Some(Picometres(5)),
    };
    let arc = CanonicalArc {
        start: point,
        end: CanonicalPoint::new(20, 10),
        center: CanonicalPoint::new(10, 10),
        direction: ArcDirection::Clockwise,
        quadrant: QuadrantMode::Multi,
        width: Some(Picometres(5)),
        source_resolution: Picometres(1),
    };
    let contour = CanonicalContour {
        segments: vec![
            ContourSegment::Line(line.clone()),
            ContourSegment::Arc(arc.clone()),
        ],
        closed: true,
    };
    let geometries = vec![
        Geometry::Point(point),
        Geometry::Line(line),
        Geometry::Arc(arc),
        Geometry::Contour(contour.clone()),
        Geometry::Region(CanonicalRegion {
            contours: vec![contour],
        }),
        Geometry::Flash(CanonicalFlash {
            position: point,
            aperture_id: "aperture-v1-reference".into(),
        }),
        Geometry::Drill(DrillFeature {
            position: point,
            diameter: Picometres(10),
            tool_id: "tool-v1-reference".into(),
        }),
        Geometry::Route(RouteFeature {
            segments: vec![],
            tool_id: "tool-v1-reference".into(),
        }),
        Geometry::Slot(SlotFeature {
            start: point,
            end: CanonicalPoint::new(20, 30),
            width: Picometres(10),
            tool_id: "tool-v1-reference".into(),
        }),
    ];
    let encoded = serde_json::to_string(&geometries).unwrap();
    assert!(encoded.contains("\"arc\""));
    assert!(encoded.contains("\"region\""));
    assert!(encoded.contains("\"drill\""));
    assert!(encoded.contains("\"route\""));
    assert!(encoded.contains("\"slot\""));
    assert!(!encoded.contains("f32"));

    assert!(matches!(
        TransformChain {
            operations: vec![TransformOperation::Translate {
                x: Picometres(i64::MAX),
                y: Picometres(0),
            }],
        }
        .materialize(CanonicalPoint::new(1, 0)),
        Err(FabricationError::ArithmeticOverflow)
    ));
    assert!(serde_json::from_str::<CapabilityState>("\"invented\"").is_err());
}

#[test]
fn model_generated_schema_structurally_excludes_runtime_incompatible_values() {
    let schema = report_schema();
    let definitions = schema["$defs"].as_object().unwrap();
    let fabrication = &definitions["fabricationReview"];
    assert_eq!(fabrication["additionalProperties"], false);
    for (property, definition) in [
        ("layers", "manufacturingLayer"),
        ("tools", "manufacturingTool"),
        ("features", "manufacturingFeature"),
        ("connectivity", "objectSemantics"),
        ("constraints", "manufacturingConstraint"),
        ("warnings", "manufacturingWarning"),
    ] {
        assert_eq!(
            fabrication["properties"][property]["items"]["$ref"],
            format!("#/$defs/{definition}")
        );
    }
    for definition in [
        "manufacturingLayer",
        "manufacturingTool",
        "manufacturingFeature",
        "objectSemantics",
        "assemblyEvidence",
        "constructionEvidence",
        "manufacturingConstraint",
        "capabilityRecord",
        "manufacturingOmission",
        "manufacturingConflict",
        "manufacturingWarning",
        "stepRepeat",
    ] {
        let value = &definitions[definition];
        assert_eq!(value["type"], "object", "{definition}");
        assert_eq!(value["additionalProperties"], false, "{definition}");
        assert!(
            !value["required"].as_array().unwrap().is_empty(),
            "{definition}"
        );
    }
    let geometry = &definitions["geometry"]["oneOf"];
    assert_eq!(geometry.as_array().unwrap().len(), 9);
    for variant in geometry.as_array().unwrap() {
        assert_eq!(variant["type"], "object");
        assert_eq!(variant["additionalProperties"], false);
        assert_eq!(variant["required"], serde_json::json!(["kind", "value"]));
        assert!(variant["properties"]["kind"]["const"].is_string());
        assert!(variant["properties"]["value"]["$ref"].is_string());
    }
    let outcome = &definitions["manufacturingInputOutcome"];
    assert_eq!(outcome["additionalProperties"], false);
    assert!(
        outcome["properties"]["state"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .any(|state| state == "failed")
    );
    assert!(
        !outcome["allOf"][0]["else"]["properties"]["reason"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason == "")
    );

    let baseline = serde_json::to_value(sample_model()).unwrap();
    serde_json::from_value::<FabricationReview>(baseline.clone()).unwrap();
    for (label, mutate) in [
        (
            "scalar layer",
            (|value: &mut serde_json::Value| value["layers"] = serde_json::json!([42]))
                as fn(&mut serde_json::Value),
        ),
        ("null layer", |value: &mut serde_json::Value| {
            value["layers"] = serde_json::json!([null]);
        }),
        ("empty geometry", |value: &mut serde_json::Value| {
            value["features"][0]["geometry"] = serde_json::json!({});
        }),
        ("malformed assembly", |value: &mut serde_json::Value| {
            value["assembly"] = serde_json::json!({
                "placements": [42],
                "maskLayerIds": [],
                "pasteLayerIds": []
            });
        }),
        ("empty outcome reason", |value: &mut serde_json::Value| {
            value["inputOutcomes"] = serde_json::json!([{
                "id": format!("input-v1-{}", "0".repeat(64)),
                "virtualPath": "board.gtl",
                "artifactDigest": null,
                "kindCandidate": "gerber",
                "size": 1,
                "state": "omitted",
                "reason": ""
            }]);
        }),
    ] {
        let mut mutation = baseline.clone();
        mutate(&mut mutation);
        assert!(
            serde_json::from_value::<FabricationReview>(mutation).is_err(),
            "runtime accepted {label}"
        );
    }
}

#[test]
fn model_generated_schema_is_semantically_and_byte_equal() {
    let checked = include_str!("../../../schemas/report-2.0.json");
    let generated = format!(
        "{}\n",
        serde_json::to_string_pretty(&report_schema()).unwrap()
    );
    assert_eq!(generated.as_bytes(), checked.as_bytes());
    let parsed: serde_json::Value = serde_json::from_str(checked).unwrap();
    assert_eq!(parsed, report_schema());
    assert_eq!(
        parsed["properties"]["fabrication"]["$ref"],
        "#/$defs/fabricationReview"
    );
}

#[test]
fn model_report_integration_retains_native_facts_and_revalidates_digest() {
    let board =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/narrow-board.kicad_pcb");
    let mut report = review(
        &board,
        ReviewOptions {
            board: None,
            schematic: None,
            bom: None,
            placement: None,
            supply_snapshot: None,
            preset: Preset::named("standard").unwrap(),
            native: NativeMode::Off,
            tool_version: "test".into(),
            scope: ReviewScope::Design,
            profile: None,
        },
    )
    .unwrap();
    assert_eq!(report.fabrication.status, FabricationStatus::Partial);
    assert_eq!(
        package_capability(&report.fabrication, CapabilityId::NativeKicadFacts),
        CapabilityState::Complete
    );
    assert_eq!(
        package_capability(&report.fabrication, CapabilityId::PackageReconciliation),
        CapabilityState::NotProvided
    );
    validate_report(&report).unwrap();
    report.fabrication.model_digest = "0".repeat(64);
    assert!(validate_report(&report).is_err());
}

#[test]
fn capability_dispatch_requires_every_prerequisite_to_be_complete() {
    for state in [
        CapabilityState::Partial,
        CapabilityState::NotProvided,
        CapabilityState::Unsupported,
        CapabilityState::Failed,
        CapabilityState::Stale,
        CapabilityState::Omitted,
    ] {
        let ledger = CapabilityLedger {
            records: GERBER_SYNTAX_ANALYZER
                .prerequisites
                .iter()
                .map(|id| CapabilityRecord {
                    id: *id,
                    state: if *id == CapabilityId::DocumentSyntax {
                        state
                    } else {
                        CapabilityState::Complete
                    },
                    authority: Authority::Explicit,
                    document_ids: vec![],
                    provenance: vec![],
                    detail: String::new(),
                })
                .collect(),
        };
        let outcome = dispatch_analyzer(
            GERBER_SYNTAX_ANALYZER,
            &ledger,
            Some(SemanticAnalyzerResult::Pass),
        );
        assert_eq!(
            outcome.status,
            AnalyzerDispatchStatus::NotChecked,
            "{state:?} must not dispatch a semantic pass"
        );
        assert_eq!(
            outcome.incomplete_prerequisites,
            [CapabilityId::DocumentSyntax]
        );
    }

    let absent = dispatch_analyzer(
        GERBER_SYNTAX_ANALYZER,
        &CapabilityLedger::default(),
        Some(SemanticAnalyzerResult::Pass),
    );
    assert_eq!(absent.status, AnalyzerDispatchStatus::NotChecked);
    assert_eq!(absent.incomplete_prerequisites.len(), 2);
}

#[test]
fn capability_dispatch_rejects_duplicate_prerequisites_in_both_orders() {
    for states in [
        [CapabilityState::Complete, CapabilityState::Stale],
        [CapabilityState::Stale, CapabilityState::Complete],
    ] {
        let ledger = CapabilityLedger {
            records: states
                .into_iter()
                .map(|state| CapabilityRecord {
                    id: CapabilityId::DocumentSyntax,
                    state,
                    authority: Authority::Explicit,
                    document_ids: vec![],
                    provenance: vec![],
                    detail: String::new(),
                })
                .chain(std::iter::once(CapabilityRecord {
                    id: CapabilityId::UnitsAndFormat,
                    state: CapabilityState::Complete,
                    authority: Authority::Explicit,
                    document_ids: vec![],
                    provenance: vec![],
                    detail: String::new(),
                }))
                .collect(),
        };
        let outcome = dispatch_analyzer(
            GERBER_SYNTAX_ANALYZER,
            &ledger,
            Some(SemanticAnalyzerResult::Pass),
        );
        assert_eq!(outcome.status, AnalyzerDispatchStatus::NotChecked);
        assert_eq!(
            outcome.incomplete_prerequisites,
            [CapabilityId::DocumentSyntax]
        );
    }
}

#[test]
fn capability_complete_prerequisites_preserve_semantic_result_only() {
    let ledger = CapabilityLedger {
        records: PACKAGE_GERBERS_ANALYZER
            .prerequisites
            .iter()
            .map(|id| CapabilityRecord {
                id: *id,
                state: CapabilityState::Complete,
                authority: Authority::Explicit,
                document_ids: vec![],
                provenance: vec![],
                detail: String::new(),
            })
            .collect(),
    };
    for (result, expected) in [
        (SemanticAnalyzerResult::Pass, AnalyzerDispatchStatus::Pass),
        (
            SemanticAnalyzerResult::Attention,
            AnalyzerDispatchStatus::Attention,
        ),
        (SemanticAnalyzerResult::Fail, AnalyzerDispatchStatus::Fail),
    ] {
        assert_eq!(
            dispatch_analyzer(PACKAGE_GERBERS_ANALYZER, &ledger, Some(result)).status,
            expected
        );
    }
    assert_eq!(
        dispatch_analyzer(PACKAGE_GERBERS_ANALYZER, &ledger, None).status,
        AnalyzerDispatchStatus::NotChecked,
        "capabilities cannot invent an analyzer result"
    );
    assert_eq!(
        STABLE_FABRICATION_ANALYZERS.map(|item| item.check_family),
        ["package-gerbers", "gerber-syntax", "drill-data"]
    );
}

#[test]
fn capability_adapter_result_has_no_gate_or_approval_policy_fields() {
    let value = serde_json::to_value(AdapterResult::default()).unwrap();
    let keys = value
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(keys, ["capabilities", "conflicts", "facts", "omissions"]);
    let encoded = value.to_string();
    for forbidden in ["approval", "score", "severity", "gate", "pass"] {
        assert!(!encoded.contains(forbidden), "adapter leaked {forbidden}");
    }
}

#[test]
fn capability_inventory_preserves_non_utf8_bytes_and_exact_digest() {
    let bytes = b"G04 binary \xff comment*\nM02*\n";
    let inventory = retained_inventory(
        "fab/board-F_Cu.gtl",
        ManufacturingKindCandidate::Gerber,
        bytes,
    );
    inventory.validate().unwrap();
    assert_eq!(inventory.inputs[0].original_bytes, bytes);
    assert_eq!(
        inventory.outcomes[0].artifact_digest.clone(),
        Some(format!("{:x}", Sha256::digest(bytes)))
    );
    let model = legacy_inventory_review(&inventory).unwrap();
    assert_eq!(model.status, FabricationStatus::Partial);
    assert_eq!(
        model.input_outcomes[0].state,
        ManufacturingLoadState::Retained
    );
    assert!(model.capabilities.records.iter().any(|record| {
        record.id == CapabilityId::LegacyTokenScreening && record.state == CapabilityState::Partial
    }));
}

#[test]
fn legacy_fabrication_filename_and_token_perfection_never_passes_or_builds_stackup() {
    let root = temp_dir("legacy-perfect");
    let gerber = |name: &str| {
        fs::write(
            root.join(name),
            format!("G04 {name}*\n%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.1*%\nM02*\n"),
        )
        .unwrap();
    };
    for name in [
        "board-F_Cu.gtl",
        "board-B_Cu.gbl",
        "board-Edge_Cuts.gko",
        "board-F_Mask.gts",
        "board-B_Mask.gbs",
    ] {
        gerber(name);
    }
    fs::write(
        root.join("board.drl"),
        "M48\nMETRIC\nT01C0.3\n%\nT01\nX1Y1\nM30\n",
    )
    .unwrap();

    let report = review(&root, review_options(ReviewScope::Fabrication)).unwrap();
    for check_id in ["package-gerbers", "gerber-syntax", "drill-data"] {
        assert_ne!(
            coverage_status(&report, check_id),
            ratemypcb_core::CoverageStatus::Passed,
            "legacy screening passed {check_id}"
        );
    }
    assert!(!report.approval_eligible);
    assert!(report.stackup.is_none());
    assert!(
        report
            .fabrication
            .capabilities
            .records
            .iter()
            .any(|record| {
                record.id == CapabilityId::LegacyFilenameScreening
                    && record.state == CapabilityState::Partial
                    && record.authority == Authority::FilenameInference
            })
    );
    assert!(report.limitations.iter().any(|limitation| {
        limitation.contains("filenames and browser rendering never supply authority")
    }));
    validate_report(&report).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn capability_fabrication_evidence_is_bound_to_model_digest() {
    let root = temp_dir("fabrication-evidence-binding");
    fs::write(root.join("board-F_Cu.gtl"), b"%MOMM*%\nM02*\n").unwrap();
    let mut report = review(&root, review_options(ReviewScope::Fabrication)).unwrap();
    let evidence_ids = report
        .evidence
        .iter()
        .filter(|record| {
            matches!(
                record.check_id.as_str(),
                "package-gerbers" | "gerber-syntax" | "drill-data"
            )
        })
        .map(|record| record.id.clone())
        .collect::<Vec<_>>();
    report.fabrication.capabilities.records[0].state = CapabilityState::Stale;
    report.fabrication.refresh_digests().unwrap();
    assert!(
        validate_report(&report)
            .unwrap_err()
            .to_string()
            .contains("Fabrication evidence is not bound")
    );
    assert_eq!(
        evidence_ids,
        report
            .evidence
            .iter()
            .filter(|record| {
                matches!(
                    record.check_id.as_str(),
                    "package-gerbers" | "gerber-syntax" | "drill-data"
                )
            })
            .map(|record| record.id.clone())
            .collect::<Vec<_>>()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn capability_conflict_cannot_coexist_with_forged_passed_coverage() {
    let root = temp_dir("conflict-pass");
    fs::write(root.join("board-F_Cu.gtl"), b"M02*").unwrap();
    let mut report = review(&root, review_options(ReviewScope::Fabrication)).unwrap();
    let capability = report
        .fabrication
        .capabilities
        .records
        .iter_mut()
        .find(|record| record.id == CapabilityId::DocumentSyntax)
        .unwrap();
    capability.state = CapabilityState::Complete;
    let left = capability.provenance[0].clone();
    let mut right = left.clone();
    right.location.subrecord = Some(1);
    report.fabrication.conflicts.push(Conflict {
        id: "conflict-v1-document-syntax".into(),
        kind: ConflictKind::Other,
        affected_capabilities: vec![CapabilityId::DocumentSyntax],
        left: ConflictFact {
            canonical_value: "valid".into(),
            authority: Authority::FileContent,
            provenance: left,
        },
        right: ConflictFact {
            canonical_value: "invalid".into(),
            authority: Authority::FileContent,
            provenance: right,
        },
    });
    report.fabrication.refresh_digests().unwrap();
    let evidence_id = report
        .evidence
        .iter()
        .find(|record| record.check_id == "gerber-syntax")
        .unwrap()
        .id
        .clone();
    report
        .coverage
        .iter_mut()
        .find(|coverage| coverage.id == evidence_id)
        .unwrap()
        .status = ratemypcb_core::CoverageStatus::Passed;
    assert!(validate_report(&report).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn capability_gate_mutation_rejects_a_pass_over_partial_legacy_evidence() {
    let root = temp_dir("forged-pass");
    fs::write(root.join("board-F_Cu.gtl"), "%FSLAX46Y46*%\nM02*\n").unwrap();
    let mut report = review(&root, review_options(ReviewScope::Fabrication)).unwrap();
    let evidence_id = report
        .evidence
        .iter()
        .find(|record| record.check_id == "gerber-syntax" && record.kind == "coverage")
        .unwrap()
        .id
        .clone();
    report
        .coverage
        .iter_mut()
        .find(|coverage| coverage.id == evidence_id)
        .unwrap()
        .status = ratemypcb_core::CoverageStatus::Passed;
    assert!(
        validate_report(&report)
            .unwrap_err()
            .to_string()
            .contains("incomplete capability prerequisites")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_fabrication_token_mutation_cannot_reduce_risk_or_improve_approval() {
    let root = temp_dir("legacy-mutation");
    let path = root.join("board-F_Cu.gtl");
    fs::write(&path, "%FSLAX46Y46*%\n%MOMM*%\nM02*\n").unwrap();
    let token_perfect = review(&root, review_options(ReviewScope::Fabrication)).unwrap();
    fs::write(&path, [0xff, 0x00, 0x80]).unwrap();
    let malformed = review(&root, review_options(ReviewScope::Fabrication)).unwrap();
    assert_eq!(token_perfect.observed_risk, malformed.observed_risk);
    assert!(!token_perfect.approval_eligible);
    assert!(!malformed.approval_eligible);
    assert_ne!(
        token_perfect.fabrication.input_outcomes[0].artifact_digest,
        malformed.fabrication.input_outcomes[0].artifact_digest
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_fabrication_hidden_directory_inputs_are_never_silently_skipped() {
    let root = temp_dir("hidden-directory");
    fs::create_dir(root.join(".fabrication")).unwrap();
    fs::write(root.join(".fabrication/board.gtl"), b"M02*").unwrap();
    let report = review(&root, review_options(ReviewScope::Fabrication)).unwrap();
    assert_eq!(report.fabrication.input_outcomes.len(), 1);
    assert_eq!(
        report.fabrication.input_outcomes[0].virtual_path,
        ".fabrication/board.gtl"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_fabrication_over_limit_inputs_have_explicit_absent_digests() {
    let root = temp_dir("absent-over-limit-digest");
    let file = fs::File::create(root.join("too-large.gtl")).unwrap();
    file.set_len(MANUFACTURING_LIMITS.raw_bytes_per_file + 1)
        .unwrap();
    let report = review(&root, review_options(ReviewScope::Fabrication)).unwrap();
    let outcome = &report.fabrication.input_outcomes[0];
    assert_eq!(
        outcome.reason,
        Some(ManufacturingLoadReason::PerFileByteLimit)
    );
    assert!(outcome.artifact_digest.is_none());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_fabrication_file_and_aggregate_bounds_are_explicit_outcomes() {
    let per_file_root = temp_dir("per-file-limit");
    fs::write(
        per_file_root.join("too-large.gtl"),
        vec![b'x'; MANUFACTURING_LIMITS.raw_bytes_per_file as usize + 1],
    )
    .unwrap();
    let per_file = review(&per_file_root, review_options(ReviewScope::Fabrication)).unwrap();
    assert_eq!(
        per_file.fabrication.input_outcomes[0].reason,
        Some(ManufacturingLoadReason::PerFileByteLimit)
    );
    assert_eq!(
        coverage_status(&per_file, "gerber-syntax"),
        ratemypcb_core::CoverageStatus::Failed
    );
    fs::remove_dir_all(per_file_root).unwrap();

    let aggregate_root = temp_dir("aggregate-limit");
    for index in 0..5_u8 {
        let path = aggregate_root.join(format!("layer-{index}.gtl"));
        let file = fs::File::create(path).unwrap();
        file.set_len(MANUFACTURING_LIMITS.raw_bytes_per_file)
            .unwrap();
        use std::io::{Seek, SeekFrom, Write};
        let mut file = file;
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(&[index]).unwrap();
    }
    fs::write(aggregate_root.join("overflow.gtl"), b"x").unwrap();
    let aggregate = review(&aggregate_root, review_options(ReviewScope::Fabrication)).unwrap();
    let aggregate_omission = aggregate
        .fabrication
        .input_outcomes
        .iter()
        .find(|outcome| outcome.virtual_path == "overflow.gtl")
        .unwrap();
    assert_eq!(
        aggregate_omission.reason,
        Some(ManufacturingLoadReason::AggregateByteLimit)
    );
    assert!(aggregate_omission.artifact_digest.is_none());
    fs::remove_dir_all(aggregate_root).unwrap();
}

#[test]
fn legacy_fabrication_recognized_file_limit_records_every_artifact() {
    let root = temp_dir("file-count-limit");
    for index in 0..=MANUFACTURING_LIMITS.recognized_files {
        fs::write(
            root.join(format!("layer-{index:03}.gtl")),
            index.to_string(),
        )
        .unwrap();
    }
    let report = review(&root, review_options(ReviewScope::Fabrication)).unwrap();
    assert_eq!(
        report.fabrication.input_outcomes.len(),
        MANUFACTURING_LIMITS.recognized_files + 1
    );
    assert_eq!(
        report
            .fabrication
            .input_outcomes
            .iter()
            .filter(|outcome| outcome.state == ManufacturingLoadState::Retained)
            .count(),
        MANUFACTURING_LIMITS.recognized_files
    );
    assert_eq!(
        report.fabrication.input_outcomes.last().unwrap().reason,
        Some(ManufacturingLoadReason::RecognizedFileLimit)
    );
    assert!(
        report
            .fabrication
            .input_outcomes
            .last()
            .unwrap()
            .artifact_digest
            .is_none()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_fabrication_browser_parser_is_presentation_only() {
    let core = include_str!("../src/lib.rs");
    let browser = include_str!("../../ratemypcb-cli/assets/board-view.js");
    let viewer = include_str!("../../ratemypcb-cli/assets/local-viewer.js");
    assert!(browser.contains("export function parseGerber"));
    assert!(viewer.contains("parseGerber"));
    assert!(!core.contains("parseGerber"));
    assert!(!browser.contains("approvalEligible"));
    assert!(!browser.contains("requiredEvidence"));
    assert!(!browser.contains("ratemypcb_core"));
}

const SPIKE_ROUTE_RECORD: &str = "%TF.FileFunction,NonPlated,1,4,NPTH,Route*%";

#[derive(Clone, Debug, PartialEq, Eq)]
struct SpikeNormalizationWarning {
    byte_start: usize,
    byte_end: usize,
}

#[derive(Debug)]
struct SpikeBoundary {
    original_digest: String,
    parser_copy: Vec<u8>,
    warnings: Vec<SpikeNormalizationWarning>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SpikeResolvedError {
    line: usize,
    kind: &'static str,
    detail: String,
}

#[derive(Debug, PartialEq, Eq)]
struct SpikeParseOutcome {
    original_digest: String,
    warnings: Vec<SpikeNormalizationWarning>,
    parser_records: usize,
    successful_records: usize,
    parser_error_count: usize,
    resolved_errors: Vec<SpikeResolvedError>,
    unaccounted_errors: Vec<String>,
    route_fields: Option<Vec<String>>,
}

impl SpikeParseOutcome {
    fn accepted(&self) -> bool {
        self.unaccounted_errors.is_empty() && self.parser_error_count == self.resolved_errors.len()
    }
}

fn spike_trim_spaces(mut bytes: &[u8]) -> &[u8] {
    while bytes.first() == Some(&b' ') {
        bytes = &bytes[1..];
    }
    while bytes.last() == Some(&b' ') {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

fn spike_preflight(bytes: &[u8], deadline: Duration) -> Result<SpikeBoundary, String> {
    if bytes.len() as u64 > MANUFACTURING_LIMITS.raw_bytes_per_file {
        return Err("raw-byte-limit".into());
    }
    if deadline.is_zero() {
        return Err("deadline".into());
    }
    for (index, byte) in bytes.iter().copied().enumerate() {
        if byte == b'\r' && bytes.get(index + 1) != Some(&b'\n') {
            return Err(format!("ambiguous-carriage-return:{index}"));
        }
        if (byte < b' ' && !matches!(byte, b'\r' | b'\n')) || byte == 0x7f {
            return Err(format!("control-byte:{index}"));
        }
    }

    let started = Instant::now();
    let mut parser_copy = bytes.to_vec();
    let mut warnings = Vec::new();
    let mut line_start = 0_usize;
    let mut line_count = 0_u64;
    let mut command_count = 0_u64;
    for line_with_ending in bytes.split_inclusive(|byte| *byte == b'\n') {
        if started.elapsed() >= deadline {
            return Err("deadline".into());
        }
        line_count = line_count
            .checked_add(1)
            .ok_or_else(|| "line-count-overflow".to_string())?;
        if line_count > MANUFACTURING_LIMITS.records_per_file {
            return Err("line-limit".into());
        }
        let mut content_end = line_start + line_with_ending.len();
        if line_with_ending.last() == Some(&b'\n') {
            content_end -= 1;
        }
        if content_end > line_start && bytes[content_end - 1] == b'\r' {
            content_end -= 1;
        }
        let line = &bytes[line_start..content_end];
        if line.len() > MANUFACTURING_LIMITS.max_line_bytes {
            return Err("line-text-limit".into());
        }
        command_count = command_count
            .checked_add(line.iter().filter(|byte| **byte == b'*').count() as u64)
            .ok_or_else(|| "command-count-overflow".to_string())?;
        if command_count > MANUFACTURING_LIMITS.records_per_file {
            return Err("command-limit".into());
        }
        let trimmed = spike_trim_spaces(line);
        if !trimmed.is_empty() && !trimmed.ends_with(b"*") && !trimmed.ends_with(b"*%") {
            return Err(format!("truncated-line:{line_count}"));
        }

        if line.iter().any(|byte| *byte >= 0x80) {
            let ordinary_comment = trimmed.starts_with(b"G04 ")
                && !trimmed.starts_with(b"G04 #@!")
                && trimmed.ends_with(b"*")
                && trimmed.iter().filter(|byte| **byte == b'*').count() == 1
                && !trimmed.contains(&b'%');
            if !ordinary_comment {
                return Err(format!("invalid-byte-trust-zone:{line_count}"));
            }
            let payload_start =
                line_start + (trimmed.as_ptr() as usize - line.as_ptr() as usize) + 4;
            let payload_end =
                line_start + (trimmed.as_ptr() as usize - line.as_ptr() as usize) + trimmed.len()
                    - 1;
            let mut index = line_start;
            while index < content_end {
                if bytes[index] < 0x80 {
                    index += 1;
                    continue;
                }
                if index < payload_start || index >= payload_end {
                    return Err(format!("invalid-byte-trust-zone:{line_count}"));
                }
                let start = index;
                while index < payload_end && bytes[index] >= 0x80 {
                    parser_copy[index] = b'?';
                    index += 1;
                }
                warnings.push(SpikeNormalizationWarning {
                    byte_start: start,
                    byte_end: index,
                });
            }
        }
        line_start += line_with_ending.len();
    }
    if line_start != bytes.len() {
        return Err("unaccounted-bytes".into());
    }
    std::str::from_utf8(&parser_copy).map_err(|_| "parser-copy-not-utf8".to_string())?;
    Ok(SpikeBoundary {
        original_digest: format!("{:x}", Sha256::digest(bytes)),
        parser_copy,
        warnings,
    })
}

fn spike_route_fields(line: &str) -> Option<Vec<String>> {
    if line != SPIKE_ROUTE_RECORD {
        return None;
    }
    Some(
        line.trim_start_matches('%')
            .trim_end_matches("*%")
            .split(',')
            .map(str::to_owned)
            .collect(),
    )
}

fn spike_parse(bytes: &[u8], deadline: Duration) -> Result<SpikeParseOutcome, String> {
    let boundary = spike_preflight(bytes, deadline)?;
    let (document, fatal_error) =
        match parse_gerber(BufReader::new(Cursor::new(boundary.parser_copy.as_slice()))) {
            Ok(document) => (document, None),
            Err((document, error)) => (document, Some(error.to_string())),
        };

    // Account for all parser errors first. Successful records are inspected only below.
    let parser_errors = document.errors();
    let parser_error_count = parser_errors.len();
    let raw_lines: Vec<&[u8]> = bytes.split(|byte| *byte == b'\n').collect();
    let raw_route_count = raw_lines
        .iter()
        .filter(|line| line.strip_suffix(b"\r").unwrap_or(line) == SPIKE_ROUTE_RECORD.as_bytes())
        .count();
    let mut route_fields = None;
    let mut resolved_errors = Vec::new();
    let mut unaccounted_errors = Vec::new();
    for error in parser_errors {
        let exact_route_error = match (&error.error, &error.line) {
            (ContentError::InvalidParameter { parameter }, Some((line_number, parser_line)))
                if parameter == "Route" && parser_line == SPIKE_ROUTE_RECORD =>
            {
                raw_lines
                    .get(line_number - 1)
                    .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
                    == Some(SPIKE_ROUTE_RECORD.as_bytes())
            }
            _ => false,
        };
        if exact_route_error && route_fields.is_none() && raw_route_count == 1 {
            let (line, _) = error.line.as_ref().expect("matched route context");
            route_fields = spike_route_fields(SPIKE_ROUTE_RECORD);
            resolved_errors.push(SpikeResolvedError {
                line: *line,
                kind: "exact-route-file-function",
                detail: "InvalidParameter(Route) resolved only by exact raw Route record".into(),
            });
        } else {
            unaccounted_errors.push(error.to_string());
        }
    }
    if raw_route_count != usize::from(route_fields.is_some()) {
        unaccounted_errors.push(format!(
            "route-record/parser-error mismatch: raw={raw_route_count} resolved={}",
            route_fields.is_some()
        ));
    }
    if let Some(error) = fatal_error {
        unaccounted_errors.push(format!("fatal parser error: {error}"));
    }

    // This direct field iteration is intentionally after errors() accounting. Parser filtering
    // helpers are never a success signal.
    let successful_records = document
        .commands
        .iter()
        .filter(|record| record.is_ok())
        .count();
    Ok(SpikeParseOutcome {
        original_digest: boundary.original_digest,
        warnings: boundary.warnings,
        parser_records: document.commands.len(),
        successful_records,
        parser_error_count,
        resolved_errors,
        unaccounted_errors,
        route_fields,
    })
}

fn gerber_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/fabrication/gerber")
        .join(name)
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_owned()
}

#[derive(Default)]
struct SpikeCorpusMetrics {
    corpus_available: bool,
    digests_match: bool,
    files: usize,
    clean_before_resolution: usize,
    parser_records: usize,
    resolved_errors: usize,
    unaccounted_errors: usize,
    normalization_warnings: usize,
    newline_independent_commands: bool,
}

fn spike_recommendation(metrics: &SpikeCorpusMetrics) -> &'static str {
    if metrics.corpus_available
        && metrics.digests_match
        && metrics.files == 32
        && metrics.clean_before_resolution == 31
        && metrics.parser_records == 102_909
        && metrics.resolved_errors == 1
        && metrics.unaccounted_errors == 0
        && metrics.normalization_warnings == 32
        && metrics.newline_independent_commands
    {
        "PASS"
    } else {
        "STOP"
    }
}

fn spike_supports_newline_independent_commands() -> bool {
    let Ok(bytes) = fs::read(gerber_fixture("simple-x2.gbr")) else {
        return false;
    };
    let compact: Vec<u8> = bytes
        .into_iter()
        .filter(|byte| !matches!(byte, b'\r' | b'\n'))
        .collect();
    let expected_records = compact.iter().filter(|byte| **byte == b'*').count();
    spike_parse(&compact, Duration::from_secs(5)).is_ok_and(|outcome| {
        outcome.accepted()
            && outcome.parser_records == expected_records
            && outcome.successful_records == expected_records
    })
}

fn collect_archive_gerbers(
    archive_label: &str,
    archive_bytes: &[u8],
    files: &mut Vec<(String, Vec<u8>)>,
) -> Result<usize, String> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(archive_bytes)).map_err(|error| error.to_string())?;
    if archive.len() > MANUFACTURING_LIMITS.archive_entries {
        return Err("official-archive-entry-limit".into());
    }
    let mut expanded_bytes = 0_u64;
    let mut gerber_files = 0_usize;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_owned();
        if entry.enclosed_name().is_none() {
            return Err(format!("unsafe official archive path: {name}"));
        }
        expanded_bytes = expanded_bytes
            .checked_add(entry.size())
            .ok_or_else(|| "official-archive-size-overflow".to_string())?;
        if expanded_bytes > MANUFACTURING_LIMITS.archive_expanded_bytes {
            return Err("official-archive-expanded-byte-limit".into());
        }
        if !Path::new(&name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("gbr"))
        {
            continue;
        }
        if entry.size() == 0 || entry.size() > MANUFACTURING_LIMITS.raw_bytes_per_file {
            return Err(format!("official-gerber-size-limit: {name}"));
        }
        let expected_size = entry.size();
        let mut bytes = Vec::with_capacity(
            usize::try_from(expected_size).map_err(|_| "official-gerber-size-overflow")?,
        );
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| error.to_string())?;
        if bytes.len() as u64 != expected_size {
            return Err(format!("official-gerber-size-mismatch: {name}"));
        }
        let logical_path = format!("{archive_label}/{name}");
        if files
            .iter()
            .any(|(existing, _)| existing.eq_ignore_ascii_case(&logical_path))
        {
            return Err(format!("duplicate official Gerber path: {logical_path}"));
        }
        files.push((logical_path, bytes));
        gerber_files += 1;
    }
    Ok(gerber_files)
}

#[test]
fn gerber_adoption_spike_sanitized_fixtures_account_for_every_error() {
    let simple = spike_parse(
        &fs::read(gerber_fixture("simple-x2.gbr")).unwrap(),
        Duration::from_secs(5),
    )
    .unwrap();
    assert!(simple.accepted());
    assert_eq!(simple.parser_error_count, 0);
    assert!(simple.route_fields.is_none());

    let route = spike_parse(
        &fs::read(gerber_fixture("route-file-function.gbr")).unwrap(),
        Duration::from_secs(5),
    )
    .unwrap();
    assert!(route.accepted());
    assert_eq!(route.parser_error_count, 1);
    assert_eq!(route.resolved_errors.len(), 1);
    assert_eq!(
        route
            .route_fields
            .as_ref()
            .unwrap()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["TF.FileFunction", "NonPlated", "1", "4", "NPTH", "Route"]
    );

    let unsupported = spike_parse(
        &fs::read(gerber_fixture("unsupported-semantic.gbr")).unwrap(),
        Duration::from_secs(5),
    )
    .unwrap();
    assert!(!unsupported.accepted());
    assert_eq!(unsupported.parser_error_count, 1);
    assert_eq!(unsupported.unaccounted_errors.len(), 1);
}

#[test]
fn gerber_adoption_spike_newline_insignificant_stream_is_fully_represented() {
    let bytes = fs::read(gerber_fixture("simple-x2.gbr")).unwrap();
    let control = spike_parse(&bytes, Duration::from_secs(5)).unwrap();
    let commands = bytes
        .split(|byte| *byte == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let compact = commands.concat();
    let expected_records = compact.iter().filter(|byte| **byte == b'*').count();
    let compact_outcome = spike_parse(&compact, Duration::from_secs(5)).unwrap();

    assert_eq!(expected_records, 9);
    assert_eq!(control.parser_records, expected_records);
    assert!(control.accepted());
    assert_eq!(compact_outcome.parser_records, expected_records);
    assert_eq!(compact_outcome.successful_records, expected_records);
    assert_eq!(compact_outcome.parser_error_count, 0);
    assert!(compact_outcome.accepted());
    assert!(spike_supports_newline_independent_commands());

    for insertion_index in 0..commands.len() {
        let mut mutated = Vec::with_capacity(compact.len() + 2);
        for (index, command) in commands.iter().enumerate() {
            if index == insertion_index {
                mutated.extend_from_slice(b"Q*");
            }
            mutated.extend_from_slice(command);
        }
        let outcome = spike_parse(&mutated, Duration::from_secs(5)).unwrap();
        assert_eq!(outcome.parser_records, expected_records + 1);
        assert_eq!(outcome.successful_records, expected_records);
        assert_eq!(outcome.parser_error_count, 1);
        assert_eq!(
            outcome.parser_records,
            outcome.successful_records + outcome.parser_error_count
        );
        assert!(!outcome.accepted());
    }

    let malformed = spike_parse(b"%FSLAX46Y46*%\nG*\nM02*\n", Duration::from_secs(5)).unwrap();
    assert_eq!(
        malformed.parser_records,
        malformed.successful_records + malformed.parser_error_count
    );
    assert!(!malformed.accepted());

    assert!(spike_preflight(b"G\nM02*\n", Duration::from_secs(5)).is_err());
    let unmatched_extended = b"%FSLAX46Y46*\nM02*\n";
    assert!(spike_preflight(unmatched_extended, Duration::from_secs(5)).is_ok());
    assert!(
        !spike_parse(unmatched_extended, Duration::from_secs(5))
            .unwrap()
            .accepted()
    );
}

#[test]
fn gerber_adoption_spike_exact_route_cannot_suppress_another_error() {
    let mut bytes = fs::read(gerber_fixture("route-file-function.gbr")).unwrap();
    let insertion = b"%TF.FileFunction,FutureSemantic*%\n";
    let terminator = bytes
        .windows(4)
        .position(|window| window == b"M02*")
        .unwrap();
    bytes.splice(terminator..terminator, insertion.iter().copied());
    let outcome = spike_parse(&bytes, Duration::from_secs(5)).unwrap();
    assert_eq!(outcome.resolved_errors.len(), 1);
    assert_eq!(outcome.parser_error_count, 2);
    assert_eq!(outcome.unaccounted_errors.len(), 1);
    assert!(!outcome.accepted());

    let generalized = bytes
        .windows(SPIKE_ROUTE_RECORD.len())
        .position(|window| window == SPIKE_ROUTE_RECORD.as_bytes())
        .unwrap();
    bytes[generalized + SPIKE_ROUTE_RECORD.len() - 8] = b'X';
    let outcome = spike_parse(&bytes, Duration::from_secs(5)).unwrap();
    assert!(outcome.route_fields.is_none());
    assert!(!outcome.accepted());
}

#[test]
fn gerber_adoption_spike_invalid_bytes_are_only_normalized_in_ordinary_comments() {
    let bytes = fs::read(gerber_fixture("invalid-comment-bytes.gbr")).unwrap();
    let first = spike_parse(&bytes, Duration::from_secs(5)).unwrap();
    let second = spike_parse(&bytes, Duration::from_secs(5)).unwrap();
    assert_eq!(first, second);
    assert!(first.accepted());
    assert_eq!(first.warnings.len(), 1);
    let warning = &first.warnings[0];
    assert_eq!(&bytes[warning.byte_start..warning.byte_end], &[0x96]);
    assert_eq!(
        first.original_digest,
        format!("{:x}", Sha256::digest(&bytes))
    );

    for rejected in [
        b"%TF.FileFunction,Copper,L1,To\x96*%\nM02*\n".as_slice(),
        b"%TA.AperFunction,Conductor\x96*%\nM02*\n".as_slice(),
        b"%TO.N,NET\x96*%\nM02*\n".as_slice(),
        b"%INname\x96*%\nM02*\n".as_slice(),
        b"G04 #@! TF.FileFunction,Copper\x96*\nM02*\n".as_slice(),
        b"G04 ambiguous\x96*X0Y0D01*\nM02*\n".as_slice(),
    ] {
        assert!(spike_preflight(rejected, Duration::from_secs(5)).is_err());
    }
    assert!(spike_preflight(b"G04 nul\0comment*\nM02*\n", Duration::from_secs(5)).is_err());
    assert!(spike_preflight(b"G04 truncated", Duration::from_secs(5)).is_err());
}

#[test]
fn gerber_adoption_spike_resource_and_deadline_bounds_fail_before_parser_success() {
    let exact_line = format!(
        "G04 {}*",
        "x".repeat(MANUFACTURING_LIMITS.max_line_bytes - "G04 ".len() - 1)
    );
    assert_eq!(exact_line.len(), MANUFACTURING_LIMITS.max_line_bytes);
    spike_preflight(
        format!("{exact_line}\nM02*\n").as_bytes(),
        Duration::from_secs(5),
    )
    .unwrap();

    let exact_file_line = format!(
        "G04 {}*\n",
        "x".repeat(MANUFACTURING_LIMITS.max_line_bytes - "G04 ".len() - 2)
    );
    let exact_file = exact_file_line
        .repeat(MANUFACTURING_LIMITS.raw_bytes_per_file as usize / exact_file_line.len());
    assert_eq!(
        exact_file.len(),
        MANUFACTURING_LIMITS.raw_bytes_per_file as usize
    );
    spike_preflight(exact_file.as_bytes(), Duration::from_secs(5)).unwrap();

    let exact_commands =
        b"G04 a*G04 b*\n".repeat(MANUFACTURING_LIMITS.records_per_file as usize / 2);
    spike_preflight(&exact_commands, Duration::from_secs(5)).unwrap();

    let oversized = vec![b' '; MANUFACTURING_LIMITS.raw_bytes_per_file as usize + 1];
    assert_eq!(
        spike_preflight(&oversized, Duration::from_secs(5)).unwrap_err(),
        "raw-byte-limit"
    );

    let overlong = format!(
        "G04 {}*\nM02*\n",
        "x".repeat(MANUFACTURING_LIMITS.max_line_bytes)
    );
    assert_eq!(
        spike_preflight(overlong.as_bytes(), Duration::from_secs(5)).unwrap_err(),
        "line-text-limit"
    );

    let too_many_commands =
        b"G04 a*G04 b*\n".repeat(MANUFACTURING_LIMITS.records_per_file as usize / 2 + 1);
    assert_eq!(
        spike_preflight(&too_many_commands, Duration::from_secs(5)).unwrap_err(),
        "command-limit"
    );
    assert_eq!(
        spike_preflight(b"M02*\n", Duration::ZERO).unwrap_err(),
        "deadline"
    );
}

#[test]
fn gerber_corpus_manifest_and_dependency_are_production_pinned() {
    const GIT_DEPENDENCY: &str = "gerber_parser = { git = \"https://github.com/ratemypcb/gerber-parser.git\", rev = \"54004bc52c11699b49cd287a49135380feee86b3\" }";

    let root = repository_root();
    let core_manifest = fs::read_to_string(root.join("crates/ratemypcb-core/Cargo.toml")).unwrap();
    let declarations = core_manifest
        .lines()
        .filter(|line| line.trim_start().starts_with("gerber_parser ="))
        .collect::<Vec<_>>();
    assert_eq!(declarations, [GIT_DEPENDENCY]);
    assert!(
        core_manifest
            .split_once("[dependencies]")
            .unwrap()
            .1
            .contains(GIT_DEPENDENCY)
    );
    assert!(!core_manifest.contains("[dev-dependencies]"));
    assert!(
        !fs::read_to_string(root.join("Cargo.toml"))
            .unwrap()
            .contains("gerber_parser")
    );
    let lock = fs::read_to_string(root.join("Cargo.lock")).unwrap();
    let locked_packages = lock
        .split("[[package]]")
        .filter(|package| package.contains("name = \"gerber_parser\""))
        .collect::<Vec<_>>();
    assert_eq!(locked_packages.len(), 1);
    assert!(locked_packages[0].contains("version = \"0.5.0\""));
    assert!(locked_packages[0].contains("source = \"git+https://github.com/ratemypcb/gerber-parser.git?rev=54004bc52c11699b49cd287a49135380feee86b3#54004bc52c11699b49cd287a49135380feee86b3\""));
    let forbidden_oracle = ["gerber", "x2"].concat();
    assert!(!lock.contains(&forbidden_oracle));

    let src = root.join("crates/ratemypcb-core/src");
    for entry in fs::read_dir(src).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|extension| extension == "rs") {
            let text = fs::read_to_string(&path).unwrap();
            if path
                .file_name()
                .is_some_and(|name| name == "fabrication.rs")
            {
                assert!(text.contains("use gerber_parser"));
            } else {
                assert!(!text.contains("gerber_parser"));
            }
            assert!(!text.contains(&forbidden_oracle));
        }
    }

    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(gerber_fixture("manifest.json")).unwrap()).unwrap();
    for fixture in manifest["fixtures"].as_array().unwrap() {
        assert_eq!(fixture["origin"], "project-authored");
        assert_eq!(fixture["license"], "MIT OR Apache-2.0");
        let path = gerber_fixture(fixture["path"].as_str().unwrap());
        let bytes = fs::read(path).unwrap();
        assert_eq!(
            fixture["sha256"].as_str().unwrap(),
            format!("{:x}", Sha256::digest(&bytes))
        );
        let outcome = spike_parse(&bytes, Duration::from_secs(5)).unwrap();
        assert_eq!(
            fixture["expectedParserRecords"].as_u64().unwrap() as usize,
            outcome.parser_records
        );
        assert_eq!(
            fixture["expectedParserErrors"].as_u64().unwrap() as usize,
            outcome.parser_error_count
        );
        assert_eq!(
            fixture["expectedResolvedErrors"].as_u64().unwrap() as usize,
            outcome.resolved_errors.len()
        );
        assert_eq!(
            fixture["expectedNormalizationWarnings"].as_u64().unwrap() as usize,
            outcome.warnings.len()
        );
        if let Some(expected) = fixture.get("expectedRouteFields") {
            assert_eq!(expected, &serde_json::json!(outcome.route_fields));
        }
        if let Some(expected) = fixture.get("expectedWarningSpans") {
            assert_eq!(
                expected,
                &serde_json::json!(
                    outcome
                        .warnings
                        .iter()
                        .map(|warning| [warning.byte_start, warning.byte_end])
                        .collect::<Vec<_>>()
                )
            );
        }
        let expected = fixture["expectedProductionResult"].as_str().unwrap();
        assert_eq!(outcome.accepted(), expected != "parser-failed");
        let inventory = retained_inventory(
            fixture["path"].as_str().unwrap(),
            ManufacturingKindCandidate::Gerber,
            &bytes,
        );
        let production = parse_gerber_document(&inventory.inputs[0]);
        assert_eq!(production.is_ok(), expected != "parser-failed");
    }
    let source = include_str!("fabrication_release.rs");
    let production = include_str!("../src/fabrication.rs");
    for text in [source, production] {
        assert!(!text.contains(&[".commands", "()"].concat()));
        assert!(!text.contains(&[".into_commands", "()"].concat()));
    }
}

#[test]
fn gerber_adoption_spike_official_local_checkpoint() {
    let Ok(root) = std::env::var("RATEMYPCB_UCAMCO_CORPUS") else {
        assert_eq!(spike_recommendation(&SpikeCorpusMetrics::default()), "STOP");
        return;
    };
    let supplied_root = PathBuf::from(root);
    let archive_root = if supplied_root.join("fab-test-1.zip").is_file() {
        supplied_root
    } else {
        supplied_root
            .parent()
            .expect("corpus path has a parent")
            .to_owned()
    };
    let archive_1 = fs::read(archive_root.join("fab-test-1.zip")).unwrap();
    let archive_2 = fs::read(archive_root.join("fab-test-2.zip")).unwrap();
    let digests_match = format!("{:x}", Sha256::digest(&archive_1))
        == "16329fda234b7f3e95651c29e8f381f445ab00ca4872d4e40eb072122d1d7625"
        && format!("{:x}", Sha256::digest(&archive_2))
            == "28ca6f3b42931d7312d3229de07350fedacea1a785e32670a21f06817db6b007";
    if !digests_match {
        println!("OFFICIAL SUMMARY digests_match=false recommendation=STOP");
        assert_eq!(spike_recommendation(&SpikeCorpusMetrics::default()), "STOP");
        return;
    }

    let mut files = Vec::new();
    assert_eq!(
        collect_archive_gerbers("fab-test-1.zip", &archive_1, &mut files).unwrap(),
        12
    );
    assert_eq!(
        collect_archive_gerbers("fab-test-2.zip", &archive_2, &mut files).unwrap(),
        20
    );
    files.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(files.len(), 32);
    let mut parser_records = 0_usize;
    let mut clean_before_resolution = 0_usize;
    let mut resolved_errors = 0_usize;
    let mut normalization_warnings = 0_usize;
    let mut routes = Vec::new();
    for (path, bytes) in &files {
        let outcome = spike_parse(bytes, Duration::from_secs(5)).unwrap();
        parser_records += outcome.parser_records;
        clean_before_resolution += usize::from(outcome.parser_error_count == 0);
        resolved_errors += outcome.resolved_errors.len();
        normalization_warnings += outcome.warnings.len();
        for warning in &outcome.warnings {
            println!(
                "NORMALIZED {path} bytes={}..{}",
                warning.byte_start, warning.byte_end
            );
        }
        for error in &outcome.resolved_errors {
            println!(
                "RESOLVED {path} line={} kind={} detail={}",
                error.line, error.kind, error.detail
            );
        }
        if let Some(fields) = &outcome.route_fields {
            routes.push((path.clone(), fields.clone()));
        }
        assert!(
            outcome.unaccounted_errors.is_empty(),
            "{path}: {:?}",
            outcome.unaccounted_errors
        );
        assert!(outcome.accepted(), "{path}");
    }
    assert_eq!(clean_before_resolution, 31);
    assert_eq!(parser_records, 102_909);
    assert_eq!(resolved_errors, 1);
    assert_eq!(normalization_warnings, 32);
    assert_eq!(routes.len(), 1);
    assert_eq!(
        routes[0].1,
        ["TF.FileFunction", "NonPlated", "1", "4", "NPTH", "Route"]
    );
    let newline_independent_commands = spike_supports_newline_independent_commands();
    assert!(
        newline_independent_commands,
        "fork candidate must represent newline-independent commands"
    );
    assert_eq!(
        spike_recommendation(&SpikeCorpusMetrics {
            corpus_available: true,
            digests_match,
            files: files.len(),
            clean_before_resolution,
            parser_records,
            resolved_errors,
            unaccounted_errors: 0,
            normalization_warnings,
            newline_independent_commands,
        }),
        "PASS"
    );
    assert_eq!(
        spike_recommendation(&SpikeCorpusMetrics {
            corpus_available: true,
            digests_match: false,
            files: 32,
            clean_before_resolution: 31,
            parser_records: 102_909,
            resolved_errors: 1,
            unaccounted_errors: 0,
            normalization_warnings: 32,
            newline_independent_commands: true,
        }),
        "STOP"
    );
    println!(
        "OFFICIAL SUMMARY files=32 pre_resolution=31/32 parser_records=102909 resolved_errors=1 unaccounted_errors=0 normalization_warnings=32 newline_independent_commands=true recommendation=PASS"
    );
}

fn production_gerber(name: &str) -> GerberProduction {
    let bytes = fs::read(gerber_fixture(name)).unwrap();
    let inventory = retained_inventory(
        &format!("fab/{name}"),
        ManufacturingKindCandidate::Gerber,
        &bytes,
    );
    let original = inventory.inputs[0].original_bytes.clone();
    let parsed = parse_gerber_document(&inventory.inputs[0]).unwrap();
    assert_eq!(inventory.inputs[0].original_bytes, original);
    assert_eq!(
        parsed.original_digest,
        format!("{:x}", Sha256::digest(&bytes))
    );
    parsed.review.validate().unwrap();
    parsed
}

#[test]
fn gerber_production_single_pass_finalization_passes_public_validation() {
    production_gerber("simple-x2.gbr")
        .review
        .validate()
        .unwrap();
}

#[test]
fn gerber_semantics_tracer_original_digest_and_fixed_point_geometry_are_deterministic() {
    let first = production_gerber("simple-x2.gbr");
    let second = production_gerber("simple-x2.gbr");
    assert_eq!(first.accounting.parser_results, 9);
    assert_eq!(first.accounting.parser_successes, 9);
    assert_eq!(first.accounting.parser_errors, 0);
    assert_eq!(first.review.features.len(), 1);
    let Geometry::Line(line) = &first.review.features[0].geometry else {
        panic!("expected canonical line")
    };
    assert_eq!(line.start, CanonicalPoint::new(0, 0));
    assert_eq!(line.end, CanonicalPoint::new(100_000_000, 0));
    assert_eq!(line.width, Some(Picometres(200_000_000)));
    assert_eq!(first.review.model_digest, second.review.model_digest);
    assert_eq!(first.review.features[0].id, second.review.features[0].id);
    assert_eq!(first.attributes.len(), 1);
    assert_eq!(
        first
            .review
            .capabilities
            .records
            .iter()
            .find(|record| record.id == CapabilityId::X2FileAttributes)
            .unwrap()
            .state,
        CapabilityState::Partial
    );
    assert!(
        !serde_json::to_string(&first.review)
            .unwrap()
            .contains("approval")
    );
}

#[test]
fn gerber_semantics_one_to_many_parser_groups_are_exact_and_ordered() {
    let bytes =
        b"%FSLAX46Y46*%%MOMM*%%ADD10C,0.1*%G54D10*G01X0Y0D02*G75*G02X100000Y0I50000J0D01*M02*";
    let parsed = production_gerber_bytes(bytes).unwrap();
    // Eight source frames; G54D10 and both combined interpolation+coordinate frames
    // contribute a second, reconciled parser result.
    assert_eq!(parsed.accounting.parser_results, 11);
    assert_eq!(parsed.accounting.parser_successes, 11);
    assert_eq!(parsed.accounting.parser_errors, 0);
    assert_eq!(parsed.review.features.len(), 1);
}

#[test]
fn gerber_semantics_modal_apertures_arcs_and_regions_are_exact() {
    let parsed = production_gerber("modal-arcs-regions.gbr");
    assert_eq!(parsed.review.apertures.len(), 4);
    assert_eq!(parsed.review.features.len(), 7);
    assert_eq!(
        parsed
            .review
            .features
            .iter()
            .filter(|feature| matches!(feature.geometry, Geometry::Arc(_)))
            .count(),
        2
    );
    let region = parsed
        .review
        .features
        .iter()
        .find_map(|feature| match &feature.geometry {
            Geometry::Region(region) => Some(region),
            _ => None,
        })
        .unwrap();
    assert_eq!(region.contours.len(), 1);
    assert!(region.contours[0].closed);
    assert_eq!(region.contours[0].segments.len(), 4);
    assert!(parsed.extents.is_some());
    let polygon = parsed
        .review
        .apertures
        .iter()
        .find(|aperture| aperture.shape == ApertureShape::Polygon)
        .expect("polygon aperture");
    assert_eq!(polygon.polygon_vertices, Some(6));
    assert_eq!(polygon.polygon_rotation_microdegrees, Some(30_000_000));
}

#[test]
fn gerber_semantics_polygon_aperture_rotation_and_vertices_are_retained() {
    let parsed =
        production_gerber_bytes(b"%FSLAX46Y46*%%MOMM*%%ADD10P,1.0X6X-45X0.250*%D10*X0Y0D03*M02*")
            .unwrap();
    let polygon = parsed
        .review
        .apertures
        .iter()
        .find(|aperture| aperture.shape == ApertureShape::Polygon)
        .expect("polygon aperture");
    assert_eq!(polygon.polygon_vertices, Some(6));
    assert_eq!(polygon.polygon_rotation_microdegrees, Some(-45_000_000));
    assert_eq!(
        polygon.dimensions,
        vec![Picometres(1_000_000_000), Picometres(250_000_000)]
    );

    let changed =
        production_gerber_bytes(b"%FSLAX46Y46*%%MOMM*%%ADD10P,1.0X5X-30X0.250*%D10*X0Y0D03*M02*")
            .unwrap();
    let changed_polygon = changed
        .review
        .apertures
        .iter()
        .find(|aperture| aperture.shape == ApertureShape::Polygon)
        .expect("changed polygon aperture");
    assert_ne!(
        (
            polygon.polygon_vertices,
            polygon.polygon_rotation_microdegrees
        ),
        (
            changed_polygon.polygon_vertices,
            changed_polygon.polygon_rotation_microdegrees
        )
    );
    assert_ne!(parsed.review.model_digest, changed.review.model_digest);
}

#[test]
fn gerber_semantics_macro_aperture_invocation_arguments_are_retained() {
    let parsed = production_gerber_bytes(
        b"%FSLAX46Y46*%%MOMM*%%AMTST*$3=$1x$2*1,1,$3,0,0*%%ADD10TST,1.25X3*%D10*X0Y0D03*M02*",
    )
    .unwrap();
    let aperture = parsed
        .review
        .apertures
        .iter()
        .find(|aperture| aperture.shape == ApertureShape::Macro)
        .expect("macro aperture");
    assert_eq!(
        aperture.macro_arguments,
        vec![
            CanonicalRational {
                numerator: "5".into(),
                denominator: 4,
            },
            CanonicalRational {
                numerator: "3".into(),
                denominator: 1,
            },
        ]
    );
    assert!(aperture.macro_id.is_some());

    let changed = production_gerber_bytes(
        b"%FSLAX46Y46*%%MOMM*%%AMTST*$3=$1x$2*1,1,$3,0,0*%%ADD10TST,1.25X4*%D10*X0Y0D03*M02*",
    )
    .unwrap();
    let changed_aperture = changed
        .review
        .apertures
        .iter()
        .find(|aperture| aperture.shape == ApertureShape::Macro)
        .expect("changed macro aperture");
    assert_ne!(aperture.macro_arguments, changed_aperture.macro_arguments);
    assert_ne!(parsed.review.model_digest, changed.review.model_digest);
}

#[test]
fn gerber_semantics_macros_blocks_transforms_and_step_repeat_are_bounded() {
    let parsed = production_gerber("transforms-step-repeat.gbr");
    assert_eq!(parsed.review.macros.len(), 1);
    assert_eq!(parsed.review.blocks.len(), 1);
    assert_eq!(parsed.review.repetitions.len(), 1);
    assert_eq!(parsed.review.apertures.len(), 3);
    assert_eq!(parsed.review.features.len(), 4);
    assert_eq!(parsed.review.repetitions[0].x_count, 2);
    assert_eq!(parsed.review.repetitions[0].y_count, 2);
    let Geometry::Flash(transformed) = &parsed.review.features[2].geometry else {
        panic!("expected transformed flash")
    };
    assert_eq!(
        parsed.review.features[2]
            .transforms
            .materialize(transformed.position)
            .unwrap()
            .point,
        transformed.position
    );
    assert!(
        parsed.review.features[2]
            .transforms
            .operations
            .iter()
            .any(|operation| matches!(operation, TransformOperation::Mirror { x: true, y: false }))
    );
    assert!(
        parsed.review.features[2]
            .transforms
            .operations
            .iter()
            .any(|operation| matches!(
                operation,
                TransformOperation::Rotate {
                    microdegrees: 90_000_000
                }
            ))
    );
    assert!(
        parsed.review.features[2]
            .transforms
            .operations
            .iter()
            .any(|operation| matches!(
                operation,
                TransformOperation::Scale {
                    numerator: 2,
                    denominator: 1
                }
            ))
    );
    assert_eq!(
        parsed
            .review
            .capabilities
            .records
            .iter()
            .find(|record| record.id == CapabilityId::GeometryExpanded)
            .unwrap()
            .state,
        CapabilityState::Partial
    );
}

#[test]
fn gerber_semantics_load_transforms_are_aperture_only_for_flash_line_arc_block_and_sr() {
    let flash = production_gerber_bytes(
        b"%FSLAX46Y46*%%MOMM*%%ADD10R,0.2X0.4*%%LMX*%%LR90*%%LS2*%D10*X1000000Y2000000D03*M02*",
    )
    .unwrap();
    let Geometry::Flash(value) = &flash.review.features[0].geometry else {
        panic!("expected flash")
    };
    assert_eq!(
        value.position,
        CanonicalPoint::new(1_000_000_000, 2_000_000_000)
    );
    assert_eq!(
        flash.review.features[0]
            .transforms
            .materialize(value.position)
            .unwrap()
            .point,
        value.position
    );
    assert_eq!(
        flash.extents,
        Some(Extent {
            min: CanonicalPoint::new(600_000_000, 1_800_000_000),
            max: CanonicalPoint::new(1_400_000_000, 2_200_000_000),
        })
    );

    let line = production_gerber_bytes(
        b"%FSLAX46Y46*%%MOMM*%%ADD10C,0.1*%%LMX*%%LR90*%%LS2*%D10*X1000000Y2000000D02*X3000000Y2000000D01*M02*",
    )
    .unwrap();
    let Geometry::Line(value) = &line.review.features[0].geometry else {
        panic!("expected line")
    };
    assert_eq!(
        value.start,
        CanonicalPoint::new(1_000_000_000, 2_000_000_000)
    );
    assert_eq!(value.end, CanonicalPoint::new(3_000_000_000, 2_000_000_000));
    assert_eq!(value.width, Some(Picometres(200_000_000)));
    assert_eq!(
        line.extents,
        Some(Extent {
            min: CanonicalPoint::new(900_000_000, 1_900_000_000),
            max: CanonicalPoint::new(3_100_000_000, 2_100_000_000),
        })
    );

    let arc = production_gerber_bytes(
        b"%FSLAX46Y46*%%MOMM*%%ADD10C,0.1*%%LMX*%%LR90*%%LS2*%D10*X1000000Y2000000D02*G75*G02*X3000000Y2000000I1000000J0D01*M02*",
    )
    .unwrap();
    let Geometry::Arc(value) = &arc.review.features[0].geometry else {
        panic!("expected arc")
    };
    assert_eq!(
        value.start,
        CanonicalPoint::new(1_000_000_000, 2_000_000_000)
    );
    assert_eq!(value.end, CanonicalPoint::new(3_000_000_000, 2_000_000_000));
    assert_eq!(
        value.center,
        CanonicalPoint::new(2_000_000_000, 2_000_000_000)
    );
    assert_eq!(value.width, Some(Picometres(200_000_000)));
    assert_eq!(
        arc.extents,
        Some(Extent {
            min: CanonicalPoint::new(900_000_000, 900_000_000),
            max: CanonicalPoint::new(3_100_000_000, 3_100_000_000),
        })
    );

    let block = production_gerber_bytes(
        b"%FSLAX46Y46*%%MOMM*%%ADD10R,0.2X0.4*%%ABD20*%D10*X1000000Y2000000D03*%AB*%%LMX*%%LR90*%%LS2*%D20*X5000000Y6000000D03*M02*",
    )
    .unwrap();
    let Geometry::Flash(inner) = &block.review.features[0].geometry else {
        panic!("expected block definition flash")
    };
    let Geometry::Flash(outer) = &block.review.features[1].geometry else {
        panic!("expected block use flash")
    };
    assert_eq!(
        inner.position,
        CanonicalPoint::new(1_000_000_000, 2_000_000_000)
    );
    assert_eq!(
        outer.position,
        CanonicalPoint::new(5_000_000_000, 6_000_000_000)
    );
    assert_eq!(
        block.extents,
        Some(Extent {
            min: CanonicalPoint::new(600_000_000, 3_800_000_000),
            max: CanonicalPoint::new(1_400_000_000, 4_200_000_000),
        })
    );

    let repeated = production_gerber_bytes(
        b"%FSLAX46Y46*%%MOMM*%%ADD10R,0.2X0.4*%%LMX*%%LR90*%%LS2*%D10*%SRX2Y2I3J4*%X1000000Y2000000D03*%SR*%M02*",
    )
    .unwrap();
    let Geometry::Flash(value) = &repeated.review.features[0].geometry else {
        panic!("expected repeated flash")
    };
    assert_eq!(
        value.position,
        CanonicalPoint::new(1_000_000_000, 2_000_000_000)
    );
    assert_eq!(
        repeated.extents,
        Some(Extent {
            min: CanonicalPoint::new(600_000_000, 1_800_000_000),
            max: CanonicalPoint::new(4_400_000_000, 6_200_000_000),
        })
    );
}

#[test]
fn gerber_semantics_units_single_quadrant_and_all_macro_primitives_are_checked() {
    let inch =
        production_gerber_bytes(b"%FSTIX24Y24*%%MOIN*%%ADD10C,0.010*%D10*X1Y1D02*X1D01*M02*")
            .unwrap();
    let Geometry::Line(line) = &inch.review.features[0].geometry else {
        panic!("expected inch line")
    };
    assert_eq!(
        line.start,
        CanonicalPoint::new(254_000_000_000, 254_000_000_000)
    );
    assert_eq!(
        line.end,
        CanonicalPoint::new(508_000_000_000, 254_000_000_000)
    );

    let single = production_gerber_bytes(
        b"%FSLAX46Y46*%%MOMM*%%ADD10C,0.1*%D10*G74*X1000000Y0D02*G03*X0Y1000000I1000000J0D01*M02*",
    )
    .unwrap();
    let Geometry::Arc(arc) = &single.review.features[0].geometry else {
        panic!("expected single-quadrant arc")
    };
    assert_eq!(arc.center, CanonicalPoint::new(0, 0));
    assert_eq!(arc.quadrant, QuadrantMode::Single);

    let macros = production_gerber_bytes(
        b"%FSLAX46Y46*%%MOMM*%%AMALL*1,1,0.1,0,0*20,1,0.1,0,0,1,0,0*21,1,1,1,0,0,0*4,1,3,0,0,1,0,0,1,0,0,0*5,1,6,0,0,1,0*7,0,0,1,0.5,0.1,0*%%ADD10ALL*%D10*X0Y0D03*M02*",
    )
    .unwrap();
    assert_eq!(macros.review.macros[0].operations.len(), 6);
    assert_eq!(macros.review.features.len(), 1);

    for unsupported in [
        b"%FSLAX46Y46*%%MOMM*%%AMFAIL*1,1,1/0,0,0*%%ADD10FAIL*%M02*".as_slice(),
        b"%FSLAX46Y46*%%MOMM*%%AMMOIRE*6,0,0,1,0.1,0.1,2,0.1,1,0*%%ADD10MOIRE*%M02*".as_slice(),
    ] {
        assert!(production_gerber_bytes(unsupported).is_err());
    }
}

#[test]
fn gerber_semantics_exact_route_evidence_resolves_only_its_parser_error() {
    let parsed = production_gerber("route-file-function.gbr");
    assert_eq!(parsed.accounting.parser_errors, 1);
    assert_eq!(parsed.accounting.resolved_route_errors, 1);
    assert_eq!(parsed.accounting.unaccounted_errors, 0);
    assert_eq!(parsed.route_file_functions.len(), 1);
    assert_eq!(
        parsed.route_file_functions[0].fields,
        ["TF.FileFunction", "NonPlated", "1", "4", "NPTH", "Route"]
    );
    assert!(parsed.route_file_functions[0].parser_issue.resolved_route);
}

#[test]
fn gerber_semantics_comment_normalization_keeps_exact_original_span() {
    let bytes = fs::read(gerber_fixture("invalid-comment-bytes.gbr")).unwrap();
    let parsed = production_gerber("invalid-comment-bytes.gbr");
    assert_eq!(parsed.normalization_warnings.len(), 1);
    let warning = &parsed.normalization_warnings[0];
    assert_eq!(&bytes[warning.byte_start..warning.byte_end], &[0x96]);
    assert_eq!([warning.byte_start, warning.byte_end], [39, 40]);
}

fn production_gerber_bytes(bytes: &[u8]) -> Result<GerberProduction, GerberParseError> {
    production_gerber_bytes_with_timeout(
        bytes,
        Duration::from_millis(MANUFACTURING_LIMITS.file_timeout_ms),
    )
}

fn production_gerber_bytes_with_timeout(
    bytes: &[u8],
    timeout: Duration,
) -> Result<GerberProduction, GerberParseError> {
    let inventory = retained_inventory(
        "fab/generated.gbr",
        ManufacturingKindCandidate::Gerber,
        bytes,
    );
    parse_gerber_document_with_timeout(&inventory.inputs[0], timeout)
}

#[test]
fn gerber_hostile_newlines_truncations_and_insertions_never_truncate_to_success() {
    let bytes = fs::read(gerber_fixture("simple-x2.gbr")).unwrap();
    for variant in [
        bytes.clone(),
        bytes
            .iter()
            .copied()
            .filter(|byte| !matches!(byte, b'\r' | b'\n'))
            .collect(),
        bytes
            .iter()
            .flat_map(|byte| {
                if *byte == b'\n' {
                    b"\r".as_slice()
                } else {
                    std::slice::from_ref(byte)
                }
            })
            .copied()
            .collect(),
        bytes
            .iter()
            .flat_map(|byte| {
                if *byte == b'\n' {
                    b"\r\n".as_slice()
                } else {
                    std::slice::from_ref(byte)
                }
            })
            .copied()
            .collect(),
    ] {
        let parsed = production_gerber_bytes(&variant).unwrap();
        assert_eq!(parsed.review.features.len(), 1);
    }

    let compact = b"%FSLAX46Y46*%%MOMM*%%AMNL*$1=0.2*21,1,$1,0.1,0,0,0*%%ADD10NL,0.1*%%ADD11C,0.1*%D10*X0Y0D03*D11*G01X0Y0D02*X100000Y0D01*M02*";
    let baseline = production_gerber_bytes(compact).unwrap();
    let insertions = [
        b"FSL".as_slice(),
        b"Y46",
        b"$1=",
        b"21,1",
        b",0,0",
        b"G01X",
        b"X100",
    ]
    .map(|needle| {
        compact
            .windows(needle.len())
            .position(|window| window == needle)
            .unwrap()
            + 2.min(needle.len() - 1)
    });
    for separator in [b"\r".as_slice(), b"\n", b"\r\n"] {
        for insertion in insertions {
            let mut within_command = compact.to_vec();
            within_command.splice(insertion..insertion, separator.iter().copied());
            let parsed = production_gerber_bytes(&within_command).unwrap();
            assert_eq!(parsed.review.features.len(), baseline.review.features.len());
            for (actual, expected) in parsed.review.features.iter().zip(&baseline.review.features) {
                match (&actual.geometry, &expected.geometry) {
                    (Geometry::Flash(actual), Geometry::Flash(expected)) => {
                        assert_eq!(actual.position, expected.position)
                    }
                    (actual, expected) => assert_eq!(actual, expected),
                }
            }
            assert_eq!(parsed.extents, baseline.extents);
            assert_eq!(parsed.accounting, baseline.accounting);
            assert_eq!(
                parsed.original_digest,
                format!("{:x}", Sha256::digest(&within_command))
            );
        }
    }

    for terminator in bytes
        .iter()
        .enumerate()
        .filter_map(|(index, byte)| (*byte == b'*').then_some(index))
    {
        assert!(production_gerber_bytes(&bytes[..terminator]).is_err());
    }

    let commands = bytes
        .split_inclusive(|byte| *byte == b'*')
        .collect::<Vec<_>>();
    for insertion in 0..commands.len() {
        let mut mutated = Vec::new();
        for (index, command) in commands.iter().enumerate() {
            if index == insertion {
                mutated.extend_from_slice(b"Q*");
            }
            mutated.extend_from_slice(command);
        }
        assert!(production_gerber_bytes(&mutated).is_err());
    }
}

#[test]
fn gerber_hostile_invalid_bytes_and_parser_siblings_fail_closed() {
    for rejected in [
        b"%FSLAX46Y46*%%MOMM*%%TF.FileFunction,Copper,L1,To\x96*%M02*".as_slice(),
        b"%FSLAX46Y46*%%MOMM*%G04 #@! TF.FileFunction,Copper\x96*M02*".as_slice(),
        b"%FSLAX46Y46*%%MOMM*%G04 nul\0comment*M02*".as_slice(),
        b"%FSLAX46Y46*%%MOMM*%%ADD10C,0.1\x96*%M02*".as_slice(),
    ] {
        assert!(matches!(
            production_gerber_bytes(rejected),
            Err(GerberParseError::InvalidByte { .. })
        ));
    }
    assert!(matches!(
        production_gerber_bytes(b"%FSLAX46Y46*"),
        Err(GerberParseError::Framing { .. })
    ));

    let route = fs::read(gerber_fixture("route-file-function.gbr")).unwrap();
    let terminator = route
        .windows(4)
        .position(|window| window == b"M02*")
        .unwrap();
    let mut sibling = route;
    sibling.splice(
        terminator..terminator,
        b"%TF.FileFunction,FutureSemantic*%\n".iter().copied(),
    );
    let Err(GerberParseError::Parser { accounting, issues }) = production_gerber_bytes(&sibling)
    else {
        panic!("route sibling error was suppressed")
    };
    assert_eq!(accounting.parser_errors, 2);
    assert_eq!(accounting.resolved_route_errors, 1);
    assert_eq!(accounting.unaccounted_errors, 1);
    assert_eq!(issues.len(), 2);
    assert_eq!(
        issues.iter().filter(|issue| issue.resolved_route).count(),
        1
    );
}

#[test]
fn gerber_hostile_semantic_state_and_expansion_bombs_are_typed_failures() {
    for bytes in [
        b"%FSLAX46Y46*%%MOMM*%D10*X0Y0D03*M02*".as_slice(),
        b"%FSLAX46Y46*%%MOMM*%%ADD10C,0*%D10*X0Y0D01*M02*".as_slice(),
        b"%FSLAX46Y46*%%MOMM*%%ADD10C,0.1*%D10*G75*G02*X100000Y0D02*X200000Y0I0J1000D01*M02*"
            .as_slice(),
        b"%FSLAX46Y46*%%MOMM*%%ADD10C,0.1*%D10*G36*X0Y0D02*X1Y0D01*M02*".as_slice(),
        b"%FSLAX46Y46*%%MOMM*%%IPPOS*%M02*".as_slice(),
        b"%FSLAX46Y46*%%MOMM*%%ADD10C,0.1*%D10*%SRX1000Y1000I1J1*%X0Y0D03*X1Y1D03*%SR*%M02*"
            .as_slice(),
    ] {
        assert!(production_gerber_bytes(bytes).is_err());
    }
    let unsupported = fs::read(gerber_fixture("unsupported-semantic.gbr")).unwrap();
    assert!(matches!(
        production_gerber_bytes(&unsupported),
        Err(GerberParseError::Parser { .. })
    ));
    assert!(matches!(
        parse_gerber_document_with_timeout(
            &retained_inventory(
                "fab/deadline.gbr",
                ManufacturingKindCandidate::Gerber,
                b"M02*"
            )
            .inputs[0],
            Duration::ZERO
        ),
        Err(GerberParseError::Deadline { .. })
    ));
}

fn aperture_nesting_document(depth: usize) -> Vec<u8> {
    let mut bytes = b"%FSLAX46Y46*%%MOMM*%%ADD10C,0.1*%".to_vec();
    let mut selected = 10;
    for level in 0..depth {
        let code = 20 + level;
        bytes.extend_from_slice(format!("%ABD{code}*%D{selected}*X0Y0D03*%AB*%").as_bytes());
        selected = code;
    }
    bytes.extend_from_slice(b"M02*");
    bytes
}

fn nested_block_document(first: usize, second: usize) -> Vec<u8> {
    let mut bytes = b"%FSLAX46Y46*%%MOMM*%%ADD10C,0.1*%%ABD20*%D10*".to_vec();
    bytes.extend_from_slice(b"X0Y0D03*\n".repeat(first).as_slice());
    bytes.extend_from_slice(b"%AB*%%ABD21*%D20*");
    bytes.extend_from_slice(b"X0Y0D03*\n".repeat(second).as_slice());
    bytes.extend_from_slice(b"%AB*%D21*X0Y0D03*M02*");
    bytes
}

fn effective_feature_boundary_document(over: bool) -> Vec<u8> {
    let mut bytes =
        b"%FSLAX46Y46*%%MOMM*%%ADD10C,0.1*%D10*%SRX475Y883I0.1J0.1*%X0Y0D03*%SR*%".to_vec();
    if over {
        bytes.extend_from_slice(b"X0Y0D03*");
    }
    bytes.extend_from_slice(b"M02*");
    bytes
}

#[test]
fn gerber_hostile_numeric_text_coordinate_nesting_sr_and_real_deadline_limits_are_exact_and_over() {
    let exact_text = format!(
        "%FSLAX46Y46*%%MOMM*%G04 {}*M02*",
        "x".repeat(MANUFACTURING_LIMITS.max_text_bytes)
    );
    let parsed = production_gerber_bytes(exact_text.as_bytes()).unwrap();
    assert_eq!(
        parsed.review.documents[0].metrics.max_text_bytes,
        MANUFACTURING_LIMITS.max_text_bytes
    );
    let over_text = format!(
        "%FSLAX46Y46*%%MOMM*%G04 {}*M02*",
        "x".repeat(MANUFACTURING_LIMITS.max_text_bytes + 1)
    );
    assert!(matches!(
        production_gerber_bytes(over_text.as_bytes()),
        Err(GerberParseError::Resource {
            resource: "metadata-text",
            ..
        })
    ));

    production_gerber_bytes(b"%FSLAX46Y46*%%MOMM*%%LS1.123456789*%M02*").unwrap();
    assert!(matches!(
        production_gerber_bytes(b"%FSLAX46Y46*%%MOMM*%%LS1.1234567890*%M02*"),
        Err(GerberParseError::Semantic {
            reason: "too-many-decimal-places",
            ..
        })
    ));

    let exact_numeric = format!(
        "%FSLAX46Y46*%%MOMM*%%LS{}1.123456789*%M02*",
        "0".repeat(MANUFACTURING_LIMITS.max_numeric_bytes - 11)
    );
    assert_eq!(
        exact_numeric
            .split("LS")
            .nth(1)
            .unwrap()
            .split('*')
            .next()
            .unwrap()
            .len(),
        MANUFACTURING_LIMITS.max_numeric_bytes
    );
    let parsed = production_gerber_bytes(exact_numeric.as_bytes()).unwrap();
    assert_eq!(
        parsed.review.documents[0].metrics.max_numeric_bytes,
        MANUFACTURING_LIMITS.max_numeric_bytes
    );
    let over_numeric = exact_numeric.replacen("LS", "LS0", 1);
    assert!(matches!(
        production_gerber_bytes(over_numeric.as_bytes()),
        Err(GerberParseError::Resource {
            resource: "numeric-token",
            ..
        })
    ));

    production_gerber_bytes(b"%FSLAX46Y46*%%MOMM*%%ADD10C,10000*%M02*").unwrap();
    assert!(matches!(
        production_gerber_bytes(b"%FSLAX46Y46*%%MOMM*%%ADD10C,10000.000000001*%M02*"),
        Err(GerberParseError::Semantic {
            reason: "coordinate-out-of-range",
            ..
        })
    ));

    let expression = |depth: usize| format!("{}1{}", "(".repeat(depth), ")".repeat(depth));
    let exact_nesting = format!(
        "%FSLAX46Y46*%%MOMM*%%AMDEP*1,1,{},0,0*%M02*",
        expression(usize::from(MANUFACTURING_LIMITS.max_nesting) - 1)
    );
    let parsed = production_gerber_bytes(exact_nesting.as_bytes()).unwrap();
    assert_eq!(
        parsed.review.documents[0].metrics.max_nesting,
        MANUFACTURING_LIMITS.max_nesting
    );
    let over_nesting = format!(
        "%FSLAX46Y46*%%MOMM*%%AMDEP*1,1,{},0,0*%M02*",
        expression(usize::from(MANUFACTURING_LIMITS.max_nesting))
    );
    assert!(matches!(
        production_gerber_bytes(over_nesting.as_bytes()),
        Err(GerberParseError::Semantic {
            reason: "expression-nesting-limit",
            ..
        })
    ));

    let exact_aperture_nesting = production_gerber_bytes(&aperture_nesting_document(usize::from(
        MANUFACTURING_LIMITS.max_aperture_nesting,
    )))
    .unwrap();
    assert_eq!(
        exact_aperture_nesting.review.documents[0]
            .metrics
            .max_aperture_nesting,
        MANUFACTURING_LIMITS.max_aperture_nesting
    );
    assert!(matches!(
        production_gerber_bytes(&aperture_nesting_document(
            usize::from(MANUFACTURING_LIMITS.max_aperture_nesting) + 1,
        )),
        Err(GerberParseError::Resource {
            resource: "aperture-nesting",
            ..
        })
    ));

    production_gerber_bytes(
        format!(
            "%FSLAX46Y46*%%MOMM*%%ADD10C,0.1*%D10*%SRX{}Y1I0.1J0.1*%X0Y0D03*%SR*%M02*",
            MANUFACTURING_LIMITS.repeat_factor
        )
        .as_bytes(),
    )
    .unwrap();
    assert!(matches!(
        production_gerber_bytes(
            format!(
                "%FSLAX46Y46*%%MOMM*%%ADD10C,0.1*%D10*%SRX{}Y1I0.1J0.1*%X0Y0D03*%SR*%M02*",
                MANUFACTURING_LIMITS.repeat_factor + 1
            )
            .as_bytes(),
        ),
        Err(GerberParseError::Resource {
            resource: "step-repeat-factor",
            ..
        })
    ));

    let mut bomb = b"%FSLAX46Y46*%%MOMM*%".to_vec();
    bomb.extend_from_slice(b"G04 deadline*".repeat(100_000).as_slice());
    bomb.extend_from_slice(b"M02*");
    let input = retained_inventory(
        "fab/deadline-bomb.gbr",
        ManufacturingKindCandidate::Gerber,
        &bomb,
    );
    assert!(matches!(
        parse_gerber_document_with_timeout(&input.inputs[0], Duration::from_micros(50)),
        Err(GerberParseError::Deadline { .. })
    ));
}

fn aperture_count_document(count: usize) -> Vec<u8> {
    let mut bytes = b"%FSLAX46Y46*%%MOMM*%".to_vec();
    for code in 10..10 + count {
        bytes.extend_from_slice(format!("%ADD{code}C,0.1*%\n").as_bytes());
    }
    bytes.extend_from_slice(b"M02*");
    bytes
}

fn macro_count_document(count: usize) -> Vec<u8> {
    let mut bytes = b"%FSLAX46Y46*%%MOMM*%".to_vec();
    for index in 0..count {
        bytes.extend_from_slice(format!("%AMM{index}*1,1,0.1,0,0*%\n").as_bytes());
    }
    bytes.extend_from_slice(b"M02*");
    bytes
}

fn macro_variable_document(count: usize) -> Vec<u8> {
    let mut bytes = b"%FSLAX46Y46*%%MOMM*%%AMV*".to_vec();
    for variable in 1..=count {
        bytes.extend_from_slice(format!("${variable}=1*\n").as_bytes());
    }
    bytes.extend_from_slice(b"1,1,0.1,0,0*%M02*");
    bytes
}

fn macro_operation_document(count: usize) -> Vec<u8> {
    let mut bytes = b"%FSLAX46Y46*%%MOMM*%%AMO*".to_vec();
    bytes.extend_from_slice(b"0 a*\n".repeat(count).as_slice());
    bytes.extend_from_slice(b"%M02*");
    bytes
}

fn block_count_document(count: usize) -> Vec<u8> {
    let mut bytes = b"%FSLAX46Y46*%%MOMM*%%ADD10C,0.1*%".to_vec();
    for index in 0..count {
        let code = 20 + index;
        bytes.extend_from_slice(format!("%ABD{code}*%D10*X0Y0D03*%AB*%\n").as_bytes());
    }
    bytes.extend_from_slice(b"M02*");
    bytes
}

#[test]
fn gerber_hostile_aperture_macro_variable_operation_and_block_counts_are_exact_and_over() {
    let exact_apertures =
        production_gerber_bytes(&aperture_count_document(MANUFACTURING_LIMITS.apertures)).unwrap();
    assert_eq!(
        exact_apertures.review.apertures.len(),
        MANUFACTURING_LIMITS.apertures
    );
    assert!(matches!(
        production_gerber_bytes(&aperture_count_document(MANUFACTURING_LIMITS.apertures + 1)),
        Err(GerberParseError::Resource {
            resource: "apertures",
            ..
        })
    ));

    let exact_macros =
        production_gerber_bytes(&macro_count_document(MANUFACTURING_LIMITS.macros)).unwrap();
    assert_eq!(
        exact_macros.review.macros.len(),
        MANUFACTURING_LIMITS.macros
    );
    assert!(matches!(
        production_gerber_bytes(&macro_count_document(MANUFACTURING_LIMITS.macros + 1)),
        Err(GerberParseError::Resource {
            resource: "macros",
            ..
        })
    ));

    let exact_variables = production_gerber_bytes(&macro_variable_document(
        MANUFACTURING_LIMITS.macro_variables,
    ))
    .unwrap();
    assert_eq!(
        exact_variables.review.macros[0].variables.len(),
        MANUFACTURING_LIMITS.macro_variables
    );
    assert!(matches!(
        production_gerber_bytes(&macro_variable_document(
            MANUFACTURING_LIMITS.macro_variables + 1,
        )),
        Err(GerberParseError::Resource {
            resource: "macro-variables",
            ..
        })
    ));

    let exact_operations = production_gerber_bytes(&macro_operation_document(
        MANUFACTURING_LIMITS.operations_per_macro,
    ))
    .unwrap();
    assert_eq!(
        exact_operations.review.macros[0].operations.len(),
        MANUFACTURING_LIMITS.operations_per_macro
    );
    assert!(matches!(
        production_gerber_bytes(&macro_operation_document(
            MANUFACTURING_LIMITS.operations_per_macro + 1,
        )),
        Err(GerberParseError::Resource {
            resource: "macro-operations",
            ..
        })
    ));

    let exact_blocks = production_gerber_bytes_with_timeout(
        &block_count_document(MANUFACTURING_LIMITS.apertures - 1),
        Duration::from_secs(30),
    )
    .unwrap();
    assert_eq!(
        exact_blocks.review.blocks.len(),
        MANUFACTURING_LIMITS.apertures - 1
    );
    assert!(matches!(
        production_gerber_bytes_with_timeout(
            &block_count_document(MANUFACTURING_LIMITS.apertures),
            Duration::from_secs(30),
        ),
        Err(GerberParseError::Resource {
            resource: "apertures",
            ..
        })
    ));
}

#[test]
fn gerber_hostile_effective_feature_boundary_is_reachable_and_one_over_fails() {
    let exact = production_gerber_bytes(&effective_feature_boundary_document(false)).unwrap();
    assert_eq!(exact.review.features.len(), 1);
    assert_eq!(
        u64::from(exact.review.repetitions[0].x_count)
            * u64::from(exact.review.repetitions[0].y_count),
        419_425
    );

    assert!(matches!(
        production_gerber_bytes(&effective_feature_boundary_document(true)),
        Err(GerberParseError::Resource {
            resource: "expanded-features",
            observed: 419_426,
            limit: 419_425,
        })
    ));
}

#[test]
fn gerber_hostile_nested_blocks_charge_checked_feature_vertex_and_allocation_weights() {
    let exact = production_gerber_bytes(&nested_block_document(646, 647)).unwrap();
    assert_eq!(exact.review.blocks.len(), 2);
    assert_eq!(exact.review.features.len(), 1_294);

    assert!(matches!(
        production_gerber_bytes(&nested_block_document(647, 647)),
        Err(GerberParseError::Resource {
            resource: "canonical-allocation",
            ..
        })
    ));
    assert!(matches!(
        production_gerber_bytes(&nested_block_document(1_001, 1_000)),
        Err(GerberParseError::Resource {
            resource: "expanded-features",
            observed: 1_001_000,
            ..
        })
    ));

    // A separate compact production case proves block weight is multiplied again by SR.
    let repeated_block = format!(
        "%FSLAX46Y46*%%MOMM*%%ADD10C,0.1*%%ABD20*%D10*{}%AB*%D20*%SRX100Y100I0.1J0.1*%X0Y0D03*%SR*%M02*",
        "X0Y0D03*".repeat(10)
    );
    let repeated = production_gerber_bytes(repeated_block.as_bytes()).unwrap();
    assert_eq!(repeated.review.blocks.len(), 1);
    assert_eq!(repeated.review.repetitions.len(), 1);
}

#[test]
fn gerber_hostile_resource_boundaries_are_exact_and_over() {
    let mut exact_raw = Vec::with_capacity(MANUFACTURING_LIMITS.raw_bytes_per_file as usize);
    let extra = MANUFACTURING_LIMITS.raw_bytes_per_file as usize - 400_000 * 10;
    for index in 0..400_000 {
        exact_raw.extend_from_slice(if index < extra {
            b"G04 abcde*\n"
        } else {
            b"G04 abcd*\n"
        });
    }
    assert_eq!(
        exact_raw.len(),
        MANUFACTURING_LIMITS.raw_bytes_per_file as usize
    );
    let exact = GerberByteBoundary::with_timeout(&exact_raw, Duration::from_secs(30)).unwrap();
    assert_eq!(
        exact.metrics.raw_bytes,
        MANUFACTURING_LIMITS.raw_bytes_per_file
    );
    assert_eq!(exact.metrics.records, MANUFACTURING_LIMITS.records_per_file);
    let mut over_raw = exact_raw.clone();
    over_raw.push(b'\n');
    assert!(matches!(
        GerberByteBoundary::with_timeout(&over_raw, Duration::from_secs(30)),
        Err(GerberParseError::Resource {
            resource: "raw-bytes",
            ..
        })
    ));

    let over_commands = b"G04 a*\n".repeat(MANUFACTURING_LIMITS.records_per_file as usize + 1);
    assert!(matches!(
        GerberByteBoundary::with_timeout(&over_commands, Duration::from_secs(30)),
        Err(GerberParseError::Resource {
            resource: "commands",
            ..
        })
    ));

    let exact_tokens = b"G04 a+a*\n".repeat(250_000);
    let token_boundary =
        GerberByteBoundary::with_timeout(&exact_tokens, Duration::from_secs(30)).unwrap();
    assert_eq!(
        token_boundary.metrics.lexical_tokens,
        MANUFACTURING_LIMITS.lexical_tokens_per_file
    );
    let mut over_tokens = exact_tokens;
    over_tokens.extend_from_slice(b"G04 a*\n");
    assert!(matches!(
        GerberByteBoundary::with_timeout(&over_tokens, Duration::from_secs(30)),
        Err(GerberParseError::Resource {
            resource: "lexical-tokens",
            ..
        })
    ));

    let exact_line = format!(
        "G04 {}*",
        "x".repeat(MANUFACTURING_LIMITS.max_line_bytes - 5)
    );
    assert_eq!(exact_line.len(), MANUFACTURING_LIMITS.max_line_bytes);
    GerberByteBoundary::new(exact_line.as_bytes()).unwrap();
    let over_line = format!("{exact_line}x");
    assert!(matches!(
        GerberByteBoundary::new(over_line.as_bytes()),
        Err(GerberParseError::Resource {
            resource: "line-bytes",
            ..
        })
    ));

    let mut exact_metadata = b"%FSLAX46Y46*%\n%MOMM*%\n".to_vec();
    for _ in 0..16 {
        exact_metadata.extend_from_slice(b"G04 ");
        exact_metadata.extend(std::iter::repeat_n(
            b'x',
            MANUFACTURING_LIMITS.max_text_bytes,
        ));
        exact_metadata.extend_from_slice(b"*\n");
    }
    exact_metadata.extend_from_slice(b"M02*\n");
    let parsed = production_gerber_bytes(&exact_metadata).unwrap();
    assert_eq!(
        parsed.review.documents[0].metrics.metadata_bytes,
        MANUFACTURING_LIMITS.metadata_bytes_per_file
    );
    let end = exact_metadata
        .windows(4)
        .position(|window| window == b"M02*")
        .unwrap();
    exact_metadata.splice(end..end, b"G04 z*\n".iter().copied());
    assert!(matches!(
        production_gerber_bytes(&exact_metadata),
        Err(GerberParseError::Resource {
            resource: "metadata-bytes",
            ..
        })
    ));

    let numeric_over = format!(
        "%FSLAX46Y46*%%MOMM*%%LS{}*%M02*",
        "9".repeat(MANUFACTURING_LIMITS.max_numeric_bytes + 1)
    );
    assert!(matches!(
        production_gerber_bytes(numeric_over.as_bytes()),
        Err(GerberParseError::Resource {
            resource: "numeric-token",
            ..
        })
    ));
}

fn mutation_matches(error: &GerberParseError, expected: &str) -> bool {
    match expected {
        "parser-failed" => matches!(error, GerberParseError::Parser { .. }),
        "parser-failed-with-route-retained" => matches!(
            error,
            GerberParseError::Parser { accounting, issues }
                if accounting.parser_errors == 2
                    && accounting.resolved_route_errors == 1
                    && accounting.unaccounted_errors == 1
                    && issues.len() == 2
                    && issues.iter().filter(|issue| issue.resolved_route).count() == 1
                    && issues.iter().filter(|issue| !issue.resolved_route).count() == 1
        ),
        "framing-or-parser-failed" => matches!(
            error,
            GerberParseError::Framing { .. } | GerberParseError::Parser { .. }
        ),
        "framing-failed" => matches!(error, GerberParseError::Framing { .. }),
        "invalid-byte" => matches!(error, GerberParseError::InvalidByte { .. }),
        "semantic-failed" => matches!(error, GerberParseError::Semantic { .. }),
        "resource-failed" => matches!(error, GerberParseError::Resource { .. }),
        _ => false,
    }
}

#[test]
fn gerber_corpus_mutation_manifest_executes_every_case_with_its_typed_failure() {
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(gerber_fixture("mutations.json")).unwrap()).unwrap();
    assert_eq!(manifest["origin"], "project-authored");
    assert_eq!(manifest["license"], "MIT OR Apache-2.0");
    let mutations = manifest["mutations"].as_array().unwrap();
    let mut executed = BTreeSet::new();
    for mutation in mutations {
        let id = mutation["id"].as_str().unwrap();
        let expected = mutation["expected"].as_str().unwrap();
        assert!(executed.insert(id));
        match id {
            "insert-unknown-at-every-boundary" => {
                let source = fs::read(gerber_fixture("simple-x2.gbr")).unwrap();
                let mut boundaries = vec![0_usize];
                let mut cursor = 0_usize;
                while cursor < source.len() {
                    while source.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                        cursor += 1;
                    }
                    if cursor == source.len() {
                        break;
                    }
                    cursor = if source[cursor] == b'%' {
                        source[cursor + 1..]
                            .iter()
                            .position(|byte| *byte == b'%')
                            .map(|offset| cursor + offset + 2)
                            .unwrap()
                    } else {
                        source[cursor..]
                            .iter()
                            .position(|byte| *byte == b'*')
                            .map(|offset| cursor + offset + 1)
                            .unwrap()
                    };
                    boundaries.push(cursor);
                }
                for boundary in boundaries {
                    let mut bytes = source.clone();
                    bytes.splice(boundary..boundary, b"Q*".iter().copied());
                    let error = production_gerber_bytes(&bytes).unwrap_err();
                    assert!(mutation_matches(&error, expected), "{id}: {error:?}");
                }
            }
            "truncate-at-every-command" => {
                let source = fs::read(gerber_fixture("simple-x2.gbr")).unwrap();
                for truncation in source
                    .iter()
                    .enumerate()
                    .filter_map(|(index, byte)| (*byte == b'*').then_some(index))
                {
                    let error = production_gerber_bytes(&source[..truncation]).unwrap_err();
                    assert!(mutation_matches(&error, expected), "{id}: {error:?}");
                }
            }
            "route-plus-sibling-error" => {
                assert_eq!(
                    expected, "parser-failed-with-route-retained",
                    "{id}: manifest expectation drift"
                );
                let mut bytes = fs::read(gerber_fixture("route-file-function.gbr")).unwrap();
                let terminator = bytes
                    .windows(4)
                    .position(|window| window == b"M02*")
                    .unwrap();
                bytes.splice(
                    terminator..terminator,
                    b"%TF.FileFunction,FutureSemantic*%".iter().copied(),
                );
                let error = production_gerber_bytes(&bytes).unwrap_err();
                assert!(mutation_matches(&error, expected), "{id}: {error:?}");
            }
            "invalid-byte-in-extended-command" => {
                let error = production_gerber_bytes(
                    b"%FSLAX46Y46*%%MOMM*%%TF.FileFunction,Copper,L1,To\x96*%M02*",
                )
                .unwrap_err();
                assert!(mutation_matches(&error, expected), "{id}: {error:?}");
            }
            "invalid-byte-in-standard-comment" => {
                let error = production_gerber_bytes(
                    b"%FSLAX46Y46*%%MOMM*%G04 #@! TF.FileFunction,Copper\x96*M02*",
                )
                .unwrap_err();
                assert!(mutation_matches(&error, expected), "{id}: {error:?}");
            }
            "control-byte-in-comment" => {
                let error = production_gerber_bytes(b"%FSLAX46Y46*%%MOMM*%G04 control\0byte*M02*")
                    .unwrap_err();
                assert!(mutation_matches(&error, expected), "{id}: {error:?}");
            }
            "unclosed-extended-command" => {
                let error = production_gerber_bytes(b"%FSLAX46Y46*%%MOMM*").unwrap_err();
                assert!(mutation_matches(&error, expected), "{id}: {error:?}");
            }
            "undefined-aperture" => {
                let error =
                    production_gerber_bytes(b"%FSLAX46Y46*%%MOMM*%D10*X0Y0D03*M02*").unwrap_err();
                assert!(mutation_matches(&error, expected), "{id}: {error:?}");
            }
            "undefined-modal-operation" => {
                let error =
                    production_gerber_bytes(b"%FSLAX46Y46*%%MOMM*%%ADD10C,0.1*%D10*X0Y0*M02*")
                        .unwrap_err();
                assert!(mutation_matches(&error, expected), "{id}: {error:?}");
            }
            "open-region" => {
                let error = production_gerber_bytes(b"%FSLAX46Y46*%%MOMM*%G36*M02*").unwrap_err();
                assert!(mutation_matches(&error, expected), "{id}: {error:?}");
            }
            "data-after-m02" => {
                let error =
                    production_gerber_bytes(b"%FSLAX46Y46*%%MOMM*%M02*G04 after*M02*").unwrap_err();
                assert!(mutation_matches(&error, expected), "{id}: {error:?}");
            }
            "step-repeat-expansion-bomb" => {
                let error = production_gerber_bytes(
                    b"%FSLAX46Y46*%%MOMM*%%ADD10C,0.1*%D10*%SRX1000Y1000I1J1*%X0Y0D03*X1Y1D03*%SR*%M02*",
                )
                .unwrap_err();
                assert!(mutation_matches(&error, expected), "{id}: {error:?}");
            }
            _ => panic!("unimplemented mutation {id}"),
        }
    }
    assert_eq!(executed.len(), mutations.len());
    assert_eq!(executed.len(), 12);
}

#[test]
fn gerber_official_production_regression() {
    let Ok(root) = std::env::var("RATEMYPCB_UCAMCO_CORPUS") else {
        eprintln!("official Gerber corpus not run: RATEMYPCB_UCAMCO_CORPUS is unset");
        return;
    };
    let root = PathBuf::from(root);
    let archive_1 = fs::read(root.join("fab-test-1.zip")).unwrap();
    let archive_2 = fs::read(root.join("fab-test-2.zip")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&archive_1)),
        "16329fda234b7f3e95651c29e8f381f445ab00ca4872d4e40eb072122d1d7625"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(&archive_2)),
        "28ca6f3b42931d7312d3229de07350fedacea1a785e32670a21f06817db6b007"
    );

    // Members are read directly from the same verified in-memory ZIP buffers above.
    let mut files = Vec::new();
    assert_eq!(
        collect_archive_gerbers("fab-test-1.zip", &archive_1, &mut files).unwrap(),
        12
    );
    assert_eq!(
        collect_archive_gerbers("fab-test-2.zip", &archive_2, &mut files).unwrap(),
        20
    );
    files.sort_by(|left, right| left.0.cmp(&right.0));

    let mut parser_records = 0_u64;
    let mut parser_successes = 0_u64;
    let mut parser_errors = 0_u64;
    let mut resolved_routes = 0_u64;
    let mut warnings = 0_usize;
    let mut features = 0_usize;
    let mut lines = 0_usize;
    let mut arcs = 0_usize;
    let mut regions = 0_usize;
    let mut flashes = 0_usize;
    let mut macros = 0_usize;
    for (path, bytes) in &files {
        let inventory = retained_inventory(path, ManufacturingKindCandidate::Gerber, bytes);
        let parsed = parse_gerber_document(&inventory.inputs[0])
            .unwrap_or_else(|error| panic!("{path}: {error:?}"));
        parser_records += parsed.accounting.parser_results;
        parser_successes += parsed.accounting.parser_successes;
        parser_errors += parsed.accounting.parser_errors;
        resolved_routes += parsed.accounting.resolved_route_errors;
        warnings += parsed.normalization_warnings.len();
        features += parsed.review.features.len();
        macros += parsed.review.macros.len();
        let mut file_lines = 0_usize;
        let mut file_arcs = 0_usize;
        let mut file_regions = 0_usize;
        let mut file_flashes = 0_usize;
        for feature in &parsed.review.features {
            match feature.geometry {
                Geometry::Line(_) => file_lines += 1,
                Geometry::Arc(_) => file_arcs += 1,
                Geometry::Region(_) => file_regions += 1,
                Geometry::Flash(_) => file_flashes += 1,
                _ => {}
            }
        }
        lines += file_lines;
        arcs += file_arcs;
        regions += file_regions;
        flashes += file_flashes;
        parsed.review.validate().unwrap();
        println!(
            "GERBER FILE {}",
            serde_json::to_string(&serde_json::json!({
                "path": path,
                "digest": format!("{:x}", Sha256::digest(bytes)),
                "outcome": "accepted",
                "parserResults": parsed.accounting.parser_results,
                "parserSuccesses": parsed.accounting.parser_successes,
                "parserErrors": parsed.accounting.parser_errors,
                "resolvedRoutes": parsed.accounting.resolved_route_errors,
                "unaccountedErrors": parsed.accounting.unaccounted_errors,
                "normalizationWarnings": parsed.normalization_warnings.len(),
                "features": parsed.review.features.len(),
                "lines": file_lines,
                "arcs": file_arcs,
                "regions": file_regions,
                "flashes": file_flashes,
                "macros": parsed.review.macros.len(),
                "modelDigest": parsed.review.model_digest,
            }))
            .unwrap()
        );
    }
    assert_eq!(files.len(), 32);
    assert_eq!(parser_records, 102_909);
    assert_eq!(parser_successes, 102_908);
    assert_eq!(parser_errors, 1);
    assert_eq!(resolved_routes, 1);
    assert_eq!(warnings, 32);
    println!(
        "PRODUCTION OFFICIAL SUMMARY files={} parser_records={} parser_successes={} parser_errors={} resolved_routes={} unaccounted_errors=0 normalization_warnings={} features={} lines={} arcs={} regions={} flashes={} macros={} direct_verified_zip_buffers=true",
        files.len(),
        parser_records,
        parser_successes,
        parser_errors,
        resolved_routes,
        warnings,
        features,
        lines,
        arcs,
        regions,
        flashes,
        macros
    );
}

fn xnc_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/fabrication/xnc")
        .join(name)
}

fn job_fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/fabrication/job")
        .join(name)
}

fn inventory_from_files(
    files: Vec<(&str, ManufacturingKindCandidate, Vec<u8>)>,
) -> ManufacturingInventory {
    let mut inventory = ManufacturingInventory::default();
    for (path, kind, bytes) in files {
        let digest = format!("{:x}", Sha256::digest(&bytes));
        let size = bytes.len() as u64;
        inventory.inputs.push(ManufacturingInput {
            virtual_path: path.into(),
            artifact_digest: digest.clone(),
            kind_candidate: kind,
            size,
            original_bytes: bytes,
            file_started: None,
        });
        inventory.outcomes.push(ManufacturingInputOutcome {
            id: input_outcome_id(path, Some(&digest), kind),
            virtual_path: path.into(),
            artifact_digest: Some(digest),
            kind_candidate: kind,
            size,
            state: ManufacturingLoadState::Retained,
            reason: None,
        });
    }
    inventory
}

fn replace_inventory_path(
    inventory: &mut ManufacturingInventory,
    path: &str,
    bytes: Vec<u8>,
) -> ManufacturingInput {
    let input = inventory
        .inputs
        .iter_mut()
        .find(|input| input.virtual_path == path)
        .unwrap();
    input.original_bytes = bytes;
    input.size = input.original_bytes.len() as u64;
    input.artifact_digest = format!("{:x}", Sha256::digest(&input.original_bytes));
    let input = input.clone();
    let outcome = inventory
        .outcomes
        .iter_mut()
        .find(|outcome| outcome.virtual_path == path)
        .unwrap();
    outcome.size = input.size;
    outcome.artifact_digest = Some(input.artifact_digest.clone());
    outcome.id = input_outcome_id(
        &outcome.virtual_path,
        outcome.artifact_digest.as_deref(),
        outcome.kind_candidate,
    );
    input
}

fn replace_inventory_input(
    inventory: &mut ManufacturingInventory,
    kind: ManufacturingKindCandidate,
    bytes: Vec<u8>,
) -> ManufacturingInput {
    let path = inventory
        .inputs
        .iter()
        .find(|input| input.kind_candidate == kind)
        .unwrap()
        .virtual_path
        .clone();
    replace_inventory_path(inventory, &path, bytes)
}

fn x2_layer(function: &str, profile: bool, sparse: bool) -> Vec<u8> {
    let geometry = if profile {
        "G36*\nX000000Y000000D02*\nX10000000Y000000D01*\nX10000000Y10000000D01*\nX000000Y10000000D01*\nX000000Y000000D01*\nG37*\n"
    } else if sparse {
        "X1000000Y1000000D02*\nX2000000Y1000000D01*\n%TD*%\nX3000000Y1000000D01*\n"
    } else {
        "X1000000Y1000000D02*\nX2000000Y1000000D01*\n"
    };
    format!(
        "G04 RateMyPCB project-authored X2 package fixture*\n%FSLAX46Y46*%\n%MOMM*%\n%TF.FileFunction,{function}*%\n%TA.AperFunction,Conductor*%\n%ADD10C,0.200*%\nD10*\n%TO.N,GND*%\n%TO.C,U1*%\n%TO.P,U1,1*%\n{geometry}M02*\n"
    )
    .into_bytes()
}

fn complete_package(job_bytes: Vec<u8>, xnc_bytes: Vec<u8>) -> ManufacturingInventory {
    inventory_from_files(vec![
        (
            "fab/top.gbr",
            ManufacturingKindCandidate::Gerber,
            x2_layer("Copper,L1,Top", false, false),
        ),
        (
            "fab/bottom.gbr",
            ManufacturingKindCandidate::Gerber,
            x2_layer("Copper,L2,Bot", false, false),
        ),
        (
            "fab/profile.gbr",
            ManufacturingKindCandidate::Gerber,
            x2_layer("Profile,NP", true, false),
        ),
        (
            "fab/holes.xnc",
            ManufacturingKindCandidate::Excellon,
            xnc_bytes,
        ),
        (
            "fab/complete.gbrjob",
            ManufacturingKindCandidate::GerberJob,
            job_bytes,
        ),
    ])
}

fn write_complete_package_directory(root: &Path) {
    let fab = root.join("fab");
    fs::create_dir_all(&fab).unwrap();
    fs::write(fab.join("top.gbr"), x2_layer("Copper,L1,Top", false, false)).unwrap();
    fs::write(
        fab.join("bottom.gbr"),
        x2_layer("Copper,L2,Bot", false, false),
    )
    .unwrap();
    fs::write(fab.join("profile.gbr"), x2_layer("Profile,NP", true, false)).unwrap();
    fs::write(
        fab.join("holes.xnc"),
        fs::read(xnc_fixture("strict.xnc")).unwrap(),
    )
    .unwrap();
    fs::write(
        fab.join("complete.gbrjob"),
        fs::read(job_fixture("complete.gbrjob")).unwrap(),
    )
    .unwrap();
}

fn package_capability(review: &FabricationReview, id: CapabilityId) -> CapabilityState {
    review
        .capabilities
        .records
        .iter()
        .find(|record| record.id == id)
        .unwrap()
        .state
}

#[test]
fn x2_job_complete_only_attributes_and_exact_route_are_preserved() {
    let bytes = x2_layer("Copper,L1,Top", false, false);
    let inventory = retained_inventory("fab/top.gbr", ManufacturingKindCandidate::Gerber, &bytes);
    let mut parsed = parse_gerber_document(&inventory.inputs[0]).unwrap();
    apply_gerber_x2(&mut parsed).unwrap();
    assert_eq!(
        parsed.file_function.as_ref().unwrap().role,
        LayerRole::Copper
    );
    assert_eq!(parsed.review.layers[0].side, LayerSide::Top);
    assert_eq!(parsed.review.layers[0].order, Some(1));
    for id in [
        CapabilityId::X2FileAttributes,
        CapabilityId::X2ApertureAttributes,
        CapabilityId::X2ObjectAttributes,
        CapabilityId::Connectivity,
        CapabilityId::Components,
        CapabilityId::Pins,
    ] {
        assert_eq!(
            package_capability(&parsed.review, id),
            CapabilityState::Complete
        );
    }
    assert_eq!(
        parsed.review.connectivity.len(),
        parsed.review.features.len()
    );
    assert!(parsed.review.x2_attributes.iter().all(|attribute| {
        (!attribute.deletion && attribute.values.iter().all(|value| !value.is_empty()))
            || (attribute.deletion
                && attribute.values.is_empty()
                && attribute.target_ids.is_empty())
    }));
    for kind in [
        X2AttributeKind::FileFunction,
        X2AttributeKind::ApertureFunction,
        X2AttributeKind::Net,
        X2AttributeKind::Component,
        X2AttributeKind::Pin,
    ] {
        assert!(
            parsed
                .review
                .x2_attributes
                .iter()
                .any(|attribute| attribute.kind == kind && !attribute.target_ids.is_empty()),
            "{kind:?}"
        );
    }
    let locations = parsed
        .review
        .x2_attributes
        .iter()
        .filter(|attribute| attribute.scope == X2AttributeScope::Object)
        .map(|attribute| attribute.provenance.location.record)
        .collect::<BTreeSet<_>>();
    assert!(locations.len() >= 3);
    parsed.review.validate().unwrap();

    let sparse = x2_layer("Copper,L1,Top", false, true);
    let inventory = retained_inventory(
        "fab/sparse.gbr",
        ManufacturingKindCandidate::Gerber,
        &sparse,
    );
    let mut parsed = parse_gerber_document(&inventory.inputs[0]).unwrap();
    apply_gerber_x2(&mut parsed).unwrap();
    assert_eq!(
        package_capability(&parsed.review, CapabilityId::X2ObjectAttributes),
        CapabilityState::Partial
    );
    assert_eq!(
        package_capability(&parsed.review, CapabilityId::Connectivity),
        CapabilityState::Partial
    );
    assert!(parsed.review.x2_attributes.iter().any(|attribute| {
        attribute.kind == X2AttributeKind::Reset
            && attribute.deletion
            && attribute.provenance.location.record > 0
    }));

    let mut route = production_gerber("route-file-function.gbr");
    let original_accounting = route.accounting.clone();
    let original_route = route.route_file_functions.clone();
    apply_gerber_x2(&mut route).unwrap();
    assert_eq!(route.accounting, original_accounting);
    assert_eq!(route.route_file_functions, original_route);
    assert_eq!(route.file_function.as_ref().unwrap().role, LayerRole::Route);
    assert_eq!(
        route.file_function.as_ref().unwrap().operation.as_deref(),
        Some("NPTH,Route")
    );
}

#[test]
fn x2_scoped_attributes_reject_reset_empty_conflict_and_trailing_unknown() {
    let aperture_reset = b"G04 scoped aperture reset*\n%FSLAX46Y46*%\n%MOMM*%\n%TF.FileFunction,Copper,L1,Top*%\n%TA.AperFunction,Conductor*%\n%ADD10C,0.200*%\n%TD.AperFunction*%\n%ADD11C,0.300*%\nD10*\nX1000000Y1000000D03*\nD11*\nX2000000Y1000000D03*\nM02*\n";
    let mut cases = vec![(aperture_reset.to_vec(), CapabilityId::X2ApertureAttributes)];
    for source in [
        String::from_utf8(x2_layer("Copper,L1,Top", false, false))
            .unwrap()
            .replace("%TO.N,GND*%", "%TO.N,*%"),
        String::from_utf8(x2_layer("Copper,L1,Top", false, false))
            .unwrap()
            .replace("%TO.P,U1,1*%", "%TO.P,U2,1*%"),
        String::from_utf8(x2_layer("Copper,L1,Top", false, false))
            .unwrap()
            .replace("M02*", "%TO.Unknown,trailing*%\nM02*"),
    ] {
        cases.push((source.into_bytes(), CapabilityId::X2ObjectAttributes));
    }
    for (index, (bytes, capability)) in cases.into_iter().enumerate() {
        let inventory = retained_inventory(
            &format!("fab/scoped-{index}.gbr"),
            ManufacturingKindCandidate::Gerber,
            &bytes,
        );
        let mut parsed = parse_gerber_document(&inventory.inputs[0]).unwrap();
        apply_gerber_x2(&mut parsed).unwrap();
        assert_eq!(
            package_capability(&parsed.review, capability),
            CapabilityState::Partial,
            "case {index}"
        );
    }
}

#[test]
fn x2_component_and_file_function_conflicts_are_order_independent_and_job_cannot_mask_them() {
    let base = String::from_utf8(x2_layer("Copper,L1,Top", false, false)).unwrap();
    for (index, attributes) in ["%TO.P,U1,1*%\n%TO.C,U2*%", "%TO.C,U2*%\n%TO.P,U1,1*%"]
        .into_iter()
        .enumerate()
    {
        let bytes = base
            .replace("%TO.C,U1*%\n%TO.P,U1,1*%", attributes)
            .into_bytes();
        let inventory = retained_inventory(
            &format!("fab/component-order-{index}.gbr"),
            ManufacturingKindCandidate::Gerber,
            &bytes,
        );
        let mut parsed = parse_gerber_document(&inventory.inputs[0]).unwrap();
        apply_gerber_x2(&mut parsed).unwrap();
        assert_eq!(
            package_capability(&parsed.review, CapabilityId::X2ObjectAttributes),
            CapabilityState::Partial,
            "order {index}"
        );
        assert!(parsed.review.conflicts.iter().any(|conflict| {
            conflict.kind == ConflictKind::Connectivity
                && conflict
                    .affected_capabilities
                    .contains(&CapabilityId::Components)
        }));
    }

    for (index, extra) in [
        "%TF.FileFunction,Copper,L1,Top*%\n",
        "%TF.FileFunction,Copper,L2,Bot*%\n",
    ]
    .into_iter()
    .enumerate()
    {
        let bytes = base
            .replacen(
                "%TF.FileFunction,Copper,L1,Top*%\n",
                &format!("%TF.FileFunction,Copper,L1,Top*%\n{extra}"),
                1,
            )
            .into_bytes();
        let inventory = retained_inventory(
            &format!("fab/file-function-{index}.gbr"),
            ManufacturingKindCandidate::Gerber,
            &bytes,
        );
        let mut parsed = parse_gerber_document(&inventory.inputs[0]).unwrap();
        apply_gerber_x2(&mut parsed).unwrap();
        assert_eq!(
            package_capability(&parsed.review, CapabilityId::X2FileAttributes),
            CapabilityState::Partial
        );
        assert!(
            parsed.review.omissions.iter().any(|omission| {
                omission
                    .affected_capabilities
                    .contains(&CapabilityId::LayerRoles)
            }) || parsed.review.conflicts.iter().any(|conflict| {
                conflict
                    .affected_capabilities
                    .contains(&CapabilityId::LayerRoles)
            })
        );

        let mut package = complete_package(
            fs::read(job_fixture("complete.gbrjob")).unwrap(),
            fs::read(xnc_fixture("strict.xnc")).unwrap(),
        );
        replace_inventory_path(&mut package, "fab/top.gbr", bytes);
        let review = analyze_manufacturing_inventory(&package).unwrap();
        assert!(
            review.omissions.iter().any(|omission| {
                omission
                    .affected_capabilities
                    .contains(&CapabilityId::LayerRoles)
            }) || review.conflicts.iter().any(|conflict| {
                conflict
                    .affected_capabilities
                    .contains(&CapabilityId::LayerRoles)
            })
        );
        assert_ne!(
            package_capability(&review, CapabilityId::LayerRoles),
            CapabilityState::Complete,
            "duplicate/conflicting FileFunction case {index}"
        );
        assert_ne!(
            package_capability(&review, CapabilityId::PackageCompleteness),
            CapabilityState::Complete,
            "matching Job masked case {index}"
        );
    }

    let conflict_bytes = base
        .replace("%TO.C,U1*%\n%TO.P,U1,1*%", "%TO.P,U1,1*%\n%TO.C,U2*%")
        .into_bytes();
    let mut package = complete_package(
        fs::read(job_fixture("complete.gbrjob")).unwrap(),
        fs::read(xnc_fixture("strict.xnc")).unwrap(),
    );
    replace_inventory_path(&mut package, "fab/top.gbr", conflict_bytes);
    let review = analyze_manufacturing_inventory(&package).unwrap();
    assert!(review.conflicts.iter().any(|conflict| {
        conflict.kind == ConflictKind::Connectivity
            && conflict
                .affected_capabilities
                .contains(&CapabilityId::Components)
    }));
}

#[test]
fn xnc_strict_preserves_tools_plating_spans_drills_slots_and_routes() {
    let bytes = fs::read(xnc_fixture("strict.xnc")).unwrap();
    let inventory = retained_inventory(
        "fab/holes.xnc",
        ManufacturingKindCandidate::Excellon,
        &bytes,
    );
    let production = parse_xnc_document(&inventory.inputs[0]).unwrap();
    assert_eq!(production.dialect, XncDialect::Strict);
    assert_eq!(production.review.tools.len(), 1);
    assert_eq!(
        production.review.tools[0].diameter,
        Some(Picometres(600_000_000))
    );
    assert_eq!(production.review.tools[0].plating, Plating::Plated);
    assert!(production.review.tools[0].span.is_some());
    assert_eq!(
        production
            .review
            .features
            .iter()
            .filter(|feature| matches!(feature.geometry, Geometry::Drill(_)))
            .count(),
        1
    );
    assert!(production.review.features.iter().any(|feature| {
        matches!(&feature.geometry, Geometry::Route(route) if matches!(route.segments.as_slice(), [ContourSegment::Line(_), ContourSegment::Arc(_)]))
    }));
    assert!(
        production
            .review
            .features
            .iter()
            .any(|feature| matches!(feature.geometry, Geometry::Slot(_)))
    );
    for id in [
        CapabilityId::DocumentSyntax,
        CapabilityId::UnitsAndFormat,
        CapabilityId::Tools,
        CapabilityId::Drills,
        CapabilityId::Routes,
        CapabilityId::Slots,
        CapabilityId::Plating,
        CapabilityId::LayerSpans,
    ] {
        assert_eq!(
            package_capability(&production.review, id),
            CapabilityState::Complete
        );
    }
    production.review.validate().unwrap();
}

#[test]
fn xnc_named_legacy_profiles_are_exact_and_fail_closed() {
    for (fixture, expected) in [
        ("kicad-legacy.drl", XncDialect::KicadLegacy),
        ("librepcb-legacy.drl", XncDialect::LibrePcbLegacy),
    ] {
        let bytes = fs::read(xnc_fixture(fixture)).unwrap();
        let inventory = retained_inventory(
            &format!("fab/{fixture}"),
            ManufacturingKindCandidate::Excellon,
            &bytes,
        );
        let production = parse_xnc_document(&inventory.inputs[0]).unwrap();
        assert_eq!(production.dialect, expected);
        production.review.validate().unwrap();
    }

    let bytes = fs::read(xnc_fixture("unsupported-legacy.drl")).unwrap();
    let inventory = retained_inventory(
        "fab/unsupported.drl",
        ManufacturingKindCandidate::Excellon,
        &bytes,
    );
    assert!(matches!(
        parse_xnc_document(&inventory.inputs[0]),
        Err(XncParseError::Unsupported { .. })
    ));

    let base = fs::read_to_string(xnc_fixture("kicad-legacy.drl")).unwrap();
    for (index, hostile) in [
        base.replace("Kicad,Pcbnew,9.0", "Unknown,Exporter,1"),
        base.replace("Kicad,Pcbnew,9.0", "xxxx,yyyy,zzzz"),
        base.replace("Kicad,Pcbnew,9.0", "Kicad,OtherApplication,9.0"),
        base.replace("Kicad,Pcbnew,9.0", "Kicad,Pcbnew,8.99"),
        base.replace(
            "; #@! TF.GenerationSoftware,Kicad,Pcbnew,9.0",
            "; #@! TF.GenerationSoftware,Kicad,Pcbnew,9.0\n; #@! TF.GenerationSoftware,Kicad,Pcbnew,9.0",
        ),
        base.replace(
            "; #@! TF.GenerationSoftware,Kicad,Pcbnew,9.0",
            "; #@! TF.GenerationSoftware,Kicad,Pcbnew,9.0\n; #@! TF.GenerationSoftware,LibrePCB,LibrePCB,1.0",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let inventory = retained_inventory(
            &format!("fab/hostile-profile-{index}.drl"),
            ManufacturingKindCandidate::Excellon,
            hostile.as_bytes(),
        );
        assert!(parse_xnc_document(&inventory.inputs[0]).is_err(), "case {index}");
    }
}

#[test]
fn xnc_hostile_truncation_numeric_resource_and_deadline_cases_fail_closed() {
    let valid = fs::read_to_string(xnc_fixture("strict.xnc")).unwrap();
    for mutated in [
        valid.replace("METRIC", "METRIC,LZ"),
        valid.replace("T01C0.600", "T01C0.600\nT01C0.700"),
        valid.replace("X1.000Y1.000", "X1000Y1.000"),
        valid.replace("G03X4.000Y3.000A1.000", "G03X4.000Y3.000I0.000J1.000"),
        valid.replace("G03X4.000Y3.000A1.000", "G03X4.000Y2.000I0.000J1.000"),
        valid.replace("G03X4.000Y3.000A1.000", "G03X4.000Y3.000A-1.000"),
        valid.replace("G03X4.000Y3.000A1.000", "G02X4.000Y3.000I-1.000J0.500"),
        valid.replace("G85X6.000Y5.000", "G85X5.000Y5.000"),
        valid.replace("M30", ""),
        valid.replace("M30", "M30\nX2.000Y2.000"),
        valid.replace("M30", "G91\nM30"),
    ] {
        let inventory = retained_inventory(
            "fab/hostile.xnc",
            ManufacturingKindCandidate::Excellon,
            mutated.as_bytes(),
        );
        assert!(parse_xnc_document(&inventory.inputs[0]).is_err());
    }
    let inventory = retained_inventory(
        "fab/deadline.xnc",
        ManufacturingKindCandidate::Excellon,
        valid.as_bytes(),
    );
    assert!(matches!(
        parse_xnc_document_with_timeout(&inventory.inputs[0], Duration::ZERO),
        Err(XncParseError::Deadline { .. })
    ));
    let overlong = format!(
        "; {}\nM48\nMETRIC\nT01C0.1\n%\nM30\n",
        "x".repeat(MANUFACTURING_LIMITS.max_line_bytes)
    );
    let inventory = retained_inventory(
        "fab/overlong.xnc",
        ManufacturingKindCandidate::Excellon,
        overlong.as_bytes(),
    );
    assert!(matches!(
        parse_xnc_document(&inventory.inputs[0]),
        Err(XncParseError::Resource { .. })
    ));
}

#[test]
fn xnc_same_ray_ij_arc_requires_nonzero_directed_sweep() {
    let document = |arc: &str| {
        format!(
            "M48\n; #@! TF.FileFunction,NonPlated,1,2,NPTH,Route\nMETRIC\nT01C0.600000\n%\nT01\nG00X1.000000Y0.000000\nM15\n{arc}\nM16\nM30\n"
        )
    };
    let same_ray = document("G03X1.000001Y0.000000I-1.000000J0.000000");
    let inventory = retained_inventory(
        "fab/zero-sweep.xnc",
        ManufacturingKindCandidate::Excellon,
        same_ray.as_bytes(),
    );
    assert!(matches!(
        parse_xnc_document(&inventory.inputs[0]),
        Err(XncParseError::Invalid {
            reason: "invalid-arc-geometry",
            ..
        })
    ));

    let quarter = document("G03X0.000000Y1.000000I-1.000000J0.000000");
    let inventory = retained_inventory(
        "fab/quarter-sweep.xnc",
        ManufacturingKindCandidate::Excellon,
        quarter.as_bytes(),
    );
    assert!(parse_xnc_document(&inventory.inputs[0]).is_ok());

    let radial_mismatch = document("G03X1.000002Y0.000000I-1.000000J0.000000");
    let inventory = retained_inventory(
        "fab/radial-mismatch.xnc",
        ManufacturingKindCandidate::Excellon,
        radial_mismatch.as_bytes(),
    );
    assert!(parse_xnc_document(&inventory.inputs[0]).is_err());
}

#[test]
fn xnc_comments_and_route_segments_use_exact_shared_limits() {
    let comment = format!(";{}\n", "x".repeat(MANUFACTURING_LIMITS.max_text_bytes - 1));
    let half = MANUFACTURING_LIMITS.metadata_bytes_per_file as usize
        / MANUFACTURING_LIMITS.max_text_bytes
        / 2;
    let exact_comments = comment.repeat(half);
    let exact = format!(
        "{exact_comments}M48\nMETRIC\nT01C0.600\n%\nT01\n{exact_comments}X1.000Y1.000\nM30\n"
    );
    let inventory = retained_inventory(
        "fab/comment-exact.xnc",
        ManufacturingKindCandidate::Excellon,
        exact.as_bytes(),
    );
    let parsed = parse_xnc_document(&inventory.inputs[0]).unwrap();
    assert_eq!(
        parsed.review.documents[0].metrics.metadata_bytes,
        MANUFACTURING_LIMITS.metadata_bytes_per_file
    );
    let over = format!(";x\n{exact}");
    let inventory = retained_inventory(
        "fab/comment-over.xnc",
        ManufacturingKindCandidate::Excellon,
        over.as_bytes(),
    );
    assert!(matches!(
        parse_xnc_document(&inventory.inputs[0]),
        Err(XncParseError::Resource {
            resource: "metadata-bytes",
            ..
        })
    ));

    let route = |segments: usize| {
        let mut source = String::from(
            "M48\n; #@! TF.FileFunction,NonPlated,1,2,NPTH,Route\nMETRIC\nT01C0.600\n%\nT01\nG00X0.000Y0.000\nM15\n",
        );
        for index in 0..segments {
            source.push_str(if index % 2 == 0 {
                "G01X1.000Y0.000\n"
            } else {
                "G01X0.000Y0.000\n"
            });
        }
        source.push_str("M16\nM30\n");
        source
    };
    let exact = route(MANUFACTURING_LIMITS.drill_route_features);
    let inventory = retained_inventory(
        "fab/route-exact.xnc",
        ManufacturingKindCandidate::Excellon,
        exact.as_bytes(),
    );
    let parsed = parse_xnc_document(&inventory.inputs[0]).unwrap();
    assert!(matches!(
        &parsed.review.features[0].geometry,
        Geometry::Route(route)
            if route.segments.len() == MANUFACTURING_LIMITS.drill_route_features
    ));
    let over = route(MANUFACTURING_LIMITS.drill_route_features + 1);
    let inventory = retained_inventory(
        "fab/route-over.xnc",
        ManufacturingKindCandidate::Excellon,
        over.as_bytes(),
    );
    assert!(matches!(
        parse_xnc_document(&inventory.inputs[0]),
        Err(XncParseError::Resource {
            resource: "drill-route-features",
            ..
        })
    ));
}

#[test]
fn xnc_physical_extents_include_finished_tools_and_full_arc_sweep() {
    let parse = |name: &str, function: &str, diameter: &str, body: &str| {
        let source = format!(
            "M48\n; #@! TF.FileFunction,{function}\nMETRIC\nT01C{diameter}\n%\nT01\n{body}M30\n"
        );
        let inventory = retained_inventory(
            &format!("fab/{name}.xnc"),
            ManufacturingKindCandidate::Excellon,
            source.as_bytes(),
        );
        parse_xnc_document(&inventory.inputs[0]).unwrap()
    };

    let drill = parse(
        "drill-boundary",
        "Plated,1,2,PTH",
        "0.600000",
        "X0.300000Y5.000000\n",
    );
    assert_eq!(
        drill.extents,
        Some(Extent {
            min: CanonicalPoint::new(0, 4_700_000_000),
            max: CanonicalPoint::new(600_000_000, 5_300_000_000),
        })
    );

    let slot = parse(
        "slot-boundary",
        "Plated,1,2,PTH",
        "0.600000",
        "G00X0.300000Y1.000000\nG85X9.700000Y1.000000\n",
    );
    assert_eq!(
        slot.extents,
        Some(Extent {
            min: CanonicalPoint::new(0, 700_000_000),
            max: CanonicalPoint::new(10_000_000_000, 1_300_000_000),
        })
    );

    let route = parse(
        "route-boundary",
        "NonPlated,1,2,NPTH,Route",
        "0.600000",
        "G00X0.300000Y2.000000\nM15\nG01X9.700000Y2.000000\nM16\n",
    );
    assert_eq!(
        route.extents,
        Some(Extent {
            min: CanonicalPoint::new(0, 1_700_000_000),
            max: CanonicalPoint::new(10_000_000_000, 2_300_000_000),
        })
    );

    let arc = parse(
        "arc-sweep",
        "Plated,1,2,PTH,Route",
        "0.200000",
        "X5.000000Y5.000000\nG00X1.464466Y8.535534\nM15\nG03X1.464466Y1.464466I3.535534J-3.535534\nM16\n",
    );
    assert!(
        arc.extents.as_ref().unwrap().min.x.0 < 0,
        "arc extents: {:?}",
        arc.extents
    );

    for center in ["0.299999", "0.299999999"] {
        let decimals = center.split_once('.').unwrap().1.len();
        let diameter = format!("0.{:0<width$}", "6", width = decimals);
        let y = format!("5.{}", "0".repeat(decimals));
        let source = format!(
            "M48\n; #@! TF.FileFunction,Plated,1,2,PTH\nMETRIC\nT01C{diameter}\n%\nT01\nX{center}Y{y}\nM30\n"
        );
        let review = analyze_manufacturing_inventory(&complete_package(
            fs::read(job_fixture("complete.gbrjob")).unwrap(),
            source.into_bytes(),
        ))
        .unwrap();
        assert_ne!(
            package_capability(&review, CapabilityId::Extents),
            CapabilityState::Complete,
            "physical drill outside by one source unit at {decimals} decimals"
        );
        assert_ne!(
            package_capability(&review, CapabilityId::PackageCompleteness),
            CapabilityState::Complete
        );
    }
}

#[test]
fn manufacturing_live_path_uses_one_nonrestartable_absolute_deadline() {
    let fabrication = include_str!("../src/fabrication.rs");
    let native = include_str!("../src/fabrication/native.rs");
    let core = include_str!("../src/lib.rs");
    assert!(fabrication.contains("pub(crate) struct ManufacturingDeadline"));
    assert!(fabrication.contains("check(\"package-profile-topology\")"));
    assert!(native.contains("deadline: ManufacturingDeadline"));
    assert!(!native.contains(
        "validate_reconciliation_derivation_with_deadline(\n        review,\n        Instant::now(),"
    ));
    assert!(core.contains("validate_with_deadline(manufacturing_deadline)"));
}

#[test]
fn manufacturing_deadlines_are_carried_across_load_parse_and_aggregate_stages() {
    let expired_file = Instant::now()
        .checked_sub(Duration::from_millis(MANUFACTURING_LIMITS.file_timeout_ms))
        .unwrap();
    let mut xnc = retained_inventory(
        "fab/expired.xnc",
        ManufacturingKindCandidate::Excellon,
        &fs::read(xnc_fixture("strict.xnc")).unwrap(),
    );
    xnc.inputs[0].file_started = Some(expired_file);
    assert!(matches!(
        parse_xnc_document(&xnc.inputs[0]),
        Err(XncParseError::Deadline { .. })
    ));

    let mut gerber = retained_inventory(
        "fab/expired.gbr",
        ManufacturingKindCandidate::Gerber,
        &x2_layer("Copper,L1,Top", false, false),
    );
    gerber.inputs[0].file_started = Some(expired_file);
    assert!(matches!(
        parse_gerber_document(&gerber.inputs[0]),
        Err(GerberParseError::Deadline { .. })
    ));

    let mut package = complete_package(
        fs::read(job_fixture("complete.gbrjob")).unwrap(),
        fs::read(xnc_fixture("strict.xnc")).unwrap(),
    );
    package.aggregate_started = Some(
        Instant::now()
            .checked_sub(Duration::from_millis(
                MANUFACTURING_LIMITS.aggregate_timeout_ms,
            ))
            .unwrap(),
    );
    assert!(matches!(
        analyze_manufacturing_inventory(&package),
        Err(PackageParseError::Deadline)
    ));
}

#[test]
fn job_virtual_inventory_paths_duplicates_and_resources_fail_closed() {
    let inventory = complete_package(
        fs::read(job_fixture("complete.gbrjob")).unwrap(),
        fs::read(xnc_fixture("strict.xnc")).unwrap(),
    );
    let job_input = inventory
        .inputs
        .iter()
        .find(|input| input.kind_candidate == ManufacturingKindCandidate::GerberJob)
        .unwrap();
    let job = parse_gerber_job_document(job_input, &inventory).unwrap();
    assert_eq!(job.references.len(), 4);
    assert_eq!(
        job.product.as_ref().unwrap().name.as_deref(),
        Some("phase5-board")
    );
    assert!(job.unsupported_fields.is_empty());

    let dangling = inventory_from_files(vec![(
        "fab/dangling.gbrjob",
        ManufacturingKindCandidate::GerberJob,
        fs::read(job_fixture("dangling.gbrjob")).unwrap(),
    )]);
    assert!(parse_gerber_job_document(&dangling.inputs[0], &dangling).is_err());

    let base = fs::read_to_string(job_fixture("complete.gbrjob")).unwrap();
    for mutated in [
        base.replace("top.gbr", "../top.gbr"),
        base.replace("\"Header\":", "\"Header\": {}, \"Header\":"),
        base.replace("top.gbr", "complete.gbrjob"),
        format!(
            "{{\"{}\":0}}",
            "nested".repeat(MANUFACTURING_LIMITS.max_text_bytes)
        ),
    ] {
        let mut mutated_inventory = inventory.clone();
        let input = mutated_inventory
            .inputs
            .iter_mut()
            .find(|input| input.kind_candidate == ManufacturingKindCandidate::GerberJob)
            .unwrap();
        input.original_bytes = mutated.into_bytes();
        input.size = input.original_bytes.len() as u64;
        input.artifact_digest = format!("{:x}", Sha256::digest(&input.original_bytes));
        let input = input.clone();
        let outcome = mutated_inventory
            .outcomes
            .iter_mut()
            .find(|outcome| outcome.kind_candidate == ManufacturingKindCandidate::GerberJob)
            .unwrap();
        outcome.size = input.size;
        outcome.artifact_digest = Some(input.artifact_digest.clone());
        outcome.id = input_outcome_id(
            &outcome.virtual_path,
            outcome.artifact_digest.as_deref(),
            outcome.kind_candidate,
        );
        assert!(parse_gerber_job_document(&input, &mutated_inventory).is_err());
    }

    for (needle, replacement, expected_path) in [
        (
            "\"GenerationSoftware\": {",
            "\"UnknownHeader\": true, \"GenerationSoftware\": {",
            "Header.UnknownHeader",
        ),
        (
            "\"ProjectId\": {",
            "\"UnknownGeneral\": true, \"ProjectId\": {",
            "GeneralSpecs.UnknownGeneral",
        ),
        (
            "\"PartNumber\": \"P5-001\"",
            "\"PartNumber\": \"P5-001\", \"UnknownProject\": true",
            "GeneralSpecs.ProjectId.UnknownProject",
        ),
        (
            "\"Path\": \"top.gbr\",",
            "\"UnknownFile\": true, \"Path\": \"top.gbr\",",
            "FilesAttributes[0].UnknownFile",
        ),
    ] {
        let mut mutated_inventory = inventory.clone();
        let input = replace_inventory_input(
            &mut mutated_inventory,
            ManufacturingKindCandidate::GerberJob,
            base.replace(needle, replacement).into_bytes(),
        );
        let parsed = parse_gerber_job_document(&input, &mutated_inventory).unwrap();
        assert_eq!(parsed.unsupported_fields, [expected_path]);
        let review = analyze_manufacturing_inventory(&mutated_inventory).unwrap();
        assert!(review.omissions.iter().any(|omission| {
            omission.kind == OmissionKind::UnsupportedRecord
                && omission.detail.contains(expected_path)
        }));
    }

    for mutated in [
        base.replace("\"Vendor\": \"RateMyPCB\"", "\"Vendor\": \"\""),
        base.replace("\"Version\": \"1\"", "\"Version\": 1"),
        base.replace("\"Name\": \"phase5-board\"", "\"Name\": \"\""),
        base.replace("\"Header\": {", "\"Header\": ["),
        base.replace("Copper,L1,Top", "Plated,1,2,PTH"),
        base.replace("Plated,1,2,PTH", "Copper,L1,Top"),
    ] {
        let mut mutated_inventory = inventory.clone();
        let input = replace_inventory_input(
            &mut mutated_inventory,
            ManufacturingKindCandidate::GerberJob,
            mutated.into_bytes(),
        );
        assert!(parse_gerber_job_document(&input, &mutated_inventory).is_err());
    }

    let mut unicode_inventory = inventory.clone();
    let gerber = unicode_inventory
        .inputs
        .iter_mut()
        .find(|input| input.virtual_path == "fab/top.gbr")
        .unwrap();
    gerber.virtual_path = "fab/töp.gbr".into();
    let outcome = unicode_inventory
        .outcomes
        .iter_mut()
        .find(|outcome| outcome.virtual_path == "fab/top.gbr")
        .unwrap();
    outcome.virtual_path = "fab/töp.gbr".into();
    outcome.id = input_outcome_id(
        &outcome.virtual_path,
        outcome.artifact_digest.as_deref(),
        outcome.kind_candidate,
    );
    let input = replace_inventory_input(
        &mut unicode_inventory,
        ManufacturingKindCandidate::GerberJob,
        base.replace("top.gbr", "töp.gbr").into_bytes(),
    );
    assert!(parse_gerber_job_document(&input, &unicode_inventory).is_err());
}

#[test]
fn file_function_domain_is_exact_and_qualifiers_remain_authoritative() {
    for (index, function) in [
        "Copper,L1",
        "Copper,L1,Top,Extra",
        "Profile",
        "Profile,",
        "Profile,GARBAGE",
        "Plated,1,2,GARBAGE",
        "NonPlated,1,2,NPTH,Unknown",
    ]
    .into_iter()
    .enumerate()
    {
        let bytes = x2_layer(function, function.starts_with("Profile"), false);
        let inventory = retained_inventory(
            &format!("fab/malformed-file-function-{index}.gbr"),
            ManufacturingKindCandidate::Gerber,
            &bytes,
        );
        if let Ok(mut parsed) = parse_gerber_document(&inventory.inputs[0]) {
            assert!(apply_gerber_x2(&mut parsed).is_err(), "{function}");
        }
    }

    let buried = "Plated,2,9,Buried";
    let inventory = retained_inventory(
        "fab/buried.gbr",
        ManufacturingKindCandidate::Gerber,
        &x2_layer(buried, false, false),
    );
    let mut gerber = parse_gerber_document(&inventory.inputs[0]).unwrap();
    apply_gerber_x2(&mut gerber).unwrap();
    let function = gerber.file_function.as_ref().unwrap();
    assert_eq!(function.qualifier.as_deref(), Some("Buried"));
    assert_eq!((function.from_layer, function.to_layer), (Some(2), Some(9)));

    let strict_xnc = fs::read_to_string(xnc_fixture("strict.xnc")).unwrap();
    let buried_xnc = strict_xnc.replace("Plated,1,2,PTH", buried);
    let inventory = retained_inventory(
        "fab/buried.xnc",
        ManufacturingKindCandidate::Excellon,
        buried_xnc.as_bytes(),
    );
    let xnc = parse_xnc_document(&inventory.inputs[0]).unwrap();
    let function = xnc.file_function.as_ref().unwrap();
    assert_eq!(function.qualifier.as_deref(), Some("Buried"));
    assert_eq!((function.from_layer, function.to_layer), (Some(2), Some(9)));
    let tool_span = xnc.review.tools[0].span.as_ref().unwrap();
    let order = |id: &Option<String>| {
        xnc.review
            .layers
            .iter()
            .find(|layer| Some(&layer.id) == id.as_ref())
            .and_then(|layer| layer.order)
    };
    assert_eq!(
        (
            order(&tool_span.from_layer_id),
            order(&tool_span.to_layer_id)
        ),
        (Some(2), Some(9))
    );

    let buried_job = fs::read_to_string(job_fixture("complete.gbrjob"))
        .unwrap()
        .replace("Plated,1,2,PTH", buried)
        .into_bytes();
    let buried_package =
        analyze_manufacturing_inventory(&complete_package(buried_job, buried_xnc.into_bytes()))
            .unwrap();
    let buried_tools = buried_package
        .tools
        .iter()
        .filter(|tool| tool.kind != ToolKind::Aperture)
        .collect::<Vec<_>>();
    assert!(!buried_tools.is_empty());
    assert!(buried_tools.iter().all(|tool| {
        tool.plating == Plating::Plated
            && tool.span.as_ref().is_some_and(|span| {
                let order = |id: &Option<String>| {
                    buried_package
                        .layers
                        .iter()
                        .find(|layer| Some(&layer.id) == id.as_ref())
                        .and_then(|layer| layer.order)
                };
                (order(&span.from_layer_id), order(&span.to_layer_id)) == (Some(2), Some(9))
            })
    }));

    let prefixed_job = fs::read_to_string(job_fixture("complete.gbrjob"))
        .unwrap()
        .replacen("Copper,L1,Top", "TF.FileFunction,Copper,L1,Top", 1)
        .into_bytes();
    assert!(
        analyze_manufacturing_inventory(&complete_package(
            prefixed_job,
            fs::read(xnc_fixture("strict.xnc")).unwrap(),
        ))
        .is_err()
    );

    let malformed_xnc = strict_xnc.replace("Plated,1,2,PTH", "Plated,1,2,GARBAGE");
    let inventory = retained_inventory(
        "fab/malformed-file-function.xnc",
        ManufacturingKindCandidate::Excellon,
        malformed_xnc.as_bytes(),
    );
    assert!(parse_xnc_document(&inventory.inputs[0]).is_err());

    let malformed_job = fs::read_to_string(job_fixture("complete.gbrjob"))
        .unwrap()
        .replace("Profile,NP", "Profile,GARBAGE")
        .into_bytes();
    let mut matching_malformed =
        complete_package(malformed_job, fs::read(xnc_fixture("strict.xnc")).unwrap());
    replace_inventory_path(
        &mut matching_malformed,
        "fab/profile.gbr",
        x2_layer("Profile,GARBAGE", true, false),
    );
    assert!(
        analyze_manufacturing_inventory(&matching_malformed).is_err(),
        "matching malformed X2 and Job FileFunction must not establish authority"
    );

    let disagreeing_job = fs::read_to_string(job_fixture("complete.gbrjob"))
        .unwrap()
        .replace("Profile,NP", "Profile,P")
        .into_bytes();
    let disagreement = analyze_manufacturing_inventory(&complete_package(
        disagreeing_job,
        fs::read(xnc_fixture("strict.xnc")).unwrap(),
    ))
    .unwrap();
    assert!(disagreement.conflicts.iter().any(|conflict| {
        conflict.kind == ConflictKind::LayerRole
            && conflict.left.canonical_value.contains("Profile,NP")
            && conflict.right.canonical_value.contains("Profile,P")
    }));
    assert_ne!(
        package_capability(&disagreement, CapabilityId::LayerRoles),
        CapabilityState::Complete
    );
}

#[test]
fn fabrication_official_phase5_xnc_regression() {
    let Ok(root) = std::env::var("RATEMYPCB_UCAMCO_CORPUS") else {
        eprintln!("official XNC corpus not run: RATEMYPCB_UCAMCO_CORPUS is unset");
        return;
    };
    let archive_bytes = fs::read(Path::new(&root).join("xnc-test-files.zip")).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&archive_bytes)),
        "9ad73e43cec479235ace152d8885ac8fbded4dc6c376e9afeb1b734b25b04e84"
    );
    let mut archive = zip::ZipArchive::new(Cursor::new(&archive_bytes)).unwrap();
    assert!(archive.len() <= MANUFACTURING_LIMITS.archive_entries);
    let mut files = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        if entry.is_dir() {
            continue;
        }
        assert!(entry.enclosed_name().is_some());
        assert!(entry.size() <= MANUFACTURING_LIMITS.raw_bytes_per_file);
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes.len() as u64, entry.size());
        files.push((entry.name().to_owned(), bytes));
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    assert_eq!(files.len(), 9);
    let mut strict = 0_usize;
    let mut kicad = 0_usize;
    let mut librepcb = 0_usize;
    let mut features = 0_usize;
    let mut unsupported = 0_usize;
    let mut records = Vec::new();
    for (path, bytes) in &files {
        let digest = format!("{:x}", Sha256::digest(bytes));
        let inventory = retained_inventory(path, ManufacturingKindCandidate::Excellon, bytes);
        match parse_xnc_document(&inventory.inputs[0]) {
            Ok(parsed) => {
                match parsed.dialect {
                    XncDialect::Strict => strict += 1,
                    XncDialect::KicadLegacy => kicad += 1,
                    XncDialect::LibrePcbLegacy => librepcb += 1,
                }
                features += parsed.review.features.len();
                parsed.review.validate().unwrap();
                let capabilities = parsed
                    .review
                    .capabilities
                    .records
                    .iter()
                    .map(|record| format!("{:?}={:?}", record.id, record.state))
                    .collect::<Vec<_>>()
                    .join("|");
                let record = serde_json::json!({
                    "path": path,
                    "digest": digest,
                    "outcome": "accepted",
                    "dialect": format!("{:?}", parsed.dialect),
                    "features": parsed.review.features.len(),
                    "tools": parsed.review.tools.len(),
                    "omissions": parsed.review.omissions.len(),
                    "extents": parsed.extents,
                    "capabilities": capabilities,
                });
                println!("XNC FILE {}", serde_json::to_string(&record).unwrap());
                records.push(record);
            }
            Err(XncParseError::Unsupported { record, command })
                if command.contains("TF.GenerationSoftware,xxxx,yyyy,zzzz") =>
            {
                unsupported += 1;
                let outcome = serde_json::json!({
                    "path": path,
                    "digest": digest,
                    "outcome": "unsupported",
                    "record": record,
                    "command": command,
                });
                println!("XNC FILE {}", serde_json::to_string(&outcome).unwrap());
                records.push(outcome);
            }
            Err(error) => panic!("{path}: {error:?}"),
        }
    }
    assert_eq!((strict, kicad, librepcb, unsupported), (4, 1, 2, 2));
    let expected: serde_json::Value = serde_json::from_str(r#"[
      {"path":"XNC format-test-files_en/2018 11 18 XNC - Ucamco samples/NonPlated.xnc","digest":"7daa8b46c79c676ab1ae37eca699c35ab3c5708489dc1aee50a206672ed1eafc","outcome":"accepted","dialect":"Strict","features":3,"tools":1,"omissions":0,"extents":{"min":{"x":2750000000,"y":2750000000},"max":{"x":117250000000,"y":117250000000}},"capabilities":"DocumentSyntax=Complete|UnitsAndFormat=Complete|Extents=Complete|Drills=Complete|Routes=NotProvided|Slots=NotProvided|Tools=Complete|Plating=Complete|LayerSpans=Complete"},
      {"path":"XNC format-test-files_en/2018 11 18 XNC - Ucamco samples/Plated.xnc","digest":"8f0f61ea4ae62f6efd29c818574f63e051dedbc39486b88a68de04c0a213fa3c","outcome":"accepted","dialect":"Strict","features":706,"tools":3,"omissions":0,"extents":{"min":{"x":1650000000,"y":1400000000},"max":{"x":112880000000,"y":118500000000}},"capabilities":"DocumentSyntax=Complete|UnitsAndFormat=Complete|Extents=Complete|Drills=Complete|Routes=NotProvided|Slots=NotProvided|Tools=Complete|Plating=Complete|LayerSpans=Complete"},
      {"path":"XNC format-test-files_en/2018 11 18 XNC - Ucamco samples/Rout.xnc","digest":"b955e9b9c2ac782995c2a982d2b3e8a416baa4d0a31c634fbc3c1c6082238fdc","outcome":"accepted","dialect":"Strict","features":1,"tools":1,"omissions":0,"extents":{"min":{"x":-1200000000,"y":-1200000000},"max":{"x":121200000000,"y":121200000000}},"capabilities":"DocumentSyntax=Complete|UnitsAndFormat=Complete|Extents=Complete|Drills=NotProvided|Routes=Complete|Slots=NotProvided|Tools=Complete|Plating=Complete|LayerSpans=Complete"},
      {"path":"XNC format-test-files_en/2018 11 18 XNC - Ucamco samples/XNC sample file from specification.xnc","digest":"220c2dd310b082d7773d51696789261257bcf2d5bd41c4e3593199d77946e6b7","outcome":"accepted","dialect":"Strict","features":9,"tools":4,"omissions":0,"extents":{"min":{"x":4500000000,"y":1100000000},"max":{"x":11500000000,"y":8400000000}},"capabilities":"DocumentSyntax=Complete|UnitsAndFormat=Complete|Extents=Complete|Drills=Complete|Routes=Complete|Slots=NotProvided|Tools=Complete|Plating=NotProvided|LayerSpans=NotProvided"},
      {"path":"XNC format-test-files_en/2021 03 13 XNC - KiCad samples/2021 03 28 KiCad XNC NPTH attributes.drl","digest":"d2f34c051f1e585959f3e894019d605d6468fe37a689fe2222898942adc2b556","outcome":"unsupported","record":2,"command":"; #@! TF.GenerationSoftware,xxxx,yyyy,zzzz"},
      {"path":"XNC format-test-files_en/2021 03 13 XNC - KiCad samples/2021 03 28 KiCad XNC PTH attributes.drl","digest":"eca3eb617d54d197c50eaad237d9e4588881c4a60e16e7010fbef54f07724681","outcome":"unsupported","record":2,"command":"; #@! TF.GenerationSoftware,xxxx,yyyy,zzzz"},
      {"path":"XNC format-test-files_en/2021 03 13 XNC - KiCad samples/2021 03 31 KiCad XNC buried attributes pic_programmer-in2-back.drl","digest":"637a424507881af429a39f3840dcd9d70c20d1a38bb5f810dfe383f123c2e406","outcome":"accepted","dialect":"KicadLegacy","features":2,"tools":1,"omissions":0,"extents":{"min":{"x":197750000000,"y":-121350000000},"max":{"x":199450000000,"y":-120650000000}},"capabilities":"DocumentSyntax=Complete|UnitsAndFormat=Complete|Extents=Complete|Drills=Complete|Routes=NotProvided|Slots=NotProvided|Tools=Complete|Plating=Complete|LayerSpans=Complete"},
      {"path":"XNC format-test-files_en/2021 04 04 XNC - LibrePCB samples/Hydro_Battery_Charger_DRILLS-NPTH.drl","digest":"1979f216a10115c8f79b4b92f0bc22a74d1023ab58aa9ca7e6878f5069037cb8","outcome":"accepted","dialect":"LibrePcbLegacy","features":7,"tools":2,"omissions":0,"extents":{"min":{"x":2500000000,"y":28000000000},"max":{"x":87100000000,"y":72000000000}},"capabilities":"DocumentSyntax=Complete|UnitsAndFormat=Complete|Extents=Complete|Drills=Complete|Routes=NotProvided|Slots=NotProvided|Tools=Complete|Plating=Complete|LayerSpans=Complete"},
      {"path":"XNC format-test-files_en/2021 04 04 XNC - LibrePCB samples/Hydro_Battery_Charger_DRILLS-PTH.drl","digest":"eb9e0764d105073711d15818b30d7871bb79e06657569a8db49d66094030e569","outcome":"accepted","dialect":"LibrePcbLegacy","features":378,"tools":11,"omissions":0,"extents":{"min":{"x":961250000,"y":4295000000},"max":{"x":118736250000,"y":95717500000}},"capabilities":"DocumentSyntax=Complete|UnitsAndFormat=Complete|Extents=Complete|Drills=Complete|Routes=NotProvided|Slots=NotProvided|Tools=Complete|Plating=Complete|LayerSpans=Complete"}
    ]"#).unwrap();
    assert_eq!(serde_json::Value::Array(records), expected);
    assert_eq!(features, 1_106);
    println!(
        "OFFICIAL XNC SUMMARY files={} accepted={} unsupported={} strict={} kicad={} librepcb={} features={} pre_remediation_permissive_features=1633 direct_verified_zip_buffers=true",
        files.len(),
        files.len() - unsupported,
        unsupported,
        strict,
        kicad,
        librepcb,
        features
    );
}

fn package_with_profile_geometry(geometry: &str) -> ManufacturingInventory {
    let profile = format!(
        "G04 profile topology*\n%FSLAX46Y46*%\n%MOMM*%\n%TF.FileFunction,Profile,NP*%\n%TA.AperFunction,Profile*%\n%ADD10C,0.200*%\nD10*\n{geometry}M02*\n"
    )
    .into_bytes();
    let mut inventory = complete_package(
        fs::read(job_fixture("complete.gbrjob")).unwrap(),
        fs::read(xnc_fixture("strict.xnc")).unwrap(),
    );
    let input = inventory
        .inputs
        .iter_mut()
        .find(|input| input.virtual_path == "fab/profile.gbr")
        .unwrap();
    input.original_bytes = profile;
    input.size = input.original_bytes.len() as u64;
    input.artifact_digest = format!("{:x}", Sha256::digest(&input.original_bytes));
    let input = input.clone();
    let outcome = inventory
        .outcomes
        .iter_mut()
        .find(|outcome| outcome.virtual_path == input.virtual_path)
        .unwrap();
    outcome.size = input.size;
    outcome.artifact_digest = Some(input.artifact_digest.clone());
    outcome.id = input_outcome_id(
        &outcome.virtual_path,
        outcome.artifact_digest.as_deref(),
        outcome.kind_candidate,
    );
    inventory
}

#[test]
fn package_profile_requires_one_closed_outer_and_contained_cutouts() {
    let rectangle = |min: i64, max: i64| {
        format!(
            "G36*\nX{min}Y{min}D02*\nX{max}Y{min}D01*\nX{max}Y{max}D01*\nX{min}Y{max}D01*\nX{min}Y{min}D01*\nG37*\n"
        )
    };
    for geometry in [
        "X000000Y000000D02*\nX10000000Y000000D01*\n".to_owned(),
        "X5000000Y5000000D03*\n".to_owned(),
        format!(
            "{}{}",
            rectangle(0, 4_000_000),
            rectangle(6_000_000, 10_000_000)
        ),
        format!("{}{}", rectangle(0, 10_000_000), rectangle(0, 3_000_000)),
    ] {
        let review =
            analyze_manufacturing_inventory(&package_with_profile_geometry(&geometry)).unwrap();
        assert_eq!(
            package_capability(&review, CapabilityId::Profile),
            CapabilityState::Partial
        );
        assert_ne!(
            package_capability(&review, CapabilityId::PackageCompleteness),
            CapabilityState::Complete
        );
    }

    let geometry = rectangle(0, 10_000_000);
    let review =
        analyze_manufacturing_inventory(&package_with_profile_geometry(&geometry)).unwrap();
    assert_eq!(
        package_capability(&review, CapabilityId::Profile),
        CapabilityState::Complete
    );
    let profile = review.profile.unwrap();
    assert_eq!(profile.contour_feature_ids.len(), 1);
    assert!(profile.cutout_feature_ids.is_empty());
}

#[test]
fn package_profile_polarity_rejects_clear_outer_dark_nested_and_mixed_regions() {
    let rectangle = |min: i64, max: i64| {
        format!(
            "G36*\nX{min}Y{min}D02*\nX{max}Y{min}D01*\nX{max}Y{max}D01*\nX{min}Y{max}D01*\nX{min}Y{min}D01*\nG37*\n"
        )
    };
    let outer = rectangle(0, 10_000_000);
    let nested = rectangle(3_000_000, 7_000_000);
    for (label, geometry) in [
        ("clear-outer", format!("%LPC*%{outer}")),
        ("dark-nested", format!("{outer}{nested}")),
        ("mixed", format!("{outer}%LPC*%{nested}")),
    ] {
        let review =
            analyze_manufacturing_inventory(&package_with_profile_geometry(&geometry)).unwrap();
        assert_ne!(
            package_capability(&review, CapabilityId::Profile),
            CapabilityState::Complete,
            "{label}"
        );
        assert_ne!(
            package_capability(&review, CapabilityId::PackageCompleteness),
            CapabilityState::Complete,
            "{label}"
        );
    }

    let review = analyze_manufacturing_inventory(&package_with_profile_geometry(&outer)).unwrap();
    assert_eq!(
        package_capability(&review, CapabilityId::Profile),
        CapabilityState::Complete
    );
}

#[test]
fn package_completeness_is_semantic_complete_only_and_conflicts_never_improve_it() {
    let job = fs::read(job_fixture("complete.gbrjob")).unwrap();
    let xnc = fs::read(xnc_fixture("strict.xnc")).unwrap();
    let complete =
        analyze_manufacturing_inventory(&complete_package(job.clone(), xnc.clone())).unwrap();
    assert_eq!(complete.status, FabricationStatus::Complete);
    for id in [
        CapabilityId::LayerRoles,
        CapabilityId::LayerOrder,
        CapabilityId::Profile,
        CapabilityId::Drills,
        CapabilityId::Plating,
        CapabilityId::LayerSpans,
        CapabilityId::Extents,
        CapabilityId::PackageCompleteness,
    ] {
        assert_eq!(package_capability(&complete, id), CapabilityState::Complete);
    }
    assert_eq!(
        dispatch_analyzer(
            PACKAGE_GERBERS_ANALYZER,
            &complete.capabilities,
            Some(SemanticAnalyzerResult::Pass)
        )
        .status,
        AnalyzerDispatchStatus::NotChecked
    );
    complete.validate().unwrap();

    let mut without_job = complete_package(job.clone(), xnc.clone());
    without_job
        .inputs
        .retain(|input| input.kind_candidate != ManufacturingKindCandidate::GerberJob);
    without_job
        .outcomes
        .retain(|outcome| outcome.kind_candidate != ManufacturingKindCandidate::GerberJob);
    let partial = analyze_manufacturing_inventory(&without_job).unwrap();
    assert_eq!(
        package_capability(&partial, CapabilityId::PackageCompleteness),
        CapabilityState::Partial
    );

    let conflict_job = String::from_utf8(job.clone())
        .unwrap()
        .replace("Copper,L1,Top", "Copper,L2,Bot")
        .into_bytes();
    let conflicted =
        analyze_manufacturing_inventory(&complete_package(conflict_job, xnc.clone())).unwrap();
    assert!(!conflicted.conflicts.is_empty());
    assert_eq!(
        package_capability(&conflicted, CapabilityId::LayerRoles),
        CapabilityState::Partial
    );
    assert_eq!(
        package_capability(&conflicted, CapabilityId::PackageCompleteness),
        CapabilityState::Partial
    );

    let unknown_plating = String::from_utf8(xnc)
        .unwrap()
        .replace("; #@! TF.FileFunction,Plated,1,2,PTH\n", "")
        .into_bytes();
    let partial = analyze_manufacturing_inventory(&complete_package(job, unknown_plating)).unwrap();
    assert_ne!(
        package_capability(&partial, CapabilityId::Plating),
        CapabilityState::Complete
    );
    assert_eq!(
        package_capability(&partial, CapabilityId::PackageCompleteness),
        CapabilityState::Partial
    );
}

fn matching_native_kicad() -> Vec<u8> {
    br#"(kicad_pcb (version 20240108) (generator ratemypcb-fixture)
  (title_block (title "phase5-board") (rev "r1"))
  (layers
    (0 "F.Cu" signal)
    (31 "B.Cu" signal)
    (44 "Edge.Cuts" user "Edge.Cuts")
  )
  (net 0 "")
  (net 1 "GND")
  (footprint "Fixture:Connector" (layer "F.Cu") (at 0 0)
    (property "Reference" "U1")
    (pad "1" thru_hole circle (at 1 1) (size 1 1) (drill 0.6)
      (layers "*.Cu" "*.Mask") (net 1 "GND"))
    (pad "1" thru_hole oval (at 5.5 5) (size 2 1) (drill oval 1.6 0.6)
      (layers "*.Cu" "*.Mask") (net 1 "GND"))
  )
  (gr_rect (start 0 0) (end 10 10) (layer "Edge.Cuts"))
)"#
    .to_vec()
}

fn four_layer_native_kicad() -> Vec<u8> {
    String::from_utf8(matching_native_kicad())
        .unwrap()
        .replace(
            "    (31 \"B.Cu\" signal)",
            "    (1 \"In1.Cu\" signal)\n    (2 \"In2.Cu\" signal)\n    (31 \"B.Cu\" signal)",
        )
        .into_bytes()
}

fn complete_four_layer_package_review() -> FabricationReview {
    let mut job: serde_json::Value =
        serde_json::from_slice(&fs::read(job_fixture("complete.gbrjob")).unwrap()).unwrap();
    job["FilesAttributes"] = serde_json::json!([
        {"Path": "top.gbr", "FileFunction": "Copper,L1,Top"},
        {"Path": "inner1.gbr", "FileFunction": "Copper,L2,Inr"},
        {"Path": "inner2.gbr", "FileFunction": "Copper,L3,Inr"},
        {"Path": "bottom.gbr", "FileFunction": "Copper,L4,Bot"},
        {"Path": "profile.gbr", "FileFunction": "Profile,NP"},
        {"Path": "holes.xnc", "FileFunction": "Plated,1,4,PTH"}
    ]);
    let xnc = fs::read_to_string(xnc_fixture("strict.xnc"))
        .unwrap()
        .replace(
            "TF.FileFunction,Plated,1,2,PTH",
            "TF.FileFunction,Plated,1,4,PTH",
        )
        .into_bytes();
    analyze_manufacturing_inventory(&inventory_from_files(vec![
        (
            "fab/top.gbr",
            ManufacturingKindCandidate::Gerber,
            x2_layer("Copper,L1,Top", false, false),
        ),
        (
            "fab/inner1.gbr",
            ManufacturingKindCandidate::Gerber,
            x2_layer("Copper,L2,Inr", false, false),
        ),
        (
            "fab/inner2.gbr",
            ManufacturingKindCandidate::Gerber,
            x2_layer("Copper,L3,Inr", false, false),
        ),
        (
            "fab/bottom.gbr",
            ManufacturingKindCandidate::Gerber,
            x2_layer("Copper,L4,Bot", false, false),
        ),
        (
            "fab/profile.gbr",
            ManufacturingKindCandidate::Gerber,
            x2_layer("Profile,NP", true, false),
        ),
        ("fab/holes.xnc", ManufacturingKindCandidate::Excellon, xnc),
        (
            "fab/complete.gbrjob",
            ManufacturingKindCandidate::GerberJob,
            serde_json::to_vec(&job).unwrap(),
        ),
    ]))
    .unwrap()
}

fn complete_package_review() -> FabricationReview {
    analyze_manufacturing_inventory(&complete_package(
        fs::read(job_fixture("complete.gbrjob")).unwrap(),
        fs::read(xnc_fixture("strict.xnc")).unwrap(),
    ))
    .unwrap()
}

fn reconciliation(
    review: &FabricationReview,
    family: ReconciliationFamily,
) -> &ManufacturingReconciliation {
    review
        .reconciliations
        .iter()
        .find(|item| item.family == family)
        .unwrap()
}

fn change_first_profile_segment(review: &mut FabricationReview) {
    let id = review.profile.as_ref().unwrap().contour_feature_ids[0].clone();
    let feature = review
        .features
        .iter_mut()
        .find(|feature| feature.id == id)
        .unwrap();
    let segment = match &mut feature.geometry {
        Geometry::Contour(contour) => &mut contour.segments[0],
        Geometry::Region(region) => &mut region.contours[0].segments[0],
        _ => panic!("profile must retain contour geometry"),
    };
    match segment {
        ContourSegment::Line(line) => line.end.x.0 += 1,
        ContourSegment::Arc(arc) => arc.end.x.0 += 1,
    }
}

fn refresh_release_source_pair(review: &mut FabricationReview) {
    let mut document_ids = review
        .documents
        .iter()
        .filter(|document| document.format != DocumentFormat::KicadPcb)
        .map(|document| &document.id)
        .collect::<Vec<_>>();
    document_ids.sort();
    let product = review.product.as_ref().map(|product| {
        (
            product.name.as_deref(),
            product.revision.as_deref(),
            product.part_number.as_deref(),
            product.authority,
        )
    });
    let encoded = serde_json::to_vec(&(
        "fabrication-identity-v1",
        "package",
        &(document_ids, product),
    ))
    .unwrap();
    let release_package_id = format!("package-v1-{:x}", Sha256::digest(encoded));
    let pair = review.source_pair.as_mut().unwrap();
    pair.release_package_id = release_package_id;
    let mut digests = pair.release_document_digests.clone();
    digests.sort();
    let encoded = serde_json::to_vec(&(
        "fabrication-identity-v1",
        "source-pair",
        &(
            &pair.native_document_id,
            &pair.native_artifact_digest,
            &pair.release_package_id,
            digests,
        ),
    ))
    .unwrap();
    pair.id = format!("source-pair-v1-{:x}", Sha256::digest(encoded));
}

fn mutate_reconciled_source(
    review: &mut FabricationReview,
    family: ReconciliationFamily,
    native: bool,
) {
    if native {
        let source = review.native_reconciliation_source.as_mut().unwrap();
        match family {
            ReconciliationFamily::Product => {
                source.review.product.as_mut().unwrap().name = Some("forged-native".into())
            }
            ReconciliationFamily::Layers => {
                let changed = source
                    .review
                    .layers
                    .iter_mut()
                    .find(|layer| layer.role == LayerRole::Copper)
                    .unwrap();
                changed.side = if changed.side == LayerSide::Top {
                    LayerSide::Bottom
                } else {
                    LayerSide::Top
                };
                let changed = changed.clone();
                let id = changed.id.clone();
                *review
                    .layers
                    .iter_mut()
                    .find(|layer| layer.id == id)
                    .unwrap() = changed;
            }
            ReconciliationFamily::Profile => {
                change_first_profile_segment(&mut source.review);
                let id = source.review.profile.as_ref().unwrap().contour_feature_ids[0].clone();
                let changed = source
                    .review
                    .features
                    .iter()
                    .find(|feature| feature.id == id)
                    .unwrap()
                    .clone();
                *review
                    .features
                    .iter_mut()
                    .find(|feature| feature.id == id)
                    .unwrap() = changed;
            }
            ReconciliationFamily::Drills => {
                let changed = source
                    .review
                    .features
                    .iter_mut()
                    .find(|feature| matches!(feature.geometry, Geometry::Drill(_)))
                    .unwrap();
                let Geometry::Drill(drill) = &mut changed.geometry else {
                    unreachable!()
                };
                drill.position.x.0 += 1;
                let changed = changed.clone();
                let id = changed.id.clone();
                *review
                    .features
                    .iter_mut()
                    .find(|feature| feature.id == id)
                    .unwrap() = changed;
            }
            ReconciliationFamily::Extents => {
                let extents = source.extents.as_mut().unwrap();
                extents.max.x.0 += 1;
                source.review.profile.as_mut().unwrap().extents = Some(extents.clone());
            }
            ReconciliationFamily::Connectivity => {
                let changed = &mut source.review.connectivity[0];
                changed.net.as_mut().unwrap().push_str("-forged");
                let changed = changed.clone();
                let feature_id = changed.feature_id.clone();
                *review
                    .connectivity
                    .iter_mut()
                    .find(|item| item.feature_id == feature_id)
                    .unwrap() = changed;
            }
        }
        source.review.refresh_digests().unwrap();
    } else {
        match family {
            ReconciliationFamily::Product => {
                review.product.as_mut().unwrap().name = Some("forged-package".into());
                refresh_release_source_pair(review);
            }
            ReconciliationFamily::Layers => {
                let native_document_id = review
                    .source_pair
                    .as_ref()
                    .unwrap()
                    .native_document_id
                    .clone();
                let changed = review
                    .layers
                    .iter_mut()
                    .find(|layer| {
                        layer.document_id != native_document_id && layer.role == LayerRole::Copper
                    })
                    .unwrap();
                changed.side = if changed.side == LayerSide::Top {
                    LayerSide::Bottom
                } else {
                    LayerSide::Top
                };
            }
            ReconciliationFamily::Profile => change_first_profile_segment(review),
            ReconciliationFamily::Drills => {
                let changed = review
                    .features
                    .iter_mut()
                    .find(|feature| {
                        review.documents.iter().any(|document| {
                            document.id == feature.document_id
                                && document.format == DocumentFormat::Excellon
                        }) && matches!(feature.geometry, Geometry::Drill(_))
                    })
                    .unwrap();
                let Geometry::Drill(drill) = &mut changed.geometry else {
                    unreachable!()
                };
                drill.position.x.0 += 1;
            }
            ReconciliationFamily::Extents => {
                review
                    .profile
                    .as_mut()
                    .unwrap()
                    .extents
                    .as_mut()
                    .unwrap()
                    .max
                    .x
                    .0 += 1
            }
            ReconciliationFamily::Connectivity => review.connectivity[0]
                .net
                .as_mut()
                .unwrap()
                .push_str("-forged"),
        }
    }
    review.refresh_digests().unwrap();
}

#[test]
fn retained_bounds_job_and_integration_facts_round_trip_and_bind_model_digest() {
    let review = complete_package_review();
    assert_eq!(review.physical_bounds.len(), 4);
    assert_eq!(review.job_file_functions.len(), 4);
    let encoded = serde_json::to_vec(&review).unwrap();
    let decoded: FabricationReview = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded, review);
    decoded.validate().unwrap();

    let mut bounds_mutation = review.clone();
    let original = bounds_mutation.model_digest.clone();
    bounds_mutation.physical_bounds[0].geometry_digest = "f".repeat(64);
    bounds_mutation.refresh_digests().unwrap();
    assert_ne!(bounds_mutation.model_digest, original);
    assert!(bounds_mutation.validate().is_err());

    let mut job_mutation = review;
    let original = job_mutation.model_digest.clone();
    job_mutation.job_file_functions[0].fields[0].push_str("-forged");
    job_mutation.refresh_digests().unwrap();
    assert_ne!(job_mutation.model_digest, original);
    assert!(job_mutation.validate().is_err());
}

#[test]
fn package_physical_bounds_reject_outside_gerber_and_accept_inside_and_boundary() {
    let job = fs::read(job_fixture("complete.gbrjob")).unwrap();
    let xnc = fs::read(xnc_fixture("strict.xnc")).unwrap();
    for (endpoint, expected) in [
        ("X9800000Y1000000D01*", CapabilityState::Complete),
        ("X9900000Y1000000D01*", CapabilityState::Complete),
        ("X11000000Y1000000D01*", CapabilityState::Partial),
    ] {
        let mut package = complete_package(job.clone(), xnc.clone());
        let top = String::from_utf8(x2_layer("Copper,L1,Top", false, false))
            .unwrap()
            .replace("X2000000Y1000000D01*", endpoint)
            .into_bytes();
        replace_inventory_path(&mut package, "fab/top.gbr", top);
        let review = analyze_manufacturing_inventory(&package).unwrap();
        assert_eq!(
            package_capability(&review, CapabilityId::Extents),
            expected,
            "endpoint={endpoint}"
        );
        assert_eq!(
            review.status == FabricationStatus::Complete,
            expected == CapabilityState::Complete,
            "endpoint={endpoint}"
        );
    }

    let mut forged = reconcile_native_package(
        complete_package_review(),
        parse_native_kicad_manufacturing("board.kicad_pcb", &matching_native_kicad()).unwrap(),
    )
    .unwrap();
    let top_document = forged
        .layers
        .iter()
        .find(|layer| {
            layer.role == LayerRole::Copper
                && layer.side == LayerSide::Top
                && layer.authority == Authority::Explicit
        })
        .unwrap()
        .document_id
        .clone();
    let line = forged
        .features
        .iter_mut()
        .find(|feature| feature.document_id == top_document)
        .and_then(|feature| match &mut feature.geometry {
            Geometry::Line(line) => Some(line),
            _ => None,
        })
        .unwrap();
    line.end.x = Picometres(11_000_000_000);
    forged.status = FabricationStatus::Complete;
    for id in [
        CapabilityId::Extents,
        CapabilityId::PackageCompleteness,
        CapabilityId::PackageReconciliation,
    ] {
        forged
            .capabilities
            .records
            .iter_mut()
            .find(|record| record.id == id)
            .unwrap()
            .state = CapabilityState::Complete;
    }
    forged.refresh_digests().unwrap();
    assert!(matches!(
        forged.validate(),
        Err(FabricationError::InvalidIdentity(ref id))
            if id == "authoritative-physical-bounds"
    ));

    forged.refresh_physical_bounds().unwrap();
    forged.refresh_digests().unwrap();
    assert!(matches!(
        forged.validate(),
        Err(FabricationError::InvalidIdentity(ref id))
            if id == "authoritative-capability:Extents"
    ));

    let bounds = forged
        .physical_bounds
        .iter_mut()
        .find(|bounds| bounds.document_id == top_document)
        .unwrap();
    bounds.extent.max.x = Picometres(10_000_000_000);
    bounds.id = fixture_stable_id(
        "physical-bounds",
        &(
            &bounds.document_id,
            &bounds.artifact_digest,
            bounds.format,
            &bounds.extent,
            bounds.resolution,
            &bounds.geometry_digest,
            &bounds.source_locations,
            (
                bounds.provenance.document_id.as_str(),
                bounds.provenance.artifact_digest.as_str(),
                bounds.provenance.producer.as_str(),
                bounds.provenance.producer_version.as_str(),
                &bounds.provenance.location,
            ),
        ),
    );
    forged.refresh_digests().unwrap();
    assert!(matches!(
        forged.validate(),
        Err(FabricationError::InvalidIdentity(ref id))
            if id == "authoritative-physical-bounds"
    ));
}

#[test]
fn gerber_full_circle_bounds_downgrade_package_reconciliation_and_approval() {
    let job = fs::read(job_fixture("complete.gbrjob")).unwrap();
    let xnc = fs::read(xnc_fixture("strict.xnc")).unwrap();
    for (start_x, expected_max_x, complete) in [
        (5_800_000, 9_900_000_000_i64, true),
        (5_900_000, 10_000_000_000_i64, true),
        (7_000_000, 11_100_000_000_i64, false),
    ] {
        let mut package = complete_package(job.clone(), xnc.clone());
        let arc =
            format!("G75*\nG03*\nX{start_x}Y5000000D02*\nX{start_x}Y5000000I2000000J000000D01*\n");
        let top = String::from_utf8(x2_layer("Copper,L1,Top", false, false))
            .unwrap()
            .replace("X1000000Y1000000D02*\nX2000000Y1000000D01*\n", &arc)
            .into_bytes();
        replace_inventory_path(&mut package, "fab/top.gbr", top);
        let package = analyze_manufacturing_inventory(&package).unwrap();
        let top_document = package
            .documents
            .iter()
            .find(|document| document.virtual_path == "fab/top.gbr")
            .unwrap();
        let top_bounds = package
            .physical_bounds
            .iter()
            .find(|bounds| bounds.document_id == top_document.id)
            .unwrap();
        assert_eq!(top_bounds.extent.max.x.0, expected_max_x, "start={start_x}");
        assert_eq!(
            package_capability(&package, CapabilityId::Extents),
            if complete {
                CapabilityState::Complete
            } else {
                CapabilityState::Partial
            },
            "start={start_x}"
        );
        assert_eq!(
            package_capability(&package, CapabilityId::PackageCompleteness),
            if complete {
                CapabilityState::Complete
            } else {
                CapabilityState::Partial
            },
            "start={start_x}"
        );

        let reconciled = reconcile_native_package(
            package,
            parse_native_kicad_manufacturing("board.kicad_pcb", &matching_native_kicad()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            reconciliation(&reconciled, ReconciliationFamily::Extents).status,
            if complete {
                ReconciliationStatus::Match
            } else {
                ReconciliationStatus::NotChecked
            },
            "start={start_x}"
        );
        assert_eq!(
            package_capability(&reconciled, CapabilityId::PackageReconciliation),
            if complete {
                CapabilityState::Complete
            } else {
                CapabilityState::Partial
            },
            "start={start_x}"
        );
        assert_eq!(
            reconciled.status == FabricationStatus::Complete,
            complete,
            "start={start_x}"
        );
        assert_eq!(
            dispatch_analyzer(
                PACKAGE_GERBERS_ANALYZER,
                &reconciled.capabilities,
                Some(SemanticAnalyzerResult::Pass),
            )
            .status
                == AnalyzerDispatchStatus::Pass,
            complete,
            "start={start_x}"
        );
    }
}

#[test]
fn gerber_full_circle_bounds_survive_macro_block_transform_and_repetition() {
    let source = br#"G04 full-circle nested physical bounds fixture*
%FSLAX46Y46*%
%MOMM*%
%TF.FileFunction,Copper,L1,Top*%
%AMDOT*1,1,0.4,0,0*%
%ADD10C,0.200*%
%ADD12DOT*%
%ABD11*%
D12*
X000000Y000000D03*
D10*
G75*
G03*
X-2000000Y000000D02*
X-2000000Y000000I2000000J000000D01*
%AB*%
D11*
%LMX*%
%LR90*%
%LS1.0*%
%SRX2Y1I1.0J0*%
X8000000Y5000000D03*
%SR*%
M02*
"#;
    let production = production_gerber_bytes(source).unwrap();
    production.review.validate().unwrap();
    let bounds = production.review.physical_bounds.first().unwrap();
    assert_eq!(bounds.extent.max.x.0, 11_100_000_000);
    assert_eq!(bounds.extent.max.y.0, 7_100_000_000);
    assert_eq!(
        production.review.blocks[0].instantiation_feature_ids.len(),
        1
    );

    let mut mismatched_instantiation = production.review;
    mismatched_instantiation.blocks[0]
        .instantiation_feature_ids
        .clear();
    mismatched_instantiation.refresh_physical_bounds().unwrap();
    mismatched_instantiation.refresh_digests().unwrap();
    assert!(matches!(
        mismatched_instantiation.validate(),
        Err(FabricationError::InvalidIdentity(reason)) if reason == "block-membership"
    ));
}

#[test]
fn block_membership_rejects_coherent_unused_block_geometry_laundering() {
    let source = br#"G04 block-membership laundering fixture*
%FSLAX46Y46*%
%MOMM*%
%TF.FileFunction,Copper,L1,Top*%
%ADD10C,0.200*%
%ABD11*%
D10*
X000000Y000000D03*
%AB*%
D10*
X1000000Y1000000D02*
X2000000Y1000000D01*
G75*
G03*
X7000000Y5000000D02*
X7000000Y5000000I-2000000J000000D01*
M02*
"#;
    let review = production_gerber_bytes(source).unwrap().review;
    review.validate().unwrap();
    let block = &review.blocks[0];
    assert!(block.instantiation_feature_ids.is_empty());
    assert!(review.features.iter().any(|feature| {
        matches!(
            &feature.membership,
            FeatureMembership::ApertureBlock { block_id, aperture_id }
                if block_id == &block.id && aperture_id == &block.aperture_id
        )
    }));
    let outside = review
        .features
        .iter()
        .find(|feature| matches!(feature.geometry, Geometry::Arc(_)))
        .unwrap()
        .id
        .clone();

    let mut laundering = review.clone();
    laundering.blocks[0].feature_ids.push(outside);
    assert!(matches!(
        laundering.refresh_physical_bounds(),
        Err(FabricationError::InvalidIdentity(reason)) if reason == "block-membership"
    ));
    for capability in &mut laundering.capabilities.records {
        capability.state = CapabilityState::Complete;
    }
    laundering.refresh_digests().unwrap();
    assert!(matches!(
        laundering.validate(),
        Err(FabricationError::InvalidIdentity(reason)) if reason == "block-membership"
    ));

    let mut duplicate = review.clone();
    let duplicated_member = duplicate.blocks[0].feature_ids[0].clone();
    duplicate.blocks[0].feature_ids.push(duplicated_member);
    duplicate.refresh_digests().unwrap();
    assert!(matches!(
        duplicate.validate(),
        Err(FabricationError::InvalidIdentity(reason)) if reason == "block-membership"
    ));

    let mut orphan = review.clone();
    let member_id = orphan.blocks[0].feature_ids[0].clone();
    let member = orphan
        .features
        .iter_mut()
        .find(|feature| feature.id == member_id)
        .unwrap();
    member.membership = FeatureMembership::ApertureBlock {
        block_id: "block-v1-ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
            .into(),
        aperture_id: orphan.blocks[0].aperture_id.clone(),
    };
    member.id = feature_id_with_membership(
        &member.document_id,
        &member.layer_id,
        member.geometry.kind_name(),
        &member.provenance.location,
        &member.membership,
    );
    orphan.blocks[0].feature_ids[0] = member.id.clone();
    orphan.refresh_digests().unwrap();
    assert!(matches!(
        orphan.validate(),
        Err(FabricationError::InvalidIdentity(reason)) if reason == "block-membership"
    ));

    let mut mismatched = review.clone();
    let member_id = mismatched.blocks[0].feature_ids[0].clone();
    let wrong_aperture = mismatched
        .apertures
        .iter()
        .find(|aperture| aperture.shape == ApertureShape::Circle)
        .unwrap()
        .id
        .clone();
    let member = mismatched
        .features
        .iter_mut()
        .find(|feature| feature.id == member_id)
        .unwrap();
    member.membership = FeatureMembership::ApertureBlock {
        block_id: mismatched.blocks[0].id.clone(),
        aperture_id: wrong_aperture,
    };
    member.id = feature_id_with_membership(
        &member.document_id,
        &member.layer_id,
        member.geometry.kind_name(),
        &member.provenance.location,
        &member.membership,
    );
    mismatched.blocks[0].feature_ids[0] = member.id.clone();
    mismatched.refresh_digests().unwrap();
    assert!(matches!(
        mismatched.validate(),
        Err(FabricationError::InvalidIdentity(reason)) if reason == "block-membership"
    ));

    let job = fs::read(job_fixture("complete.gbrjob")).unwrap();
    let xnc = fs::read(xnc_fixture("strict.xnc")).unwrap();
    let mut inventory = complete_package(job, xnc);
    replace_inventory_path(&mut inventory, "fab/top.gbr", source.to_vec());
    let mut cross_document = analyze_manufacturing_inventory(&inventory).unwrap();
    let top_block = cross_document.blocks[0].id.clone();
    let bottom_feature = cross_document
        .features
        .iter()
        .find(|feature| {
            cross_document.documents.iter().any(|document| {
                document.id == feature.document_id && document.virtual_path == "fab/bottom.gbr"
            })
        })
        .unwrap()
        .id
        .clone();
    cross_document
        .blocks
        .iter_mut()
        .find(|block| block.id == top_block)
        .unwrap()
        .feature_ids
        .push(bottom_feature);
    assert!(matches!(
        cross_document.refresh_physical_bounds(),
        Err(FabricationError::InvalidIdentity(reason)) if reason == "block-membership"
    ));
}

#[test]
fn round7_coherent_provenance_relocation_cannot_restore_approval_or_round_trip() {
    let source = br#"G04 coherent source relocation fixture*
%FSLAX46Y46*%
%MOMM*%
%TF.FileFunction,Copper,L1,Top*%
%TA.AperFunction,Conductor*%
%ADD10C,0.200*%
%TO.N,GND*%
%TO.C,U1*%
%TO.P,U1,1*%
%ABD11*%
D10*
X000000Y000000D03*
%AB*%
D10*
X1000000Y1000000D02*
X2000000Y1000000D01*
G75*
G03*
X7000000Y5000000D02*
X7000000Y5000000I2000000J000000D01*
M02*
"#;
    let job = fs::read(job_fixture("complete.gbrjob")).unwrap();
    let xnc = fs::read(xnc_fixture("strict.xnc")).unwrap();
    let mut inventory = complete_package(job, xnc);
    replace_inventory_path(&mut inventory, "fab/top.gbr", source.to_vec());
    let mut forged = analyze_manufacturing_inventory(&inventory).unwrap();
    assert_eq!(
        package_capability(&forged, CapabilityId::Extents),
        CapabilityState::Partial
    );

    let block = forged.blocks[0].clone();
    assert!(block.instantiation_feature_ids.is_empty());
    let member = forged
        .features
        .iter()
        .find(|feature| block.feature_ids.contains(&feature.id))
        .unwrap()
        .clone();
    let outside_index = forged
        .features
        .iter()
        .position(|feature| matches!(feature.geometry, Geometry::Arc(_)))
        .unwrap();
    let old_outside_id = forged.features[outside_index].id.clone();
    let outside = &mut forged.features[outside_index];
    outside.provenance = member.provenance.clone();
    outside.membership = FeatureMembership::ApertureBlock {
        block_id: block.id.clone(),
        aperture_id: block.aperture_id.clone(),
    };
    outside.id = feature_id_with_membership(
        &outside.document_id,
        &outside.layer_id,
        outside.geometry.kind_name(),
        &outside.provenance.location,
        &outside.membership,
    );
    let new_outside_id = outside.id.clone();
    forged.blocks[0].feature_ids.push(new_outside_id.clone());
    let replace = |id: &mut String| {
        if *id == old_outside_id {
            *id = new_outside_id.clone();
        }
    };
    for repeat in &mut forged.repetitions {
        for id in &mut repeat.feature_ids {
            replace(id);
        }
    }
    for semantic in &mut forged.connectivity {
        replace(&mut semantic.feature_id);
    }
    for attribute in &mut forged.x2_attributes {
        for id in &mut attribute.target_ids {
            replace(id);
        }
        attribute.id = fixture_stable_id(
            "x2-attribute",
            &(
                &attribute.document_id,
                attribute.scope,
                attribute.kind,
                &attribute.values,
                attribute.deletion,
                &attribute.target_ids,
                &attribute.provenance.location,
            ),
        );
    }
    if let Some(profile) = &mut forged.profile {
        for id in profile
            .contour_feature_ids
            .iter_mut()
            .chain(profile.cutout_feature_ids.iter_mut())
        {
            replace(id);
        }
    }
    forged.refresh_physical_bounds().unwrap();
    forged.status = FabricationStatus::Complete;
    for capability in &mut forged.capabilities.records {
        if matches!(
            capability.id,
            CapabilityId::Extents | CapabilityId::PackageCompleteness
        ) {
            capability.state = CapabilityState::Complete;
        }
    }
    forged.omissions.retain(|omission| {
        omission.affected_capabilities.iter().any(|id| {
            forged.capabilities.records.iter().any(|capability| {
                capability.id == *id && capability.state != CapabilityState::Complete
            })
        })
    });
    forged.refresh_digests().unwrap();

    let forged_valid = forged.validate().is_ok();
    let round_trip: FabricationReview =
        serde_json::from_value(serde_json::to_value(&forged).unwrap()).unwrap();
    let round_trip_valid = round_trip.validate().is_ok();
    let approval_survives = forged_valid
        && reconcile_native_package(
            forged,
            parse_native_kicad_manufacturing("board.kicad_pcb", &matching_native_kicad()).unwrap(),
        )
        .is_ok_and(|review| {
            review.status == FabricationStatus::Complete
                && dispatch_analyzer(
                    PACKAGE_GERBERS_ANALYZER,
                    &review.capabilities,
                    Some(SemanticAnalyzerResult::Pass),
                )
                .status
                    == AnalyzerDispatchStatus::Pass
        });
    assert!(
        !approval_survives && !round_trip_valid,
        "coherently relocated mutable provenance must not survive validation or regain approval"
    );
}

#[test]
fn round8_large_definition_coordinate_mutation_changes_geometry_and_model_digests() {
    let mut source = String::from("%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.200*%\n%ABD11*%\nD10*\n");
    for coordinate in 0..=MANUFACTURING_LIMITS.macros {
        source.push_str(&format!("X{coordinate:06}Y000000D03*\n"));
    }
    source.push_str("%AB*%\nD10*\nX2000000Y2000000D03*\nM02*\n");
    let mut review = production_gerber_bytes(source.as_bytes()).unwrap().review;
    review.validate().unwrap();
    let original_model_digest = review.model_digest.clone();
    let original_geometry_digest = review.physical_bounds[0].geometry_digest.clone();
    assert_eq!(
        review
            .features
            .iter()
            .filter(|feature| matches!(feature.membership, FeatureMembership::ApertureBlock { .. }))
            .count(),
        MANUFACTURING_LIMITS.macros + 1
    );
    assert_eq!(
        review.physical_bounds[0].extent,
        Extent {
            min: CanonicalPoint::new(-MAX_COORDINATE_PM, -MAX_COORDINATE_PM),
            max: CanonicalPoint::new(MAX_COORDINATE_PM, MAX_COORDINATE_PM),
        }
    );
    let definition = review
        .features
        .iter_mut()
        .find(|feature| matches!(feature.membership, FeatureMembership::ApertureBlock { .. }))
        .unwrap();
    let Geometry::Flash(flash) = &mut definition.geometry else {
        panic!("expected block-definition flash");
    };
    flash.position.x.0 += 1;
    review.refresh_physical_bounds().unwrap();
    review.refresh_digests().unwrap();
    review.validate().unwrap();
    assert_eq!(
        review.physical_bounds[0].extent,
        Extent {
            min: CanonicalPoint::new(-MAX_COORDINATE_PM, -MAX_COORDINATE_PM),
            max: CanonicalPoint::new(MAX_COORDINATE_PM, MAX_COORDINATE_PM),
        }
    );
    assert_ne!(
        review.physical_bounds[0].geometry_digest, original_geometry_digest,
        "large conservative bounds must still commit to exact definition geometry"
    );
    assert_ne!(
        review.model_digest, original_model_digest,
        "physically bound geometry changes must change model identity"
    );
}

#[test]
fn package_profile_complete_requires_an_axis_aligned_rectangle() {
    let l_profile = "G36*\nX000000Y000000D02*\nX10000000Y000000D01*\nX10000000Y4000000D01*\nX4000000Y4000000D01*\nX4000000Y10000000D01*\nX000000Y10000000D01*\nX000000Y000000D01*\nG37*\n";
    let mut package = package_with_profile_geometry(l_profile);
    let top = String::from_utf8(x2_layer("Copper,L1,Top", false, false))
        .unwrap()
        .replace(
            "X1000000Y1000000D02*\nX2000000Y1000000D01*\n",
            "X8000000Y8000000D03*\n",
        )
        .into_bytes();
    replace_inventory_path(&mut package, "fab/top.gbr", top);
    let outside_l = analyze_manufacturing_inventory(&package).unwrap();
    for capability in [
        CapabilityId::Profile,
        CapabilityId::Extents,
        CapabilityId::PackageCompleteness,
    ] {
        assert_ne!(
            package_capability(&outside_l, capability),
            CapabilityState::Complete,
            "{capability:?}"
        );
    }
    assert_ne!(outside_l.status, FabricationStatus::Complete);

    for geometry in [
        "G36*\nX000000Y5000000D02*\nX5000000Y000000D01*\nX10000000Y5000000D01*\nX5000000Y10000000D01*\nX000000Y5000000D01*\nG37*\n",
        "G36*\nX000000Y000000D02*\nX10000000Y000000D01*\nX10000000Y10000000D01*\nX6000000Y10000000D01*\nX6000000Y4000000D01*\nX4000000Y4000000D01*\nX4000000Y10000000D01*\nX000000Y10000000D01*\nX000000Y000000D01*\nG37*\n",
        "G36*\nX000000Y000000D02*\nX5000000Y000000D01*\nX10000000Y000000D01*\nX10000000Y10000000D01*\nX000000Y10000000D01*\nX000000Y000000D01*\nG37*\n",
    ] {
        let review =
            analyze_manufacturing_inventory(&package_with_profile_geometry(geometry)).unwrap();
        assert_ne!(
            package_capability(&review, CapabilityId::Profile),
            CapabilityState::Complete,
            "unsupported profile geometry must fail closed"
        );
        assert_ne!(
            package_capability(&review, CapabilityId::PackageCompleteness),
            CapabilityState::Complete
        );
    }

    let rectangle = "G36*\nX000000Y000000D02*\nX10000000Y000000D01*\nX10000000Y10000000D01*\nX000000Y10000000D01*\nX000000Y000000D01*\nG37*\n";
    let rectangle =
        analyze_manufacturing_inventory(&package_with_profile_geometry(rectangle)).unwrap();
    assert_eq!(
        package_capability(&rectangle, CapabilityId::Profile),
        CapabilityState::Complete
    );
}

#[test]
fn retained_job_authority_rejects_cross_document_top_bottom_swap() {
    let mut forged = reconcile_native_package(
        complete_package_review(),
        parse_native_kicad_manufacturing("board.kicad_pcb", &matching_native_kicad()).unwrap(),
    )
    .unwrap();
    let retained_job = forged.job_file_functions.clone();
    let gerber_ids = forged
        .documents
        .iter()
        .filter(|document| document.format == DocumentFormat::Gerber)
        .map(|document| document.id.clone())
        .collect::<BTreeSet<_>>();
    let mut layer_ids = std::collections::BTreeMap::new();
    for layer in forged
        .layers
        .iter_mut()
        .filter(|layer| gerber_ids.contains(&layer.document_id) && layer.role == LayerRole::Copper)
    {
        let old = layer.id.clone();
        match layer.side {
            LayerSide::Top => {
                layer.side = LayerSide::Bottom;
                layer.order = Some(2);
            }
            LayerSide::Bottom => {
                layer.side = LayerSide::Top;
                layer.order = Some(1);
            }
            side => panic!("unexpected package copper side {side:?}"),
        }
        layer.id = layer_id(
            &layer.document_id,
            layer.name.as_deref(),
            layer.role,
            layer.side,
            layer.order,
            layer.authority,
            &layer.provenance.location,
        );
        layer_ids.insert(old, layer.id.clone());
    }
    assert_eq!(layer_ids.len(), 2);

    let mut feature_ids = std::collections::BTreeMap::new();
    for feature in &mut forged.features {
        if let Some(layer_id) = layer_ids.get(&feature.layer_id) {
            let old = feature.id.clone();
            feature.layer_id = layer_id.clone();
            feature.id = feature_id(
                &feature.document_id,
                &feature.layer_id,
                match &feature.geometry {
                    Geometry::Point(_) => "point",
                    Geometry::Line(_) => "line",
                    Geometry::Arc(_) => "arc",
                    Geometry::Contour(_) => "contour",
                    Geometry::Region(_) => "region",
                    Geometry::Flash(_) => "flash",
                    Geometry::Drill(_) => "drill",
                    Geometry::Route(_) => "route",
                    Geometry::Slot(_) => "slot",
                },
                &feature.provenance.location,
            );
            feature_ids.insert(old, feature.id.clone());
        }
    }
    let replace = |id: &mut String| {
        if let Some(value) = layer_ids.get(id).or_else(|| feature_ids.get(id)) {
            *id = value.clone();
        }
    };
    for tool in &mut forged.tools {
        if let Some(span) = &mut tool.span {
            for id in span
                .from_layer_id
                .iter_mut()
                .chain(span.to_layer_id.iter_mut())
            {
                replace(id);
            }
        }
    }
    for block in &mut forged.blocks {
        for id in &mut block.feature_ids {
            replace(id);
        }
    }
    for repeat in &mut forged.repetitions {
        for id in &mut repeat.feature_ids {
            replace(id);
        }
    }
    for semantic in &mut forged.connectivity {
        replace(&mut semantic.feature_id);
    }
    for attribute in &mut forged.x2_attributes {
        for target in &mut attribute.target_ids {
            replace(target);
        }
        if attribute.scope == X2AttributeScope::File
            && attribute.kind == X2AttributeKind::FileFunction
            && attribute
                .values
                .first()
                .is_some_and(|value| value == "Copper")
        {
            if attribute.values.iter().any(|value| value == "Top") {
                attribute.values = vec!["Copper".into(), "L2".into(), "Bot".into()];
            } else if attribute.values.iter().any(|value| value == "Bot") {
                attribute.values = vec!["Copper".into(), "L1".into(), "Top".into()];
            }
        }
        attribute.id = fixture_stable_id(
            "x2-attribute",
            &(
                &attribute.document_id,
                attribute.scope,
                attribute.kind,
                &attribute.values,
                attribute.deletion,
                &attribute.target_ids,
                &attribute.provenance.location,
            ),
        );
    }
    for id in forged
        .assembly
        .mask_layer_ids
        .iter_mut()
        .chain(forged.assembly.paste_layer_ids.iter_mut())
    {
        replace(id);
    }
    for construction in &mut forged.construction.layers {
        if let Some(id) = &mut construction.layer_id {
            replace(id);
        }
    }
    for reconciliation in &mut forged.reconciliations {
        for id in reconciliation
            .native
            .model_ids
            .iter_mut()
            .chain(reconciliation.package.model_ids.iter_mut())
        {
            replace(id);
        }
        refresh_reconciliation_id(reconciliation);
    }
    forged.refresh_physical_bounds().unwrap();
    forged.refresh_digests().unwrap();
    assert_eq!(forged.job_file_functions, retained_job);
    assert!(matches!(
        forged.validate(),
        Err(FabricationError::InvalidIdentity(ref id))
            if id == "authoritative-capability:LayerRoles"
    ));
}

#[test]
fn native_layer_identity_rejects_duplicate_inner_names() {
    let mut native =
        parse_native_kicad_manufacturing("four-layer.kicad_pcb", &four_layer_native_kicad())
            .unwrap();
    let layer = native
        .review
        .layers
        .iter_mut()
        .find(|layer| layer.name.as_deref() == Some("In2.Cu"))
        .unwrap();
    layer.name = Some("In1.Cu".into());
    layer.id = layer_id(
        &layer.document_id,
        layer.name.as_deref(),
        layer.role,
        layer.side,
        layer.order,
        layer.authority,
        &layer.provenance.location,
    );
    native.review.refresh_digests().unwrap();
    assert!(native.review.validate().is_err());

    let mut forged = reconcile_native_package(
        complete_four_layer_package_review(),
        parse_native_kicad_manufacturing("four-layer.kicad_pcb", &four_layer_native_kicad())
            .unwrap(),
    )
    .unwrap();
    let native_id = forged
        .source_pair
        .as_ref()
        .unwrap()
        .native_document_id
        .clone();
    let outer = forged
        .layers
        .iter_mut()
        .find(|layer| layer.document_id == native_id && layer.name.as_deref() == Some("In2.Cu"))
        .unwrap();
    let old_id = outer.id.clone();
    outer.name = Some("In1.Cu".into());
    outer.id = layer_id(
        &outer.document_id,
        outer.name.as_deref(),
        outer.role,
        outer.side,
        outer.order,
        outer.authority,
        &outer.provenance.location,
    );
    let new_id = outer.id.clone();
    let source = forged.native_reconciliation_source.as_mut().unwrap();
    let nested = source
        .review
        .layers
        .iter_mut()
        .find(|layer| layer.name.as_deref() == Some("In2.Cu"))
        .unwrap();
    nested.name = Some("In1.Cu".into());
    nested.id = new_id.clone();
    source.review.refresh_digests().unwrap();
    for reconciliation in &mut forged.reconciliations {
        for id in reconciliation
            .native
            .model_ids
            .iter_mut()
            .chain(reconciliation.package.model_ids.iter_mut())
        {
            if *id == old_id {
                *id = new_id.clone();
            }
        }
        refresh_reconciliation_id(reconciliation);
    }
    forged.refresh_digests().unwrap();
    assert!(forged.validate().is_err());
}

#[test]
fn integrated_complete_package_without_native_and_native_failure_are_valid_partial() {
    let root = temp_dir("package-without-native");
    write_complete_package_directory(&root);
    let report = review(&root, review_options(ReviewScope::Fabrication)).unwrap();
    assert_eq!(report.fabrication.status, FabricationStatus::Partial);
    assert_eq!(
        package_capability(&report.fabrication, CapabilityId::PackageCompleteness),
        CapabilityState::Complete
    );
    assert_eq!(
        package_capability(&report.fabrication, CapabilityId::PackageReconciliation),
        CapabilityState::NotProvided
    );
    validate_report(&report).unwrap();
    let mut forged = report.clone();
    forged.fabrication.status = FabricationStatus::Complete;
    forged.fabrication.refresh_digests().unwrap();
    assert!(validate_report(&forged).is_err());
    fs::remove_dir_all(&root).unwrap();

    let root = temp_dir("native-parse-failure");
    write_complete_package_directory(&root);
    fs::write(
        root.join("broken.kicad_pcb"),
        r#"(kicad_pcb (version 20240108) (generator fixture)
  (layers (0 "F.Cu" signal) (1 "In1.Cu" signal) (2 "In1.Cu" signal) (31 "B.Cu" signal) (44 "Edge.Cuts" user "Edge.Cuts"))
  (gr_rect (start 0 0) (end 10 10) (layer "Edge.Cuts")))"#,
    )
    .unwrap();
    let report = review(&root, review_options(ReviewScope::Fabrication)).unwrap();
    assert_eq!(report.fabrication.status, FabricationStatus::Partial);
    assert_eq!(
        package_capability(&report.fabrication, CapabilityId::PackageReconciliation),
        CapabilityState::Failed
    );
    validate_report(&report).unwrap();
    fs::remove_dir_all(&root).unwrap();

    let mut reconciliation_failure = complete_package_review();
    reconciliation_failure.status = FabricationStatus::Partial;
    reconciliation_failure.integration_outcome = Some(
        IntegratedReconciliationOutcome::new(
            IntegratedReconciliationState::Failed,
            Some("board.kicad_pcb".into()),
            Some("a".repeat(64)),
            "native-package-reconciliation-failed",
        )
        .unwrap(),
    );
    for id in [
        CapabilityId::NativeKicadFacts,
        CapabilityId::PackageReconciliation,
    ] {
        reconciliation_failure
            .capabilities
            .records
            .retain(|record| record.id != id);
        reconciliation_failure
            .capabilities
            .records
            .push(CapabilityRecord {
                id,
                state: CapabilityState::Failed,
                authority: Authority::NativeSource,
                document_ids: vec![],
                provenance: vec![],
                detail: "Typed integrated reconciliation failure.".into(),
            });
    }
    reconciliation_failure
        .capabilities
        .records
        .sort_by_key(|record| record.id);
    reconciliation_failure.refresh_digests().unwrap();
    reconciliation_failure.validate().unwrap();
    reconciliation_failure.status = FabricationStatus::Complete;
    reconciliation_failure.refresh_digests().unwrap();
    assert!(reconciliation_failure.validate().is_err());
}

#[test]
fn package_reconciliation_native_parser_is_bounded_explicit_and_quote_safe() {
    let bytes = matching_native_kicad();
    let native = parse_native_kicad_manufacturing("board.kicad_pcb", &bytes).unwrap();
    native.review.validate().unwrap();
    assert_eq!(native.review.documents.len(), 1);
    assert_eq!(
        native.review.documents[0].artifact_digest,
        format!("{:x}", Sha256::digest(&bytes))
    );
    assert_eq!(
        native.review.product.as_ref().unwrap().name.as_deref(),
        Some("phase5-board")
    );
    assert_eq!(
        native.review.product.as_ref().unwrap().revision.as_deref(),
        Some("r1")
    );
    assert_eq!(
        package_capability(&native.review, CapabilityId::ProductIdentity),
        CapabilityState::Complete
    );
    assert_eq!(
        package_capability(&native.review, CapabilityId::Profile),
        CapabilityState::Complete
    );
    assert_eq!(
        package_capability(&native.review, CapabilityId::Drills),
        CapabilityState::Complete
    );
    assert_eq!(
        package_capability(&native.review, CapabilityId::Slots),
        CapabilityState::Complete
    );
    assert_eq!(
        package_capability(&native.review, CapabilityId::Connectivity),
        CapabilityState::Complete
    );
    assert!(
        native
            .review
            .capabilities
            .records
            .iter()
            .all(|record| record.authority != Authority::FilenameInference)
    );
    assert!(native.review.features.iter().all(|feature| {
        feature.provenance.location.byte_end < bytes.len() as u64
            && feature.provenance.artifact_digest == native.review.documents[0].artifact_digest
    }));

    let quoted_fake = String::from_utf8(bytes.clone()).unwrap().replace(
        "(property \"Reference\" \"U1\")",
        "(property \"Reference\" \"U1 (via (at 9 9) (drill 9))\")",
    );
    let quoted =
        parse_native_kicad_manufacturing("board.kicad_pcb", quoted_fake.as_bytes()).unwrap();
    assert_eq!(
        quoted
            .review
            .features
            .iter()
            .filter(|feature| matches!(feature.geometry, Geometry::Drill(_)))
            .count(),
        1
    );

    let no_title = String::from_utf8(bytes.clone())
        .unwrap()
        .replace("(title_block (title \"phase5-board\") (rev \"r1\"))", "");
    let no_title = parse_native_kicad_manufacturing(
        "filename-is-not-authority.kicad_pcb",
        no_title.as_bytes(),
    )
    .unwrap();
    assert!(no_title.review.product.is_none());
    assert_eq!(
        package_capability(&no_title.review, CapabilityId::ProductIdentity),
        CapabilityState::NotProvided
    );

    for hostile in [
        String::from_utf8(bytes.clone())
            .unwrap()
            .replace("(kicad_pcb", "(kicad_pcb ((((((((((((((((((((((((((((((((("),
        String::from_utf8(bytes.clone())
            .unwrap()
            .trim_end_matches(')')
            .to_owned(),
        String::from_utf8(bytes.clone())
            .unwrap()
            .replace("(at 1 1)", "(at 1 1) (at 2 2)"),
        String::from_utf8(bytes.clone())
            .unwrap()
            .replace("(net 1 \"GND\")", "(net 1 \"GND\") (net 1 \"OTHER\")"),
        String::from_utf8(bytes.clone())
            .unwrap()
            .replace("(layers \"*.Cu\" \"*.Mask\")", "(layers \"Unknown.Cu\")"),
        String::from_utf8(bytes.clone()).unwrap().replace(
            "(property \"Reference\" \"U1\")",
            "(property \"Reference\" \"U1\") (property \"Reference\" \"U2\")",
        ),
    ] {
        assert!(parse_native_kicad_manufacturing("hostile.kicad_pcb", hostile.as_bytes()).is_err());
    }
    let unsupported = String::from_utf8(bytes).unwrap().replace(
        "(gr_rect (start 0 0) (end 10 10) (layer \"Edge.Cuts\"))",
        "(gr_circle (center 5 5) (end 10 5) (layer \"Edge.Cuts\"))",
    );
    let unsupported =
        parse_native_kicad_manufacturing("unsupported.kicad_pcb", unsupported.as_bytes()).unwrap();
    assert_eq!(
        package_capability(&unsupported.review, CapabilityId::Profile),
        CapabilityState::Partial
    );
    assert!(!unsupported.review.omissions.is_empty());

    let rotated = String::from_utf8(matching_native_kicad())
        .unwrap()
        .replace("(at 0 0)", "(at 0 0 45)");
    let rotated =
        parse_native_kicad_manufacturing("rotated.kicad_pcb", rotated.as_bytes()).unwrap();
    assert_eq!(
        package_capability(&rotated.review, CapabilityId::Drills),
        CapabilityState::Partial
    );
    let rotated = reconcile_native_package(complete_package_review(), rotated).unwrap();
    assert_eq!(
        reconciliation(&rotated, ReconciliationFamily::Drills).status,
        ReconciliationStatus::NotChecked
    );
    assert!(matches!(
        parse_native_kicad_manufacturing_with_timeout(
            "deadline.kicad_pcb",
            &matching_native_kicad(),
            Duration::ZERO
        ),
        Err(NativeManufacturingError::Resource {
            resource: "native-deadline"
        })
    ));
    assert!(matches!(
        reconcile_native_package_with_timeout(
            complete_package_review(),
            parse_native_kicad_manufacturing("board.kicad_pcb", &matching_native_kicad()).unwrap(),
            Duration::ZERO,
        ),
        Err(FabricationError::LimitExceeded {
            resource: "reconciliation-deadline"
        })
    ));
}

#[test]
fn package_reconciliation_clean_control_is_symmetric_stable_and_complete_only() {
    let package = complete_package_review();
    let native =
        parse_native_kicad_manufacturing("board.kicad_pcb", &matching_native_kicad()).unwrap();
    let first = reconcile_native_package(package.clone(), native.clone()).unwrap();
    let second = reconcile_native_package(package, native).unwrap();
    first.validate().unwrap();
    assert_eq!(first, second);
    assert_eq!(first.status, FabricationStatus::Complete);
    let mut forged = first.clone();
    forged.reconciliations[0].native.canonical_value = "\"forged\"".into();
    forged.reconciliations[0].package.canonical_value = "\"forged\"".into();
    forged.refresh_digests().unwrap();
    assert!(forged.validate().is_err());
    assert_eq!(first.reconciliations.len(), 6);
    assert!(first.reconciliations.iter().all(|item| {
        item.status == ReconciliationStatus::Match
            && item.confidence != ReconciliationConfidence::Unavailable
            && item.native.authority == Authority::NativeSource
            && item.native.provenance.document_id != item.package.provenance.document_id
            && !item.native.model_ids.is_empty()
            && !item.package.model_ids.is_empty()
    }));
    assert_eq!(
        package_capability(&first, CapabilityId::PackageReconciliation),
        CapabilityState::Complete
    );
    assert_eq!(
        dispatch_analyzer(
            PACKAGE_GERBERS_ANALYZER,
            &first.capabilities,
            Some(SemanticAnalyzerResult::Pass)
        )
        .status,
        AnalyzerDispatchStatus::Pass
    );
    let pair = first.source_pair.as_ref().unwrap();
    assert!(pair.id.starts_with("source-pair-v1-"));
    assert_eq!(
        pair.native_artifact_digest,
        first
            .documents
            .iter()
            .find(|document| document.format == DocumentFormat::KicadPcb)
            .unwrap()
            .artifact_digest
    );
}

#[test]
fn package_reconciliation_rejects_coherent_all_family_capability_forgery() {
    let baseline_package = complete_package_review();
    let baseline_native =
        parse_native_kicad_manufacturing("board.kicad_pcb", &matching_native_kicad()).unwrap();
    let mut package = baseline_package.clone();
    let mut native = baseline_native.clone();
    let prerequisites = [
        CapabilityId::ProductIdentity,
        CapabilityId::LayerRoles,
        CapabilityId::LayerOrder,
        CapabilityId::Profile,
        CapabilityId::Drills,
        CapabilityId::Tools,
        CapabilityId::Plating,
        CapabilityId::LayerSpans,
        CapabilityId::Extents,
        CapabilityId::Connectivity,
        CapabilityId::Components,
        CapabilityId::Pins,
    ];

    for review in [&mut package, &mut native.review] {
        let product = review.product.as_mut().unwrap();
        product.name = None;
        product.revision = None;
        product.part_number = None;
        for layer in review
            .layers
            .iter_mut()
            .filter(|layer| layer.role == LayerRole::Copper)
        {
            layer.order = None;
        }
        let profile = review.profile.as_mut().unwrap();
        profile.extents = None;
        for feature_id in &profile.contour_feature_ids {
            let feature = review
                .features
                .iter_mut()
                .find(|feature| feature.id == *feature_id)
                .unwrap();
            match &mut feature.geometry {
                Geometry::Contour(contour) => contour.closed = false,
                Geometry::Region(region) => {
                    for contour in &mut region.contours {
                        contour.closed = false;
                    }
                }
                _ => panic!("profile must retain contour geometry"),
            }
        }
        let removed_drills = review
            .features
            .iter()
            .filter(|feature| matches!(feature.geometry, Geometry::Drill(_)))
            .map(|feature| feature.id.clone())
            .collect::<BTreeSet<_>>();
        review
            .features
            .retain(|feature| !removed_drills.contains(&feature.id));
        review
            .connectivity
            .retain(|item| !removed_drills.contains(&item.feature_id));
        for tool in &mut review.tools {
            if tool.kind != ToolKind::Aperture {
                tool.diameter = None;
                tool.plating = Plating::Unknown;
                tool.span = None;
            }
        }
        for item in &mut review.connectivity {
            item.net = None;
            item.component = None;
            item.pin = None;
        }
        review.omissions.clear();
        review.conflicts.clear();
        for id in prerequisites {
            review
                .capabilities
                .records
                .iter_mut()
                .find(|record| record.id == id)
                .unwrap()
                .state = CapabilityState::Complete;
        }
        review.refresh_digests().unwrap();
        assert!(review.validate().is_err());
    }
    native.extents = None;

    assert!(reconcile_native_package(package, native).is_err());

    let mut forged = reconcile_native_package(baseline_package, baseline_native).unwrap();
    let native_document_id = forged
        .source_pair
        .as_ref()
        .unwrap()
        .native_document_id
        .clone();
    let package_document_id = forged
        .documents
        .iter()
        .find(|document| document.format != DocumentFormat::KicadPcb)
        .unwrap()
        .id
        .clone();
    let clear_profile_evidence = |review: &mut FabricationReview| {
        review.omissions.retain(|omission| {
            !omission
                .affected_capabilities
                .iter()
                .any(|id| matches!(id, CapabilityId::Profile | CapabilityId::Extents))
        });
        review.conflicts.retain(|conflict| {
            !conflict
                .affected_capabilities
                .iter()
                .any(|id| matches!(id, CapabilityId::Profile | CapabilityId::Extents))
        });
        for id in [CapabilityId::Profile, CapabilityId::Extents] {
            review
                .capabilities
                .records
                .iter_mut()
                .find(|record| record.id == id)
                .unwrap()
                .state = CapabilityState::Complete;
        }
    };
    forged.profile = None;
    clear_profile_evidence(&mut forged);
    {
        let source = forged.native_reconciliation_source.as_mut().unwrap();
        source.review.profile = None;
        source.extents = None;
        clear_profile_evidence(source.review.as_mut());
        source.review.refresh_digests().unwrap();
    }
    for family in [ReconciliationFamily::Profile, ReconciliationFamily::Extents] {
        let reconciliation = forged
            .reconciliations
            .iter_mut()
            .find(|item| item.family == family)
            .unwrap();
        reconciliation.native.model_ids = vec![native_document_id.clone()];
        reconciliation.package.model_ids = vec![package_document_id.clone()];
        reconciliation.native.canonical_value = "null".into();
        reconciliation.package.canonical_value = "null".into();
        reconciliation.status = ReconciliationStatus::Match;
        reconciliation.confidence = ReconciliationConfidence::ResolutionBounded;
        refresh_reconciliation_id(reconciliation);
    }
    forged.status = FabricationStatus::Complete;
    for id in [
        CapabilityId::PackageCompleteness,
        CapabilityId::PackageReconciliation,
    ] {
        forged
            .capabilities
            .records
            .iter_mut()
            .find(|record| record.id == id)
            .unwrap()
            .state = CapabilityState::Complete;
    }
    forged.refresh_digests().unwrap();
    assert!(forged.validate().is_err());
}

#[test]
fn package_reconciliation_rejects_refreshed_bottom_to_inner_side_name_order_forgery() {
    let mut forged = reconcile_native_package(
        complete_package_review(),
        parse_native_kicad_manufacturing("board.kicad_pcb", &matching_native_kicad()).unwrap(),
    )
    .unwrap();
    let native_document_id = forged
        .source_pair
        .as_ref()
        .unwrap()
        .native_document_id
        .clone();
    let package_bottom_document_id = forged
        .layers
        .iter()
        .find(|layer| {
            layer.document_id != native_document_id
                && layer.role == LayerRole::Copper
                && layer.side == LayerSide::Bottom
        })
        .unwrap()
        .document_id
        .clone();

    let mutate_bottom = |layer: &mut ManufacturingLayer| {
        layer.name = Some("In1.Cu".into());
        layer.side = LayerSide::Inner;
        layer.order = Some(2);
    };
    mutate_bottom(
        forged
            .layers
            .iter_mut()
            .find(|layer| {
                layer.document_id == package_bottom_document_id && layer.role == LayerRole::Copper
            })
            .unwrap(),
    );
    let retained_function = forged
        .x2_attributes
        .iter_mut()
        .find(|attribute| {
            attribute.document_id == package_bottom_document_id
                && attribute.kind == X2AttributeKind::FileFunction
        })
        .unwrap();
    retained_function.values = vec!["Copper".into(), "L2".into(), "Inr".into()];

    mutate_bottom(
        forged
            .layers
            .iter_mut()
            .find(|layer| {
                layer.document_id == native_document_id && layer.name.as_deref() == Some("B.Cu")
            })
            .unwrap(),
    );
    {
        let source = forged.native_reconciliation_source.as_mut().unwrap();
        mutate_bottom(
            source
                .review
                .layers
                .iter_mut()
                .find(|layer| layer.name.as_deref() == Some("B.Cu"))
                .unwrap(),
        );
        for id in [
            CapabilityId::ProductIdentity,
            CapabilityId::LayerRoles,
            CapabilityId::LayerOrder,
            CapabilityId::Profile,
            CapabilityId::Drills,
            CapabilityId::Tools,
            CapabilityId::Plating,
            CapabilityId::LayerSpans,
            CapabilityId::Extents,
            CapabilityId::Connectivity,
            CapabilityId::Components,
            CapabilityId::Pins,
        ] {
            source
                .review
                .capabilities
                .records
                .iter_mut()
                .find(|record| record.id == id)
                .unwrap()
                .state = CapabilityState::Complete;
        }
        source.review.refresh_digests().unwrap();
    }
    for id in [
        CapabilityId::ProductIdentity,
        CapabilityId::LayerRoles,
        CapabilityId::LayerOrder,
        CapabilityId::Profile,
        CapabilityId::Drills,
        CapabilityId::Tools,
        CapabilityId::Plating,
        CapabilityId::LayerSpans,
        CapabilityId::Extents,
        CapabilityId::Connectivity,
        CapabilityId::Components,
        CapabilityId::Pins,
        CapabilityId::PackageCompleteness,
        CapabilityId::PackageReconciliation,
    ] {
        forged
            .capabilities
            .records
            .iter_mut()
            .find(|record| record.id == id)
            .unwrap()
            .state = CapabilityState::Complete;
    }
    forged.status = FabricationStatus::Complete;

    let canonical_layers = |review: &FabricationReview, native: bool| {
        let document_ids = review
            .documents
            .iter()
            .filter(|document| {
                if native {
                    document.format == DocumentFormat::KicadPcb
                } else {
                    document.format == DocumentFormat::Gerber
                }
            })
            .map(|document| document.id.as_str())
            .collect::<BTreeSet<_>>();
        let mut values = review
            .layers
            .iter()
            .filter(|layer| {
                document_ids.contains(layer.document_id.as_str())
                    && matches!(layer.role, LayerRole::Copper | LayerRole::Profile)
            })
            .map(|layer| (layer.role, layer.side, layer.order))
            .collect::<Vec<_>>();
        values.sort();
        serde_json::to_string(&values).unwrap()
    };
    let native_value = canonical_layers(
        forged
            .native_reconciliation_source
            .as_ref()
            .unwrap()
            .review
            .as_ref(),
        true,
    );
    let package_value = canonical_layers(&forged, false);
    let layers = forged
        .reconciliations
        .iter_mut()
        .find(|item| item.family == ReconciliationFamily::Layers)
        .unwrap();
    layers.native.canonical_value = native_value;
    layers.package.canonical_value = package_value;
    layers.status = ReconciliationStatus::Match;
    for reconciliation in &mut forged.reconciliations {
        reconciliation.status = ReconciliationStatus::Match;
        refresh_reconciliation_id(reconciliation);
    }
    forged.refresh_digests().unwrap();

    assert_eq!(forged.reconciliations.len(), 6);
    assert!(
        forged
            .reconciliations
            .iter()
            .all(|item| item.status == ReconciliationStatus::Match)
    );
    assert_eq!(
        dispatch_analyzer(
            PACKAGE_GERBERS_ANALYZER,
            &forged.capabilities,
            Some(SemanticAnalyzerResult::Pass),
        )
        .status,
        AnalyzerDispatchStatus::Pass
    );
    assert!(
        forged.validate().is_err(),
        "constructor-equivalent validation must reject a coherently refreshed bottom-to-inner forgery"
    );
}

fn refresh_reconciliation_id(item: &mut ManufacturingReconciliation) {
    item.id = fixture_stable_id(
        "reconciliation",
        &(
            item.family,
            &item.native.model_ids,
            &item.native.canonical_value,
            item.native.resolution,
            item.native.authority,
            &item.native.provenance.document_id,
            &item.native.provenance.artifact_digest,
            &item.native.provenance.location,
            &item.package.model_ids,
            &item.package.canonical_value,
            item.package.resolution,
            item.package.authority,
            &item.package.provenance.document_id,
            &item.package.provenance.artifact_digest,
            &item.package.provenance.location,
        ),
    );
}

#[test]
fn package_reconciliation_digest_location_and_model_id_tampering_fails_closed() {
    let reconciled = reconcile_native_package(
        complete_package_review(),
        parse_native_kicad_manufacturing("board.kicad_pcb", &matching_native_kicad()).unwrap(),
    )
    .unwrap();

    let mut forged = reconciled.clone();
    let other_digest = forged
        .documents
        .iter()
        .find(|document| document.format == DocumentFormat::Gerber)
        .unwrap()
        .artifact_digest
        .clone();
    forged.reconciliations[0].native.provenance.artifact_digest = other_digest;
    refresh_reconciliation_id(&mut forged.reconciliations[0]);
    forged.refresh_digests().unwrap();
    assert!(forged.validate().is_err());

    let mut forged = reconciled.clone();
    let native_id = forged
        .source_pair
        .as_ref()
        .unwrap()
        .native_document_id
        .clone();
    let alternate_location = forged
        .layers
        .iter()
        .find(|layer| layer.document_id == native_id)
        .unwrap()
        .provenance
        .location
        .clone();
    forged.reconciliations[0].native.provenance.location = alternate_location;
    refresh_reconciliation_id(&mut forged.reconciliations[0]);
    forged.refresh_digests().unwrap();
    assert!(matches!(
        forged.validate(),
        Err(FabricationError::InvalidConflict(ref id))
            if id == "reconciliation-derived-facts"
    ));

    let mut forged = reconciled;
    let sibling_package_id = forged
        .layers
        .iter()
        .find(|layer| layer.document_id != native_id)
        .unwrap()
        .id
        .clone();
    forged.reconciliations[0].package.model_ids[0] = sibling_package_id;
    refresh_reconciliation_id(&mut forged.reconciliations[0]);
    forged.refresh_digests().unwrap();
    assert!(matches!(
        forged.validate(),
        Err(FabricationError::InvalidConflict(ref id))
            if id == "reconciliation-derived-facts"
    ));
}

#[test]
fn package_reconciliation_rederives_retained_layer_semantics() {
    let package = complete_package_review();
    let native =
        parse_native_kicad_manufacturing("board.kicad_pcb", &matching_native_kicad()).unwrap();
    let reconciled = reconcile_native_package(package, native).unwrap();
    for native_side in [true, false] {
        let mut forged = reconciled.clone();
        let native_document_id = forged
            .source_pair
            .as_ref()
            .unwrap()
            .native_document_id
            .clone();
        let layer = forged
            .layers
            .iter_mut()
            .find(|layer| {
                (layer.document_id == native_document_id) == native_side
                    && layer.role == LayerRole::Copper
            })
            .unwrap();
        layer.side = if layer.side == LayerSide::Top {
            LayerSide::Bottom
        } else {
            LayerSide::Top
        };
        forged.refresh_digests().unwrap();
        assert!(forged.validate().is_err(), "native_side={native_side}");
    }
}

#[test]
fn package_reconciliation_rederives_all_families_and_both_prerequisite_ledgers() {
    let package = complete_package_review();
    let native =
        parse_native_kicad_manufacturing("board.kicad_pcb", &matching_native_kicad()).unwrap();
    let reconciled = reconcile_native_package(package, native).unwrap();
    let families = [
        (ReconciliationFamily::Product, CapabilityId::ProductIdentity),
        (ReconciliationFamily::Layers, CapabilityId::LayerOrder),
        (ReconciliationFamily::Profile, CapabilityId::Profile),
        (ReconciliationFamily::Drills, CapabilityId::Drills),
        (ReconciliationFamily::Extents, CapabilityId::Extents),
        (
            ReconciliationFamily::Connectivity,
            CapabilityId::Connectivity,
        ),
    ];
    for (family, prerequisite) in families {
        for native in [true, false] {
            let mut forged = reconciled.clone();
            mutate_reconciled_source(&mut forged, family, native);
            assert!(
                forged.validate().is_err(),
                "stale source family={family:?} native={native}"
            );

            let mut forged = reconciled.clone();
            let ledger = if native {
                &mut forged
                    .native_reconciliation_source
                    .as_mut()
                    .unwrap()
                    .review
                    .capabilities
            } else {
                &mut forged.capabilities
            };
            ledger
                .records
                .iter_mut()
                .find(|record| record.id == prerequisite)
                .unwrap()
                .state = CapabilityState::Partial;
            if native {
                forged
                    .native_reconciliation_source
                    .as_mut()
                    .unwrap()
                    .review
                    .refresh_digests()
                    .unwrap();
            }
            forged.refresh_digests().unwrap();
            assert!(
                forged.validate().is_err(),
                "stale prerequisite family={family:?} native={native}"
            );
        }
    }
}

fn set_reconciliation_prerequisite(
    review: &mut FabricationReview,
    id: CapabilityId,
    state: Option<CapabilityState>,
) {
    let provenance = review
        .capabilities
        .records
        .iter()
        .find(|record| record.id == id)
        .and_then(|record| record.provenance.first())
        .cloned()
        .unwrap();
    match state {
        Some(state) => {
            review
                .capabilities
                .records
                .iter_mut()
                .find(|record| record.id == id)
                .unwrap()
                .state = state;
            if state == CapabilityState::Omitted {
                review.omissions.push(Omission {
                    id: fixture_stable_id(
                        "omission",
                        &("reconciliation-prerequisite", id, &provenance.location),
                    ),
                    kind: OmissionKind::ResourceLimit,
                    affected_capabilities: vec![id],
                    provenance,
                    detail: "Fixture prerequisite omission.".into(),
                });
            }
        }
        None => review.capabilities.records.retain(|record| record.id != id),
    }
    review.refresh_digests().unwrap();
}

#[test]
fn package_reconciliation_all_families_authorities_and_conservative_states_are_not_checked() {
    let package = complete_package_review();
    let native =
        parse_native_kicad_manufacturing("board.kicad_pcb", &matching_native_kicad()).unwrap();
    for (family, prerequisite) in [
        (ReconciliationFamily::Product, CapabilityId::ProductIdentity),
        (ReconciliationFamily::Layers, CapabilityId::LayerOrder),
        (ReconciliationFamily::Profile, CapabilityId::Profile),
        (ReconciliationFamily::Drills, CapabilityId::Drills),
        (ReconciliationFamily::Extents, CapabilityId::Extents),
        (
            ReconciliationFamily::Connectivity,
            CapabilityId::Connectivity,
        ),
    ] {
        for mutate_native in [false, true] {
            for state in [
                None,
                Some(CapabilityState::Partial),
                Some(CapabilityState::Stale),
                Some(CapabilityState::Unsupported),
                Some(CapabilityState::Failed),
                Some(CapabilityState::NotProvided),
                Some(CapabilityState::Omitted),
            ] {
                let mut package = package.clone();
                let mut native = native.clone();
                if mutate_native {
                    set_reconciliation_prerequisite(&mut native.review, prerequisite, state);
                } else {
                    set_reconciliation_prerequisite(&mut package, prerequisite, state);
                }
                assert!(
                    reconcile_native_package(package, native).is_err(),
                    "constructor-derived family={family:?} native={mutate_native} state={state:?} must reject a supplied ledger mutation"
                );
            }
        }
    }
}

#[test]
fn package_reconciliation_missing_partial_stale_unsupported_and_conflicts_never_improve_approval() {
    let package = complete_package_review();
    let native =
        parse_native_kicad_manufacturing("board.kicad_pcb", &matching_native_kicad()).unwrap();
    for state in [
        None,
        Some(CapabilityState::Partial),
        Some(CapabilityState::Stale),
        Some(CapabilityState::Unsupported),
        Some(CapabilityState::Failed),
    ] {
        let mut mutated = native.clone();
        match state {
            Some(state) => {
                mutated
                    .review
                    .capabilities
                    .records
                    .iter_mut()
                    .find(|record| record.id == CapabilityId::LayerOrder)
                    .unwrap()
                    .state = state;
            }
            None => mutated
                .review
                .capabilities
                .records
                .retain(|record| record.id != CapabilityId::LayerOrder),
        }
        mutated.review.refresh_digests().unwrap();
        assert!(mutated.review.validate().is_err());
        assert!(reconcile_native_package(package.clone(), mutated).is_err());
    }

    for (needle, replacement, family) in [
        (
            "phase5-board",
            "different-board",
            ReconciliationFamily::Product,
        ),
        (
            "(31 \"B.Cu\" signal)",
            "(2 \"In1.Cu\" signal) (31 \"B.Cu\" signal)",
            ReconciliationFamily::Layers,
        ),
        ("(end 10 10)", "(end 11 10)", ReconciliationFamily::Profile),
        ("(at 1 1)", "(at 2 1)", ReconciliationFamily::Drills),
        ("\"GND\"", "\"VCC\"", ReconciliationFamily::Connectivity),
    ] {
        let source = String::from_utf8(matching_native_kicad())
            .unwrap()
            .replace(needle, replacement);
        let native =
            parse_native_kicad_manufacturing("board.kicad_pcb", source.as_bytes()).unwrap();
        let result = reconcile_native_package(package.clone(), native).unwrap();
        assert_eq!(
            reconciliation(&result, family).status,
            ReconciliationStatus::Mismatch,
            "{family:?}"
        );
        assert_ne!(
            package_capability(&result, CapabilityId::PackageReconciliation),
            CapabilityState::Complete
        );
        assert_ne!(result.status, FabricationStatus::Complete);
        result.validate().unwrap();
    }
}

#[test]
fn xnc_strict_tool_boundary_accepts_t99_and_rejects_t100() {
    let strict = fs::read_to_string(xnc_fixture("strict.xnc")).unwrap();
    let exact = strict
        .replace("T01C0.600", "T99C0.600")
        .replace("\nT01\n", "\nT99\n");
    let inventory = retained_inventory(
        "fab/tool-99.xnc",
        ManufacturingKindCandidate::Excellon,
        exact.as_bytes(),
    );
    let parsed = parse_xnc_document(&inventory.inputs[0]).unwrap();
    assert!(parsed.review.tools.iter().any(|tool| tool.code == "T99"));
    let mut selected_tool_ids = Vec::new();
    for feature in &parsed.review.features {
        if let Some(tool_id) = feature.tool_id.as_ref()
            && !selected_tool_ids.contains(tool_id)
        {
            selected_tool_ids.push(tool_id.clone());
        }
    }
    assert!(selected_tool_ids.iter().any(|tool_id| {
        parsed
            .review
            .tools
            .iter()
            .any(|tool| tool.id == *tool_id && tool.code == "T99")
    }));

    let over = strict
        .replace("T01C0.600", "T100C0.600")
        .replace("\nT01\n", "\nT100\n");
    let inventory = retained_inventory(
        "fab/tool-100.xnc",
        ManufacturingKindCandidate::Excellon,
        over.as_bytes(),
    );
    assert!(parse_xnc_document(&inventory.inputs[0]).is_err());
}

#[test]
fn fabrication_hostile_phase5_xnc_job_native_and_archive_matrix_fails_closed() {
    let strict = fs::read(xnc_fixture("strict.xnc")).unwrap();
    let strict_text = String::from_utf8(strict.clone()).unwrap();
    let mut hostile_xnc = vec![
        vec![0xff, b'\n'],
        b"M48\nMETRIC\nT01C0.6\n%\n\0M30\n".to_vec(),
        strict_text.replace("T01\nG05", "T02\nG05").into_bytes(),
        strict_text.replace("M16", "M30").into_bytes(),
        strict_text.replace("M30", "M16\nM30").into_bytes(),
        strict_text.replace("T01C0.600", "T100C0.600").into_bytes(),
        strict_text
            .replace("G03X4.000Y3.000A1.000", "G03X4.000Y3.000")
            .into_bytes(),
    ];
    hostile_xnc.push(
        format!(
            "{}M30\n",
            ";x\n".repeat(MANUFACTURING_LIMITS.records_per_file as usize + 1)
        )
        .into_bytes(),
    );
    let mut too_many_drills = String::from(
        "; #@! TF.GenerationSoftware,Ucamco,UcamX,2021.11\nM48\n; #@! TF.FileFunction,Plated,1,2,PTH\nMETRIC\nT01C0.600\n%\nT01\nG05\n",
    );
    too_many_drills
        .push_str(&"X1.000Y1.000\n".repeat(MANUFACTURING_LIMITS.drill_route_features + 1));
    too_many_drills.push_str("M30\n");
    hostile_xnc.push(too_many_drills.into_bytes());
    for bytes in hostile_xnc {
        let inventory = retained_inventory(
            "fab/hostile.xnc",
            ManufacturingKindCandidate::Excellon,
            &bytes,
        );
        assert!(parse_xnc_document(&inventory.inputs[0]).is_err());
    }

    let job = fs::read(job_fixture("complete.gbrjob")).unwrap();
    let xnc = fs::read(xnc_fixture("strict.xnc")).unwrap();
    let inventory = complete_package(job.clone(), xnc);
    let mutate_job = |bytes: Vec<u8>| {
        let mut inventory = inventory.clone();
        let input = inventory
            .inputs
            .iter_mut()
            .find(|input| input.kind_candidate == ManufacturingKindCandidate::GerberJob)
            .unwrap();
        input.original_bytes = bytes;
        input.size = input.original_bytes.len() as u64;
        input.artifact_digest = format!("{:x}", Sha256::digest(&input.original_bytes));
        let input = input.clone();
        let outcome = inventory
            .outcomes
            .iter_mut()
            .find(|outcome| outcome.kind_candidate == ManufacturingKindCandidate::GerberJob)
            .unwrap();
        outcome.size = input.size;
        outcome.artifact_digest = Some(input.artifact_digest.clone());
        outcome.id = input_outcome_id(
            &outcome.virtual_path,
            outcome.artifact_digest.as_deref(),
            outcome.kind_candidate,
        );
        (inventory, input)
    };
    let mut nested = "0".to_owned();
    for _ in 0..=MANUFACTURING_LIMITS.max_nesting {
        nested = format!("{{\"n\":{nested}}}");
    }
    let references = (0..=MANUFACTURING_LIMITS.recognized_files)
        .map(|_| "{\"Path\":\"top.gbr\",\"FileFunction\":\"Copper,L1,Top\"}")
        .collect::<Vec<_>>()
        .join(",");
    for bytes in [
        vec![0xff],
        b"{\"bad\":\"\0\"}".to_vec(),
        nested.into_bytes(),
        format!("{{\"FilesAttributes\":[{references}]}}").into_bytes(),
    ] {
        let (inventory, input) = mutate_job(bytes);
        assert!(parse_gerber_job_document(&input, &inventory).is_err());
    }

    let mut ambiguous = inventory.clone();
    let mut second = ambiguous
        .inputs
        .iter()
        .find(|input| input.kind_candidate == ManufacturingKindCandidate::GerberJob)
        .unwrap()
        .clone();
    second.virtual_path = "fab/second.gbrjob".into();
    let second_outcome = ManufacturingInputOutcome {
        id: input_outcome_id(
            &second.virtual_path,
            Some(&second.artifact_digest),
            second.kind_candidate,
        ),
        virtual_path: second.virtual_path.clone(),
        artifact_digest: Some(second.artifact_digest.clone()),
        kind_candidate: second.kind_candidate,
        size: second.size,
        state: ManufacturingLoadState::Retained,
        reason: None,
    };
    ambiguous.inputs.push(second);
    ambiguous.outcomes.push(second_outcome);
    assert!(analyze_manufacturing_inventory(&ambiguous).is_err());

    let mut exhausted = ManufacturingInventory::default();
    for index in 0..=MANUFACTURING_LIMITS.archive_entries {
        let path = format!("omitted/{index}.gbr");
        exhausted.outcomes.push(ManufacturingInputOutcome {
            id: input_outcome_id(&path, None, ManufacturingKindCandidate::Gerber),
            virtual_path: path,
            artifact_digest: None,
            kind_candidate: ManufacturingKindCandidate::Gerber,
            size: 1,
            state: ManufacturingLoadState::Omitted,
            reason: Some(ManufacturingLoadReason::RecognizedFileLimit),
        });
    }
    assert!(analyze_manufacturing_inventory(&exhausted).is_err());
}

#[test]
fn fabrication_phase5_sanitized_manifests_are_exact_and_redistribution_safe() {
    let root = repository_root().join("tests/fixtures/fabrication");
    for (directory, expected_count) in [("xnc", 4_usize), ("job", 2_usize)] {
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join(directory).join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["schemaVersion"], 1);
        assert_eq!(manifest["origin"], "project-authored");
        assert_eq!(manifest["license"], "MIT OR Apache-2.0");
        let fixtures = manifest["fixtures"].as_array().unwrap();
        assert_eq!(fixtures.len(), expected_count);
        let mut paths = BTreeSet::new();
        let mut digests = BTreeSet::new();
        for fixture in fixtures {
            let path = fixture["path"].as_str().unwrap();
            let expected = fixture["sha256"].as_str().unwrap();
            assert!(paths.insert(path));
            assert!(digests.insert(expected));
            assert_eq!(
                format!(
                    "{:x}",
                    Sha256::digest(fs::read(root.join(directory).join(path)).unwrap())
                ),
                expected
            );
            assert!(!path.contains(['/', '\\']));
        }
    }

    let package: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("package/manifest.json")).unwrap()).unwrap();
    let mutations: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("package/mutations.json")).unwrap()).unwrap();
    assert_eq!(package["origin"], "project-authored");
    assert_eq!(package["license"], "MIT OR Apache-2.0");
    assert_eq!(mutations["origin"], "project-authored");
    let ids = mutations["mutations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|mutation| {
            assert_eq!(mutation["expected"], "partial");
            mutation["id"].as_str().unwrap()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(ids.len(), mutations["mutations"].as_array().unwrap().len());
}

#[test]
fn fabrication_package_mutation_manifest_executes_every_case_table_first() {
    let root = repository_root().join("tests/fixtures/fabrication/package");
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("mutations.json")).unwrap()).unwrap();
    for mutation in manifest["mutations"].as_array().unwrap() {
        let id = mutation["id"].as_str().unwrap();
        let mut inventory = complete_package(
            fs::read(job_fixture("complete.gbrjob")).unwrap(),
            fs::read(xnc_fixture("strict.xnc")).unwrap(),
        );
        let remove_document = |inventory: &mut ManufacturingInventory, path: &str| {
            inventory.inputs.retain(|input| input.virtual_path != path);
            inventory
                .outcomes
                .retain(|outcome| outcome.virtual_path != path);
            let job_path = "fab/complete.gbrjob";
            let job = inventory
                .inputs
                .iter()
                .find(|input| input.virtual_path == job_path)
                .unwrap();
            let mut value: serde_json::Value = serde_json::from_slice(&job.original_bytes).unwrap();
            value["FilesAttributes"]
                .as_array_mut()
                .unwrap()
                .retain(|entry| {
                    entry["Path"].as_str() != Some(path.strip_prefix("fab/").unwrap_or(path))
                });
            replace_inventory_path(
                inventory,
                job_path,
                serde_json::to_vec_pretty(&value).unwrap(),
            );
        };
        match id {
            "missing-bottom" => remove_document(&mut inventory, "fab/bottom.gbr"),
            "missing-profile" => remove_document(&mut inventory, "fab/profile.gbr"),
            "missing-drill" => remove_document(&mut inventory, "fab/holes.xnc"),
            "unknown-plating" | "unknown-span" => {
                let source = fs::read_to_string(xnc_fixture("strict.xnc"))
                    .unwrap()
                    .replace("; #@! TF.FileFunction,Plated,1,2,PTH\n", "");
                replace_inventory_path(&mut inventory, "fab/holes.xnc", source.into_bytes());
            }
            "x2-job-role-conflict" => {
                let source = fs::read_to_string(job_fixture("complete.gbrjob"))
                    .unwrap()
                    .replace("Copper,L1,Top", "Copper,L2,Bot");
                replace_inventory_path(&mut inventory, "fab/complete.gbrjob", source.into_bytes());
            }
            "sparse-object-attributes" => {
                replace_inventory_path(
                    &mut inventory,
                    "fab/top.gbr",
                    x2_layer("Copper,L1,Top", false, true),
                );
            }
            "filename-only" => {
                inventory
                    .inputs
                    .retain(|input| input.kind_candidate != ManufacturingKindCandidate::GerberJob);
                inventory.outcomes.retain(|outcome| {
                    outcome.kind_candidate != ManufacturingKindCandidate::GerberJob
                });
                for (path, function, profile) in [
                    ("fab/top.gbr", "Copper,L1,Top", false),
                    ("fab/bottom.gbr", "Copper,L2,Bot", false),
                    ("fab/profile.gbr", "Profile,NP", true),
                ] {
                    let source = String::from_utf8(x2_layer(function, profile, false))
                        .unwrap()
                        .replace(&format!("%TF.FileFunction,{function}*%\n"), "")
                        .replace(
                            "RateMyPCB project-authored X2 package fixture",
                            &format!("RateMyPCB filename-only {path}"),
                        );
                    replace_inventory_path(&mut inventory, path, source.into_bytes());
                }
                let source = fs::read_to_string(xnc_fixture("strict.xnc"))
                    .unwrap()
                    .replace("; #@! TF.FileFunction,Plated,1,2,PTH\n", "");
                replace_inventory_path(&mut inventory, "fab/holes.xnc", source.into_bytes());
            }
            other => panic!("unimplemented package mutation {other}"),
        }
        let review = analyze_manufacturing_inventory(&inventory)
            .unwrap_or_else(|error| panic!("{id}: {error:?}"));
        assert_eq!(review.status, FabricationStatus::Partial, "{id}");
        assert_ne!(
            package_capability(&review, CapabilityId::PackageCompleteness),
            CapabilityState::Complete,
            "{id}"
        );
        review.validate().unwrap();
    }
}

#[test]
fn fabrication_fixed_limit_evidence_is_not_a_string_only_test_map() {
    assert!(!include_str!("fabrication_release.rs").contains(&["let mapped", " = ["].concat()));
}

#[test]
fn fabrication_inventory_path_depth_and_entry_limits_are_exact_and_one_over() {
    let outcome = |path: String| ManufacturingInputOutcome {
        id: input_outcome_id(&path, None, ManufacturingKindCandidate::Gerber),
        virtual_path: path,
        artifact_digest: None,
        kind_candidate: ManufacturingKindCandidate::Gerber,
        size: 1,
        state: ManufacturingLoadState::Omitted,
        reason: Some(ManufacturingLoadReason::RecognizedFileLimit),
    };

    let exact_path = "x".repeat(MANUFACTURING_LIMITS.normalized_path_bytes);
    ManufacturingInventory {
        inputs: vec![],
        outcomes: vec![outcome(exact_path)],
        aggregate_started: None,
    }
    .validate()
    .unwrap();
    let over_path = "x".repeat(MANUFACTURING_LIMITS.normalized_path_bytes + 1);
    assert!(
        ManufacturingInventory {
            inputs: vec![],
            outcomes: vec![outcome(over_path)],
            aggregate_started: None,
        }
        .validate()
        .is_err()
    );

    let exact_depth = vec!["d"; usize::from(MANUFACTURING_LIMITS.directory_depth) + 1].join("/");
    ManufacturingInventory {
        inputs: vec![],
        outcomes: vec![outcome(exact_depth)],
        aggregate_started: None,
    }
    .validate()
    .unwrap();
    let over_depth = vec!["d"; usize::from(MANUFACTURING_LIMITS.directory_depth) + 2].join("/");
    assert!(
        ManufacturingInventory {
            inputs: vec![],
            outcomes: vec![outcome(over_depth)],
            aggregate_started: None,
        }
        .validate()
        .is_err()
    );

    let mut entries = ManufacturingInventory {
        inputs: vec![],
        outcomes: (0..MANUFACTURING_LIMITS.archive_entries)
            .map(|index| outcome(format!("entry-{index}.gbr")))
            .collect(),
        aggregate_started: None,
    };
    entries.validate().unwrap();
    entries.outcomes.push(outcome("one-over-entry.gbr".into()));
    assert!(entries.validate().is_err());

    assert_eq!(
        serde_json::to_value(MANUFACTURING_LIMITS)
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
        32
    );
}

#[test]
fn fabrication_phase5_repeated_and_shuffled_package_output_is_deterministic() {
    let job = fs::read(job_fixture("complete.gbrjob")).unwrap();
    let xnc = fs::read(xnc_fixture("strict.xnc")).unwrap();
    let inventory = complete_package(job, xnc);
    let expected = analyze_manufacturing_inventory(&inventory).unwrap();
    let mut shuffled = inventory;
    shuffled.inputs.reverse();
    shuffled.outcomes.reverse();
    let actual = analyze_manufacturing_inventory(&shuffled).unwrap();
    assert_eq!(actual, expected);

    let native =
        parse_native_kicad_manufacturing("board.kicad_pcb", &matching_native_kicad()).unwrap();
    let first = reconcile_native_package(expected, native.clone()).unwrap();
    let second = reconcile_native_package(actual, native).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.model_digest, second.model_digest);
}
