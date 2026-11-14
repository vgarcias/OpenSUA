//! Reportes en PDF (`ESPECIFICACION_SUA.md`, §9).

pub mod cedula;
pub mod listados;
pub mod pdf;

/// Importe en centavos como `1,234.56`.
pub fn pesos(centavos: i64) -> String {
    let signo = if centavos < 0 { "-" } else { "" };
    let c = centavos.unsigned_abs();
    let entero = (c / 100).to_string();
    let mut s = String::with_capacity(entero.len() + 8);
    for (i, d) in entero.chars().enumerate() {
        if i > 0 && (entero.len() - i) % 3 == 0 {
            s.push(',');
        }
        s.push(d);
    }
    format!("{signo}{s}.{:02}", c % 100)
}

#[cfg(test)]
mod pruebas {
    use super::pesos;

    #[test]
    fn formato_de_importes() {
        assert_eq!(pesos(0), "0.00");
        assert_eq!(pesos(5), "0.05");
        assert_eq!(pesos(123_456), "1,234.56");
        assert_eq!(pesos(100_000_000), "1,000,000.00");
        assert_eq!(pesos(-123_456), "-1,234.56");
    }
}
