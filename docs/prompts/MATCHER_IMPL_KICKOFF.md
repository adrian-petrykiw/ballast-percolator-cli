# Matcher Implementation — New-chat Kickoff Prompt

Paste the block below into a fresh Claude Code session at the repo root. The handoff document (`docs/prompts/MATCHER_IMPL_HANDOFF.md`) carries the full state of the world from the design session — this kickoff just points the new chat at it and the PRD.

---

```
I'm continuing the Ballast project (devnet-only POC for compliant on-chain
bilateral derivatives on Anatoly Yakovenko's Percolator perpetual futures
protocol on Solana). Repo: ~/Documents/GitHub/ballast-percolator-cli.

The matcher PR (PRD Phase 0 Step 0.4) just finished its design phase in a
prior session. We agreed on the architecture and pivoted from PRD §4.7 FM-1
as written (matcher enforces the allowlist) to a two-layer model: LP-signature
gate via trade-nocpi (off-chain LP signing service refuses non-allowlisted
counterparties) + matcher always-rejects all trade-cpi calls to close the
side door. The matcher carries a forward-compat flag (allow_trade_cpi_fills)
so Phase 1 freight (likely Hyperp) can use the same binary in passive-fill
mode later. PRD edits to §4.7 / §4.9 Step 0.6 / SC-0.7 are part of this PR.

**Read in this order before writing any code:**

1. docs/prompts/MATCHER_IMPL_HANDOFF.md — full post-design briefing.
   Contains: D1 finding (matcher CPI ABI doesn't pass counterparty pubkey),
   architectural pivot, byte-level matcher specs, file-by-file build order,
   acceptance gates. Authoritative; supersedes parts of the older
   MATCHER_PR_HANDOFF.md.

2. CLAUDE.md — project conventions, security rules, workflow rule
   (Claude does NOT run commits / pushes / gh / on-chain writes — emits
   bash blocks for me to run).

3. docs/prompts/MATCHER_PR_HANDOFF.md §1, §2, §3, §4 — the parts still
   load-bearing: live devnet state, wallet state, OracleStale gate finding,
   Pyth VerificationLevel layout note. Skip §5 (superseded by IMPL_HANDOFF).

4. docs/prd.md §4.7 (matcher spec — note FM-1 reframe is in this PR's diff),
   §4.9 Step 0.6 (note trade-cpi → trade-nocpi diff is in this PR), SC-0.7.

5. programs/ballast-matcher/Cargo.toml + src/lib.rs — current scaffold.

6. The upstream percolator-match source (download to $TMPDIR/pmatch/ if
   not present — see IMPL_HANDOFF §9 step 6 for URLs). lib.rs has the
   MatcherCall + MatcherReturn ABI we must conform to.

7. src/abi/{accounts,instructions}.ts — encoders + account specs we use.

8. ~/.claude/projects/-Users-ap-Documents-GitHub-ballast-percolator-cli/
   memory/ — read MEMORY.md, then both linked memories
   (percolator-oracle-stale-gate.md, cargobill-monetization-roadmap.md).

**Build order I want followed (one logical commit per step, each
reviewable before moving to the next):**

1. programs/ballast-matcher/src/lib.rs — full matcher implementation per
   IMPL_HANDOFF §4. Use solana-program ~1.18 (already in Cargo.toml).
   Hand-rolled byte slicing matching upstream style; no extra deps.
2. programs/ballast-matcher/tests/integration.rs — solana-program-test
   cases per IMPL_HANDOFF §5 step 2.
3. cargo build-sbf — verify produces target/deploy/ballast_matcher.so.
4. scripts/ballast/utils/crank.ts — prependCrankIfStale helper.
5. scripts/ballast/utils/wsol.ts — extract ensureWrappedSol from
   setup-ballast-sol-market.ts (refactor only).
6. scripts/ballast/utils/matcher.ts — encodeBallastInit (TS mirror of
   the 144-byte Rust init parser) + MATCHER_CTX_SIZE + BALLAST_MAGIC.
7. scripts/ballast/setup-ballast-matcher.ts — atomic LP-init tx.
8. scripts/ballast/setup-ballast-participants.ts — hedger init+deposit.
9. Retrofit setup-ballast-sol-market.ts --insurance-only with crank prepend.
10. PRD diff (docs/prd.md): §4.7 FM-1 reframe, §4.9 Step 0.6 trade-nocpi,
    SC-0.7 rescope. See IMPL_HANDOFF §5 step 10 for suggested wording.
11. docs/reports/phase-0-step-0.4-report.md — written AFTER I run the
    deploys and report sigs back.

**What this PR is NOT:**

- Not the keeper bot (PRD Step 0.5 — separate PR).
- Not trade execution / SC-0.6 work (separate PR).
- Not authority burns (Phase 0 Step 0.7+).
- Not the monetization-roadmap doc (separate PR after this one; memory
  note already saved).

**Workflow constraints (CLAUDE.md):**

- All Ballast code in scripts/ballast/, tests/ballast/, programs/, config/,
  docs/. Never modify upstream files.
- bigint on the wire for all u64/u128/i128 (never JS number).
- Conventional commits, branch feat/phase-0-allowlist-matcher.
- Claude emits commit / push / PR / on-chain-write commands as fenced
  bash blocks; I run them in the VS Code terminal at the repo root.

**Funding I need to do before scripts run** (per IMPL_HANDOFF §7):

- LP wallet J9iCXvvxdjeDGUGUCEBPVbuhNDLYv4UEv4hDN5UHe56y to ~12 SOL
  (handles 10 SOL LP collateral + matcher rent + tx fees + program deploy
  rent (recoverable on close)).
- Hedger wallet to ~5.5 SOL.
- Oracle authority can wait (~0.1 SOL eventually, used in Step 0.7).

Read IMPL_HANDOFF §1 (the D1 finding and pivot) and §4 (byte-level
matcher specs) first, then start writing programs/ballast-matcher/src/lib.rs.
Show me the design (constants, struct layout, function signatures) before
filling in the bodies, so I can sanity-check before you commit to the
implementation.
```

---

## Notes for the user

- The handoff doc and this kickoff doc both live in `docs/prompts/` for symmetry with the existing `MATCHER_PR_KICKOFF.md`. They should be committed in a small docs-only precursor commit (single file pair) so the new session pulls them via `git pull` before starting — or just left uncommitted in the working tree if the new session is started immediately.
- If you want to commit them first as their own small PR (cleaner history), the bash block to do that is below. Otherwise let them ride in the matcher PR.
- The kickoff prompt explicitly asks the new session to show the matcher design (constants + layout + function sigs) before filling in implementations — gives you a checkpoint to catch design drift early.

### Optional: commit the handoff + kickoff as a precursor

```bash
# at repo root
git add docs/prompts/MATCHER_IMPL_HANDOFF.md docs/prompts/MATCHER_IMPL_KICKOFF.md
git commit -m "$(cat <<'EOF'
docs: matcher PR implementation handoff + kickoff (post-design)

Captures the post-design state of the world for the allowlist-matcher PR
(PRD Phase 0 Step 0.4) so a fresh chat can pick up implementation cleanly:

- D1 finding: upstream Match-CPI ABI passes only [lp_pda, ctx] + 67 bytes
  (no counterparty pubkey). PRD §4.7 FM-1 as written is unenforceable
  inside the matcher.
- Architectural pivot: hedge trades use trade-nocpi (LP signing service
  is the access-control gate); matcher defaults to always-reject on
  trade-cpi to close the side channel.
- Forward-compat flag (allow_trade_cpi_fills) in matcher_ctx so Phase 1
  freight (Hyperp) can use the same binary in passive-fill mode without
  redeploy.
- PRD edits to §4.7 / §4.9 Step 0.6 / SC-0.7 deferred into the matcher PR.

Supersedes parts of MATCHER_PR_HANDOFF.md §5; that doc remains valid for
live state, wallet state, and OracleStale + Pyth findings.
EOF
)"
```
