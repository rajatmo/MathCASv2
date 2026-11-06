#!/usr/bin/env bash
# Smoke test of a staged or unpacked Linux package: the REPL evaluates a few expressions with known results, the
# kernel answers, and, when a display is available (WAYLAND_DISPLAY or DISPLAY), the documentation window runs its
# self-test (packaging/linux/smoke-docs.selftest) and saves two screenshots.
# Usage: packaging/linux/smoke.sh PACKAGE_DIR [WORK_DIR]      exit 0 when every check passed
set -uo pipefail

pkg="$(cd "${1:?usage: smoke.sh PACKAGE_DIR [WORK_DIR]}" && pwd)"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
work="${2:-$(mktemp -d)}"
mkdir -p "$work"
failures=0
step=0
total=6

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

repl() { timeout 60 "$pkg/bin/mc-repl" -e "$1" 2>&1 | tail -n 1; }

check "factor" "(-1 + x)*(1 + x)*(1 + x^2)" "$(repl 'factor(x^4 - 1)')"
check "exact integral" "(1/2)*pi" "$(repl 'integrate(1/(1 + x^2), [x, 0, inf])')"
check "big integer" "1267650600228229401496703205376" "$(repl '2^100')"
check "example script" "0" "$(timeout 60 "$pkg/bin/mc-repl" "$pkg/examples/hello.mc" >/dev/null 2>&1; echo $?)"
# the kernel: one request, then end of input ends it cleanly
kernel_out="$(printf '%s\n' '{"id": 1, "type": "eval", "src": "1 + 1"}' | timeout 60 "$pkg/bin/mc-kernel" 2>&1)"
check "kernel answers" "yes" "$(case "$kernel_out" in *'"printed":"2"'*) echo yes ;; *) echo "$kernel_out" ;; esac)"

if [ -n "${WAYLAND_DISPLAY:-}${DISPLAY:-}" ]; then
  cp "$here/smoke-docs.selftest" "$work/smoke-docs.selftest"
  rm -f "$work/selftest.log" "$work/home.png" "$work/integrate.png"
  MCDOCS_SELFTEST="$work/smoke-docs.selftest" timeout 180 "$pkg/bin/mc-docs" > "$work/mc-docs.out" 2>&1
  status="log:$(grep -c '^done$' "$work/selftest.log" 2>/dev/null || echo 0)"
  status="$status shots:$(ls "$work/home.png" "$work/integrate.png" 2>/dev/null | wc -l)"
  check "mc-docs self-test" "log:1 shots:2" "$status"
else
  step=$((step + 1))
  echo "[$step/$total $((step * 100 / total))%] skip mc-docs self-test (no display)"
fi

echo "smoke: $failures failure(s); work folder $work"
[ "$failures" -eq 0 ]
