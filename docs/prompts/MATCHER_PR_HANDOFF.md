# Matcher PR — Handoff Document

**Source session:** 2026-05-01 (Phase 0 Step 0.2 deployment)
**Audience:** the next Claude Code (or human) chat that picks up Phase 0 Step 0.4
**Pair this with:** `docs/prompts/MATCHER_PR_KICKOFF.md` (the prompt to paste into the new chat)

This document is a complete state-of-the-world snapshot from the prior session. Read it before any other doc — it captures decisions and findings that the source files don't make obvious on inspection.

---

## 1. What just shipped (master, 2026-05-01)

**PR #3 (squash-merged):** `feat(phase-0): deploy SOL/USD slab on Pyth Pull (step 0.2)`. Closes **SC-0.1** (Market Deployment).

Files added:

- `scripts/ballast/setup-ballast-sol-market.ts` — idempotent slab deploy + insurance topup (5 SOL). Refuses to redeploy unless `--force`; supports `--insurance-only` (currently bug-prone — see §4 OracleStale below).
- `scripts/ballast/utils/pyth.ts` — hand-rolled `PriceUpdateV2` decoder that handles the variable-size `VerificationLevel` borsh enum (Full=1 byte, Partial=2 bytes; sponsored push feeds use Full).
- `docs/phase-0-step-numbering.md` — reconciles the CLAUDE.md slab-level "Step 0.X" vs PRD §4.9 action-level "Step 0.X" name overlap. **Convention going forward:** PRD §4.9 numbering is canonical.

Plus `.gitignore` was tightened twice — once for keypair filenames, once for per-deploy manifests + dump-market snapshots.

## 2. Live devnet state (verify before assuming)

```
Network:               devnet (https://api.devnet.solana.com)
Percolator program:    2SSnp35m7FQ7cRLNKGdW5UzjYFF6RBUNq7d3m5mqNByp  (v12.21+)
Upstream matcher:      4HcGCsyjAqnFua5ccuXyt8KRRQzKFbGTJkVChpS7Yfzy  (passive — DO NOT use for Ballast LP)

SOL/USD slab:          HftDjBvRArFoSnGcvxwSCN7rok5PYZtK2shckWBE5inY
  vault PDA:           J1ohDnxM63A2Qkjbp5w7T9WYCVPNwTUBWjHcrVoEVufd
  vault ATA (wSOL):    86tt7usBm2xDuevoiV65Z4TWudG4otujYRxZU64DbyFh
  oracle (Pyth Pull):  7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE  (sponsored shard 0)
  feed-id:             ef0d8b6fda2ceba41da15d4095d1da392a0d2f8ed0c6c7bc0f4cfac8c280b56d
  market type:         INVERTED (LONG = profit if SOL drops)
  risk:                mm 5%, im 10%, fee 10bps
  staleness:           max_staleness_secs=120, conf_filter_bps=200
  envelope:            permissionlessResolveStaleSlots=100 (~40s @ 400ms/slot)
  insurance:           5 SOL (Live)
  authorities:         admin / insurance / hyperp_mark all live (NOT burned — Mode-2 needs them)
  positions:           none
  market_mode:         Live
  solvent:             true
  last_oracle_price:   11954 (engine-space, after invert; = 1e12 / SOL_e6 at SOL ≈ $83.65)
```

Confirm with `npx tsx scripts/dump-market.ts --slab HftDjBvRArFoSnGcvxwSCN7rok5PYZtK2shckWBE5inY` before relying on these values. They are accurate as of 2026-05-01 but the slab is on a live devnet.

## 3. Wallet state

| Role | Pubkey | File path | SOL balance (post-deploy) | Pre-matcher-PR action |
|---|---|---|---|---|
| LP / admin / deployer | `J9iCXvvxdjeDGUGUCEBPVbuhNDLYv4UEv4hDN5UHe56y` | `~/.config/ballast/ballast-lp.json` | ~0.58 SOL liquid + 5 wSOL stranded in ATA `EAtykJ5jm93Wn1EXRRLjTK5G5bzhPhk9twF9UpXUvxrE` | Top up to ~10–12 SOL (10 SOL LP collateral per PRD §4.4 + ~0.5 SOL for matcher program rent + tx fees). The 5 stranded wSOL is reusable — `ensureWrappedSol` in the setup script detects existing balance. |
| Hedger | (run `solana-keygen pubkey ~/.config/ballast/ballast-hedger.json`) | `~/.config/ballast/ballast-hedger.json` | 0 | Top up to ~5.5 SOL (5 SOL user collateral + fees + ATA rent) |
| Oracle authority | (run `solana-keygen pubkey ~/.config/ballast/ballast-oracle-authority.json`) | `~/.config/ballast/ballast-oracle-authority.json` | 0 | No rush — only used for Step 0.7 controlled scenarios. ~0.1 SOL is plenty for tx fees |

User has access to friend devnet SOL (100+ SOL), so funding is not a constraint. Per CLAUDE.md security rule, keypairs are file-based (Phantom can't sign headlessly) and devnet-only.

## 4. Critical findings worth carrying forward

### 4.1 OracleStale gate (THE most important thing in this document)

**Symptom:** `TopUpInsurance`, `InitLP`, `DepositCollateral`, `Trade*` revert with `custom program error: 0x6` after only ~4k CU consumed. Pyth feed shows fresh (age <60s, conf <50bps).

**Cause:** Percolator's wrapper checks `current_slot - engine.lastGoodOracleSlot < permissionlessResolveStaleSlots` (= 100 slots ≈ 40 s in our config) on engine-state-changing ops. `lastGoodOracleSlot` is updated by `KeeperCrank`, not by the Pyth feed itself. Without a continuous keeper bot running, that slot ages out almost immediately and any one-shot script that touches engine state reverts.

**Why upstream `setup-devnet-market.ts` doesn't trip:** deploy → wrap → LP → deposit → topup all happen in one continuous run with seconds between steps. The slot stays fresh for the whole sequence.

**Why our `setup-ballast-sol-market.ts --insurance-only` trips:** by definition it runs minutes-to-days after deploy without the keeper bot in between.

**Fix for matcher PR:** introduce a `prependCrankIfStale()` helper that adds a `KeeperCrank` instruction at the head of any transaction containing an engine-state-changing op. Account ordering for `KeeperCrank` is in `src/abi/accounts.ts:ACCOUNTS_KEEPER_CRANK` (4 accounts: caller, slab, clock, oracle). Use `callerIdx: 65535` (sentinel for "permissionless / not-an-account") and `candidates: []`. Apply to: `InitLP`, `DepositCollateral`, `TopUpInsurance`, `WithdrawCollateral`, `TradeNoCpi`, `TradeCpi`, retroactively to the existing `--insurance-only` path in `setup-ballast-sol-market.ts`.

Captured in project memory at `~/.claude/projects/-Users-ap-Documents-GitHub-ballast-percolator-cli/memory/percolator-oracle-stale-gate.md`.

### 4.2 Pyth `PriceUpdateV2` `VerificationLevel` is variable-size

Borsh enum: `Partial { numSignatures: u8 }` = 2 bytes (tag + u8), `Full` = 1 byte (tag only). Sponsored push feeds use `Full`, so `price_message` starts at byte 41, not 42. The parser in `scripts/ballast/utils/pyth.ts` reads the variant tag at byte 40 and branches. An earlier draft assumed fixed-2 → garbage prices. If ever asked to extend the parser (e.g. for TWAP accounts, or a new feed format), branch on the tag — don't hard-code offsets.

### 4.3 Authorities NOT burned at Phase 0 init

Upstream `scripts/setup-devnet-market.ts` burns admin / insurance / insurance-operator at the end of init. Ballast deliberately does NOT — controlled scenarios in PRD §4.5 Mode 2 (admin-pushed oracle for SC-0.4 / SC-0.9) need them live. Burns happen *after* validation, before any production-style bilateral hand-off. Don't burn during matcher PR; that's Phase 0 Step 0.7+.

### 4.4 Slab rent is ~10.6 SOL on devnet (not the 5.3 originally estimated)

Recoverable on `close-slab`. Locked, not consumed. Affects budgeting only, not correctness.

### 4.5 pnpm 10 build-script gates

`bigint-buffer` falls back to pure JS, prints `bigint: Failed to load bindings, pure JS will be used`. Correct, just slower. Documented in PRD §4.9 Step 0.8 entry. Approve via `pnpm approve-builds` before stress tests; not now. Decline `bufferutil`, `utf-8-validate`, `protobufjs`, `esbuild` per PRD note.

## 5. The matcher PR — concrete scope

**PRD references:** §4.7 (Allowlist Matcher Specification — FM-1..FM-6), §4.9 Step 0.4, §4.9 Step 0.3 (participant init bundles in here because LP-init binds the matcher).

**Closes:** SC-0.2 (Participant Initialization) once participant init lands, plus partial SC-0.7 (unauthorized trade rejection requires the allowlist to actually exist, but the trade attempt itself is Step 0.6).

### 5.1 Files to create / change

**Rust (matcher program):**

- `programs/ballast-matcher/src/lib.rs` — implement per PRD §4.7 FM-1..FM-6:
  - Verify LP PDA is a signer (CRITICAL: per upstream README, "If your matcher accepts unsigned calls, attackers can bypass LP authorization and steal funds"). The `percolator` program signs the LP PDA via `invoke_signed` during CPI; the matcher must `assert!(lp_pda_account.is_signer)`.
  - Verify the LP PDA matches the stored PDA in the matcher context (FM-3).
  - Allowlist check: counterparty wallet (passed via context or instruction data — confirm against upstream `percolator-match`) must be in the on-chain allowlist (FM-1).
  - Passive pricing mode: oracle ± fixed spread (FM-6). vAMM out of scope.
  - Context layout per PRD §4.7 — 320 bytes total, Ballast magic at offset 64, `kind=0` Passive, `lp_pda` at 80, fees/spread at 112-120, `allowlist_count` at 120, four allowlist slots at 124..252.

- Optional: `programs/ballast-matcher/tests/lib.rs` — `solana-program-test` cases for unauthorized counterparty rejection, missing LP signer, etc. (PRD NFR-SEC-2: "matcher must be reviewed by at least one additional engineer before devnet deployment" — solana-program-test acts as belt-and-suspenders alongside human review).

**Build + deploy:**

- `cd programs/ballast-matcher && cargo build-sbf` — produces `target/deploy/ballast_matcher.so`.
- `solana program deploy target/deploy/ballast_matcher.so --url devnet --keypair ~/.config/ballast/ballast-lp.json` — emit as a bash block; user runs (rule per CLAUDE.md). Capture program ID into `config/ballast-config.json` (`matcherProgramId`).

**TypeScript (setup scripts):**

- `scripts/ballast/setup-ballast-matcher.ts` — atomic LP-init transaction per the upstream README's race-condition guidance:
  ```
  Tx instructions:
    [0] ComputeBudget setComputeUnitLimit
    [1] (optional) prependCrankIfStale → KeeperCrank
    [2] System.createAccount   (matcher_ctx, 320 bytes, owner = ballast-matcher program)
    [3] BallastMatcher.init    (writes magic, lp_pda, fees, spread, allowlist of [hedger, lp])
    [4] Percolator.InitLP      (LP idx 0, matcher_program = ballast-matcher, matcher_context = matcher_ctx, fee_payment = 0.12 SOL ≥ new_account_fee)
    [5] Percolator.DepositCollateral  (LP idx 0, 10 SOL — per PRD §4.4)
  Signers: payer (LP wallet), matcher_ctx keypair
  ```
  Uses LP wallet as both Percolator admin (already set at init-market time) and matcher-init signer.

- `scripts/ballast/setup-ballast-participants.ts` — InitUser + DepositCollateral for hedger:
  ```
  Tx instructions:
    [0] ComputeBudget setComputeUnitLimit
    [1] prependCrankIfStale → KeeperCrank
    [2] Percolator.InitUser    (hedger wallet, fee_payment = 0.06 SOL ≥ new_account_fee)
    [3] System.transfer + SyncNative   (wrap 5 SOL into hedger's wSOL ATA)
    [4] Percolator.DepositCollateral   (hedger idx, 5 SOL — per PRD §4.4)
  Signers: hedger keypair
  ```
  Update `config/ballast-config.json` `slabs.solUsd.hedgerIndex` and `lpIndex`.

- `scripts/ballast/utils/crank.ts` — small `prependCrankIfStale(conn, slabPk, oracle, payer)` helper. Either always-prepend (simpler), or check `engine.lastGoodOracleSlot` vs `current_slot` first (smaller txs). Recommend always-prepend — the gas overhead is negligible and the conditional-check round-trip costs an RPC call.

- Retrofit `setup-ballast-sol-market.ts` — `--insurance-only` path needs `prependCrankIfStale` applied to the TopUpInsurance tx. Same change in the deploy path is unnecessary (warmup crank in step 4 keeps the slot fresh for step 7), but defense-in-depth doesn't hurt.

**Config:**

- Update `config/ballast-config.example.json` — add `matcherProgramId` placeholder is already there. Document the required structure of the matcher context account in a comment (or in the matcher program's README).

**Docs:**

- `docs/reports/phase-0-step-0.4-report.md` — per CLAUDE.md "Validation Reports" rule. Captures: matcher program ID + deploy sig, LP-init sig, deposit sig, allowlist contents, unauthorized-trade attempt result (will need a Wallet C in the test plan to demonstrate FM-1 enforcement).

### 5.2 Open design questions to resolve in-session

1. **Allowlist storage size.** PRD §4.7 FM-4 says "up to 4 wallet pubkeys (128 bytes)". For the POC bilateral case we only need 2. Pad to 4 anyway per the spec; revisit dynamic sizing if/when production scaling is in scope.

2. **`InitLP` `fee_payment` amount.** Must be ≥ `new_account_fee` (0.06 SOL configured at init-market). Use 0.06 SOL exactly, or buffer to 0.12 SOL like upstream? Match upstream's 0.12 — gives headroom and matches the convention.

3. **Where does the counterparty wallet pubkey arrive in the matcher CPI?** Read upstream `percolator-match` reference repo (linked from upstream README) before coding the matcher. The CPI account ordering and data layout is the contract; deviating from it breaks the CPI.

4. **Should the matcher PR also add `prependCrankIfStale` to `setup-ballast-sol-market.ts --insurance-only`?** Yes, bundle. Same code path, clean fix.

5. **Atomic LP creation** — the upstream `setup-devnet-market.ts` does matcher-ctx-create + matcher-init + LP-init in a single tx. We should too. Confirm transaction size fits within Solana's 1232-byte limit (will likely be tight with all the account metas + ix data). If it busts, split into two txs but document the race-condition window.

### 5.3 Acceptance gates

- [ ] `cargo build-sbf` succeeds
- [ ] `solana program deploy` succeeds; program ID captured into `config/ballast-config.json`
- [ ] `setup-ballast-matcher.ts` runs; LP appears at idx 0 in `dump-state.ts` output with non-zero collateral
- [ ] `setup-ballast-participants.ts` runs; hedger user appears at idx 1 with 5 SOL collateral
- [ ] Re-running either script is a no-op (idempotency guard, like `setup-ballast-sol-market.ts`)
- [ ] Unauthorized wallet (Wallet C, not in allowlist) attempting `init-user` + `deposit` succeeds; `trade-cpi` against the Ballast LP fails with the matcher's allowlist error (document the exact error code)
- [ ] `--insurance-only` re-run on `setup-ballast-sol-market.ts` succeeds (the OracleStale fix)
- [ ] Validation report at `docs/reports/phase-0-step-0.4-report.md`

## 6. Doc-update PR landing alongside this handoff

A `docs:` PR (separate from the matcher PR, lands first) updates:

- `docs/prd.md` §4.4 status note (Pyth supersedes Chainlink for SOL/USD)
- `docs/prd.md` §4.5 status note (Mode 1 = Pyth Pull, not Chainlink)
- `docs/prd.md` §4.9 Step 0.2 (script renamed `setup-ballast-sol-market.ts`; Status note added)
- `docs/prd.md` §4.9 Step 0.3 entry (OracleStale gate caveat)
- `docs/prd.md` §4.9 Step 0.6, Step 0.7 (Pyth, not Chainlink)
- `docs/pyth-oracle-compatibility.md` §7 (mark items 2 + 3 resolved) and §8 (operational findings, including OracleStale gate and `VerificationLevel` variable-size note)

Cosmetic / non-blocking, deferred to a later cleanup pass:

- PRD §4.3 ASCII architecture diagram (still says "Chainlink SOL/USD Oracle")
- PRD §4.9 Step 0.1 entry (says Node.js 18+, should be 20+ to match CLAUDE.md upstream)
- PRD §6 risk register, §7 dependency matrix (cosmetic Chainlink mentions)
- README.md (the Phase 0 step numbering uses CLAUDE.md's slab-level scheme — both schemes coexist per `docs/phase-0-step-numbering.md`; non-urgent CLAUDE.md edit can collapse it to PRD §4.9 numbering when convenient)

## 7. Open follow-ups (not blocking matcher PR)

- The 5 stranded wSOL in LP's ATA `EAtykJ5jm93Wn1EXRRLjTK5G5bzhPhk9twF9UpXUvxrE` will be consumed by the LP's 10-SOL deposit in matcher PR. No unwrap needed.
- PRD §4.9 Step 0.5 keeper bot (`scripts/ballast/ballast-crank-bot.ts`): can land in a separate PR or bundle into matcher PR. Recommend separate — keeper-bot is independent and short, and decoupling avoids matcher-PR scope creep.
- CLAUDE.md numbering edit: per `docs/phase-0-step-numbering.md`, the cleanest path is to drop the slab-level "Step 0.X" labels from CLAUDE.md and keep PRD §4.9 numbering canonical. One-paragraph edit.

## 8. Workflow rule reminder

Per CLAUDE.md, Claude does NOT execute `git commit | push | tag`, `gh pr create | merge | close`, `solana program deploy | upgrade`, on-chain writes signed by Ballast keypairs, etc. Emit fenced bash blocks; user runs them. The matcher PR will involve at minimum:

- `cargo build-sbf` (Claude can run — sandbox-safe)
- `solana program deploy` (USER runs — on-chain write)
- `npx tsx scripts/ballast/setup-ballast-matcher.ts` (USER runs — on-chain write)
- `npx tsx scripts/ballast/setup-ballast-participants.ts` (USER runs — on-chain write)
- `git commit / push / gh pr create` (USER runs)

Read-only verification (`gh pr view`, `solana account`, `getAccountInfo`, `git status`, `git log`) is fine for Claude.
