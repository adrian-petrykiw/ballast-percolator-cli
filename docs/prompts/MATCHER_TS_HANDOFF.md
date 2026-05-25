# Matcher TS-side Handoff — Steps 4–9

**Source session:** 2026-05-12 (lib.rs reshape + integration tests landed; TS side handed off cleanly)
**Audience:** the next Claude Code session that picks up steps 4–9 of the matcher PR build order
**Pair this with:** `docs/prompts/MATCHER_TS_KICKOFF.md` (the kickoff prompt)
**Supersedes (in part):** `MATCHER_IMPL_HANDOFF.md` §5 step 6 — the 200-byte encoder spec is in §4.3 of this doc. Steps 4, 5, 7, 8, 9 are also re-specified here with idempotency contracts and post-reshape parameters. Everything else (workflow rule, D1 finding, FM-2/FM-3, decisions locked, devnet state) is unchanged.

---

## 1. Where we are in the build order

| Step | Status | Owner | Commit |
|---|---|---|---|
| 0 (precursor) — track Cargo.lock + handoff for layout reshape | **landed** | — | `9a28035` |
| 1 — `programs/ballast-matcher/src/lib.rs` (reshape) | **landed** | — | `65af9e6` |
| 2 — `programs/ballast-matcher/tests/integration.rs` + dev-deps | **landed** | — | `ffb7864` |
| 3 — `cargo build-sbf` verification | **landed** (artifact: `target/deploy/ballast_matcher.so`, 49,144 bytes) | — | — |
| 4 — `scripts/ballast/utils/crank.ts` | not started | next session | — |
| 5 — `scripts/ballast/utils/wsol.ts` (extract from setup-ballast-sol-market.ts) | not started | next session | — |
| 6 — `scripts/ballast/utils/matcher.ts` + tests | not started | next session | — |
| 7 — `scripts/ballast/setup-ballast-matcher.ts` | not started | next session | — |
| 8 — `scripts/ballast/setup-ballast-participants.ts` | not started | next session | — |
| 9 — `setup-ballast-sol-market.ts --insurance-only` retrofit | not started | next session | — |
| 10 — PRD diff (§4.7 FM-1, §4.9 Step 0.6, SC-0.7) | not started | future session | — |
| 11 — validation report (post-deploy) | not started | future session | — |

**This session's scope is steps 4–9.** Step 10 lands after this PR; step 11 lands after the user runs the deploys.

## 2. Decisions locked from prior sessions (DO NOT relitigate)

All from `MATCHER_LAYOUT_RESHAPE_HANDOFF.md §2` plus the dev-dep wave from session 2026-05-12:

| # | Decision |
|---|---|
| 1 | Init authorization: no extra signer. `accounts[0]` (lp_pda) is read but not required to sign at init. |
| 2 | `max_total_bps` ceiling: 9000. Validated at init AND defense-in-depth at execution. |
| 3 | Match length: `>= 67` (forward-compat). Reserved-zero at [43..67] is the tampering boundary. |
| 4 | Init alignment pads: strict zero. Bytes 3 and 69..72 must be zero in the 200-byte init payload. |
| 5 | `solana-program = "~1.18"` retained. Migration to 2.0 documented in `docs/solana-program-version-strategy.md`. |
| 6 | **Cargo.lock pin list expanded.** Wave 1 (original): blake3, indexmap, proc-macro-crate, jobserver, borsh, rayon, rayon-core. Wave 2 (added 2026-05-12): time, idna_adapter, base64ct, rpassword, async-compression, enum-iterator-derive, tempfile, ahash@0.8.x. See memory note `matcher-cargo-lock-pinning.md`. Do not run `cargo update` broadly. |
| 7 | Single `lib.rs`, no submodules. |
| 8 | No Anchor / bytemuck / borsh-derive on the wire. Hand-rolled byte slicing. |
| 9 | `BALLAST_MAGIC = 0x0054_5341_4C4C_4142` (LE bytes spell `b"BALLAST\0"`). |
| 10 | ctx layout: packed 384 bytes. Allowlist replaces upstream's `_reserved` tail. Not byte-identical to upstream (would need 512). |
| 11 | Pricing math ported verbatim from upstream `percolator-match` vamm.rs:516–683 (clone to `$TMPDIR/pmatch/` if needed for reference; not required for steps 4–9). |
| 12 | Inventory sign convention: `inventory_base -= exec_size` (taker buys ⇒ LP sells ⇒ LP inventory decreases). |
| 13 | Partial fills return `FLAG_VALID`; zero-fills return `FLAG_VALID | FLAG_PARTIAL_OK` with `exec_price = oracle`; REJECTED (allow_fills=NEVER) returns `FLAG_VALID | FLAG_REJECTED` with `exec_price = 1`. |

## 3. Live state of the matcher (post-step-2)

```
Branch:                   feat/phase-0-allowlist-matcher
HEAD commit:              ffb7864 test(matcher): add solana-program-test integration suite
Rust crate:               programs/ballast-matcher (single lib.rs, 1668 lines)
.so artifact:             programs/ballast-matcher/target/deploy/ballast_matcher.so (49,144 bytes)
Tests:                    62 inline + 10 integration = 72 passing
solana-program:           1.18.26 (pinned via "~1.18")
Cargo.lock:               tracked, ~15 transitive pins (load-bearing)
On-chain deployment:      NOT YET DEPLOYED. Program ID is set when user runs `solana program deploy`.
```

Re-verify with:
```bash
cd programs/ballast-matcher
CARGO_HOME="$TMPDIR/cargo-home" cargo test --features no-entrypoint
CARGO_HOME="$TMPDIR/cargo-home" cargo-build-sbf
```
Both should be green and the .so should be exactly 49,144 bytes.

## 4. File-by-file specs (steps 4–9)

### 4.1 Step 4 — `scripts/ballast/utils/crank.ts`

**Purpose.** Build a `KeeperCrank` instruction that callers prepend to one-shot scripts. Defends against the OracleStale 0x6 gate (memory note `percolator-oracle-stale-gate.md`): TopUpInsurance / Init* / Trade* revert with OracleStale without a recent KeeperCrank.

**Public surface.**
```typescript
export function buildKeeperCrankIx(args: {
  programId: PublicKey;   // percolator program id
  slab: PublicKey;
  oracle: PublicKey;
  payer: PublicKey;
}): TransactionInstruction;
```

**Body.** Wraps `encodeKeeperCrank({ callerIdx: 65535, candidateCount: 0 })` from `src/abi/instructions.ts` with `buildAccountMetas(ACCOUNTS_KEEPER_CRANK, [slab, oracle, payer])` (and clock — check `ACCOUNTS_KEEPER_CRANK` for the exact order). 30–50 LOC including header doc comment.

**Why caller-prepended, not auto-detected staleness:** prepending an extra KeeperCrank when state is already fresh is a no-op (~5k CU); querying RPC for slot age before deciding adds latency and a failure mode. Always-prepend is the simpler, more robust contract.

**Tests:** none (the instruction is fully determined by inputs; integration is via the scripts that use it).

### 4.2 Step 5 — `scripts/ballast/utils/wsol.ts`

**Purpose.** Extract `ensureWrappedSol` from `setup-ballast-sol-market.ts:139-165` into a shared utility so step 7 + step 8 + future scripts share the same wrap logic.

**Public surface.**
```typescript
export async function ensureWrappedSol(
  conn: Connection,
  payer: Keypair,
  amount: bigint, // lamports the ATA must hold AFTER this returns
): Promise<PublicKey>; // wSOL ATA address
```

**Behavior.** Idempotent. Reads current wSOL ATA balance; if `have >= amount` returns without submitting a tx. Otherwise submits a single tx: `[ComputeBudget(30k), SystemProgram.transfer(amount - have lamports → ata), Token.SyncNative]`. Returns the ATA address either way.

**Refactor only.** Move the function verbatim, update `setup-ballast-sol-market.ts` to import from the new location. **Do not change behavior** — this isn't a rewrite. Diff should be near-zero LOC delta in the source file, +30 LOC in the new file. **`setup-ballast-sol-market.ts` is upstream-adjacent (it's our Ballast script, not an upstream file) but tightly used; preserve its behavior exactly.**

**Tests:** none directly. Step 7 + 8 will exercise it on devnet.

### 4.3 Step 6 — `scripts/ballast/utils/matcher.ts` (CRITICAL)

**Purpose.** TypeScript mirror of the Rust `parse_init_payload` from `programs/ballast-matcher/src/lib.rs`. Produces the exact 200-byte buffer the matcher expects. **Byte-for-byte correctness is critical** — if the encoder produces the wrong bytes, the matcher rejects Init.

**Constants to export:**
```typescript
export const MATCHER_CTX_SIZE = 384;
export const INIT_LEN = 200;
export const MATCH_LEN = 67;
export const RETURN_LEN = 64;
export const ALLOWLIST_MAX = 4;
export const BALLAST_MAGIC = 0x0054_5341_4C4C_4142n;   // bigint
export const BALLAST_VERSION = 1;
export const KIND_PASSIVE = 0;
export const KIND_VAMM = 1;
export const ALLOW_FILLS_NEVER = 0;
export const ALLOW_FILLS_FILL = 1;
export const MAX_TOTAL_BPS_CEILING = 9000;
export const TAG_INIT = 2;
export const TAG_MATCH = 0;
// Return ABI v2
export const ABI_VERSION = 2;
export const FLAG_VALID = 1;
export const FLAG_PARTIAL_OK = 2;
export const FLAG_REJECTED = 4;
```

**Init payload offsets (must match Rust):**
```
0       1     tag (= TAG_INIT)
1       1     kind
2       1     allow_trade_cpi_fills
3       1     _pad0 (= 0)
4       4     trading_fee_bps                  u32 LE
8       4     base_spread_bps                  u32 LE
12      4     max_total_bps                    u32 LE
16      4     impact_k_bps                     u32 LE
20      16    liquidity_notional_e6            u128 LE   ← bigint
36      16    max_fill_abs                     u128 LE   ← bigint
52      16    max_inventory_abs                u128 LE   ← bigint
68      1     allowlist_count                  u8
69      3     _pad1 (= 0)
72      128   allowlist (4 × 32, unused all-zero)
200           (total)
```

**Encoder signature:**
```typescript
export interface BallastInitArgs {
  kind: 0 | 1;
  allowTradeCpiFills: 0 | 1;
  tradingFeeBps: number;            // u32, 0..=9000
  baseSpreadBps: number;            // u32, 0..=9000
  maxTotalBps: number;              // u32, 0..=9000
  impactKBps: number;               // u32; must be 0 if kind === KIND_PASSIVE
  liquidityNotionalE6: bigint;      // u128; must be 0 if passive, > 0 if vAMM
  maxFillAbs: bigint;               // u128
  maxInventoryAbs: bigint;          // u128 (0 = uncapped)
  allowlist: PublicKey[];           // 0..=4 entries, no duplicates
}

export function encodeBallastInit(args: BallastInitArgs): Buffer;
```

**Validation (mirrors Rust `parse_init_payload` exactly — throw on any violation):**
1. `args.allowlist.length` ∈ `[0, 4]`.
2. `kind` ∈ `{0, 1}`, `allowTradeCpiFills` ∈ `{0, 1}`.
3. `tradingFeeBps`, `baseSpreadBps`, `maxTotalBps`, `impactKBps` ∈ `[0, 2^32 − 1]`.
4. `maxTotalBps ≤ 9000`.
5. `tradingFeeBps + baseSpreadBps ≤ maxTotalBps`.
6. If `kind === KIND_PASSIVE`: `impactKBps === 0 && liquidityNotionalE6 === 0n`.
7. If `kind === KIND_VAMM`: `liquidityNotionalE6 > 0n`.
8. u128 fields: `0n <= x < 2^128`.

**Helpers:** a `u128LE(v: bigint): Buffer` writes 16 little-endian bytes; an `i128LE` for signed reads (not needed by encoder but useful for state decoding).

**State decoder (also in this file):**
```typescript
export interface MatcherState {
  magic: bigint;                    // u64
  version: number;                  // u32
  kind: number;                     // u8
  allowTradeCpiFills: number;       // u8
  lpPda: PublicKey;                 // 32 bytes
  tradingFeeBps: number;            // u32
  baseSpreadBps: number;            // u32
  maxTotalBps: number;              // u32
  impactKBps: number;               // u32
  liquidityNotionalE6: bigint;      // u128
  maxFillAbs: bigint;               // u128
  inventoryBase: bigint;            // i128 (signed!)
  lastOraclePriceE6: bigint;        // u64
  lastExecPriceE6: bigint;          // u64
  maxInventoryAbs: bigint;          // u128
  allowlistCount: number;
  allowlist: PublicKey[];           // length === allowlistCount
}

export function decodeMatcherState(ctxData: Buffer): MatcherState;
export interface MatchReturn {
  abiVersion: number;
  flags: number;
  execPriceE6: bigint;
  execSize: bigint;                 // i128
  reqId: bigint;
  lpAccountId: bigint;
  oraclePriceE6: bigint;
}
export function decodeMatchReturn(ctxData: Buffer): MatchReturn;  // reads ctxData[0..64]
```

The decoder needs to handle signed i128 — JS `BigInt` from a 16-byte LE buffer needs explicit two's complement handling for the inventory_base + exec_size fields. Use `bigint`s throughout, never `number`.

**Tests:** `tests/ballast/matcher-encoder.test.ts`. Required cases:
- `encode_minimal_zero_args_produces_200_byte_buffer_with_tag` — args all zero/empty, kind=PASSIVE, allowFills=NEVER. Bytes 0..200 except byte 0 (= 2) are zero.
- `encode_passive_with_fees_and_max_total` — assert byte ranges for fee/spread/max_total.
- `encode_vamm_requires_liquidity` — kind=VAMM, liquidity=0 → throws.
- `encode_passive_rejects_nonzero_impact_k` — throws.
- `encode_passive_rejects_nonzero_liquidity` — throws.
- `encode_rejects_max_total_above_9000` — throws.
- `encode_rejects_fee_plus_spread_above_max_total` — throws.
- `encode_with_full_allowlist` — 4 keys, bytes 72..200 hold them.
- `encode_rejects_5_allowlist_entries` — throws.
- `encode_buffer_size_is_exactly_200` — for any valid args.
- `decode_round_trip` — encode a MatcherState (manually build a 384-byte buffer with known fields), decode, assert all fields match.
- `decode_inventory_base_signed` — write -1000 as i128 LE, decode, assert `-1000n`.

Run via: `npx tsx tests/ballast/matcher-encoder.test.ts`. Follow upstream's test convention (no framework, raw `assert` + console logs, see `test/` directory for pattern).

**Cross-validation against Rust:** for one or two hand-computed cases, you can verify the encoder output matches what `parse_init_payload` accepts by comparing against expected byte arrays. The Rust inline tests in `lib.rs` (e.g. `init_total_bps_at_cap_accepted`) use the same byte patterns we expect.

### 4.4 Step 7 — `scripts/ballast/setup-ballast-matcher.ts` (HIGH-STAKES)

**Purpose.** Deploy the matcher_ctx + initialize the LP slot on the SOL/USD slab, atomically, idempotently.

**Inputs (from `config/ballast-config.json` + CLI flags):**
- `percolatorProgramId` (already set)
- `matcherProgramId` — must be set in config before running. Set after the user runs `solana program deploy programs/ballast-matcher/target/deploy/ballast_matcher.so`.
- `slabs.solUsd.slab` (already set: `HftDjBvRArFoSnGcvxwSCN7rok5PYZtK2shckWBE5inY`)
- `slabs.solUsd.oracle` (already set: `7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE`)
- Wallets per CLAUDE.md security rules (LP keypair from `~/.config/ballast/ballast-lp.json`).
- `--simulate` flag: dry-run via `simulateTransaction` and print logs; do NOT send.
- `--insurance-only` flag: skip; that's step 9.

**Idempotency contract.** Run can produce one of three outcomes:
1. **Already done** (config has `matcherCtx` AND `lpIndex !== null` AND slab parse shows LP slot at `lpIndex` with non-zero capital and kind=LP) → exit no-op with a "skipping; already initialized" log.
2. **Partial state** (config or slab shows partial init — e.g. matcherCtx pubkey but no LP slot, or LP slot exists with zero capital) → **abort** with a diagnostic log telling the user to inspect / recover manually. **Do not auto-recover.**
3. **Fresh** (no matcherCtx in config, no LP slot in slab) → proceed with full setup.

**Setup sequence:**
1. Load config + connect to devnet RPC.
2. Load LP keypair. Verify `lp.publicKey` is funded (≥ 11 SOL recommended: 10 LP + ~0.5 ctx rent + tx fees + headroom).
3. Parse current slab state via `fetchSlab(conn, slabPk, percolatorProgramId)` → `parseHeader()` + `parseEngine()`. Determine: what LP slots exist? What's their capital? What's the next free LP index? Set `lpIdx = 0` per Ballast convention; abort if slot 0 is already occupied (we shouldn't be the second LP).
4. Derive the LP PDA: `[lpPda, _bump] = deriveLpPda(percolatorProgramId, slabPk, 0)` from `src/solana/pda.ts`.
5. Generate fresh keypair `matcherCtxKp` for the matcher_ctx account.
6. Run `ensureWrappedSol(conn, lp, 10_000_000_000n)` to populate the LP's wSOL ATA. Returns the ATA pubkey.
7. Compute rent: `lamports = await conn.getMinimumBalanceForRentExemption(384)`.
8. Build the atomic tx:
   ```
   [
     ComputeBudgetProgram.setComputeUnitLimit({ units: 400_000 }),
     buildKeeperCrankIx({ programId: percolatorProgramId, slab, oracle, payer: lp.publicKey }),
     SystemProgram.createAccount({
       fromPubkey: lp.publicKey,
       newAccountPubkey: matcherCtxKp.publicKey,
       lamports,
       space: 384,
       programId: matcherProgramId,
     }),
     BallastMatcher.Init ix:
       programId: matcherProgramId
       accounts: [
         { pubkey: lpPda, isSigner: false, isWritable: false },
         { pubkey: matcherCtxKp.publicKey, isSigner: false, isWritable: true },
       ]
       data: encodeBallastInit({ kind: KIND_PASSIVE, allowTradeCpiFills: ALLOW_FILLS_NEVER,
                                  tradingFeeBps: 0, baseSpreadBps: 0, maxTotalBps: 0,
                                  impactKBps: 0, liquidityNotionalE6: 0n,
                                  maxFillAbs: 0n, maxInventoryAbs: 0n, allowlist: [] }),
     Percolator.InitLP ix:
       data: encodeInitLP({ lpIdx: 0, matcherProgramId, ... })   // see src/abi/instructions.ts
       accounts: buildAccountMetas(ACCOUNTS_INIT_LP, [...])      // see src/abi/accounts.ts
       feePayment: 120_000_000n (charged from lp.publicKey via the instruction)
     Percolator.DepositCollateral ix:
       data: encodeDepositCollateral({ lpIdx: 0, amount: 10_000_000_000n })
       accounts: buildAccountMetas(ACCOUNTS_DEPOSIT_COLLATERAL, [lp.publicKey, slabPk,
                                  payerAta, vaultAta, vaultPda, tokenProgram, lpPda])
   ]
   ```
9. Signers: `[lp, matcherCtxKp]`.
10. **Simulate first** via `conn.simulateTransaction(tx)`. If `result.err` is non-null OR any log line contains `failed`, abort and print logs.
11. If `--simulate`: print "would send" summary + exit 0 without sending.
12. Else send via `sendAndConfirmTransaction(conn, tx, [lp, matcherCtxKp], { commitment: "confirmed" })`.
13. On success:
    - Update `config/ballast-config.json`: `matcherProgramId`, `slabs.solUsd.matcherCtx = matcherCtxKp.publicKey`, `slabs.solUsd.lpIndex = 0`. Write atomically (read, mutate, write JSON.stringify with 2-space indent).
    - Write per-run manifest to `config/ballast-matcher-deploy.json` (already gitignored — verify via `git check-ignore`): tx signature, block height, ctx pubkey, LP PDA, deploy timestamp.
    - Log event to `~/.cache/ballast/events.jsonl` per CLAUDE.md conventions: `event_type=USER_INIT` (or coin new type `MATCHER_INIT` — confirm with the user). Include tx sig, slab, actor, ctx pubkey, init args.

**Why `allow_trade_cpi_fills = NEVER` for Phase 0:** the matcher's job in Phase 0 is to be a no-op for the trade-cpi path so the off-chain LP signing service's gate over trade-nocpi is the only fill path. `ALLOW_FILLS_FILL` is reserved for Phase 1 freight (Hyperp slabs can't use trade-nocpi); we'll change this later by re-initializing on a new matcher_ctx account in Phase 1.

**Failure modes to handle:**
- Simulation reports `OracleStale (0x6)` → the KeeperCrank we prepended didn't update the oracle. Re-pull Pyth via `npx tsx scripts/ballast/ballast-oracle-relay.ts` and retry.
- Simulation reports `InsufficientFunds` → LP wallet underfunded.
- `getOrCreateAssociatedTokenAccount` errors → ATA rent shortfall or RPC flakiness; retry with backoff.
- `sendAndConfirmTransaction` times out → check tx sig on solscan; if landed, treat as success; if dropped, re-run (idempotency catches it).

**Logging:** mirror `setup-ballast-sol-market.ts`'s style — section headers, indented sub-steps, explicit pubkeys for everything that touches chain.

### 4.5 Step 8 — `scripts/ballast/setup-ballast-participants.ts`

**Purpose.** Initialize the hedger user slot + fund it with 5 SOL of collateral.

**Idempotency.** If config has `slabs.solUsd.hedgerIndex !== null` AND that slab idx shows kind=User with non-zero capital → no-op. Else if partial state (idx set in config but slot empty, or slot exists with zero capital) → abort with diagnostic. Else proceed.

**Setup sequence:**
1. Load config + connect. Load hedger keypair.
2. Verify funding (≥ 5.5 SOL: 5 collateral + 0.06 init fee + ATA + fees).
3. Parse slab to find next free User idx (lowest idx where slot.kind != User).
4. Build atomic tx:
   ```
   [
     ComputeBudgetProgram.setComputeUnitLimit({ units: 300_000 }),
     buildKeeperCrankIx({ ... }),
     Percolator.InitUser ix (feePayment 60_000_000n, accounts per ACCOUNTS_INIT_USER),
     SystemProgram.transfer({ from: hedger, to: hedgerWsolAta, lamports: 5_000_000_000n }),
     Token.SyncNative on hedgerWsolAta,
     Percolator.DepositCollateral ix (amount: 5_000_000_000n, lpIdx: <new_user_idx>),
   ]
   ```
   Note: `Percolator.InitUser` returns the new user's index in the slab. Detect via state diff (pre/post slab parse).
5. Signer: `[hedger]`.
6. Simulate → send → persist `slabs.solUsd.hedgerIndex` to config.

**Tests:** none directly. Devnet integration is the test.

### 4.6 Step 9 — `setup-ballast-sol-market.ts --insurance-only` retrofit

**Purpose.** Make the `--insurance-only` codepath in `setup-ballast-sol-market.ts` prepend a KeeperCrank instruction so the TopUpInsurance call doesn't trip the OracleStale gate.

**Change.** Locate the existing `--insurance-only` block (likely a top-level branch in `main()`). Use the existing `tx` builder pattern; wrap the `TopUpInsurance` ix-build with a function that takes a `crankIx` arg and prepends it. Apply for the `--insurance-only` path. The deploy path's warmup crank already covers the deploy→topup window, so don't touch the deploy branch.

**Acceptance:** `npx tsx scripts/ballast/setup-ballast-sol-market.ts --insurance-only` succeeds where it currently OracleStales.

**Diff size:** small, ~20–40 LOC.

## 5. Acceptance gates (rerun before declaring this PR-portion done)

- [ ] `tsc --noEmit` clean (or whatever the project's TS check command is — see package.json `build` / `dev`).
- [ ] `pnpm test` passes (the inline TS unit tests including new `matcher-encoder.test.ts`).
- [ ] `cargo test --features no-entrypoint` still green (sanity — TS changes don't affect Rust).
- [ ] `cargo-build-sbf` still produces a clean .so at the expected size.
- [ ] **USER RUNS (out of band):**
  - `solana program deploy programs/ballast-matcher/target/deploy/ballast_matcher.so --url devnet` → captures program ID, writes to config.
  - `npx tsx scripts/ballast/setup-ballast-matcher.ts` → LP appears at idx 0 in dump-state.ts.
  - `npx tsx scripts/ballast/setup-ballast-participants.ts` → hedger user appears at next idx.
  - Re-running either is a no-op (idempotency check).
  - `npx tsx scripts/ballast/setup-ballast-sol-market.ts --insurance-only` succeeds.

## 6. Things to NOT do in this scope

- Do NOT modify `programs/ballast-matcher/src/lib.rs` (it's locked at `ffb7864`).
- Do NOT modify `programs/ballast-matcher/Cargo.toml` or `Cargo.lock`.
- Do NOT modify upstream files in `src/`, `scripts/` (only `scripts/ballast/`), `test/`, `tests/` (only `tests/ballast/`).
- Do NOT execute trades, deploy to mainnet, or commit keypair files.
- Do NOT burn authorities (Phase 0 Step 0.7+, separate PR).
- Do NOT write the PRD diff (step 10) or validation report (step 11) — those are after the user runs the deploys.
- Do NOT add new pins to Cargo.lock unless adding new Rust deps (which this scope doesn't).
- Do NOT use `git add -A` or `git add .` — stage specific paths only.

## 7. Workflow rule reminder

Per CLAUDE.md, Claude does NOT execute:
- `git commit / push / tag`, history-rewriting ops
- `gh pr create / merge / close`, `gh issue *`
- `solana program deploy / close / upgrade`, any signer-bearing on-chain write
- `cargo publish`, `npm publish`

For each, emit a fenced bash block (heredoc'd commit message included) at the end of the response. The user runs in the VS Code terminal at the repo root.

Free for Claude (read-only): `git status | diff | log`, `gh pr view | list | checks`, `solana account | program show | balance`, `getAccountInfo`-style RPC queries, `pnpm test`, `tsc --noEmit`, `cargo check`, `cargo test`, `cargo-build-sbf`.

**Staging:** `git add <specific paths>` and `git restore --staged <paths>` are fine for preparing a clean index. Never `git add -A`.

**One commit per step.** Build order is: step 4 → step 5 → step 6 → step 7 → step 8 → step 9. Land each as its own commit before moving to the next, so the user can review incrementally.

## 8. Live devnet state (verify before assuming)

```
Network:                  devnet
Percolator program:       2SSnp35m7FQ7cRLNKGdW5UzjYFF6RBUNq7d3m5mqNByp  (v12.21+, solana-program 1.18)
Matcher program:          NOT YET DEPLOYED  — set after step 7 by the user
SOL/USD slab:             HftDjBvRArFoSnGcvxwSCN7rok5PYZtK2shckWBE5inY
  vault PDA:              J1ohDnxM63A2Qkjbp5w7T9WYCVPNwTUBWjHcrVoEVufd
  vault ATA (wSOL):       86tt7usBm2xDuevoiV65Z4TWudG4otujYRxZU64DbyFh
  oracle (Pyth Pull):     7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE  (sponsored shard 0)
  feed-id (SOL/USD):      ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d
  market type:            INVERTED, non-Hyperp (Pyth feed)
  insurance:              5 SOL
  positions:              none
  market_mode:            Live
```

Confirm with `npx tsx scripts/dump-market.ts --slab HftDjBvRArFoSnGcvxwSCN7rok5PYZtK2shckWBE5inY` before running scripts. (Note: `dump-market.ts` is upstream — read-only call, safe.)

## 9. Funding state (verify; user has friend-SOL access if more needed)

| Role | Pubkey | File path | Required | Notes |
|---|---|---|---|---|
| LP / admin / deployer | `J9iCXvvxdjeDGUGUCEBPVbuhNDLYv4UEv4hDN5UHe56y` | `~/.config/ballast/ballast-lp.json` | ~12 SOL (10 LP + ~0.5 matcher rent + tx fees + `solana program deploy` ~3 SOL recoverable) | user said funded as of 2026-05-02; verify with `solana balance --url devnet -k ~/.config/ballast/ballast-lp.json` |
| Hedger | run `solana-keygen pubkey ~/.config/ballast/ballast-hedger.json` | `~/.config/ballast/ballast-hedger.json` | ~5.5 SOL | user said funded |
| Oracle authority | run `solana-keygen pubkey ~/.config/ballast/ballast-oracle-authority.json` | `~/.config/ballast/ballast-oracle-authority.json` | ~0.1 SOL eventually | step 0.7+ |

Stranded 5 wSOL in ATA `EAtykJ5jm93Wn1EXRRLjTK5G5bzhPhk9twF9UpXUvxrE` (LP wallet) is reusable for collateral.

## 10. Source files to read at session start

In order:

1. **`docs/prompts/MATCHER_TS_HANDOFF.md`** (this doc) — full briefing.
2. **`CLAUDE.md`** — workflow rule, security rules, coding conventions, TS bigint discipline.
3. **`docs/prompts/MATCHER_LAYOUT_RESHAPE_HANDOFF.md`** §3 — the 384-byte ctx layout that the TS encoder must mirror.
4. **`programs/ballast-matcher/src/lib.rs`** — the encoder source-of-truth. Specifically the `parse_init_payload` function and the `INIT_OFF_*` constants. **Match byte-for-byte.**
5. **`scripts/ballast/setup-ballast-sol-market.ts`** — the existing Ballast deploy script. Pattern for config I/O, idempotency guards, `ensureWrappedSol` (the source to extract in step 5).
6. **`scripts/setup-devnet-market.ts`** — upstream reference for the atomic-LP-init pattern (matcher-ctx-create + matcher-init + InitLP + DepositCollateral in one tx). **Read but do NOT modify** (upstream file).
7. **`src/abi/instructions.ts`** — `encodeInitLP`, `encodeDepositCollateral`, `encodeKeeperCrank`, `encodeInitUser` byte layouts.
8. **`src/abi/accounts.ts`** — `ACCOUNTS_INIT_LP`, `ACCOUNTS_DEPOSIT_COLLATERAL`, `ACCOUNTS_KEEPER_CRANK`, `ACCOUNTS_INIT_USER` + `buildAccountMetas` helper.
9. **`src/solana/pda.ts`** — `deriveLpPda` (LP PDA = `findProgramAddressSync([b"lp", slab, lpIdx_u16_le], percolatorProgramId)`).
10. **`src/solana/slab.ts`** — `fetchSlab` + parsers for idempotency checks. The LP slot kind/capital fields are what idempotency reads.
11. **`config/ballast-config.example.json`** + **`config/ballast-config.json`** — schema + current state.
12. **Saved memories at `~/.claude/projects/-Users-ap-Documents-GitHub-ballast-percolator-cli/memory/`:**
    - `MEMORY.md` — auto-loaded.
    - `percolator-oracle-stale-gate.md` — load-bearing for the always-prepend-KeeperCrank pattern in step 4.
    - `matcher-cargo-lock-pinning.md` — not directly relevant for TS work, but read if cargo touches anything.
    - `cargobill-monetization-roadmap.md` — context only.

## 11. Open follow-ups deferred from this scope

- **Step 10 (PRD diff):** §4.7 FM-1 reframe, §4.9 Step 0.6 trade-cpi → trade-nocpi, SC-0.7 reframe. Lands after this scope.
- **Step 11 (validation report):** written after the user runs the deploys + reports tx sigs back. `docs/reports/phase-0-step-0.4-report.md`.
- **Slab `tradingFeeBps` destination** — confirmed empirically during PRD Step 0.6 hedge trade.
- **trade-cpi double-charge investigation** — Step 0.6.
- **Phase 1 freight slab type** (Hyperp vs non-Hyperp) — Phase 1 planning.
- **Monetization-roadmap doc** — separate PR after this matcher PR. Memory note saved.
- **Keeper bot (PRD Step 0.5)** — separate PR.
- **Step 0.6 trade execution** — separate PR.
- **solana-program 2.0 migration** — deferred; see `docs/solana-program-version-strategy.md`.

## 12. Notes from the lib.rs reshape session (2026-05-12) worth carrying forward

- **Cargo sandbox quirk:** the `~/.cargo/registry/cache/` is read-only in the sandbox; use `CARGO_HOME="$TMPDIR/cargo-home"` for any cargo invocation. Not relevant for TS-only work, but if you touch the matcher crate at all, you'll need this.
- **Upstream pmatch reference:** if you need to see how the original matcher's TS-side test harness looked, clone `https://github.com/aeyakovenko/percolator-match` to `$TMPDIR/pmatch/`. They don't have one (Rust-only repo) but their `vamm.rs:760-870` shows test-case patterns.
- **One subtle thing about `decodeMatcherState`:** the magic at bytes 64..72 is `BALLAST_MAGIC = 0x0054_5341_4C4C_4142n` as a u64. Read it as little-endian bytes, not as a JS number — `Number(magic)` overflows for any u64 with bit 53+ set. Use `bigint` throughout.
- **The `lpPda` field in `MatcherState`** is what FM-3 verifies against `accounts[0].key` on every Match call. The setup script writes it via the Init payload's accounts[0]. Encoder doesn't include lp_pda (it's set by the matcher from accounts[0], not from the payload).
