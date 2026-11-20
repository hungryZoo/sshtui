#!/bin/bash
# Codex 작업 실행기. tmux 세션 `sshtui`의 새 윈도우에서 지시문을 실행하고 종료까지 대기한다.
# 사용법: tasks/run.sh <지시문 파일> <reasoning effort>
set -euo pipefail

cd "$(dirname "$0")/.."
task="$1"
effort="$2"
name="$(basename "$task" .md)"
ts="$(date +%Y%m%d-%H%M%S)"
log="logs/codex_${name}_${ts}.log"
last="logs/codex_${name}_${ts}.last.md"
channel="sshtui-${name}-${ts}"
mkdir -p logs

cmd="CARGO_NET_OFFLINE=true codex exec -m gpt-6.2-sol -s workspace-write"
cmd+=" -c 'service_tier=\"ultrafast\"' -c model_reasoning_effort=${effort}"
cmd+=" -o '${last}' - < '${task}' 2>&1 | tee '${log}'; tmux wait-for -S '${channel}'"

if tmux has-session -t sshtui 2>/dev/null; then
  tmux new-window -d -t sshtui -n "$name" "$cmd"
else
  tmux new-session -d -s sshtui -n "$name" "$cmd"
fi

echo "log: ${log}"
echo "last: ${last}"
tmux wait-for "$channel"

# 속도 등급이 요청에서 빠졌다는 경고가 있으면 실패로 처리한다.
if grep -q "service tier" "$log"; then
  echo "SERVICE TIER WARNING FOUND" >&2
  grep "service tier" "$log" >&2
  exit 3
fi
echo "done"
