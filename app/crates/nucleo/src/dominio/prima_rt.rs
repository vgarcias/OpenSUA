//! Determinación anual de la prima del seguro de riesgos de trabajo.
//!
//! Fórmula impresa en las cédulas reales (`ESPECIFICACION_SUA.md`, §6.3;
//! Ley del Seguro Social, artículo 72):
//!
//! `Prima = [(S / 365) + V × (I + D)] × (F / N) + M`
//!
//! - `S`: días subsidiados por incapacidad temporal;
//! - `I`: suma de los porcentajes de incapacidad permanente, entre 100;
//! - `D`: defunciones;
//! - `N`: trabajadores promedio expuestos al riesgo (días cotizados del año entre 365);
//! - `V` = 28 años, `M` = 0.005 (prima mínima de riesgo) y `F` = factor de prima del año.
//!
//! El resultado, por 100, no puede variar más de un punto respecto de la
//! prima anterior ni salir de la prima mínima y máxima del año. Rige del 1 de
//! marzo del año siguiente al último día de febrero del posterior.
//!
//! **Sin comprobar con un caso real:** el redondeo (aquí, a cinco decimales
//! de punto porcentual, medio hacia arriba) y el cálculo exacto de `N`. No se
//! aplican las reglas para empresas de menos de diez trabajadores ni para
//! las que cambian de actividad.

pub const V: f64 = 28.0;
pub const M: f64 = 0.005;

/// Datos de la siniestralidad de un año. Las primas van en cienmilésimas de
/// punto porcentual (7.58875 % = 758875).
#[derive(Debug, Clone, PartialEq)]
pub struct Siniestralidad {
    pub anio: u32,
    /// Días subsidiados (S).
    pub dias_subsidiados: i64,
    /// Suma de porcentajes de incapacidad permanente, en puntos (I × 100).
    pub porcentajes: i64,
    /// Defunciones (D).
    pub defunciones: i64,
    /// Días cotizados del año por todos los trabajadores.
    pub dias_cotizados: i64,
    /// Factor de prima (F).
    pub factor: f64,
    pub prima_anterior: Option<i64>,
    pub prima_minima: i64,
    pub prima_maxima: i64,
    /// Casos de riesgo de trabajo considerados.
    pub casos: usize,
    /// Incapacidades del año sin datos médicos capturados: no se pudieron clasificar.
    pub sin_clasificar: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Determinacion {
    /// Trabajadores promedio expuestos al riesgo (N).
    pub trabajadores_promedio: f64,
    /// Prima que da la fórmula, antes de los límites.
    pub calculada: i64,
    /// Prima que resulta tras aplicar los límites. Si no hay prima anterior
    /// con la cual limitar, rige la calculada, dentro del mínimo y el máximo.
    pub nueva: i64,
    /// Qué límite se aplicó, si alguno.
    pub limite: Option<&'static str>,
}

impl Siniestralidad {
    /// Aplica la fórmula y los límites. `None` si no hubo días cotizados en el año.
    pub fn determinar(&self) -> Option<Determinacion> {
        if self.dias_cotizados <= 0 {
            return None;
        }
        let n = self.dias_cotizados as f64 / 365.0;
        let (s, i, d) = (self.dias_subsidiados as f64, self.porcentajes as f64 / 100.0, self.defunciones as f64);
        let prima = ((s / 365.0) + V * (i + d)) * (self.factor / n) + M;
        // De fracción a cienmilésimas de punto porcentual: × 100 × 100000.
        let calculada = (prima * 10_000_000.0 + 0.5).floor() as i64;
        let (mut nueva, mut limite) = (calculada, None);
        if let Some(anterior) = self.prima_anterior {
            if nueva > anterior + 100_000 {
                (nueva, limite) = (anterior + 100_000, Some("no puede subir más de un punto respecto de la prima anterior"));
            } else if nueva < anterior - 100_000 {
                (nueva, limite) = (anterior - 100_000, Some("no puede bajar más de un punto respecto de la prima anterior"));
            }
        }
        if nueva < self.prima_minima {
            (nueva, limite) = (self.prima_minima, Some("no puede ser menor que la prima mínima"));
        } else if nueva > self.prima_maxima {
            (nueva, limite) = (self.prima_maxima, Some("no puede ser mayor que la prima máxima"));
        }
        Some(Determinacion { trabajadores_promedio: n, calculada, nueva, limite })
    }
}

/// `758875` → `7.58875`.
pub fn prima_en_texto(cienmilesimas: i64) -> String {
    format!("{}.{:05}", cienmilesimas / 100_000, cienmilesimas % 100_000)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn datos() -> Siniestralidad {
        Siniestralidad {
            anio: 2026,
            dias_subsidiados: 0,
            porcentajes: 0,
            defunciones: 0,
            dias_cotizados: 36_500, // 100 trabajadores promedio
            factor: 2.3,
            prima_anterior: Some(758_875),
            prima_minima: 50_000,
            prima_maxima: 1_500_000,
            casos: 0,
            sin_clasificar: 0,
        }
    }

    #[test]
    fn sin_siniestros_la_formula_da_la_prima_minima_y_solo_baja_un_punto() {
        let r = datos().determinar().unwrap();
        assert_eq!((r.trabajadores_promedio, r.calculada), (100.0, 50_000));
        // De 7.58875 solo puede bajar a 6.58875.
        assert_eq!((r.nueva, r.limite.is_some()), (658_875, true));
        // Sin prima anterior rige la calculada.
        let r = Siniestralidad { prima_anterior: None, ..datos() }.determinar().unwrap();
        assert_eq!((r.nueva, r.limite), (50_000, None));
    }

    #[test]
    fn formula_con_siniestros() {
        // S = 73 días, I = 0.15, D = 0, N = 100, F = 2.3:
        // [(73 / 365) + 28 × 0.15] × (2.3 / 100) + 0.005 = (0.2 + 4.2) × 0.023 + 0.005 = 0.1062 → 10.62000 %
        let s = Siniestralidad { dias_subsidiados: 73, porcentajes: 15, prima_anterior: Some(1_000_000), ..datos() };
        let r = s.determinar().unwrap();
        assert_eq!((r.calculada, r.nueva, r.limite), (1_062_000, 1_062_000, None));
        // Con la prima anterior en 7.58875 solo puede subir a 8.58875.
        let r = Siniestralidad { prima_anterior: Some(758_875), ..s.clone() }.determinar().unwrap();
        assert_eq!(r.nueva, 858_875);
        // Una defunción: (0 + 28 × 1) × 0.023 + 0.005 = 0.649 → 64.9 %, topada a la máxima de 15 %.
        let r = Siniestralidad { defunciones: 1, prima_anterior: None, ..datos() }.determinar().unwrap();
        assert_eq!((r.calculada, r.nueva), (6_490_000, 1_500_000));
        assert!(r.limite.unwrap().contains("máxima"));
        // Sin días cotizados no hay determinación.
        assert_eq!(Siniestralidad { dias_cotizados: 0, ..datos() }.determinar(), None);
        assert_eq!(prima_en_texto(758_875), "7.58875");
    }
}
