import { NextResponse } from "next/server";
import {
  Address,
  BASE_FEE,
  Contract,
  Keypair,
  nativeToScVal,
  TransactionBuilder,
  rpc,
} from "@stellar/stellar-sdk";
import { createRateLimiter } from "@/lib/rate-limiter";

// Server-only faucet: mints test USDC (a Stellar Asset Contract) to a user.
// The SAC admin is the asset issuer; its secret lives in USDC_ISSUER_SECRET and
// is never exposed to the client.

const MINT_AMOUNT = 5_000_000_000n; // 500 USDC at 7 decimals

// Testnet-only guard: the faucet mints a demo asset and must never serve a
// deployment configured against a real-value network (e.g. PUBLIC). The
// passphrase comes from NEXT_PUBLIC_NETWORK_PASSPHRASE, the same config the
// rest of the app runs on.
const TESTNET_PASSPHRASE = "Test SDF Network ; September 2015";

// Rate limiter: can be in-memory (default, per-instance) or backed by
// Upstash Redis / Vercel KV for global limits. See RATE_LIMITER_* env vars
// and docs/ARCHITECTURE.md for setup.
const rateLimiter = createRateLimiter({
  perIpLimitPerHour: Number(process.env.RATE_LIMITER_IP_LIMIT_PER_HOUR ?? 5),
  globalDailyCapTokens: BigInt(
    process.env.RATE_LIMITER_GLOBAL_DAILY_CAP ?? 100_000_000_000n,
  ),
});

export async function POST(req: Request) {
  const issuerSecret = process.env.USDC_ISSUER_SECRET;
  const tokenId = process.env.NEXT_PUBLIC_TOKEN_ID;
  const rpcUrl =
    process.env.NEXT_PUBLIC_RPC_URL ?? "https://soroban-testnet.stellar.org";
  const passphrase =
    process.env.NEXT_PUBLIC_NETWORK_PASSPHRASE ?? TESTNET_PASSPHRASE;

  if (!issuerSecret || !tokenId) {
    return NextResponse.json(
      { error: "Faucet is not configured on the server." },
      { status: 503 },
    );
  }

  if (passphrase !== TESTNET_PASSPHRASE) {
    return NextResponse.json(
      { error: "Faucet is available on testnet only." },
      { status: 503 },
    );
  }

  let address: string;
  try {
    const body = await req.json();
    address = body.address;
    // Throws if the address is malformed.
    Address.fromString(address);
  } catch {
    return NextResponse.json(
      { error: "A valid wallet address is required." },
      { status: 400 },
    );
  }

  // Extract client IP (works on Vercel and most proxies)
  const ip =
    req.headers.get("x-forwarded-for")?.split(",")[0].trim() ||
    req.headers.get("x-real-ip") ||
    "unknown";

  const retryAfterSeconds = await rateLimiter.checkLimit(ip, address);
  if (retryAfterSeconds !== null) {
    return NextResponse.json(
      {
        error: `Faucet already dispensed to this wallet recently. Try again in ${Math.ceil(
          retryAfterSeconds / 60,
        )} minute(s).`,
      },
      { status: 429, headers: { "Retry-After": String(retryAfterSeconds) } },
    );
  }

  try {
    const issuer = Keypair.fromSecret(issuerSecret);
    const server = new rpc.Server(rpcUrl, {
      allowHttp: rpcUrl.startsWith("http://"),
    });
    const source = await server.getAccount(issuer.publicKey());
    const token = new Contract(tokenId);

    const tx = new TransactionBuilder(source, {
      fee: BASE_FEE,
      networkPassphrase: passphrase,
    })
      .addOperation(
        token.call(
          "mint",
          new Address(address).toScVal(),
          nativeToScVal(MINT_AMOUNT, { type: "i128" }),
        ),
      )
      .setTimeout(60)
      .build();

    const prepared = await server.prepareTransaction(tx);
    prepared.sign(issuer);

    const sent = await server.sendTransaction(prepared);
    if (sent.status === "ERROR") {
      return NextResponse.json(
        { error: "Mint transaction was rejected." },
        { status: 502 },
      );
    }

    // Best-effort confirmation.
    let result = await server.getTransaction(sent.hash);
    const started = Date.now();
    while (
      result.status === rpc.Api.GetTransactionStatus.NOT_FOUND &&
      Date.now() - started < 20_000
    ) {
      await new Promise((r) => setTimeout(r, 1500));
      result = await server.getTransaction(sent.hash);
    }

    if (result.status !== rpc.Api.GetTransactionStatus.SUCCESS) {
      return NextResponse.json(
        { error: "Mint did not confirm in time. Try again shortly." },
        { status: 504 },
      );
    }

    await rateLimiter.recordMint(ip, address, MINT_AMOUNT);
    const decimals = Number(process.env.NEXT_PUBLIC_TOKEN_DECIMALS ?? "7");
    const amount = Number(MINT_AMOUNT) / 10 ** decimals;
    return NextResponse.json({ ok: true, hash: sent.hash, amount });
  } catch (e: any) {
    return NextResponse.json(
      { error: e?.message ?? "Faucet failed unexpectedly." },
      { status: 500 },
    );
  }
}
