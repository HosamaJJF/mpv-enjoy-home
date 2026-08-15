#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEST_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/mpv-enjoy-home-release-assets.XXXXXX")"
TEST_PROJECT="${TEST_ROOT}/project"
UPLOAD_LOG="${TEST_ROOT}/uploads.txt"

cleanup() {
  rm -rf -- "${TEST_ROOT:?}"
}
trap cleanup EXIT

mkdir -p "${TEST_PROJECT}/scripts"
cp "${PROJECT_DIR}/scripts/upload-release-assets.sh" "${TEST_PROJECT}/scripts/"
cp "${PROJECT_DIR}/scripts/create-portable-zip.ps1" "${TEST_PROJECT}/scripts/"
cp "${PROJECT_DIR}/LICENSE" "${TEST_PROJECT}/LICENSE"

WINDOWS_RELEASE="${TEST_PROJECT}/src-tauri/target/x86_64-pc-windows-msvc/release"
ARM64_BUNDLE="${TEST_PROJECT}/src-tauri/target/aarch64-apple-darwin/release/bundle"
X64_BUNDLE="${TEST_PROJECT}/src-tauri/target/x86_64-apple-darwin/release/bundle"

mkdir -p "${WINDOWS_RELEASE}/bundle/nsis" "${WINDOWS_RELEASE}/bundle/msi"
mkdir -p "${ARM64_BUNDLE}/dmg" "${X64_BUNDLE}/dmg"
touch "${WINDOWS_RELEASE}/mpv-enjoy-home.exe"
touch "${WINDOWS_RELEASE}/bundle/nsis/generated.exe"
touch "${WINDOWS_RELEASE}/bundle/msi/generated.msi"
touch "${ARM64_BUNDLE}/dmg/generated.dmg"
touch "${X64_BUNDLE}/dmg/generated.dmg"

export UPLOAD_LOG
gh() {
  case "$1 $2" in
    'release view')
      return 0
      ;;
    'release upload')
      if [[ ! -f "$4" ]]; then
        echo "upload target does not exist: $4" >&2
        return 1
      fi
      printf '%s\n' "$4" >>"${UPLOAD_LOG}"
      ;;
    *)
      echo "unexpected gh invocation: $*" >&2
      return 1
      ;;
  esac
}
export -f gh

if ! command -v powershell.exe >/dev/null 2>&1; then
  powershell.exe() {
    zip -q -r "${PORTABLE_DESTINATION}" "${PORTABLE_SOURCE}"
  }
  export -f powershell.exe
fi

bash "${TEST_PROJECT}/scripts/upload-release-assets.sh" \
  x86_64-pc-windows-msvc v1.2.3 owner/repo
bash "${TEST_PROJECT}/scripts/upload-release-assets.sh" \
  aarch64-apple-darwin v1.2.3 owner/repo
bash "${TEST_PROJECT}/scripts/upload-release-assets.sh" \
  x86_64-apple-darwin v1.2.3 owner/repo

WINDOWS_BUNDLE="${WINDOWS_RELEASE}/bundle"
WINDOWS_PREFIX="mpv-enjoy-home-1.2.3-windows-x64"

for expected in \
  "${WINDOWS_BUNDLE}/${WINDOWS_PREFIX}-setup.exe" \
  "${WINDOWS_BUNDLE}/${WINDOWS_PREFIX}.msi" \
  "${WINDOWS_BUNDLE}/${WINDOWS_PREFIX}.zip" \
  "${WINDOWS_BUNDLE}/${WINDOWS_PREFIX}/mpv-enjoy-home.exe" \
  "${WINDOWS_BUNDLE}/${WINDOWS_PREFIX}/LICENSE" \
  "${WINDOWS_BUNDLE}/${WINDOWS_PREFIX}/.mpv-enjoy-home-portable" \
  "${ARM64_BUNDLE}/mpv-enjoy-home-1.2.3-macos-arm64.dmg" \
  "${X64_BUNDLE}/mpv-enjoy-home-1.2.3-macos-x64.dmg"
do
  if [[ ! -f "${expected}" ]]; then
    echo "expected release asset does not exist: ${expected}" >&2
    exit 1
  fi
done

if command -v unzip >/dev/null 2>&1; then
  if ! unzip -Z1 "${WINDOWS_BUNDLE}/${WINDOWS_PREFIX}.zip" \
    | grep -Fxq "${WINDOWS_PREFIX}/.mpv-enjoy-home-portable"; then
    echo 'portable marker is missing from Windows ZIP' >&2
    exit 1
  fi
elif command -v tar >/dev/null 2>&1; then
  if ! tar -tf "${WINDOWS_BUNDLE}/${WINDOWS_PREFIX}.zip" \
    | grep -Fxq "${WINDOWS_PREFIX}/.mpv-enjoy-home-portable"; then
    echo 'portable marker is missing from Windows ZIP' >&2
    exit 1
  fi
else
  echo 'unzip or tar is required to inspect the Windows ZIP fixture' >&2
  exit 1
fi

if find "${TEST_PROJECT}/src-tauri/target" -name '*.app.tar.gz' -print -quit | grep -q .; then
  echo 'macOS app archive should not be generated' >&2
  exit 1
fi

printf '%s\n' \
  "${WINDOWS_PREFIX}-setup.exe" \
  "${WINDOWS_PREFIX}.msi" \
  "${WINDOWS_PREFIX}.zip" \
  'mpv-enjoy-home-1.2.3-macos-arm64.dmg' \
  'mpv-enjoy-home-1.2.3-macos-x64.dmg' \
  >"${TEST_ROOT}/expected-uploads.txt"
diff -u "${TEST_ROOT}/expected-uploads.txt" "${UPLOAD_LOG}"

echo 'release asset packaging fixture passed'
