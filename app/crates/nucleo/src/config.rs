//! Modo de ejecución y rutas (`ESPECIFICACION_SUA.md`, §2.2 y §2.3).
//!
//! - **Portable:** junto al ejecutable existe `config/settings.toml` con
//!   `portable = true`. Todo se guarda en el árbol de la aplicación.
//! - **Instalado:** no existe ese marcador. Se usa `%APPDATA%\SUA`.
//!
//! Si la carpeta no es escribible no se cambia de modo en silencio: se devuelve
//! `ErrorSua::CarpetaNoEscribible` para que la interfaz pida otra carpeta.

use crate::error::{ErrorSua, Resultado};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

pub const NOMBRE_BASE: &str = "sua.db";
const NOMBRE_CANDADO: &str = "sua.lock";
/// Subcarpeta que se crea dentro de la carpeta que elige el usuario.
pub const CARPETA_DE_DATOS: &str = "SUA-datos";
/// Variable de entorno que fija la carpeta de datos sin preguntar. Sirve para
/// usar el programa desde un medio de solo lectura con un acceso directo.
pub const VARIABLE_DATOS: &str = "SUA_DATOS";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Ajustes {
    /// `true`: los datos se guardan junto al ejecutable.
    pub portable: bool,
    /// Respaldos automáticos que se conservan.
    pub respaldos_a_conservar: u32,
}

impl Default for Ajustes {
    fn default() -> Self {
        Ajustes { portable: false, respaldos_a_conservar: 10 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modo {
    Portable,
    Instalado,
}

#[derive(Debug)]
pub struct Entorno {
    pub modo: Modo,
    pub ajustes: Ajustes,
    pub raiz: PathBuf,
    pub config: PathBuf,
    pub data: PathBuf,
    pub respaldos: PathBuf,
    pub logs: PathBuf,
    pub cache: PathBuf,
    pub reportes: PathBuf,
    pub archivos: PathBuf,
    /// Se mantiene abierto y bloqueado mientras viva el proceso.
    _candado: File,
}

/// Deja de un texto solo lo que puede ir en el nombre de un archivo o carpeta
/// sin salirse de donde se pone: letras y números ASCII, guion y guion bajo.
/// Se usa con datos que vienen de la base (registro patronal, periodo), que a
/// su vez pueden venir de un archivo importado.
pub fn nombre_seguro(texto: &str) -> String {
    let limpio: String = texto.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
    if limpio.is_empty() { "sin-nombre".to_string() } else { limpio }
}

/// Servicio de sincronización en la nube dentro de cuya carpeta está `ruta`,
/// si se reconoce alguno. Una base de datos en una carpeta así puede copiarse
/// a medio escribir o duplicarse cuando se usa en dos equipos.
pub fn servicio_de_sincronizacion(ruta: &Path) -> Option<&'static str> {
    let raices: Vec<PathBuf> = ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"]
        .iter()
        .filter_map(|v| std::env::var_os(v).filter(|x| !x.is_empty()).map(PathBuf::from))
        .collect();
    servicio_en(ruta, &raices)
}

/// Como `servicio_de_sincronizacion`, con las carpetas de OneDrive que declara
/// el sistema dadas aparte. Además se reconoce por el nombre de las carpetas,
/// que es lo único que hay para los demás servicios.
fn servicio_en(ruta: &Path, raices_onedrive: &[PathBuf]) -> Option<&'static str> {
    let partes = |p: &Path| -> Vec<String> { p.components().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).collect() };
    let de_la_ruta = partes(ruta);
    if raices_onedrive.iter().map(|r| partes(r)).any(|r| !r.is_empty() && de_la_ruta.starts_with(&r)) {
        return Some("OneDrive");
    }
    de_la_ruta.iter().find_map(|nombre| match nombre.as_str() {
        "onedrive" => Some("OneDrive"),
        n if n.starts_with("onedrive - ") => Some("OneDrive"),
        "dropbox" => Some("Dropbox"),
        n if n.starts_with("dropbox (") => Some("Dropbox"),
        "google drive" | "googledrive" | "mi unidad" | "my drive" => Some("Google Drive"),
        "iclouddrive" | "icloud drive" => Some("iCloud Drive"),
        _ => None,
    })
}

pub fn carpeta_del_ejecutable() -> Resultado<PathBuf> {
    let exe = std::env::current_exe().map_err(|e| ErrorSua::Ejecutable(e.to_string()))?;
    exe.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| ErrorSua::Ejecutable("el ejecutable no tiene carpeta contenedora".into()))
}

fn ruta_ajustes(carpeta_app: &Path) -> PathBuf {
    carpeta_app.join("config").join("settings.toml")
}

/// Lee `config/settings.toml`. Si no existe, devuelve los valores por defecto
/// (modo instalado).
pub fn leer_ajustes(carpeta_app: &Path) -> Resultado<Ajustes> {
    let ruta = ruta_ajustes(carpeta_app);
    match fs::read_to_string(&ruta) {
        Ok(texto) => toml::from_str(&texto).map_err(|e| ErrorSua::Ajustes(e.to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Ajustes::default()),
        Err(e) => Err(ErrorSua::archivo(ruta)(e)),
    }
}

/// Escribe el marcador de modo portable. Es lo que lleva el paquete `.zip`.
pub fn activar_portable(carpeta_app: &Path) -> Resultado<PathBuf> {
    let mut ajustes = leer_ajustes(carpeta_app)?;
    ajustes.portable = true;
    let ruta = ruta_ajustes(carpeta_app);
    crear(&carpeta_app.join("config"))?;
    let texto = toml::to_string_pretty(&ajustes).map_err(|e| ErrorSua::Ajustes(e.to_string()))?;
    fs::write(&ruta, texto).map_err(|_| ErrorSua::CarpetaNoEscribible(carpeta_app.to_path_buf()))?;
    Ok(ruta)
}

/// Carpeta de datos del modo instalado: `%APPDATA%\SUA` en Windows,
/// `~/Library/Application Support/SUA` en macOS y `$XDG_DATA_HOME/SUA`
/// (o `~/.local/share/SUA`) en Linux.
fn carpeta_del_usuario() -> Resultado<PathBuf> {
    let var = |nombre: &str| std::env::var_os(nombre).filter(|v| !v.is_empty()).map(PathBuf::from);
    let falta = |nombre: &str| ErrorSua::Ejecutable(format!("no está definida la variable {nombre}"));
    if cfg!(windows) {
        return Ok(var("APPDATA").ok_or_else(|| falta("APPDATA"))?.join("SUA"));
    }
    if cfg!(target_os = "macos") {
        return Ok(var("HOME").ok_or_else(|| falta("HOME"))?.join("Library/Application Support/SUA"));
    }
    match var("XDG_DATA_HOME") {
        Some(xdg) => Ok(xdg.join("SUA")),
        None => Ok(var("HOME").ok_or_else(|| falta("HOME"))?.join(".local/share/SUA")),
    }
}

fn crear(dir: &Path) -> Resultado<()> {
    fs::create_dir_all(dir).map_err(|_| ErrorSua::CarpetaNoEscribible(dir.to_path_buf()))
}

impl Entorno {
    /// Prepara el entorno a partir de la carpeta donde está el ejecutable.
    pub fn preparar() -> Resultado<Entorno> {
        let carpeta_app = carpeta_del_ejecutable()?;
        let ajustes = leer_ajustes(&carpeta_app)?;
        let raiz = if let Some(datos) = std::env::var_os(VARIABLE_DATOS).filter(|v| !v.is_empty()) {
            PathBuf::from(datos)
        } else if ajustes.portable {
            carpeta_app
        } else {
            carpeta_del_usuario()?
        };
        Entorno::en(raiz, ajustes)
    }

    /// Prepara el entorno con los datos en la carpeta que eligió el usuario
    /// porque la del programa no se puede escribir (§2.3). Los ajustes se siguen
    /// leyendo de la carpeta del programa. Dentro de la carpeta elegida se usa
    /// una subcarpeta propia, para no regar seis carpetas en, por ejemplo,
    /// Documentos; si lo elegido ya es esa subcarpeta, se usa tal cual.
    pub fn con_datos_en(elegida: &Path) -> Resultado<Entorno> {
        let ajustes = leer_ajustes(&carpeta_del_ejecutable()?)?;
        let ya_es = elegida.file_name().is_some_and(|n| n.eq_ignore_ascii_case(CARPETA_DE_DATOS));
        let raiz = if ya_es { elegida.to_path_buf() } else { elegida.join(CARPETA_DE_DATOS) };
        Entorno::en(raiz, ajustes)
    }

    /// Prepara el entorno sobre una carpeta dada. Lo usan `preparar`, las pruebas
    /// y el caso en que el usuario elige otra carpeta de datos.
    pub fn en(raiz: PathBuf, ajustes: Ajustes) -> Resultado<Entorno> {
        let config = raiz.join("config");
        let data = raiz.join("data");
        let respaldos = data.join("respaldos");
        let logs = raiz.join("logs");
        let cache = raiz.join("cache");
        let reportes = raiz.join("reportes");
        let archivos = raiz.join("archivos");
        for d in [&config, &data, &respaldos, &logs, &cache, &reportes, &archivos] {
            crear(d)?;
        }

        // Instancia única por carpeta de datos. El candado es del sistema
        // operativo: si el proceso muere se libera solo.
        let ruta_candado = data.join(NOMBRE_CANDADO);
        let candado = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&ruta_candado)
            .map_err(|_| ErrorSua::CarpetaNoEscribible(data.clone()))?;
        if candado.try_lock().is_err() {
            return Err(ErrorSua::InstanciaDuplicada(data));
        }

        Ok(Entorno {
            modo: if ajustes.portable { Modo::Portable } else { Modo::Instalado },
            ajustes,
            raiz,
            config,
            data,
            respaldos,
            logs,
            cache,
            reportes,
            archivos,
            _candado: candado,
        })
    }

    pub fn ruta_base(&self) -> PathBuf {
        self.data.join(NOMBRE_BASE)
    }
}

/// Carpeta temporal única para pruebas; se borra al soltarla.
#[cfg(test)]
pub(crate) struct CarpetaTemporal(pub PathBuf);

#[cfg(test)]
impl CarpetaTemporal {
    pub fn nueva(etiqueta: &str) -> Self {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "sua-prueba-{etiqueta}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        CarpetaTemporal(dir)
    }
}

#[cfg(test)]
impl Drop for CarpetaTemporal {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn sin_marcador_es_modo_instalado() {
        let t = CarpetaTemporal::nueva("cfg1");
        assert_eq!(leer_ajustes(&t.0).unwrap(), Ajustes::default());
        assert!(!leer_ajustes(&t.0).unwrap().portable);
    }

    #[test]
    fn el_marcador_activa_el_modo_portable_y_crea_el_arbol() {
        let t = CarpetaTemporal::nueva("cfg2");
        activar_portable(&t.0).unwrap();
        let ajustes = leer_ajustes(&t.0).unwrap();
        assert!(ajustes.portable);
        let e = Entorno::en(t.0.clone(), ajustes).unwrap();
        assert_eq!(e.modo, Modo::Portable);
        for d in ["config", "data", "data/respaldos", "logs", "cache", "reportes", "archivos"] {
            assert!(t.0.join(d).is_dir(), "falta {d}");
        }
        assert!(e.ruta_base().starts_with(&t.0));
    }

    #[test]
    fn segunda_instancia_sobre_la_misma_carpeta_se_rechaza() {
        let t = CarpetaTemporal::nueva("cfg3");
        let primero = Entorno::en(t.0.clone(), Ajustes::default()).unwrap();
        match Entorno::en(t.0.clone(), Ajustes::default()) {
            Err(ErrorSua::InstanciaDuplicada(_)) => {}
            otro => panic!("se esperaba InstanciaDuplicada, se obtuvo {otro:?}"),
        }
        drop(primero);
        Entorno::en(t.0.clone(), Ajustes::default()).unwrap();
    }

    #[test]
    fn la_carpeta_elegida_recibe_una_subcarpeta_propia_y_no_se_anida() {
        let t = CarpetaTemporal::nueva("cfg5");
        let e = Entorno::con_datos_en(&t.0).unwrap();
        assert_eq!(e.raiz, t.0.join(CARPETA_DE_DATOS));
        assert!(e.raiz.join("data").is_dir());
        let raiz = e.raiz.clone();
        drop(e);
        // Elegir después la propia subcarpeta no crea otra dentro.
        assert_eq!(Entorno::con_datos_en(&raiz).unwrap().raiz, raiz);
    }

    #[test]
    fn un_nombre_seguro_no_puede_salir_de_su_carpeta() {
        assert_eq!(nombre_seguro("Z9900000109"), "Z9900000109");
        assert_eq!(nombre_seguro(r"..\..\..\ab"), "ab");
        assert_eq!(nombre_seguro("../../etc"), "etc");
        assert_eq!(nombre_seguro(r"C:\Windows"), "CWindows");
        assert_eq!(nombre_seguro("a b\u{f1}.exe"), "abexe");
        assert_eq!(nombre_seguro(".."), "sin-nombre");
        assert_eq!(nombre_seguro(""), "sin-nombre");
        assert_eq!(nombre_seguro("2008-02_x"), "2008-02_x");
    }

    #[test]
    fn se_reconoce_una_carpeta_sincronizada() {
        // Las rutas de la prueba usan `\`; fuera de Windows el separador es `/`.
        let p = |r: &str| PathBuf::from(if cfg!(windows) { r.to_string() } else { r.replace('\\', "/") });
        let sin = |r: &str| servicio_en(&p(r), &[]);
        assert_eq!(sin(r"C:\Users\ana\OneDrive\Escritorio\OpenSUA-Portable"), Some("OneDrive"));
        assert_eq!(sin(r"C:\Users\ana\OneDrive - Empresa SA\SUA"), Some("OneDrive"));
        assert_eq!(sin(r"D:\Dropbox\OpenSUA-Portable"), Some("Dropbox"));
        assert_eq!(sin(r"C:\Users\ana\Dropbox (Personal)\SUA"), Some("Dropbox"));
        assert_eq!(sin(r"G:\Mi unidad\OpenSUA-Portable"), Some("Google Drive"));
        assert_eq!(sin(r"C:\Users\ana\iCloudDrive\SUA"), Some("iCloud Drive"));
        // Lo que no es: otra carpeta, o un nombre que solo se parece.
        assert_eq!(sin(r"E:\OpenSUA-Portable"), None);
        assert_eq!(sin(r"C:\Users\ana\AppData\Roaming\SUA"), None);
        assert_eq!(sin(r"C:\Respaldo de OneDrive viejo\SUA"), None);
        assert_eq!(sin(r"C:\Dropbox2\SUA"), None);
        // Una carpeta de OneDrive con otro nombre se reconoce por la variable del sistema.
        let raices = [p(r"D:\Nube")];
        assert_eq!(servicio_en(&p(r"d:\nube\OpenSUA-Portable"), &raices), Some("OneDrive"));
        assert_eq!(servicio_en(&p(r"D:\Nubes\OpenSUA-Portable"), &raices), None);
        assert_eq!(servicio_en(Path::new(r"D:\Otra\SUA"), &raices), None);
    }

    #[test]
    fn ajustes_invalidos_se_informan() {
        let t = CarpetaTemporal::nueva("cfg4");
        fs::create_dir_all(t.0.join("config")).unwrap();
        fs::write(t.0.join("config/settings.toml"), "portable = quizas").unwrap();
        assert!(matches!(leer_ajustes(&t.0), Err(ErrorSua::Ajustes(_))));
    }
}
