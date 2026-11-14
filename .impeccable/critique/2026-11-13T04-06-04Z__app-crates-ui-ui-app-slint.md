---
target: todas las pantallas
total_score: 26
max_score: 40
na_heuristics: 
p0_count: 0
p1_count: 3
target_identity: "file:D:\\otros\\sua\\app\\crates\\ui\\ui\\app.slint"
target_fingerprint: "sha256:e7886333eb75343326f911eb913fff0e86b2d272e3a57f859a772ae2f40ceb9e"
target_path: "D:\\otros\\sua\\app\\crates\\ui\\ui\\app.slint"
timestamp: 2026-11-13T04-06-04Z
slug: app-crates-ui-ui-app-slint
---
Method: dual-agent (A: revisión de diseño en subagente aislado, sin acceso al informe anterior · B: detector en subagente aislado)

# Crítica de diseño: SUA portable, segunda corrida

Destino: `app/crates/ui/ui/app.slint` (ocho pantallas). Aplicación nativa de escritorio en Slint; modo Operate; el dueño pidió un aspecto sobrio y mejor ordenado.

## Salud del diseño

| # | Heurística | Nota | Problema principal |
|---|---|---|---|
| 1 | Visibilidad del estado | 3 | Los resultados de Archivo de pago, Importar, Sistema y los errores de Cálculo caen en texto sin icono |
| 2 | Lenguaje del mundo real | 3 | Fechas AAAA-MM-DD; nombres crudos en Catálogos y Sistema |
| 3 | Control y libertad | 2 | Sin deshacer; «Guardar cálculo» sustituye sin avisar; los borrados confirman y luego se niegan |
| 4 | Consistencia | 3 | El mismo azul marca la pantalla activa y la acción principal |
| 5 | Prevención de errores | 3 | No se marcan los campos obligatorios; RFC y CURP no se validan al escribir |
| 6 | Reconocer antes que recordar | 3 | El trabajador de Movimientos se elige en otra pantalla |
| 7 | Flexibilidad y eficiencia | 1 | Sin atajos, sin Enter para guardar, sin acciones en lote |
| 8 | Estética y minimalismo | 3 | Trabajadores (18 campos seguidos) y Cálculo siguen saturados |
| 9 | Recuperación de errores | 3 | Los errores de Cálculo salen como texto plano; el aviso de «prima en cero» no lleva a corregirla |
| 10 | Ayuda | 2 | Solo notas en línea |
| | **Total** | **26/40** | **Aceptable** |

## Veredicto de especificidad

El contenido ya es propio del producto (desglose por ramo, avisos legales, notas de efecto fiscal, respaldos legibles). El lenguaje visual sigue siendo genérico y la estructura está organizada por entidades, no por el ciclo mensual; el periodo y su vencimiento no son un objeto de primer nivel.

Detector: cero hallazgos; su ayuda no lista Slint entre los tipos que analiza, así que no es señal de que esté limpio.

## Lo que funciona

1. Resultado del cálculo: total, rubros, periodo y vencimiento, avisos con icono y tres vistas.
2. Movimientos: solo los campos del tipo elegido, con notas de consecuencia.
3. Red de seguridad y contexto: respaldo antes de importar, respaldos legibles, franja de patrón activo.

## Problemas prioritarios

1. **[P1] Los borrados confirman y luego se niegan, y prometen un respaldo que no existe.** El diálogo se muestra antes de comprobar si el patrón tiene trabajadores o el trabajador movimientos; la frase «puede recuperarlo de un respaldo» es falsa porque al eliminar no se hace ninguno. Arreglo: comprobar antes de preguntar; si no se puede, deshabilitar el botón con la razón; si se puede, respaldar de verdad y decirlo. Comando: `/impeccable harden`.
2. **[P1] Cálculo mezcla dos tareas y la principal no manda.** El botón destacado es «Calcular un tramo»; salario, días y prima siguen a la vista aunque el cálculo del patrón no los usa; los errores salen como texto plano. Arreglo: dos modos con sus campos, «Todo el patrón» como principal; errores por el canal de mensajes; el aviso de la prima lleva a Patrones. Comando: `/impeccable layout`.
3. **[P1] Guardar y su mensaje quedan bajo el pliegue.** En Trabajadores están al final de un formulario que se desplaza; en la ventana por defecto (1180×740) quedan fuera de vista. Cálculo no tiene desplazamiento; las cifras usan recorte con puntos suspensivos. Arreglo: anclar la barra de acciones y el mensaje fuera del desplazamiento; desplazamiento en Cálculo; quitar el recorte de las cifras. Comando: `/impeccable adapt`.
4. **[P2] El contexto se reparte y el azul pierde significado.** Movimientos no tiene selector de trabajador; el azul marca pantalla activa, familia elegida y acción principal. Comando: `/impeccable layout`.
5. **[P2] Solo para ratón.** Sin atajos, sin Enter para guardar, sin etiquetas accesibles. Comando: `/impeccable harden`.

## Alertas por tipo de usuario

- **Alex:** Enter no guarda; veinte altas son tres pantallas por trabajador; el periodo arranca vacío.
- **Sam:** ocho paradas de tabulador antes del contenido; campos sin nombre accesible; colores de aviso que presuponen tema oscuro; la ruta de la base no se puede copiar.
- **Jordan:** sin camino sugerido; dos vías de importación sin explicar; siglas sin explicar; el INPC mensual escondido en Catálogos.

## Carga cognitiva

Fallan 5 de 8 puntos: alta. Peores casos: Trabajadores (18 campos) y Cálculo (13 controles).

## Observaciones menores

- Los textos de tabla vacía quedan pegados al fondo de la columna.
- La franja «Patrón activo» aparece en Catálogos y Sistema, donde no aplica.
- En Sistema la lista de respaldos viene con uno elegido y «Restaurar» habilitado.
- La casilla del decreto está en la fila de botones.
- Títulos de pantalla desalineados 10 px.
- El orden del menú no sigue el flujo de trabajo.
- La exportación escribe en una carpeta fija sin ofrecer abrirla.

## Preguntas para pensar

1. ¿Y si la columna vertebral fuera el periodo?
2. ¿Tiene sentido que Movimientos sea una pantalla aparte?
3. ¿Qué haría falta para confiar en «Total a pagar»?
