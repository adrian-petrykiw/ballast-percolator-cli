import { PublicKey } from '@solana/web3.js';
import {
  parseHeader,
  parseConfig,
  readNonce,
  readMatCounter,
  parseAccount,
  parseUsedIndices,
  isAccountUsed,
  AccountKind,
} from '../src/solana/slab.js';
import { describe, expect, test } from 'vitest';

// Header layout (v12.21): 136 bytes total
//   [0..8]   magic, [8..12] version, [12] bump, [13] flags
//   [16..48] admin, [48..56] nonce (RESERVED_OFF=48), [56..64] matCounter
//   [72..104] insuranceAuthority, [104..136] insuranceOperator
// Config layout: CONFIG_OFFSET = 136, CONFIG_LEN = 384
const CONFIG_OFFSET = 136;

function createMockSlab(): Buffer {
  const buf = Buffer.alloc(592); // 136 (header) + 384 (config) + padding

  buf.writeBigUInt64LE(0x504552434f4c4154n, 0); // magic
  buf.writeUInt32LE(1, 8); // version: 1
  buf.writeUInt8(255, 12); // bump: 255
  const adminBytes = Buffer.alloc(32);
  adminBytes[0] = 1;
  adminBytes.copy(buf, 16); // admin at [16..48]
  buf.writeBigUInt64LE(42n, 48); // nonce (RESERVED_OFF = 48)
  buf.writeBigUInt64LE(12345n, 56); // matCounter (RESERVED_OFF + 8)

  // MarketConfig — write at CONFIG_OFFSET = 136
  const mintBytes = Buffer.alloc(32);
  mintBytes[0] = 2;
  mintBytes.copy(buf, CONFIG_OFFSET); // collateralMint
  const vaultBytes = Buffer.alloc(32);
  vaultBytes[0] = 3;
  vaultBytes.copy(buf, CONFIG_OFFSET + 32); // vaultPubkey
  const feedIdBytes = Buffer.alloc(32);
  feedIdBytes[0] = 5;
  feedIdBytes.copy(buf, CONFIG_OFFSET + 64); // indexFeedId
  buf.writeBigUInt64LE(100n, CONFIG_OFFSET + 96); // maxStalenessSecs
  buf.writeUInt16LE(50, CONFIG_OFFSET + 104); // confFilterBps
  buf.writeUInt8(254, CONFIG_OFFSET + 106); // vaultAuthorityBump
  buf.writeUInt8(1, CONFIG_OFFSET + 107); // invert
  buf.writeUInt32LE(0, CONFIG_OFFSET + 108); // unitScale

  return buf;
}

// Constants keep in sync with slab.ts (64-account canonical slab: slabLen=25216)
const ENGINE_OFF = 520;
const ENGINE_BITMAP_OFF = 712;
const ACCOUNT_SIZE = 360;
const ENGINE_ACCOUNTS_OFF = 984; // computeLayout(64).engineAccountsOff
const SLAB_64_LEN = 25216; // layoutForDataLength requires an exact canonical size

const ACCT_CAPITAL_OFF = 0;
const ACCT_KIND_OFF = 16;
const ACCT_PNL_OFF = 24;
const ACCT_POSITION_BASIS_Q_OFF = 56;
const ACCT_MATCHER_PROGRAM_OFF = 128;
const ACCT_OWNER_OFF = 192;

function writeU128LE(buf: Buffer, offset: number, value: bigint): void {
  const lo = value & BigInt('0xFFFFFFFFFFFFFFFF');
  const hi = (value >> 64n) & BigInt('0xFFFFFFFFFFFFFFFF');
  buf.writeBigUInt64LE(lo, offset);
  buf.writeBigUInt64LE(hi, offset + 8);
}

function writeI128LE(buf: Buffer, offset: number, value: bigint): void {
  if (value < 0n) {
    value = (1n << 128n) + value;
  }
  writeU128LE(buf, offset, value);
}

function createFullMockSlab(): Buffer {
  const minSize = SLAB_64_LEN;
  const buf = Buffer.alloc(minSize);

  buf.writeBigUInt64LE(0x504552434f4c4154n, 0);
  buf.writeUInt32LE(1, 8);
  buf.writeUInt8(255, 12);
  const adminBytes = Buffer.alloc(32);
  adminBytes[0] = 1;
  adminBytes.copy(buf, 16);
  buf.writeBigUInt64LE(42n, 48);
  buf.writeBigUInt64LE(12345n, 56);

  // Bitmap: mark accounts 0 and 1 as used
  buf.writeBigUInt64LE(3n, ENGINE_OFF + ENGINE_BITMAP_OFF);

  // Account 0 (LP)
  const acc0Base = ENGINE_OFF + ENGINE_ACCOUNTS_OFF + 0 * ACCOUNT_SIZE;
  writeU128LE(buf, acc0Base + ACCT_CAPITAL_OFF, 1000000000n);
  buf.writeUInt8(1, acc0Base + ACCT_KIND_OFF); // LP
  writeI128LE(buf, acc0Base + ACCT_PNL_OFF, 0n);
  writeI128LE(buf, acc0Base + ACCT_POSITION_BASIS_Q_OFF, 0n);
  const matcherProg = Buffer.alloc(32);
  matcherProg[0] = 0xaa;
  matcherProg.copy(buf, acc0Base + ACCT_MATCHER_PROGRAM_OFF);
  const owner0 = Buffer.alloc(32);
  owner0[0] = 0x11;
  owner0.copy(buf, acc0Base + ACCT_OWNER_OFF);

  // Account 1 (User)
  const acc1Base = ENGINE_OFF + ENGINE_ACCOUNTS_OFF + 1 * ACCOUNT_SIZE;
  writeU128LE(buf, acc1Base + ACCT_CAPITAL_OFF, 500000000n);
  buf.writeUInt8(0, acc1Base + ACCT_KIND_OFF); // User
  writeI128LE(buf, acc1Base + ACCT_PNL_OFF, -100000n);
  writeI128LE(buf, acc1Base + ACCT_POSITION_BASIS_Q_OFF, 1000000n);
  const owner1 = Buffer.alloc(32);
  owner1[0] = 0x22;
  owner1.copy(buf, acc1Base + ACCT_OWNER_OFF);

  return buf;
}

describe('slab parsing', () => {
  test('parseHeader', () => {
    const header = parseHeader(createMockSlab());
    expect(header.magic).toBe(0x504552434f4c4154n);
    expect(header.version).toBe(1);
    expect(header.bump).toBe(255);
    expect(header.admin).toBeInstanceOf(PublicKey);
    expect(header.nonce).toBe(42n);
    expect(header.matCounter).toBe(12345n);
  });

  test('parseConfig', () => {
    const config = parseConfig(createMockSlab());
    expect(config.collateralMint).toBeInstanceOf(PublicKey);
    expect(config.vaultPubkey).toBeInstanceOf(PublicKey);
    expect(config.indexFeedId).toBeInstanceOf(PublicKey);
    expect(config.maxStalenessSecs).toBe(100n);
    expect(config.confFilterBps).toBe(50);
    expect(config.vaultAuthorityBump).toBe(254);
    expect(config.invert).toBe(1);
    expect(config.unitScale).toBe(0);
  });

  test('readNonce', () => {
    expect(readNonce(createMockSlab())).toBe(42n);
  });

  test('readMatCounter', () => {
    expect(readMatCounter(createMockSlab())).toBe(12345n);
  });

  test('parseHeader rejects invalid magic', () => {
    const slab = createMockSlab();
    slab.writeBigUInt64LE(0n, 0);
    expect(() => parseHeader(slab)).toThrow('Invalid slab magic');
  });

  test('parseHeader rejects short buffer', () => {
    expect(() => parseHeader(Buffer.alloc(32))).toThrow();
  });
});

describe('account parsing', () => {
  test('parseAccount kind field (LP vs User)', () => {
    const slab = createFullMockSlab();
    const acc0 = parseAccount(slab, 0);
    expect(acc0.kind).toBe(AccountKind.LP);
    expect(acc0.capital).toBe(1000000000n);

    const acc1 = parseAccount(slab, 1);
    expect(acc1.kind).toBe(AccountKind.User);
    expect(acc1.capital).toBe(500000000n);
  });

  test('parseAccount fields (position, pnl, owner)', () => {
    const acc1 = parseAccount(createFullMockSlab(), 1);
    expect(acc1.positionBasisQ).toBe(1000000n);
    expect(acc1.pnl).toBe(-100000n);
    expect(acc1.owner).toBeInstanceOf(PublicKey);
  });

  test('parseUsedIndices bitmap parsing', () => {
    const indices = parseUsedIndices(createFullMockSlab());
    expect(indices).toHaveLength(2);
    expect(indices).toContain(0);
    expect(indices).toContain(1);
    expect(indices).not.toContain(2);
  });

  test('isAccountUsed', () => {
    const slab = createFullMockSlab();
    expect(isAccountUsed(slab, 0)).toBe(true);
    expect(isAccountUsed(slab, 1)).toBe(true);
    expect(isAccountUsed(slab, 2)).toBe(false);
    expect(isAccountUsed(slab, 64)).toBe(false);
  });

  test('parseAccount rejects out of bounds index', () => {
    expect(() => parseAccount(createFullMockSlab(), 10000)).toThrow('out of range');
  });

  test('parseAccount rejects negative index', () => {
    expect(() => parseAccount(createFullMockSlab(), -1)).toThrow();
  });
});
