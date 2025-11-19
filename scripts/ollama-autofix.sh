#!/usr/bin/env bash
set -euo pipefail

# Autocode fixer helper
# Collects repo context (git, rg, diagnostics, ast-grep), sends to Ollama, optional auto-apply patch.
#
# Usage:
#   scripts/ollama-autofix.sh [--error-log path] [--ast-rules dir] [--apply]
# Env:
#   OLLAMA_MODEL (default gemma:2b)
#   WORKDIR (default pwd)
#   AUTO_APPLY=1 (same as --apply)
#   MAX_DIFF_LINES (default 400)
#   MAX_TODO (default 200)
#   CONTEXT_CACHE (default .cache/autofix)
#
# Optional: pass a rust-analyzer JSON diagnostics file via --error-log; otherwise cargo check output works.

export PATH="$HOME/.local/bin:$PATH"
PROVIDER="${LLM_PROVIDER:-ollama}"
# Default to a fast, already-cached model if available
MODEL="${OLLAMA_MODEL:-codegemma:2b}"
# Prefer Windows host daemon for speed/caching when accessible
export OLLAMA_HOST="${OLLAMA_HOST:-http://localhost:11434}"
GEMINI_MODEL="${GEMINI_MODEL:-gemini-1.5-flash}"
VERBOSE="${VERBOSE:-1}"
WORKDIR="${WORKDIR:-$(pwd)}"
MAX_DIFF_LINES="${MAX_DIFF_LINES:-400}"
MAX_TODO="${MAX_TODO:-200}"
CONTEXT_CACHE="${CONTEXT_CACHE:-.cache/autofix}"
ERROR_LOG=""
AST_RULES=""
AUTO_APPLY="${AUTO_APPLY:-0}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --error-log) ERROR_LOG="$2"; shift 2;;
    --ast-rules) AST_RULES="$2"; shift 2;;
    --apply) AUTO_APPLY=1; shift;;
    *) echo "Unknown arg: $1" >&2; exit 1;;
  esac
done

cd "$WORKDIR"
mkdir -p "$CONTEXT_CACHE"
TMPDIR=$(mktemp -d "$CONTEXT_CACHE/run-XXXX")
if [[ "${KEEP_CACHE:-0}" != "1" ]]; then
  trap 'rm -rf "$TMPDIR"' EXIT
else
  echo "[autofix][debug] KEEP_CACHE=1; preserving $TMPDIR" >&2
fi

# 1) git signals
(git status --short || true) > "$TMPDIR/status.txt"
(git diff --stat || true) > "$TMPDIR/diffstat.txt"
(git diff -U2 || true) | head -n "$MAX_DIFF_LINES" > "$TMPDIR/diff.txt"

# 2) TODO/FIXME hints
rg "TODO|FIXME" -g'*.{rs,ts,tsx,js,py,sh,md}' -n --max-count "$MAX_TODO" > "$TMPDIR/todos.txt" || true

# 3) Dependency / structure snapshot
(git ls-files || true) > "$TMPDIR/files.txt"
(cargo metadata --no-deps --format-version 1 2>/dev/null || true) > "$TMPDIR/metadata.json"

# 4) Diagnostics (rust-analyzer / cargo)
if [[ -n "$ERROR_LOG" && -f "$ERROR_LOG" ]]; then
  python - <<'PY' "$ERROR_LOG" "$TMPDIR"
import json, sys, pathlib
log = pathlib.Path(sys.argv[1]).read_text(errors='ignore').splitlines()
root = pathlib.Path(sys.argv[2])
errs = []
for line in log:
    try:
        obj = json.loads(line)
    except Exception:
        continue
    if obj.get('reason') != 'compiler-message':
        continue
    msg = obj.get('message', {})
    if msg.get('level') not in {'error','warning'}:
        continue
    spans = msg.get('spans') or []
    span = spans[0] if spans else {}
    errs.append({
        'level': msg.get('level'),
        'message': msg.get('message'),
        'code': (msg.get('code') or {}).get('code'),
        'file': span.get('file_name'),
        'line': span.get('line_start'),
    })
if not errs:
    # fallback: grep error lines
    import re
    pat=re.compile(r'error\[|error:')
    errs=[l for l in log if pat.search(l)][:200]
    root.joinpath('errors.txt').write_text('\n'.join(errs))
else:
    out='\n'.join(f"{e['level']} {e['code'] or ''} {e['file']}:{e['line']} {e['message']}" for e in errs)
    root.joinpath('errors.txt').write_text(out)
PY
else
  # quick cargo check sampler if no log provided
  (cargo check --message-format=short 2>&1 | head -n 200 || true) > "$TMPDIR/errors.txt"
fi

# 5) ast-grep (optional)
if [[ -n "$AST_RULES" && -d "$AST_RULES" ]] && command -v ast-grep >/dev/null 2>&1; then
  ast-grep scan --json --rule "$AST_RULES" . | head -n 300 > "$TMPDIR/ast-grep.json" || true
fi

# 5.5) Collect code snippets for files referenced in errors (RAG-lite)
python - "$TMPDIR/errors.txt" "$TMPDIR/snippets.txt" <<'PY'
import pathlib, sys
errors = pathlib.Path(sys.argv[1])
out = pathlib.Path(sys.argv[2])
if not errors.exists():
    sys.exit(0)
files = []
for line in errors.read_text(errors='ignore').splitlines():
    parts=line.strip().split()
    for token in parts:
        if token.endswith('.rs') and '/' in token:
            files.append(token.split(':')[0])
files = list(dict.fromkeys(files))[:6]
lines = []
for fp in files:
    p=pathlib.Path(fp)
    if not p.exists():
        continue
    try:
        text=p.read_text(errors='ignore')
    except Exception:
        continue
    snippet=text[:4000]
    lines.append(f"[snippet::{fp}]\n"+snippet)
out.write_text('\n\n'.join(lines))
PY

# 5.6) Build ast-grep quickfix patterns from diagnostics (simple line substrings)
python - "$TMPDIR/errors.txt" "$TMPDIR/ast-grep-rules.yml" <<'PY'
import pathlib, sys, re
errors = pathlib.Path(sys.argv[1])
out = pathlib.Path(sys.argv[2])
lines = []
if not errors.exists():
    sys.exit(0)
pat = re.compile(r"^(error|warning)\s+\w*\s+([^:]+):(\d+):\s*(.*)")
rules = []
for line in errors.read_text(errors='ignore').splitlines():
    m=pat.match(line.strip())
    if not m:
        continue
    file = m.group(2)
    msg = m.group(4)[:80]
    # Create a loose pattern (escaped) to search in file
    pattern = re.escape(msg.split()[0]) if msg else None
    if pattern:
        rules.append({'id': f'ra-{len(rules)}', 'message': msg, 'severity': 'error', 'language': 'rust', 'pattern': pattern, 'files': file})

if not rules:
    sys.exit(0)

import yaml
out.write_text(yaml.safe_dump({'rules': rules}))
PY

# 6) Build prompt
PROMPT_FILE="$TMPDIR/prompt.txt"
cat > "$PROMPT_FILE" <<'PROMPT'
You are an automated code-fixing agent for this repository.
Use the provided context to propose precise fixes.
Respond with:
1) Short plan.
2) Minimal unified diffs to apply (```diff fenced).
3) Any commands to run to verify.
If context is insufficient, say exactly what you need (file path / log snippet).
PROMPT

# 7) Pack context (trimmed)
python - "$TMPDIR" "$TMPDIR/context.txt" <<'PY'
import json, pathlib, sys
root = pathlib.Path(sys.argv[1])
out_path = pathlib.Path(sys.argv[2])
order = [
    'status.txt','diffstat.txt','diff.txt','errors.txt',
    'todos.txt','files.txt','metadata.json','ast-grep.json','snippets.txt',
    'AGENTS.md','docs/autocode-fixer.md'
]
parts = []
for name in order:
    p = root / name
    if p.exists():
        parts.append(f"[{name}]\n" + p.read_text()[:15000])
out_path.write_text('\n\n'.join(parts))
PY

# 8) Call LLM provider (ollama or gemini)
RESPONSE_FILE="$TMPDIR/response.txt"
if [[ "$PROVIDER" == "gemini" ]]; then
  if [[ -z "${GOOGLE_API_KEY:-}" ]]; then
    echo "GOOGLE_API_KEY is required for gemini provider" >&2
    exit 1
  fi
  echo "[autofix] sending context to Gemini model $GEMINI_MODEL" >&2
  python - "$TMPDIR/context.txt" "$TMPDIR/prompt.txt" "$RESPONSE_FILE" "$GEMINI_MODEL" <<'PY'
import json, sys, pathlib, requests, os
ctx = pathlib.Path(sys.argv[1]).read_text()
prompt = pathlib.Path(sys.argv[2]).read_text()
out = pathlib.Path(sys.argv[3])
model = sys.argv[4]
api_key=os.environ['GOOGLE_API_KEY']
url=f"https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={api_key}"
payload={
  "contents": [
    {"role":"user","parts":[{"text": prompt + "\n\n[context]\n" + ctx}]}
  ]
}
r=requests.post(url, json=payload, timeout=90)
r.raise_for_status()
data=r.json()
text='' 
if 'candidates' in data and data['candidates']:
    parts=data['candidates'][0].get('content',{}).get('parts',[])
    text=''.join(p.get('text','') for p in parts)
out.write_text(text)
print(f"[gemini] tokens? {data.get('usage_metadata',{})}", file=sys.stderr)
PY
elif [[ "$PROVIDER" == "ollama" ]]; then
  if ! command -v ollama >/dev/null 2>&1; then
    echo "ollama command not found; install from https://ollama.ai" >&2
    exit 1
  fi

  # Choose a model that exists on the host to avoid long pulls
  if command -v curl >/dev/null 2>&1; then
    TAGS=$(curl -sf "$OLLAMA_HOST/api/tags" | sed 's/"name":"/\n/g' | sed 's/".*//' | sed -n '2,$p' ) || true
  fi
  if [[ "$VERBOSE" == "1" ]]; then
    echo "[autofix][debug] available models on $OLLAMA_HOST: $TAGS" >&2
    echo "[autofix][debug] requested model: $MODEL" >&2
  fi
  if [[ -n "$TAGS" && "$TAGS" != *"$MODEL"* ]]; then
    for fallback in codegemma:2b gemma3:1b gemma3:4b; do
      if [[ "$TAGS" == *"$fallback"* ]]; then
        echo "[autofix] switching to available model $fallback" >&2
        MODEL="$fallback"
        break
      fi
    done
  fi

  echo "[autofix] sending context to ollama model $MODEL (host: $OLLAMA_HOST)" >&2
  cat <<REQ | ollama run "$MODEL" | tee "$RESPONSE_FILE"
$(cat "$PROMPT_FILE")

[context]
$(cat "$TMPDIR/context.txt")
REQ
else
  echo "LLM_PROVIDER=$PROVIDER not implemented; falling back to ollama" >&2
  if ! command -v ollama >/dev/null 2>&1; then
    exit 1
  fi
  cat <<REQ | ollama run "$MODEL" | tee "$RESPONSE_FILE"
$(cat "$PROMPT_FILE")

[context]
$(cat "$TMPDIR/context.txt")
REQ
fi

# 9) Optional auto-apply patches from response
if [[ "$AUTO_APPLY" = "1" ]]; then
  echo "[autofix] attempting to extract and apply patches" >&2
  python - <<'PY' "$RESPONSE_FILE"
import io, re, subprocess, sys, pathlib
resp=pathlib.Path(sys.argv[1]).read_text()
# extract code blocks tagged diff or Begin Patch
patches=[]
for m in re.finditer(r"```diff\n(.*?)```", resp, re.S):
    patches.append(m.group(1))
for m in re.finditer(r"\*\*\* Begin Patch\n(.*?)\*\*\* End Patch", resp, re.S):
    patches.append(m.group(0))
if not patches:
    sys.exit(0)

all_patch='\n'.join(patches)
patch_file=pathlib.Path('auto-apply.patch')
patch_file.write_text(all_patch)
print(f"[autofix] wrote patch to {patch_file}")
check=subprocess.run(['git','apply','--check',str(patch_file)],check=False)
if check.returncode!=0:
    print('[autofix] patch failed --check; not applying', file=sys.stderr)
    sys.exit(0)
subprocess.run(['git','apply','--whitespace=fix','--stat',str(patch_file)],check=False)
subprocess.run(['git','apply','--whitespace=fix',str(patch_file)],check=False)
PY
fi

echo "[autofix] done. Context cache: $TMPDIR" >&2

# 10) Record manifest for RAG/history
python - <<'PY' "$TMPDIR" "$PROVIDER" "$MODEL" "$AUTO_APPLY"
import json, pathlib, sys, time
root=pathlib.Path(sys.argv[1])
manifest={
  'provider': sys.argv[2],
  'model': sys.argv[3],
  'auto_apply': sys.argv[4]=='1',
  'timestamp': time.time(),
  'context_files': sorted([p.name for p in root.iterdir() if p.is_file()]),
}
root.joinpath('run.json').write_text(json.dumps(manifest, indent=2))
PY
