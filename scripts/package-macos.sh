#!/usr/bin/env bash
set -euo pipefail

APP_NAME="Ya Player"
BINARY_NAME="ya-player"
BUNDLE_ID="app.ya-music-rc.ya-player"
VERSION="${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)}"
DMG_NAME="${DMG_NAME:-Ya-Player-macos-${VERSION}.dmg}"
CODESIGN_IDENTITY="${CODESIGN_IDENTITY:--}"

if [[ -z "${VERSION}" ]]; then
  echo "Cannot read package version from Cargo.toml" >&2
  exit 1
fi

APP_DIR="target/release/bundle/${APP_NAME}.app"
CONTENTS="${APP_DIR}/Contents"
MACOS="${CONTENTS}/MacOS"
RESOURCES="${CONTENTS}/Resources"
DMG_ROOT="target/release/dmg-root"
DMG_PATH="target/release/${DMG_NAME}"

if [[ ! -x "target/release/${BINARY_NAME}" ]]; then
  echo "Missing target/release/${BINARY_NAME}. Run cargo build --release first." >&2
  exit 1
fi

if [[ ! -f "assets/YaPlayer.icns" ]]; then
  echo "Missing assets/YaPlayer.icns. Regenerate the app icon before packaging." >&2
  exit 1
fi

rm -rf "${APP_DIR}" "${DMG_ROOT}" "${DMG_PATH}"
mkdir -p "${MACOS}" "${RESOURCES}" "${DMG_ROOT}"

cp "target/release/${BINARY_NAME}" "${MACOS}/${APP_NAME}"
cp "assets/YaPlayer.icns" "${RESOURCES}/YaPlayer.icns"
chmod +x "${MACOS}/${APP_NAME}"

cat > "${CONTENTS}/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>${APP_NAME}</string>
  <key>CFBundleIdentifier</key>
  <string>${BUNDLE_ID}</string>
  <key>CFBundleName</key>
  <string>${APP_NAME}</string>
  <key>CFBundleDisplayName</key>
  <string>${APP_NAME}</string>
  <key>CFBundleIconFile</key>
  <string>YaPlayer</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>${VERSION}</string>
  <key>CFBundleVersion</key>
  <string>${VERSION}</string>
  <key>LSApplicationCategoryType</key>
  <string>public.app-category.music</string>
  <key>LSMinimumSystemVersion</key>
  <string>12.0</string>
  <key>NSAppleEventsUsageDescription</key>
  <string>Ya Player can read the browser URL after Yandex OAuth login to capture the access token.</string>
</dict>
</plist>
PLIST

codesign_args=(--force --deep --sign "${CODESIGN_IDENTITY}")
if [[ "${CODESIGN_IDENTITY}" != "-" ]]; then
  codesign_args+=(--options runtime --timestamp)
fi

codesign "${codesign_args[@]}" "${APP_DIR}"
codesign --verify --deep --strict --verbose=2 "${APP_DIR}"

cp -R "${APP_DIR}" "${DMG_ROOT}/"
ln -s /Applications "${DMG_ROOT}/Applications"

hdiutil create \
  -volname "${APP_NAME}" \
  -srcfolder "${DMG_ROOT}" \
  -ov \
  -format UDZO \
  "${DMG_PATH}"

echo "${DMG_PATH}"
