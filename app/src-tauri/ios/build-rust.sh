#!/bin/bash
set -euo pipefail
HONGSI_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
export PATH="$HONGSI_ROOT/app/src-tauri/ios/tools:${HOME}/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:${PATH}"
export IPHONEOS_DEPLOYMENT_TARGET=17.0
export CARGO_TARGET_DIR="$HONGSI_ROOT/target"
HONGSI_CONFIGURATION="${CONFIGURATION:-Debug}"
HONGSI_PLATFORM="${PLATFORM_NAME:-iphoneos}"
HONGSI_ARCHS="${ARCHS:-arm64}"
HONGSI_OUTPUT="$HONGSI_ROOT/app/src-tauri/gen/apple/Externals/$HONGSI_PLATFORM/$HONGSI_CONFIGURATION"
mkdir -p "$HONGSI_OUTPUT"
HONGSI_PROFILE=debug
HONGSI_FLAGS=(--locked)
if [[ "$HONGSI_CONFIGURATION" == Release ]]; then HONGSI_PROFILE=release; HONGSI_FLAGS+=(--release); fi
if [[ "${1:?app or widget required}" == app ]]; then
  HONGSI_PACKAGE=hongsi-app
  HONGSI_LIBRARY=hongsi_lib
  HONGSI_FLAGS+=(--features custom-protocol)
else
  HONGSI_PACKAGE=hongsi-ios-widget
  HONGSI_LIBRARY=hongsi_widget
fi
HONGSI_LIBRARIES=()
for HONGSI_ARCH in $HONGSI_ARCHS; do
  case "$HONGSI_PLATFORM:$HONGSI_ARCH" in
    iphoneos:arm64) HONGSI_TARGET=aarch64-apple-ios ;;
    iphonesimulator:arm64) HONGSI_TARGET=aarch64-apple-ios-sim ;;
    iphonesimulator:x86_64) HONGSI_TARGET=x86_64-apple-ios ;;
    *) echo "Unsupported iOS architecture: $HONGSI_PLATFORM/$HONGSI_ARCH" >&2; exit 1 ;;
  esac
  env -u SDKROOT -u CFLAGS -u CXXFLAGS -u LDFLAGS -u CC -u CXX \
    cargo build --manifest-path "$HONGSI_ROOT/Cargo.toml" --lib \
    -p "$HONGSI_PACKAGE" --target "$HONGSI_TARGET" "${HONGSI_FLAGS[@]}"
  HONGSI_LIBRARIES+=("$CARGO_TARGET_DIR/$HONGSI_TARGET/$HONGSI_PROFILE/lib$HONGSI_LIBRARY.a")
done
if [[ ${#HONGSI_LIBRARIES[@]} -eq 1 ]]; then
  cp "${HONGSI_LIBRARIES[0]}" "$HONGSI_OUTPUT/lib$HONGSI_LIBRARY.a"
else
  xcrun lipo -create "${HONGSI_LIBRARIES[@]}" -output "$HONGSI_OUTPUT/lib$HONGSI_LIBRARY.a"
fi
