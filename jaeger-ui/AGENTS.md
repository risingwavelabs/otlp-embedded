# Jaeger UI Upgrade & Verification Notes

This file records practical steps for:

1. bumping vendored Jaeger UI assets under `jaeger-ui/build`
2. validating core behavior with Playwright

## Bump Jaeger UI

1. Check latest release:
   - `https://github.com/jaegertracing/jaeger-ui/releases/latest`
2. Inspect release assets and prefer `assets.tar.gz`:
   - `https://github.com/jaegertracing/jaeger-ui/releases/tag/<tag>`
3. Replace vendored build:
   - remove `jaeger-ui/build`
   - extract `packages/jaeger-ui/build` from `assets.tar.gz`
   - copy it to `jaeger-ui/build`
4. Update `jaeger-ui/README` with exact release version and link.

## Version Selection Rule

- Jaeger UI `2.15+` started partial migration to `/api/v3/*` (service/operation discovery).
- This project currently implements a minimal legacy `/api/*` compatibility layer.
- If `/api/v3/*` handlers are not implemented, prefer `v2.14.1` for stable search/trace flows.

## Playwright Smoke Test (mock_ui)

Use `examples/mock_ui.rs` to validate UI quickly with seeded traces.

### Start App

1. `cargo run --example mock_ui`
2. expect startup logs showing:
   - seeded trace count
   - service list

### Core Checks

1. Open `http://127.0.0.1:10188/search`
2. Search tab:
   - service dropdown should show 4 services
   - select `checkout`, operation count should become 2
   - select `POST /checkout`, click `Find Traces`
   - result list should show at least 1 trace
3. Trace detail:
   - open first trace from results
   - trace page should render timeline/graph without frontend errors

### Current Known Gaps (expected with minimal backend)

1. `/dependencies`:
   - calls `/api/dependencies`
   - currently returns `404 API not supported`
2. `/monitor`:
   - page opens
   - metrics cards may show `Could not fetch data`
