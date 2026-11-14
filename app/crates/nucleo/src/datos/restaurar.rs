//! Restauración de un respaldo sobre la base en uso.
//!
//! Pasos: se comprueba que el archivo sea una base de este programa, íntegra
//! y de una versión que se conozca; se respalda la base actual; se copia el
//! respaldo encima con la interfaz de copia de SQLite, que lo hace en una
//! sola transacción (si falla, la base queda como estaba); y se aplican las
//! migraciones que le falten al respaldo.

use super::{cifrado, migraciones, Base};
use crate::error::{ErrorSua, Resultado};
use rusqlite::{backup::Backup, OpenFlags};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoRespaldo {
    pub ruta: PathBuf,
    pub bytes: u64,
    /// Versión de esquema del respaldo.
    pub esquema: u32,
    pub patrones: i64,
    pub trabajadores: i64,
    /// `true` si el respaldo está cifrado.
    pub cifrado: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Restauracion {
    /// Copia de la base tal como estaba antes de restaurar.
    pub respaldo_previo: PathBuf,
    pub respaldo: InfoRespaldo,
    /// Migraciones aplicadas después, si el respaldo era de una versión anterior.
    pub migraciones: Vec<u32>,
}

/// Fecha y motivo de un respaldo a partir de su nombre
/// (`sua-AAAAMMDD-HHMMSS-motivo.db`): `AAAA-MM-DD HH:MM:SS UTC · motivo`.
/// Si el nombre no tiene esa forma, se devuelve tal cual.
pub fn etiqueta_de_respaldo(ruta: &Path) -> String {
    let nombre = ruta.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let partes: Vec<&str> = nombre.trim_end_matches(".db").splitn(4, '-').collect();
    match partes.as_slice() {
        ["sua", f, h, motivo] if f.len() == 8 && h.len() == 6 && f.bytes().chain(h.bytes()).all(|b| b.is_ascii_digit()) => {
            format!("{}-{}-{} {}:{}:{} UTC · {motivo}", &f[..4], &f[4..6], &f[6..], &h[..2], &h[2..4], &h[4..])
        }
        _ => nombre,
    }
}

/// Resultado de recuperar una base dañada desde un respaldo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recuperacion {
    /// Respaldo que quedó como base.
    pub respaldo: InfoRespaldo,
    /// Dónde se apartó el archivo dañado.
    pub danada: PathBuf,
}

const SOLO_LECTURA: OpenFlags = OpenFlags::SQLITE_OPEN_READ_ONLY.union(OpenFlags::SQLITE_OPEN_NO_MUTEX);

/// Abre un respaldo en solo lectura, con su contraseña si la tiene, y
/// comprueba que se pueda restaurar.
fn examinar(ruta: &Path, clave: Option<&str>) -> Resultado<InfoRespaldo> {
    let bytes = std::fs::metadata(ruta).map_err(ErrorSua::archivo(ruta))?.len();
    let no_es = || ErrorSua::Validacion("El archivo no es un respaldo de este programa.".into());
    let c = cifrado::conectar(ruta, clave, SOLO_LECTURA).map_err(|e| match e {
        ErrorSua::ClaveIncorrecta => e,
        _ => no_es(),
    })?;
    let integridad: String = c.query_row("PRAGMA integrity_check", [], |r| r.get(0)).map_err(|_| no_es())?;
    if integridad != "ok" {
        return Err(ErrorSua::Integridad(integridad));
    }
    let esquema: Option<u32> = c.query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0)).map_err(|_| no_es())?;
    let esquema = esquema.filter(|v| *v > 0).ok_or_else(no_es)?;
    if esquema > migraciones::version_conocida() {
        return Err(ErrorSua::EsquemaFuturo { encontrada: esquema, conocida: migraciones::version_conocida() });
    }
    // Las tablas de captura existen desde el esquema 2.
    let contar = |tabla: &str| c.query_row(&format!("SELECT COUNT(*) FROM {tabla}"), [], |r| r.get::<_, i64>(0)).unwrap_or(0);
    Ok(InfoRespaldo { ruta: ruta.to_path_buf(), bytes, esquema, patrones: contar("patron"), trabajadores: contar("asegura"), cifrado: clave.is_some() })
}

/// Examina un respaldo probando las contraseñas dadas, en orden. Devuelve
/// también la que sirvió (`None` si el respaldo no está cifrado). Si el archivo
/// no empieza como un SQLite sin cifrar y ninguna contraseña lo abre, el error
/// es `ClaveIncorrecta`.
fn examinar_con(ruta: &Path, claves: &[Option<&str>]) -> Resultado<(InfoRespaldo, Option<String>)> {
    std::fs::metadata(ruta).map_err(ErrorSua::archivo(ruta))?;
    if !cifrado::cabecera_ilegible(ruta) {
        return examinar(ruta, None).map(|i| (i, None));
    }
    for clave in claves.iter().flatten() {
        match examinar(ruta, Some(clave)) {
            Err(ErrorSua::ClaveIncorrecta) => continue,
            otro => return otro.map(|i| (i, Some(clave.to_string()))),
        }
    }
    Err(ErrorSua::ClaveIncorrecta)
}

/// Archivos que SQLite deja junto a una base (diario y, en modo WAL, sus dos auxiliares).
const AUXILIARES: [&str; 3] = ["-journal", "-wal", "-shm"];

fn con_sufijo(ruta: &Path, sufijo: &str) -> PathBuf {
    let mut texto = ruta.as_os_str().to_os_string();
    texto.push(sufijo);
    PathBuf::from(texto)
}

/// El respaldo más reciente de la carpeta que se puede restaurar: íntegro, de
/// una versión conocida y, si está cifrado, que abra con `clave`. Los dañados
/// y los cifrados con otra contraseña se saltan. `None` si no hay ninguno.
pub fn ultimo_respaldo_util(carpeta_respaldos: &Path, clave: Option<&str>) -> Option<InfoRespaldo> {
    super::respaldos_en(carpeta_respaldos).ok()?.iter().rev().find_map(|r| examinar_con(r, &[clave]).ok().map(|(i, _)| i))
}

/// Sustituye una base que no se puede abrir por el respaldo más reciente que
/// sí sirve. La base no debe estar abierta. El archivo dañado no se borra: se
/// aparta junto a la base con el nombre `sua-danada-<fecha>.db`, con su diario
/// si lo tenía (dejarlo en su sitio haría que SQLite lo aplicara al respaldo).
/// Después hay que abrir la base de la manera habitual: queda con la
/// contraseña del respaldo (`clave`), o sin ella si el respaldo no está cifrado.
pub fn recuperar(ruta_base: &Path, carpeta_respaldos: &Path, clave: Option<&str>) -> Resultado<Recuperacion> {
    let respaldo = ultimo_respaldo_util(carpeta_respaldos, clave).ok_or_else(|| {
        ErrorSua::Validacion(format!("No hay ningún respaldo que se pueda restaurar en {}.", carpeta_respaldos.display()))
    })?;
    let carpeta = ruta_base.parent().unwrap_or(Path::new("."));
    let marca = crate::bitacora::marca_utc(std::time::SystemTime::now()).replace(['-', ':'], "").replace(' ', "-");
    let mut danada = carpeta.join(format!("sua-danada-{marca}.db"));
    let mut n = 1;
    while danada.exists() {
        n += 1;
        danada = carpeta.join(format!("sua-danada-{marca}-{n}.db"));
    }
    std::fs::rename(ruta_base, &danada).map_err(ErrorSua::archivo(ruta_base))?;
    for sufijo in AUXILIARES {
        let auxiliar = con_sufijo(ruta_base, sufijo);
        if auxiliar.exists() {
            std::fs::rename(&auxiliar, con_sufijo(&danada, sufijo)).map_err(ErrorSua::archivo(&auxiliar))?;
        }
    }
    if let Err(e) = std::fs::copy(&respaldo.ruta, ruta_base) {
        // Sin copia no hay base: se deja todo como estaba.
        let _ = std::fs::remove_file(ruta_base);
        let _ = std::fs::rename(&danada, ruta_base);
        for sufijo in AUXILIARES {
            let _ = std::fs::rename(con_sufijo(&danada, sufijo), con_sufijo(ruta_base, sufijo));
        }
        return Err(ErrorSua::archivo(&respaldo.ruta)(e));
    }
    cifrado::marcar(ruta_base, respaldo.cifrado);
    Ok(Recuperacion { respaldo, danada })
}

impl Base {
    /// Abre un respaldo en solo lectura y comprueba que se pueda restaurar.
    /// Si está cifrado se prueba con la contraseña de la base en uso.
    pub fn describir_respaldo(&self, ruta: &Path) -> Resultado<InfoRespaldo> {
        self.describir_respaldo_con_clave(ruta, None)
    }

    /// Como `describir_respaldo`, probando antes con `clave_del_respaldo`.
    pub fn describir_respaldo_con_clave(&self, ruta: &Path, clave_del_respaldo: Option<&str>) -> Resultado<InfoRespaldo> {
        self.examinar_ajeno(ruta, clave_del_respaldo).map(|(i, _)| i)
    }

    fn examinar_ajeno(&self, ruta: &Path, clave_del_respaldo: Option<&str>) -> Resultado<(InfoRespaldo, Option<String>)> {
        std::fs::metadata(ruta).map_err(ErrorSua::archivo(ruta))?;
        let mismo = |a: &Path, b: &Path| std::fs::canonicalize(a).ok().zip(std::fs::canonicalize(b).ok()).is_some_and(|(a, b)| a == b);
        if mismo(ruta, &self.ruta) {
            return Err(ErrorSua::Validacion("Ese archivo es la base en uso, no un respaldo.".into()));
        }
        examinar_con(ruta, &[clave_del_respaldo, self.clave.as_deref()])
    }

    /// Respaldos de la carpeta que no están cifrados. Con la base cifrada son
    /// copias de los datos que cualquiera puede leer.
    pub fn respaldos_sin_cifrar(&self) -> Resultado<Vec<PathBuf>> {
        Ok(self.respaldos()?.into_iter().filter(|r| !cifrado::cabecera_ilegible(r)).collect())
    }

    /// Sustituye la base en uso por el contenido de un respaldo. Antes guarda
    /// una copia de la base actual (`antes-de-restaurar`).
    pub fn restaurar(&self, ruta: &Path) -> Resultado<Restauracion> {
        self.restaurar_con_clave(ruta, None)
    }

    /// Como `restaurar`, para un respaldo cifrado con una contraseña distinta
    /// de la de la base en uso (`clave_del_respaldo`). La base conserva su
    /// propia protección: sigue con su contraseña, o sin ella, tenga la que
    /// tenga el respaldo.
    pub fn restaurar_con_clave(&self, ruta: &Path, clave_del_respaldo: Option<&str>) -> Resultado<Restauracion> {
        let nombre = || ruta.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let hecho = self.con_permiso(crate::datos::usuarios::Permiso::Administrar, "Restauró respaldo", nombre, || self.restaurar_sin_control(ruta, clave_del_respaldo))?;
        // Los usuarios y la bitácora son ahora los del respaldo.
        self.revalidar_sesion()?;
        Ok(hecho)
    }

    fn restaurar_sin_control(&self, ruta: &Path, clave_del_respaldo: Option<&str>) -> Resultado<Restauracion> {
        let (respaldo, clave_origen) = self.examinar_ajeno(ruta, clave_del_respaldo)?;
        // Sin podar todavía: al podar se podría borrar justo el respaldo elegido.
        let respaldo_previo = self.respaldar_sin_podar("antes-de-restaurar")?;
        let propia = self.clave.as_deref();
        // La copia de SQLite va página a página y exige la misma protección en
        // los dos lados. Si el respaldo tiene otra, primero se pasa a un archivo
        // temporal con la protección de la base en uso.
        let temporal = (clave_origen.as_deref() != propia).then(|| con_sufijo(&self.ruta, ".restaurando"));
        let copiado = (|| -> Resultado<Vec<u32>> {
            let origen = match &temporal {
                Some(t) => {
                    let _ = std::fs::remove_file(t);
                    cifrado::exportar(&cifrado::conectar(ruta, clave_origen.as_deref(), cifrado::ESCRITURA)?, t, propia)?;
                    cifrado::conectar(t, propia, SOLO_LECTURA)?
                }
                None => cifrado::conectar(ruta, propia, SOLO_LECTURA)?,
            };
            let mut destino = cifrado::conectar(&self.ruta, propia, OpenFlags::default())?;
            Backup::new(&origen, &mut destino)?.run_to_completion(256, Duration::ZERO, None)?;
            migraciones::migrar(&mut destino)
        })();
        if let Some(t) = &temporal {
            let _ = std::fs::remove_file(t);
        }
        let migraciones = copiado?;
        let integridad = self.integridad()?;
        if integridad != "ok" {
            return Err(ErrorSua::Integridad(integridad));
        }
        self.podar_respaldos()?;
        Ok(Restauracion { respaldo_previo, respaldo, migraciones })
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use rusqlite::Connection;
    use crate::config::{CarpetaTemporal, Modo};
    use crate::datos::captura::Patron;

    fn patron(reg: &str) -> Patron {
        Patron { reg_pat: reg.into(), rfc_pat: "XAXX010101000".into(), nom_pat: "PATRON FICTICIO".into(), ..Patron::default() }
    }

    fn registros(b: &Base) -> Vec<String> {
        b.patrones().unwrap().into_iter().map(|p| p.reg_pat).collect()
    }

    #[test]
    fn restaurar_devuelve_la_base_al_momento_del_respaldo() {
        let t = CarpetaTemporal::nueva("res1");
        let b = Base::abrir_en(&t.0.join("sua.db"), &t.0.join("respaldos"), Modo::Portable, 10).unwrap();
        b.guardar_patron(&patron("Z9900000109")).unwrap();
        let respaldo = b.respaldar("manual").unwrap();
        b.guardar_patron(&patron("Z9900000208")).unwrap();
        assert_eq!(registros(&b).len(), 2);

        let info = b.describir_respaldo(&respaldo).unwrap();
        assert_eq!((info.esquema, info.patrones, info.trabajadores), (migraciones::version_conocida(), 1, 0));
        let r = b.restaurar(&respaldo).unwrap();
        assert_eq!(registros(&b), ["Z9900000109"]);
        assert!(r.migraciones.is_empty() && r.respaldo == info);
        assert_eq!(b.integridad().unwrap(), "ok");
        // La base sigue sirviendo para escribir después de restaurar.
        b.guardar_patron(&patron("Z9900000307")).unwrap();
        assert_eq!(registros(&b).len(), 2);
        // Lo que había antes quedó guardado y se puede recuperar.
        assert_eq!(b.describir_respaldo(&r.respaldo_previo).unwrap().patrones, 2);
        b.restaurar(&r.respaldo_previo).unwrap();
        assert_eq!(registros(&b), ["Z9900000109", "Z9900000208"]);
        // Al reabrir, todo sigue ahí.
        drop(b);
        let b = Base::abrir_en(&t.0.join("sua.db"), &t.0.join("respaldos"), Modo::Portable, 10).unwrap();
        assert_eq!(registros(&b).len(), 2);
    }

    /// En modo instalado la base usa diario WAL; el respaldo no. Restaurar debe conservar el WAL.
    #[test]
    fn restaurar_en_modo_instalado_conserva_el_diario_wal() {
        let t = CarpetaTemporal::nueva("res4");
        let b = Base::abrir_en(&t.0.join("sua.db"), &t.0.join("respaldos"), Modo::Instalado, 10).unwrap();
        assert_eq!(b.modo_diario.to_lowercase(), "wal");
        b.guardar_patron(&patron("Z9900000109")).unwrap();
        let respaldo = b.respaldar("manual").unwrap();
        b.guardar_patron(&patron("Z9900000208")).unwrap();
        b.restaurar(&respaldo).unwrap();
        assert_eq!(registros(&b), ["Z9900000109"]);
        let diario: String = b.conexion().query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
        assert_eq!(diario.to_lowercase(), "wal");
        b.guardar_patron(&patron("Z9900000307")).unwrap();
        assert_eq!((registros(&b).len(), b.integridad().unwrap().as_str()), (2, "ok"));
    }

    #[test]
    fn un_respaldo_de_version_anterior_se_migra_al_restaurar() {
        let t = CarpetaTemporal::nueva("res2");
        let b = Base::abrir_en(&t.0.join("sua.db"), &t.0.join("respaldos"), Modo::Portable, 10).unwrap();
        b.guardar_patron(&patron("Z9900000109")).unwrap();
        let respaldo = b.respaldar("manual").unwrap();
        // Se deja el respaldo como lo habría hecho una versión anterior (esquema 5).
        Connection::open(&respaldo)
            .unwrap()
            .execute_batch(
                "DROP TABLE bitacora_accesos; DROP TABLE usuario; DROP TABLE cat_UMI;
                 ALTER TABLE calculo_linea DROP COLUMN amortizacion;
                 ALTER TABLE calculo_linea DROP COLUMN seguro_vivienda;
                 ALTER TABLE calculo_linea DROP COLUMN num_credito;
                 ALTER TABLE calculo DROP COLUMN extemporaneo;
                 DELETE FROM schema_version WHERE version >= 6;",
            )
            .unwrap();
        assert_eq!(b.describir_respaldo(&respaldo).unwrap().esquema, 5);
        b.guardar_patron(&patron("Z9900000208")).unwrap();
        let r = b.restaurar(&respaldo).unwrap();
        assert_eq!(r.migraciones, [6, 7, 8]);
        assert_eq!(b.version_esquema().unwrap(), migraciones::version_conocida());
        assert_eq!(b.filas_de_catalogo("UMI").unwrap(), 11);
        assert_eq!(registros(&b), ["Z9900000109"]);
    }

    #[test]
    fn lo_que_no_es_un_respaldo_valido_se_rechaza_sin_tocar_la_base() {
        let t = CarpetaTemporal::nueva("res3");
        let b = Base::abrir_en(&t.0.join("sua.db"), &t.0.join("respaldos"), Modo::Portable, 10).unwrap();
        b.guardar_patron(&patron("Z9900000109")).unwrap();

        let texto = t.0.join("notas.db");
        std::fs::write(&texto, "esto no es una base de datos, solo texto de prueba").unwrap();
        // Un archivo que no empieza como SQLite puede ser un respaldo cifrado con otra contraseña: no se distingue.
        assert!(matches!(b.restaurar(&texto), Err(ErrorSua::ClaveIncorrecta)));
        assert!(matches!(b.restaurar(&t.0.join("no-existe.db")), Err(ErrorSua::Archivo { .. })));
        assert!(matches!(b.restaurar(&t.0.join("sua.db")), Err(ErrorSua::Validacion(m)) if m.contains("base en uso")));
        // Una base SQLite cualquiera, sin la tabla de versiones.
        let ajena = t.0.join("ajena.db");
        Connection::open(&ajena).unwrap().execute_batch("CREATE TABLE x (y); INSERT INTO x VALUES (1);").unwrap();
        assert!(matches!(b.restaurar(&ajena), Err(ErrorSua::Validacion(_))));
        // Un respaldo de una versión más nueva del programa.
        let futuro = b.respaldar("manual").unwrap();
        Connection::open(&futuro).unwrap().execute("INSERT INTO schema_version VALUES (99, 'futura', '')", []).unwrap();
        assert!(matches!(b.restaurar(&futuro), Err(ErrorSua::EsquemaFuturo { encontrada: 99, .. })));

        assert_eq!(registros(&b), ["Z9900000109"]);
        // Ningún intento fallido dejó un respaldo «antes de restaurar».
        assert!(b.respaldos().unwrap().iter().all(|r| !r.to_string_lossy().contains("antes-de-restaurar")));
    }

    /// Estropea una base cerrada: `desde` 0 borra el encabezado; más adelante, las páginas de datos.
    fn estropear(ruta: &Path, desde: usize) {
        let mut bytes = std::fs::read(ruta).unwrap();
        for b in &mut bytes[desde..] {
            *b = 0xA5;
        }
        std::fs::write(ruta, bytes).unwrap();
    }

    #[test]
    fn una_base_danada_se_recupera_del_ultimo_respaldo_que_sirve() {
        for (caso, desde) in [("encabezado", 0), ("paginas", 4096)] {
            let t = CarpetaTemporal::nueva("rec1");
            let (ruta, respaldos) = (t.0.join("sua.db"), t.0.join("respaldos"));
            let abrir = || Base::abrir_en(&ruta, &respaldos, Modo::Portable, 10);
            let b = abrir().unwrap();
            b.guardar_patron(&patron("Z9900000109")).unwrap();
            let bueno = b.respaldar("manual").unwrap();
            b.guardar_patron(&patron("Z9900000208")).unwrap();
            let reciente = b.respaldar("manual").unwrap();
            assert_ne!(bueno, reciente);
            drop(b);
            // Se dañan la base y el respaldo más reciente.
            estropear(&ruta, desde);
            estropear(&reciente, 0);

            let error = abrir().err().unwrap_or_else(|| panic!("{caso}: la base dañada se abrió"));
            assert!(error.es_base_danada(), "{caso}: {error:?}");
            assert_eq!(ultimo_respaldo_util(&respaldos, None).unwrap().ruta, bueno, "{caso}");
            // Queda además un diario huérfano (se escribe aquí porque SQLite, al intentar abrir, borra el que no entiende).
            std::fs::write(con_sufijo(&ruta, "-journal"), b"resto de un diario").unwrap();

            let r = recuperar(&ruta, &respaldos, None).unwrap();
            assert_eq!((r.respaldo.ruta.clone(), r.respaldo.patrones), (bueno.clone(), 1), "{caso}");
            // El archivo dañado y su diario se apartaron, no se borraron.
            assert!(r.danada.exists() && con_sufijo(&r.danada, "-journal").exists() && !con_sufijo(&ruta, "-journal").exists(), "{caso}");
            let b = abrir().unwrap();
            assert_eq!((registros(&b), b.integridad().unwrap()), (vec!["Z9900000109".to_string()], "ok".to_string()), "{caso}");
            // El respaldo usado sigue en su sitio y la base recuperada admite cambios.
            assert!(bueno.exists());
            b.guardar_patron(&patron("Z9900000307")).unwrap();
        }
    }

    #[test]
    fn sin_respaldo_util_no_se_toca_la_base_danada() {
        let t = CarpetaTemporal::nueva("rec2");
        let (ruta, respaldos) = (t.0.join("sua.db"), t.0.join("respaldos"));
        drop(Base::abrir_en(&ruta, &respaldos, Modo::Portable, 10).unwrap());
        estropear(&ruta, 0);
        let antes = std::fs::read(&ruta).unwrap();
        assert!(ultimo_respaldo_util(&respaldos, None).is_none());
        assert!(matches!(recuperar(&ruta, &respaldos, None), Err(ErrorSua::Validacion(m)) if m.contains("No hay ningún respaldo")));
        assert_eq!(std::fs::read(&ruta).unwrap(), antes);
        assert_eq!(std::fs::read_dir(&t.0).unwrap().count(), 1, "no debe quedar ninguna copia apartada");
    }

    /// Los errores que no son de archivo dañado no ofrecen restaurar.
    #[test]
    fn solo_el_dano_del_archivo_cuenta_como_base_danada() {
        assert!(ErrorSua::Integridad("x".into()).es_base_danada());
        assert!(!ErrorSua::EsquemaFuturo { encontrada: 9, conocida: 7 }.es_base_danada());
        assert!(!ErrorSua::Validacion("x".into()).es_base_danada());
    }

    fn abrir_con(t: &CarpetaTemporal, modo: Modo, clave: Option<&str>) -> Base {
        Base::abrir_en_con_clave(&t.0.join("sua.db"), &t.0.join("respaldos"), modo, 10, clave).unwrap()
    }

    #[test]
    fn los_respaldos_de_una_base_cifrada_salen_cifrados_y_se_restauran() {
        for modo in [Modo::Portable, Modo::Instalado] {
            let t = CarpetaTemporal::nueva("resc1");
            let b = abrir_con(&t, modo, Some("clave de prueba"));
            b.guardar_patron(&patron("Z9900000109")).unwrap();
            let respaldo = b.respaldar("manual").unwrap();
            assert!(cifrado::cabecera_ilegible(&respaldo));
            assert!(!std::fs::read(&respaldo).unwrap().windows(15).any(|v| v == b"PATRON FICTICIO"));
            assert!(b.respaldos_sin_cifrar().unwrap().is_empty());
            b.guardar_patron(&patron("Z9900000208")).unwrap();

            let info = b.describir_respaldo(&respaldo).unwrap();
            assert_eq!((info.patrones, info.cifrado), (1, true));
            let r = b.restaurar(&respaldo).unwrap();
            assert_eq!(registros(&b), ["Z9900000109"]);
            assert!(cifrado::cabecera_ilegible(&r.respaldo_previo));
            // Sigue cifrada, utilizable, y abre de nuevo con la misma contraseña.
            b.guardar_patron(&patron("Z9900000307")).unwrap();
            assert_eq!(b.integridad().unwrap(), "ok");
            drop(b);
            assert!(cifrado::cabecera_ilegible(&t.0.join("sua.db")));
            assert_eq!(registros(&abrir_con(&t, modo, Some("clave de prueba"))).len(), 2);
        }
    }

    /// El respaldo y la base tienen protección distinta: otra contraseña, o una cifrada y el otro no.
    #[test]
    fn restaurar_un_respaldo_con_otra_proteccion_conserva_la_de_la_base() {
        let t = CarpetaTemporal::nueva("resc2");
        let ruta = t.0.join("sua.db");
        // Respaldo sin cifrar, con un patrón.
        let b = abrir_con(&t, Modo::Portable, None);
        b.guardar_patron(&patron("Z9900000109")).unwrap();
        let claro = b.respaldar("manual").unwrap();
        drop(b);
        // La base pasa a tener contraseña y gana un patrón; respaldo cifrado con esa contraseña.
        cifrado::cambiar_clave(&ruta, None, Some("clave vieja")).unwrap();
        let b = abrir_con(&t, Modo::Portable, Some("clave vieja"));
        b.guardar_patron(&patron("Z9900000208")).unwrap();
        let viejo = b.respaldar("manual").unwrap();
        assert_eq!(b.respaldos_sin_cifrar().unwrap(), [claro.clone()]);
        drop(b);
        // Cambia la contraseña y gana otro.
        cifrado::cambiar_clave(&ruta, Some("clave vieja"), Some("clave nueva")).unwrap();
        let b = abrir_con(&t, Modo::Portable, Some("clave nueva"));
        b.guardar_patron(&patron("Z9900000307")).unwrap();
        assert_eq!(registros(&b).len(), 3);

        // El respaldo con la contraseña vieja no abre con la nueva: hay que darla.
        assert!(matches!(b.describir_respaldo(&viejo), Err(ErrorSua::ClaveIncorrecta)));
        assert!(matches!(b.restaurar(&viejo), Err(ErrorSua::ClaveIncorrecta)));
        assert!(matches!(b.restaurar_con_clave(&viejo, Some("no es")), Err(ErrorSua::ClaveIncorrecta)));
        assert_eq!(registros(&b).len(), 3, "un intento fallido no cambia la base");
        assert_eq!(b.describir_respaldo_con_clave(&viejo, Some("clave vieja")).unwrap().patrones, 2);
        b.restaurar_con_clave(&viejo, Some("clave vieja")).unwrap();
        assert_eq!(registros(&b), ["Z9900000109", "Z9900000208"]);
        // El respaldo sin cifrar se restaura sin pedir nada.
        b.restaurar(&claro).unwrap();
        assert_eq!(registros(&b), ["Z9900000109"]);
        b.guardar_patron(&patron("Z9900000406")).unwrap();
        assert_eq!(b.integridad().unwrap(), "ok");
        drop(b);
        // La base sigue cifrada con la contraseña nueva y no quedó el archivo temporal.
        assert!(cifrado::cabecera_ilegible(&ruta) && !con_sufijo(&ruta, ".restaurando").exists());
        let b = abrir_con(&t, Modo::Portable, Some("clave nueva"));
        assert_eq!(registros(&b).len(), 2);

        // Al revés: base sin cifrar, respaldo cifrado.
        let cifrado_reciente = b.respaldar("manual").unwrap();
        drop(b);
        cifrado::cambiar_clave(&ruta, Some("clave nueva"), None).unwrap();
        let b = abrir_con(&t, Modo::Portable, None);
        assert!(matches!(b.restaurar(&cifrado_reciente), Err(ErrorSua::ClaveIncorrecta)));
        b.restaurar_con_clave(&cifrado_reciente, Some("clave nueva")).unwrap();
        assert_eq!(registros(&b).len(), 2);
        drop(b);
        assert!(!cifrado::cabecera_ilegible(&ruta));
    }

    #[test]
    fn una_base_cifrada_danada_se_recupera_con_su_clave() {
        let t = CarpetaTemporal::nueva("resc3");
        let (ruta, respaldos) = (t.0.join("sua.db"), t.0.join("respaldos"));
        let b = abrir_con(&t, Modo::Portable, Some("clave de prueba"));
        b.guardar_patron(&patron("Z9900000109")).unwrap();
        let respaldo = b.respaldar("manual").unwrap();
        drop(b);
        estropear(&ruta, 4096);
        let abrir = |clave| Base::abrir_en_con_clave(&ruta, &respaldos, Modo::Portable, 10, clave);
        // Con contraseña, el daño y una contraseña equivocada no se distinguen.
        assert!(matches!(abrir(None), Err(ErrorSua::FaltaClave)));
        assert!(abrir(Some("clave de prueba")).is_err());
        // Sin la contraseña no hay respaldo que sirva; con ella, sí.
        assert!(ultimo_respaldo_util(&respaldos, None).is_none());
        assert!(ultimo_respaldo_util(&respaldos, Some("otra")).is_none());
        let r = recuperar(&ruta, &respaldos, Some("clave de prueba")).unwrap();
        assert_eq!((r.respaldo.ruta, r.respaldo.cifrado), (respaldo, true));
        assert!(cifrado::marcada_como_cifrada(&ruta));
        assert_eq!(registros(&abrir(Some("clave de prueba")).unwrap()), ["Z9900000109"]);
    }

    #[test]
    fn etiquetas_de_respaldo() {
        assert_eq!(etiqueta_de_respaldo(Path::new("x/sua-20261113-012454-antes-de-esquema-4.db")), "2026-11-13 01:24:54 UTC · antes-de-esquema-4");
        assert_eq!(etiqueta_de_respaldo(Path::new("copia de mi base.db")), "copia de mi base.db");
    }
}
