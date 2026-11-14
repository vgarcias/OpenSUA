//! Cifrado opcional de la base (`ESPECIFICACION_SUA.md`, §11) con SQLCipher.
//!
//! Una base sin contraseña es un archivo SQLite corriente. Con contraseña, todo
//! el archivo va cifrado (AES-256; la clave sale de la contraseña con
//! PBKDF2-HMAC-SHA512) y sin ella no se distingue de un archivo dañado. Por eso
//! junto a la base se deja una marca, `cifrada.txt`, que dice que la base tiene
//! contraseña: sirve para pedirla en vez de dar la base por dañada. La marca no
//! guarda la contraseña ni nada que ayude a adivinarla.
//!
//! La contraseña no se guarda en ningún lado. Si se olvida, no hay forma de
//! recuperar los datos.

use crate::error::{ErrorSua, Resultado};
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};

/// Primeros bytes de todo archivo SQLite sin cifrar.
const MAGIA: &[u8; 16] = b"SQLite format 3\0";
const NOMBRE_MARCA: &str = "cifrada.txt";
/// Para exportar hace falta poder crear el archivo que se adjunta.
pub(crate) const ESCRITURA: OpenFlags = OpenFlags::SQLITE_OPEN_READ_WRITE.union(OpenFlags::SQLITE_OPEN_CREATE).union(OpenFlags::SQLITE_OPEN_NO_MUTEX);
/// Largo mínimo de una contraseña nueva.
pub const LARGO_MINIMO: usize = 8;

fn ruta_marca(ruta_base: &Path) -> PathBuf {
    ruta_base.parent().unwrap_or(Path::new(".")).join(NOMBRE_MARCA)
}

/// `true` si junto a la base está la marca de que tiene contraseña.
pub fn marcada_como_cifrada(ruta_base: &Path) -> bool {
    ruta_marca(ruta_base).exists()
}

/// Pone o quita la marca. No poder escribirla no es un error: solo cambia el
/// mensaje que verá el usuario si un día el archivo no abre.
pub(crate) fn marcar(ruta_base: &Path, cifrada: bool) {
    let marca = ruta_marca(ruta_base);
    if cifrada {
        if !marca.exists() {
            let _ = std::fs::write(&marca, "La base de datos sua.db tiene contrase\u{f1}a. Este archivo solo sirve para que el programa la pida al abrir;\r\nno contiene la contrase\u{f1}a. No lo borre.\r\n");
        }
    } else {
        let _ = std::fs::remove_file(&marca);
    }
}

/// `true` si el archivo existe, no está vacío y no empieza como un SQLite sin
/// cifrar: o tiene contraseña o está dañado.
pub fn cabecera_ilegible(ruta: &Path) -> bool {
    use std::io::Read;
    let mut inicio = [0u8; 16];
    match std::fs::File::open(ruta).and_then(|mut f| f.read(&mut inicio)) {
        Ok(0) | Err(_) => false,
        Ok(n) => n < 16 || &inicio != MAGIA,
    }
}

/// Comprueba que una contraseña nueva se pueda aceptar.
pub fn validar_clave_nueva(clave: &str, repetida: &str) -> Resultado<()> {
    if clave != repetida {
        return Err(ErrorSua::Validacion("Las dos contraseñas no son iguales.".into()));
    }
    if clave.chars().count() < LARGO_MINIMO {
        return Err(ErrorSua::Validacion(format!("La contraseña debe tener al menos {LARGO_MINIMO} caracteres.")));
    }
    if clave.trim() != clave {
        return Err(ErrorSua::Validacion("La contraseña no puede empezar ni terminar con un espacio.".into()));
    }
    Ok(())
}

/// OpenSSL, la biblioteca que cifra, lee por costumbre un archivo de
/// configuración del equipo: el que diga la variable de entorno `OPENSSL_CONF`
/// (la ponen algunos instaladores de otros programas) o el de una carpeta fija.
/// Una configuración ajena puede dejar al programa sin cifrado, y entonces una
/// base con contraseña no abre. Aquí se le dice, antes de usarla por primera
/// vez, que no lea ninguna: el programa no depende de la configuración del equipo.
pub(crate) fn aislar_openssl() {
    extern "C" {
        fn OPENSSL_init_crypto(opciones: u64, ajustes: *const core::ffi::c_void) -> core::ffi::c_int;
    }
    const OPENSSL_INIT_NO_LOAD_CONFIG: u64 = 0x0000_0080;
    static UNA_VEZ: std::sync::Once = std::sync::Once::new();
    // SAFETY: función de inicialización de OpenSSL, pensada para llamarse así; el segundo argumento admite nulo.
    UNA_VEZ.call_once(|| unsafe {
        OPENSSL_init_crypto(OPENSSL_INIT_NO_LOAD_CONFIG, std::ptr::null());
    });
}

fn es_no_legible(e: &rusqlite::Error) -> bool {
    matches!(e, rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::NotADatabase)
}

/// Abre un archivo con su contraseña (o sin ella) y comprueba que se pueda
/// leer. Con contraseña, un archivo ilegible es `ClaveIncorrecta`; sin ella,
/// `FaltaClave` si la base está marcada como cifrada, y si no el error de SQLite
/// (archivo dañado).
pub(crate) fn conectar(ruta: &Path, clave: Option<&str>, banderas: OpenFlags) -> Resultado<Connection> {
    aislar_openssl();
    let conn = Connection::open_with_flags(ruta, banderas)?;
    // SQLCipher escribe en la salida de error cada página que no puede descifrar (una
    // contraseña equivocada son varias líneas): el error ya se informa por su cuenta.
    let _ = conn.execute_batch("PRAGMA cipher_log_level = NONE;");
    if let Some(clave) = clave {
        conn.pragma_update(None, "key", clave)?;
    }
    match conn.query_row("SELECT COUNT(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0)) {
        Ok(_) => Ok(conn),
        Err(e) if es_no_legible(&e) => Err(match clave {
            Some(_) => ErrorSua::ClaveIncorrecta,
            None if marcada_como_cifrada(ruta) => ErrorSua::FaltaClave,
            None => ErrorSua::Sqlite(e),
        }),
        Err(e) => Err(e.into()),
    }
}

fn con_sufijo(ruta: &Path, sufijo: &str) -> PathBuf {
    let mut texto = ruta.as_os_str().to_os_string();
    texto.push(sufijo);
    PathBuf::from(texto)
}

/// Copia todo el contenido de una conexión abierta a un archivo nuevo, cifrado
/// con `clave` o sin cifrar si es `None`. El archivo no debe existir.
pub(crate) fn exportar(conn: &Connection, destino: &Path, clave: Option<&str>) -> Resultado<()> {
    let texto = destino.to_str().ok_or_else(|| ErrorSua::Ejecutable("la ruta de la copia no es texto válido".into()))?;
    conn.execute("ATTACH DATABASE ?1 AS copia KEY ?2", [texto, clave.unwrap_or("")])?;
    let exportado = conn.query_row("SELECT sqlcipher_export('copia')", [], |_| Ok(()));
    let separado = conn.execute_batch("DETACH DATABASE copia");
    exportado?;
    separado?;
    Ok(())
}

/// Pone, cambia o quita la contraseña de una base **cerrada**: `actual` es la
/// que tiene (o `None`), `nueva` la que tendrá (o `None` para dejarla sin
/// cifrar). Se escribe una copia completa con la contraseña nueva, se comprueba
/// que abra y esté íntegra, y solo entonces sustituye al archivo original; si
/// algo falla antes, la base queda como estaba.
pub fn cambiar_clave(ruta: &Path, actual: Option<&str>, nueva: Option<&str>) -> Resultado<()> {
    if actual == nueva {
        return Err(ErrorSua::Validacion("La contraseña nueva es igual a la actual.".into()));
    }
    let nuevo = con_sufijo(ruta, ".nueva");
    let anterior = con_sufijo(ruta, ".anterior");
    let limpiar = |p: &Path| {
        for sufijo in ["", "-journal", "-wal", "-shm"] {
            let _ = std::fs::remove_file(con_sufijo(p, sufijo));
        }
    };
    limpiar(&nuevo);
    limpiar(&anterior);
    let escrito = (|| -> Resultado<()> {
        {
            let origen = conectar(ruta, actual, ESCRITURA)?;
            // Con diario WAL lo pendiente se pasa al archivo antes de copiar.
            let _ = origen.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)");
            exportar(&origen, &nuevo, nueva)?;
        }
        let copia = conectar(&nuevo, nueva, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
        let integridad: String = copia.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if integridad != "ok" {
            return Err(ErrorSua::Integridad(integridad));
        }
        Ok(())
    })();
    if let Err(e) = escrito {
        limpiar(&nuevo);
        return Err(e);
    }
    std::fs::rename(ruta, &anterior).map_err(ErrorSua::archivo(ruta))?;
    if let Err(e) = std::fs::rename(&nuevo, ruta) {
        let _ = std::fs::rename(&anterior, ruta);
        limpiar(&nuevo);
        return Err(ErrorSua::archivo(ruta)(e));
    }
    // El archivo anterior contiene los mismos datos con la protección vieja: no debe quedar.
    for sufijo in ["-journal", "-wal", "-shm"] {
        let _ = std::fs::remove_file(con_sufijo(ruta, sufijo));
    }
    limpiar(&anterior);
    marcar(ruta, nueva.is_some());
    Ok(())
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::config::{CarpetaTemporal, Modo};
    use crate::datos::captura::Patron;
    use crate::datos::Base;

    fn patron(reg: &str) -> Patron {
        Patron { reg_pat: reg.into(), rfc_pat: "XAXX010101000".into(), nom_pat: "PATRON FICTICIO".into(), ..Patron::default() }
    }

    fn abrir(t: &CarpetaTemporal, modo: Modo, clave: Option<&str>) -> Resultado<Base> {
        Base::abrir_en_con_clave(&t.0.join("sua.db"), &t.0.join("respaldos"), modo, 10, clave)
    }

    fn contiene(ruta: &Path, texto: &[u8]) -> bool {
        std::fs::read(ruta).unwrap().windows(texto.len()).any(|v| v == texto)
    }

    #[test]
    fn una_base_nueva_con_clave_queda_cifrada_y_solo_abre_con_ella() {
        let t = CarpetaTemporal::nueva("cif1");
        let ruta = t.0.join("sua.db");
        {
            let b = abrir(&t, Modo::Portable, Some("clave de prueba")).unwrap();
            b.guardar_patron(&patron("Z9900000109")).unwrap();
            assert!(b.cifrada());
        }
        assert!(cabecera_ilegible(&ruta) && marcada_como_cifrada(&ruta));
        // Ni el nombre del patrón ni el de una tabla se leen en el archivo.
        assert!(!contiene(&ruta, b"PATRON FICTICIO") && !contiene(&ruta, b"schema_version"));
        assert!(matches!(abrir(&t, Modo::Portable, None), Err(ErrorSua::FaltaClave)));
        assert!(matches!(abrir(&t, Modo::Portable, Some("otra clave")), Err(ErrorSua::ClaveIncorrecta)));
        // Ningún intento fallido la estropeó.
        let b = abrir(&t, Modo::Portable, Some("clave de prueba")).unwrap();
        assert_eq!((b.patrones().unwrap().len(), b.integridad().unwrap().as_str()), (1, "ok"));
        assert!(b.migraciones_aplicadas.is_empty());
    }

    #[test]
    fn una_base_sin_clave_sigue_siendo_sqlite_corriente() {
        let t = CarpetaTemporal::nueva("cif2");
        let ruta = t.0.join("sua.db");
        {
            let b = abrir(&t, Modo::Portable, None).unwrap();
            b.guardar_patron(&patron("Z9900000109")).unwrap();
            assert!(!b.cifrada());
        }
        assert!(!cabecera_ilegible(&ruta) && !marcada_como_cifrada(&ruta));
        assert!(contiene(&ruta, b"PATRON FICTICIO"));
        // Darle una contraseña a una base que no la tiene no la abre.
        assert!(matches!(abrir(&t, Modo::Portable, Some("clave de prueba")), Err(ErrorSua::ClaveIncorrecta)));
    }

    #[test]
    fn poner_cambiar_y_quitar_la_clave_conserva_los_datos() {
        for modo in [Modo::Portable, Modo::Instalado] {
            let t = CarpetaTemporal::nueva("cif3");
            let ruta = t.0.join("sua.db");
            {
                let b = abrir(&t, modo, None).unwrap();
                b.guardar_patron(&patron("Z9900000109")).unwrap();
            }
            let registros = |clave: Option<&str>| -> Vec<String> {
                let b = abrir(&t, modo, clave).unwrap();
                assert_eq!(b.integridad().unwrap(), "ok");
                assert_eq!(b.modo_diario.to_lowercase(), if modo == Modo::Portable { "delete" } else { "wal" });
                b.patrones().unwrap().into_iter().map(|p| p.reg_pat).collect()
            };
            // Poner.
            cambiar_clave(&ruta, None, Some("primera clave")).unwrap();
            assert!(cabecera_ilegible(&ruta) && marcada_como_cifrada(&ruta) && !contiene(&ruta, b"PATRON FICTICIO"));
            assert_eq!(registros(Some("primera clave")), ["Z9900000109"]);
            // Cambiar: la anterior deja de servir.
            cambiar_clave(&ruta, Some("primera clave"), Some("segunda clave")).unwrap();
            assert!(matches!(abrir(&t, modo, Some("primera clave")), Err(ErrorSua::ClaveIncorrecta)));
            assert_eq!(registros(Some("segunda clave")), ["Z9900000109"]);
            // Quitar.
            cambiar_clave(&ruta, Some("segunda clave"), None).unwrap();
            assert!(!cabecera_ilegible(&ruta) && !marcada_como_cifrada(&ruta));
            assert_eq!(registros(None), ["Z9900000109"]);
            // No quedan copias intermedias con los datos.
            let sobrantes: Vec<String> = std::fs::read_dir(&t.0).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).filter(|n| n.contains("nueva") || n.contains("anterior")).collect();
            assert!(sobrantes.is_empty(), "{sobrantes:?}");
        }
    }

    #[test]
    fn con_la_clave_actual_equivocada_no_se_cambia_nada() {
        let t = CarpetaTemporal::nueva("cif4");
        let ruta = t.0.join("sua.db");
        drop(abrir(&t, Modo::Portable, Some("clave de prueba")).unwrap());
        let antes = std::fs::read(&ruta).unwrap();
        assert!(matches!(cambiar_clave(&ruta, Some("equivocada"), Some("otra nueva")), Err(ErrorSua::ClaveIncorrecta)));
        assert!(matches!(cambiar_clave(&ruta, None, Some("otra nueva")), Err(ErrorSua::FaltaClave)));
        assert!(matches!(cambiar_clave(&ruta, Some("clave de prueba"), Some("clave de prueba")), Err(ErrorSua::Validacion(_))));
        assert_eq!(std::fs::read(&ruta).unwrap(), antes);
        assert!(!con_sufijo(&ruta, ".nueva").exists());
    }

    #[test]
    fn reglas_de_una_clave_nueva() {
        assert!(validar_clave_nueva("contraseña larga", "contraseña larga").is_ok());
        assert!(validar_clave_nueva("ñandú123", "ñandú123").is_ok());
        assert!(matches!(validar_clave_nueva("contraseña larga", "contraseña larg"), Err(ErrorSua::Validacion(m)) if m.contains("iguales")));
        assert!(matches!(validar_clave_nueva("corta", "corta"), Err(ErrorSua::Validacion(m)) if m.contains("al menos")));
        assert!(validar_clave_nueva(" con espacio ", " con espacio ").is_err());
    }

    #[test]
    fn una_clave_con_comillas_no_rompe_nada() {
        let t = CarpetaTemporal::nueva("cif5");
        let clave = "it's \"rara\"; DROP TABLE patron;--";
        drop(abrir(&t, Modo::Portable, Some(clave)).unwrap());
        let b = abrir(&t, Modo::Portable, Some(clave)).unwrap();
        b.guardar_patron(&patron("Z9900000109")).unwrap();
        cambiar_clave(&t.0.join("otra.db"), None, None).unwrap_err();
        drop(b);
        cambiar_clave(&t.0.join("sua.db"), Some(clave), Some("otra 'clave' más")).unwrap();
        assert_eq!(abrir(&t, Modo::Portable, Some("otra 'clave' más")).unwrap().patrones().unwrap().len(), 1);
    }
}
