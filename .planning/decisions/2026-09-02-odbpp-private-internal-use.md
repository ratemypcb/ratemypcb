# ODB++ private internal-use decision

Date: 2026-09-02

Status: Accepted for a deferred private lane

## Decision

Phase 6 remains complete and no-go for ODB++ and IPC-2581 in the current public release. FMT-04 remains not applicable. Public behavior, release criteria, and dependencies do not change.

Private ODB++ development and internal processing of customer-supplied files may continue in a separate post-release lane. Publication rights remain unresolved. This permission does not establish public adoption or a public support claim.

## Gates for private integration

Private product integration requires all of these gates:

1. Resolve publication rights.
2. Assemble a rights-cleared representative corpus.
3. Prove semantic conformance for the supported subset.
4. Prove hostile-input security.
5. Set and pass performance and resource limits.
6. Assign maintenance ownership.
7. Approve the private deployment boundary.
8. Approve customer-data handling.
9. Approve the product disclaimer.
10. Approve each exact product claim.

## Public boundary

The deferred lane adds no ODB++ implementation, package, corpus, or test input to the public repository. Plans 07-10 and 07-11 and the Phase 8 release criteria stay unchanged. The public repository remains independently buildable.