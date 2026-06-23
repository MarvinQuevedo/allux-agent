---
layout: default
title: 00 — ALF Wire Format
parent: Orchestra Mode
nav_order: 0
---

# 00 — ALF: Allux Line Format

ALF is the token-efficient line-based format used on every LLM ↔ driver
boundary. Internal persistence (`state.json`, `events.log`) stays in JSON;
ALF is only for prompts and model output.

## Why not JSON / TOML / YAML

| Format | Tokens for a 6-field TaskSpec | Risk with small LLMs |
|--------|-------------------------------|----------------------|
| JSON   | ~35 | Mismatched braces / stray commas |
| TOML   | ~30 | Emits headers correctly most of the time |
| YAML   | ~22 | Indentation drift |
| **ALF**| **~14** | Mild key-name drift; recoverable |

ALF wins on input tokens (shorter role prompts) and output tokens (cheaper
planner / diagnoser / worker final reports). Savings compound across
planner + worker + diagnoser per task.

## Core rules

1. One record = one block. Blocks are separated by a line containing only
   `.` (a single period at column 0).
2. Each line inside a block is either:
   - **Field line**: `<key><SP><value>` — value is everything after the
     first space, trimmed of trailing whitespace.
   - **Block opener**: `<key>:` on its own line, followed by lines, closed
     by `:end`. Used for multi-line strings.
3. Keys are lowercase ASCII, `[a-z_][a-z0-9_]*`.
4. Arrays are comma-separated on a single line. Whitespace around commas
   is trimmed.
5. Empty / null values are written as `-` (a single dash).
6. Unknown keys are **ignored** by the parser (forward compatibility).
7. Comments: lines beginning with `#` at column 0 are ignored.

## Special markers for file arrays

The `files` array is the most common typed array. Each element is
`<path>:<change>` where change is:

- `+` → create
- `~` → modify
- `-` → delete

Example:
```
files src/index.html:+, src/styles.css:+, README.md:~
```

## Example — TaskSpec

```
id T01.02
parent T01
title Create homepage HTML
desc Build src/index.html with hero section and contact form
deps T01.01
kw clinic, doctor, services, contact
files src/index.html:+, src/styles.css:~
cmd -
skip -
tools read_file, write_file, edit_file
max_rounds 4
.
```

## Example — List of TaskSpec (plan.alf)

```
id T01
title Project scaffold
desc Set up Next.js 14 with App Router and TypeScript
deps -
kw next.js, typescript, setup
files package.json:+, tsconfig.json:+
cmd npm install
tools -
max_rounds 4
.
id T02
title Home page
desc Implement the landing page with clinic hero
deps T01
kw clinic, hero, doctor
files src/app/page.tsx:+
cmd -
tools -
max_rounds 4
.
```

## Example — Worker FinalReport

```
status ok
summary Created src/index.html with semantic main/header/footer, added hero and contact form
files_touched src/index.html, src/styles.css
.
```

## Example — Diagnosis

```
root_cause The worker wrote styles.css but did not create index.html
strategy RetryWithHint
hint Create src/index.html first; styles.css already exists on disk
.
```

## Multi-line values (block form)

When a string must contain newlines, use the block form:

```
summary:
This change did three things:
- added the nav
- wired onClick
- removed legacy layer
:end
```

Leading/trailing blank lines inside the block are trimmed. The `:end`
marker must be on its own line at column 0.

## Parser

```rust
// src/orchestra/alf.rs
pub struct AlfRecord {
    pub fields: BTreeMap<String, AlfValue>,
}

pub enum AlfValue {
    Scalar(String),           // including "-" as empty sentinel
    List(Vec<String>),        // comma-split, trimmed
    Block(String),            // multi-line body from `key:` ... `:end`
}

pub fn parse(input: &str) -> Result<Vec<AlfRecord>, AlfError>;
pub fn parse_one(input: &str) -> Result<AlfRecord, AlfError>;

pub fn write(rec: &AlfRecord) -> String;
pub fn write_many(recs: &[AlfRecord]) -> String;
```

Errors are non-fatal where possible:

| Error | Recovery |
|-------|----------|
| Unknown key | log + ignore |
| Missing required key | return `AlfError::Missing(key)` |
| Block not closed before `.` or EOF | return `AlfError::UnclosedBlock` |
| Invalid file-change marker | return `AlfError::BadMarker(raw)` |

## Typed decoding layer

Per schema, a `FromAlf` trait implemented by each message type:

```rust
pub trait FromAlf: Sized {
    fn from_alf(rec: &AlfRecord) -> Result<Self, AlfError>;
}

pub trait ToAlf {
    fn to_alf(&self) -> AlfRecord;
}
```

Implementations live next to the types in `src/orchestra/types.rs`:

```rust
impl FromAlf for TaskSpec { /* map fields -> struct */ }
impl ToAlf for TaskSpec   { /* struct -> fields, preserving order */ }
// likewise for TaskReport, Diagnosis, FinalReport fragments, ValidationReport (read-only rendering)
```

## Field-name dictionary (stable)

These names are the contract; the prompts for each role reference them
verbatim.

### TaskSpec fields

| Key | Type | Required | Notes |
|-----|------|----------|-------|
| `id` | scalar | yes | matches `T\d{2}(\.\d{2})?` |
| `parent` | scalar | no | `-` for L1 |
| `title` | scalar | yes | ≤ 80 chars |
| `desc` | scalar or block | yes | ≤ 400 chars scalar; use block if longer |
| `deps` | list | no | earlier ids only |
| `kw` | list | yes | expected_keywords |
| `files` | list | yes | `<path>:<+|~|->` elements |
| `cmd` | list | no | extra_commands |
| `skip` | list | no | skip_checks |
| `tools` | list | no | allowed_tools; `-` = all |
| `max_rounds` | scalar int | no | default 4 |

### TaskReport fields

| Key | Type | Required | Notes |
|-----|------|----------|-------|
| `task_id` | scalar | internal only | filled by driver |
| `status` | scalar | yes | `ok` / `failed` / `needs_review` |
| `summary` | scalar or block | yes | ≤ 300 chars scalar |
| `files_touched` | list | yes | relative paths |

### Diagnosis fields

| Key | Type | Required | Notes |
|-----|------|----------|-------|
| `root_cause` | scalar | yes | ≤ 200 chars |
| `strategy` | scalar | yes | one of 5 enum names |
| `hint` | scalar or block | no | ≤ 300 chars scalar |

### Worker input prompt fields (driver → LLM)

When the driver emits a spec to the LLM for a worker, it uses the same
keys above plus:

| Key | Type | Notes |
|-----|------|-------|
| `goal` | scalar or block | user's verbatim goal |
| `artifacts` | block | compact index (see below) |
| `hint` | scalar or block | present only on retry |

### ArtifactIndex (compact projection)

Rendered inline as a block whose body is one file per line:

```
artifacts:
src/index.html   homepage markup, 1.2KB
src/styles.css   global styles, 480B
src/app/page.tsx Next.js entry, 720B
:end
```

Columns are whitespace-aligned but not required to be. Path is first
token; description is the rest of the line. Used in prompts only; the
on-disk `artifacts/index.json` stays JSON.

## LLM prompt hints

Each role's system prompt includes a literal "FORMAT" section:

```
FORMAT:
- Reply in ALF. One or more records separated by a line containing only `.`.
- Each line is `<key> <value>`. Arrays are comma-separated.
- Use `-` for empty/none. Do NOT use quotes.
- Close blocks with `:end`.
- Do NOT add prose before or after the record(s).
```

With two worked examples (ok / failure path) this is enough for local
models ≥ 7B to produce ALF reliably. Smaller / rustier models may need
one retry with the error appended.

## Recovery policy on malformed output

| Severity | Action |
|----------|--------|
| Missing trailing `.` | auto-append and parse |
| Extra prose before record | drop everything before the first recognized key |
| Extra prose after `.` | drop it |
| Required key missing | retry once with `AlfError::Missing(key)` appended to the prompt |
| Two retries failed | planner: escalate/abort per mode. worker: synthesize `NeedsReview` report |

## Why not something existing (brief rationale)

- **CSV / TSV**: no good nesting story; escaping rules LLMs get wrong.
- **Protobuf text format**: fine token-wise but verbose enum names.
- **EDN / S-expr**: parens are tokens; LLMs mis-balance them.
- **S-structured-prose**: too fuzzy; parsing heuristics accumulate bugs.

ALF is deliberately minimal and line-oriented because the failure modes
of small LLMs are bracket mismatches and indentation drift. Neither
exists in ALF.
