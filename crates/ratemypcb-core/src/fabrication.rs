use gerber_parser::gerber_types::{
    Command as ParserCommand, CommentContent as ParserCommentContent, DCode as ParserDCode,
    ExtendedCode as ParserExtendedCode, FunctionCode as ParserFunctionCode, GCode as ParserGCode,
    MCode as ParserMCode, Operation as ParserOperation,
};
use gerber_parser::{ContentError, parse as parse_gerber};
use serde::de::{Error as _, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::{BufReader, Cursor, Read, Write};
use std::time::{Duration, Instant};

mod native;
pub use native::*;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ManufacturingKindCandidate {
    Gerber,
    Excellon,
    GerberJob,
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
    pub file_started: Option<Instant>,
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
    pub aggregate_started: Option<Instant>,
}

impl ManufacturingInventory {
    pub fn validate(&self) -> Result<(), FabricationError> {
        self.validate_with_deadline(ManufacturingDeadline::for_inventory(
            self,
            Duration::from_millis(MANUFACTURING_LIMITS.aggregate_timeout_ms),
        ))
    }

    pub(crate) fn validate_with_deadline(
        &self,
        deadline: ManufacturingDeadline,
    ) -> Result<(), FabricationError> {
        self.validate_with_deadline_observer(deadline, &mut |_| {})
    }

    fn validate_with_deadline_observer(
        &self,
        deadline: ManufacturingDeadline,
        observer: &mut impl FnMut(usize),
    ) -> Result<(), FabricationError> {
        deadline.check("manufacturing-inventory")?;
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
            deadline.check("manufacturing-inventory")?;
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
            deadline.check("manufacturing-inventory")?;
            let size = u64::try_from(input.original_bytes.len())
                .map_err(|_| FabricationError::ArithmeticOverflow)?;
            retained_bytes = retained_bytes
                .checked_add(size)
                .ok_or(FabricationError::ArithmeticOverflow)?;
            if size != input.size
                || size > MANUFACTURING_LIMITS.raw_bytes_per_file
                || sha256_with_deadline_observer(
                    &input.original_bytes,
                    deadline,
                    "manufacturing-inventory-hash",
                    observer,
                )? != input.artifact_digest
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
        deadline.check("manufacturing-inventory")?;
        Ok(())
    }

    #[cfg(test)]
    fn validate_with_deadline_counting(
        &self,
        deadline: ManufacturingDeadline,
        mut observer: impl FnMut(usize),
    ) -> Result<(), FabricationError> {
        self.validate_with_deadline_observer(deadline, &mut observer)
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

#[derive(Clone, Copy, Debug)]
pub(crate) struct ManufacturingDeadline {
    at: Instant,
}

impl ManufacturingDeadline {
    pub(crate) fn from_timeout(timeout: Duration) -> Self {
        let now = Instant::now();
        Self {
            at: now.checked_add(timeout).unwrap_or(now),
        }
    }

    pub(crate) fn from_aggregate_start(aggregate_started: Instant) -> Self {
        let now = Instant::now();
        Self {
            at: aggregate_started
                .checked_add(Duration::from_millis(
                    MANUFACTURING_LIMITS.aggregate_timeout_ms,
                ))
                .unwrap_or(now),
        }
    }

    #[cfg(test)]
    pub(crate) fn from_starts(file_started: Instant, aggregate_started: Instant) -> Self {
        Self::from_aggregate_start(aggregate_started).for_file_started(file_started)
    }

    pub(crate) fn for_file_started(self, file_started: Instant) -> Self {
        let now = Instant::now();
        let file = file_started
            .checked_add(Duration::from_millis(MANUFACTURING_LIMITS.file_timeout_ms))
            .unwrap_or(now);
        Self {
            at: self.at.min(file),
        }
    }

    pub(crate) fn for_inventory(inventory: &ManufacturingInventory, requested: Duration) -> Self {
        let now = Instant::now();
        let contract = inventory
            .aggregate_started
            .unwrap_or(now)
            .checked_add(Duration::from_millis(
                MANUFACTURING_LIMITS.aggregate_timeout_ms,
            ))
            .unwrap_or(now);
        let requested = now.checked_add(requested).unwrap_or(now);
        Self {
            at: contract.min(requested),
        }
    }

    pub(crate) fn for_input(self, input: &ManufacturingInput) -> Self {
        let now = Instant::now();
        let file = input
            .file_started
            .unwrap_or(now)
            .checked_add(Duration::from_millis(MANUFACTURING_LIMITS.file_timeout_ms))
            .unwrap_or(now);
        Self {
            at: self.at.min(file),
        }
    }

    pub(crate) fn with_file_limit(self) -> Self {
        let now = Instant::now();
        let file = now
            .checked_add(Duration::from_millis(MANUFACTURING_LIMITS.file_timeout_ms))
            .unwrap_or(now);
        Self {
            at: self.at.min(file),
        }
    }

    pub(crate) fn with_aggregate_limit(self) -> Self {
        let now = Instant::now();
        let aggregate = now
            .checked_add(Duration::from_millis(
                MANUFACTURING_LIMITS.aggregate_timeout_ms,
            ))
            .unwrap_or(now);
        Self {
            at: self.at.min(aggregate),
        }
    }

    pub(crate) fn check(self, resource: &'static str) -> Result<(), FabricationError> {
        if Instant::now() >= self.at {
            Err(FabricationError::LimitExceeded { resource })
        } else {
            Ok(())
        }
    }
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

const RECONCILIATION_VALUE_BYTES: u64 = MANUFACTURING_LIMITS.canonical_allocation_bytes / 8;

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

pub(crate) fn parse_decimal_microdegrees(value: &str) -> Result<i64, FabricationError> {
    let (negative, unsigned) = value
        .strip_prefix('-')
        .map_or((false, value), |value| (true, value));
    let unsigned = unsigned.strip_prefix('+').unwrap_or(unsigned);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if whole.is_empty()
        || fraction.len() > 6
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(FabricationError::InvalidNumber);
    }
    let scale = 10_i128.pow(fraction.len() as u32);
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<i128>()
            .map_err(|_| FabricationError::InvalidNumber)?
    };
    let mut scaled = whole
        .parse::<i128>()
        .ok()
        .and_then(|whole| whole.checked_mul(scale))
        .and_then(|whole| whole.checked_add(fraction))
        .ok_or(FabricationError::ArithmeticOverflow)?;
    if negative {
        scaled = -scaled;
    }
    scaled
        .checked_mul(1_000_000)
        .and_then(|value| value.checked_div(scale))
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(FabricationError::ArithmeticOverflow)
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
    GerberJob,
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
    pub fn kind_name(&self) -> &'static str {
        self.kind()
    }

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

    fn vertex_count_with_deadline(
        &self,
        deadline: ManufacturingDeadline,
    ) -> Result<usize, FabricationError> {
        deadline.check("fabrication-limits-validation")?;
        if let Self::Region(region) = self {
            let mut count = 0_usize;
            for contour in &region.contours {
                deadline.check("fabrication-limits-validation")?;
                count = count
                    .checked_add(
                        contour
                            .segments
                            .len()
                            .saturating_add(usize::from(contour.closed)),
                    )
                    .ok_or(FabricationError::ArithmeticOverflow)?;
            }
            Ok(count)
        } else {
            Ok(self.vertex_count())
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum FeatureMembership {
    TopLevel,
    ApertureBlock {
        block_id: String,
        aperture_id: String,
    },
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
    pub membership: FeatureMembership,
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
    pub aperture_id: String,
    pub feature_ids: Vec<String>,
    pub instantiation_feature_ids: Vec<String>,
    pub definition_end: StructuralLocation,
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
pub struct DocumentPhysicalBounds {
    pub id: String,
    pub document_id: String,
    pub artifact_digest: String,
    pub format: DocumentFormat,
    pub extent: Extent,
    pub resolution: Picometres,
    pub geometry_digest: String,
    pub source_locations: Vec<StructuralLocation>,
    pub provenance: ManufacturingProvenance,
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

/// Exact same-object native KiCad ownership; no cross-document or proximity association.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PadHoleAssociation {
    pub id: String,
    pub pad_id: String,
    pub hole_id: String,
    pub tool_id: String,
    pub applicable_layer_ids: Vec<String>,
    pub plating: Plating,
    pub span: LayerSpan,
    pub pad_geometry: Geometry,
    pub hole_geometry: Geometry,
    pub pad_provenance: ManufacturingProvenance,
    pub hole_provenance: ManufacturingProvenance,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum X2AttributeScope {
    File,
    Aperture,
    Object,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum X2AttributeKind {
    FileFunction,
    ApertureFunction,
    Net,
    Component,
    Pin,
    Reset,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScopedX2Attribute {
    pub id: String,
    pub document_id: String,
    pub scope: X2AttributeScope,
    pub kind: X2AttributeKind,
    pub values: Vec<String>,
    pub deletion: bool,
    pub target_ids: Vec<String>,
    pub provenance: ManufacturingProvenance,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AssemblyPlacementOrigin {
    KicadBoard,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AssemblySideConvention {
    TopBottom,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AssemblyBottomMirroring {
    Mirrored,
    Unmirrored,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AssemblyRotationDirection {
    CounterClockwise,
    Clockwise,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AssemblyFittedState {
    Fitted,
    NotFitted,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub struct AssemblyPlacementConvention {
    pub unit: Option<SourceUnit>,
    pub origin: AssemblyPlacementOrigin,
    pub side: AssemblySideConvention,
    pub bottom_mirroring: AssemblyBottomMirroring,
    pub rotation_direction: AssemblyRotationDirection,
}

impl AssemblyPlacementConvention {
    pub fn native_kicad() -> Self {
        Self {
            unit: Some(SourceUnit::Millimetre),
            origin: AssemblyPlacementOrigin::KicadBoard,
            side: AssemblySideConvention::TopBottom,
            bottom_mirroring: AssemblyBottomMirroring::Mirrored,
            rotation_direction: AssemblyRotationDirection::CounterClockwise,
        }
    }

    fn complete(self) -> bool {
        self.unit.is_some()
            && self.origin != AssemblyPlacementOrigin::Unknown
            && self.side != AssemblySideConvention::Unknown
            && self.bottom_mirroring != AssemblyBottomMirroring::Unknown
            && self.rotation_direction != AssemblyRotationDirection::Unknown
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AssemblyPlacement {
    pub id: String,
    pub occurrence_id: Option<String>,
    pub reference: String,
    pub side: LayerSide,
    pub position: CanonicalPoint,
    pub rotation_microdegrees: i64,
    pub fitted: AssemblyFittedState,
    pub revision: Option<String>,
    pub convention: AssemblyPlacementConvention,
    pub provenance: ManufacturingProvenance,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeclaredAssemblyPlacement {
    pub id: String,
    pub reference: String,
    pub side: LayerSide,
    pub position: CanonicalPoint,
    pub rotation_microdegrees: i64,
    pub fitted: AssemblyFittedState,
    pub revision: String,
    pub convention: AssemblyPlacementConvention,
    pub source_path: String,
    pub artifact_digest: String,
    pub line: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum NativeCourtyardRunState {
    Complete,
    Partial,
    NotRun,
    Disabled,
    Failed,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum NativeCourtyardKind {
    Overlap,
    Malformed,
    Missing,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum NativeExclusionState {
    Active,
    Excluded,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeCourtyardObservation {
    pub id: String,
    pub kind: NativeCourtyardKind,
    pub exclusion: NativeExclusionState,
    pub location: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeCourtyardEvidence {
    pub state: NativeCourtyardRunState,
    pub tool: String,
    pub version: Option<String>,
    pub source: Option<String>,
    pub observations: Vec<NativeCourtyardObservation>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AssemblyEvidence {
    pub placements: Vec<AssemblyPlacement>,
    pub declared_placements: Vec<DeclaredAssemblyPlacement>,
    pub native_courtyard: Option<NativeCourtyardEvidence>,
    pub mask_layer_ids: Vec<String>,
    pub paste_layer_ids: Vec<String>,
}

pub fn normalize_native_courtyard_report(
    report: &crate::NativeDrc,
) -> Result<NativeCourtyardEvidence, FabricationError> {
    native::normalize_native_courtyard_report(report)
}

pub(crate) fn retain_native_assembly_only(
    target: &mut FabricationReview,
    source: &FabricationReview,
    deadline: ManufacturingDeadline,
) -> Result<(), FabricationError> {
    native::retain_native_assembly_only(target, source, deadline)
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
        CapabilityId::PackageReconciliation,
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

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationFamily {
    Product,
    Layers,
    Profile,
    Drills,
    Extents,
    Connectivity,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationStatus {
    Match,
    Mismatch,
    NotChecked,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationConfidence {
    Exact,
    ResolutionBounded,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReconciliationFact {
    pub model_ids: Vec<String>,
    pub canonical_value: String,
    pub resolution: Option<Picometres>,
    pub authority: Authority,
    pub provenance: ManufacturingProvenance,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManufacturingReconciliation {
    pub id: String,
    pub family: ReconciliationFamily,
    pub status: ReconciliationStatus,
    pub confidence: ReconciliationConfidence,
    pub native: ReconciliationFact,
    pub package: ReconciliationFact,
    pub smallest_evidence_action: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JobFileFunctionFact {
    pub id: String,
    pub job_document_id: String,
    pub job_artifact_digest: String,
    pub referenced_virtual_path: String,
    pub referenced_document_id: String,
    pub referenced_artifact_digest: String,
    pub fields: Vec<String>,
    pub role: LayerRole,
    pub side: LayerSide,
    pub order: Option<i32>,
    pub plating: Plating,
    pub from_layer: Option<i32>,
    pub to_layer: Option<i32>,
    pub qualifier: Option<String>,
    pub operation: Option<String>,
    pub omission: Option<String>,
    pub conflict_ids: Vec<String>,
    pub provenance: ManufacturingProvenance,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum IntegratedReconciliationState {
    NotProvided,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IntegratedReconciliationOutcome {
    pub id: String,
    pub state: IntegratedReconciliationState,
    pub attempted_native_path: Option<String>,
    pub attempted_native_digest: Option<String>,
    pub reason: String,
}

impl IntegratedReconciliationOutcome {
    pub fn new(
        state: IntegratedReconciliationState,
        attempted_native_path: Option<String>,
        attempted_native_digest: Option<String>,
        reason: impl Into<String>,
    ) -> Result<Self, FabricationError> {
        let mut outcome = Self {
            id: String::new(),
            state,
            attempted_native_path,
            attempted_native_digest,
            reason: reason.into(),
        };
        let valid = match outcome.state {
            IntegratedReconciliationState::NotProvided => {
                outcome.attempted_native_path.is_none() && outcome.attempted_native_digest.is_none()
            }
            IntegratedReconciliationState::Failed => {
                outcome
                    .attempted_native_path
                    .as_deref()
                    .is_some_and(valid_virtual_path)
                    && outcome
                        .attempted_native_digest
                        .as_deref()
                        .is_some_and(lowercase_sha256)
            }
        };
        if outcome.reason.is_empty() || !valid {
            return Err(FabricationError::InvalidIdentity(
                "integration-outcome".into(),
            ));
        }
        outcome.id = integration_outcome_id(&outcome);
        Ok(outcome)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManufacturingSourcePair {
    pub id: String,
    pub native_document_id: String,
    pub native_artifact_digest: String,
    pub release_package_id: String,
    pub release_document_digests: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManufacturingWarning {
    pub code: String,
    pub message: String,
    pub provenance: Option<ManufacturingProvenance>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeReconciliationSource {
    pub review: Box<FabricationReview>,
    pub extents: Option<Extent>,
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
    pub physical_bounds: Vec<DocumentPhysicalBounds>,
    pub profile: Option<BoardProfile>,
    pub connectivity: Vec<ObjectSemantics>,
    pub pad_hole_associations: Vec<PadHoleAssociation>,
    pub x2_attributes: Vec<ScopedX2Attribute>,
    pub job_file_functions: Vec<JobFileFunctionFact>,
    pub assembly: AssemblyEvidence,
    pub construction: ConstructionEvidence,
    pub constraints: Vec<ManufacturingConstraint>,
    pub capabilities: CapabilityLedger,
    pub omissions: Vec<Omission>,
    pub conflicts: Vec<Conflict>,
    pub source_pair: Option<ManufacturingSourcePair>,
    pub native_reconciliation_source: Option<NativeReconciliationSource>,
    pub integration_outcome: Option<IntegratedReconciliationOutcome>,
    pub reconciliations: Vec<ManufacturingReconciliation>,
    pub warnings: Vec<ManufacturingWarning>,
    pub limits: ManufacturingLimits,
    pub estimated_allocation_bytes: u64,
}

impl FabricationReview {
    fn empty_unfinalized() -> Self {
        Self {
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
            physical_bounds: vec![],
            profile: None,
            connectivity: vec![],
            pad_hole_associations: vec![],
            x2_attributes: vec![],
            job_file_functions: vec![],
            assembly: AssemblyEvidence::default(),
            construction: ConstructionEvidence::default(),
            constraints: vec![],
            capabilities: CapabilityLedger::default(),
            omissions: vec![],
            conflicts: vec![],
            source_pair: None,
            native_reconciliation_source: None,
            integration_outcome: None,
            reconciliations: vec![],
            warnings: vec![],
            limits: MANUFACTURING_LIMITS,
            estimated_allocation_bytes: 0,
        }
    }

    fn empty_with_deadline(deadline: ManufacturingDeadline) -> Result<Self, FabricationError> {
        let mut review = Self::empty_unfinalized();
        review.refresh_digests_with_deadline(deadline)?;
        Ok(review)
    }
}

impl Default for FabricationReview {
    fn default() -> Self {
        Self::empty_with_deadline(ManufacturingDeadline::from_timeout(Duration::from_millis(
            MANUFACTURING_LIMITS.aggregate_timeout_ms,
        )))
        .expect("empty model is serializable")
    }
}

pub fn legacy_inventory_review(
    inventory: &ManufacturingInventory,
) -> Result<FabricationReview, FabricationError> {
    legacy_inventory_review_with_deadline(
        inventory,
        ManufacturingDeadline::for_inventory(
            inventory,
            Duration::from_millis(MANUFACTURING_LIMITS.aggregate_timeout_ms),
        ),
    )
}

pub(crate) fn legacy_inventory_review_with_deadline(
    inventory: &ManufacturingInventory,
    deadline: ManufacturingDeadline,
) -> Result<FabricationReview, FabricationError> {
    inventory.validate_with_deadline(deadline)?;
    let mut review = FabricationReview::empty_with_deadline(deadline)?;
    review.status = if inventory.outcomes.is_empty() {
        FabricationStatus::NotProvided
    } else if inventory.inputs.is_empty() {
        FabricationStatus::Failed
    } else {
        FabricationStatus::Partial
    };
    review.input_outcomes = inventory.outcomes.clone();
    for outcome in inventory
        .outcomes
        .iter()
        .filter(|outcome| outcome.artifact_digest.is_some())
        .take(MANUFACTURING_LIMITS.recognized_files)
    {
        let format = match outcome.kind_candidate {
            ManufacturingKindCandidate::Gerber => DocumentFormat::Gerber,
            ManufacturingKindCandidate::Excellon => DocumentFormat::Excellon,
            ManufacturingKindCandidate::GerberJob => DocumentFormat::GerberJob,
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
    review.refresh_digests_with_deadline(deadline)?;
    review.validate_with_deadline(deadline)?;
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

pub fn layer_id(
    document_id: &str,
    name: Option<&str>,
    role: LayerRole,
    side: LayerSide,
    order: Option<i32>,
    authority: Authority,
    location: &StructuralLocation,
) -> String {
    stable_id(
        "layer",
        &(document_id, name, role, side, order, authority, location),
    )
    .expect("identity tuple serializes")
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
    feature_id_with_membership(
        document_id,
        layer_id,
        semantic_kind,
        location,
        &FeatureMembership::TopLevel,
    )
}

pub fn feature_id_with_membership(
    document_id: &str,
    layer_id: &str,
    semantic_kind: &str,
    location: &StructuralLocation,
    membership: &FeatureMembership,
) -> String {
    match membership {
        FeatureMembership::TopLevel => {
            stable_id("feature", &(document_id, layer_id, semantic_kind, location))
        }
        FeatureMembership::ApertureBlock {
            block_id,
            aperture_id,
        } => stable_id(
            "feature",
            &(
                document_id,
                layer_id,
                semantic_kind,
                location,
                "aperture-block",
                block_id,
                aperture_id,
            ),
        ),
    }
    .expect("feature identity tuple serializes")
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

fn source_pair_id_with_deadline(
    native_document_id: &str,
    native_artifact_digest: &str,
    release_package_id: &str,
    release_document_digests: &[String],
    deadline: ManufacturingDeadline,
) -> Result<String, FabricationError> {
    let mut digests = BTreeSet::new();
    for digest in release_document_digests {
        deadline.check("source-pair-identity")?;
        digests.insert(digest);
    }
    stable_id_with_deadline(
        deadline,
        "source-pair-identity",
        "source-pair",
        &(
            native_document_id,
            native_artifact_digest,
            release_package_id,
            digests,
        ),
    )
}

fn reconciliation_id_with_deadline(
    family: ReconciliationFamily,
    native: &ReconciliationFact,
    package: &ReconciliationFact,
    deadline: ManufacturingDeadline,
) -> Result<String, FabricationError> {
    stable_id_with_deadline(
        deadline,
        "reconciliation-identity",
        "reconciliation",
        &(
            family,
            &native.model_ids,
            &native.canonical_value,
            native.resolution,
            native.authority,
            &native.provenance.document_id,
            &native.provenance.artifact_digest,
            &native.provenance.location,
            &package.model_ids,
            &package.canonical_value,
            package.resolution,
            package.authority,
            &package.provenance.document_id,
            &package.provenance.artifact_digest,
            &package.provenance.location,
        ),
    )
}

fn stable_id(kind: &str, fields: &impl Serialize) -> Result<String, FabricationError> {
    let mut writer = DeadlineWriter::unbounded("fabrication-identity", true);
    serde_json::to_writer(&mut writer, &("fabrication-identity-v1", kind, fields))
        .map_err(|error| FabricationError::Serialization(error.to_string()))?;
    if writer.overflow {
        return Err(FabricationError::ArithmeticOverflow);
    }
    Ok(format!(
        "{kind}-v1-{}",
        writer.digest().expect("identity hashing enabled")
    ))
}

fn pad_id(document_id: &str, location: &StructuralLocation) -> Result<String, FabricationError> {
    stable_id("pad", &(document_id, location))
}

pub(crate) fn assembly_placement_id(
    document_id: &str,
    occurrence_id: Option<&str>,
    reference: &str,
    location: &StructuralLocation,
) -> Result<String, FabricationError> {
    stable_id(
        "assembly-placement",
        &(document_id, occurrence_id, reference, location),
    )
}

pub(crate) fn declared_assembly_placement_id(
    source_path: &str,
    artifact_digest: &str,
    line: u64,
    reference: &str,
) -> Result<String, FabricationError> {
    stable_id(
        "declared-assembly-placement",
        &(source_path, artifact_digest, line, reference),
    )
}

fn native_courtyard_observation_id(
    kind: NativeCourtyardKind,
    exclusion: NativeExclusionState,
    tool: &str,
    version: &str,
    location: &str,
) -> Result<String, FabricationError> {
    stable_id(
        "native-courtyard-observation",
        &(kind, exclusion, tool, version, location),
    )
}

fn pad_hole_association_id(association: &PadHoleAssociation) -> Result<String, FabricationError> {
    stable_id(
        "pad-hole-association",
        &(
            &association.pad_id,
            &association.hole_id,
            &association.tool_id,
            &association.applicable_layer_ids,
            association.plating,
            &association.span,
            &association.pad_geometry,
            &association.hole_geometry,
            canonical_provenance(&association.pad_provenance),
            canonical_provenance(&association.hole_provenance),
        ),
    )
}

fn stable_id_with_deadline(
    deadline: ManufacturingDeadline,
    resource: &'static str,
    kind: &str,
    fields: &impl Serialize,
) -> Result<String, FabricationError> {
    Ok(format!(
        "{kind}-v1-{}",
        hash_serialized_with_deadline(
            deadline,
            resource,
            &("fabrication-identity-v1", kind, fields),
        )?
    ))
}

impl FabricationReview {
    pub fn refresh_digests(&mut self) -> Result<(), FabricationError> {
        self.refresh_digests_with_deadline(ManufacturingDeadline::from_timeout(
            Duration::from_millis(MANUFACTURING_LIMITS.aggregate_timeout_ms),
        ))
    }

    pub fn refresh_physical_bounds(&mut self) -> Result<(), FabricationError> {
        let deadline = ManufacturingDeadline::from_timeout(Duration::from_millis(
            MANUFACTURING_LIMITS.aggregate_timeout_ms,
        ));
        self.physical_bounds =
            derive_release_physical_bounds(self, ReconciliationBudget { deadline })?;
        Ok(())
    }

    pub(crate) fn refresh_digests_with_deadline(
        &mut self,
        deadline: ManufacturingDeadline,
    ) -> Result<(), FabricationError> {
        deadline.check("fabrication-digest-refresh")?;
        self.package_id = self.expected_package_id(deadline)?;
        self.model_digest = self.expected_model_digest(deadline)?;
        self.estimated_allocation_bytes = self.estimate_allocation(deadline)?;
        deadline.check("fabrication-digest-refresh")?;
        Ok(())
    }

    fn finalize_trusted_with_deadline(
        &mut self,
        deadline: ManufacturingDeadline,
    ) -> Result<(), FabricationError> {
        deadline.check("fabrication-finalization")?;
        if self.limits != MANUFACTURING_LIMITS {
            return Err(FabricationError::LimitExceeded {
                resource: "limits-contract",
            });
        }
        self.validate_limits(deadline)?;
        self.physical_bounds =
            derive_release_physical_bounds(self, ReconciliationBudget { deadline })?;
        self.validate_identities_and_references_with_deadline(deadline, false)?;
        self.package_id = self.expected_package_id(deadline)?;
        self.model_digest = self.expected_model_digest(deadline)?;
        self.estimated_allocation_bytes = self.estimate_allocation(deadline)?;
        if self.estimated_allocation_bytes > self.limits.canonical_allocation_bytes {
            return Err(FabricationError::LimitExceeded {
                resource: "canonical-allocation",
            });
        }
        deadline.check("fabrication-finalization")?;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), FabricationError> {
        self.validate_with_deadline(ManufacturingDeadline::from_timeout(Duration::from_millis(
            MANUFACTURING_LIMITS.aggregate_timeout_ms,
        )))
    }

    pub(crate) fn validate_with_deadline(
        &self,
        deadline: ManufacturingDeadline,
    ) -> Result<(), FabricationError> {
        deadline.check("fabrication-validation")?;
        if self.limits != MANUFACTURING_LIMITS {
            return Err(FabricationError::LimitExceeded {
                resource: "limits-contract",
            });
        }
        self.validate_limits(deadline)?;
        self.validate_identities_and_references_with_deadline(deadline, true)?;
        if self.package_id != self.expected_package_id(deadline)? {
            return Err(FabricationError::PackageIdentityMismatch);
        }
        if !lowercase_sha256(&self.model_digest)
            || self.model_digest != self.expected_model_digest(deadline)?
        {
            return Err(FabricationError::DigestMismatch);
        }
        let estimated = self.estimate_allocation(deadline)?;
        if self.estimated_allocation_bytes != estimated {
            return Err(FabricationError::AllocationEstimateMismatch);
        }
        if estimated > self.limits.canonical_allocation_bytes {
            return Err(FabricationError::LimitExceeded {
                resource: "canonical-allocation",
            });
        }
        deadline.check("fabrication-validation")?;
        Ok(())
    }

    fn expected_package_id(
        &self,
        deadline: ManufacturingDeadline,
    ) -> Result<String, FabricationError> {
        let mut documents = BTreeSet::new();
        for document in &self.documents {
            deadline.check("fabrication-package-identity")?;
            documents.insert(&document.id);
        }
        let product = self.product.as_ref().map(|product| {
            (
                product.name.as_deref(),
                product.revision.as_deref(),
                product.part_number.as_deref(),
                product.authority,
            )
        });
        stable_id_with_deadline(
            deadline,
            "fabrication-package-identity",
            "package",
            &(documents, product),
        )
    }

    fn expected_model_digest(
        &self,
        deadline: ManufacturingDeadline,
    ) -> Result<String, FabricationError> {
        let mut records = BTreeSet::new();
        let mut feature_records = BTreeMap::new();
        let mut aperture_records = BTreeMap::new();
        let mut macro_records = BTreeMap::new();
        let mut block_records = BTreeMap::new();
        let mut repeat_records = BTreeMap::new();
        let mut physically_bound_documents = BTreeSet::new();
        for bounds in &self.physical_bounds {
            deadline.check("fabrication-model-digest")?;
            physically_bound_documents.insert(bounds.document_id.as_str());
        }
        records.insert(canonical_json_with_deadline(
            deadline,
            "status",
            &self.status,
        )?);
        records.insert(canonical_json_with_deadline(
            deadline,
            "package",
            &self.package_id,
        )?);
        records.insert(canonical_json_with_deadline(
            deadline,
            "limits",
            &self.limits,
        )?);
        for outcome in &self.input_outcomes {
            deadline.check("fabrication-model-digest")?;
            records.insert(canonical_json_with_deadline(
                deadline,
                "input-outcome",
                outcome,
            )?);
        }
        if let Some(product) = &self.product {
            records.insert(canonical_json_with_deadline(
                deadline,
                "product",
                &(
                    &product.name,
                    &product.revision,
                    &product.part_number,
                    product.authority,
                    canonical_provenances(&product.provenance, deadline)?,
                ),
            )?);
        }
        for document in &self.documents {
            deadline.check("fabrication-model-digest")?;
            records.insert(canonical_json_with_deadline(
                deadline,
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
            deadline.check("fabrication-model-digest")?;
            records.insert(canonical_json_with_deadline(
                deadline,
                "layer",
                &(
                    &layer.id,
                    &layer.document_id,
                    &layer.name,
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
            deadline.check("fabrication-model-digest")?;
            records.insert(canonical_json_with_deadline(
                deadline,
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
        for (index, aperture) in self.apertures.iter().enumerate() {
            if index % 1024 == 0 {
                deadline.check("fabrication-model-digest")?;
            }
            if !physically_bound_documents.contains(aperture.document_id.as_str()) {
                aperture_records.insert(
                    aperture.id.as_str(),
                    (
                        aperture.shape,
                        &aperture.dimensions,
                        aperture.polygon_vertices,
                        aperture.polygon_rotation_microdegrees,
                        &aperture.macro_id,
                        &aperture.macro_arguments,
                    ),
                );
            }
        }
        for (index, definition) in self.macros.iter().enumerate() {
            if index % 1024 == 0 {
                deadline.check("fabrication-model-digest")?;
            }
            if !physically_bound_documents.contains(definition.document_id.as_str()) {
                macro_records.insert(
                    definition.id.as_str(),
                    (
                        &definition.name,
                        &definition.variables,
                        &definition.operations,
                    ),
                );
            }
        }
        for (index, block) in self.blocks.iter().enumerate() {
            if index % 1024 == 0 {
                deadline.check("fabrication-model-digest")?;
            }
            if !physically_bound_documents.contains(block.document_id.as_str()) {
                block_records.insert(
                    block.id.as_str(),
                    (
                        &block.aperture_id,
                        &block.feature_ids,
                        &block.instantiation_feature_ids,
                        &block.definition_end,
                    ),
                );
            }
        }
        for (index, repeat) in self.repetitions.iter().enumerate() {
            if index % 1024 == 0 {
                deadline.check("fabrication-model-digest")?;
            }
            if !physically_bound_documents.contains(repeat.document_id.as_str()) {
                repeat_records.insert(
                    repeat.id.as_str(),
                    (
                        &repeat.feature_ids,
                        repeat.x_count,
                        repeat.y_count,
                        repeat.x_step,
                        repeat.y_step,
                    ),
                );
            }
        }
        for feature in &self.features {
            deadline.check("fabrication-model-digest")?;
            if physically_bound_documents.contains(feature.document_id.as_str()) {
                continue;
            }
            feature_records.insert(
                feature.id.as_str(),
                (
                    &feature.tool_id,
                    feature.polarity,
                    &feature.geometry,
                    &feature.transforms,
                    &feature.membership,
                ),
            );
        }
        if !(aperture_records.is_empty()
            && macro_records.is_empty()
            && block_records.is_empty()
            && repeat_records.is_empty()
            && feature_records.is_empty())
        {
            records.insert(format!(
                "retained-geometry-v4:{}",
                hash_serialized_with_deadline(
                    deadline,
                    "fabrication-model-digest",
                    &(
                        aperture_records,
                        macro_records,
                        block_records,
                        repeat_records,
                        feature_records,
                    ),
                )?
            ));
        }
        for bounds in &self.physical_bounds {
            deadline.check("fabrication-model-digest")?;
            records.insert(canonical_json_with_deadline(
                deadline,
                "physical-bounds",
                &(
                    &bounds.id,
                    &bounds.document_id,
                    &bounds.artifact_digest,
                    bounds.format,
                    &bounds.extent,
                    bounds.resolution,
                    &bounds.geometry_digest,
                    &bounds.source_locations,
                    canonical_provenance(&bounds.provenance),
                ),
            )?);
        }
        if let Some(profile) = &self.profile {
            let contours = canonical_refs(&profile.contour_feature_ids, deadline)?;
            let cutouts = canonical_refs(&profile.cutout_feature_ids, deadline)?;
            records.insert(canonical_json_with_deadline(
                deadline,
                "profile",
                &(
                    contours,
                    cutouts,
                    &profile.extents,
                    canonical_provenances(&profile.provenance, deadline)?,
                ),
            )?);
        }
        for semantic in &self.connectivity {
            deadline.check("fabrication-model-digest")?;
            records.insert(canonical_json_with_deadline(
                deadline,
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
        for association in &self.pad_hole_associations {
            deadline.check("fabrication-model-digest")?;
            records.insert(canonical_json_with_deadline(
                deadline,
                "pad-hole-association",
                &(
                    &association.id,
                    &association.pad_id,
                    &association.hole_id,
                    &association.tool_id,
                    canonical_refs(&association.applicable_layer_ids, deadline)?,
                    association.plating,
                    &association.span,
                    &association.pad_geometry,
                    &association.hole_geometry,
                    canonical_provenance(&association.pad_provenance),
                    canonical_provenance(&association.hole_provenance),
                ),
            )?);
        }
        for attribute in &self.x2_attributes {
            deadline.check("fabrication-model-digest")?;
            let targets = canonical_refs(&attribute.target_ids, deadline)?;
            records.insert(canonical_json_with_deadline(
                deadline,
                "x2-attribute",
                &(
                    &attribute.id,
                    &attribute.document_id,
                    attribute.scope,
                    attribute.kind,
                    &attribute.values,
                    attribute.deletion,
                    targets,
                    canonical_provenance(&attribute.provenance),
                ),
            )?);
        }
        for fact in &self.job_file_functions {
            deadline.check("fabrication-model-digest")?;
            records.insert(canonical_json_with_deadline(
                deadline,
                "job-file-function",
                fact,
            )?);
        }
        for placement in &self.assembly.placements {
            deadline.check("fabrication-model-digest")?;
            records.insert(canonical_json_with_deadline(
                deadline,
                "placement",
                &(
                    &placement.id,
                    &placement.occurrence_id,
                    &placement.reference,
                    placement.side,
                    placement.position,
                    placement.rotation_microdegrees,
                    placement.fitted,
                    &placement.revision,
                    placement.convention,
                    canonical_provenance(&placement.provenance),
                ),
            )?);
        }
        for placement in &self.assembly.declared_placements {
            deadline.check("fabrication-model-digest")?;
            records.insert(canonical_json_with_deadline(
                deadline,
                "declared-placement",
                placement,
            )?);
        }
        if let Some(courtyard) = &self.assembly.native_courtyard {
            records.insert(canonical_json_with_deadline(
                deadline,
                "native-courtyard",
                courtyard,
            )?);
        }
        let mask_layers = canonical_refs(&self.assembly.mask_layer_ids, deadline)?;
        let paste_layers = canonical_refs(&self.assembly.paste_layer_ids, deadline)?;
        records.insert(canonical_json_with_deadline(
            deadline,
            "assembly-layers",
            &(mask_layers, paste_layers),
        )?);
        for layer in &self.construction.layers {
            deadline.check("fabrication-model-digest")?;
            records.insert(canonical_json_with_deadline(
                deadline,
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
        records.insert(canonical_json_with_deadline(
            deadline,
            "construction",
            &(
                &self.construction.total_thickness,
                &self.construction.finish,
            ),
        )?);
        for constraint in &self.constraints {
            deadline.check("fabrication-model-digest")?;
            records.insert(canonical_json_with_deadline(
                deadline,
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
            deadline.check("fabrication-model-digest")?;
            let documents = canonical_refs(&capability.document_ids, deadline)?;
            records.insert(canonical_json_with_deadline(
                deadline,
                "capability",
                &(
                    capability.id,
                    capability.state,
                    capability.authority,
                    documents,
                    canonical_provenances(&capability.provenance, deadline)?,
                ),
            )?);
        }
        for omission in &self.omissions {
            deadline.check("fabrication-model-digest")?;
            let affected = canonical_refs(&omission.affected_capabilities, deadline)?;
            records.insert(canonical_json_with_deadline(
                deadline,
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
            deadline.check("fabrication-model-digest")?;
            let affected = canonical_refs(&conflict.affected_capabilities, deadline)?;
            records.insert(canonical_json_with_deadline(
                deadline,
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
        if let Some(pair) = &self.source_pair {
            let digests = canonical_refs(&pair.release_document_digests, deadline)?;
            records.insert(canonical_json_with_deadline(
                deadline,
                "source-pair",
                &(
                    &pair.id,
                    &pair.native_document_id,
                    &pair.native_artifact_digest,
                    &pair.release_package_id,
                    digests,
                ),
            )?);
        }
        if let Some(source) = &self.native_reconciliation_source {
            records.insert(canonical_json_with_deadline(
                deadline,
                "native-reconciliation-source",
                &(&source.review.model_digest, &source.extents),
            )?);
        }
        if let Some(outcome) = &self.integration_outcome {
            records.insert(canonical_json_with_deadline(
                deadline,
                "integration-outcome",
                outcome,
            )?);
        }
        for reconciliation in &self.reconciliations {
            deadline.check("fabrication-model-digest")?;
            let native_ids = canonical_refs(&reconciliation.native.model_ids, deadline)?;
            let package_ids = canonical_refs(&reconciliation.package.model_ids, deadline)?;
            records.insert(canonical_json_with_deadline(
                deadline,
                "reconciliation",
                &(
                    &reconciliation.id,
                    reconciliation.family,
                    reconciliation.status,
                    reconciliation.confidence,
                    native_ids,
                    &reconciliation.native.canonical_value,
                    reconciliation.native.resolution,
                    reconciliation.native.authority,
                    canonical_provenance(&reconciliation.native.provenance),
                    package_ids,
                    &reconciliation.package.canonical_value,
                    reconciliation.package.resolution,
                    reconciliation.package.authority,
                    canonical_provenance(&reconciliation.package.provenance),
                ),
            )?);
        }
        deadline.check("fabrication-model-digest")?;
        let mut hasher = Sha256::new();
        let mut first = true;
        for record in records {
            deadline.check("fabrication-model-digest")?;
            if !first {
                hasher.update(b"\n");
            }
            update_sha256_with_deadline(
                &mut hasher,
                record.as_bytes(),
                deadline,
                "fabrication-model-digest",
            )?;
            first = false;
        }
        deadline.check("fabrication-model-digest")?;
        Ok(format!("{:x}", hasher.finalize()))
    }
    fn expanded_feature_instances(
        &self,
        deadline: ManufacturingDeadline,
    ) -> Result<u64, FabricationError> {
        let mut total =
            u64::try_from(self.features.len()).map_err(|_| FabricationError::ArithmeticOverflow)?;
        let mut originals_reused = BTreeSet::new();
        for repeat in &self.repetitions {
            deadline.check("fabrication-expansion-validation")?;
            let grid = u64::from(repeat.x_count)
                .checked_mul(u64::from(repeat.y_count))
                .ok_or(FabricationError::ArithmeticOverflow)?;
            let feature_count = u64::try_from(repeat.feature_ids.len())
                .map_err(|_| FabricationError::ArithmeticOverflow)?;
            let repeated = feature_count
                .checked_mul(grid)
                .ok_or(FabricationError::ArithmeticOverflow)?;
            let mut newly_reused_originals = 0_u64;
            for feature_id in &repeat.feature_ids {
                deadline.check("fabrication-expansion-validation")?;
                if originals_reused.insert(feature_id.as_str()) {
                    newly_reused_originals = newly_reused_originals
                        .checked_add(1)
                        .ok_or(FabricationError::ArithmeticOverflow)?;
                }
            }
            total = total
                .checked_add(repeated)
                .and_then(|total| total.checked_sub(newly_reused_originals))
                .ok_or(FabricationError::ArithmeticOverflow)?;
        }
        Ok(total)
    }

    fn validate_limits(&self, deadline: ManufacturingDeadline) -> Result<(), FabricationError> {
        deadline.check("fabrication-limits-validation")?;
        if self.documents.len() > self.limits.recognized_files
            || self.input_outcomes.len() > self.limits.archive_entries
            || self.features.len() > self.limits.geometry_features
            || self.layers.len() > self.limits.geometry_features
            || self.blocks.len() > self.limits.geometry_features
            || self.repetitions.len() > self.limits.geometry_features
            || self.connectivity.len() > self.limits.geometry_features
            || self.pad_hole_associations.len() > self.limits.geometry_features
            || self.x2_attributes.len() > self.limits.geometry_features
            || self.physical_bounds.len() > self.limits.recognized_files
            || self.job_file_functions.len() > self.limits.recognized_files
            || self.constraints.len() > self.limits.geometry_features
            || self.capabilities.records.len() > self.limits.geometry_features
            || self.omissions.len() > self.limits.geometry_features
            || self.conflicts.len() > self.limits.geometry_features
            || self.reconciliations.len() > 6
            || self.warnings.len() > self.limits.geometry_features
            || self.assembly.placements.len() > self.limits.geometry_features
            || self.assembly.declared_placements.len() > self.limits.geometry_features
            || self
                .assembly
                .native_courtyard
                .as_ref()
                .is_some_and(|evidence| evidence.observations.len() > self.limits.geometry_features)
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
            deadline.check("fabrication-limits-validation")?;
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
        let mut retained_documents = BTreeSet::new();
        for document in &self.documents {
            deadline.check("fabrication-limits-validation")?;
            let metrics = &document.metrics;
            retained_documents.insert((
                document.virtual_path.as_str(),
                document.artifact_digest.as_str(),
                document.metrics.raw_bytes,
            ));
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
        for outcome in self
            .input_outcomes
            .iter()
            .filter(|outcome| outcome.state == ManufacturingLoadState::Retained)
        {
            deadline.check("fabrication-limits-validation")?;
            if !retained_documents.contains(&(
                outcome.virtual_path.as_str(),
                outcome
                    .artifact_digest
                    .as_deref()
                    .expect("retained input digest"),
                outcome.size,
            )) {
                return Err(FabricationError::DanglingReference(
                    "retained-manufacturing-input".into(),
                ));
            }
        }
        if raw > self.limits.raw_bytes_aggregate
            || records > self.limits.records_aggregate
            || tokens > self.limits.lexical_tokens_aggregate
        {
            return Err(FabricationError::LimitExceeded {
                resource: "aggregate-input",
            });
        }
        for repeat in &self.repetitions {
            deadline.check("fabrication-limits-validation")?;
            if repeat.feature_ids.len() > self.limits.geometry_features
                || repeat.x_count == 0
                || repeat.y_count == 0
                || repeat.x_count > self.limits.repeat_factor
                || repeat.y_count > self.limits.repeat_factor
            {
                return Err(FabricationError::LimitExceeded {
                    resource: "definition-expansion",
                });
            }
        }
        if self.expanded_feature_instances(deadline)?
            > u64::try_from(self.limits.geometry_features)
                .map_err(|_| FabricationError::ArithmeticOverflow)?
        {
            return Err(FabricationError::LimitExceeded {
                resource: "definition-expansion",
            });
        }
        let mut macro_variables = 0_usize;
        for item in &self.macros {
            deadline.check("fabrication-limits-validation")?;
            macro_variables = macro_variables
                .checked_add(item.variables.len())
                .ok_or(FabricationError::ArithmeticOverflow)?;
            if item.operations.len() > self.limits.operations_per_macro {
                return Err(FabricationError::LimitExceeded {
                    resource: "definition-expansion",
                });
            }
            for text in item.variables.iter().chain(item.operations.iter()) {
                deadline.check("fabrication-limits-validation")?;
                if text.len() > self.limits.max_text_bytes {
                    return Err(FabricationError::LimitExceeded {
                        resource: "definition-expansion",
                    });
                }
            }
        }
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
        {
            return Err(FabricationError::LimitExceeded {
                resource: "definition-expansion",
            });
        }
        for item in &self.physical_bounds {
            deadline.check("fabrication-limits-validation")?;
            if item.source_locations.len() > self.limits.geometry_features {
                return Err(FabricationError::LimitExceeded {
                    resource: "definition-expansion",
                });
            }
        }
        for item in &self.job_file_functions {
            deadline.check("fabrication-limits-validation")?;
            if item.fields.len() > 8 || item.conflict_ids.len() > self.limits.geometry_features {
                return Err(FabricationError::LimitExceeded {
                    resource: "definition-expansion",
                });
            }
        }
        for item in &self.pad_hole_associations {
            deadline.check("fabrication-limits-validation")?;
            if item.applicable_layer_ids.is_empty()
                || item.applicable_layer_ids.len() > self.limits.geometry_features
                || item.pad_geometry.vertex_count_with_deadline(deadline)?
                    > self.limits.contour_vertices
                || item.hole_geometry.vertex_count_with_deadline(deadline)?
                    > self.limits.contour_vertices
            {
                return Err(FabricationError::LimitExceeded {
                    resource: "definition-expansion",
                });
            }
        }
        for item in &self.apertures {
            deadline.check("fabrication-limits-validation")?;
            if item.dimensions.len() > self.limits.geometry_features {
                return Err(FabricationError::LimitExceeded {
                    resource: "definition-expansion",
                });
            }
        }
        for item in &self.blocks {
            deadline.check("fabrication-limits-validation")?;
            if item.feature_ids.len() > self.limits.geometry_features {
                return Err(FabricationError::LimitExceeded {
                    resource: "definition-expansion",
                });
            }
        }
        for item in &self.capabilities.records {
            deadline.check("fabrication-limits-validation")?;
            if item.document_ids.len() > self.limits.recognized_files
                || item.provenance.len() > self.limits.geometry_features
            {
                return Err(FabricationError::LimitExceeded {
                    resource: "definition-expansion",
                });
            }
        }
        for item in &self.omissions {
            deadline.check("fabrication-limits-validation")?;
            if item.affected_capabilities.len() > self.limits.geometry_features {
                return Err(FabricationError::LimitExceeded {
                    resource: "definition-expansion",
                });
            }
        }
        for item in &self.conflicts {
            deadline.check("fabrication-limits-validation")?;
            if item.affected_capabilities.len() > self.limits.geometry_features {
                return Err(FabricationError::LimitExceeded {
                    resource: "definition-expansion",
                });
            }
        }
        let mut vertices = 0_usize;
        let mut drill_routes = 0_usize;
        for item in &self.features {
            deadline.check("fabrication-limits-validation")?;
            if item.transforms.operations.len() > usize::from(self.limits.max_nesting) {
                return Err(FabricationError::LimitExceeded {
                    resource: "definition-expansion",
                });
            }
            vertices = vertices
                .checked_add(item.geometry.vertex_count_with_deadline(deadline)?)
                .ok_or(FabricationError::ArithmeticOverflow)?;
            drill_routes = drill_routes
                .checked_add(match &item.geometry {
                    Geometry::Drill(_) | Geometry::Slot(_) => 1,
                    Geometry::Route(route) => route.segments.len(),
                    _ => 0,
                })
                .ok_or(FabricationError::ArithmeticOverflow)?;
        }
        if vertices > self.limits.contour_vertices
            || drill_routes > self.limits.drill_route_features
        {
            return Err(FabricationError::LimitExceeded {
                resource: "canonical-model",
            });
        }
        for text in self.all_texts() {
            deadline.check("fabrication-limits-validation")?;
            if text.len() > self.limits.max_text_bytes || text.chars().any(char::is_control) {
                return Err(FabricationError::LimitExceeded {
                    resource: "canonical-model",
                });
            }
        }
        deadline.check("fabrication-limits-validation")?;
        Ok(())
    }

    fn validate_identities_and_references_with_deadline(
        &self,
        deadline: ManufacturingDeadline,
        rederive_physical_bounds: bool,
    ) -> Result<(), FabricationError> {
        let check_deadline = || deadline.check("fabrication-reference-validation");
        let authoritative_budget = ReconciliationBudget { deadline };
        check_deadline()?;
        let mut ids = HashSet::new();
        let mut document_ids = HashSet::new();
        let mut documents_by_id = HashMap::new();
        for document in &self.documents {
            check_deadline()?;
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
            documents_by_id.insert(document.id.as_str(), document);
        }
        let mut layer_ids = HashSet::new();
        let mut layer_documents = HashMap::new();
        let mut layers_by_id = HashMap::new();
        for layer in &self.layers {
            check_deadline()?;
            validate_provenance(&layer.provenance, &self.documents, deadline)?;
            if !document_ids.contains(layer.document_id.as_str())
                || layer.id
                    != layer_id(
                        &layer.document_id,
                        layer.name.as_deref(),
                        layer.role,
                        layer.side,
                        layer.order,
                        layer.authority,
                        &layer.provenance.location,
                    )
            {
                return Err(FabricationError::InvalidIdentity(layer.id.clone()));
            }
            insert_id(&mut ids, &layer.id)?;
            layer_ids.insert(layer.id.as_str());
            layer_documents.insert(layer.id.as_str(), layer.document_id.as_str());
            layers_by_id.insert(layer.id.as_str(), layer);
        }
        let mut tool_ids = HashSet::new();
        let mut tool_documents = HashMap::new();
        let mut tools_by_id = HashMap::new();
        for tool in &self.tools {
            check_deadline()?;
            validate_provenance(&tool.provenance, &self.documents, deadline)?;
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
            tool_documents.insert(tool.id.as_str(), tool.document_id.as_str());
            tools_by_id.insert(tool.id.as_str(), tool);
        }
        let mut macro_ids = HashSet::new();
        for definition in &self.macros {
            check_deadline()?;
            validate_provenance(&definition.provenance, &self.documents, deadline)?;
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
        let mut apertures_by_id = HashMap::new();
        for aperture in &self.apertures {
            check_deadline()?;
            validate_provenance(&aperture.provenance, &self.documents, deadline)?;
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
            apertures_by_id.insert(aperture.id.as_str(), aperture);
        }
        let mut blocks_by_id = HashMap::new();
        let mut block_ranges = BTreeMap::<&str, Vec<&ApertureBlock>>::new();
        let mut matched_block_apertures = BTreeSet::new();
        for block in &self.blocks {
            check_deadline()?;
            validate_provenance(&block.provenance, &self.documents, deadline)?;
            let aperture = apertures_by_id.get(block.aperture_id.as_str()).copied();
            let start = &block.provenance.location;
            if !document_ids.contains(block.document_id.as_str())
                || block.id != record_id("block", &block.document_id, start)
                || block.aperture_id != aperture_id(&block.document_id, ApertureShape::Block, start)
                || aperture.is_none_or(|aperture| {
                    aperture.document_id != block.document_id
                        || aperture.shape != ApertureShape::Block
                        || !aperture.dimensions.is_empty()
                        || aperture.provenance.location != *start
                })
                || start.record >= block.definition_end.record
                || start.byte_end >= block.definition_end.byte_start
                || blocks_by_id.insert(block.id.as_str(), block).is_some()
                || !matched_block_apertures.insert(block.aperture_id.as_str())
            {
                return Err(FabricationError::InvalidIdentity("block-membership".into()));
            }
            let ranges = block_ranges.entry(block.document_id.as_str()).or_default();
            if ranges.last().is_some_and(|previous| {
                previous.definition_end.byte_end >= block.provenance.location.byte_start
            }) {
                return Err(FabricationError::InvalidIdentity("block-membership".into()));
            }
            ranges.push(block);
        }
        let mut expected_block_apertures = BTreeSet::new();
        for aperture in &self.apertures {
            check_deadline()?;
            if aperture.shape == ApertureShape::Block {
                expected_block_apertures.insert(aperture.id.as_str());
            }
        }
        if matched_block_apertures != expected_block_apertures {
            return Err(FabricationError::InvalidIdentity("block-membership".into()));
        }

        let mut feature_ids = HashSet::new();
        let mut features_by_id = HashMap::new();
        let mut members_by_block = HashMap::<&str, Vec<&str>>::new();
        let mut instantiations_by_aperture = HashMap::<&str, Vec<&str>>::new();
        let mut quantized_documents = BTreeSet::new();
        for feature in &self.features {
            check_deadline()?;
            validate_provenance(&feature.provenance, &self.documents, deadline)?;
            let containing_block =
                block_ranges
                    .get(feature.document_id.as_str())
                    .and_then(|ranges| {
                        let index = ranges.partition_point(|block| {
                            block.provenance.location.byte_start
                                < feature.provenance.location.byte_start
                        });
                        index
                            .checked_sub(1)
                            .and_then(|index| ranges.get(index).copied())
                            .filter(|block| {
                                block.provenance.location.byte_end
                                    < feature.provenance.location.byte_start
                                    && feature.provenance.location.byte_end
                                        < block.definition_end.byte_start
                            })
                    });
            let membership_valid = match &feature.membership {
                FeatureMembership::TopLevel => containing_block.is_none(),
                FeatureMembership::ApertureBlock {
                    block_id,
                    aperture_id,
                } => containing_block.is_some_and(|block| {
                    block.id == *block_id
                        && block.aperture_id == *aperture_id
                        && block.document_id == feature.document_id
                }),
            };
            let referenced_aperture = match &feature.geometry {
                Geometry::Flash(flash) => Some(flash.aperture_id.as_str()),
                _ => None,
            };
            let referenced_geometry_tool = match &feature.geometry {
                Geometry::Drill(drill) => Some(drill.tool_id.as_str()),
                Geometry::Route(route) => Some(route.tool_id.as_str()),
                Geometry::Slot(slot) => Some(slot.tool_id.as_str()),
                _ => None,
            };
            if !document_ids.contains(feature.document_id.as_str())
                || layer_documents.get(feature.layer_id.as_str()).copied()
                    != Some(feature.document_id.as_str())
                || feature.tool_id.as_deref().is_some_and(|id| {
                    tool_documents.get(id).copied() != Some(feature.document_id.as_str())
                })
                || referenced_geometry_tool.is_some_and(|id| {
                    tool_documents.get(id).copied() != Some(feature.document_id.as_str())
                })
                || referenced_aperture.is_some_and(|id| {
                    apertures_by_id
                        .get(id)
                        .is_none_or(|aperture| aperture.document_id != feature.document_id)
                })
            {
                return Err(FabricationError::DanglingReference(feature.id.clone()));
            }
            if !membership_valid
                || feature.id
                    != feature_id_with_membership(
                        &feature.document_id,
                        &feature.layer_id,
                        feature.geometry.kind(),
                        &feature.provenance.location,
                        &feature.membership,
                    )
            {
                return Err(FabricationError::InvalidIdentity("block-membership".into()));
            }
            validate_geometry(&feature.geometry, &aperture_ids, &tool_ids, deadline)?;
            if !feature.transforms.operations.is_empty()
                && validate_transformed_geometry(&feature.geometry, &feature.transforms, deadline)?
            {
                quantized_documents.insert(feature.document_id.as_str());
            }
            insert_id(&mut ids, &feature.id)?;
            feature_ids.insert(feature.id.as_str());
            features_by_id.insert(feature.id.as_str(), feature);
            if let FeatureMembership::ApertureBlock { block_id, .. } = &feature.membership {
                members_by_block
                    .entry(block_id.as_str())
                    .or_default()
                    .push(feature.id.as_str());
            }
            if let Geometry::Flash(flash) = &feature.geometry {
                instantiations_by_aperture
                    .entry(flash.aperture_id.as_str())
                    .or_default()
                    .push(feature.id.as_str());
            }
        }
        for block in &self.blocks {
            check_deadline()?;
            let expected_members = members_by_block
                .get(block.id.as_str())
                .cloned()
                .unwrap_or_default();
            let expected_instantiations = instantiations_by_aperture
                .get(block.aperture_id.as_str())
                .cloned()
                .unwrap_or_default();
            let members_match = block.feature_ids.len() == expected_members.len()
                && checked_all_with_deadline(
                    block.feature_ids.iter().zip(&expected_members),
                    deadline,
                    "block-membership",
                    |(supplied, expected)| supplied == expected,
                )?;
            let instantiations_match = block.instantiation_feature_ids.len()
                == expected_instantiations.len()
                && checked_all_with_deadline(
                    block
                        .instantiation_feature_ids
                        .iter()
                        .zip(&expected_instantiations),
                    deadline,
                    "block-membership",
                    |(supplied, expected)| supplied == expected,
                )?;
            let supplied_members = checked_btree_set_with_deadline(
                block.feature_ids.iter().map(String::as_str),
                deadline,
                "block-membership",
            )?;
            let supplied_instantiations = checked_btree_set_with_deadline(
                block.instantiation_feature_ids.iter().map(String::as_str),
                deadline,
                "block-membership",
            )?;
            if !members_match
                || !instantiations_match
                || supplied_members.len() != block.feature_ids.len()
                || supplied_instantiations.len() != block.instantiation_feature_ids.len()
            {
                return Err(FabricationError::InvalidIdentity("block-membership".into()));
            }
            insert_id(&mut ids, &block.id)?;
        }
        for repeat in &self.repetitions {
            check_deadline()?;
            validate_provenance(&repeat.provenance, &self.documents, deadline)?;
            if repeat.id != record_id("repeat", &repeat.document_id, &repeat.provenance.location) {
                return Err(FabricationError::DanglingReference(repeat.id.clone()));
            }
            for (index, feature_id) in repeat.feature_ids.iter().enumerate() {
                if index % 1024 == 0 {
                    check_deadline()?;
                }
                if !feature_ids.contains(feature_id.as_str())
                    || features_by_id
                        .get(feature_id.as_str())
                        .is_none_or(|feature| feature.document_id != repeat.document_id)
                {
                    return Err(FabricationError::DanglingReference(repeat.id.clone()));
                }
            }
            validate_length(repeat.x_step)?;
            validate_length(repeat.y_step)?;
            let offset = CanonicalPoint {
                x: repeat_max_offset(repeat.x_step, repeat.x_count)?,
                y: repeat_max_offset(repeat.y_step, repeat.y_count)?,
            };
            for feature_id in &repeat.feature_ids {
                check_deadline()?;
                let feature = features_by_id
                    .get(feature_id.as_str())
                    .copied()
                    .ok_or_else(|| FabricationError::DanglingReference(feature_id.clone()))?;
                validate_transformed_geometry_at_offset(
                    &feature.geometry,
                    &feature.transforms,
                    offset,
                    deadline,
                )?;
            }
            insert_id(&mut ids, &repeat.id)?;
        }
        if let Some(product) = &self.product {
            if product.provenance.is_empty() {
                return Err(FabricationError::InvalidProvenance("product".into()));
            }
            for provenance in &product.provenance {
                check_deadline()?;
                validate_provenance(provenance, &self.documents, deadline)?;
            }
        }
        if let Some(profile) = &self.profile {
            if profile.provenance.is_empty() {
                return Err(FabricationError::InvalidProvenance("profile".into()));
            }
            for feature_id in profile
                .contour_feature_ids
                .iter()
                .chain(&profile.cutout_feature_ids)
            {
                check_deadline()?;
                if !feature_ids.contains(feature_id.as_str()) {
                    return Err(FabricationError::DanglingReference("profile".into()));
                }
            }
            if let Some(extents) = &profile.extents {
                validate_point(extents.min)?;
                validate_point(extents.max)?;
                if extents.min.x > extents.max.x || extents.min.y > extents.max.y {
                    return Err(FabricationError::InvalidIdentity("profile-extents".into()));
                }
            }
            for provenance in &profile.provenance {
                check_deadline()?;
                validate_provenance(provenance, &self.documents, deadline)?;
            }
        }
        for semantic in &self.connectivity {
            check_deadline()?;
            if !feature_ids.contains(semantic.feature_id.as_str()) {
                return Err(FabricationError::DanglingReference(
                    semantic.feature_id.clone(),
                ));
            }
            validate_provenance(&semantic.provenance, &self.documents, deadline)?;
        }
        let mut association_ids = HashSet::new();
        let mut associated_pad_ids = HashSet::new();
        let mut associated_hole_ids = HashSet::new();
        for association in &self.pad_hole_associations {
            check_deadline()?;
            validate_provenance(&association.pad_provenance, &self.documents, deadline)?;
            validate_provenance(&association.hole_provenance, &self.documents, deadline)?;
            let hole = features_by_id
                .get(association.hole_id.as_str())
                .copied()
                .ok_or_else(|| FabricationError::DanglingReference(association.hole_id.clone()))?;
            let tool = tools_by_id
                .get(association.tool_id.as_str())
                .copied()
                .ok_or_else(|| FabricationError::DanglingReference(association.tool_id.clone()))?;
            let Geometry::Drill(hole_geometry) = &association.hole_geometry else {
                return Err(FabricationError::InvalidIdentity(association.id.clone()));
            };
            let Geometry::Drill(source_hole) = &hole.geometry else {
                return Err(FabricationError::InvalidIdentity(
                    association.hole_id.clone(),
                ));
            };
            let materialized = hole.transforms.materialize(source_hole.position)?;
            let expected_hole = Geometry::Drill(DrillFeature {
                position: materialized.point,
                diameter: source_hole.diameter,
                tool_id: source_hole.tool_id.clone(),
            });
            let mut layer_set = BTreeSet::new();
            let mut applicable_layers = Vec::with_capacity(association.applicable_layer_ids.len());
            for layer_id in &association.applicable_layer_ids {
                check_deadline()?;
                let layer = layers_by_id
                    .get(layer_id.as_str())
                    .copied()
                    .ok_or_else(|| FabricationError::DanglingReference(layer_id.clone()))?;
                if !layer_set.insert(layer_id.as_str())
                    || layer.role != LayerRole::Copper
                    || layer.document_id != association.pad_provenance.document_id
                    || layer.order.is_none()
                {
                    return Err(FabricationError::InvalidIdentity(association.id.clone()));
                }
                applicable_layers.push(layer);
            }
            let ordered = applicable_layers
                .windows(2)
                .all(|pair| pair[0].order < pair[1].order);
            let span_matches = applicable_layers.first().is_some_and(|layer| {
                association.span.from_layer_id.as_deref() == Some(layer.id.as_str())
            }) && applicable_layers.last().is_some_and(|layer| {
                association.span.to_layer_id.as_deref() == Some(layer.id.as_str())
            });
            let document = documents_by_id
                .get(association.pad_provenance.document_id.as_str())
                .copied();
            let (pad_center, _, pad_resolution) = exact_circle_geometry(&association.pad_geometry)?;
            if !association_ids.insert(association.id.as_str())
                || !associated_pad_ids.insert(association.pad_id.as_str())
                || !associated_hole_ids.insert(association.hole_id.as_str())
                || association.id != pad_hole_association_id(association)?
                || association.pad_id
                    != pad_id(
                        &association.pad_provenance.document_id,
                        &association.pad_provenance.location,
                    )?
                || association.pad_provenance != association.hole_provenance
                || association.pad_provenance.producer != KICAD_MANUFACTURING_ADAPTER
                || association.pad_provenance.producer_version
                    != KICAD_MANUFACTURING_ADAPTER_VERSION
                || document.is_none_or(|document| {
                    document.format != DocumentFormat::KicadPcb
                        || document.adapter != KICAD_MANUFACTURING_ADAPTER
                        || document.adapter_version != KICAD_MANUFACTURING_ADAPTER_VERSION
                        || document.parse_status != ParseStatus::Complete
                        || document
                            .numeric_format
                            .as_ref()
                            .is_none_or(|format| format.resolution != pad_resolution)
                })
                || hole.document_id != association.pad_provenance.document_id
                || hole.provenance != association.hole_provenance
                || hole.tool_id.as_deref() != Some(association.tool_id.as_str())
                || !materialized.quantization.is_empty()
                || association.hole_geometry != expected_hole
                || hole_geometry.position != pad_center
                || hole_geometry.tool_id != association.tool_id
                || tool.document_id != association.pad_provenance.document_id
                || tool.kind != ToolKind::Drill
                || tool.diameter != Some(hole_geometry.diameter)
                || association.plating != Plating::Plated
                || tool.plating != association.plating
                || tool.span.as_ref() != Some(&association.span)
                || !ordered
                || !span_matches
            {
                return Err(FabricationError::InvalidIdentity(association.id.clone()));
            }
            validate_geometry(
                &association.pad_geometry,
                &aperture_ids,
                &tool_ids,
                deadline,
            )?;
            validate_geometry(
                &association.hole_geometry,
                &aperture_ids,
                &tool_ids,
                deadline,
            )?;
            insert_id(&mut ids, &association.pad_id)?;
            insert_id(&mut ids, &association.id)?;
        }
        for attribute in &self.x2_attributes {
            check_deadline()?;
            validate_provenance(&attribute.provenance, &self.documents, deadline)?;
            let mut targets = BTreeSet::new();
            let mut targets_valid = true;
            for (index, target) in attribute.target_ids.iter().enumerate() {
                if index % 1024 == 0 {
                    check_deadline()?;
                }
                targets.insert(target);
                targets_valid &= match attribute.scope {
                    X2AttributeScope::File => target == &attribute.document_id,
                    X2AttributeScope::Aperture => aperture_ids.contains(target.as_str()),
                    X2AttributeScope::Object => feature_ids.contains(target.as_str()),
                };
            }
            let valid_kind = matches!(
                (attribute.scope, attribute.kind),
                (X2AttributeScope::File, X2AttributeKind::FileFunction)
                    | (
                        X2AttributeScope::Aperture,
                        X2AttributeKind::ApertureFunction | X2AttributeKind::Reset
                    )
                    | (
                        X2AttributeScope::Object,
                        X2AttributeKind::Net
                            | X2AttributeKind::Component
                            | X2AttributeKind::Pin
                            | X2AttributeKind::Reset
                    )
            );
            let mut values_valid = true;
            for (index, value) in attribute.values.iter().enumerate() {
                if index % 1024 == 0 {
                    check_deadline()?;
                }
                values_valid &= !value.is_empty();
            }
            if attribute.document_id != attribute.provenance.document_id
                || attribute.id != scoped_x2_attribute_id_with_deadline(attribute, deadline)?
                || !valid_kind
                || targets.len() != attribute.target_ids.len()
                || !targets_valid
                || attribute.deletion != attribute.values.is_empty()
                || !values_valid
                || (attribute.deletion && !attribute.target_ids.is_empty())
            {
                return Err(FabricationError::InvalidIdentity(attribute.id.clone()));
            }
            insert_id(&mut ids, &attribute.id)?;
        }
        let mut bound_documents = BTreeSet::new();
        for bounds in &self.physical_bounds {
            check_deadline()?;
            let document = documents_by_id
                .get(bounds.document_id.as_str())
                .copied()
                .ok_or_else(|| FabricationError::DanglingReference(bounds.id.clone()))?;
            let mut locations = BTreeSet::new();
            for (index, location) in bounds.source_locations.iter().enumerate() {
                if index % 1024 == 0 {
                    check_deadline()?;
                }
                locations.insert(location);
            }
            if !bound_documents.insert(bounds.document_id.as_str())
                || !matches!(
                    bounds.format,
                    DocumentFormat::Gerber | DocumentFormat::Excellon
                )
                || bounds.format != document.format
                || bounds.artifact_digest != document.artifact_digest
                || !lowercase_sha256(&bounds.geometry_digest)
                || bounds.id != physical_bounds_id_with_deadline(bounds, deadline)?
                || document
                    .numeric_format
                    .as_ref()
                    .is_none_or(|format| format.resolution != bounds.resolution)
                || locations.len() != bounds.source_locations.len()
                || bounds.source_locations.is_empty()
            {
                return Err(FabricationError::InvalidIdentity(bounds.id.clone()));
            }
            validate_provenance(&bounds.provenance, &self.documents, deadline)?;
            insert_id(&mut ids, &bounds.id)?;
            validate_point(bounds.extent.min)?;
            validate_point(bounds.extent.max)?;
            if bounds.extent.min.x > bounds.extent.max.x
                || bounds.extent.min.y > bounds.extent.max.y
            {
                return Err(FabricationError::InvalidIdentity(bounds.id.clone()));
            }
        }
        let expected_bounds = if rederive_physical_bounds {
            Some(derive_release_physical_bounds(self, authoritative_budget)?)
        } else {
            None
        };
        let requires_bounds = checked_any_with_deadline(
            &self.documents,
            deadline,
            "authoritative-physical-bounds",
            |document| {
                matches!(
                    document.format,
                    DocumentFormat::Gerber | DocumentFormat::Excellon
                ) && document.parse_status == ParseStatus::Complete
                    && matches!(
                        document.adapter.as_str(),
                        "gerber-parser-ratemypcb" | "ratemypcb-xnc"
                    )
            },
        )?;
        if let Some(expected_bounds) = expected_bounds {
            if requires_bounds || !self.physical_bounds.is_empty() {
                let supplied_digest = hash_serialized_with_deadline(
                    deadline,
                    "authoritative-physical-bounds",
                    &self.physical_bounds,
                )?;
                let expected_digest = hash_serialized_with_deadline(
                    deadline,
                    "authoritative-physical-bounds",
                    &expected_bounds,
                )?;
                if supplied_digest != expected_digest {
                    return Err(FabricationError::InvalidIdentity(
                        "authoritative-physical-bounds".into(),
                    ));
                }
            }
        }

        let mut retained_conflict_ids = BTreeSet::new();
        for conflict in &self.conflicts {
            check_deadline()?;
            retained_conflict_ids.insert(conflict.id.as_str());
        }
        let mut job_fact_ids = BTreeSet::new();
        let mut job_targets = BTreeSet::new();
        for fact in &self.job_file_functions {
            check_deadline()?;
            let job = documents_by_id
                .get(fact.job_document_id.as_str())
                .copied()
                .filter(|document| document.format == DocumentFormat::GerberJob)
                .ok_or_else(|| FabricationError::DanglingReference(fact.id.clone()))?;
            let referenced = documents_by_id
                .get(fact.referenced_document_id.as_str())
                .copied()
                .filter(|document| document.format != DocumentFormat::GerberJob)
                .ok_or_else(|| FabricationError::DanglingReference(fact.id.clone()))?;
            let parsed = package_file_function(&X2Attribute {
                name: "TF.FileFunction".into(),
                values: fact.fields.clone(),
                provenance: fact.provenance.clone(),
            })?;
            let mut conflicts = BTreeSet::new();
            let mut conflicts_valid = true;
            for conflict_id in &fact.conflict_ids {
                check_deadline()?;
                conflicts.insert(conflict_id);
                conflicts_valid &= retained_conflict_ids.contains(conflict_id.as_str());
            }
            if !job_fact_ids.insert(fact.id.as_str())
                || !job_targets.insert(fact.referenced_document_id.as_str())
                || fact.id != job_file_function_fact_id_with_deadline(fact, deadline)?
                || fact.job_artifact_digest != job.artifact_digest
                || fact.referenced_virtual_path != referenced.virtual_path
                || fact.referenced_artifact_digest != referenced.artifact_digest
                || fact.provenance.document_id != job.id
                || fact.provenance.artifact_digest != job.artifact_digest
                || parsed.role != fact.role
                || parsed.side != fact.side
                || parsed.order != fact.order
                || parsed.plating != fact.plating
                || parsed.from_layer != fact.from_layer
                || parsed.to_layer != fact.to_layer
                || parsed.qualifier != fact.qualifier
                || parsed.operation != fact.operation
                || conflicts.len() != fact.conflict_ids.len()
                || !conflicts_valid
            {
                return Err(FabricationError::InvalidIdentity(fact.id.clone()));
            }
            validate_provenance(&fact.provenance, &self.documents, deadline)?;
            insert_id(&mut ids, &fact.id)?;
        }

        if let Some(outcome) = &self.integration_outcome {
            check_deadline()?;
            let shape_valid = match outcome.state {
                IntegratedReconciliationState::NotProvided => {
                    outcome.attempted_native_path.is_none()
                        && outcome.attempted_native_digest.is_none()
                }
                IntegratedReconciliationState::Failed => {
                    outcome
                        .attempted_native_path
                        .as_deref()
                        .is_some_and(valid_virtual_path)
                        && outcome
                            .attempted_native_digest
                            .as_deref()
                            .is_some_and(lowercase_sha256)
                }
            };
            insert_id(&mut ids, &outcome.id)?;
            if outcome.id != integration_outcome_id(outcome)
                || outcome.reason.is_empty()
                || !shape_valid
                || self.source_pair.is_some()
                || self.native_reconciliation_source.is_some()
                || !self.reconciliations.is_empty()
            {
                return Err(FabricationError::InvalidIdentity(outcome.id.clone()));
            }
        }

        let mut native_placement_references = BTreeSet::new();
        let mut native_placement_occurrences = BTreeSet::new();
        let mut native_assembly_complete = !self.assembly.placements.is_empty();
        for placement in &self.assembly.placements {
            check_deadline()?;
            validate_point(placement.position)?;
            validate_provenance(&placement.provenance, &self.documents, deadline)?;
            if placement.id
                != assembly_placement_id(
                    &placement.provenance.document_id,
                    placement.occurrence_id.as_deref(),
                    &placement.reference,
                    &placement.provenance.location,
                )?
                || placement.reference.is_empty()
                || placement.reference.trim() != placement.reference
                || placement.reference.len() > self.limits.max_text_bytes
                || placement.occurrence_id.as_deref().is_some_and(|id| {
                    id.is_empty()
                        || id.trim() != id
                        || id.len() > self.limits.max_text_bytes
                        || id.chars().any(char::is_control)
                })
                || !(0..360_000_000).contains(&placement.rotation_microdegrees)
            {
                return Err(FabricationError::InvalidIdentity(placement.id.clone()));
            }
            insert_id(&mut ids, &placement.id)?;
            native_assembly_complete &=
                matches!(placement.side, LayerSide::Top | LayerSide::Bottom)
                    && placement.fitted != AssemblyFittedState::Unknown
                    && placement.revision.as_deref().is_some_and(|revision| {
                        !revision.is_empty()
                            && revision.trim() == revision
                            && revision.len() <= self.limits.max_text_bytes
                            && !revision.chars().any(char::is_control)
                    })
                    && placement.convention.complete()
                    && native_placement_references.insert(placement.reference.as_str())
                    && placement
                        .occurrence_id
                        .as_deref()
                        .is_some_and(|id| native_placement_occurrences.insert(id));
        }
        let mut declared_placement_keys = BTreeSet::new();
        for placement in &self.assembly.declared_placements {
            check_deadline()?;
            validate_point(placement.position)?;
            if placement.id
                != declared_assembly_placement_id(
                    &placement.source_path,
                    &placement.artifact_digest,
                    placement.line,
                    &placement.reference,
                )?
                || placement.reference.is_empty()
                || placement.reference.trim() != placement.reference
                || placement.reference.len() > self.limits.max_text_bytes
                || !matches!(placement.side, LayerSide::Top | LayerSide::Bottom)
                || !(0..360_000_000).contains(&placement.rotation_microdegrees)
                || placement.fitted == AssemblyFittedState::Unknown
                || placement.revision.is_empty()
                || placement.revision.trim() != placement.revision
                || placement.revision.len() > self.limits.max_text_bytes
                || !placement.convention.complete()
                || !valid_virtual_path(&placement.source_path)
                || placement.source_path.len() > self.limits.normalized_path_bytes
                || !lowercase_sha256(&placement.artifact_digest)
                || placement.line == 0
                || placement.line > self.limits.geometry_features as u64
                || !declared_placement_keys.insert((
                    placement.artifact_digest.as_str(),
                    placement.reference.as_str(),
                ))
            {
                return Err(FabricationError::InvalidIdentity(placement.id.clone()));
            }
            insert_id(&mut ids, &placement.id)?;
        }
        if let Some(courtyard) = &self.assembly.native_courtyard {
            let complete_metadata = courtyard.tool == "kicad-cli"
                && courtyard
                    .version
                    .as_deref()
                    .and_then(crate::schematic::KiCadMajor::parse)
                    .is_some()
                && courtyard.source.as_deref().is_some_and(|source| {
                    !source.is_empty()
                        && source.len() <= self.limits.normalized_path_bytes
                        && !source.chars().any(char::is_control)
                });
            if courtyard.tool.is_empty()
                || courtyard.tool.len() > self.limits.max_text_bytes
                || courtyard
                    .version
                    .as_deref()
                    .is_some_and(|version| version.len() > self.limits.max_text_bytes)
                || (courtyard.state == NativeCourtyardRunState::Complete && !complete_metadata)
                || (matches!(
                    courtyard.state,
                    NativeCourtyardRunState::NotRun
                        | NativeCourtyardRunState::Disabled
                        | NativeCourtyardRunState::Failed
                ) && !courtyard.observations.is_empty())
            {
                return Err(FabricationError::InvalidIdentity(
                    "native-courtyard-evidence".into(),
                ));
            }
            let mut observations = BTreeSet::new();
            for observation in &courtyard.observations {
                check_deadline()?;
                let Some(version) = courtyard.version.as_deref() else {
                    return Err(FabricationError::InvalidIdentity(observation.id.clone()));
                };
                if observation.location.is_empty()
                    || observation.location.len() > self.limits.max_text_bytes
                    || observation.location.chars().any(char::is_control)
                    || observation.id
                        != native_courtyard_observation_id(
                            observation.kind,
                            observation.exclusion,
                            &courtyard.tool,
                            version,
                            &observation.location,
                        )?
                    || !observations.insert((
                        observation.kind,
                        observation.exclusion,
                        observation.location.as_str(),
                    ))
                {
                    return Err(FabricationError::InvalidIdentity(observation.id.clone()));
                }
                insert_id(&mut ids, &observation.id)?;
            }
        }
        for layer_id in self
            .assembly
            .mask_layer_ids
            .iter()
            .chain(&self.assembly.paste_layer_ids)
        {
            check_deadline()?;
            if !layer_ids.contains(layer_id.as_str()) {
                return Err(FabricationError::DanglingReference("assembly-layer".into()));
            }
        }
        for layer in &self.construction.layers {
            check_deadline()?;
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
            validate_provenance(&layer.provenance, &self.documents, deadline)?;
        }
        validate_positive_length_option(self.construction.total_thickness)?;
        for constraint in &self.constraints {
            check_deadline()?;
            validate_provenance(&constraint.provenance, &self.documents, deadline)?;
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
        if !quantized_documents.is_empty() {
            for capability in &self.capabilities.records {
                check_deadline()?;
                if capability.id == CapabilityId::GeometryExpanded
                    && capability.state == CapabilityState::Complete
                {
                    let mut applies = capability.document_ids.is_empty();
                    for document_id in &capability.document_ids {
                        check_deadline()?;
                        applies |= quantized_documents.contains(document_id.as_str());
                    }
                    if applies {
                        return Err(FabricationError::InvalidIdentity(
                            "quantized-expanded-geometry".into(),
                        ));
                    }
                }
            }
        }
        let mut capabilities = HashSet::new();
        let mut capability_states = BTreeMap::new();
        for capability in &self.capabilities.records {
            check_deadline()?;
            if !capabilities.insert(capability.id) {
                return Err(FabricationError::DuplicateId(format!(
                    "capability:{:?}",
                    capability.id
                )));
            }
            capability_states.insert(capability.id, capability.state);
            for document_id in &capability.document_ids {
                check_deadline()?;
                if !document_ids.contains(document_id.as_str()) {
                    return Err(FabricationError::DanglingReference(format!(
                        "capability:{:?}",
                        capability.id
                    )));
                }
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
                check_deadline()?;
                validate_provenance(provenance, &self.documents, deadline)?;
            }
        }
        for omission in &self.omissions {
            check_deadline()?;
            insert_id(&mut ids, &omission.id)?;
            validate_provenance(&omission.provenance, &self.documents, deadline)?;
            if omission.affected_capabilities.is_empty() {
                return Err(FabricationError::InvalidOmission(omission.id.clone()));
            }
            for capability_id in &omission.affected_capabilities {
                check_deadline()?;
                if capability_states
                    .get(capability_id)
                    .is_none_or(|state| *state == CapabilityState::Complete)
                {
                    return Err(FabricationError::InvalidOmission(omission.id.clone()));
                }
            }
        }
        let mut omitted_capabilities = BTreeSet::new();
        for omission in &self.omissions {
            check_deadline()?;
            for capability_id in &omission.affected_capabilities {
                check_deadline()?;
                omitted_capabilities.insert(*capability_id);
            }
        }
        for capability in &self.capabilities.records {
            check_deadline()?;
            if capability.state == CapabilityState::Omitted
                && !omitted_capabilities.contains(&capability.id)
            {
                return Err(FabricationError::InvalidOmission(format!(
                    "capability:{:?}",
                    capability.id
                )));
            }
        }
        for conflict in &self.conflicts {
            check_deadline()?;
            insert_id(&mut ids, &conflict.id)?;
            validate_provenance(&conflict.left.provenance, &self.documents, deadline)?;
            validate_provenance(&conflict.right.provenance, &self.documents, deadline)?;
            let mut affected_valid = !conflict.affected_capabilities.is_empty();
            for capability_id in &conflict.affected_capabilities {
                check_deadline()?;
                affected_valid &= capabilities.contains(capability_id)
                    && capability_states
                        .get(capability_id)
                        .is_some_and(|state| *state != CapabilityState::Complete);
            }
            if !affected_valid
                || conflict.left.canonical_value == conflict.right.canonical_value
                || conflict.left.provenance == conflict.right.provenance
            {
                return Err(FabricationError::InvalidConflict(conflict.id.clone()));
            }
        }
        let assembly_affected = self.omissions.iter().any(|omission| {
            omission
                .affected_capabilities
                .contains(&CapabilityId::Assembly)
        }) || self.conflicts.iter().any(|conflict| {
            conflict
                .affected_capabilities
                .contains(&CapabilityId::Assembly)
        });
        let assembly_provided = !self.assembly.placements.is_empty() || assembly_affected;
        let assembly_complete = native_assembly_complete && !assembly_affected;
        match capability_states.get(&CapabilityId::Assembly) {
            Some(CapabilityState::Complete) if !assembly_complete => {
                return Err(FabricationError::InvalidIdentity(
                    "assembly-capability".into(),
                ));
            }
            Some(state) if assembly_complete && *state != CapabilityState::Complete => {
                return Err(FabricationError::InvalidIdentity(
                    "assembly-capability".into(),
                ));
            }
            Some(CapabilityState::NotProvided) if assembly_provided => {
                return Err(FabricationError::InvalidIdentity(
                    "assembly-capability".into(),
                ));
            }
            None if assembly_provided => {
                return Err(FabricationError::DanglingReference(
                    "assembly-capability".into(),
                ));
            }
            _ => {}
        }
        if assembly_complete {
            let capability = self
                .capabilities
                .records
                .iter()
                .find(|record| record.id == CapabilityId::Assembly)
                .ok_or_else(|| FabricationError::DanglingReference("assembly-capability".into()))?;
            let placement_provenance = self
                .assembly
                .placements
                .iter()
                .map(|placement| canonical_provenance(&placement.provenance))
                .collect::<BTreeSet<_>>();
            let capability_provenance = capability
                .provenance
                .iter()
                .map(canonical_provenance)
                .collect::<BTreeSet<_>>();
            let placement_documents = self
                .assembly
                .placements
                .iter()
                .map(|placement| placement.provenance.document_id.as_str())
                .collect::<BTreeSet<_>>();
            let capability_documents = capability
                .document_ids
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            if placement_provenance.len() != self.assembly.placements.len()
                || capability_provenance.len() != capability.provenance.len()
                || placement_provenance != capability_provenance
                || placement_documents != capability_documents
            {
                return Err(FabricationError::InvalidIdentity(
                    "assembly-capability".into(),
                ));
            }
        }
        native::validate_authoritative_states(self, authoritative_budget)?;
        if let Some(pair) = &self.source_pair {
            let native = documents_by_id
                .get(pair.native_document_id.as_str())
                .copied()
                .filter(|document| document.format == DocumentFormat::KicadPcb)
                .ok_or_else(|| FabricationError::DanglingReference(pair.id.clone()))?;
            let mut release_documents = BTreeMap::new();
            for document in &self.documents {
                check_deadline()?;
                if document.format != DocumentFormat::KicadPcb {
                    release_documents.insert(document.id.as_str(), document);
                }
            }
            let release_ids = checked_btree_set_with_deadline(
                release_documents.keys().copied(),
                deadline,
                "manufacturing-source-pair",
            )?;
            let product = self.product.as_ref().map(|product| {
                (
                    product.name.as_deref(),
                    product.revision.as_deref(),
                    product.part_number.as_deref(),
                    product.authority,
                )
            });
            let expected_release_package = stable_id_with_deadline(
                deadline,
                "manufacturing-source-pair",
                "package",
                &(release_ids, product),
            )?;
            let expected_digests = checked_btree_set_with_deadline(
                release_documents
                    .values()
                    .map(|document| document.artifact_digest.as_str()),
                deadline,
                "manufacturing-source-pair",
            )?;
            let supplied_digests = checked_btree_set_with_deadline(
                pair.release_document_digests.iter().map(String::as_str),
                deadline,
                "manufacturing-source-pair",
            )?;
            if native.artifact_digest != pair.native_artifact_digest
                || pair.release_package_id != expected_release_package
                || supplied_digests != expected_digests
                || pair.id
                    != source_pair_id_with_deadline(
                        &pair.native_document_id,
                        &pair.native_artifact_digest,
                        &pair.release_package_id,
                        &pair.release_document_digests,
                        deadline,
                    )?
            {
                return Err(FabricationError::InvalidIdentity(pair.id.clone()));
            }
        } else if !self.reconciliations.is_empty() || self.native_reconciliation_source.is_some() {
            return Err(FabricationError::DanglingReference(
                "manufacturing-source-pair".into(),
            ));
        }
        if self.source_pair.is_some() {
            if self.reconciliations.is_empty() || self.native_reconciliation_source.is_none() {
                return Err(FabricationError::DanglingReference(
                    "native-reconciliation-source".into(),
                ));
            }
            native::validate_reconciliation_derivation_with_deadline(self, deadline)?;
        }
        let native_document_id = self
            .source_pair
            .as_ref()
            .map(|pair| pair.native_document_id.as_str());
        let mut native_model_ids = BTreeSet::new();
        if let Some(native_document_id) = native_document_id {
            native_model_ids.insert(native_document_id);
            for item in &self.layers {
                check_deadline()?;
                if item.document_id == native_document_id {
                    native_model_ids.insert(item.id.as_str());
                }
            }
            for item in &self.tools {
                check_deadline()?;
                if item.document_id == native_document_id {
                    native_model_ids.insert(item.id.as_str());
                }
            }
            for item in &self.features {
                check_deadline()?;
                if item.document_id == native_document_id {
                    native_model_ids.insert(item.id.as_str());
                }
            }
        }
        let model_id_is_native = |id: &str| native_model_ids.contains(id);
        let mut families = BTreeSet::new();
        for reconciliation in &self.reconciliations {
            check_deadline()?;
            deadline.check("reconciliation-validation")?;
            let mut supplied_native_ids = BTreeSet::new();
            let mut native_ids_valid = true;
            for model_id in &reconciliation.native.model_ids {
                check_deadline()?;
                supplied_native_ids.insert(model_id);
                native_ids_valid &= ids.contains(model_id) && model_id_is_native(model_id);
            }
            let mut supplied_package_ids = BTreeSet::new();
            let mut package_ids_valid = true;
            for model_id in &reconciliation.package.model_ids {
                check_deadline()?;
                supplied_package_ids.insert(model_id);
                package_ids_valid &= ids.contains(model_id) && !model_id_is_native(model_id);
            }
            let mut canonical_values_valid = true;
            for value in [
                &reconciliation.native.canonical_value,
                &reconciliation.package.canonical_value,
            ] {
                check_deadline()?;
                canonical_values_valid &= value.len() as u64 <= RECONCILIATION_VALUE_BYTES
                    && canonical_json_valid_with_deadline(
                        value,
                        deadline,
                        "reconciliation-canonical-json",
                    )?;
            }
            if !families.insert(reconciliation.family)
                || reconciliation.id
                    != reconciliation_id_with_deadline(
                        reconciliation.family,
                        &reconciliation.native,
                        &reconciliation.package,
                        deadline,
                    )?
                || reconciliation.native.model_ids.is_empty()
                || reconciliation.package.model_ids.is_empty()
                || supplied_native_ids.len() != reconciliation.native.model_ids.len()
                || supplied_package_ids.len() != reconciliation.package.model_ids.len()
                || !native_ids_valid
                || !package_ids_valid
                || reconciliation.native.authority != Authority::NativeSource
                || matches!(
                    reconciliation.package.authority,
                    Authority::NativeSource | Authority::FilenameInference | Authority::Unknown
                )
                || !canonical_values_valid
                || reconciliation.smallest_evidence_action.is_empty()
            {
                return Err(FabricationError::InvalidIdentity(reconciliation.id.clone()));
            }
            validate_provenance(&reconciliation.native.provenance, &self.documents, deadline)?;
            validate_provenance(
                &reconciliation.package.provenance,
                &self.documents,
                deadline,
            )?;
            if native_document_id != Some(reconciliation.native.provenance.document_id.as_str())
                || reconciliation.package.provenance.document_id
                    == reconciliation.native.provenance.document_id
            {
                return Err(FabricationError::InvalidProvenance(
                    reconciliation.id.clone(),
                ));
            }
            validate_positive_length_option(reconciliation.native.resolution)?;
            validate_positive_length_option(reconciliation.package.resolution)?;
            let values_equivalent = native::reconciliation_values_equivalent(
                reconciliation,
                ReconciliationBudget { deadline },
            )?;
            let valid_status = match reconciliation.status {
                ReconciliationStatus::Match => values_equivalent,
                ReconciliationStatus::Mismatch => {
                    !values_equivalent
                        && reconciliation.confidence != ReconciliationConfidence::Unavailable
                }
                ReconciliationStatus::NotChecked => {
                    reconciliation.confidence == ReconciliationConfidence::Unavailable
                }
            };
            if !valid_status {
                return Err(FabricationError::InvalidConflict(reconciliation.id.clone()));
            }
        }
        let mut reconciliation_capability = None;
        for capability in &self.capabilities.records {
            check_deadline()?;
            if capability.id == CapabilityId::PackageReconciliation {
                reconciliation_capability = Some(capability);
                break;
            }
        }
        if let Some(capability) = reconciliation_capability {
            let complete = self.reconciliations.len() == 6
                && checked_all_with_deadline(
                    &self.reconciliations,
                    deadline,
                    "package-reconciliation-capability",
                    |item| item.status == ReconciliationStatus::Match,
                )?;
            if (capability.state == CapabilityState::Complete) != complete {
                return Err(FabricationError::InvalidIdentity(
                    "package-reconciliation-capability".into(),
                ));
            }
        }
        for warning in &self.warnings {
            check_deadline()?;
            if let Some(provenance) = &warning.provenance {
                validate_provenance(provenance, &self.documents, deadline)?;
            }
        }
        Ok(())
    }
    fn estimate_allocation(
        &self,
        deadline: ManufacturingDeadline,
    ) -> Result<u64, FabricationError> {
        deadline.check("fabrication-allocation-estimate")?;
        let lengths = [
            serialized_len_with_deadline(deadline, &self.input_outcomes)?,
            serialized_len_with_deadline(deadline, &self.product)?,
            serialized_len_with_deadline(deadline, &self.documents)?,
            serialized_len_with_deadline(deadline, &self.layers)?,
            serialized_len_with_deadline(deadline, &self.tools)?,
            definition_storage_len_with_deadline(deadline, self)?,
            feature_storage_len_with_deadline(deadline, &self.features)?,
            serialized_len_with_deadline(deadline, &self.physical_bounds)?,
            serialized_len_with_deadline(deadline, &self.profile)?,
            serialized_len_with_deadline(deadline, &self.connectivity)?,
            serialized_len_with_deadline(deadline, &self.pad_hole_associations)?,
            serialized_len_with_deadline(deadline, &self.x2_attributes)?,
            serialized_len_with_deadline(deadline, &self.job_file_functions)?,
            serialized_len_with_deadline(deadline, &self.assembly)?,
            serialized_len_with_deadline(deadline, &self.construction)?,
            serialized_len_with_deadline(deadline, &self.constraints)?,
            serialized_len_with_deadline(deadline, &self.capabilities)?,
            serialized_len_with_deadline(deadline, &self.omissions)?,
            serialized_len_with_deadline(deadline, &self.conflicts)?,
            serialized_len_with_deadline(deadline, &self.source_pair)?,
            serialized_len_with_deadline(
                deadline,
                &self
                    .native_reconciliation_source
                    .as_ref()
                    .map(|source| (source.review.estimated_allocation_bytes, &source.extents)),
            )?,
            serialized_len_with_deadline(deadline, &self.integration_outcome)?,
            serialized_len_with_deadline(deadline, &self.reconciliations)?,
            serialized_len_with_deadline(deadline, &self.warnings)?,
        ];
        let bytes = lengths.into_iter().try_fold(0_u64, |sum, length| {
            sum.checked_add(length)
                .ok_or(FabricationError::ArithmeticOverflow)
        })?;
        let feature_definitions =
            u64::try_from(self.features.len()).map_err(|_| FabricationError::ArithmeticOverflow)?;
        let additional_instances = self
            .expanded_feature_instances(deadline)?
            .checked_sub(feature_definitions)
            .ok_or(FabricationError::ArithmeticOverflow)?;
        // ponytail: compact repeats charge one u64 index; materializing analyzers must add a
        // full-feature allocation bound before cloning expanded geometry.
        let expansion_bytes = additional_instances
            .checked_mul(u64::try_from(std::mem::size_of::<u64>()).unwrap_or(8))
            .ok_or(FabricationError::ArithmeticOverflow)?;
        let estimated = bytes
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(expansion_bytes))
            .and_then(|bytes| bytes.checked_add(1024))
            .ok_or(FabricationError::ArithmeticOverflow)?;
        deadline.check("fabrication-allocation-estimate")?;
        Ok(estimated)
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
            .chain(self.product.iter().flat_map(|product| {
                [
                    product.name.as_deref(),
                    product.revision.as_deref(),
                    product.part_number.as_deref(),
                ]
                .into_iter()
                .flatten()
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
                self.x2_attributes
                    .iter()
                    .flat_map(|attribute| attribute.values.iter().map(String::as_str)),
            )
            .chain(self.job_file_functions.iter().flat_map(|fact| {
                [
                    Some(fact.referenced_virtual_path.as_str()),
                    fact.qualifier.as_deref(),
                    fact.operation.as_deref(),
                    fact.omission.as_deref(),
                ]
                .into_iter()
                .flatten()
                .chain(fact.fields.iter().map(String::as_str))
            }))
            .chain(self.integration_outcome.iter().flat_map(|outcome| {
                [
                    outcome.attempted_native_path.as_deref(),
                    Some(outcome.reason.as_str()),
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
                self.reconciliations
                    .iter()
                    .map(|reconciliation| reconciliation.smallest_evidence_action.as_str()),
            )
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

struct DeadlineWriter {
    deadline: Option<ManufacturingDeadline>,
    resource: &'static str,
    bytes: Option<Vec<u8>>,
    hasher: Option<Sha256>,
    hash_buffer: Vec<u8>,
    length: u64,
    expired: bool,
    overflow: bool,
    bytes_since_check: usize,
    writes_since_check: usize,
}

impl DeadlineWriter {
    fn new(
        deadline: ManufacturingDeadline,
        resource: &'static str,
        retain: bool,
        hash: bool,
    ) -> Self {
        Self {
            deadline: Some(deadline),
            resource,
            bytes: retain.then(Vec::new),
            hasher: hash.then(Sha256::new),
            hash_buffer: if hash {
                Vec::with_capacity(4096)
            } else {
                Vec::new()
            },
            length: 0,
            expired: false,
            overflow: false,
            bytes_since_check: 0,
            writes_since_check: 0,
        }
    }

    fn unbounded(resource: &'static str, hash: bool) -> Self {
        Self {
            deadline: None,
            resource,
            bytes: None,
            hasher: hash.then(Sha256::new),
            hash_buffer: if hash {
                Vec::with_capacity(4096)
            } else {
                Vec::new()
            },
            length: 0,
            expired: false,
            overflow: false,
            bytes_since_check: 0,
            writes_since_check: 0,
        }
    }

    fn digest(self) -> Option<String> {
        self.hasher.map(|mut hasher| {
            hasher.update(&self.hash_buffer);
            format!("{:x}", hasher.finalize())
        })
    }
}

impl Write for DeadlineWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        for chunk in buffer.chunks(4096) {
            let check = self.bytes_since_check.saturating_add(chunk.len()) > 4096
                || self.writes_since_check >= 1024;
            if check {
                if self
                    .deadline
                    .is_some_and(|deadline| deadline.check(self.resource).is_err())
                {
                    self.expired = true;
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        self.resource,
                    ));
                }
                self.bytes_since_check = 0;
                self.writes_since_check = 0;
            }
            let Ok(length) = u64::try_from(chunk.len()) else {
                self.overflow = true;
                return Err(std::io::Error::other("serialized length overflow"));
            };
            let Some(total) = self.length.checked_add(length) else {
                self.overflow = true;
                return Err(std::io::Error::other("serialized length overflow"));
            };
            self.length = total;
            self.bytes_since_check = self.bytes_since_check.saturating_add(chunk.len());
            self.writes_since_check = self.writes_since_check.saturating_add(1);
            if let Some(bytes) = &mut self.bytes {
                bytes.extend_from_slice(chunk);
            }
            if let Some(hasher) = &mut self.hasher {
                self.hash_buffer.extend_from_slice(chunk);
                if self.hash_buffer.len() >= 4096 {
                    hasher.update(&self.hash_buffer);
                    self.hash_buffer.clear();
                }
            }
        }
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn serialize_with_deadline(
    deadline: ManufacturingDeadline,
    resource: &'static str,
    value: &impl Serialize,
    retain: bool,
) -> Result<(u64, Option<Vec<u8>>), FabricationError> {
    deadline.check(resource)?;
    let mut writer = DeadlineWriter::new(deadline, resource, retain, false);
    let result = serde_json::to_writer(&mut writer, value);
    if writer.expired {
        return Err(FabricationError::LimitExceeded { resource });
    }
    if writer.overflow {
        return Err(FabricationError::ArithmeticOverflow);
    }
    result.map_err(|error| FabricationError::Serialization(error.to_string()))?;
    deadline.check(resource)?;
    Ok((writer.length, writer.bytes))
}

fn hash_serialized_with_deadline(
    deadline: ManufacturingDeadline,
    resource: &'static str,
    value: &impl Serialize,
) -> Result<String, FabricationError> {
    deadline.check(resource)?;
    let mut writer = DeadlineWriter::new(deadline, resource, false, true);
    let result = serde_json::to_writer(&mut writer, value);
    if writer.expired {
        return Err(FabricationError::LimitExceeded { resource });
    }
    if writer.overflow {
        return Err(FabricationError::ArithmeticOverflow);
    }
    result.map_err(|error| FabricationError::Serialization(error.to_string()))?;
    deadline.check(resource)?;
    Ok(writer.digest().expect("serialization hashing enabled"))
}

fn feature_storage_len_with_deadline(
    deadline: ManufacturingDeadline,
    features: &[ManufacturingFeature],
) -> Result<u64, FabricationError> {
    fn add(total: &mut u64, value: usize) -> Result<(), FabricationError> {
        *total = total
            .checked_add(u64::try_from(value).map_err(|_| FabricationError::ArithmeticOverflow)?)
            .ok_or(FabricationError::ArithmeticOverflow)?;
        Ok(())
    }
    fn add_segments(total: &mut u64, segments: &[ContourSegment]) -> Result<(), FabricationError> {
        add(
            total,
            segments
                .len()
                .checked_mul(std::mem::size_of::<ContourSegment>())
                .ok_or(FabricationError::ArithmeticOverflow)?,
        )
    }
    let mut total = 0_u64;
    for feature in features {
        deadline.check("fabrication-allocation-estimate")?;
        add(&mut total, std::mem::size_of::<ManufacturingFeature>())?;
        for text in [
            feature.id.as_str(),
            feature.document_id.as_str(),
            feature.layer_id.as_str(),
            feature.tool_id.as_deref().unwrap_or_default(),
            feature.provenance.document_id.as_str(),
            feature.provenance.artifact_digest.as_str(),
            feature.provenance.producer.as_str(),
            feature.provenance.producer_version.as_str(),
            feature
                .provenance
                .source_lexeme
                .as_deref()
                .unwrap_or_default(),
        ] {
            add(&mut total, text.len())?;
        }
        add(
            &mut total,
            feature
                .transforms
                .operations
                .len()
                .checked_mul(std::mem::size_of::<TransformOperation>())
                .ok_or(FabricationError::ArithmeticOverflow)?,
        )?;
        if let FeatureMembership::ApertureBlock {
            block_id,
            aperture_id,
        } = &feature.membership
        {
            add(&mut total, block_id.len())?;
            add(&mut total, aperture_id.len())?;
        }
        match &feature.geometry {
            Geometry::Contour(contour) => add_segments(&mut total, &contour.segments)?,
            Geometry::Region(region) => {
                add(
                    &mut total,
                    region
                        .contours
                        .len()
                        .checked_mul(std::mem::size_of::<CanonicalContour>())
                        .ok_or(FabricationError::ArithmeticOverflow)?,
                )?;
                for contour in &region.contours {
                    deadline.check("fabrication-allocation-estimate")?;
                    add_segments(&mut total, &contour.segments)?;
                }
            }
            Geometry::Flash(flash) => add(&mut total, flash.aperture_id.len())?,
            Geometry::Drill(drill) => add(&mut total, drill.tool_id.len())?,
            Geometry::Route(route) => {
                add_segments(&mut total, &route.segments)?;
                add(&mut total, route.tool_id.len())?;
            }
            Geometry::Slot(slot) => add(&mut total, slot.tool_id.len())?,
            Geometry::Point(_) | Geometry::Line(_) | Geometry::Arc(_) => {}
        }
    }
    deadline.check("fabrication-allocation-estimate")?;
    Ok(total)
}

fn definition_storage_len_with_deadline(
    deadline: ManufacturingDeadline,
    review: &FabricationReview,
) -> Result<u64, FabricationError> {
    fn charge(total: &mut u64, count: usize, bytes: usize) -> Result<(), FabricationError> {
        let value = count
            .checked_mul(bytes)
            .and_then(|value| u64::try_from(value).ok())
            .ok_or(FabricationError::ArithmeticOverflow)?;
        *total = total
            .checked_add(value)
            .ok_or(FabricationError::ArithmeticOverflow)?;
        Ok(())
    }
    let mut total = 0_u64;
    for (index, aperture) in review.apertures.iter().enumerate() {
        if index % 1024 == 0 {
            deadline.check("fabrication-allocation-estimate")?;
        }
        charge(
            &mut total,
            1,
            std::mem::size_of::<ApertureDefinition>() + 1024,
        )?;
        charge(
            &mut total,
            aperture.dimensions.len(),
            std::mem::size_of::<Picometres>(),
        )?;
        charge(
            &mut total,
            aperture.macro_arguments.len(),
            std::mem::size_of::<CanonicalRational>() + 64,
        )?;
    }
    for (index, definition) in review.macros.iter().enumerate() {
        if index % 1024 == 0 {
            deadline.check("fabrication-allocation-estimate")?;
        }
        charge(&mut total, 1, std::mem::size_of::<MacroDefinition>() + 512)?;
        for value in definition.variables.iter().chain(&definition.operations) {
            charge(&mut total, 1, value.len() + std::mem::size_of::<String>())?;
        }
    }
    for (index, block) in review.blocks.iter().enumerate() {
        if index % 1024 == 0 {
            deadline.check("fabrication-allocation-estimate")?;
        }
        charge(
            &mut total,
            1,
            std::mem::size_of::<ApertureBlock>() + block.aperture_id.len() + 512,
        )?;
        charge(&mut total, block.feature_ids.len(), 128)?;
        charge(&mut total, block.instantiation_feature_ids.len(), 128)?;
    }
    for (index, repeat) in review.repetitions.iter().enumerate() {
        if index % 1024 == 0 {
            deadline.check("fabrication-allocation-estimate")?;
        }
        charge(&mut total, 1, std::mem::size_of::<StepRepeat>() + 512)?;
        charge(&mut total, repeat.feature_ids.len(), 128)?;
    }
    deadline.check("fabrication-allocation-estimate")?;
    Ok(total)
}

fn serialized_len_with_deadline(
    deadline: ManufacturingDeadline,
    value: &impl Serialize,
) -> Result<u64, FabricationError> {
    serialize_with_deadline(deadline, "fabrication-allocation-estimate", value, false)
        .map(|(length, _)| length)
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
    deadline: ManufacturingDeadline,
) -> Result<(), FabricationError> {
    let mut matched = None;
    for document in documents {
        deadline.check("fabrication-provenance-validation")?;
        if document.id == provenance.document_id {
            matched = Some(document);
            break;
        }
    }
    let document = matched
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
    deadline: ManufacturingDeadline,
) -> Result<(), FabricationError> {
    const RESOURCE: &str = "fabrication-geometry-validation";
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
        validate_positive_length(arc.source_resolution)?;
        if arc.quadrant == QuadrantMode::Single
            && !valid_single_quadrant_arc(
                arc.start,
                arc.end,
                arc.center,
                arc.direction,
                arc.source_resolution,
            )
        {
            return Err(FabricationError::InvalidIdentity("arc-geometry".into()));
        }
        Ok(())
    }
    fn segment(
        segment: &ContourSegment,
        deadline: ManufacturingDeadline,
    ) -> Result<(), FabricationError> {
        deadline.check(RESOURCE)?;
        match segment {
            ContourSegment::Line(value) => line(value),
            ContourSegment::Arc(value) => arc(value),
        }
    }
    fn contour(
        contour: &CanonicalContour,
        deadline: ManufacturingDeadline,
    ) -> Result<(), FabricationError> {
        deadline.check(RESOURCE)?;
        for item in &contour.segments {
            segment(item, deadline)?;
        }
        Ok(())
    }
    match geometry {
        Geometry::Point(point) => validate_point(*point),
        Geometry::Line(value) => line(value),
        Geometry::Arc(value) => arc(value),
        Geometry::Contour(value) => contour(value, deadline),
        Geometry::Region(value) => {
            for value in &value.contours {
                contour(value, deadline)?;
            }
            Ok(())
        }
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
            for item in &value.segments {
                segment(item, deadline)?;
            }
            Ok(())
        }
        Geometry::Slot(value) => {
            validate_point(value.start)?;
            validate_point(value.end)?;
            validate_positive_length(value.width)?;
            if value.start == value.end {
                return Err(FabricationError::InvalidIdentity("zero-length-slot".into()));
            }
            if !tool_ids.contains(value.tool_id.as_str()) {
                return Err(FabricationError::DanglingReference(value.tool_id.clone()));
            }
            Ok(())
        }
    }
}

pub(crate) fn exact_circle_geometry(
    geometry: &Geometry,
) -> Result<(CanonicalPoint, Picometres, Picometres), FabricationError> {
    let Geometry::Contour(contour) = geometry else {
        return Err(FabricationError::InvalidIdentity("circle-geometry".into()));
    };
    let [ContourSegment::Arc(first), ContourSegment::Arc(second)] = contour.segments.as_slice()
    else {
        return Err(FabricationError::InvalidIdentity("circle-geometry".into()));
    };
    let radius = i128::from(first.start.x.0)
        .checked_sub(i128::from(first.center.x.0))
        .map(i128::unsigned_abs)
        .and_then(|value| i64::try_from(value).ok())
        .map(Picometres)
        .ok_or(FabricationError::ArithmeticOverflow)?;
    let opposite = i128::from(first.center.x.0)
        .checked_sub(i128::from(first.end.x.0))
        .map(i128::unsigned_abs)
        .and_then(|value| i64::try_from(value).ok())
        .map(Picometres)
        .ok_or(FabricationError::ArithmeticOverflow)?;
    if !contour.closed
        || first.center != second.center
        || first.start != second.end
        || first.end != second.start
        || first.start.y != first.center.y
        || first.end.y != first.center.y
        || first.start == first.end
        || radius != opposite
        || radius.0 <= 0
        || first.width.is_some()
        || second.width.is_some()
        || first.direction != second.direction
        || first.quadrant != QuadrantMode::Multi
        || second.quadrant != QuadrantMode::Multi
        || first.source_resolution != second.source_resolution
        || first.source_resolution.0 <= 0
    {
        return Err(FabricationError::InvalidIdentity("circle-geometry".into()));
    }
    Ok((first.center, radius, first.source_resolution))
}

fn validate_transformed_geometry(
    geometry: &Geometry,
    transforms: &TransformChain,
    deadline: ManufacturingDeadline,
) -> Result<bool, FabricationError> {
    validate_transformed_geometry_at_offset(
        geometry,
        transforms,
        CanonicalPoint::default(),
        deadline,
    )
}

fn validate_transformed_geometry_at_offset(
    geometry: &Geometry,
    transforms: &TransformChain,
    offset: CanonicalPoint,
    deadline: ManufacturingDeadline,
) -> Result<bool, FabricationError> {
    const RESOURCE: &str = "fabrication-transformed-geometry-validation";
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
        deadline: ManufacturingDeadline,
    ) -> Result<(), FabricationError> {
        deadline.check(RESOURCE)?;
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
                segment(value, transforms, offset, &mut quantized, deadline)?;
            }
        }
        Geometry::Region(value) => {
            for contour in &value.contours {
                deadline.check(RESOURCE)?;
                for value in &contour.segments {
                    segment(value, transforms, offset, &mut quantized, deadline)?;
                }
            }
        }
        Geometry::Flash(value) => point(value.position, transforms, offset, &mut quantized)?,
        Geometry::Drill(value) => point(value.position, transforms, offset, &mut quantized)?,
        Geometry::Route(value) => {
            for value in &value.segments {
                segment(value, transforms, offset, &mut quantized, deadline)?;
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

type CanonicalProvenance<'a> = (&'a str, &'a str, &'a str, &'a str, &'a StructuralLocation);

fn canonical_provenance(provenance: &ManufacturingProvenance) -> CanonicalProvenance<'_> {
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
    deadline: ManufacturingDeadline,
) -> Result<Vec<CanonicalProvenance<'_>>, FabricationError> {
    let mut values = BTreeSet::new();
    for provenance in provenances {
        deadline.check("fabrication-model-digest")?;
        values.insert(canonical_provenance(provenance));
    }
    let mut canonical = Vec::with_capacity(values.len());
    for value in values {
        deadline.check("fabrication-model-digest")?;
        canonical.push(value);
    }
    Ok(canonical)
}

fn canonical_refs<T: Ord>(
    values: &[T],
    deadline: ManufacturingDeadline,
) -> Result<BTreeSet<&T>, FabricationError> {
    checked_btree_set_with_deadline(values.iter(), deadline, "fabrication-model-digest")
}

fn checked_all_with_deadline<I, F>(
    values: I,
    deadline: ManufacturingDeadline,
    resource: &'static str,
    mut predicate: F,
) -> Result<bool, FabricationError>
where
    I: IntoIterator,
    F: FnMut(I::Item) -> bool,
{
    for value in values {
        deadline.check(resource)?;
        if !predicate(value) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn checked_any_with_deadline<I, F>(
    values: I,
    deadline: ManufacturingDeadline,
    resource: &'static str,
    mut predicate: F,
) -> Result<bool, FabricationError>
where
    I: IntoIterator,
    F: FnMut(I::Item) -> bool,
{
    for value in values {
        deadline.check(resource)?;
        if predicate(value) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn checked_btree_set_with_deadline<I>(
    values: I,
    deadline: ManufacturingDeadline,
    resource: &'static str,
) -> Result<BTreeSet<I::Item>, FabricationError>
where
    I: IntoIterator,
    I::Item: Ord,
{
    let mut output = BTreeSet::new();
    for value in values {
        deadline.check(resource)?;
        output.insert(value);
    }
    Ok(output)
}

fn checked_retain_with_deadline<T: Clone>(
    values: &mut Vec<T>,
    deadline: ManufacturingDeadline,
    resource: &'static str,
    mut retain: impl FnMut(&T) -> bool,
) -> Result<(), FabricationError> {
    let mut output = Vec::with_capacity(values.len());
    for value in values.iter() {
        deadline.check(resource)?;
        if retain(value) {
            output.push(value.clone());
        }
    }
    *values = output;
    Ok(())
}

#[cfg(test)]
fn checked_slice_equal_with_deadline<T: PartialEq>(
    left: &[T],
    right: &[T],
    deadline: ManufacturingDeadline,
    resource: &'static str,
) -> Result<bool, FabricationError> {
    if left.len() != right.len() {
        return Ok(false);
    }
    checked_all_with_deadline(
        left.iter().zip(right),
        deadline,
        resource,
        |(left, right)| left == right,
    )
}

fn canonical_json_with_deadline(
    deadline: ManufacturingDeadline,
    label: &str,
    value: &impl Serialize,
) -> Result<String, FabricationError> {
    let (_, bytes) =
        serialize_with_deadline(deadline, "fabrication-model-digest", &(label, value), true)?;
    String::from_utf8(bytes.expect("canonical JSON retention enabled"))
        .map_err(|error| FabricationError::Serialization(error.to_string()))
}

struct CanonicalDeadlineReader<R> {
    inner: R,
    deadline: ManufacturingDeadline,
    resource: &'static str,
}

impl<R: Read> Read for CanonicalDeadlineReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.deadline.check(self.resource).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::TimedOut, "canonical JSON deadline")
        })?;
        let bounded = buffer.len().min(4096);
        self.inner.read(&mut buffer[..bounded])
    }
}

fn canonical_json_from_reader_with_deadline(
    reader: impl Read,
    deadline: ManufacturingDeadline,
    resource: &'static str,
) -> Result<Value, FabricationError> {
    let reader = CanonicalDeadlineReader {
        inner: reader,
        deadline,
        resource,
    };
    let mut deserializer =
        serde_json::Deserializer::from_reader(BufReader::with_capacity(4096, reader));
    let value = Value::deserialize(&mut deserializer).map_err(|_| {
        deadline.check(resource).err().unwrap_or_else(|| {
            FabricationError::InvalidIdentity("reconciliation-canonical-json".into())
        })
    })?;
    deserializer.end().map_err(|_| {
        deadline.check(resource).err().unwrap_or_else(|| {
            FabricationError::InvalidIdentity("reconciliation-canonical-json".into())
        })
    })?;
    deadline.check(resource)?;
    Ok(value)
}

fn parse_canonical_json_with_deadline(
    value: &str,
    deadline: ManufacturingDeadline,
    resource: &'static str,
) -> Result<Value, FabricationError> {
    canonical_json_from_reader_with_deadline(Cursor::new(value.as_bytes()), deadline, resource)
}

fn canonical_json_valid_with_deadline(
    value: &str,
    deadline: ManufacturingDeadline,
    resource: &'static str,
) -> Result<bool, FabricationError> {
    match parse_canonical_json_with_deadline(value, deadline, resource) {
        Ok(_) => Ok(true),
        Err(error @ FabricationError::LimitExceeded { .. }) => Err(error),
        Err(_) => Ok(false),
    }
}

fn chunked_bytes_equal_with_deadline(
    left: &[u8],
    right: &[u8],
    deadline: ManufacturingDeadline,
    resource: &'static str,
) -> Result<bool, FabricationError> {
    if left.len() != right.len() {
        return Ok(false);
    }
    for (left, right) in left.chunks(4096).zip(right.chunks(4096)) {
        deadline.check(resource)?;
        if left != right {
            return Ok(false);
        }
    }
    deadline.check(resource)?;
    Ok(true)
}

fn chunked_str_equal_with_deadline(
    left: &str,
    right: &str,
    deadline: ManufacturingDeadline,
    resource: &'static str,
) -> Result<bool, FabricationError> {
    chunked_bytes_equal_with_deadline(left.as_bytes(), right.as_bytes(), deadline, resource)
}

fn update_sha256_with_deadline_observer(
    hasher: &mut Sha256,
    value: &[u8],
    deadline: ManufacturingDeadline,
    resource: &'static str,
    observer: &mut impl FnMut(usize),
) -> Result<(), FabricationError> {
    for chunk in value.chunks(4096) {
        deadline.check(resource)?;
        observer(chunk.len());
        hasher.update(chunk);
    }
    Ok(())
}

fn update_sha256_with_deadline(
    hasher: &mut Sha256,
    value: &[u8],
    deadline: ManufacturingDeadline,
    resource: &'static str,
) -> Result<(), FabricationError> {
    update_sha256_with_deadline_observer(hasher, value, deadline, resource, &mut |_| {})
}

fn sha256_with_deadline_observer(
    value: &[u8],
    deadline: ManufacturingDeadline,
    resource: &'static str,
    observer: &mut impl FnMut(usize),
) -> Result<String, FabricationError> {
    let mut hasher = Sha256::new();
    update_sha256_with_deadline_observer(&mut hasher, value, deadline, resource, observer)?;
    deadline.check(resource)?;
    Ok(format!("{:x}", hasher.finalize()))
}

pub(crate) fn sha256_with_deadline(
    value: &[u8],
    deadline: ManufacturingDeadline,
    resource: &'static str,
) -> Result<String, FabricationError> {
    sha256_with_deadline_observer(value, deadline, resource, &mut |_| {})
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
        "required": ["status", "packageId", "modelDigest", "inputOutcomes", "product", "documents", "layers", "tools", "apertures", "macros", "blocks", "repetitions", "features", "physicalBounds", "profile", "connectivity", "padHoleAssociations", "x2Attributes", "jobFileFunctions", "assembly", "construction", "constraints", "capabilities", "omissions", "conflicts", "sourcePair", "nativeReconciliationSource", "integrationOutcome", "reconciliations", "warnings", "limits", "estimatedAllocationBytes"],
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
            "physicalBounds": { "type": "array", "maxItems": MANUFACTURING_LIMITS.recognized_files, "items": { "$ref": "#/$defs/documentPhysicalBounds" } },
            "profile": { "oneOf": [{ "$ref": "#/$defs/boardProfile" }, { "type": "null" }] },
            "connectivity": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/objectSemantics" } },
            "padHoleAssociations": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/padHoleAssociation" } },
            "x2Attributes": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/scopedX2Attribute" } },
            "jobFileFunctions": { "type": "array", "maxItems": MANUFACTURING_LIMITS.recognized_files, "items": { "$ref": "#/$defs/jobFileFunctionFact" } },
            "assembly": { "$ref": "#/$defs/assemblyEvidence" },
            "construction": { "$ref": "#/$defs/constructionEvidence" },
            "constraints": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/manufacturingConstraint" } },
            "capabilities": { "$ref": "#/$defs/capabilityLedger" },
            "omissions": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/manufacturingOmission" } },
            "conflicts": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/manufacturingConflict" } },
            "sourcePair": { "oneOf": [{ "$ref": "#/$defs/manufacturingSourcePair" }, { "type": "null" }] },
            "nativeReconciliationSource": { "oneOf": [{ "$ref": "#/$defs/nativeReconciliationSource" }, { "type": "null" }] },
            "integrationOutcome": { "oneOf": [{ "$ref": "#/$defs/integratedReconciliationOutcome" }, { "type": "null" }] },
            "reconciliations": { "type": "array", "maxItems": 6, "items": { "$ref": "#/$defs/manufacturingReconciliation" } },
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
                    "kindCandidate": { "enum": ["gerber", "excellon", "gerber_job"] },
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
            json!({ "type": "object", "additionalProperties": false, "required": ["id", "documentId", "apertureId", "featureIds", "instantiationFeatureIds", "definitionEnd", "provenance"], "properties": {
                "id": { "type": "string", "pattern": "^block-v1-[0-9a-f]{64}$" }, "documentId": { "$ref": "#/$defs/documentId" },
                "apertureId": { "type": "string", "pattern": "^aperture-v1-[0-9a-f]{64}$" },
                "featureIds": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "uniqueItems": true, "items": { "$ref": "#/$defs/featureId" } },
                "instantiationFeatureIds": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "uniqueItems": true, "items": { "$ref": "#/$defs/featureId" } },
                "definitionEnd": { "$ref": "#/$defs/structuralLocation" }, "provenance": { "$ref": "#/$defs/manufacturingProvenance" }
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
            "documentPhysicalBounds",
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["id", "documentId", "artifactDigest", "format", "extent", "resolution", "geometryDigest", "sourceLocations", "provenance"],
                "properties": {
                    "id": { "type": "string", "pattern": "^physical-bounds-v1-[0-9a-f]{64}$" },
                    "documentId": { "$ref": "#/$defs/documentId" },
                    "artifactDigest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                    "format": { "enum": ["gerber", "excellon"] },
                    "extent": { "$ref": "#/$defs/extent" },
                    "resolution": { "$ref": "#/$defs/positivePicometres" },
                    "geometryDigest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                    "sourceLocations": { "type": "array", "minItems": 1, "maxItems": MANUFACTURING_LIMITS.geometry_features, "uniqueItems": true, "items": { "$ref": "#/$defs/structuralLocation" } },
                    "provenance": { "$ref": "#/$defs/manufacturingProvenance" }
                }
            }),
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
            "padHoleAssociation",
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["id", "padId", "holeId", "toolId", "applicableLayerIds", "plating", "span", "padGeometry", "holeGeometry", "padProvenance", "holeProvenance"],
                "properties": {
                    "id": { "type": "string", "pattern": "^pad-hole-association-v1-[0-9a-f]{64}$" },
                    "padId": { "type": "string", "pattern": "^pad-v1-[0-9a-f]{64}$" },
                    "holeId": { "$ref": "#/$defs/featureId" },
                    "toolId": { "$ref": "#/$defs/toolId" },
                    "applicableLayerIds": { "type": "array", "minItems": 1, "maxItems": MANUFACTURING_LIMITS.geometry_features, "uniqueItems": true, "items": { "$ref": "#/$defs/layerId" } },
                    "plating": { "const": "plated" },
                    "span": { "$ref": "#/$defs/layerSpan" },
                    "padGeometry": { "type": "object", "additionalProperties": false, "required": ["kind", "value"], "properties": { "kind": { "const": "contour" }, "value": { "$ref": "#/$defs/canonicalContour" } } },
                    "holeGeometry": { "type": "object", "additionalProperties": false, "required": ["kind", "value"], "properties": { "kind": { "const": "drill" }, "value": { "$ref": "#/$defs/drillFeature" } } },
                    "padProvenance": { "$ref": "#/$defs/manufacturingProvenance" },
                    "holeProvenance": { "$ref": "#/$defs/manufacturingProvenance" }
                }
            }),
        ),
        (
            "scopedX2Attribute",
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["id", "documentId", "scope", "kind", "values", "deletion", "targetIds", "provenance"],
                "properties": {
                    "id": { "type": "string", "pattern": "^x2-attribute-v1-[0-9a-f]{64}$" },
                    "documentId": { "$ref": "#/$defs/documentId" },
                    "scope": { "enum": ["file", "aperture", "object"] },
                    "kind": { "enum": ["file_function", "aperture_function", "net", "component", "pin", "reset"] },
                    "values": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "type": "string", "minLength": 1, "maxLength": MANUFACTURING_LIMITS.max_text_bytes } },
                    "deletion": { "type": "boolean" },
                    "targetIds": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "uniqueItems": true, "items": { "type": "string", "pattern": "^(document|aperture|feature)-v1-[0-9a-f]{64}$" } },
                    "provenance": { "$ref": "#/$defs/manufacturingProvenance" }
                }
            }),
        ),
        (
            "jobFileFunctionFact",
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["id", "jobDocumentId", "jobArtifactDigest", "referencedVirtualPath", "referencedDocumentId", "referencedArtifactDigest", "fields", "role", "side", "order", "plating", "fromLayer", "toLayer", "qualifier", "operation", "omission", "conflictIds", "provenance"],
                "properties": {
                    "id": { "type": "string", "pattern": "^job-file-function-v1-[0-9a-f]{64}$" },
                    "jobDocumentId": { "$ref": "#/$defs/documentId" },
                    "jobArtifactDigest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                    "referencedVirtualPath": { "type": "string", "minLength": 1, "maxLength": MANUFACTURING_LIMITS.normalized_path_bytes },
                    "referencedDocumentId": { "$ref": "#/$defs/documentId" },
                    "referencedArtifactDigest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                    "fields": { "type": "array", "minItems": 1, "maxItems": 8, "items": { "type": "string", "minLength": 1, "maxLength": MANUFACTURING_LIMITS.max_text_bytes } },
                    "role": { "enum": ["copper", "solder_mask", "paste", "legend", "profile", "drill_map", "route", "assembly", "fabrication_drawing", "other", "unknown"] },
                    "side": { "enum": ["top", "bottom", "inner", "both", "not_applicable", "unknown"] },
                    "order": { "type": ["integer", "null"], "minimum": -2147483648_i64, "maximum": 2147483647_i64 },
                    "plating": { "enum": ["plated", "non_plated", "mixed", "unknown"] },
                    "fromLayer": { "type": ["integer", "null"], "minimum": -2147483648_i64, "maximum": 2147483647_i64 },
                    "toLayer": { "type": ["integer", "null"], "minimum": -2147483648_i64, "maximum": 2147483647_i64 },
                    "qualifier": { "type": ["string", "null"], "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                    "operation": { "type": ["string", "null"], "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                    "omission": { "type": ["string", "null"], "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                    "conflictIds": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "uniqueItems": true, "items": { "type": "string", "minLength": 1, "maxLength": MANUFACTURING_LIMITS.max_text_bytes } },
                    "provenance": { "$ref": "#/$defs/manufacturingProvenance" }
                }
            }),
        ),
        (
            "assemblyPlacementConvention",
            json!({ "type": "object", "additionalProperties": false, "required": ["unit", "origin", "side", "bottomMirroring", "rotationDirection"], "properties": {
                "unit": { "enum": ["millimetre", "inch", null] },
                "origin": { "enum": ["kicad_board", "unknown"] },
                "side": { "enum": ["top_bottom", "unknown"] },
                "bottomMirroring": { "enum": ["mirrored", "unmirrored", "unknown"] },
                "rotationDirection": { "enum": ["counter_clockwise", "clockwise", "unknown"] }
            } }),
        ),
        (
            "assemblyPlacement",
            json!({ "type": "object", "additionalProperties": false, "required": ["id", "occurrenceId", "reference", "side", "position", "rotationMicrodegrees", "fitted", "revision", "convention", "provenance"], "properties": {
                "id": { "type": "string", "pattern": "^assembly-placement-v1-[0-9a-f]{64}$" },
                "occurrenceId": { "type": ["string", "null"], "minLength": 1, "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                "reference": { "type": "string", "minLength": 1, "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                "side": { "enum": ["top", "bottom", "inner", "both", "not_applicable", "unknown"] },
                "position": { "$ref": "#/$defs/canonicalPoint" },
                "rotationMicrodegrees": { "type": "integer", "minimum": 0, "maximum": 359_999_999 },
                "fitted": { "enum": ["fitted", "not_fitted", "unknown"] },
                "revision": { "type": ["string", "null"], "minLength": 1, "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                "convention": { "$ref": "#/$defs/assemblyPlacementConvention" },
                "provenance": { "$ref": "#/$defs/manufacturingProvenance" }
            } }),
        ),
        (
            "declaredAssemblyPlacement",
            json!({ "type": "object", "additionalProperties": false, "required": ["id", "reference", "side", "position", "rotationMicrodegrees", "fitted", "revision", "convention", "sourcePath", "artifactDigest", "line"], "properties": {
                "id": { "type": "string", "pattern": "^declared-assembly-placement-v1-[0-9a-f]{64}$" },
                "reference": { "type": "string", "minLength": 1, "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                "side": { "enum": ["top", "bottom"] },
                "position": { "$ref": "#/$defs/canonicalPoint" },
                "rotationMicrodegrees": { "type": "integer", "minimum": 0, "maximum": 359_999_999 },
                "fitted": { "enum": ["fitted", "not_fitted"] },
                "revision": { "type": "string", "minLength": 1, "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                "convention": { "$ref": "#/$defs/assemblyPlacementConvention" },
                "sourcePath": { "type": "string", "minLength": 1, "maxLength": MANUFACTURING_LIMITS.normalized_path_bytes },
                "artifactDigest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                "line": { "type": "integer", "minimum": 1, "maximum": MANUFACTURING_LIMITS.geometry_features }
            } }),
        ),
        (
            "nativeCourtyardObservation",
            json!({ "type": "object", "additionalProperties": false, "required": ["id", "kind", "exclusion", "location"], "properties": {
                "id": { "type": "string", "pattern": "^native-courtyard-observation-v1-[0-9a-f]{64}$" },
                "kind": { "enum": ["overlap", "malformed", "missing"] },
                "exclusion": { "enum": ["active", "excluded", "unknown"] },
                "location": { "type": "string", "minLength": 1, "maxLength": MANUFACTURING_LIMITS.max_text_bytes }
            } }),
        ),
        (
            "nativeCourtyardEvidence",
            json!({ "type": "object", "additionalProperties": false, "required": ["state", "tool", "version", "source", "observations"], "properties": {
                "state": { "enum": ["complete", "partial", "not_run", "disabled", "failed"] },
                "tool": { "type": "string", "minLength": 1, "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                "version": { "type": ["string", "null"], "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                "source": { "type": ["string", "null"], "maxLength": MANUFACTURING_LIMITS.normalized_path_bytes },
                "observations": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/nativeCourtyardObservation" } }
            } }),
        ),
        (
            "assemblyEvidence",
            json!({ "type": "object", "additionalProperties": false, "required": ["placements", "declaredPlacements", "nativeCourtyard", "maskLayerIds", "pasteLayerIds"], "properties": {
                "placements": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/assemblyPlacement" } },
                "declaredPlacements": { "type": "array", "maxItems": MANUFACTURING_LIMITS.geometry_features, "items": { "$ref": "#/$defs/declaredAssemblyPlacement" } },
                "nativeCourtyard": { "oneOf": [{ "$ref": "#/$defs/nativeCourtyardEvidence" }, { "type": "null" }] },
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
                    "format": { "enum": ["gerber", "excellon", "gerber_job", "kicad_pcb", "unknown"] },
                    "adapter": { "type": "string", "minLength": 1, "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                    "adapterVersion": { "type": "string", "minLength": 1, "maxLength": MANUFACTURING_LIMITS.max_text_bytes },
                    "parseStatus": { "enum": ["complete", "partial", "failed", "unsupported", "not_provided"] },
                    "numericFormat": { "oneOf": [{ "$ref": "#/$defs/sourceNumericFormat" }, { "type": "null" }] },
                    "metrics": { "$ref": "#/$defs/documentMetrics" }
                }
            }),
        ),
        (
            "featureMembership",
            json!({
                "oneOf": [
                    { "type": "object", "additionalProperties": false, "required": ["kind"], "properties": { "kind": { "const": "top_level" } } },
                    { "type": "object", "additionalProperties": false, "required": ["kind", "blockId", "apertureId"], "properties": {
                        "kind": { "const": "aperture_block" },
                        "blockId": { "type": "string", "pattern": "^block-v1-[0-9a-f]{64}$" },
                        "apertureId": { "type": "string", "pattern": "^aperture-v1-[0-9a-f]{64}$" }
                    } }
                ]
            }),
        ),
        (
            "manufacturingFeature",
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["id", "documentId", "layerId", "toolId", "polarity", "geometry", "transforms", "membership", "provenance"],
                "properties": {
                    "id": { "type": "string", "pattern": "^feature-v1-[0-9a-f]{64}$" },
                    "documentId": { "$ref": "#/$defs/documentId" }, "layerId": { "$ref": "#/$defs/layerId" },
                    "toolId": { "oneOf": [{ "$ref": "#/$defs/toolId" }, { "type": "null" }] },
                    "polarity": { "$ref": "#/$defs/layerPolarity" },
                    "geometry": { "$ref": "#/$defs/geometry" }, "transforms": { "$ref": "#/$defs/transformChain" },
                    "membership": { "$ref": "#/$defs/featureMembership" }, "provenance": { "$ref": "#/$defs/manufacturingProvenance" }
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
            "reconciliationFact",
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["modelIds", "canonicalValue", "resolution", "authority", "provenance"],
                "properties": {
                    "modelIds": { "type": "array", "minItems": 1, "maxItems": MANUFACTURING_LIMITS.geometry_features, "uniqueItems": true, "items": { "type": "string", "minLength": 1, "maxLength": MANUFACTURING_LIMITS.max_text_bytes } },
                    "canonicalValue": { "type": "string", "maxLength": RECONCILIATION_VALUE_BYTES },
                    "resolution": { "oneOf": [{ "$ref": "#/$defs/positivePicometres" }, { "type": "null" }] },
                    "authority": { "$ref": "#/$defs/authority" },
                    "provenance": { "$ref": "#/$defs/manufacturingProvenance" }
                }
            }),
        ),
        (
            "manufacturingReconciliation",
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["id", "family", "status", "confidence", "native", "package", "smallestEvidenceAction"],
                "properties": {
                    "id": { "type": "string", "pattern": "^reconciliation-v1-[0-9a-f]{64}$" },
                    "family": { "enum": ["product", "layers", "profile", "drills", "extents", "connectivity"] },
                    "status": { "enum": ["match", "mismatch", "not_checked"] },
                    "confidence": { "enum": ["exact", "resolution_bounded", "unavailable"] },
                    "native": { "$ref": "#/$defs/reconciliationFact" },
                    "package": { "$ref": "#/$defs/reconciliationFact" },
                    "smallestEvidenceAction": { "type": "string", "minLength": 1, "maxLength": MANUFACTURING_LIMITS.max_text_bytes }
                }
            }),
        ),
        (
            "nativeReconciliationSource",
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["review", "extents"],
                "properties": {
                    "review": { "$ref": "#/$defs/fabricationReview" },
                    "extents": { "oneOf": [{ "$ref": "#/$defs/extent" }, { "type": "null" }] }
                }
            }),
        ),
        (
            "manufacturingSourcePair",
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["id", "nativeDocumentId", "nativeArtifactDigest", "releasePackageId", "releaseDocumentDigests"],
                "properties": {
                    "id": { "type": "string", "pattern": "^source-pair-v1-[0-9a-f]{64}$" },
                    "nativeDocumentId": { "$ref": "#/$defs/documentId" },
                    "nativeArtifactDigest": { "type": "string", "pattern": "^[0-9a-f]{64}$" },
                    "releasePackageId": { "type": "string", "pattern": "^package-v1-[0-9a-f]{64}$" },
                    "releaseDocumentDigests": { "type": "array", "minItems": 1, "maxItems": MANUFACTURING_LIMITS.recognized_files, "uniqueItems": true, "items": { "type": "string", "pattern": "^[0-9a-f]{64}$" } }
                }
            }),
        ),
        (
            "integratedReconciliationOutcome",
            json!({
                "type": "object", "additionalProperties": false,
                "required": ["id", "state", "attemptedNativePath", "attemptedNativeDigest", "reason"],
                "properties": {
                    "id": { "type": "string", "pattern": "^integration-outcome-v1-[0-9a-f]{64}$" },
                    "state": { "enum": ["not_provided", "failed"] },
                    "attemptedNativePath": { "type": ["string", "null"], "maxLength": MANUFACTURING_LIMITS.normalized_path_bytes },
                    "attemptedNativeDigest": { "type": ["string", "null"], "pattern": "^[0-9a-f]{64}$" },
                    "reason": { "type": "string", "minLength": 1, "maxLength": MANUFACTURING_LIMITS.max_text_bytes }
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageFileFunction {
    pub raw: String,
    pub role: LayerRole,
    pub side: LayerSide,
    pub order: Option<i32>,
    pub plating: Plating,
    pub from_layer: Option<i32>,
    pub to_layer: Option<i32>,
    pub qualifier: Option<String>,
    pub operation: Option<String>,
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
    pub file_function: Option<PackageFileFunction>,
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
        Self::build(bytes, ManufacturingDeadline::from_timeout(timeout))
    }

    fn build(bytes: &[u8], deadline: ManufacturingDeadline) -> Result<Self, GerberParseError> {
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
        check_gerber_deadline(deadline, "byte-boundary")?;

        let mut max_line_bytes = 0_usize;
        let mut line_bytes = 0_usize;
        let mut index = 0_usize;
        while index < bytes.len() {
            if index % 4_096 == 0 {
                check_gerber_deadline(deadline, "byte-boundary")?;
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
            check_gerber_deadline(deadline, "byte-framing")?;
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
                .checked_add(count_gerber_tokens(&parser_frame, deadline)?)
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
        check_gerber_deadline(deadline, "byte-boundary")?;
        let original_digest = sha256_with_deadline(bytes, deadline, "gerber-boundary-hash")
            .map_err(|error| match error {
                FabricationError::LimitExceeded { .. } => GerberParseError::Deadline {
                    stage: "boundary-hash",
                },
                error => GerberParseError::Canonical(error),
            })?;
        let mut original_bytes = Vec::with_capacity(bytes.len());
        for chunk in bytes.chunks(4096) {
            check_gerber_deadline(deadline, "gerber-boundary-copy")?;
            original_bytes.extend_from_slice(chunk);
        }
        Ok(Self {
            original_bytes,
            original_digest,
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

fn count_gerber_tokens(
    bytes: &[u8],
    deadline: ManufacturingDeadline,
) -> Result<u64, GerberParseError> {
    let mut count = 0_u64;
    let mut in_word = false;
    for (index, byte) in bytes.iter().copied().enumerate() {
        if index & 0x0fff == 0 {
            check_gerber_deadline(deadline, "lexical-tokens")?;
        }
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
    deadline: ManufacturingDeadline,
    stage: &'static str,
) -> Result<(), GerberParseError> {
    if Instant::now() >= deadline.at {
        Err(GerberParseError::Deadline { stage })
    } else {
        Ok(())
    }
}

struct GerberDeadlineReader<'a> {
    cursor: Cursor<&'a [u8]>,
    deadline: ManufacturingDeadline,
}

impl Read for GerberDeadlineReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if Instant::now() >= self.deadline.at {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Gerber parser deadline",
            ));
        }
        let bounded = buffer.len().min(4096);
        self.cursor.read(&mut buffer[..bounded])
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
            file_started: None,
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
            aggregate_started: None,
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
                file_started: None,
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
            aggregate_started: None,
        };
        assert!(matches!(
            parse_gerber_inventory_with_timeout(&bomb, Duration::from_micros(50)),
            Err(GerberParseError::Deadline { stage: "aggregate" })
        ));
    }

    #[test]
    fn round7_deadline_inventory_validation_does_not_restart_or_consume_all_bytes() {
        let mut bytes = b"%FSLAX46Y46*%%MOMM*%".to_vec();
        bytes.extend_from_slice(b"G04 aggregate-deadline*\n".repeat(100_000).as_slice());
        bytes.extend_from_slice(b"M02*");
        let digest = sha256(&bytes);
        let size = bytes.len() as u64;
        let path = "round7-inventory-deadline.gbr".to_string();
        let inventory = ManufacturingInventory {
            inputs: vec![ManufacturingInput {
                virtual_path: path.clone(),
                artifact_digest: digest.clone(),
                kind_candidate: ManufacturingKindCandidate::Gerber,
                size,
                original_bytes: bytes,
                file_started: None,
            }],
            outcomes: vec![ManufacturingInputOutcome {
                id: input_outcome_id(&path, Some(&digest), ManufacturingKindCandidate::Gerber),
                virtual_path: path,
                artifact_digest: Some(digest),
                kind_candidate: ManufacturingKindCandidate::Gerber,
                size,
                state: ManufacturingLoadState::Retained,
                reason: None,
            }],
            aggregate_started: None,
        };
        let mut consumed = 0_usize;
        let result = inventory.validate_with_deadline_counting(
            ManufacturingDeadline::from_timeout(Duration::from_millis(2)),
            |bytes| {
                consumed += bytes;
                std::thread::sleep(Duration::from_micros(100));
            },
        );
        assert!(matches!(
            result,
            Err(FabricationError::LimitExceeded {
                resource: "manufacturing-inventory-hash"
            })
        ));
        assert!(consumed > 0 && consumed < size as usize);
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
            file_started: None,
        };
        let timeout = Duration::from_millis(100);

        let parser_started = Instant::now();
        let parser_deadline = ManufacturingDeadline::from_timeout(timeout);
        let boundary = GerberByteBoundary::build(&input.original_bytes, parser_deadline).unwrap();
        assert!(parser_started.elapsed() < timeout);
        std::thread::sleep(timeout);
        assert!(matches!(
            parse_gerber_document_after_boundary(&input, boundary, parser_deadline),
            Err(GerberParseError::Deadline {
                stage: "parser-reconciliation"
            })
        ));

        let interpreter_started = Instant::now();
        let interpreter_deadline = ManufacturingDeadline::from_timeout(timeout);
        let boundary =
            GerberByteBoundary::build(&input.original_bytes, interpreter_deadline).unwrap();
        let document_id = document_id(&boundary.original_digest, DocumentFormat::Gerber).unwrap();
        let (accounting, issues, routes) =
            account_gerber_parser(&boundary, &document_id, interpreter_deadline).unwrap();
        assert!(interpreter_started.elapsed() < timeout);
        std::thread::sleep(timeout);
        assert!(matches!(
            GerberInterpreter::new(
                &input,
                boundary,
                accounting,
                issues,
                routes,
                interpreter_deadline,
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
    deadline: ManufacturingDeadline,
) -> Result<
    (
        GerberParserAccounting,
        Vec<GerberParserIssue>,
        Vec<GerberRouteFileFunctionEvidence>,
    ),
    GerberParseError,
> {
    check_gerber_deadline(deadline, "parser-reconciliation")?;
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
        deadline,
    };
    let (document, fatal) = match parse_gerber(BufReader::new(reader)) {
        Ok(document) => (document, false),
        Err((document, _)) => (document, true),
    };
    check_gerber_deadline(deadline, "parser-reconciliation")?;

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
    let deadline = ManufacturingDeadline::from_timeout(timeout).for_input(input);
    parse_gerber_document_with_deadline(input, deadline).map(|(production, _, _)| production)
}

fn parse_gerber_document_with_deadline(
    input: &ManufacturingInput,
    deadline: ManufacturingDeadline,
) -> Result<(GerberProduction, u64, u64), GerberParseError> {
    let digest = sha256_with_deadline(&input.original_bytes, deadline, "gerber-input-hash")
        .map_err(|error| match error {
            FabricationError::LimitExceeded { .. } => GerberParseError::Deadline {
                stage: "input-hash",
            },
            error => GerberParseError::Canonical(error),
        })?;
    if input.kind_candidate != ManufacturingKindCandidate::Gerber
        || input.size != input.original_bytes.len() as u64
        || input.size > MANUFACTURING_LIMITS.raw_bytes_per_file
        || input.artifact_digest != digest
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
    let boundary = GerberByteBoundary::build(&input.original_bytes, deadline)?;
    let source_records = boundary.metrics.records;
    let lexical_tokens = boundary.metrics.lexical_tokens;
    let production = parse_gerber_document_after_boundary(input, boundary, deadline)?;
    Ok((production, source_records, lexical_tokens))
}

fn parse_gerber_document_after_boundary(
    input: &ManufacturingInput,
    boundary: GerberByteBoundary,
    deadline: ManufacturingDeadline,
) -> Result<GerberProduction, GerberParseError> {
    let document_id = document_id(&boundary.original_digest, DocumentFormat::Gerber)
        .map_err(GerberParseError::Canonical)?;
    let (accounting, issues, routes) = account_gerber_parser(&boundary, &document_id, deadline)?;
    GerberInterpreter::new(input, boundary, accounting, issues, routes, deadline).run()
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
    let deadline = ManufacturingDeadline::for_inventory(inventory, timeout);
    inventory
        .validate_with_deadline(deadline)
        .map_err(|error| {
            if deadline.check("manufacturing-aggregate").is_err() {
                GerberParseError::Deadline { stage: "aggregate" }
            } else {
                GerberParseError::Canonical(error)
            }
        })?;
    check_gerber_deadline(deadline, "aggregate")?;
    let mut aggregate = GerberAggregateAccounting::default();
    let mut result = Vec::new();
    for input in inventory
        .inputs
        .iter()
        .filter(|input| input.kind_candidate == ManufacturingKindCandidate::Gerber)
    {
        check_gerber_deadline(deadline, "aggregate")?;
        let file_deadline = deadline.for_input(input);
        let (parsed, source_records, lexical_tokens) =
            match parse_gerber_document_with_deadline(input, file_deadline) {
                Err(GerberParseError::Deadline { .. }) if file_deadline.at == deadline.at => {
                    return Err(GerberParseError::Deadline { stage: "aggregate" });
                }
                outcome => outcome?,
            };
        check_gerber_deadline(deadline, "aggregate")?;
        aggregate.add(
            input.size,
            source_records,
            parsed.accounting.parser_results,
            lexical_tokens,
        )?;
        result.push(parsed);
    }
    check_gerber_deadline(deadline, "aggregate")?;
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
    deadline: ManufacturingDeadline,
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
        deadline: ManufacturingDeadline,
    ) -> Self {
        let document_id = document_id(&boundary.original_digest, DocumentFormat::Gerber)
            .expect("validated original digest");
        let first = boundary
            .frames
            .first()
            .expect("parser accepted a nonempty document");
        let first_provenance =
            gerber_provenance_for(&document_id, &boundary.original_digest, first);
        let layer_id = layer_id(
            &document_id,
            None,
            LayerRole::Unknown,
            LayerSide::Unknown,
            None,
            Authority::FileContent,
            &first_provenance.location,
        );
        Self {
            input,
            boundary,
            accounting,
            parser_issues,
            routes,
            deadline,
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
        let frames = std::mem::take(&mut self.boundary.frames);
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
            let mut matched_frame = None;
            for frame in &frames {
                self.deadline("normalization-warning")?;
                if warning.byte_start >= frame.byte_start && warning.byte_end <= frame.byte_end {
                    matched_frame = Some(frame);
                    break;
                }
            }
            let frame = matched_frame.expect("normalization warning belongs to one frame");
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
        let block_index_deadline = self.deadline;
        let mut block_instantiations = HashMap::<&str, Vec<String>>::new();
        for feature in &self.features {
            check_gerber_deadline(block_index_deadline, "block-instantiation-index")?;
            if let Geometry::Flash(flash) = &feature.geometry {
                block_instantiations
                    .entry(flash.aperture_id.as_str())
                    .or_default()
                    .push(feature.id.clone());
            }
        }
        for block in &mut self.blocks {
            check_gerber_deadline(block_index_deadline, "block-instantiation-index")?;
            block.instantiation_feature_ids = block_instantiations
                .remove(block.aperture_id.as_str())
                .unwrap_or_default();
        }
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
        let deadline = self.deadline;
        let mut review = FabricationReview::empty_with_deadline(deadline)
            .map_err(GerberParseError::Canonical)?;
        review.status = FabricationStatus::Partial;
        review.input_outcomes = vec![outcome];
        review.documents = vec![document];
        review.layers = vec![layer];
        review.tools = self.tools;
        review.apertures = self.aperture_facts;
        review.macros = self.macros;
        review.blocks = self.blocks;
        review.repetitions = self.repetitions;
        review.features = self.features;
        review.capabilities = CapabilityLedger {
            records: capabilities,
        };
        review.omissions = omissions;
        review.warnings = self.warnings;
        review
            .finalize_trusted_with_deadline(deadline)
            .map_err(GerberParseError::Canonical)?;
        check_gerber_deadline(deadline, "canonicalization")?;
        Ok(GerberProduction {
            review,
            original_digest: self.boundary.original_digest,
            accounting: self.accounting,
            parser_issues: self.parser_issues,
            normalization_warnings: self.boundary.warnings,
            attributes: self.attributes,
            route_file_functions: self.routes,
            file_function: None,
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
        check_gerber_deadline(self.deadline, stage)
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
            let mut feature_ids = Vec::with_capacity(self.features.len() - block.feature_start);
            for feature in &self.features[block.feature_start..] {
                check_gerber_deadline(self.deadline, "aperture-block-close")?;
                feature_ids.push(feature.id.clone());
            }
            let mut bounds = GerberBounds::default();
            let mut expansion = GerberExpansionWeight::default();
            for (feature, weight) in self.features[block.feature_start..]
                .iter()
                .zip(&self.feature_weights[block.feature_start..])
            {
                check_gerber_deadline(self.deadline, "aperture-block-close")?;
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
            let id = record_id("block", &self.document_id, &provenance.location);
            let aperture_id = aperture_id(
                &self.document_id,
                ApertureShape::Block,
                &provenance.location,
            );
            self.blocks.push(ApertureBlock {
                id,
                document_id: self.document_id.clone(),
                aperture_id,
                feature_ids,
                instantiation_feature_ids: Vec::new(),
                definition_end: self.provenance(frame).location,
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
                check_gerber_deadline(self.deadline, "step-repeat-close")?;
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
            let mut feature_ids = Vec::with_capacity(self.features.len() - repeat.feature_start);
            let mut base_bounds = GerberBounds::default();
            for feature in &self.features[repeat.feature_start..] {
                check_gerber_deadline(self.deadline, "step-repeat-close")?;
                feature_ids.push(feature.id.clone());
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
            ArcDirection::Clockwise => cross < 0,
            ArcDirection::CounterClockwise => cross > 0,
        }
}

fn valid_single_quadrant_arc(
    start: CanonicalPoint,
    end: CanonicalPoint,
    center: CanonicalPoint,
    direction: ArcDirection,
    resolution: Picometres,
) -> bool {
    if start == end || resolution.0 <= 0 {
        return false;
    }
    let radius_squared = |point: CanonicalPoint| {
        let x = i128::from(point.x.0) - i128::from(center.x.0);
        let y = i128::from(point.y.0) - i128::from(center.y.0);
        (x * x + y * y) as u128
    };
    let start_radius = integer_sqrt(radius_squared(start));
    let end_radius = integer_sqrt(radius_squared(end));
    start_radius > 0
        && end_radius > 0
        && start_radius.abs_diff(end_radius) <= resolution.0 as u128
        && single_quadrant_sweep(start, end, center, direction)
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
        let membership = self
            .block
            .as_ref()
            .map_or(FeatureMembership::TopLevel, |block| {
                let location = &self.provenance(&block.start).location;
                FeatureMembership::ApertureBlock {
                    block_id: record_id("block", &self.document_id, location),
                    aperture_id: aperture_id(&self.document_id, ApertureShape::Block, location),
                }
            });
        let id = feature_id_with_membership(
            &self.document_id,
            &self.layer_id,
            geometry.kind(),
            &provenance.location,
            &membership,
        );
        let feature = ManufacturingFeature {
            id,
            document_id: self.document_id.clone(),
            layer_id: self.layer_id.clone(),
            tool_id,
            polarity,
            geometry,
            transforms,
            membership,
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

#[derive(Clone, Debug)]
struct X2Attribute {
    name: String,
    values: Vec<String>,
    provenance: ManufacturingProvenance,
}

fn scoped_x2_attribute_id(attribute: &ScopedX2Attribute) -> String {
    stable_id(
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
    )
    .expect("X2 attribute identity serializes")
}

fn scoped_x2_attribute_id_with_deadline(
    attribute: &ScopedX2Attribute,
    deadline: ManufacturingDeadline,
) -> Result<String, FabricationError> {
    stable_id_with_deadline(
        deadline,
        "x2-attribute-identity",
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
    )
}

fn scoped_x2_attribute(
    document_id: &str,
    scope: X2AttributeScope,
    kind: X2AttributeKind,
    values: Vec<String>,
    deletion: bool,
    provenance: ManufacturingProvenance,
) -> ScopedX2Attribute {
    let mut attribute = ScopedX2Attribute {
        id: String::new(),
        document_id: document_id.into(),
        scope,
        kind,
        values,
        deletion,
        target_ids: Vec::new(),
        provenance,
    };
    attribute.id = scoped_x2_attribute_id(&attribute);
    attribute
}

fn x2_attribute(evidence: &GerberAttributeEvidence) -> Result<X2Attribute, FabricationError> {
    let body = match evidence.kind {
        GerberAttributeKind::StandardComment => evidence
            .raw
            .strip_prefix("G04 #@! ")
            .and_then(|value| value.strip_suffix('*')),
        _ => evidence
            .raw
            .strip_prefix('%')
            .and_then(|value| value.strip_suffix("*%")),
    }
    .ok_or_else(|| FabricationError::InvalidIdentity("x2-attribute-framing".into()))?;
    let mut fields = body.split(',');
    let name = fields.next().unwrap_or_default();
    if name.is_empty()
        || name.len() > MANUFACTURING_LIMITS.max_text_bytes
        || !name.is_ascii()
        || name.chars().any(char::is_control)
    {
        return Err(FabricationError::InvalidIdentity(
            "x2-attribute-name".into(),
        ));
    }
    let values = fields
        .map(|value| {
            if value.len() > MANUFACTURING_LIMITS.max_text_bytes
                || !value.is_ascii()
                || value.chars().any(char::is_control)
            {
                Err(FabricationError::InvalidIdentity(
                    "x2-attribute-value".into(),
                ))
            } else {
                Ok(value.to_owned())
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(X2Attribute {
        name: name.to_owned(),
        values,
        provenance: evidence.provenance.clone(),
    })
}

fn parse_layer_number(value: &str) -> Option<i32> {
    value
        .strip_prefix('L')?
        .parse::<i32>()
        .ok()
        .filter(|value| *value > 0)
}

fn parse_positive_layer(value: &str) -> Option<i32> {
    value.parse::<i32>().ok().filter(|value| *value > 0)
}

fn package_file_function(attribute: &X2Attribute) -> Result<PackageFileFunction, FabricationError> {
    if attribute.name != "TF.FileFunction"
        || attribute.values.is_empty()
        || attribute.values.iter().any(String::is_empty)
    {
        return Err(FabricationError::InvalidIdentity("x2-file-function".into()));
    }
    let fields = &attribute.values;
    let (role, side, order, plating, from_layer, to_layer, qualifier, operation) = match fields[0]
        .as_str()
    {
        "Copper" if fields.len() == 3 => {
            let order = parse_layer_number(&fields[1])
                .ok_or_else(|| FabricationError::InvalidIdentity("x2-copper-layer".into()))?;
            let side = match fields[2].as_str() {
                "Top" => LayerSide::Top,
                "Bot" => LayerSide::Bottom,
                "Inr" => LayerSide::Inner,
                _ => {
                    return Err(FabricationError::InvalidIdentity("x2-copper-side".into()));
                }
            };
            (
                LayerRole::Copper,
                side,
                Some(order),
                Plating::Unknown,
                None,
                None,
                None,
                None,
            )
        }
        "Soldermask" | "Paste" | "Legend" if fields.len() == 2 => {
            let side = match fields[1].as_str() {
                "Top" => LayerSide::Top,
                "Bot" => LayerSide::Bottom,
                _ => {
                    return Err(FabricationError::InvalidIdentity("x2-layer-side".into()));
                }
            };
            let role = match fields[0].as_str() {
                "Soldermask" => LayerRole::SolderMask,
                "Paste" => LayerRole::Paste,
                _ => LayerRole::Legend,
            };
            (role, side, None, Plating::Unknown, None, None, None, None)
        }
        "Profile" if fields.len() == 2 && matches!(fields[1].as_str(), "P" | "NP") => (
            LayerRole::Profile,
            LayerSide::NotApplicable,
            None,
            Plating::Unknown,
            None,
            None,
            Some(fields[1].clone()),
            None,
        ),
        "Plated" | "NonPlated" if matches!(fields.len(), 4 | 5) => {
            let from = parse_positive_layer(&fields[1])
                .ok_or_else(|| FabricationError::InvalidIdentity("x2-file-function-span".into()))?;
            let to = parse_positive_layer(&fields[2])
                .ok_or_else(|| FabricationError::InvalidIdentity("x2-file-function-span".into()))?;
            if from > to {
                return Err(FabricationError::InvalidIdentity(
                    "x2-file-function-span".into(),
                ));
            }
            let valid_qualifier = match fields[0].as_str() {
                "Plated" => matches!(fields[3].as_str(), "PTH" | "Blind" | "Buried"),
                "NonPlated" => fields[3] == "NPTH",
                _ => false,
            };
            if !valid_qualifier {
                return Err(FabricationError::InvalidIdentity(
                    "x2-drill-plating-function".into(),
                ));
            }
            let role = match fields.get(4).map(String::as_str) {
                None | Some("Drill") => LayerRole::DrillMap,
                Some("Route") => LayerRole::Route,
                _ => {
                    return Err(FabricationError::InvalidIdentity(
                        "x2-drill-route-function".into(),
                    ));
                }
            };
            (
                role,
                LayerSide::NotApplicable,
                None,
                if fields[0] == "Plated" {
                    Plating::Plated
                } else {
                    Plating::NonPlated
                },
                Some(from),
                Some(to),
                Some(fields[3].clone()),
                Some(match fields.get(4) {
                    Some(operation) => format!("{},{}", fields[3], operation),
                    None => fields[3].clone(),
                }),
            )
        }
        _ => {
            return Err(FabricationError::InvalidIdentity(
                "unsupported-x2-file-function".into(),
            ));
        }
    };
    Ok(PackageFileFunction {
        raw: format!("TF.FileFunction,{}", fields.join(",")),
        role,
        side,
        order,
        plating,
        from_layer,
        to_layer,
        qualifier,
        operation,
        provenance: attribute.provenance.clone(),
    })
}

fn set_capability(review: &mut FabricationReview, record: CapabilityRecord) {
    review
        .capabilities
        .records
        .retain(|existing| existing.id != record.id);
    review.capabilities.records.push(record);
    review.capabilities.records.sort_by_key(|item| item.id);
}

fn semantic_capability(
    id: CapabilityId,
    state: CapabilityState,
    authority: Authority,
    document_id: &str,
    provenance: Option<&ManufacturingProvenance>,
    detail: &str,
) -> CapabilityRecord {
    CapabilityRecord {
        id,
        state,
        authority,
        document_ids: if state == CapabilityState::NotProvided {
            Vec::new()
        } else {
            vec![document_id.to_owned()]
        },
        provenance: provenance.into_iter().cloned().collect(),
        detail: detail.into(),
    }
}

fn rekey_document_layer(
    review: &mut FabricationReview,
    document_id: &str,
    function: &PackageFileFunction,
    authority: Authority,
    deadline: ManufacturingDeadline,
) -> Result<(), FabricationError> {
    deadline.check("document-layer-rekey")?;
    let layer_index = review
        .layers
        .iter()
        .position(|layer| layer.document_id == document_id && layer.role != LayerRole::Copper)
        .or_else(|| {
            review
                .layers
                .iter()
                .position(|layer| layer.document_id == document_id)
        })
        .ok_or_else(|| FabricationError::DanglingReference("document-layer".into()))?;
    let layer = &mut review.layers[layer_index];
    let old_layer_id = layer.id.clone();
    layer.role = function.role;
    layer.side = function.side;
    layer.order = function.order;
    layer.authority = authority;
    layer.provenance = function.provenance.clone();
    layer.id = layer_id(
        &layer.document_id,
        layer.name.as_deref(),
        layer.role,
        layer.side,
        layer.order,
        layer.authority,
        &layer.provenance.location,
    );
    let new_layer_id = layer.id.clone();
    let mut feature_ids = BTreeMap::new();
    for feature in &mut review.features {
        deadline.check("document-layer-rekey")?;
        if feature.layer_id != old_layer_id {
            continue;
        }
        let old_id = feature.id.clone();
        feature.layer_id = new_layer_id.clone();
        feature.id = feature_id_with_membership(
            &feature.document_id,
            &feature.layer_id,
            feature.geometry.kind(),
            &feature.provenance.location,
            &feature.membership,
        );
        feature_ids.insert(old_id, feature.id.clone());
    }
    let replace = |id: &mut String| {
        if let Some(replacement) = feature_ids.get(id) {
            *id = replacement.clone();
        }
    };
    for block in &mut review.blocks {
        deadline.check("document-layer-rekey")?;
        for id in &mut block.feature_ids {
            replace(id);
        }
        for id in &mut block.instantiation_feature_ids {
            replace(id);
        }
    }
    for repeat in &mut review.repetitions {
        deadline.check("document-layer-rekey")?;
        for id in &mut repeat.feature_ids {
            replace(id);
        }
    }
    for semantic in &mut review.connectivity {
        deadline.check("document-layer-rekey")?;
        replace(&mut semantic.feature_id);
    }
    for attribute in &mut review.x2_attributes {
        deadline.check("document-layer-rekey")?;
        for target in &mut attribute.target_ids {
            replace(target);
        }
        attribute.id = scoped_x2_attribute_id_with_deadline(attribute, deadline)?;
    }
    if let Some(profile) = &mut review.profile {
        for id in profile
            .contour_feature_ids
            .iter_mut()
            .chain(profile.cutout_feature_ids.iter_mut())
        {
            replace(id);
        }
    }
    for id in review
        .assembly
        .mask_layer_ids
        .iter_mut()
        .chain(review.assembly.paste_layer_ids.iter_mut())
    {
        if *id == old_layer_id {
            *id = new_layer_id.clone();
        }
    }
    for construction in &mut review.construction.layers {
        if construction.layer_id.as_deref() == Some(old_layer_id.as_str()) {
            construction.layer_id = Some(new_layer_id.clone());
        }
    }
    Ok(())
}

enum X2TimelineItem<'a> {
    Attribute(&'a X2Attribute),
    Aperture(&'a ApertureDefinition),
    Feature(&'a ManufacturingFeature),
}

struct X2ScopeAnalysis {
    records: Vec<ScopedX2Attribute>,
    connectivity: Vec<ObjectSemantics>,
    aperture_any: bool,
    aperture_complete: bool,
    object_any: bool,
    net_complete: bool,
    component_complete: bool,
    pin_complete: bool,
    file_supported: bool,
    unsupported: Vec<(X2AttributeScope, ManufacturingProvenance)>,
    component_conflict: Option<(
        String,
        ManufacturingProvenance,
        String,
        ManufacturingProvenance,
    )>,
}

fn analyze_x2_scopes<'a>(
    document_id: &str,
    attributes: &'a [X2Attribute],
    apertures: &'a [ApertureDefinition],
    features: &'a [ManufacturingFeature],
    deadline: ManufacturingDeadline,
) -> Result<X2ScopeAnalysis, FabricationError> {
    let mut timeline = BTreeMap::new();
    for (index, attribute) in attributes.iter().enumerate() {
        deadline.check("x2-timeline")?;
        timeline.insert(
            (attribute.provenance.location.record, 0_u8, index),
            X2TimelineItem::Attribute(attribute),
        );
    }
    for (index, aperture) in apertures.iter().enumerate() {
        deadline.check("x2-timeline")?;
        timeline.insert(
            (aperture.provenance.location.record, 1_u8, index),
            X2TimelineItem::Aperture(aperture),
        );
    }
    for (index, feature) in features.iter().enumerate() {
        deadline.check("x2-timeline")?;
        timeline.insert(
            (feature.provenance.location.record, 2_u8, index),
            X2TimelineItem::Feature(feature),
        );
    }

    let mut records = Vec::<ScopedX2Attribute>::new();
    let mut connectivity = Vec::new();
    let mut aperture_active = None::<usize>;
    let mut object_active = BTreeMap::<X2AttributeKind, usize>::new();
    let mut aperture_any = false;
    let mut aperture_supported = true;
    let mut aperture_count = 0_usize;
    let mut aperture_covered = 0_usize;
    let mut object_any = false;
    let mut object_supported = true;
    let mut feature_count = 0_usize;
    let mut net_covered = 0_usize;
    let mut component_covered = 0_usize;
    let mut pin_covered = 0_usize;
    let mut file_supported = true;
    let mut unsupported = Vec::new();
    let mut component_conflict = None;

    for (_, item) in timeline {
        deadline.check("x2-timeline")?;
        match item {
            X2TimelineItem::Attribute(attribute) => {
                let valid_values = |count: usize| {
                    attribute.values.len() == count
                        && attribute.values.iter().all(|value| !value.is_empty())
                };
                let push_record =
                    |records: &mut Vec<ScopedX2Attribute>, scope, kind, values, deletion| {
                        records.push(scoped_x2_attribute(
                            document_id,
                            scope,
                            kind,
                            values,
                            deletion,
                            attribute.provenance.clone(),
                        ));
                        records.len() - 1
                    };
                match attribute.name.as_str() {
                    "TF.FileFunction"
                        if !attribute.values.is_empty()
                            && attribute.values.iter().all(|value| !value.is_empty()) =>
                    {
                        let index = push_record(
                            &mut records,
                            X2AttributeScope::File,
                            X2AttributeKind::FileFunction,
                            attribute.values.clone(),
                            false,
                        );
                        records[index].target_ids.push(document_id.into());
                    }
                    name if name.starts_with("TF") => {
                        file_supported = false;
                        unsupported.push((X2AttributeScope::File, attribute.provenance.clone()));
                    }
                    "TA.AperFunction"
                        if !attribute.values.is_empty()
                            && attribute.values.iter().all(|value| !value.is_empty()) =>
                    {
                        aperture_any = true;
                        aperture_active = Some(push_record(
                            &mut records,
                            X2AttributeScope::Aperture,
                            X2AttributeKind::ApertureFunction,
                            attribute.values.clone(),
                            false,
                        ));
                    }
                    "TO.N" if valid_values(1) => {
                        object_any = true;
                        let index = push_record(
                            &mut records,
                            X2AttributeScope::Object,
                            X2AttributeKind::Net,
                            attribute.values.clone(),
                            false,
                        );
                        object_active.insert(X2AttributeKind::Net, index);
                    }
                    "TO.C" if valid_values(1) => {
                        object_any = true;
                        if let Some(previous) = object_active
                            .get(&X2AttributeKind::Component)
                            .and_then(|index| records.get(*index))
                            .filter(|previous| previous.values[0] != attribute.values[0])
                        {
                            object_supported = false;
                            component_conflict.get_or_insert_with(|| {
                                (
                                    previous.values[0].clone(),
                                    previous.provenance.clone(),
                                    attribute.values[0].clone(),
                                    attribute.provenance.clone(),
                                )
                            });
                        }
                        let index = push_record(
                            &mut records,
                            X2AttributeScope::Object,
                            X2AttributeKind::Component,
                            attribute.values.clone(),
                            false,
                        );
                        object_active.insert(X2AttributeKind::Component, index);
                    }
                    "TO.P" if valid_values(2) => {
                        object_any = true;
                        if let Some(previous) = object_active
                            .get(&X2AttributeKind::Component)
                            .and_then(|index| records.get(*index))
                            .filter(|previous| previous.values[0] != attribute.values[0])
                        {
                            object_supported = false;
                            component_conflict.get_or_insert_with(|| {
                                (
                                    previous.values[0].clone(),
                                    previous.provenance.clone(),
                                    attribute.values[0].clone(),
                                    attribute.provenance.clone(),
                                )
                            });
                        }
                        let component = push_record(
                            &mut records,
                            X2AttributeScope::Object,
                            X2AttributeKind::Component,
                            vec![attribute.values[0].clone()],
                            false,
                        );
                        let pin = push_record(
                            &mut records,
                            X2AttributeScope::Object,
                            X2AttributeKind::Pin,
                            vec![attribute.values[1].clone()],
                            false,
                        );
                        object_active.insert(X2AttributeKind::Component, component);
                        object_active.insert(X2AttributeKind::Pin, pin);
                    }
                    "TD" if attribute.values.is_empty() => {
                        aperture_any = true;
                        object_any = true;
                        push_record(
                            &mut records,
                            X2AttributeScope::Aperture,
                            X2AttributeKind::Reset,
                            Vec::new(),
                            true,
                        );
                        push_record(
                            &mut records,
                            X2AttributeScope::Object,
                            X2AttributeKind::Reset,
                            Vec::new(),
                            true,
                        );
                        aperture_active = None;
                        object_active.clear();
                    }
                    "TD.AperFunction" if attribute.values.is_empty() => {
                        aperture_any = true;
                        push_record(
                            &mut records,
                            X2AttributeScope::Aperture,
                            X2AttributeKind::ApertureFunction,
                            Vec::new(),
                            true,
                        );
                        aperture_active = None;
                    }
                    "TD.N" | "TD.C" | "TD.P" if attribute.values.is_empty() => {
                        object_any = true;
                        let kind = match attribute.name.as_str() {
                            "TD.N" => X2AttributeKind::Net,
                            "TD.C" => X2AttributeKind::Component,
                            _ => X2AttributeKind::Pin,
                        };
                        push_record(
                            &mut records,
                            X2AttributeScope::Object,
                            kind,
                            Vec::new(),
                            true,
                        );
                        object_active.remove(&kind);
                    }
                    name if name.starts_with("TA") => {
                        aperture_any = true;
                        aperture_supported = false;
                        unsupported
                            .push((X2AttributeScope::Aperture, attribute.provenance.clone()));
                    }
                    name if name.starts_with("TO") => {
                        object_any = true;
                        object_supported = false;
                        unsupported.push((X2AttributeScope::Object, attribute.provenance.clone()));
                    }
                    name if name.starts_with("TD") => {
                        aperture_any = true;
                        object_any = true;
                        aperture_supported = false;
                        object_supported = false;
                        unsupported
                            .push((X2AttributeScope::Aperture, attribute.provenance.clone()));
                        unsupported.push((X2AttributeScope::Object, attribute.provenance.clone()));
                    }
                    _ => {}
                }
            }
            X2TimelineItem::Aperture(aperture) => {
                aperture_count += 1;
                if let Some(index) = aperture_active {
                    records[index].target_ids.push(aperture.id.clone());
                    aperture_covered += 1;
                }
            }
            X2TimelineItem::Feature(feature) => {
                feature_count += 1;
                let value = |kind| {
                    object_active
                        .get(&kind)
                        .and_then(|index| records.get(*index))
                        .map(|record| record.values[0].clone())
                };
                let net = value(X2AttributeKind::Net);
                let component = value(X2AttributeKind::Component);
                let pin = value(X2AttributeKind::Pin);
                net_covered += usize::from(net.is_some());
                component_covered += usize::from(component.is_some());
                pin_covered += usize::from(pin.is_some());
                for index in object_active.values() {
                    records[*index].target_ids.push(feature.id.clone());
                }
                if !object_active.is_empty() {
                    let provenance = object_active
                        .values()
                        .filter_map(|index| records.get(*index))
                        .max_by_key(|record| record.provenance.location.record)
                        .expect("nonempty X2 object state")
                        .provenance
                        .clone();
                    connectivity.push(ObjectSemantics {
                        feature_id: feature.id.clone(),
                        net,
                        component,
                        pin,
                        provenance,
                    });
                }
            }
        }
    }

    Ok(X2ScopeAnalysis {
        records,
        connectivity,
        aperture_any,
        aperture_complete: aperture_supported
            && aperture_count > 0
            && aperture_count == aperture_covered,
        object_any,
        net_complete: object_supported && feature_count > 0 && net_covered == feature_count,
        component_complete: object_supported
            && feature_count > 0
            && component_covered == feature_count,
        pin_complete: object_supported && feature_count > 0 && pin_covered == feature_count,
        file_supported,
        unsupported,
        component_conflict,
    })
}

pub fn apply_gerber_x2(production: &mut GerberProduction) -> Result<(), FabricationError> {
    apply_gerber_x2_with_deadline(
        production,
        ManufacturingDeadline::from_timeout(Duration::from_millis(
            MANUFACTURING_LIMITS.aggregate_timeout_ms,
        )),
    )
}

fn apply_gerber_x2_with_deadline(
    production: &mut GerberProduction,
    deadline: ManufacturingDeadline,
) -> Result<(), FabricationError> {
    deadline.check("x2-analysis")?;
    let mut attributes = Vec::with_capacity(production.attributes.len());
    for attribute in &production.attributes {
        deadline.check("x2-attribute-index")?;
        attributes.push(x2_attribute(attribute)?);
    }
    let mut file_functions = Vec::new();
    for attribute in &attributes {
        deadline.check("x2-file-functions")?;
        if attribute.name == "TF.FileFunction" {
            file_functions.push(package_file_function(attribute)?);
        }
    }
    let first_provenance = production
        .review
        .documents
        .first()
        .map(inventory_provenance);
    let document_id = production
        .review
        .documents
        .first()
        .map(|document| document.id.clone())
        .ok_or_else(|| FabricationError::DanglingReference("gerber-document".into()))?;
    checked_retain_with_deadline(
        &mut production.review.omissions,
        deadline,
        "x2-omission-retention",
        |omission| {
            !omission
                .affected_capabilities
                .contains(&CapabilityId::X2FileAttributes)
        },
    )?;
    if file_functions.len() == 1 {
        let function = file_functions[0].clone();
        rekey_document_layer(
            &mut production.review,
            &document_id,
            &function,
            Authority::X2,
            deadline,
        )?;
        let state = CapabilityState::Complete;
        set_capability(
            &mut production.review,
            semantic_capability(
                CapabilityId::X2FileAttributes,
                state,
                Authority::X2,
                &document_id,
                Some(&function.provenance),
                "One typed FileFunction establishes this document role only without unsupported file attributes.",
            ),
        );
        set_capability(
            &mut production.review,
            semantic_capability(
                CapabilityId::LayerRoles,
                state,
                Authority::X2,
                &document_id,
                Some(&function.provenance),
                "The document role is explicitly supplied by X2 FileFunction.",
            ),
        );
        production.file_function = Some(function);
    } else {
        let state = if file_functions.is_empty() {
            CapabilityState::NotProvided
        } else {
            CapabilityState::Partial
        };
        let provenance = file_functions
            .first()
            .map(|function| &function.provenance)
            .or(first_provenance.as_ref());
        for id in [CapabilityId::X2FileAttributes, CapabilityId::LayerRoles] {
            set_capability(
                &mut production.review,
                semantic_capability(
                    id,
                    state,
                    Authority::X2,
                    &document_id,
                    provenance,
                    "FileFunction is absent, duplicated, or conflicting and cannot establish authority.",
                ),
            );
        }
        if let Some(first) = file_functions.first() {
            for duplicate in file_functions.iter().skip(1) {
                deadline.check("x2-file-functions")?;
                if same_function(first, duplicate) {
                    production.review.omissions.push(Omission {
                        id: stable_id(
                            "omission",
                            &(
                                &document_id,
                                "duplicate-x2-file-function",
                                &first.provenance.location,
                                &duplicate.provenance.location,
                            ),
                        )?,
                        kind: OmissionKind::InvalidRecord,
                        affected_capabilities: vec![
                            CapabilityId::X2FileAttributes,
                            CapabilityId::LayerRoles,
                        ],
                        provenance: duplicate.provenance.clone(),
                        detail: "Duplicate typed FileFunction records cannot establish document role authority."
                            .into(),
                    });
                } else {
                    production.review.conflicts.push(Conflict {
                        id: stable_id(
                            "conflict",
                            &(
                                &document_id,
                                "conflicting-x2-file-function",
                                &first.provenance.location,
                                &duplicate.provenance.location,
                            ),
                        )?,
                        kind: ConflictKind::LayerRole,
                        affected_capabilities: vec![
                            CapabilityId::X2FileAttributes,
                            CapabilityId::LayerRoles,
                        ],
                        left: ConflictFact {
                            canonical_value: first.raw.clone(),
                            authority: Authority::X2,
                            provenance: first.provenance.clone(),
                        },
                        right: ConflictFact {
                            canonical_value: duplicate.raw.clone(),
                            authority: Authority::X2,
                            provenance: duplicate.provenance.clone(),
                        },
                    });
                }
            }
        }
    }

    let analysis = analyze_x2_scopes(
        &document_id,
        &attributes,
        &production.review.apertures,
        &production.review.features,
        deadline,
    )?;
    if !analysis.file_supported {
        let provenance = file_functions
            .first()
            .map(|function| &function.provenance)
            .or(first_provenance.as_ref());
        for id in [CapabilityId::X2FileAttributes, CapabilityId::LayerRoles] {
            set_capability(
                &mut production.review,
                semantic_capability(
                    id,
                    CapabilityState::Partial,
                    Authority::X2,
                    &document_id,
                    provenance,
                    "Unsupported or malformed X2 file attributes prevent complete role authority.",
                ),
            );
        }
    }
    let mut aperture_provenance = None;
    let mut object_provenance = None;
    for attribute in &attributes {
        deadline.check("x2-attribute-provenance")?;
        if aperture_provenance.is_none()
            && (attribute.name.starts_with("TA") || attribute.name.starts_with("TD"))
        {
            aperture_provenance = Some(&attribute.provenance);
        }
        if object_provenance.is_none()
            && (attribute.name.starts_with("TO") || attribute.name.starts_with("TD"))
        {
            object_provenance = Some(&attribute.provenance);
        }
        if aperture_provenance.is_some() && object_provenance.is_some() {
            break;
        }
    }
    let aperture_state = if !analysis.aperture_any {
        CapabilityState::NotProvided
    } else if analysis.aperture_complete {
        CapabilityState::Complete
    } else {
        CapabilityState::Partial
    };
    set_capability(
        &mut production.review,
        semantic_capability(
            CapabilityId::X2ApertureAttributes,
            aperture_state,
            Authority::X2,
            &document_id,
            aperture_provenance,
            "Aperture attributes are complete only when ordered nonempty scope covers every aperture after resets and deletions.",
        ),
    );

    production.review.connectivity = analysis.connectivity;
    let object_state = |complete| {
        if !analysis.object_any {
            CapabilityState::NotProvided
        } else if complete {
            CapabilityState::Complete
        } else {
            CapabilityState::Partial
        }
    };
    for (id, complete, detail) in [
        (
            CapabilityId::Connectivity,
            analysis.net_complete,
            "Every eligible feature must have an explicit scoped nonempty net attribute.",
        ),
        (
            CapabilityId::Components,
            analysis.component_complete,
            "Every eligible feature must have an explicit consistent scoped nonempty component attribute.",
        ),
        (
            CapabilityId::Pins,
            analysis.pin_complete,
            "Every eligible feature must have an explicit scoped nonempty pin attribute.",
        ),
    ] {
        set_capability(
            &mut production.review,
            semantic_capability(
                id,
                object_state(complete),
                Authority::X2,
                &document_id,
                object_provenance,
                detail,
            ),
        );
    }
    set_capability(
        &mut production.review,
        semantic_capability(
            CapabilityId::X2ObjectAttributes,
            object_state(
                analysis.net_complete && analysis.component_complete && analysis.pin_complete,
            ),
            Authority::X2,
            &document_id,
            object_provenance,
            "Object attributes are complete only with ordered, nonempty, conflict-free net/component/pin coverage after resets and deletions.",
        ),
    );

    if let Some((left, left_provenance, right, right_provenance)) = analysis.component_conflict {
        production.review.conflicts.push(Conflict {
            id: stable_id(
                "conflict",
                &(
                    &document_id,
                    "x2-component-scope",
                    &left_provenance.location,
                    &right_provenance.location,
                ),
            )?,
            kind: ConflictKind::Connectivity,
            affected_capabilities: vec![CapabilityId::X2ObjectAttributes, CapabilityId::Components],
            left: ConflictFact {
                canonical_value: left,
                authority: Authority::X2,
                provenance: left_provenance,
            },
            right: ConflictFact {
                canonical_value: right,
                authority: Authority::X2,
                provenance: right_provenance,
            },
        });
    }
    for (scope, provenance) in analysis.unsupported {
        let capability = match scope {
            X2AttributeScope::File => CapabilityId::X2FileAttributes,
            X2AttributeScope::Aperture => CapabilityId::X2ApertureAttributes,
            X2AttributeScope::Object => CapabilityId::X2ObjectAttributes,
        };
        production.review.omissions.push(Omission {
            id: stable_id(
                "omission",
                &("x2-scoped-attribute", scope, &provenance.location),
            )?,
            kind: OmissionKind::UnsupportedRecord,
            affected_capabilities: vec![capability],
            provenance,
            detail:
                "Unsupported, empty, or malformed scoped X2 attribute prevents complete coverage."
                    .into(),
        });
    }
    production.review.x2_attributes = analysis.records;
    for attribute in &mut production.review.x2_attributes {
        deadline.check("x2-attribute-identity")?;
        attribute.id = scoped_x2_attribute_id_with_deadline(attribute, deadline)?;
    }
    production.review.physical_bounds =
        derive_release_physical_bounds(&production.review, ReconciliationBudget { deadline })?;
    production.review.refresh_digests_with_deadline(deadline)?;
    production.review.validate_with_deadline(deadline)
}

pub const XNC_ADAPTER_VERSION: &str = "xnc-2021.11-ratemypcb-1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XncDialect {
    Strict,
    KicadLegacy,
    LibrePcbLegacy,
}

#[derive(Debug)]
pub enum XncParseError {
    Resource {
        resource: &'static str,
        observed: u64,
        limit: u64,
    },
    Invalid {
        record: u64,
        reason: &'static str,
    },
    Unsupported {
        record: u64,
        command: String,
    },
    Deadline {
        stage: &'static str,
    },
    Canonical(FabricationError),
}

impl std::fmt::Display for XncParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for XncParseError {}

#[derive(Clone, Debug)]
pub struct XncProduction {
    pub review: FabricationReview,
    pub dialect: XncDialect,
    pub file_function: Option<PackageFileFunction>,
    pub extents: Option<Extent>,
}

#[derive(Clone, Debug)]
struct XncLine {
    record: u64,
    byte_start: usize,
    byte_end: usize,
    text: String,
}

#[derive(Clone)]
struct XncToolDefinition {
    code: u16,
    source_code: String,
    diameter: Picometres,
    provenance: ManufacturingProvenance,
}

fn xnc_error(record: u64, reason: &'static str) -> XncParseError {
    XncParseError::Invalid { record, reason }
}

fn check_xnc_deadline(
    deadline: ManufacturingDeadline,
    stage: &'static str,
) -> Result<(), XncParseError> {
    if Instant::now() >= deadline.at {
        Err(XncParseError::Deadline { stage })
    } else {
        Ok(())
    }
}

fn xnc_lines(
    bytes: &[u8],
    deadline: ManufacturingDeadline,
) -> Result<(Vec<XncLine>, u64, usize, u64, usize), XncParseError> {
    if bytes.len() as u64 > MANUFACTURING_LIMITS.raw_bytes_per_file {
        return Err(XncParseError::Resource {
            resource: "raw-bytes",
            observed: bytes.len() as u64,
            limit: MANUFACTURING_LIMITS.raw_bytes_per_file,
        });
    }
    check_xnc_deadline(deadline, "xnc-framing")?;
    let mut lines = Vec::new();
    let mut tokens = 0_u64;
    let mut max_line = 0_usize;
    let mut metadata_bytes = 0_u64;
    let mut max_text_bytes = 0_usize;
    let mut start = 0_usize;
    while start < bytes.len() {
        if start % 4_096 == 0 {
            check_xnc_deadline(deadline, "xnc-framing")?;
        }
        let newline = bytes[start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |offset| start + offset);
        let mut end = newline;
        if end > start && bytes[end - 1] == b'\r' {
            end -= 1;
        }
        let line_bytes = &bytes[start..end];
        max_line = max_line.max(line_bytes.len());
        if line_bytes.len() > MANUFACTURING_LIMITS.max_line_bytes {
            return Err(XncParseError::Resource {
                resource: "line-bytes",
                observed: line_bytes.len() as u64,
                limit: MANUFACTURING_LIMITS.max_line_bytes as u64,
            });
        }
        if line_bytes
            .iter()
            .any(|byte| !byte.is_ascii() || (*byte < b' ' && *byte != b'\t') || *byte == 0x7f)
        {
            return Err(xnc_error(lines.len() as u64, "invalid-byte"));
        }
        let text = std::str::from_utf8(line_bytes)
            .map_err(|_| xnc_error(lines.len() as u64, "invalid-utf8"))?
            .trim()
            .to_owned();
        if !text.is_empty() {
            if text.starts_with(';') {
                metadata_bytes = metadata_bytes.checked_add(text.len() as u64).ok_or(
                    XncParseError::Resource {
                        resource: "metadata-bytes",
                        observed: u64::MAX,
                        limit: MANUFACTURING_LIMITS.metadata_bytes_per_file,
                    },
                )?;
                max_text_bytes = max_text_bytes.max(text.len());
                if metadata_bytes > MANUFACTURING_LIMITS.metadata_bytes_per_file
                    || text.len() > MANUFACTURING_LIMITS.max_text_bytes
                {
                    return Err(XncParseError::Resource {
                        resource: "metadata-bytes",
                        observed: metadata_bytes,
                        limit: MANUFACTURING_LIMITS.metadata_bytes_per_file,
                    });
                }
            }
            tokens = tokens
                .checked_add(
                    text.as_bytes()
                        .windows(2)
                        .filter(|pair| {
                            pair[0].is_ascii_alphanumeric() != pair[1].is_ascii_alphanumeric()
                        })
                        .count() as u64
                        + 1,
                )
                .ok_or(XncParseError::Resource {
                    resource: "lexical-tokens",
                    observed: u64::MAX,
                    limit: MANUFACTURING_LIMITS.lexical_tokens_per_file,
                })?;
            if tokens > MANUFACTURING_LIMITS.lexical_tokens_per_file {
                return Err(XncParseError::Resource {
                    resource: "lexical-tokens",
                    observed: tokens,
                    limit: MANUFACTURING_LIMITS.lexical_tokens_per_file,
                });
            }
            if lines.len() as u64 >= MANUFACTURING_LIMITS.records_per_file {
                return Err(XncParseError::Resource {
                    resource: "records",
                    observed: lines.len() as u64 + 1,
                    limit: MANUFACTURING_LIMITS.records_per_file,
                });
            }
            lines.push(XncLine {
                record: lines.len() as u64,
                byte_start: start,
                byte_end: end.saturating_sub(1),
                text,
            });
        }
        start = newline.saturating_add(1);
    }
    Ok((lines, tokens, max_line, metadata_bytes, max_text_bytes))
}

fn xnc_provenance(document_id: &str, digest: &str, line: &XncLine) -> ManufacturingProvenance {
    ManufacturingProvenance {
        document_id: document_id.into(),
        artifact_digest: digest.into(),
        producer: "ratemypcb-xnc".into(),
        producer_version: XNC_ADAPTER_VERSION.into(),
        location: StructuralLocation {
            record: line.record,
            subrecord: None,
            byte_start: line.byte_start as u64,
            byte_end: line.byte_end as u64,
        },
        source_lexeme: None,
    }
}

fn xnc_attribute(line: &XncLine, document_id: &str, digest: &str) -> Option<X2Attribute> {
    let body = line.text.strip_prefix("; #@! ")?;
    let mut fields = body.split(',');
    Some(X2Attribute {
        name: fields.next()?.to_owned(),
        values: fields.map(str::to_owned).collect(),
        provenance: xnc_provenance(document_id, digest, line),
    })
}

fn xnc_dialect(
    lines: &[XncLine],
    deadline: ManufacturingDeadline,
) -> Result<XncDialect, XncParseError> {
    let mut generation = None;
    for line in lines {
        check_xnc_deadline(deadline, "xnc-dialect")?;
        let Some(value) = line.text.strip_prefix("; #@! TF.GenerationSoftware,") else {
            continue;
        };
        if generation.replace((line, value)).is_some() {
            return Err(xnc_error(line.record, "duplicate-generation-signature"));
        }
    }
    let Some((line, value)) = generation else {
        return Ok(XncDialect::Strict);
    };
    match value {
        "Kicad,Pcbnew,9.0" | "Kicad,Pcbnew,(5.99.0-10065-g0a0935e0f3-dirty)" => {
            Ok(XncDialect::KicadLegacy)
        }
        "LibrePCB,LibrePCB,1.0" | "LibrePCB,LibrePCB,0.2.0-unstable" => {
            Ok(XncDialect::LibrePcbLegacy)
        }
        _ if value == "xxxx,yyyy,zzzz"
            || value.starts_with("Kicad,")
            || value.starts_with("LibrePCB,") =>
        {
            Err(XncParseError::Unsupported {
                record: line.record,
                command: bounded_command(&line.text),
            })
        }
        _ => Ok(XncDialect::Strict),
    }
}

fn xnc_tool_code(value: &str, dialect: XncDialect) -> Option<u16> {
    let digits = value.strip_prefix('T')?;
    let valid_width = match dialect {
        XncDialect::Strict => digits.len() == 2,
        XncDialect::KicadLegacy | XncDialect::LibrePcbLegacy => (1..=2).contains(&digits.len()),
    };
    valid_width
        .then(|| digits.parse::<u16>().ok())
        .flatten()
        .filter(|code| (1..=MANUFACTURING_LIMITS.strict_tool_max).contains(code))
}

fn xnc_number_profile(value: &str) -> Option<(u8, u8)> {
    let unsigned = value.strip_prefix(['-', '+']).unwrap_or(value);
    let (integer, decimal) = unsigned.split_once('.')?;
    if integer.is_empty()
        || decimal.is_empty()
        || !integer.bytes().all(|byte| byte.is_ascii_digit())
        || !decimal.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some((
        u8::try_from(integer.len()).ok()?.max(1),
        u8::try_from(decimal.len()).ok()?,
    ))
}

fn xnc_length(value: &str, unit: SourceUnit, record: u64) -> Result<Picometres, XncParseError> {
    if value.len() > MANUFACTURING_LIMITS.max_numeric_bytes || xnc_number_profile(value).is_none() {
        return Err(xnc_error(record, "invalid-explicit-decimal"));
    }
    Picometres::parse_decimal(value, unit).map_err(XncParseError::Canonical)
}

fn xnc_fields(source: &str, record: u64) -> Result<BTreeMap<u8, &str>, XncParseError> {
    let bytes = source.as_bytes();
    let mut position = 0_usize;
    let mut fields = BTreeMap::new();
    while position < bytes.len() {
        let tag = bytes[position];
        if !tag.is_ascii_uppercase() {
            return Err(xnc_error(record, "invalid-coordinate-tag"));
        }
        position += 1;
        let start = position;
        while position < bytes.len() && !bytes[position].is_ascii_uppercase() {
            position += 1;
        }
        if start == position || fields.insert(tag, &source[start..position]).is_some() {
            return Err(xnc_error(record, "empty-or-duplicate-coordinate-field"));
        }
    }
    Ok(fields)
}

fn xnc_point(
    source: &str,
    unit: SourceUnit,
    record: u64,
) -> Result<(CanonicalPoint, u8, u8), XncParseError> {
    let fields = xnc_fields(source, record)?;
    if fields.len() != 2 || !fields.contains_key(&b'X') || !fields.contains_key(&b'Y') {
        return Err(xnc_error(record, "coordinates-require-x-and-y"));
    }
    let x = fields[&b'X'];
    let y = fields[&b'Y'];
    let (xi, xd) = xnc_number_profile(x).ok_or_else(|| xnc_error(record, "invalid-x"))?;
    let (yi, yd) = xnc_number_profile(y).ok_or_else(|| xnc_error(record, "invalid-y"))?;
    Ok((
        CanonicalPoint {
            x: xnc_length(x, unit, record)?,
            y: xnc_length(y, unit, record)?,
        },
        xi.max(yi),
        xd.max(yd),
    ))
}

fn integer_sqrt(value: u128) -> u128 {
    if value < 2 {
        return value;
    }
    let mut current = 1_u128 << (value.ilog2() / 2 + 1);
    loop {
        let next = (current + value / current) / 2;
        if next >= current {
            return current;
        }
        current = next;
    }
}

fn xnc_radius_center(
    start: CanonicalPoint,
    end: CanonicalPoint,
    radius: Picometres,
    direction: ArcDirection,
    record: u64,
) -> Result<CanonicalPoint, XncParseError> {
    if radius.0 <= 0 {
        return Err(xnc_error(record, "non-positive-arc-radius"));
    }
    let dx = i128::from(end.x.0) - i128::from(start.x.0);
    let dy = i128::from(end.y.0) - i128::from(start.y.0);
    let chord_squared = dx
        .checked_mul(dx)
        .and_then(|x| dy.checked_mul(dy).and_then(|y| x.checked_add(y)))
        .ok_or_else(|| xnc_error(record, "arc-radius-overflow"))?;
    let radius = i128::from(radius.0);
    let discriminant = radius
        .checked_mul(radius)
        .and_then(|value| value.checked_mul(4))
        .and_then(|value| value.checked_sub(chord_squared))
        .filter(|value| *value >= 0)
        .ok_or_else(|| xnc_error(record, "arc-radius-too-small"))?;
    let chord = integer_sqrt(chord_squared as u128) as i128;
    if chord == 0 {
        return Err(xnc_error(record, "zero-length-arc"));
    }
    let height = integer_sqrt(discriminant as u128) as i128;
    let denominator = chord
        .checked_mul(2)
        .ok_or_else(|| xnc_error(record, "arc-center-overflow"))?;
    let mut candidates = BTreeSet::new();
    for sign in [-1_i128, 1] {
        let x_numerator = (i128::from(start.x.0) + i128::from(end.x.0))
            .checked_mul(chord)
            .and_then(|value| value.checked_add(sign * -dy * height))
            .ok_or_else(|| xnc_error(record, "arc-center-overflow"))?;
        let y_numerator = (i128::from(start.y.0) + i128::from(end.y.0))
            .checked_mul(chord)
            .and_then(|value| value.checked_add(sign * dx * height))
            .ok_or_else(|| xnc_error(record, "arc-center-overflow"))?;
        let center = CanonicalPoint::new(
            i64::try_from(rounded_div(x_numerator, denominator))
                .map_err(|_| xnc_error(record, "arc-center-overflow"))?,
            i64::try_from(rounded_div(y_numerator, denominator))
                .map_err(|_| xnc_error(record, "arc-center-overflow"))?,
        );
        if single_quadrant_sweep(start, end, center, direction) {
            candidates.insert(center);
        }
    }
    if candidates.len() != 1 {
        return Err(xnc_error(record, "ambiguous-radius-arc"));
    }
    Ok(*candidates.first().expect("one XNC arc center"))
}

fn xnc_feature(
    document_id: &str,
    layer_id: &str,
    tool_id: &str,
    geometry: Geometry,
    provenance: ManufacturingProvenance,
) -> ManufacturingFeature {
    ManufacturingFeature {
        id: feature_id(document_id, layer_id, geometry.kind(), &provenance.location),
        document_id: document_id.into(),
        layer_id: layer_id.into(),
        tool_id: Some(tool_id.into()),
        polarity: LayerPolarity::Unknown,
        geometry,
        transforms: TransformChain::default(),
        membership: FeatureMembership::TopLevel,
        provenance,
    }
}

fn xnc_tool_radius(width: Picometres, record: u64) -> Result<i64, XncParseError> {
    if width.0 <= 0 {
        return Err(xnc_error(record, "non-positive-tool-width"));
    }
    width
        .0
        .checked_add(1)
        .map(|value| value / 2)
        .ok_or_else(|| xnc_error(record, "tool-radius-overflow"))
}

fn xnc_include_physical_point(
    bounds: &mut GerberBounds,
    point: CanonicalPoint,
    padding: i64,
    record: u64,
) -> Result<(), XncParseError> {
    bounds
        .include_box(
            point
                .x
                .0
                .checked_sub(padding)
                .ok_or_else(|| xnc_error(record, "physical-extent-overflow"))?,
            point
                .y
                .0
                .checked_sub(padding)
                .ok_or_else(|| xnc_error(record, "physical-extent-overflow"))?,
            point
                .x
                .0
                .checked_add(padding)
                .ok_or_else(|| xnc_error(record, "physical-extent-overflow"))?,
            point
                .y
                .0
                .checked_add(padding)
                .ok_or_else(|| xnc_error(record, "physical-extent-overflow"))?,
        )
        .map_err(|_| xnc_error(record, "physical-extent-out-of-range"))
}

fn polar_half(vector: (i128, i128)) -> u8 {
    u8::from(vector.1 < 0 || vector.1 == 0 && vector.0 < 0)
}

fn polar_order(left: (i128, i128), right: (i128, i128)) -> Ordering {
    polar_half(left).cmp(&polar_half(right)).then_with(|| {
        let cross = left.0 * right.1 - left.1 * right.0;
        if cross > 0 {
            Ordering::Less
        } else if cross < 0 {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    })
}

fn counter_clockwise_sweep_contains(
    start: (i128, i128),
    end: (i128, i128),
    candidate: (i128, i128),
) -> bool {
    if polar_order(start, end) != Ordering::Greater {
        polar_order(start, candidate) != Ordering::Greater
            && polar_order(candidate, end) != Ordering::Greater
    } else {
        polar_order(start, candidate) != Ordering::Greater
            || polar_order(candidate, end) != Ordering::Greater
    }
}

fn xnc_arc_sweep_contains(arc: &CanonicalArc, candidate: (i128, i128)) -> bool {
    let start = (
        i128::from(arc.start.x.0) - i128::from(arc.center.x.0),
        i128::from(arc.start.y.0) - i128::from(arc.center.y.0),
    );
    let end = (
        i128::from(arc.end.x.0) - i128::from(arc.center.x.0),
        i128::from(arc.end.y.0) - i128::from(arc.center.y.0),
    );
    match arc.direction {
        ArcDirection::CounterClockwise => counter_clockwise_sweep_contains(start, end, candidate),
        ArcDirection::Clockwise => counter_clockwise_sweep_contains(end, start, candidate),
    }
}

fn xnc_arc_radius(arc: &CanonicalArc, record: u64) -> Result<i64, XncParseError> {
    let squared = |point: CanonicalPoint| -> Result<u128, XncParseError> {
        let x = i128::from(point.x.0) - i128::from(arc.center.x.0);
        let y = i128::from(point.y.0) - i128::from(arc.center.y.0);
        x.checked_mul(x)
            .and_then(|x| y.checked_mul(y).and_then(|y| x.checked_add(y)))
            .and_then(|value| u128::try_from(value).ok())
            .ok_or_else(|| xnc_error(record, "arc-radius-overflow"))
    };
    let squared = squared(arc.start)?.max(squared(arc.end)?);
    let floor = integer_sqrt(squared);
    let radius = floor
        .checked_add(u128::from(floor.checked_mul(floor) != Some(squared)))
        .and_then(|value| i64::try_from(value).ok())
        .ok_or_else(|| xnc_error(record, "arc-radius-overflow"))?;
    if radius <= 0 {
        return Err(xnc_error(record, "non-positive-arc-radius"));
    }
    Ok(radius)
}

fn xnc_segment_bounds(
    bounds: &mut GerberBounds,
    segment: &ContourSegment,
    record: u64,
    deadline: ManufacturingDeadline,
) -> Result<(), XncParseError> {
    check_xnc_deadline(deadline, "xnc-physical-bounds")?;
    match segment {
        ContourSegment::Line(line) => {
            let padding = xnc_tool_radius(
                line.width
                    .ok_or_else(|| xnc_error(record, "route-without-tool-width"))?,
                record,
            )?;
            xnc_include_physical_point(bounds, line.start, padding, record)?;
            xnc_include_physical_point(bounds, line.end, padding, record)
        }
        ContourSegment::Arc(arc) => {
            let padding = xnc_tool_radius(
                arc.width
                    .ok_or_else(|| xnc_error(record, "route-without-tool-width"))?,
                record,
            )?;
            xnc_include_physical_point(bounds, arc.start, padding, record)?;
            xnc_include_physical_point(bounds, arc.end, padding, record)?;
            let radius = xnc_arc_radius(arc, record)?;
            for (direction, vector) in [
                ((1_i64, 0_i64), (1_i128, 0_i128)),
                ((0, 1), (0, 1)),
                ((-1, 0), (-1, 0)),
                ((0, -1), (0, -1)),
            ] {
                if xnc_arc_sweep_contains(arc, vector) {
                    let point =
                        CanonicalPoint::new(
                            arc.center
                                .x
                                .0
                                .checked_add(
                                    direction.0.checked_mul(radius).ok_or_else(|| {
                                        xnc_error(record, "physical-extent-overflow")
                                    })?,
                                )
                                .ok_or_else(|| xnc_error(record, "physical-extent-overflow"))?,
                            arc.center
                                .y
                                .0
                                .checked_add(
                                    direction.1.checked_mul(radius).ok_or_else(|| {
                                        xnc_error(record, "physical-extent-overflow")
                                    })?,
                                )
                                .ok_or_else(|| xnc_error(record, "physical-extent-overflow"))?,
                        );
                    xnc_include_physical_point(bounds, point, padding, record)?;
                }
            }
            Ok(())
        }
    }
}

fn xnc_physical_bounds<'a>(
    features: impl IntoIterator<Item = &'a ManufacturingFeature>,
    deadline: ManufacturingDeadline,
) -> Result<GerberBounds, XncParseError> {
    let mut bounds = GerberBounds::default();
    for feature in features {
        check_xnc_deadline(deadline, "xnc-physical-bounds")?;
        let record = feature.provenance.location.record;
        match &feature.geometry {
            Geometry::Drill(drill) => xnc_include_physical_point(
                &mut bounds,
                drill.position,
                xnc_tool_radius(drill.diameter, record)?,
                record,
            )?,
            Geometry::Slot(slot) => {
                let padding = xnc_tool_radius(slot.width, record)?;
                xnc_include_physical_point(&mut bounds, slot.start, padding, record)?;
                xnc_include_physical_point(&mut bounds, slot.end, padding, record)?;
            }
            Geometry::Route(route) => {
                for segment in &route.segments {
                    xnc_segment_bounds(&mut bounds, segment, record, deadline)?;
                }
            }
            _ => return Err(xnc_error(record, "unsupported-xnc-geometry")),
        }
    }
    Ok(bounds)
}

fn physical_half_ceil(value: i64) -> Result<i64, FabricationError> {
    if value < 0 {
        return Err(FabricationError::CoordinateOutOfRange);
    }
    value
        .checked_add(1)
        .map(|value| value / 2)
        .ok_or(FabricationError::ArithmeticOverflow)
}

fn physical_bounds_id_with_deadline(
    bounds: &DocumentPhysicalBounds,
    deadline: ManufacturingDeadline,
) -> Result<String, FabricationError> {
    stable_id_with_deadline(
        deadline,
        "physical-bounds-identity",
        "physical-bounds",
        &(
            &bounds.document_id,
            &bounds.artifact_digest,
            bounds.format,
            &bounds.extent,
            bounds.resolution,
            &bounds.geometry_digest,
            &bounds.source_locations,
            canonical_provenance(&bounds.provenance),
        ),
    )
}

fn physical_include_point(
    bounds: &mut GerberBounds,
    point: CanonicalPoint,
    padding: i64,
) -> Result<(), FabricationError> {
    bounds
        .include_box(
            point
                .x
                .0
                .checked_sub(padding)
                .ok_or(FabricationError::ArithmeticOverflow)?,
            point
                .y
                .0
                .checked_sub(padding)
                .ok_or(FabricationError::ArithmeticOverflow)?,
            point
                .x
                .0
                .checked_add(padding)
                .ok_or(FabricationError::ArithmeticOverflow)?,
            point
                .y
                .0
                .checked_add(padding)
                .ok_or(FabricationError::ArithmeticOverflow)?,
        )
        .map_err(|_| FabricationError::CoordinateOutOfRange)
}

fn transformed_physical_point(
    transforms: &TransformChain,
    point: CanonicalPoint,
    offset: CanonicalPoint,
) -> Result<(CanonicalPoint, i64), FabricationError> {
    let materialized = transforms.materialize(point)?;
    let error = materialized
        .quantization
        .iter()
        .try_fold(0_u64, |total, item| total.checked_add(item.max_error_pm))
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(FabricationError::ArithmeticOverflow)?;
    Ok((
        CanonicalPoint::new(
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
        ),
        error,
    ))
}

fn transformed_physical_padding(
    width: Option<Picometres>,
    transforms: &TransformChain,
) -> Result<i64, FabricationError> {
    let Some(width) = width else {
        return Ok(0);
    };
    validate_positive_length(width)?;
    let mut numerator = i128::from(width.0);
    let mut denominator = 1_i128;
    for operation in &transforms.operations {
        if let TransformOperation::Scale {
            numerator: scale_numerator,
            denominator: scale_denominator,
        } = *operation
        {
            if scale_numerator == 0 || scale_denominator == 0 {
                return Err(FabricationError::InvalidScale);
            }
            numerator = numerator
                .checked_mul(i128::from(scale_numerator).abs())
                .ok_or(FabricationError::ArithmeticOverflow)?;
            denominator = denominator
                .checked_mul(i128::from(scale_denominator).abs())
                .ok_or(FabricationError::ArithmeticOverflow)?;
        }
    }
    let scaled = numerator
        .checked_add(denominator - 1)
        .ok_or(FabricationError::ArithmeticOverflow)?
        / denominator;
    let scaled = i64::try_from(scaled).map_err(|_| FabricationError::ArithmeticOverflow)?;
    physical_half_ceil(scaled).map_err(|_| FabricationError::ArithmeticOverflow)
}

fn physical_arc_radius(arc: &CanonicalArc) -> Result<i64, FabricationError> {
    let squared = |point: CanonicalPoint| -> Result<u128, FabricationError> {
        let x = i128::from(point.x.0) - i128::from(arc.center.x.0);
        let y = i128::from(point.y.0) - i128::from(arc.center.y.0);
        x.checked_mul(x)
            .and_then(|x| y.checked_mul(y).and_then(|y| x.checked_add(y)))
            .and_then(|value| u128::try_from(value).ok())
            .ok_or(FabricationError::ArithmeticOverflow)
    };
    let squared = squared(arc.start)?.max(squared(arc.end)?);
    let floor = integer_sqrt(squared);
    let radius = floor
        .checked_add(u128::from(floor.checked_mul(floor) != Some(squared)))
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(FabricationError::ArithmeticOverflow)?;
    if radius <= 0 {
        return Err(FabricationError::InvalidIdentity(
            "physical-arc-radius".into(),
        ));
    }
    Ok(radius)
}

fn physical_segment_bounds(
    bounds: &mut GerberBounds,
    segment: &ContourSegment,
    transforms: &TransformChain,
    offset: CanonicalPoint,
    budget: ReconciliationBudget,
) -> Result<(), FabricationError> {
    budget.check()?;
    match segment {
        ContourSegment::Line(line) => {
            let padding = transformed_physical_padding(line.width, transforms)?;
            for point in [line.start, line.end] {
                let (point, error) = transformed_physical_point(transforms, point, offset)?;
                physical_include_point(
                    bounds,
                    point,
                    padding
                        .checked_add(error)
                        .ok_or(FabricationError::ArithmeticOverflow)?,
                )?;
            }
        }
        ContourSegment::Arc(arc) => {
            let (start, start_error) = transformed_physical_point(transforms, arc.start, offset)?;
            let (end, end_error) = transformed_physical_point(transforms, arc.end, offset)?;
            let (center, center_error) =
                transformed_physical_point(transforms, arc.center, offset)?;
            let mirrored = transforms
                .operations
                .iter()
                .fold(false, |mirrored, operation| {
                    if let TransformOperation::Mirror { x, y } = operation {
                        mirrored ^ (*x ^ *y)
                    } else {
                        mirrored
                    }
                });
            let direction = if mirrored {
                match arc.direction {
                    ArcDirection::Clockwise => ArcDirection::CounterClockwise,
                    ArcDirection::CounterClockwise => ArcDirection::Clockwise,
                }
            } else {
                arc.direction
            };
            let transformed = CanonicalArc {
                start,
                end,
                center,
                direction,
                quadrant: arc.quadrant,
                width: arc.width,
                source_resolution: arc.source_resolution,
            };
            let padding = transformed_physical_padding(arc.width, transforms)?;
            physical_include_point(
                bounds,
                start,
                padding
                    .checked_add(start_error)
                    .ok_or(FabricationError::ArithmeticOverflow)?,
            )?;
            physical_include_point(
                bounds,
                end,
                padding
                    .checked_add(end_error)
                    .ok_or(FabricationError::ArithmeticOverflow)?,
            )?;
            let radius = physical_arc_radius(&transformed)?;
            for (direction, vector) in [
                ((1_i64, 0_i64), (1_i128, 0_i128)),
                ((0, 1), (0, 1)),
                ((-1, 0), (-1, 0)),
                ((0, -1), (0, -1)),
            ] {
                budget.check()?;
                if (transformed.quadrant == QuadrantMode::Multi
                    && transformed.start == transformed.end)
                    || xnc_arc_sweep_contains(&transformed, vector)
                {
                    let point = CanonicalPoint::new(
                        center
                            .x
                            .0
                            .checked_add(
                                direction
                                    .0
                                    .checked_mul(radius)
                                    .ok_or(FabricationError::ArithmeticOverflow)?,
                            )
                            .ok_or(FabricationError::ArithmeticOverflow)?,
                        center
                            .y
                            .0
                            .checked_add(
                                direction
                                    .1
                                    .checked_mul(radius)
                                    .ok_or(FabricationError::ArithmeticOverflow)?,
                            )
                            .ok_or(FabricationError::ArithmeticOverflow)?,
                    );
                    physical_include_point(
                        bounds,
                        point,
                        padding
                            .checked_add(center_error)
                            .ok_or(FabricationError::ArithmeticOverflow)?,
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn macro_primitive_radius(
    code: &str,
    values: &[GerberRational],
    unit: SourceUnit,
) -> Result<i64, FabricationError> {
    let length = |index: usize| -> Result<i64, FabricationError> {
        values
            .get(index)
            .ok_or_else(|| FabricationError::InvalidIdentity("macro-argument".into()))?
            .to_picometres(unit)
            .map(|value| value.0)
            .map_err(|_| FabricationError::InvalidNumber)
    };
    let integer = |index: usize| -> Result<i64, FabricationError> {
        values
            .get(index)
            .ok_or_else(|| FabricationError::InvalidIdentity("macro-argument".into()))?
            .exact_i64()
            .map_err(|_| FabricationError::InvalidNumber)
    };
    let radial = |points: &[(i64, i64)], padding: i64| -> Result<i64, FabricationError> {
        points.iter().try_fold(padding, |radius, (x, y)| {
            x.abs()
                .checked_add(y.abs())
                .and_then(|point| point.checked_add(padding))
                .map(|point| radius.max(point))
                .ok_or(FabricationError::ArithmeticOverflow)
        })
    };
    match code {
        "1" => radial(&[(length(2)?, length(3)?)], physical_half_ceil(length(1)?)?),
        "20" => radial(
            &[(length(2)?, length(3)?), (length(4)?, length(5)?)],
            physical_half_ceil(length(1)?)?,
        ),
        "21" => radial(
            &[(length(3)?, length(4)?)],
            physical_half_ceil(length(1)?.max(length(2)?))?,
        ),
        "4" => {
            let vertices =
                usize::try_from(integer(1)?).map_err(|_| FabricationError::InvalidNumber)?;
            let mut radius = 0_i64;
            for index in 0..=vertices {
                radius = radius.max(radial(
                    &[(length(2 + index * 2)?, length(3 + index * 2)?)],
                    0,
                )?);
            }
            Ok(radius)
        }
        "5" => radial(&[(length(2)?, length(3)?)], physical_half_ceil(length(4)?)?),
        "6" => radial(
            &[(length(0)?, length(1)?)],
            physical_half_ceil(length(2)?.max(length(7)?))?,
        ),
        "7" => radial(&[(length(0)?, length(1)?)], physical_half_ceil(length(2)?)?),
        _ => Err(FabricationError::InvalidIdentity(
            "unsupported-macro-primitive".into(),
        )),
    }
}

fn macro_aperture_bounds(
    review: &FabricationReview,
    aperture: &ApertureDefinition,
    unit: SourceUnit,
    budget: ReconciliationBudget,
) -> Result<GerberBounds, FabricationError> {
    let macro_id = aperture
        .macro_id
        .as_deref()
        .ok_or_else(|| FabricationError::DanglingReference(aperture.id.clone()))?;
    let mut definition = None;
    for candidate in &review.macros {
        budget.check()?;
        if candidate.id == macro_id {
            definition = Some(candidate);
            break;
        }
    }
    let definition =
        definition.ok_or_else(|| FabricationError::DanglingReference(macro_id.into()))?;
    let mut variables = BTreeMap::new();
    for (index, value) in aperture.macro_arguments.iter().enumerate() {
        budget.check()?;
        let numerator = value
            .numerator
            .parse::<i128>()
            .map_err(|_| FabricationError::InvalidNumber)?;
        let value = GerberRational::new(numerator, i128::from(value.denominator))
            .map_err(|_| FabricationError::InvalidNumber)?;
        variables.insert(index as u32 + 1, value);
    }
    let mut bounds = GerberBounds::default();
    for operation in &definition.operations {
        budget.check()?;
        if let Some(assignment) = operation.strip_prefix('$') {
            let (number, expression) = assignment
                .split_once('=')
                .ok_or_else(|| FabricationError::InvalidIdentity(definition.id.clone()))?;
            let number = number
                .parse::<u32>()
                .map_err(|_| FabricationError::InvalidNumber)?;
            let (value, _) = GerberExpressionParser::parse(expression, &variables)
                .map_err(|_| FabricationError::InvalidNumber)?;
            variables.insert(
                number,
                value.ok_or_else(|| FabricationError::InvalidIdentity(definition.id.clone()))?,
            );
            continue;
        }
        if operation.starts_with('0') {
            continue;
        }
        let mut fields = Vec::new();
        for field in operation.split(',') {
            budget.check()?;
            fields.push(field);
        }
        let mut values = Vec::with_capacity(fields.len().saturating_sub(1));
        for field in &fields[1..] {
            budget.check()?;
            values.push(
                GerberExpressionParser::parse(field, &variables)
                    .map_err(|_| FabricationError::InvalidNumber)?
                    .0
                    .ok_or_else(|| FabricationError::InvalidIdentity(definition.id.clone()))?,
            );
        }
        let radius = macro_primitive_radius(fields[0], &values, unit)?;
        bounds
            .include_box(-radius, -radius, radius, radius)
            .map_err(|_| FabricationError::CoordinateOutOfRange)?;
    }
    Ok(bounds)
}

fn aperture_local_bounds(
    review: &FabricationReview,
    feature_index: &BTreeMap<&str, &ManufacturingFeature>,
    aperture: &ApertureDefinition,
    unit: SourceUnit,
    budget: ReconciliationBudget,
    aperture_stack: &mut BTreeSet<String>,
) -> Result<GerberBounds, FabricationError> {
    budget.check()?;
    if !aperture_stack.insert(aperture.id.clone()) {
        return Err(FabricationError::InvalidIdentity(
            "recursive-aperture".into(),
        ));
    }
    let result = match aperture.shape {
        ApertureShape::Circle | ApertureShape::Polygon => {
            let diameter = aperture
                .dimensions
                .first()
                .copied()
                .ok_or_else(|| FabricationError::InvalidIdentity(aperture.id.clone()))?;
            let radius = physical_half_ceil(diameter.0)?;
            let mut bounds = GerberBounds::default();
            bounds
                .include_box(-radius, -radius, radius, radius)
                .map_err(|_| FabricationError::CoordinateOutOfRange)?;
            Ok(bounds)
        }
        ApertureShape::Rectangle | ApertureShape::Obround => {
            let [width, height, ..] = aperture.dimensions.as_slice() else {
                return Err(FabricationError::InvalidIdentity(aperture.id.clone()));
            };
            let half_width = physical_half_ceil(width.0)?;
            let half_height = physical_half_ceil(height.0)?;
            let mut bounds = GerberBounds::default();
            bounds
                .include_box(-half_width, -half_height, half_width, half_height)
                .map_err(|_| FabricationError::CoordinateOutOfRange)?;
            Ok(bounds)
        }
        ApertureShape::Macro => macro_aperture_bounds(review, aperture, unit, budget),
        ApertureShape::Block => {
            let mut block = None;
            for candidate in &review.blocks {
                budget.check()?;
                if candidate.document_id == aperture.document_id
                    && candidate.aperture_id == aperture.id
                    && candidate.provenance.location == aperture.provenance.location
                {
                    block = Some(candidate);
                    break;
                }
            }
            let block =
                block.ok_or_else(|| FabricationError::DanglingReference(aperture.id.clone()))?;
            let mut bounds = GerberBounds::default();
            for (index, feature_id) in block.feature_ids.iter().enumerate() {
                if index % 1024 == 0 {
                    budget.check()?;
                }
                let feature = feature_index
                    .get(feature_id.as_str())
                    .copied()
                    .filter(|feature| {
                        feature.document_id == block.document_id
                            && matches!(
                                &feature.membership,
                                FeatureMembership::ApertureBlock { block_id, aperture_id }
                                    if block_id == &block.id && aperture_id == &block.aperture_id
                            )
                    })
                    .ok_or_else(|| FabricationError::InvalidIdentity("block-membership".into()))?;
                bounds.merge(physical_feature_bounds(
                    review,
                    feature_index,
                    feature,
                    CanonicalPoint::default(),
                    unit,
                    budget,
                    aperture_stack,
                )?);
            }
            Ok(bounds)
        }
        ApertureShape::Unknown => Err(FabricationError::InvalidIdentity(
            "unknown-aperture-bounds".into(),
        )),
    };
    aperture_stack.remove(&aperture.id);
    result
}

fn physical_aperture_width(
    review: &FabricationReview,
    feature_index: &BTreeMap<&str, &ManufacturingFeature>,
    feature: &ManufacturingFeature,
    unit: SourceUnit,
    budget: ReconciliationBudget,
    aperture_stack: &mut BTreeSet<String>,
) -> Result<Option<Picometres>, FabricationError> {
    let Some(tool_id) = feature.tool_id.as_deref() else {
        return Ok(None);
    };
    let mut tool = None;
    for candidate in &review.tools {
        budget.check()?;
        if candidate.id == tool_id && candidate.kind == ToolKind::Aperture {
            tool = Some(candidate);
            break;
        }
    }
    let Some(tool) = tool else {
        return Ok(None);
    };
    let mut aperture = None;
    for candidate in &review.apertures {
        budget.check()?;
        if candidate.document_id == tool.document_id
            && candidate.provenance.location == tool.provenance.location
        {
            aperture = Some(candidate);
            break;
        }
    }
    let aperture = aperture.ok_or_else(|| FabricationError::DanglingReference(tool.id.clone()))?;
    let extent = aperture_local_bounds(
        review,
        feature_index,
        aperture,
        unit,
        budget,
        aperture_stack,
    )?
    .extent()
    .ok_or_else(|| FabricationError::InvalidIdentity(aperture.id.clone()))?;
    let radius = [
        extent.min.x.0.unsigned_abs(),
        extent.min.y.0.unsigned_abs(),
        extent.max.x.0.unsigned_abs(),
        extent.max.y.0.unsigned_abs(),
    ]
    .into_iter()
    .max()
    .and_then(|radius| i64::try_from(radius).ok())
    .ok_or(FabricationError::ArithmeticOverflow)?;
    let width = radius
        .checked_mul(2)
        .map(Picometres)
        .ok_or(FabricationError::ArithmeticOverflow)?;
    validate_positive_length(width)?;
    Ok(Some(width))
}

#[allow(clippy::too_many_arguments)]
fn physical_feature_segment_bounds(
    review: &FabricationReview,
    feature_index: &BTreeMap<&str, &ManufacturingFeature>,
    feature: &ManufacturingFeature,
    segment: &ContourSegment,
    offset: CanonicalPoint,
    unit: SourceUnit,
    budget: ReconciliationBudget,
    aperture_stack: &mut BTreeSet<String>,
) -> Result<GerberBounds, FabricationError> {
    let mut segment = segment.clone();
    match &mut segment {
        ContourSegment::Line(line) if line.width.is_none() => {
            line.width = physical_aperture_width(
                review,
                feature_index,
                feature,
                unit,
                budget,
                aperture_stack,
            )?;
        }
        ContourSegment::Arc(arc) if arc.width.is_none() => {
            arc.width = physical_aperture_width(
                review,
                feature_index,
                feature,
                unit,
                budget,
                aperture_stack,
            )?;
        }
        _ => {}
    }
    let mut bounds = GerberBounds::default();
    physical_segment_bounds(&mut bounds, &segment, &feature.transforms, offset, budget)?;
    Ok(bounds)
}

fn physical_feature_bounds(
    review: &FabricationReview,
    feature_index: &BTreeMap<&str, &ManufacturingFeature>,
    feature: &ManufacturingFeature,
    offset: CanonicalPoint,
    unit: SourceUnit,
    budget: ReconciliationBudget,
    aperture_stack: &mut BTreeSet<String>,
) -> Result<GerberBounds, FabricationError> {
    budget.check()?;
    let mut bounds = GerberBounds::default();
    match &feature.geometry {
        Geometry::Point(point) => {
            let (point, error) = transformed_physical_point(&feature.transforms, *point, offset)?;
            physical_include_point(&mut bounds, point, error)?;
        }
        Geometry::Line(line) => bounds.merge(physical_feature_segment_bounds(
            review,
            feature_index,
            feature,
            &ContourSegment::Line(line.clone()),
            offset,
            unit,
            budget,
            aperture_stack,
        )?),
        Geometry::Arc(arc) => bounds.merge(physical_feature_segment_bounds(
            review,
            feature_index,
            feature,
            &ContourSegment::Arc(arc.clone()),
            offset,
            unit,
            budget,
            aperture_stack,
        )?),
        Geometry::Contour(contour) => {
            for segment in &contour.segments {
                bounds.merge(physical_feature_segment_bounds(
                    review,
                    feature_index,
                    feature,
                    segment,
                    offset,
                    unit,
                    budget,
                    aperture_stack,
                )?);
            }
        }
        Geometry::Region(region) => {
            for contour in &region.contours {
                for segment in &contour.segments {
                    physical_segment_bounds(
                        &mut bounds,
                        segment,
                        &feature.transforms,
                        offset,
                        budget,
                    )?;
                }
            }
        }
        Geometry::Flash(flash) => {
            let mut aperture = None;
            for candidate in &review.apertures {
                budget.check()?;
                if candidate.id == flash.aperture_id {
                    aperture = Some(candidate);
                    break;
                }
            }
            let aperture = aperture
                .ok_or_else(|| FabricationError::DanglingReference(flash.aperture_id.clone()))?;
            let local = aperture_local_bounds(
                review,
                feature_index,
                aperture,
                unit,
                budget,
                aperture_stack,
            )?;
            let extent = local
                .extent()
                .ok_or_else(|| FabricationError::InvalidIdentity(aperture.id.clone()))?;
            for local in [
                extent.min,
                CanonicalPoint::new(extent.min.x.0, extent.max.y.0),
                CanonicalPoint::new(extent.max.x.0, extent.min.y.0),
                extent.max,
            ] {
                budget.check()?;
                let point = CanonicalPoint::new(
                    flash
                        .position
                        .x
                        .0
                        .checked_add(local.x.0)
                        .ok_or(FabricationError::ArithmeticOverflow)?,
                    flash
                        .position
                        .y
                        .0
                        .checked_add(local.y.0)
                        .ok_or(FabricationError::ArithmeticOverflow)?,
                );
                let (point, error) =
                    transformed_physical_point(&feature.transforms, point, offset)?;
                physical_include_point(&mut bounds, point, error)?;
            }
        }
        Geometry::Drill(drill) => {
            let (point, error) =
                transformed_physical_point(&feature.transforms, drill.position, offset)?;
            let padding = transformed_physical_padding(Some(drill.diameter), &feature.transforms)?;
            physical_include_point(
                &mut bounds,
                point,
                padding
                    .checked_add(error)
                    .ok_or(FabricationError::ArithmeticOverflow)?,
            )?;
        }
        Geometry::Route(route) => {
            for segment in &route.segments {
                bounds.merge(physical_feature_segment_bounds(
                    review,
                    feature_index,
                    feature,
                    segment,
                    offset,
                    unit,
                    budget,
                    aperture_stack,
                )?);
            }
        }
        Geometry::Slot(slot) => {
            let padding = transformed_physical_padding(Some(slot.width), &feature.transforms)?;
            for source in [slot.start, slot.end] {
                let (point, error) =
                    transformed_physical_point(&feature.transforms, source, offset)?;
                physical_include_point(
                    &mut bounds,
                    point,
                    padding
                        .checked_add(error)
                        .ok_or(FabricationError::ArithmeticOverflow)?,
                )?;
            }
        }
    }
    Ok(bounds)
}

fn definition_features_for_physical_bounds<'a>(
    review: &'a FabricationReview,
    document: &ManufacturingDocument,
    budget: ReconciliationBudget,
) -> Result<HashSet<&'a str>, FabricationError> {
    let mut features = HashMap::new();
    for feature in review
        .features
        .iter()
        .filter(|feature| feature.document_id == document.id)
    {
        budget.check()?;
        features.insert(feature.id.as_str(), feature);
    }
    let mut apertures = HashMap::new();
    for aperture in review
        .apertures
        .iter()
        .filter(|aperture| aperture.document_id == document.id)
    {
        budget.check()?;
        apertures.insert(aperture.id.as_str(), aperture);
    }
    let mut memberships = HashSet::new();
    let mut block_ids = BTreeSet::new();
    for block in review
        .blocks
        .iter()
        .filter(|block| block.document_id == document.id)
    {
        budget.check()?;
        let aperture_matches = apertures
            .get(block.aperture_id.as_str())
            .is_some_and(|aperture| {
                aperture.shape == ApertureShape::Block
                    && aperture.provenance.location == block.provenance.location
            });
        if !block_ids.insert(block.id.as_str())
            || block.id != record_id("block", &document.id, &block.provenance.location)
            || block.aperture_id
                != aperture_id(
                    &document.id,
                    ApertureShape::Block,
                    &block.provenance.location,
                )
            || !aperture_matches
            || block.provenance.location.byte_end >= block.definition_end.byte_start
        {
            return Err(FabricationError::InvalidIdentity("block-membership".into()));
        }
        for feature_id in &block.feature_ids {
            budget.check()?;
            let feature = features
                .get(feature_id.as_str())
                .copied()
                .ok_or_else(|| FabricationError::InvalidIdentity("block-membership".into()))?;
            if !memberships.insert(feature.id.as_str())
                || !matches!(
                    &feature.membership,
                    FeatureMembership::ApertureBlock { block_id, aperture_id }
                        if block_id == &block.id && aperture_id == &block.aperture_id
                )
                || block.provenance.location.byte_end >= feature.provenance.location.byte_start
                || feature.provenance.location.byte_end >= block.definition_end.byte_start
            {
                return Err(FabricationError::InvalidIdentity("block-membership".into()));
            }
        }
    }
    for feature in features.values() {
        budget.check()?;
        match &feature.membership {
            FeatureMembership::TopLevel if memberships.contains(feature.id.as_str()) => {
                return Err(FabricationError::InvalidIdentity("block-membership".into()));
            }
            FeatureMembership::ApertureBlock { block_id, .. }
                if !block_ids.contains(block_id.as_str())
                    || !memberships.contains(feature.id.as_str()) =>
            {
                return Err(FabricationError::InvalidIdentity("block-membership".into()));
            }
            _ => {}
        }
    }
    // This set validates parser-shaped membership, but it is never exclusion authority:
    // report reviews do not retain the bytes needed to prove the claimed ranges.
    Ok(memberships)
}

fn conservative_document_geometry_digest(
    review: &FabricationReview,
    document: &ManufacturingDocument,
    budget: ReconciliationBudget,
) -> Result<String, FabricationError> {
    let mut apertures = BTreeMap::new();
    for aperture in &review.apertures {
        budget.check()?;
        if aperture.document_id == document.id {
            apertures.insert(
                aperture.id.as_str(),
                (
                    aperture.shape,
                    &aperture.dimensions,
                    aperture.polygon_vertices,
                    aperture.polygon_rotation_microdegrees,
                    &aperture.macro_id,
                    &aperture.macro_arguments,
                ),
            );
        }
    }
    let mut macros = BTreeMap::new();
    for definition in &review.macros {
        budget.check()?;
        if definition.document_id == document.id {
            macros.insert(
                definition.id.as_str(),
                (
                    &definition.name,
                    &definition.variables,
                    &definition.operations,
                ),
            );
        }
    }
    let mut blocks = BTreeMap::new();
    for block in &review.blocks {
        budget.check()?;
        if block.document_id == document.id {
            blocks.insert(
                block.id.as_str(),
                (
                    &block.aperture_id,
                    &block.feature_ids,
                    &block.instantiation_feature_ids,
                    &block.definition_end,
                ),
            );
        }
    }
    let mut repetitions = BTreeMap::new();
    for repeat in &review.repetitions {
        budget.check()?;
        if repeat.document_id == document.id {
            repetitions.insert(
                repeat.id.as_str(),
                (
                    &repeat.feature_ids,
                    repeat.x_count,
                    repeat.y_count,
                    repeat.x_step,
                    repeat.y_step,
                ),
            );
        }
    }
    let mut features = BTreeMap::new();
    for feature in &review.features {
        budget.check()?;
        if feature.document_id == document.id {
            features.insert(
                feature.id.as_str(),
                (
                    &feature.tool_id,
                    feature.polarity,
                    &feature.geometry,
                    &feature.transforms,
                    &feature.membership,
                ),
            );
        }
    }
    hash_serialized_with_deadline(
        budget.deadline,
        "physical-geometry-digest",
        &(
            "physical-geometry-v5-conservative-unproven-definitions",
            &document.id,
            &document.artifact_digest,
            apertures,
            macros,
            blocks,
            repetitions,
            features,
        ),
    )
}

fn derive_document_physical_bounds(
    review: &FabricationReview,
    document: &ManufacturingDocument,
    budget: ReconciliationBudget,
) -> Result<Option<DocumentPhysicalBounds>, FabricationError> {
    if !matches!(
        document.format,
        DocumentFormat::Gerber | DocumentFormat::Excellon
    ) {
        return Ok(None);
    }
    let Some(numeric_format) = document.numeric_format.as_ref() else {
        return Ok(None);
    };
    let definition_features = definition_features_for_physical_bounds(review, document, budget)?;
    let conservative_full_extent = definition_features.len() > MANUFACTURING_LIMITS.macros;
    let mut bounds = GerberBounds::default();
    if conservative_full_extent {
        // ponytail: without byte-anchored proof, large definition sets use the full
        // coordinate contract instead of expensive narrowing. Reparse bytes to regain precision.
        bounds
            .include_box(
                -MAX_COORDINATE_PM,
                -MAX_COORDINATE_PM,
                MAX_COORDINATE_PM,
                MAX_COORDINATE_PM,
            )
            .map_err(|_| FabricationError::CoordinateOutOfRange)?;
        let extent = bounds.extent().expect("full coordinate extent");
        let geometry_digest = conservative_document_geometry_digest(review, document, budget)?;
        let mut result = DocumentPhysicalBounds {
            id: String::new(),
            document_id: document.id.clone(),
            artifact_digest: document.artifact_digest.clone(),
            format: document.format,
            extent,
            resolution: numeric_format.resolution,
            geometry_digest,
            source_locations: vec![inventory_provenance(document).location.clone()],
            provenance: inventory_provenance(document),
        };
        result.id = physical_bounds_id_with_deadline(&result, budget.deadline)?;
        return Ok(Some(result));
    } else {
        let mut physical_features = Vec::new();
        for feature in &review.features {
            budget.check()?;
            if feature.document_id == document.id {
                // Definition membership is deliberately not an exclusion predicate.
                physical_features.push(feature);
            }
        }
        let mut document_repeats = Vec::new();
        for repeat in &review.repetitions {
            budget.check()?;
            if repeat.document_id == document.id {
                document_repeats.push(repeat);
            }
        }
        if physical_features.is_empty() && document_repeats.is_empty() {
            return Ok(None);
        }
        let mut feature_index = BTreeMap::new();
        for feature in &review.features {
            budget.check()?;
            if feature_index.insert(feature.id.as_str(), feature).is_some() {
                return Err(FabricationError::DuplicateId(feature.id.clone()));
            }
        }
        let mut aperture_stack = BTreeSet::new();
        for feature in physical_features {
            budget.check()?;
            bounds.merge(physical_feature_bounds(
                review,
                &feature_index,
                feature,
                CanonicalPoint::default(),
                numeric_format.unit,
                budget,
                &mut aperture_stack,
            )?);
        }
        for repeat in document_repeats {
            budget.check()?;
            let max_x = repeat_max_offset(repeat.x_step, repeat.x_count)?;
            let max_y = repeat_max_offset(repeat.y_step, repeat.y_count)?;
            for feature_id in &repeat.feature_ids {
                let feature = feature_index
                    .get(feature_id.as_str())
                    .copied()
                    .ok_or_else(|| FabricationError::DanglingReference(feature_id.clone()))?;
                for offset in [
                    CanonicalPoint::default(),
                    CanonicalPoint::new(max_x.0, 0),
                    CanonicalPoint::new(0, max_y.0),
                    CanonicalPoint::new(max_x.0, max_y.0),
                ] {
                    budget.check()?;
                    bounds.merge(physical_feature_bounds(
                        review,
                        &feature_index,
                        feature,
                        offset,
                        numeric_format.unit,
                        budget,
                        &mut aperture_stack,
                    )?);
                }
            }
        }
    }
    let Some(extent) = bounds.extent() else {
        return Ok(None);
    };
    let mut feature_records = BTreeMap::new();
    let mut aperture_records = BTreeMap::new();
    let mut macro_records = BTreeMap::new();
    let mut block_records = BTreeMap::new();
    let mut repeat_records = BTreeMap::new();
    let mut locations = BTreeSet::new();
    for feature in review
        .features
        .iter()
        .filter(|feature| feature.document_id == document.id)
    {
        budget.check()?;
        feature_records.insert(feature.id.as_str(), feature);
        locations.insert(feature.provenance.location.clone());
    }
    for aperture in review
        .apertures
        .iter()
        .filter(|aperture| aperture.document_id == document.id)
    {
        budget.check()?;
        aperture_records.insert(aperture.id.as_str(), aperture);
        locations.insert(aperture.provenance.location.clone());
    }
    for definition in review
        .macros
        .iter()
        .filter(|definition| definition.document_id == document.id)
    {
        budget.check()?;
        macro_records.insert(definition.id.as_str(), definition);
        locations.insert(definition.provenance.location.clone());
    }
    for block in review
        .blocks
        .iter()
        .filter(|block| block.document_id == document.id)
    {
        budget.check()?;
        block_records.insert(block.id.as_str(), block);
        locations.insert(block.provenance.location.clone());
    }
    for repeat in review
        .repetitions
        .iter()
        .filter(|repeat| repeat.document_id == document.id)
    {
        budget.check()?;
        repeat_records.insert(repeat.id.as_str(), repeat);
        locations.insert(repeat.provenance.location.clone());
    }
    budget.check()?;
    let geometry_digest = hash_serialized_with_deadline(
        budget.deadline,
        "physical-geometry-digest",
        &(
            "physical-geometry-v3",
            feature_records,
            aperture_records,
            macro_records,
            block_records,
            repeat_records,
        ),
    )?;
    let mut source_locations = Vec::with_capacity(locations.len());
    for location in locations {
        budget.check()?;
        source_locations.push(location);
    }
    let mut result = DocumentPhysicalBounds {
        id: String::new(),
        document_id: document.id.clone(),
        artifact_digest: document.artifact_digest.clone(),
        format: document.format,
        extent,
        resolution: numeric_format.resolution,
        geometry_digest,
        source_locations,
        provenance: inventory_provenance(document),
    };
    result.id = physical_bounds_id_with_deadline(&result, budget.deadline)?;
    Ok(Some(result))
}

fn derive_release_physical_bounds(
    review: &FabricationReview,
    budget: ReconciliationBudget,
) -> Result<Vec<DocumentPhysicalBounds>, FabricationError> {
    let mut output = BTreeMap::new();
    for document in &review.documents {
        budget.check()?;
        if let Some(bounds) = derive_document_physical_bounds(review, document, budget)? {
            output.insert(bounds.document_id.clone(), bounds);
        }
    }
    budget.check()?;
    Ok(output.into_values().collect())
}

pub fn parse_xnc_document(input: &ManufacturingInput) -> Result<XncProduction, XncParseError> {
    parse_xnc_document_with_timeout(
        input,
        Duration::from_millis(MANUFACTURING_LIMITS.file_timeout_ms),
    )
}

pub fn parse_xnc_document_with_timeout(
    input: &ManufacturingInput,
    timeout: Duration,
) -> Result<XncProduction, XncParseError> {
    parse_xnc_document_with_deadline(
        input,
        ManufacturingDeadline::from_timeout(timeout).for_input(input),
    )
}

fn parse_xnc_document_with_deadline(
    input: &ManufacturingInput,
    deadline: ManufacturingDeadline,
) -> Result<XncProduction, XncParseError> {
    let digest = sha256_with_deadline(&input.original_bytes, deadline, "xnc-input-hash").map_err(
        |error| match error {
            FabricationError::LimitExceeded { .. } => XncParseError::Deadline {
                stage: "input-hash",
            },
            error => XncParseError::Canonical(error),
        },
    )?;
    if input.kind_candidate != ManufacturingKindCandidate::Excellon
        || input.size != input.original_bytes.len() as u64
        || input.artifact_digest != digest
        || !valid_virtual_path(&input.virtual_path)
    {
        return Err(xnc_error(0, "invalid-xnc-input-identity"));
    }
    let (lines, lexical_tokens, max_line_bytes, metadata_bytes, max_text_bytes) =
        xnc_lines(&input.original_bytes, deadline)?;
    let (m48_index, first) = lines
        .iter()
        .enumerate()
        .find(|(_, line)| !line.text.starts_with(';'))
        .ok_or_else(|| xnc_error(0, "empty-xnc"))?;
    if first.text != "M48" {
        return Err(xnc_error(first.record, "missing-m48"));
    }
    let dialect = xnc_dialect(&lines, deadline)?;
    let document_id = document_id(&input.artifact_digest, DocumentFormat::Excellon)
        .map_err(XncParseError::Canonical)?;
    let mut unit = None;
    let mut tool_definitions = BTreeMap::<u16, XncToolDefinition>::new();
    let mut file_function = None;
    let mut max_integer_digits = 1_u8;
    let mut max_decimal_digits = 0_u8;
    let mut body_start = None;
    for line in lines.iter().skip(m48_index + 1) {
        check_xnc_deadline(deadline, "xnc-header")?;
        if line.text == "%" {
            body_start = Some(line.record as usize + 1);
            break;
        }
        if line.text.starts_with(';') {
            if let Some(attribute) = xnc_attribute(line, &document_id, &input.artifact_digest)
                && attribute.name == "TF.FileFunction"
            {
                if file_function.is_some() {
                    return Err(xnc_error(line.record, "duplicate-file-function"));
                }
                file_function =
                    Some(package_file_function(&attribute).map_err(XncParseError::Canonical)?);
            }
            continue;
        }
        match line.text.as_str() {
            "METRIC" if unit.is_none() => unit = Some(SourceUnit::Millimetre),
            "INCH" if unit.is_none() => unit = Some(SourceUnit::Inch),
            "METRIC,TZ" if unit.is_none() && dialect == XncDialect::LibrePcbLegacy => {
                unit = Some(SourceUnit::Millimetre)
            }
            "FMAT,2" if dialect != XncDialect::Strict => {}
            _ if line.text.starts_with('T') && line.text.contains('C') => {
                let (code, diameter) = line
                    .text
                    .split_once('C')
                    .ok_or_else(|| xnc_error(line.record, "invalid-tool-definition"))?;
                let code_value = xnc_tool_code(code, dialect)
                    .ok_or_else(|| xnc_error(line.record, "invalid-tool-code"))?;
                let unit = unit.ok_or_else(|| xnc_error(line.record, "tool-before-unit"))?;
                let (integer, decimal) = xnc_number_profile(diameter)
                    .ok_or_else(|| xnc_error(line.record, "invalid-tool-diameter"))?;
                max_integer_digits = max_integer_digits.max(integer);
                max_decimal_digits = max_decimal_digits.max(decimal);
                let definition = XncToolDefinition {
                    code: code_value,
                    source_code: code.into(),
                    diameter: xnc_length(diameter, unit, line.record)?,
                    provenance: xnc_provenance(&document_id, &input.artifact_digest, line),
                };
                if tool_definitions.insert(code_value, definition).is_some() {
                    return Err(xnc_error(line.record, "duplicate-tool"));
                }
            }
            _ => {
                return Err(XncParseError::Unsupported {
                    record: line.record,
                    command: bounded_command(&line.text),
                });
            }
        }
    }
    let body_start =
        body_start.ok_or_else(|| xnc_error(lines.len() as u64, "missing-header-end"))?;
    let unit = unit.ok_or_else(|| xnc_error(0, "missing-unit"))?;
    if tool_definitions.is_empty() {
        return Err(xnc_error(0, "missing-tools"));
    }
    let base_provenance = file_function
        .as_ref()
        .map(|function| function.provenance.clone())
        .unwrap_or_else(|| xnc_provenance(&document_id, &input.artifact_digest, first));
    let primary_role = file_function
        .as_ref()
        .map_or(LayerRole::DrillMap, |function| function.role);
    let mut primary_location = base_provenance.location.clone();
    primary_location.subrecord = Some(2);
    let primary_provenance = ManufacturingProvenance {
        location: primary_location,
        ..base_provenance.clone()
    };
    let primary_layer_id = layer_id(
        &document_id,
        None,
        primary_role,
        LayerSide::NotApplicable,
        None,
        if file_function.is_some() {
            Authority::Explicit
        } else {
            Authority::Unknown
        },
        &primary_provenance.location,
    );
    let mut layers = vec![ManufacturingLayer {
        id: primary_layer_id.clone(),
        document_id: document_id.clone(),
        name: None,
        role: primary_role,
        side: LayerSide::NotApplicable,
        context: LayerContext::Board,
        polarity: LayerPolarity::Unknown,
        order: None,
        authority: if file_function.is_some() {
            Authority::Explicit
        } else {
            Authority::Unknown
        },
        provenance: primary_provenance,
    }];
    let mut span_ids = None;
    if let Some(function) = &file_function
        && let (Some(from), Some(to)) = (function.from_layer, function.to_layer)
    {
        let mut ids = Vec::new();
        for (subrecord, order) in [(0_u32, from), (1_u32, to)] {
            let mut location = function.provenance.location.clone();
            location.subrecord = Some(subrecord);
            let provenance = ManufacturingProvenance {
                location,
                ..function.provenance.clone()
            };
            let name = format!("L{order}");
            let id = layer_id(
                &document_id,
                Some(&name),
                LayerRole::Copper,
                LayerSide::Unknown,
                Some(order),
                Authority::Explicit,
                &provenance.location,
            );
            layers.push(ManufacturingLayer {
                id: id.clone(),
                document_id: document_id.clone(),
                name: Some(name),
                role: LayerRole::Copper,
                side: LayerSide::Unknown,
                context: LayerContext::Board,
                polarity: LayerPolarity::Unknown,
                order: Some(order),
                authority: Authority::Explicit,
                provenance,
            });
            ids.push(id);
        }
        span_ids = Some(LayerSpan {
            from_layer_id: Some(ids[0].clone()),
            to_layer_id: Some(ids[1].clone()),
        });
    }
    let mut tools = BTreeMap::new();
    for definition in tool_definitions.values() {
        let kind = if file_function
            .as_ref()
            .is_some_and(|function| function.role == LayerRole::Route)
        {
            ToolKind::Route
        } else {
            ToolKind::Drill
        };
        let identity_kind = format!("{kind:?}:{}", definition.source_code);
        let id = tool_id(
            &document_id,
            &identity_kind,
            &definition.provenance.location,
        );
        tools.insert(
            definition.code,
            ManufacturingTool {
                id,
                document_id: document_id.clone(),
                code: definition.source_code.clone(),
                kind,
                diameter: Some(definition.diameter),
                plating: file_function
                    .as_ref()
                    .map_or(Plating::Unknown, |function| function.plating),
                span: span_ids.clone(),
                provenance: definition.provenance.clone(),
            },
        );
    }
    let mut selected = None;
    let mut position = None;
    let mut route: Option<Vec<ContourSegment>> = None;
    let mut features = Vec::new();
    let mut terminated = false;
    let mut drills = 0_usize;
    let mut routes = 0_usize;
    let mut slots = 0_usize;
    let mut drill_route_units = 0_usize;
    for line in lines.iter().skip(body_start) {
        check_xnc_deadline(deadline, "xnc-interpretation")?;
        if terminated {
            return Err(xnc_error(line.record, "data-after-m30"));
        }
        if line.text.starts_with(';') {
            continue;
        }
        match line.text.as_str() {
            "M30" => {
                if route.is_some() {
                    return Err(xnc_error(line.record, "m30-inside-route"));
                }
                terminated = true;
            }
            "G05" if route.is_none() => {}
            "G90" if dialect != XncDialect::Strict => {}
            "M71" if dialect == XncDialect::LibrePcbLegacy => {}
            "M15" => {
                if route.is_some() || position.is_none() || selected.is_none() {
                    return Err(xnc_error(line.record, "invalid-route-start"));
                }
                route = Some(Vec::new());
            }
            "M16" => {
                let segments = route
                    .take()
                    .ok_or_else(|| xnc_error(line.record, "route-not-open"))?;
                if segments.is_empty() {
                    return Err(xnc_error(line.record, "empty-route"));
                }
                let tool = tools
                    .get(&selected.expect("route has selected tool"))
                    .expect("selected tool exists");
                features.push(xnc_feature(
                    &document_id,
                    &primary_layer_id,
                    &tool.id,
                    Geometry::Route(RouteFeature {
                        segments,
                        tool_id: tool.id.clone(),
                    }),
                    xnc_provenance(&document_id, &input.artifact_digest, line),
                ));
                routes += 1;
            }
            _ if line.text.starts_with('T') && !line.text.contains('C') => {
                let digits = line.text.strip_prefix('T').unwrap_or_default();
                if digits == "0" && dialect != XncDialect::Strict && route.is_none() {
                    selected = None;
                } else {
                    let code = xnc_tool_code(&line.text, dialect)
                        .ok_or_else(|| xnc_error(line.record, "invalid-tool-selection"))?;
                    if !tools.contains_key(&code) || route.is_some() {
                        return Err(xnc_error(line.record, "undefined-tool-selection"));
                    }
                    selected = Some(code);
                }
            }
            _ if line.text.starts_with("G00") => {
                if route.is_some() {
                    return Err(xnc_error(line.record, "rapid-inside-route"));
                }
                let (point, integer, decimal) = xnc_point(&line.text[3..], unit, line.record)?;
                max_integer_digits = max_integer_digits.max(integer);
                max_decimal_digits = max_decimal_digits.max(decimal);
                position = Some(point);
            }
            _ if line.text.starts_with("G85") => {
                if route.is_some() {
                    return Err(xnc_error(line.record, "slot-inside-route"));
                }
                let start = position.ok_or_else(|| xnc_error(line.record, "slot-without-start"))?;
                let (end, integer, decimal) = xnc_point(&line.text[3..], unit, line.record)?;
                max_integer_digits = max_integer_digits.max(integer);
                max_decimal_digits = max_decimal_digits.max(decimal);
                if start == end {
                    return Err(xnc_error(line.record, "zero-length-slot"));
                }
                let tool = tools
                    .get(&selected.ok_or_else(|| xnc_error(line.record, "slot-without-tool"))?)
                    .ok_or_else(|| xnc_error(line.record, "slot-without-tool"))?;
                let width = tool
                    .diameter
                    .ok_or_else(|| xnc_error(line.record, "slot-tool-without-diameter"))?;
                features.push(xnc_feature(
                    &document_id,
                    &primary_layer_id,
                    &tool.id,
                    Geometry::Slot(SlotFeature {
                        start,
                        end,
                        width,
                        tool_id: tool.id.clone(),
                    }),
                    xnc_provenance(&document_id, &input.artifact_digest, line),
                ));
                position = Some(end);
                slots += 1;
                drill_route_units += 1;
            }
            _ if line.text.starts_with("G01")
                || line.text.starts_with("G02")
                || line.text.starts_with("G03") =>
            {
                let segments = route
                    .as_mut()
                    .ok_or_else(|| xnc_error(line.record, "route-command-outside-route"))?;
                let start =
                    position.ok_or_else(|| xnc_error(line.record, "route-without-position"))?;
                let command = &line.text[..3];
                let fields = xnc_fields(&line.text[3..], line.record)?;
                let x = fields
                    .get(&b'X')
                    .ok_or_else(|| xnc_error(line.record, "route-without-x"))?;
                let y = fields
                    .get(&b'Y')
                    .ok_or_else(|| xnc_error(line.record, "route-without-y"))?;
                let end = CanonicalPoint {
                    x: xnc_length(x, unit, line.record)?,
                    y: xnc_length(y, unit, line.record)?,
                };
                for value in fields.values() {
                    let (integer, decimal) = xnc_number_profile(value)
                        .ok_or_else(|| xnc_error(line.record, "invalid-route-coordinate"))?;
                    max_integer_digits = max_integer_digits.max(integer);
                    max_decimal_digits = max_decimal_digits.max(decimal);
                }
                let tool = tools
                    .get(&selected.ok_or_else(|| xnc_error(line.record, "route-without-tool"))?)
                    .ok_or_else(|| xnc_error(line.record, "route-without-tool"))?;
                let width = tool.diameter;
                if command == "G01" {
                    if fields.len() != 2 {
                        return Err(xnc_error(line.record, "invalid-linear-route-fields"));
                    }
                    segments.push(ContourSegment::Line(CanonicalLine { start, end, width }));
                } else {
                    let direction = if command == "G02" {
                        ArcDirection::Clockwise
                    } else {
                        ArcDirection::CounterClockwise
                    };
                    let center = if let Some(radius) = fields.get(&b'A') {
                        if fields.len() != 3 {
                            return Err(xnc_error(line.record, "invalid-radius-route-fields"));
                        }
                        xnc_radius_center(
                            start,
                            end,
                            xnc_length(radius, unit, line.record)?,
                            direction,
                            line.record,
                        )?
                    } else if fields
                        .keys()
                        .all(|tag| matches!(tag, b'X' | b'Y' | b'I' | b'J'))
                        && (fields.contains_key(&b'I') || fields.contains_key(&b'J'))
                    {
                        CanonicalPoint::new(
                            start
                                .x
                                .0
                                .checked_add(fields.get(&b'I').map_or(Ok(0), |value| {
                                    xnc_length(value, unit, line.record).map(|value| value.0)
                                })?)
                                .ok_or_else(|| xnc_error(line.record, "arc-center-overflow"))?,
                            start
                                .y
                                .0
                                .checked_add(fields.get(&b'J').map_or(Ok(0), |value| {
                                    xnc_length(value, unit, line.record).map(|value| value.0)
                                })?)
                                .ok_or_else(|| xnc_error(line.record, "arc-center-overflow"))?,
                        )
                    } else {
                        return Err(xnc_error(line.record, "arc-without-radius-or-center"));
                    };
                    let resolution =
                        SourceNumericFormat::new(unit, max_integer_digits, max_decimal_digits)
                            .map_err(XncParseError::Canonical)?
                            .resolution;
                    if !valid_single_quadrant_arc(start, end, center, direction, resolution) {
                        return Err(xnc_error(line.record, "invalid-arc-geometry"));
                    }
                    segments.push(ContourSegment::Arc(CanonicalArc {
                        start,
                        end,
                        center,
                        direction,
                        quadrant: QuadrantMode::Single,
                        width,
                        source_resolution: resolution,
                    }));
                }
                drill_route_units += 1;
                position = Some(end);
            }
            _ if line.text.starts_with(['X', 'Y']) => {
                if route.is_some() {
                    return Err(xnc_error(line.record, "drill-inside-route"));
                }
                let (point, integer, decimal) = xnc_point(&line.text, unit, line.record)?;
                max_integer_digits = max_integer_digits.max(integer);
                max_decimal_digits = max_decimal_digits.max(decimal);
                let tool = tools
                    .get(&selected.ok_or_else(|| xnc_error(line.record, "drill-without-tool"))?)
                    .ok_or_else(|| xnc_error(line.record, "drill-without-tool"))?;
                let diameter = tool
                    .diameter
                    .ok_or_else(|| xnc_error(line.record, "drill-tool-without-diameter"))?;
                features.push(xnc_feature(
                    &document_id,
                    &primary_layer_id,
                    &tool.id,
                    Geometry::Drill(DrillFeature {
                        position: point,
                        diameter,
                        tool_id: tool.id.clone(),
                    }),
                    xnc_provenance(&document_id, &input.artifact_digest, line),
                ));
                position = Some(point);
                drills += 1;
                drill_route_units += 1;
            }
            _ => {
                return Err(XncParseError::Unsupported {
                    record: line.record,
                    command: bounded_command(&line.text),
                });
            }
        }
        if drill_route_units > MANUFACTURING_LIMITS.drill_route_features {
            return Err(XncParseError::Resource {
                resource: "drill-route-features",
                observed: drill_route_units as u64,
                limit: MANUFACTURING_LIMITS.drill_route_features as u64,
            });
        }
    }
    if !terminated {
        return Err(xnc_error(lines.len() as u64, "missing-m30"));
    }
    if route.is_some() {
        return Err(xnc_error(lines.len() as u64, "unclosed-route"));
    }
    let numeric_format = SourceNumericFormat::new(unit, max_integer_digits, max_decimal_digits)
        .map_err(XncParseError::Canonical)?;
    let mut max_record_text = 0_usize;
    for line in &lines {
        check_xnc_deadline(deadline, "xnc-document-metrics")?;
        max_record_text = max_record_text.max(line.text.len());
    }
    let document = ManufacturingDocument {
        id: document_id.clone(),
        virtual_path: input.virtual_path.clone(),
        artifact_digest: input.artifact_digest.clone(),
        format: DocumentFormat::Excellon,
        adapter: "ratemypcb-xnc".into(),
        adapter_version: XNC_ADAPTER_VERSION.into(),
        parse_status: ParseStatus::Complete,
        numeric_format: Some(numeric_format),
        metrics: DocumentMetrics {
            raw_bytes: input.size,
            records: lines.len() as u64,
            lexical_tokens,
            metadata_bytes,
            max_line_bytes,
            max_text_bytes,
            max_numeric_bytes: MANUFACTURING_LIMITS.max_numeric_bytes.min(max_record_text),
            ..DocumentMetrics::default()
        },
    };
    let outcome = ManufacturingInputOutcome {
        id: input_outcome_id(
            &input.virtual_path,
            Some(&input.artifact_digest),
            ManufacturingKindCandidate::Excellon,
        ),
        virtual_path: input.virtual_path.clone(),
        artifact_digest: Some(input.artifact_digest.clone()),
        kind_candidate: ManufacturingKindCandidate::Excellon,
        size: input.size,
        state: ManufacturingLoadState::Retained,
        reason: None,
    };
    let capability_provenance = file_function
        .as_ref()
        .map(|function| &function.provenance)
        .unwrap_or(&base_provenance);
    let mut capabilities = Vec::new();
    let mut push_capability = |id, state, detail| {
        capabilities.push(semantic_capability(
            id,
            state,
            Authority::Explicit,
            &document_id,
            Some(capability_provenance),
            detail,
        ));
    };
    push_capability(
        CapabilityId::DocumentSyntax,
        CapabilityState::Complete,
        "Strict framed XNC state completed through one M30.",
    );
    push_capability(
        CapabilityId::UnitsAndFormat,
        CapabilityState::Complete,
        "Every coordinate uses an explicit decimal under an explicit unit.",
    );
    push_capability(
        CapabilityId::Tools,
        CapabilityState::Complete,
        "Every selected finished tool was uniquely defined.",
    );
    push_capability(
        CapabilityId::Drills,
        if drills > 0 {
            CapabilityState::Complete
        } else {
            CapabilityState::NotProvided
        },
        "All drill hits retain tool and finished diameter.",
    );
    push_capability(
        CapabilityId::Routes,
        if routes > 0 {
            CapabilityState::Complete
        } else {
            CapabilityState::NotProvided
        },
        "All bounded linear and arc route segments are retained.",
    );
    push_capability(
        CapabilityId::Slots,
        if slots > 0 {
            CapabilityState::Complete
        } else {
            CapabilityState::NotProvided
        },
        "All G85 slots retain start, end, width, and tool.",
    );
    push_capability(
        CapabilityId::Plating,
        if file_function
            .as_ref()
            .is_some_and(|function| function.plating != Plating::Unknown)
        {
            CapabilityState::Complete
        } else {
            CapabilityState::NotProvided
        },
        "Plating is complete only from explicit FileFunction.",
    );
    push_capability(
        CapabilityId::LayerSpans,
        if span_ids.is_some() {
            CapabilityState::Complete
        } else {
            CapabilityState::NotProvided
        },
        "Layer span is complete only from explicit FileFunction endpoints.",
    );
    push_capability(
        CapabilityId::Extents,
        if features.is_empty() {
            CapabilityState::NotProvided
        } else {
            CapabilityState::Complete
        },
        "Fixed-point drill/slot/route extents are deterministic.",
    );
    capabilities.sort_by_key(|capability| capability.id);
    let extents = xnc_physical_bounds(&features, deadline)?.extent();
    let x2_attributes = if let Some(function) = &file_function {
        let mut attribute = scoped_x2_attribute(
            &document_id,
            X2AttributeScope::File,
            X2AttributeKind::FileFunction,
            function
                .raw
                .strip_prefix("TF.FileFunction,")
                .expect("canonical FileFunction prefix")
                .split(',')
                .map(str::to_owned)
                .collect(),
            false,
            function.provenance.clone(),
        );
        attribute.target_ids.push(document_id.clone());
        attribute.id = scoped_x2_attribute_id_with_deadline(&attribute, deadline)
            .map_err(XncParseError::Canonical)?;
        vec![attribute]
    } else {
        Vec::new()
    };
    let mut ordered_tools = Vec::with_capacity(tools.len());
    for tool in tools.into_values() {
        check_xnc_deadline(deadline, "xnc-tool-order")?;
        ordered_tools.push(tool);
    }
    let mut review =
        FabricationReview::empty_with_deadline(deadline).map_err(XncParseError::Canonical)?;
    review.status = FabricationStatus::Partial;
    review.input_outcomes = vec![outcome];
    review.documents = vec![document];
    review.layers = layers;
    review.tools = ordered_tools;
    review.features = features;
    review.x2_attributes = x2_attributes;
    review.capabilities = CapabilityLedger {
        records: capabilities,
    };
    review.physical_bounds =
        derive_release_physical_bounds(&review, ReconciliationBudget { deadline })
            .map_err(XncParseError::Canonical)?;
    review
        .refresh_digests_with_deadline(deadline)
        .map_err(XncParseError::Canonical)?;
    review
        .validate_with_deadline(deadline)
        .map_err(XncParseError::Canonical)?;
    check_xnc_deadline(deadline, "xnc-canonicalization")?;
    Ok(XncProduction {
        review,
        dialect,
        file_function,
        extents,
    })
}

pub const GERBER_JOB_ADAPTER_VERSION: &str = "gerber-job-2023.06-ratemypcb-1";

#[derive(Clone, Debug)]
pub struct GerberJobReference {
    pub virtual_path: String,
    pub document_id: String,
    pub file_function: PackageFileFunction,
    pub provenance: ManufacturingProvenance,
}

#[derive(Clone, Debug)]
pub struct GerberJobProduction {
    pub document: ManufacturingDocument,
    pub product: Option<ProductIdentity>,
    pub references: Vec<GerberJobReference>,
    pub unsupported_fields: Vec<String>,
}

#[derive(Debug)]
pub enum GerberJobParseError {
    Resource {
        resource: &'static str,
        observed: u64,
        limit: u64,
    },
    Invalid {
        reason: String,
    },
    Deadline,
    Canonical(FabricationError),
}

impl std::fmt::Display for GerberJobParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for GerberJobParseError {}

struct UniqueJson(Value);

impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct UniqueVisitor;

        impl<'de> Visitor<'de> for UniqueVisitor {
            type Value = UniqueJson;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("JSON without duplicate object keys")
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
                Ok(UniqueJson(Value::Bool(value)))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
                Ok(UniqueJson(Value::Number(value.into())))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
                Ok(UniqueJson(Value::Number(value.into())))
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                serde_json::Number::from_f64(value)
                    .map(Value::Number)
                    .map(UniqueJson)
                    .ok_or_else(|| E::custom("non-finite JSON number"))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(UniqueJson(Value::String(value.into())))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
                Ok(UniqueJson(Value::String(value)))
            }

            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(UniqueJson(Value::Null))
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(UniqueJson(Value::Null))
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                UniqueJson::deserialize(deserializer)
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut values = Vec::new();
                while let Some(value) = sequence.next_element::<UniqueJson>()? {
                    values.push(value.0);
                }
                Ok(UniqueJson(Value::Array(values)))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(A::Error::custom(format!("duplicate JSON key: {key}")));
                    }
                    values.insert(key, map.next_value::<UniqueJson>()?.0);
                }
                Ok(UniqueJson(Value::Object(values)))
            }
        }

        deserializer.deserialize_any(UniqueVisitor)
    }
}

#[derive(Default)]
struct JsonMetrics {
    nodes: u64,
    text_bytes: u64,
    max_text_bytes: usize,
    max_depth: u8,
}

fn note_json_text(text: &str, metrics: &mut JsonMetrics) -> Result<(), GerberJobParseError> {
    if text.len() > MANUFACTURING_LIMITS.max_text_bytes || text.chars().any(char::is_control) {
        return Err(GerberJobParseError::Resource {
            resource: "json-text",
            observed: text.len() as u64,
            limit: MANUFACTURING_LIMITS.max_text_bytes as u64,
        });
    }
    metrics.text_bytes =
        metrics
            .text_bytes
            .checked_add(text.len() as u64)
            .ok_or(GerberJobParseError::Resource {
                resource: "json-metadata",
                observed: u64::MAX,
                limit: MANUFACTURING_LIMITS.metadata_bytes_per_file,
            })?;
    metrics.max_text_bytes = metrics.max_text_bytes.max(text.len());
    if metrics.text_bytes > MANUFACTURING_LIMITS.metadata_bytes_per_file {
        return Err(GerberJobParseError::Resource {
            resource: "json-metadata",
            observed: metrics.text_bytes,
            limit: MANUFACTURING_LIMITS.metadata_bytes_per_file,
        });
    }
    Ok(())
}

fn measure_json(
    value: &Value,
    depth: u8,
    metrics: &mut JsonMetrics,
    deadline: ManufacturingDeadline,
) -> Result<(), GerberJobParseError> {
    deadline
        .check("job-json-metrics")
        .map_err(|_| GerberJobParseError::Deadline)?;
    if depth > MANUFACTURING_LIMITS.max_nesting {
        return Err(GerberJobParseError::Resource {
            resource: "json-depth",
            observed: u64::from(depth),
            limit: u64::from(MANUFACTURING_LIMITS.max_nesting),
        });
    }
    metrics.max_depth = metrics.max_depth.max(depth);
    metrics.nodes = metrics
        .nodes
        .checked_add(1)
        .ok_or(GerberJobParseError::Resource {
            resource: "json-nodes",
            observed: u64::MAX,
            limit: MANUFACTURING_LIMITS.records_per_file,
        })?;
    if metrics.nodes > MANUFACTURING_LIMITS.records_per_file {
        return Err(GerberJobParseError::Resource {
            resource: "json-nodes",
            observed: metrics.nodes,
            limit: MANUFACTURING_LIMITS.records_per_file,
        });
    }
    match value {
        Value::String(text) => note_json_text(text, metrics)?,
        Value::Array(values) => {
            for value in values {
                measure_json(value, depth + 1, metrics, deadline)?;
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                note_json_text(key, metrics)?;
                measure_json(value, depth + 1, metrics, deadline)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn job_provenance(
    document_id: &str,
    digest: &str,
    input_size: u64,
    record: u64,
) -> ManufacturingProvenance {
    ManufacturingProvenance {
        document_id: document_id.into(),
        artifact_digest: digest.into(),
        producer: "ratemypcb-gerber-job".into(),
        producer_version: GERBER_JOB_ADAPTER_VERSION.into(),
        location: StructuralLocation {
            record,
            subrecord: None,
            byte_start: 0,
            byte_end: input_size.saturating_sub(1),
        },
        source_lexeme: None,
    }
}

fn resolve_job_path(job_path: &str, reference: &str) -> Option<String> {
    if reference.is_empty()
        || !reference.is_ascii()
        || reference.starts_with('/')
        || reference.contains(['\\', ':'])
        || reference
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return None;
    }
    let directory = job_path.rsplit_once('/').map(|(directory, _)| directory);
    let path = directory.map_or_else(
        || reference.to_owned(),
        |directory| format!("{directory}/{reference}"),
    );
    valid_virtual_path(&path).then_some(path)
}

pub fn parse_gerber_job_document(
    input: &ManufacturingInput,
    inventory: &ManufacturingInventory,
) -> Result<GerberJobProduction, GerberJobParseError> {
    parse_gerber_job_document_with_timeout(
        input,
        inventory,
        Duration::from_millis(MANUFACTURING_LIMITS.file_timeout_ms),
    )
}

fn parse_gerber_job_document_with_timeout(
    input: &ManufacturingInput,
    inventory: &ManufacturingInventory,
    timeout: Duration,
) -> Result<GerberJobProduction, GerberJobParseError> {
    parse_gerber_job_document_with_deadline(
        input,
        inventory,
        ManufacturingDeadline::from_timeout(timeout).for_input(input),
    )
}

fn parse_gerber_job_document_with_deadline(
    input: &ManufacturingInput,
    inventory: &ManufacturingInventory,
    deadline: ManufacturingDeadline,
) -> Result<GerberJobProduction, GerberJobParseError> {
    let digest = sha256_with_deadline(&input.original_bytes, deadline, "job-input-hash").map_err(
        |error| match error {
            FabricationError::LimitExceeded { .. } => GerberJobParseError::Deadline,
            error => GerberJobParseError::Canonical(error),
        },
    )?;
    if input.kind_candidate != ManufacturingKindCandidate::GerberJob
        || input.size != input.original_bytes.len() as u64
        || input.artifact_digest != digest
        || !valid_virtual_path(&input.virtual_path)
    {
        return Err(GerberJobParseError::Invalid {
            reason: "invalid-job-input-identity".into(),
        });
    }
    inventory
        .validate_with_deadline(deadline)
        .map_err(GerberJobParseError::Canonical)?;
    // Portable Job identity is intentionally ASCII-only: unsupported Unicode normalization
    // forms fail closed instead of comparing differently across filesystems.
    if !input.virtual_path.is_ascii()
        || inventory
            .outcomes
            .iter()
            .any(|outcome| !outcome.virtual_path.is_ascii())
    {
        return Err(GerberJobParseError::Invalid {
            reason: "non-portable-job-path".into(),
        });
    }
    check_xnc_deadline(deadline, "job-json").map_err(|_| GerberJobParseError::Deadline)?;
    let reader = GerberDeadlineReader {
        cursor: Cursor::new(input.original_bytes.as_slice()),
        deadline,
    };
    let mut deserializer =
        serde_json::Deserializer::from_reader(BufReader::with_capacity(4096, reader));
    let value = UniqueJson::deserialize(&mut deserializer)
        .map_err(|error| {
            if deadline.check("job-json").is_err() {
                GerberJobParseError::Deadline
            } else {
                GerberJobParseError::Invalid {
                    reason: format!("invalid-job-json:{error}"),
                }
            }
        })?
        .0;
    deserializer.end().map_err(|error| {
        if deadline.check("job-json").is_err() {
            GerberJobParseError::Deadline
        } else {
            GerberJobParseError::Invalid {
                reason: format!("trailing-job-json:{error}"),
            }
        }
    })?;
    let mut metrics = JsonMetrics::default();
    measure_json(&value, 0, &mut metrics, deadline)?;
    let root = value
        .as_object()
        .ok_or_else(|| GerberJobParseError::Invalid {
            reason: "job-root-not-object".into(),
        })?;
    let job_document_id = document_id(&input.artifact_digest, DocumentFormat::GerberJob)
        .map_err(GerberJobParseError::Canonical)?;
    let mut unsupported_fields = root
        .keys()
        .filter(|key| !matches!(key.as_str(), "Header" | "GeneralSpecs" | "FilesAttributes"))
        .cloned()
        .collect::<Vec<_>>();
    let header = root
        .get("Header")
        .and_then(Value::as_object)
        .ok_or_else(|| GerberJobParseError::Invalid {
            reason: "missing-or-invalid-header".into(),
        })?;
    unsupported_fields.extend(
        header
            .keys()
            .filter(|key| key.as_str() != "GenerationSoftware")
            .map(|key| format!("Header.{key}")),
    );
    let generation = header
        .get("GenerationSoftware")
        .and_then(Value::as_object)
        .ok_or_else(|| GerberJobParseError::Invalid {
            reason: "missing-or-invalid-generation-software".into(),
        })?;
    unsupported_fields.extend(
        generation
            .keys()
            .filter(|key| !matches!(key.as_str(), "Vendor" | "Application" | "Version"))
            .map(|key| format!("Header.GenerationSoftware.{key}")),
    );
    for field in ["Vendor", "Application", "Version"] {
        if generation
            .get(field)
            .and_then(Value::as_str)
            .is_none_or(|value| value.trim().is_empty())
        {
            return Err(GerberJobParseError::Invalid {
                reason: format!("invalid-generation-software-{field}"),
            });
        }
    }
    let general = root
        .get("GeneralSpecs")
        .and_then(Value::as_object)
        .ok_or_else(|| GerberJobParseError::Invalid {
            reason: "missing-or-invalid-general-specs".into(),
        })?;
    unsupported_fields.extend(
        general
            .keys()
            .filter(|key| key.as_str() != "ProjectId")
            .map(|key| format!("GeneralSpecs.{key}")),
    );
    let project = general
        .get("ProjectId")
        .and_then(Value::as_object)
        .ok_or_else(|| GerberJobParseError::Invalid {
            reason: "missing-or-invalid-project-id".into(),
        })?;
    unsupported_fields.extend(
        project
            .keys()
            .filter(|key| !matches!(key.as_str(), "Name" | "Revision" | "PartNumber"))
            .map(|key| format!("GeneralSpecs.ProjectId.{key}")),
    );
    let identity = |field: &str| -> Result<Option<String>, GerberJobParseError> {
        match project.get(field) {
            None => Ok(None),
            Some(Value::String(value)) if !value.trim().is_empty() => Ok(Some(value.clone())),
            _ => Err(GerberJobParseError::Invalid {
                reason: format!("invalid-project-id-{field}"),
            }),
        }
    };
    let product_provenance =
        job_provenance(&job_document_id, &input.artifact_digest, input.size, 0);
    let product = ProductIdentity {
        name: identity("Name")?,
        revision: identity("Revision")?,
        part_number: identity("PartNumber")?,
        authority: Authority::Explicit,
        provenance: vec![product_provenance.clone()],
    };
    if product.name.is_none() && product.revision.is_none() && product.part_number.is_none() {
        return Err(GerberJobParseError::Invalid {
            reason: "empty-project-identity".into(),
        });
    }
    let product = Some(product);
    let files = root
        .get("FilesAttributes")
        .and_then(Value::as_array)
        .ok_or_else(|| GerberJobParseError::Invalid {
            reason: "missing-files-attributes".into(),
        })?;
    if files.is_empty() || files.len() > MANUFACTURING_LIMITS.recognized_files {
        return Err(GerberJobParseError::Resource {
            resource: "job-file-references",
            observed: files.len() as u64,
            limit: MANUFACTURING_LIMITS.recognized_files as u64,
        });
    }
    let mut inventory_paths = BTreeMap::<String, &ManufacturingInputOutcome>::new();
    for outcome in &inventory.outcomes {
        let folded = outcome.virtual_path.to_ascii_lowercase();
        if inventory_paths.insert(folded, outcome).is_some() {
            return Err(GerberJobParseError::Invalid {
                reason: "case-conflicting-inventory-path".into(),
            });
        }
    }
    let mut seen = BTreeSet::new();
    let mut references = Vec::new();
    for (index, entry) in files.iter().enumerate() {
        check_xnc_deadline(deadline, "job-references")
            .map_err(|_| GerberJobParseError::Deadline)?;
        let object = entry
            .as_object()
            .ok_or_else(|| GerberJobParseError::Invalid {
                reason: "job-file-reference-not-object".into(),
            })?;
        unsupported_fields.extend(
            object
                .keys()
                .filter(|key| !matches!(key.as_str(), "Path" | "FileFunction"))
                .map(|key| format!("FilesAttributes[{index}].{key}")),
        );
        let reference = object.get("Path").and_then(Value::as_str).ok_or_else(|| {
            GerberJobParseError::Invalid {
                reason: "job-reference-without-path".into(),
            }
        })?;
        let virtual_path = resolve_job_path(&input.virtual_path, reference).ok_or_else(|| {
            GerberJobParseError::Invalid {
                reason: format!("unsafe-job-reference:{reference}"),
            }
        })?;
        if !seen.insert(virtual_path.to_ascii_lowercase()) {
            return Err(GerberJobParseError::Invalid {
                reason: format!("duplicate-job-reference:{virtual_path}"),
            });
        }
        let outcome = inventory_paths
            .get(&virtual_path.to_ascii_lowercase())
            .copied()
            .ok_or_else(|| GerberJobParseError::Invalid {
                reason: format!("dangling-job-reference:{virtual_path}"),
            })?;
        if outcome.virtual_path != virtual_path
            || outcome.state != ManufacturingLoadState::Retained
            || outcome.kind_candidate == ManufacturingKindCandidate::GerberJob
        {
            return Err(GerberJobParseError::Invalid {
                reason: format!("wrong-kind-or-case-job-reference:{virtual_path}"),
            });
        }
        let raw_function = object
            .get("FileFunction")
            .and_then(Value::as_str)
            .ok_or_else(|| GerberJobParseError::Invalid {
                reason: format!("job-reference-without-file-function:{virtual_path}"),
            })?;
        if raw_function.starts_with("TF.FileFunction,") {
            return Err(GerberJobParseError::Invalid {
                reason: format!("prefixed-job-file-function:{virtual_path}"),
            });
        }
        let provenance = job_provenance(
            &job_document_id,
            &input.artifact_digest,
            input.size,
            index as u64 + 1,
        );
        let function = package_file_function(&X2Attribute {
            name: "TF.FileFunction".into(),
            values: raw_function.split(',').map(str::to_owned).collect(),
            provenance: provenance.clone(),
        })
        .map_err(GerberJobParseError::Canonical)?;
        let format = match outcome.kind_candidate {
            ManufacturingKindCandidate::Gerber => DocumentFormat::Gerber,
            ManufacturingKindCandidate::Excellon => DocumentFormat::Excellon,
            ManufacturingKindCandidate::GerberJob => {
                return Err(GerberJobParseError::Invalid {
                    reason: format!("recursive-job-reference:{virtual_path}"),
                });
            }
        };
        let compatible = match format {
            DocumentFormat::Gerber => matches!(
                function.role,
                LayerRole::Copper
                    | LayerRole::SolderMask
                    | LayerRole::Paste
                    | LayerRole::Legend
                    | LayerRole::Profile
                    | LayerRole::Route
            ),
            DocumentFormat::Excellon => {
                matches!(function.role, LayerRole::DrillMap | LayerRole::Route)
            }
            _ => false,
        };
        if !compatible {
            return Err(GerberJobParseError::Invalid {
                reason: format!("incompatible-job-file-function:{virtual_path}"),
            });
        }
        let artifact_digest =
            outcome
                .artifact_digest
                .as_deref()
                .ok_or_else(|| GerberJobParseError::Invalid {
                    reason: format!("job-reference-without-digest:{virtual_path}"),
                })?;
        references.push(GerberJobReference {
            virtual_path,
            document_id: document_id(artifact_digest, format)
                .map_err(GerberJobParseError::Canonical)?,
            file_function: function,
            provenance,
        });
    }
    unsupported_fields.sort();
    unsupported_fields.dedup();
    let mut max_line_bytes = 0;
    for line in input.original_bytes.split(|byte| *byte == b'\n') {
        check_xnc_deadline(deadline, "job-line-scan").map_err(|_| GerberJobParseError::Deadline)?;
        max_line_bytes = max_line_bytes.max(line.strip_suffix(b"\r").unwrap_or(line).len());
    }
    if max_line_bytes > MANUFACTURING_LIMITS.max_line_bytes {
        return Err(GerberJobParseError::Resource {
            resource: "job-line-bytes",
            observed: max_line_bytes as u64,
            limit: MANUFACTURING_LIMITS.max_line_bytes as u64,
        });
    }
    check_xnc_deadline(deadline, "job-canonicalization")
        .map_err(|_| GerberJobParseError::Deadline)?;
    Ok(GerberJobProduction {
        document: ManufacturingDocument {
            id: job_document_id,
            virtual_path: input.virtual_path.clone(),
            artifact_digest: input.artifact_digest.clone(),
            format: DocumentFormat::GerberJob,
            adapter: "ratemypcb-gerber-job".into(),
            adapter_version: GERBER_JOB_ADAPTER_VERSION.into(),
            parse_status: if unsupported_fields.is_empty() {
                ParseStatus::Complete
            } else {
                ParseStatus::Partial
            },
            numeric_format: None,
            metrics: DocumentMetrics {
                raw_bytes: input.size,
                records: metrics.nodes,
                lexical_tokens: metrics.nodes,
                metadata_bytes: metrics.text_bytes,
                max_line_bytes,
                max_text_bytes: metrics.max_text_bytes,
                max_nesting: metrics.max_depth,
                ..DocumentMetrics::default()
            },
        },
        product,
        references,
        unsupported_fields,
    })
}

#[derive(Debug)]
pub enum PackageParseError {
    Input { path: String, reason: String },
    Canonical(FabricationError),
    Deadline,
}

impl std::fmt::Display for PackageParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for PackageParseError {}

fn same_function(left: &PackageFileFunction, right: &PackageFileFunction) -> bool {
    left.role == right.role
        && left.side == right.side
        && left.order == right.order
        && left.plating == right.plating
        && left.from_layer == right.from_layer
        && left.to_layer == right.to_layer
        && left.qualifier == right.qualifier
        && left.operation == right.operation
}

fn job_file_function_fact_id(fact: &JobFileFunctionFact) -> String {
    stable_id(
        "job-file-function",
        &(
            &fact.job_document_id,
            &fact.job_artifact_digest,
            &fact.referenced_virtual_path,
            &fact.referenced_document_id,
            &fact.referenced_artifact_digest,
            &fact.fields,
            fact.role,
            fact.side,
            fact.order,
            fact.plating,
            fact.from_layer,
            fact.to_layer,
            &fact.qualifier,
            &fact.operation,
            &fact.provenance.location,
        ),
    )
    .expect("Job FileFunction identity serializes")
}

fn job_file_function_fact_id_with_deadline(
    fact: &JobFileFunctionFact,
    deadline: ManufacturingDeadline,
) -> Result<String, FabricationError> {
    stable_id_with_deadline(
        deadline,
        "job-file-function-identity",
        "job-file-function",
        &(
            &fact.job_document_id,
            &fact.job_artifact_digest,
            &fact.referenced_virtual_path,
            &fact.referenced_document_id,
            &fact.referenced_artifact_digest,
            &fact.fields,
            fact.role,
            fact.side,
            fact.order,
            fact.plating,
            fact.from_layer,
            fact.to_layer,
            &fact.qualifier,
            &fact.operation,
            &fact.provenance.location,
        ),
    )
}

fn retained_job_file_function(
    job: &ManufacturingDocument,
    referenced: &ManufacturingDocument,
    reference: &GerberJobReference,
) -> JobFileFunctionFact {
    let function = &reference.file_function;
    let mut fact = JobFileFunctionFact {
        id: String::new(),
        job_document_id: job.id.clone(),
        job_artifact_digest: job.artifact_digest.clone(),
        referenced_virtual_path: reference.virtual_path.clone(),
        referenced_document_id: referenced.id.clone(),
        referenced_artifact_digest: referenced.artifact_digest.clone(),
        fields: function
            .raw
            .strip_prefix("TF.FileFunction,")
            .expect("canonical FileFunction prefix")
            .split(',')
            .map(str::to_owned)
            .collect(),
        role: function.role,
        side: function.side,
        order: function.order,
        plating: function.plating,
        from_layer: function.from_layer,
        to_layer: function.to_layer,
        qualifier: function.qualifier.clone(),
        operation: function.operation.clone(),
        omission: None,
        conflict_ids: Vec::new(),
        provenance: reference.provenance.clone(),
    };
    fact.id = job_file_function_fact_id(&fact);
    fact
}

fn integration_outcome_id(outcome: &IntegratedReconciliationOutcome) -> String {
    stable_id(
        "integration-outcome",
        &(
            outcome.state,
            &outcome.attempted_native_path,
            &outcome.attempted_native_digest,
            &outcome.reason,
        ),
    )
    .expect("integration outcome identity serializes")
}

fn append_review(target: &mut FabricationReview, mut source: FabricationReview) {
    target.documents.append(&mut source.documents);
    target.layers.append(&mut source.layers);
    target.tools.append(&mut source.tools);
    target.apertures.append(&mut source.apertures);
    target.macros.append(&mut source.macros);
    target.blocks.append(&mut source.blocks);
    target.repetitions.append(&mut source.repetitions);
    target.features.append(&mut source.features);
    target.physical_bounds.append(&mut source.physical_bounds);
    target.connectivity.append(&mut source.connectivity);
    target
        .pad_hole_associations
        .append(&mut source.pad_hole_associations);
    target.x2_attributes.append(&mut source.x2_attributes);
    target
        .job_file_functions
        .append(&mut source.job_file_functions);
    target
        .assembly
        .placements
        .append(&mut source.assembly.placements);
    target
        .assembly
        .mask_layer_ids
        .append(&mut source.assembly.mask_layer_ids);
    target
        .assembly
        .paste_layer_ids
        .append(&mut source.assembly.paste_layer_ids);
    target
        .construction
        .layers
        .append(&mut source.construction.layers);
    target.constraints.append(&mut source.constraints);
    target.omissions.append(&mut source.omissions);
    target.conflicts.append(&mut source.conflicts);
    target.warnings.append(&mut source.warnings);
}

fn aggregate_capability(
    id: CapabilityId,
    state: CapabilityState,
    authority: Authority,
    documents: &[&ManufacturingDocument],
    provenance: &[ManufacturingProvenance],
    detail: &str,
) -> CapabilityRecord {
    CapabilityRecord {
        id,
        state,
        authority,
        document_ids: if state == CapabilityState::NotProvided {
            Vec::new()
        } else {
            documents
                .iter()
                .map(|document| document.id.clone())
                .collect()
        },
        provenance: if state == CapabilityState::NotProvided {
            Vec::new()
        } else {
            provenance.to_vec()
        },
        detail: detail.into(),
    }
}

fn polygon_cross(a: CanonicalPoint, b: CanonicalPoint, c: CanonicalPoint) -> i128 {
    (i128::from(b.x.0) - i128::from(a.x.0)) * (i128::from(c.y.0) - i128::from(a.y.0))
        - (i128::from(b.y.0) - i128::from(a.y.0)) * (i128::from(c.x.0) - i128::from(a.x.0))
}

fn point_on_segment(point: CanonicalPoint, start: CanonicalPoint, end: CanonicalPoint) -> bool {
    polygon_cross(start, end, point) == 0
        && point.x.0 >= start.x.0.min(end.x.0)
        && point.x.0 <= start.x.0.max(end.x.0)
        && point.y.0 >= start.y.0.min(end.y.0)
        && point.y.0 <= start.y.0.max(end.y.0)
}

fn polygon_segments_intersect(
    a: CanonicalPoint,
    b: CanonicalPoint,
    c: CanonicalPoint,
    d: CanonicalPoint,
) -> bool {
    let crosses = [
        polygon_cross(a, b, c),
        polygon_cross(a, b, d),
        polygon_cross(c, d, a),
        polygon_cross(c, d, b),
    ];
    (crosses[0].signum() != crosses[1].signum() && crosses[2].signum() != crosses[3].signum())
        || (crosses[0] == 0 && point_on_segment(c, a, b))
        || (crosses[1] == 0 && point_on_segment(d, a, b))
        || (crosses[2] == 0 && point_on_segment(a, c, d))
        || (crosses[3] == 0 && point_on_segment(b, c, d))
}

fn profile_polygon(
    feature: &ManufacturingFeature,
    deadline: ManufacturingDeadline,
) -> Result<Option<Vec<CanonicalPoint>>, PackageParseError> {
    deadline
        .check("package-profile-topology")
        .map_err(|_| PackageParseError::Deadline)?;
    if feature.polarity != LayerPolarity::Dark {
        return Ok(None);
    }
    let contour = match &feature.geometry {
        Geometry::Contour(contour) => contour,
        Geometry::Region(region) if region.contours.len() == 1 => &region.contours[0],
        _ => return Ok(None),
    };
    if !contour.closed || contour.segments.len() < 3 {
        return Ok(None);
    }
    let mut points = Vec::with_capacity(contour.segments.len());
    let mut expected = None;
    for segment in &contour.segments {
        deadline
            .check("package-profile-topology")
            .map_err(|_| PackageParseError::Deadline)?;
        let ContourSegment::Line(line) = segment else {
            return Ok(None);
        };
        let Ok(start) = feature.transforms.materialize(line.start) else {
            return Ok(None);
        };
        let Ok(end) = feature.transforms.materialize(line.end) else {
            return Ok(None);
        };
        if start.point == end.point || expected.is_some_and(|expected| expected != start.point) {
            return Ok(None);
        }
        points.push(start.point);
        expected = Some(end.point);
    }
    if expected != points.first().copied() || points.len() != 4 {
        return Ok(None);
    }
    for index in 0..points.len() {
        deadline
            .check("package-profile-topology")
            .map_err(|_| PackageParseError::Deadline)?;
        let next = points[(index + 1) % points.len()];
        if points[index].x != next.x && points[index].y != next.y {
            return Ok(None);
        }
    }
    let (Some(min_x), Some(min_y), Some(max_x), Some(max_y)) = (
        points.iter().map(|point| point.x).min(),
        points.iter().map(|point| point.y).min(),
        points.iter().map(|point| point.x).max(),
        points.iter().map(|point| point.y).max(),
    ) else {
        return Ok(None);
    };
    if min_x == max_x
        || min_y == max_y
        || points.iter().copied().collect::<BTreeSet<_>>()
            != [
                CanonicalPoint { x: min_x, y: min_y },
                CanonicalPoint { x: min_x, y: max_y },
                CanonicalPoint { x: max_x, y: min_y },
                CanonicalPoint { x: max_x, y: max_y },
            ]
            .into_iter()
            .collect()
    {
        return Ok(None);
    }
    let mut area = 0_i128;
    for index in 0..points.len() {
        deadline
            .check("package-profile-topology")
            .map_err(|_| PackageParseError::Deadline)?;
        let next = points[(index + 1) % points.len()];
        area += i128::from(points[index].x.0) * i128::from(next.y.0)
            - i128::from(points[index].y.0) * i128::from(next.x.0);
    }
    if area == 0 {
        return Ok(None);
    }
    for left in 0..points.len() {
        deadline
            .check("package-profile-topology")
            .map_err(|_| PackageParseError::Deadline)?;
        for right in (left + 1)..points.len() {
            deadline
                .check("package-profile-topology")
                .map_err(|_| PackageParseError::Deadline)?;
            if right == left + 1 || (left == 0 && right + 1 == points.len()) {
                continue;
            }
            if polygon_segments_intersect(
                points[left],
                points[(left + 1) % points.len()],
                points[right],
                points[(right + 1) % points.len()],
            ) {
                return Ok(None);
            }
        }
    }
    Ok(Some(points))
}

fn inferred_function(document: &ManufacturingDocument) -> Option<PackageFileFunction> {
    let name = document.virtual_path.to_ascii_lowercase();
    let (role, side, order) = if name.ends_with(".gtl") || name.contains("f_cu") {
        (LayerRole::Copper, LayerSide::Top, Some(1))
    } else if name.ends_with(".gbl") || name.contains("b_cu") {
        (LayerRole::Copper, LayerSide::Bottom, None)
    } else if name.ends_with(".gko")
        || name.ends_with(".gm1")
        || name.contains("profile")
        || name.contains("edge")
    {
        (LayerRole::Profile, LayerSide::NotApplicable, None)
    } else {
        return None;
    };
    Some(PackageFileFunction {
        raw: "filename-inference".into(),
        role,
        side,
        order,
        plating: Plating::Unknown,
        from_layer: None,
        to_layer: None,
        qualifier: None,
        operation: None,
        provenance: inventory_provenance(document),
    })
}

pub fn analyze_manufacturing_inventory(
    inventory: &ManufacturingInventory,
) -> Result<FabricationReview, PackageParseError> {
    analyze_manufacturing_inventory_with_deadline(
        inventory,
        ManufacturingDeadline::for_inventory(
            inventory,
            Duration::from_millis(MANUFACTURING_LIMITS.aggregate_timeout_ms),
        ),
    )
}

pub(crate) fn analyze_manufacturing_inventory_with_deadline(
    inventory: &ManufacturingInventory,
    deadline: ManufacturingDeadline,
) -> Result<FabricationReview, PackageParseError> {
    inventory
        .validate_with_deadline(deadline)
        .map_err(|error| match error {
            FabricationError::LimitExceeded { .. } => PackageParseError::Deadline,
            error => PackageParseError::Canonical(error),
        })?;
    deadline
        .check("manufacturing-aggregate")
        .map_err(|_| PackageParseError::Deadline)?;
    if inventory.inputs.is_empty() {
        return legacy_inventory_review_with_deadline(inventory, deadline)
            .map_err(PackageParseError::Canonical);
    }
    let mut input_index = BTreeMap::new();
    for input in &inventory.inputs {
        deadline
            .check("manufacturing-input-order")
            .map_err(|_| PackageParseError::Deadline)?;
        input_index.insert(input.virtual_path.as_str(), input);
    }
    let mut inputs = Vec::with_capacity(input_index.len());
    for input in input_index.into_values() {
        deadline
            .check("manufacturing-input-order")
            .map_err(|_| PackageParseError::Deadline)?;
        inputs.push(input);
    }
    let mut gerbers = Vec::new();
    let mut xnc = Vec::new();
    for input in &inputs {
        deadline
            .check("manufacturing-aggregate")
            .map_err(|_| PackageParseError::Deadline)?;
        let file_deadline = deadline.for_input(input);
        match input.kind_candidate {
            ManufacturingKindCandidate::Gerber => {
                let mut production = parse_gerber_document_with_deadline(input, file_deadline)
                    .map(|(production, _, _)| production)
                    .map_err(|error| PackageParseError::Input {
                        path: input.virtual_path.clone(),
                        reason: error.to_string(),
                    })?;
                apply_gerber_x2_with_deadline(&mut production, file_deadline)
                    .map_err(PackageParseError::Canonical)?;
                gerbers.push(production);
            }
            ManufacturingKindCandidate::Excellon => xnc.push(
                parse_xnc_document_with_deadline(input, file_deadline).map_err(|error| {
                    PackageParseError::Input {
                        path: input.virtual_path.clone(),
                        reason: error.to_string(),
                    }
                })?,
            ),
            ManufacturingKindCandidate::GerberJob => {}
        }
    }
    let mut jobs = Vec::new();
    for input in inputs
        .iter()
        .copied()
        .filter(|input| input.kind_candidate == ManufacturingKindCandidate::GerberJob)
    {
        deadline
            .check("manufacturing-aggregate")
            .map_err(|_| PackageParseError::Deadline)?;
        jobs.push(
            parse_gerber_job_document_with_deadline(input, inventory, deadline.for_input(input))
                .map_err(|error| PackageParseError::Input {
                    path: input.virtual_path.clone(),
                    reason: error.to_string(),
                })?,
        );
    }
    if jobs.len() > 1 {
        return Err(PackageParseError::Input {
            path: "manufacturing-inventory".into(),
            reason: "ambiguous-gerber-job-products".into(),
        });
    }

    let mut gerber_capabilities = Vec::with_capacity(gerbers.len());
    let mut gerber_functions = BTreeMap::new();
    for production in &gerbers {
        deadline
            .check("manufacturing-aggregation")
            .map_err(|_| PackageParseError::Deadline)?;
        gerber_capabilities.push(production.review.capabilities.clone());
        if let Some(function) = &production.file_function {
            gerber_functions.insert(production.review.documents[0].id.clone(), function.clone());
        }
    }
    let mut xnc_functions = BTreeMap::new();
    for production in &xnc {
        deadline
            .check("manufacturing-aggregation")
            .map_err(|_| PackageParseError::Deadline)?;
        if let Some(function) = &production.file_function {
            xnc_functions.insert(production.review.documents[0].id.clone(), function.clone());
        }
    }
    let mut outcome_index = BTreeMap::new();
    for outcome in &inventory.outcomes {
        deadline
            .check("manufacturing-outcome-order")
            .map_err(|_| PackageParseError::Deadline)?;
        outcome_index.insert(outcome.virtual_path.as_str(), outcome.clone());
    }
    let mut input_outcomes = Vec::with_capacity(outcome_index.len());
    for outcome in outcome_index.into_values() {
        deadline
            .check("manufacturing-outcome-order")
            .map_err(|_| PackageParseError::Deadline)?;
        input_outcomes.push(outcome);
    }
    let mut review =
        FabricationReview::empty_with_deadline(deadline).map_err(PackageParseError::Canonical)?;
    review.status = FabricationStatus::Partial;
    review.input_outcomes = input_outcomes;
    for production in gerbers {
        append_review(&mut review, production.review);
    }
    for production in xnc {
        append_review(&mut review, production.review);
    }
    let job = jobs.pop();
    if let Some(job) = &job {
        review.product = job.product.clone();
        review.documents.push(job.document.clone());
        for field in &job.unsupported_fields {
            let provenance = inventory_provenance(&job.document);
            review.omissions.push(Omission {
                id: stable_id(
                    "omission",
                    &("gerber-job-unsupported-field", field, &provenance.location),
                )
                .map_err(PackageParseError::Canonical)?,
                kind: OmissionKind::UnsupportedRecord,
                affected_capabilities: vec![CapabilityId::PackageCompleteness],
                provenance,
                detail: format!(
                    "Gerber Job field {field} is outside the exact supported 2023.06 subset."
                ),
            });
        }
    }

    if let Some(job) = &job {
        for reference in &job.references {
            deadline
                .check("manufacturing-job-authority")
                .map_err(|_| PackageParseError::Deadline)?;
            let mut referenced = None;
            for document in &review.documents {
                deadline
                    .check("manufacturing-job-authority")
                    .map_err(|_| PackageParseError::Deadline)?;
                if document.id == reference.document_id {
                    referenced = Some(document);
                    break;
                }
            }
            let referenced = referenced.ok_or_else(|| PackageParseError::Input {
                path: reference.virtual_path.clone(),
                reason: "dangling-retained-job-reference".into(),
            })?;
            review.job_file_functions.push(retained_job_file_function(
                &job.document,
                referenced,
                reference,
            ));
        }
        let mut ordered = BTreeMap::new();
        for fact in std::mem::take(&mut review.job_file_functions) {
            deadline
                .check("manufacturing-job-authority-order")
                .map_err(|_| PackageParseError::Deadline)?;
            ordered.insert(fact.referenced_document_id.clone(), fact);
        }
        for fact in ordered.into_values() {
            deadline
                .check("manufacturing-job-authority-order")
                .map_err(|_| PackageParseError::Deadline)?;
            review.job_file_functions.push(fact);
        }
    }

    let mut role_conflicts = Vec::new();
    let mut job_references = BTreeMap::new();
    if let Some(job) = &job {
        for reference in &job.references {
            deadline
                .check("manufacturing-aggregation")
                .map_err(|_| PackageParseError::Deadline)?;
            job_references.insert(reference.document_id.clone(), reference);
        }
    }
    let mut target_document_ids = Vec::new();
    for document in &review.documents {
        deadline
            .check("manufacturing-aggregation")
            .map_err(|_| PackageParseError::Deadline)?;
        if document.format != DocumentFormat::GerberJob {
            target_document_ids.push(document.id.clone());
        }
    }
    for document_id in &target_document_ids {
        deadline
            .check("manufacturing-aggregation")
            .map_err(|_| PackageParseError::Deadline)?;
        let supplied = gerber_functions
            .get(document_id)
            .or_else(|| xnc_functions.get(document_id));
        if let Some(reference) = job_references.get(document_id) {
            if let Some(supplied) =
                supplied.filter(|supplied| !same_function(supplied, &reference.file_function))
            {
                let conflict_id = stable_id(
                    "conflict",
                    &(
                        document_id,
                        "x2-job-file-function",
                        &supplied.provenance.location,
                        &reference.provenance.location,
                    ),
                )
                .map_err(PackageParseError::Canonical)?;
                let mut retained_fact = None;
                for fact in &mut review.job_file_functions {
                    deadline
                        .check("manufacturing-aggregation")
                        .map_err(|_| PackageParseError::Deadline)?;
                    if fact.referenced_document_id == *document_id {
                        retained_fact = Some(fact);
                        break;
                    }
                }
                retained_fact
                    .expect("parsed Job reference retained")
                    .conflict_ids
                    .push(conflict_id.clone());
                role_conflicts.push(Conflict {
                    id: conflict_id,
                    kind: ConflictKind::LayerRole,
                    affected_capabilities: vec![CapabilityId::LayerRoles],
                    left: ConflictFact {
                        canonical_value: supplied.raw.clone(),
                        authority: Authority::X2,
                        provenance: supplied.provenance.clone(),
                    },
                    right: ConflictFact {
                        canonical_value: reference.file_function.raw.clone(),
                        authority: Authority::Explicit,
                        provenance: reference.provenance.clone(),
                    },
                });
            } else {
                rekey_document_layer(
                    &mut review,
                    document_id,
                    &reference.file_function,
                    Authority::Explicit,
                    deadline,
                )
                .map_err(PackageParseError::Canonical)?;
            }
        } else if supplied.is_none() {
            let mut document = None;
            for candidate in &review.documents {
                deadline
                    .check("manufacturing-aggregation")
                    .map_err(|_| PackageParseError::Deadline)?;
                if candidate.id == *document_id {
                    document = Some(candidate);
                    break;
                }
            }
            if let Some(function) = document.and_then(inferred_function) {
                rekey_document_layer(
                    &mut review,
                    document_id,
                    &function,
                    Authority::FilenameInference,
                    deadline,
                )
                .map_err(PackageParseError::Canonical)?;
            }
        }
    }
    review.conflicts.extend(role_conflicts);

    let mut gerber_documents = Vec::new();
    let mut xnc_documents = Vec::new();
    let mut semantic_documents = Vec::new();
    let mut all_provenance = Vec::new();
    let mut gerber_provenance = Vec::new();
    let mut xnc_provenance = Vec::new();
    for document in &review.documents {
        deadline
            .check("manufacturing-aggregation")
            .map_err(|_| PackageParseError::Deadline)?;
        if document.format != DocumentFormat::GerberJob {
            semantic_documents.push(document);
            all_provenance.push(inventory_provenance(document));
        }
        if document.format == DocumentFormat::Gerber {
            gerber_documents.push(document);
            gerber_provenance.push(inventory_provenance(document));
        }
        if document.format == DocumentFormat::Excellon {
            xnc_documents.push(document);
            xnc_provenance.push(inventory_provenance(document));
        }
    }

    let mut mask_layer_ids = Vec::new();
    let mut paste_layer_ids = Vec::new();
    for layer in &review.layers {
        deadline
            .check("manufacturing-aggregation")
            .map_err(|_| PackageParseError::Deadline)?;
        if layer.role == LayerRole::SolderMask {
            mask_layer_ids.push(layer.id.clone());
        }
        if layer.role == LayerRole::Paste {
            paste_layer_ids.push(layer.id.clone());
        }
    }
    review.assembly.mask_layer_ids = mask_layer_ids;
    review.assembly.paste_layer_ids = paste_layer_ids;
    review.physical_bounds =
        derive_release_physical_bounds(&review, ReconciliationBudget { deadline })
            .map_err(PackageParseError::Canonical)?;
    let authoritative = derive_authoritative_states(
        &review,
        AuthoritativeReviewKind::Package,
        ReconciliationBudget { deadline },
    )
    .map_err(PackageParseError::Canonical)?;
    review.profile = authoritative.expected_profile.clone();
    let package_complete =
        authoritative.state(CapabilityId::PackageCompleteness) == CapabilityState::Complete;
    let state = |complete: bool, provided: bool| {
        if complete {
            CapabilityState::Complete
        } else if provided {
            CapabilityState::Partial
        } else {
            CapabilityState::NotProvided
        }
    };
    let mut x2_states = BTreeMap::new();
    for id in [
        CapabilityId::X2FileAttributes,
        CapabilityId::X2ApertureAttributes,
        CapabilityId::X2ObjectAttributes,
    ] {
        let mut any_state = false;
        let mut all_not_provided = true;
        let mut all_complete = true;
        for ledger in &gerber_capabilities {
            deadline
                .check("manufacturing-capability-aggregation")
                .map_err(|_| PackageParseError::Deadline)?;
            let mut state = CapabilityState::NotProvided;
            for record in &ledger.records {
                deadline
                    .check("manufacturing-capability-aggregation")
                    .map_err(|_| PackageParseError::Deadline)?;
                if record.id == id {
                    state = record.state;
                    break;
                }
            }
            any_state = true;
            all_not_provided &= state == CapabilityState::NotProvided;
            all_complete &= state == CapabilityState::Complete;
        }
        x2_states.insert(
            id,
            if !any_state || all_not_provided {
                CapabilityState::NotProvided
            } else if all_complete {
                CapabilityState::Complete
            } else {
                CapabilityState::Partial
            },
        );
    }
    let x2_state = |id| x2_states[&id];
    let units_complete = !semantic_documents.is_empty()
        && checked_all_with_deadline(
            &semantic_documents,
            deadline,
            "manufacturing-capability-aggregation",
            |document| document.numeric_format.is_some(),
        )
        .map_err(PackageParseError::Canonical)?;
    let routes_provided = checked_any_with_deadline(
        &review.features,
        deadline,
        "manufacturing-capability-aggregation",
        |feature| matches!(feature.geometry, Geometry::Route(_)),
    )
    .map_err(PackageParseError::Canonical)?;
    let slots_provided = checked_any_with_deadline(
        &review.features,
        deadline,
        "manufacturing-capability-aggregation",
        |feature| matches!(feature.geometry, Geometry::Slot(_)),
    )
    .map_err(PackageParseError::Canonical)?;
    let mut capabilities = vec![
        aggregate_capability(
            CapabilityId::ProductIdentity,
            authoritative.state(CapabilityId::ProductIdentity),
            Authority::Explicit,
            &semantic_documents,
            &all_provenance,
            "Gerber Job supplies explicit product identity when present.",
        ),
        aggregate_capability(
            CapabilityId::DocumentSyntax,
            CapabilityState::Complete,
            Authority::FileContent,
            &semantic_documents,
            &all_provenance,
            "Every retained Gerber, XNC, and Job document completed its bounded parser.",
        ),
        aggregate_capability(
            CapabilityId::UnitsAndFormat,
            state(units_complete, !semantic_documents.is_empty()),
            Authority::FileContent,
            &semantic_documents,
            &all_provenance,
            "Every geometry document has explicit units and numeric resolution.",
        ),
        aggregate_capability(
            CapabilityId::LayerRoles,
            authoritative.state(CapabilityId::LayerRoles),
            Authority::Explicit,
            &semantic_documents,
            &all_provenance,
            "Every intended document requires consistent typed X2/Job FileFunction authority.",
        ),
        aggregate_capability(
            CapabilityId::LayerOrder,
            authoritative.state(CapabilityId::LayerOrder),
            Authority::Explicit,
            &gerber_documents,
            &gerber_provenance,
            "Copper order must be unique, complete, and contiguous.",
        ),
        aggregate_capability(
            CapabilityId::Profile,
            authoritative.state(CapabilityId::Profile),
            Authority::Explicit,
            &gerber_documents,
            &gerber_provenance,
            "Exactly one explicit profile with retained geometry and extents is required.",
        ),
        aggregate_capability(
            CapabilityId::Extents,
            authoritative.state(CapabilityId::Extents),
            Authority::FileContent,
            &semantic_documents,
            &all_provenance,
            "Every package extent must fit the explicit profile at source resolution.",
        ),
        aggregate_capability(
            CapabilityId::Tools,
            authoritative.state(CapabilityId::Tools),
            Authority::Explicit,
            &xnc_documents,
            &xnc_provenance,
            "Every XNC tool is uniquely defined with finished diameter.",
        ),
        aggregate_capability(
            CapabilityId::Drills,
            authoritative.state(CapabilityId::Drills),
            Authority::Explicit,
            &xnc_documents,
            &xnc_provenance,
            "At least one bounded XNC drill hit is retained.",
        ),
        aggregate_capability(
            CapabilityId::Routes,
            state(routes_provided, !xnc_documents.is_empty()),
            Authority::Explicit,
            &xnc_documents,
            &xnc_provenance,
            "XNC routes remain exact when supplied.",
        ),
        aggregate_capability(
            CapabilityId::Slots,
            state(slots_provided, !xnc_documents.is_empty()),
            Authority::Explicit,
            &xnc_documents,
            &xnc_provenance,
            "XNC slots remain exact when supplied.",
        ),
        aggregate_capability(
            CapabilityId::Plating,
            authoritative.state(CapabilityId::Plating),
            Authority::Explicit,
            &xnc_documents,
            &xnc_provenance,
            "Every XNC document requires explicit plating.",
        ),
        aggregate_capability(
            CapabilityId::LayerSpans,
            authoritative.state(CapabilityId::LayerSpans),
            Authority::Explicit,
            &xnc_documents,
            &xnc_provenance,
            "Every XNC document requires explicit layer span.",
        ),
        aggregate_capability(
            CapabilityId::X2FileAttributes,
            x2_state(CapabilityId::X2FileAttributes),
            Authority::X2,
            &gerber_documents,
            &gerber_provenance,
            "Typed X2 FileFunction coverage is aggregated without filename authority.",
        ),
        aggregate_capability(
            CapabilityId::X2ApertureAttributes,
            x2_state(CapabilityId::X2ApertureAttributes),
            Authority::X2,
            &gerber_documents,
            &gerber_provenance,
            "Aperture attribute coverage is complete-only.",
        ),
        aggregate_capability(
            CapabilityId::X2ObjectAttributes,
            x2_state(CapabilityId::X2ObjectAttributes),
            Authority::X2,
            &gerber_documents,
            &gerber_provenance,
            "Object attribute coverage is complete-only.",
        ),
        aggregate_capability(
            CapabilityId::Connectivity,
            authoritative.state(CapabilityId::Connectivity),
            Authority::X2,
            &gerber_documents,
            &gerber_provenance,
            "Net coverage is complete only for every eligible feature.",
        ),
        aggregate_capability(
            CapabilityId::Components,
            authoritative.state(CapabilityId::Components),
            Authority::X2,
            &gerber_documents,
            &gerber_provenance,
            "Component coverage is complete only for every eligible feature.",
        ),
        aggregate_capability(
            CapabilityId::Pins,
            authoritative.state(CapabilityId::Pins),
            Authority::X2,
            &gerber_documents,
            &gerber_provenance,
            "Pin coverage is complete only for every eligible feature.",
        ),
        aggregate_capability(
            CapabilityId::PackageCompleteness,
            authoritative.state(CapabilityId::PackageCompleteness),
            Authority::Explicit,
            &semantic_documents,
            &all_provenance,
            "Product, roles/order, profile, drills, plating/span, extents, and claimed connectivity must all be complete.",
        ),
    ];
    for id in [
        CapabilityId::GeometryLines,
        CapabilityId::GeometryArcs,
        CapabilityId::GeometryRegions,
        CapabilityId::GeometryFlashes,
        CapabilityId::Polarity,
        CapabilityId::Transforms,
        CapabilityId::Repetition,
        CapabilityId::Apertures,
        CapabilityId::Macros,
        CapabilityId::GeometryExpanded,
    ] {
        let mut complete = !gerber_capabilities.is_empty();
        for ledger in &gerber_capabilities {
            deadline
                .check("manufacturing-capability-aggregation")
                .map_err(|_| PackageParseError::Deadline)?;
            let mut ledger_complete = false;
            for record in &ledger.records {
                deadline
                    .check("manufacturing-capability-aggregation")
                    .map_err(|_| PackageParseError::Deadline)?;
                ledger_complete |= record.id == id && record.state == CapabilityState::Complete;
            }
            complete &= ledger_complete;
        }
        capabilities.push(aggregate_capability(
            id,
            state(complete, !gerber_capabilities.is_empty()),
            Authority::FileContent,
            &gerber_documents,
            &gerber_provenance,
            "Aggregate Gerber semantic capability.",
        ));
    }
    capabilities.sort_by_key(|capability| capability.id);
    review.capabilities = CapabilityLedger {
        records: capabilities,
    };
    let mut incomplete_capabilities = BTreeSet::new();
    for capability in &review.capabilities.records {
        deadline
            .check("manufacturing-capability-aggregation")
            .map_err(|_| PackageParseError::Deadline)?;
        if capability.state != CapabilityState::Complete {
            incomplete_capabilities.insert(capability.id);
        }
    }
    checked_retain_with_deadline(
        &mut review.omissions,
        deadline,
        "manufacturing-capability-aggregation",
        |omission| {
            omission
                .affected_capabilities
                .iter()
                .all(|id| incomplete_capabilities.contains(id))
        },
    )
    .map_err(PackageParseError::Canonical)?;
    if !package_complete {
        let provenance =
            all_provenance
                .first()
                .cloned()
                .ok_or_else(|| PackageParseError::Input {
                    path: "manufacturing-inventory".into(),
                    reason: "package-has-no-semantic-document".into(),
                })?;
        review.omissions.push(Omission {
            id: stable_id("omission", &("package-completeness", &provenance.location))
                .map_err(PackageParseError::Canonical)?,
            kind: OmissionKind::MissingSemanticRecord,
            affected_capabilities: vec![CapabilityId::PackageCompleteness],
            provenance,
            detail: "One or more declared package prerequisites are incomplete.".into(),
        });
    }
    review.status = authoritative.status;
    review
        .finalize_trusted_with_deadline(deadline)
        .map_err(PackageParseError::Canonical)?;
    deadline
        .check("manufacturing-aggregate")
        .map_err(|_| PackageParseError::Deadline)?;
    Ok(review)
}
