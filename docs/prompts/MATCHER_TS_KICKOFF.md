# Matcher TS-side Kickoff — Steps 4–9

Paste this as the first message of the next Claude Code session.

---

I'm continuing the Ballast project (devnet-only POC for compliant on-chain
bilateral derivatives on Anatoly Yakovenko's Percolator perpetual futures
protocol on Solana). Repo: `~/Documents/GitHub/ballast-percolator-cli`.

The matcher PR (PRD Phase 0 Step 0.4, branch `feat/phase-0-allowlist-matcher`)
has steps 1–3 of its build order landed:

- **Step 1 (commit `65af9e6`):** `programs/ballast-matcher/src/lib.rs` reshaped to
  the upstream-aligned 384-byte ctx layout with vAMM + inventory + audit fields.
- **Step 2 (commit `ffb7864`):** `programs/ballast-matcher/tests/integration.rs`
  added with 10 solana-program-test cases. Total now 62 inline + 10 integration
  = 72 passing tests. Dev-deps added (solana-program-test, solana-sdk, tokio).
  Cargo.lock got 8 additional pins for transitive edition2024/rustc-1.83
  drift; full list in the memory note `matcher-cargo-lock-pinning.md`.
- **Step 3:** `cargo-build-sbf` produces `target/deploy/ballast_matcher.so`
  at 49,144 bytes. Dev-deps don't bleed into the cdylib (verified).

**This session's scope is steps 4–9: the TypeScript side + on-chain setup
scripts.** Step 10 (PRD diff) and step 11 (validation report) come after.

**Read in this order before writing any code:**

1. `docs/prompts/MATCHER_TS_HANDOFF.md` — **authoritative for this session.**
   Contains: status of each step, decisions locked from prior sessions (do
   not relitigate), per-file specs for steps 4–9 (function signatures,
   idempotency contracts, failure modes), the **corrected 200-byte
   `encodeBallastInit` spec** (supersedes `MATCHER_IMPL_HANDOFF.md §5 step 6`
   which was written pre-reshape and references the obsolete 144-byte
   payload), acceptance gates, devnet state, funding state, reading list.

2. `CLAUDE.md` — workflow rule (Claude does NOT run commits, pushes, gh,
   on-chain writes — emits bash blocks for the user). Security rules.
   TS bigint discipline (`bigint` for ALL u64/u128/i128 wire values, never
   JS `number`).

3. `docs/prompts/MATCHER_LAYOUT_RESHAPE_HANDOFF.md` §3 — the 384-byte ctx
   layout the TS encoder/decoder must mirror byte-for-byte. (§4–§11 are
   historical context for step 1, which is done; skip on first pass.)

4. `programs/ballast-matcher/src/lib.rs` — the source of truth the TS
   encoder mirrors. Look at `parse_init_payload`, `INIT_OFF_*` constants,
   `MatcherState::read_from`, and the offset constants. The TS encoder must
   produce byte buffers that this parser accepts; the TS decoder must
   produce the same struct values this state would represent.

5. `scripts/ballast/setup-ballast-sol-market.ts` — existing Ballast deploy
   script. Pattern for: config I/O (read/mutate/write JSON), idempotency
   guards (check on-chain state, not just config), `ensureWrappedSol`
   (the source for step 5's extraction), section-header logging style.

6. `scripts/setup-devnet-market.ts` — **upstream**, do NOT modify.
   Reference pattern for the atomic-LP-init transaction shape (createAccount
   + matcher init + InitLP + DepositCollateral in one tx with two signers).

7. `src/abi/instructions.ts` + `src/abi/accounts.ts` — instruction
   encoders and account-meta builders used by all setup scripts. Read but
   do NOT modify (upstream-untouched).

8. `src/solana/pda.ts` — `deriveLpPda` derivation (seeds: `[b"lp", slab,
   lpIdx_u16_le]`, program: percolator program id, NOT our matcher).

9. `src/solana/slab.ts` — `fetchSlab` + state parsers for idempotency.

10. `~/.claude/projects/-Users-ap-Documents-GitHub-ballast-percolator-cli/memory/`
    — read `MEMORY.md`, then `percolator-oracle-stale-gate.md` (load-bearing
    for step 4's always-prepend-KeeperCrank pattern). `matcher-cargo-lock-pinning.md`
    and `cargobill-monetization-roadmap.md` are background only.

**Build order I want followed (one logical commit per step, each reviewable
before moving to the next):**

- **Step 4** — `scripts/ballast/utils/crank.ts`. ~50 LOC. Build a KeeperCrank
  instruction with `callerIdx=65535` and no candidates. Wraps
  `encodeKeeperCrank` + `ACCOUNTS_KEEPER_CRANK` from `src/abi/`. Always
  unconditional — the "if stale" intent is the caller's contract, not a
  runtime check.

- **Step 5** — `scripts/ballast/utils/wsol.ts`. Refactor only. Extract
  `ensureWrappedSol` from `setup-ballast-sol-market.ts:139-165` verbatim
  into a shared utility. Update the original file to import. No behavior
  change.

- **Step 6** — `scripts/ballast/utils/matcher.ts` + `tests/ballast/matcher-encoder.test.ts`.
  **The byte-correctness-critical piece.** TypeScript mirror of the Rust
  `parse_init_payload` (200-byte payload, post-reshape spec). Exports
  `encodeBallastInit`, `decodeMatcherState`, `decodeMatchReturn`, plus
  constants (`MATCHER_CTX_SIZE = 384`, `BALLAST_MAGIC` as bigint, kind/flag
  consts). Validation mirrors the Rust parser exactly. Tests cross-validate
  against the byte patterns the Rust inline tests use.

- **Step 7** — `scripts/ballast/setup-ballast-matcher.ts`. **High-stakes.**
  Atomic LP-init tx for the SOL/USD slab. Three-outcome idempotency
  (already-done / partial-abort / fresh-proceed). Simulate before send.
  `--simulate` flag. Persists matcherProgramId + matcherCtx + lpIndex to
  `config/ballast-config.json`. Writes per-run manifest to gitignored
  `config/ballast-matcher-deploy.json`. Phase 0 init uses
  `kind=KIND_PASSIVE`, `allowTradeCpiFills=ALLOW_FILLS_NEVER` (fill paths
  dormant; LP signing service is the gate via trade-nocpi). See handoff
  §4.4 for the full setup sequence and failure modes.

- **Step 8** — `scripts/ballast/setup-ballast-participants.ts`. Hedger
  init+deposit. Similar shape to step 7 but smaller: InitUser + 5 SOL
  wrap + DepositCollateral. Idempotency via slab state diff to detect the
  assigned user index.

- **Step 9** — Retrofit `scripts/ballast/setup-ballast-sol-market.ts`'s
  `--insurance-only` path to prepend a KeeperCrank. ~20–40 LOC change.
  Don't touch the deploy path (its existing warmup crank covers the window).

**For each step:**

1. Read what you need from the file list above (no need to read everything
   upfront — read step-relevant files first).
2. Show me the design (function signatures, idempotency contract, key
   decisions) before writing implementation.
3. Implement. Run tsc / pnpm test / sanity-check.
4. Emit a single commit block: `git add <specific paths>` then
   `git commit -m "..."` for me to run.

**Decisions already locked (per MATCHER_TS_HANDOFF.md §2 + the earlier
RESHAPE_HANDOFF §2 — DO NOT relitigate):**

- 200-byte init payload, 384-byte ctx, `BALLAST_MAGIC = b"BALLAST\0"`.
- `max_total_bps` ceiling 9000; fee+spread ≤ max_total.
- Init has no extra signer; auth via atomic createAccount+init.
- bigint for ALL u64/u128/i128 on the wire.
- Hand-rolled byte slicing (no Anchor/bytemuck/borsh).
- LP PDA derived as `findProgramAddressSync([b"lp", slab, idx_u16_le],
  percolatorProgramId)`. **Note: from Percolator's program id, not our
  matcher's.** The matcher stores this PDA at init and FM-3-verifies it on
  every Match.
- Phase 0 matcher init: `kind=PASSIVE`, `allow_trade_cpi_fills=NEVER`,
  all-zero fees/spreads/caps. Phase 1 vAMM enables by re-initializing on a
  new ctx with different flags.
- One commit per step. The user runs all on-chain writes (`solana program
  deploy`, the setup scripts) — Claude emits bash blocks but does not
  execute.

**What this PR is NOT:**

- Not the PRD diff (step 10 — follows after step 9).
- Not the validation report (step 11 — after the user runs the deploys).
- Not the keeper bot (PRD Step 0.5 — separate PR).
- Not trade execution (Step 0.6 — separate PR after Step 0.5).
- Not authority burns (Phase 0 Step 0.7+).
- Not the monetization-roadmap doc (separate PR after this matcher PR).
- Not the solana-program 2.0 migration (deferred).

**Workflow constraints (CLAUDE.md):**

- All Ballast TS code in `scripts/ballast/` and `tests/ballast/`. Never
  modify `scripts/` (upstream), `src/` (upstream), `test/` (upstream),
  `tests/` outside `tests/ballast/`.
- Conventional commits, branch `feat/phase-0-allowlist-matcher` (already
  on this branch — confirm with `git branch --show-current`).
- Claude emits commit / push / PR / on-chain-write commands as fenced bash
  blocks; I run them in the VS Code terminal at the repo root.
- Stage specific paths only; never `git add -A` or `git add .`.

I should be on branch `feat/phase-0-allowlist-matcher` with HEAD at
`ffb7864`. Confirm with `git branch --show-current` and `git log -1 --oneline`
first.

Read `MATCHER_TS_HANDOFF.md` §1 + §2 + §4.1 first (where we are, locked
decisions, step 4 spec), then start writing `scripts/ballast/utils/crank.ts`.
Show me the file's design (imports + function signature + ~3-line behavior
sketch) before filling in the body, so I can sanity-check before you commit
to the implementation.
