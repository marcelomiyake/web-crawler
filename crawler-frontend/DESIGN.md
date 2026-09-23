# Frontend design notes

- **Service ID:** `web-crawler-frontend`
**Status:** Implemented design notes; the OpenDesign tool was not present in this environment, so no OpenDesign prototype or acceptance handoff is claimed.

> Project documentation index: [Documentation index](../docs/README.md)

## Visual language

- A quiet field-notes/archive motif: warm off-white canvas, forest-green actions, low-contrast borders, readable serif display type, and monospaced URL/status details.
- The layout prioritizes the crawl form and active job, with recent archives in a secondary rail that moves below the workspace on narrow screens.
- System font fallbacks keep the UI usable without remote font requests or a runtime design dependency.

## Interaction states

- Initial empty archive; validated submission; submitting; job running; complete and complete-with-errors; saved pages; escaped source snapshot; deletion confirmation; API/network error; page refresh.
- Recent job IDs are stored locally. No page bodies, credentials, or complete URLs are copied into browser storage.
- Job polling is every three seconds while this page is open. Server status remains authoritative.

## Accessibility and responsive behavior

- Form controls have explicit labels; validation and network errors use an alert region, completion notices use a status region, and progress uses the progressbar role.
- Controls are keyboard-operable with visible focus. Reduced-motion preferences are honored. Page source is rendered through Vue text interpolation, never HTML insertion.
- At 800px the archive rail moves below the main workspace. At 540px the header, form actions, counters, and page rows reflow for small screens.

## Verification hooks

- `src/App.spec.ts` checks visible error feedback and that archived markup remains text.
- `e2e/archive.spec.ts` exercises a browser flow using an API stub; it does not validate the live crawler or substitute for the controlled Kind smoke test.
