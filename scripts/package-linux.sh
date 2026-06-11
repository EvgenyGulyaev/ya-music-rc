#!/usr/bin/env bash
set -euo pipefail

BINARY_NAME="ya-player"
VERSION="${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)}"
ARCH="${ARCH:-$(uname -m)}"
PACKAGE_NAME="ya-player-linux-${ARCH}-${VERSION}"
PACKAGE_DIR="target/release/package/${PACKAGE_NAME}"
ARCHIVE_PATH="target/release/Ya-Player-linux-${ARCH}-${VERSION}.tar.gz"

if [[ -z "${VERSION}" ]]; then
  echo "Cannot read package version from Cargo.toml" >&2
  exit 1
fi

if [[ ! -x "target/release/${BINARY_NAME}" ]]; then
  echo "Missing target/release/${BINARY_NAME}. Run cargo build --release first." >&2
  exit 1
fi

rm -rf "${PACKAGE_DIR}" "${ARCHIVE_PATH}"
mkdir -p \
  "${PACKAGE_DIR}/bin" \
  "${PACKAGE_DIR}/share/applications" \
  "${PACKAGE_DIR}/share/icons/hicolor/512x512/apps"

cp "target/release/${BINARY_NAME}" "${PACKAGE_DIR}/bin/ya-player"
cp "assets/YaPlayer.png" "${PACKAGE_DIR}/share/icons/hicolor/512x512/apps/ya-player.png"
chmod +x "${PACKAGE_DIR}/bin/ya-player"

cat > "${PACKAGE_DIR}/share/applications/ya-player.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Ya Player
Comment=Lightweight Yandex Music player
Exec=ya-player
Icon=ya-player
Categories=Audio;Music;Player;
Terminal=false
DESKTOP

cat > "${PACKAGE_DIR}/README.txt" <<README
Ya Player ${VERSION} for Linux

Run:
  ./bin/ya-player

If your desktop environment does not see the app icon automatically, install the
desktop file and icon from share/applications and share/icons.
README

tar -czf "${ARCHIVE_PATH}" -C "$(dirname "${PACKAGE_DIR}")" "$(basename "${PACKAGE_DIR}")"
echo "${ARCHIVE_PATH}"
