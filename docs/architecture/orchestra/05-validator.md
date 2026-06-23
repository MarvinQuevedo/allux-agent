---
layout: default
title: 05 — Validator
parent: Orchestra Mode
nav_order: 5
---

# 05 — Validator

The validator is **entirely deterministic code**. No LLM call in the common
path. It runs after every worker attempt and produces a `ValidationReport`.

## Public API

```rust
// src/orchestra/validator/mod.rs
pub fn validate(
    spec: &TaskSpec,
    workspace: &Path,
    pre_snapshot: &FileSnapshot,
) -> ValidationReport;
```

`FileSnapshot` is captured by the driver before the worker starts:

```rust
pub struct FileSnapshot {
    pub files: BTreeMap<PathBuf, FileState>,
}

pub struct FileState {
    pub exists: bool,
    pub size: u64,
    pub mtime: u64,
    pub sha256: Option<String>,   // only computed for small files
}
```

## Aggregation

```rust
impl ValidationReport {
    pub fn aggregate(outcomes: Vec<(String, CheckOutcome)>) -> Self {
        let has_fail = outcomes.iter().any(|(_, o)| matches!(o, CheckOutcome::Fail { .. }));
        if has_fail {
            return Self { outcomes, verdict: Verdict::Failed, score: 0.0, /* ... */ };
        }
        let softs: Vec<f32> = outcomes.iter().filter_map(|(_, o)| match o {
            CheckOutcome::Soft(s) => Some(*s),
            CheckOutcome::Pass    => Some(1.0),
            CheckOutcome::Fail {..} => None, // unreachable
        }).collect();
        let score = if softs.is_empty() { 1.0 } else { softs.iter().sum::<f32>() / softs.len() as f32 };
        let verdict = if      score >= 0.7 { Verdict::Ok }
                      else if score >= 0.5 { Verdict::Uncertain }
                      else                 { Verdict::Failed };
        Self { outcomes, verdict, score, /* ... */ }
    }
}
```

## Check catalog

Each check is in `src/orchestra/validator/checks/`. One file per family.

### Family 1 — Structural (`checks/structural.rs`)

| Name | Signature | Signal | Notes |
|------|-----------|--------|-------|
| `file_exists` | `(path)` | Pass/Fail | — |
| `file_size_in_range` | `(path, min, max)` | Pass/Fail | mins default by ext |
| `diff_has_changes` | `(path, pre_snapshot)` | Pass/Fail | compares sha256 & size |
| `diff_is_addition` | `(path, pre_snapshot)` | Soft | ratio of bytes added vs removed |

Default size mins (bytes) by extension (only applied when `min_bytes=None`):
```
.html → 100   .css → 50    .js/.ts → 30
.rs   → 30    .py  → 20    .md    → 40
.json → 2     .toml → 10   default → 1
```

### Family 2 — Syntax (`checks/syntax.rs`)

| Extension | Implementation | Fail criterion |
|-----------|----------------|----------------|
| `.json` | `serde_json::from_str` | Err |
| `.toml` | `toml::from_str::<toml::Value>` | Err |
| `.yaml`/`.yml` | `serde_yaml::from_str::<serde_yaml::Value>` | Err |
| `.md` | `pulldown_cmark::Parser::new`; count unclosed code fences | unclosed > 0 |
| `.html` | custom `html_tag_balance` — see below | unbalanced |
| `.css` | custom `brace_balance` | unbalanced |
| `.rs` | `syn::parse_file` | Err |
| `.py` | spawn `python3 -m py_compile <path>` | exit != 0 |
| `.js` | spawn `node --check <path>` | exit != 0 |
| `.ts` | spawn `tsc --noEmit <path>` if `tsc` in PATH; else skip | exit != 0 |
| `.sh` | spawn `bash -n <path>` | exit != 0 |

External command checks skip gracefully (`CheckOutcome::Soft(0.8)` with a
note) if the tool is missing.

HTML tag balance:
```rust
fn html_tag_balance(src: &str) -> Result<(), String> {
    let void: &[&str] = &["area","base","br","col","embed","hr","img","input",
                          "link","meta","source","track","wbr"];
    let mut stack: Vec<String> = Vec::new();
    // scan <tagname ...> and </tagname>, skip <!-- --> and <script>...</script>
    // return Err if stack non-empty at end or mismatched pop
    # ...
    Ok(())
}
```

CSS brace balance is a two-line scanner counting `{` and `}` outside
strings and comments.

### Family 3 — Content heuristics (`checks/content.rs`)

All receive the file contents as `&str`.

#### `no_placeholders`

```rust
const PLACEHOLDER_PATTERNS: &[&str] = &[
    r"\bTODO\b", r"\bFIXME\b", r"\bXXX\b", r"\bTBD\b",
    r"<INSERT[^>]*>", r"<!--\s*YOUR\s", r"\[PLACEHOLDER\]",
    r"(?i)\blorem ipsum\b",
    r"(?i)\breplace\s+this\b",
    r"<your-[a-z-]+-here>",
];
```

Each match is checked against the per-task `skip_checks` / whitelist before
failing. Signal: **Fail** on first unignored match.

#### `no_loop_repetition`

```rust
fn no_loop_repetition(content: &str, max_ratio: f32) -> CheckOutcome {
    if content.len() < 200 { return CheckOutcome::Pass; }
    let compressed = zstd::encode_all(content.as_bytes(), 3).unwrap_or_default();
    let ratio = compressed.len() as f32 / content.len() as f32;
    if ratio < max_ratio {
        CheckOutcome::Fail { reason: format!("zstd ratio {:.2} < {:.2}", ratio, max_ratio) }
    } else {
        CheckOutcome::Pass
    }
}
```

Default `max_ratio = 0.15`. Natural text/code is typically 0.25 – 0.6.

#### `unique_line_ratio`

```rust
fn unique_line_ratio(content: &str) -> CheckOutcome {
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() < 10 { return CheckOutcome::Pass; }
    let uniq = lines.iter().collect::<std::collections::HashSet<_>>().len();
    let ratio = uniq as f32 / lines.len() as f32;
    CheckOutcome::Soft(ratio.min(1.0))
}
```

#### `ngram_repetition`

Check 2-grams and 3-grams. If the top n-gram covers > 15% of all n-grams,
Fail. Otherwise Pass.

#### `entropy_reasonable`

Shannon entropy per byte. Expected band: `3.0 .. 7.5`.

```rust
fn shannon_entropy(bytes: &[u8]) -> f32 {
    let mut freq = [0u64; 256];
    for &b in bytes { freq[b as usize] += 1; }
    let n = bytes.len() as f32;
    -freq.iter().filter(|&&c| c > 0)
        .map(|&c| { let p = c as f32 / n; p * p.log2() }).sum::<f32>()
}
```

| Entropy | Outcome |
|---------|---------|
| `< 2.0` | Fail (degenerate) |
| `2.0 .. 3.0` | Soft(0.3) |
| `3.0 .. 7.5` | Pass |
| `7.5 .. 7.9` | Soft(0.6) |
| `>= 7.9` | Fail (likely binary / random) |

#### `keywords_present`

```rust
fn keywords_present(content: &str, keywords: &[String], min_hit: f32) -> CheckOutcome {
    if keywords.is_empty() { return CheckOutcome::Pass; }
    let hay = content.to_lowercase();
    let hits = keywords.iter()
        .filter(|k| hay.contains(&k.to_lowercase()))
        .count();
    let ratio = hits as f32 / keywords.len() as f32;
    if ratio < min_hit { CheckOutcome::Soft(ratio) }
    else               { CheckOutcome::Pass }
}
```

Default `min_hit = 0.4` for L1, `0.3` for L2.

#### `language_matches`

Classical trigram-frequency language detector. A single table of ~200
top trigrams per language (EN, ES) is embedded in the source.

```rust
fn language_matches(content: &str, target: Language) -> CheckOutcome {
    let scores = score_languages(content);   // BTreeMap<Language, f32>
    let detected = scores.iter().max_by(|a,b| a.1.partial_cmp(b.1).unwrap()).map(|(l,_)| *l);
    if detected == Some(target) { CheckOutcome::Pass }
    else if let Some(&s) = scores.get(&target) {
        CheckOutcome::Soft((s / scores.values().sum::<f32>()).clamp(0.0, 1.0))
    } else { CheckOutcome::Soft(0.0) }
}
```

#### `no_empty_critical_blocks`

Regex per language, scoped to definitions only:

| Language | Regex |
|----------|-------|
| Rust     | `fn\s+\w+[^{]*\{\s*\}` |
| JS/TS    | `function\s+\w+[^{]*\{\s*\}` and `=>\s*\{\s*\}` |
| Python   | `def\s+\w+[^:]*:\s*\n\s*pass\s*$` |

Match → Fail (unless `skip_checks` contains `"no_empty_critical_blocks"`).

### Family 4 — Cross-file (`checks/cross_file.rs`)

#### `references_resolve`

| Source type | What to check |
|-------------|---------------|
| `.html` | `href`/`src` to local paths (ignore `http`, `//`, `data:`) |
| `.md` | `[text](local/path)` links |
| `.js`/`.ts` | relative `import` / `require` paths |
| `.py` | relative imports |
| `.rs` | `mod foo;` resolves to `foo.rs` or `foo/mod.rs` |

For each unresolved reference: Soft(0.0). Aggregated over all references:
Soft = resolved / total. If total = 0, Pass.

#### `symbols_defined`

Extract identifiers called in the diff; ripgrep the workspace for their
definition. Soft = defined / called.

### Family 5 — Execution (`checks/execution.rs`)

#### `command_exits_zero`

```rust
fn command_exits_zero(cmd: &str, cwd: &Path, timeout_s: u64) -> CheckOutcome {
    // spawn with timeout; stdout/stderr captured for the event log
    match run_with_timeout(cmd, cwd, timeout_s) {
        Ok(status) if status.success() => CheckOutcome::Pass,
        Ok(status) => CheckOutcome::Fail {
            reason: format!("exit {} for `{}`", status.code().unwrap_or(-1), cmd)
        },
        Err(e) => CheckOutcome::Fail { reason: format!("spawn error: {e}") },
    }
}
```

Default timeout 60 s. Stdout/stderr are **not** returned in the outcome;
only written to the event log for later inspection.

### Family 6 — Escape hatch (`checks/manual.rs`)

#### `manual_review`

Returns `CheckOutcome::Soft(0.5)` with the note. Aggregation will tend to
push the verdict to `Uncertain` unless other checks pull it up.

## Auto-detect extra checks

`src/orchestra/validator/auto.rs` augments the spec's check list with
deterministic rules:

| Workspace signal | Added check |
|------------------|-------------|
| `Cargo.toml` exists and `.rs` in `files_touched` | `cargo check --quiet` |
| `package.json` has `scripts.build` and JS/TS in `files_touched` | `npm run build` |
| `pyproject.toml`/`setup.py` and `.py` in `files_touched` | `python -m py_compile <file>` per file |
| `.github/workflows/*.yml` in `files_touched` | `actionlint` if installed |
| Any `.sql` in `files_touched` | `sqlfluff lint` if installed |

All auto-added checks are tagged so the report can explain why they ran.

## Performance targets

| Phase | Target time on a 200-LOC file |
|-------|-------------------------------|
| Structural + content heuristics | < 50 ms |
| Syntax (Rust via `syn`) | < 200 ms |
| Execution (`cargo check`) | variable; capped at 60 s |

Running the full deterministic battery (no execution checks) on a typical
task should stay under 500 ms.

## Failure reporting format

Used by the driver when escalating to the user:

```
Task T02.03 validation: FAILED (score 0.42)
  ✗ FileExists(src/index.html): file does not exist
  ✗ NoPlaceholders(src/app.js): matched `TODO` at line 17
  ⚠ KeywordsPresent(src/index.html): 0.20 (expected ≥ 0.40)
  ✓ FileSizeInRange(src/style.css): 1234 bytes
```

The formatter is `ValidationReport::render_human` in
`src/orchestra/validator/mod.rs`.
