#!/usr/bin/env bash
set -euo pipefail

TARGET="${1:?usage: upload-release-assets.sh <target>}"
REF="${2:?usage: upload-release-assets.sh <target> <ref>}"
REPO="${3:?usage: upload-release-assets.sh <target> <ref> <repo>}"

VERSION="${REF#v}"
BUNDLE_DIR="src-tauri/target/${TARGET}/release/bundle"
APP_NAME="mpv-enjoy Home"

cd "${BUNDLE_DIR}"

create_release() {
  gh release create "${REF}" --repo "${REPO}" --draft --title "${VERSION}" || true
}

case "${TARGET}" in
  x86_64-pc-windows-msvc)
    create_release
    for f in nsis/*.exe; do
      [ -f "${f}" ] || continue
      gh release upload "${REF}" "${f}#mpv-enjoy-home_${VERSION}_windows-x64-setup.exe" \
        --repo "${REPO}" --clobber
    done
    for f in msi/*.msi; do
      [ -f "${f}" ] || continue
      gh release upload "${REF}" "${f}#mpv-enjoy-home_${VERSION}_windows-x64.msi" \
        --repo "${REPO}" --clobber
    done
    ;;
  aarch64-apple-darwin)
    create_release
    for f in dmg/*.dmg; do
      [ -f "${f}" ] || continue
      gh release upload "${REF}" "${f}#mpv-enjoy-home_${VERSION}_macos-aarch64.dmg" \
        --repo "${REPO}" --clobber
    done
    tar -czf "mpv-enjoy-home_${VERSION}_macos-aarch64.app.tar.gz" -C macos "${APP_NAME}.app"
    gh release upload "${REF}" "mpv-enjoy-home_${VERSION}_macos-aarch64.app.tar.gz" \
      --repo "${REPO}" --clobber
    ;;
  x86_64-apple-darwin)
    create_release
    for f in dmg/*.dmg; do
      [ -f "${f}" ] || continue
      gh release upload "${REF}" "${f}#mpv-enjoy-home_${VERSION}_macos-x86_64.dmg" \
        --repo "${REPO}" --clobber
    done
    tar -czf "mpv-enjoy-home_${VERSION}_macos-x86_64.app.tar.gz" -C macos "${APP_NAME}.app"
    gh release upload "${REF}" "mpv-enjoy-home_${VERSION}_macos-x86_64.app.tar.gz" \
      --repo "${REPO}" --clobber
    ;;
  *)
    echo "unsupported target: ${TARGET}" >&2
    exit 1
    ;;
esac
