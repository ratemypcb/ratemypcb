use super::*;
use serde_json::Value as JsonValue;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

pub const KICAD_MANUFACTURING_ADAPTER_VERSION: &str = "kicad-pcb-source-ratemypcb-1";

#[derive(Debug)]
pub enum NativeManufacturingError {
    Resource { resource: &'static str },
    Invalid { reason: &'static str },
    Canonical(FabricationError),
}

impl std::fmt::Display for NativeManufacturingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for NativeManufacturingError {}

#[derive(Clone, Debug)]
pub struct NativeManufacturing {
    pub review: FabricationReview,
    pub extents: Option<Extent>,
}

#[derive(Clone, Debug)]
struct NativeForm {
    start: usize,
    end: usize,
    name_start: usize,
    name_end: usize,
    parent: Option<usize>,
}

#[derive(Clone, Copy, Debug, Default)]
struct NativeMetrics {
    lexical_tokens: u64,
    metadata_bytes: u64,
    max_text_bytes: usize,
    max_numeric_bytes: usize,
    max_nesting: u8,
    decimal_digits: u8,
}

struct NativeSyntax<'a> {
    source: &'a str,
    forms: Vec<NativeForm>,
    children: Vec<Vec<usize>>,
    root: usize,
    metrics: NativeMetrics,
    deadline: ManufacturingDeadline,
}

fn deadline(deadline: ManufacturingDeadline) -> Result<(), NativeManufacturingError> {
    if Instant::now() >= deadline.at {
        Err(NativeManufacturingError::Resource {
            resource: "native-deadline",
        })
    } else {
        Ok(())
    }
}

fn note_atom(token: &str, metrics: &mut NativeMetrics) -> Result<(), NativeManufacturingError> {
    if token.is_empty() {
        return Ok(());
    }
    if token.len() > MANUFACTURING_LIMITS.max_text_bytes {
        return Err(NativeManufacturingError::Resource {
            resource: "native-atom-bytes",
        });
    }
    metrics.lexical_tokens =
        metrics
            .lexical_tokens
            .checked_add(1)
            .ok_or(NativeManufacturingError::Resource {
                resource: "native-lexical-tokens",
            })?;
    if metrics.lexical_tokens > MANUFACTURING_LIMITS.lexical_tokens_per_file {
        return Err(NativeManufacturingError::Resource {
            resource: "native-lexical-tokens",
        });
    }
    let unsigned = token.strip_prefix(['+', '-']).unwrap_or(token);
    let numeric = unsigned.as_bytes().first().is_some_and(u8::is_ascii_digit)
        && unsigned
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.');
    if !numeric && token.len() > MANUFACTURING_LIMITS.max_text_bytes {
        return Err(NativeManufacturingError::Resource {
            resource: "native-atom-bytes",
        });
    }
    if numeric {
        metrics.max_numeric_bytes = metrics.max_numeric_bytes.max(token.len());
        if token.len() > MANUFACTURING_LIMITS.max_numeric_bytes || unsigned.matches('.').count() > 1
        {
            return Err(NativeManufacturingError::Invalid {
                reason: "native-numeric-token",
            });
        }
        if let Some((_, fraction)) = unsigned.split_once('.') {
            if fraction.len() > usize::from(MANUFACTURING_LIMITS.max_decimal_places) {
                return Err(NativeManufacturingError::Invalid {
                    reason: "native-decimal-resolution",
                });
            }
            metrics.decimal_digits = metrics
                .decimal_digits
                .max(u8::try_from(fraction.len()).unwrap_or(u8::MAX));
        }
    }
    Ok(())
}

impl<'a> NativeSyntax<'a> {
    fn parse(
        source: &'a str,
        deadline_at: ManufacturingDeadline,
    ) -> Result<Self, NativeManufacturingError> {
        let bytes = source.as_bytes();
        let mut forms = Vec::<NativeForm>::new();
        let mut children = Vec::<Vec<usize>>::new();
        let mut stack = Vec::<usize>::new();
        let mut quoted = false;
        let mut escaped = false;
        let mut string_start = 0_usize;
        let mut atom_start = None;
        let mut metrics = NativeMetrics::default();

        for (index, byte) in bytes.iter().copied().enumerate() {
            if index & 0x0fff == 0 {
                deadline(deadline_at)?;
            }
            if quoted {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    let length = index.saturating_sub(string_start);
                    metrics.max_text_bytes = metrics.max_text_bytes.max(length);
                    metrics.metadata_bytes = metrics
                        .metadata_bytes
                        .checked_add(length as u64)
                        .ok_or(NativeManufacturingError::Resource {
                            resource: "native-metadata-bytes",
                        })?;
                    if length > MANUFACTURING_LIMITS.max_text_bytes
                        || metrics.metadata_bytes > MANUFACTURING_LIMITS.metadata_bytes_per_file
                    {
                        return Err(NativeManufacturingError::Resource {
                            resource: "native-metadata-bytes",
                        });
                    }
                    metrics.lexical_tokens = metrics.lexical_tokens.checked_add(1).ok_or(
                        NativeManufacturingError::Resource {
                            resource: "native-lexical-tokens",
                        },
                    )?;
                    if metrics.lexical_tokens > MANUFACTURING_LIMITS.lexical_tokens_per_file {
                        return Err(NativeManufacturingError::Resource {
                            resource: "native-lexical-tokens",
                        });
                    }
                    quoted = false;
                }
                continue;
            }

            match byte {
                b'"' => {
                    if let Some(start) = atom_start.take() {
                        note_atom(&source[start..index], &mut metrics)?;
                    }
                    quoted = true;
                    string_start = index + 1;
                }
                b'(' => {
                    if let Some(start) = atom_start.take() {
                        note_atom(&source[start..index], &mut metrics)?;
                    }
                    let mut name_start = index + 1;
                    while bytes
                        .get(name_start)
                        .is_some_and(|byte| byte.is_ascii_whitespace())
                    {
                        name_start += 1;
                    }
                    let mut name_end = name_start;
                    while bytes.get(name_end).is_some_and(|byte| {
                        !byte.is_ascii_whitespace() && !matches!(*byte, b'(' | b')' | b'"')
                    }) {
                        name_end += 1;
                    }
                    if name_start == name_end {
                        return Err(NativeManufacturingError::Invalid {
                            reason: "native-empty-form",
                        });
                    }
                    let parent = stack.last().copied();
                    let form_index = forms.len();
                    forms.push(NativeForm {
                        start: index,
                        end: 0,
                        name_start,
                        name_end,
                        parent,
                    });
                    children.push(Vec::new());
                    if let Some(parent) = parent {
                        children[parent].push(form_index);
                    }
                    stack.push(form_index);
                    metrics.max_nesting = metrics
                        .max_nesting
                        .max(u8::try_from(stack.len()).unwrap_or(u8::MAX));
                    if forms.len() as u64 > MANUFACTURING_LIMITS.records_per_file
                        || metrics.max_nesting > MANUFACTURING_LIMITS.max_nesting
                    {
                        return Err(NativeManufacturingError::Resource {
                            resource: "native-structure",
                        });
                    }
                }
                b')' => {
                    if let Some(start) = atom_start.take() {
                        note_atom(&source[start..index], &mut metrics)?;
                    }
                    let form = stack.pop().ok_or(NativeManufacturingError::Invalid {
                        reason: "native-unmatched-close",
                    })?;
                    forms[form].end = index + 1;
                }
                byte if byte.is_ascii_whitespace() => {
                    if let Some(start) = atom_start.take() {
                        note_atom(&source[start..index], &mut metrics)?;
                    }
                }
                _ => {
                    atom_start.get_or_insert(index);
                }
            };
        }
        if let Some(start) = atom_start {
            note_atom(&source[start..], &mut metrics)?;
        }
        if quoted || escaped || !stack.is_empty() || forms.is_empty() {
            return Err(NativeManufacturingError::Invalid {
                reason: "native-unclosed-structure",
            });
        }
        let mut roots = Vec::new();
        for (index, form) in forms.iter().enumerate() {
            if index & 0x0fff == 0 {
                deadline(deadline_at)?;
            }
            if form.parent.is_none() {
                roots.push(index);
            }
        }
        if roots.len() != 1 {
            return Err(NativeManufacturingError::Invalid {
                reason: "native-root-count",
            });
        }
        let root = roots[0];
        let prefix = source[..forms[root].start]
            .trim_matches(|character: char| character.is_whitespace() || character == '\u{feff}');
        let suffix = source[forms[root].end..].trim();
        let syntax = Self {
            source,
            forms,
            children,
            root,
            metrics,
            deadline: deadline_at,
        };
        if !prefix.is_empty() || !suffix.is_empty() || syntax.name(root) != "kicad_pcb" {
            return Err(NativeManufacturingError::Invalid {
                reason: "native-source-root",
            });
        }
        deadline(deadline_at)?;
        Ok(syntax)
    }

    fn name(&self, index: usize) -> &str {
        let form = &self.forms[index];
        &self.source[form.name_start..form.name_end]
    }

    fn children_named_checked(
        &self,
        parent: usize,
        name: &str,
    ) -> Result<Vec<usize>, NativeManufacturingError> {
        let mut matching = Vec::new();
        for (position, index) in self.children[parent].iter().copied().enumerate() {
            if position & 0x0fff == 0 {
                deadline(self.deadline)?;
            }
            if self.name(index) == name {
                matching.push(index);
            }
        }
        Ok(matching)
    }

    fn child(&self, parent: usize, name: &str) -> Result<Option<usize>, NativeManufacturingError> {
        Ok(self
            .children_named_checked(parent, name)?
            .into_iter()
            .next())
    }

    fn unique_child(
        &self,
        parent: usize,
        name: &str,
    ) -> Result<Option<usize>, NativeManufacturingError> {
        let mut first = None;
        for (position, index) in self.children[parent].iter().copied().enumerate() {
            if position & 0x0fff == 0 {
                deadline(self.deadline)?;
            }
            if self.name(index) != name {
                continue;
            }
            if first.replace(index).is_some() {
                return Err(NativeManufacturingError::Invalid {
                    reason: "duplicate-native-field",
                });
            }
        }
        Ok(first)
    }

    fn tokens(&self, index: usize) -> Result<Vec<String>, NativeManufacturingError> {
        fn push_token(
            tokens: &mut Vec<String>,
            current: &mut Vec<u8>,
        ) -> Result<(), NativeManufacturingError> {
            if !current.is_empty() {
                tokens.push(String::from_utf8(std::mem::take(current)).map_err(|_| {
                    NativeManufacturingError::Invalid {
                        reason: "native-token-utf8",
                    }
                })?);
            }
            Ok(())
        }

        let form = &self.forms[index];
        let bytes = &self.source.as_bytes()[form.name_end..form.end - 1];
        let mut tokens = Vec::new();
        let mut quoted = false;
        let mut escaped = false;
        let mut current = Vec::new();
        for (index, byte) in bytes.iter().copied().enumerate() {
            if index & 0x0fff == 0 {
                deadline(self.deadline)?;
            }
            if quoted {
                if escaped {
                    if !matches!(byte, b'\\' | b'"') {
                        return Err(NativeManufacturingError::Invalid {
                            reason: "unsupported-native-escape",
                        });
                    }
                    current.push(byte);
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    quoted = false;
                    push_token(&mut tokens, &mut current)?;
                } else {
                    current.push(byte);
                }
                continue;
            }
            match byte {
                b'"' => {
                    push_token(&mut tokens, &mut current)?;
                    quoted = true;
                }
                b'(' => break,
                byte if byte.is_ascii_whitespace() => {
                    push_token(&mut tokens, &mut current)?;
                }
                _ => current.push(byte),
            }
        }
        if quoted || escaped {
            return Err(NativeManufacturingError::Invalid {
                reason: "native-token-structure",
            });
        }
        push_token(&mut tokens, &mut current)?;
        Ok(tokens)
    }

    fn child_tokens(
        &self,
        parent: usize,
        name: &str,
    ) -> Result<Option<Vec<String>>, NativeManufacturingError> {
        self.unique_child(parent, name)?
            .map(|index| self.tokens(index))
            .transpose()
    }
}

fn provenance(
    syntax: &NativeSyntax<'_>,
    document_id: &str,
    digest: &str,
    form: usize,
) -> ManufacturingProvenance {
    let value = &syntax.forms[form];
    ManufacturingProvenance {
        document_id: document_id.into(),
        artifact_digest: digest.into(),
        producer: "ratemypcb-kicad-source".into(),
        producer_version: KICAD_MANUFACTURING_ADAPTER_VERSION.into(),
        location: StructuralLocation {
            record: form as u64,
            subrecord: None,
            byte_start: value.start as u64,
            byte_end: value.end.saturating_sub(1) as u64,
        },
        source_lexeme: None,
    }
}

fn parse_length(value: &str, reason: &'static str) -> Result<Picometres, NativeManufacturingError> {
    Picometres::parse_decimal(value, SourceUnit::Millimetre)
        .map_err(|_| NativeManufacturingError::Invalid { reason })
}

fn parse_angle(value: Option<&String>) -> Result<i64, NativeManufacturingError> {
    let Some(value) = value else { return Ok(0) };
    let (negative, value) = value
        .strip_prefix('-')
        .map_or((false, value.as_str()), |value| (true, value));
    let value = value.strip_prefix('+').unwrap_or(value);
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || fraction.len() > 6
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(NativeManufacturingError::Invalid {
            reason: "native-angle",
        });
    }
    let scale = 10_i128.pow(fraction.len() as u32);
    let fraction = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<i128>()
            .map_err(|_| NativeManufacturingError::Invalid {
                reason: "native-angle",
            })?
    };
    let mut degrees = whole
        .parse::<i128>()
        .ok()
        .and_then(|whole| whole.checked_mul(scale))
        .and_then(|whole| fraction.checked_add(whole))
        .ok_or(NativeManufacturingError::Invalid {
            reason: "native-angle",
        })?;
    if negative {
        degrees = -degrees;
    }
    let microdegrees = degrees
        .checked_mul(1_000_000)
        .and_then(|value| value.checked_div(scale))
        .and_then(|value| i64::try_from(value).ok())
        .ok_or(NativeManufacturingError::Invalid {
            reason: "native-angle",
        })?;
    if microdegrees.unsigned_abs() > 360_000_000_u64 * 1_000 {
        return Err(NativeManufacturingError::Invalid {
            reason: "native-angle",
        });
    }
    Ok(microdegrees)
}

fn pose(
    syntax: &NativeSyntax<'_>,
    parent: usize,
    field: &str,
) -> Result<(CanonicalPoint, i64), NativeManufacturingError> {
    let values = syntax
        .child_tokens(parent, field)?
        .ok_or(NativeManufacturingError::Invalid {
            reason: "missing-native-point",
        })?;
    if !(2..=3).contains(&values.len()) {
        return Err(NativeManufacturingError::Invalid {
            reason: "invalid-native-point",
        });
    }
    Ok((
        CanonicalPoint {
            x: parse_length(&values[0], "invalid-native-coordinate")?,
            y: parse_length(&values[1], "invalid-native-coordinate")?,
        },
        parse_angle(values.get(2))?,
    ))
}

fn point(
    syntax: &NativeSyntax<'_>,
    parent: usize,
    field: &str,
) -> Result<CanonicalPoint, NativeManufacturingError> {
    pose(syntax, parent, field).map(|value| value.0)
}

fn layer_names(
    syntax: &NativeSyntax<'_>,
    parent: usize,
) -> Result<Vec<String>, NativeManufacturingError> {
    syntax
        .child_tokens(parent, "layers")?
        .ok_or(NativeManufacturingError::Invalid {
            reason: "native-layers",
        })
}

fn layer_name(
    syntax: &NativeSyntax<'_>,
    parent: usize,
) -> Result<Option<String>, NativeManufacturingError> {
    Ok(syntax
        .child_tokens(parent, "layer")?
        .and_then(|values| values.into_iter().next()))
}

fn feature(
    document_id: &str,
    layer_id: &str,
    tool_id: Option<&str>,
    geometry: Geometry,
    transforms: TransformChain,
    provenance: ManufacturingProvenance,
) -> ManufacturingFeature {
    ManufacturingFeature {
        id: feature_id(document_id, layer_id, geometry.kind(), &provenance.location),
        document_id: document_id.into(),
        layer_id: layer_id.into(),
        tool_id: tool_id.map(str::to_owned),
        polarity: LayerPolarity::Unknown,
        geometry,
        transforms,
        membership: FeatureMembership::TopLevel,
        provenance,
    }
}

fn transforms(operations: impl IntoIterator<Item = TransformOperation>) -> TransformChain {
    TransformChain {
        operations: operations.into_iter().collect(),
    }
}

fn point_order(left: &CanonicalPoint, right: &CanonicalPoint) -> Ordering {
    (left.x, left.y).cmp(&(right.x, right.y))
}

fn extent(points: impl IntoIterator<Item = CanonicalPoint>) -> Option<Extent> {
    let mut bounds = GerberBounds::default();
    let mut count = 0_usize;
    for point in points {
        bounds.include(point);
        count += 1;
    }
    (count > 0).then(|| bounds.extent()).flatten()
}

fn extent_area(value: &Extent) -> i128 {
    i128::from(value.max.x.0 - value.min.x.0) * i128::from(value.max.y.0 - value.min.y.0)
}

struct ProfilePiece {
    feature: ManufacturingFeature,
    extent: Extent,
    provenance: ManufacturingProvenance,
}

fn line_profile_pieces(
    document_id: &str,
    layer_id: &str,
    lines: Vec<(CanonicalLine, ManufacturingProvenance)>,
    deadline_at: ManufacturingDeadline,
) -> Result<Vec<ProfilePiece>, NativeManufacturingError> {
    if lines.is_empty() {
        return Ok(Vec::new());
    }
    let mut adjacency = BTreeMap::<CanonicalPoint, Vec<usize>>::new();
    for (index, (line, _)) in lines.iter().enumerate() {
        if index & 0x0fff == 0 {
            deadline(deadline_at)?;
        }
        if line.start == line.end {
            return Err(NativeManufacturingError::Invalid {
                reason: "native-zero-profile-line",
            });
        }
        adjacency.entry(line.start).or_default().push(index);
        adjacency.entry(line.end).or_default().push(index);
    }
    if adjacency.len() < 3 {
        return Err(NativeManufacturingError::Invalid {
            reason: "native-open-profile",
        });
    }
    for segments in adjacency.values() {
        deadline(deadline_at)?;
        if segments.len() != 2 {
            return Err(NativeManufacturingError::Invalid {
                reason: "native-open-profile",
            });
        }
    }
    let mut unused = BTreeSet::new();
    for index in 0..lines.len() {
        deadline(deadline_at)?;
        unused.insert(index);
    }
    let mut output = Vec::new();
    while let Some(first) = unused.iter().next().copied() {
        deadline(deadline_at)?;
        let first_line = &lines[first].0;
        let start = if point_order(&first_line.start, &first_line.end).is_le() {
            first_line.start
        } else {
            first_line.end
        };
        let mut current = start;
        let mut segments = Vec::new();
        let provenance = lines[first].1.clone();
        loop {
            deadline(deadline_at)?;
            let mut next = None;
            if let Some(indices) = adjacency.get(&current) {
                for index in indices {
                    deadline(deadline_at)?;
                    if unused.contains(index) {
                        next = Some(*index);
                        break;
                    }
                }
            }
            let next = next.ok_or(NativeManufacturingError::Invalid {
                reason: "native-profile-topology",
            })?;
            unused.remove(&next);
            let line = &lines[next].0;
            let oriented = if line.start == current {
                line.clone()
            } else if line.end == current {
                CanonicalLine {
                    start: line.end,
                    end: line.start,
                    width: line.width,
                }
            } else {
                return Err(NativeManufacturingError::Invalid {
                    reason: "native-profile-topology",
                });
            };
            current = oriented.end;
            segments.push(ContourSegment::Line(oriented));
            if current == start {
                break;
            }
            if segments.len() > lines.len() {
                return Err(NativeManufacturingError::Invalid {
                    reason: "native-profile-cycle",
                });
            }
        }
        if segments.len() != 4
            || segments.iter().any(|segment| {
                !matches!(segment, ContourSegment::Line(line) if line.start.x == line.end.x || line.start.y == line.end.y)
            })
        {
            return Err(NativeManufacturingError::Invalid {
                reason: "unsupported-native-line-profile",
            });
        }
        let corners = segments
            .iter()
            .filter_map(|segment| match segment {
                ContourSegment::Line(line) => Some(line.start),
                ContourSegment::Arc(_) => None,
            })
            .collect::<BTreeSet<_>>();
        if corners.len() != 4 {
            return Err(NativeManufacturingError::Invalid {
                reason: "native-profile-cycle",
            });
        }
        let points = segments.iter().flat_map(|segment| match segment {
            ContourSegment::Line(line) => [line.start, line.end],
            ContourSegment::Arc(arc) => [arc.start, arc.end],
        });
        let piece_extent = extent(points).ok_or(NativeManufacturingError::Invalid {
            reason: "native-profile-extents",
        })?;
        let profile_feature = feature(
            document_id,
            layer_id,
            None,
            Geometry::Contour(CanonicalContour {
                segments,
                closed: true,
            }),
            TransformChain::default(),
            provenance.clone(),
        );
        output.push(ProfilePiece {
            feature: profile_feature,
            extent: piece_extent,
            provenance,
        });
    }
    Ok(output)
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct NativeToolKey {
    kind: ToolKind,
    diameter: Picometres,
    plating: Plating,
    from_layer_id: Option<String>,
    to_layer_id: Option<String>,
}

fn ensure_tool(
    tools: &mut Vec<ManufacturingTool>,
    identities: &mut BTreeMap<NativeToolKey, String>,
    document_id: &str,
    key: NativeToolKey,
    provenance: &ManufacturingProvenance,
) -> Result<String, NativeManufacturingError> {
    if let Some(id) = identities.get(&key) {
        return Ok(id.clone());
    }
    if tools.len() >= MANUFACTURING_LIMITS.apertures {
        return Err(NativeManufacturingError::Resource {
            resource: "native-tools",
        });
    }
    let code = format!("N{:04}", tools.len() + 1);
    let identity_kind = format!("{:?}:{code}", key.kind);
    let id = tool_id(document_id, &identity_kind, &provenance.location);
    tools.push(ManufacturingTool {
        id: id.clone(),
        document_id: document_id.into(),
        code,
        kind: key.kind,
        diameter: Some(key.diameter),
        plating: key.plating,
        span: Some(LayerSpan {
            from_layer_id: key.from_layer_id.clone(),
            to_layer_id: key.to_layer_id.clone(),
        }),
        provenance: provenance.clone(),
    });
    identities.insert(key, id.clone());
    Ok(id)
}

fn reference(
    syntax: &NativeSyntax<'_>,
    footprint: usize,
) -> Result<Option<String>, NativeManufacturingError> {
    let mut reference = None;
    for property in syntax.children_named_checked(footprint, "property")? {
        let values = syntax.tokens(property)?;
        if values.first().is_some_and(|value| value == "Reference") {
            let value = values.get(1).filter(|value| !value.is_empty()).cloned();
            if reference.replace(value).is_some() {
                return Err(NativeManufacturingError::Invalid {
                    reason: "duplicate-native-reference",
                });
            }
        }
    }
    for text in syntax.children_named_checked(footprint, "fp_text")? {
        let values = syntax.tokens(text)?;
        if values.first().is_some_and(|value| value == "reference") {
            let value = values.get(1).filter(|value| !value.is_empty()).cloned();
            if reference.replace(value).is_some() {
                return Err(NativeManufacturingError::Invalid {
                    reason: "duplicate-native-reference",
                });
            }
        }
    }
    Ok(reference.flatten())
}

fn copper_layer_names_valid(names: &[String], layer_ids: &BTreeMap<String, String>) -> bool {
    names
        .iter()
        .filter(|name| name.ends_with(".Cu"))
        .all(|name| matches!(name.as_str(), "*.Cu" | "F&B.Cu") || layer_ids.contains_key(name))
}

fn resolved_copper_layers(names: &[String], layer_ids: &BTreeMap<String, String>) -> Vec<String> {
    let mut output = Vec::new();
    for name in names {
        match name.as_str() {
            "*.Cu" | "F&B.Cu" => {
                for name in ["F.Cu", "B.Cu"] {
                    if let Some(id) = layer_ids.get(name)
                        && !output.contains(id)
                    {
                        output.push(id.clone());
                    }
                }
            }
            _ if name.ends_with(".Cu") => {
                if let Some(id) = layer_ids.get(name)
                    && !output.contains(id)
                {
                    output.push(id.clone());
                }
            }
            _ => {}
        }
    }
    output
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AuthoritativeReviewKind {
    Package,
    Native,
}

const RECONCILIATION_PREREQUISITES: [CapabilityId; 12] = [
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

#[derive(Debug)]
pub(super) struct AuthoritativeDerivation {
    states: BTreeMap<CapabilityId, CapabilityState>,
    pub(super) expected_profile: Option<BoardProfile>,
    pub(super) status: FabricationStatus,
}

impl AuthoritativeDerivation {
    pub(super) fn state(&self, id: CapabilityId) -> CapabilityState {
        self.states
            .get(&id)
            .copied()
            .unwrap_or(CapabilityState::NotProvided)
    }
}

struct AuthoritativeIndex<'a> {
    formats: BTreeMap<&'a str, DocumentFormat>,
    layers_by_document: BTreeMap<&'a str, Vec<&'a ManufacturingLayer>>,
    layers_by_id: BTreeMap<&'a str, &'a ManufacturingLayer>,
    tools_by_document: BTreeMap<&'a str, Vec<&'a ManufacturingTool>>,
    features_by_document: BTreeMap<&'a str, Vec<&'a ManufacturingFeature>>,
    features_by_id: BTreeMap<&'a str, &'a ManufacturingFeature>,
    connectivity_by_feature: BTreeMap<&'a str, Vec<&'a ObjectSemantics>>,
    file_functions_by_document: BTreeMap<&'a str, Vec<PackageFileFunction>>,
    job_functions_by_document: BTreeMap<&'a str, Vec<&'a JobFileFunctionFact>>,
    physical_bounds_by_document: BTreeMap<&'a str, Vec<&'a DocumentPhysicalBounds>>,
    object_attribute_documents: BTreeSet<&'a str>,
    blocked: BTreeSet<CapabilityId>,
}

impl<'a> AuthoritativeIndex<'a> {
    fn build(
        review: &'a FabricationReview,
        budget: ReconciliationBudget,
    ) -> Result<Self, FabricationError> {
        budget.check()?;
        let mut index = Self {
            formats: BTreeMap::new(),
            layers_by_document: BTreeMap::new(),
            layers_by_id: BTreeMap::new(),
            tools_by_document: BTreeMap::new(),
            features_by_document: BTreeMap::new(),
            features_by_id: BTreeMap::new(),
            connectivity_by_feature: BTreeMap::new(),
            file_functions_by_document: BTreeMap::new(),
            job_functions_by_document: BTreeMap::new(),
            physical_bounds_by_document: BTreeMap::new(),
            object_attribute_documents: BTreeSet::new(),
            blocked: BTreeSet::new(),
        };
        for document in &review.documents {
            budget.check()?;
            index.formats.insert(document.id.as_str(), document.format);
        }
        for layer in &review.layers {
            budget.check()?;
            index
                .layers_by_document
                .entry(layer.document_id.as_str())
                .or_default()
                .push(layer);
            index.layers_by_id.insert(layer.id.as_str(), layer);
        }
        for tool in &review.tools {
            budget.check()?;
            index
                .tools_by_document
                .entry(tool.document_id.as_str())
                .or_default()
                .push(tool);
        }
        for feature in &review.features {
            budget.check()?;
            index
                .features_by_document
                .entry(feature.document_id.as_str())
                .or_default()
                .push(feature);
            index.features_by_id.insert(feature.id.as_str(), feature);
        }
        for item in &review.connectivity {
            budget.check()?;
            index
                .connectivity_by_feature
                .entry(item.feature_id.as_str())
                .or_default()
                .push(item);
        }
        for attribute in &review.x2_attributes {
            budget.check()?;
            if attribute.scope == X2AttributeScope::File
                && attribute.kind == X2AttributeKind::FileFunction
                && !attribute.deletion
            {
                let function = package_file_function(&X2Attribute {
                    name: "TF.FileFunction".into(),
                    values: attribute.values.clone(),
                    provenance: attribute.provenance.clone(),
                })?;
                index
                    .file_functions_by_document
                    .entry(attribute.document_id.as_str())
                    .or_default()
                    .push(function);
            }
            if attribute.scope == X2AttributeScope::Object && !attribute.deletion {
                index
                    .object_attribute_documents
                    .insert(attribute.document_id.as_str());
            }
        }
        for fact in &review.job_file_functions {
            budget.check()?;
            index
                .job_functions_by_document
                .entry(fact.referenced_document_id.as_str())
                .or_default()
                .push(fact);
        }
        for bounds in &review.physical_bounds {
            budget.check()?;
            index
                .physical_bounds_by_document
                .entry(bounds.document_id.as_str())
                .or_default()
                .push(bounds);
        }
        for omission in &review.omissions {
            budget.check()?;
            index
                .blocked
                .extend(omission.affected_capabilities.iter().copied());
        }
        for conflict in &review.conflicts {
            budget.check()?;
            index
                .blocked
                .extend(conflict.affected_capabilities.iter().copied());
        }
        Ok(index)
    }

    fn format(&self, document_id: &str) -> Option<DocumentFormat> {
        self.formats.get(document_id).copied()
    }

    fn is_blocked(&self, id: CapabilityId) -> bool {
        self.blocked.contains(&id)
    }
}

fn derived_state(complete: bool, provided: bool, blocked: bool) -> CapabilityState {
    if complete && !blocked {
        CapabilityState::Complete
    } else if provided {
        CapabilityState::Partial
    } else {
        CapabilityState::NotProvided
    }
}

fn extent_contains_at_resolution(outer: &Extent, inner: &Extent, resolution: Picometres) -> bool {
    resolution.0 > 0
        && inner.min.x >= outer.min.x
        && inner.min.y >= outer.min.y
        && inner.max.x <= outer.max.x
        && inner.max.y <= outer.max.y
}

fn package_profile_from_index(
    review: &FabricationReview,
    index: &AuthoritativeIndex<'_>,
    budget: ReconciliationBudget,
) -> Result<Option<BoardProfile>, FabricationError> {
    let mut layers = Vec::new();
    for layer in &review.layers {
        budget.check()?;
        if index.format(&layer.document_id) == Some(DocumentFormat::Gerber)
            && layer.role == LayerRole::Profile
            && layer.authority == Authority::Explicit
        {
            layers.push(layer);
        }
    }
    if layers.len() != 1 {
        return Ok(None);
    }
    let layer = layers[0];
    let features = index
        .features_by_document
        .get(layer.document_id.as_str())
        .map(Vec::as_slice)
        .unwrap_or_default();
    if features.len() != 1 {
        return Ok(None);
    }
    let polygon = match profile_polygon(features[0], budget.deadline) {
        Ok(Some(polygon)) => polygon,
        Ok(None) => return Ok(None),
        Err(PackageParseError::Deadline) => {
            return Err(FabricationError::LimitExceeded {
                resource: "reconciliation-deadline",
            });
        }
        Err(PackageParseError::Canonical(error)) => return Err(error),
        Err(_) => return Err(FabricationError::InvalidIdentity("package-profile".into())),
    };
    budget.check()?;
    let extent = polygon.iter().fold(
        Extent {
            min: polygon[0],
            max: polygon[0],
        },
        |mut extent, point| {
            extent.min.x.0 = extent.min.x.0.min(point.x.0);
            extent.min.y.0 = extent.min.y.0.min(point.y.0);
            extent.max.x.0 = extent.max.x.0.max(point.x.0);
            extent.max.y.0 = extent.max.y.0.max(point.y.0);
            extent
        },
    );
    Ok(Some(BoardProfile {
        contour_feature_ids: vec![features[0].id.clone()],
        cutout_feature_ids: Vec::new(),
        extents: Some(extent),
        provenance: vec![layer.provenance.clone()],
    }))
}

fn native_profile_from_index(
    review: &FabricationReview,
    index: &AuthoritativeIndex<'_>,
    budget: ReconciliationBudget,
) -> Result<Option<BoardProfile>, FabricationError> {
    let mut profile_layers = Vec::new();
    for layer in &review.layers {
        budget.check()?;
        if index.format(&layer.document_id) == Some(DocumentFormat::KicadPcb)
            && layer.role == LayerRole::Profile
        {
            profile_layers.push(layer);
        }
    }
    if profile_layers.is_empty() {
        return Ok(None);
    }
    if profile_layers.len() != 1 {
        return Ok(None);
    }
    let layer = profile_layers[0];
    if layer.name.as_deref() != Some("Edge.Cuts")
        || layer.side != LayerSide::NotApplicable
        || layer.order.is_some()
        || layer.authority != Authority::NativeSource
    {
        return Ok(None);
    }
    let mut pieces = Vec::new();
    for feature in index
        .features_by_document
        .get(layer.document_id.as_str())
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        budget.check()?;
        if feature.layer_id != layer.id {
            continue;
        }
        let Geometry::Contour(contour) = &feature.geometry else {
            return Ok(None);
        };
        if !contour.closed
            || contour.segments.len() != 4
            || contour.segments.iter().any(|segment| {
                !matches!(segment, ContourSegment::Line(line) if line.start.x == line.end.x || line.start.y == line.end.y)
            })
        {
            return Ok(None);
        }
        let points = contour.segments.iter().flat_map(|segment| match segment {
            ContourSegment::Line(line) => [line.start, line.end],
            ContourSegment::Arc(arc) => [arc.start, arc.end],
        });
        let Some(piece_extent) = extent(points) else {
            return Ok(None);
        };
        pieces.push((feature, piece_extent));
    }
    if pieces.is_empty() {
        return Ok(None);
    }
    let max_area = pieces
        .iter()
        .map(|(_, piece_extent)| extent_area(piece_extent))
        .max()
        .unwrap_or_default();
    let outer = pieces
        .iter()
        .enumerate()
        .filter(|(_, (_, piece_extent))| extent_area(piece_extent) == max_area)
        .map(|(position, _)| position)
        .collect::<Vec<_>>();
    if outer.len() != 1 {
        return Ok(None);
    }
    let outer = outer[0];
    let outer_extent = pieces[outer].1.clone();
    for (position, (_, piece_extent)) in pieces.iter().enumerate() {
        budget.check()?;
        if position != outer
            && !(piece_extent.min.x > outer_extent.min.x
                && piece_extent.min.y > outer_extent.min.y
                && piece_extent.max.x < outer_extent.max.x
                && piece_extent.max.y < outer_extent.max.y)
        {
            return Ok(None);
        }
    }
    Ok(Some(BoardProfile {
        contour_feature_ids: vec![pieces[outer].0.id.clone()],
        cutout_feature_ids: pieces
            .iter()
            .enumerate()
            .filter(|(position, _)| *position != outer)
            .map(|(_, (feature, _))| feature.id.clone())
            .collect(),
        extents: Some(outer_extent),
        provenance: pieces
            .iter()
            .map(|(feature, _)| feature.provenance.clone())
            .collect(),
    }))
}

fn copper_order_complete(
    copper: &[&ManufacturingLayer],
    kind: AuthoritativeReviewKind,
    budget: ReconciliationBudget,
) -> Result<bool, FabricationError> {
    if copper.len() < 2 {
        return Ok(false);
    }
    let mut by_order = BTreeMap::new();
    let mut names = BTreeSet::new();
    for layer in copper {
        budget.check()?;
        let Some(order) = layer.order else {
            return Ok(false);
        };
        if by_order.insert(order, *layer).is_some() {
            return Ok(false);
        }
        if kind == AuthoritativeReviewKind::Native {
            let Some(name) = layer.name.as_deref() else {
                return Ok(false);
            };
            if !names.insert(name) {
                return Ok(false);
            }
        }
    }
    for index in 0..copper.len() {
        budget.check()?;
        let order = index as i32 + 1;
        let Some(layer) = by_order.get(&order) else {
            return Ok(false);
        };
        let last = index + 1 == copper.len();
        match kind {
            AuthoritativeReviewKind::Native => {
                let expected_name = if index == 0 {
                    "F.Cu".to_owned()
                } else if last {
                    "B.Cu".to_owned()
                } else {
                    format!("In{index}.Cu")
                };
                let expected_side = if index == 0 {
                    LayerSide::Top
                } else if last {
                    LayerSide::Bottom
                } else {
                    LayerSide::Inner
                };
                if layer.name.as_deref() != Some(expected_name.as_str())
                    || layer.side != expected_side
                    || layer.authority != Authority::NativeSource
                {
                    return Ok(false);
                }
            }
            AuthoritativeReviewKind::Package => {
                let expected_side = if index == 0 {
                    LayerSide::Top
                } else if last {
                    LayerSide::Bottom
                } else {
                    LayerSide::Inner
                };
                if layer.side != expected_side
                    || layer.name.is_some()
                    || layer.authority != Authority::Explicit
                {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}

pub(super) fn derive_authoritative_states(
    review: &FabricationReview,
    kind: AuthoritativeReviewKind,
    budget: ReconciliationBudget,
) -> Result<AuthoritativeDerivation, FabricationError> {
    let index = AuthoritativeIndex::build(review, budget)?;
    budget.check()?;
    let native = kind == AuthoritativeReviewKind::Native;
    let mut native_documents = Vec::new();
    let mut gerber_documents = Vec::new();
    let mut xnc_documents = Vec::new();
    let mut job_documents = Vec::new();
    for document in &review.documents {
        budget.check()?;
        match document.format {
            DocumentFormat::KicadPcb => native_documents.push(document),
            DocumentFormat::Gerber => gerber_documents.push(document),
            DocumentFormat::Excellon => xnc_documents.push(document),
            DocumentFormat::GerberJob => job_documents.push(document),
            _ => {}
        }
    }
    let mut semantic_documents = Vec::new();
    if native {
        for document in &native_documents {
            budget.check()?;
            semantic_documents.push(*document);
        }
    } else {
        for document in gerber_documents.iter().chain(&xnc_documents) {
            budget.check()?;
            semantic_documents.push(*document);
        }
    }
    let native_document_ids = checked_btree_set_with_deadline(
        native_documents.iter().map(|document| document.id.as_str()),
        budget.deadline,
        "reconciliation-deadline",
    )?;
    let gerber_document_ids = checked_btree_set_with_deadline(
        gerber_documents.iter().map(|document| document.id.as_str()),
        budget.deadline,
        "reconciliation-deadline",
    )?;
    let product_provided = review.product.is_some();
    let product_complete = if let Some(product) = &review.product {
        let nonempty =
            |value: &Option<String>| value.as_ref().is_some_and(|value| !value.is_empty());
        if native {
            product.authority == Authority::NativeSource
                && nonempty(&product.name)
                && nonempty(&product.revision)
                && checked_all_with_deadline(
                    &product.provenance,
                    budget.deadline,
                    "reconciliation-deadline",
                    |provenance| native_document_ids.contains(provenance.document_id.as_str()),
                )?
        } else {
            let mut supplied_identity = false;
            for value in [
                product.name.as_ref(),
                product.revision.as_ref(),
                product.part_number.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                budget.check()?;
                supplied_identity |= !value.is_empty();
            }
            product.authority == Authority::Explicit
                && supplied_identity
                && job_documents.len() == 1
                && !product.provenance.is_empty()
                && checked_all_with_deadline(
                    &product.provenance,
                    budget.deadline,
                    "reconciliation-deadline",
                    |provenance| provenance.document_id == job_documents[0].id,
                )?
        }
    } else {
        false
    };

    let mut role_provided = false;
    let mut job_complete = false;
    let role_complete = if native {
        native_documents.len() == 1
            && !review.layers.is_empty()
            && checked_all_with_deadline(
                &review.layers,
                budget.deadline,
                "reconciliation-deadline",
                |layer| {
                    role_provided = true;
                    if layer.document_id != native_documents[0].id
                        || layer.authority != Authority::NativeSource
                    {
                        return false;
                    }
                    match layer.role {
                        LayerRole::Copper => layer
                            .name
                            .as_ref()
                            .is_some_and(|name| name.ends_with(".Cu")),
                        LayerRole::Profile => {
                            layer.name.as_deref() == Some("Edge.Cuts")
                                && layer.side == LayerSide::NotApplicable
                                && layer.order.is_none()
                        }
                        _ => false,
                    }
                },
            )?
    } else {
        role_provided = !semantic_documents.is_empty();
        let job_id = (job_documents.len() == 1).then(|| job_documents[0].id.as_str());
        let mut documents_complete = !semantic_documents.is_empty();
        for document in &semantic_documents {
            budget.check()?;
            let layers = index
                .layers_by_document
                .get(document.id.as_str())
                .map(Vec::as_slice)
                .unwrap_or_default();
            let mut from_job = None;
            for layer in layers {
                budget.check()?;
                if layer.authority == Authority::Explicit
                    && Some(layer.provenance.document_id.as_str()) == job_id
                    && layer.role != LayerRole::Unknown
                {
                    from_job = Some(*layer);
                    break;
                }
            }
            let facts = index
                .job_functions_by_document
                .get(document.id.as_str())
                .map(Vec::as_slice)
                .unwrap_or_default();
            let functions = index
                .file_functions_by_document
                .get(document.id.as_str())
                .map(Vec::as_slice)
                .unwrap_or_default();
            let (Some(layer), [fact]) = (from_job, facts) else {
                documents_complete = false;
                break;
            };
            let job_matches = fact.omission.is_none()
                && fact.conflict_ids.is_empty()
                && fact.role == layer.role
                && fact.side == layer.side
                && fact.order == layer.order;
            let x2_matches = functions.len() <= 1
                && functions.first().is_none_or(|function| {
                    function.role == fact.role
                        && function.side == fact.side
                        && function.order == fact.order
                        && function.plating == fact.plating
                        && function.from_layer == fact.from_layer
                        && function.to_layer == fact.to_layer
                        && function.qualifier == fact.qualifier
                        && function.operation == fact.operation
                });
            if !(job_matches && x2_matches) {
                documents_complete = false;
                break;
            }
        }
        job_complete = job_id.is_some()
            && job_documents[0].parse_status == ParseStatus::Complete
            && product_complete
            && documents_complete;
        job_complete
    };

    let mut copper = Vec::new();
    for layer in &review.layers {
        budget.check()?;
        let selected = if native {
            native_document_ids.contains(layer.document_id.as_str())
        } else {
            gerber_document_ids.contains(layer.document_id.as_str())
        };
        if selected && layer.role == LayerRole::Copper {
            copper.push(layer);
        }
    }
    let order_complete = copper_order_complete(&copper, kind, budget)?;

    let expected_profile = if native {
        native_profile_from_index(review, &index, budget)?
    } else {
        package_profile_from_index(review, &index, budget)?
    };
    let mut profile_provided = review.profile.is_some();
    for layer in &review.layers {
        budget.check()?;
        profile_provided |= layer.role == LayerRole::Profile
            && if native {
                index.format(&layer.document_id) == Some(DocumentFormat::KicadPcb)
            } else {
                index.format(&layer.document_id) == Some(DocumentFormat::Gerber)
            };
    }
    let profile_complete = expected_profile.is_some();

    let relevant_document_ids = if native {
        native_document_ids.clone()
    } else {
        checked_btree_set_with_deadline(
            xnc_documents.iter().map(|document| document.id.as_str()),
            budget.deadline,
            "reconciliation-deadline",
        )?
    };
    let mut tools = Vec::new();
    for document_id in &relevant_document_ids {
        budget.check()?;
        tools.extend(
            index
                .tools_by_document
                .get(document_id)
                .into_iter()
                .flat_map(|tools| tools.iter().copied()),
        );
    }
    let tools_provided = !tools.is_empty() || (!native && !xnc_documents.is_empty());
    let mut tools_complete = !tools.is_empty();
    let mut drills = 0_usize;
    for document_id in &relevant_document_ids {
        for feature in index
            .features_by_document
            .get(document_id)
            .into_iter()
            .flat_map(|features| features.iter().copied())
        {
            budget.check()?;
            drills += usize::from(matches!(feature.geometry, Geometry::Drill(_)));
        }
    }
    let drill_provided = drills > 0 || (!native && !xnc_documents.is_empty());
    let drills_complete = drills > 0;

    let mut plating_complete = !tools.is_empty();
    let mut spans_complete = !tools.is_empty();
    for tool in &tools {
        budget.check()?;
        tools_complete &= tool.diameter.is_some_and(|diameter| diameter.0 > 0);
        plating_complete &= matches!(tool.plating, Plating::Plated | Plating::NonPlated);
        let span_valid = tool.span.as_ref().is_some_and(|span| {
            let (Some(from), Some(to)) = (&span.from_layer_id, &span.to_layer_id) else {
                return false;
            };
            let (Some(from_layer), Some(to_layer)) = (
                index.layers_by_id.get(from.as_str()),
                index.layers_by_id.get(to.as_str()),
            ) else {
                return false;
            };
            from_layer.document_id == tool.document_id && to_layer.document_id == tool.document_id
        });
        spans_complete &= span_valid;
    }
    if !native {
        for document in &xnc_documents {
            budget.check()?;
            let functions = index
                .file_functions_by_document
                .get(document.id.as_str())
                .map(Vec::as_slice)
                .unwrap_or_default();
            if functions.len() != 1 {
                plating_complete = false;
                spans_complete = false;
                continue;
            }
            let function = &functions[0];
            plating_complete &= matches!(function.plating, Plating::Plated | Plating::NonPlated);
            spans_complete &= function.from_layer.is_some() && function.to_layer.is_some();
            for tool in index
                .tools_by_document
                .get(document.id.as_str())
                .into_iter()
                .flat_map(|tools| tools.iter().copied())
            {
                budget.check()?;
                plating_complete &= tool.plating == function.plating;
                let orders = tool.span.as_ref().and_then(|span| {
                    let from = index.layers_by_id.get(span.from_layer_id.as_deref()?)?;
                    let to = index.layers_by_id.get(span.to_layer_id.as_deref()?)?;
                    Some((from.order, to.order))
                });
                spans_complete &= orders == Some((function.from_layer, function.to_layer));
            }
        }
    }

    let extents_provided = expected_profile
        .as_ref()
        .and_then(|profile| profile.extents.as_ref())
        .is_some()
        && (native || !index.physical_bounds_by_document.is_empty());
    let extents_complete = if native {
        extents_provided
    } else if let Some(profile_extent) = expected_profile
        .as_ref()
        .and_then(|profile| profile.extents.as_ref())
    {
        let mut release_documents = Vec::new();
        for document in gerber_documents.iter().chain(&xnc_documents) {
            budget.check()?;
            release_documents.push(*document);
        }
        let mut complete = !release_documents.is_empty();
        for document in release_documents {
            budget.check()?;
            let bounds = index
                .physical_bounds_by_document
                .get(document.id.as_str())
                .map(Vec::as_slice)
                .unwrap_or_default();
            complete &= matches!(bounds, [bounds] if extent_contains_at_resolution(
                profile_extent,
                &bounds.extent,
                bounds.resolution,
            ));
        }
        complete
    } else {
        false
    };

    let mut profile_layer_ids = BTreeSet::new();
    for layer in &review.layers {
        budget.check()?;
        if layer.role == LayerRole::Profile {
            profile_layer_ids.insert(layer.id.as_str());
        }
    }
    let connectivity_document_ids = if native {
        native_document_ids
    } else {
        gerber_document_ids
    };
    let mut eligible = Vec::new();
    for document_id in &connectivity_document_ids {
        for feature in index
            .features_by_document
            .get(document_id)
            .into_iter()
            .flat_map(|features| features.iter().copied())
        {
            budget.check()?;
            if !native || !profile_layer_ids.contains(feature.layer_id.as_str()) {
                eligible.push(feature);
            }
        }
    }
    let connectivity_provided = !review.connectivity.is_empty()
        || checked_any_with_deadline(
            &connectivity_document_ids,
            budget.deadline,
            "reconciliation-deadline",
            |document_id| index.object_attribute_documents.contains(document_id),
        )?;
    let mut connectivity_complete = !eligible.is_empty();
    let mut components_complete = !eligible.is_empty();
    let mut pins_complete = !eligible.is_empty();
    for feature in &eligible {
        budget.check()?;
        let item = index
            .connectivity_by_feature
            .get(feature.id.as_str())
            .filter(|items| items.len() == 1)
            .map(|items| items[0]);
        connectivity_complete &= item
            .and_then(|item| item.net.as_ref())
            .is_some_and(|value| !value.is_empty());
        components_complete &= item
            .and_then(|item| item.component.as_ref())
            .is_some_and(|value| !value.is_empty());
        pins_complete &= item
            .and_then(|item| item.pin.as_ref())
            .is_some_and(|value| !value.is_empty());
    }

    let mut states = BTreeMap::new();
    for (id, complete, provided) in [
        (
            CapabilityId::ProductIdentity,
            product_complete,
            product_provided,
        ),
        (CapabilityId::LayerRoles, role_complete, role_provided),
        (CapabilityId::LayerOrder, order_complete, !copper.is_empty()),
        (CapabilityId::Profile, profile_complete, profile_provided),
        (CapabilityId::Drills, drills_complete, drill_provided),
        (CapabilityId::Tools, tools_complete, tools_provided),
        (CapabilityId::Plating, plating_complete, tools_provided),
        (CapabilityId::LayerSpans, spans_complete, tools_provided),
        (CapabilityId::Extents, extents_complete, extents_provided),
        (
            CapabilityId::Connectivity,
            connectivity_complete,
            connectivity_provided,
        ),
        (
            CapabilityId::Components,
            components_complete,
            connectivity_provided,
        ),
        (CapabilityId::Pins, pins_complete, connectivity_provided),
    ] {
        states.insert(id, derived_state(complete, provided, index.is_blocked(id)));
    }
    if !native {
        let package_complete = job_complete
            && RECONCILIATION_PREREQUISITES
                .iter()
                .all(|id| states.get(id) == Some(&CapabilityState::Complete));
        states.insert(
            CapabilityId::PackageCompleteness,
            derived_state(
                package_complete,
                !semantic_documents.is_empty(),
                index.is_blocked(CapabilityId::PackageCompleteness),
            ),
        );
        let reconciliation_provided = review.source_pair.is_some()
            || review.native_reconciliation_source.is_some()
            || review.integration_outcome.is_some()
            || !review.reconciliations.is_empty();
        let expected_families = [
            ReconciliationFamily::Product,
            ReconciliationFamily::Layers,
            ReconciliationFamily::Profile,
            ReconciliationFamily::Drills,
            ReconciliationFamily::Extents,
            ReconciliationFamily::Connectivity,
        ];
        let families = review
            .reconciliations
            .iter()
            .map(|item| item.family)
            .collect::<BTreeSet<_>>();
        let reconciliation_complete = package_complete
            && review.reconciliations.len() == expected_families.len()
            && families == expected_families.into_iter().collect()
            && review
                .reconciliations
                .iter()
                .all(|item| item.status == ReconciliationStatus::Match);
        let reconciliation_state = match review.integration_outcome.as_ref().map(|item| item.state)
        {
            Some(IntegratedReconciliationState::NotProvided) => CapabilityState::NotProvided,
            Some(IntegratedReconciliationState::Failed) => CapabilityState::Failed,
            None => derived_state(
                reconciliation_complete,
                reconciliation_provided,
                index.is_blocked(CapabilityId::PackageReconciliation),
            ),
        };
        states.insert(CapabilityId::PackageReconciliation, reconciliation_state);
    }
    let status = if review.documents.is_empty() {
        if review
            .integration_outcome
            .as_ref()
            .is_some_and(|outcome| outcome.state == IntegratedReconciliationState::Failed)
        {
            FabricationStatus::Failed
        } else {
            FabricationStatus::NotProvided
        }
    } else if native || review.integration_outcome.is_some() {
        FabricationStatus::Partial
    } else if states.get(&CapabilityId::PackageCompleteness) == Some(&CapabilityState::Complete)
        && (review.source_pair.is_none()
            || states.get(&CapabilityId::PackageReconciliation) == Some(&CapabilityState::Complete))
    {
        FabricationStatus::Complete
    } else {
        FabricationStatus::Partial
    };
    Ok(AuthoritativeDerivation {
        states,
        expected_profile,
        status,
    })
}

fn authoritative_review_kind(
    review: &FabricationReview,
    budget: ReconciliationBudget,
) -> Result<Option<AuthoritativeReviewKind>, FabricationError> {
    let mut native_capability = false;
    let mut package_capability = false;
    let mut prerequisite_ids = BTreeSet::new();
    for record in &review.capabilities.records {
        budget.check()?;
        native_capability |= record.id == CapabilityId::NativeKicadFacts;
        package_capability |= record.id == CapabilityId::PackageCompleteness;
        prerequisite_ids.insert(record.id);
    }
    let mut has_native_document = false;
    let mut only_native_documents = !review.documents.is_empty();
    for document in &review.documents {
        budget.check()?;
        has_native_document |= document.format == DocumentFormat::KicadPcb;
        only_native_documents &= document.format == DocumentFormat::KicadPcb;
    }
    if native_capability && has_native_document && only_native_documents {
        Ok(Some(AuthoritativeReviewKind::Native))
    } else if review.source_pair.is_some()
        || (package_capability
            && RECONCILIATION_PREREQUISITES
                .iter()
                .all(|id| prerequisite_ids.contains(id)))
    {
        Ok(Some(AuthoritativeReviewKind::Package))
    } else {
        Ok(None)
    }
}

fn validate_authoritative_states_for_kind(
    review: &FabricationReview,
    kind: AuthoritativeReviewKind,
    budget: ReconciliationBudget,
) -> Result<(), FabricationError> {
    let derived = derive_authoritative_states(review, kind, budget)?;
    if review.profile != derived.expected_profile {
        return Err(FabricationError::InvalidIdentity(
            "authoritative-profile".into(),
        ));
    }
    for id in RECONCILIATION_PREREQUISITES {
        budget.check()?;
        let supplied = review
            .capabilities
            .records
            .iter()
            .find(|record| record.id == id)
            .ok_or_else(|| {
                FabricationError::DanglingReference(format!("authoritative-capability:{id:?}"))
            })?;
        if supplied.state != derived.state(id) {
            return Err(FabricationError::InvalidIdentity(format!(
                "authoritative-capability:{id:?}"
            )));
        }
    }
    if kind == AuthoritativeReviewKind::Package {
        for id in [
            CapabilityId::PackageCompleteness,
            CapabilityId::PackageReconciliation,
        ] {
            let supplied = review
                .capabilities
                .records
                .iter()
                .find(|record| record.id == id);
            if id == CapabilityId::PackageReconciliation
                && supplied.is_none()
                && review.source_pair.is_none()
                && review.integration_outcome.is_none()
            {
                continue;
            }
            if supplied.is_none_or(|record| record.state != derived.state(id)) {
                return Err(FabricationError::InvalidIdentity(format!(
                    "authoritative-capability:{id:?}"
                )));
            }
        }
    }
    if let Some(outcome) = &review.integration_outcome {
        let expected = match outcome.state {
            IntegratedReconciliationState::NotProvided => CapabilityState::NotProvided,
            IntegratedReconciliationState::Failed => CapabilityState::Failed,
        };
        if review
            .capabilities
            .records
            .iter()
            .find(|record| record.id == CapabilityId::NativeKicadFacts)
            .is_none_or(|record| record.state != expected)
        {
            return Err(FabricationError::InvalidIdentity(
                "authoritative-native-integration".into(),
            ));
        }
    }
    if review.status != derived.status {
        return Err(FabricationError::InvalidIdentity(
            "authoritative-fabrication-status".into(),
        ));
    }
    Ok(())
}

pub(super) fn validate_authoritative_states(
    review: &FabricationReview,
    budget: ReconciliationBudget,
) -> Result<(), FabricationError> {
    let Some(kind) = authoritative_review_kind(review, budget)? else {
        return Ok(());
    };
    validate_authoritative_states_for_kind(review, kind, budget)
}

fn reconciliation_prerequisites_complete(
    derived: &AuthoritativeDerivation,
    ids: &[CapabilityId],
) -> bool {
    ids.iter()
        .all(|id| derived.state(*id) == CapabilityState::Complete)
}

pub fn parse_native_kicad_manufacturing(
    virtual_path: &str,
    bytes: &[u8],
) -> Result<NativeManufacturing, NativeManufacturingError> {
    parse_native_kicad_manufacturing_with_timeout(
        virtual_path,
        bytes,
        Duration::from_millis(MANUFACTURING_LIMITS.file_timeout_ms),
    )
}

pub fn parse_native_kicad_manufacturing_with_timeout(
    virtual_path: &str,
    bytes: &[u8],
    timeout: Duration,
) -> Result<NativeManufacturing, NativeManufacturingError> {
    parse_native_kicad_manufacturing_with_deadline(
        virtual_path,
        bytes,
        ManufacturingDeadline::from_timeout(timeout).with_file_limit(),
    )
}

pub(crate) fn parse_native_kicad_manufacturing_with_deadline(
    virtual_path: &str,
    bytes: &[u8],
    deadline_at: ManufacturingDeadline,
) -> Result<NativeManufacturing, NativeManufacturingError> {
    deadline(deadline_at)?;
    let mut invalid_byte = false;
    for chunk in bytes.chunks(4096) {
        deadline(deadline_at)?;
        invalid_byte |= chunk
            .iter()
            .any(|byte| (*byte < b' ' && !matches!(*byte, b'\n' | b'\r' | b'\t')) || *byte == 0x7f);
    }
    if bytes.len() as u64 > MANUFACTURING_LIMITS.raw_bytes_per_file
        || !valid_virtual_path(virtual_path)
        || virtual_path.len() > MANUFACTURING_LIMITS.normalized_path_bytes
        || invalid_byte
    {
        return Err(NativeManufacturingError::Resource {
            resource: "native-source-bytes",
        });
    }
    let source = std::str::from_utf8(bytes).map_err(|_| NativeManufacturingError::Invalid {
        reason: "native-source-utf8",
    })?;
    let mut max_line_bytes = 0;
    for line in source.lines() {
        deadline(deadline_at)?;
        max_line_bytes = max_line_bytes.max(line.len());
    }
    if max_line_bytes > MANUFACTURING_LIMITS.max_line_bytes {
        return Err(NativeManufacturingError::Resource {
            resource: "native-line-bytes",
        });
    }
    let syntax = NativeSyntax::parse(source, deadline_at)?;
    let numeric_format =
        SourceNumericFormat::new(SourceUnit::Millimetre, 8, syntax.metrics.decimal_digits)
            .map_err(NativeManufacturingError::Canonical)?;
    let artifact_digest = sha256_with_deadline(bytes, deadline_at, "native-input-hash").map_err(
        |error| match error {
            FabricationError::LimitExceeded { .. } => NativeManufacturingError::Resource {
                resource: "native-deadline",
            },
            error => NativeManufacturingError::Canonical(error),
        },
    )?;
    let document_id = document_id(&artifact_digest, DocumentFormat::KicadPcb)
        .map_err(NativeManufacturingError::Canonical)?;
    let whole_provenance = ManufacturingProvenance {
        document_id: document_id.clone(),
        artifact_digest: artifact_digest.clone(),
        producer: "ratemypcb-kicad-source".into(),
        producer_version: KICAD_MANUFACTURING_ADAPTER_VERSION.into(),
        location: StructuralLocation {
            record: syntax.root as u64,
            subrecord: None,
            byte_start: 0,
            byte_end: bytes.len().saturating_sub(1) as u64,
        },
        source_lexeme: None,
    };

    let layer_table =
        syntax
            .unique_child(syntax.root, "layers")?
            .ok_or(NativeManufacturingError::Invalid {
                reason: "native-layer-table",
            })?;
    let mut layer_specs = BTreeMap::<u8, (String, usize)>::new();
    let mut layer_names_seen = BTreeSet::new();
    for child in syntax.children[layer_table].iter().copied() {
        deadline(deadline_at)?;
        let Ok(number) = syntax.name(child).parse::<u8>() else {
            continue;
        };
        let values = syntax.tokens(child)?;
        let Some(name) = values
            .first()
            .filter(|name| name.ends_with(".Cu") || *name == "Edge.Cuts")
        else {
            continue;
        };
        if !layer_names_seen.insert(name.clone())
            || layer_specs.insert(number, (name.clone(), child)).is_some()
        {
            return Err(NativeManufacturingError::Invalid {
                reason: "native-duplicate-layer",
            });
        }
    }
    let mut ordered_layer_specs = Vec::with_capacity(layer_specs.len());
    for (number, (name, child)) in layer_specs {
        deadline(deadline_at)?;
        ordered_layer_specs.push((number, name, child));
    }
    let layer_specs = ordered_layer_specs;
    let mut copper_specs = Vec::new();
    for spec in &layer_specs {
        deadline(deadline_at)?;
        if spec.1.ends_with(".Cu") {
            copper_specs.push(spec);
        }
    }
    if copper_specs.is_empty() {
        return Err(NativeManufacturingError::Invalid {
            reason: "native-copper-layers",
        });
    }
    let copper_count = copper_specs.len();
    let mut layers = Vec::new();
    let mut layer_ids = BTreeMap::new();
    let mut copper_order = 0_i32;
    for (_, name, form) in &layer_specs {
        deadline(deadline_at)?;
        let role = if name == "Edge.Cuts" {
            LayerRole::Profile
        } else {
            copper_order += 1;
            LayerRole::Copper
        };
        let side = match name.as_str() {
            "F.Cu" => LayerSide::Top,
            "B.Cu" => LayerSide::Bottom,
            "Edge.Cuts" => LayerSide::NotApplicable,
            _ => LayerSide::Inner,
        };
        let location = provenance(&syntax, &document_id, &artifact_digest, *form);
        let order = (role == LayerRole::Copper).then_some(copper_order);
        let id = layer_id(
            &document_id,
            Some(name),
            role,
            side,
            order,
            Authority::NativeSource,
            &location.location,
        );
        layer_ids.insert(name.clone(), id.clone());
        layers.push(ManufacturingLayer {
            id,
            document_id: document_id.clone(),
            name: Some(name.clone()),
            role,
            side,
            context: LayerContext::Board,
            polarity: LayerPolarity::Unknown,
            order,
            authority: Authority::NativeSource,
            provenance: location,
        });
    }
    let mut expected_order = 1_i32;
    let mut order_is_contiguous = true;
    let mut first_copper_id = None;
    for layer in &layers {
        deadline(deadline_at)?;
        if layer.role == LayerRole::Copper {
            first_copper_id.get_or_insert_with(|| layer.id.clone());
            order_is_contiguous &= layer.order == Some(expected_order);
            expected_order += 1;
        }
    }
    let layer_order_complete = layer_ids.contains_key("F.Cu")
        && layer_ids.contains_key("B.Cu")
        && copper_count >= 2
        && order_is_contiguous
        && expected_order - 1 == copper_count as i32;
    let top_layer_id = layer_ids.get("F.Cu").cloned().or(first_copper_id).ok_or(
        NativeManufacturingError::Invalid {
            reason: "native-copper-layers",
        },
    )?;
    let profile_layer_id = layer_ids.get("Edge.Cuts").cloned();

    let mut profile_pieces = Vec::new();
    let mut unsupported_profile = Vec::new();
    if let Some(profile_layer_id) = &profile_layer_id {
        for rect in syntax.children_named_checked(syntax.root, "gr_rect")? {
            deadline(deadline_at)?;
            if layer_name(&syntax, rect)?.as_deref() != Some("Edge.Cuts") {
                continue;
            }
            let start = point(&syntax, rect, "start")?;
            let end = point(&syntax, rect, "end")?;
            if start.x == end.x || start.y == end.y {
                return Err(NativeManufacturingError::Invalid {
                    reason: "native-zero-profile-rect",
                });
            }
            let points = [
                start,
                CanonicalPoint::new(end.x.0, start.y.0),
                end,
                CanonicalPoint::new(start.x.0, end.y.0),
            ];
            let segments = (0..4)
                .map(|index| {
                    ContourSegment::Line(CanonicalLine {
                        start: points[index],
                        end: points[(index + 1) % 4],
                        width: None,
                    })
                })
                .collect::<Vec<_>>();
            let evidence = provenance(&syntax, &document_id, &artifact_digest, rect);
            let profile_feature = feature(
                &document_id,
                profile_layer_id,
                None,
                Geometry::Contour(CanonicalContour {
                    segments,
                    closed: true,
                }),
                TransformChain::default(),
                evidence.clone(),
            );
            profile_pieces.push(ProfilePiece {
                feature: profile_feature,
                extent: extent(points).ok_or(NativeManufacturingError::Invalid {
                    reason: "native-profile-extents",
                })?,
                provenance: evidence,
            });
        }
        let mut line_segments = Vec::new();
        for line in syntax.children_named_checked(syntax.root, "gr_line")? {
            deadline(deadline_at)?;
            if layer_name(&syntax, line)?.as_deref() == Some("Edge.Cuts") {
                line_segments.push((
                    CanonicalLine {
                        start: point(&syntax, line, "start")?,
                        end: point(&syntax, line, "end")?,
                        width: None,
                    },
                    provenance(&syntax, &document_id, &artifact_digest, line),
                ));
            }
        }
        let first_line_form = line_segments
            .first()
            .map(|(_, provenance)| provenance.location.record as usize)
            .unwrap_or(layer_table);
        match line_profile_pieces(&document_id, profile_layer_id, line_segments, deadline_at) {
            Ok(mut pieces) => profile_pieces.append(&mut pieces),
            Err(_) => unsupported_profile.push(first_line_form),
        }
        for (index, form) in syntax.forms.iter().enumerate() {
            if index & 0x0fff == 0 {
                deadline(deadline_at)?;
            }
            let name = syntax.name(index);
            if matches!(name, "gr_rect" | "gr_line" | "layer" | "layers") {
                continue;
            }
            if (name.starts_with("gr_") || name.starts_with("fp_"))
                && syntax.child(index, "layer")?.is_some()
                && layer_name(&syntax, index)?.as_deref() == Some("Edge.Cuts")
            {
                unsupported_profile.push(index);
            }
            if form.parent.is_none() && index != syntax.root {
                unsupported_profile.push(index);
            }
        }
    }

    let mut profile = None;
    let mut profile_extent = None;
    if unsupported_profile.is_empty() && !profile_pieces.is_empty() {
        let mut max_area = 0_i128;
        for piece in &profile_pieces {
            deadline(deadline_at)?;
            max_area = max_area.max(extent_area(&piece.extent));
        }
        let mut outer = Vec::new();
        for (index, piece) in profile_pieces.iter().enumerate() {
            deadline(deadline_at)?;
            if extent_area(&piece.extent) == max_area {
                outer.push(index);
            }
        }
        if outer.len() == 1 {
            let outer = outer[0];
            let outer_extent = profile_pieces[outer].extent.clone();
            let mut contained = true;
            for (index, piece) in profile_pieces.iter().enumerate() {
                deadline(deadline_at)?;
                contained &= index == outer
                    || (piece.extent.min.x > outer_extent.min.x
                        && piece.extent.min.y > outer_extent.min.y
                        && piece.extent.max.x < outer_extent.max.x
                        && piece.extent.max.y < outer_extent.max.y);
            }
            if contained {
                let mut cutout_feature_ids = Vec::new();
                let mut provenance = Vec::with_capacity(profile_pieces.len());
                for (index, piece) in profile_pieces.iter().enumerate() {
                    deadline(deadline_at)?;
                    if index != outer {
                        cutout_feature_ids.push(piece.feature.id.clone());
                    }
                    provenance.push(piece.provenance.clone());
                }
                profile_extent = Some(outer_extent.clone());
                profile = Some(BoardProfile {
                    contour_feature_ids: vec![profile_pieces[outer].feature.id.clone()],
                    cutout_feature_ids,
                    extents: Some(outer_extent),
                    provenance,
                });
            } else {
                unsupported_profile.push(profile_pieces.first().map_or(layer_table, |piece| {
                    piece.provenance.location.record as usize
                }));
            }
        } else {
            unsupported_profile.push(profile_pieces.first().map_or(layer_table, |piece| {
                piece.provenance.location.record as usize
            }));
        }
    }

    let mut nets = BTreeMap::new();
    let mut net_names = BTreeSet::new();
    for net in syntax.children_named_checked(syntax.root, "net")? {
        deadline(deadline_at)?;
        let values = syntax.tokens(net)?;
        let id = values
            .first()
            .and_then(|value| value.parse::<u32>().ok())
            .ok_or(NativeManufacturingError::Invalid {
                reason: "native-net-id",
            })?;
        let name = values.get(1).cloned().unwrap_or_default();
        if nets.insert(id, name.clone()).is_some() || (!name.is_empty() && !net_names.insert(name))
        {
            return Err(NativeManufacturingError::Invalid {
                reason: "duplicate-native-net",
            });
        }
    }

    let mut tools = Vec::new();
    let mut tool_identities = BTreeMap::new();
    let mut features = Vec::with_capacity(profile_pieces.len());
    for piece in profile_pieces {
        deadline(deadline_at)?;
        features.push(piece.feature);
    }
    let mut connectivity = Vec::new();
    let mut eligible_pads = 0_usize;
    let mut drill_features = 0_usize;
    let mut plating_complete = true;
    let mut spans_complete = true;
    let mut drills_exact = true;
    let mut inexact_drill_provenance = None;

    let mut footprints = syntax.children_named_checked(syntax.root, "footprint")?;
    footprints.extend(syntax.children_named_checked(syntax.root, "module")?);
    for footprint in footprints {
        deadline(deadline_at)?;
        let (footprint_position, footprint_angle) = pose(&syntax, footprint, "at")?;
        let footprint_reference = reference(&syntax, footprint)?;
        for pad in syntax.children_named_checked(footprint, "pad")? {
            deadline(deadline_at)?;
            let values = syntax.tokens(pad)?;
            if values.len() < 3 {
                return Err(NativeManufacturingError::Invalid {
                    reason: "invalid-native-pad",
                });
            }
            let pin = values.first().filter(|value| !value.is_empty()).cloned();
            let kind = values.get(1).map(String::as_str).unwrap_or_default();
            let (pad_position, pad_angle) = pose(&syntax, pad, "at")?;
            let names = layer_names(&syntax, pad)?;
            let declares_copper = names.iter().any(|name| name.ends_with(".Cu"));
            if !declares_copper {
                continue;
            }
            if !copper_layer_names_valid(&names, &layer_ids) {
                return Err(NativeManufacturingError::Invalid {
                    reason: "dangling-native-layer",
                });
            }
            let copper_layers = resolved_copper_layers(&names, &layer_ids);
            if copper_layers.is_empty() {
                return Err(NativeManufacturingError::Invalid {
                    reason: "missing-native-copper-layer",
                });
            }
            eligible_pads += 1;
            let layer_id = copper_layers
                .first()
                .cloned()
                .unwrap_or_else(|| top_layer_id.clone());
            let evidence = provenance(&syntax, &document_id, &artifact_digest, pad);
            let mut operations = Vec::new();
            let drill = syntax.child_tokens(pad, "drill")?;
            let geometry = if let Some(drill) = drill {
                if footprint_angle.rem_euclid(90_000_000) != 0
                    || pad_angle.rem_euclid(90_000_000) != 0
                {
                    drills_exact = false;
                    inexact_drill_provenance.get_or_insert_with(|| evidence.clone());
                }
                let plating = match kind {
                    "thru_hole" => Plating::Plated,
                    "np_thru_hole" => Plating::NonPlated,
                    _ => Plating::Unknown,
                };
                plating_complete &= plating != Plating::Unknown;
                let from_layer_id = copper_layers.first().cloned();
                let to_layer_id = copper_layers.last().cloned();
                spans_complete &= from_layer_id.is_some() && to_layer_id.is_some();
                let (tool_kind, diameter, geometry) = match drill.as_slice() {
                    [diameter] => {
                        let diameter = parse_length(diameter, "invalid-native-drill")?;
                        (ToolKind::Drill, diameter, (false, diameter, diameter))
                    }
                    [shape, first, second] if shape == "oval" => {
                        let first = parse_length(first, "invalid-native-drill")?;
                        let second = parse_length(second, "invalid-native-drill")?;
                        if first == second {
                            (ToolKind::Drill, first, (false, first, Picometres(0)))
                        } else {
                            let (major, minor, vertical) = if first > second {
                                (first, second, false)
                            } else {
                                (second, first, true)
                            };
                            (
                                ToolKind::Composite,
                                minor,
                                (true, major, Picometres(i64::from(vertical))),
                            )
                        }
                    }
                    _ => {
                        return Err(NativeManufacturingError::Invalid {
                            reason: "unsupported-native-drill-shape",
                        });
                    }
                };
                if diameter.0 <= 0 {
                    return Err(NativeManufacturingError::Invalid {
                        reason: "invalid-native-drill",
                    });
                }
                let tool_id = ensure_tool(
                    &mut tools,
                    &mut tool_identities,
                    &document_id,
                    NativeToolKey {
                        kind: tool_kind,
                        diameter,
                        plating,
                        from_layer_id,
                        to_layer_id,
                    },
                    &evidence,
                )?;
                drill_features += 1;
                if drill_features > MANUFACTURING_LIMITS.drill_route_features {
                    return Err(NativeManufacturingError::Resource {
                        resource: "native-drill-features",
                    });
                }
                if geometry.0 {
                    let major = geometry.1;
                    let vertical = geometry.2.0 != 0;
                    let half = major
                        .0
                        .checked_sub(diameter.0)
                        .and_then(|value| value.checked_div(2))
                        .ok_or(NativeManufacturingError::Invalid {
                            reason: "invalid-native-slot",
                        })?;
                    if pad_angle != 0 || vertical {
                        operations.push(TransformOperation::Rotate {
                            microdegrees: pad_angle
                                .checked_add(if vertical { 90_000_000 } else { 0 })
                                .ok_or(NativeManufacturingError::Invalid {
                                    reason: "native-angle",
                                })?,
                        });
                    }
                    operations.push(TransformOperation::Translate {
                        x: pad_position.x,
                        y: pad_position.y,
                    });
                    if footprint_angle != 0 {
                        operations.push(TransformOperation::Rotate {
                            microdegrees: footprint_angle,
                        });
                    }
                    operations.push(TransformOperation::Translate {
                        x: footprint_position.x,
                        y: footprint_position.y,
                    });
                    Geometry::Slot(SlotFeature {
                        start: CanonicalPoint::new(-half, 0),
                        end: CanonicalPoint::new(half, 0),
                        width: diameter,
                        tool_id,
                    })
                } else {
                    operations.push(TransformOperation::Translate {
                        x: pad_position.x,
                        y: pad_position.y,
                    });
                    if footprint_angle != 0 {
                        operations.push(TransformOperation::Rotate {
                            microdegrees: footprint_angle,
                        });
                    }
                    operations.push(TransformOperation::Translate {
                        x: footprint_position.x,
                        y: footprint_position.y,
                    });
                    Geometry::Drill(DrillFeature {
                        position: CanonicalPoint::default(),
                        diameter,
                        tool_id,
                    })
                }
            } else {
                operations.push(TransformOperation::Translate {
                    x: pad_position.x,
                    y: pad_position.y,
                });
                if footprint_angle != 0 {
                    operations.push(TransformOperation::Rotate {
                        microdegrees: footprint_angle,
                    });
                }
                operations.push(TransformOperation::Translate {
                    x: footprint_position.x,
                    y: footprint_position.y,
                });
                Geometry::Point(CanonicalPoint::default())
            };
            let tool_id = match &geometry {
                Geometry::Drill(value) => Some(value.tool_id.clone()),
                Geometry::Slot(value) => Some(value.tool_id.clone()),
                _ => None,
            };
            let native_feature = feature(
                &document_id,
                &layer_id,
                tool_id.as_deref(),
                geometry,
                transforms(operations),
                evidence.clone(),
            );
            let net = syntax.child_tokens(pad, "net")?.and_then(|values| {
                let id = values.first()?.parse::<u32>().ok()?;
                let declared = values.get(1)?;
                nets.get(&id)
                    .filter(|known| !known.is_empty() && *known == declared)
                    .cloned()
            });
            connectivity.push(ObjectSemantics {
                feature_id: native_feature.id.clone(),
                net,
                component: footprint_reference.clone(),
                pin,
                provenance: evidence,
            });
            features.push(native_feature);
        }
    }

    for via in syntax.children_named_checked(syntax.root, "via")? {
        deadline(deadline_at)?;
        let (position, _) = pose(&syntax, via, "at")?;
        let drill = syntax
            .child_tokens(via, "drill")?
            .and_then(|values| values.into_iter().next())
            .ok_or(NativeManufacturingError::Invalid {
                reason: "native-via-drill",
            })?;
        let diameter = parse_length(&drill, "native-via-drill")?;
        if diameter.0 <= 0 {
            return Err(NativeManufacturingError::Invalid {
                reason: "native-via-drill",
            });
        }
        let names = layer_names(&syntax, via)?;
        if !copper_layer_names_valid(&names, &layer_ids) {
            return Err(NativeManufacturingError::Invalid {
                reason: "dangling-native-layer",
            });
        }
        let copper_layers = resolved_copper_layers(&names, &layer_ids);
        if copper_layers.is_empty() {
            return Err(NativeManufacturingError::Invalid {
                reason: "missing-native-copper-layer",
            });
        }
        let from_layer_id = copper_layers.first().cloned();
        let to_layer_id = copper_layers.last().cloned();
        spans_complete &= from_layer_id.is_some() && to_layer_id.is_some();
        let evidence = provenance(&syntax, &document_id, &artifact_digest, via);
        let tool_id = ensure_tool(
            &mut tools,
            &mut tool_identities,
            &document_id,
            NativeToolKey {
                kind: ToolKind::Drill,
                diameter,
                plating: Plating::Plated,
                from_layer_id,
                to_layer_id,
            },
            &evidence,
        )?;
        let native_feature = feature(
            &document_id,
            &top_layer_id,
            Some(&tool_id),
            Geometry::Drill(DrillFeature {
                position,
                diameter,
                tool_id: tool_id.clone(),
            }),
            TransformChain::default(),
            evidence,
        );
        features.push(native_feature);
        drill_features += 1;
        if drill_features > MANUFACTURING_LIMITS.drill_route_features {
            return Err(NativeManufacturingError::Resource {
                resource: "native-drill-features",
            });
        }
    }

    if features.len() > MANUFACTURING_LIMITS.geometry_features
        || drill_features > MANUFACTURING_LIMITS.drill_route_features
        || connectivity.len() > MANUFACTURING_LIMITS.geometry_features
    {
        return Err(NativeManufacturingError::Resource {
            resource: "native-canonical-features",
        });
    }

    let title_block = syntax.unique_child(syntax.root, "title_block")?;
    let title = title_block
        .map(|title_block| syntax.child_tokens(title_block, "title"))
        .transpose()?
        .flatten()
        .and_then(|values| values.into_iter().next())
        .filter(|value| !value.is_empty());
    let revision = title_block
        .map(|title_block| syntax.child_tokens(title_block, "rev"))
        .transpose()?
        .flatten()
        .and_then(|values| values.into_iter().next())
        .filter(|value| !value.is_empty());
    let product_provenance =
        title_block.map(|form| provenance(&syntax, &document_id, &artifact_digest, form));
    let product = (title.is_some() || revision.is_some()).then(|| ProductIdentity {
        name: title.clone(),
        revision: revision.clone(),
        part_number: None,
        authority: Authority::NativeSource,
        provenance: product_provenance.clone().into_iter().collect(),
    });
    let product_state = match (&title, &revision) {
        (Some(_), Some(_)) => CapabilityState::Complete,
        (Some(_), None) | (None, Some(_)) => CapabilityState::Partial,
        (None, None) => CapabilityState::NotProvided,
    };
    let mut connectivity_complete = eligible_pads > 0 && connectivity.len() == eligible_pads;
    for item in &connectivity {
        deadline(deadline_at)?;
        connectivity_complete &= item.net.as_ref().is_some_and(|value| !value.is_empty())
            && item
                .component
                .as_ref()
                .is_some_and(|value| !value.is_empty())
            && item.pin.as_ref().is_some_and(|value| !value.is_empty());
    }
    let connectivity_state = if connectivity_complete {
        CapabilityState::Complete
    } else if connectivity.is_empty() {
        CapabilityState::NotProvided
    } else {
        CapabilityState::Partial
    };
    let profile_state = if profile.is_some() {
        CapabilityState::Complete
    } else if profile_layer_id.is_some() {
        CapabilityState::Partial
    } else {
        CapabilityState::NotProvided
    };
    let extents_state = if profile_extent.is_some() {
        CapabilityState::Complete
    } else if profile_layer_id.is_some() {
        CapabilityState::Partial
    } else {
        CapabilityState::NotProvided
    };
    let tool_state = if tools.is_empty() {
        CapabilityState::NotProvided
    } else {
        CapabilityState::Complete
    };
    let mut has_drills = false;
    let mut has_slots = false;
    for feature in &features {
        deadline(deadline_at)?;
        has_drills |= matches!(feature.geometry, Geometry::Drill(_));
        has_slots |= matches!(feature.geometry, Geometry::Slot(_));
    }
    let drill_state = if has_drills {
        if drills_exact {
            CapabilityState::Complete
        } else {
            CapabilityState::Partial
        }
    } else {
        CapabilityState::NotProvided
    };
    let slot_state = if has_slots {
        if drills_exact {
            CapabilityState::Complete
        } else {
            CapabilityState::Partial
        }
    } else {
        CapabilityState::NotProvided
    };
    let plating_state = if tools.is_empty() {
        CapabilityState::NotProvided
    } else if plating_complete {
        CapabilityState::Complete
    } else {
        CapabilityState::Partial
    };
    let span_state = if tools.is_empty() {
        CapabilityState::NotProvided
    } else if spans_complete {
        CapabilityState::Complete
    } else {
        CapabilityState::Partial
    };
    let parse_complete = unsupported_profile.is_empty();
    let document = ManufacturingDocument {
        id: document_id.clone(),
        virtual_path: virtual_path.into(),
        artifact_digest: artifact_digest.clone(),
        format: DocumentFormat::KicadPcb,
        adapter: "ratemypcb-kicad-source".into(),
        adapter_version: KICAD_MANUFACTURING_ADAPTER_VERSION.into(),
        parse_status: if parse_complete {
            ParseStatus::Complete
        } else {
            ParseStatus::Partial
        },
        numeric_format: Some(numeric_format),
        metrics: DocumentMetrics {
            raw_bytes: bytes.len() as u64,
            records: syntax.forms.len() as u64,
            lexical_tokens: syntax.metrics.lexical_tokens,
            metadata_bytes: syntax.metrics.metadata_bytes,
            max_line_bytes,
            max_text_bytes: syntax.metrics.max_text_bytes,
            max_numeric_bytes: syntax.metrics.max_numeric_bytes,
            max_nesting: syntax.metrics.max_nesting,
            ..DocumentMetrics::default()
        },
    };
    let documents = vec![&document];
    let evidence = vec![whole_provenance.clone()];
    let mut capabilities = vec![
        aggregate_capability(
            CapabilityId::ProductIdentity,
            product_state,
            Authority::NativeSource,
            &documents,
            product_provenance.as_slice(),
            "Explicit KiCad title and revision supply native product identity; filenames never do.",
        ),
        aggregate_capability(
            CapabilityId::NativeKicadFacts,
            if parse_complete {
                CapabilityState::Complete
            } else {
                CapabilityState::Partial
            },
            Authority::NativeSource,
            &documents,
            &evidence,
            "Bounded KiCad source facts retain supported layers, profile topology, drills, pads, and nets.",
        ),
        aggregate_capability(
            CapabilityId::LayerRoles,
            CapabilityState::Complete,
            Authority::NativeSource,
            &documents,
            &evidence,
            "The explicit KiCad layer table supplies copper and Edge.Cuts roles.",
        ),
        aggregate_capability(
            CapabilityId::LayerOrder,
            if layer_order_complete {
                CapabilityState::Complete
            } else {
                CapabilityState::Partial
            },
            Authority::NativeSource,
            &documents,
            &evidence,
            "Copper order is the explicit KiCad layer-table order with F.Cu first and B.Cu last.",
        ),
        aggregate_capability(
            CapabilityId::Profile,
            profile_state,
            Authority::NativeSource,
            &documents,
            &evidence,
            "Supported closed Edge.Cuts rectangle and axis-aligned four-line contours retain outer profile and cutouts.",
        ),
        aggregate_capability(
            CapabilityId::Extents,
            extents_state,
            Authority::NativeSource,
            &documents,
            &evidence,
            "Native profile extents are checked fixed-point coordinates.",
        ),
        aggregate_capability(
            CapabilityId::Tools,
            tool_state,
            Authority::NativeSource,
            &documents,
            &evidence,
            "Native drill and slot tools retain finished dimensions.",
        ),
        aggregate_capability(
            CapabilityId::Drills,
            drill_state,
            Authority::NativeSource,
            &documents,
            &evidence,
            "Native via and through-hole drill centers are retained.",
        ),
        aggregate_capability(
            CapabilityId::Slots,
            slot_state,
            Authority::NativeSource,
            &documents,
            &evidence,
            "Native oval pad slots retain transformed endpoints and width.",
        ),
        aggregate_capability(
            CapabilityId::Plating,
            plating_state,
            Authority::NativeSource,
            &documents,
            &evidence,
            "Pad kind and via semantics explicitly determine plating when available.",
        ),
        aggregate_capability(
            CapabilityId::LayerSpans,
            span_state,
            Authority::NativeSource,
            &documents,
            &evidence,
            "Explicit KiCad copper layer lists determine drill spans.",
        ),
        aggregate_capability(
            CapabilityId::Connectivity,
            connectivity_state,
            Authority::NativeSource,
            &documents,
            &evidence,
            "Every eligible native copper pad needs explicit net, component, and pin identity.",
        ),
        aggregate_capability(
            CapabilityId::Components,
            connectivity_state,
            Authority::NativeSource,
            &documents,
            &evidence,
            "Every eligible native copper pad needs explicit component identity.",
        ),
        aggregate_capability(
            CapabilityId::Pins,
            connectivity_state,
            Authority::NativeSource,
            &documents,
            &evidence,
            "Every eligible native copper pad needs explicit pin identity.",
        ),
    ];
    capabilities.sort_by_key(|record| record.id);
    let mut omissions = Vec::new();
    if let Some(form) = unsupported_profile.first().copied() {
        let evidence = provenance(&syntax, &document_id, &artifact_digest, form);
        omissions.push(Omission {
            id: stable_id("omission", &("native-profile-syntax", &evidence.location))
                .map_err(NativeManufacturingError::Canonical)?,
            kind: OmissionKind::UnsupportedRecord,
            affected_capabilities: vec![CapabilityId::NativeKicadFacts, CapabilityId::Profile, CapabilityId::Extents],
            provenance: evidence,
            detail: "Unsupported or ambiguous Edge.Cuts syntax prevents complete native profile evidence.".into(),
        });
    }
    if let Some(provenance) = inexact_drill_provenance {
        let mut affected_capabilities = vec![CapabilityId::Drills];
        if slot_state == CapabilityState::Partial {
            affected_capabilities.push(CapabilityId::Slots);
        }
        omissions.push(Omission {
            id: stable_id(
                "omission",
                &("native-drill-transform", &provenance.location),
            )
            .map_err(NativeManufacturingError::Canonical)?,
            kind: OmissionKind::UnsupportedRecord,
            affected_capabilities,
            provenance,
            detail: "Non-right-angle pad or footprint rotation remains explicit but cannot enter exact drill reconciliation.".into(),
        });
    }
    if connectivity_state == CapabilityState::Partial {
        omissions.push(Omission {
            id: stable_id(
                "omission",
                &("native-connectivity", &whole_provenance.location),
            )
            .map_err(NativeManufacturingError::Canonical)?,
            kind: OmissionKind::MissingSemanticRecord,
            affected_capabilities: vec![
                CapabilityId::Connectivity,
                CapabilityId::Components,
                CapabilityId::Pins,
            ],
            provenance: whole_provenance.clone(),
            detail:
                "At least one eligible native copper pad lacks net, component, or pin identity."
                    .into(),
        });
    }
    let mut review = FabricationReview::empty_with_deadline(deadline_at)
        .map_err(NativeManufacturingError::Canonical)?;
    review.status = FabricationStatus::Partial;
    review.product = product;
    review.documents = vec![document];
    review.layers = layers;
    review.tools = tools;
    review.features = features;
    review.profile = profile;
    review.connectivity = connectivity;
    review.capabilities = CapabilityLedger {
        records: capabilities,
    };
    review.omissions = omissions;
    let authoritative = derive_authoritative_states(
        &review,
        AuthoritativeReviewKind::Native,
        ReconciliationBudget {
            deadline: deadline_at,
        },
    )
    .map_err(NativeManufacturingError::Canonical)?;
    review.profile = authoritative.expected_profile.clone();
    profile_extent = review
        .profile
        .as_ref()
        .and_then(|profile| profile.extents.clone());
    for id in RECONCILIATION_PREREQUISITES {
        let record = review
            .capabilities
            .records
            .iter_mut()
            .find(|record| record.id == id)
            .ok_or(NativeManufacturingError::Invalid {
                reason: "native-capability-ledger",
            })?;
        record.state = authoritative.state(id);
    }
    review.status = authoritative.status;
    review
        .refresh_digests_with_deadline(deadline_at)
        .map_err(NativeManufacturingError::Canonical)?;
    review
        .validate_with_deadline(deadline_at)
        .map_err(NativeManufacturingError::Canonical)?;
    deadline(deadline_at)?;
    Ok(NativeManufacturing {
        review,
        extents: profile_extent,
    })
}

#[derive(Clone, Copy)]
pub(super) struct ReconciliationBudget {
    pub(super) deadline: ManufacturingDeadline,
}

impl ReconciliationBudget {
    pub(super) fn check(self) -> Result<(), FabricationError> {
        self.deadline.check("reconciliation-deadline")
    }
}

fn checked_filter_map<I, T, U, F>(
    items: I,
    budget: ReconciliationBudget,
    mut map: F,
) -> Result<Vec<U>, FabricationError>
where
    I: IntoIterator<Item = T>,
    F: FnMut(T) -> Option<U>,
{
    let mut output = Vec::new();
    for item in items {
        budget.check()?;
        if let Some(value) = map(item) {
            output.push(value);
        }
    }
    Ok(output)
}

fn checked_refs_equal<T: PartialEq>(
    left: &[&T],
    right: &[T],
    budget: ReconciliationBudget,
) -> Result<bool, FabricationError> {
    if left.len() != right.len() {
        return Ok(false);
    }
    for (left, right) in left.iter().zip(right) {
        budget.check()?;
        if *left != right {
            return Ok(false);
        }
    }
    Ok(true)
}

fn serialized<T: Serialize>(
    value: &T,
    budget: ReconciliationBudget,
) -> Result<String, FabricationError> {
    let (_, bytes) =
        serialize_with_deadline(budget.deadline, "reconciliation-deadline", value, true)?;
    let value = String::from_utf8(bytes.expect("retained reconciliation bytes"))
        .map_err(|error| FabricationError::Serialization(error.to_string()))?;
    if value.len() as u64 > RECONCILIATION_VALUE_BYTES {
        return Err(FabricationError::LimitExceeded {
            resource: "reconciliation-value",
        });
    }
    Ok(value)
}

fn fact(
    model_ids: Vec<String>,
    canonical_value: String,
    resolution: Option<Picometres>,
    authority: Authority,
    provenance: ManufacturingProvenance,
    budget: ReconciliationBudget,
) -> Result<ReconciliationFact, FabricationError> {
    let mut unique = BTreeSet::new();
    for model_id in model_ids {
        budget.check()?;
        unique.insert(model_id);
    }
    let mut model_ids = Vec::with_capacity(unique.len());
    for model_id in unique {
        budget.check()?;
        model_ids.push(model_id);
    }
    Ok(ReconciliationFact {
        model_ids,
        canonical_value,
        resolution,
        authority,
        provenance,
    })
}

fn json_resolution_equal(
    left: &JsonValue,
    right: &JsonValue,
    tolerance: i64,
    budget: ReconciliationBudget,
) -> Result<bool, FabricationError> {
    budget.check()?;
    match (left, right) {
        (JsonValue::Number(left), JsonValue::Number(right)) => Ok(left
            .as_i64()
            .zip(right.as_i64())
            .is_some_and(|(left, right)| left.abs_diff(right) <= tolerance as u64)),
        (JsonValue::String(left), JsonValue::String(right)) => chunked_str_equal_with_deadline(
            left,
            right,
            budget.deadline,
            "reconciliation-value-equality",
        ),
        (JsonValue::Array(left), JsonValue::Array(right)) => {
            if left.len() != right.len() {
                return Ok(false);
            }
            for (left, right) in left.iter().zip(right) {
                budget.check()?;
                if !json_resolution_equal(left, right, tolerance, budget)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (JsonValue::Object(left), JsonValue::Object(right)) => {
            if left.len() != right.len() {
                return Ok(false);
            }
            for ((left_key, left), (right_key, right)) in left.iter().zip(right) {
                budget.check()?;
                if !chunked_str_equal_with_deadline(
                    left_key,
                    right_key,
                    budget.deadline,
                    "reconciliation-key-equality",
                )? || !json_resolution_equal(left, right, tolerance, budget)?
                {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(left == right),
    }
}

fn facts_equivalent(
    confidence: ReconciliationConfidence,
    native: &ReconciliationFact,
    package: &ReconciliationFact,
    budget: ReconciliationBudget,
) -> Result<bool, FabricationError> {
    budget.check()?;
    match confidence {
        ReconciliationConfidence::Exact => chunked_str_equal_with_deadline(
            &native.canonical_value,
            &package.canonical_value,
            budget.deadline,
            "reconciliation-value-equality",
        ),
        ReconciliationConfidence::ResolutionBounded => {
            let tolerance = native
                .resolution
                .into_iter()
                .chain(package.resolution)
                .map(|value| value.0)
                .max()
                .unwrap_or(0);
            if tolerance < 0 {
                return Ok(false);
            }
            let parse = |value: &str| {
                parse_canonical_json_with_deadline(
                    value,
                    budget.deadline,
                    "reconciliation-canonical-json",
                )
            };
            let native = match parse(&native.canonical_value) {
                Ok(value) => value,
                Err(error @ FabricationError::LimitExceeded { .. }) => return Err(error),
                Err(_) => return Ok(false),
            };
            let package = match parse(&package.canonical_value) {
                Ok(value) => value,
                Err(error @ FabricationError::LimitExceeded { .. }) => return Err(error),
                Err(_) => return Ok(false),
            };
            json_resolution_equal(&native, &package, tolerance, budget)
        }
        ReconciliationConfidence::Unavailable => Ok(false),
    }
}

pub(super) fn reconciliation_values_equivalent(
    reconciliation: &ManufacturingReconciliation,
    budget: ReconciliationBudget,
) -> Result<bool, FabricationError> {
    facts_equivalent(
        reconciliation.confidence,
        &reconciliation.native,
        &reconciliation.package,
        budget,
    )
}

fn add_reconciliation(
    output: &mut Vec<ManufacturingReconciliation>,
    family: ReconciliationFamily,
    prerequisites: bool,
    native: ReconciliationFact,
    package: ReconciliationFact,
    policy: (ReconciliationConfidence, &str),
    budget: ReconciliationBudget,
) -> Result<(), FabricationError> {
    budget.check()?;
    let (available_confidence, action) = policy;
    let confidence = if prerequisites {
        available_confidence
    } else {
        ReconciliationConfidence::Unavailable
    };
    let status = if !prerequisites {
        ReconciliationStatus::NotChecked
    } else if facts_equivalent(confidence, &native, &package, budget)? {
        ReconciliationStatus::Match
    } else {
        ReconciliationStatus::Mismatch
    };
    output.push(ManufacturingReconciliation {
        id: reconciliation_id_with_deadline(family, &native, &package, budget.deadline)?,
        family,
        status,
        confidence,
        native,
        package,
        smallest_evidence_action: action.into(),
    });
    Ok(())
}

fn materialized_point(
    feature: &ManufacturingFeature,
    point: CanonicalPoint,
) -> Result<CanonicalPoint, FabricationError> {
    feature
        .transforms
        .materialize(point)
        .map(|point| point.point)
}

fn canonical_line(
    feature: &ManufacturingFeature,
    line: &CanonicalLine,
) -> Result<JsonValue, FabricationError> {
    let mut start = materialized_point(feature, line.start)?;
    let mut end = materialized_point(feature, line.end)?;
    if point_order(&start, &end).is_gt() {
        std::mem::swap(&mut start, &mut end);
    }
    Ok(json!(["line", start, end]))
}

fn canonical_arc(
    feature: &ManufacturingFeature,
    arc: &CanonicalArc,
) -> Result<JsonValue, FabricationError> {
    let start = materialized_point(feature, arc.start)?;
    let end = materialized_point(feature, arc.end)?;
    let center = materialized_point(feature, arc.center)?;
    let forward = json!(["arc", start, end, center, arc.direction]);
    let reverse_direction = match arc.direction {
        ArcDirection::Clockwise => ArcDirection::CounterClockwise,
        ArcDirection::CounterClockwise => ArcDirection::Clockwise,
    };
    let reverse = json!(["arc", end, start, center, reverse_direction]);
    Ok(if point_order(&start, &end).is_le() {
        forward
    } else {
        reverse
    })
}

fn canonical_contour(
    feature: &ManufacturingFeature,
    contour: &CanonicalContour,
    budget: ReconciliationBudget,
) -> Result<JsonValue, FabricationError> {
    let mut segments = BTreeMap::new();
    for (index, segment) in contour.segments.iter().enumerate() {
        budget.check()?;
        let value = match segment {
            ContourSegment::Line(line) => canonical_line(feature, line)?,
            ContourSegment::Arc(arc) => canonical_arc(feature, arc)?,
        };
        segments.insert((value.to_string(), index), value);
    }
    let mut ordered = Vec::with_capacity(segments.len());
    for segment in segments.into_values() {
        budget.check()?;
        ordered.push(segment);
    }
    Ok(json!({
        "closed": contour.closed,
        "segments": ordered
    }))
}

fn canonical_profile(
    review: &FabricationReview,
    profile: Option<&BoardProfile>,
    budget: ReconciliationBudget,
) -> Result<String, FabricationError> {
    let Some(profile) = profile else {
        return serialized(&JsonValue::Null, budget);
    };
    budget.check()?;
    let mut features = BTreeMap::new();
    for feature in &review.features {
        budget.check()?;
        features.insert(feature.id.as_str(), feature);
    }
    let collect = |ids: &[String]| -> Result<Vec<JsonValue>, FabricationError> {
        let mut contours = BTreeMap::new();
        let mut sequence = 0_usize;
        for id in ids {
            budget.check()?;
            let feature = features
                .get(id.as_str())
                .copied()
                .ok_or_else(|| FabricationError::DanglingReference(id.clone()))?;
            let mut push = |value: JsonValue| -> Result<(), FabricationError> {
                let key = serialized(&value, budget)?;
                contours.insert((key, sequence), value);
                sequence += 1;
                Ok(())
            };
            match &feature.geometry {
                Geometry::Contour(contour) => push(canonical_contour(feature, contour, budget)?)?,
                Geometry::Region(region) => {
                    for contour in &region.contours {
                        budget.check()?;
                        push(canonical_contour(feature, contour, budget)?)?;
                    }
                }
                Geometry::Line(line) => push(json!({
                    "closed": false,
                    "segments": [canonical_line(feature, line)?]
                }))?,
                Geometry::Arc(arc) => push(json!({
                    "closed": false,
                    "segments": [canonical_arc(feature, arc)?]
                }))?,
                _ => return Err(FabricationError::InvalidIdentity(id.clone())),
            }
        }
        let mut output = Vec::with_capacity(contours.len());
        for contour in contours.into_values() {
            budget.check()?;
            output.push(contour);
        }
        Ok(output)
    };
    serialized(
        &json!({
            "contours": collect(&profile.contour_feature_ids)?,
            "cutouts": collect(&profile.cutout_feature_ids)?,
        }),
        budget,
    )
}

fn layer_span_value(layers: &BTreeMap<&str, &ManufacturingLayer>, id: Option<&String>) -> String {
    id.and_then(|id| layers.get(id.as_str()).copied())
        .map_or_else(
            || "unknown".into(),
            |layer| {
                format!(
                    "{:?}:{}",
                    layer.role,
                    layer
                        .order
                        .map_or_else(|| "?".into(), |order| order.to_string())
                )
            },
        )
}

fn canonical_drills(
    review: &FabricationReview,
    document_ids: Option<&BTreeSet<&str>>,
    budget: ReconciliationBudget,
) -> Result<String, FabricationError> {
    let mut tools = BTreeMap::new();
    for tool in &review.tools {
        budget.check()?;
        tools.insert(tool.id.as_str(), tool);
    }
    let mut layers = BTreeMap::new();
    for layer in &review.layers {
        budget.check()?;
        layers.insert(layer.id.as_str(), layer);
    }
    let mut values = BTreeMap::new();
    let mut sequence = 0_usize;
    for feature in &review.features {
        budget.check()?;
        if document_ids.is_some_and(|ids| !ids.contains(feature.document_id.as_str())) {
            continue;
        }
        let tool = feature
            .tool_id
            .as_deref()
            .and_then(|id| tools.get(id))
            .copied();
        let Some(tool) = tool else { continue };
        let span = tool.span.as_ref();
        let common = json!({
            "plating": tool.plating,
            "from": layer_span_value(&layers, span.and_then(|span| span.from_layer_id.as_ref())),
            "to": layer_span_value(&layers, span.and_then(|span| span.to_layer_id.as_ref())),
        });
        let mut push = |value: JsonValue| {
            values.insert((value.to_string(), sequence), value);
            sequence += 1;
        };
        match &feature.geometry {
            Geometry::Drill(drill) => push(json!({
                "kind": "drill",
                "position": materialized_point(feature, drill.position)?,
                "diameter": drill.diameter,
                "tool": common,
            })),
            Geometry::Slot(slot) => {
                let mut start = materialized_point(feature, slot.start)?;
                let mut end = materialized_point(feature, slot.end)?;
                if point_order(&start, &end).is_gt() {
                    std::mem::swap(&mut start, &mut end);
                }
                push(json!({
                    "kind": "slot",
                    "start": start,
                    "end": end,
                    "width": slot.width,
                    "tool": common,
                }));
            }
            _ => {}
        }
    }
    let mut ordered = Vec::with_capacity(values.len());
    for value in values.into_values() {
        budget.check()?;
        ordered.push(value);
    }
    serialized(&ordered, budget)
}

fn canonical_layers(
    review: &FabricationReview,
    document_ids: &BTreeSet<&str>,
    budget: ReconciliationBudget,
) -> Result<String, FabricationError> {
    let mut copper = BTreeMap::new();
    let mut profile = None;
    let mut native = false;
    for document in &review.documents {
        budget.check()?;
        native |= document_ids.contains(document.id.as_str())
            && document.format == DocumentFormat::KicadPcb;
    }
    for layer in &review.layers {
        budget.check()?;
        if !document_ids.contains(layer.document_id.as_str()) {
            continue;
        }
        match layer.role {
            LayerRole::Copper => {
                let Some(order) = layer.order else {
                    return Err(FabricationError::InvalidIdentity(
                        "canonical-layer-order".into(),
                    ));
                };
                let name = if native {
                    layer.name.clone().ok_or_else(|| {
                        FabricationError::InvalidIdentity("canonical-layer-name".into())
                    })?
                } else if order == 1 {
                    "F.Cu".into()
                } else {
                    String::new()
                };
                if copper
                    .insert(order, (name, layer.side, layer.order))
                    .is_some()
                {
                    return Err(FabricationError::InvalidIdentity(
                        "canonical-layer-order".into(),
                    ));
                }
            }
            LayerRole::Profile
                if profile
                    .replace(("Edge.Cuts".to_owned(), layer.side, layer.order))
                    .is_some() =>
            {
                return Err(FabricationError::InvalidIdentity(
                    "canonical-profile-layer".into(),
                ));
            }
            LayerRole::Profile => {}
            _ => {}
        }
    }
    let copper_count = copper.len();
    let mut values = Vec::new();
    for (index, (order, (mut name, side, supplied_order))) in copper.into_iter().enumerate() {
        budget.check()?;
        if !native {
            name = if index == 0 {
                "F.Cu".into()
            } else if index + 1 == copper_count {
                "B.Cu".into()
            } else {
                format!("In{index}.Cu")
            };
        }
        values.push((name, LayerRole::Copper, side, supplied_order, order));
    }
    if let Some((name, side, order)) = profile {
        values.push((name, LayerRole::Profile, side, order, i32::MAX));
    }
    serialized(&values, budget)
}

fn canonical_connectivity(
    review: &FabricationReview,
    native: bool,
    budget: ReconciliationBudget,
) -> Result<String, FabricationError> {
    let mut native_document_ids = BTreeSet::new();
    for document in &review.documents {
        budget.check()?;
        if document.format == DocumentFormat::KicadPcb {
            native_document_ids.insert(document.id.as_str());
        }
    }
    let mut feature_documents = BTreeMap::new();
    for feature in &review.features {
        budget.check()?;
        feature_documents.insert(feature.id.as_str(), feature.document_id.as_str());
    }
    let mut unique = BTreeSet::new();
    for (index, item) in review.connectivity.iter().enumerate() {
        if index & 0x0fff == 0 {
            budget.check()?;
        }
        if feature_documents
            .get(item.feature_id.as_str())
            .is_some_and(|document| native_document_ids.contains(document) == native)
        {
            unique.insert((&item.net, &item.component, &item.pin));
        }
    }
    serialized(&unique, budget)
}

fn review_provenance(
    review: &FabricationReview,
    format: DocumentFormat,
) -> Result<ManufacturingProvenance, FabricationError> {
    if let Some(provenance) = review
        .layers
        .iter()
        .find(|layer| {
            review
                .documents
                .iter()
                .any(|document| document.id == layer.document_id && document.format == format)
        })
        .map(|layer| layer.provenance.clone())
    {
        return Ok(provenance);
    }
    review
        .documents
        .iter()
        .find(|document| document.format == format)
        .map(inventory_provenance)
        .ok_or_else(|| FabricationError::DanglingReference(format!("{format:?}")))
}

fn append_native_facts(
    target: &mut FabricationReview,
    source: &FabricationReview,
    budget: ReconciliationBudget,
) -> Result<(), FabricationError> {
    for document in &source.documents {
        budget.check()?;
        target.documents.push(document.clone());
    }
    for layer in &source.layers {
        budget.check()?;
        target.layers.push(layer.clone());
    }
    for tool in &source.tools {
        budget.check()?;
        target.tools.push(tool.clone());
    }
    for feature in &source.features {
        budget.check()?;
        target.features.push(feature.clone());
    }
    for item in &source.connectivity {
        budget.check()?;
        target.connectivity.push(item.clone());
    }
    for warning in &source.warnings {
        budget.check()?;
        target.warnings.push(warning.clone());
    }
    for omission in &source.omissions {
        budget.check()?;
        target.warnings.push(ManufacturingWarning {
            code: "native-manufacturing-omission".into(),
            message: omission.detail.clone(),
            provenance: Some(omission.provenance.clone()),
        });
    }
    Ok(())
}

fn derive_reconciliations(
    package: &FabricationReview,
    native_review: &FabricationReview,
    native_extents: &Option<Extent>,
    native_document: &ManufacturingDocument,
    budget: ReconciliationBudget,
) -> Result<Vec<ManufacturingReconciliation>, FabricationError> {
    budget.check()?;
    let package_authoritative =
        derive_authoritative_states(package, AuthoritativeReviewKind::Package, budget)?;
    let native_authoritative =
        derive_authoritative_states(native_review, AuthoritativeReviewKind::Native, budget)?;
    let package_product_provenance = package
        .product
        .as_ref()
        .and_then(|product| product.provenance.first())
        .cloned()
        .unwrap_or_else(|| inventory_provenance(&package.documents[0]));
    let native_product_provenance = native_review
        .product
        .as_ref()
        .and_then(|product| product.provenance.first())
        .cloned()
        .unwrap_or_else(|| inventory_provenance(native_document));
    let package_product_value = package
        .product
        .as_ref()
        .map(|product| (&product.name, &product.revision));
    let native_product_value = native_review
        .product
        .as_ref()
        .map(|product| (&product.name, &product.revision));

    let package_gerber_ids = checked_btree_set_with_deadline(
        checked_filter_map(package.documents.iter(), budget, |document| {
            (document.format == DocumentFormat::Gerber).then_some(document.id.as_str())
        })?,
        budget.deadline,
        "reconciliation-deadline",
    )?;
    let native_document_ids = checked_btree_set_with_deadline(
        [native_document.id.as_str()],
        budget.deadline,
        "reconciliation-deadline",
    )?;
    let package_xnc_ids = checked_btree_set_with_deadline(
        checked_filter_map(package.documents.iter(), budget, |document| {
            (document.format == DocumentFormat::Excellon).then_some(document.id.as_str())
        })?,
        budget.deadline,
        "reconciliation-deadline",
    )?;
    let package_profile = package.profile.as_ref();
    let native_profile = native_review.profile.as_ref();
    let native_resolution = native_document
        .numeric_format
        .as_ref()
        .map(|format| format.resolution);
    let package_provenance = review_provenance(package, DocumentFormat::Gerber)?;
    let package_profile_resolution = package
        .documents
        .iter()
        .find(|document| document.id == package_provenance.document_id)
        .and_then(|document| document.numeric_format.as_ref())
        .map(|format| format.resolution);
    let mut package_physical_extent: Option<Extent> = None;
    let mut package_physical_ids = Vec::new();
    let mut package_extent_resolution: Option<Picometres> = None;
    for bounds in &package.physical_bounds {
        budget.check()?;
        package_physical_ids.push(bounds.id.clone());
        package_extent_resolution = Some(
            package_extent_resolution.map_or(bounds.resolution, |resolution| {
                Picometres(resolution.0.max(bounds.resolution.0))
            }),
        );
        if let Some(extent) = &mut package_physical_extent {
            extent.min.x.0 = extent.min.x.0.min(bounds.extent.min.x.0);
            extent.min.y.0 = extent.min.y.0.min(bounds.extent.min.y.0);
            extent.max.x.0 = extent.max.x.0.max(bounds.extent.max.x.0);
            extent.max.y.0 = extent.max.y.0.max(bounds.extent.max.y.0);
        } else {
            package_physical_extent = Some(bounds.extent.clone());
        }
    }
    let mut package_drill_resolution = None;
    for document in &package.documents {
        budget.check()?;
        if document.format == DocumentFormat::Excellon {
            if let Some(format) = &document.numeric_format {
                package_drill_resolution = Some(
                    package_drill_resolution.map_or(format.resolution, |resolution: Picometres| {
                        Picometres(resolution.0.max(format.resolution.0))
                    }),
                );
            }
        }
    }
    let native_provenance = review_provenance(native_review, DocumentFormat::KicadPcb)?;

    let package_layer_ids = checked_filter_map(package.layers.iter(), budget, |layer| {
        (package_gerber_ids.contains(layer.document_id.as_str())
            && matches!(layer.role, LayerRole::Copper | LayerRole::Profile))
        .then(|| layer.id.clone())
    })?;
    let native_layer_ids = checked_filter_map(native_review.layers.iter(), budget, |layer| {
        matches!(layer.role, LayerRole::Copper | LayerRole::Profile).then(|| layer.id.clone())
    })?;
    let package_drill_ids = checked_filter_map(package.features.iter(), budget, |feature| {
        (package_xnc_ids.contains(feature.document_id.as_str())
            && matches!(feature.geometry, Geometry::Drill(_) | Geometry::Slot(_)))
        .then(|| feature.id.clone())
    })?;
    let native_drill_ids = checked_filter_map(native_review.features.iter(), budget, |feature| {
        matches!(feature.geometry, Geometry::Drill(_) | Geometry::Slot(_))
            .then(|| feature.id.clone())
    })?;
    let package_feature_ids = checked_btree_set_with_deadline(
        checked_filter_map(package.features.iter(), budget, |feature| {
            package_gerber_ids
                .contains(feature.document_id.as_str())
                .then_some(feature.id.as_str())
        })?,
        budget.deadline,
        "reconciliation-deadline",
    )?;
    let package_connectivity_ids =
        checked_filter_map(package.connectivity.iter(), budget, |item| {
            package_feature_ids
                .contains(item.feature_id.as_str())
                .then(|| item.feature_id.clone())
        })?;
    let native_connectivity_ids =
        checked_filter_map(native_review.connectivity.iter(), budget, |item| {
            Some(item.feature_id.clone())
        })?;
    let native_profile_ids = if let Some(profile) = native_profile {
        checked_filter_map(
            profile
                .contour_feature_ids
                .iter()
                .chain(&profile.cutout_feature_ids),
            budget,
            |id| Some(id.clone()),
        )?
    } else {
        vec![native_document.id.clone()]
    };
    let package_profile_ids = if let Some(profile) = package_profile {
        checked_filter_map(
            profile
                .contour_feature_ids
                .iter()
                .chain(&profile.cutout_feature_ids),
            budget,
            |id| Some(id.clone()),
        )?
    } else {
        vec![package.documents[0].id.clone()]
    };
    let native_extent_ids = if let Some(profile) = native_profile {
        checked_filter_map(profile.contour_feature_ids.iter(), budget, |id| {
            Some(id.clone())
        })?
    } else {
        vec![native_document.id.clone()]
    };
    let package_extent_ids = if package_physical_ids.is_empty() {
        if let Some(profile) = package_profile {
            checked_filter_map(profile.contour_feature_ids.iter(), budget, |id| {
                Some(id.clone())
            })?
        } else {
            vec![package.documents[0].id.clone()]
        }
    } else {
        package_physical_ids
    };

    let mut reconciliations = Vec::new();
    add_reconciliation(
        &mut reconciliations,
        ReconciliationFamily::Product,
        reconciliation_prerequisites_complete(
            &package_authoritative,
            &[CapabilityId::ProductIdentity],
        ) && reconciliation_prerequisites_complete(
            &native_authoritative,
            &[CapabilityId::ProductIdentity],
        ),
        fact(
            vec![native_document.id.clone()],
            serialized(&native_product_value, budget)?,
            None,
            Authority::NativeSource,
            native_product_provenance,
            budget,
        )?,
        fact(
            vec![package_product_provenance.document_id.clone()],
            serialized(&package_product_value, budget)?,
            None,
            package
                .product
                .as_ref()
                .map_or(Authority::Unknown, |product| product.authority),
            package_product_provenance,
            budget,
        )?,
        (
            ReconciliationConfidence::Exact,
            "Align the explicit native title/revision and Gerber Job product identity.",
        ),
        budget,
    )?;
    add_reconciliation(
        &mut reconciliations,
        ReconciliationFamily::Layers,
        reconciliation_prerequisites_complete(
            &package_authoritative,
            &[CapabilityId::LayerRoles, CapabilityId::LayerOrder],
        ) && reconciliation_prerequisites_complete(
            &native_authoritative,
            &[CapabilityId::LayerRoles, CapabilityId::LayerOrder],
        ),
        fact(
            native_layer_ids,
            canonical_layers(native_review, &native_document_ids, budget)?,
            None,
            Authority::NativeSource,
            native_provenance.clone(),
            budget,
        )?,
        fact(
            package_layer_ids,
            canonical_layers(package, &package_gerber_ids, budget)?,
            None,
            Authority::Explicit,
            package_provenance.clone(),
            budget,
        )?,
        (
            ReconciliationConfidence::Exact,
            "Regenerate the release package from the selected native layer table.",
        ),
        budget,
    )?;
    add_reconciliation(
        &mut reconciliations,
        ReconciliationFamily::Profile,
        reconciliation_prerequisites_complete(&package_authoritative, &[CapabilityId::Profile])
            && reconciliation_prerequisites_complete(
                &native_authoritative,
                &[CapabilityId::Profile],
            ),
        fact(
            native_profile_ids,
            canonical_profile(native_review, native_profile, budget)?,
            native_resolution,
            Authority::NativeSource,
            native_provenance.clone(),
            budget,
        )?,
        fact(
            package_profile_ids,
            canonical_profile(package, package_profile, budget)?,
            package_profile_resolution,
            Authority::Explicit,
            package_provenance.clone(),
            budget,
        )?,
        (
            ReconciliationConfidence::ResolutionBounded,
            "Regenerate or correct the release profile and cutouts from Edge.Cuts.",
        ),
        budget,
    )?;
    add_reconciliation(
        &mut reconciliations,
        ReconciliationFamily::Drills,
        reconciliation_prerequisites_complete(
            &package_authoritative,
            &[
                CapabilityId::Drills,
                CapabilityId::Tools,
                CapabilityId::Plating,
                CapabilityId::LayerSpans,
            ],
        ) && reconciliation_prerequisites_complete(
            &native_authoritative,
            &[
                CapabilityId::Drills,
                CapabilityId::Tools,
                CapabilityId::Plating,
                CapabilityId::LayerSpans,
            ],
        ),
        fact(
            native_drill_ids,
            canonical_drills(native_review, None, budget)?,
            native_resolution,
            Authority::NativeSource,
            native_provenance.clone(),
            budget,
        )?,
        fact(
            package_drill_ids,
            canonical_drills(package, Some(&package_xnc_ids), budget)?,
            package_drill_resolution,
            Authority::Explicit,
            review_provenance(package, DocumentFormat::Excellon)?,
            budget,
        )?,
        (
            ReconciliationConfidence::ResolutionBounded,
            "Regenerate drill/slot outputs or reconcile finished tool, plating, and span facts.",
        ),
        budget,
    )?;
    add_reconciliation(
        &mut reconciliations,
        ReconciliationFamily::Extents,
        reconciliation_prerequisites_complete(&package_authoritative, &[CapabilityId::Extents])
            && reconciliation_prerequisites_complete(
                &native_authoritative,
                &[CapabilityId::Extents],
            ),
        fact(
            native_extent_ids,
            serialized(native_extents, budget)?,
            native_resolution,
            Authority::NativeSource,
            native_provenance.clone(),
            budget,
        )?,
        fact(
            package_extent_ids,
            serialized(&package_physical_extent, budget)?,
            package_extent_resolution.or(package_profile_resolution),
            Authority::Explicit,
            package_provenance.clone(),
            budget,
        )?,
        (
            ReconciliationConfidence::ResolutionBounded,
            "Regenerate release geometry until package and native extents agree at declared resolution.",
        ),
        budget,
    )?;
    add_reconciliation(
        &mut reconciliations,
        ReconciliationFamily::Connectivity,
        reconciliation_prerequisites_complete(
            &package_authoritative,
            &[
                CapabilityId::Connectivity,
                CapabilityId::Components,
                CapabilityId::Pins,
            ],
        ) && reconciliation_prerequisites_complete(
            &native_authoritative,
            &[
                CapabilityId::Connectivity,
                CapabilityId::Components,
                CapabilityId::Pins,
            ],
        ),
        fact(
            native_connectivity_ids,
            canonical_connectivity(native_review, true, budget)?,
            None,
            Authority::NativeSource,
            native_provenance.clone(),
            budget,
        )?,
        fact(
            package_connectivity_ids,
            canonical_connectivity(package, false, budget)?,
            None,
            Authority::X2,
            package_provenance.clone(),
            budget,
        )?,
        (
            ReconciliationConfidence::Exact,
            "Regenerate X2 object attributes or correct native pad/net identity.",
        ),
        budget,
    )?;

    Ok(reconciliations)
}

pub(super) fn validate_reconciliation_derivation_with_deadline(
    review: &FabricationReview,
    deadline: ManufacturingDeadline,
) -> Result<(), FabricationError> {
    let budget = ReconciliationBudget { deadline };
    budget.check()?;
    let source = review
        .native_reconciliation_source
        .as_ref()
        .ok_or_else(|| {
            FabricationError::DanglingReference("native-reconciliation-source".into())
        })?;
    source.review.validate_with_deadline(deadline)?;
    budget.check()?;
    validate_authoritative_states_for_kind(review, AuthoritativeReviewKind::Package, budget)?;
    validate_authoritative_states_for_kind(
        &source.review,
        AuthoritativeReviewKind::Native,
        budget,
    )?;
    if source.review.source_pair.is_some()
        || source.review.native_reconciliation_source.is_some()
        || !source.review.reconciliations.is_empty()
        || source.review.documents.len() != 1
        || source.review.documents[0].format != DocumentFormat::KicadPcb
        || source.extents
            != source
                .review
                .profile
                .as_ref()
                .and_then(|profile| profile.extents.clone())
    {
        return Err(FabricationError::InvalidIdentity(
            "native-reconciliation-source".into(),
        ));
    }
    let native_document = &source.review.documents[0];
    let pair = review
        .source_pair
        .as_ref()
        .ok_or_else(|| FabricationError::DanglingReference("manufacturing-source-pair".into()))?;
    if native_document.id != pair.native_document_id
        || native_document.artifact_digest != pair.native_artifact_digest
    {
        return Err(FabricationError::InvalidIdentity(
            "native-reconciliation-source".into(),
        ));
    }
    let native_id = native_document.id.as_str();
    let documents = checked_filter_map(review.documents.iter(), budget, |item| {
        (item.id == native_id).then_some(item)
    })?;
    let layers = checked_filter_map(review.layers.iter(), budget, |item| {
        (item.document_id == native_id).then_some(item)
    })?;
    let tools = checked_filter_map(review.tools.iter(), budget, |item| {
        (item.document_id == native_id).then_some(item)
    })?;
    let features = checked_filter_map(review.features.iter(), budget, |item| {
        (item.document_id == native_id).then_some(item)
    })?;
    let native_feature_ids = checked_btree_set_with_deadline(
        checked_filter_map(source.review.features.iter(), budget, |item| {
            Some(item.id.as_str())
        })?,
        budget.deadline,
        "reconciliation-deadline",
    )?;
    let connectivity = checked_filter_map(review.connectivity.iter(), budget, |item| {
        native_feature_ids
            .contains(item.feature_id.as_str())
            .then_some(item)
    })?;
    if !checked_refs_equal(&documents, &source.review.documents, budget)?
        || !checked_refs_equal(&layers, &source.review.layers, budget)?
        || !checked_refs_equal(&tools, &source.review.tools, budget)?
        || !checked_refs_equal(&features, &source.review.features, budget)?
        || !checked_refs_equal(&connectivity, &source.review.connectivity, budget)?
    {
        return Err(FabricationError::InvalidIdentity(
            "native-reconciliation-source".into(),
        ));
    }
    budget.check()?;
    let expected = derive_reconciliations(
        review,
        source.review.as_ref(),
        &source.extents,
        native_document,
        budget,
    )?;
    let supplied_hash = hash_serialized_with_deadline(
        deadline,
        "reconciliation-derived-facts",
        &review.reconciliations,
    )?;
    let expected_hash =
        hash_serialized_with_deadline(deadline, "reconciliation-derived-facts", &expected)?;
    if supplied_hash != expected_hash {
        return Err(FabricationError::InvalidConflict(
            "reconciliation-derived-facts".into(),
        ));
    }
    Ok(())
}

pub fn reconcile_native_package(
    package: FabricationReview,
    native: NativeManufacturing,
) -> Result<FabricationReview, FabricationError> {
    reconcile_native_package_with_timeout(
        package,
        native,
        Duration::from_millis(MANUFACTURING_LIMITS.aggregate_timeout_ms),
    )
}

pub fn reconcile_native_package_with_timeout(
    package: FabricationReview,
    native: NativeManufacturing,
    timeout: Duration,
) -> Result<FabricationReview, FabricationError> {
    reconcile_native_package_with_deadline(
        package,
        native,
        ManufacturingDeadline::from_timeout(timeout).with_aggregate_limit(),
    )
}

pub(crate) fn reconcile_native_package_with_deadline(
    mut package: FabricationReview,
    native: NativeManufacturing,
    deadline: ManufacturingDeadline,
) -> Result<FabricationReview, FabricationError> {
    let budget = ReconciliationBudget { deadline };
    budget.check()?;
    package.validate_with_deadline(deadline)?;
    native.review.validate_with_deadline(deadline)?;
    validate_authoritative_states_for_kind(&package, AuthoritativeReviewKind::Package, budget)?;
    validate_authoritative_states_for_kind(
        &native.review,
        AuthoritativeReviewKind::Native,
        budget,
    )?;
    if package
        .estimated_allocation_bytes
        .checked_add(native.review.estimated_allocation_bytes)
        .is_none_or(|bytes| bytes > MANUFACTURING_LIMITS.canonical_allocation_bytes)
    {
        return Err(FabricationError::LimitExceeded {
            resource: "reconciliation-allocation",
        });
    }
    let aggregate_metric =
        |select: fn(&DocumentMetrics) -> u64, limit: u64| -> Result<bool, FabricationError> {
            let mut total = 0_u64;
            for document in package.documents.iter().chain(&native.review.documents) {
                budget.check()?;
                total = total
                    .checked_add(select(&document.metrics))
                    .ok_or(FabricationError::ArithmeticOverflow)?;
            }
            Ok(total <= limit)
        };
    if !aggregate_metric(
        |metrics| metrics.raw_bytes,
        MANUFACTURING_LIMITS.raw_bytes_aggregate,
    )? || !aggregate_metric(
        |metrics| metrics.records,
        MANUFACTURING_LIMITS.records_aggregate,
    )? || !aggregate_metric(
        |metrics| metrics.lexical_tokens,
        MANUFACTURING_LIMITS.lexical_tokens_aggregate,
    )? {
        return Err(FabricationError::LimitExceeded {
            resource: "reconciliation-aggregate-input",
        });
    }
    budget.check()?;
    if package.documents.is_empty() {
        return Err(FabricationError::DanglingReference(
            "release-package".into(),
        ));
    }
    let native_documents =
        checked_filter_map(native.review.documents.iter(), budget, |document| {
            (document.format == DocumentFormat::KicadPcb).then_some(document)
        })?;
    if native_documents.len() != 1 {
        return Err(FabricationError::DanglingReference(
            "native-document".into(),
        ));
    }
    let native_document = native_documents[0].clone();
    let release_package_id = package.package_id.clone();
    let mut release_document_digests = BTreeSet::new();
    for document in &package.documents {
        budget.check()?;
        release_document_digests.insert(document.artifact_digest.clone());
    }
    let mut ordered_release_digests = Vec::with_capacity(release_document_digests.len());
    for digest in release_document_digests {
        budget.check()?;
        ordered_release_digests.push(digest);
    }
    let release_document_digests = ordered_release_digests;

    let reconciliations = derive_reconciliations(
        &package,
        &native.review,
        &native.extents,
        &native_document,
        budget,
    )?;
    let mut native_capability = None;
    for record in &native.review.capabilities.records {
        budget.check()?;
        if record.id == CapabilityId::NativeKicadFacts {
            native_capability = Some(record.clone());
            break;
        }
    }
    let native_capability = native_capability
        .ok_or_else(|| FabricationError::DanglingReference("native-capability".into()))?;
    append_native_facts(&mut package, &native.review, budget)?;
    let native_reconciliation_source = NativeReconciliationSource {
        review: Box::new(native.review),
        extents: native.extents,
    };
    set_capability(&mut package, native_capability);
    package.source_pair = Some(ManufacturingSourcePair {
        id: source_pair_id_with_deadline(
            &native_document.id,
            &native_document.artifact_digest,
            &release_package_id,
            &release_document_digests,
            deadline,
        )?,
        native_document_id: native_document.id.clone(),
        native_artifact_digest: native_document.artifact_digest.clone(),
        release_package_id,
        release_document_digests,
    });
    package.native_reconciliation_source = Some(native_reconciliation_source);
    package.reconciliations = reconciliations;
    let all_match = package.reconciliations.len() == 6
        && checked_all_with_deadline(
            &package.reconciliations,
            deadline,
            "reconciliation-deadline",
            |item| item.status == ReconciliationStatus::Match,
        )?;
    let mut reconciliation_evidence = Vec::with_capacity(package.reconciliations.len() * 2);
    for item in &package.reconciliations {
        budget.check()?;
        reconciliation_evidence.push(item.native.provenance.clone());
        reconciliation_evidence.push(item.package.provenance.clone());
    }
    let reconciliation_capability = {
        let documents = checked_filter_map(package.documents.iter(), budget, Some)?;
        aggregate_capability(
            CapabilityId::PackageReconciliation,
            if all_match {
                CapabilityState::Complete
            } else {
                CapabilityState::Partial
            },
            Authority::Explicit,
            &documents,
            &reconciliation_evidence,
            "All six symmetric native/package families must match; incomplete evidence is not checked.",
        )
    };
    set_capability(&mut package, reconciliation_capability);
    checked_retain_with_deadline(
        &mut package.omissions,
        deadline,
        "reconciliation-deadline",
        |omission| {
            !omission
                .affected_capabilities
                .contains(&CapabilityId::PackageReconciliation)
        },
    )?;
    if !all_match {
        let evidence = package
            .reconciliations
            .first()
            .map(|item| item.native.provenance.clone())
            .ok_or_else(|| FabricationError::DanglingReference("package-reconciliation".into()))?;
        package.omissions.push(Omission {
            id: stable_id("omission", &("package-reconciliation", &evidence.location))?,
            kind: OmissionKind::MissingSemanticRecord,
            affected_capabilities: vec![CapabilityId::PackageReconciliation],
            provenance: evidence,
            detail:
                "At least one native/package family mismatches or lacks complete prerequisites."
                    .into(),
        });
    }
    let authoritative =
        derive_authoritative_states(&package, AuthoritativeReviewKind::Package, budget)?;
    let mut reconciliation_capability = None;
    for record in &mut package.capabilities.records {
        budget.check()?;
        if record.id == CapabilityId::PackageReconciliation {
            reconciliation_capability = Some(record);
            break;
        }
    }
    reconciliation_capability
        .ok_or_else(|| FabricationError::DanglingReference("package-reconciliation".into()))?
        .state = authoritative.state(CapabilityId::PackageReconciliation);
    package.status = authoritative.status;
    package.finalize_trusted_with_deadline(deadline)?;
    budget.check()?;
    Ok(package)
}

#[cfg(test)]
mod authoritative_budget_tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering as AtomicOrdering},
    };

    #[derive(Clone)]
    struct SlowEq {
        value: usize,
        seen: Arc<AtomicUsize>,
    }

    impl PartialEq for SlowEq {
        fn eq(&self, other: &Self) -> bool {
            self.seen.fetch_add(1, AtomicOrdering::Relaxed);
            std::thread::sleep(Duration::from_micros(25));
            self.value == other.value
        }
    }

    fn high_cardinality_review() -> FabricationReview {
        FabricationReview {
            features: (0..50_000)
                .map(|index| ManufacturingFeature {
                    id: format!("feature-{index}"),
                    document_id: "document".into(),
                    layer_id: "layer".into(),
                    tool_id: None,
                    polarity: LayerPolarity::Dark,
                    geometry: Geometry::Point(CanonicalPoint::new(index, index)),
                    transforms: TransformChain::default(),
                    membership: FeatureMembership::TopLevel,
                    provenance: ManufacturingProvenance {
                        document_id: "document".into(),
                        artifact_digest: "0".repeat(64),
                        producer: "budget-test".into(),
                        producer_version: "1".into(),
                        location: StructuralLocation {
                            record: index as u64,
                            subrecord: None,
                            byte_start: index as u64,
                            byte_end: index as u64,
                        },
                        source_lexeme: None,
                    },
                })
                .collect(),
            ..FabricationReview::default()
        }
    }

    fn square_lines() -> Vec<(CanonicalLine, ManufacturingProvenance)> {
        let points = [
            CanonicalPoint::new(0, 0),
            CanonicalPoint::new(10, 0),
            CanonicalPoint::new(10, 10),
            CanonicalPoint::new(0, 10),
        ];
        (0..4)
            .map(|index| {
                (
                    CanonicalLine {
                        start: points[index],
                        end: points[(index + 1) % 4],
                        width: None,
                    },
                    ManufacturingProvenance {
                        document_id: "document".into(),
                        artifact_digest: "0".repeat(64),
                        producer: "budget-test".into(),
                        producer_version: "1".into(),
                        location: StructuralLocation {
                            record: index as u64,
                            subrecord: None,
                            byte_start: index as u64,
                            byte_end: index as u64,
                        },
                        source_lexeme: None,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn round8_checked_operation_families_expire_mid_operation() {
        const ITEMS: usize = 10_000;
        let seen = Arc::new(AtomicUsize::new(0));
        let scan_seen = Arc::clone(&seen);
        let scan = checked_all_with_deadline(
            0..ITEMS,
            ManufacturingDeadline::from_timeout(Duration::from_millis(2)),
            "round8-checked-scan",
            move |_| {
                scan_seen.fetch_add(1, AtomicOrdering::Relaxed);
                std::thread::sleep(Duration::from_micros(25));
                true
            },
        );
        assert!(matches!(
            scan,
            Err(FabricationError::LimitExceeded {
                resource: "round8-checked-scan"
            })
        ));
        assert!((1..ITEMS).contains(&seen.load(AtomicOrdering::Relaxed)));

        seen.store(0, AtomicOrdering::Relaxed);
        let collect_seen = Arc::clone(&seen);
        let collected = checked_btree_set_with_deadline(
            (0..ITEMS).inspect(move |_| {
                collect_seen.fetch_add(1, AtomicOrdering::Relaxed);
                std::thread::sleep(Duration::from_micros(25));
            }),
            ManufacturingDeadline::from_timeout(Duration::from_millis(2)),
            "round8-checked-collect",
        );
        assert!(matches!(
            collected,
            Err(FabricationError::LimitExceeded {
                resource: "round8-checked-collect"
            })
        ));
        assert!((1..ITEMS).contains(&seen.load(AtomicOrdering::Relaxed)));

        seen.store(0, AtomicOrdering::Relaxed);
        let retain_seen = Arc::clone(&seen);
        let mut retained = (0..ITEMS).collect::<Vec<_>>();
        let retain = checked_retain_with_deadline(
            &mut retained,
            ManufacturingDeadline::from_timeout(Duration::from_millis(2)),
            "round8-checked-retain",
            move |_| {
                retain_seen.fetch_add(1, AtomicOrdering::Relaxed);
                std::thread::sleep(Duration::from_micros(25));
                true
            },
        );
        assert!(matches!(
            retain,
            Err(FabricationError::LimitExceeded {
                resource: "round8-checked-retain"
            })
        ));
        assert!((1..ITEMS).contains(&seen.load(AtomicOrdering::Relaxed)));
        assert_eq!(
            retained.len(),
            ITEMS,
            "failed retention must not partially mutate"
        );

        seen.store(0, AtomicOrdering::Relaxed);
        let left = (0..ITEMS)
            .map(|value| SlowEq {
                value,
                seen: Arc::clone(&seen),
            })
            .collect::<Vec<_>>();
        let right = left.clone();
        let compared = checked_slice_equal_with_deadline(
            &left,
            &right,
            ManufacturingDeadline::from_timeout(Duration::from_millis(2)),
            "round8-checked-compare",
        );
        assert!(matches!(
            compared,
            Err(FabricationError::LimitExceeded {
                resource: "round8-checked-compare"
            })
        ));
        assert!((1..ITEMS).contains(&seen.load(AtomicOrdering::Relaxed)));
    }

    #[test]
    fn authoritative_derivation_high_cardinality_and_tiny_budget_are_bounded() {
        let review = high_cardinality_review();
        let derived = derive_authoritative_states(
            &review,
            AuthoritativeReviewKind::Package,
            ReconciliationBudget {
                deadline: ManufacturingDeadline::from_timeout(Duration::from_secs(5)),
            },
        )
        .unwrap();
        assert_eq!(
            derived.state(CapabilityId::PackageCompleteness),
            CapabilityState::NotProvided
        );

        let tiny = ReconciliationBudget {
            deadline: ManufacturingDeadline::from_timeout(Duration::from_nanos(1)),
        };
        assert!(matches!(
            derive_authoritative_states(&review, AuthoritativeReviewKind::Package, tiny),
            Err(FabricationError::LimitExceeded {
                resource: "reconciliation-deadline"
            })
        ));
        assert!(matches!(
            validate_authoritative_states_for_kind(&review, AuthoritativeReviewKind::Package, tiny,),
            Err(FabricationError::LimitExceeded {
                resource: "reconciliation-deadline"
            })
        ));

        assert_eq!(
            line_profile_pieces(
                "document",
                "layer",
                square_lines(),
                ManufacturingDeadline::from_timeout(Duration::from_secs(1)),
            )
            .unwrap()
            .len(),
            1
        );
        assert!(matches!(
            line_profile_pieces(
                "document",
                "layer",
                square_lines(),
                ManufacturingDeadline::from_timeout(Duration::ZERO),
            ),
            Err(NativeManufacturingError::Resource {
                resource: "native-deadline"
            })
        ));
    }

    #[test]
    fn validation_deadline_expires_cooperatively_during_populated_high_cardinality_scan() {
        let digest = "0".repeat(64);
        let document_id = document_id(&digest, DocumentFormat::KicadPcb).unwrap();
        let document = ManufacturingDocument {
            id: document_id.clone(),
            virtual_path: "board.kicad_pcb".into(),
            artifact_digest: digest.clone(),
            format: DocumentFormat::KicadPcb,
            adapter: "budget-test".into(),
            adapter_version: "1".into(),
            parse_status: ParseStatus::Complete,
            numeric_format: None,
            metrics: DocumentMetrics {
                raw_bytes: 1,
                records: 50_000,
                ..DocumentMetrics::default()
            },
        };
        let layers = (0..50_000_u64)
            .map(|record| {
                let location = StructuralLocation {
                    record,
                    subrecord: None,
                    byte_start: 0,
                    byte_end: 0,
                };
                ManufacturingLayer {
                    id: layer_id(
                        &document_id,
                        Some(&format!("layer-{record}")),
                        LayerRole::Other,
                        LayerSide::NotApplicable,
                        None,
                        Authority::NativeSource,
                        &location,
                    ),
                    document_id: document_id.clone(),
                    name: Some(format!("layer-{record}")),
                    role: LayerRole::Other,
                    side: LayerSide::NotApplicable,
                    context: LayerContext::Board,
                    polarity: LayerPolarity::Unknown,
                    order: None,
                    authority: Authority::NativeSource,
                    provenance: ManufacturingProvenance {
                        document_id: document_id.clone(),
                        artifact_digest: digest.clone(),
                        producer: "budget-test".into(),
                        producer_version: "1".into(),
                        location,
                        source_lexeme: None,
                    },
                }
            })
            .collect();
        let mut review = FabricationReview {
            documents: vec![document],
            layers,
            tools: vec![ManufacturingTool {
                id: tool_id(
                    &document_id,
                    "Drill:T1",
                    &StructuralLocation {
                        record: 0,
                        subrecord: None,
                        byte_start: 0,
                        byte_end: 0,
                    },
                ),
                document_id: document_id.clone(),
                code: "T1".into(),
                kind: ToolKind::Drill,
                diameter: Some(Picometres(1)),
                plating: Plating::Plated,
                span: None,
                provenance: ManufacturingProvenance {
                    document_id: document_id.clone(),
                    artifact_digest: digest.clone(),
                    producer: "budget-test".into(),
                    producer_version: "1".into(),
                    location: StructuralLocation {
                        record: 0,
                        subrecord: None,
                        byte_start: 0,
                        byte_end: 0,
                    },
                    source_lexeme: None,
                },
            }],
            profile: Some(BoardProfile {
                contour_feature_ids: vec![],
                cutout_feature_ids: vec![],
                extents: Some(Extent {
                    min: CanonicalPoint::new(0, 0),
                    max: CanonicalPoint::new(10, 10),
                }),
                provenance: vec![],
            }),
            connectivity: vec![ObjectSemantics {
                feature_id: "pending".into(),
                net: Some("GND".into()),
                component: Some("U1".into()),
                pin: Some("1".into()),
                provenance: ManufacturingProvenance {
                    document_id: document_id.clone(),
                    artifact_digest: digest,
                    producer: "budget-test".into(),
                    producer_version: "1".into(),
                    location: StructuralLocation {
                        record: 0,
                        subrecord: None,
                        byte_start: 0,
                        byte_end: 0,
                    },
                    source_lexeme: None,
                },
            }],
            ..FabricationReview::default()
        };
        review.refresh_digests().unwrap();
        let deadline = ManufacturingDeadline::from_timeout(Duration::from_micros(500));
        deadline.check("pre-entry").unwrap();
        assert!(matches!(
            review.validate_with_deadline(deadline),
            Err(FabricationError::LimitExceeded {
                resource: "fabrication-limits-validation"
            })
        ));
    }

    #[test]
    fn validation_deadline_expires_inside_live_hundred_thousand_segment_route() {
        let digest = "1".repeat(64);
        let document_id = document_id(&digest, DocumentFormat::KicadPcb).unwrap();
        let location = StructuralLocation {
            record: 0,
            subrecord: None,
            byte_start: 0,
            byte_end: 0,
        };
        let layer_id = layer_id(
            &document_id,
            Some("Route"),
            LayerRole::Route,
            LayerSide::NotApplicable,
            None,
            Authority::NativeSource,
            &location,
        );
        let tool_id = tool_id(&document_id, "Route:T1", &location);
        let segments = (0..MANUFACTURING_LIMITS.drill_route_features)
            .map(|_| {
                ContourSegment::Arc(CanonicalArc {
                    start: CanonicalPoint::new(1, 0),
                    end: CanonicalPoint::new(0, 1),
                    center: CanonicalPoint::new(0, 0),
                    direction: ArcDirection::CounterClockwise,
                    quadrant: QuadrantMode::Single,
                    width: Some(Picometres(1)),
                    source_resolution: Picometres(1),
                })
            })
            .collect::<Vec<_>>();
        let feature = ManufacturingFeature {
            id: feature_id(&document_id, &layer_id, "route", &location),
            document_id: document_id.clone(),
            layer_id: layer_id.clone(),
            tool_id: Some(tool_id.clone()),
            polarity: LayerPolarity::Dark,
            geometry: Geometry::Route(RouteFeature {
                segments,
                tool_id: tool_id.clone(),
            }),
            transforms: TransformChain::default(),
            membership: FeatureMembership::TopLevel,
            provenance: ManufacturingProvenance {
                document_id: document_id.clone(),
                artifact_digest: digest.clone(),
                producer: "budget-test".into(),
                producer_version: "1".into(),
                location: location.clone(),
                source_lexeme: None,
            },
        };
        let review = FabricationReview {
            documents: vec![ManufacturingDocument {
                id: document_id.clone(),
                virtual_path: "board.kicad_pcb".into(),
                artifact_digest: digest.clone(),
                format: DocumentFormat::KicadPcb,
                adapter: "budget-test".into(),
                adapter_version: "1".into(),
                parse_status: ParseStatus::Complete,
                numeric_format: None,
                metrics: DocumentMetrics {
                    raw_bytes: 1,
                    records: 1,
                    ..DocumentMetrics::default()
                },
            }],
            layers: vec![ManufacturingLayer {
                id: layer_id,
                document_id: document_id.clone(),
                name: Some("Route".into()),
                role: LayerRole::Route,
                side: LayerSide::NotApplicable,
                context: LayerContext::Board,
                polarity: LayerPolarity::Dark,
                order: None,
                authority: Authority::NativeSource,
                provenance: ManufacturingProvenance {
                    document_id: document_id.clone(),
                    artifact_digest: digest.clone(),
                    producer: "budget-test".into(),
                    producer_version: "1".into(),
                    location: location.clone(),
                    source_lexeme: None,
                },
            }],
            tools: vec![ManufacturingTool {
                id: tool_id,
                document_id: document_id.clone(),
                code: "T1".into(),
                kind: ToolKind::Route,
                diameter: Some(Picometres(1)),
                plating: Plating::Unknown,
                span: None,
                provenance: ManufacturingProvenance {
                    document_id,
                    artifact_digest: digest,
                    producer: "budget-test".into(),
                    producer_version: "1".into(),
                    location,
                    source_lexeme: None,
                },
            }],
            features: vec![feature],
            ..FabricationReview::default()
        };
        review
            .validate_limits(ManufacturingDeadline::from_timeout(Duration::from_secs(5)))
            .unwrap();
        let mut one_over = review.clone();
        let Geometry::Route(route) = &mut one_over.features[0].geometry else {
            unreachable!();
        };
        route.segments.push(route.segments[0].clone());
        assert!(matches!(
            one_over.validate_limits(ManufacturingDeadline::from_timeout(Duration::from_secs(5))),
            Err(FabricationError::LimitExceeded {
                resource: "canonical-model"
            })
        ));

        let deadline = ManufacturingDeadline::from_timeout(Duration::from_micros(500));
        deadline.check("pre-validation-entry").unwrap();
        assert!(matches!(
            review.validate_with_deadline(deadline),
            Err(FabricationError::LimitExceeded { .. })
        ));

        let Geometry::Route(route) = &review.features[0].geometry else {
            unreachable!();
        };
        assert_eq!(
            route.segments.len(),
            MANUFACTURING_LIMITS.drill_route_features
        );
        let aperture_ids = HashSet::new();
        let tool_ids = HashSet::from([review.tools[0].id.as_str()]);
        let deadline = ManufacturingDeadline::from_timeout(Duration::from_micros(500));
        deadline.check("pre-nested-entry").unwrap();
        assert!(matches!(
            validate_geometry(
                &review.features[0].geometry,
                &aperture_ids,
                &tool_ids,
                deadline,
            ),
            Err(FabricationError::LimitExceeded {
                resource: "fabrication-geometry-validation"
            })
        ));
        let deadline = ManufacturingDeadline::from_timeout(Duration::from_micros(500));
        deadline.check("pre-transformed-entry").unwrap();
        assert!(matches!(
            validate_transformed_geometry(
                &review.features[0].geometry,
                &review.features[0].transforms,
                deadline,
            ),
            Err(FabricationError::LimitExceeded {
                resource: "fabrication-transformed-geometry-validation"
            })
        ));

        let deadline = ManufacturingDeadline::from_timeout(Duration::from_micros(500));
        deadline.check("pre-canonicalization").unwrap();
        assert!(matches!(
            canonical_json_with_deadline(deadline, "features", &review.features),
            Err(FabricationError::LimitExceeded {
                resource: "fabrication-model-digest"
            })
        ));
    }

    #[test]
    fn round7_deadline_canonical_reconciliation_parse_and_equality_are_cooperative() {
        struct CountingReader {
            bytes: Vec<u8>,
            offset: usize,
            consumed: usize,
        }

        impl std::io::Read for CountingReader {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                if self.offset == self.bytes.len() {
                    return Ok(0);
                }
                std::thread::sleep(Duration::from_micros(100));
                let count = buffer
                    .len()
                    .min(64)
                    .min(self.bytes.len().saturating_sub(self.offset));
                buffer[..count].copy_from_slice(&self.bytes[self.offset..self.offset + count]);
                self.offset += count;
                self.consumed += count;
                Ok(count)
            }
        }

        let mut source = b"[".to_vec();
        source.extend_from_slice(b"0,".repeat(100_000).as_slice());
        source.extend_from_slice(b"0]");
        let source_len = source.len();
        let mut reader = CountingReader {
            bytes: source,
            offset: 0,
            consumed: 0,
        };
        assert!(matches!(
            canonical_json_from_reader_with_deadline(
                &mut reader,
                ManufacturingDeadline::from_timeout(Duration::from_millis(2)),
                "reconciliation-canonical-json",
            ),
            Err(FabricationError::LimitExceeded {
                resource: "reconciliation-canonical-json"
            })
        ));
        assert!(reader.consumed > 0 && reader.consumed < source_len);

        let values = JsonValue::Array(vec![JsonValue::from(1); 100_000]);
        let budget = ReconciliationBudget {
            deadline: ManufacturingDeadline::from_timeout(Duration::from_micros(500)),
        };
        assert!(matches!(
            json_resolution_equal(&values, &values, 0, budget),
            Err(FabricationError::LimitExceeded { .. })
        ));
    }

    #[test]
    fn deadline_serialization_interrupts_before_all_hundred_thousand_records() {
        use serde::ser::SerializeSeq;
        use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

        struct CountingSegments<'a> {
            seen: &'a AtomicUsize,
            delay: bool,
        }

        impl Serialize for CountingSegments<'_> {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                let segment = ContourSegment::Line(CanonicalLine {
                    start: CanonicalPoint::new(0, 0),
                    end: CanonicalPoint::new(1, 1),
                    width: Some(Picometres(1)),
                });
                let mut sequence = serializer.serialize_seq(Some(100_000))?;
                for index in 0..100_000 {
                    self.seen.fetch_add(1, AtomicOrdering::Relaxed);
                    if self.delay && index % 64 == 0 {
                        std::thread::sleep(Duration::from_micros(50));
                    }
                    sequence.serialize_element(&segment)?;
                }
                sequence.end()
            }
        }

        let seen = AtomicUsize::new(0);
        let deadline = ManufacturingDeadline::from_timeout(Duration::from_micros(500));
        deadline.check("pre-serialization").unwrap();
        assert!(matches!(
            canonical_json_with_deadline(
                deadline,
                "segments",
                &CountingSegments {
                    seen: &seen,
                    delay: true,
                },
            ),
            Err(FabricationError::LimitExceeded {
                resource: "fabrication-model-digest"
            })
        ));
        assert!(seen.load(AtomicOrdering::Relaxed) < 100_000);

        seen.store(0, AtomicOrdering::Relaxed);
        canonical_json_with_deadline(
            ManufacturingDeadline::from_timeout(Duration::from_secs(5)),
            "segments",
            &CountingSegments {
                seen: &seen,
                delay: false,
            },
        )
        .unwrap();
        assert_eq!(seen.load(AtomicOrdering::Relaxed), 100_000);
    }
}
