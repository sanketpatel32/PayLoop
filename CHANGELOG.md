# Changelog

All notable changes to PayLoop are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the contract follows
the semantic version policy in
[contracts/invoice/README.md](contracts/invoice/README.md#versioning).

## [Unreleased]

### Fixed
- `invoice_count` now returns the number of invoices ever created
  (`NextId - 1`), not the next id.
- `get_invoice_range` caps `len` at `MAX_PAGE_LEN` (25) and uses saturating
  `start + len` arithmetic so extreme `u32` inputs can't trap on wasm32.
- Web: `/pay/<id>` validates the id before the RPC call, recognizes the
  on-chain `InvoiceNotFound` error code, and shows "—" (never a false 0)
  when a balance read fails.

### Changed
- Contract `version()` now reports `v1_1_0` under the documented semver
  policy (was `v1`). The testnet deployment still reports `v1`.

### Added
- `CHANGELOG.md`, `docs/GLOSSARY.md`, and `examples/` (Rust cross-contract
  call + Node.js read/create script).

## [v1] - 2026-07-19

Initial testnet release: create → fund → instant-release invoice flow,
multi-token allowlist, on-chain income history, Freighter-integrated web app.

- Contract `CAQVSBNVL7OI66IDTYCR7XL4VJKMSOYGBW5D6SWLTWTINTCQO2OGCSXS`
  deployed to Stellar testnet (`contracts/deployment.testnet.json`).
- Web app: create, pay, dashboard, history, activity, withdraw pages.
