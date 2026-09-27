#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
work="${KELP_SHOWCASE_DIR:-$HOME/Library/Caches/kelp-showcase}"
site="$root/site"
raw="$work/raw"
kelp="$root/target/release/kelp"
fonts="$root/crates/kelp/assets/fonts"
mkdir -p "$work" "$raw" "$site"

cargo build --release -p kelp --manifest-path "$root/Cargo.toml"

echo "== showcase repos"
if [ ! -d "$work/git" ]; then
  git clone -q --bare --filter=blob:none https://github.com/git/git.git "$work/git"
fi
git -C "$work/git" commit-graph write --reachable >/dev/null 2>&1 || true

for dir in kelp kelp-review kelp-fix; do rm -rf "${work:?}/${dir:?}"; done
git clone -q "$root" "$work/kelp"
cd "$work/kelp"
git config user.name "Vlad Jerca"
git config user.email "6339681+vladjerca@users.noreply.github.com"
git branch -q feat/inline-review
git branch -q fix/worktree-paths HEAD~3
git worktree add -q ../kelp-review feat/inline-review
git worktree add -q ../kelp-fix fix/worktree-paths
printf '\n## Notes\n\nTry a denser sidebar.\n' >> README.md
printf 'Scratch notes for the next release.\n' > NOTES.md
git add NOTES.md
python3 - "$work/kelp" <<'PY'
import json, subprocess, sys
repo = sys.argv[1]
sha = subprocess.check_output(["git", "-C", repo, "log", "-1", "--format=%H", "--grep=^feat: staging and committing"], text=True).strip()
path = "crates/kelp-core/src/diff.rs"
text = subprocess.check_output(["git", "-C", repo, "show", f"{sha}:{path}"], text=True).splitlines()
i = next(n for n, line in enumerate(text) if line.strip() == "pub raw: String,")
anchor = {"side": "New", "line_hint": i + 1, "text": text[i], "before": text[max(0, i - 2):i], "after": text[i + 1:i + 3]}
comment = lambda body, t: {"author": "Vlad Jerca", "time": t, "body": body}
review = {"threads": [{
    "id": 1, "path": path, "commit": sha, "anchor": anchor, "resolved": False,
    "comments": [
        comment("Keep the raw line next to the display text? Patches need exact bytes (tabs, CRLF), the view wants tabs expanded.", 1790470000),
        comment("Yes - hunk_patch writes raw, the diff view paints text. Covered by the CRLF round-trip test.", 1790471800),
    ]}], "next_id": 1}
import os
os.makedirs(f"{repo}/.git/kelp", exist_ok=True)
json.dump(review, open(f"{repo}/.git/kelp/comments.json", "w"), indent=2)
print(sha)
PY
review_sha="$(git log -1 --format=%H --grep='^feat: staging and committing')"
cd "$root"

echo "== screenshots"
shoot() {
  local name="$1" repo="$2"; shift 2
  env KELP_SCREENSHOT="$raw/$name.png" "$@" timeout 120 "$kelp" "$repo"
}
shoot graph "$work/git" KELP_SCREENSHOT_WAIT=12
shoot review "$work/kelp" KELP_OFFLINE=1 KELP_SCREENSHOT_WAIT=1.5 \
  KELP_SELECT_COMMIT="$review_sha" KELP_OPEN_DIFF=path:crates/kelp-core/src/diff.rs
shoot staging "$work/kelp" KELP_SCREENSHOT_WAIT=4 KELP_SELECT_WIP=1 KELP_OPEN_DIFF=unstaged:README.md
shoot worktrees "$work/kelp" KELP_SCREENSHOT_WAIT=4 KELP_OPEN_WORKTREES=1

echo "== compress"
compress() {
  local src="$1" dst="$2" colors="$3"
  pngquant --force --speed 1 --strip "$colors" --output "$dst" "$src"
  oxipng -q -o 6 --strip safe "$dst"
  local psnr
  psnr="$( (magick compare -metric PSNR "$src" "$dst" null: 2>&1 || true) | awk '{print $1}')"
  printf '  %-22s %6s KB -> %5s KB  PSNR %s dB\n' "$(basename "$dst")" \
    "$(( $(stat -f %z "$src") / 1024 ))" "$(( $(stat -f %z "$dst") / 1024 ))" "$psnr"
  awk -v p="$psnr" 'BEGIN { exit !(p == "inf" || p + 0 >= 40) }' || { echo "PSNR below 40 dB for $dst" >&2; exit 1; }
}
for name in graph review staging worktrees; do
  magick "$raw/$name.png" -resize 2200x "$raw/$name-2200.png"
  compress "$raw/$name-2200.png" "$site/shot-$name.png" 256
done

echo "== icons"
inkscape "$root/crates/kelp/assets/icon.svg" --export-type=png --export-width=180 --export-filename="$raw/apple-touch-icon.png" >/dev/null 2>&1
inkscape "$root/crates/kelp/assets/icon.svg" --export-type=png --export-width=64 --export-filename="$raw/favicon.png" >/dev/null 2>&1
inkscape "$root/crates/kelp/assets/mascot.svg" --export-type=png --export-width=720 --export-filename="$raw/mascot.png" >/dev/null 2>&1
compress "$raw/apple-touch-icon.png" "$site/apple-touch-icon.png" 64
compress "$raw/favicon.png" "$site/favicon.png" 64
compress "$raw/mascot.png" "$site/mascot.png" 64
cp "$root/crates/kelp/assets/mascot.svg" "$site/mascot.svg"

echo "== og image"
magick -size 1200x630 radial-gradient:'#1e2a22'-'#15181e' \
  \( "$raw/graph.png" -crop 1100x1000+720+180 -resize 460x -alpha set \
     -channel A -fx 'a*0.6*min(1,i/(w*0.45))*min(1,(h-j)/(h*0.3))' +channel \) -geometry +760+40 -composite \
  \( "$raw/mascot.png" -resize 330x330 \) -geometry +40+150 -composite \
  -font "$fonts/IBMPlexSans-SemiBold.ttf" -fill '#f3f1ec' -pointsize 104 -annotate +380+285 'Kelp' \
  -font "$fonts/IBMPlexSans-Regular.ttf" -fill '#b4b9c2' -pointsize 31 -annotate +384+342 'The git graph, grown in Rust.' \
  -pointsize 25 -fill '#9aa1ad' -annotate +384+387 'Fast, native, 0% CPU when idle.' \
  -font "$fonts/JetBrainsMono-Regular.ttf" -fill '#8fd16a' -pointsize 22 -annotate +384+462 '$ brew install hobo-ware/tap/kelp' \
  "$raw/og.png"
compress "$raw/og.png" "$site/kelp-og.png" 256

echo "Done. Assets in $site"
