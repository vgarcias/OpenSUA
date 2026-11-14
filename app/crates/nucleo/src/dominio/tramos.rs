//! De movimientos a tramos de días cotizados.
//!
//! Reglas comprobadas con archivos de pago reales: los días que resultan
//! coinciden con los del archivo en 28 de 28 trabajadores (mes) y 39 de 39
//! (bimestre). `Decompilacion_SUA.md`, sección 15.5.
//!
//! - `00` y `13` (salario al inicio del bimestre), `01` (alta) y `08`
//!   (reingreso) abren un tramo con su salario.
//! - `02` (baja) lo cierra; **el día de la baja se cotiza**.
//! - `07` (modificación de salario) cierra el tramo el día anterior y abre otro
//!   con el salario nuevo. Regla de ley, sin caso en la muestra.
//! - Ausentismos (`11`) e incapacidades (`12`) **no se descuentan todavía**.

/// Fecha como número de día (días desde el 1-ene-1970).
use crate::dominio::calculo::DiasCotizados;

pub type Dia = i64;

pub fn dia(anio: i64, mes: i64, d: i64) -> Dia {
    // Algoritmo de fecha civil de Howard Hinnant.
    let a = if mes <= 2 { anio - 1 } else { anio };
    let era = a.div_euclid(400);
    let yoe = a.rem_euclid(400);
    let doy = (153 * (if mes > 2 { mes - 3 } else { mes + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `AAAA-MM-DD` → día. `None` si el texto no tiene esa forma.
pub fn dia_de_iso(texto: &str) -> Option<Dia> {
    if !crate::datos::captura::es_fecha_iso(texto) {
        return None;
    }
    Some(dia(texto[..4].parse().ok()?, texto[5..7].parse().ok()?, texto[8..].parse().ok()?))
}

pub fn dias_del_mes(anio: i64, mes: i64) -> i64 {
    let (a2, m2) = if mes == 12 { (anio + 1, 1) } else { (anio, mes + 1) };
    dia(a2, m2, 1) - dia(anio, mes, 1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Periodo {
    pub anio: i64,
    pub mes: i64,
}

impl Periodo {
    /// `AAAAMM`
    pub fn de_texto(texto: &str) -> Option<Periodo> {
        if texto.len() != 6 || !texto.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let (anio, mes) = (texto[..4].parse().ok()?, texto[4..].parse().ok()?);
        ((1..=12).contains(&mes) && anio >= 1997).then_some(Periodo { anio, mes })
    }

    /// Primer y último día del mes.
    pub fn mes_completo(&self) -> (Dia, Dia) {
        let ini = dia(self.anio, self.mes, 1);
        (ini, ini + dias_del_mes(self.anio, self.mes) - 1)
    }

    /// `true` en los meses pares: en ellos se paga además el bimestre.
    pub fn cierra_bimestre(&self) -> bool {
        self.mes % 2 == 0
    }

    /// Primer y último día del bimestre al que pertenece el mes.
    pub fn bimestre(&self) -> (Dia, Dia) {
        let primero = if self.cierra_bimestre() { self.mes - 1 } else { self.mes };
        let ini = dia(self.anio, primero, 1);
        (ini, dia(self.anio, primero + 1, 1) + dias_del_mes(self.anio, primero + 1) - 1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Movimiento {
    pub tipo: String,
    pub fecha: Dia,
    /// Salario diario integrado en centavos (0 en las bajas).
    pub sdi: i64,
    /// Días del ausentismo (`11`) o de la incapacidad (`12`), contados desde
    /// la fecha; 0 en los demás movimientos.
    pub dias: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Incidencia {
    Ausentismo,
    Incapacidad,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tramo {
    pub desde: Dia,
    pub hasta: Dia,
    pub sdi: i64,
}

impl Tramo {
    /// Días del tramo que caen dentro de `[inicio, fin]`.
    pub fn dias_en(&self, (inicio, fin): (Dia, Dia)) -> u32 {
        let (a, b) = (self.desde.max(inicio), self.hasta.min(fin));
        if b >= a {
            (b - a + 1) as u32
        } else {
            0
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tramos {
    pub tramos: Vec<Tramo>,
    /// Ausentismos e incapacidades: (clase, primer día, último día). Solo
    /// cuentan los días que caen dentro de un tramo.
    pub incidencias: Vec<(Incidencia, Dia, Dia)>,
    /// Movimientos que no se pudieron aplicar (tipo no manejado, o fuera de
    /// secuencia: baja sin alta, modificación sin estar activo).
    pub no_aplicados: usize,
}

impl Tramos {
    /// Ausentismo o incapacidad que cubre un día. Si coinciden, es incapacidad.
    pub fn incidencia_del_dia(&self, d: Dia) -> Option<Incidencia> {
        let cubre = |clase: Incidencia| self.incidencias.iter().any(|(c, a, b)| *c == clase && (*a..=*b).contains(&d));
        if cubre(Incidencia::Incapacidad) {
            Some(Incidencia::Incapacidad)
        } else if cubre(Incidencia::Ausentismo) {
            Some(Incidencia::Ausentismo)
        } else {
            None
        }
    }

    /// Días de un tramo dentro de `[inicio, fin]`, con sus ausentismos e incapacidades.
    pub fn dias_cotizados(&self, tramo: &Tramo, (inicio, fin): (Dia, Dia)) -> DiasCotizados {
        let mut r = DiasCotizados::default();
        for d in tramo.desde.max(inicio)..=tramo.hasta.min(fin) {
            r.naturales += 1;
            match self.incidencia_del_dia(d) {
                Some(Incidencia::Incapacidad) => r.incapacidad += 1,
                Some(Incidencia::Ausentismo) => r.ausentismo += 1,
                None => {}
            }
        }
        r
    }

    /// `true` si el día cae en algún tramo.
    pub fn activo_el(&self, d: Dia) -> bool {
        self.tramos.iter().any(|t| (t.desde..=t.hasta).contains(&d))
    }
}

/// Arma los tramos de un trabajador. `hasta`: último día a considerar; un
/// trabajador que sigue activo cotiza hasta ese día.
pub fn tramos(movimientos: &[Movimiento], hasta: Dia) -> Tramos {
    let mut ordenados: Vec<&Movimiento> = movimientos.iter().filter(|m| m.fecha <= hasta).collect();
    // El mismo día, la baja va antes que el reingreso.
    ordenados.sort_by_key(|m| (m.fecha, m.tipo != "02"));
    let mut r = Tramos::default();
    let mut activo: Option<(Dia, i64)> = None;
    for m in ordenados {
        match (m.tipo.as_str(), activo) {
            ("02", Some((desde, sdi))) => {
                r.tramos.push(Tramo { desde, hasta: m.fecha, sdi });
                activo = None;
            }
            ("00" | "13" | "01" | "08", previo) => {
                if let Some((desde, sdi)) = previo {
                    if m.fecha > desde {
                        r.tramos.push(Tramo { desde, hasta: m.fecha - 1, sdi });
                    }
                }
                activo = Some((m.fecha, m.sdi));
            }
            ("07", Some((desde, sdi))) => {
                if m.fecha > desde {
                    r.tramos.push(Tramo { desde, hasta: m.fecha - 1, sdi });
                }
                activo = Some((m.fecha, m.sdi));
            }
            ("11" | "12", _) if m.dias > 0 => {
                let clase = if m.tipo == "11" { Incidencia::Ausentismo } else { Incidencia::Incapacidad };
                r.incidencias.push((clase, m.fecha, m.fecha + Dia::from(m.dias) - 1));
            }
            // Movimientos de crédito INFONAVIT: no cambian los días; los trata `dominio::credito`.
            ("15" | "16" | "17" | "18" | "19" | "20", _) => {}
            _ => r.no_aplicados += 1,
        }
    }
    if let Some((desde, sdi)) = activo {
        r.tramos.push(Tramo { desde, hasta, sdi });
    }
    r
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn mov(tipo: &str, iso: &str, sdi: i64) -> Movimiento {
        Movimiento { tipo: tipo.into(), fecha: dia_de_iso(iso).unwrap(), sdi, dias: 0 }
    }

    fn incidencia(tipo: &str, iso: &str, dias: u32) -> Movimiento {
        Movimiento { tipo: tipo.into(), fecha: dia_de_iso(iso).unwrap(), sdi: 0, dias }
    }

    #[test]
    fn ausentismos_e_incapacidades_por_tramo_y_periodo() {
        let p = Periodo::de_texto("202602").unwrap();
        let movs = [
            mov("01", "2026-01-01", 30_000),
            mov("07", "2026-02-16", 35_000),
            incidencia("11", "2026-01-12", 2),
            // Incapacidad a caballo entre enero y febrero: 3 días de enero y 4 de febrero.
            incidencia("12", "2026-01-29", 7),
            // Ausentismo que coincide en un día con la incapacidad: ese día es de incapacidad.
            incidencia("11", "2026-02-04", 2),
            incidencia("11", "2026-02-20", 1),
        ];
        let t = tramos(&movs, p.bimestre().1);
        assert_eq!((t.tramos.len(), t.incidencias.len(), t.no_aplicados), (2, 4, 0));
        let d = t.dias_cotizados(&t.tramos[0], p.mes_completo());
        assert_eq!((d.naturales, d.ausentismo, d.incapacidad), (15, 1, 4));
        let d = t.dias_cotizados(&t.tramos[1], p.mes_completo());
        assert_eq!((d.naturales, d.ausentismo, d.incapacidad), (13, 1, 0));
        let d = t.dias_cotizados(&t.tramos[0], p.bimestre());
        assert_eq!((d.naturales, d.ausentismo, d.incapacidad), (46, 3, 7));
        assert!(t.activo_el(dia(2026, 2, 28)) && !t.activo_el(dia(2025, 12, 31)));
        // Una incidencia fuera de todo tramo no cuenta; una sin días no se aplica.
        let t = tramos(&[mov("01", "2026-02-10", 30_000), incidencia("11", "2026-02-02", 3), incidencia("12", "2026-02-12", 0)], p.bimestre().1);
        let d = t.dias_cotizados(&t.tramos[0], p.mes_completo());
        assert_eq!((d.naturales, d.ausentismo, d.incapacidad, t.no_aplicados), (19, 0, 0, 1));
    }

    #[test]
    fn fechas_y_periodos() {
        assert_eq!(dia(1970, 1, 1), 0);
        assert_eq!(dia_de_iso("2008-03-01").unwrap() - dia_de_iso("2008-02-01").unwrap(), 29);
        assert_eq!(dias_del_mes(2026, 2), 28);
        assert_eq!(dias_del_mes(2024, 2), 29);
        assert_eq!(dias_del_mes(2026, 12), 31);

        let feb = Periodo::de_texto("200802").unwrap();
        assert!(feb.cierra_bimestre());
        let (a, b) = feb.bimestre();
        assert_eq!((a, b - a + 1), (dia(2008, 1, 1), 60));
        let ene = Periodo::de_texto("200801").unwrap();
        assert!(!ene.cierra_bimestre());
        assert_eq!(ene.bimestre(), feb.bimestre());
        let (a, b) = Periodo::de_texto("200612").unwrap().bimestre();
        assert_eq!((a, b), (dia(2006, 11, 1), dia(2006, 12, 31)));
        for malo in ["2008", "200813", "200800", "abcdef"] {
            assert_eq!(Periodo::de_texto(malo), None);
        }
    }

    /// Secuencias tomadas de archivos reales (solo fechas y tipos): los días
    /// que resultan son los que traía el registro del trabajador.
    #[test]
    fn dias_como_en_los_archivos_reales() {
        let p = Periodo::de_texto("200802").unwrap();
        let fin = p.bimestre().1;

        // Activo todo el bimestre: 29 días de febrero, 60 del bimestre.
        let t = tramos(&[mov("13", "2008-01-01", 10_000)], fin);
        let suma = |t: &Tramos, r: (Dia, Dia)| t.tramos.iter().map(|x| x.dias_en(r)).sum::<u32>();
        assert_eq!((suma(&t, p.mes_completo()), suma(&t, p.bimestre())), (29, 60));

        // Alta el 11 de enero, baja el 18 de febrero, reingreso el 26: 22 y 43.
        let t = tramos(&[mov("08", "2008-02-26", 9_000), mov("01", "2008-01-11", 9_000), mov("02", "2008-02-18", 0)], fin);
        assert_eq!(t.tramos.len(), 2);
        assert_eq!((suma(&t, p.mes_completo()), suma(&t, p.bimestre())), (22, 43));
        assert_eq!(t.no_aplicados, 0);

        // Alta el 25 de enero y baja el 6 de febrero: 6 y 13. El día de la baja cuenta.
        let t = tramos(&[mov("01", "2008-01-25", 9_000), mov("02", "2008-02-06", 0)], fin);
        assert_eq!((suma(&t, p.mes_completo()), suma(&t, p.bimestre())), (6, 13));
    }

    #[test]
    fn modificacion_de_salario_parte_el_tramo() {
        let p = Periodo::de_texto("202604").unwrap();
        let t = tramos(&[mov("00", "2026-03-01", 30_000), mov("07", "2026-04-16", 35_000)], p.bimestre().1);
        assert_eq!(t.tramos.len(), 2);
        assert_eq!((t.tramos[0].sdi, t.tramos[0].dias_en(p.mes_completo())), (30_000, 15));
        assert_eq!((t.tramos[1].sdi, t.tramos[1].dias_en(p.mes_completo())), (35_000, 15));
        assert_eq!(t.tramos[0].dias_en(p.bimestre()), 31 + 15);
    }

    #[test]
    fn movimientos_fuera_de_secuencia_o_posteriores() {
        let fin = dia(2026, 2, 28);
        // Baja sin alta previa y movimiento de tipo no manejado: no se aplican, y se cuentan.
        let t = tramos(&[mov("02", "2026-01-10", 0), mov("09", "2026-01-12", 0)], fin);
        assert!(t.tramos.is_empty());
        assert_eq!(t.no_aplicados, 2);
        // Un movimiento posterior al periodo se ignora.
        let t = tramos(&[mov("00", "2026-01-01", 30_000), mov("02", "2026-03-15", 0)], fin);
        assert_eq!(t.tramos, vec![Tramo { desde: dia(2026, 1, 1), hasta: fin, sdi: 30_000 }]);
        // Baja y reingreso el mismo día: la baja va primero.
        let t = tramos(&[mov("00", "2026-01-01", 30_000), mov("08", "2026-01-20", 40_000), mov("02", "2026-01-20", 0)], fin);
        assert_eq!(t.tramos.len(), 2);
        assert_eq!((t.tramos[0].hasta, t.tramos[1].desde, t.tramos[1].sdi), (dia(2026, 1, 20), dia(2026, 1, 20), 40_000));
    }
}
