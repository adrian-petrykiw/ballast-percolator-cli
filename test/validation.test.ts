import {
  validatePublicKey,
  validateIndex,
  validateAmount,
  validateU128,
  validateI64,
  validateI128,
  validateBps,
  ValidationError,
} from '../src/validation.js';
import { describe, expect, test } from 'vitest';

describe('validatePublicKey', () => {
  test('accepts valid pubkeys', () => {
    const pk = validatePublicKey('11111111111111111111111111111111', '--slab');
    expect(pk.toBase58()).toBe('11111111111111111111111111111111');

    const pk2 = validatePublicKey('3K1P8KXJHg4Uk2upGiorjjFdSxGxq2sjxrrFaBjZ34D9', '--slab');
    expect(pk2.toBase58()).toBe('3K1P8KXJHg4Uk2upGiorjjFdSxGxq2sjxrrFaBjZ34D9');
  });

  test('rejects invalid pubkeys', () => {
    expect(() => validatePublicKey('invalid', '--slab')).toThrow('not a valid base58');
    expect(() => validatePublicKey('', '--slab')).toThrow('not a valid base58');
  });
});

describe('validateIndex', () => {
  test('accepts valid indices', () => {
    expect(validateIndex('0', '--idx')).toBe(0);
    expect(validateIndex('123', '--idx')).toBe(123);
    expect(validateIndex('65535', '--idx')).toBe(65535);
  });

  test('rejects invalid indices', () => {
    expect(() => validateIndex('-1', '--idx')).toThrow('non-negative');
    expect(() => validateIndex('65536', '--idx')).toThrow('65535');
    expect(() => validateIndex('abc', '--idx')).toThrow('not a valid number');
  });
});

describe('validateAmount', () => {
  test('accepts valid amounts', () => {
    expect(validateAmount('0', '--amt')).toBe(0n);
    expect(validateAmount('1000000000000', '--amt')).toBe(1000000000000n);
    expect(validateAmount('18446744073709551615', '--amt')).toBe(18446744073709551615n);
  });

  test('rejects invalid amounts', () => {
    expect(() => validateAmount('-100', '--amt')).toThrow('non-negative');
    expect(() => validateAmount('18446744073709551616', '--amt')).toThrow('u64 max');
    expect(() => validateAmount('abc', '--amt')).toThrow('not a valid number');
  });
});

describe('validateU128', () => {
  test('accepts valid u128 values', () => {
    expect(validateU128('0', '--val')).toBe(0n);
    const u128Max = '340282366920938463463374607431768211455';
    expect(validateU128(u128Max, '--val')).toBe(340282366920938463463374607431768211455n);
  });

  test('rejects invalid u128 values', () => {
    expect(() => validateU128('-1', '--val')).toThrow('non-negative');
    expect(() => validateU128('340282366920938463463374607431768211456', '--val')).toThrow(
      'u128 max',
    );
  });
});

describe('validateI64', () => {
  test('accepts valid i64 values', () => {
    expect(validateI64('0', '--val')).toBe(0n);
    expect(validateI64('1000', '--val')).toBe(1000n);
    expect(validateI64('-1000', '--val')).toBe(-1000n);
    expect(validateI64('9223372036854775807', '--val')).toBe(9223372036854775807n);
    expect(validateI64('-9223372036854775808', '--val')).toBe(-9223372036854775808n);
  });

  test('rejects out-of-range i64 values', () => {
    expect(() => validateI64('9223372036854775808', '--val')).toThrow('i64 max');
    expect(() => validateI64('-9223372036854775809', '--val')).toThrow('i64 min');
  });
});

describe('validateI128', () => {
  test('accepts valid i128 values', () => {
    expect(validateI128('0', '--size')).toBe(0n);
    expect(validateI128('500', '--size')).toBe(500n);
    expect(validateI128('-500', '--size')).toBe(-500n);
    const i128Max = '170141183460469231731687303715884105727';
    expect(validateI128(i128Max, '--size')).toBe(170141183460469231731687303715884105727n);
    const i128Min = '-170141183460469231731687303715884105728';
    expect(validateI128(i128Min, '--size')).toBe(-170141183460469231731687303715884105728n);
  });

  test('rejects out-of-range i128 values', () => {
    expect(() => validateI128('170141183460469231731687303715884105728', '--size')).toThrow(
      'i128 max',
    );
    expect(() => validateI128('-170141183460469231731687303715884105729', '--size')).toThrow(
      'i128 min',
    );
  });
});

describe('validateBps', () => {
  test('accepts valid bps values', () => {
    expect(validateBps('0', '--bps')).toBe(0);
    expect(validateBps('10000', '--bps')).toBe(10000);
    expect(validateBps('5000', '--bps')).toBe(5000);
  });

  test('rejects invalid bps values', () => {
    expect(() => validateBps('-1', '--bps')).toThrow('non-negative');
    expect(() => validateBps('10001', '--bps')).toThrow('10000');
  });
});

describe('ValidationError', () => {
  test('has correct properties', () => {
    const err = new ValidationError('--amount', 'must be positive');
    expect(err.message).toContain('--amount');
    expect(err.message).toContain('must be positive');
    expect(err.name).toBe('ValidationError');
    expect(err.field).toBe('--amount');
  });
});
