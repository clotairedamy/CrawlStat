#!/usr/bin/env bash
#
# Regenerate the marketing screenshots.
#
# Opens the app once per screen at a fixed window size, captures just that
# window's rectangle, and writes PNG plus WebP into the website's public
# folder.
#
# This exists because the screenshots on crawlstat.com went stale: refreshing
# them was a manual chore of launching, clicking through, cropping and
# exporting, so nobody did it. Now it is one command, which is the only way
# screenshots ever stay current.
#
#     ./packaging/screenshots.sh [path-to-website-repo]
#
# The app must be built in debug — the route override that makes this
# repeatable is compiled out of release builds.

set -euo pipefail

SITE="${1:-/Users/clotaire/Documents/Development/Crawlstat Website}"
# The crawl the screenshots are taken against. Most screens show nothing
# without one open, which is how the first run of this script produced a set
# of "No crawl open" placeholders.
PROJECTS="${HOME}/Library/Containers/com.crawlstat.crawlstat/Data/Library/Application Support/CrawlStat/projects"
CRAWL="${CRAWLSTAT_SCREENSHOT_CRAWL:-$(ls -S "${PROJECTS}"/*.db 2>/dev/null | head -1)}"
OUT="${SITE}/public/screenshots"
APP="app/build/macos/Build/Products/Debug/crawlstat.app"
WIDTH=1440
HEIGHT=900

[ -d "$APP" ] || {
  echo "error: no debug build at ${APP}"
  echo "       cd app && flutter build macos --debug"
  exit 1
}
mkdir -p "$OUT"

[ -n "$CRAWL" ] && [ -f "$CRAWL" ] || {
  echo "error: no crawl database found in ${PROJECTS}"
  echo "       run a crawl first, or set CRAWLSTAT_SCREENSHOT_CRAWL"
  exit 1
}
echo "==> Using crawl: $(basename "$CRAWL")"

# id:route — the screens worth showing, in the order the site uses them.
SCREENS=(
  "projects:/"
  "crawl:/crawl"
  "results:/results"
  "issues:/issues"
  "plan:/plan"
  "visualise:/visualise"
  "reader-flow:/reader-flow"
  "compare:/compare"
  "settings:/settings"
)

shoot() {
  local name="$1" route="$2"

  # Wait for the old instance to actually be gone. Without this the next
  # capture can land on the dying previous window, which is how the first run
  # produced screenshots whose filenames did not match their contents.
  pkill -f 'Debug/crawlstat.app' 2>/dev/null || true
  for _ in $(seq 1 40); do
    pgrep -f 'Debug/crawlstat.app' >/dev/null 2>&1 || break
    sleep 0.25
  done

  open -n "$APP" \
    --env CRAWLSTAT_DEV_PRO=1 \
    --env "CRAWLSTAT_INITIAL_ROUTE=${route}" \
    --env "CRAWLSTAT_INITIAL_PROJECT=${CRAWL}"
  # Long enough for the Rust bridge to come up, the crawl file to open and
  # the entrance animations to finish.
  sleep 9

  osascript <<OSA >/dev/null 2>&1 || true
tell application "crawlstat" to activate
delay 1
tell application "System Events" to tell process "crawlstat"
  set position of window 1 to {0, 33}
  set size of window 1 to {${WIDTH}, ${HEIGHT}}
end tell
OSA
  sleep 3

  # Capture the window's rectangle, never the whole screen: nothing of
  # whatever else is open gets published to a website by accident.
  screencapture -x -o -R "0,33,${WIDTH},${HEIGHT}" "${OUT}/${name}-dark.png"
  echo "  ${name}-dark.png"
}

echo "==> Capturing ${#SCREENS[@]} screens at ${WIDTH}x${HEIGHT}"
for entry in "${SCREENS[@]}"; do
  shoot "${entry%%:*}" "${entry#*:}"
done

pkill -f 'Debug/crawlstat.app' 2>/dev/null || true

echo "==> Converting to WebP"
python3 - "$OUT" <<'PY'
import glob, os, sys
try:
    from PIL import Image
except ImportError:
    sys.exit("  Pillow not installed — PNGs written, WebP skipped")

out = sys.argv[1]
png_total = webp_total = 0
for path in sorted(glob.glob(os.path.join(out, '*.png'))):
    image = Image.open(path)
    target = path.replace('.png', '.webp')
    image.save(target, 'WEBP', quality=86, method=6)
    png_total += os.path.getsize(path)
    webp_total += os.path.getsize(target)
print(f"  {png_total/1024/1024:.2f} MB PNG -> {webp_total/1024/1024:.2f} MB WebP")
PY

echo
echo "==> Done. Review them, then commit the website repo."
