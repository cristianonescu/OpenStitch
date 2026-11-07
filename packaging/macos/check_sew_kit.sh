#!/bin/sh
# Checks Help ▸ Ready-to-Sew Designs inside a built OpenStitch.app without clicking (#319).
#   packaging/macos/check_sew_kit.sh <path/to/OpenStitch.app>
# `openstitch --sew-kit` runs the menu item's code (sew_kit::open_kit) from the bundle's own
# executable, so the kit is found from the real Contents/MacOS path. A stand-in `open` first on
# PATH records the folder it is given instead of showing Finder. Then the kit is moved away and
# the editor must fall back to the GitHub page; the kit is put back afterwards.
# What only a real click shows (Finder in front, at that folder) stays a manual check: see
# docs/DISTRIBUTION.md "Ready-to-Sew Designs on macOS".
set -eu

app=$(cd "$1" && pwd -P)
exe="$app/Contents/MacOS/openstitch"
kit="$app/Contents/Resources/sew"
work=$(mktemp -d)
trap 'rm -rf "$work"; if [ -d "$work.sew" ]; then mv "$work.sew" "$kit"; fi' EXIT

test -f "$kit/manifest.json" || { echo "no packaged kit at $kit" >&2; exit 1; }

mkdir "$work/bin"
cat > "$work/bin/open" <<SHIM
#!/bin/sh
printf '%s\n' "\$@" > "$work/open-args"
SHIM
chmod +x "$work/bin/open"

# 1. With the kit: Finder (open) is asked for exactly the bundle's Resources/sew.
out=$(PATH="$work/bin:$PATH" "$exe" --sew-kit)
echo "openstitch --sew-kit: $out"
test "$out" = "folder $kit" || { echo "expected: folder $kit" >&2; exit 1; }
# The opener is spawned, not waited for: give it a moment to write.
for _ in 1 2 3 4 5 6 7 8 9 10; do
    [ -s "$work/open-args" ] && break
    sleep 0.2
done
test "$(cat "$work/open-args")" = "$kit" || {
    echo "open was given: $(cat "$work/open-args" 2>/dev/null || echo nothing)" >&2
    exit 1
}
echo "open was given the bundled kit: $kit"

# 2. Without the kit: no opener, the GitHub page.
rm -f "$work/open-args"
mv "$kit" "$work.sew"
out=$(PATH="$work/bin:$PATH" "$exe" --sew-kit)
echo "openstitch --sew-kit (kit moved away): $out"
mv "$work.sew" "$kit"
case $out in
    "online https://github.com/cristianonescu/OpenStitch/tree/main/examples/sew") ;;
    *) echo "expected the GitHub page" >&2; exit 1 ;;
esac
test ! -e "$work/open-args" || { echo "open must not run without a kit" >&2; exit 1; }
echo "Ready-to-Sew Designs: bundled kit and GitHub fallback OK"
