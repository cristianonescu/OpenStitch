#!/bin/sh
# Builds OpenStitch.app from a release binary and zips it.
#   packaging/macos/bundle.sh <path/to/openstitch> <version> <out-dir> [<path/to/openstitch-thumbnailer>]
# With the thumbnailer, the Quick Look thumbnail extension is built into Contents/PlugIns
# (packaging/macos/quicklook, #557; needs Xcode's swiftc).
# The bundle is ad-hoc signed only (no Developer ID): Gatekeeper asks before the first launch.
set -eu

bin=$1
version=$2
out=$3
thumbnailer=${4:-}
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)

app="$out/OpenStitch.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin" "$app/Contents/MacOS/openstitch"
cp "$root/assets/icons/openstitch.icns" "$app/Contents/Resources/openstitch.icns"
sed "s/@VERSION@/$version/g" "$here/Info.plist" > "$app/Contents/Info.plist"
printf 'APPL????' > "$app/Contents/PkgInfo"
cp "$root/THIRD_PARTY_NOTICES.md" "$root/LICENSE-MIT" "$root/LICENSE-APACHE" "$app/Contents/Resources/"
# The ready-to-sew kit Help ▸ Ready-to-Sew Designs opens (examples/sew).
cp -R "$root/examples/sew" "$app/Contents/Resources/sew"
plutil -lint "$app/Contents/Info.plist"
if [ -n "$thumbnailer" ]; then
    "$here/quicklook/build.sh" "$app" "$thumbnailer" "$version"
fi
codesign --force --sign - "$app"
(cd "$out" && ditto -c -k --keepParent OpenStitch.app "OpenStitch-$version-macos-$(uname -m).zip")
echo "$out/OpenStitch-$version-macos-$(uname -m).zip"
