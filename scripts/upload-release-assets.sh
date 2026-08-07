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

if ! gh release view "${REF}" --repo "${REPO}" >/dev/null 2>&1; then
  echo "release ${REF} not found: create the draft release before pushing the tag" >&2
  exit 1
fi

upload() {
  gh release upload "${REF}" "$1" --repo "${REPO}" --clobber
}

case "${TARGET}" in
  x86_64-pc-windows-msvc)
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
    for f in dmg/*.dmg; do
      [ -f "${f}" ] || continue
      mv "${f}" "${PREFIX}_macos-aarch64.dmg"
    done
    tar -czf "${PREFIX}_macos-aarch64.app.tar.gz" -C macos "${APP_NAME}.app"
    upload "${PREFIX}_macos-aarch64.dmg"
    upload "${PREFIX}_macos-aarch64.app.tar.gz"
    ;;
  x86_64-apple-darwin)
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
