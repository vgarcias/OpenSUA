//! Cálculos guardados: una fotografía por patrón y periodo del resultado de
//! [`Base::calcular_periodo`]. Guardar de nuevo el mismo periodo sustituye al
//! anterior. Al eliminar un patrón se eliminan sus cálculos guardados.

use super::periodo::{CalculoPeriodo, LineaCalculo};
use super::Base;
use crate::dominio::calculo::{CuotasBimestre, CuotasMes};
use crate::error::{ErrorSua, Resultado};
use rusqlite::{params, OptionalExtension};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalculoGuardado {
    /// Fecha y hora UTC, `AAAA-MM-DD HH:MM:SS`.
    pub guardado_en: String,
    pub calculo: CalculoPeriodo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumenCalculo {
    pub periodo: String,
    pub guardado_en: String,
    pub trabajadores: i64,
    /// Total en centavos (IMSS + RCV + INFONAVIT).
    pub total: i64,
}

const COLUMNAS_DE_IMPORTE: &str = "cuota_fija, excedente_patronal, excedente_obrero, pd_patronal, pd_obrero, gmp_patronal, gmp_obrero, \
                                   riesgos_trabajo, iv_patronal, iv_obrero, guarderias, retiro, cyv_patronal, cyv_obrero, infonavit";
const COLUMNAS_DE_INCIDENCIAS: &str = "ausentismos_mes, incapacidad_mes, ausentismos_bimestre, incapacidad_bimestre";
const COLUMNAS_DE_CREDITO: &str = "amortizacion, seguro_vivienda, num_credito";

impl Base {
    /// Guarda el cálculo de un patrón en su periodo, sustituyendo el que
    /// hubiera. Devuelve la fecha y hora en que quedó guardado.
    pub fn guardar_calculo(&self, reg_pat: &str, r: &CalculoPeriodo) -> Resultado<String> {
        self.con_permiso(crate::datos::usuarios::Permiso::Capturar, "Guardó cálculo", || format!("{reg_pat} periodo {}", r.periodo), || self.guardar_calculo_sin_control(reg_pat, r))
    }

    fn guardar_calculo_sin_control(&self, reg_pat: &str, r: &CalculoPeriodo) -> Resultado<String> {
        if r.lineas.is_empty() {
            return Err(ErrorSua::Validacion("El cálculo no tiene trabajadores con días cotizados: no hay nada que guardar.".into()));
        }
        let ahora = crate::bitacora::marca_utc(std::time::SystemTime::now());
        let tx = self.transaccion()?;
        tx.execute("DELETE FROM calculo WHERE reg_pat = ?1 AND periodo = ?2", [reg_pat, r.periodo.as_str()])?;
        tx.execute(
            "INSERT INTO calculo (reg_pat, periodo, cierra_bimestre, decreto, prima_cienmilesimas, guardado_en, avisos, extemporaneo) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                reg_pat,
                r.periodo,
                r.cierra_bimestre,
                r.decreto,
                r.prima_rt.unwrap_or(-1),
                ahora,
                r.avisos.join("\n"),
                r.extemporaneo.as_ref().map(|e| e.a_texto()).unwrap_or_default()
            ],
        )?;
        let id = tx.last_insert_rowid();
        {
            let mut ins = tx.prepare(&format!(
                "INSERT INTO calculo_linea (calculo_id, orden, num_afil, nombre, sdi, dias_mes, dias_bimestre, {COLUMNAS_DE_IMPORTE}, {COLUMNAS_DE_INCIDENCIAS}, {COLUMNAS_DE_CREDITO})
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29)"
            ))?;
            for (orden, l) in r.lineas.iter().enumerate() {
                let (m, b) = (&l.mes, &l.bimestre);
                ins.execute(params![
                    id,
                    orden as i64,
                    l.nss,
                    l.nombre,
                    l.sdi,
                    l.dias_mes,
                    l.dias_bimestre,
                    m.cuota_fija,
                    m.excedente_patronal,
                    m.excedente_obrero,
                    m.pd_patronal,
                    m.pd_obrero,
                    m.gmp_patronal,
                    m.gmp_obrero,
                    m.riesgos_trabajo,
                    m.iv_patronal,
                    m.iv_obrero,
                    m.guarderias,
                    b.retiro,
                    b.cyv_patronal,
                    b.cyv_obrero,
                    b.infonavit,
                    l.ausentismos_mes,
                    l.incapacidad_mes,
                    l.ausentismos_bimestre,
                    l.incapacidad_bimestre,
                    b.amortizacion,
                    b.seguro_vivienda,
                    l.credito,
                ])?;
            }
        }
        tx.commit()?;
        Ok(ahora)
    }

    /// El cálculo guardado de un patrón en un periodo `AAAAMM`, si existe.
    pub fn calculo_guardado(&self, reg_pat: &str, periodo: &str) -> Resultado<Option<CalculoGuardado>> {
        let cabecera = self
            .conn
            .query_row(
                "SELECT id, cierra_bimestre, decreto, prima_cienmilesimas, guardado_en, avisos, extemporaneo FROM calculo WHERE reg_pat = ?1 AND periodo = ?2",
                [reg_pat, periodo],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, bool>(1)?, r.get::<_, u8>(2)?, r.get::<_, i64>(3)?, r.get::<_, String>(4)?, r.get::<_, String>(5)?, r.get::<_, String>(6)?)),
            )
            .optional()?;
        let Some((id, cierra_bimestre, decreto, prima, guardado_en, avisos, extemporaneo)) = cabecera else { return Ok(None) };
        let mut calculo = CalculoPeriodo {
            periodo: periodo.to_string(),
            cierra_bimestre,
            decreto,
            prima_rt: (prima >= 0).then_some(prima),
            avisos: avisos.lines().map(str::to_string).collect(),
            extemporaneo: crate::dominio::extemporaneo::Extemporaneo::de_texto(&extemporaneo),
            ..CalculoPeriodo::default()
        };
        let mut stmt = self.conn.prepare(&format!(
            "SELECT num_afil, nombre, sdi, dias_mes, dias_bimestre, {COLUMNAS_DE_IMPORTE}, {COLUMNAS_DE_INCIDENCIAS}, {COLUMNAS_DE_CREDITO} FROM calculo_linea WHERE calculo_id = ?1 ORDER BY orden"
        ))?;
        let lineas = stmt.query_map([id], |r| {
            Ok(LineaCalculo {
                nss: r.get(0)?,
                nombre: r.get(1)?,
                sdi: r.get(2)?,
                dias_mes: r.get(3)?,
                dias_bimestre: r.get(4)?,
                ausentismos_mes: r.get(20)?,
                incapacidad_mes: r.get(21)?,
                ausentismos_bimestre: r.get(22)?,
                incapacidad_bimestre: r.get(23)?,
                credito: r.get(26)?,
                mes: CuotasMes {
                    cuota_fija: r.get(5)?,
                    excedente_patronal: r.get(6)?,
                    excedente_obrero: r.get(7)?,
                    pd_patronal: r.get(8)?,
                    pd_obrero: r.get(9)?,
                    gmp_patronal: r.get(10)?,
                    gmp_obrero: r.get(11)?,
                    riesgos_trabajo: r.get(12)?,
                    iv_patronal: r.get(13)?,
                    iv_obrero: r.get(14)?,
                    guarderias: r.get(15)?,
                },
                bimestre: CuotasBimestre {
                    retiro: r.get(16)?,
                    cyv_patronal: r.get(17)?,
                    cyv_obrero: r.get(18)?,
                    infonavit: r.get(19)?,
                    amortizacion: r.get(24)?,
                    seguro_vivienda: r.get(25)?,
                },
            })
        })?;
        for linea in lineas {
            let l = linea?;
            calculo.mes.sumar(&l.mes);
            calculo.bimestre.sumar(&l.bimestre);
            calculo.lineas.push(l);
        }
        Ok(Some(CalculoGuardado { guardado_en, calculo }))
    }

    /// Cálculos guardados de un patrón, del periodo más reciente al más antiguo.
    pub fn calculos_guardados(&self, reg_pat: &str) -> Resultado<Vec<ResumenCalculo>> {
        let suma = format!("{} + amortizacion + seguro_vivienda", COLUMNAS_DE_IMPORTE.replace(", ", " + "));
        let mut stmt = self.conn.prepare(&format!(
            "SELECT c.periodo, c.guardado_en, COUNT(l.orden), COALESCE(SUM({suma}), 0), c.extemporaneo
             FROM calculo c LEFT JOIN calculo_linea l ON l.calculo_id = c.id
             WHERE c.reg_pat = ?1 GROUP BY c.id ORDER BY c.periodo DESC"
        ))?;
        let filas = stmt.query_map([reg_pat], |r| {
            // El total incluye actualización y recargos si el cálculo fue extemporáneo.
            let extemporaneo = crate::dominio::extemporaneo::Extemporaneo::de_texto(&r.get::<_, String>(4)?).map_or(0, |e| e.total());
            Ok(ResumenCalculo { periodo: r.get(0)?, guardado_en: r.get(1)?, trabajadores: r.get(2)?, total: r.get::<_, i64>(3)? + extemporaneo })
        })?;
        Ok(filas.collect::<Result<_, _>>()?)
    }

    pub fn eliminar_calculo(&self, reg_pat: &str, periodo: &str) -> Resultado<bool> {
        self.con_permiso(crate::datos::usuarios::Permiso::Capturar, "Eliminó cálculo guardado", || format!("{reg_pat} periodo {periodo}"), || self.eliminar_calculo_sin_control(reg_pat, periodo))
    }

    fn eliminar_calculo_sin_control(&self, reg_pat: &str, periodo: &str) -> Resultado<bool> {
        Ok(self.conn.execute("DELETE FROM calculo WHERE reg_pat = ?1 AND periodo = ?2", [reg_pat, periodo])? > 0)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::config::{CarpetaTemporal, Modo};
    use crate::datos::captura::{Patron, Trabajador};
    use crate::dominio::validar::luhn;

    const REG: &str = "Z9900000109";

    fn base_con_dos_trabajadores(t: &CarpetaTemporal) -> Base {
        let b = Base::abrir_en(&t.0.join("sua.db"), &t.0.join("respaldos"), Modo::Portable, 3).unwrap();
        b.guardar_patron(&Patron { reg_pat: REG.into(), rfc_pat: "XAXX010101000".into(), nom_pat: "PATRON FICTICIO".into(), ..Patron::default() })
            .unwrap();
        b.guardar_prima_rt(REG, "2008-02", 758_875).unwrap();
        for (n, sdi) in [(1u64, 10_000), (2, 9_000)] {
            let base10 = format!("{:010}", 9_000_000_000u64 + n);
            let nss = format!("{base10}{}", luhn(&base10).unwrap());
            b.guardar_trabajador(&Trabajador {
                reg_patr: REG.into(),
                num_afil: nss.clone(),
                nom_aseg: format!("FICTICIO$$TRABAJADOR {n}"),
                sal_imss: sdi,
                ..Trabajador::default()
            })
            .unwrap();
            b.guardar_movimiento(REG, &nss, "01", "2008-01-01", sdi).unwrap();
        }
        b
    }

    #[test]
    fn un_calculo_guardado_se_recupera_identico() {
        let t = CarpetaTemporal::nueva("calc1");
        let b = base_con_dos_trabajadores(&t);
        assert_eq!(b.calculo_guardado(REG, "200802").unwrap(), None);
        assert!(b.calculos_guardados(REG).unwrap().is_empty());

        // Con un ausentismo y una incapacidad, para que también se guarden sus días.
        let nss = b.trabajadores(REG, "").unwrap()[0].num_afil.clone();
        b.guardar_incidencia(REG, &nss, "11", "2008-02-05", 2, "").unwrap();
        b.guardar_incidencia(REG, &nss, "12", "2008-01-10", 4, "AB123456").unwrap();
        // Y con un crédito INFONAVIT, para que se guarden amortización, seguro y número.
        b.guardar_credito(REG, &nss, "15", "2008-01-01", "0912345678", 1, 250_000).unwrap();
        let febrero = b.calcular_periodo(REG, "200802", false).unwrap();
        assert_eq!((febrero.lineas[0].ausentismos_mes, febrero.lineas[0].ausentismos_bimestre, febrero.lineas[0].incapacidad_bimestre), (2, 2, 4));
        assert!(febrero.lineas[0].bimestre.amortizacion > 0 && febrero.lineas[0].bimestre.seguro_vivienda > 0 && febrero.lineas[0].credito == "0912345678");
        let cuando = b.guardar_calculo(REG, &febrero).unwrap();
        let g = b.calculo_guardado(REG, "200802").unwrap().unwrap();
        assert_eq!(g.calculo, febrero);
        assert_eq!(g.guardado_en, cuando);

        // El mismo periodo pagado tarde: se guardan actualización y recargos, y el total los incluye.
        let tarde = b.calcular_periodo_con_pago(REG, "200802", false, Some("2008-05-20")).unwrap();
        assert!(tarde.extemporaneo.is_some());
        b.guardar_calculo(REG, &tarde).unwrap();
        assert_eq!(b.calculo_guardado(REG, "200802").unwrap().unwrap().calculo, tarde);
        assert_eq!(b.calculos_guardados(REG).unwrap()[0].total, tarde.total());
        b.guardar_calculo(REG, &febrero).unwrap();

        // Otro periodo, con aviso: la prima todavía no estaba vigente.
        let enero = b.calcular_periodo(REG, "200801", false).unwrap();
        assert!(enero.prima_rt.is_none() && !enero.avisos.is_empty());
        b.guardar_calculo(REG, &enero).unwrap();
        assert_eq!(b.calculo_guardado(REG, "200801").unwrap().unwrap().calculo, enero);

        let lista = b.calculos_guardados(REG).unwrap();
        assert_eq!(lista.iter().map(|x| x.periodo.as_str()).collect::<Vec<_>>(), ["200802", "200801"]);
        assert_eq!((lista[0].trabajadores, lista[0].total), (2, febrero.total()));
        assert_eq!(lista[1].total, enero.total());
    }

    #[test]
    fn guardar_de_nuevo_sustituye_y_no_sigue_a_la_captura() {
        let t = CarpetaTemporal::nueva("calc2");
        let b = base_con_dos_trabajadores(&t);
        let antes = b.calcular_periodo(REG, "200802", false).unwrap();
        b.guardar_calculo(REG, &antes).unwrap();

        // Cambia la captura: el cálculo guardado no cambia hasta guardarlo otra vez.
        let nss = antes.lineas[1].nss.clone();
        b.guardar_movimiento(REG, &nss, "02", "2008-02-10", 0).unwrap();
        let despues = b.calcular_periodo(REG, "200802", false).unwrap();
        assert_ne!(despues.total(), antes.total());
        assert_eq!(b.calculo_guardado(REG, "200802").unwrap().unwrap().calculo, antes);

        b.guardar_calculo(REG, &despues).unwrap();
        assert_eq!(b.calculo_guardado(REG, "200802").unwrap().unwrap().calculo, despues);
        assert_eq!(b.calculos_guardados(REG).unwrap().len(), 1);
        let n: i64 = b.conexion().query_row("SELECT COUNT(*) FROM calculo_linea", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2);

        assert!(b.eliminar_calculo(REG, "200802").unwrap());
        assert!(!b.eliminar_calculo(REG, "200802").unwrap());
        let n: i64 = b.conexion().query_row("SELECT COUNT(*) FROM calculo_linea", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn un_calculo_vacio_no_se_guarda() {
        let t = CarpetaTemporal::nueva("calc3");
        let b = base_con_dos_trabajadores(&t);
        // Antes de las altas no hay días cotizados.
        let vacio = b.calcular_periodo(REG, "200706", false).unwrap();
        assert!(vacio.lineas.is_empty());
        assert!(matches!(b.guardar_calculo(REG, &vacio), Err(ErrorSua::Validacion(_))));
        assert!(b.calculos_guardados(REG).unwrap().is_empty());
    }
}
