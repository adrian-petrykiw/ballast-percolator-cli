import { PublicKey } from '@solana/web3.js';
import { deriveVaultAuthority, deriveLpPda } from '../src/solana/pda.js';
import { describe, expect, test } from 'vitest';

describe('deriveVaultAuthority', () => {
  test('returns a valid off-curve PDA with bump', () => {
    const programId = PublicKey.unique();
    const slab = PublicKey.unique();
    const [pda, bump] = deriveVaultAuthority(programId, slab);

    expect(pda).toBeInstanceOf(PublicKey);
    expect(typeof bump).toBe('number');
    expect(bump >= 0 && bump <= 255).toBe(true);
    expect(PublicKey.isOnCurve(pda.toBytes())).toBe(false);
  });

  test('is deterministic', () => {
    const programId = PublicKey.unique();
    const slab = PublicKey.unique();
    const [pda, bump] = deriveVaultAuthority(programId, slab);
    const [pda2, bump2] = deriveVaultAuthority(programId, slab);

    expect(pda.equals(pda2)).toBe(true);
    expect(bump).toBe(bump2);
  });
});

describe('deriveLpPda', () => {
  test('returns valid off-curve PDAs for each index', () => {
    const programId = PublicKey.unique();
    const slab = PublicKey.unique();
    const [pda0] = deriveLpPda(programId, slab, 0);
    const [pda1] = deriveLpPda(programId, slab, 1);
    const [pda100] = deriveLpPda(programId, slab, 100);

    expect(pda0).toBeInstanceOf(PublicKey);
    expect(pda1).toBeInstanceOf(PublicKey);
    expect(pda100).toBeInstanceOf(PublicKey);
    expect(PublicKey.isOnCurve(pda0.toBytes())).toBe(false);
  });

  test('different indices produce different PDAs', () => {
    const programId = PublicKey.unique();
    const slab = PublicKey.unique();
    const [pda0] = deriveLpPda(programId, slab, 0);
    const [pda1] = deriveLpPda(programId, slab, 1);
    const [pda100] = deriveLpPda(programId, slab, 100);

    expect(pda0.equals(pda1)).toBe(false);
    expect(pda0.equals(pda100)).toBe(false);
    expect(pda1.equals(pda100)).toBe(false);
  });

  test('is deterministic', () => {
    const programId = PublicKey.unique();
    const slab = PublicKey.unique();
    const [pda0] = deriveLpPda(programId, slab, 0);
    const [pda0b] = deriveLpPda(programId, slab, 0);

    expect(pda0.equals(pda0b)).toBe(true);
  });
});

test('vault PDA and LP PDA are distinct', () => {
  const programId = PublicKey.unique();
  const slab = PublicKey.unique();
  const [vaultPda] = deriveVaultAuthority(programId, slab);
  const [lpPda] = deriveLpPda(programId, slab, 0);

  expect(vaultPda.equals(lpPda)).toBe(false);
});
