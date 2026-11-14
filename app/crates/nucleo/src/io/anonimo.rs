//! Anonimizador de archivos de pago `.SUA`.
//!
//! Sustituye todo lo que identifica al patrón y a los trabajadores por datos
//! ficticios, y conserva lo que hace falta para comprobar el cálculo: periodo,
//! prima de riesgos de trabajo, salarios, días, fechas de los movimientos e
//! importes. El resultado sigue siendo un archivo que [`leer`] acepta y cuya
//! verificación interna cuadra, así que sirve como prueba de oro dentro del
//! repositorio y para que un usuario comparta un caso sin exponer a nadie.
//!
//! Lo que se sustituye:
//! - registro patronal y RFC del patrón, en todos los registros;
//! - razón social, domicilio, población, código postal, teléfono y actividad;
//! - NSS (el mismo NSS recibe siempre el mismo sustituto), RFC, CURP, nombre y
//!   número de crédito INFONAVIT de cada trabajador;
//! - folio de incapacidad de los movimientos;
//! - número de control del registro 06 y el bloque numérico que le sigue, que
//!   no se sabe interpretar y podría derivarse de los datos originales.
//!
//! Lo que se conserva y podría, junto con otros datos, acotar de quién se
//! trata: entidad, delegación y subdelegación, área geográfica, periodo, folio
//! SUA, fecha de pago, número de trabajadores, salarios y fechas de alta y baja.

use super::pago::{leer, ErrorPago, LARGO_REGISTRO};
use crate::dominio::validar::luhn;
use std::collections::HashMap;

pub const REGISTRO_PATRONAL_FICTICIO: &str = "Z9900000109";
const RFC_PATRON_FICTICIO: &str = "XAXX010101000";
const RFC_TRABAJADOR_FICTICIO: &str = "XEXX010101000";
const CURP_FICTICIA: &str = "XEXX010101HNEXXXA4";

/// Escribe `texto` en las posiciones `desde..=hasta` (1‑based), completando con
/// espacios a la derecha.
fn pon(r: &mut [u8], desde: usize, hasta: usize, texto: &str) {
    let campo = &mut r[desde - 1..hasta];
    campo.fill(b' ');
    let n = texto.len().min(campo.len());
    campo[..n].copy_from_slice(&texto.as_bytes()[..n]);
}

fn en_blanco(campo: &[u8]) -> bool {
    campo.iter().all(|&b| b == b' ')
}

/// NSS ficticio número `n` (desde 1): diez dígitos y su dígito verificador.
fn nss_ficticio(n: usize) -> String {
    let diez = format!("{n:010}");
    let digito = luhn(&diez).unwrap_or('0');
    format!("{diez}{digito}")
}

/// Devuelve una copia anonimizada del archivo. Falla si la entrada no es un
/// archivo de pago legible.
pub fn anonimizar(datos: &[u8]) -> Result<Vec<u8>, ErrorPago> {
    leer(datos)?;
    let mut salida = datos.to_vec();
    let original: Vec<u8> = datos[2..13].to_vec();
    let nuevo = REGISTRO_PATRONAL_FICTICIO.as_bytes();
    let mut sustitutos: HashMap<Vec<u8>, String> = HashMap::new();
    let mut sustituto_de = |nss: &[u8]| -> String {
        let siguiente = sustitutos.len() + 1;
        sustitutos.entry(nss.to_vec()).or_insert_with(|| nss_ficticio(siguiente)).clone()
    };
    let (mut creditos, mut folios) = (0usize, 0usize);

    for r in salida.chunks_mut(LARGO_REGISTRO) {
        let tipo = [r[0], r[1]];
        pon(r, 3, 13, REGISTRO_PATRONAL_FICTICIO);
        if &tipo != b"04" {
            pon(r, 14, 26, RFC_PATRON_FICTICIO);
        }
        match &tipo {
            b"02" => {
                pon(r, 39, 88, "PATRON ANONIMO SA DE CV");
                pon(r, 89, 128, "DOMICILIO ANONIMO");
                pon(r, 129, 168, "POBLACION ANONIMA");
                pon(r, 171, 175, "00000");
                for b in &mut r[175..190] {
                    if b.is_ascii_digit() {
                        *b = b'0';
                    }
                }
                pon(r, 204, 243, "ACTIVIDAD ANONIMA");
            }
            b"03" => {
                let nss = sustituto_de(&r[32..43]);
                pon(r, 33, 43, &nss);
                pon(r, 44, 56, RFC_TRABAJADOR_FICTICIO);
                pon(r, 57, 74, CURP_FICTICIA);
                if r[74..84].iter().any(|&b| b != b' ' && b != b'0') {
                    creditos += 1;
                    pon(r, 75, 84, &format!("{creditos:010}"));
                }
                let numero = &nss[4..10];
                pon(r, 95, 144, &format!("ANONIMO$ANONIMO$TRABAJADOR {numero}"));
                // Los tres últimos caracteres repiten el inicio del registro patronal.
                if r[292..295] == original[..3] {
                    r[292..295].copy_from_slice(&nuevo[..3]);
                } else {
                    pon(r, 293, 295, "");
                }
            }
            b"04" => {
                for g in 0..7 {
                    let base = 14 + g * 38;
                    if en_blanco(&r[base - 1..base + 37]) {
                        continue;
                    }
                    let nss = sustituto_de(&r[base - 1..base + 10]);
                    pon(r, base, base + 10, &nss);
                    if r[base + 20..base + 28].iter().any(|&b| b != b' ' && b != b'0') {
                        folios += 1;
                        pon(r, base + 21, base + 28, &format!("AA{folios:06}"));
                    }
                }
            }
            b"06" => {
                // La marca de las posiciones 69-70 repite el inicio del registro patronal.
                if r[68..70] == original[..2] {
                    r[68..70].copy_from_slice(&nuevo[..2]);
                }
                for b in &mut r[120..238] {
                    if !b.is_ascii_whitespace() {
                        *b = b'0';
                    }
                }
            }
            _ => {}
        }
    }
    Ok(salida)
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::io::pago::pruebas::ejemplo;

    #[test]
    fn el_anonimizado_se_lee_cuadra_y_conserva_lo_que_importa_al_calculo() {
        let antes = leer(&ejemplo()).unwrap();
        let datos = anonimizar(&ejemplo()).unwrap();
        let a = leer(&datos).unwrap();
        assert_eq!(a.verificar(), Vec::<String>::new());

        assert_eq!(a.patron.registro_patronal, REGISTRO_PATRONAL_FICTICIO);
        assert_eq!(a.patron.razon_social, "PATRON ANONIMO SA DE CV");
        assert_eq!(a.control.numero_control, "0".repeat(19));
        assert_eq!((a.patron.periodo.as_str(), a.patron.prima_rt), (antes.patron.periodo.as_str(), antes.patron.prima_rt));
        assert_eq!(a.totales, antes.totales);
        assert_eq!(a.trabajadores.len(), antes.trabajadores.len());
        for (x, y) in a.trabajadores.iter().zip(&antes.trabajadores) {
            assert_eq!(crate::dominio::validar::nss(&x.nss).mensaje, "NSS correcto.", "{}", x.nss);
            assert!(x.nombre.starts_with("ANONIMO$ANONIMO$TRABAJADOR "));
            assert_eq!((x.sdi, x.dias_mes, x.dias_bimestre, x.cuota_fija, x.retiro, x.cyv_obrera), (y.sdi, y.dias_mes, y.dias_bimestre, y.cuota_fija, y.retiro, y.cyv_obrera));
        }
        // Cada movimiento sigue apuntando a su trabajador, con su fecha y salario.
        for (m, n) in a.movimientos.iter().zip(&antes.movimientos) {
            assert!(a.trabajadores.iter().any(|t| t.nss == m.nss));
            assert_eq!((&m.tipo, &m.fecha, m.sdi), (&n.tipo, &n.fecha, n.sdi));
        }
    }

    #[test]
    fn no_queda_ningun_identificador_del_original() {
        // El ejemplo ya usa el registro patronal y los NSS ficticios: se le ponen otros para la prueba.
        let mut datos = ejemplo();
        let otros = [b"12345678903", b"98765432106"];
        for (i, r) in datos.chunks_mut(LARGO_REGISTRO).enumerate() {
            r[2..13].copy_from_slice(b"B5512345106");
            match i {
                1 | 2 => r[32..43].copy_from_slice(otros[i - 1]),
                3 => {
                    r[13..24].copy_from_slice(otros[0]);
                    r[51..62].copy_from_slice(otros[1]);
                }
                _ => {}
            }
        }
        let antes = leer(&datos).unwrap();
        assert_eq!(antes.trabajadores[1].nss, "98765432106");
        let salida = anonimizar(&datos).unwrap();
        let texto = String::from_utf8_lossy(&salida);
        for prohibido in ["B5512345106", "B55", "PATRON FICTICIO DE PRUEBA", "FICTICIO$DE PRUEBA", "1234567890123456789"] {
            assert!(!texto.contains(prohibido), "queda «{prohibido}»");
        }
        for t in &antes.trabajadores {
            assert!(!texto.contains(&t.nss), "queda un NSS del original");
        }
    }

    #[test]
    fn lo_que_no_es_un_archivo_de_pago_se_rechaza() {
        assert!(anonimizar(b"no es un archivo de pago").is_err());
        assert!(anonimizar(&[]).is_err());
    }
}
