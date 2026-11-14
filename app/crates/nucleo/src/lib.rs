//! Núcleo de OpenSUA. No depende de ninguna interfaz gráfica: lo usan por
//! igual la línea de comandos y la futura interfaz (`ESPECIFICACION_SUA.md`, §4).

pub mod bitacora;
pub mod config;
pub mod datos;
pub mod dominio;
pub mod error;
pub mod io;
pub mod reportes;

pub use error::{ErrorSua, Resultado};
