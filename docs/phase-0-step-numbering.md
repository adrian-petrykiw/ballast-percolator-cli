# Phase 0 Step Numbering — Reconciliation Note

**Status:** Flagged during Phase 0 Step 0.0 implementation (2026-05-01).
**Audience:** anyone reading both `CLAUDE.md` and `docs/prd.md` and wondering
why "Step 0.1" means two different things.

## Two coexisting schemes

| Scheme | "Step 0.1" means | Scope of "Phase 0" |
|---|---|---|
| **CLAUDE.md** (§ "PRD Reference") | A *whole slab/use-case* (e.g. EUR/USD slab) | Treasury hedging, multi-asset |
| **PRD §4.9** (Phase 0 Execution Steps) | A *single action on the SOL/USD slab* (e.g. environment setup) | SOL/USD only |

Concretely:

- CLAUDE.md says: Phase 0 Step 0.0 = SOL/USD slab; Step 0.1 = EUR/USD slab; Step 0.2 = more FX.
- PRD §4.9 says: Step 0.1 = environment setup; 0.2 = slab deploy; 0.3 = participants; 0.4 = matcher; … 0.8 = stress.

These are not contradictions — they describe orthogonal axes (use-case rollout vs. action rollout) — but they share the prefix `Step 0.X`, which is confusing in PRs and commit messages.

## Mapping (use this when in doubt)

| Action (PRD §4.9) | SOL/USD slab (CLAUDE.md "Step 0.0") | EUR/USD slab (CLAUDE.md "Step 0.1") |
|---|---|---|
| Environment setup | §4.9 Step 0.1 | (already done) |
| Slab deployment | §4.9 Step 0.2 | new EUR/USD `setup-ballast-fx-market.ts` |
| Participant init | §4.9 Step 0.3 | reuse pattern, new wallets / context |
| Allowlist matcher | §4.9 Step 0.4 | reuse program, new context account |
| Keeper crank + logger | §4.9 Step 0.5 | extend crank-bot to new slab |
| Trade execution + SC validation | §4.9 Step 0.6 | repeat with EUR/USD economic semantics (NORMAL, not INVERTED) |
| Controlled scenarios | §4.9 Step 0.7 | repeat |
| Stress | §4.9 Step 0.8 | repeat |

## Convention going forward

- In commit messages and PR titles, prefer **PRD §4.9 step numbers**
  (e.g. `feat(phase-0): step 0.2 deploy SOL/USD slab`).
- When the slab being acted on is ambiguous, qualify it:
  `feat(phase-0/sol-usd): step 0.4 deploy allowlist matcher`.
- CLAUDE.md's slab-level numbering is fine for high-level planning but
  shouldn't appear in code or commits without the slab qualifier.

## Why not just renumber one of them

Both files have already been reviewed and merged (PRs #1, #2). Renumbering
would force a churny re-review of unchanged content. A small reconciliation
doc is cheaper than rewriting either source.

If the team prefers to collapse the two schemes, the cleanest direction is
to drop the slab-level "Step 0.X" labels from CLAUDE.md (talk about
"SOL/USD" / "EUR/USD" by name) and keep PRD §4.9's action numbering as
canonical. That's a one-paragraph CLAUDE.md edit when convenient — out of
scope for the current SOL/USD deployment PR.
