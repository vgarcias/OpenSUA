//! Medición de rendimiento del núcleo (`ESPECIFICACION_SUA.md`, §13.1).
//! No corre con `cargo test` normal: tarda y sus tiempos solo valen en modo
//! release.
//!
//!     cargo test --release --test rendimiento -- --ignored --nocapture
//!
//! Los límites del final son holgados a propósito: avisan de un deterioro
//! grande (un cálculo que pase de segundos a minutos), no de variaciones
//! entre equipos.

use sua_nucleo::config::Modo;
use sua_nucleo::datos::Base;
use sua_nucleo::dominio::validar::luhn;
use sua_nucleo::io::pago::{ArchivoPago, Control06, Movimiento04, Patron02, Totales05, Trabajador03};
use sua_nucleo::io::txt::Clase;
use std::time::{Duration, Instant};

const REG: &str = "Z9900000109";

/// Un archivo de pago en memoria con `n` trabajadores ficticios, cada uno con
/// su alta. Uno de cada diez tiene además una modificación de salario.
fn archivo(n: usize) -> ArchivoPago {
    let mut trabajadores = Vec::with_capacity(n);
    let mut movimientos = Vec::with_capacity(n + n / 10);
    for i in 0..n {
        let diez = format!("{:010}", 1_000_000 + i);
        let nss = format!("{diez}{}", luhn(&diez).unwrap());
        let sdi = 30_000 + (i as i64 % 400) * 250;
        trabajadores.push(Trabajador03 {
            nss: nss.clone(),
            rfc: "XEXX010101000".into(),
            curp: "XEXX010101HNEXXXA4".into(),
            nombre: format!("FICTICIO$DE PRUEBA$TRABAJADOR {i}"),
            sdi,
            tipo_trabajador: "1".into(),
            jornada_reducida: "0".into(),
            movimientos: 1,
            ..Trabajador03::default()
        });
        movimientos.push(Movimiento04 { nss: nss.clone(), tipo: "01".into(), fecha: "20250102".into(), sdi, ..Movimiento04::default() });
        if i % 10 == 0 {
            movimientos.push(Movimiento04 { nss, tipo: "07".into(), fecha: "20260116".into(), sdi: sdi + 1_500, ..Movimiento04::default() });
        }
    }
    ArchivoPago {
        bytes: 0,
        patron: Patron02 {
            registro_patronal: REG.into(),
            rfc: "XAXX010101000".into(),
            periodo: "202602".into(),
            razon_social: "PATRON FICTICIO DE RENDIMIENTO SA DE CV".into(),
            entidad: "09".into(),
            codigo_postal: "06000".into(),
            prima_rt: 758_875,
            fecha_prima_rt: "202503".into(),
            delegacion: "31".into(),
            subdelegacion: "12".into(),
            area_geografica: "A".into(),
            ..Patron02::default()
        },
        trabajadores,
        movimientos,
        totales: Totales05::default(),
        control: Control06::default(),
    }
}

fn medir<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let inicio = Instant::now();
    let r = f();
    (r, inicio.elapsed())
}

fn ms(d: Duration) -> String {
    format!("{:>9.1}", d.as_secs_f64() * 1000.0)
}

#[test]
#[ignore = "medición: correr en release con --ignored --nocapture"]
fn tiempos_con_100_1000_y_5000_trabajadores() {
    let raiz = std::env::temp_dir().join(format!("sua-rendimiento-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&raiz);
    println!("\n trabajadores | importar .SUA | cálculo mes | cálculo bimestre | listar | exportar TXT | importar TXT (trab.) | base (MB)");
    let mut peor_calculo = Duration::ZERO;
    for n in [100usize, 1_000, 5_000] {
        let dir = raiz.join(n.to_string());
        std::fs::create_dir_all(&dir).unwrap();
        let base = Base::abrir_en(&dir.join("sua.db"), &dir.join("respaldos"), Modo::Portable, 3).unwrap();
        let a = archivo(n);

        let (r, t_importar) = medir(|| base.importar_pago(&a).unwrap());
        assert_eq!(r.trabajadores_nuevos, n);
        let (mes, t_mes) = medir(|| base.calcular_periodo(REG, "202601", false).unwrap());
        let (bim, t_bim) = medir(|| base.calcular_periodo(REG, "202602", false).unwrap());
        assert_eq!((mes.lineas.len(), bim.lineas.len()), (n, n), "avisos: {:?}", bim.avisos);
        assert!(mes.mes.total() > 0 && bim.bimestre.total() > 0);
        let (lista, t_listar) = medir(|| base.trabajadores(REG, "").unwrap());
        assert_eq!(lista.len(), n);
        let ((txt, renglones), t_exportar) = medir(|| base.exportar_txt(Clase::Trabajadores, REG).unwrap());
        assert_eq!(renglones, n);

        // Importación por TXT: el camino por renglones, sobre una base que solo tiene al patrón.
        let otra = Base::abrir_en(&dir.join("otra.db"), &dir.join("respaldos"), Modo::Portable, 3).unwrap();
        let (patrones, _) = base.exportar_txt(Clase::Patrones, REG).unwrap();
        otra.importar_txt(Clase::Patrones, &patrones).unwrap();
        let (resumen, t_txt) = medir(|| otra.importar_txt(Clase::Trabajadores, &txt).unwrap());
        println!("   resumen de la importación TXT: {resumen:?}");

        // Para medir la interfaz con muchos datos: deja una copia de la base más grande.
        if let (5_000, Some(destino)) = (n, std::env::var_os("SUA_RENDIMIENTO_CONSERVAR")) {
            drop(otra);
            std::fs::copy(dir.join("sua.db"), destino).unwrap();
        }
        let tamano = std::fs::metadata(dir.join("sua.db")).unwrap().len() as f64 / 1_048_576.0;
        println!(" {n:>12} | {} ms | {} ms | {} ms | {} ms | {} ms | {} ms | {tamano:.1}", ms(t_importar), ms(t_mes), ms(t_bim), ms(t_listar), ms(t_exportar), ms(t_txt));
        peor_calculo = peor_calculo.max(t_mes).max(t_bim);
    }
    let _ = std::fs::remove_dir_all(&raiz);
    // Con 5 000 trabajadores el cálculo debe seguir siendo cosa de segundos.
    assert!(peor_calculo < Duration::from_secs(20), "el cálculo tardó {peor_calculo:?}");
}
