#!/usr/bin/env bash
# Publish an Android app release (APK) to the Kalks VPS. Run on YOUR machine (not on the server, not part of deploy.sh):
#
#   deploy/publish-app.sh <path/to/app-release.apk> <version> <build>     e.g.  deploy/publish-app.sh build/app-release.apk 1.4.0 42
#
# Uploads the APK as kalks-<version>.apk to /srv/kalks/downloads on the server, then writes latest.json
# {version, build, file, size, sha256, minAndroid, releasedAt} next to it. Caddy serves both at
# https://app.kalkstrade.com/downloads/*, and the Client Area's /download page reads latest.json (cached 1 minute).
# The APK goes up first and is checked on the server before latest.json changes, so the page never links a missing or
# half-uploaded file. A version that is already published with different contents is refused (versioned APKs are
# cached for a year): bump the version instead.
#
# Environment:
#   KALKS_SSH      ssh target (default kalks-vps: a Host in ~/.ssh/config for the kalks user on the VPS)
#   DOWNLOADS_DIR  directory on the server (default /srv/kalks/downloads)
#   MIN_ANDROID    lowest Android version the build installs on (default 7.0, the app's minSdk 24)
#   DRY_RUN=1      print latest.json and stop (nothing is uploaded)
set -euo pipefail

usage() { sed -n '2,5p' "$0" | sed 's/^# \{0,1\}//' >&2; exit 2; }
[ $# -eq 3 ] || usage
APK=$1 VERSION=$2 BUILD=$3
SSH_TARGET=${KALKS_SSH:-kalks-vps}
DIR=${DOWNLOADS_DIR:-/srv/kalks/downloads}
MIN_ANDROID=${MIN_ANDROID:-7.0}

[ -f "$APK" ] || { echo "no such file: $APK" >&2; exit 1; }
[[ $APK == *.apk ]] || { echo "not an .apk: $APK" >&2; exit 1; }
[[ $VERSION =~ ^[0-9]{1,4}\.[0-9]{1,4}\.[0-9]{1,4}([-+][0-9A-Za-z.-]{1,32})?$ ]] || { echo "version must look like 1.4.0" >&2; exit 1; }
[[ $BUILD =~ ^[1-9][0-9]{0,9}$ ]] || { echo "build must be a positive integer (the versionCode)" >&2; exit 1; }
[[ $MIN_ANDROID =~ ^[0-9]{1,2}(\.[0-9]{1,2}){0,2}$ ]] || { echo "MIN_ANDROID must look like 7.0" >&2; exit 1; }

# when the Android build tools are installed, make sure the APK really is this version
if command -v aapt >/dev/null 2>&1; then
  badging=$(aapt dump badging "$APK" | head -1)
  [[ $badging == *"versionCode='$BUILD'"* && $badging == *"versionName='$VERSION'"* ]] || { echo "the APK says: $badging" >&2; echo "which doesn't match version $VERSION build $BUILD" >&2; exit 1; }
fi

FILE="kalks-$VERSION.apk"
SIZE=$(wc -c < "$APK" | tr -d ' ')
if command -v sha256sum >/dev/null 2>&1; then SHA=$(sha256sum "$APK" | cut -d' ' -f1); else SHA=$(shasum -a 256 "$APK" | cut -d' ' -f1); fi
RELEASED=$(date -u +%Y-%m-%dT%H:%M:%SZ)
MANIFEST=$(mktemp)
trap 'rm -f "$MANIFEST"' EXIT
printf '{\n  "version": "%s",\n  "build": %s,\n  "file": "%s",\n  "size": %s,\n  "sha256": "%s",\n  "minAndroid": "%s",\n  "releasedAt": "%s"\n}\n' \
  "$VERSION" "$BUILD" "$FILE" "$SIZE" "$SHA" "$MIN_ANDROID" "$RELEASED" > "$MANIFEST"

echo "latest.json:"
cat "$MANIFEST"
[ "${DRY_RUN:-}" = 1 ] && { echo "DRY_RUN=1: nothing uploaded"; exit 0; }

# the directory must exist and be writable by the ssh user (one-time setup: deploy/README.md)
ssh "$SSH_TARGET" "test -d '$DIR' -a -w '$DIR'" || { echo "$SSH_TARGET:$DIR is missing or not writable (see deploy/README.md, Android app downloads)" >&2; exit 1; }

# a published version never changes
remote_sha=$(ssh "$SSH_TARGET" "if [ -f '$DIR/$FILE' ]; then sha256sum '$DIR/$FILE' | cut -d' ' -f1; fi")
if [ -n "$remote_sha" ] && [ "$remote_sha" != "$SHA" ]; then
  echo "$FILE is already published with a different SHA-256 ($remote_sha). Bump the version instead." >&2
  exit 1
fi

if [ -z "$remote_sha" ]; then
  echo "uploading $FILE ($SIZE bytes)…"
  rsync --partial -t "$APK" "$SSH_TARGET:$DIR/.$FILE.part"
  got=$(ssh "$SSH_TARGET" "sha256sum '$DIR/.$FILE.part' | cut -d' ' -f1")
  [ "$got" = "$SHA" ] || { echo "upload check failed (server SHA-256 $got)" >&2; exit 1; }
  ssh "$SSH_TARGET" "chmod 644 '$DIR/.$FILE.part' && mv -f '$DIR/.$FILE.part' '$DIR/$FILE'"
else
  echo "$FILE already on the server (same SHA-256): not uploaded again"
fi

# then switch latest.json (atomic rename, so the page never reads half a file)
rsync "$MANIFEST" "$SSH_TARGET:$DIR/.latest.json.part"
ssh "$SSH_TARGET" "chmod 644 '$DIR/.latest.json.part' && mv -f '$DIR/.latest.json.part' '$DIR/latest.json'"

echo "published: https://app.kalkstrade.com/downloads/$FILE"
echo "page:      https://app.kalkstrade.com/download (updates within a minute)"
