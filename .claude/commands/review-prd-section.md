Read the PRD at `docs/prd.md`.
Focus on the section referenced in $ARGUMENTS (e.g., "phase-0", "section-4.7", "risk-register").

Analyze:
1. Are the requirements specific enough to implement without ambiguity?
2. Are there implicit dependencies not called out?
3. Do the success criteria cover edge cases?
4. Are there conflicts with upstream Percolator behavior?
5. Is the estimated complexity realistic?

Cross-reference with:
- The upstream percolator-cli README and scripts
- The Percolator program's documented behavior (oracle detection, matcher CPI, keeper requirements)
- The current state of `config/ballast-config.json` and deployed infrastructure

Present findings with specific recommendations for any issues found.
