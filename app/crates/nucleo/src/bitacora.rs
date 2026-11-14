//! Bitácora de eventos en `logs/app.log`. Texto plano, una línea por evento.
//! No se escriben datos personales (nombres, NSS, CURP, RFC, salarios).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct Bitacora {
    ruta: PathBuf,
}

/// Fecha y hora UTC `AAAA-MM-DD HH:MM:SS` sin dependencias externas.
pub fn marca_utc(t: SystemTime) -> String {
    let s = t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let (dias, resto) = (s.div_euclid(86_400), s.rem_euclid(86_400));
    // Algoritmo de fecha civil de Howard Hinnant.
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let a = yoe + era * 400 + i64::from(m <= 2);
    format!("{a:04}-{m:02}-{d:02} {:02}:{:02}:{:02}", resto / 3600, resto % 3600 / 60, resto % 60)
}

impl Bitacora {
    pub fn en(carpeta_logs: &Path) -> Self {
        Bitacora { ruta: carpeta_logs.join("app.log") }
    }

    pub fn ruta(&self) -> &Path {
        &self.ruta
    }

    fn escribir(&self, nivel: &str, mensaje: &str) {
        // La bitácora nunca debe tumbar la aplicación: los fallos se ignoran.
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&self.ruta) {
            let _ = writeln!(f, "{} UTC [{nivel}] {}", marca_utc(SystemTime::now()), mensaje.replace('\n', " | "));
        }
    }

    pub fn info(&self, mensaje: &str) {
        self.escribir("INFO", mensaje);
    }

    pub fn aviso(&self, mensaje: &str) {
        self.escribir("AVISO", mensaje);
    }

    pub fn error(&self, mensaje: &str) {
        self.escribir("ERROR", mensaje);
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use std::time::Duration;

    #[test]
    fn marca_de_tiempo() {
        assert_eq!(marca_utc(UNIX_EPOCH), "1970-01-01 00:00:00");
        // 12-nov-2026 15:04:05 UTC
        assert_eq!(marca_utc(UNIX_EPOCH + Duration::from_secs(1_794_495_845)), "2026-11-12 15:04:05");
        // 29-feb-2024, año bisiesto
        assert_eq!(marca_utc(UNIX_EPOCH + Duration::from_secs(1_709_164_800)), "2024-02-29 00:00:00");
    }
}
