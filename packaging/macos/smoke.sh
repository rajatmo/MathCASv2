#!/usr/bin/env bash
# Smoke test of a macOS disk image: the image mounts, the bundle's signature verifies, the REPL evaluates a few
# expressions with known results, the kernel answers, and the two windows (the documentation window and the
# notebook) run a self-test each and save screenshots.
# Usage: packaging/macos/smoke.sh IMAGE.dmg [WORK_DIR]      exit 0 when every check passed
set -uo pipefail

dmg="${1:?usage: smoke.sh IMAGE.dmg [WORK_DIR]}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
work="${2:-$(mktemp -d)}"
mkdir -p "$work"
work="$(cd "$work" && pwd)"
failures=0
step=0
total=9

check() {
  step=$((step + 1))
  local what="$1" expected="$2" actual="$3"
  if [ "$actual" = "$expected" ]; then
    echo "[$step/$total $((step * 100 / total))%] ok   $what"
  else
    echo "[$step/$total $((step * 100 / total))%] FAIL $what: expected '$expected', got '$actual'"
    failures=$((failures + 1))
  fi
}

# run a command with a time limit (macOS has no `timeout`): the alarm ends it; standard input stays the caller's
limited() {
  local secs="$1"; shift
  # (the block form of exec never splits a lone argument into words: the bundle's path has a space)
  perl -e 'alarm shift; exec { $ARGV[0] } @ARGV or die "exec: $!"' "$secs" "$@"
}

mount="$work/mount"
mkdir -p "$mount"
hdiutil attach "$dmg" -nobrowse -readonly -mountpoint "$mount" >/dev/null
check "image mounts" "yes" "$([ -d "$mount/MathCAS Notebook.app" ] && echo yes || ls "$mount")"
app="$mount/MathCAS Notebook.app"
bin="$app/Contents/MacOS"

check "signature verifies" "0" "$(codesign --verify --deep --strict "$app" >/dev/null 2>&1; echo $?)"

repl() { limited 60 "$bin/mc-repl" -e "$1" 2>&1 | tail -n 1; }
check "factor" "(-1 + x)*(1 + x)*(1 + x^2)" "$(repl 'factor(x^4 - 1)')"
check "exact integral" "(1/2)*pi" "$(repl 'integrate(1/(1 + x^2), [x, 0, inf])')"
check "compiled numerics" "true" "$(repl 'abs(n_integrate(exp(-x^2), [x, -inf, inf]) - sqrt(pi)) < 10^-8')"
check "example script" "0" "$(limited 60 "$bin/mc-repl" "$mount/examples/hello.mc" >/dev/null 2>&1; echo $?)"
kernel_out="$(printf '%s\n' '{"id": 1, "type": "eval", "src": "1 + 1"}' | limited 60 "$bin/mc-kernel" 2>&1)"
if printf '%s' "$kernel_out" | grep -q '"printed":"2"'; then kernel_ok=yes; else kernel_ok="$kernel_out"; fi
check "kernel answers" "yes" "$kernel_ok"

# the documentation window: home, a search, a function page
cp "$here/smoke-docs.selftest" "$work/smoke-docs.selftest"
rm -f "$work/selftest.log" "$work/home.png" "$work/integrate.png"
(cd "$work" && MCDOCS_SELFTEST="$work/smoke-docs.selftest" limited 240 "$bin/mc-docs" > "$work/mc-docs.out" 2>&1)
count() { grep -c "$1" "$2" 2>/dev/null | head -n 1 | tr -d ' ' || true; }
status="log:$(count '^done$' "$work/selftest.log")"
status="$status shots:$(ls "$work/home.png" "$work/integrate.png" 2>/dev/null | wc -l | tr -d ' ')"
check "mc-docs self-test" "log:1 shots:2" "$status"

# the notebook: type an input, evaluate it with the kernel, a plot, a screenshot
nb="$work/notebook"
mkdir -p "$nb/cfg"
sed -e "s|@OUT@|$nb|g" "$here/smoke-notebook.selftest" > "$nb/smoke-notebook.selftest"
(cd "$nb" && MCNB_SELFTEST="$nb/smoke-notebook.selftest" MCNB_CONFIG_DIR="$nb/cfg" limited 300 "$bin/mc-notebook" > "$nb/mc-notebook.out" 2>&1)
status="shots:$(ls "$nb/typed.png" "$nb/evaluated.png" 2>/dev/null | wc -l | tr -d ' ')"
status="$status ok:$(count '^  ok output' "$nb/selftest.log")"
status="$status fail:$(count 'FAIL' "$nb/selftest.log")"
check "mc-notebook self-test" "shots:2 ok:1 fail:0" "$status"

hdiutil detach "$mount" >/dev/null 2>&1 || true
echo "smoke: $failures failure(s); work folder $work"
[ "$failures" -eq 0 ]
