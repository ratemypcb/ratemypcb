# Phase 2 Accessibility Checklist

## Automated and browser checks completed

- [x] Unique static HTML IDs and resolved ARIA target references.
- [x] Tablist/tab/tabpanel wiring and Home/End/arrow source behavior.
- [x] Table caption/headers, live BOM/evidence counts, textual status, canvas description.
- [x] Viewer mode buttons expose `aria-pressed`; zoom controls have explicit accessible names.
- [x] Visible focus, forced-colors, reduced-motion, responsive, and print rules are present.
- [x] Attention text uses a contrast-safe dark amber while retaining text labels.
- [x] Chrome runtime switched tabs and moved claim focus to evidence without a dead target.
- [x] Chrome 320px mechanical check found no horizontal page overflow.
- [x] Chrome PDF text retained limitations, provenance IDs, evidence details, and both panels.

These checks are not WCAG conformance.

## Human/browser-needed

- [ ] Hands-on Tab/Shift+Tab, Enter/Space, disclosure, copy permission, and no-trap review.
- [ ] Chromium+NVDA or Safari+VoiceOver qualitative spot check.
- [ ] Real-browser accessibility audit with an approved tool: zero serious/critical.
- [ ] Cross-browser responsive and print-preview inspection beyond the Chrome subset.
