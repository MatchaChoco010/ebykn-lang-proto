#!/usr/bin/env bash
# SessionStart フック：道具の有無と版を表示し、pre-commit フックを有効にする。
# 何が失敗してもセッションは止めない。
cd "${CLAUDE_PROJECT_DIR:-$(dirname "$0")/../..}" 2>/dev/null || exit 0

for tool in cargo mdbook verus; do
  if command -v "$tool" >/dev/null 2>&1; then
    echo "[session-start] $tool: $("$tool" --version 2>&1 | head -n 1)"
  else
    echo "[session-start] $tool: なし"
  fi
done

if [ -z "$(git config --get core.hooksPath 2>/dev/null)" ]; then
  if git config core.hooksPath .githooks 2>/dev/null; then
    echo "[session-start] core.hooksPath を .githooks に設定しました"
  else
    echo "[session-start] core.hooksPath を設定できませんでした"
  fi
fi
exit 0
