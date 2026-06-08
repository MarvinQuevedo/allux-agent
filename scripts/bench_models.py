#!/usr/bin/env python3
"""
bench_models.py — Tool-calling benchmark for Allux's local Ollama models.

Replicates the exact request shape Allux sends (same 7 tool definitions, same
system prompt, same num_ctx) and measures, per model:
  - whether it emits a VALID tool call for agentic tasks (the thing Allux depends on)
  - tokens/sec (generation speed)
  - latency to load + respond
  - effect of `think` on/off

Usage:
  python3 scripts/bench_models.py                 # all installed models, think off
  python3 scripts/bench_models.py --think both    # compare think on vs off
  python3 scripts/bench_models.py --models qwen3.6:35b gemma4:26b
"""
import argparse
import json
import urllib.request

OLLAMA = "http://localhost:11434"
NUM_CTX = 8192  # matches Allux config default

# Kept in sync with src/prompts.rs::AGENT_SYSTEM_PROMPT
SYSTEM_PROMPT = (
    "You are Allux, a local code assistant powered by Ollama. "
    "You help with software engineering and device administration tasks. "
    "You have these tools: read_file, write_file, edit_file, glob, grep, tree, bash. "
    "Act, don't just describe: when a task needs running, building, testing, installing, "
    "or inspecting the system, call `bash` directly. "
    "Use grep/glob to locate code and read_file before editing; prefer edit_file over "
    "rewriting whole files. Take the most direct path and avoid redundant exploration. "
    "Be concise and precise."
)

# Exact tool definitions from src/tools/mod.rs
TOOLS = [
    {"type": "function", "function": {"name": "read_file",
        "description": "Read the full contents of a file. Returns file content with line numbers.",
        "parameters": {"type": "object", "properties": {
            "path": {"type": "string", "description": "Path to the file to read"}}, "required": ["path"]}}},
    {"type": "function", "function": {"name": "write_file",
        "description": "Create or overwrite a file with the given content.",
        "parameters": {"type": "object", "properties": {
            "path": {"type": "string", "description": "Path to write"},
            "content": {"type": "string", "description": "Content to write"}}, "required": ["path", "content"]}}},
    {"type": "function", "function": {"name": "edit_file",
        "description": "Replace an exact string in a file with a new string. old_str must match exactly.",
        "parameters": {"type": "object", "properties": {
            "path": {"type": "string", "description": "Path to the file"},
            "old_str": {"type": "string", "description": "Exact string to replace"},
            "new_str": {"type": "string", "description": "Replacement string"}}, "required": ["path", "old_str", "new_str"]}}},
    {"type": "function", "function": {"name": "glob",
        "description": "Find files matching a glob pattern (e.g. '**/*.rs'). Returns matching paths.",
        "parameters": {"type": "object", "properties": {
            "pattern": {"type": "string", "description": "Glob pattern"},
            "dir": {"type": "string", "description": "Base directory (default: current dir)"}}, "required": ["pattern"]}}},
    {"type": "function", "function": {"name": "grep",
        "description": "Search for a regex pattern in files. Returns matching lines with file:line context.",
        "parameters": {"type": "object", "properties": {
            "pattern": {"type": "string", "description": "Regex pattern to search"},
            "path": {"type": "string", "description": "File or directory to search (default: current dir)"},
            "case_insensitive": {"type": "boolean", "description": "Case-insensitive search"}}, "required": ["pattern"]}}},
    {"type": "function", "function": {"name": "tree",
        "description": "Show the directory tree structure.",
        "parameters": {"type": "object", "properties": {
            "path": {"type": "string", "description": "Root path (default: current dir)"},
            "depth": {"type": "integer", "description": "Max depth (default: 3)"}}}}},
    {"type": "function", "function": {"name": "bash",
        "description": "Execute a shell command and return its output. Use for builds, tests, git, etc.",
        "parameters": {"type": "object", "properties": {
            "command": {"type": "string", "description": "Shell command to execute"}}, "required": ["command"]}}},
]

# (name, user prompt, set of acceptable tool names for the FIRST call)
TESTS = [
    ("read",   "Read the file Cargo.toml and tell me the package version.", {"read_file"}),
    ("glob",   "List all the Rust source files under the src/tools directory.", {"glob", "tree", "bash"}),
    ("bash",   "Run the project's test suite and report whether it passes.", {"bash"}),
    ("grep",   "Find every place in the codebase where SYSTEM_PROMPT is defined.", {"grep", "bash"}),
]


def post(path, payload, timeout=600):
    req = urllib.request.Request(
        OLLAMA + path,
        data=json.dumps(payload).encode(),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read())


def installed_models():
    with urllib.request.urlopen(OLLAMA + "/api/tags") as r:
        data = json.loads(r.read())
    return [m["name"] for m in data["models"]]


def run_case(model, prompt, think):
    payload = {
        "model": model,
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": prompt},
        ],
        "tools": TOOLS,
        "stream": False,
        "options": {"temperature": 0.0, "num_ctx": NUM_CTX},
    }
    if think is not None:
        payload["think"] = think
    return post("/api/chat", payload)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--models", nargs="*", help="models to test (default: all installed)")
    ap.add_argument("--think", choices=["off", "on", "both"], default="off")
    args = ap.parse_args()

    models = args.models or installed_models()
    think_modes = {"off": [False], "on": [True], "both": [False, True]}[args.think]

    print(f"\nBenchmark: {len(models)} model(s) × {len(TESTS)} tasks × think={args.think}")
    print(f"num_ctx={NUM_CTX}, temperature=0.0\n")

    rows = []
    for model in models:
        for think in think_modes:
            tag = f"{model}  (think={'on' if think else 'off'})"
            print(f"── {tag}")
            passed = 0
            tps_samples = []
            for name, prompt, ok_tools in TESTS:
                try:
                    r = run_case(model, prompt, think)
                except Exception as e:
                    print(f"   {name:6} ERROR: {e}")
                    continue
                msg = r.get("message", {})
                calls = msg.get("tool_calls") or []
                called = calls[0]["function"]["name"] if calls else None
                good = called in ok_tools
                passed += good
                ev = r.get("eval_count", 0)
                edur = r.get("eval_duration", 1) / 1e9
                tps = ev / edur if edur > 0 else 0
                if tps:
                    tps_samples.append(tps)
                total_s = r.get("total_duration", 0) / 1e9
                mark = "✓" if good else "✗"
                got = called or ("TEXT:" + (msg.get("content", "")[:40].replace("\n", " ")))
                print(f"   {name:6} {mark}  tool={got:<28} {tps:5.1f} tok/s  {total_s:5.1f}s")
            avg_tps = sum(tps_samples) / len(tps_samples) if tps_samples else 0
            rows.append((tag, passed, len(TESTS), avg_tps))
            print(f"   → {passed}/{len(TESTS)} tool tasks OK, avg {avg_tps:.1f} tok/s\n")

    print("=" * 64)
    print(f"{'MODEL (think)':<44}{'TOOLS':>7}{'TOK/S':>9}")
    print("-" * 64)
    for tag, p, t, tps in sorted(rows, key=lambda x: (-x[1], -x[3])):
        print(f"{tag:<44}{f'{p}/{t}':>7}{tps:>9.1f}")
    print("=" * 64)


if __name__ == "__main__":
    main()
