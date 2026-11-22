/**
 * Rate limiter interface and implementations.
 * Supports in-memory (default), Upstash Redis, and Vercel KV backends.
 */

export interface RateLimiterConfig {
  perIpLimitPerHour: number; // Max mints per IP per hour
  globalDailyCapTokens: bigint; // Global daily mint limit in base units
}

export interface RateLimiter {
  /**
   * Check if IP+address combination has hit rate limits.
   * Returns null if allowed, or remaining wait time in seconds if rate-limited.
   */
  checkLimit(ip: string, address: string): Promise<number | null>;

  /**
   * Record a successful mint.
   */
  recordMint(ip: string, address: string, amount: bigint): Promise<void>;

  /**
   * Get current daily mint total (for monitoring).
   */
  getDailyTotal(): Promise<bigint>;
}

const DEFAULT_CONFIG: RateLimiterConfig = {
  perIpLimitPerHour: 5,
  globalDailyCapTokens: 100_000_000_000n, // 10,000 USDC at 7 decimals
};

/**
 * In-memory rate limiter (default, per-instance, best-effort).
 * Used when no external store is configured.
 */
export class InMemoryRateLimiter implements RateLimiter {
  private perIpWindow = 60 * 60 * 1000; // 1 hour
  private perAddressWindow = 60 * 60 * 1000; // 1 hour
  private dailyWindow = 24 * 60 * 60 * 1000; // 1 day
  private ipMints = new Map<string, number[]>(); // IP -> array of timestamps
  private addressMints = new Map<string, number[]>(); // Address -> array of timestamps
  private dailyTotal = 0n;
  private dailyResetTime = Date.now() + this.dailyWindow;

  private config: RateLimiterConfig;

  constructor(config: RateLimiterConfig = DEFAULT_CONFIG) {
    // Explicit field (not a parameter property) so `node --test` can load this
    // file with type-stripping — see lib/__tests__/rate-limiter.test.mjs.
    this.config = config;
  }

  async checkLimit(ip: string, address: string): Promise<number | null> {
    const now = Date.now();

    // Reset daily total if window has passed
    if (now > this.dailyResetTime) {
      this.dailyTotal = 0n;
      this.dailyResetTime = now + this.dailyWindow;
    }

    // Check per-IP limit
    const ipMintTimes = this.ipMints.get(ip) ?? [];
    const recentIpMints = ipMintTimes.filter((ts) => now - ts < this.perIpWindow);
    if (recentIpMints.length >= this.config.perIpLimitPerHour) {
      const oldestMint = Math.min(...recentIpMints);
      const remainingMs = this.perIpWindow - (now - oldestMint);
      return Math.ceil(remainingMs / 1000);
    }

    // Check per-address limit (keep existing wallet-level protection)
    const addressMintTimes = this.addressMints.get(address) ?? [];
    const recentAddressMints = addressMintTimes.filter(
      (ts) => now - ts < this.perAddressWindow
    );
    if (recentAddressMints.length > 0) {
      const lastMint = Math.max(...recentAddressMints);
      const remainingMs = this.perAddressWindow - (now - lastMint);
      return Math.ceil(remainingMs / 1000);
    }

    return null;
  }

  async recordMint(ip: string, address: string, amount: bigint): Promise<void> {
    const now = Date.now();

    // Record IP mint
    const ipMintTimes = this.ipMints.get(ip) ?? [];
    ipMintTimes.push(now);
    this.ipMints.set(ip, ipMintTimes);

    // Record address mint
    const addressMintTimes = this.addressMints.get(address) ?? [];
    addressMintTimes.push(now);
    this.addressMints.set(address, addressMintTimes);

    // Update daily total
    this.dailyTotal += amount;

    // Cleanup old entries
    this.cleanupOldEntries();
  }

  async getDailyTotal(): Promise<bigint> {
    const now = Date.now();
    if (now > this.dailyResetTime) {
      this.dailyTotal = 0n;
      this.dailyResetTime = now + this.dailyWindow;
    }
    return this.dailyTotal;
  }

  private cleanupOldEntries(): void {
    const now = Date.now();
    // Cleanup IP mints
    for (const [ip, times] of this.ipMints) {
      const filtered = times.filter((ts) => now - ts < this.perIpWindow);
      if (filtered.length === 0) {
        this.ipMints.delete(ip);
      } else {
        this.ipMints.set(ip, filtered);
      }
    }
    // Cleanup address mints
    for (const [address, times] of this.addressMints) {
      const filtered = times.filter((ts) => now - ts < this.perAddressWindow);
      if (filtered.length === 0) {
        this.addressMints.delete(address);
      } else {
        this.addressMints.set(address, filtered);
      }
    }
  }
}

/**
 * Redis-backed rate limiter using Upstash Redis.
 * Requires UPSTASH_REDIS_REST_URL and UPSTASH_REDIS_REST_TOKEN env vars.
 */
export class RedisRateLimiter implements RateLimiter {
  private client: any;
  private perIpWindow = 3600; // 1 hour in seconds
  private perAddressWindow = 3600; // 1 hour in seconds
  private dailyWindow = 86400; // 1 day in seconds

  private config: RateLimiterConfig;

  constructor(config: RateLimiterConfig = DEFAULT_CONFIG) {
    this.config = config;
    const url = process.env.UPSTASH_REDIS_REST_URL;
    const token = process.env.UPSTASH_REDIS_REST_TOKEN;

    if (!url || !token) {
      throw new Error(
        "Redis rate limiter requires UPSTASH_REDIS_REST_URL and UPSTASH_REDIS_REST_TOKEN"
      );
    }

    // Simple Redis client using fetch-based REST API
    this.client = {
      baseUrl: url,
      token,
      request: async (method: string, key: string, ...args: any[]) => {
        const response = await fetch(`${url}/${method}/${key}`, {
          method: "POST",
          headers: {
            Authorization: `Bearer ${token}`,
            "Content-Type": "application/json",
          },
          body: JSON.stringify(args),
        });
        const result = await response.json();
        if (result.error) throw new Error(result.error);
        return result.result;
      },
    };
  }

  async checkLimit(ip: string, address: string): Promise<number | null> {
    const ipKey = `faucet:ip:${ip}`;
    const addressKey = `faucet:addr:${address}`;

    // Check per-IP limit
    const ipCount = await this.client.request("GET", ipKey);
    if (ipCount && Number(ipCount) >= this.config.perIpLimitPerHour) {
      const ttl = await this.client.request("TTL", ipKey);
      return Math.max(1, ttl as number);
    }

    // Check per-address limit
    const addressCount = await this.client.request("GET", addressKey);
    if (addressCount && Number(addressCount) > 0) {
      const ttl = await this.client.request("TTL", addressKey);
      return Math.max(1, ttl as number);
    }

    return null;
  }

  async recordMint(ip: string, address: string, amount: bigint): Promise<void> {
    const ipKey = `faucet:ip:${ip}`;
    const addressKey = `faucet:addr:${address}`;
    const dailyKey = `faucet:daily:total`;

    await this.client.request("INCR", ipKey);
    await this.client.request("EXPIRE", ipKey, String(this.perIpWindow));

    await this.client.request("INCR", addressKey);
    await this.client.request("EXPIRE", addressKey, String(this.perAddressWindow));

    await this.client.request("INCRBY", dailyKey, String(amount));
    await this.client.request("EXPIRE", dailyKey, String(this.dailyWindow));
  }

  async getDailyTotal(): Promise<bigint> {
    const dailyKey = `faucet:daily:total`;
    const total = await this.client.request("GET", dailyKey);
    return BigInt(total ?? 0);
  }
}

/**
 * Vercel KV-backed rate limiter.
 * Requires KV_REST_API_URL and KV_REST_API_TOKEN env vars.
 */
export class VercelKVRateLimiter implements RateLimiter {
  private client: any;
  private perIpWindow = 3600;
  private perAddressWindow = 3600;
  private dailyWindow = 86400;

  private config: RateLimiterConfig;

  constructor(config: RateLimiterConfig = DEFAULT_CONFIG) {
    this.config = config;
    const url = process.env.KV_REST_API_URL;
    const token = process.env.KV_REST_API_TOKEN;

    if (!url || !token) {
      throw new Error(
        "Vercel KV rate limiter requires KV_REST_API_URL and KV_REST_API_TOKEN"
      );
    }

    this.client = {
      baseUrl: url,
      token,
      request: async (method: string, key: string, ...args: any[]) => {
        const response = await fetch(`${url}/${method}/${key}`, {
          method: "POST",
          headers: {
            Authorization: `Bearer ${token}`,
            "Content-Type": "application/json",
          },
          body: JSON.stringify(args),
        });
        const result = await response.json();
        if (result.error) throw new Error(result.error);
        return result.result;
      },
    };
  }

  async checkLimit(ip: string, address: string): Promise<number | null> {
    const ipKey = `faucet:ip:${ip}`;
    const addressKey = `faucet:addr:${address}`;

    const ipCount = await this.client.request("GET", ipKey);
    if (ipCount && Number(ipCount) >= this.config.perIpLimitPerHour) {
      const ttl = await this.client.request("TTL", ipKey);
      return Math.max(1, ttl as number);
    }

    const addressCount = await this.client.request("GET", addressKey);
    if (addressCount && Number(addressCount) > 0) {
      const ttl = await this.client.request("TTL", addressKey);
      return Math.max(1, ttl as number);
    }

    return null;
  }

  async recordMint(ip: string, address: string, amount: bigint): Promise<void> {
    const ipKey = `faucet:ip:${ip}`;
    const addressKey = `faucet:addr:${address}`;
    const dailyKey = `faucet:daily:total`;

    await this.client.request("INCR", ipKey);
    await this.client.request("EXPIRE", ipKey, String(this.perIpWindow));

    await this.client.request("INCR", addressKey);
    await this.client.request("EXPIRE", addressKey, String(this.perAddressWindow));

    await this.client.request("INCRBY", dailyKey, String(amount));
    await this.client.request("EXPIRE", dailyKey, String(this.dailyWindow));
  }

  async getDailyTotal(): Promise<bigint> {
    const dailyKey = `faucet:daily:total`;
    const total = await this.client.request("GET", dailyKey);
    return BigInt(total ?? 0);
  }
}

/**
 * Factory to create the appropriate rate limiter based on env vars.
 */
export function createRateLimiter(
  config: RateLimiterConfig = DEFAULT_CONFIG
): RateLimiter {
  if (process.env.UPSTASH_REDIS_REST_URL && process.env.UPSTASH_REDIS_REST_TOKEN) {
    return new RedisRateLimiter(config);
  }

  if (process.env.KV_REST_API_URL && process.env.KV_REST_API_TOKEN) {
    return new VercelKVRateLimiter(config);
  }

  return new InMemoryRateLimiter(config);
}
