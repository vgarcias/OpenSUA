---
target: todas las pantallas
total_score: 21
max_score: 40
na_heuristics: 
p0_count: 0
p1_count: 3
target_identity: "file:D:\\otros\\sua\\app\\crates\\ui\\ui\\app.slint"
target_fingerprint: "sha256:5b7febcaef62a75212ded7aa1875d6dbbcd102c8c5e3e68bbb3868a5eca5e3f6"
target_path: "D:\\otros\\sua\\app\\crates\\ui\\ui\\app.slint"
timestamp: 2026-11-13T03-20-32Z
slug: app-crates-ui-ui-app-slint
---
Method: dual-agent (A: revisión de diseño en subagente aislado · B: detector en subagente aislado)

# Crítica de diseño: SUA portable, ocho pantallas

Destino: `app/crates/ui/ui/app.slint` (Patrones, Trabajadores, Movimientos, Archivo de pago, Cálculo, Importar y exportar, Catálogos, Sistema). Aplicación nativa de escritorio en Slint; modo Operate.

## Salud del diseño

| # | Heurística | Nota | Problema principal |
|---|---|---|---|
| 1 | Visibilidad del estado | 2 | El patrón activo y el periodo no se ven de forma global; el pie solo muestra una ruta de disco; nada indica progreso |
| 2 | Lenguaje del mundo real | 3 | El vocabulario del oficio está bien; Catálogos y Sistema muestran nombres crudos de la base ("IdClaveEdo", "Diario: delete") |
| 3 | Control y libertad | 2 | Eliminar no confirma ni se deshace; los formularios no tienen cancelar |
| 4 | Consistencia | 3 | El esquema lista–formulario es estable; hay dos "Jornada" y dos "Tipo" en Trabajadores, y los PDF están repartidos entre pantallas |
| 5 | Prevención de errores | 2 | Todo es texto libre: fechas, sexo, entidad, ramo, secuela, tipo de descuento |
| 6 | Reconocer antes que recordar | 2 | Los códigos se teclean de memoria; la leyenda que los explica es un párrafo de 12 px |
| 7 | Flexibilidad y eficiencia | 1 | Sin atajos de teclado ni Enter para guardar; las tablas no se ordenan |
| 8 | Estética y minimalismo | 3 | Sobrio y legible, pero los formularios son densos y planos |
| 9 | Recuperación de errores | 2 | El error llega tras Guardar, lejos del campo; el icono del mensaje se deduce del texto |
| 10 | Ayuda | 1 | No hay ayuda ni enlace al manual; solo notas pequeñas |
| | **Total** | **21/40** | **Aceptable** |

## Veredicto de especificidad

**Revisión de diseño.** La interfaz es genérica: barra lateral, tabla a la izquierda, formulario de 400 px a la derecha y bloques de texto monoespaciado. Con otras etiquetas serviría para cualquier programa de altas y bajas. Lo específico del SUA (total a pagar, vencimiento, desglose por ramo, periodo) está en la lógica y en los textos, pero no llega a la presentación.

**Detector.** Cero hallazgos sobre `app/crates/ui/ui/app.slint`. El detector no indicó si sabe leer archivos de Slint, así que no se toma como señal de que esté limpio. Sin superposición visual: la aplicación no es web.

## Impresión general

Funciona y es predecible, pero se lee como una consola con formularios. La mayor oportunidad está en el resultado del cálculo: es el momento que justifica el programa y hoy es el menos diseñado.

## Lo que funciona

1. Respaldo antes de lo masivo y confirmación al restaurar, con un texto que dice qué se pierde y que antes se guarda una copia.
2. Mensajes con icono y texto: el significado no depende solo del color.
3. Estructura estable: lista a la izquierda, edición a la derecha, botones en el mismo orden y siempre con texto.

## Problemas prioritarios

1. **[P1] Movimientos: un solo formulario para 12 tipos, con códigos tecleados.** De 11 campos solo aplican 2 a 4 según el tipo; el resto queda atenuado sin explicar por qué. Ramo, riesgo, secuela y tipo de descuento hay que saberlos de memoria. Arreglo: mostrar solo los campos del tipo elegido; listas desplegables con etiquetas; agrupar los tipos. Comando: `/impeccable distill`.
2. **[P1] Los resultados son texto monoespaciado sin jerarquía.** Cálculo, archivo de pago e importación. El total no destaca, los avisos se confunden con las cifras, el texto no se puede seleccionar ni copiar, y en la ventana mínima las columnas se parten. Arreglo: tarjeta con total a pagar, periodo y vencimiento; tabla real para el detalle; avisos arriba con icono; botón Copiar. Comando: `/impeccable layout`.
3. **[P1] El patrón activo es un estado escondido.** Rige Cálculo, Importar y Exportar, pero solo se elige en una lista de Trabajadores; Movimientos exige haber elegido un trabajador en otra pantalla. Arreglo: selector de patrón y periodo permanente en la cabecera; elegir una fila en Patrones debe activarlo. Comando: `/impeccable shape`. Nota: el «Sin patrón» de las capturas es un efecto de abrir directo en la pantalla; por el menú sí aparece.
4. **[P2] Eliminar sin confirmación.** Un movimiento se borra al primer clic; borrar un patrón sin trabajadores se lleva sus cálculos guardados sin avisar. La aplicación ya impide borrar un patrón con trabajadores o un trabajador con movimientos. Arreglo: confirmación que diga qué se pierde; separar el botón del grupo principal. Comando: `/impeccable harden`.
5. **[P2] Validación tardía y mensajes frágiles.** Los errores aparecen después de Guardar, lejos del campo; el icono del mensaje se decide por el inicio del texto. Arreglo: tipo de mensaje como dato; listas desplegables para sexo, tipo, jornada y entidad; formato de fecha guiado. Comando: `/impeccable harden`.

## Alertas por tipo de usuario

- **Alex (experto):** sin Ctrl+S ni Enter para guardar; N altas son N pasadas por 17 campos; en Cálculo no se distingue qué campos usa «un tramo» y cuáles «todo el patrón»; los días no se proponen a partir del periodo.
- **Sam (teclado y accesibilidad):** campos deshabilitados gris sobre gris; el formato de fecha solo aparece como texto de ejemplo; ayuda crítica en 12 px; los bloques de resultado no se seleccionan. El foco de teclado no se pudo verificar.
- **Jordan (primera vez):** la primera pantalla no dice por dónde empezar; términos sin explicar («Área geográf.», «Deleg. y subd.», «tramo», «Decreto de exención»); «Archivo de pago» e «Importar y exportar» parecen lo mismo.

## Carga cognitiva

Fallan 6 de 8 puntos: carga alta. Peores casos: Trabajadores (17 campos en un bloque), Movimientos (11 campos, 12 tipos) y la zona superior de Cálculo (13 controles).

## Observaciones menores

- Textos truncados en Patrones («Trabajad…») y en el buscador de Trabajadores.
- Los títulos de pantalla no coinciden con el menú.
- Etiquetas de respaldo en UTC y con nombres técnicos.
- «Compactar base» no explica qué hace.
- En 900×560 el formulario de Trabajadores probablemente se corta: no tiene desplazamiento.
- Las tablas vacías no orientan.

## Preguntas para pensar

1. Si lo que se hace cada mes es calcular todo el patrón, ¿por qué hay ocho secciones al mismo nivel en vez de un ciclo (periodo, revisar, calcular, pagar)?
2. Si el resultado se leyera como un documento con total, vencimiento y desglose, ¿haría falta el PDF aparte?
3. ¿Quién impide hoy que un dato mal tecleado llegue al IMSS?
