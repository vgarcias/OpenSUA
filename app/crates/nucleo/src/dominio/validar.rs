//! Validación de identificadores (ver `recursos-validacion-nss-curp-rfc.md`).
//!
//! NSS y CURP se rechazan si el dígito verificador falla. En el RFC un dígito
//! incorrecto es solo advertencia: existen RFC aceptados por el SAT que no
//! pasan el cálculo. Del crédito INFONAVIT solo se valida el formato.

/// Resultado de comprobar un dígito verificador.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Digito {
    Correcto,
    Incorrecto { esperado: char },
    NoEvaluado(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resultado {
    pub formato_valido: bool,
    pub digito: Digito,
    pub mensaje: String,
}

impl Resultado {
    fn formato(msg: &str) -> Self {
        Resultado {
            formato_valido: false,
            digito: Digito::NoEvaluado("formato inválido"),
            mensaje: msg.to_string(),
        }
    }
}

/// Dígito de Luhn (módulo 10) para una cadena de dígitos ASCII.
pub fn luhn(digitos: &str) -> Option<char> {
    if digitos.is_empty() || !digitos.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n = digitos.len();
    let mut suma = 0u32;
    for (i, b) in digitos.bytes().enumerate() {
        let mut d = (b - b'0') as u32;
        if (n - i) % 2 == 1 {
            d *= 2;
            if d > 9 {
                d -= 9;
            }
        }
        suma += d;
    }
    char::from_digit((10 - suma % 10) % 10, 10)
}

pub fn nss(texto: &str) -> Resultado {
    let t = texto.trim();
    if t.len() != 11 || !t.bytes().all(|b| b.is_ascii_digit()) {
        return Resultado::formato("El NSS debe tener exactamente 11 dígitos.");
    }
    let esperado = luhn(&t[..10]).unwrap();
    let capturado = t.as_bytes()[10] as char;
    if esperado == capturado {
        Resultado {
            formato_valido: true,
            digito: Digito::Correcto,
            mensaje: "NSS correcto.".into(),
        }
    } else {
        Resultado {
            formato_valido: true,
            digito: Digito::Incorrecto { esperado },
            mensaje: format!("Dígito verificador incorrecto: se esperaba {esperado}."),
        }
    }
}

const DIC_CURP: &str = "0123456789ABCDEFGHIJKLMNÑOPQRSTUVWXYZ";

pub fn curp(texto: &str) -> Resultado {
    let t: Vec<char> = texto.trim().to_uppercase().chars().collect();
    if t.len() != 18 {
        return Resultado::formato("La CURP debe tener exactamente 18 caracteres.");
    }
    let dic: Vec<char> = DIC_CURP.chars().collect();
    let mut suma = 0usize;
    for (i, c) in t[..17].iter().enumerate() {
        match dic.iter().position(|d| d == c) {
            Some(v) => suma += v * (18 - i),
            None => return Resultado::formato("La CURP contiene caracteres no permitidos."),
        }
    }
    let d = (10 - suma % 10) % 10;
    let esperado = char::from_digit(d as u32, 10).unwrap();
    if t[17] == esperado {
        Resultado {
            formato_valido: true,
            digito: Digito::Correcto,
            mensaje: "CURP correcta.".into(),
        }
    } else {
        Resultado {
            formato_valido: true,
            digito: Digito::Incorrecto { esperado },
            mensaje: format!("Dígito verificador incorrecto: se esperaba {esperado}."),
        }
    }
}

fn valor_rfc(c: char) -> Option<u32> {
    match c {
        '0'..='9' => Some(c as u32 - '0' as u32),
        'A'..='N' => Some(10 + c as u32 - 'A' as u32),
        '&' => Some(24),
        'O'..='Z' => Some(25 + c as u32 - 'O' as u32),
        ' ' => Some(37),
        'Ñ' => Some(38),
        _ => None,
    }
}

/// Dígito verificador del RFC a partir de las 12 posiciones ponderables
/// (persona física: las 12 primeras; persona moral: un espacio + las 11 primeras).
pub fn digito_rfc(doce: &[char]) -> Option<char> {
    if doce.len() != 12 {
        return None;
    }
    let mut suma = 0u32;
    for (i, c) in doce.iter().enumerate() {
        suma += valor_rfc(*c)? * (13 - i as u32);
    }
    Some(match 11 - suma % 11 {
        11 => '0',
        10 => 'A',
        d => char::from_digit(d, 10).unwrap(),
    })
}

pub fn rfc(texto: &str) -> Resultado {
    let t: Vec<char> = texto.trim().to_uppercase().chars().collect();
    let texto_norm: String = t.iter().collect();
    if texto_norm == "XAXX010101000" || texto_norm == "XEXX010101000" {
        return Resultado {
            formato_valido: true,
            digito: Digito::NoEvaluado("RFC genérico"),
            mensaje: "RFC genérico reservado: se acepta sin calcular el dígito.".into(),
        };
    }
    if t.len() == 10 || t.len() == 9 {
        return Resultado {
            formato_valido: true,
            digito: Digito::NoEvaluado("sin homoclave"),
            mensaje: "RFC sin homoclave: no hay dígito verificador que validar.".into(),
        };
    }
    let doce: Vec<char> = match t.len() {
        13 => t[..12].to_vec(),
        12 => std::iter::once(' ').chain(t[..11].iter().copied()).collect(),
        _ => return Resultado::formato("El RFC debe tener 12 o 13 caracteres (o 9 o 10 sin homoclave)."),
    };
    let esperado = match digito_rfc(&doce) {
        Some(e) => e,
        None => return Resultado::formato("El RFC contiene caracteres no permitidos."),
    };
    if *t.last().unwrap() == esperado {
        Resultado {
            formato_valido: true,
            digito: Digito::Correcto,
            mensaje: "RFC correcto.".into(),
        }
    } else {
        Resultado {
            formato_valido: true,
            digito: Digito::Incorrecto { esperado },
            mensaje: format!(
                "Advertencia: el dígito calculado es {esperado}. El SAT acepta algunos RFC que no pasan este cálculo; no se rechaza."
            ),
        }
    }
}

/// Solo formato: 10 dígitos ASCII, tratado como texto, distinto de cero.
/// No calcula el dígito verificador (algoritmo de INFONAVIT sin confirmar).
pub fn credito_infonavit(texto: &str) -> Resultado {
    let t = texto.trim();
    if t.len() != 10 || !t.bytes().all(|b| b.is_ascii_digit()) {
        return Resultado::formato("El número de crédito debe contener exactamente 10 dígitos.");
    }
    if t == "0000000000" {
        return Resultado::formato("El número de crédito no puede ser cero.");
    }
    let prefijo: u32 = t[..2].parse().unwrap();
    let en_rango = (1..=32).contains(&prefijo) || prefijo > 72;
    Resultado {
        formato_valido: true,
        digito: Digito::NoEvaluado("algoritmo de INFONAVIT sin confirmar"),
        mensaje: if en_rango {
            "Formato válido. El dígito verificador no se evalúa.".into()
        } else {
            "Formato válido, pero las dos primeras posiciones están fuera del rango que acepta el SUA (01–32 o mayor que 72).".into()
        },
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn nss_luhn() {
        assert_eq!(luhn("0123456789"), Some('7'));
        assert_eq!(luhn("5004561234"), Some('3'));
        assert_eq!(nss("01234567897").digito, Digito::Correcto);
        assert_eq!(nss("01234567890").digito, Digito::Incorrecto { esperado: '7' });
        assert!(!nss("123").formato_valido);
        assert!(!nss("0123456789A").formato_valido);
    }

    #[test]
    fn curp_ejemplo_renapo() {
        assert_eq!(curp("HEGG560427MVZRRL04").digito, Digito::Correcto);
        assert_eq!(curp("hegg560427mvzrrl04").digito, Digito::Correcto);
        assert_eq!(curp("HEGG560427MVZRRL05").digito, Digito::Incorrecto { esperado: '4' });
        assert!(!curp("HEGG560427").formato_valido);
    }

    #[test]
    fn rfc_fisica_moral_y_especiales() {
        // Persona moral del caso público validate-rfc#10: el cálculo da 3.
        assert_eq!(rfc("LME060822IH3").digito, Digito::Correcto);
        // El SAT también acepta la terminación 5: advertencia, no rechazo.
        let r = rfc("LME060822IH5");
        assert!(r.formato_valido);
        assert_eq!(r.digito, Digito::Incorrecto { esperado: '3' });
        // Genéricos y sin homoclave no se evalúan.
        assert!(matches!(rfc("XAXX010101000").digito, Digito::NoEvaluado(_)));
        assert!(matches!(rfc("XEXX010101000").digito, Digito::NoEvaluado(_)));
        assert!(matches!(rfc("HEGG560427").digito, Digito::NoEvaluado(_)));
        assert!(!rfc("ABC").formato_valido);
    }

    #[test]
    fn credito_solo_formato() {
        let r = credito_infonavit(" 0912345678 ");
        assert!(r.formato_valido);
        assert!(matches!(r.digito, Digito::NoEvaluado(_)));
        assert!(!credito_infonavit("0000000000").formato_valido);
        assert!(!credito_infonavit("123456789").formato_valido);
        assert!(!credito_infonavit("12345A7890").formato_valido);
        assert!(!credito_infonavit("１２３４５６７８９０").formato_valido);
        assert!(credito_infonavit("5012345678").mensaje.contains("fuera del rango"));
    }
}
