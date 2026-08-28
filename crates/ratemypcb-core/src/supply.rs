use crate::{
    AlternateCandidateReview, ApprovedAlternateReview, BomJudgment, BomLineReview, Error,
    LifecycleReview, ProviderCheckReview, SupplyOfferReview,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const MAX_PARTS: usize = 10_000;
const MAX_OBSERVATIONS: usize = 50_000;
const MAX_TEXT: usize = 512;
const MAX_AGE_SECONDS: u64 = 86_400;
const MAX_FUTURE_SKEW_SECONDS: u64 = 300;
const MAX_LIFECYCLE_ASSERTIONS: usize = 32;
const MAX_OFFERS_PER_PART: usize = 256;
const MAX_PRICE_BREAKS: usize = 64;
const MAX_ALTERNATES: usize = 64;
const MAX_APPROVED_ALTERNATES: usize = 32;
const MAX_EVIDENCE_REFS: usize = 32;
const NAMED_PROVIDERS: [&str; 3] = ["mouser", "digikey", "lcsc"];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Snapshot {
    schema_version: String,
    generated_at_unix: u64,
    expires_at_unix: u64,
    legal_expires_at_unix: u64,
    demand: Demand,
    terms: Vec<TermsProfile>,
    parts: Vec<Part>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Demand {
    build_quantity: u64,
    attrition_bps: u32,
    spares: u64,
    region: String,
    currency: String,
    packaging: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TermsProfile {
    provider: String,
    decision: TermsDecision,
    query: Permission,
    memory_retention: Permission,
    disk_retention: Permission,
    html_embedding: Permission,
    sharing: Permission,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum TermsDecision {
    Approved,
    SyntheticTestData,
    NotApproved,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum Permission {
    Permitted,
    Forbidden,
    Unknown,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Part {
    identity: Identity,
    match_status: MatchStatus,
    #[serde(default)]
    lifecycle_assertions: Vec<LifecycleAssertion>,
    provider_checks: Vec<ProviderCheck>,
    #[serde(default)]
    offers: Vec<Offer>,
    #[serde(default)]
    alternate_candidates: Vec<AlternateCandidate>,
    #[serde(default)]
    approved_alternates: Vec<ApprovedAlternate>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Identity {
    manufacturer: String,
    mpn: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum MatchStatus {
    Exact,
    Ambiguous,
    NotFound,
    Error,
    NotChecked,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LifecycleAssertion {
    provider: String,
    raw: String,
    normalized: Lifecycle,
    observed_at_unix: u64,
    provenance: Provenance,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
enum Lifecycle {
    Active,
    New,
    Nrnd,
    LastTimeBuy,
    Eol,
    Obsolete,
    Unknown,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProviderCheck {
    provider: String,
    status: CheckStatus,
    #[serde(default)]
    error_kind: Option<ErrorKind>,
    #[serde(default)]
    retrieved_at_unix: Option<u64>,
    #[serde(default)]
    upstream_at_unix: Option<u64>,
    #[serde(default)]
    provenance: Option<Provenance>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum CheckStatus {
    Checked,
    NotFound,
    Error,
    NotChecked,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum ErrorKind {
    Authentication,
    Quota,
    Provider,
    Transport,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Offer {
    observation_id: String,
    provider: String,
    seller: String,
    seller_original: String,
    authorization: Authorization,
    sku: String,
    packaging: String,
    region: String,
    stock_status: StockStatus,
    stock: Option<u64>,
    moq: Option<u64>,
    order_multiple: Option<u64>,
    factory_lead_time_days: Option<u64>,
    #[serde(default)]
    price_breaks: Vec<PriceBreak>,
    retrieved_at_unix: u64,
    upstream_at_unix: Option<u64>,
    legal_expires_at_unix: u64,
    provenance: Provenance,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum Authorization {
    Authorized,
    Unauthorized,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum StockStatus {
    InStock,
    OutOfStock,
    Unknown,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PriceBreak {
    quantity: u64,
    unit_price: String,
    currency: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Provenance {
    source: String,
    artifact_digest: String,
    location: String,
    synthetic: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AlternateCandidate {
    identity: Identity,
    source: String,
    evidence_id: String,
    provenance: Provenance,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ApprovedAlternate {
    identity: Identity,
    authority_kind: AuthorityKind,
    authority: String,
    approved_at_unix: u64,
    evidence_refs: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum AuthorityKind {
    Engineering,
    User,
}

pub(crate) struct SupplySummary {
    pub fresh: bool,
    pub legal_expires_at_unix: u64,
    pub imported_v1: bool,
    pub exact: usize,
    pub attention: Vec<String>,
}

pub(crate) fn evaluate(
    source: &str,
    lines: &mut [BomLineReview],
    now: u64,
) -> Result<SupplySummary, Error> {
    evaluate_with_policy(source, lines, now, false)
}

#[cfg(test)]
pub(crate) fn evaluate_trusted_fixture(
    source: &str,
    lines: &mut [BomLineReview],
    now: u64,
) -> Result<SupplySummary, Error> {
    evaluate_with_policy(source, lines, now, true)
}

fn evaluate_with_policy(
    source: &str,
    lines: &mut [BomLineReview],
    now: u64,
    allow_durable_provider_records: bool,
) -> Result<SupplySummary, Error> {
    let value: Value = serde_json::from_str(source)
        .map_err(|error| Error::Invalid(format!("Invalid supply snapshot JSON: {error}")))?;
    let version = value
        .get("schemaVersion")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Invalid("Supply snapshot requires schemaVersion.".into()))?;
    let (snapshot, imported_v1) = match version {
        "2.0" => (
            serde_json::from_value(value)
                .map_err(|error| Error::Invalid(format!("Invalid supply snapshot v2: {error}")))?,
            false,
        ),
        "1.0" => (import_v1(value, now)?, true),
        _ => {
            return Err(Error::Invalid(format!(
                "Unsupported supply snapshot schema version: {version}"
            )));
        }
    };
    validate(&snapshot, now, allow_durable_provider_records)?;
    let fresh = true;
    let by_identity: BTreeMap<_, _> = snapshot
        .parts
        .iter()
        .map(|part| (canonical_identity(&part.identity), part))
        .collect();
    let mut attention = Vec::new();
    let mut exact = 0;
    for line in lines {
        evaluate_line(line, &snapshot, &by_identity, now, fresh, &mut attention);
        if line.identity.status == "pass" {
            exact += 1;
        }
    }
    Ok(SupplySummary {
        fresh,
        legal_expires_at_unix: snapshot.legal_expires_at_unix,
        imported_v1,
        exact,
        attention,
    })
}

fn validate(
    snapshot: &Snapshot,
    now: u64,
    allow_durable_provider_records: bool,
) -> Result<(), Error> {
    if snapshot.schema_version != "2.0"
        || snapshot.parts.len() > MAX_PARTS
        || !current_time(snapshot.generated_at_unix, now)
        || snapshot.expires_at_unix < now
        || snapshot.legal_expires_at_unix < now
        || snapshot.expires_at_unix < snapshot.generated_at_unix
        || snapshot.legal_expires_at_unix < snapshot.generated_at_unix
        || snapshot.expires_at_unix > snapshot.generated_at_unix.saturating_add(MAX_AGE_SECONDS)
        || snapshot.legal_expires_at_unix
            > snapshot.generated_at_unix.saturating_add(MAX_AGE_SECONDS)
        || snapshot.demand.build_quantity == 0
        || snapshot.demand.attrition_bps > 100_000
        || !valid_text(&snapshot.demand.region)
        || !valid_currency(&snapshot.demand.currency)
        || !valid_text(&snapshot.demand.packaging)
    {
        return Err(Error::Invalid(
            "Supply snapshot v2 has invalid version, bounds, demand, or timestamps.".into(),
        ));
    }
    let terms: BTreeMap<_, _> = snapshot
        .terms
        .iter()
        .map(|term| (canonical(&term.provider), term))
        .collect();
    if snapshot.terms.len() != NAMED_PROVIDERS.len()
        || terms.len() != NAMED_PROVIDERS.len()
        || snapshot
            .terms
            .iter()
            .any(|term| !NAMED_PROVIDERS.contains(&term.provider.as_str()))
        || !NAMED_PROVIDERS
            .iter()
            .all(|provider| terms.contains_key(&canonical(provider)))
    {
        return Err(Error::Invalid(
            "Supply snapshot requires one terms profile for each named provider.".into(),
        ));
    }
    let mut identities = BTreeSet::new();
    let mut observations = BTreeSet::new();
    let mut count = 0;
    for part in &snapshot.parts {
        if part.lifecycle_assertions.len() > MAX_LIFECYCLE_ASSERTIONS
            || part.provider_checks.len() != NAMED_PROVIDERS.len()
            || part.offers.len() > MAX_OFFERS_PER_PART
            || part.alternate_candidates.len() > MAX_ALTERNATES
            || part.approved_alternates.len() > MAX_APPROVED_ALTERNATES
        {
            return Err(Error::Invalid(
                "Supply snapshot nested collections exceed their bounds.".into(),
            ));
        }
        if !valid_identity(&part.identity) || !identities.insert(canonical_identity(&part.identity))
        {
            return Err(Error::Invalid(
                "Supply snapshot identities must be valid and unique manufacturer+MPN pairs."
                    .into(),
            ));
        }
        let checks: BTreeSet<_> = part
            .provider_checks
            .iter()
            .map(|check| canonical(&check.provider))
            .collect();
        if checks.len() != NAMED_PROVIDERS.len()
            || part
                .provider_checks
                .iter()
                .any(|check| !NAMED_PROVIDERS.contains(&check.provider.as_str()))
            || !NAMED_PROVIDERS
                .iter()
                .all(|provider| checks.contains(&canonical(provider)))
        {
            return Err(Error::Invalid(
                "Each part requires one independent check for Mouser, DigiKey, and LCSC.".into(),
            ));
        }
        for check in &part.provider_checks {
            let checked = check.status != CheckStatus::NotChecked;
            if (check.status == CheckStatus::Error) != check.error_kind.is_some()
                || checked != check.retrieved_at_unix.is_some()
                || checked != check.provenance.is_some()
                || check
                    .retrieved_at_unix
                    .is_some_and(|retrieved| !current_time(retrieved, now))
                || check.upstream_at_unix.is_some_and(|upstream| {
                    !current_time(upstream, now)
                        || check
                            .retrieved_at_unix
                            .is_none_or(|retrieved| upstream > retrieved)
                })
            {
                return Err(Error::Invalid(
                    "Provider check state conflicts with its error, time, or provenance fields."
                        .into(),
                ));
            }
            if let Some(provenance) = &check.provenance {
                validate_durable_record(
                    &check.provider,
                    provenance,
                    &terms,
                    allow_durable_provider_records,
                )?;
            }
        }
        let known_refs: BTreeSet<_> = part
            .offers
            .iter()
            .map(|offer| offer.observation_id.as_str())
            .chain(
                part.alternate_candidates
                    .iter()
                    .map(|candidate| candidate.evidence_id.as_str()),
            )
            .collect();
        for approved in &part.approved_alternates {
            let approved_identity = canonical_identity(&approved.identity);
            let matching_candidate_refs: BTreeSet<_> = part
                .alternate_candidates
                .iter()
                .filter(|candidate| canonical_identity(&candidate.identity) == approved_identity)
                .map(|candidate| candidate.evidence_id.as_str())
                .collect();
            if !valid_identity(&approved.identity)
                || !valid_text(&approved.authority)
                || !matches!(
                    approved.authority_kind,
                    AuthorityKind::Engineering | AuthorityKind::User
                )
                || !current_time(approved.approved_at_unix, now)
                || approved.evidence_refs.is_empty()
                || approved.evidence_refs.len() > MAX_EVIDENCE_REFS
                || approved
                    .evidence_refs
                    .iter()
                    .any(|reference| !known_refs.contains(reference.as_str()))
                || approved
                    .evidence_refs
                    .iter()
                    .all(|reference| !matching_candidate_refs.contains(reference.as_str()))
            {
                return Err(Error::Invalid(
                    "Approved alternate requires authority and resolvable evidence references."
                        .into(),
                ));
            }
        }
        for candidate in &part.alternate_candidates {
            if !valid_identity(&candidate.identity)
                || !valid_text(&candidate.source)
                || !valid_text(&candidate.evidence_id)
                || !observations.insert(candidate.evidence_id.as_str())
            {
                return Err(Error::Invalid("Invalid alternate candidate.".into()));
            }
            validate_durable_record(
                &candidate.source,
                &candidate.provenance,
                &terms,
                allow_durable_provider_records,
            )?;
        }
        for assertion in &part.lifecycle_assertions {
            if !NAMED_PROVIDERS.contains(&assertion.provider.as_str())
                || !valid_text(&assertion.raw)
                || !current_time(assertion.observed_at_unix, now)
            {
                return Err(Error::Invalid("Invalid lifecycle assertion.".into()));
            }
            validate_durable_record(
                &assertion.provider,
                &assertion.provenance,
                &terms,
                allow_durable_provider_records,
            )?;
        }
        for offer in &part.offers {
            count += 1;
            if !observations.insert(offer.observation_id.as_str())
                || !valid_text(&offer.observation_id)
                || !NAMED_PROVIDERS.contains(&offer.provider.as_str())
                || part.provider_checks.iter().any(|check| {
                    check.provider == offer.provider && check.status != CheckStatus::Checked
                })
                || !part
                    .provider_checks
                    .iter()
                    .any(|check| check.provider == offer.provider)
                || !valid_text(&offer.seller)
                || !valid_text(&offer.seller_original)
                || !valid_text(&offer.sku)
                || !valid_text(&offer.packaging)
                || !valid_text(&offer.region)
                || !current_time(offer.retrieved_at_unix, now)
                || offer.legal_expires_at_unix < now
                || offer.legal_expires_at_unix < offer.retrieved_at_unix
                || offer.legal_expires_at_unix > snapshot.legal_expires_at_unix
                || offer.upstream_at_unix.is_some_and(|upstream| {
                    upstream > offer.retrieved_at_unix || !current_time(upstream, now)
                })
                || offer.price_breaks.len() > MAX_PRICE_BREAKS
                || offer.moq == Some(0)
                || offer.order_multiple == Some(0)
                || ((offer.stock_status == StockStatus::Unknown) != offer.stock.is_none())
            {
                return Err(Error::Invalid(
                    "Supply offer has invalid identity, quantities, or timestamps.".into(),
                ));
            }
            validate_durable_record(
                &offer.provider,
                &offer.provenance,
                &terms,
                allow_durable_provider_records,
            )?;
            let mut previous = 0;
            for price in &offer.price_breaks {
                if price.quantity == 0
                    || price.quantity <= previous
                    || !valid_decimal(&price.unit_price)
                    || !valid_currency(&price.currency)
                {
                    return Err(Error::Invalid(
                        "Price breaks must be sorted, unique, positive decimal-string amounts."
                            .into(),
                    ));
                }
                previous = price.quantity;
            }
        }
    }
    if count > MAX_OBSERVATIONS {
        return Err(Error::Invalid(
            "Supply snapshot has too many observations.".into(),
        ));
    }
    Ok(())
}

fn evaluate_line(
    line: &mut BomLineReview,
    snapshot: &Snapshot,
    by_identity: &BTreeMap<(String, String), &Part>,
    now: u64,
    fresh: bool,
    attention: &mut Vec<String>,
) {
    let (Some(manufacturer), Some(mpn), Some(quantity)) =
        (line.manufacturer.clone(), line.mpn.clone(), line.quantity)
    else {
        return;
    };
    let key = (canonical(&manufacturer), canonical(&mpn));
    let Some(part) = by_identity.get(&key) else {
        line.identity = judgment(
            "attention",
            "No exact manufacturer+MPN record exists in the supply snapshot.",
        );
        line.release_impact = judgment("attention", "Exact supply identity was not found.");
        attention.push(format!("{} {}: not found", manufacturer, mpn));
        return;
    };
    if part.match_status != MatchStatus::Exact {
        line.identity = judgment(
            "attention",
            &format!(
                "Supply identity state is {:?}; it cannot be treated as an exact match.",
                part.match_status
            ),
        );
        line.release_impact = judgment("attention", "Supply identity is unresolved.");
        attention.push(format!("{} {}: {:?}", manufacturer, mpn, part.match_status));
        populate_checks(line, part);
        return;
    }
    line.identity = judgment(
        "pass",
        "Canonical manufacturer+MPN exactly matched; raw BOM identity is preserved.",
    );
    let base = (quantity as u64).checked_mul(snapshot.demand.build_quantity);
    let required = base.and_then(|base| {
        let attrition = base
            .checked_mul(snapshot.demand.attrition_bps as u64)?
            .checked_add(9_999)?
            / 10_000;
        base.checked_add(attrition)?
            .checked_add(snapshot.demand.spares)
    });
    let Some(required) = required else {
        line.sourceability = judgment(
            "attention",
            "Required quantity overflowed bounded arithmetic.",
        );
        line.release_impact = judgment("attention", "Demand could not be calculated safely.");
        attention.push(format!("{} {}: demand overflow", manufacturer, mpn));
        return;
    };
    line.required_quantity = Some(required);
    populate_checks(line, part);
    let lifecycle_states: BTreeSet<_> = part
        .lifecycle_assertions
        .iter()
        .map(|assertion| assertion.normalized)
        .filter(|state| *state != Lifecycle::Unknown)
        .collect();
    line.lifecycle_assertions = part
        .lifecycle_assertions
        .iter()
        .map(|assertion| LifecycleReview {
            provider: assertion.provider.clone(),
            raw: assertion.raw.clone(),
            normalized: match assertion.normalized {
                Lifecycle::Active => "active",
                Lifecycle::New => "new",
                Lifecycle::Nrnd => "nrnd",
                Lifecycle::LastTimeBuy => "last-time-buy",
                Lifecycle::Eol => "eol",
                Lifecycle::Obsolete => "obsolete",
                Lifecycle::Unknown => "unknown",
            }
            .into(),
            observed_at_unix: assertion.observed_at_unix,
            provenance: assertion.provenance.location.clone(),
        })
        .collect();
    line.lifecycle_conflict = lifecycle_states.len() > 1;
    line.lifecycle = if line.lifecycle_conflict {
        judgment(
            "attention",
            "Authoritative lifecycle assertions conflict across sources.",
        )
    } else if let Some(state) = lifecycle_states.iter().next() {
        match state {
            Lifecycle::Active | Lifecycle::New => judgment(
                "pass",
                &format!(
                    "Lifecycle is {:?}; raw assertions and source times are retained.",
                    state
                ),
            ),
            _ => judgment(
                "attention",
                &format!(
                    "Lifecycle is {:?}; raw assertions and source times are retained.",
                    state
                ),
            ),
        }
    } else {
        judgment(
            "not-checked",
            "No authoritative lifecycle assertion was supplied.",
        )
    };
    let mut usable = Vec::new();
    for offer in &part.offers {
        let purchasable = purchase_quantity(required, offer);
        let upstream_current = offer
            .upstream_at_unix
            .is_some_and(|upstream| now.saturating_sub(upstream) <= MAX_AGE_SECONDS);
        let commercial_match = fresh
            && now <= offer.legal_expires_at_unix
            && upstream_current
            && purchasable.is_some()
            && canonical(&offer.region) == canonical(&snapshot.demand.region)
            && canonical(&offer.packaging) == canonical(&snapshot.demand.packaging);
        let allowed = commercial_match
            && offer.authorization == Authorization::Authorized
            && offer.stock_status == StockStatus::InStock
            && offer
                .stock
                .zip(purchasable)
                .is_some_and(|(stock, buy)| stock >= buy);
        let price = commercial_match
            .then(|| purchasable.unwrap())
            .and_then(|buy| applicable_price(offer, buy, &snapshot.demand.currency));
        line.offers.push(SupplyOfferReview {
            observation_id: offer.observation_id.clone(),
            provider: offer.provider.clone(),
            seller: offer.seller.clone(),
            seller_original: offer.seller_original.clone(),
            authorization: format!("{:?}", offer.authorization).to_ascii_lowercase(),
            sku: offer.sku.clone(),
            packaging: offer.packaging.clone(),
            region: offer.region.clone(),
            stock_status: match offer.stock_status {
                StockStatus::InStock => "in-stock",
                StockStatus::OutOfStock => "out-of-stock",
                StockStatus::Unknown => "unknown",
            }
            .into(),
            stock: offer.stock,
            moq: offer.moq,
            order_multiple: offer.order_multiple,
            factory_lead_time_days: offer.factory_lead_time_days,
            purchasable_quantity: purchasable,
            applicable_unit_price: price.map(|value| value.to_string()),
            currency: price.map(|_| snapshot.demand.currency.clone()),
            retrieved_at_unix: offer.retrieved_at_unix,
            upstream_at_unix: offer.upstream_at_unix,
            legal_expires_at_unix: offer.legal_expires_at_unix,
            usable: allowed,
            provenance: offer.provenance.location.clone(),
        });
        if allowed {
            usable.push((offer, purchasable.unwrap(), price));
        }
    }
    if let Some((offer, buy, price)) = usable.first() {
        line.stock = offer.stock;
        line.moq = offer.moq;
        line.distributors = usable
            .iter()
            .map(|(offer, _, _)| offer.seller.clone())
            .collect();
        line.sourceability = judgment(
            "pass",
            &format!(
                "Authorized {} offer covers required quantity {required} at purchasable quantity {buy}.",
                offer.provider
            ),
        );
        if let Some(price) = price {
            line.unit_price_decimal = Some((*price).to_string());
            line.currency = Some(snapshot.demand.currency.clone());
            line.pricing = judgment(
                "pass",
                &format!(
                    "Applicable price is {price} {} at purchasable quantity {buy}.",
                    snapshot.demand.currency
                ),
            );
        } else {
            line.pricing = judgment(
                "not-checked",
                "No price break applies in the requested currency at the purchasable quantity.",
            );
        }
    } else {
        line.sourceability = judgment(
            if part.offers.is_empty() {
                "not-checked"
            } else {
                "attention"
            },
            "No independently usable authorized offer covers demand with known region, packaging, MOQ, order multiple, and stock.",
        );
    }
    line.alternate_candidates = part
        .alternate_candidates
        .iter()
        .map(|candidate| AlternateCandidateReview {
            manufacturer: candidate.identity.manufacturer.clone(),
            mpn: candidate.identity.mpn.clone(),
            source: candidate.source.clone(),
            evidence_id: candidate.evidence_id.clone(),
            provenance: candidate.provenance.location.clone(),
        })
        .collect();
    line.approved_alternates = part
        .approved_alternates
        .iter()
        .map(|approved| ApprovedAlternateReview {
            manufacturer: approved.identity.manufacturer.clone(),
            mpn: approved.identity.mpn.clone(),
            authority_kind: match approved.authority_kind {
                AuthorityKind::Engineering => "engineering",
                AuthorityKind::User => "user",
            }
            .into(),
            authority: approved.authority.clone(),
            approved_at_unix: approved.approved_at_unix,
            evidence_refs: approved.evidence_refs.clone(),
        })
        .collect();
    line.alternate_mpns.extend(
        part.alternate_candidates
            .iter()
            .map(|candidate| candidate.identity.mpn.clone()),
    );
    line.alternatives = if part.approved_alternates.is_empty() {
        judgment(
            "not-checked",
            if part.alternate_candidates.is_empty() {
                "No approved alternate evidence was supplied."
            } else {
                "Suggestions are candidates only and do not reduce release risk."
            },
        )
    } else {
        judgment(
            "pass",
            &format!(
                "{} alternate(s) carry explicit authority and evidence references.",
                part.approved_alternates.len()
            ),
        )
    };
    let statuses = [
        &line.lifecycle.status,
        &line.sourceability.status,
        &line.pricing.status,
    ];
    let provider_incomplete = part
        .provider_checks
        .iter()
        .any(|check| matches!(check.status, CheckStatus::Error | CheckStatus::NotChecked));
    line.release_impact = if statuses.contains(&&"attention".to_string()) {
        judgment(
            "attention",
            "Lifecycle or demand-aware supply evidence needs release attention.",
        )
    } else if statuses.contains(&&"not-checked".to_string()) || provider_incomplete {
        judgment(
            "not-checked",
            "At least one required supply or named-provider state was not checked.",
        )
    } else {
        judgment(
            "pass",
            "Exact identity, lifecycle, and demand-aware commercial evidence passed.",
        )
    };
    if line.release_impact.status != "pass" {
        attention.push(format!(
            "{} {}: {}",
            manufacturer, mpn, line.release_impact.status
        ));
    }
}

fn populate_checks(line: &mut BomLineReview, part: &Part) {
    line.provider_checks = part
        .provider_checks
        .iter()
        .map(|check| ProviderCheckReview {
            provider: check.provider.clone(),
            status: match check.status {
                CheckStatus::Checked => "checked",
                CheckStatus::NotFound => "not-found",
                CheckStatus::Error => "error",
                CheckStatus::NotChecked => "not-checked",
            }
            .into(),
            error_kind: check
                .error_kind
                .map(|kind| format!("{:?}", kind).to_ascii_lowercase()),
            retrieved_at_unix: check.retrieved_at_unix,
            upstream_at_unix: check.upstream_at_unix,
            provenance: check
                .provenance
                .as_ref()
                .map(|provenance| provenance.location.clone()),
        })
        .collect();
}

fn purchase_quantity(required: u64, offer: &Offer) -> Option<u64> {
    let minimum = required.max(offer.moq?);
    let multiple = offer.order_multiple?;
    minimum
        .checked_add(multiple - 1)
        .map(|value| value / multiple * multiple)
}

fn applicable_price<'a>(offer: &'a Offer, quantity: u64, currency: &str) -> Option<&'a str> {
    offer
        .price_breaks
        .iter()
        .rfind(|price| price.quantity <= quantity && price.currency.eq_ignore_ascii_case(currency))
        .map(|price| price.unit_price.as_str())
}

fn import_v1(value: Value, _now: u64) -> Result<Snapshot, Error> {
    let generated = value
        .get("generatedAtUnix")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let parts = value
        .get("parts")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Invalid("Supply snapshot v1 requires a parts array.".into()))?;
    let mut imported: BTreeMap<(String, String), Part> = BTreeMap::new();
    for part in parts {
        let manufacturer = part
            .get("manufacturer")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let mpn = part.get("mpn").and_then(Value::as_str).unwrap_or("").trim();
        if manufacturer.is_empty() || mpn.is_empty() {
            continue;
        }
        let identity = Identity {
            manufacturer: manufacturer.into(),
            mpn: mpn.into(),
        };
        let key = canonical_identity(&identity);
        if let Some(existing) = imported.get_mut(&key) {
            existing.match_status = MatchStatus::Ambiguous;
            continue;
        }
        imported.insert(
            key,
            Part {
                identity,
                match_status: MatchStatus::Exact,
                lifecycle_assertions: vec![],
                provider_checks: not_checked_provider_checks(),
                offers: vec![],
                // v1 alternates may be MPN-only and cannot establish exact identity or authority.
                alternate_candidates: vec![],
                approved_alternates: vec![],
            },
        );
    }
    Ok(Snapshot {
        schema_version: "2.0".into(),
        generated_at_unix: generated,
        expires_at_unix: generated.saturating_add(86_400),
        legal_expires_at_unix: generated.saturating_add(86_400),
        demand: Demand {
            build_quantity: 1,
            attrition_bps: 0,
            spares: 0,
            region: "unknown".into(),
            currency: "XXX".into(),
            packaging: "unknown".into(),
        },
        terms: default_terms(),
        parts: imported.into_values().collect(),
    })
}

fn not_checked_provider_checks() -> Vec<ProviderCheck> {
    NAMED_PROVIDERS
        .iter()
        .map(|provider| ProviderCheck {
            provider: (*provider).into(),
            status: CheckStatus::NotChecked,
            error_kind: None,
            retrieved_at_unix: None,
            upstream_at_unix: None,
            provenance: None,
        })
        .collect()
}
fn default_terms() -> Vec<TermsProfile> {
    NAMED_PROVIDERS
        .iter()
        .map(|provider| TermsProfile {
            provider: (*provider).into(),
            decision: TermsDecision::NotApproved,
            query: Permission::Unknown,
            memory_retention: Permission::Unknown,
            disk_retention: Permission::Forbidden,
            html_embedding: Permission::Forbidden,
            sharing: Permission::Forbidden,
        })
        .collect()
}
fn canonical(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase()
}
fn canonical_identity(identity: &Identity) -> (String, String) {
    (canonical(&identity.manufacturer), canonical(&identity.mpn))
}
fn valid_identity(identity: &Identity) -> bool {
    valid_text(&identity.manufacturer) && valid_text(&identity.mpn)
}
fn valid_text(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= MAX_TEXT && !value.chars().any(char::is_control)
}
fn valid_currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase())
}
fn valid_decimal(value: &str) -> bool {
    let (whole, fraction) = value
        .split_once('.')
        .map_or((value, None), |(whole, fraction)| (whole, Some(fraction)));
    !whole.is_empty()
        && whole.bytes().all(|byte| byte.is_ascii_digit())
        && (whole == "0" || !whole.starts_with('0'))
        && fraction.is_none_or(|fraction| {
            !fraction.is_empty()
                && fraction.len() <= 12
                && fraction.bytes().all(|byte| byte.is_ascii_digit())
        })
}
fn current_time(value: u64, now: u64) -> bool {
    value <= now.saturating_add(MAX_FUTURE_SKEW_SECONDS)
        && now.saturating_sub(value) <= MAX_AGE_SECONDS
}

fn validate_durable_record(
    provider: &str,
    provenance: &Provenance,
    terms: &BTreeMap<String, &TermsProfile>,
    allow_durable_provider_records: bool,
) -> Result<(), Error> {
    validate_provenance(provenance)?;
    if !allow_durable_provider_records {
        return Err(Error::Invalid(
            "Durable provider records are disabled pending provider-specific written approval."
                .into(),
        ));
    }
    let term = terms
        .get(&canonical(provider))
        .ok_or_else(|| Error::Invalid(format!("Provider {provider} has no terms profile.")))?;
    if provenance.synthetic {
        if term.decision != TermsDecision::SyntheticTestData {
            return Err(Error::Invalid(
                "Synthetic fixture provenance requires a synthetic-test-data terms decision."
                    .into(),
            ));
        }
        return Ok(());
    }
    let retainable = term.decision == TermsDecision::Approved
        && term.query == Permission::Permitted
        && term.memory_retention == Permission::Permitted
        && term.disk_retention == Permission::Permitted
        && term.html_embedding == Permission::Permitted
        && term.sharing == Permission::Permitted;
    if !retainable {
        return Err(Error::Invalid(format!(
            "Provider {provider} records cannot be retained or embedded under the supplied terms profile."
        )));
    }
    Ok(())
}

fn validate_provenance(provenance: &Provenance) -> Result<(), Error> {
    if !valid_text(&provenance.source)
        || !valid_text(&provenance.location)
        || provenance.artifact_digest.len() != 64
        || !provenance
            .artifact_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(Error::Invalid(
            "Supply observation provenance is incomplete.".into(),
        ));
    }
    Ok(())
}
fn judgment(status: &str, detail: &str) -> BomJudgment {
    BomJudgment {
        status: status.into(),
        detail: detail.into(),
    }
}
