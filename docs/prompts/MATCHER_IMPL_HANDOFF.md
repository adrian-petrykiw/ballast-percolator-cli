# Matcher Implementation — Handoff Document

**Source session:** 2026-05-02 (matcher PR design review, post-spike)
**Audience:** the next Claude Code session that picks up Phase 0 Step 0.4 implementation
**Pair this with:** `docs/prompts/MATCHER_IMPL_KICKOFF.md` (the kickoff prompt)
**Supersedes (in part):** `docs/prompts/MATCHER_PR_HANDOFF.md` — that doc was the pre-design briefing. Several decisions in §5 of that doc are reframed by the **D1 finding** below. Read this doc first, then refer back to the original for live devnet state (§2), wallet state (§3), and findings unrelated to the matcher CPI (§4.1, §4.2).

This document is the post-design state of the world. Read it before any other doc — it captures the architectural pivot that the original handoff didn't anticipate.

---

## 1. The D1 finding (architectural pivot — read first)

**Symptom.** PRD §4.7 FM-1 ("matcher MUST reject any trade where the counterparty wallet is not in the allowlist") is unenforceable inside the matcher.

**Cause.** Upstream Percolator's Match-CPI contract passes the matcher exactly two accounts (`lp_pda`, `ctx`) and 67 bytes of instruction data. The 67 bytes are: `tag(1) + req_id(8) + lp_idx(2) + lp_account_id(8) + oracle_price_e6(8) + req_size(16) + reserved(24)`. **The counterparty pubkey is in neither**, and the 24-byte reserved area can't fit a 32-byte pubkey (parser rejects non-zero reserved bytes anyway). Confirmed by upstream source: see [`/tmp/claude-501/pmatch/lib.rs:46-216`](https://raw.githubusercontent.com/aeyakovenko/percolator-match/master/src/lib.rs) and [`/tmp/claude-501/pmatch/vamm.rs:432-500`](https://raw.githubusercontent.com/aeyakovenko/percolator-match/master/src/vamm.rs). Solana CPI semantics also exclude any out-of-band "extra accounts" mechanism — the matcher only sees what's in `ix.accounts`.

**Pivot decision.**

1. **Hedge trades route through `trade-nocpi`** (not `trade-cpi` as the PRD originally said). `trade-nocpi` requires both `user` and `lp` to sign — the LP wallet's signature is the on-chain access-control gate. Ballast operates an off-chain LP signing service that runs the allowlist/KYC check before signing. Non-allowlisted callers can't get the LP signature → tx never gets fully signed → can't be submitted.

2. **Matcher closes the trade-cpi side door.** By default, the Ballast matcher's Match handler returns `FLAG_VALID | FLAG_REJECTED` with `exec_size = 0` (zero-fill) regardless of caller. This prevents the trade-cpi standing delegation from being used as a leak around the LP signature gate. Note: "always-reject" doesn't discriminate by counterparty (the matcher can't see them); it's a blanket no-op on trade-cpi calls.

3. **Forward-compat flag in matcher init.** The matcher_ctx carries a 1-byte `allow_trade_cpi_fills` flag. `0` = always-reject (Phase 0 default). `1` = passive-fill at `oracle_price ± (trading_fee_bps + base_spread_bps)`. Phase 1 freight (Hyperp slab — see §6) will need passive-fill mode + Architecture #2 (signing-service-as-only-caller); the same matcher binary serves both phases.

4. **On-chain allowlist becomes audit metadata.** The `allowlist_count` + 4 pubkey slots in matcher_ctx are the LP signing service's public commitment to whom it will sign for. The matcher reads them but cannot act on them (FM-1 is enforced off-chain).

5. **PRD edits in this PR.** §4.7 FM-1 reframed (LP-signature gating + audit metadata), §4.9 Step 0.6 line 449 (trade-cpi → trade-nocpi for hedge), §4.9 Step 0.6 line 463-464 (Wallet C demonstration via signature withholding), SC-0.7 rescoped.

## 2. The two-layer fee model (relevant for monetization)

Confirmed via upstream code, captured in `~/.claude/.../memory/cargobill-monetization-roadmap.md`:

- **Slab-level** `tradingFeeBps` (= 10 bps in our config, immutable from InitMarket): protocol fee on every trade. Destination empirically TBD in Step 0.6 (likely LP).
- **Matcher-level** `trading_fee_bps` + `base_spread_bps`: LP's revenue knobs in trade-cpi only. `trading_fee_bps` is a capital transfer (user → LP); `base_spread_bps` is a price differential captured by LP but consumes the engine's per-slot price-move budget.

Phase 0 trade-nocpi exec price = `slab.lastEffectivePriceE6` (engine's dt-capped staircase, NOT raw oracle). LP earns slab `tradingFeeBps` (modest revenue, by design — bilateral POC).

## 3. Hyperp finding (Phase 1 implication, captured for awareness)

`indexFeedId == 0` ⇒ Hyperp slab ⇒ `trade-nocpi` disabled by protocol (`HyperpTradeNoCpiDisabled`, custom error 27 — see [`src/abi/errors.ts:120`](../../src/abi/errors.ts#L120)). Hyperp markets force trade-cpi.

Phase 1 freight (FBX) will likely be Hyperp (no on-chain freight oracle). Implication: Phase 0's trade-nocpi pivot does not generalize to Phase 1. Phase 1 needs **passive-fill mode** + **Architecture #2** (Ballast backend is the only entity submitting trade-cpi calls; frontend KYC = the gate). Same matcher binary, different `allow_trade_cpi_fills` flag.

This is captured in detail in the monetization-roadmap memory; **not a blocker for this PR**.

## 4. Matcher byte-level specs

### 4.1 matcher_ctx layout (320 bytes total — extends PRD §4.7 with the new flag at byte 77)

```
Offset  Size  Field                     Description
0       64    [matcher return scratch]  Written on each Match call (per Percolator ABI v2)
64      8     magic                     BALLAST_MAGIC; u64 LE = 0x0054_5341_4C4C_4142
                                        (LE bytes spell b"BALLAST\0")
72      4     version                   = 1
76      1     kind                      = 0 (Passive); only kind defined for now
77      1     allow_trade_cpi_fills     0 = always-reject default; 1 = passive-fill (Phase 1)
78      2     _pad0
80      32    lp_pda                    LP PDA for signature verification (FM-3)
112     4     trading_fee_bps           u32 LE; LP capital-transfer fee (passive-fill mode)
116     4     base_spread_bps           u32 LE; LP price-differential spread (passive-fill mode)
120     1     allowlist_count           u8; 0..=4
121     3     _pad1
124     32    allowlist_0               Public commitment to allowed counterparty (audit only)
156     32    allowlist_1
188     32    allowlist_2
220     32    allowlist_3
252     68    _reserved
```

**Initialized state:** all bytes zero except the fields above. The first 64 bytes (return scratch) get overwritten on every Match call; for Init we leave them zero.

### 4.2 Init instruction (tag=2) — Ballast custom 144-byte payload

```
Offset  Size  Field                     Notes
0       1     tag                       = 2
1       1     kind                      = 0
2       1     allow_trade_cpi_fills     Per call; 0 default
3       1     _pad
4       4     trading_fee_bps           u32 LE
8       4     base_spread_bps           u32 LE
12      1     allowlist_count           0..=4
13      3     _pad
16      32    allowlist_0               Pubkey or all-zero if unused
48      32    allowlist_1
80      32    allowlist_2
112     32    allowlist_3
Total:  144 bytes
```

**Validations:**
- `allow_trade_cpi_fills` must be 0 or 1.
- `allowlist_count` must be ≤ 4.
- For `i` in `[allowlist_count, 4)`, the corresponding 32-byte slot must be all zero (defensive).
- Standard upstream validations: matcher_ctx must be owned by this program; matcher_ctx must be writable; size ≥ 320; not already initialized (magic check).
- After Init: write magic + version + kind + flag + lp_pda (from `accounts[0].key`) + fees + spread + allowlist.

### 4.3 Match instruction (tag=0) — fixed 67-byte payload (upstream Percolator-Match ABI)

```
Offset  Size  Field                     Notes
0       1     tag                       = 0
1       8     req_id                    u64 LE; echo back in return
9       2     lp_idx                    u16 LE; LP slot index (informational here)
11      8     lp_account_id             u64 LE; echo back
19      8     oracle_price_e6           u64 LE; engine's dt-capped staircase price
27      16    req_size                  i128 LE; positive = user buys, negative = user sells
43      24    reserved                  Must be all zero (upstream parser enforces)
Total:  67 bytes
```

### 4.4 MatcherReturn (64 bytes, written to `ctx[0..64]` per ABI v2)

```
Offset  Size  Field                     Notes
0       4     abi_version               = 2
4       4     flags                     bitfield: FLAG_VALID=1, FLAG_PARTIAL_OK=2, FLAG_REJECTED=4
8       8     exec_price_e6             u64 LE
16      16    exec_size                 i128 LE; signed fill size
32      8     req_id                    u64 LE; echo from call
40      8     lp_account_id             u64 LE; echo from call
48      8     oracle_price_e6           u64 LE; echo from call
56      8     reserved                  = 0
```

**Always-reject return** (default Phase 0 behavior):
- `abi_version = 2`
- `flags = FLAG_VALID | FLAG_REJECTED` = 5
- `exec_price_e6 = 1` (NOT 0 — match upstream's `MatcherReturn::rejected()` to avoid downstream divide-by-zero)
- `exec_size = 0`
- echo req_id, lp_account_id, oracle_price_e6 from the call

**Passive-fill return** (Phase 1 / when flag=1):
- `abi_version = 2`
- `flags = FLAG_VALID` = 1
- `exec_price_e6 = oracle_price_e6 * (BPS_DENOM ± total_bps) / BPS_DENOM`
  where `total_bps = base_spread_bps + trading_fee_bps`, sign matches `is_buy = req_size > 0` (buy → +, sell → −)
- `exec_size = req_size` (no max-fill cap in our layout — we removed those upstream fields per PRD §4.7)
- echo req_id, lp_account_id, oracle_price_e6
- Use `checked_mul` / `checked_add` everywhere; return `ProgramError::ArithmeticOverflow` on overflow.

### 4.5 Match handler — required checks (in order)

1. Verify `ctx_account.owner == program_id` → else `IncorrectProgramId`.
2. Verify `ctx_account.data_len() >= 320` → else `AccountDataTooSmall`.
3. Read magic at ctx[64..72]; verify == `BALLAST_MAGIC` → else `UninitializedAccount`.
4. **FM-2: verify `lp_pda.is_signer == true`** → else `MissingRequiredSignature`. The percolator program signs the LP PDA via `invoke_signed` during the CPI; our matcher must enforce this is set.
5. **FM-3: read stored lp_pda from ctx[80..112]; verify it equals `accounts[0].key`** → else `InvalidAccountData`. Prevents context substitution attacks.
6. Parse the 67-byte MatcherCall; verify reserved bytes [43..67] are zero → else `InvalidInstructionData`.
7. Read `allow_trade_cpi_fills` byte from ctx[77]:
   - `0`: write always-reject return; done.
   - `1`: compute passive-fill return; write it; done.
   - other: return `InvalidAccountData` (defensive).

### 4.6 Init handler — required checks (in order)

1. Verify `ctx_account.owner == program_id` → else `IncorrectProgramId`.
2. Verify `ctx_account.is_writable` → else `InvalidAccountData`.
3. Verify `ctx_account.data_len() >= 320` → else `AccountDataTooSmall`.
4. Verify ctx is NOT already initialized (magic byte at ctx[64..72] must be zero) → else `AccountAlreadyInitialized`.
5. Parse the 144-byte init payload; validate kind=0, flag in {0,1}, allowlist_count ≤ 4, unused slots zero.
6. Write magic, version, kind, flag, lp_pda (from `accounts[0].key`), fees, spread, allowlist.

## 5. File-by-file build order

This is the proposed order for incremental commits. Each commit reviewable before moving to the next.

1. **`programs/ballast-matcher/src/lib.rs`** — full implementation per §4 above. Use `solana-program ~1.18` (already in Cargo.toml). Prefer hand-rolled byte slicing matching the upstream pattern; do NOT add `bytemuck` or other deps. Include `entrypoint!` gated by `#[cfg(not(feature = "no-entrypoint"))]`. Constants module for offsets, magic, version, abi_version, flags.
2. **`programs/ballast-matcher/tests/integration.rs`** — `solana-program-test` cases:
   - Init writes magic + lp_pda + fields correctly.
   - Init rejects re-init.
   - Match rejects when `lp_pda.is_signer == false` → `MissingRequiredSignature`.
   - Match rejects when stored lp_pda doesn't match → `InvalidAccountData`.
   - Match in always-reject mode writes correct return (FLAG_VALID|FLAG_REJECTED, size=0, price=1).
   - Match in passive-fill mode rounds correctly (buy = oracle + total_bps, sell = oracle − total_bps).
3. **Build:** `cd programs/ballast-matcher && cargo build-sbf` — verify produces `target/deploy/ballast_matcher.so`.
4. **`scripts/ballast/utils/crank.ts`** — `prependCrankIfStale(payerPk, slabPk, oraclePk): TransactionInstruction`. Always-prepends a `KeeperCrank` (callerIdx=65535, no candidates). One small file.
5. **`scripts/ballast/utils/wsol.ts`** — extract `ensureWrappedSol` from setup-ballast-sol-market.ts so all scripts share it. Refactor only.
6. **`scripts/ballast/utils/matcher.ts`** — TypeScript mirror of the Rust init parser: `encodeBallastInit({allowTradeCpiFills, tradingFeeBps, baseSpreadBps, allowlist})` → 144-byte Buffer. Plus `MATCHER_CTX_SIZE = 320` and `BALLAST_MAGIC` constants.
7. **`scripts/ballast/setup-ballast-matcher.ts`** — atomic LP-init tx:
   - Idempotency: if `cfg.slabs.solUsd.matcherCtx` is set AND parsed slab shows LP idx 0 with non-zero capital, exit no-op.
   - Wraps SOL via `ensureWrappedSol`.
   - Tx (single, atomic): `[ComputeBudget(400_000), KeeperCrank, SystemProgram.createAccount(matcherCtx, 320 bytes, owner=matcherProgramId), BallastMatcher.Init, Percolator.InitLP(matcherProgram, matcherCtx, feePayment=120_000_000n), Percolator.DepositCollateral(idx=0, 10_000_000_000n)]`.
   - Signers: `[lpPayer, matcherCtxKeypair]`.
   - Dry-run via `simulateTransaction` first; abort if it errors.
   - On success: persist `matcherProgramId`, `slabs.solUsd.matcherCtx`, `slabs.solUsd.lpIndex = 0` to `config/ballast-config.json`. Write per-run manifest to `config/ballast-matcher-deploy.json` (gitignored).
8. **`scripts/ballast/setup-ballast-participants.ts`** — hedger init+deposit:
   - Idempotency: if `cfg.slabs.solUsd.hedgerIndex` is set AND that slab idx shows kind=User, exit no-op.
   - Tx: `[ComputeBudget, KeeperCrank, Percolator.InitUser(feePayment=60_000_000n), SystemProgram.transfer + SyncNative (wrap 5 SOL), Percolator.DepositCollateral(5_000_000_000n)]`.
   - Signer: hedger keypair.
   - Detect index via parseUsedIndices diff; persist `slabs.solUsd.hedgerIndex`.
9. **`scripts/ballast/setup-ballast-sol-market.ts` retrofit** — wrap the `topUpInsurance` tx-build to take a `crankIx` arg and prepend it. Apply for the `--insurance-only` path (the deploy path's warmup crank already covers the deploy → topup window).
10. **PRD diff (`docs/prd.md`):**
    - §4.7: reframe FM-1. Suggested new wording: "FM-1: The on-chain allowlist in matcher_ctx is the LP signing service's public commitment to allowed counterparties. Allowlist enforcement happens off-chain: the LP signing service refuses to co-sign trade-nocpi transactions from non-allowlisted counterparties. The matcher's `allow_trade_cpi_fills = 0` default closes the trade-cpi side channel (zero-fill on all trade-cpi calls), preventing bypass of the LP-signature gate. The matcher itself enforces FM-2 (LP PDA signer) and FM-3 (LP PDA matches stored). The Match-CPI ABI does not pass counterparty pubkey to the matcher (upstream Percolator design); per-counterparty enforcement at trade-cpi time is therefore not implementable inside the matcher and is moved to the off-chain signing service. Future production scaling may use Architecture #2 (signing service is the sole trade-cpi caller) to combine LP fee/spread revenue with allowlist enforcement; see `docs/monetization-roadmap.md` (post-matcher-PR)."
    - §4.9 Step 0.6 line 449: change "via `trade-cpi`" to "via `trade-nocpi` (LP wallet co-signs each trade)".
    - §4.9 Step 0.6 lines 463-464: change Wallet C attempt to: "Attempt trade-nocpi from Wallet C against the Ballast LP. The hedger constructs the tx; Wallet C cannot obtain LP-wallet signature (Wallet C is not in the allowlist); transaction is unsubmittable. Confirm by attempting the request through the LP signing service and capturing the refusal."
    - SC-0.7 rescope: similar reframing.
    - Add `allow_trade_cpi_fills` field to the §4.7 layout table at byte 77.
11. **`docs/reports/phase-0-step-0.4-report.md`** — written *after* the user runs the deployments and reports tx sigs back. Captures: matcher program ID + deploy sig, atomic LP-init sig, matcher_ctx pubkey, hedger init+deposit sig + index, `--insurance-only` re-run sig (proves OracleStale fix), allowlist contents, SC-0.2 sign-off, partial SC-0.7.

## 6. Live devnet state (verify before assuming)

Slab and program IDs are unchanged from the original handoff:

```
Network:               devnet
Percolator program:    2SSnp35m7FQ7cRLNKGdW5UzjYFF6RBUNq7d3m5mqNByp  (v12.21+)
SOL/USD slab:          HftDjBvRArFoSnGcvxwSCN7rok5PYZtK2shckWBE5inY
  vault PDA:           J1ohDnxM63A2Qkjbp5w7T9WYCVPNwTUBWjHcrVoEVufd
  vault ATA (wSOL):    86tt7usBm2xDuevoiV65Z4TWudG4otujYRxZU64DbyFh
  oracle (Pyth Pull):  7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE  (sponsored shard 0)
  feed-id:             ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d
  market type:         INVERTED, non-Hyperp (Pyth feed)
  insurance:           5 SOL
  positions:           none
  market_mode:         Live
```

Confirm with `npx tsx scripts/dump-market.ts --slab HftDjBvRArFoSnGcvxwSCN7rok5PYZtK2shckWBE5inY` before running scripts.

## 7. Wallet funding required before scripts run

(Original handoff §3, restated.)

| Role | Pubkey | File path | Required SOL |
|---|---|---|---|
| LP / admin / deployer | `J9iCXvvxdjeDGUGUCEBPVbuhNDLYv4UEv4hDN5UHe56y` | `~/.config/ballast/ballast-lp.json` | ~12 SOL (10 LP collateral + ~0.5 matcher rent + tx fees + `solana program deploy` ~3 SOL recoverable). The 5 stranded wSOL in ATA `EAtykJ5jm93Wn1EXRRLjTK5G5bzhPhk9twF9UpXUvxrE` is reusable. |
| Hedger | (run `solana-keygen pubkey ~/.config/ballast/ballast-hedger.json`) | `~/.config/ballast/ballast-hedger.json` | ~5.5 SOL (5 user collateral + 0.06 init fee + ATA rent + tx fees) |
| Oracle authority | (run `solana-keygen pubkey ~/.config/ballast/ballast-oracle-authority.json`) | `~/.config/ballast/ballast-oracle-authority.json` | ~0.1 SOL (only used for Step 0.7 controlled scenarios) |

User has access to friend devnet SOL (100+ SOL); funding is not a constraint.

## 8. Open follow-ups deferred from this PR

- **Slab `tradingFeeBps` destination** (LP / insurance / fee pool) — confirmed empirically during PRD Step 0.6 hedge trade by inspecting pre/post LP capital + insurance balance + vault delta.
- **trade-cpi double-charge?** Whether trade-cpi charges slab `tradingFeeBps` on top of matcher fees, or one supersedes the other. Same Step 0.6 confirmation.
- **Phase 1 freight slab type** (Hyperp vs non-Hyperp+admin-pushed) — design question deferred to Phase 1 planning. See `~/.claude/.../memory/cargobill-monetization-roadmap.md`.
- **Monetization doc** (`docs/monetization-roadmap.md`) — lands in the PR after this matcher PR. Memory note already saved.
- **PRD §4.3 ASCII diagram, §4.9 Step 0.1 Node version, §6/§7 Chainlink mentions, README phase numbering, CLAUDE.md numbering edit** — all already classified as deferred cosmetics in the original `MATCHER_PR_HANDOFF.md` §6/§7. Still deferred.
- **Keeper bot (PRD Step 0.5)** — separate PR after this one.
- **Step 0.6 trade execution** — separate PR after Step 0.5.

## 9. Source files to read at session start

In order, with what to extract from each:

1. **`docs/prompts/MATCHER_IMPL_HANDOFF.md`** (this doc) — full briefing.
2. **`CLAUDE.md`** — workflow rule (Claude does NOT run commits/pushes/gh/on-chain writes — emits bash blocks for the user).
3. **`docs/prompts/MATCHER_PR_HANDOFF.md`** §1, §2, §3, §4 — what's still load-bearing from the prior session (live state, wallet state, OracleStale gate, Pyth VerificationLevel layout).
4. **`docs/prd.md`** §4.7 (matcher spec — note: FM-1 reframe is part of this PR's diff), §4.9 Step 0.6 (note: trade-cpi → trade-nocpi diff is part of this PR), SC-0.7 (reframing).
5. **`programs/ballast-matcher/Cargo.toml`** + **`programs/ballast-matcher/src/lib.rs`** — current scaffold (no logic).
6. **Upstream `percolator-match` source** (download to `$TMPDIR/pmatch/` if not present):
   - `lib.rs` — MatcherCall layout, MatcherReturn struct, process_instruction dispatch, security invariants.
   - `passive_lp_matcher.rs` — pricing math reference (we don't reuse this code; we re-implement passive pricing inline).
   - `vamm.rs` — `MatcherCtx::is_initialized`, `process_init`, `process_call`, `compute_passive_execution`. The Ballast layout shadows several upstream fields; do NOT import the upstream struct, write our own.
7. **`src/abi/instructions.ts`** — `encodeInitLP`, `encodeDepositCollateral`, `encodeKeeperCrank`, `encodeInitUser`, `encodeTopUpInsurance` byte layouts.
8. **`src/abi/accounts.ts`** — `ACCOUNTS_INIT_LP`, `ACCOUNTS_DEPOSIT_COLLATERAL`, `ACCOUNTS_KEEPER_CRANK`, `ACCOUNTS_INIT_USER`, `ACCOUNTS_TOPUP_INSURANCE`, `WELL_KNOWN.{clock,tokenProgram}` and the `buildAccountMetas` helper.
9. **`scripts/setup-devnet-market.ts`** — upstream reference for the atomic-LP-init pattern (matcher-ctx-create + matcher-init + InitLP in one tx). Read but DO NOT modify (upstream file).
10. **`scripts/ballast/setup-ballast-sol-market.ts`** — the existing Ballast deploy script; pattern for config I/O, idempotency, wrapped-SOL handling.
11. **Saved memories at `~/.claude/projects/-Users-ap-Documents-GitHub-ballast-percolator-cli/memory/`:**
    - `percolator-oracle-stale-gate.md` — the OracleStale 0x6 gate (already in MEMORY.md auto-loaded).
    - `cargobill-monetization-roadmap.md` — two-layer fee model + Phase 0/1 implications + production architectures.

## 10. Workflow rule (from CLAUDE.md, restated)

Claude does NOT execute:
- `git commit`, `git push`, `git tag`, history-rewriting ops
- `gh pr create | merge | close`, `gh issue *`, `gh release create`
- `solana program deploy | close | upgrade`, any signer-bearing on-chain write
- `cargo publish`, `npm publish`

For each, emit a fenced bash block at the end of the response (heredoc'd commit message included), the user runs in the VS Code terminal at the repo root.

Free for Claude (read-only):
- `git status | diff | log | show | branch`, `gh pr view | list | checks`, `solana account | program show | balance`, `getAccountInfo`-style RPC queries
- `git add <specific paths>` and `git restore --staged <paths>` to prepare a clean index for the user (never `git add -A`)
- `cargo build-sbf` (sandbox-safe)

## 11. Acceptance gates (rerun before declaring this PR done)

- [ ] `cargo build-sbf` succeeds; `programs/ballast-matcher/target/deploy/ballast_matcher.so` produced.
- [ ] `cargo test -p ballast-matcher --features no-entrypoint` passes the integration cases listed in §5 step 2.
- [ ] `solana program deploy` succeeds (USER runs); program ID captured into `config/ballast-config.json`.
- [ ] `setup-ballast-matcher.ts` runs successfully (USER runs); LP appears at idx 0 in `dump-state.ts` output with non-zero capital.
- [ ] `setup-ballast-participants.ts` runs successfully (USER runs); hedger user appears at the next idx with 5 SOL collateral.
- [ ] Re-running either script is a no-op (idempotency guard).
- [ ] `setup-ballast-sol-market.ts --insurance-only` succeeds (the OracleStale fix).
- [ ] PRD diff lands (§4.7 FM-1, §4.9 Step 0.6, SC-0.7).
- [ ] Validation report at `docs/reports/phase-0-step-0.4-report.md`.
- [ ] Wallet C demonstration (signing-service refusal) — may be deferred to Step 0.6 if Wallet C tooling isn't ready yet; note in the report.

## 12. Things to NOT do in this PR

- Do NOT modify upstream files (anything in `scripts/` not under `scripts/ballast/`, anything in `src/`, `tests/` not under `tests/ballast/`, `test/`).
- Do NOT burn authorities (Phase 0 Step 0.7+, after validation).
- Do NOT implement the keeper bot (separate PR).
- Do NOT execute trades (separate PR).
- Do NOT deploy to mainnet under any circumstances.
- Do NOT commit keypair files; never log private keys.
- Do NOT use `git add -A` or `git add .`; stage specific paths only.
