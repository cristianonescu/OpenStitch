#!/bin/sh
# Checks the Quick Look thumbnail extension in a built OpenStitch.app (#557, #1182).
#   packaging/macos/quicklook/check.sh <OpenStitch.app> <openstitch-quicklook-check> <design.pes>
# 1. The extension's bundle: its Info.plist, the types it covers, its architectures, its
#    signature and sandbox. It holds a single executable with the renderer linked in, and that
#    executable starts no process: Quick Look's sandbox for thumbnail extensions refuses to
#    (#1182, where a bundled helper program made every thumbnail fail).
# 2. Its Swift code path to the Rust renderer (Renderer.swift, built as a command): a design
#    gives a square PNG, a malformed file a clean error, no hang.
# 3. With QUICKLOOK_REGISTER=1 (CI only: it registers the extension with the system): pluginkit
#    must list it, then macOS is asked for the design's thumbnail as Finder asks
#    (QLThumbnailGenerator, `openstitch-quicklook-check --system`). No thumbnail within
#    QUICKLOOK_TIMEOUT seconds (default 60), or an error, fails the check.
#    Never set this on someone's own Mac (docs/DISTRIBUTION.md).
set -eu

app=$(cd "$1" && pwd -P)
check=$2
design=$3
appex="$app/Contents/PlugIns/OpenStitchThumbnail.appex"
plist="$appex/Contents/Info.plist"
exe="$appex/Contents/MacOS/OpenStitchThumbnail"
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
lipo -info "$exe" | grep -q arm64
lipo -info "$exe" | grep -q x86_64
test "$(ls "$appex/Contents/MacOS")" = OpenStitchThumbnail || {
    echo "the extension must hold only its own executable: $(ls "$appex/Contents/MacOS")" >&2; exit 1; }
for arch in arm64 x86_64; do
    nm -u -arch "$arch" "$exe" | grep -q _openstitch_ql_thumbnail && {
        echo "the renderer is not linked into the $arch extension" >&2; exit 1; }
    nm -arch "$arch" "$exe" | grep -q ' T _openstitch_ql_thumbnail$' || {
        echo "the $arch extension lacks the renderer" >&2; exit 1; }
    if nm -u -arch "$arch" "$exe" | grep -E '^_(posix_spawnp?|execve|fork|vfork)$'; then
        echo "the $arch extension can start a process, which its sandbox refuses (#1182)" >&2; exit 1
    fi
done
codesign --verify --deep --strict "$appex"
# Its entitlements are exactly the app sandbox, nothing wider.
codesign -d --entitlements - --xml "$appex" 2>/dev/null > "$work/ent.plist"
test "$(plutil -convert json -o - "$work/ent.plist")" = '{"com.apple.security.app-sandbox":true}' || {
    echo "the extension's entitlements changed: $(plutil -convert json -o - "$work/ent.plist")" >&2; exit 1; }
echo "extension bundle OK"

# 2. The Swift path to the Rust renderer.
"$check" "$design" "$work/thumb.png" 256
test "$(sips -g pixelWidth -g pixelHeight "$work/thumb.png" | awk '/pixel/ {print $2}' | tr '\n' ' ')" = "256 256 "
printf '#PES0001 not a design' > "$work/junk.pes"
if "$check" "$work/junk.pes" "$work/junk.png" 2> "$work/err"; then
    echo "a malformed file must fail" >&2; exit 1
fi
grep -q "the OpenStitch renderer failed" "$work/err"
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
    "$check" --system "${QUICKLOOK_TIMEOUT:-60}" "$work/design.pes" "$work/ql.png" 256
    sips -g pixelWidth -g pixelHeight "$work/ql.png"
    echo "Quick Look drew the thumbnail through the extension"
fi
