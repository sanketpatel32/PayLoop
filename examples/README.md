# Examples

Two ways to integrate with the PayLoop invoice contract:

- **`rust-reader/`** — a minimal Soroban contract that calls `get_invoice`
  and `version` on a PayLoop deployment (cross-contract call).
- **`node-reader/`** — a Node.js script that reads an invoice and creates one,
  using `@stellar/stellar-sdk` against Soroban RPC.

Both target testnet and use the contract id in
[`../contracts/deployment.testnet.json`](../contracts/deployment.testnet.json).
Note that the `v1` testnet deployment predates `invoice_count`,
`get_invoice_range` and `get_client_history` — call `get_invoice` and
`get_invoice_history` there, and see `contracts/invoice/README.md` for the
full interface the current source exposes.
