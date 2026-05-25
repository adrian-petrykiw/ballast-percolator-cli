# Supply Chain Hardening Plan

> **Status:** PLANNED. Not yet implemented. This is the working backlog for a follow-up commit/PR — owned by the same person who reads it.

## Why this exists

The PreToolUse hook (`.claude/hooks/block-mutating-commands.mjs`), sandbox, and `permissions.deny` together control **what the agent can do** inside this repo. They do not control **what we install**. Given the cadence of npm and crate compromises through 2025–2026 (Shai-Hulud worm, ua-parser-js maintainer hijack, rspack incident, tj-actions/changed-files compromise, multiple Solana-ecosystem typosquats), a single malicious dep can:

- Exfiltrate devnet keypairs from `~/.config/ballast/`
- Modify the matcher's deployed Solana program binary at build time
- Pivot to mainnet infrastructure once mainnet keys land on the same machine
- Plant persistence in dev tooling that survives across projects

This is the second-most-important security work after agent guardrails, and it's about to become the *most* important: mainnet keys, mainnet RPC, and production data are landing in this repo soon (per CargoBill product timeline). The current "pnpm install and hope" posture is below the bar for financial software in production.

## Scope

Devnet + (imminent) mainnet. Same machine, same dev workflow. The matcher program will eventually be deployed to mainnet with real fee economics; supply-chain trust failures there have direct financial consequences and reputational consequences that long outlast the bug.

## Implementation backlog (cost-ordered)

### Tier 1 — must do, near-zero cost

#### 1. Block postinstall script execution

Add to `.npmrc` at repo root:

```
ignore-scripts=true
```

**Why.** ~70% of recent npm-worm attacks (Shai-Hulud, ua-parser-js) use `postinstall` for RCE. This single switch closes the entire class.

**Tradeoff.** A few packages legitimately need install scripts — `esbuild`, `sharp`, `bcrypt`, `node-gyp`-using crypto libs. Allowlist those explicitly via pnpm's `onlyBuiltDependencies` config in `package.json`:

```json
{
  "pnpm": {
    "onlyBuiltDependencies": ["esbuild", "sharp"]
  }
}
```

**Verify.** `pnpm install` completes; no postinstall hooks ran for non-allowlisted packages. Run a known-good package (e.g., `esbuild`) to confirm it still builds.

#### 2. `pnpm audit` in CI

Add a job to `.github/workflows/ci.yml`:

```yaml
- name: pnpm audit
  run: pnpm audit --audit-level=high
```

**Why.** Catches known CVEs in the dep tree. Reactive only (won't catch 0-days like Shai-Hulud) but cheap baseline.

**Tradeoff.** Will fail builds on newly-disclosed CVEs in transitive deps you can't immediately fix. Use `pnpm.overrides` for emergency pins; track via Dependabot.

#### 3. `cargo audit` in CI

Same workflow file:

```yaml
- name: cargo audit
  run: cargo install cargo-audit && cargo audit --deny warnings
```

**Why.** Same as #2, for the Rust side. The matcher's transitive deps via `solana-program 1.18` are real attack surface (per memory note `matcher-cargo-lock-pinning.md`).

#### 4. Pin GitHub Actions to commit SHAs

Replace mutable tag refs throughout `.github/workflows/`:

```yaml
# Before
- uses: actions/checkout@v4

# After
- uses: actions/checkout@b4ffde65f46336ab88eb53be808477a3936bae11  # v4.1.1
```

**Why.** Tag mutability is a documented attack class (tj-actions/changed-files compromise, March 2025). Pinning to SHA forces explicit upgrade review and prevents silent malicious replacement.

**Tool.** `pinact` or `ratchet` to automate. Dependabot understands SHA-pinned actions and will PR upgrades.

### Tier 2 — high value, low cost

#### 5. Socket.dev GitHub App

Install: <https://socket.dev/install>

**Why.** Behavioral risk analysis on every dep PR. Flags packages that newly use network calls, filesystem access, shell exec, env-var reads, or eval. Catches *novel* attacks that audit databases don't yet know about — exactly the Shai-Hulud failure mode where the malicious code is in versions the CVE database hasn't seen.

**Cost.** Free for OSS. ~$40/mo for private repos at this size. Worth it.

**Setup.** Install the GitHub App, configure thresholds in `.socketrc`, require the Socket check to pass before merge.

#### 6. Dependabot version updates

Enable in repo Settings → Code security → Dependabot. Configure `.github/dependabot.yml`:

```yaml
version: 2
updates:
  - package-ecosystem: npm
    directory: /
    schedule: { interval: weekly }
  - package-ecosystem: cargo
    directory: /programs/ballast-matcher
    schedule: { interval: weekly }
  - package-ecosystem: github-actions
    directory: /
    schedule: { interval: weekly }
```

**Why.** Keeps deps current. Most successful supply-chain attacks land via stale versions of compromised packages — the newer version has the malicious code removed, but you're still pinned to the bad one.

#### 7. Verify lockfile integrity in CI

```yaml
- name: pnpm install (locked)
  run: pnpm install --frozen-lockfile
```

**Why.** Prevents lockfile mutation during install. Required for reproducible builds and supply-chain forensics. Also catches drift between `package.json` and `pnpm-lock.yaml`.

### Tier 3 — defense in depth, moderate cost

#### 8. `cargo deny` for license + advisory enforcement

Add `deny.toml` at repo root with policies for advisories, licenses, sources. Run in CI:

```yaml
- run: cargo install cargo-deny && cargo deny check
```

**Why.** Programmatic supply-chain policy. Fails the build on disallowed crates (e.g., GPL-licensed when we ship MIT, sources outside crates.io, known-vulnerable transitive deps).

#### 9. SBOM generation in CI

Generate via `cyclonedx-npm` (Node) and `cargo-sbom` (Rust). Upload as build artifact, attest with GitHub's built-in provenance support.

**Why.** Required for SOC2 / ISO 27001 / FedRAMP compliance frameworks. Useful even pre-compliance for "what is actually in our build?" — the answer is non-trivial once transitive deps are counted.

#### 10. Restrict npm/cargo to specific registries

`.npmrc`:
```
registry=https://registry.npmjs.org/
```

`.cargo/config.toml`:
```toml
[source.crates-io]
replace-with = "vendored-sources"  # or explicit registry
```

**Why.** Blocks typosquat-via-registry-mirror attacks. Forces all deps through the official registries we audit.

### Tier 4 — enterprise, defer until mainnet shipped

#### 11. `cargo vet` distributed crate audits

Heavy setup; requires ongoing maintenance of `supply-chain/audits.toml`. Used by Mozilla, Google, etc.

**Why.** Cryptographically-attested code reviews of every crate in the dep tree. The state-of-the-art for Rust supply-chain trust.

**Defer.** Until mainnet ships and there's organizational capacity. Pre-mainnet is too early.

#### 12. Internal npm/cargo mirror

Run a private Verdaccio or JFrog Artifactory mirror.

**Why.** Authoritative control over what packages can be installed. Required for some compliance regimes.

**Defer.** Premature for a single-repo POC; only relevant once Ballast has multiple repos and shared infrastructure.

#### 13. Vendored dependencies

Fork critical deps into the repo (e.g., the matcher's most critical Solana crates).

**Why.** Eliminates registry-trust entirely for the most security-critical deps. The xz-utils backdoor showed that even multi-year-trusted maintainers can flip.

**Defer.** Until there's a list of "irreplaceable trust" deps and the manpower to maintain forks.

## Agent-side overlap (already in PreToolUse hook v1)

The committed `.claude/hooks/block-mutating-commands.mjs` already blocks the agent from running:

- `pnpm install`, `pnpm i`, `pnpm add`, `pnpm update`, `pnpm upgrade`, `pnpm dlx`
- `npm install`, `npm i`, `npm add`, `npm update`, `npm exec`, `npm ci`
- `yarn install`, `yarn add`, `yarn upgrade`, `yarn dlx`
- `cargo install`, `cargo add`, `cargo remove`, `cargo update`, `cargo upgrade`

The agent can edit `package.json` and `Cargo.toml`; the human runs install. This preserves a human-review gate for every dep change — the foundation of supply-chain trust. Tier 1.1 above (`ignore-scripts=true`) closes the most common bypass even if a malicious package slips past human review.

## Order of implementation

1. **Now (committed in current PR):** Hook denies install commands. This file written for tracking.
2. **Next commit (separate PR):** Tier 1 — `.npmrc ignore-scripts`, `pnpm audit` + `cargo audit` in CI, Action SHA pinning.
3. **Within two weeks of #2:** Tier 2 — Socket.dev install, Dependabot config, lockfile CI gate.
4. **Before any mainnet code path:** Tier 3 — `cargo deny`, SBOM in CI, registry restriction.
5. **Post-mainnet, ongoing:** Tier 4 — `cargo vet`, internal mirror, vendoring (case-by-case).

## References

- [Shai-Hulud npm worm](https://socket.dev/blog/shai-hulud-the-novel-self-replicating-worm-infecting-hundreds-of-npm-packages) — Sep 2024
- [tj-actions/changed-files compromise](https://www.wiz.io/blog/tj-actions-supply-chain-attack) — Mar 2025
- [Socket.dev](https://socket.dev) — behavioral package analysis (the tool you specifically asked about)
- [cargo-audit](https://github.com/RustSec/rustsec/tree/main/cargo-audit)
- [cargo-deny](https://github.com/EmbarkStudios/cargo-deny)
- [cargo-vet](https://github.com/mozilla/cargo-vet)
- [OpenSSF best practices for npm](https://openssf.org/blog/2022/09/01/npm-best-practices-for-supply-chain-attacks/)
- [pnpm onlyBuiltDependencies docs](https://pnpm.io/package_json#pnpmonlybuiltdependencies)
- [GitHub Actions SHA pinning rationale](https://docs.github.com/en/actions/security-guides/security-hardening-for-github-actions#using-third-party-actions)
