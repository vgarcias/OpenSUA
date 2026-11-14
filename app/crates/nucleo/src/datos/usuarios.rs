//! Usuarios, roles y bitácora de accesos (`ESPECIFICACION_SUA.md`, §11).
//!
//! El inicio de sesión es opcional: mientras no exista ningún usuario, todo
//! está permitido y no se anota nada. Al crear el primer usuario (que tiene que
//! ser administrador) la base pasa a exigir sesión: sin ella, las funciones
//! que cambian datos devuelven `SesionRequerida`.
//!
//! Los permisos se comprueban aquí, en el núcleo, no en la pantalla: la
//! interfaz deshabilita botones por comodidad, pero quien manda es `exigir`.
//!
//! Qué protege y qué no: los usuarios dicen quién hizo qué y limitan errores.
//! No impiden leer el archivo de la base con otro programa; eso lo hace el
//! cifrado (`cifrado.rs`), que es aparte.

use super::{cifrado, Base};
use crate::bitacora::marca_utc;
use crate::error::{ErrorSua, Resultado};
use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use std::time::SystemTime;

/// Renglones de bitácora que se conservan; al abrir se borran los más antiguos.
const MAXIMO_DE_ACCESOS: i64 = 50_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rol {
    /// Todo, incluidos usuarios, contraseña de la base, restaurar y eliminar.
    Administrador,
    /// Captura, importa, calcula, exporta y respalda.
    Capturista,
    /// Ve, calcula sin guardar y saca reportes.
    Consulta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permiso {
    /// Altas y cambios, movimientos (también eliminarlos, para corregir), importar, guardar cálculos.
    Capturar,
    /// Eliminar patrones y trabajadores.
    Eliminar,
    /// Exportar TXT con los datos.
    Exportar,
    /// Hacer respaldos.
    Respaldar,
    /// Usuarios, bitácora, contraseña de la base, restaurar, compactar.
    Administrar,
}

impl Rol {
    pub const TODOS: [Rol; 3] = [Rol::Administrador, Rol::Capturista, Rol::Consulta];

    pub fn clave(self) -> &'static str {
        match self {
            Rol::Administrador => "administrador",
            Rol::Capturista => "capturista",
            Rol::Consulta => "consulta",
        }
    }

    pub fn nombre(self) -> &'static str {
        match self {
            Rol::Administrador => "Administrador",
            Rol::Capturista => "Capturista",
            Rol::Consulta => "Consulta",
        }
    }

    pub fn de_clave(clave: &str) -> Option<Rol> {
        Rol::TODOS.into_iter().find(|r| r.clave() == clave)
    }

    pub fn puede(self, permiso: Permiso) -> bool {
        match self {
            Rol::Administrador => true,
            Rol::Capturista => matches!(permiso, Permiso::Capturar | Permiso::Exportar | Permiso::Respaldar),
            Rol::Consulta => false,
        }
    }
}

impl Permiso {
    /// Lo que se le dice a quien no lo tiene.
    fn negativa(self) -> &'static str {
        match self {
            Permiso::Capturar => "Su usuario es de consulta: no puede capturar, importar ni guardar.",
            Permiso::Eliminar => "Solo un administrador puede eliminar patrones y trabajadores.",
            Permiso::Exportar => "Su usuario es de consulta: no puede exportar archivos con los datos.",
            Permiso::Respaldar => "Su usuario es de consulta: no puede hacer respaldos.",
            Permiso::Administrar => "Solo un administrador puede hacer esto.",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Usuario {
    pub nombre: String,
    pub nombre_completo: String,
    pub rol: Rol,
    pub activo: bool,
    /// Fecha y hora UTC de alta.
    pub creado: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sesion {
    pub nombre: String,
    pub rol: Rol,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Acceso {
    /// Fecha y hora UTC.
    pub momento: String,
    pub usuario: String,
    pub accion: String,
    pub detalle: String,
}

/// Deja el nombre de usuario como se guarda: sin espacios alrededor y en minúsculas.
pub fn normalizar_nombre(nombre: &str) -> String {
    nombre.trim().to_lowercase()
}

fn validar_nombre(nombre: &str) -> Resultado<()> {
    let largo = nombre.chars().count();
    if !(3..=20).contains(&largo) || !nombre.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-')) {
        return Err(ErrorSua::Validacion("El nombre de usuario debe tener de 3 a 20 caracteres: letras sin acento, números, punto, guion o guion bajo.".into()));
    }
    Ok(())
}

fn huella_de(clave: &str) -> Resultado<String> {
    let sal = SaltString::generate(&mut OsRng);
    Argon2::default().hash_password(clave.as_bytes(), &sal).map(|h| h.to_string()).map_err(|e| ErrorSua::Ejecutable(format!("no se pudo proteger la contraseña: {e}")))
}

fn coincide(clave: &str, huella: &str) -> bool {
    PasswordHash::new(huella).is_ok_and(|h| Argon2::default().verify_password(clave.as_bytes(), &h).is_ok())
}

impl Base {
    /// Cuenta los usuarios y ajusta el estado interno. Se llama al abrir y
    /// después de restaurar un respaldo.
    pub(crate) fn releer_usuarios(&self) -> Resultado<()> {
        let n: i64 = self.conn.query_row("SELECT COUNT(*) FROM usuario", [], |r| r.get(0))?;
        self.con_usuarios.set(n > 0);
        Ok(())
    }

    pub(crate) fn podar_accesos(&self) -> Resultado<()> {
        self.conn.execute("DELETE FROM bitacora_accesos WHERE id <= (SELECT MAX(id) FROM bitacora_accesos) - ?1", [MAXIMO_DE_ACCESOS])?;
        Ok(())
    }

    /// `true` si hay usuarios registrados: entonces hace falta iniciar sesión.
    pub fn hay_usuarios(&self) -> bool {
        self.con_usuarios.get()
    }

    pub fn sesion(&self) -> Option<Sesion> {
        self.sesion.borrow().clone()
    }

    /// `true` si quien usa el programa puede hacer eso: no hay usuarios, o
    /// hay sesión y su rol lo permite.
    pub fn puede(&self, permiso: Permiso) -> bool {
        self.exigir(permiso).is_ok()
    }

    pub fn exigir(&self, permiso: Permiso) -> Resultado<()> {
        if !self.hay_usuarios() {
            return Ok(());
        }
        match &*self.sesion.borrow() {
            None => Err(ErrorSua::SesionRequerida),
            Some(s) if s.rol.puede(permiso) => Ok(()),
            Some(_) => Err(ErrorSua::SinPermiso(permiso.negativa().to_string())),
        }
    }

    /// Anota una acción en la bitácora de accesos con el usuario de la sesión.
    /// Sin usuarios registrados no se anota nada. Un fallo al anotar no
    /// deshace lo que ya se hizo.
    pub fn anotar(&self, accion: &str, detalle: &str) {
        if !self.hay_usuarios() {
            return;
        }
        let usuario = self.sesion.borrow().as_ref().map(|s| s.nombre.clone()).unwrap_or_else(|| "(sin sesión)".to_string());
        self.anotar_como(&usuario, accion, detalle);
    }

    fn anotar_como(&self, usuario: &str, accion: &str, detalle: &str) {
        let _ = self.conn.execute(
            "INSERT INTO bitacora_accesos (momento, usuario, accion, detalle) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![marca_utc(SystemTime::now()), usuario, accion, detalle],
        );
    }

    /// Ejecuta una operación que cambia datos: comprueba el permiso, la hace y,
    /// si salió bien, la anota. Las operaciones que llaman a otras (una
    /// importación guarda cientos de trabajadores) se anotan una sola vez.
    pub(crate) fn con_permiso<T>(&self, permiso: Permiso, accion: &str, detalle: impl FnOnce() -> String, operacion: impl FnOnce() -> Resultado<T>) -> Resultado<T> {
        self.exigir(permiso)?;
        self.profundidad.set(self.profundidad.get() + 1);
        let resultado = operacion();
        self.profundidad.set(self.profundidad.get() - 1);
        if resultado.is_ok() && self.profundidad.get() == 0 && self.hay_usuarios() {
            self.anotar(accion, &detalle());
        }
        resultado
    }

    fn fila_de_usuario(&self, nombre: &str) -> Resultado<Option<(Usuario, String)>> {
        let mut consulta = self.conn.prepare("SELECT nombre, nombre_completo, rol, activo, creado, huella FROM usuario WHERE nombre = ?1")?;
        let mut filas = consulta.query([nombre])?;
        let Some(f) = filas.next()? else { return Ok(None) };
        let rol: String = f.get(2)?;
        let usuario = Usuario {
            nombre: f.get(0)?,
            nombre_completo: f.get(1)?,
            rol: Rol::de_clave(&rol).ok_or_else(|| ErrorSua::Validacion(format!("Rol desconocido en la base: {rol}")))?,
            activo: f.get::<_, i64>(3)? != 0,
            creado: f.get(4)?,
        };
        Ok(Some((usuario, f.get(5)?)))
    }

    /// Lista de usuarios, por nombre. Solo para administradores.
    pub fn usuarios(&self) -> Resultado<Vec<Usuario>> {
        self.exigir(Permiso::Administrar)?;
        let nombres: Vec<String> = self.conn.prepare("SELECT nombre FROM usuario ORDER BY nombre")?.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
        let mut lista = Vec::new();
        for n in nombres {
            if let Some((u, _)) = self.fila_de_usuario(&n)? {
                lista.push(u);
            }
        }
        Ok(lista)
    }

    fn administradores_activos(&self) -> Resultado<i64> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM usuario WHERE rol = 'administrador' AND activo = 1", [], |r| r.get(0))?)
    }

    /// Da de alta un usuario. El primero tiene que ser administrador y queda
    /// con la sesión iniciada: desde ese momento la base exige sesión.
    pub fn crear_usuario(&self, nombre: &str, nombre_completo: &str, rol: Rol, clave: &str, repetida: &str) -> Resultado<Usuario> {
        self.exigir(Permiso::Administrar)?;
        let nombre = normalizar_nombre(nombre);
        validar_nombre(&nombre)?;
        cifrado::validar_clave_nueva(clave, repetida)?;
        let primero = !self.hay_usuarios();
        if primero && rol != Rol::Administrador {
            return Err(ErrorSua::Validacion("El primer usuario tiene que ser administrador.".into()));
        }
        if self.fila_de_usuario(&nombre)?.is_some() {
            return Err(ErrorSua::Validacion(format!("Ya existe el usuario {nombre}.")));
        }
        self.conn.execute(
            "INSERT INTO usuario (nombre, nombre_completo, rol, huella, activo, creado) VALUES (?1, ?2, ?3, ?4, 1, ?5)",
            rusqlite::params![nombre, nombre_completo.trim(), rol.clave(), huella_de(clave)?, marca_utc(SystemTime::now())],
        )?;
        self.con_usuarios.set(true);
        if primero {
            *self.sesion.borrow_mut() = Some(Sesion { nombre: nombre.clone(), rol });
            self.anotar("Creó el primer administrador; desde ahora se pide usuario", &nombre);
        } else {
            self.anotar("Creó usuario", &format!("{nombre} ({})", rol.nombre()));
        }
        Ok(self.fila_de_usuario(&nombre)?.expect("recién creado").0)
    }

    /// Cambia el nombre completo, el rol o el estado de un usuario. No se
    /// puede dejar la base sin ningún administrador activo.
    pub fn modificar_usuario(&self, nombre: &str, nombre_completo: &str, rol: Rol, activo: bool) -> Resultado<()> {
        self.exigir(Permiso::Administrar)?;
        let nombre = normalizar_nombre(nombre);
        let (antes, _) = self.fila_de_usuario(&nombre)?.ok_or_else(|| ErrorSua::Validacion(format!("No existe el usuario {nombre}.")))?;
        let era_administrador = antes.rol == Rol::Administrador && antes.activo;
        let sigue = rol == Rol::Administrador && activo;
        if era_administrador && !sigue && self.administradores_activos()? <= 1 {
            return Err(ErrorSua::Validacion("Es el único administrador activo: no se puede desactivar ni cambiar de rol. Cree antes otro administrador.".into()));
        }
        self.conn.execute(
            "UPDATE usuario SET nombre_completo = ?2, rol = ?3, activo = ?4 WHERE nombre = ?1",
            rusqlite::params![nombre, nombre_completo.trim(), rol.clave(), activo as i64],
        )?;
        self.anotar("Modificó usuario", &format!("{nombre} ({}, {})", rol.nombre(), if activo { "activo" } else { "desactivado" }));
        // Si se cambió a sí mismo, la sesión toma el rol nuevo.
        let propia = self.sesion.borrow().as_ref().is_some_and(|s| s.nombre == nombre);
        if propia {
            *self.sesion.borrow_mut() = Some(Sesion { nombre, rol });
        }
        Ok(())
    }

    /// Un administrador pone una contraseña nueva a cualquier usuario.
    pub fn cambiar_clave_de_usuario(&self, nombre: &str, clave: &str, repetida: &str) -> Resultado<()> {
        self.exigir(Permiso::Administrar)?;
        let nombre = normalizar_nombre(nombre);
        cifrado::validar_clave_nueva(clave, repetida)?;
        let cambiados = self.conn.execute("UPDATE usuario SET huella = ?2 WHERE nombre = ?1", rusqlite::params![nombre, huella_de(clave)?])?;
        if cambiados == 0 {
            return Err(ErrorSua::Validacion(format!("No existe el usuario {nombre}.")));
        }
        self.anotar("Cambió la contraseña de un usuario", &nombre);
        Ok(())
    }

    /// El usuario de la sesión cambia su propia contraseña, dando la actual.
    pub fn cambiar_mi_clave(&self, actual: &str, nueva: &str, repetida: &str) -> Resultado<()> {
        let sesion = self.sesion().ok_or(ErrorSua::SesionRequerida)?;
        let (_, huella) = self.fila_de_usuario(&sesion.nombre)?.ok_or(ErrorSua::SesionRequerida)?;
        if !coincide(actual, &huella) {
            return Err(ErrorSua::Validacion("La contraseña actual no es correcta.".into()));
        }
        cifrado::validar_clave_nueva(nueva, repetida)?;
        self.conn.execute("UPDATE usuario SET huella = ?2 WHERE nombre = ?1", rusqlite::params![sesion.nombre, huella_de(nueva)?])?;
        self.anotar("Cambió su contraseña", "");
        Ok(())
    }

    /// Comprueba usuario y contraseña y abre la sesión. El error es el mismo
    /// si el usuario no existe, está desactivado o la contraseña no coincide.
    pub fn iniciar_sesion(&self, nombre: &str, clave: &str) -> Resultado<Sesion> {
        let nombre = normalizar_nombre(nombre);
        let fila = self.fila_de_usuario(&nombre)?;
        let valido = match &fila {
            Some((u, huella)) => coincide(clave, huella) && u.activo,
            None => {
                // Mismo trabajo que con un usuario que existe: no se delata cuáles hay por el tiempo que tarda.
                let _ = huella_de(clave);
                false
            }
        };
        if !valido {
            self.anotar_como(&nombre.chars().take(20).collect::<String>(), "Intento de entrada rechazado", "");
            return Err(ErrorSua::AccesoDenegado);
        }
        let sesion = Sesion { nombre, rol: fila.expect("validado").0.rol };
        *self.sesion.borrow_mut() = Some(sesion.clone());
        self.anotar("Entró", sesion.rol.nombre());
        Ok(sesion)
    }

    pub fn cerrar_sesion(&self) {
        if self.sesion.borrow().is_some() {
            self.anotar("Salió", "");
            *self.sesion.borrow_mut() = None;
        }
    }

    /// Después de restaurar un respaldo los usuarios son los del respaldo.
    /// Devuelve `true` si la sesión sigue valiendo (el mismo usuario existe y
    /// está activo; toma el rol que tenga ahí) o si ya no hay usuarios.
    /// Con `false` la sesión queda cerrada y hay que volver a entrar.
    pub fn revalidar_sesion(&self) -> Resultado<bool> {
        self.releer_usuarios()?;
        if !self.hay_usuarios() {
            *self.sesion.borrow_mut() = None;
            return Ok(true);
        }
        let actual = self.sesion();
        let vigente = match actual {
            Some(s) => self.fila_de_usuario(&s.nombre)?.filter(|(u, _)| u.activo).map(|(u, _)| Sesion { nombre: u.nombre, rol: u.rol }),
            None => None,
        };
        let vale = vigente.is_some();
        *self.sesion.borrow_mut() = vigente;
        Ok(vale)
    }

    /// Pone una contraseña nueva a un administrador **sin sesión**, y lo
    /// reactiva si estaba desactivado. Es la salida para una contraseña
    /// olvidada y solo la ofrece la línea de comandos: quien puede ejecutarla
    /// ya tiene el archivo de la base (y su contraseña, si está cifrada).
    pub fn restablecer_administrador(&self, nombre: &str, clave: &str, repetida: &str) -> Resultado<()> {
        let nombre = normalizar_nombre(nombre);
        let (usuario, _) = self.fila_de_usuario(&nombre)?.ok_or_else(|| ErrorSua::Validacion(format!("No existe el usuario {nombre}.")))?;
        if usuario.rol != Rol::Administrador {
            return Err(ErrorSua::Validacion(format!("{nombre} no es administrador. Restablezca la contraseña de un administrador y, con él, cambie la de los demás.")));
        }
        cifrado::validar_clave_nueva(clave, repetida)?;
        self.conn.execute("UPDATE usuario SET huella = ?2, activo = 1 WHERE nombre = ?1", rusqlite::params![nombre, huella_de(clave)?])?;
        self.anotar_como("(línea de comandos)", "Restableció la contraseña de un administrador sin iniciar sesión", &nombre);
        Ok(())
    }

    /// Nombres de los administradores, para decir a quién se le puede restablecer la contraseña.
    pub fn nombres_de_administradores(&self) -> Resultado<Vec<String>> {
        Ok(self.conn.prepare("SELECT nombre FROM usuario WHERE rol = 'administrador' ORDER BY nombre")?.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?)
    }

    /// Los últimos renglones de la bitácora de accesos, del más reciente al
    /// más antiguo. Solo para administradores.
    pub fn accesos(&self, limite: u32) -> Resultado<Vec<Acceso>> {
        self.exigir(Permiso::Administrar)?;
        let mut consulta = self.conn.prepare("SELECT momento, usuario, accion, detalle FROM bitacora_accesos ORDER BY id DESC LIMIT ?1")?;
        let filas = consulta.query_map([limite], |r| Ok(Acceso { momento: r.get(0)?, usuario: r.get(1)?, accion: r.get(2)?, detalle: r.get(3)? }))?;
        Ok(filas.collect::<Result<_, _>>()?)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::config::{CarpetaTemporal, Modo};
    use crate::datos::captura::{Patron, Trabajador};
    use crate::io::txt::Clase;

    const CLAVE: &str = "clave de prueba";

    fn abrir(t: &CarpetaTemporal) -> Base {
        Base::abrir_en(&t.0.join("sua.db"), &t.0.join("respaldos"), Modo::Portable, 10).unwrap()
    }

    fn patron(reg: &str) -> Patron {
        Patron { reg_pat: reg.into(), rfc_pat: "XAXX010101000".into(), nom_pat: "PATRON FICTICIO".into(), ..Patron::default() }
    }

    fn trabajador(reg: &str) -> Trabajador {
        Trabajador { reg_patr: reg.into(), num_afil: "90000000019".into(), nom_aseg: "EJEMPLO$UNO$ANA".into(), sal_imss: 31_504, fec_alt: "2026-01-01".into(), ..Trabajador::default() }
    }

    /// Base con un administrador, un capturista y un usuario de consulta; queda la sesión del administrador.
    fn con_tres_usuarios(t: &CarpetaTemporal) -> Base {
        let b = abrir(t);
        b.crear_usuario("Admin", "Ana Administradora", Rol::Administrador, CLAVE, CLAVE).unwrap();
        b.crear_usuario("captura", "", Rol::Capturista, CLAVE, CLAVE).unwrap();
        b.crear_usuario("consulta", "", Rol::Consulta, CLAVE, CLAVE).unwrap();
        b
    }

    fn acciones(b: &Base) -> Vec<String> {
        b.accesos(100).unwrap().into_iter().rev().map(|a| format!("{}: {}", a.usuario, a.accion)).collect()
    }

    #[test]
    fn sin_usuarios_todo_esta_permitido_y_no_se_anota_nada() {
        let t = CarpetaTemporal::nueva("usu1");
        let b = abrir(&t);
        assert!(!b.hay_usuarios() && b.sesion().is_none());
        assert!(b.puede(Permiso::Administrar) && b.puede(Permiso::Eliminar));
        b.guardar_patron(&patron("Z9900000109")).unwrap();
        b.eliminar_patron("Z9900000109").unwrap();
        assert!(b.usuarios().unwrap().is_empty() && b.accesos(10).unwrap().is_empty());
    }

    #[test]
    fn el_primer_usuario_es_administrador_y_queda_con_la_sesion() {
        let t = CarpetaTemporal::nueva("usu2");
        let b = abrir(&t);
        assert!(matches!(b.crear_usuario("captura", "", Rol::Capturista, CLAVE, CLAVE), Err(ErrorSua::Validacion(m)) if m.contains("primer usuario")));
        assert!(matches!(b.crear_usuario("ad", "", Rol::Administrador, CLAVE, CLAVE), Err(ErrorSua::Validacion(m)) if m.contains("3 a 20")));
        assert!(matches!(b.crear_usuario("admin", "", Rol::Administrador, "corta", "corta"), Err(ErrorSua::Validacion(m)) if m.contains("al menos")));
        assert!(matches!(b.crear_usuario("admin", "", Rol::Administrador, CLAVE, "otra cosa"), Err(ErrorSua::Validacion(m)) if m.contains("iguales")));
        assert!(!b.hay_usuarios());
        let u = b.crear_usuario("  Admin ", " Ana ", Rol::Administrador, CLAVE, CLAVE).unwrap();
        assert_eq!((u.nombre.as_str(), u.nombre_completo.as_str(), u.rol, u.activo), ("admin", "Ana", Rol::Administrador, true));
        assert_eq!(b.sesion(), Some(Sesion { nombre: "admin".into(), rol: Rol::Administrador }));
        assert!(matches!(b.crear_usuario("ADMIN", "", Rol::Consulta, CLAVE, CLAVE), Err(ErrorSua::Validacion(m)) if m.contains("Ya existe")));
        // La contraseña no queda escrita en la base: solo su huella.
        let huella: String = b.conexion().query_row("SELECT huella FROM usuario", [], |r| r.get(0)).unwrap();
        assert!(huella.starts_with("$argon2") && !huella.contains(CLAVE));
        drop(b);
        // Al volver a abrir hay usuarios y no hay sesión: nada se puede cambiar hasta entrar.
        let b = abrir(&t);
        assert!(b.hay_usuarios() && b.sesion().is_none());
        assert!(matches!(b.guardar_patron(&patron("Z9900000109")), Err(ErrorSua::SesionRequerida)));
        assert!(matches!(b.usuarios(), Err(ErrorSua::SesionRequerida)));
        assert!(matches!(b.iniciar_sesion("admin", "no es"), Err(ErrorSua::AccesoDenegado)));
        assert!(matches!(b.iniciar_sesion("nadie", CLAVE), Err(ErrorSua::AccesoDenegado)));
        assert_eq!(b.iniciar_sesion(" ADMIN ", CLAVE).unwrap().rol, Rol::Administrador);
        b.guardar_patron(&patron("Z9900000109")).unwrap();
    }

    #[test]
    fn cada_rol_puede_lo_suyo_y_nada_mas() {
        let t = CarpetaTemporal::nueva("usu3");
        let b = con_tres_usuarios(&t);
        let reg = "Z9900000109";
        b.guardar_patron(&patron(reg)).unwrap();
        let sin_permiso = |r: Resultado<()>| matches!(r, Err(ErrorSua::SinPermiso(_)));

        // Consulta: ve y calcula, no cambia nada ni saca los datos.
        b.iniciar_sesion("consulta", CLAVE).unwrap();
        assert_eq!(b.patrones().unwrap().len(), 1);
        b.calcular_periodo(reg, "202601", false).unwrap();
        assert!(sin_permiso(b.guardar_patron(&patron("Z9900000208")).map(|_| ())));
        assert!(sin_permiso(b.guardar_trabajador(&trabajador(reg)).map(|_| ())));
        assert!(sin_permiso(b.guardar_movimiento(reg, "90000000019", "02", "2026-02-01", 0)));
        assert!(sin_permiso(b.guardar_inpc("202602", "140.000", "1.47")));
        assert!(sin_permiso(b.importar_txt(Clase::Trabajadores, b"").map(|_| ())));
        assert!(sin_permiso(b.exportar_txt(Clase::Trabajadores, reg).map(|_| ())));
        assert!(sin_permiso(b.respaldar("manual").map(|_| ())));
        assert!(sin_permiso(b.eliminar_patron(reg)));
        assert!(sin_permiso(b.compactar()));
        assert!(sin_permiso(b.usuarios().map(|_| ())) && sin_permiso(b.accesos(10).map(|_| ())));
        assert!(sin_permiso(b.crear_usuario("otro", "", Rol::Consulta, CLAVE, CLAVE).map(|_| ())));

        // Capturista: captura, exporta y respalda; no elimina patrones ni trabajadores ni administra.
        b.iniciar_sesion("captura", CLAVE).unwrap();
        b.guardar_trabajador(&trabajador(reg)).unwrap();
        b.exportar_txt(Clase::Trabajadores, reg).unwrap();
        let respaldo = b.respaldar("manual").unwrap();
        assert!(sin_permiso(b.eliminar_trabajador(reg, "90000000019")));
        assert!(sin_permiso(b.eliminar_patron(reg)));
        assert!(sin_permiso(b.restaurar(&respaldo).map(|_| ())));
        assert!(sin_permiso(b.compactar()));
        assert!(sin_permiso(b.modificar_usuario("captura", "", Rol::Administrador, true)));
        assert!(sin_permiso(b.cambiar_clave_de_usuario("admin", CLAVE, CLAVE)));

        // Administrador: todo.
        b.iniciar_sesion("admin", CLAVE).unwrap();
        b.eliminar_trabajador(reg, "90000000019").unwrap();
        b.eliminar_patron(reg).unwrap();
        b.compactar().unwrap();
        assert_eq!(b.usuarios().unwrap().len(), 3);
    }

    #[test]
    fn la_bitacora_dice_quien_hizo_que_sin_datos_personales() {
        let t = CarpetaTemporal::nueva("usu4");
        let b = con_tres_usuarios(&t);
        let reg = "Z9900000109";
        b.guardar_patron(&patron(reg)).unwrap();
        b.iniciar_sesion("captura", CLAVE).unwrap();
        b.guardar_trabajador(&trabajador(reg)).unwrap();
        let _ = b.eliminar_patron(reg);
        let _ = b.iniciar_sesion("captura", "equivocada");
        b.cerrar_sesion();
        b.iniciar_sesion("admin", CLAVE).unwrap();
        assert_eq!(
            acciones(&b),
            [
                "admin: Creó el primer administrador; desde ahora se pide usuario",
                "admin: Creó usuario",
                "admin: Creó usuario",
                "admin: Guardó patrón",
                "captura: Entró",
                "captura: Guardó trabajador",
                "captura: Intento de entrada rechazado",
                "captura: Salió",
                "admin: Entró",
            ]
        );
        // Lo que no se pudo hacer no figura como hecho, y no hay nombres de personas.
        let todo: String = b.accesos(100).unwrap().iter().map(|a| format!("{} {} {}|", a.usuario, a.accion, a.detalle)).collect();
        assert!(!todo.contains("Eliminó") && !todo.contains("EJEMPLO") && !todo.contains("PATRON FICTICIO"));
        assert!(todo.contains("Z9900000109 90000000019"));
    }

    #[test]
    fn una_importacion_se_anota_una_sola_vez() {
        let t = CarpetaTemporal::nueva("usu5");
        let b = con_tres_usuarios(&t);
        let origen = CarpetaTemporal::nueva("usu5b");
        let o = abrir(&origen);
        o.guardar_patron(&patron("Z9900000109")).unwrap();
        o.guardar_trabajador(&trabajador("Z9900000109")).unwrap();
        let (patrones, _) = o.exportar_txt(Clase::Patrones, "Z9900000109").unwrap();
        let (trabajadores, _) = o.exportar_txt(Clase::Trabajadores, "Z9900000109").unwrap();
        b.importar_txt(Clase::Patrones, &patrones).unwrap();
        b.importar_txt(Clase::Trabajadores, &trabajadores).unwrap();
        assert_eq!(b.trabajadores_de("Z9900000109").unwrap(), 1);
        let hechas = acciones(&b);
        assert_eq!(hechas[3..], ["admin: Importó TXT", "admin: Importó TXT"], "{hechas:?}");
    }

    #[test]
    fn siempre_queda_un_administrador_activo() {
        let t = CarpetaTemporal::nueva("usu6");
        let b = con_tres_usuarios(&t);
        assert!(matches!(b.modificar_usuario("admin", "", Rol::Administrador, false), Err(ErrorSua::Validacion(m)) if m.contains("único administrador")));
        assert!(matches!(b.modificar_usuario("admin", "", Rol::Consulta, true), Err(ErrorSua::Validacion(m)) if m.contains("único administrador")));
        // Con un segundo administrador, el primero sí se puede desactivar; y deja de poder entrar.
        b.modificar_usuario("captura", "Carlos", Rol::Administrador, true).unwrap();
        b.modificar_usuario("admin", "", Rol::Administrador, false).unwrap();
        assert!(matches!(b.iniciar_sesion("admin", CLAVE), Err(ErrorSua::AccesoDenegado)));
        let lista = b.iniciar_sesion("captura", CLAVE).and_then(|_| b.usuarios()).unwrap();
        assert_eq!(lista.iter().map(|u| (u.nombre.as_str(), u.rol, u.activo)).collect::<Vec<_>>(), [("admin", Rol::Administrador, false), ("captura", Rol::Administrador, true), ("consulta", Rol::Consulta, true)]);
    }

    #[test]
    fn contrasenas_de_usuario_cambiar_y_restablecer() {
        let t = CarpetaTemporal::nueva("usu7");
        let b = con_tres_usuarios(&t);
        // El administrador le pone otra a un usuario.
        b.cambiar_clave_de_usuario("consulta", "nueva de consulta", "nueva de consulta").unwrap();
        assert!(b.iniciar_sesion("consulta", CLAVE).is_err());
        b.iniciar_sesion("consulta", "nueva de consulta").unwrap();
        // Cada quien cambia la suya dando la actual.
        assert!(matches!(b.cambiar_mi_clave("no es", "otra más larga", "otra más larga"), Err(ErrorSua::Validacion(m)) if m.contains("actual")));
        b.cambiar_mi_clave("nueva de consulta", "otra más larga", "otra más larga").unwrap();
        b.cerrar_sesion();
        b.iniciar_sesion("consulta", "otra más larga").unwrap();
        b.cerrar_sesion();
        // Contraseña olvidada del administrador: se restablece sin sesión, solo a administradores, y queda anotado.
        assert!(matches!(b.restablecer_administrador("consulta", CLAVE, CLAVE), Err(ErrorSua::Validacion(m)) if m.contains("no es administrador")));
        assert!(b.restablecer_administrador("nadie", CLAVE, CLAVE).is_err());
        b.restablecer_administrador("admin", "restablecida 1", "restablecida 1").unwrap();
        assert!(b.iniciar_sesion("admin", CLAVE).is_err());
        b.iniciar_sesion("admin", "restablecida 1").unwrap();
        assert!(acciones(&b).iter().any(|a| a == "(línea de comandos): Restableció la contraseña de un administrador sin iniciar sesión"));
        assert_eq!(b.nombres_de_administradores().unwrap(), ["admin"]);
    }

    #[test]
    fn tras_restaurar_valen_los_usuarios_del_respaldo() {
        let t = CarpetaTemporal::nueva("usu8");
        let b = abrir(&t);
        let sin_usuarios = b.respaldar("manual").unwrap();
        b.crear_usuario("admin", "", Rol::Administrador, CLAVE, CLAVE).unwrap();
        let con_admin = b.respaldar("manual").unwrap();
        b.crear_usuario("otro", "", Rol::Administrador, CLAVE, CLAVE).unwrap();
        b.iniciar_sesion("otro", CLAVE).unwrap();
        // El respaldo no conoce a «otro»: la sesión deja de valer y nada se puede cambiar hasta entrar.
        b.restaurar(&con_admin).unwrap();
        assert!(!b.revalidar_sesion().unwrap() && b.sesion().is_none() && b.hay_usuarios());
        assert!(matches!(b.guardar_patron(&patron("Z9900000109")), Err(ErrorSua::SesionRequerida)));
        b.iniciar_sesion("admin", CLAVE).unwrap();
        assert!(b.revalidar_sesion().unwrap());
        // Un respaldo anterior a los usuarios deja la base sin inicio de sesión.
        b.restaurar(&sin_usuarios).unwrap();
        assert!(b.revalidar_sesion().unwrap() && !b.hay_usuarios() && b.sesion().is_none());
        b.guardar_patron(&patron("Z9900000109")).unwrap();
    }
}
