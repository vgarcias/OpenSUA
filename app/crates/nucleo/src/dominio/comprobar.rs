//! Comprobación de un archivo de pago contra el motor de cálculo propio:
//! recalcula las cuotas de cada trabajador con su salario y sus días, y las
//! compara con las que trae el archivo.
//!
//! El archivo solo da un salario y un total de días por trabajador. Si el
//! trabajador tuvo varios periodos o salarios en el mes, el SUA calculó por
//! tramos y aquí puede haber diferencias de centavos que no son un error.

use crate::dominio::calculo::Tasas;
use crate::io::pago::ArchivoPago;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Comprobacion {
    pub trabajadores: usize,
    pub conceptos_comparados: usize,
    pub conceptos_iguales: usize,
    /// Suma de las diferencias en valor absoluto, en centavos.
    pub diferencia_total: i64,
    /// Una línea por concepto que no coincide. El trabajador se identifica por
    /// su número de orden en el archivo, sin datos personales.
    pub diferencias: Vec<String>,
}

impl Comprobacion {
    pub fn coincide(&self) -> bool {
        self.conceptos_comparados == self.conceptos_iguales
    }
}

pub fn comprobar_pago(a: &ArchivoPago, tasas: &Tasas) -> Comprobacion {
    let mut r = Comprobacion { trabajadores: a.trabajadores.len(), ..Comprobacion::default() };
    for (i, x) in a.trabajadores.iter().enumerate() {
        let mut comparar = |concepto: &str, archivo: i64, calculado: i64| {
            r.conceptos_comparados += 1;
            if archivo == calculado {
                r.conceptos_iguales += 1;
            } else {
                r.diferencia_total += (archivo - calculado).abs();
                r.diferencias.push(format!(
                    "trabajador {}: {concepto}: el archivo trae {}.{:02} y el cálculo da {}.{:02}",
                    i + 1,
                    archivo / 100,
                    archivo % 100,
                    calculado / 100,
                    calculado % 100
                ));
            }
        };
        if x.dias_mes > 0 {
            let c = tasas.mes(x.sdi, x.dias_mes as u32);
            comparar("cuota fija", x.cuota_fija, c.cuota_fija);
            comparar("excedente", x.excedente, c.excedente_patronal + c.excedente_obrero);
            comparar("prestaciones en dinero", x.prestaciones_dinero, c.pd_patronal + c.pd_obrero);
            comparar("gastos médicos de pensionados", x.gastos_medicos, c.gmp_patronal + c.gmp_obrero);
            comparar("riesgos de trabajo", x.riesgos_trabajo, c.riesgos_trabajo);
            comparar("invalidez y vida", x.invalidez_vida, c.iv_patronal + c.iv_obrero);
            comparar("guarderías", x.guarderias, c.guarderias);
            for (concepto, codificado, calculado) in [
                ("excedente, parte obrera", x.excedente_obrero, c.excedente_obrero),
                ("prestaciones en dinero, parte obrera", x.prestaciones_dinero_obrero, c.pd_obrero),
                ("gastos médicos, parte obrera", x.gastos_medicos_obrero, c.gmp_obrero),
                ("invalidez y vida, parte obrera", x.invalidez_vida_obrero, c.iv_obrero),
            ] {
                if let Some(v) = codificado {
                    comparar(concepto, v, calculado);
                }
            }
        }
        if x.dias_bimestre > 0 {
            let b = tasas.bimestre(x.sdi, x.dias_bimestre as u32);
            comparar("retiro", x.retiro, b.retiro);
            comparar("cesantía y vejez patronal", x.cyv_patronal, b.cyv_patronal);
            comparar("cesantía y vejez obrera", x.cyv_obrera, b.cyv_obrero);
            // Un archivo puede pagar RCV sin INFONAVIT (pagos separados).
            if x.aportacion_patronal > 0 {
                comparar("aportación INFONAVIT", x.aportacion_patronal, b.infonavit);
            }
        }
    }
    r
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::dominio::calculo::pruebas::tasas;
    use crate::io::pago;

    #[test]
    fn el_archivo_ficticio_coincide_con_el_motor() {
        // El archivo de prueba se arma con las tasas de 2008 y la prima 7.5888 %.
        let a = pago::leer(&pago::pruebas::ejemplo()).unwrap();
        let mut t = tasas(2008, 52.59, 0);
        t.prima_rt = 7.5888;
        let r = comprobar_pago(&a, &t);
        assert_eq!(r.trabajadores, 2);
        assert_eq!(r.conceptos_comparados, 2 * (7 + 4 + 4));
        assert!(r.coincide(), "{:?}", r.diferencias);
    }

    #[test]
    fn con_otras_tasas_se_listan_las_diferencias() {
        let a = pago::leer(&pago::pruebas::ejemplo()).unwrap();
        let r = comprobar_pago(&a, &tasas(2006, 48.67, 0));
        assert!(!r.coincide());
        assert!(r.diferencia_total > 0);
        assert!(r.diferencias.iter().any(|d| d.starts_with("trabajador 1: cuota fija")));
        // Retiro (2 %) no depende del año: debe seguir coincidiendo.
        assert!(!r.diferencias.iter().any(|d| d.contains("retiro")));
    }

    /// Con archivos reales (variable `SUA_MUESTRAS`): casi todo debe coincidir.
    /// Se conoce una diferencia de un centavo en cesantía patronal.
    #[test]
    fn muestras_reales_si_estan_disponibles() {
        use crate::config::{CarpetaTemporal, Modo};
        use crate::datos::Base;
        let Some(carpeta) = std::env::var_os("SUA_MUESTRAS") else { return };
        let t = CarpetaTemporal::nueva("comp");
        let base = Base::abrir_en(&t.0.join("sua.db"), &t.0.join("respaldos"), Modo::Portable, 3).unwrap();
        let mut pendientes = vec![std::path::PathBuf::from(carpeta)];
        let (mut comparados, mut iguales, mut archivos) = (0, 0, 0);
        while let Some(dir) = pendientes.pop() {
            for e in std::fs::read_dir(&dir).unwrap().flatten() {
                let ruta = e.path();
                if ruta.is_dir() {
                    pendientes.push(ruta);
                } else if ruta.extension().is_some_and(|x| x.eq_ignore_ascii_case("sua")) {
                    let a = pago::leer(&std::fs::read(&ruta).unwrap()).unwrap();
                    let con = comprobar_pago(&a, &base.tasas(&a.patron.periodo, a.patron.prima_rt, true).unwrap().tasas);
                    let sin = comprobar_pago(&a, &base.tasas(&a.patron.periodo, a.patron.prima_rt, false).unwrap().tasas);
                    let mejor = if con.conceptos_iguales >= sin.conceptos_iguales { con } else { sin };
                    comparados += mejor.conceptos_comparados;
                    iguales += mejor.conceptos_iguales;
                    archivos += 1;
                    assert!(mejor.diferencia_total <= 1, "diferencia de {} centavos", mejor.diferencia_total);
                }
            }
        }
        assert!(archivos > 0);
        assert!(comparados - iguales <= 2, "{iguales} de {comparados}");
    }
}
