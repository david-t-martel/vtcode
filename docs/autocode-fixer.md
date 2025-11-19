# Autocode Fixer (Ollama + rg/git)

Prototype to let a local Ollama model (default `gemma:2b`) reason over repo signals and propose patches.

## Quick start

```
scripts/ollama-autofix.sh [--error-log path] [--ast-rules rules_dir] [--apply]
```
- `OLLAMA_MODEL` overrides the model (default `gemma:2b`).
- `WORKDIR` points at another checkout.
- `--apply` or `AUTO_APPLY=1` attempts to git-apply model diffs.
- `--ast-rules DIR` includes ast-grep results if available.

## What it sends
- `git status --short`
- `git diff --stat` and first ~400 lines of unified diff
- Optional error snippets (JSON rustc logs summarized; text logs trimmed)
- First 200 TODO/FIXME hits (rg over common source files)
- Workspace graph: git ls-files + cargo metadata
- Optional ast-grep findings

## Suggested workflow
1. Run `cargo check --message-format=short > /tmp/check.log` (or your build command).
2. `scripts/ollama-autofix.sh --error-log /tmp/check.log --apply` to get a plan and auto-apply if possible.
3. Re-run checks; iterate.

## TODO / Next steps
- Integrate rust-analyzer direct diagnostics stream when available.
- Stream responses and chunk context to fit smaller models.
- Provider switch: allow OpenAI/Gemini/DeepSeek via `LLM_PROVIDER` with API keys.
- Cache run context artifacts under `.cache/autofix/` for repeatable prompts and retrieval.
