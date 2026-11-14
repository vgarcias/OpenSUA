# Especificación del formato de los archivos .SUA de prueba

Archivos de texto plano, **longitud fija**, codificación **ASCII** (solo
mayúsculas), separador de línea **CRLF** (Windows). Cada archivo termina con
CRLF.

## Estructura general

Cada archivo contiene tres tipos de registro, en este orden:

1. **Un registro de encabezado** (tipo `1`) — datos del patrón, periodo y totales.
2. **N registros de detalle** (tipo `2`) — uno por trabajador.
3. **Un registro de trailer** (tipo `3`) — totales de control.

## Convenciones de formato

- **Campos numéricos**: enteros, rellenos con ceros a la izquierda, **sin punto
  decimal**. Los importes llevan 2 decimales implícitos
  (ejemplo: `0001234567` = 12,345.67 pesos).
- **Campos alfanuméricos**: mayúsculas, rellenos con espacios a la derecha.
- **Fechas**: `AAAAMMDD` (8 dígitos).
- **Periodo mensual** (cuotas IMSS): `AAAAMM` (6 dígitos).
- **Bimestre** (cuotas RCV e INFONAVIT): `AAAAB` (7 dígitos), donde `B` es el
  número de bimestre del año (1 a 6). Ejemplo: `202605` = quinto bimestre
  de 2026 (sep-oct).
- **Tipos de documento**: `01` = oportuno, `02` = extemporáneo,
  `03` = diferencias, `05` = responsabilidad solidaria 15-A.

---

## Registro tipo 1 — Encabezado (171 posiciones)

| Pos | Lon | Campo | Formato |
|-----|-----|-------|---------|
| 1 | 1 | Tipo de registro (`1`) | X |
| 2 | 5 | Versión del SUA (`3.7.1`) | X |
| 7 | 11 | Registro patronal IMSS | X |
| 18 | 13 | RFC del patrón | X |
| 31 | 50 | Nombre o razón social | X |
| 81 | 6 | Periodo mensual IMSS (`AAAAMM`) | 9 |
| 87 | 7 | Bimestre RCV (`AAAAB`) | 9 |
| 94 | 2 | Tipo de documento | 9 |
| 96 | 8 | Fecha de generación (`AAAAMMDD`) | 9 |
| 104 | 6 | Folio SUA | 9 |
| 110 | 4 | Número de cotizantes | 9 |
| 114 | 7 | Prima de Riesgos de Trabajo (2 ent + 5 dec, ej. `0024500` = 2.45 %) | 9 |
| 121 | 7 | UMA diaria (5 ent + 2 dec) | 9 |
| 128 | 11 | Total cuotas IMSS (9 ent + 2 dec) | 9 |
| 139 | 11 | Total RCV | 9 |
| 150 | 11 | Total aportación INFONAVIT | 9 |
| 161 | 11 | Total general | 9 |

## Registro tipo 2 — Detalle / trabajador (298 posiciones)

| Pos | Lon | Campo | Formato |
|-----|-----|-------|---------|
| 1 | 1 | Tipo de registro (`2`) | X |
| 2 | 11 | Registro patronal IMSS | X |
| 13 | 11 | Número de Seguridad Social (NSS) | 9 |
| 24 | 50 | Nombre del trabajador | X |
| 74 | 13 | RFC del trabajador | X |
| 87 | 18 | CURP | X |
| 105 | 8 | Fecha de alta (`AAAAMMDD`) | 9 |
| 113 | 7 | Salario Base de Cotización — SBC (5 ent + 2 dec) | 9 |
| 120 | 3 | Días cotizados en el mes | 9 |
| 123 | 3 | Días cotizados en el bimestre | 9 |
| 126 | 9 | Cuota fija EYM (patronal) | 9 |
| 135 | 9 | EYM prestaciones en dinero patronal | 9 |
| 144 | 9 | EYM prestaciones en dinero obrera | 9 |
| 153 | 9 | EYM gastos médicos pensionados (patronal) | 9 |
| 162 | 9 | EYM cuota excedente patronal | 9 |
| 171 | 9 | EYM cuota excedente obrera | 9 |
| 180 | 9 | Riesgos de Trabajo (patronal) | 9 |
| 189 | 9 | Invalidez y Vida patronal | 9 |
| 198 | 9 | Invalidez y Vida obrera | 9 |
| 207 | 9 | Guarderías y Prestaciones Sociales (patronal) | 9 |
| 216 | 9 | Subtotal IMSS patronal | 9 |
| 225 | 9 | Subtotal IMSS obrero | 9 |
| 234 | 9 | Total cuotas IMSS del trabajador | 9 |
| 243 | 9 | Retiro 2 % (patronal, bimestral) | 9 |
| 252 | 9 | Cesantía y Vejez patronal (bimestral) | 9 |
| 261 | 9 | Cesantía y Vejez obrera (bimestral) | 9 |
| 270 | 9 | Subtotal RCV | 9 |
| 279 | 9 | Aportación INFONAVIT 5 % (bimestral) | 9 |
| 288 | 11 | Total del trabajador (IMSS + RCV + INFONAVIT) | 9 |

## Registro tipo 3 — Trailer (86 posiciones)

| Pos | Lon | Campo | Formato |
|-----|-----|-------|---------|
| 1 | 1 | Tipo de registro (`3`) | X |
| 2 | 11 | Registro patronal IMSS | X |
| 13 | 5 | Número de registros de detalle | 9 |
| 18 | 5 | Total de cotizantes | 9 |
| 23 | 7 | Total de días cotizados en el mes | 9 |
| 30 | 13 | Suma de SBC × días del mes (11 ent + 2 dec) | 9 |
| 43 | 11 | Total cuotas IMSS | 9 |
| 54 | 11 | Total RCV | 9 |
| 65 | 11 | Total aportación INFONAVIT | 9 |
| 76 | 11 | Total general | 9 |

---

## Reglas de cálculo aplicadas (LSS 1997, valores 2026)

Parámetros: UMA diaria **117.31**; tope de SBC para EYM, IV y Guarderías =
**25 UMA = 2,932.75**; salario mínimo general **315.04**.

Cuotas IMSS (mensuales; base = SBC × días del mes):

| Concepto | Tasa | Base | Tope |
|----------|------|------|------|
| Cuota fija EYM | 20.40 % de la UMA diaria | por día cotizado | — |
| EYM prestaciones en dinero | 0.25 % pat + 0.25 % obr | SBC | 25 UMA |
| EYM gastos médicos pensionados | 1.05 % pat | SBC | 25 UMA |
| EYM excedente | 0.40 % pat + 0.40 % obr | SBC − 3 UMA | 25 UMA |
| Riesgos de Trabajo | prima del patrón | SBC | sin tope |
| Invalidez y Vida | 1.75 % pat + 0.625 % obr | SBC | 25 UMA |
| Guarderías y P. Sociales | 1.00 % pat | SBC | 25 UMA |

Cuotas RCV e INFONAVIT (bimestrales; base = SBC × días del bimestre):

| Concepto | Tasa | Base | Tope |
|----------|------|------|------|
| Retiro | 2.00 % pat | SBC | sin tope |
| Cesantía y Vejez | 3.15 % pat + 1.125 % obr | SBC | 25 UMA |
| Aportación INFONAVIT | 5.00 % pat | SBC | sin tope |

Cada cuota se redondea a 2 decimales (half-up); los subtotales son la suma de
los importes ya redondeados. Los archivos del **primer mes del bimestre**
llevan RCV e INFONAVIT en ceros (el patrón los paga en el mes de cierre del
bimestre), como se hace comúnmente con el SUA.

## Nota importante

El formato interno real del archivo de pago que genera el SUA es propietario y
el IMSS no publica su especificación. Este layout es un diseño de referencia
documentado, alineado con los conceptos y convenciones oficiales del SUA
(campos numéricos sin punto con decimales implícitas, primas RT con 2 enteros y
5 decimales, periodos mensuales para IMSS y bimestrales para RCV/INFONAVIT),
pensado para probar la lógica de lectura, validación y totalización de un
importador.
