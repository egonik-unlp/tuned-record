#!/usr/bin/env bash
# build-windows.sh — builds the Windows app and its setup program on this Linux machine.
#
#   scripts/build-windows.sh            release build + target/windows/tuned-record-setup-<ver>.exe
#                                       (and loopback_check.exe, for checking alignment on Windows)
#   scripts/build-windows.sh --smoke    also starts the .exe under Wine for a few seconds
#
# Cross-compiles to x86_64-pc-windows-gnu with MinGW (linker and C++ settings live in
# .cargo/config.toml), checks that the .exe needs only Windows system DLLs, and packages it with
# NSIS (installer/tuned-record.nsi). Prerequisites (Debian/Ubuntu):
#   sudo apt install mingw-w64 nsis clang libclang-dev unzip curl
#   rustup target add x86_64-pc-windows-gnu

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TARGET=x86_64-pc-windows-gnu
OUT="$ROOT/target/windows"
SDK_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/tuned-record/asiosdk"
SDK_URL="https://www.steinberg.net/asiosdk"
SMOKE=false
[ "${1:-}" = "--smoke" ] && SMOKE=true

fail() { echo "build-windows: $*" >&2; exit 1; }

# 1. Prerequisites.
missing=()
for tool in x86_64-w64-mingw32-gcc-posix x86_64-w64-mingw32-g++-posix x86_64-w64-mingw32-objdump \
            makensis clang curl unzip; do
  command -v "$tool" >/dev/null || missing+=("$tool")
done
$SMOKE && { command -v wine >/dev/null || missing+=(wine); }
[ ${#missing[@]} -eq 0 ] || fail "missing: ${missing[*]} (see the header of this script)"
rustup target list --installed | grep -qx "$TARGET" \
  || fail "Rust target missing: rustup target add $TARGET"
# .cargo/config.toml points at GCC 13's static libstdc++; say so if the toolchain moved.
gcc_lib="$(dirname "$(x86_64-w64-mingw32-gcc-posix -print-libgcc-file-name)")"
grep -qF "native=$gcc_lib\"" "$ROOT/.cargo/config.toml" \
  || fail "MinGW's GCC lib dir is $gcc_lib: update the -L path in .cargo/config.toml"

# 2. ASIO SDK, downloaded once (asio-sys would otherwise fetch it into /tmp on every reboot).
if [ ! -d "$SDK_DIR/common" ]; then
  echo "Downloading the Steinberg ASIO SDK into $SDK_DIR"
  tmp="$(mktemp -d)"
  curl -fsSL --output "$tmp/asiosdk.zip" "$SDK_URL"
  unzip -q "$tmp/asiosdk.zip" -d "$tmp/x"
  sdk="$(dirname "$(find "$tmp/x" -type d -name common -print -quit)")"
  [ -d "$sdk/host" ] || fail "unexpected ASIO SDK layout in $SDK_URL"
  mkdir -p "$(dirname "$SDK_DIR")"
  rm -rf "$SDK_DIR"
  mv "$sdk" "$SDK_DIR"
  rm -rf "$tmp"
fi
export CPAL_ASIO_DIR="$SDK_DIR"

# 3. Build.
cd "$ROOT"
cargo build --release --target "$TARGET" --bin tuned-record --example loopback_check
EXE="$ROOT/target/$TARGET/release/tuned-record.exe"

# 4. Only Windows system DLLs may be imported: anything else would be missing on the other PC.
allowed='^(advapi32|bcrypt|bcryptprimitives|combase|comctl32|comdlg32|crypt32|d3d11|d3d12|d3dcompiler_47|dwmapi|dxgi|gdi32|imm32|kernel32|mmdevapi|msvcrt|ntdll|ole32|oleaut32|opengl32|propsys|rpcrt4|setupapi|shell32|shlwapi|uiautomationcore|user32|userenv|uxtheme|version|winmm|ws2_32|api-ms-win-.*)\.dll$'
extra="$(x86_64-w64-mingw32-objdump -p "$EXE" | awk '/DLL Name:/ {print tolower($3)}' | sort -u \
  | grep -Ev "$allowed" || true)"
[ -z "$extra" ] || fail "the .exe needs DLLs that Windows doesn't ship: $extra"

# 5. Setup program.
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
mkdir -p "$OUT"
SETUP="$OUT/tuned-record-setup-$VERSION.exe"
x86_64-w64-mingw32-strip -o "$OUT/tuned-record.exe" "$EXE"
x86_64-w64-mingw32-strip -o "$OUT/loopback_check.exe" \
  "$ROOT/target/$TARGET/release/examples/loopback_check.exe"
makensis -V2 -DVERSION="$VERSION" -DEXE="$OUT/tuned-record.exe" -DOUTFILE="$SETUP" \
  installer/tuned-record.nsi
echo "Built $SETUP (and $OUT/loopback_check.exe)"

# 6. Smoke test: the .exe starts and keeps running under Wine. Wine has no ASIO driver, so the
#    app shows "no ASIO driver found". Wine's Direct3D shader compiler can't build egui's shaders,
#    so wgpu is pointed at Vulkan here (real Windows is unaffected).
if $SMOKE; then
  prefix="$ROOT/target/wine-smoke"
  crash="$prefix/drive_c/users/$USER/AppData/Roaming/tuned-record/crash.log"
  rm -f "$crash"
  status=0
  WINEPREFIX="$prefix" WINEDEBUG=-all WGPU_BACKEND=vulkan \
    timeout 20 wine "$OUT/tuned-record.exe" >"$OUT/smoke.log" 2>&1 || status=$?
  [ ! -f "$crash" ] || fail "smoke test: the app panicked: $(cat "$crash")"
  [ "$status" -eq 124 ] || fail "smoke test: the app exited early (status $status, see $OUT/smoke.log)"
  echo "Smoke test passed: the app ran for 20 s under Wine without crashing"
fi
