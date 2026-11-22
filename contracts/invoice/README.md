# PayLoop invoice contract

A self-contained **Soroban invoice + instant-release escrow** contract written
with `soroban-sdk` 27. Any Stellar dApp that needs "request a payment, have it
settled atomically on-chain, keep an on-chain record of paid invoices" can reuse
it as-is: it is not tied to PayLoop's frontend. A default payment token is set
per deployment, and additional allowlisted tokens can be chosen per invoice
(it works with any token contract, including SACs for issued assets such as
USDC).

Deployed testnet instance (v1 WASM): see the addresses table in the root
[README](../../README.md) and
[`contracts/deployment.testnet.json`](../deployment.testnet.json).

## Lifecycle

```
                    create_invoice (freelancer signs)
                    ┌──────────────────────────────┐
                    │  status = Pending            │
                    └──────────────┬───────────────┘
                                   │  fund_invoice (client signs)
                                   │  1. require_auth(client)
                                   │  2. balance(client) >= amount
                                   │  3. token.transfer(client → freelancer)
                                   ▼
                    ┌──────────────────────────────┐
                    │  status = Paid, paid_at set  │   (terminal state)
                    └──────────────────────────────┘
```

There are exactly two states (`Pending`, `Paid`) and one transition. Funding is
**instant release**: the contract never custodies funds — the transfer and the
status flip happen atomically in the client's one transaction, so there is no
window where the contract can lose, withhold, or revert escrowed money.

`release_funds` is retained in the ABI as the seam for a future held-escrow
(milestone) model. In the deployed v1 design it performs **no state change and
no funds movement** — do not build against it.

## Storage model

| Key | Storage kind | Set by | Purpose |
| --- | --- | --- | --- |
| `Admin` | instance | `initialize` | initializer address; only used to authorize growing the allowlist |
| `DefaultToken` | instance | `initialize` | token contract used when `create_invoice` is called; immutable after init |
| `AllowedTokens` | instance | `initialize`, `add_allowed_token` | tokens an invoice may be created with; entries are never removed |
| `NextId` | instance | `create_invoice` | monotonically increasing invoice counter starting at 1 |
| `Invoice(id)` | persistent | `create_invoice`, `fund_invoice` | the full `Invoice` record |
| `History(freelancer)` | persistent | `create_invoice` | per-freelancer ordered list of invoice ids |
| `ClientHistory(client)` | persistent | `create_invoice` | per-client ordered list of invoice ids |

Known scaling limitation: `get_invoice_history` loads every invoice struct for
one freelancer in a single call, and `get_client_history` does the same for a
client. Past a few hundred invoices per address a deployed instance will hit
Soroban read-footprint limits — integrators should page with
`get_invoice_range(freelancer, start, len)` (max `len` = `MAX_PAGE_LEN` = 25)
instead. A client-side paginated accessor doesn't exist yet; fetch each id
from `ClientHistory` via `get_invoice` if you need client-side paging.

`invoice_count`, `get_invoice_range`, `get_client_history`,
`create_invoice_with_token`, `add_allowed_token`, `get_allowed_tokens` and
the `ClientHistory` key are newer than the testnet deployment — the deployed
instance is `v1` and does not have them (see [versioning](#versioning)).

## Entrypoints

| Function | Caller must authorize | Arguments | Returns | Emits |
| --- | --- | --- | --- | --- |
| `initialize` | `admin` | `admin`, `payment_token` | `()` | — |
| `add_allowed_token` | `admin` | `admin`, `token` | `()` | — |
| `create_invoice` | `freelancer` | `freelancer`, `client`, `amount` (i128), `description`, `due_date` (u64) | `u64` invoice id | `InvoiceCreated` |
| `create_invoice_with_token` | `freelancer` | `freelancer`, `client`, `amount` (i128), `description`, `due_date` (u64), `token` | `u64` invoice id | `InvoiceCreated` |
| `fund_invoice` | `invoice.client` | `invoice_id` | `()` | `InvoicePaid` |
| `release_funds` | — (inert in v1) | `invoice_id` | `()` | — |
| `get_invoice` | read-only | `invoice_id` | `Invoice` | — |
| `get_invoice_history` | read-only | `freelancer` | `Vec<Invoice>` | — |
| `invoice_count` | read-only | — | `u64` total invoices created | — |
| `get_invoice_range` | read-only | `freelancer`, `start` (u32), `len` (u32) | `Vec<Invoice>` page | — |
| `get_client_history` | read-only | `client` | `Vec<Invoice>` | — |
| `default_token_address` | read-only | — | token `Address` | — |
| `get_allowed_tokens` | read-only | — | `Vec<Address>` | — |
| `version` | read-only | — | `"v1_1_0"` symbol (see [versioning](#versioning)) | — |

Semantics worth knowing:

- Only the **freelancer being credited** can create an invoice, so nobody can
  forge an invoice in someone else's payment history.
- `create_invoice` keeps the original five-argument ABI and uses the default
  token. `create_invoice_with_token` stores an explicit allowlisted token on the
  invoice; `fund_invoice` always transfers that stored token, never a global
  default.
- The allowlist only grows: removing a token would make existing unpaid
  invoices unfundable. The admin can add tokens but cannot rotate, pause, or
  recover funds.
- Funding always moves the **invoice amount that was recorded at creation**;
  the payer cannot pay a different amount and the freelancer cannot change the
  amount after creation.
- `due_date` is informational (rendered in UIs, used for overdue badges); the
  contract does not enforce it. There is no expiry: an overdue invoice can
  still be paid so late payments are not locked out.
- Double funding is impossible: a second `fund_invoice` on a `Paid` invoice
  returns `AlreadyPaid`.
- Only the named client can fund: `fund_invoice` calls
  `invoice.client.require_auth()` and transfers from `invoice.client`, so a
  third party cannot pay someone else's invoice. Requiring the named client's
  auth is also what makes the `Paid` event trustworthy as a receipt.
- `invoice_count` is the number of invoices ever created (`NextId - 1`), so
  `1..=invoice_count()` enumerates every id exactly once.
- `get_invoice_range` pages a freelancer's history: `len` is capped at
  `MAX_PAGE_LEN` (25) to bound the read footprint, `start` beyond the end
  returns an empty page, and `start + len` is saturating so extreme `u32`
  inputs can never trap.

## Versioning

`version()` returns a symbol `vMAJOR_MINOR_PATCH` encoding semantic version
(`Symbol` cannot contain dots, so underscores separate the numbers):

- **MAJOR** — a breaking ABI change: a removed or re-signed entrypoint, a
  changed error code, a changed `Invoice`/`DataKey` layout. Requires a new
  contract id and a migration plan.
- **MINOR** — an ABI addition that keeps everything wire-compatible: new
  entrypoints, or error codes *appended* after the existing ones. A redeploy
  is enough; existing clients keep working.
- **PATCH** — no ABI change at all: internal fixes, gas improvements, docs.

The v1 testnet deployment predates `get_invoice_range`, `get_client_history`,
`invoice_count`, `create_invoice_with_token`, `add_allowed_token`,
`get_allowed_tokens` and the `ClientHistory` key — ABI additions that keep
codes 1–5 stable — so the source is `v1_1_0` and bumps to `v1_1_0` on the
next redeploy. `deployment.testnet.json` records what is actually live via
its `contractVersion` field. Each bump gets a CHANGELOG entry.

## Integration examples

See [`../../examples/`](../../examples/README.md): a Rust contract that calls
`get_invoice`/`version` via cross-contract invocation, and a Node.js script
that reads and creates invoices over Soroban RPC.

## Error codes

| Code | Name | Raised when |
| --- | --- | --- |
| 1 | `NotInitialized` | read/init state missing — contract not initialized |
| 2 | `AlreadyInitialized` | `initialize` called twice |
| 3 | `InvoiceNotFound` | unknown invoice id |
| 4 | `AlreadyPaid` | funding an already-`Paid` invoice |
| 5 | `InvalidAmount` | `amount <= 0` |
| 6 | `InvalidDescription` | empty `description` |
| 7 | `InvalidParties` | `freelancer == client` |
| 8 | `InsufficientBalance` | client's token balance below `invoice.amount` at fund time (advisory pre-check; the token transfer remains the authoritative check) |
| 9 | `TokenNotAllowed` | `create_invoice_with_token` called with a token outside the allowlist |

Codes 6–9 were appended after the original testnet deployment, so v1 wire
encodings of codes 1–5 are unchanged.

## Events

| Event | Topics | Data |
| --- | --- | --- |
| `InvoiceCreated` | `freelancer`, `client` | `id`, `amount` |
| `InvoicePaid` | `freelancer`, `client` | `id`, `amount`, `paid_at` |

Marking users as topics means a platform can craft event claims
(`?topic0=...&topic1=<freelancer>`) against Soroban RPC to listen only for one
participant's activity, instead of scanning invoices.

## Build, test, verify

```bash
cd contracts/invoice
cargo test                              # 18 unit tests (auth + edge cases)
cargo fmt --all --check
cargo clippy --all-targets
cargo build --target wasm32v1-none --release
# -> ../target/wasm32v1-none/release/payloop_invoice.wasm
```

Full deploy walkthrough (keys, SAC, initialize, faucet): [../DEPLOYMENT.md](../DEPLOYMENT.md).
The contract CI (`.github/workflows/contract.yml`) runs all four steps above and
uploads the release WASM as an artifact on every push touching `contracts/`.

> Note on the local dev dependency pin: `Cargo.toml` pins
> `ed25519-dalek = "=2.2.0"` because `soroban-env-host`'s testutils PRNG breaks
> with its 3.x line. Do not widen that pin without re-running the auth tests.

## Integration examples

Everything below uses the **live testnet deployment**; the same code works
against any instance you deploy yourself.

### 1. TypeScript (browser or Node), `@stellar/stellar-sdk`

```ts
import {
  Address, BASE_FEE, Contract, nativeToScVal, scValToNative,
  rpc, TransactionBuilder,
} from "@stellar/stellar-sdk";

const CONTRACT_ID = "CAQVSBNVL7OI66IDTYCR7XL4VJKMSOYGBW5D6SWLTWTINTCQO2OGCSXS";
const RPC_URL = "https://soroban-testnet.stellar.org";
const PASSPHRASE = "Test SDF Network ; September 2015";

const server = new rpc.Server(RPC_URL);

export async function simulateRead(method: string, args: any[]) {
  // Simulate as a throwaway source: pure reads need no real signer/fee.
  const source = new (await import("@stellar/stellar-sdk")).Account(
    "GBTDIHYJA4OPTDBDRKH5A5PHBSH7QVAEY46V27DCB24SXNRXIQNHSNUM", "0");
  const tx = new TransactionBuilder(source, { fee: BASE_FEE, networkPassphrase: PASSPHRASE })
    .addOperation(new Contract(CONTRACT_ID).call(method, ...args))
    .setTimeout(30)
    .build();
  const sim = await server.simulateTransaction(tx);
  if (rpc.Api.isSimulationError(sim)) throw new Error(sim.error);
  return scValToNative(sim.result!.retval);
}

// Read one invoice (status: 0 = Pending, 1 = Paid).
export const readInvoice = (id: bigint | number) =>
  simulateRead("get_invoice", [nativeToScVal(BigInt(id), { type: "u64" })]);

export const readHistory = (freelancer: string) =>
  simulateRead("get_invoice_history", [new Address(freelancer).toScVal()]);
```

Writes (create/fund) follow the standard four-step flow — build →
`prepareTransaction` → Freighter wallet signature → `sendTransaction` + poll
`getTransaction` — which the PayLoop app implements in
[`web/lib/contract.ts`](../../web/lib/contract.ts) and is a working reference
for a Freighter-integrated integration.

### 2. CLI smoke test

```bash
stellar contract invoke --id CONTRACT_ID --source deployer --network testnet \
  -- create_invoice \
  --freelancer <FREELANCER_ADDR> --client <CLIENT_ADDR> \
  --amount 2000000000 --description "Landing page copy" --due_date 0

stellar contract invoke --id CONTRACT_ID --source deployer --network testnet \
  -- get_invoice --invoice_id 1
```

## Multi-token setup and v1 migration notes

A fresh deployment sets the allowlist to the default token during
`initialize`. To invoice in another stablecoin, the admin first calls
`add_allowed_token(admin, token)` once per token, then freelancers use
`create_invoice_with_token`. The frontend reads `get_allowed_tokens` (falling
back to `NEXT_PUBLIC_TOKENS` on v1 deployments) for its token selector, reads
`symbol()`/`decimals()` from each token, and needs each SAC's classic asset
issuer in `NEXT_PUBLIC_TOKENS` to build trustlines.

Existing v1 invoices need no migration: their `Invoice.token` was already
stored at creation and `fund_invoice` has always used that field. They remain
fundable on the new deployment as long as their token is in the allowlist (the
default token always is). Because there is no in-place upgrade, point clients
at the new contract address and keep reading old invoices from the old
contract, or replay their records off-chain; do not assume old ids exist in the
new instance. Encoding of `Invoice` and of error codes 1–8 is unchanged.

## Reuse notes

- **Token choice**: `initialize(admin, payment_token)` accepts any Soroban
  token contract address (SAC or custom) as the default. Additional tokens are
  opt-in via `add_allowed_token`; every invoice stores the token it was created
  with and funding uses that token.
- **Decimals**: the contract is decimal-agnostic; amounts are raw `i128` base
  units. Your UI handles human conversion using each token's `decimals()`
  entrypoint (see `web/lib/contract.ts` and `web/lib/format.ts`).
- **What you'd extend for a real escrow**: replace instant release with
  `deposit` into the contract + `release_funds(invoice_id)` gated on a trigger,
  and add a `Refunded`/`Disputed` status on the enum. Renaming the enum's
  existing variants is a breaking change for stored data; additive variants
  are not.
- **Deploying your copy after edits** — there is no in-place upgrade path (no
  `set_admin`/upgrade function exists by design), so applying source changes
  means a fresh deploy with config re-pointed at the new addresses. Error
  codes 1–5 are unchanged and 6–8 are appended, so clients compiled against
  the v1 ABI keep working; the new create/fund validations are behavior
  changes rather than ABI breaks and belong in release notes when a v2
  instance ships.
