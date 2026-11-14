//! Pruebas de oro (`ESPECIFICACION_SUA.md`, §14): archivos de pago reales de
//! 2006 a 2008, anonimizados con `sua-cli anonimizar-sua` (ver `oro/LEEME.md`).
//! El motor debe reproducir los importes que calculó el SUA oficial.
//!
//! Los números esperados son los que hoy se obtienen. Si una prueba falla
//! después de tocar el motor, las tasas o los tramos, es que el cálculo cambió:
//! solo se actualiza la tabla si el cambio acerca el resultado al del archivo.

use sua_nucleo::config::Modo;
use sua_nucleo::datos::Base;
use sua_nucleo::dominio::comprobar::comprobar_pago;
use sua_nucleo::io::pago;
use std::path::PathBuf;

struct Caso {
    nombre: &'static str,
    datos: &'static [u8],
    trabajadores: usize,
    /// Importes por trabajador comparados con el motor, y cuántos coinciden.
    comparados: usize,
    iguales: usize,
    /// El archivo aplica el decreto de exención de 2008.
    decreto: bool,
    /// Diferencia, en centavos, entre los totales del archivo y el cálculo de
    /// todo el patrón a partir de los movimientos importados.
    diferencia_de_totales: i64,
}

macro_rules! caso {
    ($archivo:literal, $trabajadores:expr, $comparados:expr, $iguales:expr, $decreto:expr, $diferencia:expr) => {
        Caso {
            nombre: $archivo,
            datos: include_bytes!(concat!("../oro/", $archivo)),
            trabajadores: $trabajadores,
            comparados: $comparados,
            iguales: $iguales,
            decreto: $decreto,
            diferencia_de_totales: $diferencia,
        }
    };
}

fn casos() -> Vec<Caso> {
    vec![
        caso!("pago_200605.SUA", 9, 99, 99, false, 0),
        caso!("pago_200606.SUA", 9, 102, 102, false, 0),
        caso!("pago_200706.SUA", 8, 24, 24, false, 0),
        // Un centavo sin explicar en cesantía y vejez patronal (Decompilacion_SUA.md, §15).
        caso!("pago_200802.SUA", 12, 180, 179, false, 1),
        caso!("pago_200806.SUA", 10, 51, 51, true, 0),
    ]
}

struct Carpeta(PathBuf);

impl Carpeta {
    fn nueva(etiqueta: &str) -> Carpeta {
        let dir = std::env::temp_dir().join(format!("sua-oro-{etiqueta}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Carpeta(dir)
    }

    fn base(&self) -> Base {
        Base::abrir_en(&self.0.join("sua.db"), &self.0.join("respaldos"), Modo::Portable, 3).unwrap()
    }
}

impl Drop for Carpeta {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn los_archivos_se_leen_y_cuadran_por_dentro() {
    for c in casos() {
        let a = pago::leer(c.datos).unwrap_or_else(|e| panic!("{}: {e}", c.nombre));
        assert_eq!(a.verificar(), Vec::<String>::new(), "{}", c.nombre);
        assert_eq!(a.trabajadores.len(), c.trabajadores, "{}", c.nombre);
    }
}

#[test]
fn el_motor_reproduce_los_importes_de_cada_trabajador() {
    let carpeta = Carpeta::nueva("motor");
    let base = carpeta.base();
    let (mut comparados, mut iguales) = (0, 0);
    for c in casos() {
        let a = pago::leer(c.datos).unwrap();
        let tasas = base.tasas(&a.patron.periodo, a.patron.prima_rt, c.decreto).unwrap().tasas;
        let r = comprobar_pago(&a, &tasas);
        assert_eq!((r.conceptos_comparados, r.conceptos_iguales), (c.comparados, c.iguales), "{}: {:?}", c.nombre, r.diferencias);
        // Lo que no coincide es, como mucho, un centavo.
        assert!(r.diferencia_total <= (c.comparados - c.iguales) as i64, "{}: {} centavos", c.nombre, r.diferencia_total);
        comparados += r.conceptos_comparados;
        iguales += r.conceptos_iguales;
    }
    assert_eq!((comparados, iguales), (456, 455));
}

#[test]
fn importar_y_calcular_todo_el_patron_da_los_totales_del_archivo() {
    for c in casos() {
        let a = pago::leer(c.datos).unwrap();
        let carpeta = Carpeta::nueva(&c.nombre[5..11]);
        let base = carpeta.base();
        base.importar_pago(&a).unwrap();
        let r = base.calcular_periodo(&a.patron.registro_patronal, &a.patron.periodo, c.decreto).unwrap();

        // Los días salen de los movimientos y deben coincidir trabajador por trabajador.
        for x in &a.trabajadores {
            let linea = r.lineas.iter().find(|l| l.nss == x.nss).unwrap_or_else(|| panic!("{}: falta un trabajador en el cálculo", c.nombre));
            if x.dias_mes > 0 {
                assert_eq!(linea.dias_mes, x.dias_mes as u32, "{}: días del mes", c.nombre);
            }
            if x.dias_bimestre > 0 {
                assert_eq!(linea.dias_bimestre, x.dias_bimestre as u32, "{}: días del bimestre", c.nombre);
            }
        }

        let (s, mut diferencia) = (&a.totales, 0i64);
        if s.suma_imss > 0 {
            diferencia += (r.mes.total() - s.suma_imss).abs();
        }
        if s.suma_rcv > 0 {
            diferencia += (r.bimestre.retiro + r.bimestre.cyv_patronal + r.bimestre.cyv_obrero - s.suma_rcv).abs();
        }
        if a.control.total_infonavit > 0 {
            diferencia += (r.bimestre.infonavit - a.control.total_infonavit).abs();
        }
        assert_eq!(diferencia, c.diferencia_de_totales, "{}: diferencia de totales en centavos", c.nombre);
    }
}

/// Ningún archivo de oro debe traer datos de personas: solo los sustitutos
/// que pone el anonimizador.
#[test]
fn los_archivos_de_oro_estan_anonimizados() {
    for c in casos() {
        let a = pago::leer(c.datos).unwrap();
        assert_eq!(a.patron.registro_patronal, sua_nucleo::io::anonimo::REGISTRO_PATRONAL_FICTICIO, "{}", c.nombre);
        assert_eq!(a.patron.razon_social, "PATRON ANONIMO SA DE CV", "{}", c.nombre);
        assert_eq!(a.control.numero_control, "0".repeat(19), "{}", c.nombre);
        for t in &a.trabajadores {
            assert!(t.nss.starts_with("000000"), "{}", c.nombre);
            assert!(t.nombre.starts_with("ANONIMO$ANONIMO$TRABAJADOR "), "{}", c.nombre);
            assert_eq!((t.rfc.as_str(), t.curp.as_str()), ("XEXX010101000", "XEXX010101HNEXXXA4"), "{}", c.nombre);
        }
        // Anonimizar de nuevo no cambia nada: no quedó nada por sustituir.
        assert_eq!(sua_nucleo::io::anonimo::anonimizar(c.datos).unwrap(), c.datos, "{}", c.nombre);
    }
}
