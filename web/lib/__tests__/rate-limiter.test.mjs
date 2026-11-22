import test from "node:test";
import assert from "node:assert/strict";
import { InMemoryRateLimiter } from "../rate-limiter.ts";

const TEST_CONFIG = {
  perIpLimitPerHour: 3,
  globalDailyCapTokens: 1_000_000_000n,
};

const newLimiter = () => new InMemoryRateLimiter(TEST_CONFIG);

test("allows first mint for new IP and address", async () => {
  const limiter = newLimiter();
  const result = await limiter.checkLimit("192.168.1.1", "GADDRESS1");
  assert.equal(result, null);
});

test("blocks second mint to same address within window", async () => {
  const limiter = newLimiter();
  const ip = "192.168.1.1";
  const address = "GADDRESS1";

  // First mint should succeed
  assert.equal(await limiter.checkLimit(ip, address), null);
  await limiter.recordMint(ip, address, 5_000_000_000n);

  // Second mint to same address should be blocked
  const result = await limiter.checkLimit(ip, address);
  assert.notEqual(result, null);
  assert.ok(result > 0);
});

test("enforces per-IP limit", async () => {
  const limiter = newLimiter();
  const ip = "192.168.1.1";
  const addresses = ["GADDRESS1", "GADDRESS2", "GADDRESS3", "GADDRESS4"];

  // Record mints up to the limit
  for (let i = 0; i < TEST_CONFIG.perIpLimitPerHour; i++) {
    assert.equal(await limiter.checkLimit(ip, addresses[i]), null);
    await limiter.recordMint(ip, addresses[i], 5_000_000_000n);
  }

  // Next mint from same IP should be blocked
  const result = await limiter.checkLimit(ip, addresses[3]);
  assert.notEqual(result, null);
});

test("allows different IPs independent limits", async () => {
  const limiter = newLimiter();
  const addr1 = "GADDRESS1";
  const addr2 = "GADDRESS2";
  const ip1 = "192.168.1.1";
  const ip2 = "192.168.1.2";

  // First IP mints
  assert.equal(await limiter.checkLimit(ip1, addr1), null);
  await limiter.recordMint(ip1, addr1, 5_000_000_000n);

  // Second IP with different address should be allowed
  assert.equal(await limiter.checkLimit(ip2, addr2), null);
  await limiter.recordMint(ip2, addr2, 5_000_000_000n);
});

test("getDailyTotal tracks total mints", async () => {
  const limiter = newLimiter();
  assert.equal(await limiter.getDailyTotal(), 0n);

  await limiter.recordMint("192.168.1.1", "GADDRESS1", 5_000_000_000n);
  assert.equal(await limiter.getDailyTotal(), 5_000_000_000n);

  await limiter.recordMint("192.168.1.2", "GADDRESS2", 3_000_000_000n);
  assert.equal(await limiter.getDailyTotal(), 8_000_000_000n);
});
