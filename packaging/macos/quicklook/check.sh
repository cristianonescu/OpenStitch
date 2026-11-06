#!/bin/sh
# Checks the Quick Look thumbnail extension in a built OpenStitch.app (#557).
#   packaging/macos/quicklook/check.sh <OpenStitch.app> <openstitch-quicklook-check> <openstitch-thumbnailer> <design.pes>
# The third argument is the helper as built, before bundle signing: the bundled copy carries the
# sandbox-inherit entitlement, so macOS stops it when its parent is not a sandboxed extension.
# 1. The extension's bundle: its Info.plist, the types it covers, its signatures.
# 2. Its Swift code path to the Rust renderer (Renderer.swift, built as a command): a design
#    gives a square PNG, a malformed file a clean error, no hang.
# 3. With QUICKLOOK_REGISTER=1 (CI only: it registers the extension with the system):
#    pluginkit must list it (a failure), then `qlmanage -t` is asked for the design's
#    thumbnail through it, for at most 90 seconds. On a runner without a login session
#    qlmanage can wait forever (PR #671's first run hung 78 minutes after registering), so a
#    missing thumbnail there is a warning, not a failure; the owner's Finder check covers it.
#    Never set this on someone's own Mac (docs/DISTRIBUTION.md).
set -eu

app=$(cd "$1" && pwd -P)
check=$2
built_helper=$3
design=$4
appex="$app/Contents/PlugIns/OpenStitchThumbnail.appex"
plist="$appex/Contents/Info.plist"
helper="$appex/Contents/MacOS/openstitch-thumbnailer"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# 1. Bundle.
plutil -lint "$plist"
test "$(plutil -extract NSExtension.NSExtensionPointIdentifier raw -o - "$plist")" = com.apple.quicklook.thumbnail
types=$(plutil -extract NSExtension.NSExtensionAttributes.QLSupportedContentTypes xml1 -o - "$plist")
for id in design pes dst jef; do
    echo "$types" | grep -q "io.github.cristianonescu.openstitch.$id" || {
        echo "the extension does not cover io.github.cristianonescu.openstitch.$id" >&2; exit 1; }
done
lipo -info "$appex/Contents/MacOS/OpenStitchThumbnail" | grep -q arm64
lipo -info "$appex/Contents/MacOS/OpenStitchThumbnail" | grep -q x86_64
codesign --verify --strict "$helper"
codesign --verify --deep --strict "$appex"
codesign -d --entitlements - "$appex" 2>/dev/null | grep -q com.apple.security.app-sandbox
codesign -d --entitlements - "$helper" 2>/dev/null | grep -q com.apple.security.inherit
echo "extension bundle OK"

# 2. The Swift path to the Rust renderer (the bundled helper's code: same binary, unsigned).
"$check" "$built_helper" "$design" "$work/thumb.png" 256
test "$(sips -g pixelWidth -g pixelHeight "$work/thumb.png" | awk '/pixel/ {print $2}' | tr '\n' ' ')" = "256 256 "
printf '#PES0001 not a design' > "$work/junk.pes"
if "$check" "$built_helper" "$work/junk.pes" "$work/junk.png" 2> "$work/err"; then
    echo "a malformed file must fail" >&2; exit 1
fi
grep -q "openstitch-thumbnailer exited" "$work/err"
test ! -e "$work/junk.png"
echo "Swift -> Rust renderer OK"

# 3. Through Quick Look itself (CI runners only).
if [ "${QUICKLOOK_REGISTER:-0}" = 1 ]; then
    cp "$design" "$work/design.pes"
    /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$app"
    pluginkit -a "$appex"
    pluginkit -m -v -i io.github.cristianonescu.openstitch.thumbnail | tee "$work/plugins"
    grep -q io.github.cristianonescu.openstitch.thumbnail "$work/plugins" || {
        echo "pluginkit does not list the extension" >&2; exit 1; }
    mkdir "$work/ql"
    qlmanage -t -s 256 -o "$work/ql" "$work/design.pes" < /dev/null > "$work/ql.log" 2>&1 &
    ql=$!
    waited=0
    while kill -0 "$ql" 2>/dev/null && [ ! -s "$work/ql/design.pes.png" ] && [ "$waited" -lt 90 ]; do
        sleep 1
        waited=$((waited + 1))
    done
    kill "$ql" 2>/dev/null || true
    wait "$ql" 2>/dev/null || true
    cat "$work/ql.log"
    if [ -s "$work/ql/design.pes.png" ]; then
        sips -g pixelWidth -g pixelHeight "$work/ql/design.pes.png"
        echo "Quick Look drew the thumbnail through the extension"
    else
        echo "::warning::qlmanage drew no thumbnail within ${waited}s on this runner (no login session); the extension is registered, the Finder check is the owner's (docs/DISTRIBUTION.md)"
    fi
fi
