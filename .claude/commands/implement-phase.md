Read the PRD at `docs/prd.md`.
Read CLAUDE.md for project conventions, directory structure, and security rules.
Identify the specific phase step referenced in $ARGUMENTS (e.g., "phase-0-step-0.0" or "phase-0-step-0.4").
Read the corresponding section in the PRD including:
- Entry criteria (what must be complete before starting)
- Actions (numbered implementation steps)
- Exit criteria (what must be true when done)
- Success criteria (SC-X.X identifiers)

Check that entry criteria are satisfied by examining repo state and `config/ballast-config.json`.

Plan the implementation approach:
1. List every file that needs to be created or modified
2. Identify any dependencies (deployed programs, funded wallets, running services)
3. Flag any blocking prerequisites that aren't met
4. Estimate which success criteria this step will satisfy

Present the plan for my approval before writing any code.
After I approve, implement the changes following these rules:
- All new scripts go in `scripts/ballast/`
- All new tests go in `tests/ballast/`
- Every script has a header comment explaining purpose, prerequisites, and PRD reference
- Use `config/ballast-config.json` for all pubkeys — never hardcode
- Never modify upstream percolator files in `scripts/` or `src/`

After implementation:
1. Run `pnpm build` to verify TypeScript compilation
2. Run `pnpm test` if any test files were added
3. If both pass, create a commit: `feat(ballast): <description> [PRD Phase X Step Y]`
4. If either fails, fix issues and re-run before committing
