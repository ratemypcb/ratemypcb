use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::{Error, forms, quoted, scalar};

pub type CoreError = Error;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StackupSource {
    Kicad,
    Gerber,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StackupLayerKind {
    Copper,
    Dielectric,
    Core,
    Prepreg,
    Mask,
    Silkscreen,
    Substrate,
    Other,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StackupLayer {
    pub name: String,
    pub kind: StackupLayerKind,
    pub thickness_mm: Option<f64>,
    pub material: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Stackup {
    pub source: StackupSource,
    pub layer_count: u32,
    pub layers: Vec<StackupLayer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thickness_mm: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl Stackup {
    pub fn from_kicad_source(board_s_expr: &str) -> Result<Stackup, Error> {
        let Some(stackup) = forms(board_s_expr, "stackup").into_iter().next() else {
            return Err(Error::Invalid(
                "Board contains no (stackup) section; provide Gerbers for layer-count inference."
                    .into(),
            ));
        };
        let mut layers = Vec::new();
        let mut warnings = Vec::new();
        let mut explicit_copper = 0_usize;
        let mut wildcard_copper = false;
        let mut copper_thicknesses = 0_usize;
        let mut dielectric_thicknesses = 0_usize;
        for form in forms(stackup, "layer") {
            let Some(name) = quoted(form, "layer").filter(|name| !name.is_empty()) else {
                warnings.push("Skipped a stackup layer entry without a layer name.".into());
                continue;
            };
            let declared_type = type_atom(form);
            let kind = match declared_type.as_deref().and_then(classify_kind) {
                Some(kind) => kind,
                None => {
                    warnings.push(format!(
                        "Stackup layer \"{name}\" has an unrecognized or missing type{}; treated as other.",
                        declared_type
                            .as_deref()
                            .map(|value| format!(" \"{value}\""))
                            .unwrap_or_default()
                    ));
                    StackupLayerKind::Other
                }
            };
            let thickness_mm = match scalar(form, "thickness") {
                Some(value) if value.is_finite() && value > 0.0 => Some(value),
                Some(_) => {
                    warnings.push(format!(
                        "Stackup layer \"{name}\" has an invalid thickness; it was ignored."
                    ));
                    None
                }
                None => None,
            };
            match kind {
                StackupLayerKind::Copper => {
                    if name == "*.Cu" {
                        wildcard_copper = true;
                    } else {
                        explicit_copper += 1;
                    }
                    if thickness_mm.is_some() {
                        copper_thicknesses += 1;
                    }
                }
                StackupLayerKind::Dielectric
                | StackupLayerKind::Core
                | StackupLayerKind::Prepreg
                    if thickness_mm.is_some() =>
                {
                    dielectric_thicknesses += 1;
                }
                _ => {}
            }
            layers.push(StackupLayer {
                name,
                kind,
                thickness_mm,
                material: quoted(form, "material"),
            });
        }
        let mut layer_count = explicit_copper as u32;
        if wildcard_copper {
            let table_count = copper_table_count(board_s_expr);
            if table_count == 0 {
                warnings.push(
                    "The stackup \"*.Cu\" entry could not be expanded against the board layer table."
                        .into(),
                );
            } else {
                layer_count += table_count as u32;
            }
        }
        if let Some(finish) = quoted(stackup, "copper_finish").filter(|v| !v.trim().is_empty()) {
            warnings.push(format!("The stackup declares copper finish \"{finish}\"."));
        }
        let thickness_mm = (copper_thicknesses > 0 && dielectric_thicknesses > 0).then(|| {
            layers
                .iter()
                .filter_map(|layer| layer.thickness_mm)
                .sum::<f64>()
        });
        Ok(Stackup {
            source: StackupSource::Kicad,
            layer_count,
            layers,
            thickness_mm,
            warnings,
        })
    }

    pub fn from_gerber_filename_inference(filenames: &[&str]) -> Stackup {
        let matched: BTreeSet<String> = filenames
            .iter()
            .map(|filename| {
                filename
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or(filename)
                    .to_ascii_lowercase()
            })
            .filter(|name| copper_gerber(name))
            .collect();
        Stackup {
            source: StackupSource::Gerber,
            layer_count: matched.len() as u32,
            layers: vec![],
            thickness_mm: None,
            warnings: vec![format!(
                "partial filename inference from {} Gerber filenames; not construction evidence",
                matched.len()
            )],
        }
    }
}

fn classify_kind(declared: &str) -> Option<StackupLayerKind> {
    match declared.to_ascii_lowercase().as_str() {
        "copper" => Some(StackupLayerKind::Copper),
        "dielectric" => Some(StackupLayerKind::Dielectric),
        "core" => Some(StackupLayerKind::Core),
        "prepreg" => Some(StackupLayerKind::Prepreg),
        "mask" => Some(StackupLayerKind::Mask),
        "silkscreen" => Some(StackupLayerKind::Silkscreen),
        "substrate" => Some(StackupLayerKind::Substrate),
        "paste" | "adhesive" | "user" => Some(StackupLayerKind::Other),
        _ => None,
    }
}

fn type_atom(form: &str) -> Option<String> {
    if let Some(value) = quoted(form, "type") {
        return Some(value);
    }
    let start = form.find("(type ")? + 6;
    form[start..]
        .split([')', ' ', '\t', '\r', '\n'])
        .next()
        .map(str::to_string)
        .filter(|atom| !atom.is_empty())
}

fn copper_table_count(source: &str) -> usize {
    forms(source, "layers")
        .into_iter()
        .next()
        .map(|table| {
            table[1..table.len().saturating_sub(1)]
                .split('(')
                .filter(|entry| {
                    let mut tokens = entry
                        .split(|character: char| character.is_whitespace() || character == ')')
                        .filter(|token| !token.is_empty());
                    tokens.next().is_some_and(|id| id.parse::<u32>().is_ok())
                        && tokens.next().is_some_and(|name| name.starts_with('"'))
                        && tokens.next() == Some("signal")
                })
                .count()
        })
        .unwrap_or(0)
}

fn copper_gerber(name: &str) -> bool {
    match name.rsplit_once('.') {
        Some((stem, extension)) => copper_extension(extension) || copper_stem(stem),
        None => copper_stem(name),
    }
}

fn copper_extension(extension: &str) -> bool {
    if matches!(extension, "gtl" | "gbl") {
        return true;
    }
    let Some(rest) = extension.strip_prefix('g') else {
        return false;
    };
    if let Some(plane) = rest.strip_prefix('p') {
        return matches!(plane.parse::<u32>(), Ok(1..=9));
    }
    matches!(rest.parse::<u32>(), Ok(1..=99))
}

fn copper_stem(stem: &str) -> bool {
    stem.contains("f_cu") || stem.contains("b_cu") || contains_inner_cu(stem)
}

fn contains_inner_cu(stem: &str) -> bool {
    for (index, _) in stem.match_indices("in") {
        let boundary_free = index == 0
            || !stem[..index]
                .chars()
                .next_back()
                .is_some_and(|character| character.is_ascii_alphanumeric());
        let tail = &stem[index + 2..];
        let digits = tail
            .bytes()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        if boundary_free && digits > 0 && tail[digits..].starts_with("_cu") {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(actual: Option<f64>, expected: f64) -> bool {
        actual.is_some_and(|actual| (actual - expected).abs() < 1e-9)
    }

    const TWO_LAYER: &str = r#"(kicad_pcb (version 20240108) (setup (stackup
        (layer "F.SilkS" (type "Silkscreen"))
        (layer "F.Paste" (type "Paste"))
        (layer "F.Mask" (type "Mask") (thickness 0.01))
        (layer "F.Cu" (type "Copper") (thickness 0.035))
        (layer "dielectric 1" (type "Core") (thickness 1.51) (material "FR4") (epsilon_r 4.5))
        (layer "B.Cu" (type "Copper") (thickness 0.035))
        (layer "B.Mask" (type "Mask") (thickness 0.01))
        (layer "B.Paste" (type "Paste"))
        (layer "B.SilkS" (type "Silkscreen"))
        (copper_finish "ENIG")
    )))"#;

    const FOUR_LAYER: &str = r#"(kicad_pcb (version 20240108) (setup (stackup
        (layer "F.Cu" (type "Copper") (thickness 0.035))
        (layer "dielectric 1" (type "Prepreg") (thickness 0.2104) (material "FR4"))
        (layer "In1.Cu" (type "Copper") (thickness 0.0175))
        (layer "dielectric 2" (type "Core") (thickness 0.5079) (material "FR4"))
        (layer "In2.Cu" (type "Copper") (thickness 0.0175))
        (layer "dielectric 3" (type "Prepreg") (thickness 0.2104) (material "FR4"))
        (layer "B.Cu" (type "Copper") (thickness 0.035))
    )))"#;

    #[test]
    fn parses_two_layer_stackup() {
        let stackup = Stackup::from_kicad_source(TWO_LAYER).unwrap();
        assert_eq!(stackup.source, StackupSource::Kicad);
        assert_eq!(stackup.layer_count, 2);
        assert_eq!(stackup.layers.len(), 9);
        assert_eq!(stackup.layers[0].kind, StackupLayerKind::Silkscreen);
        assert_eq!(stackup.layers[1].kind, StackupLayerKind::Other);
        assert_eq!(stackup.layers[2].kind, StackupLayerKind::Mask);
        assert_eq!(stackup.layers[3].name, "F.Cu");
        assert_eq!(stackup.layers[3].kind, StackupLayerKind::Copper);
        assert_eq!(stackup.layers[3].thickness_mm, Some(0.035));
        assert_eq!(stackup.layers[4].kind, StackupLayerKind::Core);
        assert_eq!(stackup.layers[4].material.as_deref(), Some("FR4"));
        assert!(approx(stackup.thickness_mm, 1.6));
        assert!(stackup.warnings.iter().any(|w| w.contains("ENIG")));
    }

    #[test]
    fn parses_four_layer_core_and_prepreg() {
        let stackup = Stackup::from_kicad_source(FOUR_LAYER).unwrap();
        assert_eq!(stackup.layer_count, 4);
        let dielectrics: Vec<_> = stackup
            .layers
            .iter()
            .filter(|layer| {
                matches!(
                    layer.kind,
                    StackupLayerKind::Core | StackupLayerKind::Prepreg
                )
            })
            .collect();
        assert_eq!(
            dielectrics
                .iter()
                .map(|layer| layer.kind)
                .collect::<Vec<_>>(),
            [
                StackupLayerKind::Prepreg,
                StackupLayerKind::Core,
                StackupLayerKind::Prepreg
            ]
        );
        assert!(approx(stackup.thickness_mm, 1.0337));
        assert!(stackup.warnings.is_empty());
    }

    #[test]
    fn missing_stackup_section_is_an_error() {
        let error = Stackup::from_kicad_source("(kicad_pcb (version 20240108))").unwrap_err();
        assert!(matches!(error, Error::Invalid(_)));
    }

    #[test]
    fn malformed_entries_become_warnings() {
        let source = r#"(kicad_pcb (version 20240108) (setup (stackup
            (layer (type "Copper"))
            (layer "F.Cu" (type "Copper") (thickness 0.035))
            (layer "mystery 1" (type "Bogus"))
            (layer "dielectric 1" (type "Core") (thickness 1.6) (material "FR4"))
            (layer "X.Mask" (type "Mask") (thickness -1))
        )))"#;
        let stackup = Stackup::from_kicad_source(source).unwrap();
        assert_eq!(stackup.layer_count, 1);
        assert_eq!(stackup.layers.len(), 4);
        assert_eq!(stackup.layers[1].kind, StackupLayerKind::Other);
        assert_eq!(stackup.layers[1].thickness_mm, None);
        assert_eq!(stackup.warnings.len(), 3);
        assert!(approx(stackup.thickness_mm, 1.635));
    }

    #[test]
    fn accepts_unquoted_type_atoms() {
        let source = r#"(kicad_pcb (version 20240108) (setup (stackup
            (layer "F.Cu" (type copper) (thickness 0.035))
            (layer "dielectric 1" (type dielectric) (material "FR4") (thickness 1.6))
        )))"#;
        let stackup = Stackup::from_kicad_source(source).unwrap();
        assert_eq!(stackup.layer_count, 1);
        assert_eq!(stackup.layers[0].kind, StackupLayerKind::Copper);
        assert_eq!(stackup.layers[1].kind, StackupLayerKind::Dielectric);
        assert!(approx(stackup.thickness_mm, 1.635));
    }

    #[test]
    fn expands_wildcard_copper_against_layer_table() {
        let source = r#"(kicad_pcb (version 20211014)
            (layers (0 "F.Cu" signal) (31 "B.Cu" signal) (39 "F.Silkscreen" silk) (44 "Edge.Cuts" user))
            (setup (stackup
                (layer "F.Silkscreen" (type "Silkscreen"))
                (layer "F.Mask" (type "Mask") (thickness 0.01))
                (layer "*.Cu" (type "Copper") (thickness 0.035))
                (layer "dielectric 1" (type "Core") (thickness 1.51) (epsilon_r 4.5))
                (layer "B.Mask" (type "Mask") (thickness 0.01))
            ))
        )"#;
        let stackup = Stackup::from_kicad_source(source).unwrap();
        assert_eq!(stackup.layer_count, 2);
        assert!(approx(stackup.thickness_mm, 1.565));
    }

    #[test]
    fn infers_kicad_style_gerber_layers() {
        let stackup = Stackup::from_gerber_filename_inference(&[
            "fab/proj-F_Cu.gbr",
            "fab/proj-In1_Cu.gbr",
            "fab/proj-B_cu.GBR",
            "fab/proj-F_Mask.gbr",
            "fab/proj-Edge_Cuts.gbr",
            "fab/proj.drl",
        ]);
        assert_eq!(stackup.source, StackupSource::Gerber);
        assert_eq!(stackup.layer_count, 3);
        assert!(stackup.layers.is_empty());
        assert_eq!(stackup.thickness_mm, None);
        assert_eq!(
            stackup.warnings,
            vec!["partial filename inference from 3 Gerber filenames; not construction evidence"]
        );
    }

    #[test]
    fn infers_protel_style_gerber_layers() {
        let stackup = Stackup::from_gerber_filename_inference(&[
            "board.GTL",
            "board.g1",
            "board.gp1",
            "board.gbl",
            "board.gts",
            "board.gbo",
        ]);
        assert_eq!(stackup.layer_count, 4);
    }

    #[test]
    fn dedupes_case_insensitive_gerber_names() {
        let stackup = Stackup::from_gerber_filename_inference(&[
            "Board-F_Cu.gbr",
            "board-f_cu.gbr",
            "BOARD-B_CU.GBR",
        ]);
        assert_eq!(stackup.layer_count, 2);
    }

    #[test]
    fn serializes_exact_contract_json() {
        let stackup = Stackup {
            source: StackupSource::Kicad,
            layer_count: 4,
            layers: vec![StackupLayer {
                name: "F.Cu".into(),
                kind: StackupLayerKind::Copper,
                thickness_mm: Some(0.035),
                material: None,
            }],
            thickness_mm: Some(1.6),
            warnings: vec!["x".into()],
        };
        assert_eq!(
            serde_json::to_string(&stackup).unwrap(),
            r#"{"source":"kicad","layerCount":4,"layers":[{"name":"F.Cu","kind":"copper","thicknessMm":0.035,"material":null}],"thicknessMm":1.6,"warnings":["x"]}"#
        );
        let round_trip: Stackup =
            serde_json::from_str(&serde_json::to_string(&stackup).unwrap()).unwrap();
        assert_eq!(round_trip, stackup);
    }

    #[test]
    fn omits_optional_fields_when_absent() {
        let stackup = Stackup::from_gerber_filename_inference(&[]);
        let json = serde_json::to_value(&stackup).unwrap();
        assert!(json.get("thicknessMm").is_none());
        assert_eq!(
            json.get("warnings"),
            Some(&serde_json::json!([
                "partial filename inference from 0 Gerber filenames; not construction evidence"
            ]))
        );
        let empty = Stackup {
            warnings: vec![],
            ..stackup.clone()
        };
        assert!(
            serde_json::to_value(&empty)
                .unwrap()
                .get("warnings")
                .is_none()
        );
    }
}
