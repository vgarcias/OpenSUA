//! Escritor mínimo de PDF, sin dependencias: páginas con texto y líneas.
//!
//! Usa las fuentes estándar Helvetica y Helvetica-Bold con codificación
//! WinAnsi, que cubre los acentos y la eñe del español, así que no se incrusta
//! ninguna fuente. Los flujos de contenido van sin comprimir: un reporte de
//! cientos de renglones ocupa unas decenas de kilobytes.
//!
//! Coordenadas en puntos (1/72 de pulgada), con el origen arriba a la
//! izquierda: `y` crece hacia abajo y es la línea base del texto.

use std::fmt::Write as _;

pub const CARTA_ANCHO: f32 = 612.0;
pub const CARTA_ALTO: f32 = 792.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fuente {
    Normal,
    Negrita,
}

/// Anchos de Helvetica para los caracteres 32 a 126, en milésimas del tamaño
/// (Adobe Font Metrics).
const ANCHOS: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, // espacio a /
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, // 0 a ?
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, // @ a O
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556, // P a _
    333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, // ` a o
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584, // p a ~
];

/// Letra sin acento, para medir: en Helvetica las acentuadas miden lo mismo.
fn sin_acento(c: char) -> char {
    match c {
        'á' | 'à' | 'ä' | 'â' => 'a',
        'é' | 'è' | 'ë' | 'ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' | 'º' => 'o',
        'ú' | 'ù' | 'ü' | 'û' => 'u',
        'ñ' => 'n',
        'Á' | 'À' | 'Ä' | 'Â' => 'A',
        'É' | 'È' | 'Ë' | 'Ê' => 'E',
        'Í' | 'Ì' | 'Ï' | 'Î' => 'I',
        'Ó' | 'Ò' | 'Ö' | 'Ô' => 'O',
        'Ú' | 'Ù' | 'Ü' | 'Û' => 'U',
        'Ñ' => 'N',
        '–' | '—' | '·' => '_',
        otro => otro,
    }
}

/// Ancho en puntos de un texto en Helvetica normal. Exacto para ASCII y para
/// las letras acentuadas del español; aproximado para lo demás. En negrita los
/// dígitos, el punto y la coma miden igual, así que los importes se alinean
/// igual de bien; las letras en negrita son algo más anchas.
pub fn ancho_texto(texto: &str, tamano: f32) -> f32 {
    let milesimas: u32 = texto
        .chars()
        .map(|c| {
            let c = sin_acento(c);
            if c == '…' {
                1000
            } else if (' '..='~').contains(&c) {
                u32::from(ANCHOS[c as usize - 32])
            } else {
                556
            }
        })
        .sum();
    milesimas as f32 * tamano / 1000.0
}

/// Recorta el texto para que quepa en `ancho` puntos, terminándolo en «…».
pub fn recortar(texto: &str, tamano: f32, ancho: f32) -> String {
    if ancho_texto(texto, tamano) <= ancho {
        return texto.to_string();
    }
    let mut corto = String::new();
    for c in texto.chars() {
        corto.push(c);
        if ancho_texto(&corto, tamano) + ancho_texto("…", tamano) > ancho {
            corto.pop();
            break;
        }
    }
    format!("{}…", corto.trim_end())
}

/// Parte un texto en renglones que quepan en `ancho` puntos, sin cortar palabras.
pub fn partir(texto: &str, tamano: f32, ancho: f32) -> Vec<String> {
    let mut renglones: Vec<String> = Vec::new();
    for palabra in texto.split_whitespace() {
        match renglones.last_mut() {
            Some(r) if ancho_texto(r, tamano) + ancho_texto(" ", tamano) + ancho_texto(palabra, tamano) <= ancho => {
                r.push(' ');
                r.push_str(palabra);
            }
            _ => renglones.push(palabra.to_string()),
        }
    }
    renglones
}

/// Texto como cadena literal de PDF en WinAnsi (Windows-1252).
fn literal(texto: &str) -> Vec<u8> {
    let mut v = vec![b'('];
    for c in texto.chars() {
        let b = match c {
            '(' | ')' | '\\' => {
                v.push(b'\\');
                c as u8
            }
            ' '..='~' => c as u8,
            '\u{a0}'..='\u{ff}' => c as u32 as u8,
            '–' => 0x96,
            '—' => 0x97,
            '…' => 0x85,
            '€' => 0x80,
            _ => b'?',
        };
        v.push(b);
    }
    v.push(b')');
    v
}

pub struct Pdf {
    ancho: f32,
    alto: f32,
    paginas: Vec<Vec<u8>>,
}

impl Pdf {
    /// Documento vacío con páginas del tamaño dado en puntos.
    pub fn nuevo(ancho: f32, alto: f32) -> Pdf {
        Pdf { ancho, alto, paginas: Vec::new() }
    }

    pub fn ancho(&self) -> f32 {
        self.ancho
    }

    pub fn alto(&self) -> f32 {
        self.alto
    }

    pub fn paginas(&self) -> usize {
        self.paginas.len()
    }

    /// Empieza una página nueva; lo que se dibuje después va en ella.
    pub fn pagina(&mut self) {
        self.paginas.push(Vec::new());
    }

    fn actual(&mut self) -> &mut Vec<u8> {
        if self.paginas.is_empty() {
            self.paginas.push(Vec::new());
        }
        self.paginas.last_mut().expect("hay al menos una página")
    }

    pub fn texto(&mut self, x: f32, y: f32, tamano: f32, fuente: Fuente, texto: &str) {
        if texto.is_empty() {
            return;
        }
        let nombre = match fuente {
            Fuente::Normal => "F1",
            Fuente::Negrita => "F2",
        };
        let y_pdf = self.alto - y;
        let cuerpo = literal(texto);
        let p = self.actual();
        p.extend_from_slice(format!("BT /{nombre} {tamano:.2} Tf {x:.2} {y_pdf:.2} Td ").as_bytes());
        p.extend_from_slice(&cuerpo);
        p.extend_from_slice(b" Tj ET\n");
    }

    /// Texto que termina en `x_derecha`. Pensado para importes.
    pub fn texto_derecha(&mut self, x_derecha: f32, y: f32, tamano: f32, fuente: Fuente, texto: &str) {
        self.texto(x_derecha - ancho_texto(texto, tamano), y, tamano, fuente, texto);
    }

    pub fn linea(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, grosor: f32) {
        let alto = self.alto;
        let p = self.actual();
        p.extend_from_slice(format!("{grosor:.2} w {x1:.2} {:.2} m {x2:.2} {:.2} l S\n", alto - y1, alto - y2).as_bytes());
    }

    /// El documento completo. `titulo` va en los metadatos.
    pub fn bytes(&self, titulo: &str) -> Vec<u8> {
        let vacia = [Vec::new()];
        let paginas: &[Vec<u8>] = if self.paginas.is_empty() { &vacia } else { &self.paginas };
        let n = paginas.len();
        // Objetos: 1 catálogo, 2 árbol de páginas, 3 y 4 fuentes, 5 información;
        // después, por cada página, su objeto y su flujo de contenido.
        let mut objetos: Vec<Vec<u8>> = Vec::with_capacity(5 + 2 * n);
        objetos.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
        let hijos: String = (0..n).fold(String::new(), |mut s, i| {
            let _ = write!(s, "{} 0 R ", 6 + 2 * i);
            s
        });
        objetos.push(format!("<< /Type /Pages /Kids [{}] /Count {n} >>", hijos.trim_end()).into_bytes());
        for base in ["Helvetica", "Helvetica-Bold"] {
            objetos.push(format!("<< /Type /Font /Subtype /Type1 /BaseFont /{base} /Encoding /WinAnsiEncoding >>").into_bytes());
        }
        let mut info = b"<< /Title ".to_vec();
        info.extend_from_slice(&literal(titulo));
        info.extend_from_slice(b" /Producer (OpenSUA) >>");
        objetos.push(info);
        for (i, contenido) in paginas.iter().enumerate() {
            objetos.push(
                format!(
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.2} {:.2}] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents {} 0 R >>",
                    self.ancho,
                    self.alto,
                    7 + 2 * i
                )
                .into_bytes(),
            );
            let mut flujo = format!("<< /Length {} >>\nstream\n", contenido.len()).into_bytes();
            flujo.extend_from_slice(contenido);
            flujo.extend_from_slice(b"\nendstream");
            objetos.push(flujo);
        }

        let mut salida = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
        let mut posiciones = Vec::with_capacity(objetos.len());
        for (i, objeto) in objetos.iter().enumerate() {
            posiciones.push(salida.len());
            salida.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
            salida.extend_from_slice(objeto);
            salida.extend_from_slice(b"\nendobj\n");
        }
        let inicio_xref = salida.len();
        salida.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objetos.len() + 1).as_bytes());
        for p in &posiciones {
            salida.extend_from_slice(format!("{p:010} 00000 n \n").as_bytes());
        }
        salida.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R /Info 5 0 R >>\nstartxref\n{inicio_xref}\n%%EOF\n", objetos.len() + 1).as_bytes());
        salida
    }
}

/// Comprobaciones de estructura para las pruebas de los reportes.
#[cfg(test)]
pub(crate) mod comprobar {
    pub fn contiene(pdf: &[u8], texto: &[u8]) -> bool {
        pdf.windows(texto.len()).any(|v| v == texto)
    }

    /// Verifica cabecera, tabla de referencias (cada posición debe apuntar al
    /// objeto que dice) y largo de cada flujo. Devuelve el número de páginas.
    pub fn estructura(pdf: &[u8]) -> usize {
        assert!(pdf.starts_with(b"%PDF-1.4\n"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        let texto = String::from_utf8_lossy(pdf);
        let cola = &texto[texto.rfind("startxref\n").unwrap() + 10..];
        let inicio: usize = cola.lines().next().unwrap().parse().unwrap();
        assert!(pdf[inicio..].starts_with(b"xref\n"));
        let tabla = std::str::from_utf8(&pdf[inicio..]).unwrap();
        let mut renglones = tabla.lines().skip(1);
        let total: usize = renglones.next().unwrap().strip_prefix("0 ").unwrap().parse().unwrap();
        assert_eq!(renglones.next().unwrap(), "0000000000 65535 f ");
        for numero in 1..total {
            let r = renglones.next().unwrap();
            assert!(r.ends_with(" 00000 n "), "{r}");
            let pos: usize = r[..10].parse().unwrap();
            assert!(pdf[pos..].starts_with(format!("{numero} 0 obj\n").as_bytes()), "objeto {numero}");
        }
        // Cada flujo mide lo que declara.
        let mut resto = pdf;
        let mut flujos = 0;
        while let Some(i) = resto.windows(8).position(|v| v == b"/Length ") {
            let desde = i + 8;
            let fin = desde + resto[desde..].iter().position(|b| !b.is_ascii_digit()).unwrap();
            let largo: usize = std::str::from_utf8(&resto[desde..fin]).unwrap().parse().unwrap();
            let datos = fin + resto[fin..].windows(7).position(|v| v == b"stream\n").unwrap() + 7;
            assert!(resto[datos + largo..].starts_with(b"\nendstream"));
            resto = &resto[datos + largo..];
            flujos += 1;
        }
        let paginas = pdf.windows(11).filter(|v| *v == b"/Type /Page").count() - 1;
        assert_eq!(flujos, paginas);
        paginas
    }
}

#[cfg(test)]
mod pruebas {
    use super::comprobar::{contiene, estructura};
    use super::*;

    #[test]
    fn documento_valido_con_varias_paginas() {
        let mut d = Pdf::nuevo(CARTA_ANCHO, CARTA_ALTO);
        d.pagina();
        d.texto(40.0, 50.0, 12.0, Fuente::Negrita, "Título (prueba) \\ año");
        d.linea(40.0, 56.0, 300.0, 56.0, 0.5);
        d.pagina();
        d.texto_derecha(200.0, 50.0, 8.0, Fuente::Normal, "1,234.56");
        let b = d.bytes("Cédula de prueba");
        assert_eq!(estructura(&b), 2);
        assert_eq!(d.paginas(), 2);
        // Acentos en WinAnsi y paréntesis escapados.
        assert!(contiene(&b, b"(T\xedtulo \\(prueba\\) \\\\ a\xf1o) Tj"));
        assert!(contiene(&b, b"/Title (C\xe9dula de prueba)"));
        // 1,234.56 a 8 puntos mide 6 × 556 + 2 × 278 milésimas = 31.14 puntos.
        assert!(contiene(&b, b"/F1 8.00 Tf 168.86 742.00 Td (1,234.56) Tj"));
        assert!(contiene(&b, b"0.50 w 40.00 736.00 m 300.00 736.00 l S"));
    }

    #[test]
    fn un_documento_sin_paginas_sigue_siendo_valido() {
        assert_eq!(estructura(&Pdf::nuevo(CARTA_ANCHO, CARTA_ALTO).bytes("vacío")), 1);
    }

    #[test]
    fn medida_y_recorte_de_texto() {
        assert!((ancho_texto("0123456789", 10.0) - 55.6).abs() < 0.001);
        assert_eq!(ancho_texto("AÑO", 10.0), ancho_texto("ANO", 10.0));
        assert_eq!(recortar("CORTO", 8.0, 100.0), "CORTO");
        let r = recortar("NOMBRE MUY LARGO DE UN TRABAJADOR FICTICIO", 8.0, 80.0);
        assert!(r.ends_with('…') && r.starts_with("NOMBRE MUY") && ancho_texto(&r, 8.0) <= 80.0, "{r}");
        let p = partir("uno dos tres cuatro cinco seis", 10.0, 60.0);
        assert!(p.len() > 1 && p.iter().all(|x| ancho_texto(x, 10.0) <= 60.0) && p.join(" ") == "uno dos tres cuatro cinco seis", "{p:?}");
    }
}
