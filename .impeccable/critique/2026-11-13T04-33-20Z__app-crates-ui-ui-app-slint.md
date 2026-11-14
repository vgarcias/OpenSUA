---
target: todas las pantallas
total_score: 26
max_score: 40
na_heuristics: 
p0_count: 0
p1_count: 3
target_identity: "file:D:\\otros\\sua\\app\\crates\\ui\\ui\\app.slint"
target_fingerprint: "sha256:8e4f0ee2659910e3d47cdd2e55f81a4f439dfb49d5c65479c478272647ed3c13"
target_path: "D:\\otros\\sua\\app\\crates\\ui\\ui\\app.slint"
timestamp: 2026-11-13T04-33-20Z
slug: app-crates-ui-ui-app-slint
---
Method: dual-agent (A: revisión de diseño en subagente aislado, sin acceso a los informes anteriores · B: detector en subagente aislado)

# Crítica de diseño: SUA portable, tercera corrida

Destino: `app/crates/ui/ui/app.slint` (ocho pantallas). Aplicación nativa en Slint; modo Operate; aspecto sobrio y mejor ordenado, sin rediseño.

## Salud del diseño

| # | Heurística | Nota | Problema principal |
|---|---|---|---|
| 1 | Visibilidad del estado | 3 | Sistema, Importar y Archivo de pago responden con texto plano, sin icono |
| 2 | Lenguaje del mundo real | 3 | Jerga suelta: «tramo», «Diario: delete», «Días m / b», «Inc / Aus» |
| 3 | Control y libertad | 2 | Sin Esc ni deshacer; cambiar de patrón o Ctrl+N descarta el formulario sin avisar |
| 4 | Consistencia | 3 | Enter solo actúa en Movimientos y Cálculo |
| 5 | Prevención de errores | 3 | RFC y CURP no se validan al escribir |
| 6 | Reconocer antes que recordar | 3 | Los formatos viven en textos de ejemplo que desaparecen |
| 7 | Flexibilidad y eficiencia | 2 | Hay atajos; no hay ordenar columnas ni Enter en los formularios principales |
| 8 | Estética y minimalismo | 3 | Trabajadores tiene 17 campos seguidos |
| 9 | Recuperación de errores | 3 | Mensajes concretos |
| 10 | Ayuda | 1 | No hay ayuda ni glosario |
| | **Total** | **26/40** | **Aceptable** |

Carga cognitiva: fallan 3 de 8 puntos (moderada; antes 5 y 6).

## Veredicto de especificidad

Se siente hecho para este producto (franja de patrón y trabajador, avisos con icono, total con avisos) y la sobriedad pedida está lograda. Le falta carácter en las tablas y en las salidas de texto en Consolas. Detector: cero hallazgos; no analiza Slint.

## Lo que funciona

1. Eliminar: botón deshabilitado con su motivo; solo se confirma cuando procede.
2. Resultado del cálculo: jerarquía clara, «preliminar» y botón para registrar la prima.
3. Franja de contexto y estados vacíos con acción.

## Problemas prioritarios

1. **[P1] No se ve qué registro se está editando, y el tamaño mínimo aprieta.** Ninguna fila se distingue de las rayas alternas en Patrones y Trabajadores; en 1056×600 el nombre en Patrones queda en «P…». Arreglo: fila activa con fondo propio; ancho mínimo para «Nombre»; formularios por secciones. Comando: `/impeccable layout`.
2. **[P1] Los resultados de operaciones críticas no tienen estado.** Restaurar, respaldar, compactar e importar responden con texto plano; en Sistema ese texto empuja los botones; «Rechazados: N» pesa lo mismo que «Nuevos: N». Arreglo: línea de mensaje con icono y detalle en el cuadro de texto. Comando: `/impeccable harden`.
3. **[P1] Callejón sin salida al eliminar un patrón.** El motivo manda a dar de baja a los trabajadores, pero un trabajador dado de baja sigue contando y con movimientos tampoco se elimina: un patrón con historial nunca se puede borrar. Arreglo: decidir la regla o decir la verdad en el mensaje. Comando: `/impeccable clarify`.
4. **[P2] Teclado inconsistente y atajos que prometen de más.** Enter no guarda en Patrones ni Trabajadores; Ctrl+S se traga la pulsación donde no actúa; el pie anuncia atajos que no aplican en todas las pantallas; los números no aparecen en el menú. Comando: `/impeccable harden`.
5. **[P2] Cálculo: dos cifras para «INFONAVIT» y etiquetas crípticas.** El rubro suma aportación, amortización y seguro; la columna «Aportación» trae solo la patronal. «Días m / b» e «Inc / Aus» no se entienden; las acciones están encima del resultado. Comando: `/impeccable clarify`.

## Alertas por tipo de usuario

- **Alex:** no se ordenan ni filtran las tablas; Enter no guarda en los formularios principales; no hay Esc; fechas sin selector.
- **Sam:** las opciones propias no anuncian cuál está elegida; los mensajes no se anuncian; ocho paradas de tabulador; colores fijos del tema oscuro.
- **Jordan:** textos de ejemplo que parecen datos; «Archivo de pago» e «Importar y exportar» no se distinguen; sin ayuda.

## Observaciones menores

- El título salta de altura en Catálogos y Sistema.
- Catálogos muestra columnas con nombres crudos.
- La lista de respaldos aparece vacía, sin texto guía.
- «Restaurar» y «Determinar prima de RT» deshabilitados no dicen por qué.
- Los errores de Catálogos van al pie.
- Movimientos repite la instrucción de elegir trabajador.
- «Guardar», «Registrar» y «Guardar mes» nombran lo mismo de tres maneras.

## Preguntas para pensar

1. ¿Una barra de pasos para el flujo mensual y un menú «Herramientas» aparte?
2. ¿Y si cada resultado crítico fuera una tabla con estado y el texto solo un detalle copiable?
