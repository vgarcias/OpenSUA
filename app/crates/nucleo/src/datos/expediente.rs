//! Datos complementarios del trabajador: datos afiliatorios y datos médicos
//! de cada incapacidad (ramo, tipo de riesgo, secuela, control y porcentaje).
//!
//! Los códigos son los del catálogo `CatalogoRT` del SUA. Los datos médicos
//! no cambian las cuotas: sirven para la determinación anual de la prima de
//! riesgos de trabajo ([`crate::dominio::prima_rt`]).

use super::captura::es_fecha_iso;
use super::Base;
use crate::dominio::extemporaneo::iso_de_dia;
use crate::dominio::prima_rt::Siniestralidad;
use crate::dominio::tramos::{dia, dia_de_iso, tramos, Movimiento};
use crate::error::{ErrorSua, Resultado};
use rusqlite::{params, OptionalExtension};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Afiliacion {
    pub reg_patr: String,
    pub num_afil: String,
    /// Código postal del trabajador (5 dígitos) o vacío.
    pub cpp_trab: String,
    /// `AAAA-MM-DD` o vacío.
    pub fec_nac: String,
    pub lug_nac: String,
    /// Clave de la entidad de nacimiento (2 dígitos) o vacío.
    pub ent_trab: String,
    /// Unidad de medicina familiar (hasta 3 dígitos) o vacío.
    pub umf_trab: String,
    pub ocupa: String,
    /// `M`, `F` o vacío.
    pub sexo: String,
    /// `0` fijo, `1` variable, `2` mixto, o vacío.
    pub tip_sal: String,
    /// Un dígito, o vacío.
    pub jor_hor: String,
}

impl Afiliacion {
    /// `true` si no se capturó ningún dato.
    pub fn vacia(&self) -> bool {
        [&self.cpp_trab, &self.fec_nac, &self.lug_nac, &self.ent_trab, &self.umf_trab, &self.ocupa, &self.sexo, &self.tip_sal, &self.jor_hor].iter().all(|c| c.trim().is_empty())
    }
}

/// Datos médicos de una incapacidad.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DatosIncapacidad {
    /// 1 riesgos de trabajo, 2 enfermedad general, 3 maternidad.
    pub ramo: u8,
    /// 1 accidente de trabajo, 2 accidente en trayecto, 3 enfermedad profesional; 0 si el ramo no es riesgos de trabajo.
    pub tipo_riesgo: u8,
    /// 0 a 9 (4 = defunción).
    pub secuela: u8,
    /// 0 a 9.
    pub control: u8,
    /// Porcentaje de incapacidad permanente, 0 a 100.
    pub porcentaje: u8,
}

/// Una incapacidad con sus datos médicos, para la prima y para el TXT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncapacidadCompleta {
    pub num_afil: String,
    pub folio: String,
    /// `AAAA-MM-DD`
    pub inicio: String,
    /// `AAAA-MM-DD`
    pub termino: String,
    pub dias: u32,
    /// `None` si no se han capturado los datos médicos.
    pub datos: Option<DatosIncapacidad>,
}

fn mal(m: &str) -> ErrorSua {
    ErrorSua::Validacion(m.to_string())
}

pub fn validar_datos_de_incapacidad(d: &DatosIncapacidad) -> Resultado<()> {
    if !(1..=3).contains(&d.ramo) {
        return Err(mal("Ramo: 1 riesgos de trabajo, 2 enfermedad general o 3 maternidad."));
    }
    if d.ramo == 1 {
        if !(1..=3).contains(&d.tipo_riesgo) {
            return Err(mal("Tipo de riesgo: 1 accidente de trabajo, 2 accidente en trayecto o 3 enfermedad profesional."));
        }
    } else if d.tipo_riesgo != 0 || d.porcentaje != 0 {
        return Err(mal("El tipo de riesgo y el porcentaje solo se capturan en riesgos de trabajo."));
    }
    if d.secuela > 9 || d.control > 9 {
        return Err(mal("Secuela y control: un dígito de 0 a 9, según el catálogo CatalogoRT."));
    }
    if d.porcentaje > 100 {
        return Err(mal("Porcentaje de incapacidad: de 0 a 100."));
    }
    Ok(())
}

impl Base {
    fn trabajador_existe(&self, reg_pat: &str, nss: &str) -> Resultado<()> {
        let n: i64 = self.conn.query_row("SELECT COUNT(*) FROM asegura WHERE reg_patr = ?1 AND num_afil = ?2", [reg_pat, nss], |r| r.get(0))?;
        if n == 0 {
            return Err(mal("Elija primero un trabajador registrado."));
        }
        Ok(())
    }

    /// Alta o cambio de los datos afiliatorios de un trabajador.
    pub fn guardar_afiliacion(&self, datos: &Afiliacion) -> Resultado<()> {
        self.con_permiso(crate::datos::usuarios::Permiso::Capturar, "Guardó datos afiliatorios", || format!("{} {}", datos.reg_patr, datos.num_afil), || self.guardar_afiliacion_sin_control(datos))
    }

    fn guardar_afiliacion_sin_control(&self, datos: &Afiliacion) -> Resultado<()> {
        let m = |s: &String| s.trim().to_uppercase();
        let a = Afiliacion {
            reg_patr: m(&datos.reg_patr),
            num_afil: m(&datos.num_afil),
            cpp_trab: m(&datos.cpp_trab),
            fec_nac: m(&datos.fec_nac),
            lug_nac: m(&datos.lug_nac),
            ent_trab: m(&datos.ent_trab),
            umf_trab: m(&datos.umf_trab),
            ocupa: m(&datos.ocupa),
            sexo: m(&datos.sexo),
            tip_sal: m(&datos.tip_sal),
            jor_hor: m(&datos.jor_hor),
        };
        self.trabajador_existe(&a.reg_patr, &a.num_afil)?;
        let digitos = |s: &str, largos: &[usize]| s.is_empty() || (largos.contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit()));
        if !digitos(&a.cpp_trab, &[5]) {
            return Err(mal("Código postal del trabajador: 5 dígitos."));
        }
        if !a.fec_nac.is_empty() && !es_fecha_iso(&a.fec_nac) {
            return Err(mal("Fecha de nacimiento: use el formato AAAA-MM-DD."));
        }
        if a.lug_nac.chars().count() > 25 || a.ocupa.chars().count() > 12 {
            return Err(mal("Lugar de nacimiento: hasta 25 caracteres. Ocupación: hasta 12."));
        }
        if !digitos(&a.ent_trab, &[2]) || !digitos(&a.umf_trab, &[1, 2, 3]) {
            return Err(mal("Entidad de nacimiento: 2 dígitos. Unidad de medicina familiar: hasta 3 dígitos."));
        }
        if !matches!(a.sexo.as_str(), "" | "M" | "F") {
            return Err(mal("Sexo: M o F."));
        }
        if !matches!(a.tip_sal.as_str(), "" | "0" | "1" | "2") {
            return Err(mal("Tipo de salario: 0 fijo, 1 variable o 2 mixto."));
        }
        if !digitos(&a.jor_hor, &[1]) {
            return Err(mal("Jornada: un dígito."));
        }
        self.conn.execute(
            "INSERT INTO afiliacion (reg_patr, num_afil, cpp_trab, fec_nac, lug_nac, ent_trab, umf_trab, ocupa, sexo, tip_sal, jor_hor)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT (reg_patr, num_afil) DO UPDATE SET cpp_trab = excluded.cpp_trab, fec_nac = excluded.fec_nac, lug_nac = excluded.lug_nac,
                 ent_trab = excluded.ent_trab, umf_trab = excluded.umf_trab, ocupa = excluded.ocupa, sexo = excluded.sexo, tip_sal = excluded.tip_sal, jor_hor = excluded.jor_hor",
            params![a.reg_patr, a.num_afil, a.cpp_trab, a.fec_nac, a.lug_nac, a.ent_trab, a.umf_trab, a.ocupa, a.sexo, a.tip_sal, a.jor_hor],
        )?;
        Ok(())
    }

    pub fn afiliacion(&self, reg_pat: &str, nss: &str) -> Resultado<Option<Afiliacion>> {
        Ok(self
            .conn
            .query_row(
                "SELECT reg_patr, num_afil, cpp_trab, fec_nac, lug_nac, ent_trab, umf_trab, ocupa, sexo, tip_sal, jor_hor FROM afiliacion WHERE reg_patr = ?1 AND num_afil = ?2",
                [reg_pat, nss],
                |r| {
                    Ok(Afiliacion {
                        reg_patr: r.get(0)?,
                        num_afil: r.get(1)?,
                        cpp_trab: r.get(2)?,
                        fec_nac: r.get(3)?,
                        lug_nac: r.get(4)?,
                        ent_trab: r.get(5)?,
                        umf_trab: r.get(6)?,
                        ocupa: r.get(7)?,
                        sexo: r.get(8)?,
                        tip_sal: r.get(9)?,
                        jor_hor: r.get(10)?,
                    })
                },
            )
            .optional()?)
    }

    /// Guarda los datos médicos de una incapacidad ya registrada (movimiento
    /// `12` con ese folio).
    pub fn guardar_datos_de_incapacidad(&self, reg_pat: &str, nss: &str, folio: &str, datos: &DatosIncapacidad) -> Resultado<()> {
        self.con_permiso(crate::datos::usuarios::Permiso::Capturar, "Guardó datos médicos de incapacidad", || format!("{reg_pat} {nss} folio {folio}"), || self.guardar_datos_de_incapacidad_sin_control(reg_pat, nss, folio, datos))
    }

    fn guardar_datos_de_incapacidad_sin_control(&self, reg_pat: &str, nss: &str, folio: &str, datos: &DatosIncapacidad) -> Resultado<()> {
        validar_datos_de_incapacidad(datos)?;
        let folio = folio.trim().to_uppercase();
        let movimiento = self.movimientos(reg_pat, nss)?.into_iter().find(|m| m.tipo == "12" && m.folio == folio).ok_or_else(|| mal("No hay una incapacidad registrada con ese folio."))?;
        let termino = dia_de_iso(&movimiento.fecha).map(|d| iso_de_dia(d + i64::from(movimiento.dias) - 1)).unwrap_or_default();
        self.conn.execute(
            "INSERT INTO incapacidades (reg_pat, num_afi, fec_acc, fol_inc, ram_seg, tip_rie, secuela, con_inc, dia_sub, por_inc, fec_ter)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT (reg_pat, num_afi, fol_inc) DO UPDATE SET fec_acc = excluded.fec_acc, ram_seg = excluded.ram_seg, tip_rie = excluded.tip_rie,
                 secuela = excluded.secuela, con_inc = excluded.con_inc, dia_sub = excluded.dia_sub, por_inc = excluded.por_inc, fec_ter = excluded.fec_ter",
            params![
                reg_pat,
                nss,
                movimiento.fecha,
                folio,
                datos.ramo.to_string(),
                datos.tipo_riesgo.to_string(),
                datos.secuela.to_string(),
                datos.control.to_string(),
                movimiento.dias,
                datos.porcentaje,
                termino
            ],
        )?;
        Ok(())
    }

    /// Incapacidades de un patrón (movimientos `12`), con sus datos médicos si los tienen.
    pub fn incapacidades(&self, reg_pat: &str) -> Resultado<Vec<IncapacidadCompleta>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.num_afil, m.fol_inc, m.fec_inic, m.num_dias, i.ram_seg, i.tip_rie, i.secuela, i.con_inc, i.por_inc
             FROM movtos m LEFT JOIN incapacidades i ON i.reg_pat = m.reg_patr AND i.num_afi = m.num_afil AND i.fol_inc = m.fol_inc
             WHERE m.reg_patr = ?1 AND m.tip_movs = '12' ORDER BY m.num_afil, m.fec_inic",
        )?;
        let filas = stmt.query_map([reg_pat], |r| {
            let inicio: String = r.get(2)?;
            let dias: u32 = r.get(3)?;
            let codigo = |i: usize| -> rusqlite::Result<Option<u8>> { Ok(r.get::<_, Option<String>>(i)?.map(|t| t.trim().parse().unwrap_or(0))) };
            let datos = match codigo(4)? {
                Some(ramo) => Some(DatosIncapacidad {
                    ramo,
                    tipo_riesgo: codigo(5)?.unwrap_or(0),
                    secuela: codigo(6)?.unwrap_or(0),
                    control: codigo(7)?.unwrap_or(0),
                    porcentaje: r.get::<_, Option<u8>>(8)?.unwrap_or(0),
                }),
                None => None,
            };
            let termino = dia_de_iso(&inicio).map(|d| iso_de_dia(d + i64::from(dias) - 1)).unwrap_or_default();
            Ok(IncapacidadCompleta { num_afil: r.get(0)?, folio: r.get(1)?, inicio, termino, dias, datos })
        })?;
        Ok(filas.collect::<Result<_, _>>()?)
    }
}

impl Base {
    /// Reúne los datos del año para determinar la prima de riesgos de trabajo:
    /// días cotizados de todos los trabajadores (los mismos por los que se
    /// paga el ramo: sin ausentismos ni incapacidades), y los casos de riesgo
    /// de trabajo terminados en el año, sin contar los accidentes en trayecto.
    /// Usa el factor de prima sin acreditación de la STPS.
    pub fn siniestralidad(&self, reg_pat: &str, anio: u32) -> Resultado<Siniestralidad> {
        let (inicio, fin) = (dia(i64::from(anio), 1, 1), dia(i64::from(anio), 12, 31));
        let mut dias_cotizados = 0i64;
        for t in self.trabajadores(reg_pat, "")? {
            let mut movs: Vec<Movimiento> = self
                .movimientos(reg_pat, &t.num_afil)?
                .iter()
                .filter_map(|m| Some(Movimiento { tipo: m.tipo.clone(), fecha: dia_de_iso(&m.fecha)?, sdi: m.sdi, dias: m.dias }))
                .collect();
            // Como en el cálculo: sin movimientos de empleo, activo desde su fecha de alta.
            if !movs.iter().any(|m| matches!(m.tipo.as_str(), "00" | "13" | "01" | "02" | "07" | "08")) {
                movs.push(Movimiento { tipo: "01".into(), fecha: dia_de_iso(&t.fec_alt).unwrap_or(inicio), sdi: t.sal_imss, dias: 0 });
            }
            let ts = tramos(&movs, fin);
            dias_cotizados += ts.tramos.iter().map(|tramo| i64::from(ts.dias_cotizados(tramo, (inicio, fin)).otros_ramos())).sum::<i64>();
        }
        let (mut dias_subsidiados, mut porcentajes, mut defunciones, mut casos, mut sin_clasificar) = (0i64, 0i64, 0i64, 0usize, 0usize);
        let del_anio = format!("{anio}-");
        for i in self.incapacidades(reg_pat)?.iter().filter(|i| i.termino.starts_with(&del_anio)) {
            match i.datos {
                None => sin_clasificar += 1,
                Some(d) if d.ramo == 1 && d.tipo_riesgo != 2 => {
                    casos += 1;
                    dias_subsidiados += i64::from(i.dias);
                    porcentajes += i64::from(d.porcentaje);
                    defunciones += i64::from(d.secuela == 4);
                }
                Some(_) => {}
            }
        }
        let falta = |que: &str| mal(&format!("El catálogo no tiene {que} para {anio}."));
        let del_catalogo = |catalogo: &str, columna: usize| -> Resultado<Option<f64>> {
            let t = self.leer_catalogo(catalogo)?;
            Ok(t.filas.iter().find(|f| f.first().is_some_and(|a| a.trim() == anio.to_string())).and_then(|f| f.get(columna)?.trim().parse().ok()))
        };
        let factor = del_catalogo("Factor_Prima", 2)?.ok_or_else(|| falta("factor de prima"))?;
        let cienmilesimas = |v: f64| (v * 100_000.0).round() as i64;
        let prima_minima = cienmilesimas(del_catalogo("Prima_Minima", 1)?.ok_or_else(|| falta("prima mínima"))?);
        let prima_maxima = cienmilesimas(del_catalogo("Prima_Minima", 2)?.ok_or_else(|| falta("prima máxima"))?);
        // La prima que regía al terminar el año: la más reciente hasta febrero del siguiente.
        let prima_anterior: Option<i64> = self
            .conn
            .query_row(
                "SELECT prima_cienmilesimas FROM patron_prima_rt WHERE reg_pat = ?1 AND vigente_desde <= ?2 ORDER BY vigente_desde DESC LIMIT 1",
                params![reg_pat, format!("{}-02", anio + 1)],
                |r| r.get(0),
            )
            .optional()?;
        Ok(Siniestralidad { anio, dias_subsidiados, porcentajes, defunciones, dias_cotizados, factor, prima_anterior, prima_minima, prima_maxima, casos, sin_clasificar })
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::config::{CarpetaTemporal, Modo};
    use crate::datos::captura::{Patron, Trabajador};
    use crate::dominio::validar::luhn;

    const REG: &str = "Z9900000109";

    fn base(t: &CarpetaTemporal) -> (Base, String) {
        let b = Base::abrir_en(&t.0.join("sua.db"), &t.0.join("respaldos"), Modo::Portable, 3).unwrap();
        b.guardar_patron(&Patron { reg_pat: REG.into(), rfc_pat: "XAXX010101000".into(), nom_pat: "PATRON FICTICIO".into(), ..Patron::default() })
            .unwrap();
        let nss = format!("9000000001{}", luhn("9000000001").unwrap());
        b.guardar_trabajador(&Trabajador { reg_patr: REG.into(), num_afil: nss.clone(), nom_aseg: "FICTICIO$$UNO".into(), sal_imss: 30_000, ..Trabajador::default() })
            .unwrap();
        b.guardar_movimiento(REG, &nss, "01", "2026-01-01", 30_000).unwrap();
        (b, nss)
    }

    #[test]
    fn datos_afiliatorios() {
        let t = CarpetaTemporal::nueva("exp1");
        let (b, nss) = base(&t);
        assert_eq!(b.afiliacion(REG, &nss).unwrap(), None);
        let a = Afiliacion {
            reg_patr: REG.into(),
            num_afil: nss.clone(),
            cpp_trab: "06000".into(),
            fec_nac: "1990-05-17".into(),
            lug_nac: "ciudad ficticia".into(),
            ent_trab: "09".into(),
            umf_trab: "12".into(),
            ocupa: "albañil".into(),
            sexo: "m".into(),
            tip_sal: "0".into(),
            jor_hor: "8".into(),
        };
        assert!(!a.vacia() && Afiliacion::default().vacia());
        b.guardar_afiliacion(&a).unwrap();
        let g = b.afiliacion(REG, &nss).unwrap().unwrap();
        assert_eq!((g.lug_nac.as_str(), g.ocupa.as_str(), g.sexo.as_str(), g.fec_nac.as_str()), ("CIUDAD FICTICIA", "ALBAÑIL", "M", "1990-05-17"));
        // Guardar de nuevo sustituye.
        b.guardar_afiliacion(&Afiliacion { umf_trab: "7".into(), ..a.clone() }).unwrap();
        assert_eq!(b.afiliacion(REG, &nss).unwrap().unwrap().umf_trab, "7");
        for (malo, texto) in [
            (Afiliacion { cpp_trab: "600".into(), ..a.clone() }, "Código postal"),
            (Afiliacion { fec_nac: "17/05/1990".into(), ..a.clone() }, "Fecha de nacimiento"),
            (Afiliacion { sexo: "X".into(), ..a.clone() }, "Sexo"),
            (Afiliacion { tip_sal: "5".into(), ..a.clone() }, "Tipo de salario"),
            (Afiliacion { ocupa: "OCUPACION DEMASIADO LARGA".into(), ..a.clone() }, "Ocupación"),
            (Afiliacion { num_afil: "00000000000".into(), ..a.clone() }, "trabajador registrado"),
        ] {
            assert!(matches!(b.guardar_afiliacion(&malo), Err(ErrorSua::Validacion(m)) if m.contains(texto)), "{texto}");
        }
        // Al eliminar al trabajador se van sus datos afiliatorios.
        let id = b.movimientos(REG, &nss).unwrap()[0].id;
        b.eliminar_movimiento(id).unwrap();
        b.eliminar_trabajador(REG, &nss).unwrap();
        assert_eq!(b.afiliacion(REG, &nss).unwrap(), None);
    }

    #[test]
    fn datos_medicos_de_la_incapacidad() {
        let t = CarpetaTemporal::nueva("exp2");
        let (b, nss) = base(&t);
        let rt = DatosIncapacidad { ramo: 1, tipo_riesgo: 1, secuela: 1, control: 1, porcentaje: 0 };
        assert!(matches!(b.guardar_datos_de_incapacidad(REG, &nss, "AB123456", &rt), Err(ErrorSua::Validacion(m)) if m.contains("ese folio")));
        b.guardar_incidencia(REG, &nss, "12", "2026-02-25", 10, "AB123456").unwrap();
        b.guardar_incidencia(REG, &nss, "12", "2026-04-01", 3, "AB123457").unwrap();
        for (malo, texto) in [
            (DatosIncapacidad { ramo: 4, ..rt }, "Ramo"),
            (DatosIncapacidad { tipo_riesgo: 0, ..rt }, "Tipo de riesgo"),
            (DatosIncapacidad { ramo: 2, ..rt }, "solo se capturan"),
            (DatosIncapacidad { secuela: 10, ..rt }, "Secuela"),
            (DatosIncapacidad { porcentaje: 101, ..rt }, "Porcentaje"),
        ] {
            assert!(matches!(b.guardar_datos_de_incapacidad(REG, &nss, "AB123456", &malo), Err(ErrorSua::Validacion(m)) if m.contains(texto)), "{texto}");
        }
        b.guardar_datos_de_incapacidad(REG, &nss, "ab123456", &rt).unwrap();
        b.guardar_datos_de_incapacidad(REG, &nss, "AB123456", &DatosIncapacidad { secuela: 3, control: 5, porcentaje: 15, ..rt }).unwrap();
        let lista = b.incapacidades(REG).unwrap();
        assert_eq!(lista.len(), 2);
        assert_eq!((lista[0].folio.as_str(), lista[0].inicio.as_str(), lista[0].termino.as_str(), lista[0].dias), ("AB123456", "2026-02-25", "2026-03-06", 10));
        assert_eq!(lista[0].datos, Some(DatosIncapacidad { ramo: 1, tipo_riesgo: 1, secuela: 3, control: 5, porcentaje: 15 }));
        assert_eq!(lista[1].datos, None);
        // Al eliminar el movimiento se eliminan sus datos médicos.
        let id = b.movimientos(REG, &nss).unwrap().into_iter().find(|m| m.folio == "AB123456").unwrap().id;
        b.eliminar_movimiento(id).unwrap();
        assert_eq!(b.incapacidades(REG).unwrap().len(), 1);
        let n: i64 = b.conexion().query_row("SELECT COUNT(*) FROM incapacidades", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn siniestralidad_del_anio_y_prima() {
        let t = CarpetaTemporal::nueva("exp3");
        let (b, nss) = base(&t);
        b.guardar_prima_rt(REG, "2026-03", 758_875).unwrap();
        // Activo todo 2026 (365 días). Un riesgo de trabajo de 10 días con 15 % de incapacidad permanente,
        // un accidente en trayecto (no cuenta), una enfermedad general (no cuenta) y una sin clasificar.
        b.guardar_incidencia(REG, &nss, "12", "2026-02-25", 10, "AB123456").unwrap();
        b.guardar_datos_de_incapacidad(REG, &nss, "AB123456", &DatosIncapacidad { ramo: 1, tipo_riesgo: 1, secuela: 3, control: 5, porcentaje: 15 }).unwrap();
        b.guardar_incidencia(REG, &nss, "12", "2026-05-04", 4, "AB123457").unwrap();
        b.guardar_datos_de_incapacidad(REG, &nss, "AB123457", &DatosIncapacidad { ramo: 1, tipo_riesgo: 2, secuela: 1, control: 1, porcentaje: 0 }).unwrap();
        b.guardar_incidencia(REG, &nss, "12", "2026-07-06", 3, "AB123458").unwrap();
        b.guardar_datos_de_incapacidad(REG, &nss, "AB123458", &DatosIncapacidad { ramo: 2, tipo_riesgo: 0, secuela: 0, control: 1, porcentaje: 0 }).unwrap();
        b.guardar_incidencia(REG, &nss, "12", "2026-09-07", 2, "AB123459").unwrap();
        b.guardar_incidencia(REG, &nss, "11", "2026-10-05", 1, "").unwrap();

        let s = b.siniestralidad(REG, 2026).unwrap();
        // 365 días menos 19 de incapacidad y 1 de ausentismo.
        assert_eq!((s.dias_cotizados, s.dias_subsidiados, s.porcentajes, s.defunciones, s.casos, s.sin_clasificar), (345, 10, 15, 0, 1, 1));
        assert_eq!((s.factor, s.prima_anterior, s.prima_minima, s.prima_maxima), (2.3, Some(758_875), 50_000, 1_500_000));
        let r = s.determinar().unwrap();
        // Con un solo trabajador la fórmula se dispara; la prima solo puede subir un punto.
        assert!(r.calculada > 1_500_000);
        assert_eq!(r.nueva, 858_875);
        // Otro año: sin casos, y la prima anterior es la misma.
        let s = b.siniestralidad(REG, 2027).unwrap();
        assert_eq!((s.dias_cotizados, s.casos, s.determinar().unwrap().nueva), (365, 0, 658_875));
        // Un año sin catálogo.
        assert!(matches!(b.siniestralidad(REG, 2040), Err(ErrorSua::Validacion(m)) if m.contains("2040")));
    }
}
