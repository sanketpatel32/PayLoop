# Glossary

Plain-language definitions of the Stellar and PayLoop terms used across the
app and docs.

- **Anchor** — a regulated service that moves money between fiat (like Naira)
  and on-chain assets. PayLoop's withdraw flow is a mock; a live version would
  use an anchor via SEP-24.
- **Freighter** — the Stellar wallet browser extension PayLoop uses. It holds
  your keys, signs transactions, and is your login: there's no separate
  PayLoop account.
- **Fund an invoice** — the client paying an invoice. In PayLoop the payment
  transfers straight to the freelancer ("instant release") and the invoice is
  marked `Paid` on-chain.
- **Invoice** — an on-chain record naming a freelancer, a client, an amount,
  a description and a due date. Once created it can't be edited or deleted.
- **RPC / Soroban RPC** — the API service used to read contract state and
  submit transactions to the network (the web app reads invoices through it).
- **SAC (Stellar Asset Contract)** — a bridge that lets a classic Stellar
  asset (like USDC) be used inside Soroban smart contracts. PayLoop's test
  USDC is a SAC.
- **SEP-24** — the Stellar standard for hosted deposit/withdrawal flows
  through anchors — the planned production path for cashing out.
- **Simulation** — a read-only trial run of a contract call through the RPC,
  used by the app to read invoices and balances without spending fees.
- **Soroban** — Stellar's smart-contract platform. PayLoop's invoice contract
  is a Soroban contract written in Rust, compiled to WebAssembly.
- **Stroop** — the smallest USDC/XLM unit: 1 USDC = 10,000,000 stroops
  (7 decimals). Amounts in `lib/contract.ts` are BigInt stroops.
- **Testnet** — the public Stellar sandbox network PayLoop runs on. Test USDC
  and XLM are free and worthless; transactions are publicly visible.
- **Trustline** — an opt-in on your Stellar account allowing it to hold a
  specific asset. "Enable USDC" in the app creates the USDC trustline; every
  wallet needs it once before it can receive or pay USDC.
- **XLM / Lumens** — Stellar's native asset, needed in small amounts to pay
  transaction fees (a fraction of a cent) and account reserves.
