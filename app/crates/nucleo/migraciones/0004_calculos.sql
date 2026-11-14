-- Cálculos guardados: uno por patrón y periodo. Guardar de nuevo el mismo
-- periodo sustituye al anterior. Importes en centavos; los totales no se
-- guardan, son la suma de los renglones.

CREATE TABLE calculo (
    id                  INTEGER PRIMARY KEY,
    reg_pat             TEXT NOT NULL REFERENCES patron(reg_pat) ON UPDATE CASCADE ON DELETE CASCADE,
    periodo             TEXT NOT NULL CHECK (length(periodo) = 6),
    cierra_bimestre     INTEGER NOT NULL CHECK (cierra_bimestre IN (0, 1)),
    decreto             INTEGER NOT NULL DEFAULT 0 CHECK (decreto BETWEEN 0 AND 100),
    -- Prima de riesgos de trabajo aplicada; -1 si el patrón no tenía prima vigente.
    prima_cienmilesimas INTEGER NOT NULL DEFAULT -1,
    -- Fecha y hora UTC, AAAA-MM-DD HH:MM:SS.
    guardado_en         TEXT NOT NULL,
    -- Avisos del cálculo, uno por renglón.
    avisos              TEXT NOT NULL DEFAULT '',
    UNIQUE (reg_pat, periodo)
) STRICT;

-- El trabajador se guarda por NSS y nombre, sin llave foránea: el cálculo
-- guardado es una fotografía y no cambia si después se modifica la captura.
CREATE TABLE calculo_linea (
    calculo_id         INTEGER NOT NULL REFERENCES calculo(id) ON DELETE CASCADE,
    orden              INTEGER NOT NULL,
    num_afil           TEXT NOT NULL,
    nombre             TEXT NOT NULL,
    sdi                INTEGER NOT NULL DEFAULT 0,
    dias_mes           INTEGER NOT NULL DEFAULT 0,
    dias_bimestre      INTEGER NOT NULL DEFAULT 0,
    cuota_fija         INTEGER NOT NULL DEFAULT 0,
    excedente_patronal INTEGER NOT NULL DEFAULT 0,
    excedente_obrero   INTEGER NOT NULL DEFAULT 0,
    pd_patronal        INTEGER NOT NULL DEFAULT 0,
    pd_obrero          INTEGER NOT NULL DEFAULT 0,
    gmp_patronal       INTEGER NOT NULL DEFAULT 0,
    gmp_obrero         INTEGER NOT NULL DEFAULT 0,
    riesgos_trabajo    INTEGER NOT NULL DEFAULT 0,
    iv_patronal        INTEGER NOT NULL DEFAULT 0,
    iv_obrero          INTEGER NOT NULL DEFAULT 0,
    guarderias         INTEGER NOT NULL DEFAULT 0,
    retiro             INTEGER NOT NULL DEFAULT 0,
    cyv_patronal       INTEGER NOT NULL DEFAULT 0,
    cyv_obrero         INTEGER NOT NULL DEFAULT 0,
    infonavit          INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (calculo_id, orden)
) STRICT;
