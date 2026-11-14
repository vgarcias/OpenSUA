//! El cifrado no debe depender de la configuración de OpenSSL del equipo.
//!
//! OpenSSL lee la variable `OPENSSL_CONF` del entorno con que arranca el
//! proceso, así que la prueba se lanza a sí misma como proceso hijo con la
//! variable ya puesta; cambiarla dentro del mismo proceso no tendría efecto.

use sua_nucleo::config::Modo;
use sua_nucleo::datos::Base;
use std::path::Path;

const NOMBRE: &str = "una_configuracion_ajena_de_openssl_no_impide_abrir_una_base_cifrada";
const CARPETA_DEL_HIJO: &str = "SUA_PRUEBA_AISLAMIENTO";

fn cuerpo(carpeta: &Path) {
    let abrir = || Base::abrir_en_con_clave(&carpeta.join("sua.db"), &carpeta.join("respaldos"), Modo::Portable, 3, Some("clave de prueba"));
    {
        let base = abrir().expect("la base cifrada debe crearse aunque haya una configuración ajena");
        assert!(base.cifrada());
        base.respaldar("manual").unwrap();
    }
    let base = abrir().expect("y debe volver a abrir");
    assert_eq!(base.integridad().unwrap(), "ok");
}

#[test]
fn una_configuracion_ajena_de_openssl_no_impide_abrir_una_base_cifrada() {
    if let Some(carpeta) = std::env::var_os(CARPETA_DEL_HIJO) {
        return cuerpo(Path::new(&carpeta));
    }
    let carpeta = std::env::temp_dir().join(format!("sua-prueba-aislamiento-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&carpeta);
    std::fs::create_dir_all(&carpeta).unwrap();
    // Una configuración que pide un proveedor que no existe: si OpenSSL la
    // leyera, se quedaría sin algoritmos y la contraseña no se podría aplicar.
    let ajena = carpeta.join("ajena.cnf");
    std::fs::write(&ajena, "openssl_conf = inicio\n[inicio]\nproviders = prov\n[prov]\nnoexiste = sec\n[sec]\nactivate = 1\n").unwrap();
    let hijo = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", NOMBRE, "--nocapture"])
        .env("OPENSSL_CONF", &ajena)
        .env(CARPETA_DEL_HIJO, &carpeta)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&carpeta);
    assert!(hijo.status.success(), "con OPENSSL_CONF ajeno la base cifrada no abrió:\n{}", String::from_utf8_lossy(&hijo.stderr));
}
