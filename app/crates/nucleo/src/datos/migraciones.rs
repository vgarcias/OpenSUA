//! Migraciones versionadas hacia adelante (`ESPECIFICACION_SUA.md`, §5.5).
//!
//! Reglas: cada migración se aplica una sola vez, dentro de una transacción, y
//! queda anotada en `schema_version`. Una migración publicada no se edita:
//! los cambios van en una nueva. Ninguna borra datos del usuario.

use crate::error::{ErrorSua, Resultado};
use rusqlite::{Connection, Transaction};

pub enum Paso {
    Sql(&'static str),
    Codigo(fn(&Transaction) -> Resultado<()>),
}

pub struct Migracion {
    pub version: u32,
    pub nombre: &'static str,
    pub paso: Paso,
}

pub const MIGRACIONES: &[Migracion] = &[
    Migracion { version: 1, nombre: "catalogos de semilla", paso: Paso::Codigo(sembrar_catalogos) },
    Migracion { version: 2, nombre: "tablas de captura", paso: Paso::Sql(include_str!("../../migraciones/0002_captura.sql")) },
    Migracion { version: 3, nombre: "salario minimo y UMA 2019-2026", paso: Paso::Codigo(salarios_2019_2026) },
    Migracion { version: 4, nombre: "calculos guardados", paso: Paso::Sql(include_str!("../../migraciones/0004_calculos.sql")) },
    Migracion {
        version: 5,
        nombre: "ausentismos e incapacidades en calculos guardados",
        paso: Paso::Sql(include_str!("../../migraciones/0005_incidencias_en_calculo.sql")),
    },
    Migracion { version: 6, nombre: "UMI y creditos INFONAVIT", paso: Paso::Codigo(umi_y_creditos) },
    Migracion { version: 7, nombre: "pago extemporaneo en calculos guardados", paso: Paso::Sql(include_str!("../../migraciones/0007_extemporaneo.sql")) },
    Migracion { version: 8, nombre: "usuarios y bitacora de accesos", paso: Paso::Sql(include_str!("../../migraciones/0008_usuarios.sql")) },
];

/// Catálogo añadido por la migración 3; no viene del `SUA.MDB`.
pub const CATALOGO_ZLFN: &str = "SALARIO_ZLFN";

/// Salario mínimo general, salario mínimo de la Zona Libre de la Frontera
/// Norte y UMA de 2019 a 2026 (archivo aportado por el usuario el 12-nov-2026).
/// Columnas: año, mínimo general, mínimo ZLFN, UMA diaria, mensual, anual.
const SALARIOS_2019_2026: &str = include_str!("../../semilla/Salario_minimo_y_UMA_2019_2026.csv");

/// Catálogo añadido por la migración 6; no viene del `SUA.MDB`.
pub const CATALOGO_UMI: &str = "UMI";

/// UMI diaria (Unidad Mixta INFONAVIT) de 2016 a 2026 (archivo aportado por
/// el usuario el 12-nov-2026). Columnas: año, UMI diaria, incremento anual.
const UMI_2016_2026: &str = include_str!("../../semilla/umi_infonavit_2016_2026.csv");

/// Crea el catálogo de la UMI (un renglón por año) y añade a los cálculos
/// guardados la amortización, el seguro de daños y el número de crédito.
fn umi_y_creditos(tx: &Transaction) -> Resultado<()> {
    let semilla = |detalle: String| ErrorSua::Semilla { catalogo: "umi_infonavit_2016_2026".into(), detalle };
    let tabla = ident(&tabla_de_catalogo(CATALOGO_UMI));
    tx.execute(&format!("CREATE TABLE {tabla} (\"Anio\" TEXT NOT NULL, \"Valor\" TEXT NOT NULL) STRICT"), [])?;
    let texto = UMI_2016_2026.trim_start_matches('\u{feff}');
    let mut lector = csv::ReaderBuilder::new().has_headers(true).flexible(true).from_reader(texto.as_bytes());
    let mut filas = 0;
    for registro in lector.records() {
        let r = registro.map_err(|e| semilla(e.to_string()))?;
        let (anio, valor) = (r.get(0).unwrap_or("").trim(), r.get(1).unwrap_or("").trim());
        anio.parse::<u32>().map_err(|_| semilla(format!("año inválido: «{anio}»")))?;
        valor.parse::<f64>().map_err(|_| semilla(format!("valor no numérico en {anio}")))?;
        tx.execute(&format!("INSERT INTO {tabla} VALUES (?1, ?2)"), [anio, valor])?;
        filas += 1;
    }
    if filas == 0 {
        return Err(semilla("sin renglones".into()));
    }
    tx.execute_batch(
        "ALTER TABLE calculo_linea ADD COLUMN amortizacion    INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE calculo_linea ADD COLUMN seguro_vivienda INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE calculo_linea ADD COLUMN num_credito     TEXT NOT NULL DEFAULT '';",
    )?;
    Ok(())
}

/// Los catálogos del `SUA.MDB` terminaban en 2018 con un renglón «hasta 2999».
/// Esta migración cierra esos renglones y añade un renglón por año:
/// salario mínimo del 1 de enero al 31 de diciembre; UMA del 1 de febrero al
/// 31 de enero siguiente. El último año queda abierto.
fn salarios_2019_2026(tx: &Transaction) -> Resultado<()> {
    let semilla = |detalle: String| ErrorSua::Semilla { catalogo: "Salario_minimo_y_UMA_2019_2026".into(), detalle };
    let texto = SALARIOS_2019_2026.trim_start_matches('\u{feff}');
    let mut lector = csv::ReaderBuilder::new().has_headers(true).from_reader(texto.as_bytes());
    let mut filas: Vec<(u32, String, String, String)> = Vec::new();
    for registro in lector.records() {
        let r = registro.map_err(|e| semilla(e.to_string()))?;
        let campo = |i: usize| r.get(i).map(|s| s.trim().to_string()).ok_or_else(|| semilla(format!("falta la columna {}", i + 1)));
        let anio: u32 = campo(0)?.parse().map_err(|_| semilla("año inválido".into()))?;
        for i in 1..=3 {
            campo(i)?.parse::<f64>().map_err(|_| semilla(format!("valor no numérico en {anio}")))?;
        }
        filas.push((anio, campo(1)?, campo(2)?, campo(3)?));
    }
    filas.sort();
    let (Some(primero), Some(ultimo)) = (filas.first().map(|f| f.0), filas.last().map(|f| f.0)) else {
        return Err(semilla("sin renglones".into()));
    };
    const HORA: &str = "12:00:00 a. m.";

    tx.execute(&format!("CREATE TABLE {} (\"FEC_INI\" TEXT NOT NULL, \"FEC_TER\" TEXT NOT NULL, \"Valor\" TEXT NOT NULL) STRICT", ident(&tabla_de_catalogo(CATALOGO_ZLFN))), [])?;
    // Cierra los renglones abiertos de 2018.
    tx.execute(
        "UPDATE \"cat_SALARIO\" SET \"FEC_TER\" = ?1 WHERE \"FEC_TER\" LIKE '31/12/2999%'",
        [format!("31/12/{} {HORA}", primero - 1)],
    )?;
    tx.execute("UPDATE \"cat_UMA\" SET \"FEC_TER\" = ?1 WHERE \"FEC_TER\" LIKE '31/12/2999%'", [format!("31/01/{primero} {HORA}")])?;

    for (anio, general, zlfn, uma) in &filas {
        let abierto = *anio == ultimo;
        let ini_anio = format!("01/01/{anio} {HORA}");
        let fin_anio = if abierto { format!("31/12/2999 {HORA}") } else { format!("31/12/{anio} {HORA}") };
        tx.execute(
            "INSERT INTO \"cat_SALARIO\" (\"FEC_INI\", \"FEC_TER\", \"ZONAA\", \"AumentoA\", \"ZONAB\", \"AumentoB\", \"ZONAC\", \"AumentoC\")
             VALUES (?1, ?2, ?3, '0.0000', ?3, '0.0000', ?3, '0.0000')",
            rusqlite::params![ini_anio, fin_anio, general],
        )?;
        tx.execute(
            &format!("INSERT INTO {} VALUES (?1, ?2, ?3)", ident(&tabla_de_catalogo(CATALOGO_ZLFN))),
            rusqlite::params![ini_anio, fin_anio, zlfn],
        )?;
        let fin_uma = if abierto { format!("31/12/2999 {HORA}") } else { format!("31/01/{} {HORA}", anio + 1) };
        tx.execute(
            "INSERT INTO \"cat_UMA\" (\"Fec_Ini\", \"Fec_Ter\", \"Valor\") VALUES (?1, ?2, ?3)",
            rusqlite::params![format!("01/02/{anio} {HORA}"), fin_uma, uma],
        )?;
    }
    Ok(())
}

/// (nombre del catálogo, CSV exportado del `SUA.MDB` original o derivado del DOF).
/// Se guardan como texto, tal como vienen; la interpretación de fechas e
/// importes se hace al leerlos.
pub const CATALOGOS: &[(&str, &str)] = &[
    ("Ausentismos", include_str!("../../semilla/Ausentismos.csv")),
    ("CEAV_Patronal", include_str!("../../semilla/CEAV_Patronal.csv")),
    ("Cacofonias", include_str!("../../semilla/Cacofonias.csv")),
    ("Canamu", include_str!("../../semilla/Canamu.csv")),
    ("CatalogoRT", include_str!("../../semilla/CatalogoRT.csv")),
    ("Delegacion", include_str!("../../semilla/Delegacion.csv")),
    ("Dias_Inhabiles", include_str!("../../semilla/Dias_Inhabiles.csv")),
    ("Errores", include_str!("../../semilla/Errores.csv")),
    ("Estados", include_str!("../../semilla/Estados.csv")),
    ("Factor_Prima", include_str!("../../semilla/Factor_Prima.csv")),
    ("INPC", include_str!("../../semilla/INPC.csv")),
    ("MES_PRO", include_str!("../../semilla/MES_PRO.csv")),
    ("Municipios", include_str!("../../semilla/Municipios.csv")),
    ("Parametros", include_str!("../../semilla/Parametros.csv")),
    ("Prima_Minima", include_str!("../../semilla/Prima_Minima.csv")),
    ("Prima_RT", include_str!("../../semilla/Prima_RT.csv")),
    ("SALARIO", include_str!("../../semilla/SALARIO.csv")),
    ("Subdelega", include_str!("../../semilla/Subdelega.csv")),
    ("TablaFraccion", include_str!("../../semilla/TablaFraccion.csv")),
    ("UMA", include_str!("../../semilla/UMA.csv")),
    ("Versiones", include_str!("../../semilla/Versiones.csv")),
];

pub fn ident(nombre: &str) -> String {
    format!("\"{}\"", nombre.replace('"', "\"\""))
}

pub fn tabla_de_catalogo(catalogo: &str) -> String {
    format!("cat_{catalogo}")
}

pub fn version_conocida() -> u32 {
    MIGRACIONES.last().map(|m| m.version).unwrap_or(0)
}

pub fn version_actual(conn: &Connection) -> Resultado<u32> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
             version     INTEGER PRIMARY KEY,
             nombre      TEXT NOT NULL,
             aplicada_en TEXT NOT NULL
         );",
    )?;
    let v: Option<u32> = conn.query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))?;
    Ok(v.unwrap_or(0))
}

/// Aplica las migraciones pendientes. Devuelve las versiones aplicadas.
pub fn migrar(conn: &mut Connection) -> Resultado<Vec<u32>> {
    let actual = version_actual(conn)?;
    if actual > version_conocida() {
        return Err(ErrorSua::EsquemaFuturo { encontrada: actual, conocida: version_conocida() });
    }
    let mut aplicadas = Vec::new();
    for m in MIGRACIONES.iter().filter(|m| m.version > actual) {
        let tx = conn.transaction()?;
        match &m.paso {
            Paso::Sql(sql) => tx.execute_batch(sql)?,
            Paso::Codigo(f) => f(&tx)?,
        }
        tx.execute(
            "INSERT INTO schema_version (version, nombre, aplicada_en) VALUES (?1, ?2, ?3)",
            rusqlite::params![m.version, m.nombre, crate::bitacora::marca_utc(std::time::SystemTime::now())],
        )?;
        tx.commit()?;
        aplicadas.push(m.version);
    }
    Ok(aplicadas)
}

fn sembrar_catalogos(tx: &Transaction) -> Resultado<()> {
    for (nombre, csv_texto) in CATALOGOS {
        let semilla = |detalle: String| ErrorSua::Semilla { catalogo: nombre.to_string(), detalle };
        let texto = csv_texto.trim_start_matches('\u{feff}');
        let mut lector = csv::ReaderBuilder::new().has_headers(true).from_reader(texto.as_bytes());
        let columnas: Vec<String> = lector
            .headers()
            .map_err(|e| semilla(e.to_string()))?
            .iter()
            .map(str::to_string)
            .collect();
        if columnas.is_empty() {
            return Err(semilla("sin columnas".into()));
        }
        let tabla = ident(&tabla_de_catalogo(nombre));
        let definicion = columnas.iter().map(|c| format!("{} TEXT NOT NULL", ident(c))).collect::<Vec<_>>().join(", ");
        let marcadores = (1..=columnas.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(", ");
        tx.execute(&format!("CREATE TABLE {tabla} ({definicion}) STRICT"), [])?;
        let mut ins = tx.prepare(&format!("INSERT INTO {tabla} VALUES ({marcadores})"))?;
        for registro in lector.records() {
            let registro = registro.map_err(|e| semilla(e.to_string()))?;
            ins.execute(rusqlite::params_from_iter(registro.iter()))?;
        }
    }
    Ok(())
}
