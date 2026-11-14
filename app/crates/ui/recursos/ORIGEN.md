# Origen de las imágenes

Ninguna imagen de esta carpeta es oficial ni procede del SUA original.

## Iconos (`iconos\*.svg`)

- **Colección:** Lucide, paquete `lucide-static` versión 1.72.0.
- **Licencia:** ISC (texto completo en `iconos\LICENSE`). Permite usar, copiar, modificar y distribuir, conservando el aviso de derechos y de licencia.
- **Obtenidos de:** `https://unpkg.com/lucide-static@1.72.0/icons/<nombre>.svg`, el 12-nov-2026.
- **Cambio hecho:** `stroke="currentColor"` se sustituyó por `stroke="#000000"`; el color lo pone la interfaz al teñir el icono. Los nombres de archivo son los de Lucide.

| Icono | Dónde se usa |
|---|---|
| `building-2` | Menú: Patrones |
| `users` | Menú: Trabajadores |
| `arrow-left-right` | Menú: Movimientos |
| `file-input` | Menú: Archivo de pago |
| `calculator` | Menú: Cálculo; Calcular todo el patrón; Determinar prima de RT; base del icono de la aplicación |
| `arrow-down-up` | Menú: Importar y exportar |
| `book-open` | Menú: Catálogos |
| `settings` | Menú: Sistema |
| `save` | Guardar (patrón, trabajador), Guardar cálculo, Guardar mes |
| `plus` | Nuevo (patrón, trabajador) |
| `trash-2` | Eliminar, Eliminar el seleccionado |
| `calendar-plus` | Registrar movimiento |
| `search` | Campos de búsqueda de Trabajadores y Catálogos |
| `folder-open` | Abrir archivo .SUA |
| `import` | Importar del archivo de pago, Importar TXT |
| `upload` | Exportar del patrón activo |
| `equal` | Calcular un tramo |
| `file-down` | Cédula en PDF, Relación en PDF |
| `file-text` | Movimientos en PDF |
| `history` | Ver guardado |
| `database-backup` | Respaldar ahora |
| `archive-restore` | Restaurar el elegido |
| `shrink` | Compactar base |
| `circle-check` | Mensaje correcto |
| `triangle-alert` | Mensaje de advertencia |
| `circle-x` | Mensaje de error |
| `info` | Mensaje informativo |

En la carpeta pero todavía sin usar, porque la interfaz aún no tiene dónde ponerlos: `printer` (imprimir), `circle-help` (ayuda), `user-plus`, `user-minus`, `banknote`, `calendar-x`, `stethoscope` y `house` (tipos de movimiento: la tabla de movimientos solo admite texto).

## Icono de la aplicación (`app.svg`, `app.ico`, `app.png`)

- **Composición propia y provisional:** la calculadora de Lucide (`calculator`, licencia ISC) en blanco sobre un cuadro azul redondeado (`#1d6fd8`).
- `app.ico` (16, 24, 32, 48, 64 y 256 px) se incrusta en `sua.exe` al compilar (`build.rs`, con `winresource`); `app.png` es el icono de la ventana; `app.svg` va en la cabecera del menú.
- Se generan a partir de `app.svg`. Si cambia el nombre o la identidad del producto, basta sustituir estos tres archivos.
