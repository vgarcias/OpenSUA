# Archivos de oro

Cinco archivos de pago `.SUA` reales (un patrón, periodos de 2006 a 2008), **anonimizados**. Los usa `tests/oro.rs` para comprobar que el motor reproduce los importes que calculó el SUA oficial.

Se generaron con:

```
sua-cli anonimizar-sua <original.SUA> <copia.SUA>
```

El anonimizador (`src/io/anonimo.rs`) sustituye registro patronal, RFC, razón social, domicilio, código postal, teléfono y actividad del patrón; NSS, RFC, CURP, nombre y crédito de cada trabajador; folios de incapacidad; y el número de control del registro 06 con el bloque numérico que le sigue. Conserva periodo, entidad, delegación y subdelegación, área geográfica, folio SUA, fecha de pago, prima de riesgos de trabajo, salarios, días, fechas de los movimientos e importes.

Comprobado al generarlos (13-nov-2026):

- `sua-cli leer-sua` da exactamente la misma salida con cada original y con su copia.
- Ninguno de los 167 identificadores de los originales (registro patronal, RFC, NSS, CURP, palabras de nombres, razón social y domicilio, código postal, teléfono, números de control) aparece en las copias.

Los originales no están en el repositorio. Para añadir un caso: anonimizarlo, copiarlo aquí y agregar su renglón a la tabla de `tests/oro.rs`.
