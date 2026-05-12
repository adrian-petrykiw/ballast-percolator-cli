# `solana-program` version strategy

**Audience:** anyone building, deploying, or auditing the Ballast matcher program.
**TL;DR:** we're on `solana-program = "~1.18"` to mirror Percolator devnet's engine. Upstream's reference matcher (`aeyakovenko/percolator-match`) is on 2.0. The 1.18 transitive dep tree has bit-rotted; we work around it with pinned `Cargo.lock`. Migration to 2.0 is a deliberate future task with real env churn.

---

## Current state

| Crate | `solana-program` | Why |
|---|---|---|
| `aeyakovenko/percolator-prog` (the on-chain perp engine, deployed at `2SSnp35m7FQ7cRLNKGdW5UzjYFF6RBUNq7d3m5mqNByp` on devnet) | **`"1.18"`** | Anatoly's main repo, 1.18 line. |
| `aeyakovenko/percolator-match` (reference passive matcher) | **`"2.0"`** | Anatoly's reference matcher, ahead of his own engine. |
| `programs/ballast-matcher` (this repo) | **`"~1.18"`** | Mirrors the perp engine for development consistency. |

The matcher CPI is a **wire-byte ABI** (67-byte call payload, 64-byte return). Rust types in `solana-program` do not cross the CPI boundary — bytes do. **A solana-program-2.0-built matcher CPI'd by a solana-program-1.18-built percolator works perfectly.** Upstream pmatch (2.0) and upstream percolator-prog (1.18) are the proof that mixed-version is fine. We are on 1.18 by inertia, not by ABI necessity.

## Why staying on 1.18 right now

1. **Local toolchain reality.** `cargo-build-sbf` on the current dev machine bundles **rustc 1.75** (from solana CLI 1.18.x). solana-program 2.0 needs **rustc ≥ 1.79** transitively. Migrating means installing a newer solana CLI release that bundles a newer cargo-build-sbf + rustc — a one-time toolchain upgrade we don't want to bundle into Phase 0.
2. **Phase 0 schedule.** The matcher PR is on the critical path to SC-0.2 (participant init). Toolchain migration is its own validation cycle. Defer.
3. **Lockfile pinning works.** The Cargo.lock fix in this branch makes 1.18 buildable on the current toolchain. Known-quantity workaround.

## The Cargo.lock pinning workaround

The 1.18 line is in maintenance mode, but its transitive deps on crates.io kept moving. By default `cargo` resolves `^1` and `~1.18`-style constraints to the **latest semver-compatible** patch — which now includes crates that require either `edition = "2024"` (cargo ≥ 1.85) or rustc ≥ 1.77.

Specific drift culprits (May 2026):

| Transitive | Latest version | Wall hit |
|---|---|---|
| `blake3` | 1.8.5 → `digest 0.11.2` → `block-buffer 0.12.0` | edition2024 |
| `borsh-derive` | 1.6.1 | rustc ≥ 1.77 |
| `proc-macro-crate` | 3.5.0 → `toml_edit 0.25.x` | edition2024 |
| `indexmap` | 2.14.0 → `hashbrown 0.17.0` | edition2024 |
| `getrandom` | 0.3.x → `wasi 0.14` → `wasip2 1.0` → `wit-bindgen 0.57` | edition2024 |
| `rayon` | 1.12.0 + `rayon-core 1.13.0` | rustc ≥ 1.80 |

Required pins to keep the dep graph buildable on cargo 1.79 / rustc 1.75 (May 2026 baseline):

```
blake3            = 1.5.5
indexmap@2.x      = 2.6.0
proc-macro-crate@3.x = 3.2.0
jobserver         = 0.1.32   # cascades: drops getrandom 0.3 + wasip2 + wit-bindgen
borsh             = 1.5.1    # drags borsh-derive 1.5.7
rayon             = 1.10.0
rayon-core        = 1.12.1
```

Applied via `cargo update -p <crate>@<current> --precise <target>`, then verified with `cargo check --features no-entrypoint` AND `cargo-build-sbf` (the latter pulls extra build-target deps that `cargo check` doesn't).

**The Cargo.lock at `programs/ballast-matcher/Cargo.lock` is now tracked in git.** The `.gitignore` comment ("lockfile is committed for deterministic builds") was previously aspirational; this PR makes it real. **Do not delete or `cargo update` aggressively without re-validating against both `cargo check` and `cargo-build-sbf`.**

Validation grep checks (post `cargo update`):
- `grep "block-buffer" Cargo.lock` must NOT show 0.12.x
- `grep "wasip2" Cargo.lock` must be empty
- `grep "wit-bindgen" Cargo.lock` must be empty
- `grep "borsh-derive" Cargo.lock` must show 1.5.7 (or earlier 1.5.x)

---

## When to migrate to `solana-program = "2.0"`

Trigger the migration if any of these become true:

1. **The Percolator perp engine bumps to 2.x.** Anatoly upgrades `percolator-prog`'s Cargo.toml to `solana-program = "2.0"` and redeploys to devnet/mainnet. Mixed-version is technically fine but tooling alignment becomes valuable. Watch `https://github.com/aeyakovenko/percolator-prog/blob/master/Cargo.toml` periodically.
2. **A pin we depend on is yanked from crates.io.** If `borsh@1.5.1` or `blake3@1.5.5` etc. ever get yanked, our lockfile breaks. Migration is the structural fix.
3. **Phase 1 freight (FBX) starts.** Bundle the env upgrade with whatever else Phase 1 needs (likely a new program, new test infra). Don't migrate mid-Phase-0.
4. **A security advisory affects a pinned dep.** If `cargo audit` flags one of our pins for a vuln and the fix is in a newer version that requires edition2024, we have to migrate (or backport, which is harder).
5. **A team member can't reproduce the build.** If the toolchain divergence creates onboarding friction (new contributor on a fresh machine struggles to get cargo-build-sbf working), the migration's value increases.

---

## How to migrate (when the time comes)

Step-by-step playbook. Estimate: 30–60 min toolchain setup + 1–2 hr code/test verification, assuming no breaking API changes in `solana-program 2.x`.

### 1. Install solana CLI 2.x
```bash
sh -c "$(curl -sSfL https://release.solana.com/v2.0.x/install)"
solana --version            # should report 2.0.x
cargo-build-sbf --version   # should report rustc ≥ 1.79
```
Keep the 1.18 install around if needed (use `solana-install init <version>` to switch). Some legacy scripts in `scripts/` (upstream untouched) may pin to 1.18 conventions; verify.

### 2. Bump the matcher crate
In `programs/ballast-matcher/Cargo.toml`:
```toml
[dependencies]
solana-program = "2.0"   # was "~1.18"
```

### 3. Regenerate Cargo.lock cleanly
```bash
rm programs/ballast-matcher/Cargo.lock
cargo generate-lockfile --manifest-path programs/ballast-matcher/Cargo.toml
```
Most or all of the manual pins from the 1.18 era should drop out — solana-program 2.x's transitive set is curated for modern toolchains. Verify:
```bash
grep "block-buffer" Cargo.lock         # 0.10.x or newer is fine; 0.12.x is also fine in 2.x context
grep "borsh-derive" Cargo.lock         # 1.6.x or newer is fine if rustc ≥ 1.77
```
If any new pinning is needed, update this strategy doc with the new baseline.

### 4. Verify the build
```bash
cargo check --features no-entrypoint --manifest-path programs/ballast-matcher/Cargo.toml
cargo test  --features no-entrypoint --manifest-path programs/ballast-matcher/Cargo.toml
cargo-build-sbf --manifest-path programs/ballast-matcher/Cargo.toml
ls -la programs/ballast-matcher/target/deploy/ballast_matcher.so
```
All three must pass. Inline tests should pass without code changes (the Rust API surface we use — `AccountInfo`, `Pubkey`, `ProgramError`, `next_account_info`, `entrypoint!` — is stable across 1.x → 2.x).

### 5. Deploy a fresh program ID for canary
Do NOT upgrade the existing devnet matcher program in place across a major solana-program bump. Deploy as a new program ID, run an LP-init + zero-fill canary trade, confirm Match returns are byte-correct, then either swap or close+redeploy.

### 6. Update this doc
Roll the "May 2026 baseline pins" section to "post-migration: no manual pins required." Add a "migrated 2.x on YYYY-MM-DD" stamp at the top.

---

## Risks of migrating

- **Solana CLI 2.x has occasional breaking changes** in non-program tooling (`solana-test-validator` flags, etc.). Tests in `tests/integration.rs` may need minor adjustments.
- **`solana-program-test` 2.x crate** has had some signature changes (e.g. `BanksClient` API tweaks). Our integration tests would need re-validation.
- **`Pubkey::new` was removed** in solana-program 2.x in favor of `Pubkey::new_from_array`. We don't currently use `Pubkey::new` but worth grepping.
- **Lockfile pin culture changes.** Future contributors might not know about the 1.18-era pin gymnastics — make sure this doc + the `matcher-cargo-lock-pinning` memory note get updated.

## Risks of NOT migrating

- **Pin maintenance debt.** Each new `cargo update` requires running through the verification matrix to make sure nothing drifted into edition2024.
- **CI fragility.** When/if we add CI, the build matrix is locked to the 1.75/1.79 toolchain combo. Future security patches to those rustc versions are unlikely.
- **Newer crate features unavailable.** Some convenience features in newer borsh/serde versions (e.g. better derive macros) we can't access. Minor cost.
- **Auditor optics.** Reviewer might flag "why are you on a maintenance-line dep when the upstream reference is on the active line?" Defensible, but adds a paragraph of explanation to the audit report.

---

## Cross-references

- `programs/ballast-matcher/Cargo.lock` — the tracked lockfile.
- Project memory: `~/.claude/projects/-Users-ap-Documents-GitHub-ballast-percolator-cli/memory/matcher-cargo-lock-pinning.md` — quick reference for in-session lookups.
- `docs/prompts/MATCHER_LAYOUT_RESHAPE_HANDOFF.md` §5 — references the lockfile fix as a prerequisite for the matcher PR.
- Upstream Cargo.toml: https://raw.githubusercontent.com/aeyakovenko/percolator-prog/master/Cargo.toml (verify perp engine's solana-program version periodically).
- Upstream pmatch Cargo.toml: https://raw.githubusercontent.com/aeyakovenko/percolator-match/master/Cargo.toml (reference matcher; 2.0).
