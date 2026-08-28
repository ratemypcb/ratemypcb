#!/usr/bin/env python3
"""Create an offline supply-v2 request template; performs no provider calls."""

import argparse
import csv
import json
import sys
import time

NAMED_PROVIDERS = ("mouser", "digikey", "lcsc")


def normalized_header(value):
    return value.lower().replace(" ", "").replace("_", "").replace("-", "")


def canonical(value):
    collapsed = " ".join(value.split())
    return collapsed.translate(str.maketrans("abcdefghijklmnopqrstuvwxyz", "ABCDEFGHIJKLMNOPQRSTUVWXYZ"))


def identities_from_bom(path):
    with open(path, newline="", encoding="utf-8-sig") as handle:
        sample = handle.read(4096)
        handle.seek(0)
        dialect = csv.Sniffer().sniff(sample, delimiters=",;\t")
        rows = csv.DictReader(handle, dialect=dialect)
        headers = rows.fieldnames or []
        manufacturer_key = next((name for name in headers if normalized_header(name) in {"manufacturer", "mfr", "brand"}), None)
        mpn_key = next((name for name in headers if normalized_header(name) in {"mpn", "manufacturerpartnumber", "partnumber"}), None)
        if not manufacturer_key or not mpn_key:
            raise ValueError("BOM requires manufacturer and MPN columns")
        raw_identities = [
            (row.get(manufacturer_key, "").strip(), row.get(mpn_key, "").strip())
            for row in rows
        ]
        if any(not manufacturer or not mpn for manufacturer, mpn in raw_identities):
            raise ValueError("Every BOM supply identity requires manufacturer and MPN")
        identities = {}
        for identity in sorted(raw_identities):
            identities.setdefault(tuple(map(canonical, identity)), identity)
        return [identities[key] for key in sorted(identities)]


def not_checked_part(manufacturer, mpn):
    return {
        "identity": {"manufacturer": manufacturer, "mpn": mpn},
        "matchStatus": "not-checked",
        "lifecycleAssertions": [],
        "providerChecks": [
            {
                "provider": provider,
                "status": "not-checked",
                "errorKind": None,
                "retrievedAtUnix": None,
                "upstreamAtUnix": None,
                "provenance": None,
            }
            for provider in NAMED_PROVIDERS
        ],
        "offers": [],
        "alternateCandidates": [],
        "approvedAlternates": [],
    }


def snapshot(identities, args, now):
    return {
        "schemaVersion": "2.0",
        "generatedAtUnix": now,
        "expiresAtUnix": now + 86400,
        "legalExpiresAtUnix": now + 86400,
        "demand": {
            "buildQuantity": args.build_quantity,
            "attritionBps": args.attrition_bps,
            "spares": args.spares,
            "region": args.region,
            "currency": args.currency,
            "packaging": args.packaging,
        },
        "terms": [
            {
                "provider": provider,
                "decision": "not-approved",
                "query": "forbidden",
                "memoryRetention": "unknown",
                "diskRetention": "forbidden",
                "htmlEmbedding": "forbidden",
                "sharing": "forbidden",
            }
            for provider in NAMED_PROVIDERS
        ],
        "parts": [not_checked_part(*identity) for identity in identities],
    }


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("bom", nargs="?")
    result.add_argument("--output", "-o")
    result.add_argument("--build-quantity", type=int, default=1)
    result.add_argument("--attrition-bps", type=int, default=0)
    result.add_argument("--spares", type=int, default=0)
    result.add_argument("--region", default="US")
    result.add_argument("--currency", default="USD")
    result.add_argument("--packaging", default="unknown")
    result.add_argument("--self-test", action="store_true")
    return result


def self_test():
    args = parser().parse_args(["--build-quantity", "10", "--attrition-bps", "250"])
    value = snapshot([("Acme", "ABC-1")], args, 100)
    assert value["schemaVersion"] == "2.0"
    assert value["parts"][0]["matchStatus"] == "not-checked"
    assert [check["provider"] for check in value["parts"][0]["providerChecks"]] == list(NAMED_PROVIDERS)
    assert all(term["query"] == "forbidden" for term in value["terms"])
    assert canonical("  Acme   semiConductor ") == "ACME SEMICONDUCTOR"
    identities = {}
    for raw in sorted([("ACME", "ABC"), ("Acme", "abc"), (" Acme ", "ABC")]):
        identities.setdefault(tuple(map(canonical, raw)), raw)
    assert list(identities.values()) == [(" Acme ", "ABC")]


def main():
    args = parser().parse_args()
    if args.self_test:
        self_test()
        return
    if not args.bom or not args.output:
        parser().error("BOM and --output are required")
    if args.build_quantity < 1 or not 0 <= args.attrition_bps <= 100000 or args.spares < 0:
        raise ValueError("Demand quantities are outside supported bounds")
    value = snapshot(identities_from_bom(args.bom), args, int(time.time()))
    with open(args.output, "w", encoding="utf-8") as handle:
        json.dump(value, handle, indent=2, sort_keys=True)
        handle.write("\n")


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(f"enrich_bom: {error}", file=sys.stderr)
        raise SystemExit(1) from error
