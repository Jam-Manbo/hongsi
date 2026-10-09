#!/bin/bash
set -euo pipefail
HONGSI_APP="$(cd "$(dirname "$0")/.." && pwd)"
HONGSI_TEST_DIR="$(mktemp -d "${TMPDIR:-/tmp}/hongsi-ios-attendance.XXXXXX")"
trap 'rm -rf "$HONGSI_TEST_DIR"' EXIT
xcrun swiftc -swift-version 5 -parse-as-library -module-cache-path "$HONGSI_TEST_DIR/modules" \
  "$HONGSI_APP/src-tauri/plugins/ios/ios/Sources/Shared/WidgetData.swift" \
  "$HONGSI_APP/src-tauri/plugins/ios/ios/Sources/Shared/WidgetModel.swift" \
  "$HONGSI_APP/src-tauri/ios/Tests/AttendanceWidgetTests.swift" \
  -o "$HONGSI_TEST_DIR/attendance-tests"
"$HONGSI_TEST_DIR/attendance-tests"
