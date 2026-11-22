// Node.js example: read an invoice and create one on the PayLoop testnet
// contract via Soroban RPC.
//
// Usage:
//   npm install
//   node index.mjs read <invoice_id>
//   node index.mjs create <freelancer_secret> <client_address> <amount_usdc> "description" <due_epoch>
//
// Uses the deployment pinned in ../../contracts/deployment.testnet.json.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import {
  Contract,
  Keypair,
  Networks,
  rpc as StellarRpc,
  TransactionBuilder,
  BASE_FEE,
  Address,
  nativeToScVal,
  scValToNative,
} from "@stellar/stellar-sdk";

const here = dirname(fileURLToPath(import.meta.url));
const deployment = JSON.parse(
  readFileSync(join(here, "../../contracts/deployment.testnet.json"), "utf8")
);
const server = new StellarRpc.Server(deployment.rpcUrl);
const contract = new Contract(deployment.contractId);

async function simulate(method, ...args) {
  const account = await server.getAccount(
    "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF" // stub account for read-only sims
  );
  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: deployment.networkPassphrase,
  })
    .addOperation(contract.call(method, ...args))
    .setTimeout(30)
    .build();
  const sim = await server.simulateTransaction(tx);
  if (StellarRpc.Api.isSimulationError(sim)) throw new Error(sim.error);
  return scValToNative(sim.result.retval);
}

async function invoke(signer, method, ...args) {
  const account = await server.getAccount(signer.publicKey());
  const tx = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: deployment.networkPassphrase,
  })
    .addOperation(contract.call(method, ...args))
    .setTimeout(30)
    .build();
  const prepared = await server.prepareTransaction(tx);
  prepared.sign(signer);
  const sent = await server.sendTransaction(prepared);
  let res = await server.getTransaction(sent.hash);
  while (res.status === "NOT_FOUND") {
    await new Promise((r) => setTimeout(r, 1500));
    res = await server.getTransaction(sent.hash);
  }
  if (res.status !== "SUCCESS") throw new Error(`tx ${sent.hash}: ${res.status}`);
  return sent.hash;
}

const [, , cmd, ...rest] = process.argv;

if (cmd === "read") {
  const invoice = await simulate("get_invoice", nativeToScVal(BigInt(rest[0]), { type: "u64" }));
  console.log(JSON.stringify(invoice, (_, v) => (typeof v === "bigint" ? v.toString() : v), 2));
} else if (cmd === "create") {
  const [secret, client, amount, description, due] = rest;
  const signer = Keypair.fromSecret(secret);
  const id = await invoke(
    signer,
    "create_invoice",
    new Address(signer.publicKey()).toScVal(),
    new Address(client).toScVal(),
    nativeToScVal(BigInt(Math.round(Number(amount) * 10 ** deployment.tokenDecimals)), { type: "i128" }),
    nativeToScVal(description, { type: "string" }),
    nativeToScVal(BigInt(due), { type: "u64" })
  );
  console.log(`created invoice, tx: ${id}`);
} else {
  console.log("usage: node index.mjs read <id> | create <secret> <client> <amount> <desc> <due_epoch>");
  process.exit(1);
}
