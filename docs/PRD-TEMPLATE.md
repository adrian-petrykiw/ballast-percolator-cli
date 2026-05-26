# PRD: [Feature Name]

> **Status:** Draft | In Review | Approved | In Progress | Shipped | Archived
> **Owner:** [name]
> **Created:** [YYYY-MM-DD]
> **Last updated:** [YYYY-MM-DD]

---

## Problem

What situation or gap does this address? Who is affected and how? One paragraph, no jargon. If you can't explain the problem clearly here, the solution will be wrong.

## Scope

What we are building. Be specific about the boundaries — what is in scope for this iteration.

## Non-goals

What we are explicitly NOT doing in this iteration, and why. Non-goals prevent scope creep and make tradeoffs legible to future readers.

## Success criteria

Numbered, measurable, verifiable. Each criterion should be falsifiable — if you can't tell in five minutes whether it passed or failed, rewrite it.

- SC-1: [measurable outcome]
- SC-2: [measurable outcome]
- SC-3: [measurable outcome]

## Technical approach

### Architecture / design decisions

What are the key structural choices? What alternatives were considered and rejected?

### Data flow

If applicable: how does data move through this feature? Diagram if complex.

### Dependencies

External services, on-chain programs, APIs, or internal modules this feature depends on. Note whether each dependency is already available or needs to be built/integrated.

### Risks and mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| [risk] | High / Med / Low | High / Med / Low | [how we handle it] |

## Rollout / implementation phases

Break the work into phases small enough to ship and validate independently. Each phase should leave the system in a working state.

- **Phase 1:** [description] — validates SC-X
- **Phase 2:** [description] — validates SC-Y
- **Phase 3:** [description] — validates SC-Z

## Validation

How do we know this shipped correctly? Reference the success criteria and specify where validation results are documented (e.g., `docs/reports/phase-X-step-Y-report.md`).

## Open questions

Questions that are unresolved at the time of writing. Each should be assigned to someone and have a target resolution date.

- [ ] [question] — owner: [name], resolve by: [YYYY-MM-DD]
