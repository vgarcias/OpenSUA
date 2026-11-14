//! Captura de patrones y trabajadores, e importación desde un archivo de pago.
//!
//! Toda escritura valida antes y va en transacción. Reglas de severidad
//! (`ESPECIFICACION_SUA.md`, §8): NSS y CURP con dígito incorrecto se rechazan;
//! en el RFC un dígito incorrecto es solo advertencia.

use super::Base;
use crate::dominio::validar::{self, Digito};
use crate::error::{ErrorSua, Resultado};
use crate::io::pago::ArchivoPago;
use rusqlite::{params, OptionalExtension};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Patron {
    pub reg_pat: String,
    pub rfc_pat: String,
    pub nom_pat: String,
    pub act_pat: String,
    pub dom_pat: String,
    pub mun_pat: String,
    pub cpp_pat: String,
    pub ent_pat: String,
    pub tel_pat: String,
    pub zon_pat: String,
    /// Delegación (2) + subdelegación (2).
    pub del_pat: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Trabajador {
    pub reg_patr: String,
    pub num_afil: String,
    pub curp: String,
    pub rfc_curp: String,
    /// `PATERNO$MATERNO$NOMBRE`
    pub nom_aseg: String,
    /// SDI en centavos.
    pub sal_imss: i64,
    /// `AAAA-MM-DD` o vacío.
    pub fec_alt: String,
    pub tip_tra: String,
    pub sem_jord: String,
    /// Número de crédito INFONAVIT (10 dígitos) o vacío.
    pub pag_info: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problema {
    pub campo: &'static str,
    pub mensaje: String,
    /// `true`: impide guardar. `false`: advertencia.
    pub grave: bool,
}

fn grave(campo: &'static str, mensaje: impl Into<String>) -> Problema {
    Problema { campo, mensaje: mensaje.into(), grave: true }
}

fn aviso(campo: &'static str, mensaje: impl Into<String>) -> Problema {
    Problema { campo, mensaje: mensaje.into(), grave: false }
}

fn largo(texto: &str) -> usize {
    texto.chars().count()
}

/// Convierte `123.45` o `123` en centavos. `None` si no es un importe válido.
pub fn texto_a_centavos(texto: &str) -> Option<i64> {
    let t = texto.trim().replace(',', "");
    let (entero, decimales) = t.split_once('.').unwrap_or((&t, ""));
    if entero.is_empty() || entero.len() > 9 || decimales.len() > 2 {
        return None;
    }
    if !entero.bytes().chain(decimales.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    let centavos = format!("{decimales:0<2}");
    Some(entero.parse::<i64>().ok()? * 100 + centavos.parse::<i64>().ok()?)
}

pub fn centavos_a_texto(centavos: i64) -> String {
    format!("{}.{:02}", centavos / 100, centavos % 100)
}

fn rfc_problemas(campo: &'static str, rfc: &str, salida: &mut Vec<Problema>) {
    let r = validar::rfc(rfc);
    if !r.formato_valido {
        salida.push(grave(campo, r.mensaje));
    } else if matches!(r.digito, Digito::Incorrecto { .. }) {
        salida.push(aviso(campo, r.mensaje));
    }
}

pub fn validar_patron(p: &Patron) -> Vec<Problema> {
    let mut s = Vec::new();
    if p.reg_pat.len() != 11 || !p.reg_pat.bytes().all(|b| b.is_ascii_alphanumeric()) {
        s.push(grave("Registro patronal", "Debe tener exactamente 11 caracteres (letras y dígitos)."));
    }
    rfc_problemas("RFC", &p.rfc_pat, &mut s);
    if p.nom_pat.trim().is_empty() {
        s.push(grave("Nombre o razón social", "Es obligatorio."));
    }
    for (campo, valor, max) in [
        ("Nombre o razón social", &p.nom_pat, 50),
        ("Actividad", &p.act_pat, 40),
        ("Domicilio", &p.dom_pat, 50),
        ("Municipio", &p.mun_pat, 50),
        ("Entidad", &p.ent_pat, 2),
        ("Teléfono", &p.tel_pat, 15),
        ("Área geográfica", &p.zon_pat, 1),
        ("Delegación y subdelegación", &p.del_pat, 4),
    ] {
        if largo(valor) > max {
            s.push(grave(campo, format!("Admite como máximo {max} caracteres.")));
        }
    }
    if !p.cpp_pat.is_empty() && (p.cpp_pat.len() != 5 || !p.cpp_pat.bytes().all(|b| b.is_ascii_digit())) {
        s.push(grave("Código postal", "Debe tener 5 dígitos."));
    }
    s
}

pub fn validar_trabajador(t: &Trabajador) -> Vec<Problema> {
    let mut s = Vec::new();
    let n = validar::nss(&t.num_afil);
    if !n.formato_valido || matches!(n.digito, Digito::Incorrecto { .. }) {
        s.push(grave("NSS", n.mensaje));
    }
    if !t.curp.is_empty() {
        let c = validar::curp(&t.curp);
        if !c.formato_valido || matches!(c.digito, Digito::Incorrecto { .. }) {
            s.push(grave("CURP", c.mensaje));
        }
    }
    if !t.rfc_curp.is_empty() {
        if !matches!(largo(&t.rfc_curp), 10 | 13) {
            s.push(grave("RFC", "Debe tener 13 caracteres, o 10 si no tiene homoclave."));
        } else {
            rfc_problemas("RFC", &t.rfc_curp, &mut s);
        }
    }
    let partes: Vec<&str> = t.nom_aseg.split('$').collect();
    if partes.len() != 3 || partes[0].trim().is_empty() || partes[2].trim().is_empty() {
        s.push(grave("Nombre", "Se requieren apellido paterno y nombre (el materno puede ir vacío)."));
    }
    if largo(&t.nom_aseg) > 50 {
        s.push(grave("Nombre", "Apellidos y nombre no pueden pasar de 50 caracteres en total, contando los separadores."));
    }
    if t.sal_imss <= 0 {
        s.push(grave("Salario diario integrado", "Debe ser mayor que cero."));
    } else if t.sal_imss > 9_999_999 {
        s.push(grave("Salario diario integrado", "No puede pasar de 99,999.99."));
    }
    if !t.pag_info.is_empty() {
        let c = validar::credito_infonavit(&t.pag_info);
        if !c.formato_valido {
            s.push(grave("Crédito INFONAVIT", c.mensaje));
        }
    }
    if !t.fec_alt.is_empty() && !es_fecha_iso(&t.fec_alt) {
        s.push(grave("Fecha de alta", "Use el formato AAAA-MM-DD."));
    }
    s
}

pub fn es_fecha_iso(texto: &str) -> bool {
    let b = texto.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let num = |r: std::ops::Range<usize>| texto[r].parse::<u32>().ok();
    let (Some(a), Some(m), Some(d)) = (num(0..4), num(5..7), num(8..10)) else { return false };
    let bisiesto = a % 4 == 0 && (a % 100 != 0 || a % 400 == 0);
    let dias = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => 28 + u32::from(bisiesto),
        _ => return false,
    };
    a >= 1900 && (1..=dias).contains(&d)
}

/// `AAAAMMDD` → `AAAA-MM-DD`. Devuelve vacío si no es una fecha válida.
pub fn fecha_compacta_a_iso(texto: &str) -> String {
    if texto.len() == 8 && texto.bytes().all(|b| b.is_ascii_digit()) {
        let iso = format!("{}-{}-{}", &texto[..4], &texto[4..6], &texto[6..]);
        if es_fecha_iso(&iso) {
            return iso;
        }
    }
    String::new()
}

fn rechazar_si_grave(problemas: &[Problema]) -> Resultado<()> {
    let graves: Vec<String> = problemas.iter().filter(|p| p.grave).map(|p| format!("{}: {}", p.campo, p.mensaje)).collect();
    if graves.is_empty() {
        Ok(())
    } else {
        Err(ErrorSua::Validacion(graves.join("\n")))
    }
}

fn normalizar_patron(p: &Patron) -> Patron {
    let m = |s: &String| s.trim().to_uppercase();
    Patron {
        reg_pat: m(&p.reg_pat),
        rfc_pat: m(&p.rfc_pat),
        nom_pat: m(&p.nom_pat),
        act_pat: m(&p.act_pat),
        dom_pat: m(&p.dom_pat),
        mun_pat: m(&p.mun_pat),
        cpp_pat: m(&p.cpp_pat),
        ent_pat: m(&p.ent_pat),
        tel_pat: m(&p.tel_pat),
        zon_pat: m(&p.zon_pat),
        del_pat: m(&p.del_pat),
    }
}

fn normalizar_trabajador(t: &Trabajador) -> Trabajador {
    let m = |s: &String| s.trim().to_uppercase();
    let nombre = t.nom_aseg.split('$').map(|p| p.trim().to_uppercase()).collect::<Vec<_>>().join("$");
    Trabajador {
        reg_patr: m(&t.reg_patr),
        num_afil: m(&t.num_afil),
        curp: m(&t.curp),
        rfc_curp: m(&t.rfc_curp),
        nom_aseg: nombre,
        sal_imss: t.sal_imss,
        fec_alt: m(&t.fec_alt),
        tip_tra: m(&t.tip_tra),
        sem_jord: m(&t.sem_jord),
        pag_info: m(&t.pag_info),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResumenImportacion {
    pub patron_nuevo: bool,
    pub trabajadores_nuevos: usize,
    pub trabajadores_actualizados: usize,
    pub movimientos_nuevos: usize,
    pub movimientos_ya_existentes: usize,
    /// Trabajadores cuyo NSS, RFC o CURP no pasa la validación. Se importan
    /// igual: el dato viene de un archivo ya presentado.
    pub con_advertencias: usize,
}

const SQL_PATRON: &str = "INSERT INTO patron (reg_pat, rfc_pat, nom_pat, act_pat, dom_pat, mun_pat, cpp_pat, ent_pat, tel_pat, zon_pat, del_pat)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
     ON CONFLICT (reg_pat) DO UPDATE SET rfc_pat = excluded.rfc_pat, nom_pat = excluded.nom_pat, act_pat = excluded.act_pat,
        dom_pat = excluded.dom_pat, mun_pat = excluded.mun_pat, cpp_pat = excluded.cpp_pat, ent_pat = excluded.ent_pat,
        tel_pat = excluded.tel_pat, zon_pat = excluded.zon_pat, del_pat = excluded.del_pat";

const SQL_TRABAJADOR: &str = "INSERT INTO asegura (reg_patr, num_afil, curp, rfc_curp, nom_aseg, sal_imss, fec_alt, tip_tra, sem_jord, pag_info)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
     ON CONFLICT (reg_patr, num_afil) DO UPDATE SET curp = excluded.curp, rfc_curp = excluded.rfc_curp, nom_aseg = excluded.nom_aseg,
        sal_imss = excluded.sal_imss, fec_alt = excluded.fec_alt, tip_tra = excluded.tip_tra, sem_jord = excluded.sem_jord,
        pag_info = excluded.pag_info";

fn parametros_patron(p: &Patron) -> [&String; 11] {
    [&p.reg_pat, &p.rfc_pat, &p.nom_pat, &p.act_pat, &p.dom_pat, &p.mun_pat, &p.cpp_pat, &p.ent_pat, &p.tel_pat, &p.zon_pat, &p.del_pat]
}

impl Base {
    /// Alta o cambio. Devuelve las advertencias (problemas no graves).
    pub fn guardar_patron(&self, patron: &Patron) -> Resultado<Vec<Problema>> {
        self.con_permiso(crate::datos::usuarios::Permiso::Capturar, "Guardó patrón", || patron.reg_pat.clone(), || self.guardar_patron_sin_control(patron))
    }

    fn guardar_patron_sin_control(&self, patron: &Patron) -> Resultado<Vec<Problema>> {
        let p = normalizar_patron(patron);
        let problemas = validar_patron(&p);
        rechazar_si_grave(&problemas)?;
        self.conn.execute(SQL_PATRON, rusqlite::params_from_iter(parametros_patron(&p)))?;
        Ok(problemas)
    }

    pub fn patrones(&self) -> Resultado<Vec<Patron>> {
        let mut stmt = self.conn.prepare(
            "SELECT reg_pat, rfc_pat, nom_pat, act_pat, dom_pat, mun_pat, cpp_pat, ent_pat, tel_pat, zon_pat, del_pat
             FROM patron ORDER BY nom_pat, reg_pat",
        )?;
        let filas = stmt.query_map([], |r| {
            Ok(Patron {
                reg_pat: r.get(0)?,
                rfc_pat: r.get(1)?,
                nom_pat: r.get(2)?,
                act_pat: r.get(3)?,
                dom_pat: r.get(4)?,
                mun_pat: r.get(5)?,
                cpp_pat: r.get(6)?,
                ent_pat: r.get(7)?,
                tel_pat: r.get(8)?,
                zon_pat: r.get(9)?,
                del_pat: r.get(10)?,
            })
        })?;
        Ok(filas.collect::<Result<_, _>>()?)
    }

    pub fn trabajadores_de(&self, reg_pat: &str) -> Resultado<i64> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM asegura WHERE reg_patr = ?1", [reg_pat], |r| r.get(0))?)
    }

    /// Solo se puede eliminar un patrón sin trabajadores.
    pub fn eliminar_patron(&self, reg_pat: &str) -> Resultado<()> {
        self.con_permiso(crate::datos::usuarios::Permiso::Eliminar, "Eliminó patrón", || reg_pat.to_string(), || self.eliminar_patron_sin_control(reg_pat))
    }

    fn eliminar_patron_sin_control(&self, reg_pat: &str) -> Resultado<()> {
        let n = self.trabajadores_de(reg_pat)?;
        if n > 0 {
            return Err(ErrorSua::Validacion(format!(
                "El patrón tiene {n} trabajadores registrados. Elimínelos antes, o haga un respaldo y depure."
            )));
        }
        let tx = self.transaccion()?;
        tx.execute("DELETE FROM patron_prima_rt WHERE reg_pat = ?1", [reg_pat])?;
        tx.execute("DELETE FROM patron WHERE reg_pat = ?1", [reg_pat])?;
        tx.commit()?;
        Ok(())
    }

    /// Registra la prima de RT vigente desde `AAAA-MM`, en cienmilésimas
    /// (7.58875 % = 758875).
    pub fn guardar_prima_rt(&self, reg_pat: &str, vigente_desde: &str, cienmilesimas: i64) -> Resultado<()> {
        self.con_permiso(crate::datos::usuarios::Permiso::Capturar, "Guardó prima de riesgos de trabajo", || format!("{reg_pat} desde {vigente_desde}"), || self.guardar_prima_rt_sin_control(reg_pat, vigente_desde, cienmilesimas))
    }

    fn guardar_prima_rt_sin_control(&self, reg_pat: &str, vigente_desde: &str, cienmilesimas: i64) -> Resultado<()> {
        self.conn.execute(
            "INSERT INTO patron_prima_rt (reg_pat, vigente_desde, prima_cienmilesimas) VALUES (?1, ?2, ?3)
             ON CONFLICT (reg_pat, vigente_desde) DO UPDATE SET prima_cienmilesimas = excluded.prima_cienmilesimas",
            params![reg_pat, vigente_desde, cienmilesimas],
        )?;
        Ok(())
    }

    /// Prima de RT más reciente del patrón: (vigente desde, cienmilésimas).
    pub fn prima_rt_vigente(&self, reg_pat: &str) -> Resultado<Option<(String, i64)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT vigente_desde, prima_cienmilesimas FROM patron_prima_rt WHERE reg_pat = ?1 ORDER BY vigente_desde DESC LIMIT 1",
                [reg_pat],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    pub fn guardar_trabajador(&self, trabajador: &Trabajador) -> Resultado<Vec<Problema>> {
        self.con_permiso(crate::datos::usuarios::Permiso::Capturar, "Guardó trabajador", || format!("{} {}", trabajador.reg_patr, trabajador.num_afil), || self.guardar_trabajador_sin_control(trabajador))
    }

    fn guardar_trabajador_sin_control(&self, trabajador: &Trabajador) -> Resultado<Vec<Problema>> {
        let t = normalizar_trabajador(trabajador);
        let problemas = validar_trabajador(&t);
        rechazar_si_grave(&problemas)?;
        let existe: i64 = self.conn.query_row("SELECT COUNT(*) FROM patron WHERE reg_pat = ?1", [&t.reg_patr], |r| r.get(0))?;
        if existe == 0 {
            return Err(ErrorSua::Validacion("Elija primero un patrón registrado.".into()));
        }
        self.conn.execute(
            SQL_TRABAJADOR,
            params![t.reg_patr, t.num_afil, t.curp, t.rfc_curp, t.nom_aseg, t.sal_imss, t.fec_alt, t.tip_tra, t.sem_jord, t.pag_info],
        )?;
        Ok(problemas)
    }

    /// Trabajadores de un patrón. `filtro` busca en NSS, nombre, RFC y CURP.
    pub fn trabajadores(&self, reg_pat: &str, filtro: &str) -> Resultado<Vec<Trabajador>> {
        let patron_like = format!("%{}%", filtro.trim().to_uppercase().replace('%', "").replace('_', ""));
        let mut stmt = self.conn.prepare(
            "SELECT reg_patr, num_afil, curp, rfc_curp, nom_aseg, sal_imss, fec_alt, tip_tra, sem_jord, pag_info
             FROM asegura
             WHERE reg_patr = ?1 AND (num_afil LIKE ?2 OR nom_aseg LIKE ?2 OR rfc_curp LIKE ?2 OR curp LIKE ?2)
             ORDER BY nom_aseg, num_afil",
        )?;
        let filas = stmt.query_map(params![reg_pat, patron_like], |r| {
            Ok(Trabajador {
                reg_patr: r.get(0)?,
                num_afil: r.get(1)?,
                curp: r.get(2)?,
                rfc_curp: r.get(3)?,
                nom_aseg: r.get(4)?,
                sal_imss: r.get(5)?,
                fec_alt: r.get(6)?,
                tip_tra: r.get(7)?,
                sem_jord: r.get(8)?,
                pag_info: r.get(9)?,
            })
        })?;
        Ok(filas.collect::<Result<_, _>>()?)
    }

    /// Número de crédito INFONAVIT de un trabajador, buscado por su llave
    /// (vacío si no tiene). `None` si el trabajador no existe. A diferencia de
    /// `trabajadores`, no recorre a todos los del patrón.
    pub fn credito_registrado(&self, reg_pat: &str, nss: &str) -> Resultado<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT pag_info FROM asegura WHERE reg_patr = ?1 AND num_afil = ?2", [reg_pat, nss], |r| r.get(0))
            .optional()?)
    }

    pub fn movimientos_de(&self, reg_pat: &str, nss: &str) -> Resultado<i64> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM movtos WHERE reg_patr = ?1 AND num_afil = ?2",
            [reg_pat, nss],
            |r| r.get(0),
        )?)
    }

    /// Solo se puede eliminar un trabajador sin movimientos ni incapacidades.
    pub fn eliminar_trabajador(&self, reg_pat: &str, nss: &str) -> Resultado<()> {
        self.con_permiso(crate::datos::usuarios::Permiso::Eliminar, "Eliminó trabajador", || format!("{reg_pat} {nss}"), || self.eliminar_trabajador_sin_control(reg_pat, nss))
    }

    fn eliminar_trabajador_sin_control(&self, reg_pat: &str, nss: &str) -> Resultado<()> {
        let movs = self.movimientos_de(reg_pat, nss)?;
        let incs: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM incapacidades WHERE reg_pat = ?1 AND num_afi = ?2",
            [reg_pat, nss],
            |r| r.get(0),
        )?;
        if movs + incs > 0 {
            return Err(ErrorSua::Validacion(format!(
                "El trabajador tiene {movs} movimientos y {incs} incapacidades registrados; no se puede eliminar."
            )));
        }
        self.conn.execute("DELETE FROM asegura WHERE reg_patr = ?1 AND num_afil = ?2", [reg_pat, nss])?;
        Ok(())
    }

    /// Carga en la base el patrón, los trabajadores y los movimientos de un
    /// archivo de pago ya leído. Todo o nada. No importa los importes: esos se
    /// recalculan. Repetir la importación no duplica movimientos.
    pub fn importar_pago(&self, a: &ArchivoPago) -> Resultado<ResumenImportacion> {
        self.con_permiso(crate::datos::usuarios::Permiso::Capturar, "Importó archivo de pago", || format!("periodo {}, {} trabajadores", a.patron.periodo, a.trabajadores.len()), || self.importar_pago_sin_control(a))
    }

    fn importar_pago_sin_control(&self, a: &ArchivoPago) -> Resultado<ResumenImportacion> {
        let p = &a.patron;
        // Letras y números solamente: el registro patronal da nombre a carpetas y
        // archivos, y un archivo manipulado podría traer aquí una ruta.
        if p.registro_patronal.len() != 11 || !p.registro_patronal.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err(ErrorSua::Validacion("El registro patronal del archivo no tiene 11 letras o números.".into()));
        }
        let mut resumen = ResumenImportacion::default();
        let tx = self.transaccion()?;

        let ya: i64 = tx.query_row("SELECT COUNT(*) FROM patron WHERE reg_pat = ?1", [&p.registro_patronal], |r| r.get(0))?;
        resumen.patron_nuevo = ya == 0;
        let recorta = |s: &str, max: usize| s.chars().take(max).collect::<String>();
        let patron = Patron {
            reg_pat: p.registro_patronal.clone(),
            rfc_pat: p.rfc.clone(),
            nom_pat: p.razon_social.clone(),
            act_pat: recorta(&p.actividad, 40),
            dom_pat: recorta(&p.calle_colonia, 50),
            mun_pat: recorta(&p.poblacion, 50),
            cpp_pat: if p.codigo_postal.len() == 5 { p.codigo_postal.clone() } else { String::new() },
            ent_pat: p.entidad.clone(),
            tel_pat: p.telefono_inf.clone(),
            zon_pat: p.area_geografica.clone(),
            del_pat: format!("{}{}", p.delegacion, p.subdelegacion),
        };
        tx.execute(SQL_PATRON, rusqlite::params_from_iter(parametros_patron(&patron)))?;
        // Solo dígitos: el texto viene de un archivo y una letra acentuada ocupa dos bytes.
        if p.fecha_prima_rt.len() == 6 && p.fecha_prima_rt.bytes().all(|b| b.is_ascii_digit()) && p.prima_rt > 0 {
            tx.execute(
                "INSERT INTO patron_prima_rt (reg_pat, vigente_desde, prima_cienmilesimas) VALUES (?1, ?2, ?3)
                 ON CONFLICT (reg_pat, vigente_desde) DO UPDATE SET prima_cienmilesimas = excluded.prima_cienmilesimas",
                params![patron.reg_pat, format!("{}-{}", &p.fecha_prima_rt[..4], &p.fecha_prima_rt[4..]), p.prima_rt],
            )?;
        }

        for x in &a.trabajadores {
            let t = Trabajador {
                reg_patr: patron.reg_pat.clone(),
                num_afil: x.nss.clone(),
                curp: if x.curp.len() == 18 { x.curp.clone() } else { String::new() },
                rfc_curp: if matches!(x.rfc.len(), 10 | 13) { x.rfc.clone() } else { String::new() },
                nom_aseg: x.nombre.clone(),
                sal_imss: x.sdi,
                fec_alt: String::new(),
                tip_tra: x.tipo_trabajador.clone(),
                sem_jord: x.jornada_reducida.clone(),
                pag_info: if x.credito_infonavit_inf.len() == 10 { x.credito_infonavit_inf.clone() } else { String::new() },
            };
            if t.num_afil.len() != 11 {
                return Err(ErrorSua::Validacion("Hay un trabajador cuyo NSS no tiene 11 caracteres; no se importó nada.".into()));
            }
            if !validar_trabajador(&t).is_empty() {
                resumen.con_advertencias += 1;
            }
            let existe: i64 = tx.query_row(
                "SELECT COUNT(*) FROM asegura WHERE reg_patr = ?1 AND num_afil = ?2",
                [&t.reg_patr, &t.num_afil],
                |r| r.get(0),
            )?;
            if existe == 0 {
                resumen.trabajadores_nuevos += 1;
            } else {
                resumen.trabajadores_actualizados += 1;
            }
            // Si ya existía se conserva su fecha de alta capturada.
            tx.execute(
                "INSERT INTO asegura (reg_patr, num_afil, curp, rfc_curp, nom_aseg, sal_imss, fec_alt, tip_tra, sem_jord, pag_info)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT (reg_patr, num_afil) DO UPDATE SET curp = excluded.curp, rfc_curp = excluded.rfc_curp,
                    nom_aseg = excluded.nom_aseg, sal_imss = excluded.sal_imss, tip_tra = excluded.tip_tra,
                    sem_jord = excluded.sem_jord, pag_info = excluded.pag_info",
                params![t.reg_patr, t.num_afil, t.curp, t.rfc_curp, t.nom_aseg, t.sal_imss, t.fec_alt, t.tip_tra, t.sem_jord, t.pag_info],
            )?;
        }

        for m in &a.movimientos {
            let fecha = fecha_compacta_a_iso(&m.fecha);
            if fecha.is_empty() || m.tipo.len() != 2 {
                return Err(ErrorSua::Validacion("Hay un movimiento con fecha o tipo inválidos; no se importó nada.".into()));
            }
            let existe: i64 = tx.query_row(
                "SELECT COUNT(*) FROM movtos WHERE reg_patr = ?1 AND num_afil = ?2 AND tip_movs = ?3 AND fec_inic = ?4",
                params![patron.reg_pat, m.nss, m.tipo, fecha],
                |r| r.get(0),
            )?;
            if existe > 0 {
                resumen.movimientos_ya_existentes += 1;
                continue;
            }
            tx.execute(
                "INSERT INTO movtos (reg_patr, num_afil, tip_movs, fec_inic, num_dias, sal_movt, fol_inc, mes_ano)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![patron.reg_pat, m.nss, m.tipo, fecha, m.dias_inf, m.sdi, m.folio_incapacidad_inf, p.periodo],
            )?;
            resumen.movimientos_nuevos += 1;
        }
        tx.commit()?;
        Ok(resumen)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::config::{CarpetaTemporal, Modo};

    fn base(t: &CarpetaTemporal) -> Base {
        Base::abrir_en(&t.0.join("sua.db"), &t.0.join("respaldos"), Modo::Portable, 3).unwrap()
    }

    // Datos ficticios. El NSS lleva dígito de Luhn válido.
    fn patron() -> Patron {
        Patron {
            reg_pat: "z9900000109".into(),
            rfc_pat: "xaxx010101000".into(),
            nom_pat: "  patrón ficticio de prueba ".into(),
            cpp_pat: "06000".into(),
            ..Patron::default()
        }
    }

    fn nss_valido(base10: &str) -> String {
        format!("{base10}{}", validar::luhn(base10).unwrap())
    }

    fn trabajador() -> Trabajador {
        Trabajador {
            reg_patr: "Z9900000109".into(),
            num_afil: nss_valido("9000000001"),
            nom_aseg: "ficticio$de prueba$uno".into(),
            sal_imss: 31_504,
            ..Trabajador::default()
        }
    }

    #[test]
    fn importes_como_texto() {
        for (texto, centavos) in [("315.04", 31_504), ("100", 10_000), ("2,932.75", 293_275), ("0.5", 50), (" 7 ", 700)] {
            assert_eq!(texto_a_centavos(texto), Some(centavos), "{texto}");
        }
        for malo in ["", "abc", "1.234", "-5", "1.2.3", "."] {
            assert_eq!(texto_a_centavos(malo), None, "{malo}");
        }
        assert_eq!(centavos_a_texto(31_504), "315.04");
        assert_eq!(centavos_a_texto(5), "0.05");
    }

    #[test]
    fn fechas() {
        assert!(es_fecha_iso("2024-02-29"));
        assert!(!es_fecha_iso("2026-02-29"));
        assert!(!es_fecha_iso("2026-13-01"));
        assert!(!es_fecha_iso("20260101"));
        assert_eq!(fecha_compacta_a_iso("20080218"), "2008-02-18");
        assert_eq!(fecha_compacta_a_iso("20081301"), "");
    }

    #[test]
    fn patron_alta_cambio_y_baja() {
        let t = CarpetaTemporal::nueva("cap1");
        let b = base(&t);
        assert!(b.guardar_patron(&patron()).unwrap().is_empty());
        let lista = b.patrones().unwrap();
        assert_eq!(lista.len(), 1);
        // Se normaliza a mayúsculas y sin espacios sobrantes.
        assert_eq!(lista[0].reg_pat, "Z9900000109");
        assert_eq!(lista[0].nom_pat, "PATRÓN FICTICIO DE PRUEBA");

        let mut cambio = patron();
        cambio.nom_pat = "Otro nombre".into();
        b.guardar_patron(&cambio).unwrap();
        let lista = b.patrones().unwrap();
        assert_eq!((lista.len(), lista[0].nom_pat.as_str()), (1, "OTRO NOMBRE"));

        b.guardar_prima_rt("Z9900000109", "2025-03", 758_875).unwrap();
        b.guardar_prima_rt("Z9900000109", "2026-03", 500_000).unwrap();
        assert_eq!(b.prima_rt_vigente("Z9900000109").unwrap(), Some(("2026-03".into(), 500_000)));

        b.eliminar_patron("Z9900000109").unwrap();
        assert!(b.patrones().unwrap().is_empty());
    }

    #[test]
    fn patron_invalido_no_se_guarda() {
        let t = CarpetaTemporal::nueva("cap2");
        let b = base(&t);
        let mut p = patron();
        p.reg_pat = "CORTO".into();
        p.nom_pat = String::new();
        p.cpp_pat = "123".into();
        let Err(ErrorSua::Validacion(m)) = b.guardar_patron(&p) else { panic!("debió rechazarse") };
        for esperado in ["Registro patronal", "Nombre o razón social", "Código postal"] {
            assert!(m.contains(esperado), "falta {esperado} en {m}");
        }
        assert!(b.patrones().unwrap().is_empty());
    }

    #[test]
    fn rfc_con_digito_incorrecto_es_advertencia_no_rechazo() {
        let p = Patron { rfc_pat: "GODE561231GR9".into(), ..patron() };
        let problemas = validar_patron(&normalizar_patron(&p));
        // Con el dígito correcto no hay problemas; con otro, solo advertencia.
        let esperado = validar::digito_rfc(&"GODE561231GR".chars().collect::<Vec<_>>()).unwrap();
        if esperado == '9' {
            assert!(problemas.is_empty());
        } else {
            assert!(problemas.iter().all(|x| !x.grave) && problemas.len() == 1, "{problemas:?}");
        }
    }

    #[test]
    fn trabajador_alta_busqueda_y_baja() {
        let t = CarpetaTemporal::nueva("cap3");
        let b = base(&t);
        // Sin patrón registrado no se acepta.
        assert!(matches!(b.guardar_trabajador(&trabajador()), Err(ErrorSua::Validacion(_))));
        b.guardar_patron(&patron()).unwrap();
        b.guardar_trabajador(&trabajador()).unwrap();
        let mut otro = trabajador();
        otro.num_afil = nss_valido("9000000002");
        otro.nom_aseg = "Segundo$$dos".into();
        b.guardar_trabajador(&otro).unwrap();

        assert_eq!(b.trabajadores("Z9900000109", "").unwrap().len(), 2);
        let hallados = b.trabajadores("Z9900000109", "segundo").unwrap();
        assert_eq!(hallados.len(), 1);
        assert_eq!(hallados[0].nom_aseg, "SEGUNDO$$DOS");
        assert_eq!(b.trabajadores("Z9900000109", "%").unwrap().len(), 2);

        // Un patrón con trabajadores no se puede eliminar.
        assert!(matches!(b.eliminar_patron("Z9900000109"), Err(ErrorSua::Validacion(_))));
        b.eliminar_trabajador("Z9900000109", &otro.num_afil).unwrap();
        assert_eq!(b.trabajadores("Z9900000109", "").unwrap().len(), 1);
    }

    #[test]
    fn trabajador_invalido_no_se_guarda() {
        let t = CarpetaTemporal::nueva("cap4");
        let b = base(&t);
        b.guardar_patron(&patron()).unwrap();
        let mut x = trabajador();
        let correcto = x.num_afil.clone();
        // Dígito verificador del NSS alterado.
        let dv = if correcto.ends_with('0') { '1' } else { '0' };
        x.num_afil = format!("{}{dv}", &correcto[..10]);
        x.nom_aseg = "SOLO UN CAMPO".into();
        x.sal_imss = 0;
        x.pag_info = "123".into();
        x.fec_alt = "31/12/2025".into();
        let Err(ErrorSua::Validacion(m)) = b.guardar_trabajador(&x) else { panic!("debió rechazarse") };
        for esperado in ["NSS", "Nombre", "Salario", "Crédito", "Fecha de alta"] {
            assert!(m.contains(esperado), "falta {esperado} en {m}");
        }
        assert!(b.trabajadores("Z9900000109", "").unwrap().is_empty());
    }

    #[test]
    fn importar_archivo_de_pago_es_repetible_y_no_duplica() {
        let t = CarpetaTemporal::nueva("cap5");
        let b = base(&t);
        let archivo = crate::io::pago::leer(&crate::io::pago::pruebas::ejemplo()).unwrap();

        let r = b.importar_pago(&archivo).unwrap();
        assert!(r.patron_nuevo);
        assert_eq!((r.trabajadores_nuevos, r.trabajadores_actualizados), (2, 0));
        assert_eq!((r.movimientos_nuevos, r.movimientos_ya_existentes), (2, 0));

        let patrones = b.patrones().unwrap();
        assert_eq!(patrones.len(), 1);
        assert_eq!(patrones[0].del_pat, "3112");
        assert_eq!(b.prima_rt_vigente(&patrones[0].reg_pat).unwrap(), Some(("2025-03".into(), 758_880)));
        let trabajadores = b.trabajadores(&patrones[0].reg_pat, "").unwrap();
        assert_eq!(trabajadores.len(), 2);
        assert_eq!(trabajadores[0].sal_imss, 10_000);
        assert_eq!(b.movimientos_de(&patrones[0].reg_pat, &trabajadores[0].num_afil).unwrap(), 1);

        // Segunda vez: nada nuevo, nada duplicado.
        let r = b.importar_pago(&archivo).unwrap();
        assert!(!r.patron_nuevo);
        assert_eq!((r.trabajadores_nuevos, r.trabajadores_actualizados), (0, 2));
        assert_eq!((r.movimientos_nuevos, r.movimientos_ya_existentes), (0, 2));

        // Con movimientos, el trabajador ya no se puede eliminar.
        assert!(matches!(b.eliminar_trabajador(&patrones[0].reg_pat, &trabajadores[0].num_afil), Err(ErrorSua::Validacion(_))));
    }
}
