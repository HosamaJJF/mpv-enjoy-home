#!/usr/bin/env bash
set -euo pipefail

TARGET="${1:?usage: upload-release-assets.sh <target>}"
REF="${2:?usage: upload-release-assets.sh <target> <ref>}"
REPO="${3:?usage: upload-release-assets.sh <target> <ref> <repo>}"

VERSION="${REF#v}"
PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUNDLE_DIR="${PROJECT_DIR}/src-tauri/target/${TARGET}/release/bundle"
PREFIX="mpv-enjoy-home-${VERSION}"

cd "${BUNDLE_DIR}"

if ! gh release view "${REF}" --repo "${REPO}" >/dev/null 2>&1; then
  echo "release ${REF} not found: create the draft release before pushing the tag" >&2
  exit 1
fi

upload() {
  gh release upload "${REF}" "$1" --repo "${REPO}" --clobber
}

move_single_bundle() {
  local destination="$1"
  local description="$2"
  shift 2

  if [[ "$#" -ne 1 ]]; then
    echo "expected exactly one ${description}, found $#" >&2
    exit 1
  fi

  mv "$1" "${destination}"
}

shopt -s nullglob

case "${TARGET}" in
  x86_64-pc-windows-msvc)
    WINDOWS_PREFIX="${PREFIX}-windows-x64"
    PORTABLE_DIR="${WINDOWS_PREFIX}"

    move_single_bundle "${WINDOWS_PREFIX}-setup.exe" "NSIS executable" nsis/*.exe
    move_single_bundle "${WINDOWS_PREFIX}.msi" "MSI package" msi/*.msi

    if [[ ! -f ../mpv-enjoy-home.exe ]]; then
      echo "release executable not found: ${BUNDLE_DIR}/../mpv-enjoy-home.exe" >&2
      exit 1
    fi

    mkdir "${PORTABLE_DIR}"
    cp ../mpv-enjoy-home.exe "${PORTABLE_DIR}/mpv-enjoy-home.exe"
    cp "${PROJECT_DIR}/LICENSE" "${PORTABLE_DIR}/LICENSE"
    PORTABLE_SOURCE="${PORTABLE_DIR}" \
      PORTABLE_DESTINATION="${WINDOWS_PREFIX}.zip" \
      powershell.exe -NoLogo -NoProfile -NonInteractive -Command \
      'Compress-Archive -LiteralPath $env:PORTABLE_SOURCE -DestinationPath $env:PORTABLE_DESTINATION -CompressionLevel Optimal -Force'

    upload "${WINDOWS_PREFIX}-setup.exe"
    upload "${WINDOWS_PREFIX}.msi"
    upload "${WINDOWS_PREFIX}.zip"
    ;;
  aarch64-apple-darwin)
    move_single_bundle "${PREFIX}-macos-arm64.dmg" "Apple Silicon DMG" dmg/*.dmg
    upload "${PREFIX}-macos-arm64.dmg"
    ;;
  x86_64-apple-darwin)
    move_single_bundle "${PREFIX}-macos-x64.dmg" "Intel DMG" dmg/*.dmg
    upload "${PREFIX}-macos-x64.dmg"
    ;;
  *)
    echo "unsupported target: ${TARGET}" >&2
    exit 1
    ;;
esac
