# Conjunto de datos de prueba — Archivos .SUA ficticios

Datos ficticios para validar y probar un importador de archivos SUA
(Sistema Único de Autodeterminación del IMSS). Incluye registros
patronales, trabajadores con salarios, días cotizados e importes mensuales
(IMSS) y bimestrales (RCV e INFONAVIT) calculados con las tasas de la LSS
1997 y los valores 2026.

> Todos los registros patronales, NSS, RFC, CURP, nombres e importes son
> ficticios. Cualquier coincidencia con datos reales es accidental.

## Contenido

| Archivo | Descripción |
|---------|-------------|
| `*.SUA` (7 archivos) | Archivos de pago ficticios, formato longitud fija |
| `ERRORES_202609.SUA` | Archivo con defectos intencionales (pruebas negativas) |
| `LAYOUT_SUA.md` | Especificación completa del formato (posiciones y campos) |
| `esperados_detalle.csv` | Valores esperados por trabajador (para aserciones) |
| `esperados_totales.csv` | Totales esperados por archivo |
| `generar_sua.py` | Generador (Python 3, sin dependencias) |
| `generar_errores.py` | Generador del archivo con defectos |
| `valida_sua.py` | Validador/parser de referencia |

## Inventario de archivos .SUA

| Archivo | Registro patronal | Periodo IMSS | Bimestre RCV | Trab. | Escenario |
|---------|-------------------|--------------|--------------|-------|-----------|
| `ALPHA_202609.SUA` | A4623891107 | sep-2026 | 2026-05 | 10 | Solo IMSS (mes 1 del bimestre) |
| `ALPHA_202610.SUA` | A4623891107 | oct-2026 | 2026-05 | 10 | IMSS + RCV (cierre de bimestre) |
| `BETA_202610.SUA` | B7319052208 | oct-2026 | 2026-05 | 8 | IMSS + RCV, prima RT baja |
| `GAMMA_202611.SUA` | C1104578903 | nov-2026 | 2026-06 | 9 | Solo IMSS |
| `GAMMA_202612.SUA` | C1104578903 | dic-2026 | 2026-06 | 9 | IMSS + RCV, 31 días |
| `DELTA_202611.SUA` | D5682031401 | nov-2026 | 2026-06 | 6 | Solo IMSS, empresa pequeña |
| `EPSILON_202611.SUA` | E9043126705 | nov-2026 | 2026-06 | 1 | Pago extemporáneo (tipo doc 02), un solo trabajador |

## Escenarios de prueba cubiertos

**Salarios y topes:**
- Trabajador con SBC igual al salario mínimo (315.04).
- Trabajador con SBC exactamente en el tope de 25 UMA (2,932.75).
- Trabajador por encima del tope (SBC 4,500): topes aplicados a EYM, IV,
  Guarderías y CV; Retiro e INFONAVIT sin tope.
- Trabajadores sin cuota excedente (SBC ≤ 3 UMA = 351.93).
- Rango salarial amplio (de 315.04 a 4,500.00).

**Días cotizados:**
- Meses de 29, 30 y 31 días.
- Alta a mitad de mes (15 días) y días bimestrales no uniformes (59, 60, 61).
- Días bimestrales que no coinciden con mes × 2.

**Periodos:**
- Cuotas IMSS mensuales y RCV/INFONAVIT bimestrales en el mismo archivo.
- Archivos del primer mes del bimestre con RCV e INFONAVIT en ceros
  (el importador no debe duplicar el pago del bimestre al sumar los dos meses).
- Tipos de documento 01 (oportuno) y 02 (extemporáneo).

**Registros patronales:**
- Cinco registros patronales distintos, cada uno con su prima de RT
  (0.825 %, 1.10 %, 1.675 %, 2.45 %, 3.05 %) — codificada con 2 enteros
  y 5 decimales, como en el SUA.
- Archivos consecutivos del mismo registro patronal (ALPHA y GAMMA).

**Volúmenes:**
- Desde 1 trabajador (mínimo) hasta 10 por archivo, 43 trabajadores en total.

## Pruebas negativas: `ERRORES_202609.SUA`

Defectos intencionales (el importador debe rechazar el archivo):

1. Total de cuotas IMSS del encabezado alterado (+100.00).
2. NSS con carácter alfabético en el detalle #4 (`11058423A45`).
3. Registro de detalle #7 truncado a 250 caracteres.
4. Tipo de registro inválido (`9`) en el detalle #9.
5. NSS duplicado (detalle #11 repite el #2).
6. Trailer con número de registros alterado (`99`).

## Uso del validador

```bash
python3 valida_sua.py                 # valida todos los *.SUA (falla con ERRORES_*)
python3 valida_sua.py --csv           # además compara contra los CSV esperados
python3 valida_sua.py GAMMA_202612.SUA # valida un archivo específico
```

El validador verifica: longitudes y tipos de registro, campos numéricos,
NSS duplicados, recálculo completo de cuotas por trabajador, y consistencia
de totales encabezado/detalles/trailer. Sirve además como parser de
referencia (registro por registro, sin dependencias externas).

## Regenerar los datos

```bash
python3 generar_sua.py      # genera los 7 archivos válidos y los CSV esperados
python3 generar_errores.py   # genera ERRORES_202609.SUA (requiere ALPHA_202609.SUA)
```

## Parámetros usados (2026)

- UMA diaria: 117.31 (tope SBC 25 UMA = 2,932.75) — [INEGI](https://www.inegi.org.mx/temas/uma/)
- Salario mínimo general: 315.04 — [CONASAMI](https://www.gob.mx/conasami)
- Tasas de la LSS 1997: ver `LAYOUT_SUA.md`

## Nota sobre el formato real

El archivo de pago que genera el SUA es un formato propietario cuyo layout
no está publicado oficialmente por el IMSS. Este conjunto usa un formato de
referencia documentado (`LAYOUT_SUA.md`), alineado con las convenciciones
oficiales del SUA (longitud fija, importes sin punto con decimales
implícitas, prima RT con 2 enteros y 5 decimales, periodo mensual para
IMSS y bimestral para RCV/INFONAVIT). Si tu importador lee el formato real
de disco de pago, usa estos archivos para la lógica de validación y
totalización, y ajusta las posiciones según tu layout interno.
