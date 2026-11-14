//! Importación y exportación de los TXT de ancho fijo del SUA
//! (`ESPECIFICACION_SUA.md`, §7.1 y §7.2; layouts en `Revision_Manual_SUA.html`).
//!
//! Cuatro clases de archivo: patrones (313 caracteres), trabajadores (168),
//! movimientos (49) y movimientos de crédito INFONAVIT (52). Texto en
//! Windows‑1252 con fin de renglón CRLF.
//!
//! El layout de trabajadores está comprobado con dos exportaciones reales del
//! SUA, de 164 y de 167 caracteres: el importador acepta de 164 a 168 y
//! completa lo que falta. Los otros tres salen del manual, sin archivo real.
//!
//! Además, datos afiliatorios (80) e incapacidades con sus datos médicos
//! (57), también del manual. No se maneja la aportación voluntaria
//! (movimiento 09).

use crate::datos::captura::{Patron, Trabajador};
use crate::datos::expediente::{Afiliacion, DatosIncapacidad};
use crate::datos::periodo::MovimientoGuardado;
use crate::datos::Base;
use crate::dominio::credito::{creditos, MovCredito};
use crate::dominio::tramos::dia_de_iso;
use crate::error::{ErrorSua, Resultado};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clase {
    Patrones,
    Trabajadores,
    Movimientos,
    Creditos,
    Afiliatorios,
    Incapacidades,
}

impl Clase {
    pub const TODAS: [Clase; 6] = [Clase::Patrones, Clase::Trabajadores, Clase::Movimientos, Clase::Creditos, Clase::Afiliatorios, Clase::Incapacidades];

    pub fn nombre(self) -> &'static str {
        match self {
            Clase::Patrones => "Patrones",
            Clase::Trabajadores => "Trabajadores",
            Clase::Movimientos => "Movimientos",
            Clase::Creditos => "Créditos INFONAVIT",
            Clase::Afiliatorios => "Datos afiliatorios",
            Clase::Incapacidades => "Incapacidades",
        }
    }

    /// Nombre con que se exporta.
    pub fn archivo(self) -> &'static str {
        match self {
            Clase::Patrones => "Patron.txt",
            Clase::Trabajadores => "Aseg.txt",
            Clase::Movimientos => "Movt.txt",
            Clase::Creditos => "Cred.txt",
            Clase::Afiliatorios => "Afil.txt",
            Clase::Incapacidades => "Incap.txt",
        }
    }

    /// Largo del registro completo.
    pub fn largo(self) -> usize {
        match self {
            Clase::Patrones => 313,
            Clase::Trabajadores => 168,
            Clase::Movimientos => 49,
            Clase::Creditos => 52,
            Clase::Afiliatorios => 80,
            Clase::Incapacidades => 57,
        }
    }

    /// Largo mínimo aceptado al importar: hasta el último campo obligatorio.
    fn largo_minimo(self) -> usize {
        match self {
            Clase::Patrones => 242,
            Clase::Trabajadores => 164,
            Clase::Movimientos => 32,
            Clase::Creditos => 42,
            Clase::Afiliatorios => 22,
            Clase::Incapacidades => 49,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResumenTxt {
    /// Renglones con contenido.
    pub procesados: usize,
    pub nuevos: usize,
    pub actualizados: usize,
    /// Movimientos que ya estaban registrados (mismo tipo y fecha).
    pub ya_existian: usize,
    /// (número de renglón, motivo). El motivo no lleva datos personales.
    pub rechazados: Vec<(usize, String)>,
    /// Avisos sobre renglones que sí se aceptaron.
    pub avisos: Vec<(usize, String)>,
}

// ------------------------------------------------------------------ texto

fn decodificar(datos: &[u8]) -> String {
    // Windows‑1252 leído como Latin‑1: coinciden en todas las letras del español.
    datos.iter().map(|b| *b as char).collect()
}

fn codificar(texto: &str) -> Vec<u8> {
    texto.chars().map(|c| if (c as u32) < 256 { c as u32 as u8 } else { b'?' }).collect()
}

/// Un renglón como caracteres, completado con espacios hasta `largo`.
struct Renglon(Vec<char>);

impl Renglon {
    /// Campo que empieza en la posición `desde` (contada desde 1), sin espacios a los lados.
    fn campo(&self, desde: usize, largo: usize) -> String {
        self.0[desde - 1..desde - 1 + largo].iter().collect::<String>().trim().to_string()
    }

    /// Campo numérico: vacío o solo ceros cuenta como ausente.
    fn numero(&self, desde: usize, largo: usize) -> Result<Option<i64>, String> {
        let t = self.campo(desde, largo);
        if t.is_empty() {
            return Ok(None);
        }
        match t.parse::<i64>() {
            Ok(0) => Ok(None),
            Ok(v) if t.bytes().all(|b| b.is_ascii_digit()) => Ok(Some(v)),
            _ => Err(format!("las posiciones {desde} a {} deben ser numéricas", desde + largo - 1)),
        }
    }

    /// Fecha `ddmmaaaa` como `AAAA-MM-DD`; en blanco o en ceros, vacío.
    fn fecha(&self, desde: usize) -> Result<String, String> {
        let t = self.campo(desde, 8);
        if t.is_empty() || t == "00000000" {
            return Ok(String::new());
        }
        let iso = if t.len() == 8 && t.is_ascii() { format!("{}-{}-{}", &t[4..], &t[2..4], &t[..2]) } else { String::new() };
        if crate::datos::captura::es_fecha_iso(&iso) {
            Ok(iso)
        } else {
            Err(format!("fecha inválida en las posiciones {desde} a {} (se espera ddmmaaaa)", desde + 7))
        }
    }
}

/// Texto alineado a la izquierda, recortado o completado con espacios.
fn alfa(texto: &str, largo: usize) -> String {
    let mut s: String = texto.chars().take(largo).collect();
    while s.chars().count() < largo {
        s.push(' ');
    }
    s
}

/// Número con ceros a la izquierda. Si no cabe, se queda con las últimas cifras.
fn num(valor: i64, largo: usize) -> String {
    let t = format!("{:0>largo$}", valor.max(0));
    t[t.len() - largo..].to_string()
}

/// Solo los dígitos del texto, con ceros a la izquierda.
fn digitos(texto: &str, largo: usize) -> String {
    let d: String = texto.chars().filter(char::is_ascii_digit).collect();
    num(d.parse().unwrap_or(0), largo)
}

/// `AAAA-MM-DD` → `ddmmaaaa`; si no es fecha, ocho ceros.
fn fecha_txt(iso: &str) -> String {
    if crate::datos::captura::es_fecha_iso(iso) {
        format!("{}{}{}", &iso[8..], &iso[5..7], &iso[..4])
    } else {
        "00000000".to_string()
    }
}

/// Valor del descuento en diezmilésimas ↔ 8 dígitos del archivo. El
/// porcentaje y el factor van con cuatro decimales; la cuota fija, con tres.
fn valor_de_archivo(tipo_descuento: u8, valor: i64) -> i64 {
    if tipo_descuento == 2 {
        valor * 10
    } else {
        valor
    }
}

fn valor_a_archivo(tipo_descuento: u8, valor: i64) -> i64 {
    if tipo_descuento == 2 {
        valor / 10
    } else {
        valor
    }
}

fn motivo(e: ErrorSua) -> String {
    e.to_string().replace('\n', "; ")
}

// ------------------------------------------------------------ importación

impl Base {
    /// Importa un TXT del SUA. Cada renglón se valida por separado: los que no
    /// pasan quedan en `rechazados` y no detienen a los demás. Los movimientos
    /// se aplican en el orden del archivo. Todo el archivo se confirma en disco
    /// de una sola vez (con un guardado por renglón, 5 000 trabajadores
    /// tardaban 45 segundos); si el programa se interrumpe a la mitad, no queda
    /// nada del archivo.
    pub fn importar_txt(&self, clase: Clase, datos: &[u8]) -> Resultado<ResumenTxt> {
        self.con_permiso(crate::datos::usuarios::Permiso::Capturar, "Importó TXT", || clase.nombre().to_string(), || self.importar_txt_sin_control(clase, datos))
    }

    fn importar_txt_sin_control(&self, clase: Clase, datos: &[u8]) -> Resultado<ResumenTxt> {
        let tx = self.transaccion()?;
        let texto = decodificar(datos);
        let mut r = ResumenTxt::default();
        for (i, crudo) in texto.split('\n').enumerate() {
            let linea = crudo.trim_end_matches(['\r', '\u{1a}']);
            if linea.trim().is_empty() {
                continue;
            }
            r.procesados += 1;
            let n = i + 1;
            let mut caracteres: Vec<char> = linea.chars().collect();
            if caracteres.len() < clase.largo_minimo() || caracteres.len() > clase.largo() {
                r.rechazados.push((n, format!("el renglón mide {} caracteres; se esperan {}", caracteres.len(), clase.largo())));
                continue;
            }
            caracteres.resize(clase.largo(), ' ');
            let renglon = Renglon(caracteres);
            let resultado = match clase {
                Clase::Patrones => self.importar_patron(&renglon, &mut r),
                Clase::Trabajadores => self.importar_trabajador(&renglon, n, &mut r),
                Clase::Movimientos => self.importar_movimiento(&renglon, &mut r),
                Clase::Creditos => self.importar_credito(&renglon, &mut r),
                Clase::Afiliatorios => self.importar_afiliacion(&renglon, &mut r),
                Clase::Incapacidades => self.importar_incapacidad(&renglon, &mut r),
            };
            if let Err(m) = resultado {
                r.rechazados.push((n, m));
            }
        }
        tx.commit()?;
        Ok(r)
    }

    fn importar_patron(&self, l: &Renglon, r: &mut ResumenTxt) -> Result<(), String> {
        // En el layout de patrones la Ñ viaja como diagonal.
        let texto = |desde, largo| l.campo(desde, largo).replace('/', "Ñ");
        let entidad = l.numero(220, 2)?;
        let p = Patron {
            reg_pat: l.campo(1, 11),
            rfc_pat: l.campo(12, 13),
            nom_pat: texto(25, 50),
            act_pat: texto(75, 40),
            dom_pat: texto(115, 50),
            mun_pat: texto(165, 50),
            cpp_pat: l.numero(215, 5)?.map(|v| num(v, 5)).unwrap_or_default(),
            ent_pat: entidad.map(|v| num(v, 2)).unwrap_or_default(),
            tel_pat: l.numero(222, 15)?.map(|v| v.to_string()).unwrap_or_default(),
            zon_pat: l.campo(238, 1),
            del_pat: l.numero(239, 4)?.map(|v| num(v, 4)).unwrap_or_default(),
        };
        // Prima de RT: fecha aaaamm y factor con cinco decimales.
        let fecha_prima = l.campo(243, 6);
        let prima = match (l.numero(243, 6)?, l.numero(249, 7)?) {
            (Some(_), Some(factor)) if fecha_prima.len() == 6 && fecha_prima[4..].parse::<u32>().is_ok_and(|m| (1..=12).contains(&m)) => Some((format!("{}-{}", &fecha_prima[..4], &fecha_prima[4..]), factor)),
            (None, None) => None,
            _ => return Err("prima de riesgos de trabajo: la fecha (aaaamm) o el factor no son válidos".into()),
        };
        let existia = self.patrones().map_err(motivo)?.iter().any(|x| x.reg_pat == p.reg_pat.to_uppercase());
        self.guardar_patron(&p).map_err(motivo)?;
        if let Some((desde, factor)) = prima {
            self.guardar_prima_rt(&p.reg_pat.to_uppercase(), &desde, factor).map_err(motivo)?;
        }
        if existia {
            r.actualizados += 1;
        } else {
            r.nuevos += 1;
        }
        Ok(())
    }

    fn importar_trabajador(&self, l: &Renglon, n: usize, r: &mut ResumenTxt) -> Result<(), String> {
        let (reg, nss) = (l.campo(1, 11).to_uppercase(), l.campo(12, 11));
        let t = Trabajador {
            reg_patr: reg.clone(),
            num_afil: nss.clone(),
            rfc_curp: l.campo(23, 13),
            curp: l.campo(36, 18),
            nom_aseg: l.campo(54, 50),
            tip_tra: l.campo(104, 1),
            sem_jord: l.campo(105, 1),
            fec_alt: l.fecha(106)?,
            sal_imss: l.numero(114, 7)?.ok_or("el salario diario integrado debe ser mayor que cero")?,
            pag_info: String::new(),
        };
        // Crédito INFONAVIT: número, fecha de inicio, tipo y valor del descuento.
        let credito = l.campo(138, 10);
        let con_credito = !credito.is_empty() && credito.bytes().any(|b| b != b'0');
        let descuento = if con_credito { Some((l.fecha(148)?, l.numero(156, 1)?.unwrap_or(0) as u8, l.numero(157, 8)?.unwrap_or(0))) } else { None };

        let existentes = self.movimientos(&reg, &nss).map_err(motivo)?;
        // Al actualizar no se pierde el número de crédito que ya tuviera.
        let registrado = self.credito_registrado(&reg, &nss).map_err(motivo)?;
        let existia = registrado.is_some();
        let previo = registrado.unwrap_or_default();
        self.guardar_trabajador(&Trabajador { pag_info: previo, ..t.clone() }).map_err(motivo)?;
        if existia {
            r.actualizados += 1;
        } else {
            r.nuevos += 1;
        }
        // Un trabajador sin movimientos queda dado de alta en su fecha de alta.
        if existentes.is_empty() && !t.fec_alt.is_empty() {
            if let Err(e) = self.guardar_movimiento(&reg, &nss, "01", &t.fec_alt, t.sal_imss) {
                r.avisos.push((n, format!("trabajador guardado, pero no se registró su alta: {}", motivo(e))));
            }
        }
        if let Some((inicio, tipo_descuento, valor)) = descuento {
            let ya_tiene = existentes.iter().any(|m| matches!(m.tipo.as_str(), "15" | "16" | "17" | "18" | "19" | "20"));
            if !ya_tiene {
                let alta = self.guardar_credito(&reg, &nss, "15", &inicio, &credito, tipo_descuento, valor_de_archivo(tipo_descuento, valor));
                if let Err(e) = alta {
                    r.avisos.push((n, format!("trabajador guardado, pero no se registró su crédito: {}", motivo(e))));
                }
            }
        }
        Ok(())
    }

    fn importar_movimiento(&self, l: &Renglon, r: &mut ResumenTxt) -> Result<(), String> {
        let (reg, nss, tipo) = (l.campo(1, 11).to_uppercase(), l.campo(12, 11), l.campo(23, 2));
        let fecha = l.fecha(25)?;
        if fecha.is_empty() {
            return Err("falta la fecha del movimiento".into());
        }
        if self.movimientos(&reg, &nss).map_err(motivo)?.iter().any(|m| m.tipo == tipo && m.fecha == fecha) {
            r.ya_existian += 1;
            return Ok(());
        }
        let dias = l.numero(41, 2)?.unwrap_or(0) as u32;
        let sdi = l.numero(43, 7)?.unwrap_or(0);
        match tipo.as_str() {
            "02" => self.guardar_movimiento(&reg, &nss, "02", &fecha, 0),
            "07" | "08" => self.guardar_movimiento(&reg, &nss, &tipo, &fecha, sdi),
            "11" => self.guardar_incidencia(&reg, &nss, "11", &fecha, dias, ""),
            "12" => self.guardar_incidencia(&reg, &nss, "12", &fecha, dias, &l.campo(33, 8)),
            "09" => return Err("la aportación voluntaria (09) no se maneja".into()),
            _ => return Err("tipo de movimiento no admitido en este archivo: use 02, 07, 08, 11 o 12".into()),
        }
        .map_err(motivo)?;
        r.nuevos += 1;
        Ok(())
    }

    fn importar_credito(&self, l: &Renglon, r: &mut ResumenTxt) -> Result<(), String> {
        let (reg, nss, tipo) = (l.campo(1, 11).to_uppercase(), l.campo(12, 11), l.campo(33, 2));
        let fecha = l.fecha(35)?;
        if fecha.is_empty() {
            return Err("falta la fecha del movimiento".into());
        }
        let tipo_descuento = l.numero(43, 1)?.unwrap_or(0) as u8;
        let valor = valor_de_archivo(tipo_descuento, l.numero(44, 8)?.unwrap_or(0));
        if let Some(m) = self.movimientos(&reg, &nss).map_err(motivo)?.iter().find(|m| m.tipo == tipo && m.fecha == fecha) {
            // El inicio de crédito pudo entrar antes con el archivo de trabajadores, que solo
            // trae los datos vigentes: este archivo es el que manda.
            if tipo == "15" {
                let numero = l.campo(23, 10);
                if !crate::dominio::validar::credito_infonavit(&numero).formato_valido {
                    return Err("Número de crédito: deben ser 10 dígitos, distintos de cero.".into());
                }
                crate::dominio::credito::validar_descuento(tipo_descuento, valor)?;
                self.conexion()
                    .execute("UPDATE movtos SET num_cre = ?2, tip_des = ?3, val_des = ?4 WHERE id = ?1", rusqlite::params![m.id, numero, tipo_descuento.to_string(), valor])
                    .map_err(|e| motivo(e.into()))?;
                r.actualizados += 1;
            } else {
                r.ya_existian += 1;
            }
            return Ok(());
        }
        if l.campo(52, 1).eq_ignore_ascii_case("S") {
            r.avisos.push((0, "hay créditos marcados con tabla de disminución de porcentaje: esa tabla no se aplica en el cálculo".into()));
            r.avisos.dedup();
        }
        self.guardar_credito(&reg, &nss, &tipo, &fecha, &l.campo(23, 10), tipo_descuento, valor).map_err(motivo)?;
        r.nuevos += 1;
        Ok(())
    }

    fn importar_afiliacion(&self, l: &Renglon, r: &mut ResumenTxt) -> Result<(), String> {
        let (reg, nss) = (l.campo(1, 11).to_uppercase(), l.campo(12, 11));
        let a = Afiliacion {
            reg_patr: reg.clone(),
            num_afil: nss.clone(),
            cpp_trab: l.numero(23, 5)?.map(|v| num(v, 5)).unwrap_or_default(),
            fec_nac: l.fecha(28)?,
            lug_nac: l.campo(36, 25),
            ent_trab: l.numero(61, 2)?.map(|v| num(v, 2)).unwrap_or_default(),
            umf_trab: l.numero(63, 3)?.map(|v| v.to_string()).unwrap_or_default(),
            ocupa: l.campo(66, 12),
            sexo: l.campo(78, 1),
            tip_sal: l.campo(79, 1),
            jor_hor: l.campo(80, 1),
        };
        let existia = self.afiliacion(&reg, &nss).map_err(motivo)?.is_some();
        self.guardar_afiliacion(&a).map_err(motivo)?;
        if existia {
            r.actualizados += 1;
        } else {
            r.nuevos += 1;
        }
        Ok(())
    }

    fn importar_incapacidad(&self, l: &Renglon, r: &mut ResumenTxt) -> Result<(), String> {
        let (reg, nss) = (l.campo(1, 11).to_uppercase(), l.campo(12, 11));
        let fecha = l.fecha(24)?;
        let folio = l.campo(32, 8).to_uppercase();
        let dias = l.numero(40, 3)?.unwrap_or(0);
        let codigo = |desde: usize, largo: usize| -> Result<u8, String> { Ok(l.numero(desde, largo)?.unwrap_or(0).min(255) as u8) };
        let datos = DatosIncapacidad { porcentaje: codigo(43, 3)?, ramo: codigo(46, 1)?, tipo_riesgo: codigo(47, 1)?, secuela: codigo(48, 1)?, control: codigo(49, 1)? };
        crate::datos::expediente::validar_datos_de_incapacidad(&datos).map_err(motivo)?;
        // La incapacidad pudo entrar antes con el archivo de movimientos.
        let existia = self.movimientos(&reg, &nss).map_err(motivo)?.iter().any(|m| m.tipo == "12" && m.folio == folio);
        if !existia {
            if fecha.is_empty() {
                return Err("falta la fecha de inicio de la incapacidad".into());
            }
            if dias > 99 {
                return Err("los días subsidiados pasan de 99: registre la incapacidad por certificado".into());
            }
            self.guardar_incidencia(&reg, &nss, "12", &fecha, dias as u32, &folio).map_err(motivo)?;
        }
        self.guardar_datos_de_incapacidad(&reg, &nss, &folio, &datos).map_err(motivo)?;
        if existia {
            r.actualizados += 1;
        } else {
            r.nuevos += 1;
        }
        Ok(())
    }

    // -------------------------------------------------------- exportación

    /// Genera el TXT de una clase para un patrón. Devuelve los bytes del
    /// archivo (Windows‑1252, CRLF) y el número de renglones.
    pub fn exportar_txt(&self, clase: Clase, reg_pat: &str) -> Resultado<(Vec<u8>, usize)> {
        self.con_permiso(crate::datos::usuarios::Permiso::Exportar, "Exportó TXT", || format!("{} de {reg_pat}", clase.nombre()), || self.exportar_txt_sin_control(clase, reg_pat))
    }

    fn exportar_txt_sin_control(&self, clase: Clase, reg_pat: &str) -> Resultado<(Vec<u8>, usize)> {
        let mut renglones: Vec<String> = Vec::new();
        match clase {
            Clase::Patrones => {
                let p = self
                    .patrones()?
                    .into_iter()
                    .find(|p| p.reg_pat == reg_pat)
                    .ok_or_else(|| ErrorSua::Validacion("Elija primero un patrón registrado.".into()))?;
                let prima = self.prima_rt_vigente(reg_pat)?;
                let texto = |s: &str, largo| alfa(&s.replace('Ñ', "/"), largo);
                renglones.push(
                    [
                        alfa(&p.reg_pat, 11),
                        alfa(&p.rfc_pat, 13),
                        texto(&p.nom_pat, 50),
                        texto(&p.act_pat, 40),
                        texto(&p.dom_pat, 50),
                        texto(&p.mun_pat, 50),
                        digitos(&p.cpp_pat, 5),
                        digitos(&p.ent_pat, 2),
                        digitos(&p.tel_pat, 15),
                        "0".to_string(), // reembolso de subsidios: no se captura
                        alfa(&p.zon_pat, 1),
                        digitos(&p.del_pat, 4),
                        prima.as_ref().map_or("000000".to_string(), |(desde, _)| digitos(desde, 6)),
                        num(prima.map_or(0, |(_, v)| v), 7),
                        alfa("", 50), // representante legal, clase, fracción y STyPS: no se capturan
                        alfa("", 3),
                        "0000".to_string(),
                        "0".to_string(),
                    ]
                    .concat(),
                );
            }
            Clase::Trabajadores => {
                for t in self.trabajadores(reg_pat, "")? {
                    let movs = self.movimientos(reg_pat, &t.num_afil)?;
                    let de_credito: Vec<MovCredito> = movs.iter().filter_map(a_credito).collect();
                    let vigente = creditos(&de_credito, i64::MAX / 4).vigente;
                    let inicio = movs.iter().filter(|m| m.tipo == "15").map(|m| m.fecha.as_str()).next_back().unwrap_or("");
                    let credito = match &vigente {
                        // Sin crédito, como exporta el SUA: fecha y tipo en blanco, valor en ceros.
                        None => [alfa("", 10), alfa("", 8), " ".to_string(), num(0, 8)].concat(),
                        Some(d) => [alfa(&d.numero, 10), fecha_txt(inicio), d.tipo_descuento.to_string(), num(valor_a_archivo(d.tipo_descuento, d.valor), 8)].concat(),
                    };
                    renglones.push(
                        [
                            alfa(&t.reg_patr, 11),
                            alfa(&t.num_afil, 11),
                            alfa(&t.rfc_curp, 13),
                            alfa(&t.curp, 18),
                            alfa(&t.nom_aseg, 50),
                            alfa(if t.tip_tra.is_empty() { "1" } else { &t.tip_tra }, 1),
                            alfa(if t.sem_jord.is_empty() { "0" } else { &t.sem_jord }, 1),
                            fecha_txt(&t.fec_alt),
                            num(t.sal_imss, 7),
                            alfa("", 17), // clave de ubicación: no se captura
                            credito,
                            "0".to_string(), // tipo de pensión: no se captura
                            alfa("", 3),     // clave de municipio: no se captura
                        ]
                        .concat(),
                    );
                }
            }
            Clase::Movimientos => {
                for t in self.trabajadores(reg_pat, "")? {
                    for m in self.movimientos(reg_pat, &t.num_afil)? {
                        if !matches!(m.tipo.as_str(), "02" | "07" | "08" | "11" | "12") {
                            continue;
                        }
                        renglones.push(
                            [alfa(reg_pat, 11), alfa(&t.num_afil, 11), alfa(&m.tipo, 2), fecha_txt(&m.fecha), alfa(&m.folio, 8), num(i64::from(m.dias), 2), num(m.sdi, 7)].concat(),
                        );
                    }
                }
            }
            Clase::Creditos => {
                for t in self.trabajadores(reg_pat, "")? {
                    let movs = self.movimientos(reg_pat, &t.num_afil)?;
                    let de_credito: Vec<(&MovimientoGuardado, MovCredito)> = movs.iter().filter_map(|m| Some((m, a_credito(m)?))).collect();
                    for (i, (m, c)) in de_credito.iter().enumerate() {
                        // Cada renglón lleva los datos del crédito tal como quedan tras el movimiento.
                        let hasta_aqui: Vec<MovCredito> = de_credito[..=i].iter().map(|(_, c)| c.clone()).collect();
                        let Some(d) = creditos(&hasta_aqui, c.fecha).vigente else { continue };
                        renglones.push(
                            [
                                alfa(reg_pat, 11),
                                alfa(&t.num_afil, 11),
                                alfa(&d.numero, 10),
                                alfa(&m.tipo, 2),
                                fecha_txt(&m.fecha),
                                d.tipo_descuento.to_string(),
                                num(valor_a_archivo(d.tipo_descuento, d.valor), 8),
                                "N".to_string(),
                            ]
                            .concat(),
                        );
                    }
                }
            }
            Clase::Afiliatorios => {
                for t in self.trabajadores(reg_pat, "")? {
                    let Some(a) = self.afiliacion(reg_pat, &t.num_afil)? else { continue };
                    renglones.push(
                        [
                            alfa(reg_pat, 11),
                            alfa(&t.num_afil, 11),
                            digitos(&a.cpp_trab, 5),
                            fecha_txt(&a.fec_nac),
                            alfa(&a.lug_nac, 25),
                            digitos(&a.ent_trab, 2),
                            digitos(&a.umf_trab, 3),
                            alfa(&a.ocupa, 12),
                            alfa(&a.sexo, 1),
                            alfa(if a.tip_sal.is_empty() { "0" } else { &a.tip_sal }, 1),
                            alfa(if a.jor_hor.is_empty() { "0" } else { &a.jor_hor }, 1),
                        ]
                        .concat(),
                    );
                }
            }
            Clase::Incapacidades => {
                // Solo las que tienen datos médicos: sin ellos el renglón no sería válido.
                for i in self.incapacidades(reg_pat)? {
                    let Some(d) = i.datos else { continue };
                    renglones.push(
                        [
                            alfa(reg_pat, 11),
                            alfa(&i.num_afil, 11),
                            "1".to_string(),
                            fecha_txt(&i.inicio),
                            alfa(&i.folio, 8),
                            num(i64::from(i.dias), 3),
                            num(i64::from(d.porcentaje), 3),
                            d.ramo.to_string(),
                            d.tipo_riesgo.to_string(),
                            d.secuela.to_string(),
                            d.control.to_string(),
                            fecha_txt(&i.termino),
                        ]
                        .concat(),
                    );
                }
            }
        }
        debug_assert!(renglones.iter().all(|r| r.chars().count() == clase.largo()));
        let mut salida = Vec::new();
        for r in &renglones {
            salida.extend(codificar(r));
            salida.extend_from_slice(b"\r\n");
        }
        Ok((salida, renglones.len()))
    }
}

fn a_credito(m: &MovimientoGuardado) -> Option<MovCredito> {
    let tipo: u8 = m.tipo.parse().ok()?;
    if !(15..=20).contains(&tipo) {
        return None;
    }
    Some(MovCredito { tipo, fecha: dia_de_iso(&m.fecha)?, numero: m.credito.clone(), tipo_descuento: m.tipo_descuento, valor: m.valor_descuento })
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::config::{CarpetaTemporal, Modo};
    use crate::dominio::validar::luhn;

    const REG: &str = "Z9900000109";

    fn base(t: &CarpetaTemporal, nombre: &str) -> Base {
        Base::abrir_en(&t.0.join(nombre), &t.0.join("respaldos"), Modo::Portable, 3).unwrap()
    }

    fn nss(n: u64) -> String {
        let base10 = format!("{:010}", 9_000_000_000u64 + n);
        format!("{base10}{}", luhn(&base10).unwrap())
    }

    /// Renglón de trabajador como lo exporta el SUA, sin crédito.
    fn renglon_de_trabajador(nss: &str, largo: usize) -> String {
        let mut s = [
            alfa(REG, 11),
            alfa(nss, 11),
            alfa("XAXX010101", 13),
            alfa("", 18),
            alfa("FICTICIO$DE PRUEBA$TRABAJADOR", 50),
            "10".to_string(),
            "15012026".to_string(),
            "0031504".to_string(),
            alfa("", 17),
            alfa("", 10),
            alfa("", 8),
            " ".to_string(),
            "00000000".to_string(),
            "0".to_string(),
            alfa("", 3),
        ]
        .concat();
        s.truncate(largo);
        s
    }

    #[test]
    fn formato_de_campos() {
        assert_eq!(alfa("ABC", 5), "ABC  ");
        assert_eq!(alfa("ABCDEFG", 5), "ABCDE");
        assert_eq!(alfa("AÑO", 4), "AÑO ");
        assert_eq!(num(31_504, 7), "0031504");
        assert_eq!(digitos("2026-03", 6), "202603");
        assert_eq!(fecha_txt("2026-01-15"), "15012026");
        assert_eq!(fecha_txt(""), "00000000");
        assert_eq!(codificar("AÑO"), b"A\xd1O");
        assert_eq!(decodificar(b"A\xd1O"), "AÑO");
        // Cuota fija 1,500.00: 15000000 diezmilésimas ↔ 01500000 en el archivo.
        assert_eq!((valor_a_archivo(2, 15_000_000), valor_de_archivo(2, 1_500_000)), (1_500_000, 15_000_000));
        assert_eq!((valor_a_archivo(1, 250_000), valor_de_archivo(3, 285_320)), (250_000, 285_320));
        let l = Renglon("AB 0012315012026        ".chars().collect());
        assert_eq!((l.campo(1, 3), l.numero(4, 5), l.fecha(9)), ("AB".to_string(), Ok(Some(123)), Ok("2026-01-15".to_string())));
        assert_eq!((l.numero(17, 4), l.fecha(17)), (Ok(None), Ok(String::new())));
        assert!(l.numero(1, 3).is_err() && Renglon("31022026".chars().collect()).fecha(1).is_err());
    }

    #[test]
    fn importa_trabajadores_de_164_167_y_168_caracteres() {
        let t = CarpetaTemporal::nueva("txt1");
        let b = base(&t, "sua.db");
        b.guardar_patron(&Patron { reg_pat: REG.into(), rfc_pat: "XAXX010101000".into(), nom_pat: "PATRON FICTICIO".into(), ..Patron::default() })
            .unwrap();
        let bueno = nss(4);
        let malo = format!("{}{}", &bueno[..10], if bueno.ends_with('0') { '1' } else { '0' });
        let archivo = [
            renglon_de_trabajador(&nss(1), 164),
            renglon_de_trabajador(&nss(2), 167),
            renglon_de_trabajador(&nss(3), 168),
            String::new(),
            renglon_de_trabajador(&malo, 168),
            renglon_de_trabajador(&nss(5), 120),
            renglon_de_trabajador(&nss(6), 168).replace(REG, "Z9900000208"),
        ]
        .join("\r\n");
        let r = b.importar_txt(Clase::Trabajadores, &codificar(&archivo)).unwrap();
        assert_eq!((r.procesados, r.nuevos, r.actualizados), (6, 3, 0));
        let motivos: Vec<(usize, &str)> = r.rechazados.iter().map(|(n, m)| (*n, m.as_str())).collect();
        assert_eq!(motivos.len(), 3);
        assert!(motivos[0].0 == 5 && motivos[0].1.contains("NSS"), "{motivos:?}");
        assert!(motivos[1].0 == 6 && motivos[1].1.contains("120 caracteres"));
        assert!(motivos[2].0 == 7 && motivos[2].1.contains("patrón registrado"));

        let lista = b.trabajadores(REG, "").unwrap();
        assert_eq!(lista.len(), 3);
        let x = &lista[0];
        assert_eq!((x.rfc_curp.as_str(), x.nom_aseg.as_str(), x.sal_imss, x.fec_alt.as_str(), x.tip_tra.as_str()), ("XAXX010101", "FICTICIO$DE PRUEBA$TRABAJADOR", 31_504, "2026-01-15", "1"));
        // Cada trabajador nuevo queda dado de alta en su fecha de alta.
        let m = b.movimientos(REG, &nss(1)).unwrap();
        assert_eq!((m.len(), m[0].tipo.as_str(), m[0].fecha.as_str(), m[0].sdi), (1, "01", "2026-01-15", 31_504));
        // Importar otra vez actualiza y no duplica el alta.
        let r = b.importar_txt(Clase::Trabajadores, &codificar(&renglon_de_trabajador(&nss(1), 168))).unwrap();
        assert_eq!((r.nuevos, r.actualizados, r.rechazados.len(), b.movimientos(REG, &nss(1)).unwrap().len()), (0, 1, 0, 1));
    }

    #[test]
    fn exportar_e_importar_en_otra_base_conserva_los_datos() {
        let t = CarpetaTemporal::nueva("txt2");
        let a = base(&t, "a.db");
        let patron = Patron {
            reg_pat: REG.into(),
            rfc_pat: "XAXX010101000".into(),
            nom_pat: "COMPAÑÍA FICTICIA".replace('Í', "I"),
            act_pat: "CONSTRUCCION".into(),
            dom_pat: "CALLE FICTICIA 1".into(),
            mun_pat: "CUAUHTEMOC".into(),
            cpp_pat: "06000".into(),
            ent_pat: "09".into(),
            tel_pat: "5550000000".into(),
            zon_pat: "A".into(),
            del_pat: "0101".into(),
        };
        a.guardar_patron(&patron).unwrap();
        a.guardar_prima_rt(REG, "2026-03", 758_875).unwrap();
        for (n, sdi) in [(1u64, 40_000), (2, 31_504)] {
            a.guardar_trabajador(&Trabajador {
                reg_patr: REG.into(),
                num_afil: nss(n),
                rfc_curp: "XAXX010101".into(),
                nom_aseg: format!("NUÑEZ$$TRABAJADOR {n}"),
                sal_imss: sdi,
                fec_alt: "2026-01-01".into(),
                tip_tra: "1".into(),
                sem_jord: "0".into(),
                ..Trabajador::default()
            })
            .unwrap();
            a.guardar_movimiento(REG, &nss(n), "01", "2026-01-01", sdi).unwrap();
        }
        a.guardar_incidencia(REG, &nss(1), "11", "2026-02-10", 2, "").unwrap();
        a.guardar_incidencia(REG, &nss(1), "12", "2026-02-20", 5, "AB123456").unwrap();
        a.guardar_datos_de_incapacidad(REG, &nss(1), "AB123456", &DatosIncapacidad { ramo: 1, tipo_riesgo: 3, secuela: 2, control: 5, porcentaje: 12 }).unwrap();
        a.guardar_afiliacion(&Afiliacion {
            reg_patr: REG.into(),
            num_afil: nss(2),
            cpp_trab: "06000".into(),
            fec_nac: "1990-05-17".into(),
            lug_nac: "CIUDAD FICTICIA".into(),
            ent_trab: "09".into(),
            umf_trab: "12".into(),
            ocupa: "ALBAÑIL".into(),
            sexo: "M".into(),
            tip_sal: "0".into(),
            jor_hor: "8".into(),
        })
        .unwrap();
        a.guardar_credito(REG, &nss(1), "15", "2026-01-01", "0912345678", 2, 15_000_000).unwrap();
        a.guardar_credito(REG, &nss(1), "19", "2026-03-01", "", 0, 16_005_000).unwrap();
        a.guardar_credito(REG, &nss(1), "16", "2026-04-30", "", 0, 0).unwrap();
        a.guardar_movimiento(REG, &nss(2), "02", "2026-03-15", 0).unwrap();
        a.guardar_movimiento(REG, &nss(2), "08", "2026-04-01", 31_504).unwrap();

        let archivos: Vec<(Clase, Vec<u8>, usize)> = Clase::TODAS.iter().map(|c| a.exportar_txt(*c, REG).map(|(d, n)| (*c, d, n)).unwrap()).collect();
        assert_eq!(archivos.iter().map(|x| x.2).collect::<Vec<_>>(), [1, 2, 4, 3, 1, 1]);
        for (clase, datos, renglones) in &archivos {
            // Renglones del largo exacto, terminados en CRLF; la Ñ va como un solo byte.
            assert_eq!(datos.len(), renglones * (clase.largo() + 2), "{}", clase.nombre());
            assert!(datos.ends_with(b"\r\n"));
        }
        assert!(archivos[0].1.windows(8).any(|v| v == b"COMPA/IA"));
        assert!(archivos[1].1.windows(5).any(|v| v == b"NU\xd1EZ"));
        // Crédito en el renglón del trabajador: número, fecha de inicio, tipo 2 y 1,600.50 como 01600500.
        assert!(archivos[1].1.windows(27).any(|v| v == b"091234567801012026201600500"));

        let b = base(&t, "b.db");
        for (clase, datos, renglones) in &archivos {
            let r = b.importar_txt(*clase, datos).unwrap();
            assert_eq!(r.rechazados, [], "{}", clase.nombre());
            assert_eq!(r.avisos, [], "{}", clase.nombre());
            assert_eq!(r.procesados, *renglones);
            // El inicio de crédito ya entró con el trabajador (con el valor vigente); el archivo
            // de créditos lo corrige con el valor original.
            // La incapacidad ya entró con los movimientos; su archivo le añade los datos médicos.
            assert_eq!((r.ya_existian, r.actualizados), (0, usize::from(matches!(clase, Clase::Creditos | Clase::Incapacidades))), "{}", clase.nombre());
        }
        assert_eq!(b.patrones().unwrap(), a.patrones().unwrap());
        assert_eq!(b.prima_rt_vigente(REG).unwrap(), a.prima_rt_vigente(REG).unwrap());
        assert_eq!(b.trabajadores(REG, "").unwrap(), a.trabajadores(REG, "").unwrap());
        for n in [1, 2] {
            let sin_id = |x: Vec<MovimientoGuardado>| x.into_iter().map(|m| MovimientoGuardado { id: 0, ..m }).collect::<Vec<_>>();
            assert_eq!(sin_id(b.movimientos(REG, &nss(n)).unwrap()), sin_id(a.movimientos(REG, &nss(n)).unwrap()), "trabajador {n}");
        }
        assert_eq!(b.afiliacion(REG, &nss(2)).unwrap(), a.afiliacion(REG, &nss(2)).unwrap());
        assert!(b.afiliacion(REG, &nss(2)).unwrap().is_some() && b.afiliacion(REG, &nss(1)).unwrap().is_none());
        assert_eq!(b.incapacidades(REG).unwrap(), a.incapacidades(REG).unwrap());
        assert_eq!(b.siniestralidad(REG, 2026).unwrap(), a.siniestralidad(REG, 2026).unwrap());
        // Y el cálculo sale igual en las dos bases.
        assert_eq!(b.calcular_periodo(REG, "202604", false).unwrap(), a.calcular_periodo(REG, "202604", false).unwrap());
        // Importar de nuevo no duplica nada.
        let r = b.importar_txt(Clase::Movimientos, &archivos[2].1).unwrap();
        assert_eq!((r.nuevos, r.ya_existian, r.rechazados.len()), (0, 4, 0));
    }

    #[test]
    fn renglones_invalidos_de_movimientos_y_creditos() {
        let t = CarpetaTemporal::nueva("txt3");
        let b = base(&t, "sua.db");
        b.guardar_patron(&Patron { reg_pat: REG.into(), rfc_pat: "XAXX010101000".into(), nom_pat: "PATRON FICTICIO".into(), ..Patron::default() })
            .unwrap();
        b.importar_txt(Clase::Trabajadores, &codificar(&renglon_de_trabajador(&nss(1), 168))).unwrap();
        let mov = |tipo: &str, fecha: &str, folio: &str, dias: u32, sdi: i64| [alfa(REG, 11), nss(1), tipo.to_string(), fecha.to_string(), alfa(folio, 8), num(i64::from(dias), 2), num(sdi, 7)].concat();
        let archivo = [
            mov("09", "20012026", "", 0, 50_000),
            mov("01", "20012026", "", 0, 31_504),
            mov("11", "31022026", "", 1, 0),
            mov("11", "20012026", "", 9, 0),
            mov("12", "20012026", "MAL", 3, 0),
            mov("02", "10012026", "", 0, 0),
            mov("07", "20012026", "", 0, 35_000),
            "corto".to_string(),
        ]
        .join("\r\n");
        let r = b.importar_txt(Clase::Movimientos, &codificar(&archivo)).unwrap();
        assert_eq!((r.procesados, r.nuevos, r.rechazados.len()), (8, 1, 7));
        for ((_, m), esperado) in r.rechazados.iter().zip(["(09)", "no admitido", "fecha inválida", "entre 1 y 7", "Folio", "no está activo", "caracteres"]) {
            assert!(m.contains(esperado), "{m} / {esperado}");
        }
        let cred = |tipo: &str, td: &str, valor: i64, tabla: &str| [alfa(REG, 11), nss(1), "0912345678".to_string(), tipo.to_string(), "01022026".to_string(), td.to_string(), num(valor, 8), tabla.to_string()].concat();
        let archivo = [cred("16", "0", 0, "N"), cred("15", "5", 250_000, "N"), cred("15", "1", 250_000, "S")].join("\r\n");
        let r = b.importar_txt(Clase::Creditos, &codificar(&archivo)).unwrap();
        assert_eq!((r.nuevos, r.rechazados.len(), r.avisos.len()), (1, 2, 1));
        assert!(r.rechazados[0].1.contains("que suspender") && r.rechazados[1].1.contains("Tipo de descuento") && r.avisos[0].1.contains("tabla de disminución"));
    }
}

/// Exportaciones reales de trabajadores (`ASEG.Txt`), si están disponibles en
/// `SUA_MUESTRAS`. No imprime datos personales: solo conteos y motivos.
#[cfg(test)]
mod muestras {
    use super::*;
    use crate::config::{CarpetaTemporal, Modo};

    #[test]
    fn muestras_reales_de_trabajadores_si_estan_disponibles() {
        let Some(carpeta) = std::env::var_os("SUA_MUESTRAS") else { return };
        let mut pendientes = vec![std::path::PathBuf::from(carpeta)];
        let mut archivos = 0;
        while let Some(dir) = pendientes.pop() {
            for e in std::fs::read_dir(&dir).unwrap().flatten() {
                let ruta = e.path();
                if ruta.is_dir() {
                    pendientes.push(ruta);
                    continue;
                }
                if !ruta.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("aseg.txt")) {
                    continue;
                }
                let datos = std::fs::read(&ruta).unwrap();
                let t = CarpetaTemporal::nueva("txt-real");
                let b = Base::abrir_en(&t.0.join("sua.db"), &t.0.join("respaldos"), Modo::Portable, 3).unwrap();
                // El patrón del archivo: se registra con datos ficticios para poder importar.
                let reg: String = decodificar(&datos).chars().take(11).collect();
                b.guardar_patron(&Patron { reg_pat: reg.clone(), rfc_pat: "XAXX010101000".into(), nom_pat: "PATRON DE LA MUESTRA".into(), ..Patron::default() }).unwrap();
                let r = b.importar_txt(Clase::Trabajadores, &datos).unwrap();
                println!("aseg.txt real: {} renglones, {} nuevos, {} rechazados, {} avisos", r.procesados, r.nuevos, r.rechazados.len(), r.avisos.len());
                for (n, m) in r.rechazados.iter().chain(&r.avisos) {
                    println!("  renglón {n}: {m}");
                }
                assert!(r.procesados > 0 && r.nuevos + r.rechazados.len() == r.procesados);
                // Lo exportado vuelve a tener un renglón de 168 por trabajador importado.
                let (salida, renglones) = b.exportar_txt(Clase::Trabajadores, &reg.to_uppercase()).unwrap();
                assert_eq!((renglones, salida.len()), (r.nuevos, r.nuevos * 170));
                archivos += 1;
            }
        }
        assert!(archivos > 0);
    }
}
