# Matcher Layout Reshape — Kickoff Prompt

Paste the section between the `---` markers below into a fresh Claude Code chat.

---

I'm continuing the Ballast project (devnet-only POC for compliant on-chain
bilateral derivatives on Anatoly Yakovenko's Percolator perpetual futures
protocol on Solana). Repo: ~/Documents/GitHub/ballast-percolator-cli.

The matcher PR (PRD Phase 0 Step 0.4, branch `feat/phase-0-allowlist-matcher`)
is mid-flight on step 1 of its build order. A prior session wrote a working
draft of `programs/ballast-matcher/src/lib.rs` using a minimal layout that
stripped upstream's vAMM/inventory/audit fields. After review we reversed
that call and approved an upstream-aligned reshape. Cargo.lock + the reshape
handoff + this kickoff + a solana-program version-strategy doc landed as a
precursor docs/infra commit. Step 1's reshape is the next concrete task.

**Read in this order before writing any code:**

1. docs/prompts/MATCHER_LAYOUT_RESHAPE_HANDOFF.md — authoritative for this
   session. Contains: status of each build-order step, decisions locked
   from prior sessions (do not relitigate), the reshaped 384-byte ctx
   layout, the 200-byte init payload, init validation rules, Match handler
   dispatch, pricing helper signatures, what changes between current draft
   and reshaped lib.rs (offset shifts, struct field additions, parser
   updates, handler updates, new test cases). §3.7 directs you to port
   upstream's `compute_passive_execution` and `compute_vamm_execution`
   from /tmp/claude-501/pmatch/vamm.rs:516-647 verbatim with our field
   names.

2. CLAUDE.md — project conventions, security rules, workflow rule
   (Claude does NOT run commits / pushes / gh / on-chain writes — emits
   bash blocks for me to run).

3. docs/prompts/MATCHER_IMPL_HANDOFF.md §1 (D1 finding & architectural
   pivot — still load-bearing), §4.5 (Match handler order), §4.6 (Init
   handler order), §11 (acceptance gates — superset of the reshape doc's
   §5). Skip §4.1 (layout is reshaped per the new handoff).

4. docs/solana-program-version-strategy.md — why we're pinned to 1.18,
   why the Cargo.lock pins are load-bearing. **DO NOT touch Cargo.lock**;
   if anything seems off there, read this doc first.

5. programs/ballast-matcher/src/lib.rs — current draft (minimal layout,
   34 passing inline tests). **Reshape this in place — do not reset to
   the scaffold.** Most parsers and tests carry over with offset/length
   adjustments. Reshape sequence per RESHAPE_HANDOFF.md §4.

6. programs/ballast-matcher/Cargo.toml + Cargo.lock — pinned, do not
   change.

7. Upstream `percolator-match` source at /tmp/claude-501/pmatch/:
   - lib.rs — MatcherCall layout, MatcherReturn ABI v2 struct, dispatcher.
   - vamm.rs:80-126 — upstream MatcherCtx 256-byte layout (we mirror the
     field set; different magic, different total ctx size, allowlist
     replaces upstream's _reserved tail).
   - vamm.rs:299-348 — upstream 66-byte InitParams (we have a 200-byte
     superset adding flag + allowlist).
   - vamm.rs:516-647 — port these two functions verbatim into our
     `compute_passive_fill` / `compute_vamm_fill`.
   - vamm.rs:760-870 — pattern for new vAMM/inventory test cases.

8. src/abi/{accounts,instructions}.ts — upstream-CLI encoders (read but
   do not modify; they're upstream-untouched).

9. ~/.claude/projects/-Users-ap-Documents-GitHub-ballast-percolator-cli/
   memory/ — read MEMORY.md, then matcher-cargo-lock-pinning.md if you
   need to understand the lockfile situation. percolator-oracle-stale-gate.md
   and cargobill-monetization-roadmap.md are not needed for step 1.

**Build order I want followed (one logical commit per step, each
reviewable before moving to the next):**

The full build order is in MATCHER_IMPL_HANDOFF.md §5 and updated status
is in RESHAPE_HANDOFF.md §1. The immediate next commit is step 1
(reshape lib.rs). Steps 2+ (integration tests, TS helpers, setup scripts,
PRD diff, validation report) follow in their own commits.

For step 1 specifically:

1. Reshape lib.rs in place per RESHAPE_HANDOFF.md §3 (layout) + §4 (what
   changes). Constants section first (offset shifts + new fields), then
   InitPayload struct, then parse_init_payload, then port upstream's
   pricing math into compute_passive_fill / compute_vamm_fill, then
   process_init updates, then process_match dispatch updates, then test
   updates + new test cases per §4.
2. Verify: `cargo check --features no-entrypoint`, `cargo test --features
   no-entrypoint`, `cargo-build-sbf`. All must pass.
3. Emit a single commit block: `git add programs/ballast-matcher/src/lib.rs`
   then `git commit -m "feat(matcher): ..."` for me to run.

**Decisions already locked (per RESHAPE_HANDOFF.md §2 — DO NOT relitigate):**

- No extra signer at init.
- total_bps cap: 9000 (stored as max_total_bps; validated at init AND at
  execution time).
- Match length check: >= 67 (forward-compat); reserved-zero at [43..67] is
  the tampering boundary.
- Init alignment pads: strict zero (bytes 3, 69..72 in the 200-byte init).
- solana-program: stay on ~1.18; migration documented separately.
- Cargo.lock: tracked, pins are load-bearing, do not modify.
- Single lib.rs file, no submodules.
- No Anchor / bytemuck / borsh-derive on the wire.
- BALLAST_MAGIC = 0x0054_5341_4C4C_4142 (LE bytes spell b"BALLAST\0";
  distinct from upstream's b"PERCMATC").
- Layout: packed 384 bytes (allowlist replaces upstream's _reserved). Not
  byte-identical-upstream (would need 512). Don't reshuffle this without
  reading RESHAPE_HANDOFF.md §3.2 first.

**What this PR is NOT:**

- Not the keeper bot (PRD Step 0.5 — separate PR).
- Not trade execution / SC-0.6 work (separate PR).
- Not authority burns (Phase 0 Step 0.7+).
- Not the monetization-roadmap doc (separate PR after this one; memory
  note already saved).
- Not the solana-program 2.0 migration (deferred; see
  docs/solana-program-version-strategy.md).

**Workflow constraints (CLAUDE.md):**

- All Ballast code in scripts/ballast/, tests/ballast/, programs/, config/,
  docs/. Never modify upstream files.
- bigint on the wire for all u64/u128/i128 (TS side, when we get there).
- Conventional commits, branch feat/phase-0-allowlist-matcher (already on
  this branch).
- Claude emits commit / push / PR / on-chain-write commands as fenced
  bash blocks; I run them in the VS Code terminal at the repo root.

Read RESHAPE_HANDOFF.md §3 (the new layout) and §4 (what changes between
current draft and reshape) first, then start updating
programs/ballast-matcher/src/lib.rs. Show me the design (which constants
shift, which struct fields are added, the new function signatures for
compute_passive_fill / compute_vamm_fill) before filling in bodies, so I
can sanity-check before you commit to the implementation.

I should be on branch `feat/phase-0-allowlist-matcher`. Confirm with
`git branch --show-current` first.

---
