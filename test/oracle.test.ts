import { parseChainlinkPrice } from '../src/solana/oracle.js';
import { describe, expect, test } from 'vitest';

function buildChainlinkBuffer(decimals: number, answer: bigint, size = 256): Buffer {
  const buf = Buffer.alloc(size);
  buf.writeUInt8(decimals, 138);
  buf.writeBigInt64LE(answer, 216);
  return buf;
}

describe('parseChainlinkPrice', () => {
  test('parses valid oracle data', () => {
    const buf = buildChainlinkBuffer(8, 10012345678n); // $100.12345678
    const result = parseChainlinkPrice(buf);
    expect(result.decimals).toBe(8);
    expect(result.price).toBe(10012345678n);
  });

  test('handles various decimal values', () => {
    const r6 = parseChainlinkPrice(buildChainlinkBuffer(6, 100_000_000n));
    expect(r6.decimals).toBe(6);
    expect(r6.price).toBe(100_000_000n);

    const r0 = parseChainlinkPrice(buildChainlinkBuffer(0, 42n));
    expect(r0.decimals).toBe(0);
    expect(r0.price).toBe(42n);
  });

  test('rejects buffer < 224 bytes', () => {
    expect(() => parseChainlinkPrice(Buffer.alloc(100))).toThrow('too small');
    expect(() => parseChainlinkPrice(Buffer.alloc(223))).toThrow('too small');
  });

  test('accepts minimal 224-byte buffer', () => {
    const minimal = Buffer.alloc(224);
    minimal.writeUInt8(8, 138);
    minimal.writeBigInt64LE(1000n, 216);
    expect(parseChainlinkPrice(minimal).price).toBe(1000n);
  });

  test('rejects empty buffer', () => {
    expect(() => parseChainlinkPrice(Buffer.alloc(0))).toThrow('too small');
  });

  test('rejects zero price', () => {
    expect(() => parseChainlinkPrice(buildChainlinkBuffer(8, 0n))).toThrow('non-positive');
  });

  test('rejects negative price', () => {
    expect(() => parseChainlinkPrice(buildChainlinkBuffer(8, -100n))).toThrow('non-positive');
  });

  test('rejects decimals > 18', () => {
    expect(() => parseChainlinkPrice(buildChainlinkBuffer(19, 1000n))).toThrow('decimals');
    expect(() => parseChainlinkPrice(buildChainlinkBuffer(255, 1000n))).toThrow('decimals');
  });

  test('accepts 18 decimals', () => {
    const r18 = parseChainlinkPrice(buildChainlinkBuffer(18, 1000n));
    expect(r18.decimals).toBe(18);
  });
});
