# Phase 2 UI Specification

## First screen

Order: release disposition and ≤60-character verdict; scope/artifact/evidence time; why; ≤3 actions; blocker questions; required-evidence summary; secondary scores. Decorative masthead is compact and scores never visually dominate.

## Evidence

Every claim renders its public `ev-…` ID as a link. Evidence targets are programmatically focusable, expose artifact digest, producer/version, structured location, class, confidence, freshness/time, and a Copy ID control. Following a claim updates a Return to claim link. Initial evidence hashes activate the Evidence tab and focus the target.

## Required evidence

Summary badges: completed, attention, not run, not provided, overall freshness. Each non-complete check is explicitly listed. Detail includes execution/result/freshness/confidence and linked provenance producer/version; attention is never labeled passed.

## BOM

Adjacent controls provide search, status filter, and sort by report-provided release impact or source order. Default is release impact. Each row retains compact identity and sourceability/lifecycle/stock/price/alternates/release-impact states. Render 100 rows initially and in further batches; announce shown/filtered/total counts. `not checked` remains a named option and is never mapped to zero/pass.

## Accessibility/responsive/print

Tabs implement tablist/tab/tabpanel relationships, roving tabindex, arrows, Home/End. Buttons and form controls have visible labels/focus. The canvas has textual layer/source/warning fallback. Tables have captions and headers. Status is text, not color-only. At 320px content reflows without page clipping. Print exposes decision, actions, completeness, limitations, evidence IDs/provenance, all report panels, and table headers while hiding interactive controls/canvas.
