use gerber_parser::gerber_types::{
    Command as ParserCommand, CommentContent as ParserCommentContent, DCode as ParserDCode,
    ExtendedCode as ParserExtendedCode, FunctionCode as ParserFunctionCode, GCode as ParserGCode,
    MCode as ParserMCode, Operation as ParserOperation,
};
use gerber_parser::{ContentError, parse as parse_gerber};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::{BufReader, Cursor, Read};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ManufacturingKindCandidate {
    Gerber,
    Excellon,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ManufacturingLoadState {
    Retained,
    Omitted,
    Failed,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ManufacturingLoadReason {
    RecognizedFileLimit,
    PerFileByteLimit,
    AggregateByteLimit,
    ReadFailure,
}

/// Original manufacturing bytes retained at the adapter boundary, never in report JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManufacturingInput {
    pub virtual_path: String,
    pub artifact_digest: String,
    pub kind_candidate: ManufacturingKindCandidate,
    pub size: u64,
    pub original_bytes: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct ManufacturingInputOutcome {
    pub id: String,
    pub virtual_path: String,
    pub artifact_digest: Option<String>,
    pub kind_candidate: ManufacturingKindCandidate,
    pub size: u64,
    pub state: ManufacturingLoadState,
    pub reason: Option<ManufacturingLoadReason>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ManufacturingInventory {
    pub inputs: Vec<ManufacturingInput>,
    pub outcomes: Vec<ManufacturingInputOutcome>,
}

impl ManufacturingInventory {
    pub fn validate(&self) -> Result<(), FabricationError> {
        let mut paths = BTreeSet::new();
        let mut ids = BTreeSet::new();
        let mut retained_outcomes = BTreeSet::new();
        let mut input_identities = BTreeSet::new();
        let mut retained_bytes = 0_u64;
        if self.outcomes.len() > MANUFACTURING_LIMITS.archive_entries {
            return Err(FabricationError::LimitExceeded {
                resource: "manufacturing-inventory",
            });
        }
        for outcome in &self.outcomes {
            if !paths.insert(outcome.virtual_path.as_str())
                || !ids.insert(outcome.id.as_str())
                || !valid_virtual_path(&outcome.virtual_path)
                || outcome.virtual_path.len() > MANUFACTURING_LIMITS.normalized_path_bytes
                || path_directory_depth(&outcome.virtual_path)
                    > usize::from(MANUFACTURING_LIMITS.directory_depth)
                || outcome
                    .artifact_digest
                    .as_deref()
                    .is_some_and(|digest| !lowercase_sha256(digest))
                || outcome.id
                    != input_outcome_id(
                        &outcome.virtual_path,
                        outcome.artifact_digest.as_deref(),
                        outcome.kind_candidate,
                    )
                || !valid_load_outcome_state(outcome)
            {
                return Err(FabricationError::InvalidIdentity(outcome.id.clone()));
            }
            if outcome.state == ManufacturingLoadState::Retained {
                retained_outcomes.insert((
                    outcome.virtual_path.as_str(),
                    outcome
                        .artifact_digest
                        .as_deref()
                        .expect("checked retained digest"),
                    outcome.kind_candidate,
                    outcome.size,
                ));
            }
        }
        for input in &self.inputs {
            let size = u64::try_from(input.original_bytes.len())
                .map_err(|_| FabricationError::ArithmeticOverflow)?;
            retained_bytes = retained_bytes
                .checked_add(size)
                .ok_or(FabricationError::ArithmeticOverflow)?;
            if size != input.size
                || size > MANUFACTURING_LIMITS.raw_bytes_per_file
                || sha256(&input.original_bytes) != input.artifact_digest
                || !input_identities.insert((
                    input.virtual_path.as_str(),
                    input.artifact_digest.as_str(),
                    input.kind_candidate,
                    input.size,
                ))
            {
                return Err(FabricationError::InvalidIdentity(
                    input.virtual_path.clone(),
                ));
            }
        }
        if retained_outcomes != input_identities
            || retained_outcomes.len() > MANUFACTURING_LIMITS.recognized_files
            || retained_bytes > MANUFACTURING_LIMITS.raw_bytes_aggregate
        {
            return Err(FabricationError::LimitExceeded {
                resource: "retained-manufacturing-input",
            });
        }
        Ok(())
    }
}

fn valid_load_outcome_state(outcome: &ManufacturingInputOutcome) -> bool {
    match outcome.state {
        ManufacturingLoadState::Retained => {
            outcome.artifact_digest.is_some() && outcome.reason.is_none()
        }
        ManufacturingLoadState::Omitted | ManufacturingLoadState::Failed => {
            outcome.artifact_digest.is_none() && outcome.reason.is_some()
        }
    }
}

pub fn input_outcome_id(
    virtual_path: &str,
    artifact_digest: Option<&str>,
    kind_candidate: ManufacturingKindCandidate,
) -> String {
    stable_id("input", &(virtual_path, artifact_digest, kind_candidate))
        .expect("identity tuple serializes")
}

pub const MAX_COORDINATE_PM: i64 = 10_000_000_000_000;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManufacturingLimits {
    pub recognized_files: usize,
    pub raw_bytes_per_file: u64,
    pub raw_bytes_aggregate: u64,
    pub records_per_file: u64,
    pub records_aggregate: u64,
    pub lexical_tokens_per_file: u64,
    pub lexical_tokens_aggregate: u64,
    pub max_line_bytes: usize,
    pub max_text_bytes: usize,
    pub metadata_bytes_per_file: u64,
    pub max_numeric_bytes: usize,
    pub max_decimal_places: u8,
    pub max_coordinate_pm: i64,
    pub max_nesting: u8,
    pub max_aperture_nesting: u8,
    pub apertures: usize,
    pub macros: usize,
    pub macro_variables: usize,
    pub operations_per_macro: usize,
    pub strict_tool_max: u16,
    pub geometry_features: usize,
    pub contour_vertices: usize,
    pub drill_route_features: usize,
    pub repeat_factor: u32,
    pub canonical_allocation_bytes: u64,
    pub file_timeout_ms: u64,
    pub aggregate_timeout_ms: u64,
    pub archive_compressed_bytes: u64,
    pub archive_expanded_bytes: u64,
    pub archive_entries: usize,
    pub normalized_path_bytes: usize,
    pub directory_depth: u8,
}

pub const MANUFACTURING_LIMITS: ManufacturingLimits = ManufacturingLimits {
    recognized_files: 256,
    raw_bytes_per_file: 4 * 1024 * 1024,
    raw_bytes_aggregate: 20 * 1024 * 1024,
    records_per_file: 400_000,
    records_aggregate: 1_000_000,
    lexical_tokens_per_file: 1_000_000,
    lexical_tokens_aggregate: 2_000_000,
    max_line_bytes: 16 * 1024,
    max_text_bytes: 4 * 1024,
    metadata_bytes_per_file: 64 * 1024,
    max_numeric_bytes: 64,
    max_decimal_places: 9,
    max_coordinate_pm: MAX_COORDINATE_PM,
    max_nesting: 32,
    max_aperture_nesting: 16,
    apertures: 10_000,
    macros: 1_024,
    macro_variables: 1_024,
    operations_per_macro: 4_096,
    strict_tool_max: 99,
    geometry_features: 419_425,
    contour_vertices: 1_000_000,
    drill_route_features: 100_000,
    repeat_factor: 1_000,
    canonical_allocation_bytes: 256 * 1024 * 1024,
    file_timeout_ms: 5_000,
    aggregate_timeout_ms: 30_000,
    archive_compressed_bytes: 90 * 1024 * 1024,
    archive_expanded_bytes: 256 * 1024 * 1024,
    archive_entries: 2_000,
    normalized_path_bytes: 512,
    directory_depth: 12,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FabricationError {
    InvalidNumber,
    NumericTokenTooLong,
    TooManyDecimalPlaces { actual: usize },
    FinerThanPicometre,
    ArithmeticOverflow,
    CoordinateOutOfRange,
    InvalidScale,
    InvalidDigest(String),
    InvalidIdentity(String),
    DuplicateId(String),
    DanglingReference(String),
    InvalidProvenance(String),
    InvalidOmission(String),
    InvalidConflict(String),
    LimitExceeded { resource: &'static str },
    DigestMismatch,
    PackageIdentityMismatch,
    AllocationEstimateMismatch,
    Serialization(String),
}

impl std::fmt::Display for FabricationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for FabricationError {}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SourceUnit {
    Millimetre,
    Inch,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(transparent)]
pub struct Picometres(pub i64);

impl Picometres {
    pub fn parse_decimal(value: &str, unit: SourceUnit) -> Result<Self, FabricationError> {
        if value.is_empty() || value.len() > MANUFACTURING_LIMITS.max_numeric_bytes {
            return Err(if value.len() > MANUFACTURING_LIMITS.max_numeric_bytes {
                FabricationError::NumericTokenTooLong
            } else {
                FabricationError::InvalidNumber
            });
        }
        let (negative, unsigned) = match value.as_bytes()[0] {
            b'-' => (true, &value[1..]),
            b'+' => (false, &value[1..]),
            _ => (false, value),
        };
        if unsigned.is_empty() || unsigned.matches('.').count() > 1 {
            return Err(FabricationError::InvalidNumber);
        }
        let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
        if whole.is_empty() && fraction.is_empty()
            || !whole.bytes().all(|byte| byte.is_ascii_digit())
            || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(FabricationError::InvalidNumber);
        }
        if fraction.len() > usize::from(MANUFACTURING_LIMITS.max_decimal_places) {
            return Err(FabricationError::TooManyDecimalPlaces {
                actual: fraction.len(),
            });
        }
        let mut mantissa = 0_i128;
        for byte in whole.bytes().chain(fraction.bytes()) {
            mantissa = mantissa
                .checked_mul(10)
                .and_then(|number| number.checked_add(i128::from(byte - b'0')))
                .ok_or(FabricationError::ArithmeticOverflow)?;
        }
        if negative {
            mantissa = mantissa
                .checked_neg()
                .ok_or(FabricationError::ArithmeticOverflow)?;
        }
        let factor = match unit {
            SourceUnit::Millimetre => 1_000_000_000_i128,
            SourceUnit::Inch => 25_400_000_000_i128,
        };
        let denominator = 10_i128
            .checked_pow(fraction.len() as u32)
            .ok_or(FabricationError::ArithmeticOverflow)?;
        let numerator = mantissa
            .checked_mul(factor)
            .ok_or(FabricationError::ArithmeticOverflow)?;
        if numerator % denominator != 0 {
            return Err(FabricationError::FinerThanPicometre);
        }
        let picometres = numerator / denominator;
        if picometres.unsigned_abs() > MAX_COORDINATE_PM as u128 {
            return Err(FabricationError::CoordinateOutOfRange);
        }
        Ok(Self(
            i64::try_from(picometres).map_err(|_| FabricationError::ArithmeticOverflow)?,
        ))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct SourceNumericFormat {
    pub unit: SourceUnit,
    pub integer_digits: u8,
    pub decimal_digits: u8,
    pub resolution: Picometres,
}

impl SourceNumericFormat {
    pub fn new(
        unit: SourceUnit,
        integer_digits: u8,
        decimal_digits: u8,
    ) -> Result<Self, FabricationError> {
        if integer_digits == 0 || decimal_digits > MANUFACTURING_LIMITS.max_decimal_places {
            return Err(FabricationError::InvalidNumber);
        }
        let resolution = if decimal_digits == 0 {
            Picometres::parse_decimal("1", unit)?
        } else {
            Picometres::parse_decimal(
                &format!("0.{}1", "0".repeat(usize::from(decimal_digits - 1))),
                unit,
            )?
        };
        Ok(Self {
            unit,
            integer_digits,
            decimal_digits,
            resolution,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalPoint {
    pub x: Picometres,
    pub y: Picometres,
}

impl CanonicalPoint {
    pub const fn new(x: i64, y: i64) -> Self {
        Self {
            x: Picometres(x),
            y: Picometres(y),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum TransformOperation {
    Mirror { x: bool, y: bool },
    Rotate { microdegrees: i64 },
    Scale { numerator: i64, denominator: i64 },
    Translate { x: Picometres, y: Picometres },
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct TransformChain {
    pub operations: Vec<TransformOperation>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QuantizationRecord {
    pub operation_index: usize,
    pub routine: String,
    pub max_error_pm: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MaterializedPoint {
    pub point: CanonicalPoint,
    pub quantization: Vec<QuantizationRecord>,
}

impl TransformChain {
    pub fn materialize(
        &self,
        mut point: CanonicalPoint,
    ) -> Result<MaterializedPoint, FabricationError> {
        validate_point(point)?;
        let mut quantization = Vec::new();
        for (operation_index, operation) in self.operations.iter().enumerate() {
            match *operation {
                TransformOperation::Mirror { x, y } => {
                    if x {
                        point.x.0 = point
                            .x
                            .0
                            .checked_neg()
                            .ok_or(FabricationError::ArithmeticOverflow)?;
                    }
                    if y {
                        point.y.0 = point
                            .y
                            .0
                            .checked_neg()
                            .ok_or(FabricationError::ArithmeticOverflow)?;
                    }
                }
                TransformOperation::Rotate { microdegrees } => {
                    let angle = microdegrees.rem_euclid(360_000_000);
                    point = match angle {
                        0 => point,
                        90_000_000 => CanonicalPoint::new(
                            point
                                .y
                                .0
                                .checked_neg()
                                .ok_or(FabricationError::ArithmeticOverflow)?,
                            point.x.0,
                        ),
                        180_000_000 => CanonicalPoint::new(
                            point
                                .x
                                .0
                                .checked_neg()
                                .ok_or(FabricationError::ArithmeticOverflow)?,
                            point
                                .y
                                .0
                                .checked_neg()
                                .ok_or(FabricationError::ArithmeticOverflow)?,
                        ),
                        270_000_000 => CanonicalPoint::new(
                            point.y.0,
                            point
                                .x
                                .0
                                .checked_neg()
                                .ok_or(FabricationError::ArithmeticOverflow)?,
                        ),
                        _ => {
                            let input_magnitude =
                                point.x.0.unsigned_abs() + point.y.0.unsigned_abs();
                            let rotated = cordic_rotate(point, angle)?;
                            quantization.push(QuantizationRecord {
                                operation_index,
                                routine: "cordic-microdegree-v1".into(),
                                // Conservative integer-CORDIC and microdegree residual bound.
                                max_error_pm: input_magnitude / 500_000 + 16,
                            });
                            rotated
                        }
                    };
                }
                TransformOperation::Scale {
                    numerator,
                    denominator,
                } => {
                    if denominator == 0 || numerator == 0 {
                        return Err(FabricationError::InvalidScale);
                    }
                    let (x, x_rounded) = checked_ratio(point.x.0, numerator, denominator)?;
                    let (y, y_rounded) = checked_ratio(point.y.0, numerator, denominator)?;
                    point = CanonicalPoint::new(x, y);
                    if x_rounded || y_rounded {
                        quantization.push(QuantizationRecord {
                            operation_index,
                            routine: "rational-scale-v1".into(),
                            max_error_pm: 1,
                        });
                    }
                }
                TransformOperation::Translate { x, y } => {
                    point.x.0 = point
                        .x
                        .0
                        .checked_add(x.0)
                        .ok_or(FabricationError::ArithmeticOverflow)?;
                    point.y.0 = point
                        .y
                        .0
                        .checked_add(y.0)
                        .ok_or(FabricationError::ArithmeticOverflow)?;
                }
            }
            validate_point(point)?;
        }
        Ok(MaterializedPoint {
            point,
            quantization,
        })
    }
}

const CORDIC_ATAN_MICRODEGREES: [i64; 27] = [
    45_000_000, 26_565_051, 14_036_243, 7_125_016, 3_576_334, 1_789_911, 895_174, 447_614, 223_811,
    111_906, 55_953, 27_976, 13_988, 6_994, 3_497, 1_749, 874, 437, 219, 109, 55, 27, 14, 7, 3, 2,
    1,
];

fn cordic_rotate(
    point: CanonicalPoint,
    normalized_angle: i64,
) -> Result<CanonicalPoint, FabricationError> {
    let mut angle = if normalized_angle > 180_000_000 {
        normalized_angle - 360_000_000
    } else {
        normalized_angle
    };
    let mut x = i128::from(point.x.0);
    let mut y = i128::from(point.y.0);
    if angle > 90_000_000 {
        x = -x;
        y = -y;
        angle -= 180_000_000;
    } else if angle < -90_000_000 {
        x = -x;
        y = -y;
        angle += 180_000_000;
    }
    x = rounded_div(
        x.checked_mul(607_252_935_009)
            .ok_or(FabricationError::ArithmeticOverflow)?,
        1_000_000_000_000,
    );
    y = rounded_div(
        y.checked_mul(607_252_935_009)
            .ok_or(FabricationError::ArithmeticOverflow)?,
        1_000_000_000_000,
    );
    let mut residual = angle;
    for (shift, arctangent) in CORDIC_ATAN_MICRODEGREES.into_iter().enumerate() {
        let direction = if residual >= 0 { 1_i128 } else { -1_i128 };
        let next_x = x
            .checked_sub(direction * rounded_div(y, 1_i128 << shift))
            .ok_or(FabricationError::ArithmeticOverflow)?;
        let next_y = y
            .checked_add(direction * rounded_div(x, 1_i128 << shift))
            .ok_or(FabricationError::ArithmeticOverflow)?;
        x = next_x;
        y = next_y;
        residual -= i64::try_from(direction).unwrap_or(1) * arctangent;
    }
    let point = CanonicalPoint::new(
        i64::try_from(x).map_err(|_| FabricationError::ArithmeticOverflow)?,
        i64::try_from(y).map_err(|_| FabricationError::ArithmeticOverflow)?,
    );
    validate_point(point)?;
    Ok(point)
}

fn rounded_div(numerator: i128, denominator: i128) -> i128 {
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    if remainder == 0 {
        quotient
    } else if remainder.unsigned_abs() * 2 >= denominator.unsigned_abs() {
        quotient
            + if numerator.signum() == denominator.signum() {
                1
            } else {
                -1
            }
    } else {
        quotient
    }
}

fn checked_ratio(
    value: i64,
    numerator: i64,
    denominator: i64,
) -> Result<(i64, bool), FabricationError> {
    let product = i128::from(value)
        .checked_mul(i128::from(numerator))
        .ok_or(FabricationError::ArithmeticOverflow)?;
    let denominator = i128::from(denominator);
    let rounded = product % denominator != 0;
    let result = rounded_div(product, denominator);
    Ok((
        i64::try_from(result).map_err(|_| FabricationError::ArithmeticOverflow)?,
        rounded,
    ))
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct StructuralLocation {
    pub record: u64,
    pub subrecord: Option<u32>,
    pub byte_start: u64,
    pub byte_end: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManufacturingProvenance {
    pub document_id: String,
    pub artifact_digest: String,
    pub producer: String,
    pub producer_version: String,
    pub location: StructuralLocation,
    pub source_lexeme: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DocumentFormat {
    Gerber,
    Excellon,
    KicadPcb,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ParseStatus {
    Complete,
    Partial,
    Failed,
    Unsupported,
    NotProvided,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMetrics {
    pub raw_bytes: u64,
    pub records: u64,
    pub lexical_tokens: u64,
    pub metadata_bytes: u64,
    pub max_line_bytes: usize,
    pub max_text_bytes: usize,
    pub max_numeric_bytes: usize,
    pub max_nesting: u8,
    pub max_aperture_nesting: u8,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManufacturingDocument {
    pub id: String,
    pub virtual_path: String,
    pub artifact_digest: String,
    pub format: DocumentFormat,
    pub adapter: String,
    pub adapter_version: String,
    pub parse_status: ParseStatus,
    pub numeric_format: Option<SourceNumericFormat>,
    pub metrics: DocumentMetrics,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Authority {
    NativeSource,
    Explicit,
    X2,
    FileContent,
    FilenameInference,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProductIdentity {
    pub name: Option<String>,
    pub revision: Option<String>,
    pub part_number: Option<String>,
    pub authority: Authority,
    pub provenance: Vec<ManufacturingProvenance>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum LayerRole {
    Copper,
    SolderMask,
    Paste,
    Legend,
    Profile,
    DrillMap,
    Route,
    Assembly,
    FabricationDrawing,
    Other,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum LayerSide {
    Top,
    Bottom,
    Inner,
    Both,
    NotApplicable,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum LayerContext {
    Board,
    Coupon,
    Panel,
    Component,
    Other,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum LayerPolarity {
    Positive,
    Negative,
    Dark,
    Clear,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManufacturingLayer {
    pub id: String,
    pub document_id: String,
    pub name: Option<String>,
    pub role: LayerRole,
    pub side: LayerSide,
    pub context: LayerContext,
    pub polarity: LayerPolarity,
    pub order: Option<i32>,
    pub authority: Authority,
    pub provenance: ManufacturingProvenance,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalLine {
    pub start: CanonicalPoint,
    pub end: CanonicalPoint,
    pub width: Option<Picometres>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ArcDirection {
    Clockwise,
    CounterClockwise,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum QuadrantMode {
    Single,
    Multi,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalArc {
    pub start: CanonicalPoint,
    pub end: CanonicalPoint,
    pub center: CanonicalPoint,
    pub direction: ArcDirection,
    pub quadrant: QuadrantMode,
    pub width: Option<Picometres>,
    pub source_resolution: Picometres,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ContourSegment {
    Line(CanonicalLine),
    Arc(CanonicalArc),
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalContour {
    pub segments: Vec<ContourSegment>,
    pub closed: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalRegion {
    pub contours: Vec<CanonicalContour>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalFlash {
    pub position: CanonicalPoint,
    pub aperture_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct DrillFeature {
    pub position: CanonicalPoint,
    pub diameter: Picometres,
    pub tool_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct RouteFeature {
    pub segments: Vec<ContourSegment>,
    pub tool_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct SlotFeature {
    pub start: CanonicalPoint,
    pub end: CanonicalPoint,
    pub width: Picometres,
    pub tool_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Geometry {
    Point(CanonicalPoint),
    Line(CanonicalLine),
    Arc(CanonicalArc),
    Contour(CanonicalContour),
    Region(CanonicalRegion),
    Flash(CanonicalFlash),
    Drill(DrillFeature),
    Route(RouteFeature),
    Slot(SlotFeature),
}

impl Geometry {
    fn kind(&self) -> &'static str {
        match self {
            Self::Point(_) => "point",
            Self::Line(_) => "line",
            Self::Arc(_) => "arc",
            Self::Contour(_) => "contour",
            Self::Region(_) => "region",
            Self::Flash(_) => "flash",
            Self::Drill(_) => "drill",
            Self::Route(_) => "route",
            Self::Slot(_) => "slot",
        }
    }

    fn vertex_count(&self) -> usize {
        fn contour_vertices(contour: &CanonicalContour) -> usize {
            contour
                .segments
                .len()
                .saturating_add(usize::from(contour.closed))
        }
        match self {
            Self::Contour(contour) => contour_vertices(contour),
            Self::Region(region) => region.contours.iter().map(contour_vertices).sum(),
            Self::Route(route) => route.segments.len().saturating_add(1),
            Self::Line(_) | Self::Arc(_) | Self::Slot(_) => 2,
            Self::Point(_) | Self::Flash(_) | Self::Drill(_) => 1,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManufacturingFeature {
    pub id: String,
    pub document_id: String,
    pub layer_id: String,
    pub tool_id: Option<String>,
    pub polarity: LayerPolarity,
    pub geometry: Geometry,
    pub transforms: TransformChain,
    pub provenance: ManufacturingProvenance,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    Aperture,
    Drill,
    Route,
    Composite,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Plating {
    Plated,
    NonPlated,
    Mixed,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct LayerSpan {
    pub from_layer_id: Option<String>,
    pub to_layer_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManufacturingTool {
    pub id: String,
    pub document_id: String,
    pub code: String,
    pub kind: ToolKind,
    pub diameter: Option<Picometres>,
    pub plating: Plating,
    pub span: Option<LayerSpan>,
    pub provenance: ManufacturingProvenance,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ApertureShape {
    Circle,
    Rectangle,
    Obround,
    Polygon,
    Macro,
    Block,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalRational {
    pub numerator: String,
    pub denominator: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApertureDefinition {
    pub id: String,
    pub document_id: String,
    pub shape: ApertureShape,
    pub dimensions: Vec<Picometres>,
    pub polygon_vertices: Option<u8>,
    pub polygon_rotation_microdegrees: Option<i64>,
    pub macro_id: Option<String>,
    pub macro_arguments: Vec<CanonicalRational>,
    pub provenance: ManufacturingProvenance,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MacroDefinition {
    pub id: String,
    pub document_id: String,
    pub name: String,
    pub variables: Vec<String>,
    pub operations: Vec<String>,
    pub provenance: ManufacturingProvenance,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApertureBlock {
    pub id: String,
    pub document_id: String,
    pub feature_ids: Vec<String>,
    pub provenance: ManufacturingProvenance,
}

/// Compact repetition where x/y counts include an instance at offset zero.
/// A feature's first repeat reuses its global original; later references charge full grids.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StepRepeat {
    pub id: String,
    pub document_id: String,
    pub feature_ids: Vec<String>,
    pub x_count: u32,
    pub y_count: u32,
    pub x_step: Picometres,
    pub y_step: Picometres,
    pub provenance: ManufacturingProvenance,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Extent {
    pub min: CanonicalPoint,
    pub max: CanonicalPoint,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BoardProfile {
    pub contour_feature_ids: Vec<String>,
    pub cutout_feature_ids: Vec<String>,
    pub extents: Option<Extent>,
    pub provenance: Vec<ManufacturingProvenance>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ObjectSemantics {
    pub feature_id: String,
    pub net: Option<String>,
    pub component: Option<String>,
    pub pin: Option<String>,
    pub provenance: ManufacturingProvenance,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AssemblyPlacement {
    pub reference: String,
    pub side: LayerSide,
    pub position: CanonicalPoint,
    pub rotation_microdegrees: i64,
    pub provenance: ManufacturingProvenance,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AssemblyEvidence {
    pub placements: Vec<AssemblyPlacement>,
    pub mask_layer_ids: Vec<String>,
    pub paste_layer_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConstructionLayer {
    pub layer_id: Option<String>,
    pub material: Option<String>,
    pub thickness: Option<Picometres>,
    pub authority: Authority,
    pub provenance: ManufacturingProvenance,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConstructionEvidence {
    pub layers: Vec<ConstructionLayer>,
    pub total_thickness: Option<Picometres>,
    pub finish: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintKind {
    MinimumTrackWidth,
    MinimumClearance,
    MinimumDrill,
    MinimumAnnularRing,
    CopperWeight,
    FinishedThickness,
    Impedance,
    Material,
    Finish,
    SpecialProcess,
    Other,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManufacturingConstraint {
    pub id: String,
    pub kind: ConstraintKind,
    pub value: Option<Picometres>,
    pub declared_value: Option<String>,
    pub authority: Authority,
    pub provenance: ManufacturingProvenance,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityId {
    ProductIdentity,
    DocumentSyntax,
    UnitsAndFormat,
    LayerRoles,
    LayerOrder,
    GeometryPoints,
    GeometryLines,
    GeometryArcs,
    GeometryRegions,
    GeometryFlashes,
    GeometryExpanded,
    Polarity,
    Transforms,
    Repetition,
    Apertures,
    Macros,
    Profile,
    Extents,
    Drills,
    Routes,
    Slots,
    Tools,
    Plating,
    LayerSpans,
    X2FileAttributes,
    X2ApertureAttributes,
    X2ObjectAttributes,
    Connectivity,
    Components,
    Pins,
    Assembly,
    Construction,
    Constraints,
    NativeKicadFacts,
    PackageCompleteness,
    PackageReconciliation,
    #[serde(rename = "legacy-filename-screening")]
    LegacyFilenameScreening,
    #[serde(rename = "legacy-token-screening")]
    LegacyTokenScreening,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityState {
    Complete,
    Partial,
    NotProvided,
    Unsupported,
    Failed,
    Stale,
    Omitted,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityRecord {
    pub id: CapabilityId,
    pub state: CapabilityState,
    pub authority: Authority,
    pub document_ids: Vec<String>,
    pub provenance: Vec<ManufacturingProvenance>,
    pub detail: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityLedger {
    pub records: Vec<CapabilityRecord>,
}

/// Adapter output is policy-free: canonical facts and evidence accounting only.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AdapterFacts {
    pub product: Option<ProductIdentity>,
    pub documents: Vec<ManufacturingDocument>,
    pub layers: Vec<ManufacturingLayer>,
    pub tools: Vec<ManufacturingTool>,
    pub apertures: Vec<ApertureDefinition>,
    pub macros: Vec<MacroDefinition>,
    pub blocks: Vec<ApertureBlock>,
    pub repetitions: Vec<StepRepeat>,
    pub features: Vec<ManufacturingFeature>,
    pub profile: Option<BoardProfile>,
    pub connectivity: Vec<ObjectSemantics>,
    pub assembly: AssemblyEvidence,
    pub construction: ConstructionEvidence,
    pub constraints: Vec<ManufacturingConstraint>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AdapterResult {
    pub facts: AdapterFacts,
    pub capabilities: CapabilityLedger,
    pub omissions: Vec<Omission>,
    pub conflicts: Vec<Conflict>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnalyzerRequirements {
    pub check_family: &'static str,
    pub prerequisites: &'static [CapabilityId],
}

pub const PACKAGE_GERBERS_ANALYZER: AnalyzerRequirements = AnalyzerRequirements {
    check_family: "package-gerbers",
    prerequisites: &[
        CapabilityId::LayerRoles,
        CapabilityId::Profile,
        CapabilityId::PackageCompleteness,
    ],
};

pub const GERBER_SYNTAX_ANALYZER: AnalyzerRequirements = AnalyzerRequirements {
    check_family: "gerber-syntax",
    prerequisites: &[CapabilityId::DocumentSyntax, CapabilityId::UnitsAndFormat],
};

pub const DRILL_DATA_ANALYZER: AnalyzerRequirements = AnalyzerRequirements {
    check_family: "drill-data",
    prerequisites: &[
        CapabilityId::UnitsAndFormat,
        CapabilityId::Tools,
        CapabilityId::Drills,
    ],
};

pub const STABLE_FABRICATION_ANALYZERS: [AnalyzerRequirements; 3] = [
    PACKAGE_GERBERS_ANALYZER,
    GERBER_SYNTAX_ANALYZER,
    DRILL_DATA_ANALYZER,
];

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SemanticAnalyzerResult {
    Pass,
    Attention,
    Fail,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnalyzerDispatchStatus {
    Pass,
    Attention,
    Fail,
    NotChecked,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzerOutcome {
    pub check_family: String,
    pub status: AnalyzerDispatchStatus,
    pub incomplete_prerequisites: Vec<CapabilityId>,
}

pub fn dispatch_analyzer(
    requirements: AnalyzerRequirements,
    ledger: &CapabilityLedger,
    semantic_result: Option<SemanticAnalyzerResult>,
) -> AnalyzerOutcome {
    let incomplete_prerequisites = requirements
        .prerequisites
        .iter()
        .copied()
        .filter(|required| {
            let mut matching = ledger
                .records
                .iter()
                .filter(|record| record.id == *required);
            matching
                .next()
                .is_none_or(|record| record.state != CapabilityState::Complete)
                || matching.next().is_some()
        })
        .collect::<Vec<_>>();
    let status = if incomplete_prerequisites.is_empty() {
        match semantic_result {
            Some(SemanticAnalyzerResult::Pass) => AnalyzerDispatchStatus::Pass,
            Some(SemanticAnalyzerResult::Attention) => AnalyzerDispatchStatus::Attention,
            Some(SemanticAnalyzerResult::Fail) => AnalyzerDispatchStatus::Fail,
            None => AnalyzerDispatchStatus::NotChecked,
        }
    } else {
        AnalyzerDispatchStatus::NotChecked
    };
    AnalyzerOutcome {
        check_family: requirements.check_family.into(),
        status,
        incomplete_prerequisites,
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum OmissionKind {
    MissingRecord,
    MissingSemanticRecord,
    UnsupportedRecord,
    ResourceLimit,
    InvalidRecord,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Omission {
    pub id: String,
    pub kind: OmissionKind,
    pub affected_capabilities: Vec<CapabilityId>,
    pub provenance: ManufacturingProvenance,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    ProductIdentity,
    LayerRole,
    LayerOrder,
    Plating,
    LayerSpan,
    Profile,
    Extent,
    Connectivity,
    Construction,
    Constraint,
    Other,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConflictFact {
    pub canonical_value: String,
    pub authority: Authority,
    pub provenance: ManufacturingProvenance,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    pub id: String,
    pub kind: ConflictKind,
    pub affected_capabilities: Vec<CapabilityId>,
    pub left: ConflictFact,
    pub right: ConflictFact,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManufacturingWarning {
    pub code: String,
    pub message: String,
    pub provenance: Option<ManufacturingProvenance>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FabricationStatus {
    NotProvided,
    Partial,
    Complete,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FabricationReview {
    pub status: FabricationStatus,
    pub package_id: String,
    pub model_digest: String,
    pub input_outcomes: Vec<ManufacturingInputOutcome>,
    pub product: Option<ProductIdentity>,
    pub documents: Vec<ManufacturingDocument>,
    pub layers: Vec<ManufacturingLayer>,
    pub tools: Vec<ManufacturingTool>,
    pub apertures: Vec<ApertureDefinition>,
    pub macros: Vec<MacroDefinition>,
    pub blocks: Vec<ApertureBlock>,
    pub repetitions: Vec<StepRepeat>,
    pub features: Vec<ManufacturingFeature>,
    pub profile: Option<BoardProfile>,
    pub connectivity: Vec<ObjectSemantics>,
    pub assembly: AssemblyEvidence,
    pub construction: ConstructionEvidence,
    pub constraints: Vec<ManufacturingConstraint>,
    pub capabilities: CapabilityLedger,
    pub omissions: Vec<Omission>,
    pub conflicts: Vec<Conflict>,
    pub warnings: Vec<ManufacturingWarning>,
    pub limits: ManufacturingLimits,
    pub estimated_allocation_bytes: u64,
}

impl Default for FabricationReview {
    fn default() -> Self {
        let mut review = Self {
            status: FabricationStatus::NotProvided,
            package_id: String::new(),
            model_digest: String::new(),
            input_outcomes: vec![],
            product: None,
            documents: vec![],
            layers: vec![],
            tools: vec![],
            apertures: vec![],
            macros: vec![],
            blocks: vec![],
            repetitions: vec![],
            features: vec![],
            profile: None,
            connectivity: vec![],
            assembly: AssemblyEvidence::default(),
            construction: ConstructionEvidence::default(),
            constraints: vec![],
            capabilities: CapabilityLedger::default(),
            omissions: vec![],
            conflicts: vec![],
            warnings: vec![],
            limits: MANUFACTURING_LIMITS,
            estimated_allocation_bytes: 0,
        };
        review
            .refresh_digests()
            .expect("empty model is serializable");
        review
    }
}

pub fn legacy_inventory_review(
    inventory: &ManufacturingInventory,
) -> Result<FabricationReview, FabricationError> {
    inventory.validate()?;
    let mut review = FabricationReview {
        status: if inventory.outcomes.is_empty() {
            FabricationStatus::NotProvided
        } else if inventory.inputs.is_empty() {
            FabricationStatus::Failed
        } else {
            FabricationStatus::Partial
        },
        input_outcomes: inventory.outcomes.clone(),
        ..FabricationReview::default()
    };
    for outcome in inventory
        .outcomes
        .iter()
        .filter(|outcome| outcome.artifact_digest.is_some())
        .take(MANUFACTURING_LIMITS.recognized_files)
    {
        let format = match outcome.kind_candidate {
            ManufacturingKindCandidate::Gerber => DocumentFormat::Gerber,
            ManufacturingKindCandidate::Excellon => DocumentFormat::Excellon,
        };
        let artifact_digest = outcome
            .artifact_digest
            .as_deref()
            .expect("filtered manufacturing digest");
        review.documents.push(ManufacturingDocument {
            id: document_id(artifact_digest, format)?,
            virtual_path: outcome.virtual_path.clone(),
            artifact_digest: artifact_digest.into(),
            format,
            adapter: "legacy-screening-inventory".into(),
            adapter_version: "1".into(),
            parse_status: if outcome.state == ManufacturingLoadState::Retained {
                ParseStatus::Partial
            } else {
                ParseStatus::Failed
            },
            numeric_format: None,
            metrics: DocumentMetrics {
                raw_bytes: if outcome.state == ManufacturingLoadState::Retained {
                    outcome.size
                } else {
                    0
                },
                ..DocumentMetrics::default()
            },
        });
    }

    let gerber = capability_source(
        &review.documents,
        DocumentFormat::Gerber,
        &inventory.outcomes,
        ManufacturingKindCandidate::Gerber,
    );
    let drill = capability_source(
        &review.documents,
        DocumentFormat::Excellon,
        &inventory.outcomes,
        ManufacturingKindCandidate::Excellon,
    );
    let combined = capability_source_all(&review.documents, &inventory.outcomes);
    review.capabilities.records = vec![
        capability_record(
            CapabilityId::LegacyFilenameScreening,
            Authority::FilenameInference,
            &gerber,
            "Filename observations are partial inventory only.",
        ),
        capability_record(
            CapabilityId::LegacyTokenScreening,
            Authority::FileContent,
            &combined,
            "Token observations are partial and are not a semantic parse.",
        ),
        capability_record(
            CapabilityId::DocumentSyntax,
            Authority::FileContent,
            &gerber,
            "No production Gerber syntax adapter ran.",
        ),
        capability_record(
            CapabilityId::UnitsAndFormat,
            Authority::FileContent,
            &combined,
            "Unit and format tokens are not semantic completion.",
        ),
        capability_record(
            CapabilityId::LayerRoles,
            Authority::FilenameInference,
            &gerber,
            "Layer roles are filename-inferred only.",
        ),
        capability_record(
            CapabilityId::Profile,
            Authority::FilenameInference,
            &gerber,
            "Profile presence is filename-inferred only.",
        ),
        capability_record(
            CapabilityId::PackageCompleteness,
            Authority::FilenameInference,
            &gerber,
            "Package completeness has not been semantically analyzed.",
        ),
        capability_record(
            CapabilityId::Tools,
            Authority::FileContent,
            &drill,
            "Drill tool tokens are not parsed tool facts.",
        ),
        capability_record(
            CapabilityId::Drills,
            Authority::FileContent,
            &drill,
            "Drill coordinate tokens are not parsed drill facts.",
        ),
    ];
    review.refresh_digests()?;
    review.validate()?;
    Ok(review)
}

#[derive(Clone, Debug)]
struct CapabilitySource {
    state: CapabilityState,
    document_ids: Vec<String>,
    provenance: Vec<ManufacturingProvenance>,
}

fn capability_source(
    documents: &[ManufacturingDocument],
    format: DocumentFormat,
    outcomes: &[ManufacturingInputOutcome],
    kind: ManufacturingKindCandidate,
) -> CapabilitySource {
    let matching_outcomes = outcomes
        .iter()
        .filter(|outcome| outcome.kind_candidate == kind)
        .collect::<Vec<_>>();
    let matching_documents = documents
        .iter()
        .filter(|document| document.format == format)
        .collect::<Vec<_>>();
    CapabilitySource {
        state: if matching_outcomes.is_empty() {
            CapabilityState::NotProvided
        } else if matching_outcomes
            .iter()
            .any(|outcome| outcome.state == ManufacturingLoadState::Retained)
        {
            CapabilityState::Partial
        } else {
            CapabilityState::Failed
        },
        document_ids: matching_documents
            .iter()
            .map(|document| document.id.clone())
            .collect(),
        provenance: matching_documents
            .iter()
            .copied()
            .map(inventory_provenance)
            .collect(),
    }
}

fn capability_source_all(
    documents: &[ManufacturingDocument],
    outcomes: &[ManufacturingInputOutcome],
) -> CapabilitySource {
    CapabilitySource {
        state: if outcomes.is_empty() {
            CapabilityState::NotProvided
        } else if outcomes
            .iter()
            .any(|outcome| outcome.state == ManufacturingLoadState::Retained)
        {
            CapabilityState::Partial
        } else {
            CapabilityState::Failed
        },
        document_ids: documents
            .iter()
            .map(|document| document.id.clone())
            .collect(),
        provenance: documents.iter().map(inventory_provenance).collect(),
    }
}

fn capability_record(
    id: CapabilityId,
    authority: Authority,
    source: &CapabilitySource,
    detail: &str,
) -> CapabilityRecord {
    CapabilityRecord {
        id,
        state: source.state,
        authority,
        document_ids: source.document_ids.clone(),
        provenance: source.provenance.clone(),
        detail: detail.into(),
    }
}

fn inventory_provenance(document: &ManufacturingDocument) -> ManufacturingProvenance {
    ManufacturingProvenance {
        document_id: document.id.clone(),
        artifact_digest: document.artifact_digest.clone(),
        producer: document.adapter.clone(),
        producer_version: document.adapter_version.clone(),
        location: StructuralLocation {
            record: 0,
            subrecord: None,
            byte_start: 0,
            byte_end: document.metrics.raw_bytes.saturating_sub(1),
        },
        source_lexeme: None,
    }
}

pub fn document_id(
    artifact_digest: &str,
    format: DocumentFormat,
) -> Result<String, FabricationError> {
    if !lowercase_sha256(artifact_digest) {
        return Err(FabricationError::InvalidDigest(artifact_digest.into()));
    }
    stable_id("document", &(artifact_digest, format))
}

pub fn layer_id(document_id: &str, role: LayerRole, location: &StructuralLocation) -> String {
    stable_id("layer", &(document_id, role, location)).expect("identity tuple serializes")
}

pub fn tool_id(document_id: &str, identity_kind: &str, location: &StructuralLocation) -> String {
    stable_id("tool", &(document_id, identity_kind, location)).expect("identity tuple serializes")
}

pub fn aperture_id(
    document_id: &str,
    shape: ApertureShape,
    location: &StructuralLocation,
) -> String {
    stable_id("aperture", &(document_id, shape, location)).expect("identity tuple serializes")
}

pub fn feature_id(
    document_id: &str,
    layer_id: &str,
    semantic_kind: &str,
    location: &StructuralLocation,
) -> String {
    stable_id("feature", &(document_id, layer_id, semantic_kind, location))
        .expect("identity tuple serializes")
}

pub fn constraint_id(
    document_id: &str,
    kind: ConstraintKind,
    location: &StructuralLocation,
) -> String {
    stable_id("constraint", &(document_id, kind, location)).expect("identity tuple serializes")
}

fn record_id(kind: &str, document_id: &str, location: &StructuralLocation) -> String {
    stable_id(kind, &(document_id, location)).expect("identity tuple serializes")
}

fn stable_id(kind: &str, fields: &impl Serialize) -> Result<String, FabricationError> {
    let canonical = serde_json::to_vec(&("fabrication-identity-v1", kind, fields))
        .map_err(|error| FabricationError::Serialization(error.to_string()))?;
    Ok(format!("{kind}-v1-{}", sha256(canonical)))
}

impl FabricationReview {
    pub fn refresh_digests(&mut self) -> Result<(), FabricationError> {
        self.package_id = self.expected_package_id()?;
        self.model_digest = self.expected_model_digest()?;
        self.estimated_allocation_bytes = self.estimate_allocation()?;
        Ok(())
    }

    fn finalize_trusted(&mut self) -> Result<(), FabricationError> {
        if self.limits != MANUFACTURING_LIMITS {
            return Err(FabricationError::LimitExceeded {
                resource: "limits-contract",
            });
        }
        self.validate_limits()?;
        self.validate_identities_and_references()?;
        self.package_id = self.expected_package_id()?;
        self.model_digest = self.expected_model_digest()?;
        self.estimated_allocation_bytes = self.estimate_allocation()?;
        if self.estimated_allocation_bytes > self.limits.canonical_allocation_bytes {
            return Err(FabricationError::LimitExceeded {
                resource: "canonical-allocation",
            });
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), FabricationError> {
        if self.limits != MANUFACTURING_LIMITS {
            return Err(FabricationError::LimitExceeded {
                resource: "limits-contract",
            });
        }
        self.validate_limits()?;
        self.validate_identities_and_references()?;
        if self.package_id != self.expected_package_id()? {
            return Err(FabricationError::PackageIdentityMismatch);
        }
        if !lowercase_sha256(&self.model_digest)
            || self.model_digest != self.expected_model_digest()?
        {
            return Err(FabricationError::DigestMismatch);
        }
        let estimated = self.estimate_allocation()?;
        if self.estimated_allocation_bytes != estimated {
            return Err(FabricationError::AllocationEstimateMismatch);
        }
        if estimated > self.limits.canonical_allocation_bytes {
            return Err(FabricationError::LimitExceeded {
                resource: "canonical-allocation",
            });
        }
        Ok(())
    }

    fn expected_package_id(&self) -> Result<String, FabricationError> {
        let mut documents: Vec<_> = self.documents.iter().map(|document| &document.id).collect();
        documents.sort();
        let product = self.product.as_ref().map(|product| {
            (
                product.name.as_deref(),
                product.revision.as_deref(),
                product.part_number.as_deref(),
                product.authority,
            )
        });
        stable_id("package", &(documents, product))
    }

    fn expected_model_digest(&self) -> Result<String, FabricationError> {
        let mut records = Vec::new();
        records.push(canonical_json("status", &self.status)?);
        records.push(canonical_json("package", &self.package_id)?);
        records.push(canonical_json("limits", &self.limits)?);
        for outcome in &self.input_outcomes {
            records.push(canonical_json("input-outcome", outcome)?);
        }
        if let Some(product) = &self.product {
            records.push(canonical_json(
                "product",
                &(
                    &product.name,
                    &product.revision,
                    &product.part_number,
                    product.authority,
                    canonical_provenances(&product.provenance),
                ),
            )?);
        }
        for document in &self.documents {
            records.push(canonical_json(
                "document",
                &(
                    &document.id,
                    &document.artifact_digest,
                    document.format,
                    &document.adapter,
                    &document.adapter_version,
                    document.parse_status,
                    &document.numeric_format,
                ),
            )?);
        }
        for layer in &self.layers {
            records.push(canonical_json(
                "layer",
                &(
                    &layer.id,
                    &layer.document_id,
                    layer.role,
                    layer.side,
                    layer.context,
                    layer.polarity,
                    layer.order,
                    layer.authority,
                    canonical_provenance(&layer.provenance),
                ),
            )?);
        }
        for tool in &self.tools {
            records.push(canonical_json(
                "tool",
                &(
                    &tool.id,
                    &tool.document_id,
                    &tool.code,
                    tool.kind,
                    tool.diameter,
                    tool.plating,
                    &tool.span,
                    canonical_provenance(&tool.provenance),
                ),
            )?);
        }
        for aperture in &self.apertures {
            records.push(canonical_json(
                "aperture",
                &(
                    &aperture.id,
                    &aperture.document_id,
                    aperture.shape,
                    &aperture.dimensions,
                    aperture.polygon_vertices,
                    aperture.polygon_rotation_microdegrees,
                    &aperture.macro_id,
                    &aperture.macro_arguments,
                    canonical_provenance(&aperture.provenance),
                ),
            )?);
        }
        for definition in &self.macros {
            records.push(canonical_json(
                "macro",
                &(
                    &definition.id,
                    &definition.document_id,
                    &definition.name,
                    &definition.variables,
                    &definition.operations,
                    canonical_provenance(&definition.provenance),
                ),
            )?);
        }
        for block in &self.blocks {
            let mut feature_ids = block.feature_ids.clone();
            feature_ids.sort();
            records.push(canonical_json(
                "block",
                &(
                    &block.id,
                    &block.document_id,
                    feature_ids,
                    canonical_provenance(&block.provenance),
                ),
            )?);
        }
        for repeat in &self.repetitions {
            let mut feature_ids = repeat.feature_ids.clone();
            feature_ids.sort();
            records.push(canonical_json(
                "repeat",
                &(
                    &repeat.id,
                    &repeat.document_id,
                    feature_ids,
                    repeat.x_count,
                    repeat.y_count,
                    repeat.x_step,
                    repeat.y_step,
                    canonical_provenance(&repeat.provenance),
                ),
            )?);
        }
        for feature in &self.features {
            records.push(canonical_json(
                "feature",
                &(
                    &feature.id,
                    &feature.document_id,
                    &feature.layer_id,
                    &feature.tool_id,
                    feature.polarity,
                    &feature.geometry,
                    &feature.transforms,
                    canonical_provenance(&feature.provenance),
                ),
            )?);
        }
        if let Some(profile) = &self.profile {
            let mut contours = profile.contour_feature_ids.clone();
            let mut cutouts = profile.cutout_feature_ids.clone();
            contours.sort();
            cutouts.sort();
            records.push(canonical_json(
                "profile",
                &(
                    contours,
                    cutouts,
                    &profile.extents,
                    canonical_provenances(&profile.provenance),
                ),
            )?);
        }
        for semantic in &self.connectivity {
            records.push(canonical_json(
                "connectivity",
                &(
                    &semantic.feature_id,
                    &semantic.net,
                    &semantic.component,
                    &semantic.pin,
                    canonical_provenance(&semantic.provenance),
                ),
            )?);
        }
        for placement in &self.assembly.placements {
            records.push(canonical_json(
                "placement",
                &(
                    &placement.reference,
                    placement.side,
                    placement.position,
                    placement.rotation_microdegrees,
                    canonical_provenance(&placement.provenance),
                ),
            )?);
        }
        let mut mask_layers = self.assembly.mask_layer_ids.clone();
        let mut paste_layers = self.assembly.paste_layer_ids.clone();
        mask_layers.sort();
        paste_layers.sort();
        records.push(canonical_json(
            "assembly-layers",
            &(mask_layers, paste_layers),
        )?);
        for layer in &self.construction.layers {
            records.push(canonical_json(
                "construction-layer",
                &(
                    &layer.layer_id,
                    &layer.material,
                    layer.thickness,
                    layer.authority,
                    canonical_provenance(&layer.provenance),
                ),
            )?);
        }
        records.push(canonical_json(
            "construction",
            &(
                &self.construction.total_thickness,
                &self.construction.finish,
            ),
        )?);
        for constraint in &self.constraints {
            records.push(canonical_json(
                "constraint",
                &(
                    &constraint.id,
                    constraint.kind,
                    constraint.value,
                    &constraint.declared_value,
                    constraint.authority,
                    canonical_provenance(&constraint.provenance),
                ),
            )?);
        }
        for capability in &self.capabilities.records {
            let mut documents = capability.document_ids.clone();
            documents.sort();
            records.push(canonical_json(
                "capability",
                &(
                    capability.id,
                    capability.state,
                    capability.authority,
                    documents,
                    canonical_provenances(&capability.provenance),
                ),
            )?);
        }
        for omission in &self.omissions {
            let mut affected = omission.affected_capabilities.clone();
            affected.sort();
            records.push(canonical_json(
                "omission",
                &(
                    &omission.id,
                    omission.kind,
                    affected,
                    canonical_provenance(&omission.provenance),
                ),
            )?);
        }
        for conflict in &self.conflicts {
            let mut affected = conflict.affected_capabilities.clone();
            affected.sort();
            records.push(canonical_json(
                "conflict",
                &(
                    &conflict.id,
                    conflict.kind,
                    affected,
                    &conflict.left.canonical_value,
                    conflict.left.authority,
                    canonical_provenance(&conflict.left.provenance),
                    &conflict.right.canonical_value,
                    conflict.right.authority,
                    canonical_provenance(&conflict.right.provenance),
                ),
            )?);
        }
        records.sort();
        Ok(sha256(records.join("\n")))
    }

    fn expanded_feature_instances(&self) -> Result<u64, FabricationError> {
        let mut total =
            u64::try_from(self.features.len()).map_err(|_| FabricationError::ArithmeticOverflow)?;
        let mut originals_reused = BTreeSet::new();
        for repeat in &self.repetitions {
            let grid = u64::from(repeat.x_count)
                .checked_mul(u64::from(repeat.y_count))
                .ok_or(FabricationError::ArithmeticOverflow)?;
            let feature_count = u64::try_from(repeat.feature_ids.len())
                .map_err(|_| FabricationError::ArithmeticOverflow)?;
            let repeated = feature_count
                .checked_mul(grid)
                .ok_or(FabricationError::ArithmeticOverflow)?;
            let newly_reused_originals = u64::try_from(
                repeat
                    .feature_ids
                    .iter()
                    .filter(|feature_id| originals_reused.insert(feature_id.as_str()))
                    .count(),
            )
            .map_err(|_| FabricationError::ArithmeticOverflow)?;
            total = total
                .checked_add(repeated)
                .and_then(|total| total.checked_sub(newly_reused_originals))
                .ok_or(FabricationError::ArithmeticOverflow)?;
        }
        Ok(total)
    }

    fn validate_limits(&self) -> Result<(), FabricationError> {
        if self.documents.len() > self.limits.recognized_files
            || self.input_outcomes.len() > self.limits.archive_entries
            || self.features.len() > self.limits.geometry_features
            || self.layers.len() > self.limits.geometry_features
            || self.blocks.len() > self.limits.geometry_features
            || self.repetitions.len() > self.limits.geometry_features
            || self.connectivity.len() > self.limits.geometry_features
            || self.constraints.len() > self.limits.geometry_features
            || self.capabilities.records.len() > self.limits.geometry_features
            || self.omissions.len() > self.limits.geometry_features
            || self.conflicts.len() > self.limits.geometry_features
            || self.warnings.len() > self.limits.geometry_features
            || self.assembly.placements.len() > self.limits.geometry_features
            || self.assembly.mask_layer_ids.len() > self.limits.geometry_features
            || self.assembly.paste_layer_ids.len() > self.limits.geometry_features
            || self.construction.layers.len() > self.limits.geometry_features
            || self.apertures.len() > self.limits.apertures
            || self.macros.len() > self.limits.macros
            || self.tools.len() > self.limits.apertures
        {
            return Err(FabricationError::LimitExceeded {
                resource: "collection-count",
            });
        }
        let mut outcome_ids = BTreeSet::new();
        let mut outcome_paths = BTreeSet::new();
        let mut retained_outcomes = 0_usize;
        let mut retained_outcome_bytes = 0_u64;
        for outcome in &self.input_outcomes {
            if !outcome_ids.insert(outcome.id.as_str())
                || !outcome_paths.insert(outcome.virtual_path.as_str())
                || !valid_virtual_path(&outcome.virtual_path)
                || outcome.virtual_path.len() > self.limits.normalized_path_bytes
                || path_directory_depth(&outcome.virtual_path)
                    > usize::from(self.limits.directory_depth)
                || outcome
                    .artifact_digest
                    .as_deref()
                    .is_some_and(|digest| !lowercase_sha256(digest))
                || outcome.id
                    != input_outcome_id(
                        &outcome.virtual_path,
                        outcome.artifact_digest.as_deref(),
                        outcome.kind_candidate,
                    )
                || !valid_load_outcome_state(outcome)
            {
                return Err(FabricationError::InvalidIdentity(outcome.id.clone()));
            }
            if outcome.state == ManufacturingLoadState::Retained {
                retained_outcomes += 1;
                retained_outcome_bytes = retained_outcome_bytes
                    .checked_add(outcome.size)
                    .ok_or(FabricationError::ArithmeticOverflow)?;
                if outcome.size > self.limits.raw_bytes_per_file {
                    return Err(FabricationError::LimitExceeded {
                        resource: "retained-input-size",
                    });
                }
            }
        }
        if retained_outcomes > self.limits.recognized_files
            || retained_outcome_bytes > self.limits.raw_bytes_aggregate
        {
            return Err(FabricationError::LimitExceeded {
                resource: "retained-input-aggregate",
            });
        }
        let mut raw = 0_u64;
        let mut records = 0_u64;
        let mut tokens = 0_u64;
        for document in &self.documents {
            let metrics = &document.metrics;
            raw = raw
                .checked_add(metrics.raw_bytes)
                .ok_or(FabricationError::ArithmeticOverflow)?;
            records = records
                .checked_add(metrics.records)
                .ok_or(FabricationError::ArithmeticOverflow)?;
            tokens = tokens
                .checked_add(metrics.lexical_tokens)
                .ok_or(FabricationError::ArithmeticOverflow)?;
            if !valid_virtual_path(&document.virtual_path)
                || document.virtual_path.len() > self.limits.normalized_path_bytes
                || document.adapter.is_empty()
                || document.adapter_version.is_empty()
                || document.numeric_format.as_ref().is_some_and(|format| {
                    !matches!(
                        SourceNumericFormat::new(
                            format.unit,
                            format.integer_digits,
                            format.decimal_digits,
                        ),
                        Ok(expected) if expected == *format
                    )
                })
                || metrics.raw_bytes > self.limits.raw_bytes_per_file
                || metrics.records > self.limits.records_per_file
                || metrics.lexical_tokens > self.limits.lexical_tokens_per_file
                || metrics.metadata_bytes > self.limits.metadata_bytes_per_file
                || metrics.max_line_bytes > self.limits.max_line_bytes
                || metrics.max_text_bytes > self.limits.max_text_bytes
                || metrics.max_numeric_bytes > self.limits.max_numeric_bytes
                || metrics.max_nesting > self.limits.max_nesting
                || metrics.max_aperture_nesting > self.limits.max_aperture_nesting
            {
                return Err(FabricationError::LimitExceeded {
                    resource: "per-document",
                });
            }
        }
        if self.input_outcomes.iter().any(|outcome| {
            outcome.state == ManufacturingLoadState::Retained
                && !self.documents.iter().any(|document| {
                    document.virtual_path == outcome.virtual_path
                        && outcome.artifact_digest.as_deref()
                            == Some(document.artifact_digest.as_str())
                        && document.metrics.raw_bytes == outcome.size
                })
        }) {
            return Err(FabricationError::DanglingReference(
                "retained-manufacturing-input".into(),
            ));
        }
        if raw > self.limits.raw_bytes_aggregate
            || records > self.limits.records_aggregate
            || tokens > self.limits.lexical_tokens_aggregate
        {
            return Err(FabricationError::LimitExceeded {
                resource: "aggregate-input",
            });
        }
        if self.repetitions.iter().any(|repeat| {
            repeat.feature_ids.len() > self.limits.geometry_features
                || repeat.x_count == 0
                || repeat.y_count == 0
                || repeat.x_count > self.limits.repeat_factor
                || repeat.y_count > self.limits.repeat_factor
        }) || self.expanded_feature_instances()?
            > u64::try_from(self.limits.geometry_features)
                .map_err(|_| FabricationError::ArithmeticOverflow)?
        {
            return Err(FabricationError::LimitExceeded {
                resource: "definition-expansion",
            });
        }
        let macro_variables: usize = self.macros.iter().map(|item| item.variables.len()).sum();
        if macro_variables > self.limits.macro_variables
            || self
                .product
                .as_ref()
                .is_some_and(|product| product.provenance.len() > self.limits.geometry_features)
            || self.profile.as_ref().is_some_and(|profile| {
                profile.contour_feature_ids.len() > self.limits.geometry_features
                    || profile.cutout_feature_ids.len() > self.limits.geometry_features
                    || profile.provenance.len() > self.limits.geometry_features
            })
            || self
                .apertures
                .iter()
                .any(|item| item.dimensions.len() > self.limits.geometry_features)
            || self
                .blocks
                .iter()
                .any(|item| item.feature_ids.len() > self.limits.geometry_features)
            || self.capabilities.records.iter().any(|item| {
                item.document_ids.len() > self.limits.recognized_files
                    || item.provenance.len() > self.limits.geometry_features
            })
            || self
                .omissions
                .iter()
                .any(|item| item.affected_capabilities.len() > self.limits.geometry_features)
            || self
                .conflicts
                .iter()
                .any(|item| item.affected_capabilities.len() > self.limits.geometry_features)
            || self
                .features
                .iter()
                .any(|item| item.transforms.operations.len() > usize::from(self.limits.max_nesting))
            || self.macros.iter().any(|item| {
                item.operations.len() > self.limits.operations_per_macro
                    || item
                        .variables
                        .iter()
                        .chain(item.operations.iter())
                        .any(|text| text.len() > self.limits.max_text_bytes)
            })
        {
            return Err(FabricationError::LimitExceeded {
                resource: "definition-expansion",
            });
        }
        let vertices: usize = self
            .features
            .iter()
            .map(|item| item.geometry.vertex_count())
            .sum();
        let drill_routes = self
            .features
            .iter()
            .filter(|item| {
                matches!(
                    item.geometry,
                    Geometry::Drill(_) | Geometry::Route(_) | Geometry::Slot(_)
                )
            })
            .count();
        if vertices > self.limits.contour_vertices
            || drill_routes > self.limits.drill_route_features
            || self.all_texts().any(|text| {
                text.len() > self.limits.max_text_bytes || text.chars().any(char::is_control)
            })
        {
            return Err(FabricationError::LimitExceeded {
                resource: "canonical-model",
            });
        }
        Ok(())
    }

    fn validate_identities_and_references(&self) -> Result<(), FabricationError> {
        let mut ids = HashSet::new();
        let mut document_ids = HashSet::new();
        for document in &self.documents {
            if !lowercase_sha256(&document.artifact_digest) {
                return Err(FabricationError::InvalidDigest(
                    document.artifact_digest.clone(),
                ));
            }
            if document.id != document_id(&document.artifact_digest, document.format)? {
                return Err(FabricationError::InvalidIdentity(document.id.clone()));
            }
            insert_id(&mut ids, &document.id)?;
            document_ids.insert(document.id.as_str());
        }
        let mut layer_ids = HashSet::new();
        for layer in &self.layers {
            validate_provenance(&layer.provenance, &self.documents)?;
            if !document_ids.contains(layer.document_id.as_str())
                || layer.id != layer_id(&layer.document_id, layer.role, &layer.provenance.location)
            {
                return Err(FabricationError::InvalidIdentity(layer.id.clone()));
            }
            insert_id(&mut ids, &layer.id)?;
            layer_ids.insert(layer.id.as_str());
        }
        let mut tool_ids = HashSet::new();
        for tool in &self.tools {
            validate_provenance(&tool.provenance, &self.documents)?;
            let identity_kind = format!("{:?}:{}", tool.kind, tool.code);
            if !document_ids.contains(tool.document_id.as_str())
                || tool.id != tool_id(&tool.document_id, &identity_kind, &tool.provenance.location)
            {
                return Err(FabricationError::InvalidIdentity(tool.id.clone()));
            }
            if tool.span.as_ref().is_some_and(|span| {
                span.from_layer_id
                    .iter()
                    .chain(span.to_layer_id.iter())
                    .any(|id| !layer_ids.contains(id.as_str()))
            }) {
                return Err(FabricationError::DanglingReference(tool.id.clone()));
            }
            validate_positive_length_option(tool.diameter)?;
            insert_id(&mut ids, &tool.id)?;
            tool_ids.insert(tool.id.as_str());
        }
        let mut macro_ids = HashSet::new();
        for definition in &self.macros {
            validate_provenance(&definition.provenance, &self.documents)?;
            if definition.id
                != record_id(
                    "macro",
                    &definition.document_id,
                    &definition.provenance.location,
                )
            {
                return Err(FabricationError::InvalidIdentity(definition.id.clone()));
            }
            insert_id(&mut ids, &definition.id)?;
            macro_ids.insert(definition.id.as_str());
        }
        let mut aperture_ids = HashSet::new();
        for aperture in &self.apertures {
            validate_provenance(&aperture.provenance, &self.documents)?;
            if aperture.id
                != aperture_id(
                    &aperture.document_id,
                    aperture.shape,
                    &aperture.provenance.location,
                )
                || aperture
                    .macro_id
                    .as_deref()
                    .is_some_and(|id| !macro_ids.contains(id))
            {
                return Err(FabricationError::InvalidIdentity(aperture.id.clone()));
            }
            for dimension in &aperture.dimensions {
                validate_positive_length(*dimension)?;
            }
            if aperture.macro_arguments.len() > self.limits.macro_variables {
                return Err(FabricationError::LimitExceeded {
                    resource: "macro-arguments",
                });
            }
            let invalid_shape_fields = match aperture.shape {
                ApertureShape::Polygon => {
                    aperture.macro_id.is_some() || !aperture.macro_arguments.is_empty()
                }
                ApertureShape::Macro => {
                    aperture.polygon_vertices.is_some()
                        || aperture.polygon_rotation_microdegrees.is_some()
                }
                _ => {
                    aperture.polygon_vertices.is_some()
                        || aperture.polygon_rotation_microdegrees.is_some()
                        || aperture.macro_id.is_some()
                        || !aperture.macro_arguments.is_empty()
                }
            };
            if invalid_shape_fields
                || aperture
                    .polygon_vertices
                    .is_some_and(|vertices| !(3..=12).contains(&vertices))
            {
                return Err(FabricationError::InvalidIdentity(aperture.id.clone()));
            }
            for argument in &aperture.macro_arguments {
                validate_canonical_rational(argument)?;
            }
            insert_id(&mut ids, &aperture.id)?;
            aperture_ids.insert(aperture.id.as_str());
        }
        let mut feature_ids = HashSet::new();
        let mut transformed_geometry_was_quantized = false;
        for feature in &self.features {
            validate_provenance(&feature.provenance, &self.documents)?;
            if !document_ids.contains(feature.document_id.as_str())
                || !layer_ids.contains(feature.layer_id.as_str())
            {
                return Err(FabricationError::DanglingReference(feature.id.clone()));
            }
            if feature.id
                != feature_id(
                    &feature.document_id,
                    &feature.layer_id,
                    feature.geometry.kind(),
                    &feature.provenance.location,
                )
            {
                return Err(FabricationError::InvalidIdentity(feature.id.clone()));
            }
            if feature
                .tool_id
                .as_deref()
                .is_some_and(|id| !tool_ids.contains(id))
            {
                return Err(FabricationError::DanglingReference(feature.id.clone()));
            }
            validate_geometry(&feature.geometry, &aperture_ids, &tool_ids)?;
            transformed_geometry_was_quantized |=
                validate_transformed_geometry(&feature.geometry, &feature.transforms)?;
            insert_id(&mut ids, &feature.id)?;
            feature_ids.insert(feature.id.as_str());
        }
        for block in &self.blocks {
            validate_provenance(&block.provenance, &self.documents)?;
            if block.id != record_id("block", &block.document_id, &block.provenance.location)
                || block
                    .feature_ids
                    .iter()
                    .any(|id| !feature_ids.contains(id.as_str()))
            {
                return Err(FabricationError::DanglingReference(block.id.clone()));
            }
            insert_id(&mut ids, &block.id)?;
        }
        for repeat in &self.repetitions {
            validate_provenance(&repeat.provenance, &self.documents)?;
            if repeat.id != record_id("repeat", &repeat.document_id, &repeat.provenance.location)
                || repeat
                    .feature_ids
                    .iter()
                    .any(|id| !feature_ids.contains(id.as_str()))
            {
                return Err(FabricationError::DanglingReference(repeat.id.clone()));
            }
            validate_length(repeat.x_step)?;
            validate_length(repeat.y_step)?;
            let offset = CanonicalPoint {
                x: repeat_max_offset(repeat.x_step, repeat.x_count)?,
                y: repeat_max_offset(repeat.y_step, repeat.y_count)?,
            };
            for feature_id in &repeat.feature_ids {
                let feature = self
                    .features
                    .iter()
                    .find(|feature| feature.id == *feature_id)
                    .ok_or_else(|| FabricationError::DanglingReference(feature_id.clone()))?;
                validate_transformed_geometry_at_offset(
                    &feature.geometry,
                    &feature.transforms,
                    offset,
                )?;
            }
            insert_id(&mut ids, &repeat.id)?;
        }
        if let Some(product) = &self.product {
            if product.provenance.is_empty() {
                return Err(FabricationError::InvalidProvenance("product".into()));
            }
            for provenance in &product.provenance {
                validate_provenance(provenance, &self.documents)?;
            }
        }
        if let Some(profile) = &self.profile {
            if profile.provenance.is_empty() {
                return Err(FabricationError::InvalidProvenance("profile".into()));
            }
            if profile
                .contour_feature_ids
                .iter()
                .chain(profile.cutout_feature_ids.iter())
                .any(|id| !feature_ids.contains(id.as_str()))
            {
                return Err(FabricationError::DanglingReference("profile".into()));
            }
            if let Some(extents) = &profile.extents {
                validate_point(extents.min)?;
                validate_point(extents.max)?;
                if extents.min.x > extents.max.x || extents.min.y > extents.max.y {
                    return Err(FabricationError::InvalidIdentity("profile-extents".into()));
                }
            }
            for provenance in &profile.provenance {
                validate_provenance(provenance, &self.documents)?;
            }
        }
        for semantic in &self.connectivity {
            if !feature_ids.contains(semantic.feature_id.as_str()) {
                return Err(FabricationError::DanglingReference(
                    semantic.feature_id.clone(),
                ));
            }
            validate_provenance(&semantic.provenance, &self.documents)?;
        }
        for placement in &self.assembly.placements {
            validate_point(placement.position)?;
            validate_provenance(&placement.provenance, &self.documents)?;
        }
        if self
            .assembly
            .mask_layer_ids
            .iter()
            .chain(self.assembly.paste_layer_ids.iter())
            .any(|id| !layer_ids.contains(id.as_str()))
        {
            return Err(FabricationError::DanglingReference("assembly-layer".into()));
        }
        for layer in &self.construction.layers {
            if layer
                .layer_id
                .as_deref()
                .is_some_and(|id| !layer_ids.contains(id))
            {
                return Err(FabricationError::DanglingReference(
                    "construction-layer".into(),
                ));
            }
            validate_positive_length_option(layer.thickness)?;
            validate_provenance(&layer.provenance, &self.documents)?;
        }
        validate_positive_length_option(self.construction.total_thickness)?;
        for constraint in &self.constraints {
            validate_provenance(&constraint.provenance, &self.documents)?;
            if constraint.id
                != constraint_id(
                    &constraint.provenance.document_id,
                    constraint.kind,
                    &constraint.provenance.location,
                )
            {
                return Err(FabricationError::InvalidIdentity(constraint.id.clone()));
            }
            validate_positive_length_option(constraint.value)?;
            insert_id(&mut ids, &constraint.id)?;
        }
        if transformed_geometry_was_quantized
            && self.capabilities.records.iter().any(|capability| {
                capability.id == CapabilityId::GeometryExpanded
                    && capability.state == CapabilityState::Complete
            })
        {
            return Err(FabricationError::InvalidIdentity(
                "quantized-expanded-geometry".into(),
            ));
        }
        let mut capabilities = HashSet::new();
        for capability in &self.capabilities.records {
            if !capabilities.insert(capability.id) {
                return Err(FabricationError::DuplicateId(format!(
                    "capability:{:?}",
                    capability.id
                )));
            }
            if capability
                .document_ids
                .iter()
                .any(|id| !document_ids.contains(id.as_str()))
            {
                return Err(FabricationError::DanglingReference(format!(
                    "capability:{:?}",
                    capability.id
                )));
            }
            if matches!(
                capability.state,
                CapabilityState::Complete
                    | CapabilityState::Partial
                    | CapabilityState::Stale
                    | CapabilityState::Omitted
            ) && capability.provenance.is_empty()
            {
                return Err(FabricationError::InvalidProvenance(format!(
                    "capability:{:?}",
                    capability.id
                )));
            }
            for provenance in &capability.provenance {
                validate_provenance(provenance, &self.documents)?;
            }
        }
        for omission in &self.omissions {
            insert_id(&mut ids, &omission.id)?;
            validate_provenance(&omission.provenance, &self.documents)?;
            if omission.affected_capabilities.is_empty()
                || omission.affected_capabilities.iter().any(|id| {
                    self.capabilities
                        .records
                        .iter()
                        .find(|item| item.id == *id)
                        .is_none_or(|item| item.state == CapabilityState::Complete)
                })
            {
                return Err(FabricationError::InvalidOmission(omission.id.clone()));
            }
        }
        for capability in &self.capabilities.records {
            if capability.state == CapabilityState::Omitted
                && !self
                    .omissions
                    .iter()
                    .any(|omission| omission.affected_capabilities.contains(&capability.id))
            {
                return Err(FabricationError::InvalidOmission(format!(
                    "capability:{:?}",
                    capability.id
                )));
            }
        }
        for conflict in &self.conflicts {
            insert_id(&mut ids, &conflict.id)?;
            validate_provenance(&conflict.left.provenance, &self.documents)?;
            validate_provenance(&conflict.right.provenance, &self.documents)?;
            if conflict.affected_capabilities.is_empty()
                || conflict.affected_capabilities.iter().any(|id| {
                    !capabilities.contains(id)
                        || self
                            .capabilities
                            .records
                            .iter()
                            .find(|capability| capability.id == *id)
                            .is_none_or(|capability| capability.state == CapabilityState::Complete)
                })
                || conflict.left.canonical_value == conflict.right.canonical_value
                || conflict.left.provenance == conflict.right.provenance
            {
                return Err(FabricationError::InvalidConflict(conflict.id.clone()));
            }
        }
        for warning in &self.warnings {
            if let Some(provenance) = &warning.provenance {
                validate_provenance(provenance, &self.documents)?;
            }
        }
        Ok(())
    }

    fn estimate_allocation(&self) -> Result<u64, FabricationError> {
        let lengths = [
            serialized_len(&self.input_outcomes)?,
            serialized_len(&self.product)?,
            serialized_len(&self.documents)?,
            serialized_len(&self.layers)?,
            serialized_len(&self.tools)?,
            serialized_len(&self.apertures)?,
            serialized_len(&self.macros)?,
            serialized_len(&self.blocks)?,
            serialized_len(&self.repetitions)?,
            serialized_len(&self.features)?,
            serialized_len(&self.profile)?,
            serialized_len(&self.connectivity)?,
            serialized_len(&self.assembly)?,
            serialized_len(&self.construction)?,
            serialized_len(&self.constraints)?,
            serialized_len(&self.capabilities)?,
            serialized_len(&self.omissions)?,
            serialized_len(&self.conflicts)?,
            serialized_len(&self.warnings)?,
        ];
        let bytes = lengths.into_iter().try_fold(0_u64, |sum, length| {
            sum.checked_add(length)
                .ok_or(FabricationError::ArithmeticOverflow)
        })?;
        let feature_definitions =
            u64::try_from(self.features.len()).map_err(|_| FabricationError::ArithmeticOverflow)?;
        let additional_instances = self
            .expanded_feature_instances()?
            .checked_sub(feature_definitions)
            .ok_or(FabricationError::ArithmeticOverflow)?;
        // ponytail: compact repeats charge one u64 index; materializing analyzers must add a
        // full-feature allocation bound before cloning expanded geometry.
        let expansion_bytes = additional_instances
            .checked_mul(u64::try_from(std::mem::size_of::<u64>()).unwrap_or(8))
            .ok_or(FabricationError::ArithmeticOverflow)?;
        bytes
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(expansion_bytes))
            .and_then(|bytes| bytes.checked_add(1024))
            .ok_or(FabricationError::ArithmeticOverflow)
    }

    fn all_texts(&self) -> impl Iterator<Item = &str> {
        self.input_outcomes
            .iter()
            .map(|outcome| outcome.virtual_path.as_str())
            .chain(self.documents.iter().flat_map(|document| {
                [
                    document.virtual_path.as_str(),
                    document.adapter.as_str(),
                    document.adapter_version.as_str(),
                ]
            }))
            .chain(self.layers.iter().filter_map(|layer| layer.name.as_deref()))
            .chain(self.tools.iter().map(|tool| tool.code.as_str()))
            .chain(self.macros.iter().flat_map(|definition| {
                definition
                    .variables
                    .iter()
                    .chain(definition.operations.iter())
                    .map(String::as_str)
            }))
            .chain(self.connectivity.iter().flat_map(|semantic| {
                [
                    semantic.net.as_deref(),
                    semantic.component.as_deref(),
                    semantic.pin.as_deref(),
                ]
                .into_iter()
                .flatten()
            }))
            .chain(
                self.capabilities
                    .records
                    .iter()
                    .map(|capability| capability.detail.as_str()),
            )
            .chain(
                self.omissions
                    .iter()
                    .map(|omission| omission.detail.as_str()),
            )
            .chain(self.conflicts.iter().flat_map(|conflict| {
                [
                    conflict.left.canonical_value.as_str(),
                    conflict.right.canonical_value.as_str(),
                ]
            }))
            .chain(
                self.warnings
                    .iter()
                    .flat_map(|warning| [warning.code.as_str(), warning.message.as_str()]),
            )
    }
}

fn path_directory_depth(path: &str) -> usize {
    path.split('/').count().saturating_sub(1)
}

fn valid_virtual_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with(['/', '\\'])
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && !path
            .split('/')
            .any(|component| component.is_empty() || matches!(component, "." | ".."))
        && path.as_bytes().get(1).is_none_or(|byte| *byte != b':')
}

fn serialized_len(value: &impl Serialize) -> Result<u64, FabricationError> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| FabricationError::Serialization(error.to_string()))?;
    u64::try_from(bytes.len()).map_err(|_| FabricationError::ArithmeticOverflow)
}

fn insert_id(ids: &mut HashSet<String>, id: &str) -> Result<(), FabricationError> {
    if id.is_empty() || !ids.insert(id.to_string()) {
        return Err(FabricationError::DuplicateId(id.into()));
    }
    Ok(())
}

fn validate_provenance(
    provenance: &ManufacturingProvenance,
    documents: &[ManufacturingDocument],
) -> Result<(), FabricationError> {
    let document = documents
        .iter()
        .find(|document| document.id == provenance.document_id)
        .ok_or_else(|| FabricationError::DanglingReference(provenance.document_id.clone()))?;
    let byte_location_valid = if document.metrics.raw_bytes == 0 {
        provenance.location.byte_start == 0 && provenance.location.byte_end == 0
    } else {
        provenance.location.byte_start <= provenance.location.byte_end
            && provenance.location.byte_end < document.metrics.raw_bytes
    };
    let record_location_valid = if document.metrics.records == 0 {
        provenance.location.record == 0 && provenance.location.subrecord.is_none()
    } else {
        provenance.location.record < document.metrics.records
    };
    if provenance.artifact_digest != document.artifact_digest
        || !lowercase_sha256(&provenance.artifact_digest)
        || provenance.producer.is_empty()
        || provenance.producer_version.is_empty()
        || !byte_location_valid
        || !record_location_valid
        || provenance
            .source_lexeme
            .as_deref()
            .is_some_and(|value| value.len() > MANUFACTURING_LIMITS.max_numeric_bytes)
    {
        return Err(FabricationError::InvalidProvenance(
            provenance.document_id.clone(),
        ));
    }
    Ok(())
}

fn validate_point(point: CanonicalPoint) -> Result<(), FabricationError> {
    validate_length(point.x)?;
    validate_length(point.y)
}

fn validate_length(value: Picometres) -> Result<(), FabricationError> {
    if value.0.unsigned_abs() > MAX_COORDINATE_PM as u64 {
        Err(FabricationError::CoordinateOutOfRange)
    } else {
        Ok(())
    }
}

fn validate_canonical_rational(value: &CanonicalRational) -> Result<(), FabricationError> {
    let numerator = value
        .numerator
        .parse::<i128>()
        .map_err(|_| FabricationError::InvalidNumber)?;
    if value.numerator != numerator.to_string()
        || !(1..=1_000_000_000).contains(&value.denominator)
        || gerber_gcd(numerator.unsigned_abs(), u128::from(value.denominator)) != 1
    {
        return Err(FabricationError::InvalidNumber);
    }
    Ok(())
}

fn validate_positive_length(value: Picometres) -> Result<(), FabricationError> {
    validate_length(value)?;
    if value.0 <= 0 {
        return Err(FabricationError::CoordinateOutOfRange);
    }
    Ok(())
}

fn validate_positive_length_option(value: Option<Picometres>) -> Result<(), FabricationError> {
    if let Some(value) = value {
        validate_positive_length(value)?;
    }
    Ok(())
}

fn validate_geometry(
    geometry: &Geometry,
    aperture_ids: &HashSet<&str>,
    tool_ids: &HashSet<&str>,
) -> Result<(), FabricationError> {
    fn line(line: &CanonicalLine) -> Result<(), FabricationError> {
        validate_point(line.start)?;
        validate_point(line.end)?;
        validate_positive_length_option(line.width)
    }
    fn arc(arc: &CanonicalArc) -> Result<(), FabricationError> {
        validate_point(arc.start)?;
        validate_point(arc.end)?;
        validate_point(arc.center)?;
        validate_positive_length_option(arc.width)?;
        validate_positive_length(arc.source_resolution)
    }
    fn contour(contour: &CanonicalContour) -> Result<(), FabricationError> {
        for segment in &contour.segments {
            match segment {
                ContourSegment::Line(value) => line(value)?,
                ContourSegment::Arc(value) => arc(value)?,
            }
        }
        Ok(())
    }
    match geometry {
        Geometry::Point(point) => validate_point(*point),
        Geometry::Line(value) => line(value),
        Geometry::Arc(value) => arc(value),
        Geometry::Contour(value) => contour(value),
        Geometry::Region(value) => value.contours.iter().try_for_each(contour),
        Geometry::Flash(value) => {
            validate_point(value.position)?;
            if !aperture_ids.contains(value.aperture_id.as_str()) {
                return Err(FabricationError::DanglingReference(
                    value.aperture_id.clone(),
                ));
            }
            Ok(())
        }
        Geometry::Drill(value) => {
            validate_point(value.position)?;
            validate_positive_length(value.diameter)?;
            if !tool_ids.contains(value.tool_id.as_str()) {
                return Err(FabricationError::DanglingReference(value.tool_id.clone()));
            }
            Ok(())
        }
        Geometry::Route(value) => {
            if !tool_ids.contains(value.tool_id.as_str()) {
                return Err(FabricationError::DanglingReference(value.tool_id.clone()));
            }
            value.segments.iter().try_for_each(|segment| match segment {
                ContourSegment::Line(value) => line(value),
                ContourSegment::Arc(value) => arc(value),
            })
        }
        Geometry::Slot(value) => {
            validate_point(value.start)?;
            validate_point(value.end)?;
            validate_positive_length(value.width)?;
            if !tool_ids.contains(value.tool_id.as_str()) {
                return Err(FabricationError::DanglingReference(value.tool_id.clone()));
            }
            Ok(())
        }
    }
}

fn validate_transformed_geometry(
    geometry: &Geometry,
    transforms: &TransformChain,
) -> Result<bool, FabricationError> {
    validate_transformed_geometry_at_offset(geometry, transforms, CanonicalPoint::default())
}

fn validate_transformed_geometry_at_offset(
    geometry: &Geometry,
    transforms: &TransformChain,
    offset: CanonicalPoint,
) -> Result<bool, FabricationError> {
    fn point(
        value: CanonicalPoint,
        transforms: &TransformChain,
        offset: CanonicalPoint,
        quantized: &mut bool,
    ) -> Result<(), FabricationError> {
        let materialized = transforms.materialize(value)?;
        *quantized |= !materialized.quantization.is_empty();
        validate_point(CanonicalPoint::new(
            materialized
                .point
                .x
                .0
                .checked_add(offset.x.0)
                .ok_or(FabricationError::ArithmeticOverflow)?,
            materialized
                .point
                .y
                .0
                .checked_add(offset.y.0)
                .ok_or(FabricationError::ArithmeticOverflow)?,
        ))
    }
    fn line(
        value: &CanonicalLine,
        transforms: &TransformChain,
        offset: CanonicalPoint,
        quantized: &mut bool,
    ) -> Result<(), FabricationError> {
        point(value.start, transforms, offset, quantized)?;
        point(value.end, transforms, offset, quantized)
    }
    fn arc(
        value: &CanonicalArc,
        transforms: &TransformChain,
        offset: CanonicalPoint,
        quantized: &mut bool,
    ) -> Result<(), FabricationError> {
        point(value.start, transforms, offset, quantized)?;
        point(value.end, transforms, offset, quantized)?;
        point(value.center, transforms, offset, quantized)
    }
    fn segment(
        value: &ContourSegment,
        transforms: &TransformChain,
        offset: CanonicalPoint,
        quantized: &mut bool,
    ) -> Result<(), FabricationError> {
        match value {
            ContourSegment::Line(value) => line(value, transforms, offset, quantized),
            ContourSegment::Arc(value) => arc(value, transforms, offset, quantized),
        }
    }
    let mut quantized = !transforms
        .materialize(CanonicalPoint::default())?
        .quantization
        .is_empty();
    match geometry {
        Geometry::Point(value) => point(*value, transforms, offset, &mut quantized)?,
        Geometry::Line(value) => line(value, transforms, offset, &mut quantized)?,
        Geometry::Arc(value) => arc(value, transforms, offset, &mut quantized)?,
        Geometry::Contour(value) => {
            for value in &value.segments {
                segment(value, transforms, offset, &mut quantized)?;
            }
        }
        Geometry::Region(value) => {
            for contour in &value.contours {
                for value in &contour.segments {
                    segment(value, transforms, offset, &mut quantized)?;
                }
            }
        }
        Geometry::Flash(value) => point(value.position, transforms, offset, &mut quantized)?,
        Geometry::Drill(value) => point(value.position, transforms, offset, &mut quantized)?,
        Geometry::Route(value) => {
            for value in &value.segments {
                segment(value, transforms, offset, &mut quantized)?;
            }
        }
        Geometry::Slot(value) => {
            point(value.start, transforms, offset, &mut quantized)?;
            point(value.end, transforms, offset, &mut quantized)?;
        }
    }
    Ok(quantized)
}

fn repeat_max_offset(step: Picometres, count: u32) -> Result<Picometres, FabricationError> {
    validate_length(step)?;
    let repetitions = count
        .checked_sub(1)
        .ok_or(FabricationError::ArithmeticOverflow)?;
    let offset = i128::from(step.0)
        .checked_mul(i128::from(repetitions))
        .ok_or(FabricationError::ArithmeticOverflow)?;
    let offset =
        Picometres(i64::try_from(offset).map_err(|_| FabricationError::ArithmeticOverflow)?);
    validate_length(offset)?;
    Ok(offset)
}

fn canonical_provenance(
    provenance: &ManufacturingProvenance,
) -> (&str, &str, &str, &str, &StructuralLocation) {
    (
        &provenance.document_id,
        &provenance.artifact_digest,
        &provenance.producer,
        &provenance.producer_version,
        &provenance.location,
    )
}

fn canonical_provenances(
    provenances: &[ManufacturingProvenance],
) -> Vec<(&str, &str, &str, &str, &StructuralLocation)> {
    let mut values: Vec<_> = provenances.iter().map(canonical_provenance).collect();
    values.sort();
    values
}

fn canonical_json(label: &str, value: &impl Serialize) -> Result<String, FabricationError> {
    serde_json::to_string(&(label, value))
        .map_err(|error| FabricationError::Serialization(error.to_string()))
}

fn sha256(value: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(value.as_ref()))
}

fn lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

pub(crate) fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["status", "packageId", "modelDigest", "inputOutcomes", "product", "documents", "layers", "tools", "apertures", "macros", "blocks", "repetitions", "features", "profile", "connectivity", "assembly", "construction", "constraints", "capabilities", "omissions", "conflicts", "warnings", "limits", "estimatedAllocationBytes"],
        "properties": {
            "status": { "enum": ["not_provided", "partial", "complete", "failed"] },
            "packageId": { "type": "string", "pattern": "^package-v1-[0-9a-f]{64}$" },
            "modelDigest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
            "inputOutcomes": { "type": "array", "maxItems": MANUFACTURING_LIMITS.archive_entries, "items": { "$ref": "#/$defs/manufacturingInputOutcome" } },
            "product": { "oneOf": [{ "$ref": "#/$defs/productIdentity" }, { "type": "null" }] },
            "documents": { "type": "array", "maxItems": MANUFACTURING_LIMITS.recognized_files, "items": { "$ref": "#/$defs/manufacturingDocument" } },
            "layers": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/manufacturingLayer" } },
            "tools": { "type": "array", "maxItems": MANUFACTURING_LIMITS.apertures, "items": { "$ref": "#/$defs/manufacturingTool" } },
            "apertures": { "type": "array", "maxItems": MANUFACTURING_LIMITS.apertures, "items": { "$ref": "#/$defs/apertureDefinition" } },
            "macros": { "type": "array", "maxItems": MANUFACTURING_LIMITS.macros, "items": { "$ref": "#/$defs/macroDefinition" } },
            "blocks": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/apertureBlock" } },
            "repetitions": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/stepRepeat" } },
            "features": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/manufacturingFeature" } },
            "profile": { "oneOf": [{ "$ref": "#/$defs/boardProfile" }, { "type": "null" }] },
            "connectivity": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/objectSemantics" } },
            "assembly": { "$ref": "#/$defs/assemblyEvidence" },
            "construction": { "$ref": "#/$defs/constructionEvidence" },
            "constraints": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/manufacturingConstraint" } },
            "capabilities": { "$ref": "#/$defs/capabilityLedger" },
            "omissions": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/manufacturingOmission" } },
            "conflicts": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/manufacturingConflict" } },
            "warnings": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/manufacturingWarning" } },
            "limits": { "$ref": "#/$defs/manufacturingLimits" },
            "estimatedAllocationBytes": { "type": "integer", "minimum": 0, "maximum": MANUFACTURING_LIMITS.canonical_allocation_bytes }
        }
    })
}

pub(crate) fn schema_defs() -> Vec<(&'static str, Value)> {
    let limit_properties = serde_json::to_value(MANUFACTURING_LIMITS)
        .expect("manufacturing limits serialize")
        .as_object()
        .expect("manufacturing limits serialize as an object")
        .iter()
        .map(|(name, value)| (name.clone(), json!({ "const": value })))
        .collect::<serde_json::Map<_, _>>();
    vec![
        (
            "manufacturingInputOutcome",
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["id", "virtualPath", "artifactDigest", "kindCandidate", "size", "state", "reason"],
                "allOf": [
                    { "if": { "properties": { "state": { "const": "retained" } } }, "then": {
                        "properties": { "artifactDigest": { "type": "string", "pattern": "^[0-9a-f]{64}$" }, "reason": { "type": "null" } }
                    }, "else": {
                        "properties": { "artifactDigest": { "type": "null" }, "reason": { "enum": ["recognized_file_limit", "per_file_byte_limit", "aggregate_byte_limit", "read_failure"] } }
                    } }
                ],
                "properties": {
                    "id": { "type": "string", "pattern": "^input-v1-[0-9a-f]{64}$" },
                    "virtualPath": { "type": "string", "minLength": 1, "maxLength": MANUFACTURING_LIMITS.normalized_path_bytes },
                    "artifactDigest": { "type": ["string", "null"], "pattern": "^[0-9a-f]{64}$" },
                    "kindCandidate": { "enum": ["gerber", "excellon"] },
                    "size": { "type": "integer", "minimum": 0, "maximum": 18446744073709551615_u64 },
                    "state": { "enum": ["retained", "omitted", "failed"] },
                    "reason": { "type": ["string", "null"], "enum": ["recognized_file_limit", "per_file_byte_limit", "aggregate_byte_limit", "read_failure", null] }
                }
            }),
        ),
        (
            "documentId",
            json!({ "type": "string", "pattern": "^document-v1-[0-9a-f]{64}$" }),
        ),
        (
            "layerId",
            json!({ "type": "string", "pattern": "^layer-v1-[0-9a-f]{64}$" }),
        ),
        (
            "toolId",
            json!({ "type": "string", "pattern": "^tool-v1-[0-9a-f]{64}$" }),
        ),
        (
            "featureId",
            json!({ "type": "string", "pattern": "^feature-v1-[0-9a-f]{64}$" }),
        ),
        (
            "picometres",
            json!({ "type": "integer", "minimum": -MAX_COORDINATE_PM, "maximum": MAX_COORDINATE_PM }),
        ),
        (
            "positivePicometres",
            json!({ "type": "integer", "minimum": 1, "maximum": MAX_COORDINATE_PM }),
        ),
        (
            "canonicalRational",
            json!({ "type": "object", "additionalProperties": false, "required": ["numerator", "denominator"], "properties": {
                "numerator": { "type": "string", "maxLength": 40, "pattern": "^(0|-?[1-9][0-9]*)$" },
                "denominator": { "type": "integer", "minimum": 1, "maximum": 1000000000 }
            } }),
        ),
        (
            "authority",
            json!({ "enum": ["native_source", "explicit", "x2", "file_content", "filename_inference", "unknown"] }),
        ),
        (
            "layerPolarity",
            json!({ "enum": ["positive", "negative", "dark", "clear", "unknown"] }),
        ),
        (
            "capabilityId",
            json!({ "enum": [
                "product_identity", "document_syntax", "units_and_format", "layer_roles", "layer_order",
                "geometry_points", "geometry_lines", "geometry_arcs", "geometry_regions", "geometry_flashes", "geometry_expanded",
                "polarity", "transforms", "repetition", "apertures", "macros", "profile", "extents", "drills", "routes", "slots",
                "tools", "plating", "layer_spans", "x2_file_attributes", "x2_aperture_attributes", "x2_object_attributes",
                "connectivity", "components", "pins", "assembly", "construction", "constraints", "native_kicad_facts",
                "package_completeness", "package_reconciliation", "legacy-filename-screening", "legacy-token-screening"
            ] }),
        ),
        (
            "structuralLocation",
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["record", "subrecord", "byteStart", "byteEnd"],
                "properties": {
                    "record": { "type": "integer", "minimum": 0, "maximum": 18446744073709551615_u64 },
                    "subrecord": { "type": ["integer", "null"], "minimum": 0, "maximum": 4294967295_u32 },
                    "byteStart": { "type": "integer", "minimum": 0, "maximum": 18446744073709551615_u64 },
                    "byteEnd": { "type": "integer", "minimum": 0, "maximum": 18446744073709551615_u64 }
                }
            }),
        ),
        (
            "manufacturingProvenance",
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["documentId", "artifactDigest", "producer", "producerVersion", "location", "sourceLexeme"],
                "properties": {
                    "documentId": { "$ref": "#/$defs/documentId" },
                    "artifactDigest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                    "producer": { "type": "string", "minLength": 1, "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                    "producerVersion": { "type": "string", "minLength": 1, "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                    "location": { "$ref": "#/$defs/structuralLocation" },
                    "sourceLexeme": { "type": ["string", "null"], "maxLength": MANUFACTURING_LIMITS.max_numeric_bytes }
                }
            }),
        ),
        (
            "sourceNumericFormat",
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["unit", "integerDigits", "decimalDigits", "resolution"],
                "properties": {
                    "unit": { "enum": ["millimetre", "inch"] },
                    "integerDigits": { "type": "integer", "minimum": 1, "maximum": 255 },
                    "decimalDigits": { "type": "integer", "minimum": 0, "maximum": MANUFACTURING_LIMITS.max_decimal_places },
                    "resolution": { "$ref": "#/$defs/picometres" }
                }
            }),
        ),
        (
            "documentMetrics",
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["rawBytes", "records", "lexicalTokens", "metadataBytes", "maxLineBytes", "maxTextBytes", "maxNumericBytes", "maxNesting", "maxApertureNesting"],
                "properties": {
                    "rawBytes": { "type": "integer", "minimum": 0, "maximum": MANUFACTURING_LIMITS.raw_bytes_per_file },
                    "records": { "type": "integer", "minimum": 0, "maximum": MANUFACTURING_LIMITS.records_per_file },
                    "lexicalTokens": { "type": "integer", "minimum": 0, "maximum": MANUFACTURING_LIMITS.lexical_tokens_per_file },
                    "metadataBytes": { "type": "integer", "minimum": 0, "maximum": MANUFACTURING_LIMITS.metadata_bytes_per_file },
                    "maxLineBytes": { "type": "integer", "minimum": 0, "maximum": MANUFACTURING_LIMITS.max_line_bytes },
                    "maxTextBytes": { "type": "integer", "minimum": 0, "maximum": MANUFACTURING_LIMITS.max_text_bytes },
                    "maxNumericBytes": { "type": "integer", "minimum": 0, "maximum": MANUFACTURING_LIMITS.max_numeric_bytes },
                    "maxNesting": { "type": "integer", "minimum": 0, "maximum": MANUFACTURING_LIMITS.max_nesting },
                    "maxApertureNesting": { "type": "integer", "minimum": 0, "maximum": MANUFACTURING_LIMITS.max_aperture_nesting }
                }
            }),
        ),
        (
            "productIdentity",
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["name", "revision", "partNumber", "authority", "provenance"],
                "properties": {
                    "name": { "type": ["string", "null"], "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                    "revision": { "type": ["string", "null"], "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                    "partNumber": { "type": ["string", "null"], "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                    "authority": { "$ref": "#/$defs/authority" },
                    "provenance": { "type": "array", "minItems": 1, "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/manufacturingProvenance" } }
                }
            }),
        ),
        (
            "manufacturingLayer",
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["id", "documentId", "name", "role", "side", "context", "polarity", "order", "authority", "provenance"],
                "properties": {
                    "id": { "$ref": "#/$defs/layerId" }, "documentId": { "$ref": "#/$defs/documentId" },
                    "name": { "type": ["string", "null"], "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                    "role": { "enum": ["copper", "solder_mask", "paste", "legend", "profile", "drill_map", "route", "assembly", "fabrication_drawing", "other", "unknown"] },
                    "side": { "enum": ["top", "bottom", "inner", "both", "not_applicable", "unknown"] },
                    "context": { "enum": ["board", "coupon", "panel", "component", "other", "unknown"] },
                    "polarity": { "$ref": "#/$defs/layerPolarity" }, "order": { "type": ["integer", "null"], "minimum": -2147483648_i64, "maximum": 2147483647_i64 },
                    "authority": { "$ref": "#/$defs/authority" }, "provenance": { "$ref": "#/$defs/manufacturingProvenance" }
                }
            }),
        ),
        (
            "canonicalPoint",
            json!({ "type": "object", "additionalProperties": false, "required": ["x", "y"], "properties": { "x": { "$ref": "#/$defs/picometres" }, "y": { "$ref": "#/$defs/picometres" } } }),
        ),
        (
            "canonicalLine",
            json!({ "type": "object", "additionalProperties": false, "required": ["start", "end", "width"], "properties": { "start": { "$ref": "#/$defs/canonicalPoint" }, "end": { "$ref": "#/$defs/canonicalPoint" }, "width": { "oneOf": [{ "$ref": "#/$defs/positivePicometres" }, { "type": "null" }] } } }),
        ),
        (
            "canonicalArc",
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["start", "end", "center", "direction", "quadrant", "width", "sourceResolution"],
                "properties": {
                    "start": { "$ref": "#/$defs/canonicalPoint" }, "end": { "$ref": "#/$defs/canonicalPoint" }, "center": { "$ref": "#/$defs/canonicalPoint" },
                    "direction": { "enum": ["clockwise", "counter_clockwise"] }, "quadrant": { "enum": ["single", "multi", "unknown"] },
                    "width": { "oneOf": [{ "$ref": "#/$defs/positivePicometres" }, { "type": "null" }] }, "sourceResolution": { "$ref": "#/$defs/positivePicometres" }
                }
            }),
        ),
        (
            "contourSegment",
            json!({ "oneOf": [
                { "type": "object", "additionalProperties": false, "required": ["kind", "value"], "properties": { "kind": { "const": "line" }, "value": { "$ref": "#/$defs/canonicalLine" } } },
                { "type": "object", "additionalProperties": false, "required": ["kind", "value"], "properties": { "kind": { "const": "arc" }, "value": { "$ref": "#/$defs/canonicalArc" } } }
            ] }),
        ),
        (
            "canonicalContour",
            json!({ "type": "object", "additionalProperties": false, "required": ["segments", "closed"], "properties": { "segments": { "type": "array", "maxItems": MANUFACTURING_LIMITS.contour_vertices, "items": { "$ref": "#/$defs/contourSegment" } }, "closed": { "type": "boolean" } } }),
        ),
        (
            "canonicalRegion",
            json!({ "type": "object", "additionalProperties": false, "required": ["contours"], "properties": { "contours": { "type": "array", "maxItems": MANUFACTURING_LIMITS.contour_vertices, "items": { "$ref": "#/$defs/canonicalContour" } } } }),
        ),
        (
            "canonicalFlash",
            json!({ "type": "object", "additionalProperties": false, "required": ["position", "apertureId"], "properties": { "position": { "$ref": "#/$defs/canonicalPoint" }, "apertureId": { "type": "string", "pattern": "^aperture-v1-[0-9a-f]{64}$" } } }),
        ),
        (
            "drillFeature",
            json!({ "type": "object", "additionalProperties": false, "required": ["position", "diameter", "toolId"], "properties": { "position": { "$ref": "#/$defs/canonicalPoint" }, "diameter": { "$ref": "#/$defs/positivePicometres" }, "toolId": { "$ref": "#/$defs/toolId" } } }),
        ),
        (
            "routeFeature",
            json!({ "type": "object", "additionalProperties": false, "required": ["segments", "toolId"], "properties": { "segments": { "type": "array", "maxItems": MANUFACTURING_LIMITS.contour_vertices, "items": { "$ref": "#/$defs/contourSegment" } }, "toolId": { "$ref": "#/$defs/toolId" } } }),
        ),
        (
            "slotFeature",
            json!({ "type": "object", "additionalProperties": false, "required": ["start", "end", "width", "toolId"], "properties": { "start": { "$ref": "#/$defs/canonicalPoint" }, "end": { "$ref": "#/$defs/canonicalPoint" }, "width": { "$ref": "#/$defs/positivePicometres" }, "toolId": { "$ref": "#/$defs/toolId" } } }),
        ),
        (
            "geometry",
            json!({ "oneOf": [
                { "type": "object", "additionalProperties": false, "required": ["kind", "value"], "properties": { "kind": { "const": "point" }, "value": { "$ref": "#/$defs/canonicalPoint" } } },
                { "type": "object", "additionalProperties": false, "required": ["kind", "value"], "properties": { "kind": { "const": "line" }, "value": { "$ref": "#/$defs/canonicalLine" } } },
                { "type": "object", "additionalProperties": false, "required": ["kind", "value"], "properties": { "kind": { "const": "arc" }, "value": { "$ref": "#/$defs/canonicalArc" } } },
                { "type": "object", "additionalProperties": false, "required": ["kind", "value"], "properties": { "kind": { "const": "contour" }, "value": { "$ref": "#/$defs/canonicalContour" } } },
                { "type": "object", "additionalProperties": false, "required": ["kind", "value"], "properties": { "kind": { "const": "region" }, "value": { "$ref": "#/$defs/canonicalRegion" } } },
                { "type": "object", "additionalProperties": false, "required": ["kind", "value"], "properties": { "kind": { "const": "flash" }, "value": { "$ref": "#/$defs/canonicalFlash" } } },
                { "type": "object", "additionalProperties": false, "required": ["kind", "value"], "properties": { "kind": { "const": "drill" }, "value": { "$ref": "#/$defs/drillFeature" } } },
                { "type": "object", "additionalProperties": false, "required": ["kind", "value"], "properties": { "kind": { "const": "route" }, "value": { "$ref": "#/$defs/routeFeature" } } },
                { "type": "object", "additionalProperties": false, "required": ["kind", "value"], "properties": { "kind": { "const": "slot" }, "value": { "$ref": "#/$defs/slotFeature" } } }
            ] }),
        ),
        (
            "transformOperation",
            json!({ "oneOf": [
                { "type": "object", "additionalProperties": false, "required": ["operation", "x", "y"], "properties": { "operation": { "const": "mirror" }, "x": { "type": "boolean" }, "y": { "type": "boolean" } } },
                { "type": "object", "additionalProperties": false, "required": ["operation", "microdegrees"], "properties": { "operation": { "const": "rotate" }, "microdegrees": { "type": "integer", "minimum": -9223372036854775808_i128, "maximum": 9223372036854775807_i128 } } },
                { "type": "object", "additionalProperties": false, "required": ["operation", "numerator", "denominator"], "properties": { "operation": { "const": "scale" }, "numerator": { "type": "integer", "minimum": -9223372036854775808_i128, "maximum": 9223372036854775807_i128, "not": { "const": 0 } }, "denominator": { "type": "integer", "minimum": -9223372036854775808_i128, "maximum": 9223372036854775807_i128, "not": { "const": 0 } } } },
                { "type": "object", "additionalProperties": false, "required": ["operation", "x", "y"], "properties": { "operation": { "const": "translate" }, "x": { "$ref": "#/$defs/picometres" }, "y": { "$ref": "#/$defs/picometres" } } }
            ] }),
        ),
        (
            "transformChain",
            json!({ "type": "object", "additionalProperties": false, "required": ["operations"], "properties": { "operations": { "type": "array", "maxItems": MANUFACTURING_LIMITS.max_nesting, "items": { "$ref": "#/$defs/transformOperation" } } } }),
        ),
        (
            "layerSpan",
            json!({ "type": "object", "additionalProperties": false, "required": ["fromLayerId", "toLayerId"], "properties": {
                "fromLayerId": { "oneOf": [{ "$ref": "#/$defs/layerId" }, { "type": "null" }] }, "toLayerId": { "oneOf": [{ "$ref": "#/$defs/layerId" }, { "type": "null" }] }
            } }),
        ),
        (
            "manufacturingTool",
            json!({ "type": "object", "additionalProperties": false, "required": ["id", "documentId", "code", "kind", "diameter", "plating", "span", "provenance"], "properties": {
                "id": { "$ref": "#/$defs/toolId" }, "documentId": { "$ref": "#/$defs/documentId" }, "code": { "type": "string", "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                "kind": { "enum": ["aperture", "drill", "route", "composite", "unknown"] }, "diameter": { "oneOf": [{ "$ref": "#/$defs/positivePicometres" }, { "type": "null" }] },
                "plating": { "enum": ["plated", "non_plated", "mixed", "unknown"] }, "span": { "oneOf": [{ "$ref": "#/$defs/layerSpan" }, { "type": "null" }] }, "provenance": { "$ref": "#/$defs/manufacturingProvenance" }
            } }),
        ),
        (
            "apertureDefinition",
            json!({ "type": "object", "additionalProperties": false, "required": ["id", "documentId", "shape", "dimensions", "polygonVertices", "polygonRotationMicrodegrees", "macroId", "macroArguments", "provenance"], "properties": {
                "id": { "type": "string", "pattern": "^aperture-v1-[0-9a-f]{64}$" }, "documentId": { "$ref": "#/$defs/documentId" },
                "shape": { "enum": ["circle", "rectangle", "obround", "polygon", "macro", "block", "unknown"] },
                "dimensions": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/positivePicometres" } },
                "polygonVertices": { "type": ["integer", "null"], "minimum": 3, "maximum": 12 },
                "polygonRotationMicrodegrees": { "type": ["integer", "null"], "minimum": -9223372036854775808_i128, "maximum": 9223372036854775807_i128 },
                "macroId": { "type": ["string", "null"], "pattern": "^macro-v1-[0-9a-f]{64}$" },
                "macroArguments": { "type": "array", "maxItems": MANUFACTURING_LIMITS.macro_variables, "items": { "$ref": "#/$defs/canonicalRational" } },
                "provenance": { "$ref": "#/$defs/manufacturingProvenance" }
            } }),
        ),
        (
            "macroDefinition",
            json!({ "type": "object", "additionalProperties": false, "required": ["id", "documentId", "name", "variables", "operations", "provenance"], "properties": {
                "id": { "type": "string", "pattern": "^macro-v1-[0-9a-f]{64}$" }, "documentId": { "$ref": "#/$defs/documentId" }, "name": { "type": "string", "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                "variables": { "type": "array", "maxItems": MANUFACTURING_LIMITS.macro_variables, "items": { "type": "string", "maxLength": MANUFACTURING_LIMITS.max_text_bytes } },
                "operations": { "type": "array", "maxItems": MANUFACTURING_LIMITS.operations_per_macro, "items": { "type": "string", "maxLength": MANUFACTURING_LIMITS.max_text_bytes } },
                "provenance": { "$ref": "#/$defs/manufacturingProvenance" }
            } }),
        ),
        (
            "apertureBlock",
            json!({ "type": "object", "additionalProperties": false, "required": ["id", "documentId", "featureIds", "provenance"], "properties": {
                "id": { "type": "string", "pattern": "^block-v1-[0-9a-f]{64}$" }, "documentId": { "$ref": "#/$defs/documentId" },
                "featureIds": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/featureId" } }, "provenance": { "$ref": "#/$defs/manufacturingProvenance" }
            } }),
        ),
        (
            "stepRepeat",
            json!({ "type": "object", "description": "xCount/yCount include an offset-zero instance. A feature's first repeat reuses its one global original; later references charge full grids.", "additionalProperties": false, "required": ["id", "documentId", "featureIds", "xCount", "yCount", "xStep", "yStep", "provenance"], "properties": {
                "id": { "type": "string", "pattern": "^repeat-v1-[0-9a-f]{64}$" }, "documentId": { "$ref": "#/$defs/documentId" },
                "featureIds": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/featureId" } },
                "xCount": { "type": "integer", "minimum": 1, "maximum": MANUFACTURING_LIMITS.repeat_factor }, "yCount": { "type": "integer", "minimum": 1, "maximum": MANUFACTURING_LIMITS.repeat_factor },
                "xStep": { "$ref": "#/$defs/picometres" }, "yStep": { "$ref": "#/$defs/picometres" }, "provenance": { "$ref": "#/$defs/manufacturingProvenance" }
            } }),
        ),
        (
            "extent",
            json!({ "type": "object", "additionalProperties": false, "required": ["min", "max"], "properties": { "min": { "$ref": "#/$defs/canonicalPoint" }, "max": { "$ref": "#/$defs/canonicalPoint" } } }),
        ),
        (
            "boardProfile",
            json!({ "type": "object", "additionalProperties": false, "required": ["contourFeatureIds", "cutoutFeatureIds", "extents", "provenance"], "properties": {
                "contourFeatureIds": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/featureId" } },
                "cutoutFeatureIds": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/featureId" } },
                "extents": { "oneOf": [{ "$ref": "#/$defs/extent" }, { "type": "null" }] },
                "provenance": { "type": "array", "minItems": 1, "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/manufacturingProvenance" } }
            } }),
        ),
        (
            "objectSemantics",
            json!({ "type": "object", "additionalProperties": false, "required": ["featureId", "net", "component", "pin", "provenance"], "properties": {
                "featureId": { "$ref": "#/$defs/featureId" }, "net": { "type": ["string", "null"], "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                "component": { "type": ["string", "null"], "maxLength": MANUFACTURING_LIMITS.max_text_bytes }, "pin": { "type": ["string", "null"], "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                "provenance": { "$ref": "#/$defs/manufacturingProvenance" }
            } }),
        ),
        (
            "assemblyPlacement",
            json!({ "type": "object", "additionalProperties": false, "required": ["reference", "side", "position", "rotationMicrodegrees", "provenance"], "properties": {
                "reference": { "type": "string", "maxLength": MANUFACTURING_LIMITS.max_text_bytes }, "side": { "enum": ["top", "bottom", "inner", "both", "not_applicable", "unknown"] },
                "position": { "$ref": "#/$defs/canonicalPoint" }, "rotationMicrodegrees": { "type": "integer", "minimum": -9223372036854775808_i128, "maximum": 9223372036854775807_i128 }, "provenance": { "$ref": "#/$defs/manufacturingProvenance" }
            } }),
        ),
        (
            "assemblyEvidence",
            json!({ "type": "object", "additionalProperties": false, "required": ["placements", "maskLayerIds", "pasteLayerIds"], "properties": {
                "placements": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/assemblyPlacement" } },
                "maskLayerIds": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/layerId" } },
                "pasteLayerIds": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/layerId" } }
            } }),
        ),
        (
            "constructionLayer",
            json!({ "type": "object", "additionalProperties": false, "required": ["layerId", "material", "thickness", "authority", "provenance"], "properties": {
                "layerId": { "oneOf": [{ "$ref": "#/$defs/layerId" }, { "type": "null" }] }, "material": { "type": ["string", "null"], "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                "thickness": { "oneOf": [{ "$ref": "#/$defs/positivePicometres" }, { "type": "null" }] }, "authority": { "$ref": "#/$defs/authority" }, "provenance": { "$ref": "#/$defs/manufacturingProvenance" }
            } }),
        ),
        (
            "constructionEvidence",
            json!({ "type": "object", "additionalProperties": false, "required": ["layers", "totalThickness", "finish"], "properties": {
                "layers": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/constructionLayer" } },
                "totalThickness": { "oneOf": [{ "$ref": "#/$defs/positivePicometres" }, { "type": "null" }] }, "finish": { "type": ["string", "null"], "maxLength": MANUFACTURING_LIMITS.max_text_bytes }
            } }),
        ),
        (
            "manufacturingConstraint",
            json!({ "type": "object", "additionalProperties": false, "required": ["id", "kind", "value", "declaredValue", "authority", "provenance"], "properties": {
                "id": { "type": "string", "pattern": "^constraint-v1-[0-9a-f]{64}$" },
                "kind": { "enum": ["minimum_track_width", "minimum_clearance", "minimum_drill", "minimum_annular_ring", "copper_weight", "finished_thickness", "impedance", "material", "finish", "special_process", "other", "unknown"] },
                "value": { "oneOf": [{ "$ref": "#/$defs/positivePicometres" }, { "type": "null" }] }, "declaredValue": { "type": ["string", "null"], "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                "authority": { "$ref": "#/$defs/authority" }, "provenance": { "$ref": "#/$defs/manufacturingProvenance" }
            } }),
        ),
        (
            "conflictFact",
            json!({ "type": "object", "additionalProperties": false, "required": ["canonicalValue", "authority", "provenance"], "properties": {
                "canonicalValue": { "type": "string", "maxLength": MANUFACTURING_LIMITS.max_text_bytes }, "authority": { "$ref": "#/$defs/authority" }, "provenance": { "$ref": "#/$defs/manufacturingProvenance" }
            } }),
        ),
        (
            "manufacturingWarning",
            json!({ "type": "object", "additionalProperties": false, "required": ["code", "message", "provenance"], "properties": {
                "code": { "type": "string", "maxLength": MANUFACTURING_LIMITS.max_text_bytes }, "message": { "type": "string", "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                "provenance": { "oneOf": [{ "$ref": "#/$defs/manufacturingProvenance" }, { "type": "null" }] }
            } }),
        ),
        (
            "manufacturingDocument",
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["id", "virtualPath", "artifactDigest", "format", "adapter", "adapterVersion", "parseStatus", "numericFormat", "metrics"],
                "properties": {
                    "id": { "$ref": "#/$defs/documentId" },
                    "virtualPath": { "type": "string", "minLength": 1, "maxLength": MANUFACTURING_LIMITS.normalized_path_bytes },
                    "artifactDigest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                    "format": { "enum": ["gerber", "excellon", "kicad_pcb", "unknown"] },
                    "adapter": { "type": "string", "minLength": 1, "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                    "adapterVersion": { "type": "string", "minLength": 1, "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                    "parseStatus": { "enum": ["complete", "partial", "failed", "unsupported", "not_provided"] },
                    "numericFormat": { "oneOf": [{ "$ref": "#/$defs/sourceNumericFormat" }, { "type": "null" }] },
                    "metrics": { "$ref": "#/$defs/documentMetrics" }
                }
            }),
        ),
        (
            "manufacturingFeature",
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["id", "documentId", "layerId", "toolId", "polarity", "geometry", "transforms", "provenance"],
                "properties": {
                    "id": { "type": "string", "pattern": "^feature-v1-[0-9a-f]{64}$" },
                    "documentId": { "$ref": "#/$defs/documentId" }, "layerId": { "$ref": "#/$defs/layerId" },
                    "toolId": { "oneOf": [{ "$ref": "#/$defs/toolId" }, { "type": "null" }] },
                    "polarity": { "$ref": "#/$defs/layerPolarity" },
                    "geometry": { "$ref": "#/$defs/geometry" }, "transforms": { "$ref": "#/$defs/transformChain" }, "provenance": { "$ref": "#/$defs/manufacturingProvenance" }
                }
            }),
        ),
        (
            "capabilityRecord",
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["id", "state", "authority", "documentIds", "provenance", "detail"],
                "properties": {
                    "id": { "$ref": "#/$defs/capabilityId" },
                    "state": { "enum": ["complete", "partial", "not_provided", "unsupported", "failed", "stale", "omitted"] },
                    "authority": { "$ref": "#/$defs/authority" },
                    "documentIds": { "type": "array", "maxItems": MANUFACTURING_LIMITS.recognized_files, "uniqueItems": true, "items": { "$ref": "#/$defs/documentId" } },
                    "provenance": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/manufacturingProvenance" } },
                    "detail": { "type": "string", "maxLength": MANUFACTURING_LIMITS.max_text_bytes }
                }
            }),
        ),
        (
            "capabilityLedger",
            json!({
                "type": "object", "additionalProperties": false, "required": ["records"],
                "properties": { "records": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/capabilityRecord" } } }
            }),
        ),
        (
            "manufacturingOmission",
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["id", "kind", "affectedCapabilities", "provenance", "detail"],
                "properties": {
                    "id": { "type": "string", "minLength": 1, "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                    "kind": { "enum": ["missing_record", "missing_semantic_record", "unsupported_record", "resource_limit", "invalid_record", "unknown"] },
                    "affectedCapabilities": { "type": "array", "minItems": 1, "maxItems": MANUFACTURING_LIMITS.geometry_features, "uniqueItems": true, "items": { "$ref": "#/$defs/capabilityId" } },
                    "provenance": { "$ref": "#/$defs/manufacturingProvenance" },
                    "detail": { "type": "string", "maxLength": MANUFACTURING_LIMITS.max_text_bytes }
                }
            }),
        ),
        (
            "manufacturingConflict",
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["id", "kind", "affectedCapabilities", "left", "right"],
                "properties": {
                    "id": { "type": "string", "minLength": 1, "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                    "kind": { "enum": ["product_identity", "layer_role", "layer_order", "plating", "layer_span", "profile", "extent", "connectivity", "construction", "constraint", "other"] },
                    "affectedCapabilities": { "type": "array", "minItems": 1, "maxItems": MANUFACTURING_LIMITS.geometry_features, "uniqueItems": true, "items": { "$ref": "#/$defs/capabilityId" } },
                    "left": { "$ref": "#/$defs/conflictFact" },
                    "right": { "$ref": "#/$defs/conflictFact" }
                }
            }),
        ),
        (
            "manufacturingLimits",
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["recognizedFiles", "rawBytesPerFile", "rawBytesAggregate", "recordsPerFile", "recordsAggregate", "lexicalTokensPerFile", "lexicalTokensAggregate", "maxLineBytes", "maxTextBytes", "metadataBytesPerFile", "maxNumericBytes", "maxDecimalPlaces", "maxCoordinatePm", "maxNesting", "maxApertureNesting", "apertures", "macros", "macroVariables", "operationsPerMacro", "strictToolMax", "geometryFeatures", "contourVertices", "drillRouteFeatures", "repeatFactor", "canonicalAllocationBytes", "fileTimeoutMs", "aggregateTimeoutMs", "archiveCompressedBytes", "archiveExpandedBytes", "archiveEntries", "normalizedPathBytes", "directoryDepth"],
                "properties": limit_properties
            }),
        ),
    ]
}

// Production Gerber boundary and interpreter. The dependency parser is an accounting
// authority only; all numeric semantics below are recovered from bounded source lexemes.
pub const GERBER_ADAPTER_VERSION: &str = "0.5.0+54004bc-ratemypcb-1";
const ROUTE_FILE_FUNCTION: &str = "%TF.FileFunction,NonPlated,1,4,NPTH,Route*%";
const GERBER_FEATURE_ALLOCATION_BYTES: u64 = 512;
const GERBER_VERTEX_ALLOCATION_BYTES: u64 = 128;
// One circular flash repeated on a 475 x 883 grid reaches this conservative,
// allocation-coupled production boundary without materializing the grid.
const GERBER_EXPANDED_FEATURE_LIMIT: u64 = MANUFACTURING_LIMITS.geometry_features as u64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GerberNormalizationWarning {
    pub byte_start: usize,
    pub byte_end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GerberParserIssue {
    pub line: Option<usize>,
    pub code: &'static str,
    pub context_digest: Option<String>,
    pub resolved_route: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GerberParserAccounting {
    pub parser_results: u64,
    pub parser_successes: u64,
    pub parser_errors: u64,
    pub resolved_route_errors: u64,
    pub unaccounted_errors: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GerberAttributeKind {
    File,
    Aperture,
    Object,
    Delete,
    StandardComment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GerberAttributeEvidence {
    pub kind: GerberAttributeKind,
    pub raw: String,
    pub provenance: ManufacturingProvenance,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GerberRouteFileFunctionEvidence {
    pub fields: Vec<String>,
    pub parser_issue: GerberParserIssue,
    pub provenance: ManufacturingProvenance,
}

#[derive(Clone, Debug)]
pub struct GerberProduction {
    pub review: FabricationReview,
    pub original_digest: String,
    pub accounting: GerberParserAccounting,
    pub parser_issues: Vec<GerberParserIssue>,
    pub normalization_warnings: Vec<GerberNormalizationWarning>,
    pub attributes: Vec<GerberAttributeEvidence>,
    pub route_file_functions: Vec<GerberRouteFileFunctionEvidence>,
    pub extents: Option<Extent>,
}

#[derive(Debug)]
pub enum GerberParseError {
    Resource {
        resource: &'static str,
        observed: u64,
        limit: u64,
    },
    InvalidByte {
        offset: usize,
    },
    Framing {
        record: u64,
        reason: &'static str,
    },
    Parser {
        accounting: GerberParserAccounting,
        issues: Vec<GerberParserIssue>,
    },
    Unsupported {
        record: u64,
        command: String,
    },
    Semantic {
        record: u64,
        reason: &'static str,
    },
    Deadline {
        stage: &'static str,
    },
    Canonical(FabricationError),
}

impl std::fmt::Display for GerberParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for GerberParseError {}

#[derive(Clone, Debug)]
struct GerberFrame {
    record: u64,
    line: usize,
    byte_start: usize,
    byte_end: usize,
    parser_start: usize,
    parser_end: usize,
}

#[derive(Clone, Debug)]
pub struct GerberByteBoundary {
    original_bytes: Vec<u8>,
    pub original_digest: String,
    parser_copy: Vec<u8>,
    pub warnings: Vec<GerberNormalizationWarning>,
    pub metrics: DocumentMetrics,
    frames: Vec<GerberFrame>,
}

impl GerberByteBoundary {
    pub fn new(bytes: &[u8]) -> Result<Self, GerberParseError> {
        Self::with_timeout(
            bytes,
            Duration::from_millis(MANUFACTURING_LIMITS.file_timeout_ms),
        )
    }

    pub fn with_timeout(bytes: &[u8], timeout: Duration) -> Result<Self, GerberParseError> {
        Self::build(bytes, Instant::now(), timeout)
    }

    fn build(bytes: &[u8], started: Instant, timeout: Duration) -> Result<Self, GerberParseError> {
        let raw_bytes = u64::try_from(bytes.len()).map_err(|_| GerberParseError::Resource {
            resource: "raw-bytes",
            observed: u64::MAX,
            limit: MANUFACTURING_LIMITS.raw_bytes_per_file,
        })?;
        if raw_bytes > MANUFACTURING_LIMITS.raw_bytes_per_file {
            return Err(GerberParseError::Resource {
                resource: "raw-bytes",
                observed: raw_bytes,
                limit: MANUFACTURING_LIMITS.raw_bytes_per_file,
            });
        }
        check_gerber_deadline(started, timeout, "byte-boundary")?;

        let mut max_line_bytes = 0_usize;
        let mut line_bytes = 0_usize;
        let mut index = 0_usize;
        while index < bytes.len() {
            if index % 4_096 == 0 {
                check_gerber_deadline(started, timeout, "byte-boundary")?;
            }
            let byte = bytes[index];
            if byte == b'\r' {
                max_line_bytes = max_line_bytes.max(line_bytes);
                line_bytes = 0;
                index += usize::from(bytes.get(index + 1) == Some(&b'\n')) + 1;
                continue;
            }
            if byte == b'\n' {
                max_line_bytes = max_line_bytes.max(line_bytes);
                line_bytes = 0;
                index += 1;
                continue;
            }
            if (byte < b' ' && byte != b'\t') || byte == 0x7f {
                return Err(GerberParseError::InvalidByte { offset: index });
            }
            line_bytes = line_bytes
                .checked_add(1)
                .ok_or(GerberParseError::Resource {
                    resource: "line-bytes",
                    observed: u64::MAX,
                    limit: MANUFACTURING_LIMITS.max_line_bytes as u64,
                })?;
            if line_bytes > MANUFACTURING_LIMITS.max_line_bytes {
                return Err(GerberParseError::Resource {
                    resource: "line-bytes",
                    observed: line_bytes as u64,
                    limit: MANUFACTURING_LIMITS.max_line_bytes as u64,
                });
            }
            index += 1;
        }
        max_line_bytes = max_line_bytes.max(line_bytes);

        let mut parser_copy = Vec::with_capacity(bytes.len());
        let mut warnings = Vec::new();
        let mut frames = Vec::new();
        let mut lexical_tokens = 0_u64;
        let mut cursor = 0_usize;
        let mut line = 1_usize;
        while cursor < bytes.len() {
            check_gerber_deadline(started, timeout, "byte-framing")?;
            while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
                match bytes[cursor] {
                    b'\r' => {
                        line += 1;
                        cursor += usize::from(bytes.get(cursor + 1) == Some(&b'\n')) + 1;
                    }
                    b'\n' => {
                        line += 1;
                        cursor += 1;
                    }
                    _ => cursor += 1,
                }
            }
            if cursor == bytes.len() {
                break;
            }
            let frame_line = line;
            let start = cursor;
            if bytes[cursor] == b'%' {
                cursor += 1;
                while cursor < bytes.len() && bytes[cursor] != b'%' {
                    if bytes[cursor] == b'\r' {
                        line += 1;
                        cursor += usize::from(bytes.get(cursor + 1) == Some(&b'\n')) + 1;
                    } else {
                        if bytes[cursor] == b'\n' {
                            line += 1;
                        }
                        cursor += 1;
                    }
                }
                if cursor == bytes.len() {
                    return Err(GerberParseError::Framing {
                        record: frames.len() as u64,
                        reason: "unclosed-extended-command",
                    });
                }
                cursor += 1;
                let before_percent = bytes[start..cursor - 1]
                    .iter()
                    .rposition(|byte| !byte.is_ascii_whitespace())
                    .map(|relative| start + relative);
                if before_percent.is_none_or(|position| bytes[position] != b'*') {
                    return Err(GerberParseError::Framing {
                        record: frames.len() as u64,
                        reason: "extended-command-without-star",
                    });
                }
            } else {
                while cursor < bytes.len() && bytes[cursor] != b'*' {
                    if bytes[cursor] == b'\r' {
                        line += 1;
                        cursor += usize::from(bytes.get(cursor + 1) == Some(&b'\n')) + 1;
                        continue;
                    }
                    if bytes[cursor] == b'\n' {
                        line += 1;
                        cursor += 1;
                        continue;
                    }
                    if bytes[cursor] == b'%' {
                        return Err(GerberParseError::Framing {
                            record: frames.len() as u64,
                            reason: "percent-inside-ordinary-command",
                        });
                    }
                    cursor += 1;
                }
                if cursor == bytes.len() {
                    return Err(GerberParseError::Framing {
                        record: frames.len() as u64,
                        reason: "truncated-command",
                    });
                }
                cursor += 1;
            }
            let end = cursor;
            let mut parser_frame = Vec::with_capacity(end - start);
            let mut parser_offsets = Vec::with_capacity(end - start);
            for (relative, byte) in bytes[start..end].iter().copied().enumerate() {
                if !matches!(byte, b'\r' | b'\n') {
                    parser_frame.push(byte);
                    parser_offsets.push(start + relative);
                }
            }
            let record = frames.len() as u64;
            if record >= MANUFACTURING_LIMITS.records_per_file {
                return Err(GerberParseError::Resource {
                    resource: "commands",
                    observed: record + 1,
                    limit: MANUFACTURING_LIMITS.records_per_file,
                });
            }
            lexical_tokens = lexical_tokens
                .checked_add(count_gerber_tokens(&parser_frame)?)
                .ok_or(GerberParseError::Resource {
                    resource: "lexical-tokens",
                    observed: u64::MAX,
                    limit: MANUFACTURING_LIMITS.lexical_tokens_per_file,
                })?;
            if lexical_tokens > MANUFACTURING_LIMITS.lexical_tokens_per_file {
                return Err(GerberParseError::Resource {
                    resource: "lexical-tokens",
                    observed: lexical_tokens,
                    limit: MANUFACTURING_LIMITS.lexical_tokens_per_file,
                });
            }

            if parser_frame.iter().any(|byte| *byte >= 0x80) {
                let ordinary_comment = parser_frame.starts_with(b"G04 ")
                    && !parser_frame.starts_with(b"G04 #@!")
                    && parser_frame.ends_with(b"*")
                    && parser_frame.iter().filter(|byte| **byte == b'*').count() == 1
                    && !parser_frame.contains(&b'%');
                if !ordinary_comment {
                    let relative = parser_frame
                        .iter()
                        .position(|byte| *byte >= 0x80)
                        .unwrap_or(0);
                    return Err(GerberParseError::InvalidByte {
                        offset: parser_offsets[relative],
                    });
                }
                let mut invalid = 4_usize;
                while invalid < parser_frame.len() - 1 {
                    if parser_frame[invalid] < 0x80 {
                        invalid += 1;
                        continue;
                    }
                    let warning_start = parser_offsets[invalid];
                    let mut warning_end = warning_start + 1;
                    while invalid < parser_frame.len() - 1
                        && parser_frame[invalid] >= 0x80
                        && parser_offsets[invalid] <= warning_end
                    {
                        parser_frame[invalid] = b'?';
                        warning_end = parser_offsets[invalid] + 1;
                        invalid += 1;
                    }
                    warnings.push(GerberNormalizationWarning {
                        byte_start: warning_start,
                        byte_end: warning_end,
                    });
                }
            }
            let parser_start = parser_copy.len();
            parser_copy.extend_from_slice(&parser_frame);
            let parser_end = parser_copy.len();
            frames.push(GerberFrame {
                record,
                line: frame_line,
                byte_start: start,
                byte_end: end,
                parser_start,
                parser_end,
            });
        }
        if parser_copy.iter().any(|byte| *byte >= 0x80) {
            let offset = parser_copy
                .iter()
                .position(|byte| *byte >= 0x80)
                .unwrap_or(0);
            return Err(GerberParseError::InvalidByte { offset });
        }
        check_gerber_deadline(started, timeout, "byte-boundary")?;
        Ok(Self {
            original_bytes: bytes.to_vec(),
            original_digest: sha256(bytes),
            parser_copy,
            warnings,
            metrics: DocumentMetrics {
                raw_bytes,
                records: frames.len() as u64,
                lexical_tokens,
                max_line_bytes,
                ..DocumentMetrics::default()
            },
            frames,
        })
    }

    pub fn original_bytes(&self) -> &[u8] {
        &self.original_bytes
    }

    pub fn parser_bytes(&self) -> &[u8] {
        &self.parser_copy
    }

    fn frame_text(&self, frame: &GerberFrame) -> &str {
        std::str::from_utf8(&self.parser_copy[frame.parser_start..frame.parser_end])
            .expect("Gerber parser copy is ASCII")
    }
}

fn count_gerber_tokens(bytes: &[u8]) -> Result<u64, GerberParseError> {
    let mut count = 0_u64;
    let mut in_word = false;
    for byte in bytes.iter().copied() {
        let word = byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'$');
        if word {
            if !in_word {
                count = count.checked_add(1).ok_or(GerberParseError::Resource {
                    resource: "lexical-tokens",
                    observed: u64::MAX,
                    limit: MANUFACTURING_LIMITS.lexical_tokens_per_file,
                })?;
            }
        } else if !byte.is_ascii_whitespace() && !matches!(byte, b'%' | b'*' | b',') {
            count = count.checked_add(1).ok_or(GerberParseError::Resource {
                resource: "lexical-tokens",
                observed: u64::MAX,
                limit: MANUFACTURING_LIMITS.lexical_tokens_per_file,
            })?;
        }
        in_word = word;
    }
    Ok(count)
}

fn check_gerber_deadline(
    started: Instant,
    timeout: Duration,
    stage: &'static str,
) -> Result<(), GerberParseError> {
    if timeout.is_zero() || started.elapsed() >= timeout {
        Err(GerberParseError::Deadline { stage })
    } else {
        Ok(())
    }
}

struct GerberDeadlineReader<'a> {
    cursor: Cursor<&'a [u8]>,
    started: Instant,
    timeout: Duration,
}

impl Read for GerberDeadlineReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.timeout.is_zero() || self.started.elapsed() >= self.timeout {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Gerber parser deadline",
            ));
        }
        self.cursor.read(buffer)
    }
}

fn content_error_code(error: &ContentError) -> &'static str {
    match error {
        ContentError::UnknownCommand { .. } => "unknown-command",
        ContentError::UnsupportedCommand { .. } => "unsupported-command",
        ContentError::NoEndOfFile => "missing-m02",
        ContentError::InvalidParameter { .. } => "invalid-parameter",
        ContentError::UnsupportedFileAttribute { .. } => "unsupported-file-attribute",
        ContentError::InvalidFileAttribute { .. } => "invalid-file-attribute",
        ContentError::InvalidApertureAttribute { .. } => "invalid-aperture-attribute",
        ContentError::UnsupportedApertureAttribute { .. } => "unsupported-aperture-attribute",
        ContentError::UnsupportedObjectAttribute { .. } => "unsupported-object-attribute",
        ContentError::InvalidDeleteAttribute { .. } => "invalid-delete-attribute",
        ContentError::InvalidMacroDefinition(_) => "invalid-macro",
        ContentError::UnsupportedMacroDefinition => "unsupported-macro",
        ContentError::NoEndOfLine { .. } => "unterminated-command",
        ContentError::CoordinateDataWithoutOperationCode => "undefined-modal-operation",
        ContentError::IoError(_) => "parser-io",
        _ => "parser-content-error",
    }
}

fn parser_issue(error: &gerber_parser::GerberParserErrorWithContext) -> GerberParserIssue {
    GerberParserIssue {
        line: error.line.as_ref().map(|(line, _)| *line),
        code: content_error_code(&error.error),
        context_digest: error.line.as_ref().map(|(_, context)| sha256(context)),
        resolved_route: false,
    }
}

fn gerber_provenance_for(
    document_id: &str,
    artifact_digest: &str,
    frame: &GerberFrame,
) -> ManufacturingProvenance {
    ManufacturingProvenance {
        document_id: document_id.into(),
        artifact_digest: artifact_digest.into(),
        producer: "gerber-parser-ratemypcb".into(),
        producer_version: GERBER_ADAPTER_VERSION.into(),
        location: StructuralLocation {
            record: frame.record,
            subrecord: None,
            byte_start: frame.byte_start as u64,
            byte_end: frame.byte_end.saturating_sub(1) as u64,
        },
        source_lexeme: None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParserResultKind {
    Comment,
    EndOfFile,
    Region,
    Quadrant,
    Interpolation,
    Move,
    Flash,
    Draw,
    DeprecatedSelect,
    ApertureSelect,
    CoordinateFormat,
    Unit,
    ApertureDefinition,
    ApertureMacro,
    Polarity,
    Mirroring,
    Rotation,
    Scaling,
    StepRepeat,
    ApertureBlock,
    FileAttribute,
    ApertureAttribute,
    ObjectAttribute,
    DeleteAttribute,
    Unsupported,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ParserExpectation {
    first: ParserResultKind,
    second: Option<ParserResultKind>,
}

impl ParserExpectation {
    const fn one(first: ParserResultKind) -> Self {
        Self {
            first,
            second: None,
        }
    }

    const fn two(first: ParserResultKind, second: ParserResultKind) -> Self {
        Self {
            first,
            second: Some(second),
        }
    }

    fn kinds(self) -> impl Iterator<Item = ParserResultKind> {
        [Some(self.first), self.second].into_iter().flatten()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParserStreamToken {
    Sentinel(u64),
    Result {
        document_index: usize,
        kind: ParserResultKind,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ReconciledParserGroup {
    first: usize,
    second: Option<usize>,
}

fn operation_result_kind(command: &str) -> ParserResultKind {
    if command.ends_with("D02") || command.ends_with("D2") {
        ParserResultKind::Move
    } else if command.ends_with("D03") || command.ends_with("D3") {
        ParserResultKind::Flash
    } else {
        ParserResultKind::Draw
    }
}

fn expected_parser_group(text: &str) -> ParserExpectation {
    if let Some(command) = text.strip_suffix('*') {
        if command.starts_with("G04") {
            return ParserExpectation::one(ParserResultKind::Comment);
        }
        if command == "M02" {
            return ParserExpectation::one(ParserResultKind::EndOfFile);
        }
        if matches!(command, "G36" | "G37") {
            return ParserExpectation::one(ParserResultKind::Region);
        }
        if matches!(command, "G74" | "G75") {
            return ParserExpectation::one(ParserResultKind::Quadrant);
        }
        for prefix in ["G01", "G02", "G03"] {
            if let Some(operation) = command.strip_prefix(prefix) {
                return if operation.is_empty() {
                    ParserExpectation::one(ParserResultKind::Interpolation)
                } else {
                    ParserExpectation::two(
                        ParserResultKind::Interpolation,
                        operation_result_kind(operation),
                    )
                };
            }
        }
        if command.starts_with("G54D") {
            return ParserExpectation::two(
                ParserResultKind::DeprecatedSelect,
                ParserResultKind::ApertureSelect,
            );
        }
        if command.starts_with('D') {
            return ParserExpectation::one(ParserResultKind::ApertureSelect);
        }
        if command.starts_with(['X', 'Y']) {
            return ParserExpectation::one(operation_result_kind(command));
        }
        return ParserExpectation::one(ParserResultKind::Unsupported);
    }

    let body = text
        .strip_prefix('%')
        .and_then(|body| body.strip_suffix("*%"))
        .unwrap_or(text);
    let kind = if body.starts_with("FS") {
        ParserResultKind::CoordinateFormat
    } else if body.starts_with("MO") {
        ParserResultKind::Unit
    } else if body.starts_with("ADD") {
        ParserResultKind::ApertureDefinition
    } else if body.starts_with("AM") {
        ParserResultKind::ApertureMacro
    } else if body.starts_with("LP") {
        ParserResultKind::Polarity
    } else if body.starts_with("LM") {
        ParserResultKind::Mirroring
    } else if body.starts_with("LR") {
        ParserResultKind::Rotation
    } else if body.starts_with("LS") {
        ParserResultKind::Scaling
    } else if body.starts_with("SR") {
        ParserResultKind::StepRepeat
    } else if body.starts_with("AB") {
        ParserResultKind::ApertureBlock
    } else if body.starts_with("TF") {
        ParserResultKind::FileAttribute
    } else if body.starts_with("TA") {
        ParserResultKind::ApertureAttribute
    } else if body.starts_with("TO") {
        ParserResultKind::ObjectAttribute
    } else if body.starts_with("TD") {
        ParserResultKind::DeleteAttribute
    } else {
        ParserResultKind::Unsupported
    };
    ParserExpectation::one(kind)
}

fn parser_result_kind(command: &ParserCommand) -> ParserResultKind {
    match command {
        ParserCommand::FunctionCode(ParserFunctionCode::GCode(ParserGCode::Comment(_))) => {
            ParserResultKind::Comment
        }
        ParserCommand::FunctionCode(ParserFunctionCode::MCode(ParserMCode::EndOfFile)) => {
            ParserResultKind::EndOfFile
        }
        ParserCommand::FunctionCode(ParserFunctionCode::GCode(ParserGCode::RegionMode(_))) => {
            ParserResultKind::Region
        }
        ParserCommand::FunctionCode(ParserFunctionCode::GCode(ParserGCode::QuadrantMode(_))) => {
            ParserResultKind::Quadrant
        }
        ParserCommand::FunctionCode(ParserFunctionCode::GCode(ParserGCode::InterpolationMode(
            _,
        ))) => ParserResultKind::Interpolation,
        ParserCommand::FunctionCode(ParserFunctionCode::DCode(ParserDCode::Operation(
            ParserOperation::Move(_),
        ))) => ParserResultKind::Move,
        ParserCommand::FunctionCode(ParserFunctionCode::DCode(ParserDCode::Operation(
            ParserOperation::Flash(_),
        ))) => ParserResultKind::Flash,
        ParserCommand::FunctionCode(ParserFunctionCode::DCode(ParserDCode::Operation(
            ParserOperation::Interpolate(_, _),
        ))) => ParserResultKind::Draw,
        ParserCommand::FunctionCode(ParserFunctionCode::GCode(ParserGCode::SelectAperture)) => {
            ParserResultKind::DeprecatedSelect
        }
        ParserCommand::FunctionCode(ParserFunctionCode::DCode(ParserDCode::SelectAperture(_))) => {
            ParserResultKind::ApertureSelect
        }
        ParserCommand::ExtendedCode(ParserExtendedCode::CoordinateFormat(_)) => {
            ParserResultKind::CoordinateFormat
        }
        ParserCommand::ExtendedCode(ParserExtendedCode::Unit(_)) => ParserResultKind::Unit,
        ParserCommand::ExtendedCode(ParserExtendedCode::ApertureDefinition(_)) => {
            ParserResultKind::ApertureDefinition
        }
        ParserCommand::ExtendedCode(ParserExtendedCode::ApertureMacro(_)) => {
            ParserResultKind::ApertureMacro
        }
        ParserCommand::ExtendedCode(ParserExtendedCode::LoadPolarity(_)) => {
            ParserResultKind::Polarity
        }
        ParserCommand::ExtendedCode(ParserExtendedCode::LoadMirroring(_)) => {
            ParserResultKind::Mirroring
        }
        ParserCommand::ExtendedCode(ParserExtendedCode::LoadRotation(_)) => {
            ParserResultKind::Rotation
        }
        ParserCommand::ExtendedCode(ParserExtendedCode::LoadScaling(_)) => {
            ParserResultKind::Scaling
        }
        ParserCommand::ExtendedCode(ParserExtendedCode::StepAndRepeat(_)) => {
            ParserResultKind::StepRepeat
        }
        ParserCommand::ExtendedCode(ParserExtendedCode::ApertureBlock(_)) => {
            ParserResultKind::ApertureBlock
        }
        ParserCommand::ExtendedCode(ParserExtendedCode::FileAttribute(_)) => {
            ParserResultKind::FileAttribute
        }
        ParserCommand::ExtendedCode(ParserExtendedCode::ApertureAttribute(_)) => {
            ParserResultKind::ApertureAttribute
        }
        ParserCommand::ExtendedCode(ParserExtendedCode::ObjectAttribute(_)) => {
            ParserResultKind::ObjectAttribute
        }
        ParserCommand::ExtendedCode(ParserExtendedCode::DeleteAttribute(_)) => {
            ParserResultKind::DeleteAttribute
        }
        _ => ParserResultKind::Unsupported,
    }
}

fn parser_sentinel(command: &ParserCommand, marker: &str) -> Option<u64> {
    let ParserCommand::FunctionCode(ParserFunctionCode::GCode(ParserGCode::Comment(
        ParserCommentContent::String(comment),
    ))) = command
    else {
        return None;
    };
    comment.strip_prefix(marker)?.parse().ok()
}

fn reconcile_parser_stream(
    expectations: &[ParserExpectation],
    tokens: &[ParserStreamToken],
) -> Result<Vec<ReconciledParserGroup>, &'static str> {
    let mut cursor = 0_usize;
    let mut groups = Vec::with_capacity(expectations.len());
    for (frame_index, expectation) in expectations.iter().copied().enumerate() {
        if tokens.get(cursor) != Some(&ParserStreamToken::Sentinel(frame_index as u64)) {
            return Err("missing-or-reordered-frame-sentinel");
        }
        cursor += 1;
        let mut indexes = [None, None];
        for (result_index, expected) in expectation.kinds().enumerate() {
            let Some(ParserStreamToken::Result {
                document_index,
                kind,
            }) = tokens.get(cursor).copied()
            else {
                return Err("missing-frame-parser-result");
            };
            if kind != expected && kind != ParserResultKind::Error {
                return Err("mismatched-frame-parser-result");
            }
            indexes[result_index] = Some(document_index);
            cursor += 1;
        }
        if matches!(tokens.get(cursor), Some(ParserStreamToken::Result { .. })) {
            return Err("extra-frame-parser-result");
        }
        groups.push(ReconciledParserGroup {
            first: indexes[0].expect("every frame expects a result"),
            second: indexes[1],
        });
    }
    if cursor != tokens.len() {
        return Err("extra-or-reordered-parser-group");
    }
    Ok(groups)
}

#[cfg(test)]
mod gerber_parser_reconciliation_tests {
    use super::*;

    fn result(document_index: usize, kind: ParserResultKind) -> ParserStreamToken {
        ParserStreamToken::Result {
            document_index,
            kind,
        }
    }

    #[test]
    fn expansion_weights_pass_exact_limits_and_reject_one_over() {
        let circular = GerberExpansionWeight::single(1).unwrap();
        let exact_features = circular
            .checked_mul(GERBER_EXPANDED_FEATURE_LIMIT, 0)
            .unwrap();
        let over_features = exact_features.checked_add(circular, 0).unwrap();
        assert_eq!(exact_features.enforce().unwrap(), exact_features);
        assert!(matches!(
            over_features.enforce(),
            Err(GerberParseError::Resource {
                resource: "expanded-features",
                observed,
                limit,
            }) if observed == GERBER_EXPANDED_FEATURE_LIMIT + 1
                && limit == GERBER_EXPANDED_FEATURE_LIMIT
        ));

        let exact_vertices =
            GerberExpansionWeight::single(MANUFACTURING_LIMITS.contour_vertices as u64).unwrap();
        let over_vertices =
            GerberExpansionWeight::single(MANUFACTURING_LIMITS.contour_vertices as u64 + 1)
                .unwrap();
        assert_eq!(exact_vertices.enforce().unwrap(), exact_vertices);
        assert!(matches!(
            over_vertices.enforce(),
            Err(GerberParseError::Resource {
                resource: "expanded-vertices",
                ..
            })
        ));

        let allocation_features = 400_000_u64;
        let allocation_vertices = (MANUFACTURING_LIMITS.canonical_allocation_bytes
            - allocation_features * GERBER_FEATURE_ALLOCATION_BYTES)
            / GERBER_VERTEX_ALLOCATION_BYTES;
        let exact_allocation = GerberExpansionWeight {
            features: allocation_features,
            vertices: allocation_vertices,
            allocation: MANUFACTURING_LIMITS.canonical_allocation_bytes,
        };
        let over_allocation = GerberExpansionWeight {
            vertices: allocation_vertices + 1,
            allocation: MANUFACTURING_LIMITS.canonical_allocation_bytes
                + GERBER_VERTEX_ALLOCATION_BYTES,
            ..exact_allocation
        };
        assert_eq!(exact_allocation.enforce().unwrap(), exact_allocation);
        assert!(matches!(
            over_allocation.enforce(),
            Err(GerberParseError::Resource {
                resource: "expanded-allocation",
                observed,
                limit,
            }) if observed == limit + GERBER_VERTEX_ALLOCATION_BYTES
        ));
        assert_eq!(
            GerberExpansionWeight {
                features: 1,
                vertices: 2,
                allocation: 3,
            }
            .checked_mul(10, 0)
            .unwrap()
            .checked_mul(20, 0)
            .unwrap(),
            GerberExpansionWeight {
                features: 200,
                vertices: 400,
                allocation: 600,
            }
        );
    }

    #[test]
    fn per_file_parser_record_limit_is_exact_and_over() {
        enforce_parser_record_limit(MANUFACTURING_LIMITS.records_per_file).unwrap();
        assert!(matches!(
            enforce_parser_record_limit(MANUFACTURING_LIMITS.records_per_file + 1),
            Err(GerberParseError::Resource {
                resource: "parser-records",
                ..
            })
        ));
    }

    #[test]
    fn aggregate_byte_command_record_and_token_limits_are_exact_and_over() {
        for (bytes, commands, records, tokens, resource) in [
            (
                MANUFACTURING_LIMITS.raw_bytes_aggregate,
                0,
                0,
                0,
                "aggregate-bytes",
            ),
            (
                0,
                MANUFACTURING_LIMITS.records_aggregate,
                0,
                0,
                "aggregate-commands",
            ),
            (
                0,
                0,
                MANUFACTURING_LIMITS.records_aggregate,
                0,
                "aggregate-records",
            ),
            (
                0,
                0,
                0,
                MANUFACTURING_LIMITS.lexical_tokens_aggregate,
                "aggregate-tokens",
            ),
        ] {
            let mut exact = GerberAggregateAccounting::default();
            exact.add(bytes, commands, records, tokens).unwrap();
            let mut over = exact;
            let increment = match resource {
                "aggregate-bytes" => (1, 0, 0, 0),
                "aggregate-commands" => (0, 1, 0, 0),
                "aggregate-records" => (0, 0, 1, 0),
                "aggregate-tokens" => (0, 0, 0, 1),
                _ => unreachable!(),
            };
            assert!(matches!(
                over.add(increment.0, increment.1, increment.2, increment.3),
                Err(GerberParseError::Resource { resource: actual, .. }) if actual == resource
            ));
        }
    }

    #[test]
    fn aggregate_real_deadline_bomb_is_nonzero_and_typed() {
        let bytes = b"%FSLAX46Y46*%%MOMM*%M02*".to_vec();
        let digest = sha256(&bytes);
        let size = bytes.len() as u64;
        let path = "deadline.gbr".to_string();
        let input = ManufacturingInput {
            virtual_path: path.clone(),
            artifact_digest: digest.clone(),
            kind_candidate: ManufacturingKindCandidate::Gerber,
            size: bytes.len() as u64,
            original_bytes: bytes,
        };
        let inventory = ManufacturingInventory {
            inputs: vec![input],
            outcomes: vec![ManufacturingInputOutcome {
                id: input_outcome_id(&path, Some(&digest), ManufacturingKindCandidate::Gerber),
                virtual_path: path,
                artifact_digest: Some(digest),
                kind_candidate: ManufacturingKindCandidate::Gerber,
                size,
                state: ManufacturingLoadState::Retained,
                reason: None,
            }],
        };
        assert_eq!(
            parse_gerber_inventory_with_timeout(&inventory, Duration::from_secs(1))
                .unwrap()
                .len(),
            1
        );
        let mut bomb_bytes = b"%FSLAX46Y46*%%MOMM*%".to_vec();
        bomb_bytes.extend_from_slice(b"G04 aggregate-deadline*\n".repeat(100_000).as_slice());
        bomb_bytes.extend_from_slice(b"M02*");
        let bomb_digest = sha256(&bomb_bytes);
        let bomb_path = "aggregate-deadline.gbr".to_string();
        let bomb_size = bomb_bytes.len() as u64;
        let bomb = ManufacturingInventory {
            inputs: vec![ManufacturingInput {
                virtual_path: bomb_path.clone(),
                artifact_digest: bomb_digest.clone(),
                kind_candidate: ManufacturingKindCandidate::Gerber,
                size: bomb_size,
                original_bytes: bomb_bytes,
            }],
            outcomes: vec![ManufacturingInputOutcome {
                id: input_outcome_id(
                    &bomb_path,
                    Some(&bomb_digest),
                    ManufacturingKindCandidate::Gerber,
                ),
                virtual_path: bomb_path,
                artifact_digest: Some(bomb_digest),
                kind_candidate: ManufacturingKindCandidate::Gerber,
                size: bomb_size,
                state: ManufacturingLoadState::Retained,
                reason: None,
            }],
        };
        assert!(matches!(
            parse_gerber_inventory_with_timeout(&bomb, Duration::from_micros(50)),
            Err(GerberParseError::Deadline { stage: "aggregate" })
        ));
    }

    #[test]
    fn shared_deadline_cannot_reset_after_boundary_construction() {
        let bytes = b"%FSLAX46Y46*%%MOMM*%M02*".to_vec();
        let input = ManufacturingInput {
            virtual_path: "post-boundary-deadline.gbr".into(),
            artifact_digest: sha256(&bytes),
            size: bytes.len() as u64,
            original_bytes: bytes,
            kind_candidate: ManufacturingKindCandidate::Gerber,
        };
        let timeout = Duration::from_millis(100);

        let parser_started = Instant::now();
        let boundary =
            GerberByteBoundary::build(&input.original_bytes, parser_started, timeout).unwrap();
        assert!(parser_started.elapsed() < timeout);
        std::thread::sleep(timeout);
        assert!(matches!(
            parse_gerber_document_after_boundary(&input, boundary, parser_started, timeout),
            Err(GerberParseError::Deadline {
                stage: "parser-reconciliation"
            })
        ));

        let interpreter_started = Instant::now();
        let boundary =
            GerberByteBoundary::build(&input.original_bytes, interpreter_started, timeout).unwrap();
        let document_id = document_id(&boundary.original_digest, DocumentFormat::Gerber).unwrap();
        let (accounting, issues, routes) =
            account_gerber_parser(&boundary, &document_id, interpreter_started, timeout).unwrap();
        assert!(interpreter_started.elapsed() < timeout);
        std::thread::sleep(timeout);
        assert!(matches!(
            GerberInterpreter::new(
                &input,
                boundary,
                accounting,
                issues,
                routes,
                interpreter_started,
                timeout,
            )
            .run(),
            Err(GerberParseError::Deadline {
                stage: "interpretation"
            })
        ));
    }

    #[test]
    fn injected_missing_extra_reordered_and_mismatched_groups_fail() {
        let expectations = [
            ParserExpectation::two(ParserResultKind::Interpolation, ParserResultKind::Draw),
            ParserExpectation::two(
                ParserResultKind::DeprecatedSelect,
                ParserResultKind::ApertureSelect,
            ),
        ];
        let valid = [
            ParserStreamToken::Sentinel(0),
            result(1, ParserResultKind::Interpolation),
            result(2, ParserResultKind::Draw),
            ParserStreamToken::Sentinel(1),
            result(4, ParserResultKind::DeprecatedSelect),
            result(5, ParserResultKind::ApertureSelect),
        ];
        assert!(reconcile_parser_stream(&expectations, &valid).is_ok());

        let mut missing = valid.to_vec();
        missing.remove(2);
        assert_eq!(
            reconcile_parser_stream(&expectations, &missing),
            Err("missing-frame-parser-result")
        );

        let mut extra = valid.to_vec();
        extra.insert(3, result(3, ParserResultKind::Draw));
        assert_eq!(
            reconcile_parser_stream(&expectations, &extra),
            Err("extra-frame-parser-result")
        );

        let mut reordered = valid.to_vec();
        reordered[0] = ParserStreamToken::Sentinel(1);
        assert_eq!(
            reconcile_parser_stream(&expectations, &reordered),
            Err("missing-or-reordered-frame-sentinel")
        );

        let mut mismatched = valid.to_vec();
        mismatched[2] = result(2, ParserResultKind::Flash);
        assert_eq!(
            reconcile_parser_stream(&expectations, &mismatched),
            Err("mismatched-frame-parser-result")
        );
    }
}

fn reconciliation_marker(boundary: &GerberByteBoundary) -> String {
    let digest_prefix = &boundary.original_digest[..16];
    for nonce in 0_u64.. {
        let marker = format!("RMP{digest_prefix}{nonce:x}:");
        if boundary
            .frames
            .iter()
            .all(|frame| !boundary.frame_text(frame).contains(&marker))
        {
            return marker;
        }
    }
    unreachable!("u64 sentinel nonce space is exhaustive")
}

fn enforce_parser_record_limit(observed: u64) -> Result<(), GerberParseError> {
    if observed > MANUFACTURING_LIMITS.records_per_file {
        Err(GerberParseError::Resource {
            resource: "parser-records",
            observed,
            limit: MANUFACTURING_LIMITS.records_per_file,
        })
    } else {
        Ok(())
    }
}

fn account_gerber_parser(
    boundary: &GerberByteBoundary,
    document_id: &str,
    started: Instant,
    timeout: Duration,
) -> Result<
    (
        GerberParserAccounting,
        Vec<GerberParserIssue>,
        Vec<GerberRouteFileFunctionEvidence>,
    ),
    GerberParseError,
> {
    check_gerber_deadline(started, timeout, "parser-reconciliation")?;
    let marker = reconciliation_marker(boundary);
    let marker_bytes = marker.len() as u64;
    let sentinel_bytes = u64::try_from(boundary.frames.len())
        .ok()
        .and_then(|frames| frames.checked_mul(marker_bytes.saturating_add(32)))
        .and_then(|bytes| bytes.checked_add(boundary.parser_copy.len() as u64))
        .ok_or(GerberParseError::Resource {
            resource: "parser-reconciliation-allocation",
            observed: u64::MAX,
            limit: MANUFACTURING_LIMITS.canonical_allocation_bytes,
        })?;
    if sentinel_bytes > MANUFACTURING_LIMITS.canonical_allocation_bytes {
        return Err(GerberParseError::Resource {
            resource: "parser-reconciliation-allocation",
            observed: sentinel_bytes,
            limit: MANUFACTURING_LIMITS.canonical_allocation_bytes,
        });
    }
    let mut augmented = Vec::with_capacity(sentinel_bytes as usize);
    let mut expectations = Vec::with_capacity(boundary.frames.len());
    for frame in &boundary.frames {
        augmented.extend_from_slice(format!("G04 {marker}{}*", frame.record).as_bytes());
        let text = boundary.frame_text(frame);
        expectations.push(expected_parser_group(text));
        augmented.extend_from_slice(text.as_bytes());
    }

    let reader = GerberDeadlineReader {
        cursor: Cursor::new(augmented.as_slice()),
        started,
        timeout,
    };
    let (document, fatal) = match parse_gerber(BufReader::new(reader)) {
        Ok(document) => (document, false),
        Err((document, _)) => (document, true),
    };
    check_gerber_deadline(started, timeout, "parser-reconciliation")?;

    let mut tokens = Vec::with_capacity(document.commands.len());
    for (document_index, result) in document.commands.iter().enumerate() {
        match result {
            Ok(command) => {
                if let Some(frame) = parser_sentinel(command, &marker) {
                    tokens.push(ParserStreamToken::Sentinel(frame));
                } else {
                    tokens.push(ParserStreamToken::Result {
                        document_index,
                        kind: parser_result_kind(command),
                    });
                }
            }
            Err(_) => tokens.push(ParserStreamToken::Result {
                document_index,
                kind: ParserResultKind::Error,
            }),
        }
    }
    let groups = match reconcile_parser_stream(&expectations, &tokens) {
        Ok(groups) => groups,
        Err(_) => {
            let mut accounting = GerberParserAccounting::default();
            for token in &tokens {
                if let ParserStreamToken::Result { document_index, .. } = *token {
                    accounting.parser_results += 1;
                    if document.commands[document_index].is_ok() {
                        accounting.parser_successes += 1;
                    } else {
                        accounting.parser_errors += 1;
                    }
                }
            }
            accounting.unaccounted_errors = 1;
            return Err(GerberParseError::Parser {
                accounting,
                issues: vec![GerberParserIssue {
                    line: None,
                    code: "parser-frame-result-reconciliation",
                    context_digest: None,
                    resolved_route: false,
                }],
            });
        }
    };

    let mut accounting = GerberParserAccounting::default();
    let mut issues = Vec::new();
    let mut routes = Vec::new();
    for (frame, group) in boundary.frames.iter().zip(groups) {
        for document_index in [Some(group.first), group.second].into_iter().flatten() {
            accounting.parser_results += 1;
            match &document.commands[document_index] {
                Ok(_) => accounting.parser_successes += 1,
                Err(error) => {
                    accounting.parser_errors += 1;
                    let frame_text = boundary.frame_text(frame);
                    let exact_route = frame_text == ROUTE_FILE_FUNCTION
                        && group.second.is_none()
                        && matches!(
                            &error.error,
                            ContentError::InvalidParameter { parameter } if parameter == "Route"
                        )
                        && error
                            .line
                            .as_ref()
                            .is_some_and(|(_, line)| line == ROUTE_FILE_FUNCTION)
                        && routes.is_empty();
                    let mut issue = parser_issue(error);
                    issue.line = Some(frame.line);
                    issue.context_digest = Some(sha256(frame_text));
                    if exact_route {
                        issue.resolved_route = true;
                        accounting.resolved_route_errors += 1;
                        routes.push(GerberRouteFileFunctionEvidence {
                            fields: ROUTE_FILE_FUNCTION
                                .trim_start_matches('%')
                                .trim_end_matches("*%")
                                .split(',')
                                .map(str::to_owned)
                                .collect(),
                            parser_issue: issue.clone(),
                            provenance: gerber_provenance_for(
                                document_id,
                                &boundary.original_digest,
                                frame,
                            ),
                        });
                    } else {
                        accounting.unaccounted_errors += 1;
                    }
                    issues.push(issue);
                }
            }
        }
    }
    if fatal {
        accounting.unaccounted_errors += 1;
        issues.push(GerberParserIssue {
            line: None,
            code: "fatal-parser-io",
            context_digest: None,
            resolved_route: false,
        });
    }
    let route_frames = boundary
        .frames
        .iter()
        .filter(|frame| boundary.frame_text(frame) == ROUTE_FILE_FUNCTION)
        .count();
    if route_frames != routes.len() {
        accounting.unaccounted_errors += 1;
        issues.push(GerberParserIssue {
            line: boundary
                .frames
                .iter()
                .find(|frame| boundary.frame_text(frame) == ROUTE_FILE_FUNCTION)
                .map(|frame| frame.line),
            code: "route-parser-evidence-mismatch",
            context_digest: Some(sha256(ROUTE_FILE_FUNCTION)),
            resolved_route: false,
        });
    }
    enforce_parser_record_limit(accounting.parser_results)?;
    if accounting.unaccounted_errors != 0 {
        return Err(GerberParseError::Parser { accounting, issues });
    }
    Ok((accounting, issues, routes))
}

pub fn parse_gerber_document(
    input: &ManufacturingInput,
) -> Result<GerberProduction, GerberParseError> {
    parse_gerber_document_with_timeout(
        input,
        Duration::from_millis(MANUFACTURING_LIMITS.file_timeout_ms),
    )
}

pub fn parse_gerber_document_with_timeout(
    input: &ManufacturingInput,
    timeout: Duration,
) -> Result<GerberProduction, GerberParseError> {
    parse_gerber_document_started(input, Instant::now(), timeout)
        .map(|(production, _, _)| production)
}

fn parse_gerber_document_started(
    input: &ManufacturingInput,
    started: Instant,
    timeout: Duration,
) -> Result<(GerberProduction, u64, u64), GerberParseError> {
    if input.kind_candidate != ManufacturingKindCandidate::Gerber
        || input.size != input.original_bytes.len() as u64
        || input.size > MANUFACTURING_LIMITS.raw_bytes_per_file
        || input.artifact_digest != sha256(&input.original_bytes)
    {
        return Err(GerberParseError::Semantic {
            record: 0,
            reason: "invalid-gerber-input-identity",
        });
    }
    if !valid_virtual_path(&input.virtual_path) {
        return Err(GerberParseError::Semantic {
            record: 0,
            reason: "invalid-gerber-virtual-path",
        });
    }
    let boundary = GerberByteBoundary::build(&input.original_bytes, started, timeout)?;
    let source_records = boundary.metrics.records;
    let lexical_tokens = boundary.metrics.lexical_tokens;
    let production = parse_gerber_document_after_boundary(input, boundary, started, timeout)?;
    Ok((production, source_records, lexical_tokens))
}

fn parse_gerber_document_after_boundary(
    input: &ManufacturingInput,
    boundary: GerberByteBoundary,
    started: Instant,
    timeout: Duration,
) -> Result<GerberProduction, GerberParseError> {
    let document_id = document_id(&boundary.original_digest, DocumentFormat::Gerber)
        .map_err(GerberParseError::Canonical)?;
    let (accounting, issues, routes) =
        account_gerber_parser(&boundary, &document_id, started, timeout)?;
    GerberInterpreter::new(
        input, boundary, accounting, issues, routes, started, timeout,
    )
    .run()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct GerberAggregateAccounting {
    bytes: u64,
    commands: u64,
    records: u64,
    tokens: u64,
}

impl GerberAggregateAccounting {
    fn add(
        &mut self,
        bytes: u64,
        commands: u64,
        records: u64,
        tokens: u64,
    ) -> Result<(), GerberParseError> {
        for (value, increment, resource, limit) in [
            (
                &mut self.bytes,
                bytes,
                "aggregate-bytes",
                MANUFACTURING_LIMITS.raw_bytes_aggregate,
            ),
            (
                &mut self.commands,
                commands,
                "aggregate-commands",
                MANUFACTURING_LIMITS.records_aggregate,
            ),
            (
                &mut self.records,
                records,
                "aggregate-records",
                MANUFACTURING_LIMITS.records_aggregate,
            ),
            (
                &mut self.tokens,
                tokens,
                "aggregate-tokens",
                MANUFACTURING_LIMITS.lexical_tokens_aggregate,
            ),
        ] {
            *value = value
                .checked_add(increment)
                .ok_or(GerberParseError::Resource {
                    resource,
                    observed: u64::MAX,
                    limit,
                })?;
            if *value > limit {
                return Err(GerberParseError::Resource {
                    resource,
                    observed: *value,
                    limit,
                });
            }
        }
        Ok(())
    }
}

pub fn parse_gerber_inventory(
    inventory: &ManufacturingInventory,
) -> Result<Vec<GerberProduction>, GerberParseError> {
    parse_gerber_inventory_with_timeout(
        inventory,
        Duration::from_millis(MANUFACTURING_LIMITS.aggregate_timeout_ms),
    )
}

fn parse_gerber_inventory_with_timeout(
    inventory: &ManufacturingInventory,
    timeout: Duration,
) -> Result<Vec<GerberProduction>, GerberParseError> {
    let aggregate_started = Instant::now();
    inventory.validate().map_err(GerberParseError::Canonical)?;
    check_gerber_deadline(aggregate_started, timeout, "aggregate")?;
    let file_timeout = Duration::from_millis(MANUFACTURING_LIMITS.file_timeout_ms);
    let mut aggregate = GerberAggregateAccounting::default();
    let mut result = Vec::new();
    for input in inventory
        .inputs
        .iter()
        .filter(|input| input.kind_candidate == ManufacturingKindCandidate::Gerber)
    {
        check_gerber_deadline(aggregate_started, timeout, "aggregate")?;
        let file_started = Instant::now();
        let aggregate_remaining =
            timeout.saturating_sub(file_started.duration_since(aggregate_started));
        let aggregate_limited = aggregate_remaining <= file_timeout;
        let effective_timeout = aggregate_remaining.min(file_timeout);
        let (parsed, source_records, lexical_tokens) =
            match parse_gerber_document_started(input, file_started, effective_timeout) {
                Err(GerberParseError::Deadline { .. }) if aggregate_limited => {
                    return Err(GerberParseError::Deadline { stage: "aggregate" });
                }
                outcome => outcome?,
            };
        check_gerber_deadline(aggregate_started, timeout, "aggregate")?;
        aggregate.add(
            input.size,
            source_records,
            parsed.accounting.parser_results,
            lexical_tokens,
        )?;
        result.push(parsed);
    }
    check_gerber_deadline(aggregate_started, timeout, "aggregate")?;
    Ok(result)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GerberRational {
    numerator: i128,
    denominator: i128,
}

impl GerberRational {
    fn new(numerator: i128, denominator: i128) -> Result<Self, &'static str> {
        if denominator == 0 {
            return Err("division-by-zero");
        }
        let mut numerator = numerator;
        let mut denominator = denominator;
        if denominator < 0 {
            numerator = numerator.checked_neg().ok_or("numeric-overflow")?;
            denominator = denominator.checked_neg().ok_or("numeric-overflow")?;
        }
        let divisor = gerber_gcd(numerator.unsigned_abs(), denominator as u128) as i128;
        Ok(Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        })
    }

    fn parse(value: &str) -> Result<Self, &'static str> {
        if value.is_empty() || value.len() > MANUFACTURING_LIMITS.max_numeric_bytes {
            return Err("invalid-number");
        }
        let (negative, unsigned) = match value.as_bytes()[0] {
            b'-' => (true, &value[1..]),
            b'+' => (false, &value[1..]),
            _ => (false, value),
        };
        let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
        if whole.is_empty() && fraction.is_empty()
            || fraction.len() > usize::from(MANUFACTURING_LIMITS.max_decimal_places)
            || !whole.bytes().all(|byte| byte.is_ascii_digit())
            || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err("invalid-number");
        }
        let mut numerator = 0_i128;
        for byte in whole.bytes().chain(fraction.bytes()) {
            numerator = numerator
                .checked_mul(10)
                .and_then(|value| value.checked_add(i128::from(byte - b'0')))
                .ok_or("numeric-overflow")?;
        }
        if negative {
            numerator = numerator.checked_neg().ok_or("numeric-overflow")?;
        }
        let denominator = 10_i128
            .checked_pow(fraction.len() as u32)
            .ok_or("numeric-overflow")?;
        Self::new(numerator, denominator)
    }

    fn canonical(&self) -> Result<CanonicalRational, &'static str> {
        Ok(CanonicalRational {
            numerator: self.numerator.to_string(),
            denominator: u64::try_from(self.denominator).map_err(|_| "invalid-denominator")?,
        })
    }

    fn checked_add(self, other: Self) -> Result<Self, &'static str> {
        Self::new(
            self.numerator
                .checked_mul(other.denominator)
                .and_then(|left| {
                    other
                        .numerator
                        .checked_mul(self.denominator)
                        .and_then(|right| left.checked_add(right))
                })
                .ok_or("numeric-overflow")?,
            self.denominator
                .checked_mul(other.denominator)
                .ok_or("numeric-overflow")?,
        )
    }

    fn checked_sub(self, other: Self) -> Result<Self, &'static str> {
        Self::new(
            self.numerator
                .checked_mul(other.denominator)
                .and_then(|left| {
                    other
                        .numerator
                        .checked_mul(self.denominator)
                        .and_then(|right| left.checked_sub(right))
                })
                .ok_or("numeric-overflow")?,
            self.denominator
                .checked_mul(other.denominator)
                .ok_or("numeric-overflow")?,
        )
    }

    fn checked_mul(self, other: Self) -> Result<Self, &'static str> {
        Self::new(
            self.numerator
                .checked_mul(other.numerator)
                .ok_or("numeric-overflow")?,
            self.denominator
                .checked_mul(other.denominator)
                .ok_or("numeric-overflow")?,
        )
    }

    fn checked_div(self, other: Self) -> Result<Self, &'static str> {
        Self::new(
            self.numerator
                .checked_mul(other.denominator)
                .ok_or("numeric-overflow")?,
            self.denominator
                .checked_mul(other.numerator)
                .ok_or("numeric-overflow")?,
        )
    }

    fn checked_neg(self) -> Result<Self, &'static str> {
        Self::new(
            self.numerator.checked_neg().ok_or("numeric-overflow")?,
            self.denominator,
        )
    }

    fn exact_i64(self) -> Result<i64, &'static str> {
        if self.numerator % self.denominator != 0 {
            return Err("expected-integer");
        }
        i64::try_from(self.numerator / self.denominator).map_err(|_| "numeric-overflow")
    }

    fn to_picometres(self, unit: SourceUnit) -> Result<Picometres, &'static str> {
        let factor = match unit {
            SourceUnit::Millimetre => 1_000_000_000_i128,
            SourceUnit::Inch => 25_400_000_000_i128,
        };
        let numerator = self
            .numerator
            .checked_mul(factor)
            .ok_or("numeric-overflow")?;
        if numerator % self.denominator != 0 {
            return Err("finer-than-picometre");
        }
        let value = i64::try_from(numerator / self.denominator).map_err(|_| "numeric-overflow")?;
        if value.unsigned_abs() > MAX_COORDINATE_PM as u64 {
            return Err("coordinate-out-of-range");
        }
        Ok(Picometres(value))
    }

    fn to_microdegrees(self) -> Result<i64, &'static str> {
        let numerator = self
            .numerator
            .checked_mul(1_000_000)
            .ok_or("numeric-overflow")?;
        if numerator % self.denominator != 0 {
            return Err("angle-finer-than-microdegree");
        }
        i64::try_from(numerator / self.denominator).map_err(|_| "numeric-overflow")
    }
}

fn gerber_gcd(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left.max(1)
}

#[derive(Clone, Copy)]
struct GerberExpressionValue(Option<GerberRational>);

struct GerberExpressionParser<'a> {
    source: &'a [u8],
    position: usize,
    variables: &'a BTreeMap<u32, GerberRational>,
    max_depth: u8,
}

impl<'a> GerberExpressionParser<'a> {
    fn parse(
        source: &'a str,
        variables: &'a BTreeMap<u32, GerberRational>,
    ) -> Result<(Option<GerberRational>, u8), &'static str> {
        let mut parser = Self {
            source: source.as_bytes(),
            position: 0,
            variables,
            max_depth: 0,
        };
        let value = parser.expression(0)?;
        parser.skip_spaces();
        if parser.position != parser.source.len() {
            return Err("invalid-expression");
        }
        Ok((value.0, parser.max_depth))
    }

    fn expression(&mut self, depth: u8) -> Result<GerberExpressionValue, &'static str> {
        let mut value = self.term(depth)?;
        loop {
            self.skip_spaces();
            let operation = self.source.get(self.position).copied();
            if !matches!(operation, Some(b'+') | Some(b'-')) {
                return Ok(value);
            }
            self.position += 1;
            let right = self.term(depth)?;
            value = match (value.0, right.0, operation) {
                (Some(left), Some(right), Some(b'+')) => {
                    GerberExpressionValue(Some(left.checked_add(right)?))
                }
                (Some(left), Some(right), Some(b'-')) => {
                    GerberExpressionValue(Some(left.checked_sub(right)?))
                }
                _ => GerberExpressionValue(None),
            };
        }
    }

    fn term(&mut self, depth: u8) -> Result<GerberExpressionValue, &'static str> {
        let mut value = self.factor(depth)?;
        loop {
            self.skip_spaces();
            let operation = self.source.get(self.position).copied();
            if !matches!(operation, Some(b'x') | Some(b'/')) {
                return Ok(value);
            }
            self.position += 1;
            let right = self.factor(depth)?;
            value = match (value.0, right.0, operation) {
                (Some(left), Some(right), Some(b'x')) => {
                    GerberExpressionValue(Some(left.checked_mul(right)?))
                }
                (Some(left), Some(right), Some(b'/')) => {
                    GerberExpressionValue(Some(left.checked_div(right)?))
                }
                _ => GerberExpressionValue(None),
            };
        }
    }

    fn factor(&mut self, depth: u8) -> Result<GerberExpressionValue, &'static str> {
        self.skip_spaces();
        if depth >= MANUFACTURING_LIMITS.max_nesting {
            return Err("expression-nesting-limit");
        }
        self.max_depth = self.max_depth.max(depth + 1);
        match self.source.get(self.position).copied() {
            Some(b'+') => {
                self.position += 1;
                self.factor(depth + 1)
            }
            Some(b'-') => {
                self.position += 1;
                let value = self.factor(depth + 1)?;
                Ok(GerberExpressionValue(
                    value.0.map(GerberRational::checked_neg).transpose()?,
                ))
            }
            Some(b'(') => {
                self.position += 1;
                let value = self.expression(depth + 1)?;
                self.skip_spaces();
                if self.source.get(self.position) != Some(&b')') {
                    return Err("unclosed-expression");
                }
                self.position += 1;
                Ok(value)
            }
            Some(b'$') => {
                self.position += 1;
                let start = self.position;
                while self
                    .source
                    .get(self.position)
                    .is_some_and(u8::is_ascii_digit)
                {
                    self.position += 1;
                }
                if start == self.position {
                    return Err("invalid-variable");
                }
                let number = std::str::from_utf8(&self.source[start..self.position])
                    .map_err(|_| "invalid-variable")?
                    .parse::<u32>()
                    .map_err(|_| "invalid-variable")?;
                Ok(GerberExpressionValue(self.variables.get(&number).copied()))
            }
            Some(byte) if byte.is_ascii_digit() || byte == b'.' => {
                let start = self.position;
                while self
                    .source
                    .get(self.position)
                    .is_some_and(|byte| byte.is_ascii_digit() || *byte == b'.')
                {
                    self.position += 1;
                }
                let number = std::str::from_utf8(&self.source[start..self.position])
                    .map_err(|_| "invalid-number")?;
                Ok(GerberExpressionValue(Some(GerberRational::parse(number)?)))
            }
            _ => Err("invalid-expression"),
        }
    }

    fn skip_spaces(&mut self) {
        while self
            .source
            .get(self.position)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.position += 1;
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct GerberBounds {
    min_x: Option<i64>,
    min_y: Option<i64>,
    max_x: Option<i64>,
    max_y: Option<i64>,
}

impl GerberBounds {
    fn include(&mut self, point: CanonicalPoint) {
        self.min_x = Some(self.min_x.map_or(point.x.0, |value| value.min(point.x.0)));
        self.min_y = Some(self.min_y.map_or(point.y.0, |value| value.min(point.y.0)));
        self.max_x = Some(self.max_x.map_or(point.x.0, |value| value.max(point.x.0)));
        self.max_y = Some(self.max_y.map_or(point.y.0, |value| value.max(point.y.0)));
    }

    fn include_box(
        &mut self,
        min_x: i64,
        min_y: i64,
        max_x: i64,
        max_y: i64,
    ) -> Result<(), &'static str> {
        for point in [
            CanonicalPoint::new(min_x, min_y),
            CanonicalPoint::new(max_x, max_y),
        ] {
            if point.x.0.unsigned_abs() > MAX_COORDINATE_PM as u64
                || point.y.0.unsigned_abs() > MAX_COORDINATE_PM as u64
            {
                return Err("coordinate-out-of-range");
            }
            self.include(point);
        }
        Ok(())
    }

    fn merge(&mut self, other: Self) {
        if let (Some(min_x), Some(min_y), Some(max_x), Some(max_y)) =
            (other.min_x, other.min_y, other.max_x, other.max_y)
        {
            self.include(CanonicalPoint::new(min_x, min_y));
            self.include(CanonicalPoint::new(max_x, max_y));
        }
    }

    fn translated(self, x: i64, y: i64) -> Result<Self, &'static str> {
        match (self.min_x, self.min_y, self.max_x, self.max_y) {
            (Some(min_x), Some(min_y), Some(max_x), Some(max_y)) => {
                let mut result = Self::default();
                result.include_box(
                    min_x.checked_add(x).ok_or("numeric-overflow")?,
                    min_y.checked_add(y).ok_or("numeric-overflow")?,
                    max_x.checked_add(x).ok_or("numeric-overflow")?,
                    max_y.checked_add(y).ok_or("numeric-overflow")?,
                )?;
                Ok(result)
            }
            _ => Ok(Self::default()),
        }
    }

    fn extent(self) -> Option<Extent> {
        Some(Extent {
            min: CanonicalPoint::new(self.min_x?, self.min_y?),
            max: CanonicalPoint::new(self.max_x?, self.max_y?),
        })
    }
}

#[derive(Clone)]
struct GerberMacroDefinitionInternal {
    id: String,
    operations: Vec<String>,
    variables: Vec<u32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct GerberExpansionWeight {
    features: u64,
    vertices: u64,
    allocation: u64,
}

impl GerberExpansionWeight {
    fn single(vertices: u64) -> Result<Self, GerberParseError> {
        Ok(Self {
            features: 1,
            vertices,
            allocation: GERBER_FEATURE_ALLOCATION_BYTES
                .checked_add(vertices.checked_mul(GERBER_VERTEX_ALLOCATION_BYTES).ok_or(
                    GerberParseError::Semantic {
                        record: 0,
                        reason: "expansion-weight-overflow",
                    },
                )?)
                .ok_or(GerberParseError::Semantic {
                    record: 0,
                    reason: "expansion-weight-overflow",
                })?,
        })
    }

    fn checked_add(self, other: Self, record: u64) -> Result<Self, GerberParseError> {
        Ok(Self {
            features: self.features.checked_add(other.features).ok_or(
                GerberParseError::Semantic {
                    record,
                    reason: "expansion-weight-overflow",
                },
            )?,
            vertices: self.vertices.checked_add(other.vertices).ok_or(
                GerberParseError::Semantic {
                    record,
                    reason: "expansion-weight-overflow",
                },
            )?,
            allocation: self.allocation.checked_add(other.allocation).ok_or(
                GerberParseError::Semantic {
                    record,
                    reason: "expansion-weight-overflow",
                },
            )?,
        })
    }

    fn checked_mul(self, factor: u64, record: u64) -> Result<Self, GerberParseError> {
        Ok(Self {
            features: self
                .features
                .checked_mul(factor)
                .ok_or(GerberParseError::Semantic {
                    record,
                    reason: "expansion-weight-overflow",
                })?,
            vertices: self
                .vertices
                .checked_mul(factor)
                .ok_or(GerberParseError::Semantic {
                    record,
                    reason: "expansion-weight-overflow",
                })?,
            allocation: self
                .allocation
                .checked_mul(factor)
                .ok_or(GerberParseError::Semantic {
                    record,
                    reason: "expansion-weight-overflow",
                })?,
        })
    }

    fn enforce(self) -> Result<Self, GerberParseError> {
        for (resource, observed, limit) in [
            (
                "expanded-features",
                self.features,
                GERBER_EXPANDED_FEATURE_LIMIT,
            ),
            (
                "expanded-vertices",
                self.vertices,
                MANUFACTURING_LIMITS.contour_vertices as u64,
            ),
            (
                "expanded-allocation",
                self.allocation,
                MANUFACTURING_LIMITS.canonical_allocation_bytes,
            ),
        ] {
            if observed > limit {
                return Err(GerberParseError::Resource {
                    resource,
                    observed,
                    limit,
                });
            }
        }
        Ok(self)
    }
}

#[derive(Clone)]
struct GerberApertureInternal {
    id: String,
    tool_id: String,
    width: Option<Picometres>,
    bounds: GerberBounds,
    block_depth: u8,
    zero_size: bool,
    expansion: GerberExpansionWeight,
}

struct GerberRegionBuild {
    start: GerberFrame,
    contours: Vec<CanonicalContour>,
    segments: Vec<ContourSegment>,
    contour_start: Option<CanonicalPoint>,
    polarity: LayerPolarity,
    transforms: TransformChain,
}

struct GerberRepeatBuild {
    start: GerberFrame,
    feature_start: usize,
    x_count: u32,
    y_count: u32,
    x_step: Picometres,
    y_step: Picometres,
}

#[derive(Clone, Copy)]
struct GerberModalState {
    position: CanonicalPoint,
    interpolation: Option<ArcDirection>,
    linear: bool,
    quadrant: QuadrantMode,
    selected: Option<i32>,
    last_operation: Option<u8>,
}

struct GerberBlockBuild {
    start: GerberFrame,
    code: i32,
    feature_start: usize,
    saved: GerberModalState,
    max_child_depth: u8,
}

struct GerberInterpreter<'a> {
    input: &'a ManufacturingInput,
    boundary: GerberByteBoundary,
    accounting: GerberParserAccounting,
    parser_issues: Vec<GerberParserIssue>,
    routes: Vec<GerberRouteFileFunctionEvidence>,
    started: Instant,
    timeout: Duration,
    document_id: String,
    layer_id: String,
    unit: Option<SourceUnit>,
    format_digits: Option<(u8, u8)>,
    format: Option<SourceNumericFormat>,
    coordinate_mode_absolute: bool,
    zero_omission_trailing: bool,
    position: CanonicalPoint,
    interpolation: Option<ArcDirection>,
    linear: bool,
    quadrant: QuadrantMode,
    selected: Option<i32>,
    last_operation: Option<u8>,
    polarity: LayerPolarity,
    mirror_x: bool,
    mirror_y: bool,
    rotation_microdegrees: i64,
    scale_numerator: i64,
    scale_denominator: i64,
    terminated: bool,
    region: Option<GerberRegionBuild>,
    repeat: Option<GerberRepeatBuild>,
    block: Option<GerberBlockBuild>,
    apertures: BTreeMap<i32, GerberApertureInternal>,
    aperture_codes_by_id: BTreeMap<String, i32>,
    macro_definitions: BTreeMap<String, GerberMacroDefinitionInternal>,
    tools: Vec<ManufacturingTool>,
    aperture_facts: Vec<ApertureDefinition>,
    macros: Vec<MacroDefinition>,
    blocks: Vec<ApertureBlock>,
    repetitions: Vec<StepRepeat>,
    features: Vec<ManufacturingFeature>,
    attributes: Vec<GerberAttributeEvidence>,
    warnings: Vec<ManufacturingWarning>,
    bounds: GerberBounds,
    metadata_bytes: u64,
    max_text_bytes: usize,
    max_numeric_bytes: usize,
    max_nesting: u8,
    max_aperture_nesting: u8,
    vertices: usize,
    feature_weights: Vec<GerberExpansionWeight>,
    expanded_weight: GerberExpansionWeight,
}

#[derive(Default)]
struct GerberOperationFields<'a> {
    x: Option<&'a str>,
    y: Option<&'a str>,
    i: Option<&'a str>,
    j: Option<&'a str>,
    operation: Option<u8>,
}

impl<'a> GerberInterpreter<'a> {
    fn new(
        input: &'a ManufacturingInput,
        boundary: GerberByteBoundary,
        accounting: GerberParserAccounting,
        parser_issues: Vec<GerberParserIssue>,
        routes: Vec<GerberRouteFileFunctionEvidence>,
        started: Instant,
        timeout: Duration,
    ) -> Self {
        let document_id = document_id(&boundary.original_digest, DocumentFormat::Gerber)
            .expect("validated original digest");
        let first = boundary
            .frames
            .first()
            .expect("parser accepted a nonempty document");
        let first_provenance =
            gerber_provenance_for(&document_id, &boundary.original_digest, first);
        let layer_id = layer_id(&document_id, LayerRole::Unknown, &first_provenance.location);
        Self {
            input,
            boundary,
            accounting,
            parser_issues,
            routes,
            started,
            timeout,
            document_id,
            layer_id,
            unit: None,
            format_digits: None,
            format: None,
            coordinate_mode_absolute: true,
            zero_omission_trailing: false,
            position: CanonicalPoint::default(),
            interpolation: None,
            linear: true,
            quadrant: QuadrantMode::Unknown,
            selected: None,
            last_operation: None,
            polarity: LayerPolarity::Dark,
            mirror_x: false,
            mirror_y: false,
            rotation_microdegrees: 0,
            scale_numerator: 1,
            scale_denominator: 1,
            terminated: false,
            region: None,
            repeat: None,
            block: None,
            apertures: BTreeMap::new(),
            aperture_codes_by_id: BTreeMap::new(),
            macro_definitions: BTreeMap::new(),
            tools: Vec::new(),
            aperture_facts: Vec::new(),
            macros: Vec::new(),
            blocks: Vec::new(),
            repetitions: Vec::new(),
            features: Vec::new(),
            attributes: Vec::new(),
            warnings: Vec::new(),
            bounds: GerberBounds::default(),
            metadata_bytes: 0,
            max_text_bytes: 0,
            max_numeric_bytes: 0,
            max_nesting: 0,
            max_aperture_nesting: 0,
            vertices: 0,
            feature_weights: Vec::new(),
            expanded_weight: GerberExpansionWeight::default(),
        }
    }

    fn run(mut self) -> Result<GerberProduction, GerberParseError> {
        let frames = self.boundary.frames.clone();
        for frame in &frames {
            self.deadline("interpretation")?;
            if self.terminated {
                return Err(self.semantic(frame.record, "data-after-m02"));
            }
            let text = self.boundary.frame_text(frame).to_owned();
            if text.starts_with('%') {
                self.handle_extended(frame, &text)?;
            } else {
                self.handle_normal(frame, &text)?;
            }
        }
        if !self.terminated {
            return Err(self.semantic(frames.len() as u64, "missing-m02"));
        }
        if self.region.is_some() || self.repeat.is_some() || self.block.is_some() {
            return Err(self.semantic(frames.len() as u64, "unclosed-definition"));
        }
        let unit = self.unit.ok_or_else(|| self.semantic(0, "missing-mo"))?;
        let format = self
            .format
            .clone()
            .ok_or_else(|| self.semantic(0, "missing-fs"))?;
        debug_assert_eq!(format.unit, unit);
        self.deadline("canonicalization")?;

        for warning in &self.boundary.warnings {
            let frame = frames
                .iter()
                .find(|frame| {
                    warning.byte_start >= frame.byte_start && warning.byte_end <= frame.byte_end
                })
                .expect("normalization warning belongs to one frame");
            let mut provenance = self.provenance(frame);
            provenance.location.byte_start = warning.byte_start as u64;
            provenance.location.byte_end = warning.byte_end.saturating_sub(1) as u64;
            self.warnings.push(ManufacturingWarning {
                code: "gerber-comment-byte-normalized".into(),
                message: "Invalid bytes were replaced only in the parser copy of an ordinary G04 comment.".into(),
                provenance: Some(provenance),
            });
        }

        let first = frames.first().expect("nonempty accepted document");
        let first_provenance = self.provenance(first);
        let mut metrics = self.boundary.metrics.clone();
        metrics.records = self.accounting.parser_results;
        metrics.metadata_bytes = self.metadata_bytes;
        metrics.max_text_bytes = self.max_text_bytes;
        metrics.max_numeric_bytes = self.max_numeric_bytes;
        metrics.max_nesting = self.max_nesting;
        metrics.max_aperture_nesting = self.max_aperture_nesting;
        let document = ManufacturingDocument {
            id: self.document_id.clone(),
            virtual_path: self.input.virtual_path.clone(),
            artifact_digest: self.boundary.original_digest.clone(),
            format: DocumentFormat::Gerber,
            adapter: "gerber-parser-ratemypcb".into(),
            adapter_version: GERBER_ADAPTER_VERSION.into(),
            parse_status: ParseStatus::Complete,
            numeric_format: Some(format),
            metrics,
        };
        let layer = ManufacturingLayer {
            id: self.layer_id.clone(),
            document_id: self.document_id.clone(),
            name: None,
            role: LayerRole::Unknown,
            side: LayerSide::Unknown,
            context: LayerContext::Unknown,
            polarity: LayerPolarity::Unknown,
            order: None,
            authority: Authority::FileContent,
            provenance: first_provenance.clone(),
        };
        let expanded_complete =
            self.macros.is_empty() && self.blocks.is_empty() && self.repetitions.is_empty();
        let has_attributes = !self.attributes.is_empty();
        let has_extents = self.bounds.extent().is_some();
        let capability = |id, state, detail: &str, with_provenance: bool| CapabilityRecord {
            id,
            state,
            authority: Authority::FileContent,
            document_ids: vec![self.document_id.clone()],
            provenance: if with_provenance {
                vec![first_provenance.clone()]
            } else {
                Vec::new()
            },
            detail: detail.into(),
        };
        let mut capabilities = vec![
            capability(
                CapabilityId::DocumentSyntax,
                CapabilityState::Complete,
                "Every dependency parser result and error was accounted before interpretation.",
                true,
            ),
            capability(
                CapabilityId::UnitsAndFormat,
                CapabilityState::Complete,
                "MO and FS were interpreted with checked fixed-point conversion.",
                true,
            ),
            capability(
                CapabilityId::GeometryLines,
                CapabilityState::Complete,
                "Linear draw semantics are canonical.",
                true,
            ),
            capability(
                CapabilityId::GeometryArcs,
                CapabilityState::Complete,
                "CW/CCW single and multi-quadrant arcs are canonical.",
                true,
            ),
            capability(
                CapabilityId::GeometryRegions,
                CapabilityState::Complete,
                "Closed region contours and cut-ins are canonical.",
                true,
            ),
            capability(
                CapabilityId::GeometryFlashes,
                CapabilityState::Complete,
                "D03 flashes retain exact aperture references.",
                true,
            ),
            capability(
                CapabilityId::Polarity,
                CapabilityState::Complete,
                "LP polarity is retained per feature.",
                true,
            ),
            capability(
                CapabilityId::Transforms,
                CapabilityState::Complete,
                "LM/LR/LS are retained as checked transform state.",
                true,
            ),
            capability(
                CapabilityId::Repetition,
                CapabilityState::Complete,
                "Step-repeat definitions are bounded and retained compactly.",
                true,
            ),
            capability(
                CapabilityId::Apertures,
                CapabilityState::Complete,
                "Standard, macro, and block apertures are validated and retained.",
                true,
            ),
            capability(
                CapabilityId::Macros,
                CapabilityState::Complete,
                "Supported macro variables, expressions, and primitives are checked without floats.",
                true,
            ),
            capability(
                CapabilityId::GeometryExpanded,
                if expanded_complete {
                    CapabilityState::Complete
                } else {
                    CapabilityState::Partial
                },
                if expanded_complete {
                    "Geometry needs no compact definition expansion."
                } else {
                    "Macro, block, or step-repeat definitions remain compact and bounded."
                },
                true,
            ),
            capability(
                CapabilityId::Extents,
                if has_extents {
                    CapabilityState::Complete
                } else {
                    CapabilityState::NotProvided
                },
                "Deterministic conservative fixed-point geometry extents.",
                has_extents,
            ),
            capability(
                CapabilityId::X2FileAttributes,
                if has_attributes {
                    CapabilityState::Partial
                } else {
                    CapabilityState::NotProvided
                },
                "X2 facts are retained only; Plan 05-04 owns authority and completeness.",
                has_attributes,
            ),
        ];
        capabilities.sort_by_key(|record| record.id);
        let mut omissions = Vec::new();
        if !expanded_complete {
            omissions.push(Omission {
                id: stable_id(
                    "omission",
                    &(
                        &self.document_id,
                        "compact-gerber-definitions",
                        &first_provenance.location,
                    ),
                )
                .map_err(GerberParseError::Canonical)?,
                kind: OmissionKind::MissingSemanticRecord,
                affected_capabilities: vec![CapabilityId::GeometryExpanded],
                provenance: first_provenance.clone(),
                detail: "Compact macro/block/SR definitions are not cloned into expanded geometry."
                    .into(),
            });
        }
        if has_attributes {
            omissions.push(Omission {
                id: stable_id(
                    "omission",
                    &(
                        &self.document_id,
                        "x2-authority-deferred",
                        &first_provenance.location,
                    ),
                )
                .map_err(GerberParseError::Canonical)?,
                kind: OmissionKind::MissingSemanticRecord,
                affected_capabilities: vec![CapabilityId::X2FileAttributes],
                provenance: first_provenance.clone(),
                detail: "X2 facts are retained without Plan 05-04 role/connectivity authority."
                    .into(),
            });
        }
        let outcome = ManufacturingInputOutcome {
            id: input_outcome_id(
                &self.input.virtual_path,
                Some(&self.input.artifact_digest),
                ManufacturingKindCandidate::Gerber,
            ),
            virtual_path: self.input.virtual_path.clone(),
            artifact_digest: Some(self.input.artifact_digest.clone()),
            kind_candidate: ManufacturingKindCandidate::Gerber,
            size: self.input.size,
            state: ManufacturingLoadState::Retained,
            reason: None,
        };
        let extents = self.bounds.extent();
        let started = self.started;
        let timeout = self.timeout;
        let mut review = FabricationReview {
            status: FabricationStatus::Partial,
            input_outcomes: vec![outcome],
            documents: vec![document],
            layers: vec![layer],
            tools: self.tools,
            apertures: self.aperture_facts,
            macros: self.macros,
            blocks: self.blocks,
            repetitions: self.repetitions,
            features: self.features,
            capabilities: CapabilityLedger {
                records: capabilities,
            },
            omissions,
            warnings: self.warnings,
            ..FabricationReview::default()
        };
        review
            .finalize_trusted()
            .map_err(GerberParseError::Canonical)?;
        check_gerber_deadline(started, timeout, "canonicalization")?;
        Ok(GerberProduction {
            review,
            original_digest: self.boundary.original_digest,
            accounting: self.accounting,
            parser_issues: self.parser_issues,
            normalization_warnings: self.boundary.warnings,
            attributes: self.attributes,
            route_file_functions: self.routes,
            extents,
        })
    }

    fn handle_normal(&mut self, frame: &GerberFrame, text: &str) -> Result<(), GerberParseError> {
        let command = text
            .strip_suffix('*')
            .ok_or_else(|| self.semantic(frame.record, "ordinary-command-without-terminator"))?;
        if let Some(comment) = command.strip_prefix("G04") {
            let comment = comment.strip_prefix(' ').unwrap_or(comment);
            self.note_text(frame.record, comment)?;
            if comment.starts_with("#@! ") {
                self.attributes.push(GerberAttributeEvidence {
                    kind: GerberAttributeKind::StandardComment,
                    raw: text.into(),
                    provenance: self.provenance(frame),
                });
            }
            return Ok(());
        }
        if command == "M02" {
            if self.region.is_some() || self.repeat.is_some() || self.block.is_some() {
                return Err(self.semantic(frame.record, "m02-inside-open-definition"));
            }
            self.terminated = true;
            return Ok(());
        }
        if command == "G36" {
            return self.open_region(frame);
        }
        if command == "G37" {
            return self.close_region(frame);
        }
        if command == "G74" {
            self.quadrant = QuadrantMode::Single;
            return Ok(());
        }
        if command == "G75" {
            self.quadrant = QuadrantMode::Multi;
            return Ok(());
        }
        for (prefix, linear, direction) in [
            ("G01", true, None),
            ("G02", false, Some(ArcDirection::Clockwise)),
            ("G03", false, Some(ArcDirection::CounterClockwise)),
        ] {
            if let Some(operation) = command.strip_prefix(prefix) {
                self.linear = linear;
                self.interpolation = direction;
                if operation.is_empty() {
                    return Ok(());
                }
                return self.handle_operation(frame, operation);
            }
        }
        if let Some(selection) = command.strip_prefix("G54D") {
            return self.select_aperture(frame, selection);
        }
        if let Some(selection) = command.strip_prefix('D') {
            if selection.bytes().all(|byte| byte.is_ascii_digit()) {
                let code = selection
                    .parse::<i32>()
                    .map_err(|_| self.semantic(frame.record, "invalid-aperture-selection"))?;
                if code >= 10 {
                    return self.select_aperture(frame, selection);
                }
            }
        }
        if command.starts_with(['X', 'Y']) {
            return self.handle_operation(frame, command);
        }
        Err(GerberParseError::Unsupported {
            record: frame.record,
            command: bounded_command(command),
        })
    }

    fn handle_extended(&mut self, frame: &GerberFrame, text: &str) -> Result<(), GerberParseError> {
        let body = text
            .strip_prefix('%')
            .and_then(|body| body.strip_suffix("*%"))
            .ok_or_else(|| self.semantic(frame.record, "invalid-extended-framing"))?;
        if body.starts_with("AM") {
            return self.define_macro(frame, body);
        }
        if body.contains('*') {
            return Err(self.semantic(frame.record, "unexpected-multiword-extended-command"));
        }
        if body.starts_with("FS") {
            return self.set_format(frame, body);
        }
        if body.starts_with("MO") {
            return self.set_unit(frame, body);
        }
        if body.starts_with("ADD") {
            return self.define_aperture(frame, body);
        }
        match body {
            "LPD" => {
                self.ensure_not_in_region(frame)?;
                self.polarity = LayerPolarity::Dark;
                return Ok(());
            }
            "LPC" => {
                self.ensure_not_in_region(frame)?;
                self.polarity = LayerPolarity::Clear;
                return Ok(());
            }
            "LMN" => {
                self.ensure_not_in_region(frame)?;
                self.mirror_x = false;
                self.mirror_y = false;
                return Ok(());
            }
            "LMX" => {
                self.ensure_not_in_region(frame)?;
                self.mirror_x = true;
                self.mirror_y = false;
                return Ok(());
            }
            "LMY" => {
                self.ensure_not_in_region(frame)?;
                self.mirror_x = false;
                self.mirror_y = true;
                return Ok(());
            }
            "LMXY" => {
                self.ensure_not_in_region(frame)?;
                self.mirror_x = true;
                self.mirror_y = true;
                return Ok(());
            }
            _ => {}
        }
        if let Some(rotation) = body.strip_prefix("LR") {
            self.ensure_not_in_region(frame)?;
            self.rotation_microdegrees = self.parse_angle(frame.record, rotation)?;
            return Ok(());
        }
        if let Some(scale) = body.strip_prefix("LS") {
            self.ensure_not_in_region(frame)?;
            let value = self.parse_rational(frame.record, scale)?;
            if value.numerator <= 0 {
                return Err(self.semantic(frame.record, "nonpositive-scale"));
            }
            self.scale_numerator = i64::try_from(value.numerator)
                .map_err(|_| self.semantic(frame.record, "scale-overflow"))?;
            self.scale_denominator = i64::try_from(value.denominator)
                .map_err(|_| self.semantic(frame.record, "scale-overflow"))?;
            return Ok(());
        }
        if body.starts_with("SR") {
            return self.step_repeat(frame, body);
        }
        if body.starts_with("AB") {
            return self.aperture_block(frame, body);
        }
        let kind = if body.starts_with("TF") {
            Some(GerberAttributeKind::File)
        } else if body.starts_with("TA") {
            Some(GerberAttributeKind::Aperture)
        } else if body.starts_with("TO") {
            Some(GerberAttributeKind::Object)
        } else if body.starts_with("TD") {
            Some(GerberAttributeKind::Delete)
        } else {
            None
        };
        if let Some(kind) = kind {
            self.note_text(frame.record, body)?;
            self.attributes.push(GerberAttributeEvidence {
                kind,
                raw: text.into(),
                provenance: self.provenance(frame),
            });
            return Ok(());
        }
        Err(GerberParseError::Unsupported {
            record: frame.record,
            command: bounded_command(body),
        })
    }

    fn set_format(&mut self, frame: &GerberFrame, body: &str) -> Result<(), GerberParseError> {
        let bytes = body.as_bytes();
        if self.format_digits.is_some()
            || bytes.len() != 10
            || &bytes[..2] != b"FS"
            || !matches!(bytes[2], b'L' | b'T')
            || !matches!(bytes[3], b'A' | b'I')
            || bytes[4] != b'X'
            || bytes[7] != b'Y'
            || !bytes[5..7].iter().all(u8::is_ascii_digit)
            || bytes[5..7] != bytes[8..10]
        {
            return Err(self.semantic(frame.record, "invalid-or-duplicate-fs"));
        }
        let integer = bytes[5] - b'0';
        let decimal = bytes[6] - b'0';
        self.coordinate_mode_absolute = bytes[3] == b'A';
        self.zero_omission_trailing = bytes[2] == b'T';
        self.format_digits = Some((integer, decimal));
        self.update_numeric_format(frame.record)
    }

    fn set_unit(&mut self, frame: &GerberFrame, body: &str) -> Result<(), GerberParseError> {
        if self.unit.is_some() {
            return Err(self.semantic(frame.record, "duplicate-mo"));
        }
        self.unit = Some(match body {
            "MOMM" => SourceUnit::Millimetre,
            "MOIN" => SourceUnit::Inch,
            _ => return Err(self.semantic(frame.record, "invalid-mo")),
        });
        self.update_numeric_format(frame.record)
    }

    fn update_numeric_format(&mut self, record: u64) -> Result<(), GerberParseError> {
        if let (Some(unit), Some((integer, decimal))) = (self.unit, self.format_digits) {
            self.format = Some(
                SourceNumericFormat::new(unit, integer, decimal)
                    .map_err(GerberParseError::Canonical)?,
            );
        } else if self.format_digits.is_some_and(|(integer, decimal)| {
            integer == 0 || decimal > MANUFACTURING_LIMITS.max_decimal_places
        }) {
            return Err(self.semantic(record, "invalid-fs-digits"));
        }
        Ok(())
    }

    fn select_aperture(
        &mut self,
        frame: &GerberFrame,
        selection: &str,
    ) -> Result<(), GerberParseError> {
        let code = selection
            .parse::<i32>()
            .map_err(|_| self.semantic(frame.record, "invalid-aperture-selection"))?;
        if !self.apertures.contains_key(&code) {
            return Err(self.semantic(frame.record, "undefined-aperture"));
        }
        self.selected = Some(code);
        self.last_operation = None;
        Ok(())
    }

    fn ensure_not_in_region(&self, frame: &GerberFrame) -> Result<(), GerberParseError> {
        if self.region.is_some() {
            Err(self.semantic(frame.record, "state-change-inside-region"))
        } else {
            Ok(())
        }
    }

    fn deadline(&self, stage: &'static str) -> Result<(), GerberParseError> {
        check_gerber_deadline(self.started, self.timeout, stage)
    }

    fn semantic(&self, record: u64, reason: &'static str) -> GerberParseError {
        GerberParseError::Semantic { record, reason }
    }

    fn provenance(&self, frame: &GerberFrame) -> ManufacturingProvenance {
        gerber_provenance_for(&self.document_id, &self.boundary.original_digest, frame)
    }

    fn note_text(&mut self, record: u64, text: &str) -> Result<(), GerberParseError> {
        if text.len() > MANUFACTURING_LIMITS.max_text_bytes {
            return Err(GerberParseError::Resource {
                resource: "metadata-text",
                observed: text.len() as u64,
                limit: MANUFACTURING_LIMITS.max_text_bytes as u64,
            });
        }
        self.max_text_bytes = self.max_text_bytes.max(text.len());
        self.metadata_bytes = self.metadata_bytes.checked_add(text.len() as u64).ok_or(
            GerberParseError::Resource {
                resource: "metadata-bytes",
                observed: u64::MAX,
                limit: MANUFACTURING_LIMITS.metadata_bytes_per_file,
            },
        )?;
        if self.metadata_bytes > MANUFACTURING_LIMITS.metadata_bytes_per_file {
            return Err(GerberParseError::Resource {
                resource: "metadata-bytes",
                observed: self.metadata_bytes,
                limit: MANUFACTURING_LIMITS.metadata_bytes_per_file,
            });
        }
        if text
            .chars()
            .any(|character| character.is_control() && character != '\t')
        {
            return Err(self.semantic(record, "control-in-metadata"));
        }
        Ok(())
    }

    fn note_numeric(&mut self, record: u64, value: &str) -> Result<(), GerberParseError> {
        self.max_numeric_bytes = self.max_numeric_bytes.max(value.len());
        if value.len() > MANUFACTURING_LIMITS.max_numeric_bytes {
            return Err(GerberParseError::Resource {
                resource: "numeric-token",
                observed: value.len() as u64,
                limit: MANUFACTURING_LIMITS.max_numeric_bytes as u64,
            });
        }
        let decimals = value
            .split_once('.')
            .map_or(0, |(_, fraction)| fraction.len());
        if decimals > usize::from(MANUFACTURING_LIMITS.max_decimal_places) {
            return Err(self.semantic(record, "too-many-decimal-places"));
        }
        Ok(())
    }

    fn parse_rational(
        &mut self,
        record: u64,
        value: &str,
    ) -> Result<GerberRational, GerberParseError> {
        self.note_numeric(record, value)?;
        GerberRational::parse(value).map_err(|reason| self.semantic(record, reason))
    }

    fn parse_length(&mut self, record: u64, value: &str) -> Result<Picometres, GerberParseError> {
        let unit = self
            .unit
            .ok_or_else(|| self.semantic(record, "length-before-mo"))?;
        self.parse_rational(record, value)?
            .to_picometres(unit)
            .map_err(|reason| self.semantic(record, reason))
    }

    fn parse_angle(&mut self, record: u64, value: &str) -> Result<i64, GerberParseError> {
        self.parse_rational(record, value)?
            .to_microdegrees()
            .map_err(|reason| self.semantic(record, reason))
    }

    fn aperture_transform(
        &self,
        record: u64,
        pivot: CanonicalPoint,
    ) -> Result<TransformChain, GerberParseError> {
        if !self.mirror_x
            && !self.mirror_y
            && self.rotation_microdegrees == 0
            && self.scale_numerator == self.scale_denominator
        {
            return Ok(TransformChain::default());
        }
        let mut operations = vec![TransformOperation::Translate {
            x: Picometres(
                pivot
                    .x
                    .0
                    .checked_neg()
                    .ok_or_else(|| self.semantic(record, "transform-pivot-overflow"))?,
            ),
            y: Picometres(
                pivot
                    .y
                    .0
                    .checked_neg()
                    .ok_or_else(|| self.semantic(record, "transform-pivot-overflow"))?,
            ),
        }];
        if self.mirror_x || self.mirror_y {
            operations.push(TransformOperation::Mirror {
                x: self.mirror_x,
                y: self.mirror_y,
            });
        }
        if self.rotation_microdegrees != 0 {
            operations.push(TransformOperation::Rotate {
                microdegrees: self.rotation_microdegrees,
            });
        }
        if self.scale_numerator != self.scale_denominator {
            operations.push(TransformOperation::Scale {
                numerator: self.scale_numerator,
                denominator: self.scale_denominator,
            });
        }
        operations.push(TransformOperation::Translate {
            x: pivot.x,
            y: pivot.y,
        });
        Ok(TransformChain { operations })
    }

    fn scaled_aperture_length(
        &self,
        record: u64,
        value: Picometres,
    ) -> Result<Picometres, GerberParseError> {
        let product = i128::from(value.0)
            .checked_mul(i128::from(self.scale_numerator))
            .ok_or_else(|| self.semantic(record, "aperture-scale-overflow"))?;
        let denominator = i128::from(self.scale_denominator);
        if denominator <= 0 || product % denominator != 0 {
            return Err(self.semantic(record, "aperture-scale-finer-than-picometre"));
        }
        let value = Picometres(
            i64::try_from(product / denominator)
                .map_err(|_| self.semantic(record, "aperture-scale-overflow"))?,
        );
        validate_positive_length(value).map_err(GerberParseError::Canonical)?;
        Ok(value)
    }
}

fn bounded_command(command: &str) -> String {
    command
        .chars()
        .take(MANUFACTURING_LIMITS.max_text_bytes)
        .collect()
}

impl GerberInterpreter<'_> {
    fn define_macro(&mut self, frame: &GerberFrame, body: &str) -> Result<(), GerberParseError> {
        if self.macro_definitions.len() >= MANUFACTURING_LIMITS.macros {
            return Err(GerberParseError::Resource {
                resource: "macros",
                observed: self.macro_definitions.len() as u64 + 1,
                limit: MANUFACTURING_LIMITS.macros as u64,
            });
        }
        let mut words = body.split('*');
        let name = words
            .next()
            .and_then(|header| header.strip_prefix("AM"))
            .ok_or_else(|| self.semantic(frame.record, "invalid-macro-header"))?;
        if name.is_empty()
            || name.len() > MANUFACTURING_LIMITS.max_text_bytes
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            || self.macro_definitions.contains_key(name)
        {
            return Err(self.semantic(frame.record, "invalid-or-duplicate-macro-name"));
        }
        self.note_text(frame.record, name)?;
        let operations = words
            .map(str::trim)
            .filter(|word| !word.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if operations.is_empty() || operations.len() > MANUFACTURING_LIMITS.operations_per_macro {
            return Err(GerberParseError::Resource {
                resource: "macro-operations",
                observed: operations.len() as u64,
                limit: MANUFACTURING_LIMITS.operations_per_macro as u64,
            });
        }
        let mut variables = BTreeSet::new();
        for operation in &operations {
            self.validate_macro_operation(frame.record, operation, &mut variables)?;
        }
        if variables.len() > MANUFACTURING_LIMITS.macro_variables {
            return Err(GerberParseError::Resource {
                resource: "macro-variables",
                observed: variables.len() as u64,
                limit: MANUFACTURING_LIMITS.macro_variables as u64,
            });
        }
        let provenance = self.provenance(frame);
        let id = record_id("macro", &self.document_id, &provenance.location);
        let variable_names = variables
            .iter()
            .map(|number| format!("${number}"))
            .collect::<Vec<_>>();
        self.macros.push(MacroDefinition {
            id: id.clone(),
            document_id: self.document_id.clone(),
            name: name.into(),
            variables: variable_names,
            operations: operations.clone(),
            provenance,
        });
        self.macro_definitions.insert(
            name.into(),
            GerberMacroDefinitionInternal {
                id,
                operations,
                variables: variables.into_iter().collect(),
            },
        );
        Ok(())
    }

    fn validate_macro_operation(
        &mut self,
        record: u64,
        operation: &str,
        variables: &mut BTreeSet<u32>,
    ) -> Result<(), GerberParseError> {
        self.note_text(record, operation)?;
        collect_macro_variables(operation, variables)
            .map_err(|reason| self.semantic(record, reason))?;
        if let Some(definition) = operation.strip_prefix('$') {
            let (number, expression) = definition
                .split_once('=')
                .ok_or_else(|| self.semantic(record, "invalid-variable-definition"))?;
            let number = number
                .parse::<u32>()
                .map_err(|_| self.semantic(record, "invalid-variable-definition"))?;
            if number == 0 {
                return Err(self.semantic(record, "invalid-variable-definition"));
            }
            variables.insert(number);
            self.parse_expression(record, expression, &BTreeMap::new(), false)?;
            return Ok(());
        }
        if let Some(comment) = operation.strip_prefix('0') {
            if !comment.is_empty() && !comment.starts_with(' ') {
                return Err(self.semantic(record, "invalid-macro-comment"));
            }
            return Ok(());
        }
        let fields = operation.split(',').collect::<Vec<_>>();
        let code = fields
            .first()
            .copied()
            .ok_or_else(|| self.semantic(record, "empty-macro-operation"))?;
        let valid_count = match code {
            "1" => matches!(fields.len(), 5 | 6),
            "20" => fields.len() == 8,
            "21" => fields.len() == 7,
            "4" => {
                let vertices = fields
                    .get(2)
                    .and_then(|value| value.parse::<usize>().ok())
                    .ok_or_else(|| self.semantic(record, "outline-vertices-must-be-literal"))?;
                vertices > 0 && fields.len() == vertices.saturating_mul(2).saturating_add(6)
            }
            "5" => fields.len() == 7,
            "6" => fields.len() == 10,
            "7" => fields.len() == 7,
            _ => return Err(self.semantic(record, "unsupported-macro-primitive")),
        };
        if !valid_count {
            return Err(self.semantic(record, "invalid-macro-primitive-arguments"));
        }
        for field in &fields[1..] {
            self.parse_expression(record, field, &BTreeMap::new(), false)?;
        }
        Ok(())
    }

    fn parse_expression(
        &mut self,
        record: u64,
        expression: &str,
        variables: &BTreeMap<u32, GerberRational>,
        require_known: bool,
    ) -> Result<Option<GerberRational>, GerberParseError> {
        for token in gerber_expression_numbers(expression) {
            self.note_numeric(record, token)?;
        }
        let (value, depth) = GerberExpressionParser::parse(expression, variables)
            .map_err(|reason| self.semantic(record, reason))?;
        self.max_nesting = self.max_nesting.max(depth);
        if require_known && value.is_none() {
            return Err(self.semantic(record, "undefined-macro-variable"));
        }
        Ok(value)
    }

    fn define_aperture(&mut self, frame: &GerberFrame, body: &str) -> Result<(), GerberParseError> {
        if self.format.is_none() || self.unit.is_none() {
            return Err(self.semantic(frame.record, "aperture-before-mo-fs"));
        }
        let definition = body
            .strip_prefix("ADD")
            .ok_or_else(|| self.semantic(frame.record, "invalid-aperture-definition"))?;
        let code_end = definition
            .bytes()
            .position(|byte| !byte.is_ascii_digit())
            .ok_or_else(|| self.semantic(frame.record, "missing-aperture-template"))?;
        let code = definition[..code_end]
            .parse::<i32>()
            .map_err(|_| self.semantic(frame.record, "invalid-aperture-code"))?;
        if code < 10 {
            return Err(self.semantic(frame.record, "invalid-aperture-code"));
        }
        let remainder = &definition[code_end..];
        let (template, arguments) = remainder.split_once(',').unwrap_or((remainder, ""));
        let arguments = if arguments.is_empty() {
            Vec::new()
        } else {
            arguments.split('X').collect::<Vec<_>>()
        };
        match template {
            "C" => {
                if !matches!(arguments.len(), 1 | 2) {
                    return Err(self.semantic(frame.record, "invalid-circle-aperture"));
                }
                let diameter = self.parse_length(frame.record, arguments[0])?;
                if diameter.0 < 0 {
                    return Err(self.semantic(frame.record, "negative-aperture-dimension"));
                }
                let mut dimensions = if diameter.0 == 0 {
                    Vec::new()
                } else {
                    vec![diameter]
                };
                if let Some(hole) = arguments.get(1) {
                    dimensions.push(self.positive_length(frame.record, hole)?);
                }
                let mut bounds = GerberBounds::default();
                if diameter.0 > 0 {
                    let radius = half_ceil(diameter.0)?;
                    bounds
                        .include_box(-radius, -radius, radius, radius)
                        .map_err(|reason| self.semantic(frame.record, reason))?;
                }
                self.insert_aperture(
                    frame,
                    code,
                    ApertureShape::Circle,
                    dimensions,
                    None,
                    Vec::new(),
                    None,
                    None,
                    (diameter.0 > 0).then_some(diameter),
                    bounds,
                    0,
                    GerberExpansionWeight::single(1)?,
                )
            }
            "R" | "O" => {
                if !matches!(arguments.len(), 2 | 3) {
                    return Err(self.semantic(frame.record, "invalid-rectangular-aperture"));
                }
                let x = self.positive_length(frame.record, arguments[0])?;
                let y = self.positive_length(frame.record, arguments[1])?;
                let mut dimensions = vec![x, y];
                if let Some(hole) = arguments.get(2) {
                    dimensions.push(self.positive_length(frame.record, hole)?);
                }
                let half_x = half_ceil(x.0)?;
                let half_y = half_ceil(y.0)?;
                let mut bounds = GerberBounds::default();
                bounds
                    .include_box(-half_x, -half_y, half_x, half_y)
                    .map_err(|reason| self.semantic(frame.record, reason))?;
                self.insert_aperture(
                    frame,
                    code,
                    if template == "R" {
                        ApertureShape::Rectangle
                    } else {
                        ApertureShape::Obround
                    },
                    dimensions,
                    None,
                    Vec::new(),
                    None,
                    None,
                    None,
                    bounds,
                    0,
                    GerberExpansionWeight::single(4)?,
                )
            }
            "P" => {
                if !(2..=4).contains(&arguments.len()) {
                    return Err(self.semantic(frame.record, "invalid-polygon-aperture"));
                }
                let diameter = self.positive_length(frame.record, arguments[0])?;
                self.note_numeric(frame.record, arguments[1])?;
                let vertices = arguments[1]
                    .parse::<u8>()
                    .map_err(|_| self.semantic(frame.record, "invalid-polygon-vertices"))?;
                if !(3..=12).contains(&vertices) {
                    return Err(self.semantic(frame.record, "invalid-polygon-vertices"));
                }
                let polygon_rotation_microdegrees = arguments
                    .get(2)
                    .map(|angle| self.parse_angle(frame.record, angle))
                    .transpose()?;
                let mut dimensions = vec![diameter];
                if let Some(hole) = arguments.get(3) {
                    dimensions.push(self.positive_length(frame.record, hole)?);
                }
                let radius = half_ceil(diameter.0)?;
                let mut bounds = GerberBounds::default();
                bounds
                    .include_box(-radius, -radius, radius, radius)
                    .map_err(|reason| self.semantic(frame.record, reason))?;
                self.insert_aperture(
                    frame,
                    code,
                    ApertureShape::Polygon,
                    dimensions,
                    None,
                    Vec::new(),
                    Some(vertices),
                    polygon_rotation_microdegrees,
                    None,
                    bounds,
                    0,
                    GerberExpansionWeight::single(u64::from(vertices))?,
                )
            }
            macro_name => {
                let definition = self
                    .macro_definitions
                    .get(macro_name)
                    .cloned()
                    .ok_or_else(|| self.semantic(frame.record, "undefined-aperture-macro"))?;
                let mut values = Vec::new();
                let mut macro_arguments = Vec::new();
                for argument in arguments {
                    let value = self.parse_rational(frame.record, argument)?;
                    values.push(value);
                    macro_arguments.push(value.canonical().map_err(|_| {
                        self.semantic(frame.record, "macro-argument-value-overflow")
                    })?);
                }
                let (bounds, expansion) =
                    self.evaluate_macro(frame.record, &definition, &values)?;
                self.insert_aperture(
                    frame,
                    code,
                    ApertureShape::Macro,
                    Vec::new(),
                    Some(definition.id),
                    macro_arguments,
                    None,
                    None,
                    None,
                    bounds,
                    1,
                    expansion,
                )
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_aperture(
        &mut self,
        frame: &GerberFrame,
        code: i32,
        shape: ApertureShape,
        dimensions: Vec<Picometres>,
        macro_id: Option<String>,
        macro_arguments: Vec<CanonicalRational>,
        polygon_vertices: Option<u8>,
        polygon_rotation_microdegrees: Option<i64>,
        width: Option<Picometres>,
        bounds: GerberBounds,
        block_depth: u8,
        expansion: GerberExpansionWeight,
    ) -> Result<(), GerberParseError> {
        if self.apertures.contains_key(&code) {
            return Err(self.semantic(frame.record, "duplicate-aperture"));
        }
        if self.apertures.len() >= MANUFACTURING_LIMITS.apertures {
            return Err(GerberParseError::Resource {
                resource: "apertures",
                observed: self.apertures.len() as u64 + 1,
                limit: MANUFACTURING_LIMITS.apertures as u64,
            });
        }
        let provenance = self.provenance(frame);
        let aperture_id = aperture_id(&self.document_id, shape, &provenance.location);
        let code_text = format!("D{code}");
        let tool_id = tool_id(
            &self.document_id,
            &format!("{:?}:{code_text}", ToolKind::Aperture),
            &provenance.location,
        );
        self.tools.push(ManufacturingTool {
            id: tool_id.clone(),
            document_id: self.document_id.clone(),
            code: code_text,
            kind: ToolKind::Aperture,
            diameter: width,
            plating: Plating::Unknown,
            span: None,
            provenance: provenance.clone(),
        });
        self.aperture_facts.push(ApertureDefinition {
            id: aperture_id.clone(),
            document_id: self.document_id.clone(),
            shape,
            dimensions,
            polygon_vertices,
            polygon_rotation_microdegrees,
            macro_id,
            macro_arguments,
            provenance,
        });
        self.max_aperture_nesting = self.max_aperture_nesting.max(block_depth);
        if block_depth > MANUFACTURING_LIMITS.max_aperture_nesting {
            return Err(GerberParseError::Resource {
                resource: "aperture-nesting",
                observed: block_depth as u64,
                limit: MANUFACTURING_LIMITS.max_aperture_nesting as u64,
            });
        }
        self.aperture_codes_by_id.insert(aperture_id.clone(), code);
        self.apertures.insert(
            code,
            GerberApertureInternal {
                id: aperture_id,
                tool_id,
                width,
                zero_size: shape == ApertureShape::Circle && bounds.extent().is_none(),
                bounds,
                block_depth,
                expansion,
            },
        );
        Ok(())
    }

    fn positive_length(
        &mut self,
        record: u64,
        value: &str,
    ) -> Result<Picometres, GerberParseError> {
        let value = self.parse_length(record, value)?;
        if value.0 <= 0 {
            return Err(self.semantic(record, "nonpositive-aperture-dimension"));
        }
        Ok(value)
    }

    fn evaluate_macro(
        &mut self,
        record: u64,
        definition: &GerberMacroDefinitionInternal,
        arguments: &[GerberRational],
    ) -> Result<(GerberBounds, GerberExpansionWeight), GerberParseError> {
        if definition
            .variables
            .iter()
            .copied()
            .max()
            .is_some_and(|maximum| maximum as usize > arguments.len())
            && !definition
                .operations
                .iter()
                .any(|operation| operation.starts_with('$'))
        {
            return Err(self.semantic(record, "insufficient-macro-arguments"));
        }
        let mut variables = arguments
            .iter()
            .copied()
            .enumerate()
            .map(|(index, value)| (index as u32 + 1, value))
            .collect::<BTreeMap<_, _>>();
        let mut bounds = GerberBounds::default();
        let mut expansion = GerberExpansionWeight::default();
        for operation in &definition.operations {
            self.deadline("macro-evaluation")?;
            if let Some(assignment) = operation.strip_prefix('$') {
                let (number, expression) = assignment
                    .split_once('=')
                    .ok_or_else(|| self.semantic(record, "invalid-variable-definition"))?;
                let number = number
                    .parse::<u32>()
                    .map_err(|_| self.semantic(record, "invalid-variable-definition"))?;
                let value = self
                    .parse_expression(record, expression, &variables, true)?
                    .expect("required known expression");
                variables.insert(number, value);
                continue;
            }
            if operation.starts_with('0') {
                continue;
            }
            let fields = operation.split(',').collect::<Vec<_>>();
            let values = fields[1..]
                .iter()
                .map(|field| {
                    self.parse_expression(record, field, &variables, true)
                        .map(|value| value.expect("required known expression"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            expansion = expansion.checked_add(
                self.include_macro_primitive(record, fields[0], &values, &mut bounds)?,
                record,
            )?;
        }
        Ok((bounds, expansion.enforce()?))
    }

    fn include_macro_primitive(
        &self,
        record: u64,
        code: &str,
        values: &[GerberRational],
        bounds: &mut GerberBounds,
    ) -> Result<GerberExpansionWeight, GerberParseError> {
        let unit = self.unit.expect("macro follows MO");
        let length = |index: usize| -> Result<i64, GerberParseError> {
            values
                .get(index)
                .ok_or_else(|| self.semantic(record, "missing-macro-argument"))?
                .to_picometres(unit)
                .map(|value| value.0)
                .map_err(|reason| self.semantic(record, reason))
        };
        let integer = |index: usize| -> Result<i64, GerberParseError> {
            values
                .get(index)
                .ok_or_else(|| self.semantic(record, "missing-macro-argument"))?
                .exact_i64()
                .map_err(|reason| self.semantic(record, reason))
        };
        let radial = |points: &[(i64, i64)], padding: i64| -> Result<i64, GerberParseError> {
            points.iter().try_fold(padding, |radius, (x, y)| {
                x.abs()
                    .checked_add(y.abs())
                    .and_then(|point| point.checked_add(padding))
                    .map(|point| radius.max(point))
                    .ok_or_else(|| self.semantic(record, "macro-bound-overflow"))
            })
        };
        let radius = match code {
            "1" => {
                if !matches!(integer(0)?, 0 | 1) {
                    return Err(self.semantic(record, "invalid-macro-exposure"));
                }
                let diameter = length(1)?;
                if diameter < 0 {
                    return Err(self.semantic(record, "negative-macro-diameter"));
                }
                radial(&[(length(2)?, length(3)?)], half_ceil(diameter)?)?
            }
            "20" => {
                if !matches!(integer(0)?, 0 | 1) {
                    return Err(self.semantic(record, "invalid-macro-exposure"));
                }
                let width = length(1)?;
                if width < 0 {
                    return Err(self.semantic(record, "negative-macro-width"));
                }
                radial(
                    &[(length(2)?, length(3)?), (length(4)?, length(5)?)],
                    half_ceil(width)?,
                )?
            }
            "21" => {
                if !matches!(integer(0)?, 0 | 1) {
                    return Err(self.semantic(record, "invalid-macro-exposure"));
                }
                let width = length(1)?;
                let height = length(2)?;
                if width < 0 || height < 0 {
                    return Err(self.semantic(record, "negative-macro-dimension"));
                }
                radial(&[(length(3)?, length(4)?)], half_ceil(width.max(height))?)?
            }
            "4" => {
                if !matches!(integer(0)?, 0 | 1) {
                    return Err(self.semantic(record, "invalid-macro-exposure"));
                }
                let vertices = usize::try_from(integer(1)?)
                    .map_err(|_| self.semantic(record, "invalid-outline-vertices"))?;
                let mut points = Vec::with_capacity(vertices + 1);
                for index in 0..=vertices {
                    points.push((length(2 + index * 2)?, length(3 + index * 2)?));
                }
                if points.first() != points.last() {
                    return Err(self.semantic(record, "open-macro-outline"));
                }
                radial(&points, 0)?
            }
            "5" => {
                if !matches!(integer(0)?, 0 | 1) || !(3..=12).contains(&integer(1)?) {
                    return Err(self.semantic(record, "invalid-macro-polygon"));
                }
                let diameter = length(4)?;
                if diameter < 0 {
                    return Err(self.semantic(record, "negative-macro-diameter"));
                }
                radial(&[(length(2)?, length(3)?)], half_ceil(diameter)?)?
            }
            "6" => {
                let diameter = length(2)?;
                let ring = length(3)?;
                let gap = length(4)?;
                let rings = integer(5)?;
                let cross_width = length(6)?;
                let cross_length = length(7)?;
                if diameter < 0
                    || ring < 0
                    || gap < 0
                    || rings <= 0
                    || cross_width < 0
                    || cross_length < 0
                {
                    return Err(self.semantic(record, "invalid-moire"));
                }
                radial(
                    &[(length(0)?, length(1)?)],
                    half_ceil(diameter.max(cross_length))?,
                )?
            }
            "7" => {
                let outer = length(2)?;
                let inner = length(3)?;
                let gap = length(4)?;
                if outer <= inner || inner < 0 || gap < 0 {
                    return Err(self.semantic(record, "invalid-thermal"));
                }
                radial(&[(length(0)?, length(1)?)], half_ceil(outer)?)?
            }
            _ => return Err(self.semantic(record, "unsupported-macro-primitive")),
        };
        bounds
            .include_box(-radius, -radius, radius, radius)
            .map_err(|reason| self.semantic(record, reason))?;
        match code {
            "1" => GerberExpansionWeight::single(1),
            "20" => GerberExpansionWeight::single(2),
            "21" => GerberExpansionWeight::single(4),
            "4" => GerberExpansionWeight::single(
                u64::try_from(integer(1)?)
                    .ok()
                    .and_then(|vertices| vertices.checked_add(1))
                    .ok_or_else(|| self.semantic(record, "macro-expansion-weight"))?,
            ),
            "5" => GerberExpansionWeight::single(
                u64::try_from(integer(1)?)
                    .map_err(|_| self.semantic(record, "macro-expansion-weight"))?,
            ),
            "6" => {
                let rings = u64::try_from(integer(5)?)
                    .map_err(|_| self.semantic(record, "macro-expansion-weight"))?;
                let features = rings
                    .checked_add(2)
                    .ok_or_else(|| self.semantic(record, "macro-expansion-weight"))?;
                let vertices = rings
                    .checked_mul(8)
                    .and_then(|value| value.checked_add(8))
                    .ok_or_else(|| self.semantic(record, "macro-expansion-weight"))?;
                GerberExpansionWeight {
                    features,
                    vertices,
                    allocation: features
                        .checked_mul(512)
                        .and_then(|value| {
                            vertices
                                .checked_mul(128)
                                .and_then(|vertices| value.checked_add(vertices))
                        })
                        .ok_or_else(|| self.semantic(record, "macro-expansion-weight"))?,
                }
                .enforce()
            }
            "7" => GerberExpansionWeight::single(8),
            _ => Err(self.semantic(record, "macro-expansion-weight")),
        }
    }
}

fn collect_macro_variables(
    source: &str,
    variables: &mut BTreeSet<u32>,
) -> Result<(), &'static str> {
    let bytes = source.as_bytes();
    let mut index = 0_usize;
    while index < bytes.len() {
        if bytes[index] != b'$' {
            index += 1;
            continue;
        }
        index += 1;
        let start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if start == index {
            return Err("invalid-variable");
        }
        let number = source[start..index]
            .parse::<u32>()
            .map_err(|_| "invalid-variable")?;
        if number == 0 {
            return Err("invalid-variable");
        }
        variables.insert(number);
    }
    Ok(())
}

fn gerber_expression_numbers(expression: &str) -> impl Iterator<Item = &str> {
    expression
        .split(|character: char| {
            !(character.is_ascii_digit() || matches!(character, '.' | '+' | '-'))
        })
        .filter(|token| !token.is_empty() && token.bytes().any(|byte| byte.is_ascii_digit()))
}

fn half_ceil(value: i64) -> Result<i64, GerberParseError> {
    value
        .checked_add(1)
        .map(|value| value / 2)
        .ok_or(GerberParseError::Semantic {
            record: 0,
            reason: "dimension-overflow",
        })
}

impl GerberInterpreter<'_> {
    fn aperture_block(&mut self, frame: &GerberFrame, body: &str) -> Result<(), GerberParseError> {
        if body == "AB" {
            let block = self
                .block
                .take()
                .ok_or_else(|| self.semantic(frame.record, "aperture-block-close-without-open"))?;
            if self.region.is_some() || self.repeat.is_some() {
                return Err(self.semantic(frame.record, "definition-open-at-aperture-block-close"));
            }
            if self.features.len() == block.feature_start {
                return Err(self.semantic(frame.record, "empty-aperture-block"));
            }
            let feature_ids = self.features[block.feature_start..]
                .iter()
                .map(|feature| feature.id.clone())
                .collect::<Vec<_>>();
            let mut bounds = GerberBounds::default();
            let mut expansion = GerberExpansionWeight::default();
            for (feature, weight) in self.features[block.feature_start..]
                .iter()
                .zip(&self.feature_weights[block.feature_start..])
            {
                bounds.merge(self.feature_bounds(feature)?);
                expansion = expansion.checked_add(*weight, frame.record)?;
            }
            expansion.enforce()?;
            self.position = block.saved.position;
            self.interpolation = block.saved.interpolation;
            self.linear = block.saved.linear;
            self.quadrant = block.saved.quadrant;
            self.selected = block.saved.selected;
            self.last_operation = block.saved.last_operation;
            let depth = block
                .max_child_depth
                .checked_add(1)
                .ok_or_else(|| self.semantic(frame.record, "aperture-nesting-overflow"))?;
            self.insert_aperture(
                &block.start,
                block.code,
                ApertureShape::Block,
                Vec::new(),
                None,
                Vec::new(),
                None,
                None,
                None,
                bounds,
                depth,
                expansion,
            )?;
            let provenance = self.provenance(&block.start);
            self.blocks.push(ApertureBlock {
                id: record_id("block", &self.document_id, &provenance.location),
                document_id: self.document_id.clone(),
                feature_ids,
                provenance,
            });
            return Ok(());
        }
        let code = body
            .strip_prefix("ABD")
            .ok_or_else(|| self.semantic(frame.record, "invalid-aperture-block"))?
            .parse::<i32>()
            .map_err(|_| self.semantic(frame.record, "invalid-aperture-block-code"))?;
        if code < 10
            || self.apertures.contains_key(&code)
            || self.block.is_some()
            || self.region.is_some()
            || self.repeat.is_some()
        {
            return Err(self.semantic(frame.record, "invalid-or-nested-aperture-block"));
        }
        self.block = Some(GerberBlockBuild {
            start: frame.clone(),
            code,
            feature_start: self.features.len(),
            saved: GerberModalState {
                position: self.position,
                interpolation: self.interpolation,
                linear: self.linear,
                quadrant: self.quadrant,
                selected: self.selected,
                last_operation: self.last_operation,
            },
            max_child_depth: 0,
        });
        self.position = CanonicalPoint::default();
        self.interpolation = None;
        self.linear = true;
        self.quadrant = QuadrantMode::Unknown;
        self.selected = None;
        self.last_operation = None;
        Ok(())
    }

    fn step_repeat(&mut self, frame: &GerberFrame, body: &str) -> Result<(), GerberParseError> {
        if body == "SR" {
            let repeat = self
                .repeat
                .take()
                .ok_or_else(|| self.semantic(frame.record, "sr-close-without-open"))?;
            if self.region.is_some() {
                return Err(self.semantic(frame.record, "region-open-at-sr-close"));
            }
            let feature_count = self.features.len().saturating_sub(repeat.feature_start);
            if feature_count == 0 {
                return Err(self.semantic(frame.record, "empty-step-repeat"));
            }
            let grid = u64::from(repeat.x_count)
                .checked_mul(u64::from(repeat.y_count))
                .ok_or_else(|| self.semantic(frame.record, "sr-product-overflow"))?;
            let mut repeated_weight = GerberExpansionWeight::default();
            for weight in &self.feature_weights[repeat.feature_start..] {
                repeated_weight = repeated_weight.checked_add(*weight, frame.record)?;
            }
            let extra = repeated_weight.checked_mul(grid.saturating_sub(1), frame.record)?;
            let projected_expanded = self
                .expanded_weight
                .checked_add(extra, frame.record)?
                .enforce()?;
            let projected_allocation = (self.features.len() as u64)
                .checked_mul(GERBER_FEATURE_ALLOCATION_BYTES)
                .and_then(|bytes| {
                    (self.vertices as u64)
                        .checked_mul(GERBER_VERTEX_ALLOCATION_BYTES)
                        .and_then(|vertices| bytes.checked_add(vertices))
                })
                .and_then(|bytes| bytes.checked_add(projected_expanded.allocation))
                .ok_or_else(|| self.semantic(frame.record, "allocation-overflow"))?;
            if projected_allocation > MANUFACTURING_LIMITS.canonical_allocation_bytes {
                return Err(GerberParseError::Resource {
                    resource: "canonical-allocation",
                    observed: projected_allocation,
                    limit: MANUFACTURING_LIMITS.canonical_allocation_bytes,
                });
            }
            self.expanded_weight = projected_expanded;
            let feature_ids = self.features[repeat.feature_start..]
                .iter()
                .map(|feature| feature.id.clone())
                .collect::<Vec<_>>();
            let mut base_bounds = GerberBounds::default();
            for feature in &self.features[repeat.feature_start..] {
                base_bounds.merge(self.feature_bounds(feature)?);
            }
            let max_x = i128::from(repeat.x_step.0)
                .checked_mul(i128::from(repeat.x_count - 1))
                .and_then(|value| i64::try_from(value).ok())
                .ok_or_else(|| self.semantic(frame.record, "sr-offset-overflow"))?;
            let max_y = i128::from(repeat.y_step.0)
                .checked_mul(i128::from(repeat.y_count - 1))
                .and_then(|value| i64::try_from(value).ok())
                .ok_or_else(|| self.semantic(frame.record, "sr-offset-overflow"))?;
            for (x, y) in [(0, 0), (max_x, 0), (0, max_y), (max_x, max_y)] {
                self.bounds.merge(
                    base_bounds
                        .translated(x, y)
                        .map_err(|reason| self.semantic(frame.record, reason))?,
                );
            }
            let provenance = self.provenance(&repeat.start);
            self.repetitions.push(StepRepeat {
                id: record_id("repeat", &self.document_id, &provenance.location),
                document_id: self.document_id.clone(),
                feature_ids,
                x_count: repeat.x_count,
                y_count: repeat.y_count,
                x_step: repeat.x_step,
                y_step: repeat.y_step,
                provenance,
            });
            return Ok(());
        }
        if self.repeat.is_some()
            || self.region.is_some()
            || self.block.is_some()
            || !body.starts_with("SRX")
        {
            return Err(self.semantic(frame.record, "invalid-or-nested-step-repeat"));
        }
        let fields = parse_tagged_fields(&body[2..], b"XYIJ")
            .map_err(|reason| self.semantic(frame.record, reason))?;
        let x_count = fields[0]
            .parse::<u32>()
            .map_err(|_| self.semantic(frame.record, "invalid-sr-count"))?;
        let y_count = fields[1]
            .parse::<u32>()
            .map_err(|_| self.semantic(frame.record, "invalid-sr-count"))?;
        if x_count == 0
            || y_count == 0
            || x_count > MANUFACTURING_LIMITS.repeat_factor
            || y_count > MANUFACTURING_LIMITS.repeat_factor
        {
            return Err(GerberParseError::Resource {
                resource: "step-repeat-factor",
                observed: u64::from(x_count.max(y_count)),
                limit: u64::from(MANUFACTURING_LIMITS.repeat_factor),
            });
        }
        let x_step = self.parse_length(frame.record, fields[2])?;
        let y_step = self.parse_length(frame.record, fields[3])?;
        self.repeat = Some(GerberRepeatBuild {
            start: frame.clone(),
            feature_start: self.features.len(),
            x_count,
            y_count,
            x_step,
            y_step,
        });
        Ok(())
    }

    fn open_region(&mut self, frame: &GerberFrame) -> Result<(), GerberParseError> {
        if self.region.is_some() || self.repeat.is_some() && self.block.is_some() {
            return Err(self.semantic(frame.record, "invalid-region-open"));
        }
        self.region = Some(GerberRegionBuild {
            start: frame.clone(),
            contours: Vec::new(),
            segments: Vec::new(),
            contour_start: None,
            polarity: self.polarity,
            transforms: TransformChain::default(),
        });
        self.last_operation = None;
        Ok(())
    }

    fn close_region(&mut self, frame: &GerberFrame) -> Result<(), GerberParseError> {
        let mut region = self
            .region
            .take()
            .ok_or_else(|| self.semantic(frame.record, "region-close-without-open"))?;
        self.finish_region_contour(frame.record, &mut region)?;
        if region.contours.is_empty() {
            return Err(self.semantic(frame.record, "empty-region"));
        }
        let mut provenance = self.provenance(&region.start);
        provenance.location.byte_end = frame.byte_end.saturating_sub(1) as u64;
        self.push_feature_with_provenance(
            provenance,
            Geometry::Region(CanonicalRegion {
                contours: region.contours,
            }),
            None,
            region.polarity,
            region.transforms,
        )?;
        self.last_operation = None;
        Ok(())
    }

    fn finish_region_contour(
        &self,
        record: u64,
        region: &mut GerberRegionBuild,
    ) -> Result<(), GerberParseError> {
        if region.segments.is_empty() {
            return Ok(());
        }
        let first = region
            .contour_start
            .ok_or_else(|| self.semantic(record, "region-contour-without-start"))?;
        let last = segment_end(region.segments.last().expect("nonempty region contour"));
        if first != last {
            return Err(self.semantic(record, "open-region-contour"));
        }
        region.contours.push(CanonicalContour {
            segments: std::mem::take(&mut region.segments),
            closed: true,
        });
        region.contour_start = None;
        Ok(())
    }

    fn handle_operation(
        &mut self,
        frame: &GerberFrame,
        command: &str,
    ) -> Result<(), GerberParseError> {
        let fields = self.operation_fields(frame.record, command)?;
        let operation = fields.operation.or_else(|| {
            if self.last_operation == Some(1) {
                Some(1)
            } else {
                None
            }
        });
        let operation =
            operation.ok_or_else(|| self.semantic(frame.record, "undefined-modal-operation"))?;
        if fields.x.is_none() && fields.y.is_none() {
            return Err(self.semantic(frame.record, "operation-without-coordinates"));
        }
        let target = CanonicalPoint {
            x: Picometres(self.modal_coordinate(frame.record, fields.x, self.position.x.0)?),
            y: Picometres(self.modal_coordinate(frame.record, fields.y, self.position.y.0)?),
        };
        let start = self.position;
        match operation {
            2 => {
                if fields.i.is_some() || fields.j.is_some() {
                    return Err(self.semantic(frame.record, "offset-on-move"));
                }
                if let Some(mut region) = self.region.take() {
                    self.finish_region_contour(frame.record, &mut region)?;
                    region.contour_start = Some(target);
                    self.region = Some(region);
                }
                self.position = target;
                self.last_operation = None;
            }
            3 => {
                if self.region.is_some() || fields.i.is_some() || fields.j.is_some() {
                    return Err(self.semantic(frame.record, "invalid-flash-state"));
                }
                let aperture = self.selected_aperture(frame.record)?.clone();
                if aperture.zero_size {
                    return Err(self.semantic(frame.record, "zero-size-aperture-flash"));
                }
                self.push_feature(
                    frame,
                    Geometry::Flash(CanonicalFlash {
                        position: target,
                        aperture_id: aperture.id,
                    }),
                    Some(aperture.tool_id),
                    self.polarity,
                    self.aperture_transform(frame.record, target)?,
                )?;
                self.position = target;
                self.last_operation = None;
            }
            1 => {
                let geometry = if self.linear {
                    if fields.i.is_some() || fields.j.is_some() {
                        return Err(self.semantic(frame.record, "offset-on-linear-draw"));
                    }
                    let width = if self.region.is_some() {
                        None
                    } else {
                        let aperture = self.selected_aperture(frame.record)?;
                        if aperture.zero_size {
                            return Err(self.semantic(frame.record, "zero-size-aperture-draw"));
                        }
                        let width = aperture.width.ok_or_else(|| {
                            self.semantic(frame.record, "noncircular-draw-aperture")
                        })?;
                        Some(self.scaled_aperture_length(frame.record, width)?)
                    };
                    ContourSegment::Line(CanonicalLine {
                        start,
                        end: target,
                        width,
                    })
                } else {
                    let direction = self.interpolation.ok_or_else(|| {
                        self.semantic(frame.record, "undefined-interpolation-mode")
                    })?;
                    let center = self.arc_center(
                        frame.record,
                        start,
                        target,
                        fields.i,
                        fields.j,
                        direction,
                    )?;
                    let width = if self.region.is_some() {
                        None
                    } else {
                        let aperture = self.selected_aperture(frame.record)?;
                        if aperture.zero_size {
                            return Err(self.semantic(frame.record, "zero-size-aperture-draw"));
                        }
                        let width = aperture.width.ok_or_else(|| {
                            self.semantic(frame.record, "noncircular-draw-aperture")
                        })?;
                        Some(self.scaled_aperture_length(frame.record, width)?)
                    };
                    ContourSegment::Arc(CanonicalArc {
                        start,
                        end: target,
                        center,
                        direction,
                        quadrant: self.quadrant,
                        width,
                        source_resolution: self
                            .format
                            .as_ref()
                            .expect("operation follows FS/MO")
                            .resolution,
                    })
                };
                if let Some(region) = &mut self.region {
                    region.contour_start.get_or_insert(start);
                    region.segments.push(geometry);
                } else {
                    let aperture = self.selected_aperture(frame.record)?.clone();
                    let canonical = match geometry {
                        ContourSegment::Line(line) => Geometry::Line(line),
                        ContourSegment::Arc(arc) => Geometry::Arc(arc),
                    };
                    self.push_feature(
                        frame,
                        canonical,
                        Some(aperture.tool_id),
                        self.polarity,
                        TransformChain::default(),
                    )?;
                }
                self.position = target;
                self.last_operation = Some(1);
            }
            _ => return Err(self.semantic(frame.record, "invalid-operation-code")),
        }
        Ok(())
    }

    fn operation_fields<'a>(
        &mut self,
        record: u64,
        command: &'a str,
    ) -> Result<GerberOperationFields<'a>, GerberParseError> {
        let bytes = command.as_bytes();
        let mut position = 0_usize;
        let mut fields = GerberOperationFields::default();
        while position < bytes.len() {
            let tag = bytes[position];
            if !matches!(tag, b'X' | b'Y' | b'I' | b'J' | b'D') {
                return Err(self.semantic(record, "invalid-coordinate-field"));
            }
            position += 1;
            let start = position;
            while position < bytes.len()
                && !matches!(bytes[position], b'X' | b'Y' | b'I' | b'J' | b'D')
            {
                position += 1;
            }
            if start == position {
                return Err(self.semantic(record, "empty-coordinate-field"));
            }
            let value = &command[start..position];
            self.note_numeric(record, value)?;
            match tag {
                b'X' if fields.x.replace(value).is_none() => {}
                b'Y' if fields.y.replace(value).is_none() => {}
                b'I' if fields.i.replace(value).is_none() => {}
                b'J' if fields.j.replace(value).is_none() => {}
                b'D' if fields.operation.is_none() => {
                    fields.operation = Some(match value {
                        "1" | "01" => 1,
                        "2" | "02" => 2,
                        "3" | "03" => 3,
                        _ => return Err(self.semantic(record, "invalid-operation-code")),
                    });
                    if position != bytes.len() {
                        return Err(self.semantic(record, "operation-code-not-final"));
                    }
                }
                _ => return Err(self.semantic(record, "duplicate-coordinate-field")),
            }
        }
        Ok(fields)
    }

    fn modal_coordinate(
        &mut self,
        record: u64,
        value: Option<&str>,
        current: i64,
    ) -> Result<i64, GerberParseError> {
        let Some(value) = value else {
            return Ok(current);
        };
        let parsed = self.coordinate_value(record, value)?;
        let result = if self.coordinate_mode_absolute {
            parsed
        } else {
            current
                .checked_add(parsed)
                .ok_or_else(|| self.semantic(record, "coordinate-overflow"))?
        };
        if result.unsigned_abs() > MAX_COORDINATE_PM as u64 {
            return Err(self.semantic(record, "coordinate-out-of-range"));
        }
        Ok(result)
    }

    fn coordinate_value(&mut self, record: u64, value: &str) -> Result<i64, GerberParseError> {
        let format = self
            .format
            .clone()
            .ok_or_else(|| self.semantic(record, "operation-before-mo-fs"))?;
        self.note_numeric(record, value)?;
        let (sign, digits) = match value.as_bytes().first() {
            Some(b'-') => (-1_i128, &value[1..]),
            Some(b'+') => (1_i128, &value[1..]),
            _ => (1_i128, value),
        };
        let total = usize::from(format.integer_digits) + usize::from(format.decimal_digits);
        if digits.is_empty()
            || digits.len() > total
            || !digits.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(self.semantic(record, "invalid-coordinate-number"));
        }
        let mut mantissa = digits
            .parse::<i128>()
            .map_err(|_| self.semantic(record, "coordinate-overflow"))?;
        if self.zero_omission_trailing {
            mantissa = mantissa
                .checked_mul(
                    10_i128
                        .checked_pow((total - digits.len()) as u32)
                        .ok_or_else(|| self.semantic(record, "coordinate-overflow"))?,
                )
                .ok_or_else(|| self.semantic(record, "coordinate-overflow"))?;
        }
        let rational = GerberRational::new(
            mantissa
                .checked_mul(sign)
                .ok_or_else(|| self.semantic(record, "coordinate-overflow"))?,
            10_i128
                .checked_pow(u32::from(format.decimal_digits))
                .ok_or_else(|| self.semantic(record, "coordinate-overflow"))?,
        )
        .map_err(|reason| self.semantic(record, reason))?;
        rational
            .to_picometres(format.unit)
            .map(|value| value.0)
            .map_err(|reason| self.semantic(record, reason))
    }

    fn selected_aperture(&self, record: u64) -> Result<&GerberApertureInternal, GerberParseError> {
        let code = self
            .selected
            .ok_or_else(|| self.semantic(record, "draw-without-aperture"))?;
        self.apertures
            .get(&code)
            .ok_or_else(|| self.semantic(record, "undefined-aperture"))
    }

    fn arc_center(
        &mut self,
        record: u64,
        start: CanonicalPoint,
        end: CanonicalPoint,
        i: Option<&str>,
        j: Option<&str>,
        direction: ArcDirection,
    ) -> Result<CanonicalPoint, GerberParseError> {
        if self.quadrant == QuadrantMode::Unknown || i.is_none() && j.is_none() {
            return Err(self.semantic(record, "arc-without-quadrant-or-offset"));
        }
        let offset_x = i.map_or(Ok(0), |value| self.coordinate_value(record, value))?;
        let offset_y = j.map_or(Ok(0), |value| self.coordinate_value(record, value))?;
        if self.quadrant == QuadrantMode::Multi {
            let center = CanonicalPoint::new(
                start
                    .x
                    .0
                    .checked_add(offset_x)
                    .ok_or_else(|| self.semantic(record, "arc-center-overflow"))?,
                start
                    .y
                    .0
                    .checked_add(offset_y)
                    .ok_or_else(|| self.semantic(record, "arc-center-overflow"))?,
            );
            self.validate_arc_radii(record, start, end, center)?;
            return Ok(center);
        }
        let mut candidates = BTreeSet::new();
        for x_sign in [-1_i64, 1] {
            for y_sign in [-1_i64, 1] {
                let center = CanonicalPoint::new(
                    start
                        .x
                        .0
                        .checked_add(
                            offset_x
                                .abs()
                                .checked_mul(x_sign)
                                .ok_or_else(|| self.semantic(record, "arc-center-overflow"))?,
                        )
                        .ok_or_else(|| self.semantic(record, "arc-center-overflow"))?,
                    start
                        .y
                        .0
                        .checked_add(
                            offset_y
                                .abs()
                                .checked_mul(y_sign)
                                .ok_or_else(|| self.semantic(record, "arc-center-overflow"))?,
                        )
                        .ok_or_else(|| self.semantic(record, "arc-center-overflow"))?,
                );
                if self.validate_arc_radii(record, start, end, center).is_ok()
                    && single_quadrant_sweep(start, end, center, direction)
                {
                    candidates.insert(center);
                }
            }
        }
        if candidates.len() != 1 {
            return Err(self.semantic(record, "ambiguous-single-quadrant-arc"));
        }
        Ok(*candidates.first().expect("one arc center"))
    }

    fn validate_arc_radii(
        &self,
        record: u64,
        start: CanonicalPoint,
        end: CanonicalPoint,
        center: CanonicalPoint,
    ) -> Result<(), GerberParseError> {
        let radius = |point: CanonicalPoint| -> Result<i128, GerberParseError> {
            let x = i128::from(point.x.0) - i128::from(center.x.0);
            let y = i128::from(point.y.0) - i128::from(center.y.0);
            x.checked_mul(x)
                .and_then(|x| y.checked_mul(y).and_then(|y| x.checked_add(y)))
                .ok_or_else(|| self.semantic(record, "arc-radius-overflow"))
        };
        let start_radius = radius(start)?;
        let end_radius = radius(end)?;
        let resolution = i128::from(
            self.format
                .as_ref()
                .expect("arc follows format")
                .resolution
                .0,
        );
        let max_component = [
            start.x.0 - center.x.0,
            start.y.0 - center.y.0,
            end.x.0 - center.x.0,
            end.y.0 - center.y.0,
        ]
        .into_iter()
        .map(i64::unsigned_abs)
        .max()
        .unwrap_or(0) as i128;
        // Both endpoints and offsets are independently quantized; official contour arcs
        // permit a bounded 16-grid radial disagreement at tangent joins.
        let tolerance = max_component
            .checked_mul(32)
            .and_then(|value| value.checked_mul(resolution))
            .and_then(|value| {
                resolution
                    .checked_mul(resolution)
                    .and_then(|extra| value.checked_add(extra))
            })
            .ok_or_else(|| self.semantic(record, "arc-radius-overflow"))?;
        if (start_radius - end_radius).abs() > tolerance {
            return Err(self.semantic(record, "arc-radius-mismatch"));
        }
        Ok(())
    }
}

fn parse_tagged_fields<'a>(source: &'a str, tags: &[u8]) -> Result<Vec<&'a str>, &'static str> {
    let bytes = source.as_bytes();
    let mut result = Vec::with_capacity(tags.len());
    let mut position = 0_usize;
    for (index, tag) in tags.iter().copied().enumerate() {
        if bytes.get(position) != Some(&tag) {
            return Err("invalid-tagged-fields");
        }
        position += 1;
        let start = position;
        let next_tag = tags.get(index + 1).copied();
        while position < bytes.len() && Some(bytes[position]) != next_tag {
            position += 1;
        }
        if start == position {
            return Err("empty-tagged-field");
        }
        result.push(&source[start..position]);
    }
    if position != bytes.len() {
        return Err("trailing-tagged-field-data");
    }
    Ok(result)
}

fn single_quadrant_sweep(
    start: CanonicalPoint,
    end: CanonicalPoint,
    center: CanonicalPoint,
    direction: ArcDirection,
) -> bool {
    let sx = i128::from(start.x.0) - i128::from(center.x.0);
    let sy = i128::from(start.y.0) - i128::from(center.y.0);
    let ex = i128::from(end.x.0) - i128::from(center.x.0);
    let ey = i128::from(end.y.0) - i128::from(center.y.0);
    let cross = sx * ey - sy * ex;
    let dot = sx * ex + sy * ey;
    dot >= 0
        && match direction {
            ArcDirection::Clockwise => cross <= 0,
            ArcDirection::CounterClockwise => cross >= 0,
        }
}

fn segment_end(segment: &ContourSegment) -> CanonicalPoint {
    match segment {
        ContourSegment::Line(line) => line.end,
        ContourSegment::Arc(arc) => arc.end,
    }
}

impl GerberInterpreter<'_> {
    fn push_feature(
        &mut self,
        frame: &GerberFrame,
        geometry: Geometry,
        tool_id: Option<String>,
        polarity: LayerPolarity,
        transforms: TransformChain,
    ) -> Result<(), GerberParseError> {
        self.push_feature_with_provenance(
            self.provenance(frame),
            geometry,
            tool_id,
            polarity,
            transforms,
        )
    }

    fn push_feature_with_provenance(
        &mut self,
        provenance: ManufacturingProvenance,
        geometry: Geometry,
        tool_id: Option<String>,
        polarity: LayerPolarity,
        transforms: TransformChain,
    ) -> Result<(), GerberParseError> {
        let record = provenance.location.record;
        if self.features.len() >= MANUFACTURING_LIMITS.geometry_features {
            return Err(GerberParseError::Resource {
                resource: "geometry-features",
                observed: self.features.len() as u64 + 1,
                limit: MANUFACTURING_LIMITS.geometry_features as u64,
            });
        }
        let vertices = geometry.vertex_count();
        let projected_vertices = self
            .vertices
            .checked_add(vertices)
            .ok_or_else(|| self.semantic(record, "vertex-count-overflow"))?;
        if projected_vertices > MANUFACTURING_LIMITS.contour_vertices {
            return Err(GerberParseError::Resource {
                resource: "contour-vertices",
                observed: projected_vertices as u64,
                limit: MANUFACTURING_LIMITS.contour_vertices as u64,
            });
        }
        let expansion = match &geometry {
            Geometry::Flash(flash) => {
                self.aperture_codes_by_id
                    .get(&flash.aperture_id)
                    .and_then(|code| self.apertures.get(code))
                    .ok_or_else(|| self.semantic(record, "missing-flash-aperture"))?
                    .expansion
            }
            _ => GerberExpansionWeight::single(vertices as u64)?,
        };
        let definition_only = self.block.is_some();
        let projected_expanded = if definition_only {
            self.expanded_weight
        } else {
            self.expanded_weight
                .checked_add(expansion, record)?
                .enforce()?
        };
        let projected_allocation = (self.features.len() as u64 + 1)
            .checked_mul(GERBER_FEATURE_ALLOCATION_BYTES)
            .and_then(|bytes| {
                (projected_vertices as u64)
                    .checked_mul(GERBER_VERTEX_ALLOCATION_BYTES)
                    .and_then(|vertices| bytes.checked_add(vertices))
            })
            .and_then(|bytes| bytes.checked_add(projected_expanded.allocation))
            .ok_or_else(|| self.semantic(record, "allocation-overflow"))?;
        if projected_allocation > MANUFACTURING_LIMITS.canonical_allocation_bytes {
            return Err(GerberParseError::Resource {
                resource: "canonical-allocation",
                observed: projected_allocation,
                limit: MANUFACTURING_LIMITS.canonical_allocation_bytes,
            });
        }
        let id = feature_id(
            &self.document_id,
            &self.layer_id,
            geometry.kind(),
            &provenance.location,
        );
        let feature = ManufacturingFeature {
            id,
            document_id: self.document_id.clone(),
            layer_id: self.layer_id.clone(),
            tool_id,
            polarity,
            geometry,
            transforms,
            provenance,
        };
        let child_depth = self
            .selected
            .and_then(|code| self.apertures.get(&code))
            .map_or(0, |aperture| aperture.block_depth);
        if let Some(block) = &mut self.block {
            block.max_child_depth = block.max_child_depth.max(child_depth);
        }
        if !definition_only {
            self.bounds.merge(self.feature_bounds(&feature)?);
        }
        self.vertices = projected_vertices;
        self.expanded_weight = projected_expanded;
        self.feature_weights.push(expansion);
        self.features.push(feature);
        Ok(())
    }

    fn feature_bounds(
        &self,
        feature: &ManufacturingFeature,
    ) -> Result<GerberBounds, GerberParseError> {
        self.geometry_bounds(
            &feature.geometry,
            &feature.transforms,
            feature.provenance.location.record,
        )
    }

    fn geometry_bounds(
        &self,
        geometry: &Geometry,
        transforms: &TransformChain,
        record: u64,
    ) -> Result<GerberBounds, GerberParseError> {
        let materialize = |point: CanonicalPoint| {
            transforms
                .materialize(point)
                .map(|value| value.point)
                .map_err(GerberParseError::Canonical)
        };
        let include_point = |bounds: &mut GerberBounds,
                             point: CanonicalPoint,
                             padding: i64|
         -> Result<(), GerberParseError> {
            bounds
                .include_box(
                    point
                        .x
                        .0
                        .checked_sub(padding)
                        .ok_or_else(|| self.semantic(record, "extent-overflow"))?,
                    point
                        .y
                        .0
                        .checked_sub(padding)
                        .ok_or_else(|| self.semantic(record, "extent-overflow"))?,
                    point
                        .x
                        .0
                        .checked_add(padding)
                        .ok_or_else(|| self.semantic(record, "extent-overflow"))?,
                    point
                        .y
                        .0
                        .checked_add(padding)
                        .ok_or_else(|| self.semantic(record, "extent-overflow"))?,
                )
                .map_err(|reason| self.semantic(record, reason))
        };
        let padding = |width: Option<Picometres>| -> Result<i64, GerberParseError> {
            width.map_or(Ok(0), |width| half_ceil(width.0))
        };
        let mut bounds = GerberBounds::default();
        match geometry {
            Geometry::Point(point) => include_point(&mut bounds, materialize(*point)?, 0)?,
            Geometry::Line(line) => {
                let padding = padding(line.width)?;
                include_point(&mut bounds, materialize(line.start)?, padding)?;
                include_point(&mut bounds, materialize(line.end)?, padding)?;
            }
            Geometry::Arc(arc) => {
                let start = materialize(arc.start)?;
                let end = materialize(arc.end)?;
                let center = materialize(arc.center)?;
                let radius = (start.x.0 - center.x.0)
                    .abs()
                    .checked_add((start.y.0 - center.y.0).abs())
                    .and_then(|radius| radius.checked_add(padding(arc.width).ok()?))
                    .ok_or_else(|| self.semantic(record, "extent-overflow"))?;
                include_point(&mut bounds, center, radius)?;
                include_point(&mut bounds, start, padding(arc.width)?)?;
                include_point(&mut bounds, end, padding(arc.width)?)?;
            }
            Geometry::Contour(contour) => {
                for segment in &contour.segments {
                    let geometry = match segment {
                        ContourSegment::Line(line) => Geometry::Line(line.clone()),
                        ContourSegment::Arc(arc) => Geometry::Arc(arc.clone()),
                    };
                    bounds.merge(self.geometry_bounds(&geometry, transforms, record)?);
                }
            }
            Geometry::Region(region) => {
                for contour in &region.contours {
                    for segment in &contour.segments {
                        let geometry = match segment {
                            ContourSegment::Line(line) => Geometry::Line(line.clone()),
                            ContourSegment::Arc(arc) => Geometry::Arc(arc.clone()),
                        };
                        bounds.merge(self.geometry_bounds(&geometry, transforms, record)?);
                    }
                }
            }
            Geometry::Flash(flash) => {
                let aperture = self
                    .aperture_codes_by_id
                    .get(&flash.aperture_id)
                    .and_then(|code| self.apertures.get(code))
                    .ok_or_else(|| self.semantic(record, "missing-flash-aperture"))?;
                if let (Some(min_x), Some(min_y), Some(max_x), Some(max_y)) = (
                    aperture.bounds.min_x,
                    aperture.bounds.min_y,
                    aperture.bounds.max_x,
                    aperture.bounds.max_y,
                ) {
                    for local in [
                        CanonicalPoint::new(min_x, min_y),
                        CanonicalPoint::new(min_x, max_y),
                        CanonicalPoint::new(max_x, min_y),
                        CanonicalPoint::new(max_x, max_y),
                    ] {
                        let point = CanonicalPoint::new(
                            flash
                                .position
                                .x
                                .0
                                .checked_add(local.x.0)
                                .ok_or_else(|| self.semantic(record, "extent-overflow"))?,
                            flash
                                .position
                                .y
                                .0
                                .checked_add(local.y.0)
                                .ok_or_else(|| self.semantic(record, "extent-overflow"))?,
                        );
                        include_point(&mut bounds, materialize(point)?, 0)?;
                    }
                } else {
                    include_point(&mut bounds, materialize(flash.position)?, 0)?;
                }
            }
            Geometry::Drill(drill) => {
                include_point(
                    &mut bounds,
                    materialize(drill.position)?,
                    half_ceil(drill.diameter.0)?,
                )?;
            }
            Geometry::Route(route) => {
                for segment in &route.segments {
                    let geometry = match segment {
                        ContourSegment::Line(line) => Geometry::Line(line.clone()),
                        ContourSegment::Arc(arc) => Geometry::Arc(arc.clone()),
                    };
                    bounds.merge(self.geometry_bounds(&geometry, transforms, record)?);
                }
            }
            Geometry::Slot(slot) => {
                let padding = half_ceil(slot.width.0)?;
                include_point(&mut bounds, materialize(slot.start)?, padding)?;
                include_point(&mut bounds, materialize(slot.end)?, padding)?;
            }
        }
        Ok(bounds)
    }
}
