# Security policy

## Supported

Stellar **testnet** deployment of the PayLoop contract and web app (the
addresses in the root README), plus the `main` branch of this repository.

The web app is deployed against testnet only. Any mainnet-style deployment
would require at minimum removing or gating the unauthenticated faucet route
(`web/app/api/faucet/route.ts`) and re-auditing the off-ramp flow (currently
mocked, see `docs/ANCHOR.md`).

## Reporting a vulnerability

Please use GitHub's **private security advisory** for this repository rather
than a public issue, so there is no disclosure window. Include:

- affected component (contract entrypoint / web route / doc claim),
- reproduction steps or a transaction id,
- your assessment of impact.

You will get an acknowledgement (see the timeline below); fixes land as
regular PRs (labelled `security`) until disclosure is coordinated.

## Notes for reviewers of this contract

- The contract is deliberately minimal: single initializer, no admin functions
  after `initialize`, fixed payment token, two-state invoice lifecycle.
  Security-relevant properties (who can create, who must authorize funding,
  double-payment rejection, balance pre-check) are defined in
  [contracts/invoice/README.md](contracts/invoice/README.md#entrypoints) and
  enforced in `contracts/invoice/src/lib.rs`.
- Testnet-only shared-state caveats anyone can verify on-chain: funding
  requires the named invoice client's signature (`invoice.client
  .require_auth()`), and the faucet mints freely (test asset). Neither
  implies mainnet equivalence.

## Response timeline

- **Acknowledgement:** within 3 business days of a private advisory report.
- **Initial triage** (severity assessment + repro confirmation): within 7
  days.
- **Fix and disclosure coordination:** agreed with the reporter per issue;
  critical items are prioritized over feature work. Timelines are targets,
  not guarantees — this is a volunteer-run testnet project.

## Scope

In scope:

- the PayLoop invoice contract (`contracts/invoice/`) — authorization,
  double-payment, storage and arithmetic handling;
- the web app (`web/`) — display of on-chain data, wallet signing flows;
- the API routes (`web/app/api/`) — rate limiting, input handling, secrets.

Out of scope:

- faucet abuse or drain — it mints free test assets on purpose;
- the mocked off-ramp (`docs/ANCHOR.md`) — no real funds move;
- third-party services (Stellar RPC, Freighter, Vercel, stellar.expert);
- testnet funds themselves, which have no value.

## Safe harbour

Good-faith security research is welcome. If you comply with this policy —
private advisory, no data exfiltration, no denial of service, testnet only —
we won't pursue or support legal action against you, and we'll credit you in
the disclosure notes if you want it.
