#!/bin/bash
set -euo pipefail
HONGSI_APP="$(cd "$(dirname "$0")/.." && pwd)"
HONGSI_MODE="${1:-simulator}"
if [[ $# -gt 0 ]]; then shift; fi
cd "$HONGSI_APP"
HONGSI_BUILD_COMMIT="${HONGSI_BUILD_COMMIT:-${GITHUB_SHA:-}}" pnpm build
xcodegen generate --spec src-tauri/gen/apple/project.yml
HONGSI_ARGS=(-project src-tauri/gen/apple/Hongsi.xcodeproj -scheme Hongsi -derivedDataPath "$HONGSI_APP/../target/ios-xcode")
case "$HONGSI_MODE" in
  simulator) xcodebuild "${HONGSI_ARGS[@]}" -configuration Debug -sdk iphonesimulator -destination 'generic/platform=iOS Simulator' "ARCHS=$(uname -m)" CODE_SIGN_IDENTITY=- "$@" build ;;
  device) xcodebuild "${HONGSI_ARGS[@]}" -configuration Personal -sdk iphoneos -destination 'generic/platform=iOS' "$@" build ;;
  personal) xcodebuild "${HONGSI_ARGS[@]}" -configuration Personal -sdk iphoneos -destination 'generic/platform=iOS' "$@" build ;;
  archive) xcodebuild "${HONGSI_ARGS[@]}" -configuration Release -sdk iphoneos -destination 'generic/platform=iOS' -archivePath "$HONGSI_APP/../target/ios/Hongsi.xcarchive" "$@" archive ;;
  *) echo 'Usage: ios-build.sh simulator|device|personal|archive [xcodebuild settings]' >&2; exit 1 ;;
esac
