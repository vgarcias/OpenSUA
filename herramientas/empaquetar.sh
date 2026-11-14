#!/usr/bin/env bash
# Compila y empaqueta OpenSUA para macOS o Linux.
# Uso: herramientas/empaquetar.sh <triple de destino>
#   x86_64-unknown-linux-gnu  -> OpenSUA-Portable-<versión>-linux-x86_64.tar.gz
#   aarch64-apple-darwin      -> OpenSUA-<versión>-macos-arm64.dmg
#   x86_64-apple-darwin       -> OpenSUA-<versión>-macos-x86_64.dmg
#
# Linux: carpeta portable (los datos quedan junto al programa).
# macOS: OpenSUA.app en modo instalado (datos en ~/Library/Application Support/SUA),
#        porque una app dentro de /Applications no debe escribir en su propia carpeta.
# Hace falta pandoc para el manual. Los ejecutables quedan SIN FIRMAR.
set -euo pipefail

destino="${1:?Falta el triple de destino}"
raiz="$(cd "$(dirname "$0")/.." && pwd)"
app="$raiz/app"
fuentes="$app/paquete"
version="$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' "$app/Cargo.toml" | head -1)"
cargo_home="${CARGO_HOME:-$HOME/.cargo}"
rustup_home="${RUSTUP_HOME:-$HOME/.rustup}"

# Sin rutas del equipo de compilación dentro de los ejecutables.
export RUSTFLAGS="--remap-path-prefix=$cargo_home=cargo --remap-path-prefix=$rustup_home=rustup --remap-path-prefix=$app=sua"
(cd "$app" && cargo build --release --locked --target "$destino" --target-dir target/paquete)
salida="$app/target/paquete/$destino/release"

dist="$app/dist/paquete"
rm -rf "$dist"
mkdir -p "$dist"

extras() { # $1 = carpeta donde van licencias y manual
  python3 -I "$raiz/herramientas/licencias.py" "$app" "$1/LICENCIAS-DE-TERCEROS.txt" "$destino"
  (cd "$fuentes" && pandoc Manual.md -o "$1/Manual.html" --standalone --embed-resources --toc --toc-depth=2 \
    --css=manual.css --metadata "date=Versión $version")
  cp "$fuentes/LEEME.txt" "$1/"
}

case "$destino" in
  *-linux-*)
    arq="${destino%%-*}"
    carpeta="$dist/OpenSUA-Portable"
    mkdir -p "$carpeta/config"
    cp "$salida/sua" "$salida/sua-cli" "$carpeta/"
    cp "$app/crates/ui/recursos/app.png" "$carpeta/opensua.png"
    printf 'portable = true\nrespaldos_a_conservar = 10\n' > "$carpeta/config/settings.toml"
    extras "$carpeta"
    entregable="OpenSUA-Portable-$version-linux-$arq.tar.gz"
    tar -C "$dist" -czf "$dist/$entregable" OpenSUA-Portable
    ;;
  *-apple-darwin)
    case "$destino" in aarch64-*) arq=arm64 ;; *) arq=x86_64 ;; esac
    imagen="$dist/imagen"
    bundle="$imagen/OpenSUA.app"
    mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
    cp "$salida/sua" "$salida/sua-cli" "$bundle/Contents/MacOS/"
    # Icono .icns a partir del PNG.
    iconset="$dist/OpenSUA.iconset"
    mkdir -p "$iconset"
    for t in 16 32 64 128 256 512; do
      sips -z "$t" "$t" "$app/crates/ui/recursos/app.png" --out "$iconset/icon_${t}x${t}.png" >/dev/null
      d=$((t * 2)); sips -z "$d" "$d" "$app/crates/ui/recursos/app.png" --out "$iconset/icon_${t}x${t}@2x.png" >/dev/null
    done
    iconutil -c icns "$iconset" -o "$bundle/Contents/Resources/OpenSUA.icns"
    rm -rf "$iconset"
    cat > "$bundle/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>OpenSUA</string>
  <key>CFBundleDisplayName</key><string>OpenSUA</string>
  <key>CFBundleIdentifier</key><string>mx.opensua.app</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleExecutable</key><string>sua</string>
  <key>CFBundleIconFile</key><string>OpenSUA.icns</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleDevelopmentRegion</key><string>es</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
    # Firma ad hoc: sin ella macOS en Apple Silicon no ejecuta el binario.
    codesign --force --deep --sign - "$bundle"
    extras "$imagen"
    ln -s /Applications "$imagen/Aplicaciones"
    entregable="OpenSUA-$version-macos-$arq.dmg"
    hdiutil create -volname "OpenSUA $version" -srcfolder "$imagen" -ov -format UDZO "$dist/$entregable"
    rm -rf "$imagen"
    ;;
  *) echo "Destino no soportado: $destino" >&2; exit 1 ;;
esac

(cd "$dist" && shasum -a 256 "$entregable" > "SHA256SUMS-$destino.txt")
echo "Entregable: $dist/$entregable"
