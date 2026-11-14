use std::path::PathBuf;

pub type Resultado<T> = Result<T, ErrorSua>;

#[derive(Debug, thiserror::Error)]
pub enum ErrorSua {
    #[error("No se pudo ubicar el ejecutable: {0}")]
    Ejecutable(String),
    #[error("No se puede escribir en la carpeta {0}.")]
    CarpetaNoEscribible(PathBuf),
    #[error("Ya hay otra copia de la aplicación usando la carpeta de datos {0}")]
    InstanciaDuplicada(PathBuf),
    #[error("{ruta}: {origen}")]
    Archivo {
        ruta: PathBuf,
        #[source]
        origen: std::io::Error,
    },
    #[error("settings.toml no es válido: {0}")]
    Ajustes(String),
    #[error("Base de datos: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("La base de datos no pasó la comprobación de integridad: {0}")]
    Integridad(String),
    #[error("La base de datos está protegida con contraseña.")]
    FaltaClave,
    #[error("La contraseña no es correcta, o el archivo de la base está dañado.")]
    ClaveIncorrecta,
    #[error("Hay que iniciar sesión con un usuario para hacer esto.")]
    SesionRequerida,
    #[error("Usuario o contraseña incorrectos.")]
    AccesoDenegado,
    #[error("{0}")]
    SinPermiso(String),
    #[error("La base es de una versión más nueva del programa (esquema {encontrada}, este programa conoce hasta {conocida}).")]
    EsquemaFuturo { encontrada: u32, conocida: u32 },
    #[error("Catálogo {catalogo}: {detalle}")]
    Semilla { catalogo: String, detalle: String },
    #[error("{0}")]
    Validacion(String),
    #[error("Archivo de pago: {0}")]
    Pago(#[from] crate::io::pago::ErrorPago),
}

impl ErrorSua {
    pub fn archivo(ruta: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> ErrorSua {
        let ruta = ruta.into();
        move |origen| ErrorSua::Archivo { ruta, origen }
    }

    /// El archivo de la base está dañado o no es una base de datos: es el caso
    /// en que tiene sentido ofrecer la restauración de un respaldo.
    pub fn es_base_danada(&self) -> bool {
        match self {
            ErrorSua::Integridad(_) => true,
            ErrorSua::Sqlite(rusqlite::Error::SqliteFailure(e, _)) => {
                matches!(e.code, rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase)
            }
            _ => false,
        }
    }
}
