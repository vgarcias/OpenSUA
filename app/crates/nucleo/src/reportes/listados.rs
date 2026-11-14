//! Listados en PDF: relación de trabajadores y movimientos de un periodo.
//! Hoja carta horizontal, con el mismo escritor que la cédula.

use super::pdf::{recortar, Fuente, Pdf, CARTA_ALTO, CARTA_ANCHO};
use crate::datos::captura::{centavos_a_texto, Patron, Trabajador};
use crate::datos::periodo::{MovimientoGuardado, TIPOS_DE_CREDITO, TIPOS_DE_MOVIMIENTO};
use crate::dominio::credito::descuento_en_texto;

const ANCHO: f32 = CARTA_ALTO;
const ALTO: f32 = CARTA_ANCHO;
const MARGEN: f32 = 28.0;
const DERECHA: f32 = ANCHO - MARGEN;
const RENGLONES: usize = 44;
const TAMANO: f32 = 7.5;

pub struct Columna {
    pub titulo: &'static str,
    /// Borde izquierdo si es de texto; borde derecho si es numérica.
    pub x: f32,
    pub numerica: bool,
    pub ancho: f32,
}

fn texto(titulo: &'static str, x: f32, ancho: f32) -> Columna {
    Columna { titulo, x, numerica: false, ancho }
}

fn numero(titulo: &'static str, x: f32) -> Columna {
    Columna { titulo, x, numerica: true, ancho: 0.0 }
}

/// Tabla paginada con título, datos del patrón, encabezado de columnas en
/// cada página y un renglón final con el conteo.
pub fn listado_pdf(titulo: &str, patron: &Patron, subtitulo: &str, columnas: &[Columna], filas: &[Vec<String>], nota: &str) -> Vec<u8> {
    let mut d = Pdf::nuevo(ANCHO, ALTO);
    let bloques: Vec<&[Vec<String>]> = if filas.is_empty() { vec![&[]] } else { filas.chunks(RENGLONES).collect() };
    let paginas = bloques.len();
    let renglon = |d: &mut Pdf, celdas: &[String], y: f32, fuente: Fuente| {
        for (col, celda) in columnas.iter().zip(celdas) {
            if col.numerica {
                d.texto_derecha(col.x, y, TAMANO, fuente, celda);
            } else {
                d.texto(col.x, y, TAMANO, fuente, &recortar(celda, TAMANO, col.ancho));
            }
        }
    };
    let titulos: Vec<String> = columnas.iter().map(|c| c.titulo.to_string()).collect();
    for (i, bloque) in bloques.into_iter().enumerate() {
        d.pagina();
        d.texto(MARGEN, 42.0, 13.0, Fuente::Negrita, titulo);
        d.texto_derecha(DERECHA, 42.0, 8.0, Fuente::Normal, &format!("Página {} de {paginas}", i + 1));
        d.texto(MARGEN, 57.0, 8.5, Fuente::Normal, &format!("Registro patronal: {}     RFC: {}     {}", patron.reg_pat, patron.rfc_pat, patron.nom_pat));
        d.texto(MARGEN, 69.0, 8.5, Fuente::Normal, subtitulo);
        d.linea(MARGEN, 76.0, DERECHA, 76.0, 0.8);
        renglon(&mut d, &titulos, 92.0, Fuente::Normal);
        d.linea(MARGEN, 97.0, DERECHA, 97.0, 0.4);
        let mut y = 108.0;
        for fila in bloque {
            renglon(&mut d, fila, y, Fuente::Normal);
            y += 10.0;
        }
        if i + 1 == paginas {
            d.linea(MARGEN, y - 6.0, DERECHA, y - 6.0, 0.4);
            d.texto(MARGEN, y + 4.0, TAMANO, Fuente::Negrita, &format!("Renglones: {}", filas.len()));
        }
        d.linea(MARGEN, ALTO - 38.0, DERECHA, ALTO - 38.0, 0.4);
        d.texto(MARGEN, ALTO - 28.0, 6.5, Fuente::Normal, "Documento de trabajo generado por OpenSUA. Contiene datos personales.");
        d.texto_derecha(DERECHA, ALTO - 28.0, 6.5, Fuente::Normal, nota);
    }
    d.bytes(&format!("{titulo} {}", patron.reg_pat))
}

fn nombre_legible(nom_aseg: &str) -> String {
    nom_aseg.split('$').map(str::trim).filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ")
}

/// Relación de trabajadores de un patrón.
pub fn trabajadores_pdf(patron: &Patron, trabajadores: &[Trabajador], nota: &str) -> Vec<u8> {
    let columnas = [
        texto("NSS", MARGEN, 58.0),
        texto("Nombre", 92.0, 230.0),
        texto("RFC", 330.0, 72.0),
        texto("CURP", 408.0, 100.0),
        texto("Alta", 514.0, 50.0),
        texto("Tipo", 570.0, 20.0),
        texto("Jornada", 596.0, 30.0),
        texto("Crédito INFONAVIT", 636.0, 70.0),
        numero("SDI", DERECHA),
    ];
    let filas: Vec<Vec<String>> = trabajadores
        .iter()
        .map(|t| {
            vec![
                t.num_afil.clone(),
                nombre_legible(&t.nom_aseg),
                t.rfc_curp.clone(),
                t.curp.clone(),
                t.fec_alt.clone(),
                t.tip_tra.clone(),
                t.sem_jord.clone(),
                t.pag_info.clone(),
                centavos_a_texto(t.sal_imss),
            ]
        })
        .collect();
    listado_pdf("Relación de trabajadores", patron, "Trabajadores registrados, con su salario diario integrado vigente.", &columnas, &filas, nota)
}

/// Descripción de un movimiento: `07 Modificación de salario`.
pub fn descripcion_de_movimiento(tipo: &str) -> String {
    let nombre = match tipo {
        "00" | "13" => "Salario al inicio del bimestre",
        otro => TIPOS_DE_MOVIMIENTO.iter().chain(TIPOS_DE_CREDITO).find(|(t, _)| *t == otro).map(|(_, d)| *d).unwrap_or("Otro"),
    };
    format!("{tipo} {nombre}")
}

/// Lo que acompaña a un movimiento: salario, días y folio, o datos del crédito.
pub fn detalle_de_movimiento(m: &MovimientoGuardado) -> String {
    match m.tipo.as_str() {
        "11" | "12" => format!("{} días {}", m.dias, m.folio).trim_end().to_string(),
        "15" | "18" => format!("{} {}", m.credito, descuento_en_texto(m.tipo_descuento, m.valor_descuento)).trim().to_string(),
        "19" => format!("valor {}.{:04}", m.valor_descuento / 10_000, m.valor_descuento % 10_000),
        "20" => m.credito.clone(),
        _ if m.sdi > 0 => format!("salario {}", centavos_a_texto(m.sdi)),
        _ => String::new(),
    }
}

/// Movimientos de los trabajadores de un patrón entre dos fechas
/// (`AAAA-MM-DD`, ambas incluidas). `movimientos`: por trabajador, en orden.
pub fn movimientos_pdf(patron: &Patron, desde: &str, hasta: &str, movimientos: &[(Trabajador, Vec<MovimientoGuardado>)], nota: &str) -> Vec<u8> {
    let columnas =
        [texto("NSS", MARGEN, 58.0), texto("Nombre", 92.0, 250.0), texto("Fecha", 350.0, 56.0), texto("Movimiento", 412.0, 170.0), texto("Detalle", 588.0, DERECHA - 588.0)];
    let mut filas = Vec::new();
    for (t, lista) in movimientos {
        for m in lista.iter().filter(|m| m.fecha.as_str() >= desde && m.fecha.as_str() <= hasta) {
            filas.push(vec![t.num_afil.clone(), nombre_legible(&t.nom_aseg), m.fecha.clone(), descripcion_de_movimiento(&m.tipo), detalle_de_movimiento(m)]);
        }
    }
    listado_pdf("Movimientos del periodo", patron, &format!("Movimientos con fecha del {desde} al {hasta}."), &columnas, &filas, nota)
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::reportes::pdf::comprobar::{contiene, estructura};

    fn patron() -> Patron {
        Patron { reg_pat: "Z9900000109".into(), rfc_pat: "XAXX010101000".into(), nom_pat: "PATRON FICTICIO".into(), ..Patron::default() }
    }

    fn trabajador(n: usize) -> Trabajador {
        Trabajador {
            reg_patr: "Z9900000109".into(),
            num_afil: format!("900000{n:05}"),
            nom_aseg: format!("FICTICIO$$TRABAJADOR {n}"),
            rfc_curp: "XAXX010101".into(),
            sal_imss: 31_504,
            fec_alt: "2026-01-15".into(),
            pag_info: if n == 1 { "0912345678".into() } else { String::new() },
            ..Trabajador::default()
        }
    }

    fn movimiento(tipo: &str, fecha: &str) -> MovimientoGuardado {
        MovimientoGuardado { id: 0, tipo: tipo.into(), fecha: fecha.into(), sdi: 0, dias: 0, folio: String::new(), credito: String::new(), tipo_descuento: 0, valor_descuento: 0 }
    }

    #[test]
    fn relacion_de_trabajadores_paginada() {
        let lista: Vec<Trabajador> = (0..100).map(trabajador).collect();
        let pdf = trabajadores_pdf(&patron(), &lista, "nota");
        assert_eq!(estructura(&pdf), 3);
        for t in [&b"(Relaci\xf3n de trabajadores) Tj"[..], b"(P\xe1gina 3 de 3) Tj", b"(90000000099) Tj", b"(FICTICIO TRABAJADOR 1) Tj", b"(0912345678) Tj", b"(315.04) Tj", b"(Renglones: 100) Tj", b"(nota) Tj"] {
            assert!(contiene(&pdf, t), "falta {}", String::from_utf8_lossy(t));
        }
        // Sin trabajadores sigue siendo un documento válido de una página.
        assert_eq!(estructura(&trabajadores_pdf(&patron(), &[], "")), 1);
    }

    #[test]
    fn movimientos_de_un_periodo() {
        let movs = vec![
            (
                trabajador(1),
                vec![
                    MovimientoGuardado { sdi: 31_504, ..movimiento("01", "2026-01-15") },
                    MovimientoGuardado { dias: 3, folio: "AB123456".into(), ..movimiento("12", "2026-02-10") },
                    MovimientoGuardado { credito: "0912345678".into(), tipo_descuento: 2, valor_descuento: 15_000_000, ..movimiento("15", "2026-02-01") },
                    movimiento("02", "2026-03-05"),
                ],
            ),
            (trabajador(2), vec![MovimientoGuardado { sdi: 40_000, ..movimiento("07", "2026-02-16") }]),
        ];
        let pdf = movimientos_pdf(&patron(), "2026-02-01", "2026-02-28", &movs, "");
        assert_eq!(estructura(&pdf), 1);
        for t in [&b"(12 Incapacidad) Tj"[..], b"(3 d\xedas AB123456) Tj", b"(0912345678 cuota fija 1500.00) Tj", b"(07 Modificaci\xf3n de salario) Tj", b"(salario 400.00) Tj", b"(Renglones: 3) Tj"] {
            assert!(contiene(&pdf, t), "falta {}", String::from_utf8_lossy(t));
        }
        // El alta de enero y la baja de marzo quedan fuera del periodo.
        assert!(!contiene(&pdf, b"(01 Alta) Tj") && !contiene(&pdf, b"(02 Baja) Tj"));
        assert_eq!(descripcion_de_movimiento("13"), "13 Salario al inicio del bimestre");
    }
}
