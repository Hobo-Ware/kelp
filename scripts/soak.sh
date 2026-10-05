#!/usr/bin/env bash
set -euo pipefail

hours="${1:-8}"
root="$(cd "$(dirname "$0")/.." && pwd)"
work="${KELP_SOAK_DIR:-$HOME/Library/Caches/kelp-soak}"
repo="$work/repo"
log="$work/samples.csv"

cargo build --release -p kelp --manifest-path "$root/Cargo.toml"
rm -rf "${work:?}"
mkdir -p "$work/home" "$repo"
kelp="$work/kelp"
cp "$root/target/release/kelp" "$kelp"

git -C "$repo" init -q -b main
git -C "$repo" config user.name "Soak Test"
git -C "$repo" config user.email "soak@example.com"
for i in $(seq 1 200); do
  echo "line $i" >> "$repo/notes.txt"
  git -C "$repo" add -A
  git -C "$repo" commit -q -m "seed commit $i"
done

export HOME="$work/home" KELP_OFFLINE=1 KELP_INSTANCE_SOCKET="$work/kelp.sock"
"$kelp" -w "$repo" >"$work/kelp.log" 2>&1 &
app=$!
trap 'kill "$app" 2>/dev/null || true' EXIT
sleep 5

churn() {
  local step="$1"
  case $((step % 6)) in
    0) echo "change $step" >> "$repo/notes.txt"
       git -C "$repo" commit -qam "soak commit $step" ;;
    1) git -C "$repo" switch -q -c "soak/$step" 2>/dev/null || git -C "$repo" switch -q main ;;
    2) echo "wip $step" >> "$repo/wip.txt" ;;
    3) git -C "$repo" stash push -q -u -m "soak $step" || true ;;
    4) git -C "$repo" switch -q main
       git -C "$repo" stash pop -q 2>/dev/null || true ;;
    5) if [ $((step / 6 % 20)) -eq 0 ]; then
         git -C "$repo" worktree add -q "$work/wt-$step" -b "wt/$step" 2>/dev/null || true
         script -q /dev/null "$kelp" "$work/wt-$step" </dev/null >/dev/null 2>&1 || true
       fi ;;
  esac
}

echo "elapsed_s,rss_kb,cpu_percent,threads" > "$log"
started=$(date +%s)
deadline=$((started + hours * 3600))
step=0
while [ "$(date +%s)" -lt "$deadline" ] && kill -0 "$app" 2>/dev/null; do
  churn "$step"
  step=$((step + 1))
  sleep 10
  if [ $((step % 3)) -eq 0 ]; then
    now=$(date +%s)
    rss=$(ps -o rss= -p "$app" | tr -d ' ')
    cpu=$(ps -o %cpu= -p "$app" | tr -d ' ')
    threads=$(ps -M -p "$app" | tail -n +2 | wc -l | tr -d ' ')
    echo "$((now - started)),$rss,$cpu,$threads" >> "$log"
  fi
done

summary() {
  python3 - "$log" <<'PY'
import csv, sys
rows = list(csv.DictReader(open(sys.argv[1])))
if rows:
    rss = [int(r["rss_kb"]) for r in rows]
    cpu = [float(r["cpu_percent"]) for r in rows]
    hours = int(rows[-1]["elapsed_s"]) / 3600
    print(f"samples={len(rows)} hours={hours:.2f} rss first={rss[0]//1024}MB max={max(rss)//1024}MB last={rss[-1]//1024}MB cpu avg={sum(cpu)/len(cpu):.2f}% max={max(cpu):.1f}% threads last={rows[-1]['threads']}")
PY
}

if kill -0 "$app" 2>/dev/null; then
  echo "Kelp still running after $step churn steps. Samples: $log"
  summary
else
  wait "$app" 2>/dev/null && code=0 || code=$?
  echo "Kelp exited early with code $code after $step steps. Log: $work/kelp.log" >&2
  tail -5 "$work/kelp.log" >&2
  summary
  exit 1
fi
