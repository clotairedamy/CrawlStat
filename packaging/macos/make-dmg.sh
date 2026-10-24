#!/usr/bin/env bash
#
# Build CrawlStat.app and wrap it in a drag-to-install .dmg.
#
# Uses only `hdiutil`, which ships with macOS — no Homebrew, no create-dmg, so
# a clean CI runner needs nothing installed.
#
# ## Signing
#
# Two paths, chosen by whether the signing variables are set:
#
#   * **Signed and notarised.** Set MACOS_CERT_NAME (the "Developer ID
#     Application: …" identity in the keychain), plus APPLE_ID, APPLE_TEAM_ID
#     and APPLE_APP_PASSWORD for notarisation. The result double-clicks open on
#     any Mac with no warning at all.
#   * **Unsigned.** Everything still builds and the .dmg still works, but
#     macOS will refuse to open the app on the first try and the user has to go
#     to System Settings › Privacy & Security and press "Open Anyway". That is
#     fine for people you have told what to expect, and not fine for strangers.
#
# The script says which one it did, loudly, at the end.

set -euo pipefail

VERSION="${1:-$(grep '^version:' app/pubspec.yaml | sed 's/version: *//' | cut -d+ -f1)}"
APP_NAME="CrawlStat"
BUILT_APP="app/build/macos/Build/Products/Release/crawlstat.app"
OUT_DIR="dist"
DMG="${OUT_DIR}/${APP_NAME}-${VERSION}-macos.dmg"

echo "==> Building ${APP_NAME} ${VERSION} (release)"
( cd app && flutter build macos --release )

[ -d "$BUILT_APP" ] || { echo "error: ${BUILT_APP} was not produced"; exit 1; }

mkdir -p "$OUT_DIR"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

# The .app is renamed to its product name here rather than in Xcode config so
# the bundle identifier and the build output path stay stable.
cp -R "$BUILT_APP" "${STAGE}/${APP_NAME}.app"

if [ -n "${MACOS_CERT_NAME:-}" ]; then
  echo "==> Signing with: ${MACOS_CERT_NAME}"
  # --options runtime is required for notarisation. The Rust library inside the
  # bundle is signed by the deep pass.
  codesign --force --deep --timestamp --options runtime \
    --sign "${MACOS_CERT_NAME}" "${STAGE}/${APP_NAME}.app"
  codesign --verify --deep --strict --verbose=2 "${STAGE}/${APP_NAME}.app"
  SIGNED=yes
else
  echo "==> No MACOS_CERT_NAME set — building UNSIGNED"
  # An ad-hoc signature is not trust, but an entirely unsigned bundle with a
  # native library in it can simply fail to launch on Apple silicon.
  codesign --force --deep --sign - "${STAGE}/${APP_NAME}.app"
  SIGNED=no
fi

# Drag-to-install: the app on the left, a shortcut to /Applications on the
# right, which is the gesture every Mac user already knows.
ln -s /Applications "${STAGE}/Applications"

echo "==> Creating ${DMG}"
rm -f "$DMG"
hdiutil create \
  -volname "${APP_NAME} ${VERSION}" \
  -srcfolder "$STAGE" \
  -ov -format UDZO \
  "$DMG" >/dev/null

if [ "$SIGNED" = yes ]; then
  codesign --force --timestamp --sign "${MACOS_CERT_NAME}" "$DMG"

  if [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ] && [ -n "${APPLE_APP_PASSWORD:-}" ]; then
    echo "==> Notarising (this takes a few minutes)"
    xcrun notarytool submit "$DMG" \
      --apple-id "$APPLE_ID" \
      --team-id "$APPLE_TEAM_ID" \
      --password "$APPLE_APP_PASSWORD" \
      --wait
    # Stapling puts the notarisation ticket inside the file, so Gatekeeper can
    # verify it without the user being online.
    xcrun stapler staple "$DMG"
    echo "==> Notarised and stapled"
  else
    echo "==> Signed but NOT notarised (no APPLE_ID / APPLE_TEAM_ID / APPLE_APP_PASSWORD)"
    echo "    Gatekeeper will still warn. Notarisation is what removes the warning."
  fi
fi

shasum -a 256 "$DMG" | tee "${DMG}.sha256"
echo
echo "==> Done: ${DMG}"
if [ "$SIGNED" = no ]; then
  cat <<'WARN'

    This build is UNSIGNED. On first launch macOS will say the app
    "cannot be opened because the developer cannot be verified".
    The user must open System Settings › Privacy & Security and press
    "Open Anyway".

    To remove that step entirely you need an Apple Developer Program
    membership ($99/year) and the signing variables above.
WARN
fi
