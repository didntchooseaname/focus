#!/usr/bin/env bash
set -euo pipefail
system=${1:?usage: tools/package.sh linux|windows-x64|windows-arm64}
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
cargo=${CARGO:-cargo}
version=$(sed -n '/^\[workspace\.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)
dist=$root/dist
stage=$root/target/package
mkdir -p "$dist" "$stage"

fetch() {
    [ -f "$3" ] || curl --fail --silent --show-error --location --output "$3" "$1"
    echo "$2  $3" | sha256sum --check --quiet
}

documents() { install -m 0644 LICENSE NOTICE README.md "$1"; }

case "$system" in
linux)
    $cargo build --release --locked ${JOBS:+-j "$JOBS"} -p focus -p focus-mcp
    name=focus-$version-linux-x86_64
    rm -rf "${stage:?}/$name" "$stage/AppDir"
    install -D -m 0755 -t "$stage/$name" target/release/focus target/release/focus-mcp
    install -m 0644 focus.desktop assets/icons/focus.png "$stage/$name"
    documents "$stage/$name"
    tar --create --gzip --owner=0 --group=0 --directory "$stage" --file "$dist/$name.tar.gz" "$name"

    app=$stage/AppDir
    install -D -m 0755 -t "$app/usr/bin" target/release/focus target/release/focus-mcp
    install -D -m 0644 focus.desktop "$app/usr/share/applications/focus.desktop"
    install -D -m 0644 assets/icons/focus.png "$app/usr/share/icons/hicolor/256x256/apps/focus.png"
    install -D -m 0644 -t "$app/usr/share/doc/focus" LICENSE NOTICE
    ln -s usr/share/applications/focus.desktop "$app/focus.desktop"
    ln -s usr/share/icons/hicolor/256x256/apps/focus.png "$app/focus.png"
    ln -s focus.png "$app/.DirIcon"
    mkdir -p "$app/usr/lib"
    ldd target/release/focus | awk '$3 ~ /^\// { print $3 }' \
        | grep -Ev '/(ld-linux[^/]*|libc|libm|libdl|librt|libpthread|libgcc_s|libxcb)\.so\.' \
        | xargs -r cp --dereference --target-directory "$app/usr/lib"
    cat > "$app/AppRun" <<'EOF'
#!/bin/sh
here=$(dirname "$(readlink -f "$0")")
export LD_LIBRARY_PATH="$here/usr/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
exec "$here/usr/bin/focus" "$@"
EOF
    chmod 0755 "$app/AppRun"
    desktop-file-validate "$app/usr/share/applications/focus.desktop"

    tools=$root/target/package-tools
    mkdir -p "$tools"
    fetch https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage \
        ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0 "$tools/appimagetool-1.9.1"
    fetch https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64 \
        2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d "$tools/runtime-20251108"
    chmod 0755 "$tools/appimagetool-1.9.1"
    ARCH=x86_64 "$tools/appimagetool-1.9.1" --appimage-extract-and-run --no-appstream \
        --runtime-file "$tools/runtime-20251108" "$app" "$dist/Focus-$version-x86_64.AppImage"
    ;;
windows-x64 | windows-arm64)
    case "$system" in
    windows-x64) arch=x86_64 ;;
    windows-arm64) arch=aarch64 ;;
    esac
    CARGO=$cargo JOBS=${JOBS:-3} tools/windows-release.sh "$arch"
    name=focus-$version-$system
    rm -rf "${stage:?}/$name" "$dist/$name.zip"
    install -D -m 0755 -t "$stage/$name" \
        "target/$arch-pc-windows-msvc/release/focus.exe" "target/$arch-pc-windows-msvc/release/focus-mcp.exe"
    documents "$stage/$name"
    (cd "$stage" && python3 -m zipfile --create "$dist/$name.zip" "$name")
    ;;
*)
    echo "unknown system: $system" >&2
    exit 2
    ;;
esac
ls -l "$dist"
