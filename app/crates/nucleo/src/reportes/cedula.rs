//! Cédula de determinación de cuotas en PDF, a partir de un cálculo de periodo.
//!
//! Tres partes, en hoja carta horizontal: resumen de liquidación, detalle por
//! trabajador de las cuotas del IMSS del mes y, en los meses pares, detalle de
//! RCV e INFONAVIT del bimestre.
//!
//! Es un documento de trabajo: no reproduce el formato de la cédula del SUA
//! oficial ni sirve como comprobante de pago, y así lo dice en cada página.

use super::pdf::{partir, recortar, Fuente, Pdf, CARTA_ALTO, CARTA_ANCHO};
use super::pesos;
use crate::datos::captura::Patron;
use crate::datos::periodo::{CalculoPeriodo, LineaCalculo};

pub struct Cedula<'a> {
    pub patron: &'a Patron,
    pub calculo: &'a CalculoPeriodo,
    /// Renglón libre del pie de página: versión del programa y fecha.
    pub nota: &'a str,
}

/// `cedula_<registro patronal>_<AAAAMM>.pdf`
pub fn nombre_de_archivo(reg_pat: &str, periodo: &str) -> String {
    let limpio = |s: &str| s.chars().filter(char::is_ascii_alphanumeric).collect::<String>();
    format!("cedula_{}_{}.pdf", limpio(reg_pat), limpio(periodo))
}

const ANCHO: f32 = CARTA_ALTO;
const ALTO: f32 = CARTA_ANCHO;
const MARGEN: f32 = 28.0;
const DERECHA: f32 = ANCHO - MARGEN;
const PRIMER_RENGLON: f32 = 108.0;
const ALTO_RENGLON: f32 = 10.0;
/// Renglones de detalle por página; debajo queda sitio para el de totales.
const RENGLONES: usize = 44;

const MESES: [&str; 12] =
    ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre"];

/// (mes en texto, bimestre en texto) de un periodo `AAAAMM`.
pub fn periodo_en_texto(periodo: &str) -> (String, String) {
    let (anio, mes) = (periodo.get(..4).unwrap_or(""), periodo.get(4..).and_then(|m| m.parse::<usize>().ok()).unwrap_or(0));
    if !(1..=12).contains(&mes) {
        return (periodo.to_string(), periodo.to_string());
    }
    let primero = if mes % 2 == 0 { mes - 1 } else { mes };
    (format!("{} de {anio}", MESES[mes - 1]), format!("{}–{} de {anio}", MESES[primero - 1], MESES[primero]))
}

fn prima_en_texto(cienmilesimas: Option<i64>) -> String {
    match cienmilesimas {
        Some(v) => format!("{}.{:05} %", v / 100_000, v % 100_000),
        None => "sin prima vigente".to_string(),
    }
}

struct Columna {
    titulo: &'static str,
    /// Borde izquierdo si es de texto; borde derecho si es numérica.
    x: f32,
    numerica: bool,
    /// Ancho disponible, para recortar el texto.
    ancho: f32,
}

fn texto(titulo: &'static str, x: f32, ancho: f32) -> Columna {
    Columna { titulo, x, numerica: false, ancho }
}

fn numero(titulo: &'static str, x: f32) -> Columna {
    Columna { titulo, x, numerica: true, ancho: 0.0 }
}

struct Hoja<'a> {
    d: Pdf,
    c: &'a Cedula<'a>,
    pagina: usize,
    paginas: usize,
}

impl Hoja<'_> {
    fn pagina_nueva(&mut self, titulo: &str) {
        self.d.pagina();
        self.pagina += 1;
        let (p, r) = (self.c.patron, self.c.calculo);
        let (mes, bimestre) = periodo_en_texto(&r.periodo);
        self.d.texto(MARGEN, 42.0, 13.0, Fuente::Negrita, titulo);
        self.d.texto_derecha(DERECHA, 42.0, 8.0, Fuente::Normal, &format!("Página {} de {}", self.pagina, self.paginas));
        self.d.texto(MARGEN, 57.0, 8.5, Fuente::Normal, &format!("Registro patronal: {}     RFC: {}     {}", p.reg_pat, p.rfc_pat, p.nom_pat));
        let mut datos = format!("Periodo: {mes}");
        if r.cierra_bimestre {
            datos.push_str(&format!("     Bimestre: {bimestre}"));
        }
        datos.push_str(&format!("     Prima de riesgos de trabajo: {}", prima_en_texto(r.prima_rt)));
        if let Some(e) = &r.extemporaneo {
            datos.push_str(&format!("     Pago extemporáneo: {}", e.fecha_pago));
        }
        if r.decreto > 0 {
            datos.push_str(&format!("     Decreto de exención: {} % de las cuotas patronales del IMSS", r.decreto));
        }
        self.d.texto(MARGEN, 69.0, 8.5, Fuente::Normal, &datos);
        self.d.linea(MARGEN, 76.0, DERECHA, 76.0, 0.8);

        self.d.linea(MARGEN, ALTO - 38.0, DERECHA, ALTO - 38.0, 0.4);
        self.d.texto(
            MARGEN,
            ALTO - 28.0,
            6.5,
            Fuente::Normal,
            "Documento de trabajo generado por OpenSUA. No es la cédula del SUA oficial ni un comprobante de pago.",
        );
        self.d.texto_derecha(DERECHA, ALTO - 28.0, 6.5, Fuente::Normal, self.c.nota);
    }

    fn renglon(&mut self, columnas: &[Columna], celdas: &[String], y: f32, tamano: f32, fuente: Fuente) {
        for (col, celda) in columnas.iter().zip(celdas) {
            if col.numerica {
                self.d.texto_derecha(col.x, y, tamano, fuente, celda);
            } else {
                self.d.texto(col.x, y, tamano, fuente, &recortar(celda, tamano, col.ancho));
            }
        }
    }

    /// Tabla paginada con encabezado de columnas en cada página y un renglón
    /// de totales al final, cuyo rótulo (primera celda) dispone de
    /// `ancho_rotulo` puntos.
    fn tabla(&mut self, titulo: &str, columnas: &[Columna], filas: &[Vec<String>], totales: &[String], tamano: f32, ancho_rotulo: f32) {
        let titulos: Vec<String> = columnas.iter().map(|c| c.titulo.to_string()).collect();
        let bloques: Vec<&[Vec<String>]> = if filas.is_empty() { vec![&[]] } else { filas.chunks(RENGLONES).collect() };
        let ultimo = bloques.len() - 1;
        for (i, bloque) in bloques.into_iter().enumerate() {
            self.pagina_nueva(titulo);
            self.renglon(columnas, &titulos, 92.0, tamano, Fuente::Normal);
            self.d.linea(MARGEN, 97.0, DERECHA, 97.0, 0.4);
            let mut y = PRIMER_RENGLON;
            for fila in bloque {
                self.renglon(columnas, fila, y, tamano, Fuente::Normal);
                y += ALTO_RENGLON;
            }
            if i == ultimo {
                self.d.linea(MARGEN, y - 6.0, DERECHA, y - 6.0, 0.4);
                self.d.texto(columnas[0].x, y + 4.0, tamano, Fuente::Negrita, &recortar(&totales[0], tamano, ancho_rotulo));
                self.renglon(&columnas[1..], &totales[1..], y + 4.0, tamano, Fuente::Negrita);
            }
        }
    }

    fn resumen(&mut self) {
        self.pagina_nueva("Resumen de liquidación");
        let r = self.c.calculo;
        let (mes, bimestre) = periodo_en_texto(&r.periodo);
        let (m, b) = (&r.mes, &r.bimestre);
        const X: [f32; 3] = [430.0, 530.0, 630.0];
        const TAM: f32 = 9.5;
        let mut y = 100.0;

        let encabezado = |h: &mut Self, y: &mut f32, titulo: &str| {
            h.d.texto(MARGEN, *y, 10.5, Fuente::Negrita, titulo);
            for (x, t) in X.iter().zip(["Patronal", "Obrero", "Total"]) {
                h.d.texto_derecha(*x, *y, 8.5, Fuente::Normal, t);
            }
            h.d.linea(MARGEN, *y + 4.0, X[2], *y + 4.0, 0.4);
            *y += 17.0;
        };
        let fila = |h: &mut Self, y: &mut f32, concepto: &str, patronal: i64, obrero: Option<i64>, fuente: Fuente| {
            h.d.texto(MARGEN + 10.0, *y, TAM, fuente, concepto);
            h.d.texto_derecha(X[0], *y, TAM, fuente, &pesos(patronal));
            if let Some(o) = obrero {
                h.d.texto_derecha(X[1], *y, TAM, fuente, &pesos(o));
            }
            h.d.texto_derecha(X[2], *y, TAM, fuente, &pesos(patronal + obrero.unwrap_or(0)));
            *y += 13.0;
        };

        encabezado(self, &mut y, &format!("Cuotas IMSS — {mes}"));
        fila(self, &mut y, "Cuota fija", m.cuota_fija, None, Fuente::Normal);
        fila(self, &mut y, "Excedente de tres veces la base", m.excedente_patronal, Some(m.excedente_obrero), Fuente::Normal);
        fila(self, &mut y, "Prestaciones en dinero", m.pd_patronal, Some(m.pd_obrero), Fuente::Normal);
        fila(self, &mut y, "Gastos médicos de pensionados", m.gmp_patronal, Some(m.gmp_obrero), Fuente::Normal);
        fila(self, &mut y, "Riesgos de trabajo", m.riesgos_trabajo, None, Fuente::Normal);
        fila(self, &mut y, "Invalidez y vida", m.iv_patronal, Some(m.iv_obrero), Fuente::Normal);
        fila(self, &mut y, "Guarderías y prestaciones sociales", m.guarderias, None, Fuente::Normal);
        fila(self, &mut y, "Suma IMSS", m.patronal(), Some(m.obrero()), Fuente::Negrita);
        y += 8.0;

        if r.cierra_bimestre {
            encabezado(self, &mut y, &format!("Retiro, cesantía y vejez — {bimestre}"));
            fila(self, &mut y, "Retiro", b.retiro, None, Fuente::Normal);
            fila(self, &mut y, "Cesantía en edad avanzada y vejez", b.cyv_patronal, Some(b.cyv_obrero), Fuente::Normal);
            fila(self, &mut y, "Suma RCV", b.retiro + b.cyv_patronal, Some(b.cyv_obrero), Fuente::Negrita);
            y += 8.0;
            encabezado(self, &mut y, &format!("INFONAVIT — {bimestre}"));
            fila(self, &mut y, "Aportación patronal", b.infonavit, None, Fuente::Normal);
            // Lo que se descuenta al trabajador acreditado va solo en la columna obrera.
            for (concepto, importe) in [("Amortización de créditos", b.amortizacion), ("Seguro de daños a la vivienda", b.seguro_vivienda)] {
                if importe > 0 {
                    self.d.texto(MARGEN + 10.0, y, TAM, Fuente::Normal, concepto);
                    self.d.texto_derecha(X[1], y, TAM, Fuente::Normal, &pesos(importe));
                    self.d.texto_derecha(X[2], y, TAM, Fuente::Normal, &pesos(importe));
                    y += 13.0;
                }
            }
            if b.amortizacion + b.seguro_vivienda > 0 {
                fila(self, &mut y, "Suma INFONAVIT", b.infonavit, Some(b.amortizacion + b.seguro_vivienda), Fuente::Negrita);
            }
            y += 8.0;
        }

        self.d.linea(MARGEN, y - 8.0, X[2], y - 8.0, 0.8);
        let patronal = m.patronal() + b.retiro + b.cyv_patronal + b.infonavit;
        let obrero = m.obrero() + b.cyv_obrero + b.amortizacion + b.seguro_vivienda;
        self.d.texto(MARGEN, y + 4.0, 11.0, Fuente::Negrita, if r.extemporaneo.is_some() { "Suma de cuotas" } else { "Total" });
        self.d.texto_derecha(X[0], y + 4.0, 11.0, Fuente::Negrita, &pesos(patronal));
        self.d.texto_derecha(X[1], y + 4.0, 11.0, Fuente::Negrita, &pesos(obrero));
        self.d.texto_derecha(X[2], y + 4.0, 11.0, Fuente::Negrita, &pesos(r.cuotas()));
        y += 26.0;
        if let Some(e) = &r.extemporaneo {
            self.d.texto(MARGEN, y, 10.5, Fuente::Negrita, &format!("Actualización y recargos — pago del {}", e.fecha_pago));
            for (x, t) in X.iter().zip(["Actualización", "Recargos", "Suma"]) {
                self.d.texto_derecha(*x, y, 8.5, Fuente::Normal, t);
            }
            self.d.linea(MARGEN, y + 4.0, X[2], y + 4.0, 0.4);
            y += 17.0;
            let mut rubros = vec![("Cuotas IMSS", e.actualizacion_imss, e.recargos_imss)];
            if r.cierra_bimestre {
                rubros.push(("Retiro, cesantía y vejez", e.actualizacion_rcv, e.recargos_rcv));
                rubros.push(("INFONAVIT (sin el seguro de daños)", e.actualizacion_infonavit, e.recargos_infonavit));
            }
            for (concepto, actualizacion, recargos) in rubros {
                self.d.texto(MARGEN + 10.0, y, TAM, Fuente::Normal, concepto);
                self.d.texto_derecha(X[0], y, TAM, Fuente::Normal, &pesos(actualizacion));
                self.d.texto_derecha(X[1], y, TAM, Fuente::Normal, &pesos(recargos));
                self.d.texto_derecha(X[2], y, TAM, Fuente::Normal, &pesos(actualizacion + recargos));
                y += 13.0;
            }
            y += 8.0;
            self.d.linea(MARGEN, y - 8.0, X[2], y - 8.0, 0.8);
            self.d.texto(MARGEN, y + 4.0, 11.0, Fuente::Negrita, "Total a pagar");
            self.d.texto_derecha(X[2], y + 4.0, 11.0, Fuente::Negrita, &pesos(r.total()));
            y += 26.0;
        }

        let mut notas = vec![format!("Trabajadores con días cotizados: {}.", r.lineas.len())];
        if !r.cierra_bimestre {
            notas.push("Mes impar: no cierra bimestre; solo se determinan las cuotas del IMSS.".into());
        }
        match &r.extemporaneo {
            None => notas.push("Cálculo de pago oportuno: no incluye actualización ni recargos.".into()),
            Some(e) => notas.push(format!(
                "Pago extemporáneo: vencía el {}; {} {} de mora; factor de actualización {}.{:04}; recargos {}.{:02} %. \
                 Artículos 17-A y 21 del Código Fiscal de la Federación. No incluye multas.",
                e.vencimiento,
                e.meses,
                if e.meses == 1 { "mes o fracción" } else { "meses o fracción" },
                e.factor / 10_000,
                e.factor % 10_000,
                e.tasa / 100,
                e.tasa % 100
            )),
        }
        if b.amortizacion > 0 {
            notas.push(
                "La amortización de créditos INFONAVIT se descuenta al trabajador. El seguro de daños a la vivienda es un importe fijo por bimestre que el SUA \
                 oficial suma a la amortización; aquí se muestra aparte."
                    .into(),
            );
        }
        if r.lineas.iter().any(|l| l.ausentismos_mes + l.incapacidad_mes + l.ausentismos_bimestre + l.incapacidad_bimestre > 0) {
            notas.push(
                "Ausentismos (Aus.) e incapacidades (Inc.) según el artículo 31 de la Ley del Seguro Social: en los días de ausentismo solo se cotiza \
                 enfermedades y maternidad; en los de incapacidad, solo retiro y la aportación al INFONAVIT."
                    .into(),
            );
        }
        notas.extend(r.avisos.iter().map(|a| format!("Aviso: {a}")));
        let limite = ALTO - 48.0;
        'notas: for nota in &notas {
            for renglon in partir(nota, 8.5, DERECHA - MARGEN) {
                if y > limite {
                    break 'notas;
                }
                self.d.texto(MARGEN, y, 8.5, Fuente::Normal, &renglon);
                y += 11.0;
            }
            y += 2.0;
        }
    }
}

fn rotulo_de_totales(n: usize) -> String {
    format!("Total: {n} {}", if n == 1 { "trabajador" } else { "trabajadores" })
}

/// Los días de incidencia en cero se dejan en blanco, para que resalten los que hay.
fn dias_o_nada(n: u32) -> String {
    if n == 0 {
        String::new()
    } else {
        n.to_string()
    }
}

fn importe_o_nada(centavos: i64) -> String {
    if centavos == 0 {
        String::new()
    } else {
        pesos(centavos)
    }
}

fn dias(n: u32) -> String {
    n.to_string()
}

fn detalle_mes(l: &LineaCalculo) -> Vec<String> {
    let m = &l.mes;
    // Como en la cédula del SUA, «Días» es el neto de ausentismos (manual, p. 194).
    let mut v = vec![l.nss.clone(), l.nombre.clone(), dias(l.dias_mes.saturating_sub(l.ausentismos_mes)), dias_o_nada(l.incapacidad_mes), dias_o_nada(l.ausentismos_mes), pesos(l.sdi)];
    v.extend(m.renglones().iter().map(|(_, importe)| pesos(*importe)));
    v.extend([pesos(m.patronal()), pesos(m.obrero()), pesos(m.total())]);
    v
}

fn detalle_bimestre(l: &LineaCalculo) -> Vec<String> {
    let b = &l.bimestre;
    vec![
        l.nss.clone(),
        l.nombre.clone(),
        dias(l.dias_bimestre.saturating_sub(l.ausentismos_bimestre)),
        dias_o_nada(l.incapacidad_bimestre),
        dias_o_nada(l.ausentismos_bimestre),
        pesos(l.sdi),
        pesos(b.retiro),
        pesos(b.cyv_patronal),
        pesos(b.cyv_obrero),
        pesos(b.retiro + b.cyv_patronal + b.cyv_obrero),
        pesos(b.infonavit),
        l.credito.clone(),
        importe_o_nada(b.amortizacion),
        importe_o_nada(b.seguro_vivienda),
        pesos(b.total()),
    ]
}

/// Genera la cédula. Devuelve los bytes del PDF.
pub fn cedula_pdf(c: &Cedula) -> Vec<u8> {
    let r = c.calculo;
    let del_mes: Vec<&LineaCalculo> = r.lineas.iter().filter(|l| l.dias_mes > 0).collect();
    let del_bimestre: Vec<&LineaCalculo> = if r.cierra_bimestre { r.lineas.iter().filter(|l| l.dias_bimestre > 0).collect() } else { Vec::new() };
    let hojas = |n: usize| n.div_ceil(RENGLONES).max(1);
    let paginas = 1 + hojas(del_mes.len()) + if r.cierra_bimestre { hojas(del_bimestre.len()) } else { 0 };
    let mut h = Hoja { d: Pdf::nuevo(ANCHO, ALTO), c, pagina: 0, paginas };
    let (mes, bimestre) = periodo_en_texto(&r.periodo);

    h.resumen();

    // Detalle del mes: 14 columnas de importe repartidas en el ancho que queda.
    const TITULOS_MES: [&str; 14] = [
        "C. fija", "Exc. pat.", "Exc. obr.", "PD pat.", "PD obr.", "GMP pat.", "GMP obr.", "R. trabajo", "IV pat.", "IV obr.", "Guarderías",
        "Patronal", "Obrero", "Total",
    ];
    const SDI_MES: f32 = 224.0;
    // Las tres columnas de sumas son más anchas: ahí caen los importes mayores.
    const SUMA: f32 = 44.0;
    let paso = (DERECHA - SDI_MES - 3.0 * SUMA) / 11.0;
    let mut columnas =
        vec![texto("NSS", MARGEN, 43.0), texto("Nombre", 73.0, 70.0), numero("Días", 160.0), numero("Inc.", 176.0), numero("Aus.", 192.0), numero("SDI", SDI_MES)];
    columnas.extend(TITULOS_MES.iter().enumerate().map(|(i, t)| {
        let (ramos, sumas) = ((i + 1).min(11) as f32, (i + 1).saturating_sub(11) as f32);
        numero(t, SDI_MES + paso * ramos + SUMA * sumas)
    }));
    let filas: Vec<Vec<String>> = del_mes.iter().map(|l| detalle_mes(l)).collect();
    let mut totales = vec![rotulo_de_totales(del_mes.len()), String::new(), String::new(), String::new(), String::new(), String::new()];
    totales.extend(r.mes.renglones().iter().map(|(_, importe)| pesos(*importe)));
    totales.extend([pesos(r.mes.patronal()), pesos(r.mes.obrero()), pesos(r.mes.total())]);
    h.tabla(&format!("Cuotas IMSS por trabajador — {mes}"), &columnas, &filas, &totales, 6.5, 130.0);

    if r.cierra_bimestre {
        let columnas = [
            texto("NSS", MARGEN, 50.0),
            texto("Nombre", 82.0, 128.0),
            numero("Días", 228.0),
            numero("Inc.", 246.0),
            numero("Aus.", 264.0),
            numero("SDI", 306.0),
            numero("Retiro", 352.0),
            numero("C. y V. pat.", 402.0),
            numero("C. y V. obr.", 448.0),
            numero("Suma RCV", 498.0),
            numero("Aportación", 548.0),
            texto("Crédito", 558.0, 46.0),
            numero("Amortización", 656.0),
            numero("Seguro", 696.0),
            numero("Total", DERECHA),
        ];
        let filas: Vec<Vec<String>> = del_bimestre.iter().map(|l| detalle_bimestre(l)).collect();
        let b = &r.bimestre;
        let totales = vec![
            rotulo_de_totales(del_bimestre.len()),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            pesos(b.retiro),
            pesos(b.cyv_patronal),
            pesos(b.cyv_obrero),
            pesos(b.retiro + b.cyv_patronal + b.cyv_obrero),
            pesos(b.infonavit),
            String::new(),
            importe_o_nada(b.amortizacion),
            importe_o_nada(b.seguro_vivienda),
            pesos(b.total()),
        ];
        h.tabla(&format!("RCV e INFONAVIT por trabajador — {bimestre}"), &columnas, &filas, &totales, 7.2, 180.0);
    }
    debug_assert_eq!(h.pagina, h.paginas);
    h.d.bytes(&format!("Cédula de determinación de cuotas {} {}", c.patron.reg_pat, r.periodo))
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::dominio::calculo::{CuotasBimestre, CuotasMes};
    use crate::reportes::pdf::comprobar::{contiene, estructura};

    fn patron() -> Patron {
        Patron { reg_pat: "Z9900000109".into(), rfc_pat: "XAXX010101000".into(), nom_pat: "PATRÓN FICTICIO, S. A.".into(), ..Patron::default() }
    }

    fn calculo(periodo: &str, trabajadores: usize) -> CalculoPeriodo {
        let mut r = CalculoPeriodo { periodo: periodo.into(), cierra_bimestre: periodo.ends_with("02"), prima_rt: Some(758_875), ..CalculoPeriodo::default() };
        for i in 0..trabajadores {
            let mes = CuotasMes { cuota_fija: 31_112, excedente_patronal: 100 + i as i64, riesgos_trabajo: 22_007, guarderias: 2_900, iv_obrero: 1_813, ..CuotasMes::default() };
            let bimestre = if r.cierra_bimestre { CuotasBimestre { retiro: 12_000, cyv_patronal: 18_900, cyv_obrero: 6_750, infonavit: 30_000, amortizacion: if i == 2 { 150_000 } else { 0 }, seguro_vivienda: if i == 2 { 1_300 } else { 0 } } } else { CuotasBimestre::default() };
            r.mes.sumar(&mes);
            r.bimestre.sumar(&bimestre);
            r.lineas.push(LineaCalculo {
                nss: format!("900000{i:05}"),
                nombre: format!("FICTICIO PÉREZ NÚÑEZ TRABAJADOR NÚMERO {i}"),
                sdi: 10_000,
                dias_mes: 29,
                dias_bimestre: if r.cierra_bimestre { 60 } else { 0 },
                ausentismos_mes: if i == 1 { 2 } else { 0 },
                incapacidad_mes: if i == 1 { 3 } else { 0 },
                ausentismos_bimestre: 0,
                incapacidad_bimestre: 0,
                credito: if i == 2 && r.cierra_bimestre { "0912345678".into() } else { String::new() },
                mes,
                bimestre,
            });
        }
        r
    }

    #[test]
    fn cedula_de_un_mes_par_lleva_resumen_imss_y_bimestre() {
        let (p, r) = (patron(), calculo("200802", 3));
        let pdf = cedula_pdf(&Cedula { patron: &p, calculo: &r, nota: "nota de prueba" });
        assert_eq!(estructura(&pdf), 3);
        // El segundo trabajador lleva 3 días de incapacidad y 2 de ausentismo en el mes.
        assert!(contiene(&pdf, b"(Inc.) Tj") && contiene(&pdf, b"(3) Tj") && contiene(&pdf, b"(2) Tj") && contiene(&pdf, b"Ausentismos \\(Aus.\\)"));
        // El tercer trabajador tiene crédito: amortización de 1,500.00 y seguro de 13.00.
        for texto in [&b"(0912345678) Tj"[..], b"(1,500.00) Tj", b"(13.00) Tj", b"(Amortizaci\xf3n de cr\xe9ditos) Tj", b"(Suma INFONAVIT) Tj"] {
            assert!(contiene(&pdf, texto), "falta {}", String::from_utf8_lossy(texto));
        }
        for texto in [
            &b"(Resumen de liquidaci\xf3n)"[..],
            b"(Cuotas IMSS por trabajador \x97 febrero de 2008)",
            b"(RCV e INFONAVIT por trabajador \x97 enero\x96febrero de 2008)",
            b"(P\xe1gina 3 de 3)",
            b"Registro patronal: Z9900000109     RFC: XAXX010101000     PATR\xd3N FICTICIO, S. A.",
            b"Prima de riesgos de trabajo: 7.58875 %",
            b"(nota de prueba)",
            b"No es la c\xe9dula del SUA oficial",
            b"(90000000002)",
            b"(Total: 3 trabajadores)",
        ] {
            assert!(contiene(&pdf, texto), "falta {}", String::from_utf8_lossy(texto));
        }
        // Totales: cuota fija 3 × 311.12; retiro 3 × 120.00; total general.
        for importe in ["933.36", "360.00", &pesos(r.total())] {
            assert!(contiene(&pdf, format!("({importe}) Tj").as_bytes()), "falta {importe}");
        }
        // Ningún texto se sale de la hoja por la derecha ni empieza antes del margen.
        let texto = String::from_utf8_lossy(&pdf);
        for bt in texto.split("BT ").skip(1) {
            let x: f32 = bt.split_whitespace().nth(3).unwrap().parse().unwrap();
            assert!((MARGEN - 0.01..DERECHA).contains(&x), "x = {x}: {}", bt.lines().next().unwrap());
        }
    }

    #[test]
    fn un_mes_impar_no_lleva_bimestre_y_el_detalle_se_pagina() {
        let (p, mut r) = (patron(), calculo("200801", 100));
        r.avisos.push("aviso de prueba".into());
        r.prima_rt = None;
        r.decreto = 5;
        // Importes grandes y nombre largo, para ver que caben en su columna.
        r.lineas[0].mes.cuota_fija = 123_456_789;
        r.lineas[0].nombre = "APELLIDOPATERNOLARGO APELLIDOMATERNOLARGO NOMBRE COMPUESTO".into();
        r.mes.cuota_fija += 123_456_789 - 31_112;
        let pdf = cedula_pdf(&Cedula { patron: &p, calculo: &r, nota: "" });
        // Para revisar el dibujo a ojo: SUA_PDF_DE_PRUEBA=<archivo> cargo test un_mes_impar
        if let Some(ruta) = std::env::var_os("SUA_PDF_DE_PRUEBA") {
            std::fs::write(ruta, &pdf).unwrap();
        }
        // Resumen + 100 renglones a 44 por página.
        assert_eq!(estructura(&pdf), 4);
        assert!(contiene(&pdf, b"(P\xe1gina 4 de 4)"));
        assert!(!contiene(&pdf, b"RCV e INFONAVIT"));
        assert!(contiene(&pdf, b"Mes impar"));
        assert!(contiene(&pdf, b"(Aviso: aviso de prueba)"));
        assert!(contiene(&pdf, b"sin prima vigente"));
        assert!(contiene(&pdf, b"Decreto de exenci\xf3n: 5 %"));
        assert!(contiene(&pdf, b"(90000000099)"));
        assert!(contiene(&pdf, b"(1,234,567.89) Tj"));
        // Con un solo trabajador el rótulo va en singular.
        let uno = calculo("200801", 1);
        assert!(contiene(&cedula_pdf(&Cedula { patron: &p, calculo: &uno, nota: "" }), b"(Total: 1 trabajador) Tj"));
        // Los totales salen una sola vez, en la última página.
        assert_eq!(pdf.windows(24).filter(|v| *v == b"(Total: 100 trabajadores").count(), 1);
    }

    #[test]
    fn un_pago_extemporaneo_lleva_actualizacion_recargos_y_total_a_pagar() {
        let (p, mut r) = (patron(), calculo("200802", 3));
        r.extemporaneo = Some(crate::dominio::extemporaneo::Extemporaneo {
            fecha_pago: "2008-05-20".into(),
            vencimiento: "2008-03-17".into(),
            meses: 3,
            factor: 10_066,
            tasa: 339,
            actualizacion_imss: 1_111,
            recargos_imss: 2_222,
            actualizacion_rcv: 3_333,
            recargos_rcv: 4_444,
            actualizacion_infonavit: 5_555,
            recargos_infonavit: 6_666,
        });
        let pdf = cedula_pdf(&Cedula { patron: &p, calculo: &r, nota: "" });
        assert_eq!(estructura(&pdf), 3);
        for texto in [
            &b"(Suma de cuotas) Tj"[..],
            b"(Total a pagar) Tj",
            b"pago del 2008-05-20) Tj",
            b"(11.11) Tj",
            b"(66.66) Tj",
            b"(122.21) Tj",
            b"3 meses o fracci",
            b"factor de actualizaci",
            b"1.0066; recargos 3.39 %",
            b"Pago extempor",
        ] {
            assert!(contiene(&pdf, texto), "falta {}", String::from_utf8_lossy(texto));
        }
        assert!(contiene(&pdf, format!("({}) Tj", pesos(r.total())).as_bytes()) && r.total() == r.cuotas() + 23_331);
        // Un pago oportuno no lleva esa parte.
        r.extemporaneo = None;
        let pdf = cedula_pdf(&Cedula { patron: &p, calculo: &r, nota: "" });
        assert!(!contiene(&pdf, b"Total a pagar") && contiene(&pdf, b"(Total) Tj") && contiene(&pdf, b"pago oportuno"));
    }

    #[test]
    fn un_calculo_sin_trabajadores_da_un_documento_valido() {
        let (p, r) = (patron(), calculo("200802", 0));
        assert_eq!(estructura(&cedula_pdf(&Cedula { patron: &p, calculo: &r, nota: "" })), 3);
    }

    #[test]
    fn nombre_del_archivo() {
        assert_eq!(nombre_de_archivo("Z9900000109", "200802"), "cedula_Z9900000109_200802.pdf");
        assert_eq!(nombre_de_archivo("../Z99", "2008/02"), "cedula_Z99_200802.pdf");
    }
}
