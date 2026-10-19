#!/usr/bin/env bash
# Native macOS SDK packaging; no QEMU, OS images or mobile SDKs are bundled.
set -euo pipefail
repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_dir"
if [ "$(uname -s)" != Darwin ]; then
  echo 'Build the Mac package on macOS with Xcode; Linux cross-checks cannot produce this package.' >&2
  exit 1
fi
if [ "$(uname -m)" != arm64 ]; then
  echo 'Run this native release check on an Apple Silicon Mac.' >&2
  exit 1
fi
xcrun --sdk macosx --show-sdk-path >/dev/null
export MACOSX_DEPLOYMENT_TARGET=14.0
rustup target add aarch64-apple-darwin
cargo build --locked --release --target aarch64-apple-darwin -p freelane -p freelane-desktop --features freelane-desktop/native-video
version="$(cargo metadata --locked --no-deps --format-version 1 | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"]=="freelane"))')"
revision="$(git rev-parse --short=12 HEAD)"
package_name="FreeLane-$version-$revision-macos-arm64"
package_dir="$repo_dir/target/packages/$package_name"
# Refuse an existing output so repeated builds never silently replace a package.
mkdir -p "$repo_dir/target/packages"
mkdir "$package_dir"
app="$package_dir/FreeLane.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/aarch64-apple-darwin/release/freelane "$app/Contents/MacOS/"
cp target/aarch64-apple-darwin/release/freelane-desktop "$app/Contents/MacOS/"
cp LICENSE crates/desktop/assets/DejaVu-LICENSE.txt crates/desktop/assets/Homebrew-install-LICENSE.txt crates/desktop/assets/Ubuntu-keyring-NOTICE.txt "$app/Contents/Resources/"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>org.freelane.desktop</string>
<key>CFBundleName</key><string>FreeLane</string>
<key>CFBundleDisplayName</key><string>FreeLane</string>
<key>CFBundleExecutable</key><string>freelane-desktop</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$version</string>
<key>CFBundleVersion</key><string>$version</string>
<key>LSMinimumSystemVersion</key><string>14.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
plutil -lint "$app/Contents/Info.plist"
# Ad-hoc signing is for experimental builds; this is not Developer ID/notarization.
codesign --force --sign - --entitlements scripts/macos-hypervisor.entitlements.plist "$app/Contents/MacOS/freelane"
codesign --force --sign - "$app/Contents/MacOS/freelane-desktop"
codesign --force --sign - "$app"
codesign --verify --deep --strict "$app"
lipo "$app/Contents/MacOS/freelane" -verify_arch arm64
lipo "$app/Contents/MacOS/freelane-desktop" -verify_arch arm64
# Native Dock export/signature check; viewers share a serialized agent channel.
# Fixture owns no VM or guest data.
fixture_dir="$(mktemp -d "$repo_dir/target/dock-bundle-check.XXXXXX")"
python3 - "$fixture_dir" <<'PY'
from pathlib import Path
import json, sys
root = Path(sys.argv[1])
(root / 'vm.toml').write_text('# Offline export fixture only; no guest is booted.\n')
(root / 'app.toml').write_text('name="Dock export check"\nvm=' + json.dumps(str(root / 'vm.toml')) + '\nexecutable=' + json.dumps(r'C:\Windows\System32\notepad.exe') + '\narguments=[]\n')
PY
"$app/Contents/MacOS/freelane" app-export "$fixture_dir/app.toml" "$fixture_dir/Dock check.app"
plutil -lint "$fixture_dir/Dock check.app/Contents/Info.plist"
codesign --verify --deep --strict "$fixture_dir/Dock check.app"
test -f "$fixture_dir/Dock check.app/Contents/Resources/app.toml"
# Delete only the new offline fixture after verification.
rm -rf "$fixture_dir"
# Exercise actual release CLI parsing; an offline mobile plan needs no emulator.
"$app/Contents/MacOS/freelane" mobile-plan "$repo_dir/configs/mobile.ios.example.toml" > "$package_dir/ios-plan-check.json"
cp README.md README.vi.md LICENSE "$package_dir/"
cp docs/release.md "$package_dir/START-HERE.md"
mkdir -p "$package_dir/scripts"
cp scripts/install-windows-agent.ps1 "$package_dir/scripts/"
cp crates/desktop/assets/DejaVu-LICENSE.txt "$package_dir/"
python3 - "$package_dir" <<'PY'
from pathlib import Path
import shutil, subprocess, sys
destination = Path(sys.argv[1])
for name in subprocess.check_output(['git', 'ls-files', '-z', '--', 'docs', 'configs']).decode().split('\0'):
    if not name:
        continue
    path = Path(name)
    if path.is_symlink():
        raise SystemExit('Refusing a symlink in packaged documentation')
    target = destination / path
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(path, target)
PY
cargo metadata --locked --format-version 1 --filter-platform aarch64-apple-darwin > "$package_dir/cargo-dependencies.json"
python3 - "$package_dir" "$(rustc --print sysroot)" <<'PY'
from pathlib import Path
import json, shutil, sys
package = Path(sys.argv[1])
notices = package / 'notices'
notices.mkdir()
for name in ['COPYRIGHT.html', 'COPYRIGHT-library.html']:
    shutil.copy2(Path(sys.argv[2]) / 'share/doc/rust' / name, notices / ('Rust-' + name))
metadata = json.loads((package / 'cargo-dependencies.json').read_text())
for dependency in metadata['packages']:
    source = Path(dependency['manifest_path']).parent
    for pattern in ['LICENSE*', 'COPYING*', 'NOTICE*']:
        for path in source.glob(pattern):
            if path.is_file() and not path.is_symlink():
                target = notices / (dependency['name'] + '-' + dependency['version']) / path.name
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, target)
PY
printf '%s\n' "$revision" > "$package_dir/REVISION.txt"
archive="$repo_dir/target/packages/$package_name.zip"
test ! -e "$archive"
ditto -c -k --sequesterRsrc --keepParent "$package_dir" "$archive"
(cd "$repo_dir/target/packages" && shasum -a 256 "$package_name.zip" > "$package_name.zip.sha256")
echo "Experimental Apple Silicon package: $archive"
# Finder installation: drag the signed app onto the Applications shortcut.
dmg="$repo_dir/target/packages/$package_name.dmg"
test ! -e "$dmg"
dmg_stage="$(mktemp -d "$repo_dir/target/dmg-stage.XXXXXX")"
dmg_mount="$(mktemp -d "$repo_dir/target/dmg-mount.XXXXXX")"
dmg_attached=false
cleanup_dmg() {
  if [ "$dmg_attached" = true ]; then
    hdiutil detach "$dmg_mount" || return 1
  fi
  rm -rf "$dmg_stage" "$dmg_mount"
}
trap cleanup_dmg EXIT
ditto "$package_dir" "$dmg_stage"
ln -s /Applications "$dmg_stage/Applications"
hdiutil create -volname FreeLane -srcfolder "$dmg_stage" -format UDZO -fs HFS+ "$dmg"
hdiutil verify "$dmg"
hdiutil attach "$dmg" -readonly -nobrowse -mountpoint "$dmg_mount"
dmg_attached=true
codesign --verify --deep --strict "$dmg_mount/FreeLane.app"
plutil -lint "$dmg_mount/FreeLane.app/Contents/Info.plist"
lipo "$dmg_mount/FreeLane.app/Contents/MacOS/freelane-desktop" -verify_arch arm64
test "$(readlink "$dmg_mount/Applications")" = /Applications
test -f "$dmg_mount/START-HERE.md"
hdiutil detach "$dmg_mount"
dmg_attached=false
cleanup_dmg
trap - EXIT
(cd "$repo_dir/target/packages" && shasum -a 256 "$package_name.dmg" > "$package_name.dmg.sha256")
echo "Verified Apple Silicon disk image: $dmg"
