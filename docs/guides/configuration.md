---
layout: page
title: Configuration
nav_order: 3
---

# Configuration

Allux is configured via a `config.toml` file. This file is stored in your system's standard configuration directory under the `allux` folder.

## Configuration Location

- **Linux/macOS**: `~/.config/allux/config.toml`
- **Windows**: `%AppData%\allux\config.toml`

## Configuration Options

The following table describes the available configuration options.

| Key | Type | Default | Description |
|:---|:---|:---|:---|
| `ollama_url` | `String` | `http://localhost:11434` | The base URL for your Ollama instance. |
| `model` | `String` | `llama3.2` | The model name to use for tasks (must support tool calling). |
| `context_size` | `u32` | `8192` | The maximum context window size for the model. |
| `compression_mode` | `String` | `auto` | Token compression mode: `always`, `auto`, or `manual`. |

## Example `config.toml`

```toml
ollama_url = "http://localhost:11434"
model = "qwen2.5-coder:14b"
context_size = 32768
compression_mode = "always"
```

## Managing Configuration

Allux will automatically create the configuration directory and a default `config.toml` if it doesn't exist. You can modify this file at any time and restart Allux for the changes to take effect.
