# Matcher Layout Reshape — Handoff Document

**Source session:** 2026-05-02 (matcher PR build, mid-flight reshape decision)
**Audience:** the next Claude Code session that picks up step 1 of the matcher PR build order
**Pair this with:** `docs/prompts/MATCHER_LAYOUT_RESHAPE_KICKOFF.md` (the kickoff prompt)
**Supersedes (in part):** `docs/prompts/MATCHER_IMPL_HANDOFF.md` §4.1 — the **state and init payload layouts are reshaped per §3 of this doc**. Everything else in MATCHER_IMPL_HANDOFF (D1 finding, FM-2/FM-3 invariants, build order, acceptance gates) is unchanged. Read this doc first.

---

## 1. Where we are in the build order

The matcher PR build order from `MATCHER_IMPL_HANDOFF.md` §5 still applies. Step 1 (`programs/ballast-matcher/src/lib.rs`) is **mid-flight** — we wrote a working draft using a "minimal layout" that stripped out upstream's vAMM/inventory/audit fields. After review, that turned out to leave Phase 1 vAMM half-implemented and create a real layout-migration cost (forced LP-slot reinit, blocked once admin authority is burned). **Reversing the call: the next session reshapes lib.rs to the upstream-aligned layout in §3 below.**

Status of each step:

| Step | Status | Owner | Notes |
|---|---|---|---|
| 0 (precursor) — track Cargo.lock + ship docs | **landed** in `chore: track Cargo.lock + handoff for layout reshape` | — | the lockfile pinning is required for cargo-build-sbf to even read manifests on the 1.18 toolchain. See `docs/solana-program-version-strategy.md`. |
| 1 — `programs/ballast-matcher/src/lib.rs` | **draft exists, needs reshape** | next session | working dir has a minimal-layout draft with 34 passing tests; reshape in place, don't reset to scaffold. |
| 2 — `programs/ballast-matcher/tests/integration.rs` | not started | next session | solana-program-test cases per MATCHER_IMPL_HANDOFF §5 step 2 + new vAMM dispatch cases (see §6 below) |
| 3 — `cargo build-sbf` verification | partially complete | next session | the existing draft builds; rebuild after reshape |
| 4–8 — TS-side helpers + setup scripts | not started | future session | unchanged from MATCHER_IMPL_HANDOFF §5 except `encodeBallastInit` payload size + field set updates per §4 below |
| 9 — retrofit `setup-ballast-sol-market.ts --insurance-only` | not started | future session | unchanged |
| 10 — PRD diff | not started | future session | unchanged |
| 11 — validation report | not started | future session | unchanged |

## 2. Decisions locked in this session (do not relitigate)

These were debated and settled. The next session should not re-open them without a strong reason.

| # | Decision | Rationale |
|---|---|---|
| 1 | **Init authorization: no extra signer.** `accounts[0]` (lp_pda) is read but not required to sign at init. Authorization comes from atomic create_account + init in `setup-ballast-matcher.ts`. | Matches upstream pmatch pattern. LP PDA can't sign at init by design (chicken-and-egg: percolator's invoke_signed requires the LP slot to exist, but the LP slot doesn't exist until after matcher init). |
| 2 | **`total_bps` cap: 9000.** Stored as `max_total_bps` in ctx (per upstream's pattern), validated at init AND defense-in-depth-validated at execution time. | Matches upstream `MatcherCtx::validate` heuristic. 1000 bps from the 10000-bps underflow cliff in the sell branch. Leaves room to test "many instruments" per user direction. |
| 3 | **Match length: `>= 67`** (not strict `==`). Reserved-zero check at bytes [43..67] is the actual tampering boundary. | Forward-compat with future Percolator wrappers that may append fields after the reserved area. Caller is Percolator-trusted post-FM-2/FM-3 anyway. |
| 4 | **Init alignment pads: strict zero.** Bytes 3, 13..16 in the init payload (and any new pads in the reshaped payload — see §4) must be zero. | Upstream's reserved-zero discipline applied to alignment slack. Preserves wire-extension optionality + catches client buffer-init bugs. |
| 5 | **`solana-program = "~1.18"` retained.** Migration to 2.0 is documented as a future task in `docs/solana-program-version-strategy.md`. | Toolchain churn deferred. Matcher CPI is wire-byte ABI; mixed solana-program versions across percolator engine and matcher are fine. |
| 6 | **Track Cargo.lock with corrective pins.** blake3 1.5.5, indexmap 2.6.0, proc-macro-crate 3.2.0, jobserver 0.1.32, borsh 1.5.1, rayon 1.10.0, rayon-core 1.12.1. | Solana-program 1.18 transitive deps drifted into edition2024-requiring versions. Pins force compat with cargo 1.79 / rustc 1.75. See memory note `matcher-cargo-lock-pinning.md`. |
| 7 | **Single `lib.rs`, no submodules.** All consts + parsers + handlers + entrypoint + inline tests in one file. | Simpler than upstream's lib.rs+vamm.rs split for our smaller surface. ~600 LOC after reshape. |
| 8 | **No Anchor, no bytemuck, no borsh-derive on the wire.** Hand-rolled `from_le_bytes` byte slicing. | Matches upstream pmatch idiom. Anchor/bytemuck would interpose between us and the fixed-byte CPI ABI. |
| 9 | **BALLAST_MAGIC = `0x0054_5341_4C4C_4142`** (LE bytes spell `b"BALLAST\0"`). Distinct from upstream's `MATCHER_MAGIC = 0x5045_5243_4d41_5443` (`b"PERCMATC"` reading). | Defense-in-depth: an upstream-program ctx fed to our matcher (or vice versa) fails the magic check immediately. |

## 3. Reshaped layout (THE main change)

### 3.1 Why

The minimal layout (per `MATCHER_IMPL_HANDOFF.md` §4.1) stripped these upstream fields: `max_fill_abs`, `max_inventory_abs`, `impact_k_bps`, `liquidity_notional_e6`, `max_total_bps`, `inventory_base`, `last_oracle_price_e6`, `last_exec_price_e6`. Defense was YAGNI for Phase 0. Cost: Phase 1 vAMM mode requires layout change → LP-slot reinit → market downtime → blocks if admin authority is burned in Phase 0 Step 0.7+. Senior-engineer call: take the 2–3 hours now to keep the upstream-aligned layout, slot Ballast extras into the same 320..384 region. Phase 1 vAMM becomes a flag flip + reinit-fee/spread, no layout migration.

### 3.2 ctx layout (384 bytes total)

**Bytes 0..64:** `MatcherReturn` scratch (Percolator ABI v2). Written on every Match call; left zero on init.

**Bytes 64..384:** Ballast state. Layout below uses absolute offsets.

```
Offset  Size  Field                     Notes
64      8     magic                     u64 LE; BALLAST_MAGIC = 0x0054_5341_4C4C_4142
72      4     version                   u32 LE; = 1
76      1     kind                      u8; 0 = Passive, 1 = vAMM
77      1     allow_trade_cpi_fills     u8; 0 = always-reject, 1 = passive/vAMM fill
78      2     _pad0                     [u8; 2]; must be zero
80      32    lp_pda                    [u8; 32]; FM-3 verification target
112     4     trading_fee_bps           u32 LE
116     4     base_spread_bps           u32 LE
120     4     max_total_bps             u32 LE; stored cap (≤ 9000); LP can self-tighten
124     4     impact_k_bps              u32 LE; vAMM-only; 0 if Passive
128     16    liquidity_notional_e6     u128 LE; vAMM-only; > 0 if vAMM
144     16    max_fill_abs              u128 LE; per-call cap (0 = zero-fill)
160     16    inventory_base            i128 LE; mutable on fill
176     8     last_oracle_price_e6      u64 LE; audit trail; updated on fill
184     8     last_exec_price_e6        u64 LE; audit trail; updated on fill
192     16    max_inventory_abs         u128 LE; inventory cap (0 = uncapped)
208     1     allowlist_count           u8; ≤ 4
209     3     _pad1                     [u8; 3]; must be zero
212     128   allowlist_0..3            4 × 32 bytes; audit metadata
340     44    _reserved                 [u8; 44]; zero, future Ballast fields
384            (total)
```

**Notes on the design choice (packed 384 vs separated 512):**
- Packed (384): replaces upstream's _reserved with our allowlist. We're not violating upstream's contract because we're a different program with our own magic. Smaller account, less rent.
- Separated (512): byte-identical upstream MatcherCtx in 64..320, Ballast tail at 320..512. Cleaner separation but ~33% bigger account. Not worth it given (a) our magic differs anyway so byte-identical doesn't unlock tooling, (b) we hand-roll parsers.

If a future change forces fields beyond the 44-byte reserved tail, **switch to a 512-byte ctx** rather than reshuffling. Don't shrink existing fields once mainnet exists.

### 3.3 Init payload (200 bytes total)

```
Offset  Size  Field                     Notes
0       1     tag                       = 2
1       1     kind                      0 = Passive, 1 = vAMM
2       1     allow_trade_cpi_fills     0 or 1
3       1     _pad0                     must be zero
4       4     trading_fee_bps           u32 LE
8       4     base_spread_bps           u32 LE
12      4     max_total_bps             u32 LE; ≤ 9000
16      4     impact_k_bps              u32 LE; must be 0 if Passive
20      16    liquidity_notional_e6     u128 LE; must be 0 if Passive; > 0 if vAMM
36      16    max_fill_abs              u128 LE
52      16    max_inventory_abs         u128 LE
68      1     allowlist_count           u8; ≤ 4
69      3     _pad1                     must be zero
72      128   allowlist_0..3            4 × 32; unused slots must be all zero
200            (total)
```

### 3.4 Init validation rules (in order)

1. `data.len() == 200` else `InvalidInstructionData`.
2. `data[0] == TAG_INIT (2)`.
3. `kind` ∈ {0, 1}.
4. `allow_trade_cpi_fills` ∈ {0, 1}.
5. `_pad0` and `_pad1` strict zero.
6. `max_total_bps ≤ 9000`.
7. `trading_fee_bps + base_spread_bps ≤ max_total_bps` (consistency: stored cap must accommodate fees+spread).
8. **If `kind == 0` (Passive):** `impact_k_bps == 0` AND `liquidity_notional_e6 == 0`.
9. **If `kind == 1` (vAMM):** `liquidity_notional_e6 > 0` (cannot quote vAMM with zero depth).
10. `allowlist_count ≤ 4`.
11. For `i` in `[allowlist_count, 4)`: allowlist slot `i` must be all zero.
12. ctx-account checks (in `process_init`, before parsing payload): owned by program, writable, size ≥ 384, magic == 0 (not initialized).

### 3.5 Match payload (unchanged, 67 bytes upstream-fixed)

Per MATCHER_IMPL_HANDOFF §4.3. No change.

### 3.6 Match handler dispatch

```
After FM-2 + FM-3 + parse_match_call:
  read kind, allow_trade_cpi_fills, fee, spread, max_total, impact, liquidity,
       max_fill, max_inventory, inventory_base from ctx state

  match allow_trade_cpi_fills:
    0 (NEVER)   → MatchReturn::rejected(req_id, lp_account_id, oracle_price)
    1 (FILL)    → match kind:
                    0 (Passive) → compute_passive_fill(...)
                    1 (vAMM)    → compute_vamm_fill(...)
    other       → InvalidAccountData

  if exec_size != 0:
    update inventory_base -= exec_size       (saturating_sub or checked_sub)
    update last_oracle_price_e6 = oracle
    update last_exec_price_e6   = exec_price
    write state back to ctx[64..]

  write MatchReturn to ctx[0..64]
```

### 3.7 Pricing helpers (signatures)

```rust
fn compute_passive_fill(
    oracle_price_e6: u64,
    req_size: i128,
    trading_fee_bps: u32,
    base_spread_bps: u32,
    max_total_bps: u32,           // stored cap; total clamped to this
    max_fill_abs: u128,
    inventory_base: i128,
    max_inventory_abs: u128,
    req_id: u64,
    lp_account_id: u64,
) -> Result<(MatchReturn, i128), ProgramError>;
//   Returns (return_struct, exec_size_for_inventory_update).
//   exec_size = signed; sign matches is_buy.
//   Math: total_bps = min(max_total_bps, base_spread_bps + trading_fee_bps)
//         exec_price = oracle * (BPS_DENOM ± total_bps) / BPS_DENOM
//   Caps: |fill| ≤ max_fill_abs (zero-fill if max_fill_abs == 0)
//   Inventory cap: clamp fill so |new_inventory_base| ≤ max_inventory_abs
//                  (max_inventory_abs == 0 means uncapped)
//   Zero-fill case: flags = FLAG_VALID | FLAG_PARTIAL_OK, exec_price = oracle, exec_size = 0

fn compute_vamm_fill(
    oracle_price_e6: u64,
    req_size: i128,
    trading_fee_bps: u32,
    base_spread_bps: u32,
    max_total_bps: u32,
    impact_k_bps: u32,
    liquidity_notional_e6: u128,
    max_fill_abs: u128,
    inventory_base: i128,
    max_inventory_abs: u128,
    req_id: u64,
    lp_account_id: u64,
) -> Result<(MatchReturn, i128), ProgramError>;
//   Same as passive but adds impact: total_bps = min(max_total, base + fee +
//   clamped_impact) where clamped_impact = min(impact_k * abs_notional /
//   liquidity, max_total - base - fee).
//   Reference: upstream vamm.rs:573-647.
```

The simplest approach: copy upstream's `compute_passive_execution` and `compute_vamm_execution` from `/tmp/claude-501/pmatch/vamm.rs:516-647` verbatim (with our field types/names). Their math is reviewed; don't reinvent.

## 4. What changes between current draft and reshaped lib.rs

The next session should **modify the existing draft in place**, not start over. Most parsers and tests carry over.

### Constants section
- ADD: `OFF_MAX_TOTAL_BPS = 120`, `OFF_IMPACT_K_BPS = 124`, `OFF_LIQUIDITY = 128`, `OFF_MAX_FILL_ABS = 144`, `OFF_INVENTORY_BASE = 160`, `OFF_LAST_ORACLE = 176`, `OFF_LAST_EXEC = 184`, `OFF_MAX_INVENTORY = 192`.
- SHIFT: `OFF_ALLOWLIST_COUNT 120 → 208`, `OFF_PAD1 121 → 209`, `OFF_ALLOWLIST 124 → 212`, `OFF_RESERVED 252 → 340`.
- CHANGE: `MATCHER_CTX_SIZE 320 → 384`.
- ADD `KIND_VAMM: u8 = 1` alongside existing `KIND_PASSIVE`.
- ADD `INIT_OFF_MAX_TOTAL_BPS = 12`, `INIT_OFF_IMPACT_K = 16`, `INIT_OFF_LIQUIDITY = 20`, `INIT_OFF_MAX_FILL = 36`, `INIT_OFF_MAX_INVENTORY = 52`. Shift `INIT_OFF_COUNT 12 → 68`, `INIT_OFF_PAD1 13 → 69`, `INIT_OFF_ALLOWLIST 16 → 72`. Update `INIT_LEN 144 → 200`.

### `InitPayload` struct
- ADD fields: `kind: u8`, `max_total_bps: u32`, `impact_k_bps: u32`, `liquidity_notional_e6: u128`, `max_fill_abs: u128`, `max_inventory_abs: u128`.
- Keep: `allow_trade_cpi_fills`, `trading_fee_bps`, `base_spread_bps`, `allowlist_count`, `allowlist`.

### `parse_init_payload`
- Update length check to 200.
- Read all new fields from updated offsets.
- Add the kind-vs-vAMM consistency checks (rule 8/9 from §3.4).
- Rename `kind` validation: now accepts `KIND_PASSIVE` OR `KIND_VAMM`.
- Add `max_total_bps ≤ 9000` check.
- Add `trading_fee_bps + base_spread_bps ≤ max_total_bps` check.

### `MatchReturn::passive_fill` / new `MatchReturn::vamm_fill`
- Replace the current standalone `passive_fill` constructor with two functions: `compute_passive_fill` and `compute_vamm_fill` per §3.7. They return `(MatchReturn, i128)` so the caller can do inventory updates.
- The internal pricing logic is upstream's; port `compute_passive_execution` and `compute_vamm_execution` from `/tmp/claude-501/pmatch/vamm.rs:516-647`.

### `process_init`
- Verify ctx size ≥ 384 (was 320).
- Write all the new fields. `kind` from payload (was hardcoded to 0). All u128 fields zero-initialized except where payload specifies (e.g. max_fill_abs comes from payload).
- Initialize state fields: `inventory_base = 0`, `last_oracle_price_e6 = 0`, `last_exec_price_e6 = 0`. (These are mutable; init to zero.)

### `process_match`
- After FM-2 + FM-3, read all required ctx fields (kind + allow_fills + fee + spread + max_total + impact + liquidity + max_fill + inventory + max_inventory).
- Dispatch on `allow_trade_cpi_fills`, then on `kind` per §3.6.
- Update inventory_base + last_*_price after non-zero fill, write state back.

### Inline tests
- Existing 34 tests carry over with offset/length adjustments; mostly mechanical updates.
- ADD: `init_rejects_passive_with_nonzero_impact_k`, `init_rejects_passive_with_nonzero_liquidity`, `init_rejects_vamm_with_zero_liquidity`, `init_rejects_max_total_above_9000`, `init_rejects_fee_plus_spread_above_max_total`.
- ADD: `passive_caps_at_max_fill_abs`, `passive_zero_fill_when_max_fill_zero`, `passive_inventory_cap_clamps_fill`, `passive_inventory_cap_blocks_when_at_boundary`.
- ADD: `vamm_buy_adds_impact`, `vamm_sell_subtracts_impact`, `vamm_bigger_size_more_impact`, `vamm_total_capped_at_max_total_bps`.
- ADD: `inventory_base_updated_on_fill`, `last_oracle_and_exec_price_updated_on_fill`, `state_unchanged_on_zero_fill`.

Reference upstream `/tmp/claude-501/pmatch/vamm.rs:760-870` for similar test cases.

## 5. Acceptance gates (rerun before declaring step 1 done)

- [ ] `cargo check --features no-entrypoint` — clean.
- [ ] `cargo test --features no-entrypoint` — all inline tests pass (estimated 50+ after reshape).
- [ ] `cargo-build-sbf` succeeds; `programs/ballast-matcher/target/deploy/ballast_matcher.so` produced.
- [ ] Diff of `lib.rs` is reviewable: clear separation between offset constants block, parser block, return-builder block, handler block, test block.
- [ ] No new deps added to Cargo.toml (still just `solana-program = "~1.18"`).
- [ ] Cargo.lock untouched by step 1 work (it was already pinned in the precursor commit).

## 6. Things to NOT do in this PR

(Same as MATCHER_IMPL_HANDOFF.md §12, restated for completeness.)

- Do NOT modify upstream files (anything in `scripts/` not under `scripts/ballast/`, anything in `src/`, `tests/` not under `tests/ballast/`, `test/`).
- Do NOT change `solana-program` version (deferred to future migration; see `docs/solana-program-version-strategy.md`).
- Do NOT delete or aggressively `cargo update` the Cargo.lock (the pins are load-bearing; see memory note `matcher-cargo-lock-pinning.md`).
- Do NOT add Anchor / bytemuck / borsh-derive (hand-rolled byte slicing only).
- Do NOT burn authorities (Phase 0 Step 0.7+, separate PR).
- Do NOT execute trades (separate PR after Step 0.5 keeper bot).
- Do NOT deploy to mainnet under any circumstances.
- Do NOT commit keypair files; never log private keys.
- Do NOT use `git add -A` or `git add .`; stage specific paths only.

## 7. Workflow rule reminder

Per CLAUDE.md, Claude does NOT execute commits, pushes, gh, on-chain writes, or `solana program deploy`. Emit fenced bash blocks with the exact command(s) for the user to run at the repo root. Free for Claude: `git status | diff | log`, `cargo check`, `cargo test`, `cargo-build-sbf` (sandbox-safe).

For step 1 reshape: Claude writes lib.rs + tests, runs `cargo check / test / build-sbf` to verify, then emits a single commit block with `git add programs/ballast-matcher/src/lib.rs && git commit -m "..."` for the user to run.

## 8. Open follow-ups deferred from this PR

- **Migration to `solana-program 2.0`** — see `docs/solana-program-version-strategy.md`. Defer until trigger fires.
- **Slab `tradingFeeBps` destination empirical confirmation** — Step 0.6.
- **trade-cpi double-charge investigation** — Step 0.6.
- **Phase 1 freight slab type** (Hyperp vs non-Hyperp) — Phase 1 planning.
- **Monetization-roadmap doc** — separate PR after this matcher PR.
- **Keeper bot (Step 0.5)** — separate PR.
- **Step 0.6 trade execution** — separate PR.

## 9. Source files to read at session start

In order:

1. **`docs/prompts/MATCHER_LAYOUT_RESHAPE_HANDOFF.md`** (this doc) — full briefing on the reshape + decisions locked.
2. **`CLAUDE.md`** — workflow rule.
3. **`docs/prompts/MATCHER_IMPL_HANDOFF.md`** §1 (D1 finding), §4.5 (Match handler order — still applies), §4.6 (Init handler order — still applies), §11 (acceptance gates — superset of §5 here).
4. **`docs/solana-program-version-strategy.md`** — why we're on 1.18 and how the lockfile pins work; do not alter without reading.
5. **`programs/ballast-matcher/src/lib.rs`** — current draft (minimal-layout, 34 passing tests). This is your starting point; reshape in place.
6. **`programs/ballast-matcher/Cargo.toml`** + **`programs/ballast-matcher/Cargo.lock`** — pinned, do not change.
7. **Upstream `percolator-match` source** at `/tmp/claude-501/pmatch/`:
   - `lib.rs` — MatcherCall layout, MatcherReturn struct, process_instruction dispatch.
   - `vamm.rs:80-126` — upstream MatcherCtx layout (256 bytes); we mirror the field set, different magic.
   - `vamm.rs:299-348` — upstream InitParams (66 bytes); we have a 200-byte superset that adds allowlist + flag.
   - `vamm.rs:516-647` — upstream's `compute_passive_execution` and `compute_vamm_execution`. **Port verbatim** (with field name updates) into our `compute_passive_fill` / `compute_vamm_fill`.
   - `vamm.rs:760-870` — upstream test cases; pattern for our new vAMM/inventory tests.
8. **`docs/prd.md`** §4.7 (matcher spec — note: FM-1 reframe is part of this PR's diff in step 10), §4.9 Step 0.6 (note: trade-cpi → trade-nocpi diff), SC-0.7.
9. **Saved memories at `~/.claude/projects/.../memory/`:**
   - `MEMORY.md` — auto-loaded.
   - `percolator-oracle-stale-gate.md` — relevant for steps 4+ (TS scripts), not step 1.
   - `cargobill-monetization-roadmap.md` — relevant for context, not step 1.
   - `matcher-cargo-lock-pinning.md` — read if/when touching Cargo.lock.

## 10. Live devnet state (verify before assuming)

Unchanged from MATCHER_IMPL_HANDOFF §6:

```
Network:               devnet
Percolator program:    2SSnp35m7FQ7cRLNKGdW5UzjYFF6RBUNq7d3m5mqNByp  (v12.21+, solana-program 1.18)
SOL/USD slab:          HftDjBvRArFoSnGcvxwSCN7rok5PYZtK2shckWBE5inY
  vault PDA:           J1ohDnxM63A2Qkjbp5w7T9WYCVPNwTUBWjHcrVoEVufd
  oracle (Pyth Pull):  7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE
  insurance:           5 SOL
  positions:           none
  market_mode:         Live
```

Confirm with `npx tsx scripts/dump-market.ts --slab HftDjBvRArFoSnGcvxwSCN7rok5PYZtK2shckWBE5inY` before running scripts.

## 11. Funding state (verify; user has friend SOL access if more needed)

| Role | Pubkey | File path | Required | Notes |
|---|---|---|---|---|
| LP / admin / deployer | `J9iCXvvxdjeDGUGUCEBPVbuhNDLYv4UEv4hDN5UHe56y` | `~/.config/ballast/ballast-lp.json` | ~12 SOL | user said funded as of 2026-05-02 |
| Hedger | run `solana-keygen pubkey ~/.config/ballast/ballast-hedger.json` | `~/.config/ballast/ballast-hedger.json` | ~5.5 SOL | user said funded |
| Oracle authority | run `solana-keygen pubkey ~/.config/ballast/ballast-oracle-authority.json` | `~/.config/ballast/ballast-oracle-authority.json` | ~0.1 SOL eventually | step 0.7+ |
