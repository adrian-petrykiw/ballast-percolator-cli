/**
 * Minimal Pyth `PriceUpdateV2` decoder for log-time sanity checks.
 *
 * Used by `setup-ballast-sol-market.ts` to verify the on-chain Pyth oracle
 * is live (owner = receiver program, fresh publish_time, sane price) before
 * passing it into Percolator's `init-market`. Mirrors the read-only role of
 * `parseChainlinkPrice` in `src/solana/oracle.ts` but for Pyth Pull feeds.
 *
 * The full SDK (`@pythnetwork/pyth-solana-receiver`) needs a Wallet to
 * instantiate `PythSolanaReceiver`; for buffer-only decoding we hand-parse.
 *
 * `PriceUpdateV2` layout (Anchor; allocated 134 bytes for the worst case):
 *   0..8     anchor discriminator
 *   8..40    write_authority (Pubkey)
 *   40..N    verification_level (borsh enum — VARIABLE size):
 *               tag=0 Partial { numSignatures: u8 } → 2 bytes (N=42)
 *               tag=1 Full                          → 1 byte  (N=41)
 *   N..N+84  price_message (PriceFeedMessage)
 *     +0..32   feed_id            (32 bytes, big-endian hex when stringified)
 *     +32..40  price              (i64 LE)
 *     +40..48  conf               (u64 LE)
 *     +48..52  exponent           (i32 LE, typically negative)
 *     +52..60  publish_time       (i64 LE, unix seconds)
 *     +60..68  prev_publish_time  (i64 LE)
 *     +68..76  ema_price          (i64 LE)
 *     +76..84  ema_conf           (u64 LE)
 *   N+84..N+92  posted_slot       (u64 LE)
 *
 * Source: pyth-crosschain target_chains/solana/pyth_solana_receiver_sdk
 *         (PriceUpdateV2, PriceFeedMessage, VerificationLevel — confirmed
 *         against the bundled IDL in @pythnetwork/pyth-solana-receiver
 *         v0.13.0). Sponsored push feeds use VerificationLevel::Full, so
 *         the message starts at offset 41, not 42; reading at fixed 42
 *         (an earlier draft of this file) produced garbled prices.
 */

import { Connection, PublicKey } from "@solana/web3.js";

export const PYTH_RECEIVER_PROGRAM_ID = new PublicKey(
  "rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ",
);

/** Anchor allocates the worst-case (Partial) size for the account. */
export const PRICE_UPDATE_V2_MAX_LEN = 134;
export const PRICE_UPDATE_V2_MIN_LEN = 133;

const VERIFICATION_LEVEL_OFF = 40;
const VL_FULL = 1;
const VL_PARTIAL = 0;

export interface PythPrice {
  feedIdHex: string;
  price: bigint;
  conf: bigint;
  exponent: number;
  publishTime: number;
  postedSlot: bigint;
  /** price * 10^exponent, as a JS number — for human-readable logs only. */
  priceFloat: number;
  /** Confidence as a fraction of price, in basis points (truncated to integer). */
  confBps: number;
}

export function parsePythPriceUpdateV2(data: Buffer): PythPrice {
  if (data.length < PRICE_UPDATE_V2_MIN_LEN) {
    throw new Error(
      `PriceUpdateV2 too short: ${data.length} bytes (need >= ${PRICE_UPDATE_V2_MIN_LEN})`,
    );
  }
  const tag = data.readUInt8(VERIFICATION_LEVEL_OFF);
  let messageOff: number;
  if (tag === VL_FULL) {
    messageOff = 41;
  } else if (tag === VL_PARTIAL) {
    messageOff = 42;
  } else {
    throw new Error(`Unknown VerificationLevel tag: ${tag} (expected 0=Partial, 1=Full)`);
  }

  const feedIdHex = Buffer.from(data.subarray(messageOff, messageOff + 32)).toString("hex");
  const price = data.readBigInt64LE(messageOff + 32);
  const conf = data.readBigUInt64LE(messageOff + 40);
  const exponent = data.readInt32LE(messageOff + 48);
  const publishTime = Number(data.readBigInt64LE(messageOff + 52));
  const postedSlot = data.readBigUInt64LE(messageOff + 84);

  if (price <= 0n) throw new Error(`Pyth price non-positive: ${price}`);
  if (exponent < -18 || exponent > 0) {
    throw new Error(`Pyth exponent out of expected range: ${exponent}`);
  }

  const priceFloat = Number(price) * Math.pow(10, exponent);
  const confBps = price === 0n ? 0 : Number((conf * 10_000n) / BigInt(price));

  return { feedIdHex, price, conf, exponent, publishTime, postedSlot, priceFloat, confBps };
}

export interface VerifyPythOptions {
  /** Reject if the feed_id stored on-chain doesn't match (hex, no 0x). */
  expectFeedIdHex?: string;
  /** Reject if publish_time is older than this many seconds. Default 300. */
  maxAgeSecs?: number;
  /** Reject if confidence exceeds this (in bps of price). Default 500 (5%). */
  maxConfBps?: number;
}

export interface VerifiedPythPrice extends PythPrice {
  account: PublicKey;
  ageSec: number;
}

export async function verifyPythPriceAccount(
  conn: Connection,
  pubkey: PublicKey,
  opts: VerifyPythOptions = {},
): Promise<VerifiedPythPrice> {
  const info = await conn.getAccountInfo(pubkey);
  if (!info) throw new Error(`Pyth account not found: ${pubkey.toBase58()}`);
  if (!info.owner.equals(PYTH_RECEIVER_PROGRAM_ID)) {
    throw new Error(
      `Pyth owner mismatch for ${pubkey.toBase58()}: got ${info.owner.toBase58()}, expected ${PYTH_RECEIVER_PROGRAM_ID.toBase58()}`,
    );
  }
  const parsed = parsePythPriceUpdateV2(info.data);

  if (opts.expectFeedIdHex) {
    const expected = opts.expectFeedIdHex.toLowerCase().replace(/^0x/, "");
    if (parsed.feedIdHex !== expected) {
      throw new Error(
        `Pyth feed_id mismatch on ${pubkey.toBase58()}: got ${parsed.feedIdHex}, expected ${expected}`,
      );
    }
  }

  const ageSec = Math.floor(Date.now() / 1000) - parsed.publishTime;
  const maxAge = opts.maxAgeSecs ?? 300;
  if (ageSec < 0 || ageSec > maxAge) {
    throw new Error(`Pyth feed stale: age=${ageSec}s (max ${maxAge}s)`);
  }
  const maxConf = opts.maxConfBps ?? 500;
  if (parsed.confBps > maxConf) {
    throw new Error(`Pyth confidence too wide: ${parsed.confBps}bps (max ${maxConf}bps)`);
  }

  return { ...parsed, account: pubkey, ageSec };
}
