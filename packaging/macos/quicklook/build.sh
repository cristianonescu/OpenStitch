#!/bin/sh
# Builds the Quick Look thumbnail extension (#557) into an app bundle's Contents/PlugIns.
#   packaging/macos/quicklook/build.sh <OpenStitch.app> <openstitch-thumbnailer> <version>
# The extension is a small Swift shim (outside the Cargo workspace) that runs the bundled Rust
# renderer, openstitch-thumbnailer, on the file Finder asks about. Universal (arm64 + x86_64),
# ad-hoc signed like the app (docs/DISTRIBUTION.md "Thumbnails"). Also builds
# openstitch-quicklook-check, the same Swift code path as a command, into $CHECK_OUT if set.
set -eu

app=$1
helper=$2
version=$3
here=$(cd "$(dirname "$0")" && pwd)
appex="$app/Contents/PlugIns/OpenStitchThumbnail.appex"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

rm -rf "$appex"
mkdir -p "$appex/Contents/MacOS"
sed "s/@VERSION@/$version/g" "$here/Info.plist" > "$appex/Contents/Info.plist"

# Every type the app declares (exported .osd, imported machine formats) gets thumbnails.
plist="$appex/Contents/Info.plist"
types="$work/types"
for key in UTExportedTypeDeclarations UTImportedTypeDeclarations; do
    i=0
    while id=$(/usr/libexec/PlistBuddy -c "Print :$key:$i:UTTypeIdentifier" "$app/Contents/Info.plist" 2>/dev/null); do
        echo "$id" >> "$types"
        i=$((i + 1))
    done
done
test -s "$types" || { echo "no types declared in $app/Contents/Info.plist" >&2; exit 1; }
n=0
while read -r id; do
    /usr/libexec/PlistBuddy -c "Add :NSExtension:NSExtensionAttributes:QLSupportedContentTypes:$n string $id" "$plist"
    n=$((n + 1))
done < "$types"
plutil -lint "$plist"

swiftc_for() {
    # $1: arch, $2: output, rest: swiftc arguments
    arch=$1; out=$2; shift 2
    xcrun swiftc -swift-version 5 -O -target "$arch-apple-macos11.0" -o "$out" "$@"
}
for arch in arm64 x86_64; do
    # An app extension's entry point is NSExtensionMain, not main.
    swiftc_for "$arch" "$work/ext-$arch" -module-name OpenStitchThumbnail \
        -application-extension -parse-as-library \
        -Xlinker -e -Xlinker _NSExtensionMain \
        -framework QuickLookThumbnailing \
        "$here/Renderer.swift" "$here/ThumbnailProvider.swift"
    if [ -n "${CHECK_OUT:-}" ]; then
        swiftc_for "$arch" "$work/check-$arch" -module-name OpenStitchQuickLookCheck -parse-as-library \
            "$here/Renderer.swift" "$here/Check.swift"
    fi
done
lipo -create -output "$appex/Contents/MacOS/OpenStitchThumbnail" "$work/ext-arm64" "$work/ext-x86_64"
if [ -n "${CHECK_OUT:-}" ]; then
    lipo -create -output "$CHECK_OUT" "$work/check-arm64" "$work/check-x86_64"
fi

# The Rust renderer, beside the extension's executable (Bundle.url(forAuxiliaryExecutable:)).
cp "$helper" "$appex/Contents/MacOS/openstitch-thumbnailer"

# Inside out: the helper (inherits the extension's sandbox), then the sandboxed extension.
codesign --force --sign - --entitlements "$here/Helper.entitlements" \
    "$appex/Contents/MacOS/openstitch-thumbnailer"
codesign --force --sign - --entitlements "$here/Thumbnail.entitlements" "$appex"
