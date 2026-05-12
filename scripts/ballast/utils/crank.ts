/**
 * Ballast — permissionless KeeperCrank instruction builder.
 *
 * Purpose
 * -------
 * One-shot Ballast scripts that touch engine state (TopUpInsurance, InitLP,
 * InitUser, DepositCollateral, WithdrawCollateral, Trade*) must prepend a
 * KeeperCrank to the same transaction. The wrapper-level freshness check
 * compares `current_slot - lastGoodOracleSlot` against
 * `permissionlessResolveStaleSlots` (=100 slots ≈ 40s in our config); without
 * a continuous keeper bot running, that window expires almost immediately
 * and the gated op reverts with `OracleStale (0x6)`. Cranking in the same
 * atomic transaction refreshes `lastGoodOracleSlot` before the gated op
 * fires.
 *
 * The helper is unconditional by design — prepending an extra crank when
 * state is already fresh is a ~5k-CU no-op, whereas querying the slab for
 * slot age before deciding adds an RPC round-trip and a failure mode. The
 * "only-when-stale" intent lives in the caller's contract, not in runtime
 * branching.
 *
 * Prerequisites
 * -------------
 * - Caller has the Percolator program id (NOT the Ballast matcher's),
 *   the target slab pubkey, the slab's oracle pubkey, and a payer that
 *   will sign the enclosing transaction.
 * - Caller is responsible for the ComputeBudget instruction; this helper
 *   only emits the KeeperCrank ix itself.
 *
 * PRD reference
 * -------------
 * Phase 0 Step 0.4 — allowlist matcher. Memory note
 * `percolator-oracle-stale-gate.md` is the load-bearing rationale.
 */

import { PublicKey, TransactionInstruction } from "@solana/web3.js";

import { encodeKeeperCrank } from "../../../src/abi/instructions.js";
import {
  ACCOUNTS_KEEPER_CRANK,
  WELL_KNOWN,
  buildAccountMetas,
} from "../../../src/abi/accounts.js";

export interface BuildKeeperCrankIxArgs {
  /**
   * Percolator program id — NOT the Ballast matcher's. KeeperCrank is a
   * Percolator instruction; routing it to the matcher program would fail
   * on-chain with IncorrectProgramId.
   */
  percolatorProgramId: PublicKey;
  /** The slab being cranked. */
  slab: PublicKey;
  /**
   * The slab's oracle account (Pyth Pull / Chainlink / admin-authority).
   * Type is auto-detected on-chain by account owner; no validation needed
   * client-side.
   */
  oracle: PublicKey;
  /**
   * Payer that signs the enclosing tx. Recorded as `callerIdx = 65535`
   * (u16::MAX = "permissionless caller, not a slab participant"). Pays
   * the tx fee; not credited or charged any in-protocol amount.
   */
  payer: PublicKey;
}

/**
 * Build a permissionless KeeperCrank instruction with no liquidation
 * candidates. Synchronous and deterministic — no RPC, no side effects.
 *
 * Account ordering follows `ACCOUNTS_KEEPER_CRANK`:
 * `[caller(signer), slab(writable), clock, oracle]`.
 */
export function buildKeeperCrankIx(
  args: BuildKeeperCrankIxArgs,
): TransactionInstruction {
  return new TransactionInstruction({
    programId: args.percolatorProgramId,
    keys: buildAccountMetas(ACCOUNTS_KEEPER_CRANK, [
      args.payer,
      args.slab,
      WELL_KNOWN.clock,
      args.oracle,
    ]),
    data: encodeKeeperCrank({ callerIdx: 65535, candidates: [] }),
  });
}
