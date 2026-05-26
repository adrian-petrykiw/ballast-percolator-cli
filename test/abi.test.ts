import { PublicKey } from '@solana/web3.js';
import { encU8, encU16, encU64, encI64, encU128, encI128, encPubkey } from '../src/abi/encode.js';
import {
  encodeInitUser,
  encodeDepositCollateral,
  encodeWithdrawCollateral,
  encodeKeeperCrank,
  encodeTradeNoCpi,
  encodeTradeCpi,
  encodeLiquidateAtOracle,
  encodeCloseAccount,
  encodeTopUpInsurance,
  encodeInitLP,
  encodeAdminForceCloseAccount,
  encodeWithdrawInsuranceLimited,
  encodeUpdateAdmin,
  encodeUpdateAuthority,
  AUTHORITY_KIND,
  IX_TAG,
} from '../src/abi/instructions.js';
import { describe, expect, test } from 'vitest';

describe('encode primitives', () => {
  test('encU8', () => {
    expect([...encU8(0)]).toEqual([0]);
    expect([...encU8(255)]).toEqual([255]);
    expect([...encU8(127)]).toEqual([127]);
  });

  test('encU16', () => {
    expect([...encU16(0)]).toEqual([0, 0]);
    expect([...encU16(1)]).toEqual([1, 0]);
    expect([...encU16(256)]).toEqual([0, 1]);
    expect([...encU16(0xabcd)]).toEqual([0xcd, 0xab]);
    expect([...encU16(65535)]).toEqual([255, 255]);
  });

  test('encU64', () => {
    expect([...encU64(0n)]).toEqual([0, 0, 0, 0, 0, 0, 0, 0]);
    expect([...encU64(1n)]).toEqual([1, 0, 0, 0, 0, 0, 0, 0]);
    expect([...encU64(256n)]).toEqual([0, 1, 0, 0, 0, 0, 0, 0]);
    expect([...encU64('1000000')]).toEqual([64, 66, 15, 0, 0, 0, 0, 0]);
    expect([...encU64(0xffff_ffff_ffff_ffffn)]).toEqual([255, 255, 255, 255, 255, 255, 255, 255]);
  });

  test('encI64', () => {
    expect([...encI64(0n)]).toEqual([0, 0, 0, 0, 0, 0, 0, 0]);
    expect([...encI64(1n)]).toEqual([1, 0, 0, 0, 0, 0, 0, 0]);
    expect([...encI64(-1n)]).toEqual([255, 255, 255, 255, 255, 255, 255, 255]);
    expect([...encI64(-2n)]).toEqual([254, 255, 255, 255, 255, 255, 255, 255]);
    expect([...encI64('-100')]).toEqual([156, 255, 255, 255, 255, 255, 255, 255]);
  });

  test('encU128', () => {
    expect([...encU128(0n)]).toEqual([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    expect([...encU128(1n)]).toEqual([1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    expect([...encU128(1n << 64n)]).toEqual([0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]);
    const large = 0x0102030405060708_090a0b0c0d0e0f10n;
    expect([...encU128(large)]).toEqual([
      0x10, 0x0f, 0x0e, 0x0d, 0x0c, 0x0b, 0x0a, 0x09, 0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02,
      0x01,
    ]);
  });

  test('encI128', () => {
    expect([...encI128(0n)]).toEqual([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    expect([...encI128(1n)]).toEqual([1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    expect([...encI128(-1n)]).toEqual([
      255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ]);
    expect([...encI128(-2n)]).toEqual([
      254, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ]);
    expect([...encI128(1000000n)]).toEqual([64, 66, 15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    expect([...encI128(-1000000n)]).toEqual([
      192, 189, 240, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ]);
  });

  test('encPubkey', () => {
    const pk = new PublicKey('11111111111111111111111111111111');
    const buf = encPubkey(pk);
    expect(buf.length).toBe(32);
    expect(buf.equals(Buffer.from(pk.toBytes()))).toBe(true);
  });
});

describe('IX_TAG values', () => {
  test('surviving tags', () => {
    expect(IX_TAG.InitMarket).toBe(0);
    expect(IX_TAG.InitUser).toBe(1);
    expect(IX_TAG.InitLP).toBe(2);
    expect(IX_TAG.DepositCollateral).toBe(3);
    expect(IX_TAG.WithdrawCollateral).toBe(4);
    expect(IX_TAG.KeeperCrank).toBe(5);
    expect(IX_TAG.TradeNoCpi).toBe(6);
    expect(IX_TAG.LiquidateAtOracle).toBe(7);
    expect(IX_TAG.CloseAccount).toBe(8);
    expect(IX_TAG.TopUpInsurance).toBe(9);
    expect(IX_TAG.TradeCpi).toBe(10);
    expect(IX_TAG.CloseSlab).toBe(13);
    expect(IX_TAG.UpdateConfig).toBe(14);
    expect(IX_TAG.PushOraclePrice).toBe(17);
    expect(IX_TAG.ResolveMarket).toBe(19);
    expect(IX_TAG.WithdrawInsurance).toBe(20);
    expect(IX_TAG.AdminForceCloseAccount).toBe(21);
    expect(IX_TAG.WithdrawInsuranceLimited).toBe(23);
    expect(IX_TAG.UpdateAuthority).toBe(32);
  });

  test('retired tags absent', () => {
    expect('SetRiskThreshold' in IX_TAG).toBe(false);
    expect('UpdateAdmin' in IX_TAG).toBe(false);
    expect('SetMaintenanceFee' in IX_TAG).toBe(false);
    expect('SetOracleAuthority' in IX_TAG).toBe(false);
    expect('SetInsuranceWithdrawPolicy' in IX_TAG).toBe(false);
  });
});

describe('instruction encoders', () => {
  test('encodeInitUser', () => {
    const data = encodeInitUser({ feePayment: '1000000' });
    expect(data.length).toBe(9);
    expect(data[0]).toBe(IX_TAG.InitUser);
    expect([...data.subarray(1, 9)]).toEqual([64, 66, 15, 0, 0, 0, 0, 0]);
  });

  test('encodeDepositCollateral', () => {
    const data = encodeDepositCollateral({ userIdx: 5, amount: '1000000' });
    expect(data.length).toBe(11);
    expect(data[0]).toBe(IX_TAG.DepositCollateral);
    expect([...data.subarray(1, 3)]).toEqual([5, 0]);
    expect([...data.subarray(3, 11)]).toEqual([64, 66, 15, 0, 0, 0, 0, 0]);
  });

  test('encodeWithdrawCollateral', () => {
    const data = encodeWithdrawCollateral({ userIdx: 10, amount: '500000' });
    expect(data.length).toBe(11);
    expect(data[0]).toBe(IX_TAG.WithdrawCollateral);
    expect([...data.subarray(1, 3)]).toEqual([10, 0]);
  });

  test('encodeKeeperCrank', () => {
    const data = encodeKeeperCrank({ callerIdx: 1 });
    expect(data.length).toBe(4);
    expect(data[0]).toBe(IX_TAG.KeeperCrank);
    expect([...data.subarray(1, 3)]).toEqual([1, 0]);
    expect(data[3]).toBe(1);
  });

  test('encodeTradeNoCpi', () => {
    const data = encodeTradeNoCpi({ lpIdx: 0, userIdx: 1, size: '1000000' });
    expect(data.length).toBe(21);
    expect(data[0]).toBe(IX_TAG.TradeNoCpi);
    expect([...data.subarray(1, 3)]).toEqual([0, 0]);
    expect([...data.subarray(3, 5)]).toEqual([1, 0]);
  });

  test('encodeTradeNoCpi negative size', () => {
    const data = encodeTradeNoCpi({ lpIdx: 0, userIdx: 1, size: '-1000000' });
    expect(data.length).toBe(21);
    expect([...data.subarray(5, 21)]).toEqual([
      192, 189, 240, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
    ]);
  });

  test('encodeTradeCpi', () => {
    const data = encodeTradeCpi({ lpIdx: 2, userIdx: 3, size: '-500' });
    expect(data.length).toBe(29);
    expect(data[0]).toBe(IX_TAG.TradeCpi);
    expect([...data.subarray(1, 3)]).toEqual([2, 0]);
    expect([...data.subarray(3, 5)]).toEqual([3, 0]);
    expect([...data.subarray(21, 29)]).toEqual([0, 0, 0, 0, 0, 0, 0, 0]);
  });

  test('encodeTradeCpi with limit price', () => {
    const data = encodeTradeCpi({
      lpIdx: 0,
      userIdx: 0,
      size: '100',
      limitPriceE6: '150000000',
    });
    expect(data.length).toBe(29);
    // 150_000_000 = 0x08F0D180 LE → [128, 209, 240, 8, 0, 0, 0, 0]
    expect([...data.subarray(21, 29)]).toEqual([128, 209, 240, 8, 0, 0, 0, 0]);
  });

  test('encodeLiquidateAtOracle', () => {
    const data = encodeLiquidateAtOracle({ targetIdx: 42 });
    expect(data.length).toBe(3);
    expect(data[0]).toBe(IX_TAG.LiquidateAtOracle);
    expect([...data.subarray(1, 3)]).toEqual([42, 0]);
  });

  test('encodeCloseAccount', () => {
    const data = encodeCloseAccount({ userIdx: 100 });
    expect(data.length).toBe(3);
    expect(data[0]).toBe(IX_TAG.CloseAccount);
    expect([...data.subarray(1, 3)]).toEqual([100, 0]);
  });

  test('encodeTopUpInsurance', () => {
    const data = encodeTopUpInsurance({ amount: '5000000' });
    expect(data.length).toBe(9);
    expect(data[0]).toBe(IX_TAG.TopUpInsurance);
  });

  test('encodeInitLP', () => {
    const matcherProg = PublicKey.unique();
    const matcherCtx = PublicKey.unique();
    const data = encodeInitLP({
      matcherProgram: matcherProg,
      matcherContext: matcherCtx,
      feePayment: '1000000',
    });
    expect(data.length).toBe(73);
    expect(data[0]).toBe(IX_TAG.InitLP);
  });

  test('encodeAdminForceCloseAccount', () => {
    const data = encodeAdminForceCloseAccount({ userIdx: 7 });
    expect(data.length).toBe(3);
    expect(data[0]).toBe(IX_TAG.AdminForceCloseAccount);
    expect([...data.subarray(1, 3)]).toEqual([7, 0]);
  });

  test('encodeWithdrawInsuranceLimited', () => {
    const data = encodeWithdrawInsuranceLimited({ amount: '5000' });
    expect(data.length).toBe(9);
    expect(data[0]).toBe(IX_TAG.WithdrawInsuranceLimited);
  });

  test('encodeUpdateAuthority', () => {
    const newKey = new PublicKey('11111111111111111111111111111111');
    const data = encodeUpdateAuthority({ kind: AUTHORITY_KIND.ADMIN, newPubkey: newKey });
    expect(data.length).toBe(34);
    expect(data[0]).toBe(IX_TAG.UpdateAuthority);
    expect(data[1]).toBe(AUTHORITY_KIND.ADMIN);
    expect(data.subarray(2, 34).equals(Buffer.from(newKey.toBytes()))).toBe(true);
  });

  test('encodeUpdateAuthority rejects invalid kind', () => {
    // kind=3 was retired in v12.20; runtime guard must throw
    expect(() => encodeUpdateAuthority({ kind: 3, newPubkey: PublicKey.default })).toThrow();
  });

  test('encodeUpdateAdmin back-compat shim', () => {
    const newAdmin = new PublicKey('11111111111111111111111111111111');
    const shimBytes = encodeUpdateAdmin({ newAdmin });
    const directBytes = encodeUpdateAuthority({
      kind: AUTHORITY_KIND.ADMIN,
      newPubkey: newAdmin,
    });
    expect(shimBytes.length).toBe(34);
    expect(shimBytes[0]).toBe(IX_TAG.UpdateAuthority);
    expect(shimBytes.equals(directBytes)).toBe(true);
  });
});
