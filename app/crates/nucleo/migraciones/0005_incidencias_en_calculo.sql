-- Días de ausentismo y de incapacidad de cada renglón de un cálculo guardado.
ALTER TABLE calculo_linea ADD COLUMN ausentismos_mes      INTEGER NOT NULL DEFAULT 0;
ALTER TABLE calculo_linea ADD COLUMN incapacidad_mes      INTEGER NOT NULL DEFAULT 0;
ALTER TABLE calculo_linea ADD COLUMN ausentismos_bimestre INTEGER NOT NULL DEFAULT 0;
ALTER TABLE calculo_linea ADD COLUMN incapacidad_bimestre INTEGER NOT NULL DEFAULT 0;
