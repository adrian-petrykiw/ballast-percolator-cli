# Claude Code Repo Scaffold — Operating Model

How a CargoBill repository is **structured** so that a human and Claude Code can
work it together: the instruction file, the docs information architecture, the
`.claude/` operating model, the PRD lifecycle, and the multi-session continuity
pattern. This is the **operating-model** half of the baseline.

Its sibling, [`claude-setup-rubric.md`](claude-setup-rubric.md), is the
**security** half — the six trust-boundary layers that stop an agent or a
dependency from doing damage. The two are designed to be read together:

| Concern                                         | Doc                                                                    | Answers                                                                                                        |
| ----------------------------------------------- | ---------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| **Security** — what stops harm                  | [`claude-setup-rubric.md`](claude-setup-rubric.md)                     | Can the agent push/deploy/leak? Can a bad dep execute or merge?                                                |
| **Security rollout** — copying it elsewhere     | [`security-baseline-starter-kit.md`](security-baseline-starter-kit.md) | What do I copy vs. rewrite per stack?                                                                          |
| **Operating model** — how work flows (this doc) | `claude-repo-scaffold.md`                                              | How is the repo laid out so a human + agent can plan → build → validate → ship → archive across many sessions? |

Together they are the full CargoBill baseline: **operating model + security**.
The rubric tells you which gates to build; this doc tells you how the repo is
shaped around the agent once those gates exist. They overlap deliberately at one
seam — the rubric's _Layer 6 (Documentation)_ names the doc set; **this doc is
the expansion of that layer into a working operating model.**

> **The goal: one doc + one prompt.** Just as the rubric + starter kit let you
> harden a new repo from a blueprint plus an extraction plan, this doc + the
> [kickoff prompt at the end](#scaffolding-a-new-repo--one-doc--one-prompt) let
> you scaffold a new repo's operating model in a single Claude Code session.
> Every part below pairs the _general pattern_ with the **Ballast worked
> example** so you can copy the shape and see the decision that was already made.

---

## The shape of a Claude-ready repo

The four things a new repo needs before agent-assisted work begins. Everything in
this doc elaborates one of these:

```
repo-root/
├── CLAUDE.md            # The instruction file the agent loads every session.
│                        #   Lean, actionable: overview + workflow rule +
│                        #   permission model + conventions. NOT a reference dump.
├── .claude/            # The operating model the agent runs under.
│   ├── settings.json    #   permissions (allowlist-first) + hooks + sandbox  → rubric L1
│   ├── settings.local.json.example   #   per-developer override template
│   ├── hooks/           #   PreToolUse guardrail (block-mutating-commands.mjs) → rubric L1
│   ├── commands/        #   the workflow commands that drive the PRD lifecycle
│   ├── agents/          #   (empty until needed) custom subagents
│   ├── rules/           #   (empty until needed) extracted always-on rules
│   └── skills/          #   (empty until needed) packaged multi-step skills
├── docs/               # The information architecture (Part 2).
│   ├── PRD-TEMPLATE.md  #   canonical PRD shape
│   ├── prd.md           #   the live master PRD
│   ├── architecture.md  #   system/stack reference (extracted out of CLAUDE.md)
│   ├── runbook.md       #   incident response
│   ├── claude-setup-rubric.md / security-baseline-starter-kit.md / this file
│   ├── reports/         #   per-step validation reports (auditable claims)
│   ├── archive/         #   shipped PRDs, retired
│   └── prompts/         #   kickoff + handoff docs for multi-session continuity
└── config/             # PUBLIC config only (addresses, never secrets)  → project-specific
```

The split is the whole idea: **`CLAUDE.md` is instructions** (loaded every
session, kept short), **`docs/` is reference and record** (pulled on demand),
**`.claude/` is the operating model** (enforced, not just described).

---

## Part 1 — The CLAUDE.md skeleton

`CLAUDE.md` is the one file the agent reads at the start of every session, so it
must be lean and actionable — overview, the rules the agent must obey, and the
conventions for new code. Anything that is _reference_ (full protocol/stack
details, byte layouts, command catalogs) belongs in
[`architecture.md`](architecture.md), linked from here.

The skeleton below marks every section as **`[GENERAL]`** (keep ~verbatim across
CargoBill repos), **`[PROJECT]`** (rewrite for this repo), or **`[MIXED]`** (keep
the frame, swap the specifics). The two `[GENERAL]` blocks — the **workflow
rule** and the **permission model** — are the heart of the operating model and
should change only in their command lists, never in their posture.

```markdown
# CLAUDE.md

## Project Overview [PROJECT]

One paragraph: what this is, who it's for, what stage (POC/prod), the
hard boundary (e.g. "devnet-only, no real funds, no mainnet deploy").

## Workflow Rule — Claude Does Not Run Commits/Pushes/PRs [GENERAL]

The agent never mutates shared state. For any commit / push / tag / PR /
deploy / on-chain write, it emits ONE copy-pasteable fenced bash block
(heredoc'd message, Co-Authored-By trailer) and waits for the human.
It does NOT retry after a denial.
• "Applies to" list → [PROJECT]: add this repo's deploy/publish/write CLIs
• "Free (read-only)" list [GENERAL]
• "Staging is fine" (targeted `git add`, never `add -A`) [GENERAL]
• "Enforcement" → names the PreToolUse hook [GENERAL]

## Permission Model [GENERAL]

Allowlist-first. settings.json `allow` auto-runs normal dev work;
`deny` hard-blocks catastrophic forms; the hook covers what globs can't;
everything else prompts. `autoAllowBashIfSandboxed: false`.
• the specific allow/deny patterns [PROJECT]

## Upstream / Stack Reference [PROJECT]

What this repo forks/builds on, target versions, runtime. Point to
architecture.md for the full reference — keep this short.

## Key Data Flow — the pattern every script follows [PROJECT]

The canonical Config → Context → State → … → Execute chain for this repo.

## Domain Constants / On-chain Layout [PROJECT]

The few magic numbers/offsets that must never drift. Full table → architecture.md.

## Critical Constraints [PROJECT]

The 4–6 footguns specific to this stack (overflow types, staleness gates,
wire-format pinning, anti-spam requirements …).

## Project Structure — where new code goes [PROJECT]

The dedicated dirs for this repo's code vs. untouched upstream/vendored.

## Coding Conventions [MIXED]

Conventional commits, branch naming, Co-Authored-By trailer [GENERAL]
Language style, numeric types, the data-flow pattern, header comments [PROJECT]

## Security Rules [MIXED]

Never read .env\*/secrets/keypairs; never hardcode keys; never commit
secrets; separate keys per role; public-config-only. [GENERAL]
The exact paths/mints/role files [PROJECT]

## Event Logging [MIXED]

Append-only JSONL of state-changing ops; GUARDRAIL_DECISION auto-logged
by the hook; INCIDENT during the runbook loop. [GENERAL]
The event_type enum for this domain [PROJECT]

## Testing [MIXED]

Unit (offline, in CI) vs integration (networked, never in CI) split;
no snapshots for money math; reports per shipped step. [GENERAL]
The runners, dirs, and what to test/not-test [PROJECT]

## Important Notes [PROJECT]

Operational gotchas (faucet limits, feed cadence, cooldowns …).
```

**Ballast worked example ✅** — [`CLAUDE.md`](../CLAUDE.md) is exactly this
skeleton filled in: a devnet-POC overview; the verbatim workflow rule (with
`solana`/`spl-token`/`cargo publish` added to "Applies to"); the allowlist-first
permission model; the Percolator data-flow pattern; the v12.21 layout constants
(full reference deferred to `architecture.md`); the `bigint`/oracle-staleness
constraints; the `scripts/ballast/` etc. structure; and the `events.jsonl`
schema. The reference material that _used_ to live in `CLAUDE.md` was extracted
into [`architecture.md`](architecture.md) precisely to keep the instruction file
lean — see Part 2.

---

## Part 2 — The docs/ information architecture

Each file in `docs/` has one job and one audience. The discipline is: **`CLAUDE.md`
is instructions, `architecture.md` is reference, `runbook.md` is for incidents,
the PRD is the plan, `reports/` is the record, `archive/` is history, `prompts/`
is continuity.** Don't blur them — a fat instruction file gets skimmed and a
reference doc buried in incident procedures gets lost.

| Path                                                                   | What it's for                                                                                                                                                                                                        | General / Project                    | When it changes                    |
| ---------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------ | ---------------------------------- |
| [`PRD-TEMPLATE.md`](PRD-TEMPLATE.md)                                   | Canonical PRD shape: Problem → Scope → Non-goals → **Success criteria (SC-N, falsifiable)** → Technical approach → Phases → Validation → Open questions.                                                             | **General**                          | Rarely — it's the mold.            |
| `prd.md`                                                               | The live master PRD: phases, steps, and the SC-X.Y identifiers that everything downstream references. **Read first for any implementation work.**                                                                    | Project                              | Every planning cycle.              |
| [`architecture.md`](architecture.md)                                   | The system/stack reference extracted out of `CLAUDE.md` — full command catalog, source layout, byte offsets, protocol concepts, design decisions.                                                                    | Project (frame is general)           | When the stack/upstream changes.   |
| [`runbook.md`](runbook.md)                                             | Incident response: a standard STOP→ASSESS→CONTAIN→RECOVER→RECORD→POSTMORTEM loop, then one entry per failure mode, plus a pre-deploy checklist and postmortem template.                                              | **General frame**, project incidents | When a new failure class is found. |
| [`supply-chain-hardening.md`](supply-chain-hardening.md)               | What supply-chain controls are implemented, which advisories are accepted (with rationale + resolution trigger), what's deferred.                                                                                    | General frame, project entries       | On each dep/audit decision.        |
| [`claude-setup-rubric.md`](claude-setup-rubric.md)                     | The security blueprint (6 layers).                                                                                                                                                                                   | **General**                          | Stable.                            |
| [`security-baseline-starter-kit.md`](security-baseline-starter-kit.md) | The per-stack rollout kit for the rubric.                                                                                                                                                                            | **General**                          | Stable.                            |
| `claude-repo-scaffold.md` (this)                                       | The operating-model blueprint.                                                                                                                                                                                       | **General**                          | Stable.                            |
| `reports/`                                                             | One validation report per shipped step (`phase-X-step-Y-report.md`): SC results, tx signatures, state snapshots, discrepancies. Makes every "it shipped" claim auditable. Also holds incident snapshots/postmortems. | Project                              | Every validated step / incident.   |
| `archive/`                                                             | Shipped PRDs, moved here when fully delivered, so `prd.md` stays the _live_ plan and history is preserved (not deleted).                                                                                             | Project                              | When a PRD ships.                  |
| `prompts/`                                                             | Kickoff + handoff docs that carry context across sessions (Part 5).                                                                                                                                                  | Project                              | Every multi-session effort.        |

**Ballast worked example ✅** — all of the above exist and are populated:
`prd.md` defines Phases 0–1 with SC-0.1…SC-1.8; `architecture.md` holds the full
Percolator reference; `runbook.md` catalogs 9 incidents; `reports/` and
`archive/` carry `.gitkeep` placeholders until work fills them; `prompts/` holds
nine kickoff/handoff pairs from the matcher and hardening efforts.

---

## Part 3 — The .claude/ operating model

`.claude/` is where the operating model is **enforced and packaged**, not merely
described. Four parts are live in Ballast; three directories are intentionally
empty, waiting for a real need.

### Guardrails — `settings.json` + `hooks/` _(see rubric Layer 1)_

The security mechanics — allowlist-first `permissions`, the hard-deny block, the
`block-mutating-commands.mjs` PreToolUse hook, the sandbox egress list, and
`settings.local.json.example` — are **fully specified in
[`claude-setup-rubric.md` → Layer 1](claude-setup-rubric.md#layer-1--guardrails)**.
This doc does not duplicate them; it only notes their role in the operating
model: **they are what make the workflow rule real.** The rule "the agent emits a
bash block instead of pushing" is a _convention_ in `CLAUDE.md` and an
_enforcement_ in the hook. Both must exist — the doc explains the why so nobody
disables the hook out of confusion.

### Workflow commands — `commands/`

Slash commands that encode the PRD lifecycle as repeatable prompts, so each stage
is run the same way every time. Ballast ships four, and they map onto the
lifecycle in Part 4:

| Command                                                           | Stage        | What it does                                                                                                                                                                          |
| ----------------------------------------------------------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [`review-prd-section`](../.claude/commands/review-prd-section.md) | **Plan**     | Stress-tests a PRD section for ambiguity, hidden dependencies, edge-case gaps, and conflicts with upstream behavior. Run _before_ implementing.                                       |
| [`implement-phase`](../.claude/commands/implement-phase.md)       | **Build**    | Reads the step's entry/actions/exit/SC, checks entry criteria, presents a file-by-file plan **for approval**, then implements under the repo's code-placement rules and builds/tests. |
| [`validate-step`](../.claude/commands/validate-step.md)           | **Validate** | For each SC: states the evidence needed, runs it, captures tx signatures/state, marks PASS/FAIL, and writes the report to `reports/`.                                                 |
| [`create-pr`](../.claude/commands/create-pr.md)                   | **Ship**     | Scans the diff for staged secrets/keypairs, writes a PR description keyed to the SC identifiers, and forms the PR command.                                                            |

⚠️ **The commit/PR step is human-run by design.** `implement-phase` and
`validate-step` end with a "create a commit" instruction and `create-pr` forms a
`gh pr create` — but the **workflow rule + the hook override that tail**: the
agent _emits the bash block_ and the human runs it. Treat the command files as
the description of the full lifecycle; the guardrail decides who pushes the
button. In particular, **Claude must not invoke the `create-pr` skill itself** —
it emits the equivalent `gh pr create` block. (When porting these commands to a
new repo, point `--base` at that repo's integration branch — see Part 6.)

A command file is just a Markdown prompt with optional `$ARGUMENTS`. To add one,
drop `verb-noun.md` in `commands/` describing the steps; it appears as
`/verb-noun`.

### The three empty directories — `agents/`, `rules/`, `skills/`

Present but empty in Ballast. Each has a clear "reach for this when…" trigger;
until then, leaving them empty keeps the operating model legible.

| Dir       | What it holds                                                                                                | Reach for it when…                                                                                                                                                                                  | Ballast status                                           |
| --------- | ------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------- |
| `agents/` | Custom **subagent** definitions (name, system prompt, tool allowlist, model) the main agent can dispatch to. | A task recurs as an isolated, well-scoped job worth its own constrained context and tool set — e.g. a read-only "audit-allowlist triage" agent, or a "PRD section reviewer" with no write tools.    | Empty — the 4 commands + main agent suffice so far.      |
| `rules/`  | Always-on rule snippets, factored out so `CLAUDE.md` stays lean and rules are reusable across repos.         | A `CLAUDE.md` section grows long, or the _same_ rule (e.g. the workflow rule) should be shared verbatim across repos.                                                                               | Empty — rules still inline in `CLAUDE.md`.               |
| `skills/` | Packaged multi-step **skills** (a folder with `SKILL.md` + optional scripts) the agent invokes by name.      | A multi-step procedure is stable enough to package with its own helper scripts — e.g. "deploy-and-verify-slab" bundling the pre-deploy checklist, the deploy block, and the post-deploy validation. | Empty — procedures still live as runbook/commands prose. |

**Rule of thumb:** start with everything inline in `CLAUDE.md` + a few
`commands/`. Promote to `rules/` when a section is shared or bloated, to
`agents/` when a job wants an isolated context/tool set, and to `skills/` when a
procedure is stable enough to package with scripts. Premature extraction just
adds indirection.

---

## Part 4 — The PRD → implement → validate → report → archive lifecycle

The operating model is a loop, and the four workflow commands are its stages. The
PRD's falsifiable success criteria (SC-X.Y) are the through-line: they're written
in planning, satisfied in build, checked in validation, cited in the PR, and
preserved in the archived PRD.

```
   ┌─────────────────────────────────────────────────────────────────────┐
   │                                                                       │
   ▼                                                                       │
PRD (prd.md, from PRD-TEMPLATE.md)                                         │
   │   defines Phase X / Step Y with success criteria SC-X.Y               │
   │                                                                       │
   ├─▶ /review-prd-section   "is this step specified well enough to build?"│
   │        └─ findings, gaps, conflicts → tighten the PRD                 │
   │                                                                       │
   ├─▶ /implement-phase      check entry criteria → plan (approve) → build │
   │        └─ new code in the repo's dedicated dirs; build + unit tests   │
   │                                                                       │
   ├─▶ /validate-step        run each SC → PASS/FAIL with evidence         │
   │        └─ writes docs/reports/phase-X-step-Y-report.md                │
   │                                                                       │
   ├─▶ /create-pr            secret-scan diff → PR body keyed to SC-X.Y    │
   │        └─ (agent emits the gh block; human runs it)  → feature → dev  │
   │                                                                       │
   └─▶ when the whole PRD ships: move it to docs/archive/ ────────────────┘
            prd.md stays the LIVE plan; history is preserved, not deleted
```

Each pass leaves the repo in a working, validated, auditable state. The
`reports/` entry is the durable proof a step met its criteria; the `archive/`
move is what keeps `prd.md` honest about what's still in flight. This is the
operating-model counterpart to the rubric's "a layer isn't done until its check
is required in CI" — **a step isn't done until its report shows every SC PASS.**

---

## Part 5 — Multi-session continuity: kickoff + handoff

Agent context does not survive a new conversation, so a repo worked across many
sessions needs an explicit way to carry state. CargoBill uses a **three-legged**
pattern; the legs are redundant on purpose.

1. **Handoff doc** (`docs/prompts/<EFFORT>_HANDOFF.md`) — the full
   state-of-the-world written at the _end_ of a session: what was done (file
   table), what the next session must do (numbered steps), **decisions locked (do
   not re-litigate)**, and an embedded kickoff prompt. This is the authoritative
   briefing.
2. **Kickoff prompt** (`docs/prompts/<EFFORT>_KICKOFF.md`) — a short, paste-able
   first message for the _next_ session. It is a **read-in-this-order file
   pointer list**, not a context dump: "read the handoff, then `CLAUDE.md`, then
   PRD §X, then these sources — then propose a plan; don't start coding until we
   agree on Y." Keeping it a pointer list keeps the prompt short and lets the new
   agent pull only what it needs.
3. **Memory notes** (`~/.claude/projects/<id>/memory/`) — durable cross-session
   facts and locked decisions, indexed in `MEMORY.md`. The handoff is per-effort;
   memory is the long-lived "do not re-litigate" store that outlives any one
   effort.

The discipline that makes this work: **record every locked decision exactly
once** (in the handoff's "decisions" section and/or a memory note) so it is never
re-argued, and **convert relative dates to absolute** so a note read months later
still parses.

**Ballast worked example ✅** — `docs/prompts/` holds matching `_KICKOFF` /
`_HANDOFF` pairs for the matcher PR, the matcher TS/layout reshape, and the PR
work; [`handoff-layer4.md`](handoff-layer4.md) carried the hardening effort and
embeds its own next-session kickoff verbatim; and the
`tooling-hardening-session-state.md` memory note records the locked decisions so
no session re-litigates pnpm v10, the axios allowlist, or the ESLint `project`
choice. The general kickoff that bootstrapped the whole repo is
[`prompts/KICKOFF_PROMPT.md`](prompts/KICKOFF_PROMPT.md).

> A handoff/kickoff pair is the _per-effort_ analog of this whole doc: this doc
> kicks off a **repo**; a handoff kicks off the **next session** of an effort
> inside it.

---

## Part 6 — Conventions

The small, fixed rules that keep history and review legible. All **general** —
carry them to every CargoBill repo unchanged.

- **Conventional commits** — `feat:` `fix:` `chore:` `refactor:` `docs:` `test:`,
  optional scope (Ballast uses `(ballast)`). PR titles use the same format.
- **Branch naming** — `feat/`, `fix/`, or `chore/` + short kebab-case
  (`feat/phase-0-allowlist-matcher`, `chore/tooling-rubric`). One logical change
  per branch.
- **`Co-Authored-By: Claude Code <noreply@anthropic.com>` trailer** on every
  agent-assisted commit — required for the audit trail in regulated financial
  software. Multi-line messages use a heredoc so quoting/newlines survive.
- **Branch flow: feature → `dev` → `master`.** CI runs at every hop. All feature
  and Dependabot branches target `dev`; `dev → master` is the promotion PR.
  Point each workflow command's PR `--base` at the integration branch
  (`dev`), not `master`.
- **Branch protection** — `master`: required `ci` status check + "require
  branches up to date" (strict) + `enforce_admins: true` + force-pushes off.
  `dev`: lighter — required `ci`, no up-to-date requirement, admins may bypass.
  Feature/Dependabot branches unprotected by design (their PRs still run `ci`).
  ⚠️ The required-status-check toggle is a one-time manual step in GitHub →
  Settings → Branches once the first `ci` run is green — it can't be set via `gh`
  without an admin token.
- **The agent never runs the state-changing step.** Commit / push / PR / tag /
  deploy / on-chain write are emitted as fenced bash blocks for the human; the
  hook enforces it (Part 3 / rubric Layer 1).

Full security-side conventions (SHA-pinned actions, frozen-lockfile install,
audit gate) live in [`claude-setup-rubric.md`](claude-setup-rubric.md) Layers
2 & 5.

---

## Scaffolding a new repo — one doc + one prompt

This doc is the blueprint; the prompt below is the bootstrap. Paste it into a
fresh Claude Code session in the new (or empty) repo. It scaffolds the
**operating model**; pair it with the rubric + starter-kit rollout (security) so
a new repo gets both halves of the baseline.

**Order of operations** (operating model interleaves with the rubric's
[quick-start order](claude-setup-rubric.md#quick-start-order-for-a-new-repo)):

1. **Guardrails first** (rubric Layer 1) — hook + `settings.json` before any
   other agent-assisted work, so the operating model's "agent doesn't push" rule
   is enforced from commit zero.
2. **`CLAUDE.md`** from the Part 1 skeleton — fill `[PROJECT]` slots, keep
   `[GENERAL]` blocks verbatim.
3. **`docs/` skeleton** (Part 2) — `PRD-TEMPLATE.md`, `architecture.md`,
   `runbook.md`, empty `reports/`, `archive/`, `prompts/`. Copy this doc + the
   rubric + starter kit in verbatim (they're general).
4. **`.claude/commands/`** (Part 3) — the four workflow commands, `--base` set to
   this repo's integration branch.
5. **Conventions + branch protection** (Part 6) — create `dev`, set protection on
   `dev` and `master`.
6. Run the **first PRD cycle** (Part 4) to validate the loop end-to-end.

```
Scaffold this repo's Claude Code operating model. Read, in this repo, in
order: docs/claude-repo-scaffold.md (the operating-model blueprint),
docs/claude-setup-rubric.md (the security blueprint), and
docs/security-baseline-starter-kit.md (the per-stack rollout kit). If they
aren't present, pull them from the CargoBill baseline.

Then, treating this repo's stack as <STACK> and product as <PRODUCT>:

1. Confirm guardrails exist (rubric Layer 1: settings.json + the
   block-mutating-commands hook). If not, scaffold them FIRST and stop for
   my review before anything else.
2. Draft CLAUDE.md from the Part-1 skeleton: keep the [GENERAL] workflow
   rule and permission model verbatim; fill every [PROJECT]/[MIXED] slot
   for <STACK>/<PRODUCT>. Move any reference material into
   docs/architecture.md.
3. Create the docs/ information architecture from Part 2: PRD-TEMPLATE.md,
   architecture.md, runbook.md (incident frame + this stack's failure
   modes), and empty reports/, archive/, prompts/.
4. Create the four workflow commands in .claude/commands/ (Part 3), with
   each PR command's --base pointing at `dev`.
5. Lay out the conventions and branch flow from Part 6 (feature → dev →
   master; Co-Authored-By trailer; branch-protection plan).

Follow the workflow rule throughout: do NOT commit, push, or open PRs
yourself — propose the files, then emit copy-pasteable git/gh bash blocks
for me to run. Leave agents/, rules/, skills/ empty until a real need
appears (Part 3 triggers). Present a file-by-file plan for my approval
before writing anything.
```

---

## The full baseline, at a glance

| Layer of the baseline           | Doc                                                                    | Role                                                                                                |
| ------------------------------- | ---------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| **Operating model — blueprint** | `claude-repo-scaffold.md` (this)                                       | How the repo is shaped so a human + agent plan → build → validate → ship → archive across sessions. |
| **Security — blueprint**        | [`claude-setup-rubric.md`](claude-setup-rubric.md)                     | The six trust-boundary layers that stop harm.                                                       |
| **Security — rollout**          | [`security-baseline-starter-kit.md`](security-baseline-starter-kit.md) | What to copy vs. rewrite per stack.                                                                 |
| **Per-repo bootstrap**          | the kickoff prompt (above) + the rubric's quick-start order            | The "one prompt" that turns the blueprints into a scaffolded repo.                                  |

A repo is baseline-complete when **both** halves hold: every security gate is
_required in CI_ (rubric), **and** the operating model is in place — a lean
`CLAUDE.md`, the docs IA, the workflow commands, and a first PRD cycle that ran
the loop end-to-end (this doc).
