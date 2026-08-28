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
    let layer_id = layer_id(&document_id, LayerRole::Copper, &layer_provenance.location);
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
fn model_report_integration_defaults_to_not_provided_and_revalidates_digest() {
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
    assert_eq!(report.fabrication.status, FabricationStatus::NotProvided);
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
        limitation.contains("filename and token screening remain partial evidence only")
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
    fs::write(
        root.join("board-F_Cu.gtl"),
        "%FSLAX46Y46*%\n%MOMM*%\nM02*\n",
    )
    .unwrap();
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
    let root = PathBuf::from(
        std::env::var("RATEMYPCB_UCAMCO_CORPUS")
            .expect("RATEMYPCB_UCAMCO_CORPUS must name the verified local corpus"),
    );
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
        for feature in &parsed.review.features {
            match feature.geometry {
                Geometry::Line(_) => lines += 1,
                Geometry::Arc(_) => arcs += 1,
                Geometry::Region(_) => regions += 1,
                Geometry::Flash(_) => flashes += 1,
                _ => {}
            }
        }
        parsed.review.validate().unwrap();
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
