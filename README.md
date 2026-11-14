# OpenSUA

Alternativa abierta al Sistema Único de Autodeterminación (SUA) del IMSS: captura de patrones, trabajadores y movimientos, cálculo de cuotas y lectura/generación de archivos de pago `.SUA`.

## Descargas

En [Releases](../../releases) hay paquetes para:

| Sistema | Archivo |
|---|---|
| Windows 10/11 x64 | `OpenSUA-<versión>.msi` (instalador) o `OpenSUA-Portable-<versión>.zip` |
| macOS Apple Silicon | `OpenSUA-<versión>-macos-arm64.dmg` |
| macOS Intel | `OpenSUA-<versión>-macos-x86_64.dmg` |
| Linux x64 | `OpenSUA-Portable-<versión>-linux-x86_64.tar.gz` |

Los ejecutables no están firmados. En macOS, la primera vez abre la app con clic derecho → *Abrir*, o ejecuta `xattr -dr com.apple.quarantine /Applications/OpenSUA.app`.

## Estructura

- `app/` — aplicación (Rust + Slint): `nucleo` (datos y cálculo), `sua` (línea de comandos), `ui` (interfaz). Ver `app/README.md`.
- `herramientas/` — empaquetado (`empaquetar.ps1` para Windows, `empaquetar.sh` para macOS/Linux) y utilidades.
- `sua_test_data/` — archivos `.SUA` sintéticos y resultados esperados.
- `app/crates/nucleo/semilla/` — catálogos CSV que el programa incluye (UMA, salarios mínimos, INPC, primas, municipios…).
- `prototipo/`, `prototipo-slint/` — prototipos de interfaz.

## Publicar una versión

```
git tag v0.2.0
git push origin v0.2.0
```

GitHub Actions (`.github/workflows/publicar.yml`) compila en Windows, macOS y Linux y crea la versión con los paquetes y `SHA256SUMS.txt`.
