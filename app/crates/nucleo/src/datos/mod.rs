//! SQLite embebido: apertura segura, migraciones, respaldo y compactación
//! (`ESPECIFICACION_SUA.md`, §5.5 y §12).

pub mod calculos;
pub mod captura;
pub mod cifrado;
pub mod expediente;
pub mod migraciones;
pub mod periodo;
pub mod restaurar;
pub mod tasas;
pub mod usuarios;

use crate::bitacora::marca_utc;
use crate::config::{Entorno, Modo};
use crate::error::{ErrorSua, Resultado};
use rusqlite::{Connection, OpenFlags};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Tabla {
    pub columnas: Vec<String>,
    pub filas: Vec<Vec<String>>,
}

pub struct Base {
    conn: Connection,
    ruta: PathBuf,
    carpeta_respaldos: PathBuf,
    respaldos_a_conservar: u32,
    /// Versiones de esquema aplicadas en esta apertura (vacío si ya estaba al día).
    pub migraciones_aplicadas: Vec<u32>,
    /// Respaldo que se tomó antes de migrar una base ya existente.
    pub respaldo_previo: Option<PathBuf>,
    pub modo_diario: String,
    /// Contraseña con la que se abrió, si la base está cifrada. Hace falta para restaurar.
    clave: Option<String>,
    /// Sesión iniciada, si hay usuarios (`usuarios.rs`).
    sesion: std::cell::RefCell<Option<usuarios::Sesion>>,
    /// `true` si la tabla de usuarios no está vacía: entonces se exige sesión.
    con_usuarios: std::cell::Cell<bool>,
    /// Operaciones con permiso en curso, para anotar solo la más externa.
    profundidad: std::cell::Cell<u32>,
}

/// Transacción que se puede anidar. Es un punto de guardado de SQLite: abierto
/// sin nada alrededor equivale a una transacción; abierto dentro de otro, a un
/// tramo que se puede deshacer sin perder lo anterior. Así una importación de
/// miles de renglones se confirma en disco una sola vez, aunque cada renglón
/// use funciones que abren la suya. Si se suelta sin `commit`, se deshace.
pub(crate) struct Transaccion<'a> {
    conn: &'a Connection,
    abierta: bool,
}

impl Transaccion<'_> {
    pub(crate) fn commit(mut self) -> rusqlite::Result<()> {
        self.conn.execute_batch("RELEASE sua")?;
        self.abierta = false;
        Ok(())
    }
}

impl std::ops::Deref for Transaccion<'_> {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        self.conn
    }
}

impl Drop for Transaccion<'_> {
    fn drop(&mut self) {
        if self.abierta {
            let _ = self.conn.execute_batch("ROLLBACK TO sua; RELEASE sua");
        }
    }
}

/// Respaldos de una carpeta, del más antiguo al más reciente.
pub(crate) fn respaldos_en(carpeta: &Path) -> Resultado<Vec<PathBuf>> {
    let mut lista: Vec<PathBuf> = match fs::read_dir(carpeta) {
        Ok(it) => it
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "db"))
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(ErrorSua::archivo(carpeta)(e)),
    };
    lista.sort(); // el nombre empieza por la fecha: orden cronológico
    Ok(lista)
}

impl Base {
    pub(crate) fn transaccion(&self) -> rusqlite::Result<Transaccion<'_>> {
        self.conn.execute_batch("SAVEPOINT sua")?;
        Ok(Transaccion { conn: &self.conn, abierta: true })
    }

    pub fn abrir(entorno: &Entorno) -> Resultado<Base> {
        Base::abrir_con_clave(entorno, None)
    }

    /// Abre la base con su contraseña, o crea una base nueva cifrada con ella.
    /// Sin contraseña (`None`) la base es un archivo SQLite corriente.
    pub fn abrir_con_clave(entorno: &Entorno, clave: Option<&str>) -> Resultado<Base> {
        Base::abrir_en_con_clave(
            &entorno.ruta_base(),
            &entorno.respaldos,
            entorno.modo,
            entorno.ajustes.respaldos_a_conservar,
            clave,
        )
    }

    pub fn abrir_en(ruta: &Path, carpeta_respaldos: &Path, modo: Modo, respaldos_a_conservar: u32) -> Resultado<Base> {
        Base::abrir_en_con_clave(ruta, carpeta_respaldos, modo, respaldos_a_conservar, None)
    }

    pub fn abrir_en_con_clave(ruta: &Path, carpeta_respaldos: &Path, modo: Modo, respaldos_a_conservar: u32, clave: Option<&str>) -> Resultado<Base> {
        let existia = ruta.exists();
        let conn = cifrado::conectar(ruta, clave, OpenFlags::default())?;
        cifrado::marcar(ruta, clave.is_some());

        // En memorias USB y carpetas sincronizadas WAL es frágil: en modo
        // portable se usa el diario clásico. WAL solo en modo instalado.
        let diario = match modo {
            Modo::Portable => "DELETE",
            Modo::Instalado => "WAL",
        };
        let modo_diario: String = conn.query_row(&format!("PRAGMA journal_mode={diario}"), [], |r| r.get(0))?;
        conn.execute_batch("PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;")?;

        if existia {
            let integridad: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
            if integridad != "ok" {
                return Err(ErrorSua::Integridad(integridad));
            }
        }

        let mut base = Base {
            conn,
            ruta: ruta.to_path_buf(),
            carpeta_respaldos: carpeta_respaldos.to_path_buf(),
            respaldos_a_conservar,
            migraciones_aplicadas: Vec::new(),
            respaldo_previo: None,
            modo_diario,
            clave: clave.map(str::to_string),
            sesion: std::cell::RefCell::new(None),
            con_usuarios: std::cell::Cell::new(false),
            profundidad: std::cell::Cell::new(0),
        };

        let actual = migraciones::version_actual(&base.conn)?;
        if actual > migraciones::version_conocida() {
            return Err(ErrorSua::EsquemaFuturo { encontrada: actual, conocida: migraciones::version_conocida() });
        }
        if existia && actual > 0 && actual < migraciones::version_conocida() {
            base.respaldo_previo = Some(base.respaldar_sin_control(&format!("antes-de-esquema-{}", migraciones::version_conocida()))?);
        }
        base.migraciones_aplicadas = migraciones::migrar(&mut base.conn)?;
        base.releer_usuarios()?;
        base.podar_accesos()?;
        Ok(base)
    }

    pub fn ruta(&self) -> &Path {
        &self.ruta
    }

    /// `true` si la base se abrió con contraseña (el archivo está cifrado).
    pub fn cifrada(&self) -> bool {
        self.clave.is_some()
    }

    /// `true` si `clave` es la contraseña con la que se abrió la base.
    pub fn clave_es(&self, clave: &str) -> bool {
        self.clave.as_deref() == Some(clave)
    }

    pub fn conexion(&self) -> &Connection {
        &self.conn
    }

    pub fn version_esquema(&self) -> Resultado<u32> {
        migraciones::version_actual(&self.conn)
    }

    /// Comprobación completa de integridad. Devuelve `ok` o la descripción del daño.
    pub fn integridad(&self) -> Resultado<String> {
        Ok(self.conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?)
    }

    /// Copia consistente de la base en `data/respaldos/`. Conserva los N más
    /// recientes. La copia la hace SQLite (`VACUUM INTO`), no una copia de archivo.
    pub fn respaldar(&self, motivo: &str) -> Resultado<PathBuf> {
        self.con_permiso(crate::datos::usuarios::Permiso::Respaldar, "Hizo respaldo", || motivo.to_string(), || self.respaldar_sin_control(motivo))
    }

    fn respaldar_sin_control(&self, motivo: &str) -> Resultado<PathBuf> {
        let destino = self.respaldar_sin_podar(motivo)?;
        self.podar_respaldos()?;
        Ok(destino)
    }

    fn respaldar_sin_podar(&self, motivo: &str) -> Resultado<PathBuf> {
        fs::create_dir_all(&self.carpeta_respaldos).map_err(ErrorSua::archivo(&self.carpeta_respaldos))?;
        let marca = marca_utc(SystemTime::now()).replace(['-', ':'], "").replace(' ', "-");
        let limpio: String = motivo.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
        let mut destino = self.carpeta_respaldos.join(format!("sua-{marca}-{limpio}.db"));
        let mut n = 1;
        while destino.exists() {
            n += 1;
            destino = self.carpeta_respaldos.join(format!("sua-{marca}-{limpio}-{n}.db"));
        }
        let texto = destino.to_str().ok_or_else(|| ErrorSua::Ejecutable("la ruta del respaldo no es texto válido".into()))?;
        self.conn.execute("VACUUM INTO ?1", [texto])?;
        Ok(destino)
    }

    pub fn respaldos(&self) -> Resultado<Vec<PathBuf>> {
        respaldos_en(&self.carpeta_respaldos)
    }

    fn podar_respaldos(&self) -> Resultado<()> {
        let lista = self.respaldos()?;
        let conservar = self.respaldos_a_conservar.max(1) as usize;
        if lista.len() > conservar {
            for viejo in &lista[..lista.len() - conservar] {
                fs::remove_file(viejo).map_err(ErrorSua::archivo(viejo))?;
            }
        }
        Ok(())
    }

    /// Compacta la base (equivale a «Compactar» del SUA original).
    pub fn compactar(&self) -> Resultado<()> {
        self.con_permiso(crate::datos::usuarios::Permiso::Administrar, "Compactó la base", || String::new(), || self.compactar_sin_control())
    }

    fn compactar_sin_control(&self) -> Resultado<()> {
        self.conn.execute_batch("VACUUM")?;
        Ok(())
    }

    pub fn catalogos(&self) -> Vec<&'static str> {
        migraciones::CATALOGOS.iter().map(|(n, _)| *n).chain([migraciones::CATALOGO_ZLFN, migraciones::CATALOGO_UMI]).collect()
    }

    pub fn filas_de_catalogo(&self, catalogo: &str) -> Resultado<i64> {
        let sql = format!("SELECT COUNT(*) FROM {}", migraciones::ident(&migraciones::tabla_de_catalogo(catalogo)));
        Ok(self.conn.query_row(&sql, [], |r| r.get(0))?)
    }

    pub fn leer_catalogo(&self, catalogo: &str) -> Resultado<Tabla> {
        let sql = format!("SELECT * FROM {}", migraciones::ident(&migraciones::tabla_de_catalogo(catalogo)));
        let mut stmt = self.conn.prepare(&sql)?;
        let columnas: Vec<String> = stmt.column_names().map(|c| c.to_string()).collect();
        let n = columnas.len();
        let filas = stmt
            .query_map([], |r| (0..n).map(|i| r.get::<_, String>(i)).collect::<Result<Vec<_>, _>>())?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Tabla { columnas, filas })
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::config::CarpetaTemporal;

    fn abrir(t: &CarpetaTemporal, conservar: u32) -> Base {
        Base::abrir_en(&t.0.join("sua.db"), &t.0.join("respaldos"), Modo::Portable, conservar).unwrap()
    }

    #[test]
    fn base_nueva_queda_en_la_ultima_version_con_catalogos() {
        let t = CarpetaTemporal::nueva("db1");
        let b = abrir(&t, 3);
        assert_eq!(b.version_esquema().unwrap(), migraciones::version_conocida());
        assert_eq!(b.migraciones_aplicadas, vec![1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(b.respaldo_previo.is_none());
        assert_eq!(b.modo_diario.to_lowercase(), "delete");
        assert_eq!(b.integridad().unwrap(), "ok");
        assert_eq!(b.catalogos().len(), 23);
        assert_eq!(b.filas_de_catalogo("UMI").unwrap(), 11);
        // La migración 3 añade 2019–2026 a salario mínimo y UMA y crea el catálogo de la frontera norte.
        assert_eq!(b.filas_de_catalogo("SALARIO").unwrap(), 32 + 8);
        assert_eq!(b.filas_de_catalogo("UMA").unwrap(), 3 + 8);
        assert_eq!(b.filas_de_catalogo("SALARIO_ZLFN").unwrap(), 8);
        // Conteos documentados en ESPECIFICACION_SUA.md, sección 5.4.
        for (cat, filas) in [("INPC", 344), ("Municipios", 1580), ("Canamu", 1623), ("Versiones", 58), ("Parametros", 4)] {
            assert_eq!(b.filas_de_catalogo(cat).unwrap(), filas, "{cat}");
        }
        let v = b.leer_catalogo("Versiones").unwrap();
        assert!(v.columnas.iter().any(|c| c == "Reg03Codifica"));
    }

    #[test]
    fn reabrir_no_vuelve_a_migrar_ni_duplica_catalogos() {
        let t = CarpetaTemporal::nueva("db2");
        drop(abrir(&t, 3));
        let b = abrir(&t, 3);
        assert!(b.migraciones_aplicadas.is_empty());
        assert_eq!(b.filas_de_catalogo("INPC").unwrap(), 344);
    }

    #[test]
    fn base_de_version_anterior_se_respalda_antes_de_migrar_y_conserva_datos() {
        let t = CarpetaTemporal::nueva("db3");
        {
            // Simula una base creada por una versión que solo conocía la migración 1:
            // se deshace a mano lo que hicieron las migraciones 2 a 7.
            let b = abrir(&t, 3);
            b.conexion()
                .execute_batch(
                    "DROP TABLE bitacora_accesos; DROP TABLE usuario; DROP TABLE calculo_linea; DROP TABLE calculo; DROP TABLE incapacidades; DROP TABLE movtos; DROP TABLE afiliacion; DROP TABLE asegura;
                     DROP TABLE patron_prima_rt; DROP TABLE patron;
                     DROP TABLE cat_SALARIO_ZLFN; DROP TABLE cat_UMI;
                     DELETE FROM cat_SALARIO WHERE CAST(substr(FEC_INI, 7, 4) AS INTEGER) >= 2019;
                     DELETE FROM cat_UMA WHERE CAST(substr(Fec_Ini, 7, 4) AS INTEGER) >= 2019;
                     UPDATE cat_SALARIO SET FEC_TER = '31/12/2999 12:00:00 a. m.' WHERE FEC_TER LIKE '31/12/2018%';
                     UPDATE cat_UMA SET FEC_TER = '31/12/2999 12:00:00 a. m.' WHERE FEC_TER LIKE '31/01/2019%';
                     DELETE FROM schema_version WHERE version >= 2;
                     CREATE TABLE dato_del_usuario (x TEXT); INSERT INTO dato_del_usuario VALUES ('intacto');",
                )
                .unwrap();
        }
        let b = abrir(&t, 3);
        assert_eq!(b.migraciones_aplicadas, vec![2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(b.filas_de_catalogo("SALARIO").unwrap(), 40);
        let respaldo = b.respaldo_previo.clone().expect("debió respaldar antes de migrar");
        assert!(respaldo.exists());
        let x: String = b.conexion().query_row("SELECT x FROM dato_del_usuario", [], |r| r.get(0)).unwrap();
        assert_eq!(x, "intacto");
        // El respaldo es una base válida y está en la versión anterior.
        let r = Connection::open(&respaldo).unwrap();
        assert_eq!(migraciones::version_actual(&r).unwrap(), 1);
    }

    #[test]
    fn base_de_version_futura_se_rechaza_sin_tocarla() {
        let t = CarpetaTemporal::nueva("db4");
        {
            let b = abrir(&t, 3);
            b.conexion()
                .execute("INSERT INTO schema_version VALUES (999, 'futura', 'x')", [])
                .unwrap();
        }
        let r = Base::abrir_en(&t.0.join("sua.db"), &t.0.join("respaldos"), Modo::Portable, 3);
        assert!(matches!(r, Err(ErrorSua::EsquemaFuturo { encontrada: 999, .. })));
    }

    #[test]
    fn respaldar_conserva_solo_los_mas_recientes() {
        let t = CarpetaTemporal::nueva("db5");
        let b = abrir(&t, 2);
        let mut hechos = Vec::new();
        for i in 0..4 {
            hechos.push(b.respaldar(&format!("manual {i}")).unwrap());
        }
        let quedan = b.respaldos().unwrap();
        assert_eq!(quedan.len(), 2);
        assert!(quedan.contains(&hechos[3]));
        assert!(!hechos[0].exists());
    }

    #[test]
    fn las_tablas_de_captura_validan_claves_e_integridad_referencial() {
        let t = CarpetaTemporal::nueva("db6");
        let b = abrir(&t, 3);
        let c = b.conexion();
        // Registro patronal de longitud incorrecta.
        assert!(c.execute("INSERT INTO patron (reg_pat, rfc_pat, nom_pat) VALUES ('CORTO', 'XAXX010101000', 'PRUEBA')", []).is_err());
        c.execute("INSERT INTO patron (reg_pat, rfc_pat, nom_pat) VALUES ('Z0000000000', 'XAXX010101000', 'PATRON FICTICIO')", []).unwrap();
        // Trabajador de un patrón que no existe.
        assert!(c.execute("INSERT INTO asegura (reg_patr, num_afil, nom_aseg) VALUES ('Y0000000000', '00000000000', 'A$B$C')", []).is_err());
        c.execute("INSERT INTO asegura (reg_patr, num_afil, nom_aseg, sal_imss) VALUES ('Z0000000000', '00000000000', 'A$B$C', 31504)", []).unwrap();
        // No se puede borrar un patrón con trabajadores.
        assert!(c.execute("DELETE FROM patron WHERE reg_pat = 'Z0000000000'", []).is_err());
        // Los importes son enteros (centavos): STRICT rechaza texto.
        assert!(c.execute("UPDATE asegura SET sal_imss = 'cien'", []).is_err());
    }
}
