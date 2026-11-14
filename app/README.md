# OpenSUA — aplicación

Proyecto real de la reescritura descrita en `../ESPECIFICACION_SUA.md`. Los prototipos (`../prototipo/` con egui y `../prototipo-slint/`) quedan solo como referencia de las mediciones.

Estado al 13-nov-2026: **las seis fases de la especificación están terminadas** (2 fundación, 3 aplicación, 4 portabilidad, 5 calidad, 6 distribución), con nueve pantallas en Slint. El usuario hizo la prueba manual con ratón y teclado y aprobó las fases 4 y 5; la 6 quedó aprobada a reserva de la validación en otro equipo, que hará el usuario. El producto se llama **OpenSUA** desde el 13-nov-2026 (antes «SUA portable»). Las decisiones del usuario están en la tabla de «Fase 6: distribución»; de ellas salen dos fases nuevas: **7, cifrado opcional de la base, hecha el 13-nov-2026 y aprobada por el usuario** (apartado «Fase 7»), y **8, usuarios, roles y bitácora de accesos, hecha el 13-nov-2026 y aprobada por el usuario** (apartado «Fase 8»). Últimas tandas: guardar el cálculo y cédula en PDF; ausentismos e incapacidades; créditos INFONAVIT; importar y exportar TXT del SUA; restaurar un respaldo; actualización y recargos; datos afiliatorios, datos médicos de incapacidades, prima de riesgos de trabajo y listados en PDF.

## Estructura

```
crates/nucleo/              biblioteca sin interfaz gráfica
  src/config.rs             modo portable o instalado, árbol de carpetas, instancia única, detección de carpeta sincronizada
  src/bitacora.rs           logs/app.log
  src/datos/mod.rs          SQLite: apertura, integridad, respaldo, compactación
  src/datos/migraciones.rs  migraciones versionadas y carga de catálogos
  src/datos/captura.rs      patrones, trabajadores, prima de RT e importación desde un .SUA
  src/dominio/validar.rs    NSS, CURP, RFC, crédito INFONAVIT
  src/datos/tasas.rs        tasas y bases vigentes por periodo, leídas de los catálogos
  src/dominio/calculo.rs    motor de cuotas: IMSS mensual, RCV e INFONAVIT bimestral
  src/dominio/comprobar.rs  comparación de un .SUA contra el cálculo propio
  src/dominio/tramos.rs     de movimientos a tramos de días cotizados
  src/dominio/credito.rs    de movimientos de crédito INFONAVIT a tramos con descuento
  src/dominio/extemporaneo.rs  vencimiento, meses de mora, actualización y recargos
  src/dominio/prima_rt.rs   determinación anual de la prima de riesgos de trabajo
  src/datos/periodo.rs      captura de movimientos y cálculo de un patrón completo por periodo
  src/datos/calculos.rs     cálculos guardados: uno por patrón y periodo
  src/datos/cifrado.rs      cifrado opcional de la base (SQLCipher): abrir con contraseña, ponerla, cambiarla y quitarla
  src/datos/usuarios.rs     usuarios, roles, sesión, permisos y bitácora de accesos
  src/datos/restaurar.rs    restauración de un respaldo sobre la base en uso, y recuperación de una base dañada al abrir
  src/datos/expediente.rs   datos afiliatorios, datos médicos de incapacidades y siniestralidad del año
  src/reportes/pdf.rs       escritor mínimo de PDF, sin dependencias (Helvetica, WinAnsi)
  src/reportes/cedula.rs    cédula de determinación de cuotas en PDF
  src/reportes/listados.rs  relación de trabajadores y movimientos del periodo en PDF
  src/io/pago.rs            lector del archivo de pago .SUA (base 62 incluida)
  src/io/anonimo.rs         copia de un .SUA sin datos personales
  src/io/txt.rs             importación y exportación de los TXT de ancho fijo del SUA
  migraciones/              SQL de cada versión de esquema
  oro/                      archivos de pago reales anonimizados (pruebas de oro) y su LEEME.md
  tests/oro.rs              pruebas de oro: el motor contra los importes del SUA oficial
  tests/aislamiento.rs      el cifrado no depende de la configuración de OpenSSL del equipo
  tests/robustez.rs         archivos dañados o manipulados: nunca un cierre de golpe
  tests/rendimiento.rs      medición con 100, 1 000 y 5 000 trabajadores (no corre con cargo test)
  semilla/                  catálogos CSV que se compilan dentro del ejecutable (21 del SUA.MDB + salarios y UMA 2019–2026 + UMI 2016–2026)
crates/ui/                  ejecutable `sua`: interfaz gráfica (Slint, render por software)
  recursos/                 iconos de Lucide (licencia ISC), icono de la aplicación y `ORIGEN.md`
  ui/app.slint              pantallas
  src/main.rs               conexión de las pantallas con el núcleo
  src/autoprueba.rs         autoprueba de la interfaz
crates/sua/                 ejecutable `sua-cli`: línea de comandos
```

## Compilar y probar

```
export PATH="$HOME/.cargo/bin:$PATH"      # cargo no está en el PATH de la terminal
export PATH="/c/Strawberry/perl/bin:/c/Program Files/NASM:$PATH"   # OpenSSL (cifrado) se compila con Strawberry Perl y NASM
cargo test                                # 131 pruebas: 121 del núcleo, 1 de aislamiento, 4 de oro y 5 de robustez
cargo test --release --test rendimiento -- --ignored --nocapture    # tabla de tiempos
cargo deny check advisories               # dependencias contra la base de avisos de RustSec (usa la red)
cargo build --release                     # target\release\sua.exe y sua-cli.exe
```

**Paquete portable.** Desde PowerShell: `..\herramientas\empaquetar.ps1`. Compila aparte (en `target\paquete`, unos cuatro minutos la primera vez) sustituyendo las rutas del equipo de desarrollo, revisa los ejecutables con `..\herramientas\revisar_exe.py` (falla si importan una DLL que no es del sistema o si conservan una ruta absoluta) y deja en `dist\paquete\` la carpeta `OpenSUA-Portable`, el `.zip`, el instalador `.msi` y `SHA256SUMS.txt` (detalle en «Fase 6: distribución»). Necesita `pandoc` para el manual y, para el `.msi`, WiX en `target\herramientas\wix`. Los textos que viajan en el paquete están en `paquete\`. Lo que se entrega a un usuario sale de aquí, no de `target\release`: un `cargo build` normal deja en el `.exe` cientos de rutas con el nombre del usuario que compiló.

Las pruebas usan solo datos ficticios. Dos comprobaciones opcionales con archivos de pago reales, que no se copian al repositorio:

```
SUA_MUESTRAS='D:\otros\sua\SUA' cargo test muestras_reales            # lector, motor de cálculo e importación de aseg.txt
SUA_PDF_DE_PRUEBA=<archivo.pdf> cargo test un_mes_impar                # deja una cédula ficticia de 100 trabajadores para verla
```

**Autoprueba de la interfaz.** Acciona por programa los mismos callbacks que los botones y comprueba el resultado en pantalla y en la base. Debe correr sobre una carpeta de datos vacía:

```
mkdir dist\prueba && copy target\release\sua*.exe dist\prueba\
dist\prueba\sua-cli.exe activar-portable
set SUA_AUTOPRUEBA=1
set SUA_AUTOPRUEBA_SUA=<ruta a un .SUA real>     (opcional: añade 5 pasos de lectura, comprobación e importación)
dist\prueba\sua.exe                               resultado en dist\prueba\logs\autoprueba.log
```

Si se usó un archivo real, **borre la carpeta de prueba al terminar**: la base queda con datos personales.

**Capturas con datos.** `SUA_VENTANA=1024x600` fija el tamaño de la ventana; `SUA_SECCION=0..7` abre una pantalla; `SUA_ESCENA=movimientos:<tipo 0..11>` elige al primer trabajador y un tipo de movimiento; `SUA_ESCENA=calculo:<AAAAMM>` calcula todo el patrón. Se capturan con `..\herramientas\medir_ui.ps1`.

## Pantallas (`sua.exe`)

| Pantalla | Qué hace |
|---|---|
| Patrones | Lista, alta, cambio y baja. Prima de RT por vigencia y **determinación anual de la prima** a partir de los riesgos de trabajo capturados. Un patrón con trabajadores no se puede eliminar |
| Trabajadores | Por patrón, con búsqueda por NSS, nombre, RFC o CURP. Alta, cambio y baja con validación; **datos afiliatorios** opcionales; relación de trabajadores en PDF |
| Archivo de pago | Abre un `.SUA`, muestra totales por ramo y las cuotas obreras decodificadas, verifica su consistencia, **recalcula las cuotas y las compara con las del archivo**, e importa patrón, trabajadores y movimientos (con respaldo previo) |
| Movimientos | Del trabajador elegido en Trabajadores: lista, alta (01), baja (02), modificación de salario (07) y reingreso (08), con validación de secuencia; **ausentismo (11) e incapacidad (12)** con fecha, días y folio, y en la incapacidad sus **datos médicos** (ramo, riesgo, secuela, control, porcentaje); **crédito INFONAVIT (15 a 20)**: inicio, suspensión, reinicio y modificaciones de tipo, valor y número; eliminar |
| Cálculo | **Todo el patrón** en un periodo, por tramos a partir de los movimientos, con detalle por trabajador y por ramo; o un tramo suelto (salario y días). Decreto opcional. El cálculo de un patrón se puede **guardar** (uno por patrón y periodo; guardar de nuevo sustituye), **abrir después** («Ver guardado») y sacar como **cédula en PDF**. Con una **fecha de pago** posterior al vencimiento añade actualización y recargos. Movimientos del periodo en PDF |
| Importar y exportar | TXT de ancho fijo del SUA: patrones (313 caracteres), trabajadores (168), movimientos (49), créditos INFONAVIT (52), datos afiliatorios (80) e incapacidades (57). Importa con respaldo previo y lista los renglones rechazados con su motivo; exporta los del patrón activo a `archivos\<registro patronal>\` |
| Catálogos | Los 23 catálogos, con búsqueda. En `INPC` se capturan el INPC y la tasa de recargos de los meses nuevos |
| Sistema | Modo, rutas, esquema, integridad, respaldos; respaldar y compactar; **restaurar un respaldo** de la lista, con confirmación y copia previa de la base actual; advertencia fija si la carpeta de datos está en una carpeta sincronizada; **contraseña de la base** (ponerla, cambiarla, quitarla) y aviso de respaldos sin cifrar; **usuarios** (altas, roles, desactivar), **mi contraseña** y **bitácora de accesos** |

Al abrir, si el archivo de la base está dañado, el programa propone restaurar el respaldo más reciente que sirva (detalle en «Después de la fase 6»).

Reglas de validación: NSS y CURP con dígito verificador incorrecto se rechazan; en el RFC un dígito incorrecto es solo advertencia. Los textos se guardan en mayúsculas.

## Línea de comandos (`sua-cli.exe`)

```
sua-cli diagnostico            prepara carpetas, abre la base, migra y muestra el estado
sua-cli respaldar              copia la base a data\respaldos
sua-cli compactar              compacta la base
sua-cli recuperar              si la base está dañada, la sustituye por el respaldo más reciente que sirva
sua-cli clave                  pone, cambia o quita la contraseña que cifra la base
sua-cli restablecer-usuario <nombre>   pone una contraseña nueva a un administrador que olvidó la suya
sua-cli activar-portable       crea config\settings.toml con portable = true
sua-cli leer-sua <archivo>     lee un .SUA, comprueba sumas, conteos, tamaño e importes codificados, y lo recalcula
sua-cli importar-sua <archivo> carga patrón, trabajadores y movimientos de un .SUA
sua-cli anonimizar-sua <archivo> <salida>   copia del .SUA sin datos personales (para compartir un caso o añadir una prueba de oro)
sua-cli ejemplo                carga un patrón y cinco trabajadores ficticios
```

`leer-sua` no muestra nombres, NSS, RFC ni CURP. Código de salida: 0 correcto, 1 con diferencias, 2 error.

## Qué quedó comprobado

| Aspecto | Resultado |
|---|---|
| Pruebas | 131 de 131 (121 del núcleo, 1 de aislamiento de OpenSSL, 4 de oro, 5 de robustez) |
| Calidad (fase 5) | ver el apartado «Fase 5: calidad», abajo |
| Portabilidad (fase 4) | ver el apartado «Fase 4: portabilidad», abajo |
| Días a partir de movimientos | 28 de 28 (mes) y 39 de 39 (bimestre) contra los archivos reales |
| Cálculo de un patrón completo | importando cada archivo real y calculando su periodo, los totales coinciden salvo un centavo |
| Motor de cálculo | 455 de 456 importes de 5 archivos de pago reales de 2006–2008, y los de una Emisión del IMSS de 2013 |
| Lector de `.SUA` | pasa con los 6 archivos de pago reales |
| Autoprueba de la interfaz | 95 de 95 pasos con un archivo de pago (sirve uno de `crates\nucleo\oro\`, sin datos personales); 90 sin él |
| Revisión de diseño | crítica de las ocho pantallas guardada en `..\.impeccable\critique\` (21 de 40). Aplicado: Movimientos muestra solo los campos del tipo elegido, con listas en lugar de códigos; el resultado del cálculo lleva total, rubros, avisos y tabla por trabajador o por ramo; los textos de resultado se pueden seleccionar y copiar; el patrón activo se elige en una franja fija arriba de todas las pantallas, y elegir una fila en Patrones lo activa. Además: confirmación antes de eliminar patrón, trabajador o movimiento; el estado de cada mensaje (correcto, advertencia, error) lo fija el programa y no el texto; listas desplegables para entidad, tipo de trabajador, jornada, sexo y tipo de salario; aviso bajo el campo mientras se escribe una fecha, un periodo o un NSS mal formados; respaldos listados con hora local y motivo legible; formularios con desplazamiento. Tras la segunda crítica (26 de 40): el botón Eliminar se deshabilita con su razón cuando no se puede borrar, y la confirmación ya no promete un respaldo; Guardar y su mensaje quedan fuera de la zona que se desplaza; Cálculo tiene dos modos («Todo el patrón», principal, y «Un tramo suelto») y sus errores salen con icono; el trabajador elegido aparece en la franja de contexto; el menú y los grupos de opciones marcan lo elegido con un fondo tenue y el color de acento queda para la acción principal; atajos Ctrl+S, Ctrl+N, Ctrl+1 a 8 y Enter en los campos de Movimientos y Cálculo; el periodo propone el mes anterior. Tras la tercera crítica (26 de 40): los motivos de bloqueo dicen que un registro con historial se conserva; la tabla marca la fila cargada; formularios por secciones; Sistema, Importar y exportar y Archivo de pago dan su resultado con icono (advertencia si hubo renglones rechazados); Enter guarda en Patrones y Trabajadores; el pie muestra solo los atajos de la pantalla; el menú lleva el número de atajo; rubros y columnas del cálculo con los mismos nombres; pantalla Ayuda (Ctrl+9) con el orden de trabajo y un glosario. Ventana mínima: 1056 × 600. Revisado con capturas, también en el tamaño mínimo |
| Restaurar un respaldo | devuelve la base al momento del respaldo y la deja utilizable sin reiniciar; un respaldo de esquema anterior se migra; un archivo que no es respaldo, uno dañado, la propia base o uno de una versión más nueva se rechazan sin tocar nada; probado en modo portable e instalado (WAL) |
| TXT del SUA | las dos exportaciones reales de trabajadores (`aseg.txt`, de 164 y de 167 caracteres) se importan completas: 12 de 12 renglones. Exportar de una base e importar en otra vacía deja los mismos patrones, trabajadores y movimientos, y el mismo cálculo |
| Cálculo guardado | se recupera idéntico al calculado; no cambia aunque después cambie la captura; al recalcular se dice si el guardado es igual o distinto |
| Cédula en PDF | estructura del archivo verificada por prueba (referencias, largos, páginas); dibujo revisado a ojo con 1 y con 100 trabajadores; acentos correctos; 11 KB con un trabajador |
| Memoria de la aplicación | 28–29 MB de trabajo (igual con iconos), 10–11 MB privados, 8 hilos, en las cuatro pantallas medidas |
| Ejecutables | `sua.exe` 21.2 MB y `sua-cli.exe` 7.0 MB desde la fase 7 (el cifrado añade unos 5 MB a cada uno) |
| Iconos | Lucide en menú, botones y mensajes, dibujados y teñidos por el render por software; icono incrustado en `sua.exe` (comprobado extrayéndolo) |
| Dibujo | comprobado con capturas de Patrones, Trabajadores, Movimientos, Cálculo y Sistema |
| Modo portable | con el marcador, todo queda junto al `.exe` |
| Base nueva | esquema versión 8, 23 catálogos, 4 714 filas |
| Migración real | la base de demostración pasó de esquema 2 a 3, y así hasta la 7, cada vez con respaldo previo automático |
| Migración | una base de versión anterior se respalda antes de migrar y conserva los datos del usuario |
| Respaldos | copia hecha por SQLite; se conservan los 10 más recientes |

## Lo que no se probó

- **Uso real con ratón y teclado:** lo probó el usuario el 13-nov-2026, sin reportar fallos. La autoprueba acciona los callbacks, no los clics: no cubre el foco, el orden de tabulación, el diálogo de abrir archivo ni el redimensionado de la ventana.
- Las pantallas Archivo de pago y Catálogos no se revisaron con captura.
- **Cédula en PDF:** no se comparó con una cédula del SUA oficial (no hay ninguna en la muestra); el formato es propio y cada página dice que es un documento de trabajo. No se probó que el visor de PDF se abra al pulsar el botón (la autoprueba lo omite), ni la impresión en papel, ni un PDF con el archivo abierto en otro programa. Importes de un millón o más en dos columnas contiguas del detalle quedan pegados.
- **Cálculo de periodos actuales.** La migración 3 carga salario mínimo y UMA de 2019 a 2026 (`../Salario_minimo_y_UMA_2019_2026.csv`, aportado por el usuario), así que ya se calculan con los valores de cada año. Pero **no hay ningún archivo de pago posterior a 2008** que confirme la base en UMA, la cesantía patronal por rangos ni el tratamiento de la zona fronteriza: esas reglas salen de la ley, no de una muestra. El salario mínimo de la frontera norte se guarda (`SALARIO_ZLFN`) pero el motor todavía no lo usa.
- Desde enero de 2027 el programa avisará que faltan los valores del año; hay que añadirlos cada año con una migración nueva.
- **Ausentismos e incapacidades:** programados según el artículo 31 de la Ley del Seguro Social, **sin ningún caso real que lo confirme** (no hay ninguno en la muestra). Falta comprobar contra el SUA: que el día de incidencia se asigne al tramo de salario en que cae, y cómo reporta los «días cotizados» (aquí son los días del tramo, con incapacidad y ausentismo aparte).
- **Amortización de créditos INFONAVIT: sin ningún caso real que la confirme.** Las fórmulas (abajo, en Decisiones) son las de uso común; falta comprobar contra el SUA el prorrateo por días de la cuota fija y del factor, que los días de ausentismo e incapacidad no lleven descuento, y el redondeo. El seguro de daños se toma del catálogo `Parametros` del `SUA.MDB`, que dice **13.00**; otras fuentes hablan de 15.00: confirmar.
- **Actualización y recargos:** lo único comprobado con un archivo real es un mes de recargos sin actualización (los de retiro coinciden trabajador por trabajador; los de cesantía en 7 de 8). La actualización con INPC, varios meses de mora y el corrimiento del vencimiento por día inhábil salen de la regla general, sin caso real. Los días inhábiles son los festivos de fecha fija del catálogo del SUA: no contempla los que la ley recorre a lunes. No se calculan multas ni se limita a cinco años.
- Modificación de salario dentro del periodo (programada según la regla general, sin caso real) y tope de 25 veces. El cálculo del patrón avisa cuántos movimientos no pudo aplicar.
- Fluidez del render por software con listas largas de trabajadores.
- Modo instalado (sin el marcador portable).
- Windows limpio, memoria USB real, otra cuenta de usuario (estándar), máquina virtual, escritorio remoto, Windows 10. Detalle en «Fase 4: portabilidad».

## Decisiones tomadas

- **Slint con render por software**, decidido por el usuario tras la medición (`../Comparacion_UI_egui_Slint.md`). El binario no incluye motor de GPU.
- **Dos ejecutables:** `sua.exe` es de ventana y no escribe en consola; la línea de comandos va aparte.
- **Sin el marcador, el modo es instalado**, como pide la especificación (§2.3).
- **Importar un `.SUA` no rechaza por dígitos verificadores:** los datos vienen de un archivo ya presentado; solo se cuenta cuántos no pasan la validación. Los importes no se importan, se recalcularán.
- **El motor calcula en punto flotante a propósito**, porque así lo hace el SUA y es la única forma de dar los mismos centavos (`../Decompilacion_SUA.md` §15). Los resultados se guardan en centavos enteros.
- **Ausentismos e incapacidades (LSS art. 31):** en los días de ausentismo solo se cotiza enfermedades y maternidad (cuota fija, excedente, prestaciones en dinero y gastos médicos); en los de incapacidad no se cotiza nada salvo retiro, y se sigue pagando la aportación al INFONAVIT. Un ausentismo va de 1 a 7 días, no cruza de mes y no puede sumar más de 7 en el mes; la incapacidad lleva folio de dos letras y seis números, de 1 a 99 días, y puede cruzar de mes. No se admiten incidencias traslapadas, ni en días en que el trabajador no está activo, ni una baja durante una incapacidad. De la incapacidad solo se capturan fecha, días y folio: el ramo, el tipo de riesgo y la secuela (que sirven para la prima de riesgos de trabajo, no para las cuotas) quedan pendientes.
- **Créditos INFONAVIT.** El valor del descuento se guarda en diezmilésimas. Amortización del bimestre, por cada tramo de salario y de crédito y redondeada por tramo: porcentaje = salario topado × días × porcentaje; cuota fija = cuota mensual × 2 ÷ días del bimestre × días; factor = factor × UMI diaria del año × 2 ÷ días del bimestre × días (antes de 2016, sobre el salario mínimo). Los días son los que tienen descuento vigente, sin ausentismos ni incapacidades; el día de la suspensión todavía lleva descuento. Si hay amortización se añade el seguro de daños a la vivienda, fijo por bimestre desde 2006; el SUA lo suma a la amortización, aquí se muestra aparte. La aportación patronal del 5 % no cambia. La UMI está en el catálogo `UMI` (migración 6, archivo aportado por el usuario); **cada año hay que añadir su valor**, igual que salario mínimo y UMA. No se hace: validar el dígito verificador del crédito, las reglas de fecha de inicio por tipo de descuento, la tabla de disminución de porcentaje (créditos anteriores a 1998), ni generar solos la suspensión al dar de baja y el reinicio al reingresar (no hace falta para el cálculo: fuera del empleo no hay días).
- **«Días» en la cédula** es el neto de ausentismos, como en la cédula del SUA (manual, p. 194); incapacidades y ausentismos van en columnas aparte.
- **TXT del SUA.** Windows‑1252 y CRLF. El archivo de trabajadores se acepta de 164 a 168 caracteres; los demás, hasta su largo, completando con espacios lo que falte al final. Cada renglón se valida igual que en la captura y se guarda por separado. Un trabajador nuevo queda dado de alta (movimiento 01) en su fecha de alta, y si trae crédito, con su inicio de crédito. Volver a importar actualiza y no duplica. En patrones la Ñ viaja como diagonal, como pide el manual. Límite del formato: el renglón del trabajador solo lleva el salario y el descuento vigentes, así que el alta se registra con el salario actual, no con el de origen.
- **Restaurar un respaldo.** Se valida antes (íntegro, de este programa, de un esquema conocido), se guarda la base actual como `antes-de-restaurar` y se copia el respaldo encima en una sola transacción de SQLite: si algo falla, la base queda como estaba. La aplicación no se reinicia: vuelve a cargar las pantallas. Solo se restauran respaldos de la lista (carpeta `data\respaldos`); para uno guardado en otra parte hay que copiarlo ahí. Se conservan los 10 respaldos más recientes, contando el previo a restaurar.
- **Pago extemporáneo (CFF, artículos 17‑A y 21).** Vence el 17 del mes siguiente al periodo, o el siguiente día hábil. Factor de actualización = INPC del mes anterior al pago entre INPC del mes del periodo, truncado al diezmilésimo y nunca menor que 1; si el INPC del mes anterior al pago no está, se usa el más reciente registrado y se avisa. Recargos = importe actualizado × suma de las tasas mensuales desde el mes del vencimiento, por mes o fracción contado de 17 a 17; si falta la tasa de un mes se usa la última y se avisa. Se calcula por trabajador, por separado la parte patronal y la obrera de cada rubro, como apunta el archivo real. El seguro de daños a la vivienda no lleva actualización ni recargos (manual, p. 200). El catálogo `INPC` que venía con el SUA llega a enero de 2026: **los meses siguientes hay que capturarlos** en Catálogos → INPC; sin el INPC del mes del periodo no se puede calcular un pago extemporáneo de ese periodo.
- **Prima de riesgos de trabajo.** Fórmula de la ley, impresa en las cédulas reales: `[(S/365) + 28 × (I + D)] × (F/N) + 0.005`. De lo capturado se toman los casos de riesgo de trabajo terminados en el año, sin los accidentes en trayecto; `N` son los días cotizados del año (sin ausentismos ni incapacidades) entre 365. La prima nueva no varía más de un punto respecto de la anterior ni sale del mínimo y máximo del año. Se propone en los campos de prima del patrón y el usuario decide si la guarda. **Sin comprobar con una determinación real:** el redondeo a cinco decimales, el cálculo exacto de `N` y qué casos cuentan como terminados. Usa el factor de prima sin acreditación de la STPS; no aplica las reglas para empresas de menos de diez trabajadores. No genera el formulario ni el archivo para el IMSS.
- **Datos médicos de la incapacidad:** opcionales, con los códigos del catálogo `CatalogoRT`. No cambian las cuotas; una incapacidad sin ellos no cuenta para la prima y se avisa.
- **TXT de datos afiliatorios e incapacidades:** layouts del manual, sin archivo real. La incapacidad se exporta solo si tiene datos médicos; al importar, si ya existía por el archivo de movimientos, solo se le añaden.
- **Listas del formulario de trabajadores.** Tipo de trabajador (1 permanente, 2 eventual de ciudad, 3 eventual de la construcción, 4 eventual del campo) y semana o jornada reducida (0 semana completa, 1 a 5 días, 6 jornada reducida): los nombres de las opciones son los que usa el SUA según se recuerdan, **sin confirmar contra el programa original**. Si un trabajador importado trae un código que la lista no conoce, se conserva al guardar.
- **Solo lectura del `.SUA`.** Falta el algoritmo del número de control del registro 06.
- **PDF sin bibliotecas:** el escritor propio (`reportes/pdf.rs`) usa las fuentes estándar con codificación WinAnsi, que cubre el español; no incrusta fuentes ni añade dependencias. Los PDF van a la carpeta `reportes\` como `cedula_<registro>_<AAAAMM>.pdf`; generar de nuevo sobrescribe.
- **El cálculo guardado es una fotografía:** no sigue a la captura. Al eliminar un patrón se eliminan sus cálculos guardados. Las fechas de guardado y de generación van en hora UTC.

## Fase 4: portabilidad

Hecha el 13-nov-2026 sobre el paquete que arma `empaquetar.ps1` (versión 0.2.0), en el equipo de desarrollo (Windows 11 Home).

**Cambios de la fase**

- **Carpeta que no se puede escribir** (especificación §2.3). Antes la aplicación mostraba un error y cerraba. Ahora avisa y pregunta si se quiere elegir otra carpeta para los datos; dentro de la elegida crea `SUA-datos`. No recuerda la elección (no hay dónde guardarla sin escribir fuera): pregunta cada vez. Para no preguntar, la variable de entorno `SUA_DATOS` fija la carpeta de datos; la respetan `sua.exe` y `sua-cli.exe`.
- **Rutas del equipo de desarrollo fuera del ejecutable.** Un `cargo build` normal dejaba 445 rutas absolutas en `sua.exe` (carpeta del usuario y del proyecto, en los mensajes de error internos). El paquete se compila con `--remap-path-prefix` y queda en cero.
- **Paquete:** carpeta, `.zip` (9.0 MB) y SHA‑256, con `LEEME.txt` para el usuario.

**Comprobado**

| Prueba | Resultado |
|---|---|
| DLL que importan los ejecutables | `sua.exe` 18 y `sua-cli.exe` 4, todas de Windows; ninguna de red (el programa no usa la red) ni `VCRUNTIME140.dll` |
| Rutas absolutas en los ejecutables | 0 en los dos; tampoco aparece el nombre del usuario |
| Descomprimir y usar en una carpeta con espacios, acentos y ñ | autoprueba 90 de 90 |
| Nada fuera de la carpeta | tras la autoprueba no existen `%APPDATA%\SUA` ni `%LOCALAPPDATA%\SUA`; todo queda junto al `.exe` |
| Traslado a otra unidad (de `C:` a `D:`) con datos | abre sin migrar, integridad correcta, mismos datos; la base y `settings.toml` no guardan ninguna ruta absoluta |
| Sin privilegios de administrador | autoprueba 90 de 90 lanzada sin elevar (nivel de integridad medio), con el mismo usuario |
| Carpeta de solo lectura (escritura negada por permisos) | `sua-cli` termina con «No se puede escribir en la carpeta…»; `sua.exe` muestra el aviso con la pregunta (visto en captura); con `SUA_DATOS` abre y trabaja en la otra carpeta |
| Dos copias sobre la misma carpeta | la segunda se rechaza con su mensaje; al cerrar la primera, abre |

**Sin probar, porque no se puede desde este equipo**

- **Memoria USB real** (no había ninguna conectada) y USB con el interruptor de protección contra escritura. La carpeta de solo lectura se simuló con permisos.
- **Elegir la carpeta en el diálogo** tras el aviso: solo se vio el aviso; el camino completo con el ratón no se recorrió. La parte que viene después (crear `SUA-datos` y abrir ahí) sí está probada por programa.
- **Otra cuenta de usuario estándar**, **Windows 10**, **un Windows recién instalado** y **una máquina virtual sin GPU** o por escritorio remoto. La revisión de DLL indica que no falta nada, pero no sustituye a abrirlo ahí.
- **SmartScreen y antivirus:** el ejecutable no está firmado; al bajarlo de internet o abrirlo desde USB Windows puede mostrar su aviso. Firmarlo requiere un certificado (fase 6).
- **Actualizar sin perder datos** sustituyendo los `.exe`: el mecanismo (migración con respaldo previo) está probado; no hay todavía una versión siguiente con la cual hacer la prueba completa.

## Fase 6: distribución

Hecha el 13-nov-2026. Entregables en `dist\paquete\`, armados por `..\herramientas\empaquetar.ps1`:

| Archivo | Qué es |
|---|---|
| `OpenSUA-Portable-0.2.0.zip` (9.6 MB) | La carpeta portable: `sua.exe`, `sua-cli.exe`, `Manual.html`, `LEEME.txt`, `LICENCIAS-DE-TERCEROS.txt` y `config\settings.toml` con el marcador portable. Sin datos |
| `OpenSUA-0.2.0.msi` (8.2 MB) | Instalador opcional, por usuario: no pide administrador. Instala en `%LOCALAPPDATA%\Programs\OpenSUA`, con acceso en el menú Inicio. Trabaja en modo instalado: datos en `%APPDATA%\SUA`, que se conservan al desinstalar |
| `SHA256SUMS.txt` | Huellas SHA‑256 de los dos anteriores |

**Qué se añadió**

- **Manual de usuario** (`paquete\Manual.md` → `Manual.html`, un solo archivo con estilos y cinco capturas dentro; claro u oscuro según el equipo; se puede imprimir). Las capturas están en `paquete\manual\` y se sacan con `medir_ui.ps1` sobre los datos de demostración.
- **«Acerca de»** en la pantalla Ayuda: versión, aviso de que no es del IMSS ni sustituye al SUA oficial, aviso de que los datos no van cifrados, y la **atribución a Slint** que pide su licencia gratuita.
- **`LICENCIAS-DE-TERCEROS.txt`**, generado por `..\herramientas\licencias.py`: Slint, Lucide (texto completo de su licencia), SQLite y 342 bibliotecas de Rust con su licencia declarada. Ninguna obliga a publicar el código de este programa, **siempre que Slint se use bajo su licencia gratuita** (ver abajo).
- **Instalador** (`paquete\instalador.wxs`, WiX 5.0.2). WiX no se instaló en el sistema: está en `target\herramientas\wix` y se repone con `dotnet tool install wix --version 5.0.2 --tool-path target\herramientas\wix`. Sin él, el script arma solo el `.zip`. Se eligió la versión 5 porque la 6 pide aceptar un contrato de cuota de mantenimiento, decisión que es del usuario.
- **Firma opcional:** si la variable `SUA_FIRMA_HUELLA` trae la huella de un certificado de firma de código y está `signtool`, el script firma los `.exe` y el `.msi`. **Sin probar: no hay certificado.**
- Escena de captura nueva: `SUA_ESCENA=ayuda:fin` abre Ayuda desplazada al final.

**Comprobado en el equipo de desarrollo**

| Prueba | Resultado |
|---|---|
| Autoprueba de la interfaz sobre el `.zip` definitivo | 95 de 95 |
| Instalar el `.msi` en silencio | Código 0; archivos en la carpeta del usuario, acceso en el menú Inicio, entrada para desinstalar; instalación por usuario según el registro de Windows Installer |
| Programa instalado | Modo instalado, datos en `%APPDATA%\SUA`, diario WAL, integridad correcta; **autoprueba 95 de 95 en modo instalado** (primera vez que se prueba ese modo); no escribe nada junto al programa |
| Desinstalar | Código 0; se quitan carpeta, acceso y entradas del registro; **los datos se conservan** |
| Actualizar sin perder datos | La carpeta de demostración, con datos de versiones anteriores, se abrió con cada ejecutable nuevo de las fases 4 a 6 (incluido el cambio de SQLite) con integridad correcta |
| Manual | Revisado en imagen: estilos, índice e imágenes correctos |

La instalación de prueba se desinstaló y se borraron sus datos; el equipo quedó como estaba.

**Sin hacer o sin probar**

- **Validación en un equipo limpio** (Windows 10, recién instalado, cuenta estándar, USB real, máquina virtual): no se puede desde aquí. La lista de pasos para hacerla está en `..\Validacion_en_otro_equipo.md`.
- La instalación de prueba se hizo desde una sesión con privilegios de administrador; falta confirmarla con una cuenta estándar (punto 12 de esa lista).
- **Firma del ejecutable:** sin certificado. Sin firma, SmartScreen avisa al abrir el programa bajado de internet o desde USB, y algún antivirus puede bloquearlo.
- No se probó actualizar con el `.msi` de una versión siguiente sobre esta (no existe aún); el instalador lleva el identificador de actualización fijo para que funcione.
- macOS y Linux: fuera de alcance por ahora.

**Decisiones del usuario (respondidas el 13-nov-2026)**

| Asunto | Decisión |
|---|---|
| Fase 6 | Aprobada a reserva de la validación en otro equipo, que hará el usuario y de la que pasará resultados |
| Licencia de Slint | La gratuita con atribución. La atribución de Ayuda → «Acerca de» no se puede quitar |
| Nombre del producto | **OpenSUA**. Cambiado en la ventana, los diálogos, los PDF, el manual, el LEEME, el instalador (carpeta, accesos, clave de registro; mismo `UpgradeCode`), el nombre de producto del `.exe` y los entregables (`OpenSUA-Portable-<versión>.zip`, `OpenSUA-<versión>.msi`). No cambian: los ejecutables (`sua.exe`, `sua-cli.exe`), la versión 0.2.0, el icono provisional, la carpeta de datos del modo instalado (`%APPDATA%\SUA`) ni `SUA-datos` |
| Firma de código | Se reparte sin firmar. El camino de firma de `empaquetar.ps1` sigue sin probar |
| Cifrado de la base | **Sí, opcional** (el usuario decide si pone contraseña). Hecho: fase 7 |
| Usuarios y roles | **Sí, completos:** inicio de sesión, roles y bitácora por usuario. Hecho: fase 8 |
| Alcance | **Nivel A.** No se escribirá el archivo de pago `.SUA` ni se llamará a SIPARE |
| Seguro de daños a la vivienda | El usuario no sabe si es 13.00 o 15.00: se queda en 13.00, sin confirmar |
| UMI de 2025 y 2026 | El usuario no sabe si son correctas: se quedan como están, sin confirmar |
| Git | Toda la carpeta `SUA\` excluida en `.gitignore`. Sigue sin ningún commit (no lo pidió) |
| Textos de Ayuda y de las listas | Dados por buenos |

## Fase 5: calidad

Hecha el 13-nov-2026. La especificación pide pruebas (con archivos de oro), seguridad y rendimiento.

**Pruebas de oro.** Cinco archivos de pago reales de 2006 a 2008, anonimizados, viven en `crates\nucleo\oro\` y corren con `cargo test` (antes solo se probaban con la variable `SUA_MUESTRAS` y los originales a mano). Fijan lo que el motor logra hoy: 455 de 456 importes por trabajador idénticos a los del SUA oficial, días iguales trabajador por trabajador, y totales del patrón iguales salvo el centavo conocido de febrero de 2008. Si un cambio mueve un centavo, la prueba falla. El anonimizador es también una orden de `sua-cli`; se comprobó que cada copia da la misma salida de `leer-sua` que su original y que no conserva ninguno de los 167 identificadores de los originales. La Emisión de 2013 sigue como prueba unitaria del motor.

**Seguridad: tres defectos encontrados y corregidos**

| Defecto | Efecto | Arreglo |
|---|---|---|
| Al importar un `.SUA`, una letra acentuada o Ñ en la fecha de la prima de riesgos partía el texto a media letra | La aplicación se cerraba de golpe | Se exige que el campo sean dígitos; prueba en `tests/robustez.rs` |
| Al importar un `.SUA` no se validaba el registro patronal, que da nombre a carpetas y PDF | Un archivo manipulado (registro `..\..\..\ab`) hacía escribir exportaciones y reportes fuera de la carpeta del programa | La importación rechaza un registro que no sean 11 letras o números, y los nombres de carpeta y archivo se limpian aparte (`config::nombre_seguro`) |
| El SQLite incluido tenía la vulnerabilidad RUSTSEC-2026-0459 (una base con un `-wal` manipulado puede devolver memoria ajena) | Riesgo al abrir una base o un respaldo preparado por un tercero | `rusqlite` 0.32 → 0.42, que trae SQLite corregido |

Además: 20 000 archivos de pago y unos 2 700 TXT alterados de forma determinista (recortes, basura, bytes de Windows‑1252) se leen o se rechazan sin ningún cierre; el SQL usa siempre parámetros (los únicos nombres que se insertan en el texto son de catálogos internos, entrecomillados); `cargo deny` queda sin vulnerabilidades (se acepta un aviso de biblioteca sin mantenimiento, `pin-utils`, que llega por Slint; ver `deny.toml`); `clippy` no señala nada de corrección, solo estilo.

**Rendimiento** (modo release, este equipo, disco SSD)

| Trabajadores | Importar `.SUA` | Calcular mes | Calcular bimestre | Listar | Exportar TXT | Importar TXT |
|---|---|---|---|---|---|---|
| 100 | 14 ms | 8 ms | 8 ms | 0.2 ms | 8 ms | 12 ms |
| 1 000 | 87 ms | 73 ms | 75 ms | 1.5 ms | 73 ms | 84 ms |
| 5 000 | 398 ms | 345 ms | 345 ms | 7 ms | 358 ms | 392 ms |

- **Importar TXT era el punto lento: 5 000 trabajadores tardaban 45 s** con la ventana congelada (7 s con 1 000). Dos causas: cada renglón se confirmaba en disco por separado, y por cada renglón se recorría a todos los trabajadores del patrón. Ahora el archivo entero va en una transacción y la búsqueda es por llave: 0.4 s. Cambio de comportamiento: si el programa se interrumpe a media importación no queda nada del archivo (antes quedaban los renglones ya leídos).
- Con 5 000 trabajadores la interfaz usa 37 MB en Trabajadores y 41 MB con el cálculo en pantalla (28 MB con cinco); la ventana aparece en menos de un segundo la primera vez y en unos 50 ms las siguientes. La base ocupa 1.7 MB.
- El cálculo no se pasó a un hilo aparte con barra de avance y cancelación, como sugiere la especificación (§13.1): con un tercio de segundo para 5 000 trabajadores no hace falta.
- En un disco lento o una USB los tiempos de guardar serán mayores (modo `synchronous=FULL`); no se midió.

**Sin hacer (de la sección de seguridad de la especificación); el usuario ya decidió que se hagan: fases 7 y 8**

- **Cifrado opcional de la base** (SQLCipher) para datos personales en una USB. Cambia la compilación (necesita OpenSSL) y el manejo de respaldos; conviene decidirlo antes de distribuir.
- **Usuarios, roles y bitácora de accesos** como en el SUA original. Hoy no hay inicio de sesión.
- Fluidez al desplazar listas largas: solo se comprobó con capturas que la pantalla se dibuja bien con 5 000 renglones; el desplazamiento con el ratón queda para la prueba manual.

(Restaurar al abrir una base dañada, el aviso de carpeta sincronizada y el pie que recortaba los atajos estaban en esta lista; se hicieron el 13-nov-2026: ver el apartado siguiente.)

## Fase 8: usuarios, roles y bitácora de accesos

Hecha el 13-nov-2026, a pedido del usuario (especificación §11). **Aprobada por el usuario** el mismo día; los clics siguen sin probar (apartado G de la lista de validación). **Sube el esquema a la versión 8** (tablas `usuario` y `bitacora_accesos`): una base abierta con esta versión ya no abre con las anteriores; como siempre, se respalda antes de migrar.

**Decisiones del usuario** (13-nov-2026): inicio de sesión opcional; tres roles; la contraseña olvidada del administrador se restablece con `sua-cli`; la bitácora anota sesiones y cambios.

| Asunto | Comportamiento |
|---|---|
| Sin usuarios | Todo igual que antes: no se pide nada, todo está permitido y no se anota nada |
| Primer usuario | Se crea en Sistema, «Usuarios». Tiene que ser administrador y queda con la sesión iniciada. Desde ese momento se pide usuario y contraseña al abrir |
| Administrador | Todo: usuarios, bitácora, contraseña de la base, restaurar, compactar, eliminar patrones y trabajadores |
| Capturista | Captura (también elimina movimientos y cálculos guardados, para corregir), importa, guarda cálculos, exporta TXT y respalda |
| Consulta | Ve, calcula sin guardar y saca PDF. No cambia nada, no exporta TXT ni respalda |
| Usuario que ya no debe entrar | Se desactiva, no se borra. Siempre queda al menos un administrador activo |
| Contraseñas | Huella Argon2id con sal por usuario; mínimo 8 caracteres. Cada quien cambia la suya; un administrador se la cambia a cualquiera |
| Contraseña olvidada del administrador | `sua-cli restablecer-usuario <nombre>`, sin sesión; solo para administradores; queda en la bitácora |
| Bitácora de accesos | Entradas, salidas, intentos rechazados y cada operación que cambia datos, con usuario y hora. Detalle: registro patronal, NSS, periodo o tipo; nunca nombres ni importes. Dentro de la base (cifrada si la base lo está). Se conservan 50 000 renglones. La pantalla muestra los 200 últimos |
| Restaurar un respaldo | Los usuarios y la bitácora pasan a ser los del respaldo. Si el usuario de la sesión no existe ahí, el programa se cierra y vuelve a abrir pidiendo sesión |
| `sua-cli` | Pide usuario y contraseña (o `SUA_USUARIO` y `SUA_CLAVE_USUARIO`) y respeta el rol |

**Qué protege y qué no.** Los usuarios dicen quién hizo qué y limitan errores. No impiden abrir `sua.db` con otro programa ni usar `sua-cli restablecer-usuario`: quien tiene la carpeta tiene los datos. Lo que impide leerlos es la contraseña de la base (fase 7), que se pide antes que el usuario. Así se le explicó al usuario antes de aprobar el plan, y así lo dicen la pantalla y el manual.

**Cómo está hecho**

- `datos/usuarios.rs`: `Rol`, `Permiso`, sesión dentro de `Base`, `exigir`, `con_permiso` y `anotar`. **Los permisos se comprueban en el núcleo**, no en la pantalla: la interfaz deshabilita botones por comodidad.
- Cada función pública que cambia datos (19, en `captura`, `periodo`, `calculos`, `expediente`, `tasas`, `mod`, `io/txt`, más `restaurar_con_clave`) quedó como una envoltura que llama a `con_permiso` y a su versión `_sin_control`. `con_permiso` comprueba el rol, ejecuta y, si salió bien, anota. Las operaciones que llaman a otras (una importación guarda cientos de trabajadores) se anotan una sola vez. **Una función nueva que cambie datos debe seguir el mismo patrón.**
- El respaldo previo a una migración se hace al abrir, antes de que haya sesión: usa `respaldar_sin_control`.
- Errores nuevos: `SesionRequerida`, `AccesoDenegado` (el mismo para usuario inexistente, desactivado o contraseña equivocada) y `SinPermiso` (con la razón).
- Interfaz: `VentanaClave` sirve también de inicio de sesión (`con-usuario`); propiedades `puede-capturar`, `-eliminar`, `-exportar`, `-respaldar` y `-administrar` en los 17 botones que cambian datos; secciones «Usuarios», «Mi contraseña» y «Bitácora de accesos» al final de Sistema; el pie empieza por el usuario y su rol. En `main.rs`: `iniciar_sesion`, `refrescar_usuarios`, `mostrar_usuario`, `guardar_usuario`, `relanzar`.
- Dependencias nuevas: `argon2` y `password-hash` (Rust puro, sin DLL). Se compilan optimizadas también en las pruebas (`[profile.dev.package.argon2]`), porque sin optimizar cada contraseña tarda segundos.
- SQLCipher escribía en la salida de error varias líneas por cada contraseña equivocada; se apagó (`PRAGMA cipher_log_level = NONE`).
- Variables de prueba: `SUA_USUARIO`, `SUA_CLAVE_USUARIO`, `SUA_CLAVE_USUARIO_NUEVA` (para `restablecer-usuario`) y la escena `SUA_ESCENA=sistema:fin` (Sistema desplazada al final).

| Comprobado | Resultado |
|---|---|
| Pruebas | 131 de 131: 121 del núcleo (8 nuevas de usuarios: sin usuarios, primer administrador, cada rol contra cada operación, bitácora sin datos personales, importación anotada una vez, último administrador, contraseñas y restablecimiento, restauración), 1 de aislamiento, 4 de oro, 5 de robustez |
| Autoprueba sobre el `.zip` final | 113 de 113 sin contraseña de base y **115 de 115 con contraseña de base**, con un archivo de pago de `oro/`. Los 14 pasos nuevos crean el primer administrador, un capturista y un usuario de consulta, y comprueban con cada uno los botones, el rechazo del núcleo y la bitácora |
| `sua-cli` | Sin credenciales o con contraseña equivocada no entra; consulta no respalda ni carga datos; capturista carga datos y no compacta; `restablecer-usuario` sin nombre lista los administradores, rechaza a quien no lo es y restablece al administrador (la contraseña vieja deja de servir) |
| Capturas | Ventana de inicio de sesión; Sistema como administrador (usuarios y bitácora con las acciones de la autoprueba); Sistema y Patrones como consulta, con los botones deshabilitados y el pie «consulta (Consulta: solo lectura)», en 1056 × 600 |
| Instalador | `.msi` instala y desinstala en silencio; el programa instalado abre en modo instalado (WAL) con el esquema 8 |

**No comprobado**

- Los clics: iniciar sesión escribiendo en la ventana, crear y modificar usuarios con el ratón, la lista desplegable de usuarios, «Cambiar mi contraseña». Se probó la función que hay detrás de cada botón.
- El reinicio tras restaurar un respaldo que no conoce al usuario de la sesión: la lógica del núcleo está probada; el cierre y la reapertura de la ventana por ese motivo, no (usa el mismo mecanismo que el cambio de contraseña de la base, que sí se probó).
- Con una base cifrada y con usuarios a la vez solo se probó por la línea de comandos y la autoprueba, no las dos ventanas seguidas a mano.
- Rendimiento: iniciar sesión cuesta una verificación Argon2 (décimas de segundo en este equipo); no se midió en uno lento. La bitácora añade una escritura por operación; no se repitió la tabla de tiempos de la fase 5.
- Otro equipo.

**Límites conocidos**

- No hay bloqueo por intentos fallidos ni caducidad de contraseñas; los intentos quedan en la bitácora.
- No hay cierre de sesión por inactividad ni «cambiar de usuario» sin cerrar el programa.
- La bitácora no registra consultas ni reportes generados (el usuario eligió «sesiones y cambios»).
- Un administrador puede borrar respaldos sin cifrar y restaurar respaldos; restaurar sustituye también la bitácora por la del respaldo (la restauración queda anotada en la nueva).
- Los PDF los puede sacar cualquier rol, también Consulta: son reportes, y llevan datos personales.

## Fase 7: cifrado opcional de la base

Hecha el 13-nov-2026, a pedido del usuario (especificación §11). **Aprobada por el usuario** el mismo día; los clics en las ventanas de contraseña siguen sin probar (apartado F de la lista de validación). No cambia el esquema (sigue en la versión 7).

**Qué hace.** La base puede llevar contraseña. Con ella, el archivo se cifra entero con SQLCipher 4.20 (AES-256; la clave sale de la contraseña con PBKDF2-HMAC-SHA512) y el programa la pide al abrir. Sin contraseña la base es un archivo SQLite corriente, igual que antes: las bases existentes abren sin cambio. Es un solo programa; no hay versión «con» y «sin» cifrado.

| Situación | Comportamiento |
|---|---|
| Base nueva en modo portable | Una ventana propone protegerla (contraseña dos veces, mínimo 8 caracteres). Se puede seguir sin contraseña |
| Abrir una base cifrada | Ventana de contraseña. Si no abre, se dice y se puede reintentar. Desde la misma ventana se puede restaurar un respaldo |
| Poner, cambiar o quitar la contraseña | En Sistema, «Contraseña de la base». Pide la actual, valida la nueva y confirma. **El programa se cierra, aplica el cambio y vuelve a abrir**: sustituir el archivo exige tener la base cerrada |
| Respaldos | Salen cifrados con la contraseña que tiene la base en ese momento |
| Respaldos anteriores a la contraseña | Quedan sin cifrar. Sistema avisa cuántos hay y ofrece eliminarlos |
| Restaurar un respaldo con otra protección (otra contraseña, o sin cifrar) | Sistema pide la contraseña de ese respaldo. La base conserva su propia contraseña |
| Base cifrada dañada | Da el mismo error que una contraseña equivocada. La ventana de contraseña permite restaurar el respaldo más reciente que abra con la contraseña escrita |
| TXT exportados y PDF | Sin cifrar; el mensaje de exportación lo dice |
| `sua-cli` | Pregunta la contraseña sin mostrarla, o la toma de la variable `SUA_CLAVE`. Orden nueva `sua-cli clave` |

**La contraseña no se guarda en ningún lado y no hay forma de recuperarla.** Se avisa al ofrecerla, al ponerla y en el manual.

**Cómo está hecho**

- `datos/cifrado.rs`: `conectar` (abre con contraseña y comprueba que se lee), `cambiar_clave` (escribe una copia completa con la protección nueva, la comprueba y solo entonces sustituye al original; si falla, la base queda como estaba), `exportar`, `validar_clave_nueva`.
- Errores nuevos: `FaltaClave` (la base está marcada como cifrada y no se dio contraseña) y `ClaveIncorrecta` (con contraseña el archivo no se lee: contraseña equivocada o archivo dañado, no se distinguen).
- **Marca `data\cifrada.txt`.** Un archivo cifrado y uno dañado se ven igual desde fuera. La marca dice que la base tiene contraseña, para pedirla en vez de proponer un respaldo. No contiene la contraseña. Se pone y se quita sola al abrir. Si se copia `sua.db` sin la marca, el programa creerá que está dañada: con `SUA_CLAVE` o `sua-cli` se abre igual y la marca se repone.
- Restaurar entre protecciones distintas: el respaldo se pasa primero a un archivo temporal con la protección de la base en uso (`sua.db.restaurando`, que se borra) y de ahí se copia.
- **OpenSSL no lee configuración del equipo** (`cifrado::aislar_openssl`). Sin esto, si otro programa dejó la variable `OPENSSL_CONF` (lo hacen algunos instaladores) con una configuración incompatible, la base cifrada no abría. Lo cubre `tests/aislamiento.rs`, que se lanza a sí misma en un proceso hijo con la variable puesta; se comprobó que falla sin el arreglo.
- Interfaz: `VentanaClave` en `app.slint` y, en `main.rs`, `abrir_base`, `abrir_con_ventana`, `ofrecer_clave_nueva`, `preparar_cambio_de_clave` y `aplicar_cambio_de_clave`. Sistema pasó a tener desplazamiento.
- Variables de prueba: `SUA_CLAVE` (contraseña, evita la ventana; con una carpeta vacía crea la base cifrada), `SUA_CLAVE_NUEVA` (para `sua-cli clave`; vacía la quita), `SUA_ESCENA=clave:<nueva>` (hace el cambio de contraseña de Sistema sin clics: cierra, aplica y reabre) y `SUA_AVISO_INICIAL` (texto que la copia que se cierra deja a la que abre).

**Compilación.** SQLCipher necesita OpenSSL, que se compila desde su código y queda dentro del `.exe` (ninguna DLL nueva). Requisitos nuevos en el equipo de desarrollo: **Strawberry Perl** (el Perl de Git Bash no sirve) y, opcional, **NASM**; los instaló el usuario. La primera compilación de cada carpeta de salida tarda unos 9 minutos más. La carpeta de salida debe tener una ruta corta (con la carpeta temporal de la sesión falla por el límite de 260 caracteres). `cargo` debe correr con `C:\Strawberry\perl\bin` y `C:\Program Files\NASM` delante en el PATH; `empaquetar.ps1` lo hace solo y se detiene si no encuentra Strawberry Perl.

**Paquete.** `sua.exe` 21.2 MB (antes 15.3) y `sua-cli.exe` 7.0 MB (antes 2.7); `.zip` 12.9 MB, `.msi` 11.0 MB. OpenSSL deja en el ejecutable 13 rutas de su propia compilación (`D:\otros\sua\app\target\paquete\...\openssl-build\...`): nombres de archivos fuente y carpetas de configuración que el programa no lee. `revisar_exe.py` las tolera mientras no contengan la carpeta de un usuario; el resto de la revisión no cambia. `LICENCIAS-DE-TERCEROS.txt` lleva el texto de la licencia de SQLCipher y el aviso de OpenSSL (Apache 2.0).

| Comprobado | Resultado |
|---|---|
| Pruebas | 123 de 123: 113 del núcleo (9 nuevas de cifrado y de restauración con base cifrada), 1 de aislamiento de OpenSSL, 4 de oro, 5 de robustez |
| Autoprueba sobre el `.zip` final | 99 de 99 sin contraseña; **101 de 101 con contraseña** (toda la aplicación sobre una base cifrada, con `OPENSSL_CONF` ajeno puesto): importar, calcular, respaldar, restaurar y compactar funcionan igual, y la base y sus 10 respaldos quedan cifrados |
| Cambio de contraseña con reinicio | Con el ejecutable del paquete y `SUA_ESCENA`: poner, cambiar y quitar. Tras cada paso la base abre solo con la contraseña nueva, íntegra, y la copia nueva del programa queda abierta |
| `sua-cli` | `diagnostico` con y sin contraseña, contraseña equivocada, `clave` (poner, cambiar, quitar) |
| Capturas | Ventana de contraseña, ventana de base nueva, Sistema con base cifrada y sin cifrar (también en 1056 × 600) |
| Instalador | `.msi` instala y desinstala en silencio; el programa instalado abre en modo instalado con diario WAL |
| Modo instalado (WAL) | Poner, cambiar y quitar contraseña, y respaldar y restaurar con base cifrada, por prueba del núcleo |

**No comprobado**

- Escribir la contraseña y pulsar los botones con el ratón: en la ventana de contraseña (incluido el mensaje de contraseña equivocada y «Restaurar un respaldo»), en la de base nueva y en Sistema. Se probó la lógica que hay detrás de cada botón, no el clic.
- Que el foco quede en el campo de contraseña al abrir la ventana.
- El campo «Contraseña de ese respaldo» de Sistema, que aparece tras un intento fallido: su lógica está probada en el núcleo; no se vio en pantalla.
- El botón «Eliminar los respaldos sin cifrar».
- `sua-cli` preguntando la contraseña en una consola real sin mostrarla (se probó con la entrada redirigida y con `SUA_CLAVE`).
- El tiempo de apertura en un equipo lento: derivar la clave cuesta unas décimas de segundo cada vez que se abre una conexión.
- Otro equipo: el paquete nuevo no se ha probado fuera del de desarrollo.

**Límites conocidos**

- La contraseña vive en la memoria del programa mientras está abierto.
- La bitácora, los TXT exportados, los PDF y los respaldos anteriores a la contraseña no van cifrados.
- Un archivo dañado apartado (`sua-danada-…`) conserva la protección que tenía.
- No hay límite de intentos ni espera entre ellos: lo que protege es el cifrado, no la ventana.

## Después de la fase 6: tres pendientes menores

Hechos el 13-nov-2026, sin esperar decisiones del usuario. No cambian el esquema de la base (sigue en la versión 7).

**Base dañada al abrir.** Antes solo se mostraba el error y el manual pedía sustituir el archivo a mano. Ahora, si el archivo no es una base de SQLite o no pasa la comprobación de integridad (`ErrorSua::es_base_danada`), la ventana propone el respaldo más reciente que sirva, con su fecha y cuántos patrones y trabajadores tiene. Con «Sí»: el archivo dañado se aparta como `data\sua-danada-<fecha>.db` (con su diario, si lo tenía: dejarlo haría que SQLite lo aplicara al respaldo), el respaldo se copia en su lugar, la base se abre de la forma habitual (migra si el respaldo era de un esquema anterior) y el programa abre en Sistema con el resumen. Con «No»: se muestra el error y no se toca nada. Los respaldos dañados se saltan; si ninguno sirve, se dice y la base queda como estaba. `sua-cli recuperar` hace lo mismo sin ventana. Código: `datos/restaurar.rs` (`ultimo_respaldo_util`, `recuperar`) y `abrir_base` en `crates/ui/src/main.rs`.

**Carpeta sincronizada.** `config::servicio_de_sincronizacion` reconoce OneDrive (por las variables `OneDrive`, `OneDriveConsumer` y `OneDriveCommercial`, y por el nombre de la carpeta), Dropbox, Google Drive e iCloud Drive (por el nombre). Si la carpeta de datos está dentro de una, la pantalla Sistema lleva una advertencia fija, `sua-cli diagnostico` la imprime, y la primera vez sale una ventana de aviso; para no repetirla se deja `config\aviso-sincronizacion.txt`. Es solo aviso: el programa abre igual. Reconocer por nombre puede dar un falso aviso en una carpeta que se llame «Dropbox» y no se sincronice.

**Pie de la ventana.** La ruta de datos y los atajos son ahora dos textos: si no caben, se recorta la ruta y los atajos se ven completos. Una ruta de más de 60 caracteres se abrevia por el centro (la completa está en Sistema).

| Comprobado | Resultado |
|---|---|
| Pruebas nuevas del núcleo (4) | Base dañada en el encabezado y en las páginas de datos: se detecta, se salta un respaldo dañado, se recupera del anterior, el archivo dañado y su diario quedan apartados y la base recuperada admite cambios. Sin respaldo útil no se toca nada. Reconocimiento de carpetas sincronizadas, con casos que no lo son |
| Con los ejecutables | Base de prueba dañada a propósito: `sua-cli diagnostico` da el error y sugiere `recuperar`; `sua-cli recuperar` la restaura y con la base sana no hace nada; la ventana la recupera y lo anota en la bitácora; sin respaldos, el error lo dice y no se crea ningún archivo |
| Con capturas | La ventana de pregunta y la de aviso de carpeta sincronizada; la advertencia en Sistema; el pie con una ruta de 150 caracteres en el ancho mínimo (1056), en Sistema y en Cálculo (la pantalla con más atajos) |
| Autoprueba | 95 de 95 sobre el `.zip` rearmado, con un archivo de pago de `oro\`; 90 de 90 sin él |

No comprobado: el clic en «Sí» y «No» de la pregunta (con «Sí» se probó por la vía de la autoprueba, que acepta sin preguntar; la respuesta «No» solo se leyó en el código); que el aviso de carpeta sincronizada no se repita después de aceptarlo (se vio la ventana, no se pulsó «Aceptar»); una carpeta de OneDrive o Dropbox real (se probó con una carpeta llamada «Dropbox»). Quedan como puntos 18 y 19 de `..\Validacion_en_otro_equipo.md`.

## Fase 3 cerrada: lo que queda fuera y por qué

La fase 3 (interfaz, dominio, captura, importación y exportación, reportes) se da por terminada, a falta de la aprobación del usuario. No se hizo, por estas razones:

| Qué | Por qué no |
|---|---|
| Multas del INFONAVIT | No salen de una fórmula: las determina el INFONAVIT. Haría falta un campo para capturarlas |
| Pago de diferencias | Compara lo pagado con lo que debió pagarse; necesita el archivo de pago original y decidir el Nivel A o B |
| Escribir el archivo de pago `.SUA` y pedir la línea de captura | Nivel B: falta el algoritmo del número de control del registro 06 y la decisión del usuario |
| Salario mínimo de la frontera norte | El valor está cargado, pero no se sabe con qué clave marca el SUA a un patrón de esa zona |
| Tabla de disminución de porcentaje en créditos anteriores a 1998 | La tabla no está en el `SUA.MDB` ni en el manual |
| Aportación voluntaria (movimiento 09) | No se capturó ningún caso ni su efecto en el archivo de pago |
| Icono y nombre definitivos | Los iconos de Lucide ya están aplicados; el icono de la aplicación es provisional hasta decidir nombre y color de marca (`../Imagenes_necesarias.md`, §6) |
| Licencia de Slint | Decidida el 13-nov-2026: la gratuita con atribución |

Lo que más falta no es código sino **comprobación con material real**: casi todo lo posterior al motor de cuotas sale de la ley o del manual. Sirven una cédula bimestral reciente con créditos, una de pago extemporáneo, un `.SUA` posterior a 2008, una determinación de prima presentada, y TXT reales de movimientos y créditos. Y la **prueba manual** con ratón y teclado, que nadie ha hecho.
