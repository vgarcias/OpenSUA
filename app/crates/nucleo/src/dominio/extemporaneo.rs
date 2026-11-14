//! Pago extemporáneo: actualización y recargos.
//!
//! Regla general del Código Fiscal de la Federación, que es la que cita el
//! manual del SUA (artículos 17‑A y 21):
//!
//! - **Vencimiento:** el día 17 del mes siguiente al periodo; si cae en
//!   sábado, domingo o día inhábil, el siguiente día hábil.
//! - **Factor de actualización:** INPC del mes anterior al del pago entre INPC
//!   del mes anterior al del vencimiento (que es el mes del periodo), hasta el
//!   diezmilésimo, y nunca menor que 1.
//! - **Recargos:** sobre el importe ya actualizado, la suma de las tasas
//!   mensuales por cada mes o fracción transcurrido desde el vencimiento.
//!
//! **Comprobación:** en el único archivo real pagado con recargos (un mes,
//! 1.13 %, dentro del mes del vencimiento, así que sin actualización) los de
//! retiro coinciden trabajador por trabajador con `importe × 1.13 / 100`, y
//! los de cesantía y vejez en 7 de 8 calculando por separado la parte patronal
//! y la obrera (`Decompilacion_SUA.md`, sección 15.4). Por eso aquí se calcula
//! por trabajador y por separado patronal y obrera. Lo demás —actualización,
//! varios meses de mora, días inhábiles— sale de la regla, sin caso real.
//!
//! No se calculan multas ni el límite de cinco años de recargos.

use super::tramos::{dia, Dia};

/// Día del mes en que vencen las cuotas.
pub const DIA_DE_VENCIMIENTO: i64 = 17;

/// Datos e importes de un pago extemporáneo. Importes en centavos.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Extemporaneo {
    /// `AAAA-MM-DD`
    pub fecha_pago: String,
    /// `AAAA-MM-DD`
    pub vencimiento: String,
    /// Meses o fracción de mora.
    pub meses: u32,
    /// Factor de actualización en diezmilésimas (10000 = 1.0000).
    pub factor: i64,
    /// Tasa de recargos acumulada, en centésimas de punto porcentual (113 = 1.13 %).
    pub tasa: i64,
    pub actualizacion_imss: i64,
    pub recargos_imss: i64,
    pub actualizacion_rcv: i64,
    pub recargos_rcv: i64,
    pub actualizacion_infonavit: i64,
    pub recargos_infonavit: i64,
}

impl Extemporaneo {
    pub fn total(&self) -> i64 {
        self.actualizacion_imss + self.recargos_imss + self.actualizacion_rcv + self.recargos_rcv + self.actualizacion_infonavit + self.recargos_infonavit
    }

    /// (actualización, recargos) de un importe en centavos. Redondeo al
    /// centavo, medio hacia arriba; los recargos van sobre el importe actualizado.
    pub fn cargos(&self, importe: i64) -> (i64, i64) {
        let actualizacion = (importe * (self.factor - 10_000) + 5_000) / 10_000;
        let recargos = ((importe + actualizacion) * self.tasa + 5_000) / 10_000;
        (actualizacion, recargos)
    }

    /// Forma en que se guarda con el cálculo: campos separados por `|`.
    pub fn a_texto(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            self.fecha_pago,
            self.vencimiento,
            self.meses,
            self.factor,
            self.tasa,
            self.actualizacion_imss,
            self.recargos_imss,
            self.actualizacion_rcv,
            self.recargos_rcv,
            self.actualizacion_infonavit,
            self.recargos_infonavit
        )
    }

    pub fn de_texto(texto: &str) -> Option<Extemporaneo> {
        let p: Vec<&str> = texto.split('|').collect();
        if p.len() != 11 {
            return None;
        }
        let n = |i: usize| p[i].parse::<i64>().ok();
        Some(Extemporaneo {
            fecha_pago: p[0].to_string(),
            vencimiento: p[1].to_string(),
            meses: p[2].parse().ok()?,
            factor: n(3)?,
            tasa: n(4)?,
            actualizacion_imss: n(5)?,
            recargos_imss: n(6)?,
            actualizacion_rcv: n(7)?,
            recargos_rcv: n(8)?,
            actualizacion_infonavit: n(9)?,
            recargos_infonavit: n(10)?,
        })
    }
}

/// (año, mes) del mes siguiente.
pub fn mes_siguiente(anio: i64, mes: i64) -> (i64, i64) {
    if mes == 12 {
        (anio + 1, 1)
    } else {
        (anio, mes + 1)
    }
}

/// (año, mes) del mes anterior.
pub fn mes_anterior(anio: i64, mes: i64) -> (i64, i64) {
    if mes == 1 {
        (anio - 1, 12)
    } else {
        (anio, mes - 1)
    }
}

/// Día → (año, mes, día). Algoritmo de fecha civil de Howard Hinnant.
pub fn fecha_de_dia(d: Dia) -> (i64, i64, i64) {
    let z = d + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dia_del_mes = doy - (153 * mp + 2) / 5 + 1;
    let mes = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(mes <= 2), mes, dia_del_mes)
}

pub fn iso_de_dia(d: Dia) -> String {
    let (a, m, dd) = fecha_de_dia(d);
    format!("{a:04}-{m:02}-{dd:02}")
}

/// Día de la semana: 0 domingo … 6 sábado.
pub fn dia_de_la_semana(d: Dia) -> i64 {
    (d + 4).rem_euclid(7) // el 1 de enero de 1970 fue jueves
}

/// Fecha límite de pago de un periodo: el 17 del mes siguiente, recorrido al
/// siguiente día hábil. `inhabiles`: (día, mes) de los días festivos fijos.
pub fn vencimiento(anio: i64, mes: i64, inhabiles: &[(i64, i64)]) -> Dia {
    let (a, m) = mes_siguiente(anio, mes);
    let mut d = dia(a, m, DIA_DE_VENCIMIENTO);
    loop {
        let (_, mes_del_dia, dia_del_mes) = fecha_de_dia(d);
        let festivo = inhabiles.contains(&(dia_del_mes, mes_del_dia));
        if !matches!(dia_de_la_semana(d), 0 | 6) && !festivo {
            return d;
        }
        d += 1;
    }
}

/// Meses o fracción transcurridos desde el vencimiento de un periodo hasta
/// la fecha de pago, contados de 17 a 17. Supone que el pago ya es
/// extemporáneo; devuelve al menos 1.
pub fn meses_de_mora(periodo: (i64, i64), pago: (i64, i64, i64)) -> u32 {
    let (a, m) = mes_siguiente(periodo.0, periodo.1);
    let completos = (pago.0 * 12 + pago.1) - (a * 12 + m);
    let n = completos + i64::from(pago.2 > DIA_DE_VENCIMIENTO);
    n.max(1) as u32
}

/// Factor de actualización en diezmilésimas: cociente truncado al
/// diezmilésimo, nunca menor que 1.
pub fn factor_de_actualizacion(inpc_reciente: f64, inpc_antiguo: f64) -> i64 {
    if inpc_antiguo <= 0.0 {
        return 10_000;
    }
    // Un épsilon evita que 1.0450 salga 1.0449 por la representación binaria.
    (((inpc_reciente / inpc_antiguo) * 10_000.0 + 1e-6).floor() as i64).max(10_000)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn fechas() {
        for iso in ["1970-01-01", "2008-02-29", "2026-11-12", "1999-12-31"] {
            assert_eq!(iso_de_dia(crate::dominio::tramos::dia_de_iso(iso).unwrap()), iso);
        }
        // El 12 de noviembre de 2026 es jueves; el 17 de enero de 2026, sábado.
        assert_eq!(dia_de_la_semana(dia(2026, 11, 12)), 4);
        assert_eq!(dia_de_la_semana(dia(2026, 1, 17)), 6);
        assert_eq!((mes_siguiente(2025, 12), mes_anterior(2026, 1), mes_siguiente(2026, 3)), ((2026, 1), (2025, 12), (2026, 4)));
    }

    #[test]
    fn vencimiento_el_17_o_el_siguiente_dia_habil() {
        // Octubre de 2026 vence el martes 17 de noviembre.
        assert_eq!(iso_de_dia(vencimiento(2026, 10, &[])), "2026-11-17");
        // Diciembre de 2025: el 17 de enero de 2026 es sábado; pasa al lunes 19.
        assert_eq!(iso_de_dia(vencimiento(2025, 12, &[])), "2026-01-19");
        // Si además el lunes es festivo, pasa al martes.
        assert_eq!(iso_de_dia(vencimiento(2025, 12, &[(19, 1)])), "2026-01-20");
        // Abril de 2026: el 17 de mayo es domingo; pasa al lunes 18.
        assert_eq!(iso_de_dia(vencimiento(2026, 4, &[])), "2026-05-18");
    }

    #[test]
    fn meses_o_fraccion_de_mora() {
        // Enero de 2008 vencía el 18 de febrero (el 17 fue domingo).
        assert_eq!(meses_de_mora((2008, 1), (2008, 2, 25)), 1);
        assert_eq!(meses_de_mora((2008, 1), (2008, 3, 17)), 1);
        assert_eq!(meses_de_mora((2008, 1), (2008, 3, 18)), 2);
        assert_eq!(meses_de_mora((2008, 1), (2009, 2, 17)), 12);
        assert_eq!(meses_de_mora((2007, 12), (2008, 1, 30)), 1);
        // Pago el día siguiente a un vencimiento recorrido, antes del 17 nominal del mes siguiente.
        assert_eq!(meses_de_mora((2025, 12), (2026, 1, 20)), 1);
    }

    #[test]
    fn factor_actualizacion_y_recargos() {
        assert_eq!(factor_de_actualizacion(143.588, 142.645), 10_066);
        assert_eq!(factor_de_actualizacion(100.0, 100.0), 10_000);
        // Con deflación el factor no baja de 1.
        assert_eq!(factor_de_actualizacion(99.0, 100.0), 10_000);
        assert_eq!(factor_de_actualizacion(104.5, 100.0), 10_450);
        assert_eq!(factor_de_actualizacion(1.0, 0.0), 10_000);

        // Como en el archivo real: un mes al 1.13 %, sin actualización.
        let e = Extemporaneo { factor: 10_000, tasa: 113, ..Extemporaneo::default() };
        assert_eq!(e.cargos(12_000), (0, 136)); // 120.00 × 1.13 % = 1.356
        assert_eq!(e.cargos(7_740), (0, 87)); // 77.40 × 1.13 % = 0.8746
        // Con actualización: 1,000.00 × 0.0450 = 45.00; recargos sobre 1,045.00 al 4.41 % = 46.08.
        let e = Extemporaneo { factor: 10_450, tasa: 441, ..Extemporaneo::default() };
        assert_eq!(e.cargos(100_000), (4_500, 4_608));
        assert_eq!(e.cargos(0), (0, 0));

        let e = Extemporaneo {
            fecha_pago: "2026-03-20".into(),
            vencimiento: "2026-02-17".into(),
            meses: 2,
            factor: 10_066,
            tasa: 294,
            actualizacion_imss: 1,
            recargos_imss: 2,
            actualizacion_rcv: 3,
            recargos_rcv: 4,
            actualizacion_infonavit: 5,
            recargos_infonavit: 6,
        };
        assert_eq!(e.total(), 21);
        assert_eq!(Extemporaneo::de_texto(&e.a_texto()), Some(e));
        assert_eq!(Extemporaneo::de_texto(""), None);
        assert_eq!(Extemporaneo::de_texto("a|b"), None);
    }
}
