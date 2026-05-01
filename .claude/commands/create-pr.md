Run `git log --oneline main..HEAD` to see all commits on this branch.
Run `git diff main --stat` to see changed files.

Verify no sensitive files are staged:
- Run `git diff main --name-only | grep -i keypair` — must return nothing
- Run `git diff main --name-only | grep -i '\.env'` — must return nothing
- Run `git diff main --name-only | grep -i secret` — must return nothing

If any sensitive files are found, STOP and alert me immediately.

Write a clear PR description summarizing:
1. What was implemented and why (reference the PRD phase/step)
2. Which success criteria are addressed (SC-X.X identifiers)
3. Key technical decisions made
4. Testing done (unit tests, integration tests, manual devnet validation)
5. Any follow-up work needed
6. Links to validation reports if applicable

Create the PR: `gh pr create --base main --title "<conventional-commit-prefix>(ballast): <description>" --body "<pr-body>"`.
