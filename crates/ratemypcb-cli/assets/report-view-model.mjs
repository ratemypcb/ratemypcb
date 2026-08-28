export function normalizedStatus(value) {
  return String(value).toLowerCase().replaceAll("_", "-");
}

const impactRank = new Map([
  ["attention", 0],
  ["not-checked", 1],
  ["pass", 2],
]);

const shown = (value) => value == null ? "unknown" : String(value);

export function supplyDetailGroups(line) {
  const providers = (line.providerChecks || []).map((check) =>
    `${check.provider}: ${check.status}; error ${shown(check.errorKind)}; retrieved ${shown(check.retrievedAtUnix)}; upstream ${shown(check.upstreamAtUnix)}; provenance ${shown(check.provenance)}`,
  );
  const offers = (line.offers || []).map((offer) =>
    `${offer.provider}; seller ${offer.seller}; original seller ${offer.sellerOriginal}; SKU ${offer.sku}; ${offer.authorization}; ${offer.stockStatus}; package ${offer.packaging}; region ${offer.region}; stock ${shown(offer.stock)}; MOQ ${shown(offer.moq)}; multiple ${shown(offer.orderMultiple)}; buy ${shown(offer.purchasableQuantity)}; lead time days ${shown(offer.factoryLeadTimeDays)}; applicable price ${shown(offer.applicableUnitPrice)} ${shown(offer.currency)}; retrieved ${offer.retrievedAtUnix}; upstream ${shown(offer.upstreamAtUnix)}; legal expiry ${offer.legalExpiresAtUnix}; usable ${offer.usable}; provenance ${offer.provenance}`,
  );
  const lifecycle = (line.lifecycleAssertions || []).map((assertion) =>
    `${assertion.provider}: ${assertion.raw}; normalized ${assertion.normalized}; observed ${assertion.observedAtUnix}; provenance ${assertion.provenance}`,
  );
  if (line.lifecycleConflict) lifecycle.unshift("Lifecycle conflict: yes");
  const candidates = (line.alternateCandidates || []).map((candidate) =>
    `${candidate.manufacturer} ${candidate.mpn}; source ${candidate.source}; evidence ${candidate.evidenceId}; provenance ${candidate.provenance}`,
  );
  const approved = (line.approvedAlternates || []).map((alternate) =>
    `${alternate.manufacturer} ${alternate.mpn}; authority ${alternate.authorityKind}: ${alternate.authority}; approved ${alternate.approvedAtUnix}; evidence ${alternate.evidenceRefs.join(", ")}`,
  );
  return [
    ["Named provider checks", providers],
    ["Seller-scoped offers", offers],
    ["Lifecycle assertions", lifecycle],
    ["Alternate candidates", candidates],
    ["Approved alternates", approved],
  ].filter(([, lines]) => lines.length);
}

export function schematicEvidenceRefs(report, checkId = "schematic-evidence") {
  const channel = report.evidence.find((record) => record.checkId === checkId)?.id;
  const fallback = report.evidence.find(
    (record) => record.checkId === "schematic-evidence",
  )?.id;
  return channel ? [channel] : fallback ? [fallback] : [];
}

export function filterAndSortBomLines(lines, query, filter, sort) {
  const normalizedQuery = query.trim().toLowerCase();
  const matches = lines
    .map((line, sourceIndex) => ({ line, sourceIndex }))
    .filter(({ line }) =>
      (filter === "all" || normalizedStatus(line.releaseImpact.status) === filter) &&
      (!normalizedQuery ||
        JSON.stringify([
          line.references,
          line.value,
          line.manufacturer,
          line.mpn,
        ])
          .toLowerCase()
          .includes(normalizedQuery)),
    );
  if (sort === "release-impact") {
    matches.sort(
      (a, b) =>
        (impactRank.get(normalizedStatus(a.line.releaseImpact.status)) ?? 1) -
          (impactRank.get(normalizedStatus(b.line.releaseImpact.status)) ?? 1) ||
        a.sourceIndex - b.sourceIndex,
    );
  }
  return matches;
}
