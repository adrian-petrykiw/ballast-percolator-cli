Read the PRD at `docs/prd.md`.
Identify the specific phase step referenced in $ARGUMENTS (e.g., "phase-0-step-0.6").
Read the success criteria (SC-X.X) that this step should satisfy.

For each success criterion:
1. Describe what evidence is needed to confirm it passes
2. Run the appropriate script or command to generate that evidence
3. Capture the output (transaction signatures, state dumps, error messages)
4. Compare actual vs expected results
5. Mark as PASS or FAIL with explanation

If any criterion fails:
- Diagnose the root cause
- Propose a fix
- Present the diagnosis for my approval before implementing

After all criteria are evaluated:
1. Create a validation report at `docs/reports/phase-X-step-Y-report.md`
2. Include: date, success criteria results, transaction signatures, state snapshots, any discrepancies
3. Commit: `docs(ballast): validation report for Phase X Step Y`
