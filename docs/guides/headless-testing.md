# Headless mode & model benchmarking

Allux can run the **full agentic loop without the TUI**, so you can test prompts,
compare local Ollama models, and script regression checks.

## `allux run` — headless agent

```bash
allux run [OPTIONS] "<prompt>"
```

Drives the same loop the TUI uses (same tools, system prompt, and workspace
snapshot) but prints a plain transcript and a stats summary, then exits.

| Option | Meaning |
| --- | --- |
| `-m, --model <name>` | Model to use (default: config) |
| `--ctx <n>` | Context window / `num_ctx` (default: config) |
| `--max-rounds <n>` | Max tool rounds per prompt (default: 25) |
| `-y, --yes` | Allow mutating tools (`write_file`/`edit_file`/`bash`) to run |
| `--read-only` | Deny mutating tools (**default**) |
| `--think` / `--no-think` | Toggle model reasoning (**default: `--no-think`**) |
| `--json` | Emit one JSON object per prompt (machine-readable) |
| `-v, --verbose` | Print tool-output previews |
| `-C, --cwd <dir>` | Run in this directory |
| `-f, --prompts-file <f>` | Read prompts from a file (one per line, `#` = comment) |

### Permission policy (no interactive prompts)

- Read-only tools (`read_file`, `glob`, `grep`, `tree`) always run.
- Mutating tools run only with `--yes`; otherwise a denial is fed back to the
  model so it can adapt. This keeps benchmark runs from touching the filesystem.

### Examples

```bash
# Single read-only task
allux run -m gemma4:26b "What is the package version in Cargo.toml?"

# Real agentic task with bash + writes allowed
allux run -m gemma4:26b --yes -v "Add a unit test for X and run cargo test"

# Sequential regression prompts, machine-readable
allux run -f validation/prompts-sequential.txt --json > results.jsonl
```

## `scripts/bench_models.py` — tool-calling benchmark

Replicates the exact request shape Allux sends (the 7 tool definitions, the agent
system prompt, `num_ctx=8192`) against each installed model and reports, per
model: whether it emits a **valid first tool call** for agentic tasks, plus
**tokens/sec** measured from Ollama's own `eval_duration` (so the number is not
polluted by model-load time).

```bash
python3 scripts/bench_models.py                    # all models, think off
python3 scripts/bench_models.py --think both       # compare think on/off
python3 scripts/bench_models.py --models gemma4:26b qwen3.6:35b
```

Use `bench_models.py` for **speed / tool-selection** comparisons and `allux run`
for **end-to-end agentic correctness**.

## Notes on local models

- All reasoning-capable models (Qwen3, Gemma) default to **think off**: for these
  tool tasks, thinking added latency without improving — and sometimes degraded —
  first-tool selection. Toggle at runtime with `/think on|off` in the TUI, or
  `--think`/`--no-think` headless. The flag is only sent to models that actually
  support reasoning (detected via `/api/show`).
- **Capabilities are detected up front** (tools, thinking, native context) via
  `/api/show` when the TUI starts and on every `/model` switch — no more waiting
  for a failed request to discover a model can't use tools. Headless does the same
  check before running.
- The agent system prompt lives in one place (`src/prompts.rs`) and is shared by
  the TUI and the headless runner, so both behave identically.
- The TUI's system prompt includes a workspace snapshot (tree + key files), which
  measurably improves tool selection: models go straight to `bash`/`grep` instead
  of re-running `tree` to orient themselves.
