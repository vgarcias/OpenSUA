//! Robustez de los lectores ante archivos dañados o malintencionados
//! (`ESPECIFICACION_SUA.md`, §11: validación de todas las entradas).
//!
//! El ejecutable se compila con `panic = "abort"`: un pánico al leer un
//! archivo cerraría la aplicación de golpe. Aquí se alteran archivos válidos
//! de muchas formas y se exige que cada lectura termine en un resultado o en
//! un error, nunca en pánico. Las alteraciones son deterministas (semilla
//! fija), así que un fallo se reproduce igual en cualquier equipo.

use sua_nucleo::config::Modo;
use sua_nucleo::datos::Base;
use sua_nucleo::dominio::comprobar::comprobar_pago;
use sua_nucleo::io::anonimo::anonimizar;
use sua_nucleo::io::pago;
use sua_nucleo::io::txt::Clase;
use std::path::PathBuf;

const ORO: &[u8] = include_bytes!("../oro/pago_200802.SUA");

/// Generador congruencial: suficiente para repartir alteraciones, sin dependencias.
struct Azar(u64);

impl Azar {
    fn siguiente(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn hasta(&mut self, n: usize) -> usize {
        (self.siguiente() % n as u64) as usize
    }
}

/// Bytes que más problemas dan: letras acentuadas y Ñ de Windows‑1252 (que al
/// decodificar ocupan dos bytes), nulos, saltos de línea, signos y extremos.
const DIFICILES: &[u8] = &[0xF1, 0xD1, 0xE9, 0xC1, 0x80, 0xFF, 0x00, b'\n', b'\r', b' ', b'0', b'9', b'A', b'-', b'/', b'$', b'\'', b'"', b'%'];

fn alterar(original: &[u8], azar: &mut Azar) -> Vec<u8> {
    let mut d = original.to_vec();
    match azar.hasta(10) {
        // Recortar o alargar.
        0 => d.truncate(azar.hasta(d.len() + 1)),
        1 => {
            let extra = azar.hasta(600);
            d.extend((0..extra).map(|_| DIFICILES[azar.hasta(DIFICILES.len())]));
        }
        // Quitar o duplicar un tramo.
        2 if d.len() > 2 => {
            let desde = azar.hasta(d.len() - 1);
            let largo = 1 + azar.hasta((d.len() - desde).min(400));
            d.drain(desde..desde + largo);
        }
        3 if d.len() > 2 => {
            let desde = azar.hasta(d.len() - 1);
            let largo = 1 + azar.hasta((d.len() - desde).min(400));
            let tramo = d[desde..desde + largo].to_vec();
            let destino = azar.hasta(d.len());
            d.splice(destino..destino, tramo);
        }
        // Lo más frecuente: cambiar unos cuantos bytes sin mover el resto.
        _ => {
            for _ in 0..1 + azar.hasta(8) {
                if d.is_empty() {
                    break;
                }
                let pos = azar.hasta(d.len());
                d[pos] = if azar.hasta(2) == 0 { DIFICILES[azar.hasta(DIFICILES.len())] } else { azar.hasta(256) as u8 };
            }
        }
    }
    d
}

struct Carpeta(PathBuf);

impl Carpeta {
    fn nueva(etiqueta: &str) -> Carpeta {
        let dir = std::env::temp_dir().join(format!("sua-robustez-{etiqueta}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Carpeta(dir)
    }

    fn base(&self, nombre: &str) -> Base {
        Base::abrir_en(&self.0.join(nombre), &self.0.join("respaldos"), Modo::Portable, 3).unwrap()
    }
}

impl Drop for Carpeta {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn un_archivo_de_pago_alterado_nunca_provoca_panico() {
    let carpeta = Carpeta::nueva("pago");
    let base = carpeta.base("sua.db");
    let tasas = base.tasas("200802", 758_875, false).unwrap().tasas;
    let mut azar = Azar(20261113);
    let (mut aceptados, mut rechazados) = (0, 0);
    for _ in 0..20_000 {
        let datos = alterar(ORO, &mut azar);
        match pago::leer(&datos) {
            Ok(a) => {
                aceptados += 1;
                let _ = a.verificar();
                let _ = comprobar_pago(&a, &tasas);
                let _ = anonimizar(&datos);
            }
            Err(e) => {
                rechazados += 1;
                // El mensaje también debe poder formarse.
                let _ = e.to_string();
            }
        }
    }
    // La prueba ejercita las dos ramas: archivos que se leen y archivos que se rechazan.
    assert!(aceptados > 1_000 && rechazados > 1_000, "{aceptados} aceptados, {rechazados} rechazados");
}

#[test]
fn importar_un_archivo_de_pago_alterado_nunca_provoca_panico() {
    let carpeta = Carpeta::nueva("importar");
    let base = carpeta.base("sua.db");
    let mut azar = Azar(7);
    let mut importados = 0;
    for _ in 0..600 {
        let datos = alterar(ORO, &mut azar);
        if let Ok(a) = pago::leer(&datos) {
            if base.importar_pago(&a).is_ok() {
                importados += 1;
            }
        }
    }
    assert!(importados > 0);
    // Con lo que haya quedado en la base, calcular tampoco debe fallar de golpe.
    for patron in base.patrones().unwrap() {
        for periodo in ["200801", "200802", "202602", "199912"] {
            let _ = base.calcular_periodo(&patron.reg_pat, periodo, false);
            let _ = base.calcular_periodo_con_pago(&patron.reg_pat, periodo, true, Some("2026-11-13"));
        }
    }
}

#[test]
fn un_txt_alterado_nunca_provoca_panico() {
    let carpeta = Carpeta::nueva("txt");
    // Una base con datos, de la que salen TXT válidos de cada clase.
    let origen = carpeta.base("origen.db");
    let a = pago::leer(ORO).unwrap();
    origen.importar_pago(&a).unwrap();
    let reg = &a.patron.registro_patronal;

    let destino = carpeta.base("destino.db");
    let mut azar = Azar(99);
    for clase in Clase::TODAS {
        let (valido, _) = origen.exportar_txt(clase, reg).unwrap();
        // El archivo sin alterar se importa; después, 400 versiones dañadas.
        destino.importar_txt(clase, &valido).unwrap();
        let semilla: Vec<u8> = if valido.is_empty() { vec![b'0'; clase.largo() + 2] } else { valido };
        for _ in 0..400 {
            let datos = alterar(&semilla, &mut azar);
            if let Ok(r) = destino.importar_txt(clase, &datos) {
                let _ = format!("{r:?}");
            }
        }
        // Y basura pura del largo de un renglón.
        for _ in 0..50 {
            let basura: Vec<u8> = (0..clase.largo() + azar.hasta(5)).map(|_| DIFICILES[azar.hasta(DIFICILES.len())]).collect();
            let _ = destino.importar_txt(clase, &basura);
        }
    }
    // Exportar y calcular con lo que quedó tampoco debe fallar de golpe.
    for patron in destino.patrones().unwrap() {
        for clase in Clase::TODAS {
            let _ = destino.exportar_txt(clase, &patron.reg_pat);
        }
        let _ = destino.calcular_periodo(&patron.reg_pat, "200802", false);
        let _ = destino.trabajadores(&patron.reg_pat, "");
    }
}

/// Un campo de fecha con una Ñ (un byte en el archivo, dos al decodificar) no
/// debe partir el texto a media letra. Caso encontrado al revisar el código:
/// la fecha de la prima de riesgos de trabajo del registro 02.
#[test]
fn una_letra_acentuada_en_un_campo_de_fecha_no_provoca_panico() {
    let carpeta = Carpeta::nueva("fecha");
    let base = carpeta.base("sua.db");
    for campo in [&b"200\xF13 "[..], b"\xF1\xF1\xF1   ", b"20\xF1802", b"2008\xF1 ", b"\xD1     "] {
        let mut datos = ORO.to_vec();
        datos[197..203].copy_from_slice(campo);
        let a = pago::leer(&datos).unwrap();
        // Se importa; lo único que se pierde es la prima, cuya fecha no se entiende.
        base.importar_pago(&a).unwrap();
        let _ = base.calcular_periodo(&a.patron.registro_patronal, "200802", false);
    }
}

/// El registro patronal da nombre a carpetas y archivos (`archivos\<registro>\`,
/// `trabajadores_<registro>.pdf`). Un archivo manipulado no puede meter ahí una ruta.
#[test]
fn un_registro_patronal_con_forma_de_ruta_se_rechaza_al_importar() {
    let carpeta = Carpeta::nueva("ruta");
    let base = carpeta.base("sua.db");
    for malo in [&br"..\..\..\ab"[..], b"../../../ab", br"C:\Windows\", b"Z99 0000109", b"Z99000001\xF1\xF1"] {
        assert_eq!(malo.len(), 11);
        let mut datos = ORO.to_vec();
        for r in datos.chunks_mut(pago::LARGO_REGISTRO) {
            r[2..13].copy_from_slice(malo);
        }
        let a = pago::leer(&datos).unwrap();
        assert!(base.importar_pago(&a).is_err(), "se aceptó {:?}", String::from_utf8_lossy(malo));
    }
    assert!(base.patrones().unwrap().is_empty());
}
