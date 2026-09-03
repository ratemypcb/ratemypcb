use ratemypcb_core::fabrication::{
    GerberParseError, MANUFACTURING_LIMITS, ManufacturingInput, ManufacturingKindCandidate,
    parse_gerber_document,
};
use sha2::{Digest, Sha256};

fn block_count_document(count: usize) -> Vec<u8> {
    let mut bytes = b"%FSLAX46Y46*%%MOMM*%".to_vec();
    for index in 0..count {
        let code = 10 + index;
        bytes.extend_from_slice(format!("%ABD{code}*%G36*X0D1*G37*%AB*%\n").as_bytes());
    }
    bytes.extend_from_slice(b"M02*");
    bytes
}

fn parse(
    bytes: Vec<u8>,
) -> Result<ratemypcb_core::fabrication::GerberProduction, GerberParseError> {
    let input = ManufacturingInput {
        virtual_path: "fab/generated.gbr".into(),
        artifact_digest: format!("{:x}", Sha256::digest(&bytes)),
        kind_candidate: ManufacturingKindCandidate::Gerber,
        size: bytes.len() as u64,
        original_bytes: bytes,
        file_started: None,
    };
    parse_gerber_document(&input)
}

#[test]
fn gerber_hostile_aperture_block_count_is_exact_and_over() {
    let exact = parse(block_count_document(MANUFACTURING_LIMITS.apertures)).unwrap();
    assert_eq!(exact.review.blocks.len(), MANUFACTURING_LIMITS.apertures);
    assert_eq!(exact.review.physical_bounds.len(), 1);
    assert_eq!(exact.review.physical_bounds[0].geometry_digest.len(), 64);

    assert!(matches!(
        parse(block_count_document(MANUFACTURING_LIMITS.apertures + 1)),
        Err(GerberParseError::Resource {
            resource: "apertures",
            ..
        })
    ));
}
