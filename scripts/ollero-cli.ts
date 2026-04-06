#!/usr/bin/env -S npx tsx
// @ts-nocheck

/**
 * Minimal CLI to run Ollama prompts by task.
 *
 * Usage examples:
 *   npx tsx scripts/ollero-cli.ts list
 *   npx tsx scripts/ollero-cli.ts show T01
 *   npx tsx scripts/ollero-cli.ts run T01 --model qwen3.5:9b
 *   npx tsx scripts/ollero-cli.ts ask "explica src/repl/mod.rs"
 */

import { mkdir, readFile, writeFile } from "node:fs/promises";
import { exec as execCb } from "node:child_process";
import path from "node:path";
import { promisify } from "node:util";

type Command = "list" | "show" | "run" | "ask" | "help";

type Task = {
  id: string;
  title: string;
  section: string;
  prompt: string;
};

type CliOptions = {
  model: string;
  url: string;
  tasksFile: string;
  system?: string;
  outDir: string;
  dryRun: boolean;
  autonomous: boolean;
  allowBash: boolean;
  allowWeb: boolean;
  maxRounds: number;
  cmdTimeoutMs: number;
};

const DEFAULT_MODEL = "qwen3.5:9b";
const DEFAULT_URL = "http://localhost:11434";
const DEFAULT_TASKS_FILE = "TASKS_OLLERO_TOOL_ACTIONS.md";
const DEFAULT_OUT_DIR = ".ollero-cli/runs";
const DEFAULT_MAX_ROUNDS = 10;
const DEFAULT_CMD_TIMEOUT_MS = 60_000;

const execAsync = promisify(execCb);

type ChatMessage = {
  role: "system" | "user" | "assistant" | "tool";
  content: string;
  tool_name?: string;
  tool_calls?: ToolCall[];
};

type ToolCall = {
  function: {
    name: string;
    arguments: unknown;
  };
};

type ToolDefinition = {
  type: "function";
  function: {
    name: string;
    description: string;
    parameters: unknown;
  };
};

type ChatResponse = {
  message?: {
    content?: string;
    tool_calls?: ToolCall[];
  };
  prompt_eval_count?: number;
  eval_count?: number;
};

const AUTONOMOUS_SYSTEM_PROMPT = [
  "You are Ollero operating in autonomous mode.",
  "You are allowed to execute shell commands and use internet tools.",
  "Use tools when they are necessary to complete the task with evidence.",
  "Do not ask for confirmation before using tools.",
  "Keep actions focused and return a concise final answer with what you executed.",
].join(" ");

function parseArgs(argv: string[]) {
  const [commandRaw, ...rest] = argv;
  const command: Command = (commandRaw as Command) || "help";
  const positional: string[] = [];
  const flags = new Map<string, string | true>();

  for (let i = 0; i < rest.length; i += 1) {
    const token = rest[i];
    if (token.startsWith("--")) {
      const key = token.slice(2);
      const next = rest[i + 1];
      if (!next || next.startsWith("--")) {
        flags.set(key, true);
      } else {
        flags.set(key, next);
        i += 1;
      }
      continue;
    }
    positional.push(token);
  }

  const options: CliOptions = {
    model: String(flags.get("model") ?? DEFAULT_MODEL),
    url: String(flags.get("url") ?? DEFAULT_URL),
    tasksFile: String(flags.get("tasks") ?? DEFAULT_TASKS_FILE),
    outDir: String(flags.get("out") ?? DEFAULT_OUT_DIR),
    system: typeof flags.get("system") === "string" ? String(flags.get("system")) : undefined,
    dryRun: flags.has("dry-run"),
    autonomous: flags.has("autonomous"),
    allowBash: flags.has("autonomous") || flags.has("allow-bash"),
    allowWeb: flags.has("autonomous") || flags.has("allow-web"),
    maxRounds: Number(flags.get("max-rounds") ?? DEFAULT_MAX_ROUNDS),
    cmdTimeoutMs: Number(flags.get("cmd-timeout-ms") ?? DEFAULT_CMD_TIMEOUT_MS),
  };

  return { command, positional, options };
}

function printHelp() {
  console.log(
    [
      "Ollero CLI (simple task runner for Ollama)",
      "",
      "Commands:",
      "  list                         List task IDs from markdown file",
      "  show <TASK_ID>               Show task title and prompt",
      "  run <TASK_ID> [--dry-run]    Send task prompt to Ollama",
      "  ask \"<prompt>\"               Send a direct prompt to Ollama",
      "  help                         Show this message",
      "",
      "Flags:",
      `  --model <name>   Default: ${DEFAULT_MODEL}`,
      `  --url <url>      Default: ${DEFAULT_URL}`,
      `  --tasks <file>   Default: ${DEFAULT_TASKS_FILE}`,
      `  --out <dir>      Default: ${DEFAULT_OUT_DIR}`,
      "  --system <text>  Optional system prompt",
      "  --autonomous     Enable autonomous tool loop (bash + web tools)",
      "  --allow-bash     Allow shell commands without full autonomous mode",
      "  --allow-web      Allow internet tools without full autonomous mode",
      `  --max-rounds <n> Default: ${DEFAULT_MAX_ROUNDS}`,
      `  --cmd-timeout-ms Default: ${DEFAULT_CMD_TIMEOUT_MS}`,
      "  --dry-run        Print request without sending",
    ].join("\n"),
  );
}

async function loadTasks(tasksFile: string): Promise<Task[]> {
  const raw = await readFile(tasksFile, "utf8");
  const sections = raw
    .split(/\r?\n(?=## TASK )/g)
    .filter((s) => s.trimStart().startsWith("## TASK "));

  const tasks: Task[] = [];
  for (const section of sections) {
    const headerMatch = section.match(/^## TASK (T\d+)\s*-\s*(.+)$/m);
    if (!headerMatch) continue;

    const id = headerMatch[1].trim();
    const title = headerMatch[2].trim();
    const promptMatch = section.match(/### PROMPT[\s\S]*?```(?:text|prompt)?\r?\n([\s\S]*?)\r?\n```/m);
    const prompt = promptMatch?.[1]?.trim() ?? "";
    tasks.push({ id, title, section, prompt });
  }
  return tasks;
}

function requireTask(tasks: Task[], id: string): Task {
  const task = tasks.find((t) => t.id.toLowerCase() === id.toLowerCase());
  if (!task) {
    throw new Error(`Task '${id}' not found in tasks file.`);
  }
  if (!task.prompt) {
    throw new Error(`Task '${id}' does not contain a PROMPT block.`);
  }
  return task;
}

async function callOllama(
  url: string,
  model: string,
  messages: ChatMessage[],
  tools?: ToolDefinition[],
): Promise<ChatResponse> {
  const payload = { model, stream: false, messages, tools };
  const response = await fetch(`${url}/api/chat`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });

  if (!response.ok) {
    const body = await response.text();
    throw new Error(`Ollama error ${response.status}: ${body}`);
  }

  return response.json() as Promise<ChatResponse>;
}

function nowStamp(): string {
  return new Date().toISOString().replace(/[:.]/g, "-");
}

async function saveRun(outDir: string, name: string, text: string) {
  await mkdir(outDir, { recursive: true });
  const filename = `${name}-${nowStamp()}.md`;
  const full = path.join(outDir, filename);
  await writeFile(full, text, "utf8");
  return full;
}

function toolsForOptions(options: CliOptions): ToolDefinition[] {
  const tools: ToolDefinition[] = [];
  if (options.allowBash) {
    tools.push({
      type: "function",
      function: {
        name: "bash",
        description: "Execute shell commands on the local machine.",
        parameters: {
          type: "object",
          properties: {
            command: { type: "string" },
          },
          required: ["command"],
        },
      },
    });
  }
  if (options.allowWeb) {
    tools.push({
      type: "function",
      function: {
        name: "web_search",
        description: "Search the web and return top links/snippets.",
        parameters: {
          type: "object",
          properties: {
            query: { type: "string" },
          },
          required: ["query"],
        },
      },
    });
    tools.push({
      type: "function",
      function: {
        name: "web_fetch",
        description: "Fetch URL content and return text excerpt.",
        parameters: {
          type: "object",
          properties: {
            url: { type: "string" },
          },
          required: ["url"],
        },
      },
    });
  }
  return tools;
}

function stringifyUnknown(value: unknown): string {
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

async function runBash(command: string, timeoutMs: number): Promise<string> {
  const { stdout, stderr } = await execAsync(command, {
    shell: true,
    timeout: timeoutMs,
    maxBuffer: 1024 * 1024 * 4,
  });
  const out = [stdout, stderr].filter(Boolean).join("");
  return out.trim() || "(no output)";
}

function stripHtmlTags(html: string): string {
  return html
    .replace(/<script[\s\S]*?<\/script>/gi, " ")
    .replace(/<style[\s\S]*?<\/style>/gi, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

async function webSearch(query: string): Promise<string> {
  const endpoint = `https://duckduckgo.com/html/?q=${encodeURIComponent(query)}`;
  const response = await fetch(endpoint, {
    headers: {
      "User-Agent": "ollero-cli/0.1",
    },
  });
  if (!response.ok) {
    throw new Error(`web_search failed (${response.status})`);
  }
  const html = await response.text();
  const items: string[] = [];
  const re = /<a[^>]*class="[^"]*result__a[^"]*"[^>]*href="([^"]+)"[^>]*>([\s\S]*?)<\/a>/gi;
  let m: RegExpExecArray | null;
  while ((m = re.exec(html)) && items.length < 5) {
    const href = m[1];
    const title = stripHtmlTags(m[2]);
    if (title) {
      items.push(`- ${title} -> ${href}`);
    }
  }
  if (items.length === 0) {
    const fallback = stripHtmlTags(html).slice(0, 1200);
    return `No structured results parsed.\n${fallback}`;
  }
  return items.join("\n");
}

async function webFetch(url: string): Promise<string> {
  const response = await fetch(url, {
    headers: {
      "User-Agent": "ollero-cli/0.1",
    },
  });
  if (!response.ok) {
    throw new Error(`web_fetch failed (${response.status})`);
  }
  const text = await response.text();
  return stripHtmlTags(text).slice(0, 5000);
}

async function dispatchTool(
  call: ToolCall,
  options: CliOptions,
): Promise<{ name: string; output: string }> {
  const name = call.function.name;
  const args = call.function.arguments ?? {};
  const argsObj =
    typeof args === "object" && args !== null
      ? (args as Record<string, unknown>)
      : ({ value: args } as Record<string, unknown>);

  if (name === "bash") {
    if (!options.allowBash) {
      return { name, output: "Permission denied: bash is disabled." };
    }
    const command = String(argsObj.command ?? "");
    if (!command.trim()) {
      return { name, output: "Error: missing 'command' argument." };
    }
    const output = await runBash(command, options.cmdTimeoutMs);
    return { name, output };
  }

  if (name === "web_search") {
    if (!options.allowWeb) {
      return { name, output: "Permission denied: web tools are disabled." };
    }
    const query = String(argsObj.query ?? "");
    if (!query.trim()) {
      return { name, output: "Error: missing 'query' argument." };
    }
    return { name, output: await webSearch(query) };
  }

  if (name === "web_fetch") {
    if (!options.allowWeb) {
      return { name, output: "Permission denied: web tools are disabled." };
    }
    const url = String(argsObj.url ?? "");
    if (!url.trim()) {
      return { name, output: "Error: missing 'url' argument." };
    }
    return { name, output: await webFetch(url) };
  }

  return { name, output: `Unknown tool: ${name}` };
}

async function runPrompt(
  userPrompt: string,
  runName: string,
  options: CliOptions,
): Promise<void> {
  const tools = toolsForOptions(options);
  const messages: ChatMessage[] = [];
  const systemPrompt =
    options.system ??
    (options.autonomous
      ? AUTONOMOUS_SYSTEM_PROMPT
      : undefined);

  if (systemPrompt) {
    messages.push({ role: "system", content: systemPrompt });
  }
  messages.push({ role: "user", content: userPrompt });

  if (options.dryRun) {
    console.log(
      `[dry-run] model=${options.model} url=${options.url} autonomous=${options.autonomous} allowBash=${options.allowBash} allowWeb=${options.allowWeb}`,
    );
    console.log(userPrompt);
    return;
  }

  const trace: string[] = [];
  let finalText = "";
  let totalPrompt = 0;
  let totalCompletion = 0;
  let totalToolCalls = 0;

  for (let round = 1; round <= Math.max(1, options.maxRounds); round += 1) {
    const result = await callOllama(options.url, options.model, messages, tools.length ? tools : undefined);
    totalPrompt += result.prompt_eval_count ?? 0;
    totalCompletion += result.eval_count ?? 0;

    const assistantContent = result.message?.content ?? "";
    const toolCalls = result.message?.tool_calls ?? [];
    messages.push({ role: "assistant", content: assistantContent, tool_calls: toolCalls });

    if (assistantContent.trim()) {
      finalText = assistantContent;
      trace.push(`## Assistant round ${round}\n\n${assistantContent}\n`);
    }

    if (!toolCalls.length) {
      break;
    }

    totalToolCalls += toolCalls.length;
    trace.push(`## Tool calls round ${round}\n\n${stringifyUnknown(toolCalls)}\n`);

    for (const call of toolCalls) {
      try {
        const { name, output } = await dispatchTool(call, options);
        trace.push(`### Tool ${name}\n\n${output}\n`);
        messages.push({
          role: "tool",
          tool_name: name,
          content: output,
        });
      } catch (err) {
        const output = `Error in tool ${call.function.name}: ${
          err instanceof Error ? err.message : String(err)
        }`;
        trace.push(`### Tool ${call.function.name}\n\n${output}\n`);
        messages.push({
          role: "tool",
          tool_name: call.function.name,
          content: output,
        });
      }
    }
  }

  console.log(finalText || "(no assistant text)");
  console.log(
    `\n---\nPrompt tokens(total): ${totalPrompt}\nCompletion tokens(total): ${totalCompletion}\nTool calls(total): ${totalToolCalls}`,
  );

  const saved = await saveRun(
    options.outDir,
    runName,
    [
      `# Run ${runName}`,
      "",
      "## Prompt",
      "",
      userPrompt,
      "",
      "## Final Response",
      "",
      finalText,
      "",
      "## Metrics",
      "",
      `- prompt_eval_count_total: ${totalPrompt}`,
      `- eval_count_total: ${totalCompletion}`,
      `- tool_calls_total: ${totalToolCalls}`,
      `- autonomous: ${options.autonomous}`,
      `- allow_bash: ${options.allowBash}`,
      `- allow_web: ${options.allowWeb}`,
      "",
      "## Trace",
      "",
      ...trace,
    ].join("\n"),
  );
  console.log(`Saved: ${saved}`);
}

async function main() {
  const { command, positional, options } = parseArgs(process.argv.slice(2));

  if (command === "help" || !["list", "show", "run", "ask", "help"].includes(command)) {
    printHelp();
    return;
  }

  if (command === "ask") {
    const prompt = positional.join(" ").trim();
    if (!prompt) {
      throw new Error("ask requires a prompt string.");
    }
    await runPrompt(prompt, "ask", options);
    return;
  }

  const tasks = await loadTasks(options.tasksFile);

  if (command === "list") {
    for (const t of tasks) {
      console.log(`${t.id} - ${t.title}`);
    }
    return;
  }

  const taskId = positional[0];
  if (!taskId) {
    throw new Error(`${command} requires <TASK_ID>.`);
  }
  const task = requireTask(tasks, taskId);

  if (command === "show") {
    console.log(`${task.id} - ${task.title}\n`);
    console.log(task.prompt);
    return;
  }

  if (command === "run") {
    await runPrompt(task.prompt, task.id, options);
    return;
  }
}

main().catch((err) => {
  console.error(`Error: ${err instanceof Error ? err.message : String(err)}`);
  process.exit(1);
});
