---
layout: default
title: 03 — Planner Role
parent: Orchestra Mode
nav_order: 3
---

# 03 — Planner Role

The planner converts either the user's goal (L1) or a single L1 task (L2)
into a list of `TaskSpec`. It runs **without tools**; its only job is to
return structured JSON.

## Two sub-roles

| Sub-role | Input | Output |
|----------|-------|--------|
| **L1** | `goal: String` | `Vec<TaskSpec>` (top-level tasks, no parent) |
| **L2** | `goal: String`, `l1_task: TaskSpec`, `artifacts_index: ArtifactIndex` | `Vec<TaskSpec>` (subtasks of the given L1) |

Both sub-roles return the same type. The difference is in the prompt and
the `parent` field of the produced specs.

## Invocation contract

```rust
// src/orchestra/planner.rs
pub async fn plan_l1(
    client: &OllamaClient,
    goal: &str,
    ctx_size: u32,
) -> Result<Vec<TaskSpec>>;

pub async fn plan_l2(
    client: &OllamaClient,
    goal: &str,
    l1: &TaskSpec,
    artifacts: &ArtifactIndex,
    ctx_size: u32,
) -> Result<Vec<TaskSpec>>;
```

The driver calls these functions directly. There is no conversation history
passed in.

## Prompt construction

Both prompts reference the ALF spec (see [00-format.md](00-format.md)).
The driver appends the literal **FORMAT** block and two worked examples
(ok + recovery) to each prompt.

### L1 system prompt

```
You are a task planner for a software engineering assistant.
Your only job is to break down the user's goal into a short, ordered list
of high-level tasks, each completable in under 10 minutes of work.

HARD RULES:
- Reply in ALF. Emit one or more TaskSpec records separated by a line
  containing only `.`.
- 3 to 8 tasks total. Fewer if the goal is small.
- id is `T01`, `T02`, ... in order.
- parent is `-` (L1 tasks have no parent).
- deps lists earlier ids only, comma-separated, or `-`.
- files lists concrete relative paths with `:+` (create), `:~` (modify),
  or `:-` (delete). Use `-` if genuinely unknown.
- kw is the list of literal words/phrases from the user's goal that must
  appear in the produced artifacts.
- cmd is extra shell commands for verification (e.g. `npm run build`),
  comma-separated, or `-`.
- Do NOT wrap the reply in code fences. Do NOT add prose.

FORMAT:
<literal ALF format block, see 00-format.md>

EXAMPLE:
id T01
title Project scaffold
desc Initialize Next.js 14 with TypeScript and App Router
parent -
deps -
kw next.js, typescript, setup
files package.json:+, tsconfig.json:+
cmd npm install
tools -
max_rounds 4
.

User goal:
<goal here>
```

### L2 system prompt

```
You are a task planner. You are given ONE high-level task. Break it into
2 to 6 concrete subtasks that a code-writing worker can execute in order.

HARD RULES:
- Reply in ALF. Emit one or more TaskSpec records separated by `.` lines.
- id is `<parent_id>.01`, `<parent_id>.02`, ...
- parent MUST be the given parent id.
- Use the provided artifact list to reference files that already exist;
  do not recreate them.
- files lists only the files this specific subtask touches.

FORMAT:
<literal ALF format block>

Parent task:
<parent TaskSpec in ALF>

artifacts:
<compact index body; see 00-format.md>
:end

Original user goal:
<goal here>
```

## Output parsing

The planner's output is parsed by `parse_plan_output`:

```rust
fn parse_plan_output(raw: &str, parent: Option<&TaskId>) -> Result<Vec<TaskSpec>> {
    let cleaned = alf::strip_surrounding_prose(raw);
    let records = alf::parse(&cleaned)?;
    let specs: Vec<TaskSpec> = records.iter()
        .map(TaskSpec::from_alf)
        .collect::<Result<_, _>>()?;
    validate_ids(&specs, parent)?;
    validate_deps(&specs)?;
    Ok(specs)
}
```

Validation rules (reject the plan and retry once if any fails):

1. Every `id` matches the expected pattern.
2. Every `parent` matches the expected parent (if provided).
3. Every dep in `deps` refers to an earlier id within the same plan.
4. No duplicate ids.
5. `title.len() <= 80`.
6. `desc` scalar ≤ 400 chars (use block form for longer).
7. If `files` is empty AND `cmd` is empty, emit a warning but accept.

If parsing fails twice, the driver escalates to the user (in Interactive
mode) or marks the run as failed (in Autonomous mode). On the first retry
the driver appends the specific `AlfError` to the prompt.

## Chat options

```rust
ChatOptions {
    temperature: Some(0.2),   // low, planner is structural
    num_ctx: Some(ctx_size),
}
```

Tools field must be `None`; the planner must not call tools.

## Keyword extraction (classical, no LLM)

The planner is asked to produce `expected_keywords`, but the driver
**also** independently extracts keywords from the user goal and merges
them in. This guards against weak planner output.

```rust
// src/orchestra/keywords.rs
pub fn extract_keywords(goal: &str) -> Vec<String> {
    let tokens = tokenize_lower(goal);
    let filtered = drop_stopwords(tokens);        // simple EN/ES stopword list
    let stemmed  = stem_light(filtered);          // trim common suffixes
    dedup_preserving_order(stemmed)
        .into_iter()
        .filter(|t| t.len() >= 3)
        .take(20)
        .collect()
}
```

The final `expected_keywords` of each TaskSpec is the union of the
planner-produced set and the extracted set, capped at 30 entries.

## Error handling

| Failure | Driver action |
|---------|---------------|
| JSON parse error | Retry once with "Your last reply was not valid JSON. Reply with a valid JSON array only." |
| Schema validation fails | Retry once with the specific error attached. |
| Two retries exhausted | Interactive: ask user. Autonomous: mark run failed. |
| LLM request error | Propagate with context; do not retry blindly. |

## Size bounds

- L1 prompt: goal is capped at 2,000 chars before embedding; longer goals
  trigger a pre-summarization step (`compression::compress_message`).
- L2 prompt: `ArtifactIndex` is serialized with only `path + description`
  per entry, capped at 80 entries. Older entries are pruned by `created_by`
  task age if exceeded.
