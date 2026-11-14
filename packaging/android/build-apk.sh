#!/usr/bin/env bash
# Builds the Android package of the notebook: MathCAS-v2-<version>-<commit>-android.apk.
#
#   packaging/android/build-apk.sh [OUT_DIR]
#
# The package holds, for each ABI, the notebook's native library (crates/mc-notebook-android) and the kernel program
# under a library's name (lib/<abi>/libmc_kernel.so), the activity's few Java methods (java/, compiled to
# classes.dex), the manifest and the icon. No Gradle: the JDK's compiler and the Android SDK's own tools put it
# together (javac, d8, aapt2 link, aapt add, zipalign, apksigner).
#
# Needs: the Android SDK with a platform, build-tools and the NDK (Android Studio installs them); a JDK (Android
# Studio's is used when JAVA_HOME is not set); the Rust targets (rustup target add aarch64-linux-android
# x86_64-linux-android). Runs on Linux, macOS and Windows (Git Bash).
#
# Environment:
#   ANDROID_HOME      the SDK (default: the place Android Studio installs it to)
#   ANDROID_NDK_HOME  the NDK (default: the newest one in the SDK)
#   ABIS              the ABIs to build, default "arm64-v8a x86_64" (phones and tablets; emulators on a PC)
#   KEYSTORE, KEY_ALIAS, KEYSTORE_PASS   the signing key; default: a debug key made on first use and kept in
#                     $HOME/.android/debug.keystore (packages signed with it install, but are not for a store)
#   CARGO_TARGET_DIR  where cargo builds (default: target/)
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
out="${1:-$root/target/android}"
mkdir -p "$out"
out="$(cd "$out" && pwd)"

case "$(uname -s)" in
    Linux*) host=linux-x86_64; exe=""; sdk_default="$HOME/Android/Sdk" ;;
    Darwin*) host=darwin-x86_64; exe=""; sdk_default="$HOME/Library/Android/sdk" ;;
    *) host=windows-x86_64; exe=".exe"; sdk_default="${LOCALAPPDATA:-}/Android/Sdk" ;;
esac
# a path the native tools of this system take (Windows tools do not know /c/...)
native() { if command -v cygpath >/dev/null 2>&1; then cygpath -m "$1"; else printf '%s' "$1"; fi; }
newest() { ls -1 "$1" 2>/dev/null | sort -V | tail -1; }

sdk="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$sdk_default}}"
[ -d "$sdk" ] || { echo "build-apk: no Android SDK at $sdk (set ANDROID_HOME)" >&2; exit 1; }
ndk="${ANDROID_NDK_HOME:-$sdk/ndk/$(newest "$sdk/ndk")}"
tools="$sdk/build-tools/$(newest "$sdk/build-tools")"
platform="$sdk/platforms/$(newest "$sdk/platforms")"
bin="$ndk/toolchains/llvm/prebuilt/$host/bin"
for f in "$bin/clang$exe" "$tools/aapt2$exe" "$tools/aapt$exe" "$tools/zipalign$exe" "$tools/lib/apksigner.jar" "$tools/lib/d8.jar" "$platform/android.jar"; do
    [ -e "$f" ] || { echo "build-apk: missing $f" >&2; exit 1; }
done

# a JDK for javac, d8, apksigner and keytool: JAVA_HOME, else java on the PATH, else Android Studio's
java_bin=""
for c in "${JAVA_HOME:-}/bin" "$(dirname "$(command -v java 2>/dev/null || echo /none/java)")" \
         "/c/Program Files/Android/Android Studio/jbr/bin" "/opt/android-studio/jbr/bin" \
         "/Applications/Android Studio.app/Contents/jbr/Contents/Home/bin"; do
    if [ -x "$c/java$exe" ] && [ -x "$c/javac$exe" ]; then java_bin="$c"; break; fi
done
[ -n "$java_bin" ] || { echo "build-apk: no JDK found (set JAVA_HOME)" >&2; exit 1; }

# the lowest Android version the package runs on (the manifest's minSdkVersion): the NDK links against it
min_sdk="$(sed -n 's/.*minSdkVersion="\([0-9]*\)".*/\1/p' "$here/AndroidManifest.xml")"
version="$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' "$root/Cargo.toml")"
commit="$(git -C "$root" rev-parse --short=8 HEAD 2>/dev/null || echo unknown)"
# 0.14.0 -> 1400: a number that grows with the version
IFS=. read -r major minor patch <<<"$version"
code=$((major * 10000 + minor * 100 + patch))
abis="${ABIS:-arm64-v8a x86_64}"
echo "build-apk: MathCAS v2 $version ($commit), ABIs: $abis, Android $min_sdk and later"

stage="$out/stage"
rm -rf "$stage"
mkdir -p "$stage/lib" "$stage/res/mipmap-xxxhdpi"
target_dir="${CARGO_TARGET_DIR:-$root/target}"

n=0
total=$(echo $abis | wc -w)
for abi in $abis; do
    n=$((n + 1))
    case "$abi" in
        arm64-v8a) triple=aarch64-linux-android ;;
        x86_64) triple=x86_64-linux-android ;;
        armeabi-v7a) triple=armv7-linux-androideabi ;;
        x86) triple=i686-linux-android ;;
        *) echo "build-apk: unknown ABI $abi" >&2; exit 1 ;;
    esac
    # the NDK's compiler is one program for every target: the target and the Android version are named to it
    clang_target="$triple$min_sdk"
    [ "$abi" = armeabi-v7a ] && clang_target="armv7a-linux-androideabi$min_sdk"
    env_name="$(echo "$triple" | tr 'a-z-' 'A-Z_')"
    under="$(echo "$triple" | tr '-' '_')"
    echo "build-apk: [$n/$total] building $abi"
    env "CARGO_TARGET_${env_name}_LINKER=$(native "$bin/clang$exe")" \
        "CARGO_TARGET_${env_name}_RUSTFLAGS=-C link-arg=--target=$clang_target" \
        "CC_$under=$(native "$bin/clang$exe")" "AR_$under=$(native "$bin/llvm-ar$exe")" \
        "CFLAGS_$under=--target=$clang_target" \
        cargo build --release --manifest-path "$(native "$root/Cargo.toml")" --target "$triple" \
        -p mc-notebook-android -p mc-kernel
    built="$target_dir/$triple/release"
    mkdir -p "$stage/lib/$abi"
    # without symbols: they are most of the size
    "$bin/llvm-strip$exe" -o "$stage/lib/$abi/libmc_notebook_android.so" "$built/libmc_notebook_android.so"
    "$bin/llvm-strip$exe" -o "$stage/lib/$abi/libmc_kernel.so" "$built/mc-kernel"
done

cp "$root/assets/app/icon_256.png" "$stage/res/mipmap-xxxhdpi/icon.png"
sed -e "s/@VERSION@/$version/" -e "s/@CODE@/$code/" "$here/AndroidManifest.xml" >"$stage/AndroidManifest.xml"

echo "build-apk: compiling the activity's Java"
mkdir -p "$stage/classes"
(cd "$here/java" && find . -name '*.java' | sort >"$stage/sources.txt" &&
    "$java_bin/javac$exe" -nowarn -Xlint:-options -source 8 -target 8 -encoding UTF-8         -bootclasspath "$(native "$platform/android.jar")" -classpath "$(native "$tools/core-lambda-stubs.jar")" -d "$(native "$stage/classes")" @"$(native "$stage/sources.txt")")
(cd "$stage/classes" && find . -name '*.class' | sort >"$stage/classes.txt" &&
    "$java_bin/java$exe" -cp "$(native "$tools/lib/d8.jar")" com.android.tools.r8.D8 --release --min-api "$min_sdk"         --lib "$(native "$platform/android.jar")" --output "$(native "$stage")" @"$(native "$stage/classes.txt")")
[ -f "$stage/classes.dex" ] || { echo "build-apk: no classes.dex" >&2; exit 1; }

echo "build-apk: packaging"
"$tools/aapt2$exe" compile --dir "$(native "$stage/res")" -o "$(native "$stage/res.zip")"
"$tools/aapt2$exe" link -o "$(native "$stage/unaligned.apk")" --manifest "$(native "$stage/AndroidManifest.xml")" \
    -I "$(native "$platform/android.jar")" "$(native "$stage/res.zip")"
(cd "$stage" && { echo classes.dex; find lib -type f | sort; } | while read -r f; do "$tools/aapt$exe" add unaligned.apk "$f" >/dev/null; done)

apk="$out/MathCAS-v2-$version-$commit-android.apk"
rm -f "$apk"
"$tools/zipalign$exe" -p 4 "$(native "$stage/unaligned.apk")" "$(native "$apk")"

keystore="${KEYSTORE:-$HOME/.android/debug.keystore}"
alias="${KEY_ALIAS:-androiddebugkey}"
pass="${KEYSTORE_PASS:-android}"
if [ ! -f "$keystore" ]; then
    echo "build-apk: making a debug key at $keystore"
    mkdir -p "$(dirname "$keystore")"
    "$java_bin/keytool$exe" -genkeypair -keystore "$(native "$keystore")" -storepass "$pass" -keypass "$pass" \
        -alias "$alias" -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=Android Debug,O=Android,C=US" >/dev/null
fi
"$java_bin/java$exe" -jar "$(native "$tools/lib/apksigner.jar")" sign --ks "$(native "$keystore")" \
    --ks-key-alias "$alias" --ks-pass "pass:$pass" --key-pass "pass:$pass" "$(native "$apk")"
"$java_bin/java$exe" -jar "$(native "$tools/lib/apksigner.jar")" verify "$(native "$apk")"
rm -f "$apk.idsig"

echo "build-apk: $apk ($(($(wc -c <"$apk") / 1048576)) MB)"
"$tools/aapt$exe" list "$(native "$apk")" | sed 's/^/  /'
