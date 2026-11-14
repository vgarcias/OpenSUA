-- Pago extemporáneo de un cálculo guardado: fecha de pago, vencimiento, meses
-- de mora, factor, tasa e importes de actualización y recargos, separados por
-- «|» (dominio/extemporaneo.rs). Vacío si el cálculo es de pago oportuno.
ALTER TABLE calculo ADD COLUMN extemporaneo TEXT NOT NULL DEFAULT '';
