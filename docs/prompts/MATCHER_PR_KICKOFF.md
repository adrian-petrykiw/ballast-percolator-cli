# Matcher PR — New-chat Kickoff Prompt

Paste the block below into a fresh Claude Code session at the repo root. The handoff document referenced inside (`docs/prompts/MATCHER_PR_HANDOFF.md`) carries the full state-of-the-world from the prior session — Claude reads it first, then the PRD/CLAUDE.md, then the upstream sources it needs.

---

```
I'm continuing the Ballast project (devnet-only POC for compliant on-chain
bilateral derivatives on Anatoly Yakovenko's Percolator perpetual futures
protocol on Solana). Repo: ~/Documents/GitHub/ballast-percolator-cli.

**Read in this order before doing anything:**
1. docs/prompts/MATCHER_PR_HANDOFF.md — full state-of-the-world from the
   prior session. Lives addresses, wallet balances, the OracleStale gate
   finding, the Pyth VerificationLevel layout note, and the matcher PR
   scope. Treat this as the authoritative briefing.
2. CLAUDE.md — project conventions, security rules, workflow rule
   (Claude does NOT run commits / pushes / gh / on-chain writes — emits
   bash blocks for me to run).
3. docs/prd.md §4.7 (Allowlist Matcher Specification, FM-1..FM-6),
   §4.9 Step 0.4 (Allowlist Matcher Deployment), and §4.9 Step 0.3 entry
   note about the OracleStale gate.
4. docs/pyth-oracle-compatibility.md §8 (Operational findings).
5. programs/ballast-matcher/Cargo.toml + src/lib.rs — the Rust scaffold
   from PR #1, no logic yet.
6. scripts/setup-devnet-market.ts (upstream — read-only) — its
   passive-matcher init pattern is the closest reference. We will fork
   structure, never modify.
7. The upstream `percolator-match` reference repo (linked from upstream
   percolator-cli README) — read before coding the Rust matcher to lock
   the CPI account ordering and matcher-context layout.

**Phase 0 status before this session:**
- SC-0.1 (Market Deployment) — DONE in PR #3 (squash-merged into master).
  Slab `HftDjBvRArFoSnGcvxwSCN7rok5PYZtK2shckWBE5inY`, Pyth Pull oracle,
  insurance 5 SOL, mark price seeded.
- SC-0.2 (Participant Initialization) — open. Bound to this PR because
  InitLP binds the matcher program at LP-init.
- All other SC items downstream of SC-0.2 are open.

**Wallet funding I need to do before scripts run** (per §3 of the
handoff doc):
- LP wallet `J9iCXvvxdjeDGUGUCEBPVbuhNDLYv4UEv4hDN5UHe56y` to ~12 SOL
  (currently ~0.58 liquid; the 5 stranded wSOL in ATA
  `EAtykJ5jm93Wn1EXRRLjTK5G5bzhPhk9twF9UpXUvxrE` will be consumed by
  the LP's 10 SOL deposit)
- Hedger wallet to ~5.5 SOL
- Oracle authority can wait

**Goal of this PR:**
- Implement the Ballast allowlist matcher in Rust per PRD §4.7 (FM-1..FM-6)
- Deploy to devnet (I run `solana program deploy`)
- Atomic LP-init transaction (matcher-ctx-create + matcher-init + InitLP +
  DepositCollateral) in `scripts/ballast/setup-ballast-matcher.ts`
- InitUser + DepositCollateral for the hedger in
  `scripts/ballast/setup-ballast-participants.ts`
- Introduce a `prependCrankIfStale()` helper applied to all engine-state
  ops (the OracleStale 0x6 fix from §4.1 of the handoff)
- Retrofit the helper into `setup-ballast-sol-market.ts --insurance-only`
- Validation report at docs/reports/phase-0-step-0.4-report.md
- All scripts must be reproducible and idempotent

**What this PR is NOT:**
- Not the keeper bot (PRD Step 0.5 — separate PR after this one)
- Not the trade execution / SC-0.6 work (separate PR)
- Not authority burns (Phase 0 Step 0.7+, after validation)

**Workflow constraints (CLAUDE.md):**
- All Ballast code in scripts/ballast/, tests/ballast/, programs/, config/,
  docs/. Never modify upstream files.
- bigint on the wire for all u64/u128/i128 (never JS number).
- Conventional commits, branch `feat/phase-0-allowlist-matcher`.
- Claude emits commit / push / PR / on-chain-write commands as fenced
  bash blocks; I run them in the VS Code terminal at the repo root.

Read the handoff doc (item 1) and the PRD sections (item 3) first, then
propose a concrete file-by-file plan with the design questions in §5.2 of
the handoff resolved. Don't start writing Rust until we agree on the
matcher context layout and the CPI account ordering.
```

---

## Notes for the user

- The handoff doc and this kickoff doc both live in `docs/prompts/` for symmetry with `KICKOFF_PROMPT.md`.
- If you want to skip the file lookups and inline more context in the kickoff prompt, the handoff doc is self-sufficient — pasting its contents directly into a new chat would also work, but the file-pointer approach above keeps the prompt short and lets the new Claude pull only what it needs.
- Before pasting, fund the LP and hedger wallets per §3 of the handoff doc so the matcher PR isn't blocked on a `solana airdrop` rate limit mid-session.
