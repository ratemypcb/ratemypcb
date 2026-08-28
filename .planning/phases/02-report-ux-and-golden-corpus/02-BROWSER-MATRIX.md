# Phase 2 Browser Matrix

**Status:** Chrome headless subset passed on macOS; human/assistive-technology checks remain deferred.

| Check | Target | Result |
| --- | --- | --- |
| Responsive | 320×568 and 1440×900 Chrome | Passed local screenshot and DOM checks. At 320px, page `scrollWidth` equaled `innerWidth` (320px); disposition and first action were visible. Other target sizes remain unrun. |
| Print/PDF | Chrome PDF | Passed bounded inspection: decision/actions were unclipped on page 1; extracted PDF text included evidence details, both report panels, limitations, limitation evidence IDs, and disclaimer. Safari/A4/Letter matrix remains unrun. |
| Runtime navigation | Chrome headless CDP | Passed Evidence/BOM tab switching and claim-to-evidence focus. Copy permission and full hands-on keyboard traversal remain unrun. |
| Screen reader | NVDA+Chromium or VoiceOver+Safari | Not run. |
| Browser accessibility audit | zero serious/critical | Not run; no approved audit dependency is installed. |
| 10,000 BOM + finding interaction | decision ≤1s, warm control response ≤200ms, bounded initial DOM | Passed local generated overload: useful decision at 299ms; initial DOM 5,734 nodes with 100/10,000 BOM rows and 100/10,018 evidence records; search/deep-link/category interaction 54ms; last evidence target materialized without rendering predecessors. |
| Malformed fragment | `#%` | Passed Chrome DOM dump; disposition rendered instead of remaining at Loading. |

## Commands and artifacts

- Chrome: `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome` headless mode.
- Self-contained fixture: generated through real CLI `review → digest → render` in `/tmp/ratemypcb-p2.html`.
- Screenshots: `/tmp/ratemypcb-p2-320.png`, `/tmp/ratemypcb-p2-1440.png` (not retained in repository).
- PDF/text: `/tmp/ratemypcb-p2.pdf`, `/tmp/ratemypcb-p2.txt` (not retained).
- Overload data was generated in `/tmp`, not checked in.

These local measurements are not representative-user comprehension, screen-reader evidence, a WCAG conformance claim, or a cross-browser matrix.
