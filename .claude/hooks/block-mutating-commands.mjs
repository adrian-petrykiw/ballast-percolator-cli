// .claude/hooks/block-mutating-commands.mjs
//
// PreToolUse hook on the Bash tool. Enforces the "Claude does not execute
// state-mutating commands" rule from CLAUDE.md.
//
// Blocks:
//   - git commit/push/tag/rebase/revert/cherry-pick/reset --hard/branch -D/stash drop
//   - gh pr|issue|release writes; gh workflow run; gh api with write HTTP methods
//   - solana program deploy/upgrade/close/extend/set-*-authority/write-buffer
//   - solana value/stake transfers (transfer, send, *-stake, withdraw-*, vote-update-*, ...)
//   - spl-token mint/burn/transfer/approve/revoke/wrap/unwrap/close/create-*/authorize/freeze/thaw/gc
//   - cargo|npm|pnpm|yarn publish
//   - cargo|npm|pnpm|yarn install/add/update/upgrade (supply-chain gate — user runs installs)
//   - Language-eval escape hatches: eval, python -c/-m, node -e/--eval, perl -e, ruby -e, bun -e, deno eval
//   - curl|wget|base64-d piped into sh/bash/zsh
//   - rm -rf on dangerous roots (., .., /, ~, $HOME)
//   - git -c with dangerous config keys (core.sshCommand, core.fsmonitor, ...)
//   - git add -A / --all / .
//
// Recurses into bash -c / sh -c / zsh -c, env, nice, xargs, sudo, su wrappers.
// Splits the full command on top-level ; && || | (quote- and escape-aware) so
// `cd subdir && git push` is caught.
//
// Emits structured JSON:
//   { hookSpecificOutput: { permissionDecision: "deny", permissionDecisionReason: "..." } }
// permissionDecision:"deny" from a PreToolUse hook bypasses --dangerously-skip-permissions.
//
// Audit-logs every block to ~/.cache/ballast/events.jsonl as event_type=GUARDRAIL_DECISION.
//
// No override flag at this layer, by design. The user is the only one who runs
// blocked commands, in their own terminal.

import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';

// ─── Rule tables ─────────────────────────────────────────────────────────

// (binary, first-non-flag-arg) rules. `requires` is an additional flag/arg
// that must appear somewhere in the remaining args.
const SUBCOMMAND_RULES = [
  // git mutating subcommands
  { binary: 'git', sub: /^commit$/,      requires: null,        label: 'git commit' },
  { binary: 'git', sub: /^push$/,        requires: null,        label: 'git push' },
  { binary: 'git', sub: /^tag$/,         requires: null,        label: 'git tag' },
  { binary: 'git', sub: /^rebase$/,      requires: null,        label: 'git rebase' },
  { binary: 'git', sub: /^revert$/,      requires: null,        label: 'git revert' },
  { binary: 'git', sub: /^cherry-pick$/, requires: null,        label: 'git cherry-pick' },
  { binary: 'git', sub: /^reset$/,       requires: /^--hard$/,  label: 'git reset --hard' },
  { binary: 'git', sub: /^branch$/,      requires: /^-D$/,      label: 'git branch -D' },
  { binary: 'git', sub: /^stash$/,       requires: /^drop$/,    label: 'git stash drop' },

  // gh writes (gh api writes handled in checkSpecial)
  { binary: 'gh', sub: /^pr$/,       requires: /^(create|merge|close|comment|edit|review|ready)$/, label: 'gh pr <mutate>' },
  { binary: 'gh', sub: /^issue$/,    requires: /^(create|close|comment|edit|reopen|delete)$/,       label: 'gh issue <mutate>' },
  { binary: 'gh', sub: /^release$/,  requires: /^(create|edit|delete|upload)$/,                     label: 'gh release <mutate>' },
  { binary: 'gh', sub: /^workflow$/, requires: /^run$/,                                             label: 'gh workflow run' },

  // solana program (on-chain code writes)
  {
    binary: 'solana', sub: /^program$/,
    requires: /^(deploy|close|upgrade|extend|set-upgrade-authority|set-buffer-authority|write-buffer)$/,
    label: 'solana program <mutate>',
  },

  // solana value/stake mutations (top-level subcommands, no second word)
  {
    binary: 'solana',
    sub: /^(transfer|send|pay|withdraw-from-nonce-account|withdraw-from-vote-account|withdraw-stake|delegate-stake|deactivate-stake|split-stake|merge-stake|redelegate-stake|create-stake-account|create-vote-account|create-nonce-account|create-address-with-seed|authorize-nonce-account|new-nonce|stake-authorize|vote-authorize-voter|vote-authorize-withdrawer|vote-update-validator|vote-update-commission|assign)$/,
    requires: null,
    label: 'solana <value-mutate>',
  },

  // spl-token mutations
  {
    binary: 'spl-token',
    sub: /^(mint|burn|transfer|approve|revoke|wrap|unwrap|close|create-token|create-account|create-multisig|authorize|freeze|thaw|gc)$/,
    requires: null,
    label: 'spl-token <mutate>',
  },

  // package publishing
  { binary: 'cargo', sub: /^publish$/, requires: null, label: 'cargo publish' },
  { binary: 'npm',   sub: /^publish$/, requires: null, label: 'npm publish' },
  { binary: 'pnpm',  sub: /^publish$/, requires: null, label: 'pnpm publish' },
  { binary: 'yarn',  sub: /^publish$/, requires: null, label: 'yarn publish' },

  // dependency installs / updates (supply-chain gate)
  { binary: 'pnpm',  sub: /^(install|i|add|update|up|upgrade|dlx)$/,     requires: null, label: 'pnpm <install>' },
  { binary: 'npm',   sub: /^(install|i|add|update|up|upgrade|exec|ci)$/, requires: null, label: 'npm <install>' },
  { binary: 'yarn',  sub: /^(install|add|upgrade|up|dlx)$/,              requires: null, label: 'yarn <install>' },
  { binary: 'cargo', sub: /^(install|add|remove|update|upgrade)$/,       requires: null, label: 'cargo <install>' },

  // deno eval (eats a subcommand)
  { binary: 'deno', sub: /^eval$/, requires: null, label: 'deno eval' },
];

// Rules that check for the presence of an eval-style FLAG anywhere in args.
const FLAG_RULES = [
  { binary: 'eval',    flag: null,                                          label: 'eval' },
  { binary: 'python',  flag: /^(-c|-m|--command|--module)$/,                label: 'python -c/-m' },
  { binary: 'python3', flag: /^(-c|-m|--command|--module)$/,                label: 'python3 -c/-m' },
  { binary: 'node',    flag: /^(-e|--eval|--print|-p)$/,                    label: 'node -e/--eval' },
  { binary: 'perl',    flag: /^-e$/,                                        label: 'perl -e' },
  { binary: 'ruby',    flag: /^-e$/,                                        label: 'ruby -e' },
  { binary: 'bun',     flag: /^(-e|--eval)$/,                               label: 'bun -e' },
];

// Wrappers we transparently unwrap before checking the inner command.
const WRAPPERS = new Set(['env', 'nice', 'xargs', 'sudo', 'su']);

const GIT_GLOBAL_VALUE_FLAGS = new Set(['-C', '-c', '--git-dir', '--work-tree', '--namespace']);
const GIT_GLOBAL_BOOL_FLAGS = new Set([
  '-p', '-P', '--paginate', '--no-pager', '--bare',
  '--no-replace-objects', '--literal-pathspecs',
  '--glob-pathspecs', '--noglob-pathspecs', '--icase-pathspecs',
]);

const ENV_ASSIGN_RE = /^[A-Za-z_][A-Za-z0-9_]*=/;
const DANGEROUS_RM_TARGETS = /^(\.|\.\.|\/|~|~\/|\.\/|\.\.\/|\$HOME|\$HOME\/|\$\{HOME\}|\$\{HOME\}\/|\/\*|\.\*)$/;
const DANGEROUS_GIT_C_KEYS = /^(core\.sshCommand|core\.fsmonitor|core\.editor|core\.pager|core\.hooksPath|protocol\..*\.allow|http\.extraheader|http\.followRedirects|alias\.)/;

// ─── Tokenizer (shlex-equivalent, quote- and escape-aware) ────────────────

function tokenize(cmd) {
  const tokens = [];
  let cur = '';
  let started = false;
  let inS = false, inD = false;
  let i = 0;
  while (i < cmd.length) {
    const c = cmd[i];
    if (c === '\\' && i + 1 < cmd.length && !inS) {
      cur += cmd[i + 1];
      started = true;
      i += 2;
      continue;
    }
    if (c === "'" && !inD) { inS = !inS; started = true; i++; continue; }
    if (c === '"' && !inS) { inD = !inD; started = true; i++; continue; }
    if (/\s/.test(c) && !inS && !inD) {
      if (started) { tokens.push(cur); cur = ''; started = false; }
      i++;
      continue;
    }
    cur += c;
    started = true;
    i++;
  }
  if (started) tokens.push(cur);
  return tokens;
}

function splitPipeline(cmd) {
  const parts = [];
  let cur = '';
  let inS = false, inD = false;
  let i = 0;
  while (i < cmd.length) {
    const c = cmd[i];
    const nxt = cmd[i + 1] || '';
    if (c === '\\' && i + 1 < cmd.length) {
      cur += c + cmd[i + 1];
      i += 2;
      continue;
    }
    if (c === "'" && !inD) { inS = !inS; cur += c; i++; continue; }
    if (c === '"' && !inS) { inD = !inD; cur += c; i++; continue; }
    if (!inS && !inD) {
      if ((c === '&' && nxt === '&') || (c === '|' && nxt === '|')) {
        parts.push(cur); cur = ''; i += 2; continue;
      }
      if (c === ';' || c === '|') {
        parts.push(cur); cur = ''; i++; continue;
      }
    }
    cur += c;
    i++;
  }
  if (cur.trim()) parts.push(cur);
  return parts.map((p) => p.trim()).filter((p) => p.length > 0);
}

function basename(p) {
  const idx = p.lastIndexOf('/');
  return idx === -1 ? p : p.slice(idx + 1);
}

function quoteForReconstruct(s) {
  if (s === '' || /[\s'"\\$`|&;<>(){}[\]*?#~]/.test(s)) {
    return `'${s.replace(/'/g, `'\\''`)}'`;
  }
  return s;
}

function reconstructCmd(tokens) {
  return tokens.map(quoteForReconstruct).join(' ');
}

// ─── Detection ────────────────────────────────────────────────────────────

function findSubcommand(binary, args) {
  let i = 0;
  while (i < args.length) {
    const a = args[i];
    if (binary === 'git') {
      if (GIT_GLOBAL_VALUE_FLAGS.has(a)) { i += 2; continue; }
      if (a.includes('=') && GIT_GLOBAL_VALUE_FLAGS.has(a.split('=')[0])) { i += 1; continue; }
      if (GIT_GLOBAL_BOOL_FLAGS.has(a)) { i += 1; continue; }
    }
    if (a.startsWith('-')) { i += 1; continue; }
    return { sub: a, rest: args.slice(i + 1) };
  }
  return { sub: null, rest: [] };
}

function checkSubcommandRules(binary, sub, rest) {
  for (const r of SUBCOMMAND_RULES) {
    if (r.binary !== binary) continue;
    if (!r.sub.test(sub)) continue;
    if (r.requires === null) return r.label;
    if (rest.some((a) => r.requires.test(a))) return r.label;
  }
  return null;
}

function checkFlagRules(binary, args) {
  for (const r of FLAG_RULES) {
    if (r.binary !== binary) continue;
    if (r.flag === null) return r.label;
    if (args.some((a) => r.flag.test(a))) return r.label;
  }
  return null;
}

function checkSpecial(tokens) {
  if (tokens.length === 0) return null;
  const binary = basename(tokens[0]);

  // rm -rf <dangerous-root>
  if (binary === 'rm') {
    const hasRecursive = tokens.some(
      (t) => /^-[a-zA-Z]*[rR][a-zA-Z]*$/.test(t) || t === '--recursive'
    );
    if (hasRecursive) {
      for (const t of tokens.slice(1)) {
        if (t.startsWith('-')) continue;
        if (DANGEROUS_RM_TARGETS.test(t)) return `rm -rf ${t}`;
      }
    }
  }

  // git -c <dangerous-key>=...
  if (binary === 'git') {
    for (let i = 1; i < tokens.length - 1; i++) {
      if (tokens[i] === '-c') {
        const kv = tokens[i + 1] || '';
        const key = kv.split('=')[0];
        if (DANGEROUS_GIT_C_KEYS.test(key)) return `git -c ${key}=...`;
      }
    }
  }

  // git add -A | --all | .  (per CLAUDE.md, agent stages specific paths only)
  if (binary === 'git') {
    const { sub, rest } = findSubcommand(binary, tokens.slice(1));
    if (sub === 'add') {
      for (const t of rest) {
        if (t === '-A' || t === '--all' || t === '.') return `git add ${t}`;
      }
    }
  }

  // gh api with write HTTP method
  if (binary === 'gh' && tokens[1] === 'api') {
    for (let i = 2; i < tokens.length; i++) {
      const t = tokens[i];
      if ((t === '-X' || t === '--method') && i + 1 < tokens.length) {
        const m = tokens[i + 1].toUpperCase();
        if (m === 'POST' || m === 'PUT' || m === 'DELETE' || m === 'PATCH') {
          return `gh api -X ${m}`;
        }
      }
      const eqMatch = /^(-X|--method)=(.+)$/.exec(t);
      if (eqMatch) {
        const m = eqMatch[2].toUpperCase();
        if (m === 'POST' || m === 'PUT' || m === 'DELETE' || m === 'PATCH') {
          return `gh api ${t}`;
        }
      }
    }
  }

  return null;
}

// Checks that need the FULL pre-split command string (because the pattern
// spans a pipeline operator that splitPipeline strips out).
function checkPipelinePatterns(cmd) {
  if (/\bcurl\b[^|]*\|\s*(sh|bash|zsh)\b/.test(cmd)) return 'curl | shell';
  if (/\bwget\b[^|]*\|\s*(sh|bash|zsh)\b/.test(cmd)) return 'wget | shell';
  if (/\bbase64\b[^|]*\b(-d|--decode|-D)\b[^|]*\|\s*(sh|bash|zsh)\b/.test(cmd)) {
    return 'base64 -d | shell';
  }
  return null;
}

function checkCmd(cmdStr) {
  let tokens = tokenize(cmdStr);
  while (tokens.length > 0 && ENV_ASSIGN_RE.test(tokens[0])) tokens = tokens.slice(1);
  if (tokens.length === 0) return null;

  const binary = basename(tokens[0]);

  // bash -c / sh -c / zsh -c  → recurse into wrapped command
  if ((binary === 'bash' || binary === 'sh' || binary === 'zsh') && tokens.length >= 3) {
    for (let i = 1; i < tokens.length - 1; i++) {
      if (tokens[i] === '-c') {
        for (const sub of splitPipeline(tokens[i + 1])) {
          const hit = checkCmd(sub);
          if (hit) return hit;
        }
        break;
      }
    }
  }

  // env / nice / xargs / sudo / su  → unwrap and recheck inner
  if (WRAPPERS.has(binary)) {
    const rest = tokens.slice(1);
    let j = 0;
    while (j < rest.length) {
      const t = rest[j];
      if (binary === 'env' && (t === '-i' || t === '--ignore-environment' || t === '-0' || t === '--null')) { j++; continue; }
      if (binary === 'env' && (t === '-u' || t === '--unset')) { j += 2; continue; }
      if (binary === 'sudo' && /^(-u|-g|-U|-h|-p|-D|-r|-t|-T|-C)$/.test(t)) { j += 2; continue; }
      if (binary === 'xargs' && /^(-n|-I|-J|-L|-P|-s|-E|-d)$/.test(t)) { j += 2; continue; }
      if (t.startsWith('-')) { j++; continue; }
      if (ENV_ASSIGN_RE.test(t)) { j++; continue; }
      break;
    }
    const inner = rest.slice(j);
    if (inner.length > 0) return checkCmd(reconstructCmd(inner));
  }

  // Special-case detectors (rm -rf, git -c, git add -A/., gh api)
  const special = checkSpecial(tokens);
  if (special) return special;

  // Flag-anywhere rules (eval, python -c, node -e, perl -e, ...)
  const flagHit = checkFlagRules(binary, tokens.slice(1));
  if (flagHit) return flagHit;

  // Standard (binary, subcommand) lookup
  const { sub, rest } = findSubcommand(binary, tokens.slice(1));
  if (sub === null) return null;
  return checkSubcommandRules(binary, sub, rest);
}

// ─── Audit log ────────────────────────────────────────────────────────────

function logDecision(decision, command, reason) {
  try {
    const dir = path.join(os.homedir(), '.cache', 'ballast');
    fs.mkdirSync(dir, { recursive: true });
    const entry = {
      timestamp: new Date().toISOString(),
      event_type: 'GUARDRAIL_DECISION',
      decision,
      command,
      matched_rule: reason,
    };
    fs.appendFileSync(path.join(dir, 'events.jsonl'), JSON.stringify(entry) + '\n');
  } catch {
    // best-effort; never fail the hook on log errors
  }
}

// ─── Main ─────────────────────────────────────────────────────────────────

function emitDeny(cmd, label) {
  const reason = [
    `Refused: ${label}`,
    '',
    'Command:',
    `  ${cmd}`,
    '',
    'Per CLAUDE.md (Workflow Rule), Claude does not execute commits, pushes,',
    'tags, PRs, releases, on-chain program writes, value transfers, SPL-token',
    'writes, package publishes, dependency installs, or language-eval escape',
    'hatches. Emit a fenced bash block (heredoc\'d commit messages included)',
    'for the user to run in the terminal at the repo root.',
    '',
    'There is no override flag at this layer. The user is the only one who',
    'runs state-mutating commands; this is by design.',
  ].join('\n');

  logDecision('deny', cmd, label);

  process.stdout.write(JSON.stringify({
    hookSpecificOutput: {
      hookEventName: 'PreToolUse',
      permissionDecision: 'deny',
      permissionDecisionReason: reason,
    },
  }));
}

async function main() {
  let raw = '';
  for await (const chunk of process.stdin) raw += chunk;

  let payload;
  try {
    payload = JSON.parse(raw);
  } catch {
    process.exit(0); // fail-open on malformed payload
  }

  const cmd = payload?.tool_input?.command;
  if (typeof cmd !== 'string' || cmd.trim() === '') process.exit(0);

  // Pipeline-spanning patterns (curl|sh, base64|sh) must be checked on the
  // full command string before splitPipeline strips the pipe operators.
  const pipelineHit = checkPipelinePatterns(cmd);
  if (pipelineHit) {
    emitDeny(cmd, pipelineHit);
    process.exit(0);
  }

  for (const sub of splitPipeline(cmd)) {
    const label = checkCmd(sub);
    if (label) {
      emitDeny(cmd, label);
      process.exit(0);
    }
  }

  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`hook error: ${err?.message ?? err}\n`);
  process.exit(0); // fail-open on unexpected errors
});
