# Allux Validation Suite (Autonomous CLI)

This suite validates the full autonomous workflow against a real sample project:
- analyze existing code
- apply file changes
- run checks/tests
- request follow-up updates
- process multiple prompts sequentially

## 1) Sample Project

Use: `sandbox/sample-rust-app`

The sample intentionally includes:
- a small Rust binary
- an inefficient/verbose implementation in `src/lib.rs`
- tests in `src/lib.rs`

## 2) Single Prompt Validation

Run one autonomous prompt:

```bash
node --experimental-strip-types scripts/allux-cli.ts ask "Work only inside sandbox/sample-rust-app. Improve the implementation in src/lib.rs for clarity, keep behavior, then run cargo test there. Respond in English." --autonomous --max-rounds 8 --verbose
```

Expected:
- tool calls include `read_file`, `replace_in_file` or `write_file`, and `bash`
- output confirms what changed
- `cargo test` succeeds

## 3) Multi-Input Sequential Validation (New)

Use batch file: `validation/prompts-sequential.txt`

```bash
node --experimental-strip-types scripts/allux-cli.ts ask --batch-file validation/prompts-sequential.txt --autonomous --max-rounds 8 --verbose
```

Behavior:
- prompts are executed one by one
- each prompt waits for completion before next starts
- errors are reported and execution continues by default
- use `--stop-on-error` to halt immediately

## 4) Master Supervisor Validation

Run managed cycles with ready token:

```bash
node --experimental-strip-types scripts/master-automejora.ts --cycles 2 --interval-ms 1000 --prompt "Work inside sandbox/sample-rust-app only. Apply one safe improvement and run cargo test. Include CLI_READY_TO_RESTART only when done." --max-rounds 8
```

Expected:
- supervisor waits for child completion
- if child returns non-zero, it restarts
- if `scripts/allux-cli.ts` changed, next cycle uses new version
- exits early when ready token is detected (default)

## 5) Regression Checks for Main Project

After autonomous runs:

```bash
cargo check
git status --short
```

Review:
- ensure intended files changed
- no accidental large edits
- no broken build
