# Lighthouse and SEO META Verification

This record audits the built crawler web frontend and checks its browser metadata against the published SEO META in 1 CLICK field list. Lighthouse measures this browser run; it is separate from SonarQube analysis and JEV readiness evaluation.

## Contents

- [Scope](#scope)
- [Lighthouse results](#lighthouse-results)
- [SEO META checklist](#seo-meta-checklist)
- [Findings](#findings)
- [Related documentation](#related-documentation)

## Scope

- **Date/time:** 2026-09-25 12:32:12 America/Sao_Paulo.
- **Source:** dirty worktree based on revision 948fc8c.
- **Environment:** Linux; Google Chrome 154.0.8037.57; Lighthouse CLI 13.5.0; default mobile emulation and simulated throttling.
- **Target:** production Vite preview at http://127.0.0.1:4173/; one app preview ran at a time and was stopped before the next.
- **Commands:** `npm run build`; `npx --yes lighthouse http://127.0.0.1:4173/ --output=json --output-path=/tmp/lighthouse-web-crawler.json --only-categories=performance,accessibility,best-practices,seo --chrome-flags='--headless --no-sandbox' --quiet`.
- The crawler API was not deployed; the audit covers the built browser page, not job creation or crawl behavior.

## Lighthouse results

| Performance | Accessibility | Best practices | SEO |
| ---: | ---: | ---: | ---: |
| 99 | 100 | 100 | 63 |

Scores range from 0 to 100. The SEO score reflects the intentional noindex directive.

## SEO META checklist

| Field | Result |
| --- | --- |
| HTML language | English is declared. |
| Title and length | Present: “Fieldnote · Web Crawler” (23 characters). |
| Description and length | Present: “A private, bounded web crawl archive for local use.” (51 characters). |
| Robots metadata | noindex, nofollow is intentional for this local/private app. A valid robots.txt is served with Allow: / so crawlers can read the page directive. |
| Canonical URL | Omitted because the local preview/deployment origin is not a stable public URL. |
| Headings | One H1 followed by semantic H2/H3 sections. |
| Images and alt text | No HTML image elements; this check is not applicable. |
| Links | Three labeled anchors in the default view: one internal and two external, three unique targets. |
| Favicon | SVG favicon is declared and served. |
| Open Graph, Twitter, and sitemap | Not provided because the app has no public share URL or public indexing target. |

The SEO META in 1 CLICK extension was not installed in the browser. This is a manual checklist audit against its published fields, not a claim that the extension itself ran.

## Findings

- Lighthouse confirmed the title, description, valid robots.txt, favicon, and page metadata. It reports the page as not crawlable by design because of the noindex directive.
- Performance scored 99; Accessibility and Best practices scored 100. The built page had no API-dependent console errors in this audit.
- The read-only crawl-status idea remains a future candidate only; WebMCP job creation and archive deletion are not exposed.

## Related documentation

- [Verification index](README.md)
- [Crawler frontend guide](../../crawler-frontend/README.md)
- [System Design verification section](../system-design.md#9-verification-and-acceptance)
- [Chrome Lighthouse overview](https://developer.chrome.com/docs/lighthouse/overview)
- [SEO META in 1 CLICK listing](https://chromewebstore.google.com/detail/seo-meta-in-1-click/bjogjfinolnhfhkbipphpdlldadpnmhc?hl=en-GB)
