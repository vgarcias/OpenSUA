//! Créditos INFONAVIT: de los movimientos de crédito de un trabajador a los
//! tramos de días en que hay descuento, con su tipo y su valor.
//!
//! Movimientos (`Revision_Manual_SUA.html`; manual del SUA, p. 93–105):
//! `15` inicio de crédito, `16` suspensión de descuento, `17` reinicio,
//! `18` modificación de tipo de descuento, `19` modificación de valor,
//! `20` modificación de número de crédito.
//!
//! Tipos de descuento: `1` porcentaje del salario, `2` cuota fija mensual en
//! pesos, `3` factor de descuento mensual (veces la UMI; antes de 2016, veces
//! el salario mínimo). El valor se guarda en diezmilésimas: 25 % = 250000,
//! 1,500.00 pesos = 15000000, factor 28.5320 = 285320.
//!
//! El importe lo calcula [`crate::dominio::calculo::Tasas::amortizacion`].
//! **Nada de esto está comprobado con un caso real**: no hay ninguno en la
//! muestra. No se aplica la tabla de disminución de porcentaje (créditos
//! anteriores a 1998).

use super::tramos::Dia;

/// Diezmilésimas por unidad en el valor del descuento.
pub const ESCALA_DE_VALOR: i64 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovCredito {
    /// 15 a 20.
    pub tipo: u8,
    pub fecha: Dia,
    pub numero: String,
    /// 1 porcentaje, 2 cuota fija, 3 factor; 0 si el movimiento no lo lleva.
    pub tipo_descuento: u8,
    /// En diezmilésimas; 0 si el movimiento no lo lleva.
    pub valor: i64,
}

/// Datos vigentes de un crédito.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descuento {
    pub numero: String,
    pub tipo_descuento: u8,
    pub valor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TramoCredito {
    pub desde: Dia,
    pub hasta: Dia,
    pub descuento: Descuento,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Estado {
    #[default]
    SinCredito,
    Activo,
    Suspendido,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Creditos {
    pub tramos: Vec<TramoCredito>,
    /// Estado al final de los movimientos considerados.
    pub estado: Estado,
    /// Datos del crédito al final, esté activo o suspendido.
    pub vigente: Option<Descuento>,
    /// Movimientos fuera de secuencia (suspensión sin crédito activo, etc.).
    pub no_aplicados: usize,
}

/// Arma los tramos con descuento. `hasta`: último día a considerar; se
/// ignoran los movimientos posteriores. El día de la suspensión todavía
/// lleva descuento, igual que el día de la baja todavía se cotiza.
pub fn creditos(movimientos: &[MovCredito], hasta: Dia) -> Creditos {
    let mut ordenados: Vec<&MovCredito> = movimientos.iter().filter(|m| m.fecha <= hasta).collect();
    ordenados.sort_by_key(|m| m.fecha);
    let mut r = Creditos::default();
    let mut activo: Option<(Dia, Descuento)> = None;
    let mut suspendido: Option<Descuento> = None;

    fn cerrar(r: &mut Creditos, activo: &mut Option<(Dia, Descuento)>, ultimo: Dia) -> Option<Descuento> {
        let (desde, descuento) = activo.take()?;
        if ultimo >= desde {
            r.tramos.push(TramoCredito { desde, hasta: ultimo, descuento: descuento.clone() });
        }
        Some(descuento)
    }

    for m in ordenados {
        match m.tipo {
            15 => {
                cerrar(&mut r, &mut activo, m.fecha - 1);
                suspendido = None;
                activo = Some((m.fecha, Descuento { numero: m.numero.clone(), tipo_descuento: m.tipo_descuento, valor: m.valor }));
            }
            16 if activo.is_some() => suspendido = cerrar(&mut r, &mut activo, m.fecha),
            17 if suspendido.is_some() => {
                // Si se reinicia el mismo día de la suspensión, ese día no se cuenta dos veces.
                let desde = r.tramos.last().map_or(m.fecha, |t| m.fecha.max(t.hasta + 1));
                activo = suspendido.take().map(|d| (desde, d));
            }
            18..=20 if activo.is_some() || suspendido.is_some() => {
                let mut d = match cerrar(&mut r, &mut activo, m.fecha - 1) {
                    Some(d) => {
                        activo = Some((m.fecha, d.clone()));
                        d
                    }
                    None => suspendido.clone().expect("hay crédito suspendido"),
                };
                match m.tipo {
                    18 => (d.tipo_descuento, d.valor) = (m.tipo_descuento, m.valor),
                    19 => d.valor = m.valor,
                    _ => d.numero = m.numero.clone(),
                }
                match &mut activo {
                    Some((_, a)) => *a = d,
                    None => suspendido = Some(d),
                }
            }
            _ => r.no_aplicados += 1,
        }
    }
    r.estado = match (&activo, &suspendido) {
        (Some(_), _) => Estado::Activo,
        (None, Some(_)) => Estado::Suspendido,
        (None, None) => Estado::SinCredito,
    };
    r.vigente = activo.as_ref().map(|(_, d)| d.clone()).or(suspendido);
    if let Some((desde, descuento)) = activo {
        r.tramos.push(TramoCredito { desde, hasta, descuento });
    }
    r
}

/// Valida el valor de un descuento (en diezmilésimas) según su tipo. Devuelve
/// el motivo si no es válido.
pub fn validar_descuento(tipo_descuento: u8, valor: i64) -> Result<(), &'static str> {
    let centesimas_exactas = valor % 100 == 0;
    match tipo_descuento {
        1 if valor > 0 && valor < 100 * ESCALA_DE_VALOR && centesimas_exactas => Ok(()),
        1 => Err("Porcentaje de descuento: mayor que cero y menor que 100, con dos decimales como máximo."),
        2 if valor > 0 && valor < 100_000 * ESCALA_DE_VALOR && centesimas_exactas => Ok(()),
        2 => Err("Cuota fija mensual: mayor que cero y hasta 99,999.99, con dos decimales como máximo."),
        3 if valor > 0 && valor < 1_000 * ESCALA_DE_VALOR => Ok(()),
        3 => Err("Factor de descuento: mayor que cero y hasta 999.9999."),
        _ => Err("Tipo de descuento: 1 porcentaje, 2 cuota fija o 3 factor de descuento."),
    }
}

/// `28.532` → 285320. Hasta cuatro decimales; admite comas de millares.
pub fn texto_a_valor(texto: &str) -> Option<i64> {
    let t = texto.trim().replace(',', "");
    let (entero, dec) = t.split_once('.').unwrap_or((&t, ""));
    if entero.is_empty() || entero.len() > 9 || dec.len() > 4 || !entero.bytes().chain(dec.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(entero.parse::<i64>().ok()? * ESCALA_DE_VALOR + format!("{dec:0<4}").parse::<i64>().ok()?)
}

/// Descripción legible: `25.00 %`, `cuota fija 1500.00`, `factor 28.5320`.
pub fn descuento_en_texto(tipo_descuento: u8, valor: i64) -> String {
    let (e, d) = (valor / ESCALA_DE_VALOR, valor % ESCALA_DE_VALOR);
    match tipo_descuento {
        1 => format!("{e}.{:02} %", d / 100),
        2 => format!("cuota fija {e}.{:02}", d / 100),
        3 => format!("factor {e}.{d:04}"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::dominio::tramos::dia_de_iso;

    fn mov(tipo: u8, iso: &str, numero: &str, td: u8, valor: i64) -> MovCredito {
        MovCredito { tipo, fecha: dia_de_iso(iso).unwrap(), numero: numero.into(), tipo_descuento: td, valor }
    }
    fn d(iso: &str) -> Dia {
        dia_de_iso(iso).unwrap()
    }

    #[test]
    fn inicio_suspension_y_reinicio() {
        let movs = [mov(15, "2026-01-10", "0912345678", 1, 250_000), mov(16, "2026-02-10", "", 0, 0), mov(17, "2026-03-01", "", 0, 0)];
        let c = creditos(&movs, d("2026-04-30"));
        assert_eq!(c.tramos.len(), 2);
        assert_eq!((c.tramos[0].desde, c.tramos[0].hasta), (d("2026-01-10"), d("2026-02-10")));
        assert_eq!((c.tramos[1].desde, c.tramos[1].hasta), (d("2026-03-01"), d("2026-04-30")));
        assert_eq!(c.tramos[1].descuento, Descuento { numero: "0912345678".into(), tipo_descuento: 1, valor: 250_000 });
        assert_eq!((c.estado, c.no_aplicados), (Estado::Activo, 0));

        // Visto antes del reinicio: suspendido, pero se conocen sus datos.
        let c = creditos(&movs, d("2026-02-20"));
        assert_eq!((c.tramos.len(), c.estado), (1, Estado::Suspendido));
        assert_eq!(c.vigente.unwrap().numero, "0912345678");
        // Antes del inicio no hay crédito.
        let c = creditos(&movs, d("2026-01-05"));
        assert_eq!((c.tramos.len(), c.estado, c.vigente), (0, Estado::SinCredito, None));
        // Reinicio el mismo día de la suspensión: ese día no se duplica.
        let c = creditos(&[movs[0].clone(), mov(16, "2026-02-10", "", 0, 0), mov(17, "2026-02-10", "", 0, 0)], d("2026-02-28"));
        assert_eq!((c.tramos[0].hasta, c.tramos[1].desde), (d("2026-02-10"), d("2026-02-11")));
    }

    #[test]
    fn modificaciones_parten_el_tramo() {
        let movs = [
            mov(15, "2026-01-01", "0912345678", 2, 15_000_000),
            mov(19, "2026-03-01", "", 0, 16_000_000),
            mov(18, "2026-05-01", "", 3, 285_320),
            mov(20, "2026-05-01", "0912345686", 0, 0),
        ];
        let c = creditos(&movs, d("2026-06-30"));
        let resumen: Vec<(Dia, Dia, u8, i64)> = c.tramos.iter().map(|t| (t.desde, t.hasta, t.descuento.tipo_descuento, t.descuento.valor)).collect();
        assert_eq!(
            resumen,
            [
                (d("2026-01-01"), d("2026-02-28"), 2, 15_000_000),
                (d("2026-03-01"), d("2026-04-30"), 2, 16_000_000),
                (d("2026-05-01"), d("2026-06-30"), 3, 285_320),
            ]
        );
        assert_eq!(c.tramos[2].descuento.numero, "0912345686");
        // Una modificación con el crédito suspendido cambia los datos sin abrir tramo.
        let c = creditos(&[movs[0].clone(), mov(16, "2026-01-31", "", 0, 0), mov(19, "2026-02-01", "", 0, 9_000_000)], d("2026-02-28"));
        assert_eq!((c.tramos.len(), c.estado, c.vigente.unwrap().valor), (1, Estado::Suspendido, 9_000_000));
    }

    #[test]
    fn movimientos_fuera_de_secuencia() {
        let c = creditos(&[mov(16, "2026-01-10", "", 0, 0), mov(17, "2026-01-11", "", 0, 0), mov(19, "2026-01-12", "", 0, 1)], d("2026-02-28"));
        assert_eq!((c.tramos.len(), c.no_aplicados, c.estado), (0, 3, Estado::SinCredito));
        // Un inicio nuevo sustituye al crédito anterior.
        let c = creditos(&[mov(15, "2026-01-01", "0900000001", 1, 200_000), mov(15, "2026-02-01", "0900000002", 1, 300_000)], d("2026-02-28"));
        assert_eq!((c.tramos.len(), c.tramos[0].hasta, c.no_aplicados), (2, d("2026-01-31"), 0));
    }

    #[test]
    fn valores_de_descuento() {
        assert_eq!(texto_a_valor("25"), Some(250_000));
        assert_eq!(texto_a_valor("1,500.50"), Some(15_005_000));
        assert_eq!(texto_a_valor("28.532"), Some(285_320));
        assert_eq!(texto_a_valor("28.53201"), None);
        assert_eq!(texto_a_valor("veinte"), None);
        assert_eq!(descuento_en_texto(1, 250_000), "25.00 %");
        assert_eq!(descuento_en_texto(2, 15_005_000), "cuota fija 1500.50");
        assert_eq!(descuento_en_texto(3, 285_320), "factor 28.5320");
        assert!(validar_descuento(1, 250_000).is_ok());
        assert!(validar_descuento(1, 1_000_000).is_err());
        assert!(validar_descuento(1, 250_050).is_err());
        assert!(validar_descuento(2, 15_005_000).is_ok());
        assert!(validar_descuento(2, 0).is_err());
        assert!(validar_descuento(3, 285_321).is_ok());
        assert!(validar_descuento(4, 1).is_err());
    }
}
