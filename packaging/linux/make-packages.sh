#!/usr/bin/env bash
#
# Package the Linux build as an AppImage and a .deb.
#
# Two formats because they answer different questions:
#
#   * **AppImage** — one file, no installation, no root, runs on any modern
#     distribution. The right thing to hand someone who just wants to try it.
#     The only friction is that a downloaded file is not executable, so the
#     user ticks "Allow executing file as program" in their file manager's
#     properties dialog (or runs chmod +x).
#   * **.deb** — for Debian and Ubuntu users who would rather it landed in
#     their application menu like everything else.
#
# Neither needs signing: Linux has no Gatekeeper and no SmartScreen, so a
# download simply runs. This is by far the easiest platform to ship to.

set -euo pipefail

VERSION="${1:-$(grep '^version:' app/pubspec.yaml | sed 's/version: *//' | cut -d+ -f1)}"
APP_NAME="CrawlStat"
BUNDLE="app/build/linux/x64/release/bundle"
OUT_DIR="dist"

echo "==> Building ${APP_NAME} ${VERSION} (release)"
( cd app && flutter build linux --release )

[ -d "$BUNDLE" ] || { echo "error: ${BUNDLE} was not produced"; exit 1; }
mkdir -p "$OUT_DIR"

# ─── shared desktop metadata ──────────────────────────────────────────────

write_desktop_entry() {
  cat > "$1" <<DESKTOP
[Desktop Entry]
Type=Application
Name=${APP_NAME}
Comment=A desktop SEO crawler that runs on your machine
Exec=crawlstat %U
Icon=crawlstat
Terminal=false
Categories=Development;Network;
Keywords=SEO;crawler;audit;website;
DESKTOP
}

# Built by packaging/icons/make-icons.py, with the macOS and Windows icons.
ICON_SRC="packaging/icons/crawlstat.png"

# ─── AppImage ─────────────────────────────────────────────────────────────

echo "==> AppImage"
APPDIR="$(mktemp -d)/${APP_NAME}.AppDir"
mkdir -p "${APPDIR}/usr/bin" "${APPDIR}/usr/share/icons/hicolor/512x512/apps"
cp -R "${BUNDLE}/." "${APPDIR}/usr/bin/"
cp "$ICON_SRC" "${APPDIR}/usr/share/icons/hicolor/512x512/apps/crawlstat.png"
cp "$ICON_SRC" "${APPDIR}/crawlstat.png"
write_desktop_entry "${APPDIR}/crawlstat.desktop"

# AppRun must resolve the bundled libraries relative to itself, or the app
# finds the host's Flutter libraries (or none) and dies on start.
cat > "${APPDIR}/AppRun" <<'APPRUN'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
export LD_LIBRARY_PATH="${HERE}/usr/bin/lib:${LD_LIBRARY_PATH}"
exec "${HERE}/usr/bin/crawlstat" "$@"
APPRUN
chmod +x "${APPDIR}/AppRun"

if ! command -v appimagetool >/dev/null 2>&1; then
  echo "    fetching appimagetool"
  curl -fsSL -o /tmp/appimagetool \
    https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage
  chmod +x /tmp/appimagetool
  APPIMAGETOOL=/tmp/appimagetool
else
  APPIMAGETOOL=appimagetool
fi

# ARCH is read by appimagetool and is not inferred.
ARCH=x86_64 "$APPIMAGETOOL" --no-appstream "$APPDIR" \
  "${OUT_DIR}/${APP_NAME}-${VERSION}-x86_64.AppImage"

# ─── .deb ─────────────────────────────────────────────────────────────────

echo "==> .deb"
DEBROOT="$(mktemp -d)/deb"
mkdir -p "${DEBROOT}/DEBIAN" \
         "${DEBROOT}/usr/lib/crawlstat" \
         "${DEBROOT}/usr/bin" \
         "${DEBROOT}/usr/share/applications" \
         "${DEBROOT}/usr/share/icons/hicolor/512x512/apps"

cp -R "${BUNDLE}/." "${DEBROOT}/usr/lib/crawlstat/"
cp "$ICON_SRC" "${DEBROOT}/usr/share/icons/hicolor/512x512/apps/crawlstat.png"
write_desktop_entry "${DEBROOT}/usr/share/applications/crawlstat.desktop"
ln -s /usr/lib/crawlstat/crawlstat "${DEBROOT}/usr/bin/crawlstat"

# GTK and the standard C/C++ runtime are the only things not bundled. Keeping
# the list short and honest matters: an over-declared dependency makes the
# package refuse to install on a system where it would have worked.
cat > "${DEBROOT}/DEBIAN/control" <<CONTROL
Package: crawlstat
Version: ${VERSION}
Section: web
Priority: optional
Architecture: amd64
Depends: libgtk-3-0, libstdc++6, libc6
Maintainer: CrawlStat <hello@crawlstat.com>
Homepage: https://crawlstat.com
Description: A desktop SEO crawler that runs on your machine
 CrawlStat crawls a site, runs 45 SEO and accessibility audits, and ranks
 what to fix first by how much of the site each problem actually touches.
 Crawl data never leaves the machine.
CONTROL

dpkg-deb --build --root-owner-group "$DEBROOT" \
  "${OUT_DIR}/${APP_NAME}-${VERSION}-amd64.deb" >/dev/null

# ─── checksums ────────────────────────────────────────────────────────────

cd "$OUT_DIR"
for file in "${APP_NAME}-${VERSION}-x86_64.AppImage" "${APP_NAME}-${VERSION}-amd64.deb"; do
  sha256sum "$file" | tee "${file}.sha256"
done

echo
echo "==> Done"
ls -lh "${APP_NAME}-${VERSION}"*
