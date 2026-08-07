#!/usr/bin/env bash
set -euo pipefail

TARGET="${1:?usage: upload-release-assets.sh <target>}"
REF="${2:?usage: upload-release-assets.sh <target> <ref>}"
REPO="${3:?usage: upload-release-assets.sh <target> <ref> <repo>}"

VERSION="${REF#v}"
BUNDLE_DIR="src-tauri/target/${TARGET}/release/bundle"
APP_NAME="mpv-enjoy Home"
PREFIX="mpv-enjoy-home_${VERSION}"

cd "${BUNDLE_DIR}"

create_release() {
  gh release create "${REF}" --repo "${REPO}" --draft --title "${VERSION}" || true
}

upload() {
  gh release upload "${REF}" "$1" --repo "${REPO}" --clobber
}

case "${TARGET}" in
  x86_64-pc-windows-msvc)
    create_release
    for f in nsis/*.exe; do
      [ -f "${f}" ] || continue
      mv "${f}" "${PREFIX}_windows-x64-setup.exe"
    done
    for f in msi/*.msi; do
      [ -f "${f}" ] || continue
      mv "${f}" "${PREFIX}_windows-x64.msi"
    done
    upload "${PREFIX}_windows-x64-setup.exe"
    upload "${PREFIX}_windows-x64.msi"
    ;;
  aarch64-apple-darwin)
    create_release
    for f in dmg/*.dmg; do
      [ -f "${f}" ] || continue
      mv "${f}" "${PREFIX}_macos-aarch64.dmg"
    done
    tar -czf "${PREFIX}_macos-aarch64.app.tar.gz" -C macos "${APP_NAME}.app"
    upload "${PREFIX}_macos-aarch64.dmg"
    upload "${PREFIX}_macos-aarch64.app.tar.gz"
    ;;
  x86_64-apple-darwin)
    create_release
    for f in dmg/*.dmg; do
      [ -f "${f}" ] || continue
      mv "${f}" "${PREFIX}_macos-x86_64.dmg"
    done
    tar -czf "${PREFIX}_macos-x86_64.app.tar.gz" -C macos "${APP_NAME}.app"
    upload "${PREFIX}_macos-x86_64.dmg"
    upload "${PREFIX}_macos-x86_64.app.tar.gz"
    ;;
  *)
    echo "unsupported target: ${TARGET}" >&2
    exit 1
    ;;
esac
