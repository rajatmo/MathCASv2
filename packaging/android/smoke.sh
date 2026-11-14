#!/usr/bin/env bash
# Smoke test of the Android package on a running emulator or a connected device:
#
#   packaging/android/smoke.sh PACKAGE.apk [WORK_DIR]
#
# Installs the package, starts the notebook with a self-test (it types an input, evaluates it with the kernel, makes
# a plot and takes screenshots), and checks the self-test's log, the screenshots and the system log. The work folder
# (default: a temporary one) keeps the log, the screenshots and the app's lines of the system log.
#
# Needs adb (the Android SDK's platform-tools; ANDROID_HOME names the SDK) and one device: `adb devices` must list
# exactly one, or ANDROID_SERIAL names it.
set -uo pipefail

apk="${1:?usage: smoke.sh PACKAGE.apk [WORK_DIR]}"
work="${2:-$(mktemp -d)}"
mkdir -p "$work"
here="$(cd "$(dirname "$0")" && pwd)"

case "$(uname -s)" in
    Linux*) sdk_default="$HOME/Android/Sdk"; exe="" ;;
    Darwin*) sdk_default="$HOME/Library/Android/sdk"; exe="" ;;
    *) sdk_default="${LOCALAPPDATA:-}/Android/Sdk"; exe=".exe" ;;
esac
adb="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$sdk_default}}/platform-tools/adb$exe"
[ -x "$adb" ] || adb="$(command -v adb)" || { echo "smoke: adb not found (set ANDROID_HOME)" >&2; exit 1; }
# Git Bash would rewrite the device's /sdcard/... paths into Windows paths
export MSYS_NO_PATHCONV=1 MSYS2_ARG_CONV_EXCL='*'
sh() { "$adb" shell "$@" | tr -d '\r'; }

pkg=io.github.rajatmo.mathcas
files="/sdcard/Android/data/$pkg/files"
passed=0
failed=0
check() { # name, wanted, got
    if [ "$2" = "$3" ]; then
        passed=$((passed + 1))
        echo "ok   $1"
    else
        failed=$((failed + 1))
        echo "FAIL $1: wanted [$2], got [$3]"
    fi
}

echo "smoke: device $(sh getprop ro.product.model), Android $(sh getprop ro.build.version.release), $(sh getprop ro.product.cpu.abi)"
"$adb" install -r "$apk" >"$work/install.out" 2>&1
check "the package installs" "Success" "$(tail -1 "$work/install.out" | tr -d '\r')"

# the self-test script goes into the app's notebooks folder; the app runs it when it starts
sh am force-stop "$pkg"
sh rm -rf "$files/smoke" "$files/selftest.txt"
sh ls "$files" >"$work/before.txt" 2>/dev/null
sh mkdir -p "$files/smoke"
sed -e "s|@OUT@|$files/smoke|g" "$here/smoke-notebook.selftest" >"$work/selftest.txt"
"$adb" push "$work/selftest.txt" "$files/selftest.txt" >/dev/null
"$adb" logcat -c
sh am start -W -n "$pkg/.MathCasActivity" >"$work/start.out"
check "the activity starts" "1" "$(grep -c '^Status: ok' "$work/start.out")"

# the self-test's last step writes "finished" to its log (at most 3 minutes; a look every 2 seconds)
tries=0
until sh cat "$files/smoke/selftest.log" 2>/dev/null | grep -q '^finished'; do
    tries=$((tries + 1))
    if [ "$tries" -ge 90 ]; then break; fi
    printf '\rsmoke: waiting for the self-test [%d%%]' $((tries * 100 / 90))
    "$adb" shell sleep 2
done
printf '\n'

"$adb" pull "$files/smoke" "$work" >/dev/null 2>&1
"$adb" logcat -d -s MathCAS:V >"$work/logcat.txt" 2>&1
sh rm -rf "$files/selftest.txt" "$files/smoke"
sh am force-stop "$pkg"
# the calculation the self-test made is not the user's: the notebooks that were not there before go
sh ls "$files" >"$work/after.txt" 2>/dev/null
grep -v -x -F -f "$work/before.txt" "$work/after.txt" | grep '^Calculation [0-9]*\.mcn$' | while read -r made; do
    sh rm -f "$files/$made"
done

log="$work/smoke/selftest.log"
check "the self-test ran to its end" "1" "$(grep -c '^finished' "$log" 2>/dev/null)"
check "the integral's value" "1" "$(grep -c '^result smoke/integral: pass' "$log" 2>/dev/null)"
check "the plot" "1" "$(grep -c '^result smoke/plot: pass' "$log" 2>/dev/null)"
check "three screenshots" "3" "$(ls "$work/smoke"/*.png 2>/dev/null | wc -l | tr -d ' ')"
check "no panic in the system log" "0" "$(grep -c 'panic' "$work/logcat.txt")"

echo "smoke: $passed passed, $failed failed; files in $work"
[ "$failed" -eq 0 ]
