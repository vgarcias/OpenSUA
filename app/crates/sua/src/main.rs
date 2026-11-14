//! Línea de comandos. La interfaz gráfica es el ejecutable `sua` (`crates/ui`),
//! sobre el mismo núcleo (`sua-nucleo`).

use std::path::Path;
use std::process::ExitCode;
use sua_nucleo::bitacora::Bitacora;
use sua_nucleo::config::{self, Entorno, Modo};
use sua_nucleo::datos::Base;
use sua_nucleo::dominio::comprobar::comprobar_pago;
use sua_nucleo::io::pago;
use sua_nucleo::{ErrorSua, Resultado};

const AYUDA: &str = "\
OpenSUA — línea de comandos

Uso: sua-cli <orden>

  diagnostico            Prepara carpetas, abre la base, aplica migraciones y muestra el estado.
  respaldar              Copia la base a data\\respaldos.
  compactar              Compacta la base de datos.
  recuperar              Si la base está dañada, la sustituye por el respaldo más reciente
                         que sirva. El archivo dañado se guarda aparte, no se borra.
  clave                  Pone, cambia o quita la contraseña que cifra la base.
                         Si se olvida, los datos no se pueden recuperar.
  restablecer-usuario <nombre>
                         Pone una contraseña nueva a un administrador que olvidó la suya.
                         No pide sesión; queda anotado en la bitácora de accesos.
  activar-portable       Crea config\\settings.toml con portable = true junto al ejecutable.
  leer-sua <archivo>     Lee un archivo de pago .SUA y comprueba su consistencia.
                         No muestra nombres, NSS, RFC ni CURP.
  importar-sua <archivo> Carga en la base el patrón, los trabajadores y los movimientos
                         de un archivo de pago (hace un respaldo antes).
  anonimizar-sua <archivo> <salida>
                         Escribe una copia del archivo de pago sin datos personales:
                         cambia patrón, nombres, NSS, RFC, CURP y números de control,
                         y conserva periodo, salarios, días e importes.
  ejemplo                Carga un patrón y cinco trabajadores ficticios, para demostraciones.

Si la base tiene contraseña, cada orden la pregunta. Para no escribirla cada vez
se puede dejar en la variable de entorno SUA_CLAVE mientras dure la sesión.
Si hay usuarios registrados, cada orden pide usuario y contraseña (o los toma de
SUA_USUARIO y SUA_CLAVE_USUARIO) y respeta su rol.
";

fn pesos(centavos: i64) -> String {
    let entero = (centavos / 100).to_string();
    let mut con_comas = String::new();
    for (i, c) in entero.chars().enumerate() {
        if i > 0 && (entero.len() - i) % 3 == 0 {
            con_comas.push(',');
        }
        con_comas.push(c);
    }
    format!("{con_comas}.{:02}", centavos % 100)
}

/// Variable de entorno con la contraseña de la base, para no escribirla en cada orden.
const VARIABLE_CLAVE: &str = "SUA_CLAVE";

/// Lee un renglón del teclado sin mostrarlo en pantalla.
fn leer_sin_eco(pregunta: &str) -> Resultado<String> {
    use std::io::Write;
    #[cfg(windows)]
    mod consola {
        #[link(name = "kernel32")]
        extern "system" {
            pub fn GetStdHandle(cual: u32) -> *mut core::ffi::c_void;
            pub fn GetConsoleMode(consola: *mut core::ffi::c_void, modo: *mut u32) -> i32;
            pub fn SetConsoleMode(consola: *mut core::ffi::c_void, modo: u32) -> i32;
        }
        pub const ENTRADA: u32 = -10i32 as u32;
        pub const ECO: u32 = 0x0004;
    }
    eprint!("{pregunta}");
    let _ = std::io::stderr().flush();
    let mut renglon = String::new();
    #[cfg(windows)]
    {
        let mut modo = 0u32;
        // SAFETY: llamadas simples a la consola; si la entrada no es una consola, GetConsoleMode falla y no se toca nada.
        let (entrada, es_consola) = unsafe {
            let entrada = consola::GetStdHandle(consola::ENTRADA);
            (entrada, consola::GetConsoleMode(entrada, &mut modo) != 0)
        };
        if es_consola {
            // SAFETY: se quita el eco y se repone el modo anterior al terminar.
            unsafe { consola::SetConsoleMode(entrada, modo & !consola::ECO) };
        }
        let leido = std::io::stdin().read_line(&mut renglon);
        if es_consola {
            // SAFETY: repone el modo leído arriba.
            unsafe { consola::SetConsoleMode(entrada, modo) };
            eprintln!();
        }
        leido.map_err(|e| ErrorSua::Ejecutable(format!("no se pudo leer la contraseña: {e}")))?;
    }
    #[cfg(unix)]
    {
        // SAFETY: tcgetattr/tcsetattr sobre la entrada estándar; si no es una terminal, tcgetattr falla y no se toca nada.
        let (anterior, es_terminal) = unsafe {
            let mut t: libc::termios = std::mem::zeroed();
            let ok = libc::tcgetattr(libc::STDIN_FILENO, &mut t) == 0;
            (t, ok)
        };
        if es_terminal {
            let mut sin_eco = anterior;
            sin_eco.c_lflag &= !libc::ECHO;
            // SAFETY: se quita el eco y se repone el modo anterior al terminar.
            unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &sin_eco) };
        }
        let leido = std::io::stdin().read_line(&mut renglon);
        if es_terminal {
            // SAFETY: repone el modo leído arriba.
            unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &anterior) };
            eprintln!();
        }
        leido.map_err(|e| ErrorSua::Ejecutable(format!("no se pudo leer la contraseña: {e}")))?;
    }
    #[cfg(not(any(windows, unix)))]
    std::io::stdin().read_line(&mut renglon).map_err(|e| ErrorSua::Ejecutable(format!("no se pudo leer la contraseña: {e}")))?;
    Ok(renglon.trim_end_matches(['\r', '\n']).to_string())
}

/// Contraseña de la base: de la variable `SUA_CLAVE` o preguntada sin eco.
fn pedir_clave() -> Resultado<String> {
    match std::env::var(VARIABLE_CLAVE) {
        Ok(clave) if !clave.is_empty() => Ok(clave),
        _ => leer_sin_eco("Contraseña de la base: "),
    }
}

/// Abre la base; si tiene contraseña, la toma de `SUA_CLAVE` o la pregunta.
/// Devuelve también la contraseña usada.
fn abrir_base(entorno: &Entorno) -> Resultado<(Base, Option<String>)> {
    match Base::abrir(entorno) {
        Err(ErrorSua::FaltaClave) => {}
        // Sin la marca de base cifrada, una base con contraseña parece dañada: si hay contraseña a la mano, se prueba.
        Err(e) if e.es_base_danada() && std::env::var_os(VARIABLE_CLAVE).is_some_and(|v| !v.is_empty()) => {}
        otro => return otro.map(|b| (b, None)),
    }
    let clave = pedir_clave()?;
    Base::abrir_con_clave(entorno, Some(&clave)).map(|b| (b, Some(clave)))
}

/// Si la base tiene usuarios, inicia sesión: usuario y contraseña de las
/// variables `SUA_USUARIO` y `SUA_CLAVE_USUARIO`, o preguntados.
fn iniciar_sesion(base: &Base) -> Resultado<()> {
    if !base.hay_usuarios() {
        return Ok(());
    }
    let de_variable = |nombre: &str| std::env::var(nombre).ok().filter(|v| !v.is_empty());
    let usuario = match de_variable("SUA_USUARIO") {
        Some(u) => u,
        None => {
            use std::io::Write;
            eprint!("Usuario: ");
            let _ = std::io::stderr().flush();
            let mut renglon = String::new();
            std::io::stdin().read_line(&mut renglon).map_err(|e| ErrorSua::Ejecutable(format!("no se pudo leer el usuario: {e}")))?;
            renglon.trim().to_string()
        }
    };
    let clave = match de_variable("SUA_CLAVE_USUARIO") {
        Some(c) => c,
        None => leer_sin_eco("Contraseña del usuario: ")?,
    };
    base.iniciar_sesion(&usuario, &clave).map(|_| ()).inspect_err(|e| {
        if matches!(e, ErrorSua::AccesoDenegado) {
            eprintln!("Si un administrador olvidó su contraseña: «sua-cli restablecer-usuario <nombre>».");
        }
    })
}

fn abrir() -> Resultado<(Entorno, Bitacora, Base)> {
    let entorno = Entorno::preparar()?;
    let bitacora = Bitacora::en(&entorno.logs);
    let (base, _) = abrir_base(&entorno).inspect_err(|e| {
        bitacora.error(&format!("No se pudo abrir la base: {e}"));
        if e.es_base_danada() || matches!(e, ErrorSua::ClaveIncorrecta) {
            eprintln!("Si la base está dañada, con «sua-cli recuperar» se sustituye por el respaldo más reciente que sirva.");
        }
    })?;
    iniciar_sesion(&base)?;
    if let Some(r) = &base.respaldo_previo {
        bitacora.info(&format!("Respaldo previo a la migración: {}", r.display()));
    }
    if !base.migraciones_aplicadas.is_empty() {
        bitacora.info(&format!("Migraciones aplicadas: {:?}", base.migraciones_aplicadas));
    }
    Ok((entorno, bitacora, base))
}

/// Sustituye una base dañada por el último respaldo que sirva. Con la base sana no hace nada.
fn recuperar() -> Resultado<bool> {
    use sua_nucleo::datos::restaurar;
    let entorno = Entorno::preparar()?;
    let bitacora = Bitacora::en(&entorno.logs);
    // Con contraseña, una base dañada y una contraseña equivocada dan el mismo error:
    // la contraseña se da por buena si abre algún respaldo.
    let clave = match Base::abrir(&entorno) {
        Ok(_) => {
            println!("La base abre bien: no hace falta recuperarla. Para volver a un respaldo use la pantalla Sistema.");
            return Ok(true);
        }
        Err(ErrorSua::FaltaClave) => {
            let clave = pedir_clave()?;
            match Base::abrir_con_clave(&entorno, Some(&clave)) {
                Ok(_) => {
                    println!("La base abre bien con esa contraseña: no hace falta recuperarla.");
                    return Ok(true);
                }
                Err(ErrorSua::ClaveIncorrecta) => println!("La base no abre con esa contraseña: o no es la correcta o el archivo está dañado."),
                Err(e) if e.es_base_danada() => println!("La base está dañada: {e}"),
                Err(e) => return Err(e),
            }
            Some(clave)
        }
        Err(e) if e.es_base_danada() => {
            println!("La base está dañada: {e}");
            None
        }
        Err(e) => return Err(e),
    };
    let r = restaurar::recuperar(&entorno.ruta_base(), &entorno.respaldos, clave.as_deref()).map_err(|e| match (&e, &clave) {
        (ErrorSua::Validacion(_), Some(_)) => ErrorSua::Validacion(format!("{e} Si la contraseña que escribió no es la correcta, vuelva a intentarlo con la correcta.")),
        _ => e,
    })?;
    let base = Base::abrir_con_clave(&entorno, clave.as_deref().filter(|_| r.respaldo.cifrado))?;
    bitacora.info(&format!("Base recuperada del respaldo {}; el archivo dañado quedó en {}.", r.respaldo.ruta.display(), r.danada.display()));
    println!("Respaldo restaurado: {}", restaurar::etiqueta_de_respaldo(&r.respaldo.ruta));
    println!("Contenía:            {} patrones y {} trabajadores", r.respaldo.patrones, r.respaldo.trabajadores);
    println!("Archivo dañado:      {}", r.danada.display());
    println!("Integridad:          {}", base.integridad()?);
    Ok(true)
}

/// Pone, cambia o quita la contraseña de la base. La nueva se toma de
/// `SUA_CLAVE_NUEVA` (vacía para quitarla) o se pregunta dos veces.
fn clave() -> Resultado<bool> {
    use sua_nucleo::datos::cifrado;
    let entorno = Entorno::preparar()?;
    let bitacora = Bitacora::en(&entorno.logs);
    let (base, actual) = abrir_base(&entorno)?;
    iniciar_sesion(&base)?;
    base.exigir(sua_nucleo::datos::usuarios::Permiso::Administrar)?;
    base.anotar("Cambió la contraseña de la base desde la línea de comandos", "");
    let sin_cifrar = base.respaldos_sin_cifrar()?.len();
    drop(base);
    let nueva = match std::env::var("SUA_CLAVE_NUEVA") {
        Ok(nueva) => {
            if !nueva.is_empty() {
                cifrado::validar_clave_nueva(&nueva, &nueva)?;
            }
            nueva
        }
        Err(_) => {
            eprintln!("Si olvida la contraseña, los datos no se pueden recuperar: no hay forma de restablecerla.");
            let nueva = leer_sin_eco(if actual.is_some() { "Contraseña nueva (vacía para quitarla): " } else { "Contraseña nueva: " })?;
            if !nueva.is_empty() {
                cifrado::validar_clave_nueva(&nueva, &leer_sin_eco("Repita la contraseña nueva: ")?)?;
            }
            nueva
        }
    };
    let nueva = Some(nueva.as_str()).filter(|n| !n.is_empty());
    if actual.is_none() && nueva.is_none() {
        println!("La base no tiene contraseña y no se le puso ninguna.");
        return Ok(false);
    }
    cifrado::cambiar_clave(&entorno.ruta_base(), actual.as_deref(), nueva)?;
    let hecho = match (&actual, nueva) {
        (None, Some(_)) => "La base quedó protegida con contraseña.",
        (Some(_), Some(_)) => "La contraseña de la base se cambió.",
        _ => "Se quitó la contraseña: la base ya no está cifrada.",
    };
    bitacora.info(hecho);
    println!("{hecho}");
    if nueva.is_some() && sin_cifrar > 0 {
        println!("Hay {sin_cifrar} respaldos anteriores sin cifrar en {}: cualquiera puede leerlos. Bórrelos si ya no los necesita.", entorno.respaldos.display());
    }
    if actual.is_some() {
        println!("Los respaldos hechos antes conservan la protección que tenían entonces.");
    }
    Ok(true)
}

/// Contraseña olvidada: pone una nueva a un administrador sin iniciar sesión.
/// La nueva se toma de `SUA_CLAVE_USUARIO_NUEVA` o se pregunta dos veces.
fn restablecer_usuario(nombre: Option<&String>) -> Resultado<bool> {
    let entorno = Entorno::preparar()?;
    let bitacora = Bitacora::en(&entorno.logs);
    let (base, _) = abrir_base(&entorno)?;
    if !base.hay_usuarios() {
        println!("La base no tiene usuarios: el programa abre sin pedir ninguno.");
        return Ok(false);
    }
    let Some(nombre) = nombre else {
        eprintln!("Falta el nombre del administrador. Administradores registrados: {}", base.nombres_de_administradores()?.join(", "));
        return Ok(false);
    };
    let (nueva, repetida) = match std::env::var("SUA_CLAVE_USUARIO_NUEVA") {
        Ok(nueva) => (nueva.clone(), nueva),
        Err(_) => (leer_sin_eco("Contraseña nueva: ")?, leer_sin_eco("Repita la contraseña nueva: ")?),
    };
    base.restablecer_administrador(nombre, &nueva, &repetida)?;
    bitacora.info(&format!("Contraseña del administrador {nombre} restablecida desde la línea de comandos."));
    println!("La contraseña de {nombre} quedó restablecida. El cambio está anotado en la bitácora de accesos.");
    Ok(true)
}

fn diagnostico() -> Resultado<()> {
    let (entorno, bitacora, base) = abrir()?;
    println!("Modo:               {}", if entorno.modo == Modo::Portable { "portable" } else { "instalado" });
    println!("Carpeta de datos:   {}", entorno.raiz.display());
    if let Some(servicio) = config::servicio_de_sincronizacion(&entorno.raiz) {
        println!("  Advertencia:      está dentro de {servicio}; conviene moverla a una carpeta que no se sincronice.");
    }
    println!("Base de datos:      {}", base.ruta().display());
    println!("Bitácora:           {}", bitacora.ruta().display());
    println!("Esquema:            versión {}", base.version_esquema()?);
    println!("Migraciones ahora:  {:?}", base.migraciones_aplicadas);
    println!("Modo de diario:     {}", base.modo_diario);
    println!("Cifrado:            {}", if base.cifrada() { "sí, con contraseña" } else { "no" });
    println!(
        "Usuarios:           {}",
        match base.sesion() {
            Some(s) => format!("sí; sesión de {} ({})", s.nombre, s.rol.nombre()),
            None => "no hay: el programa abre sin pedir usuario".to_string(),
        }
    );
    println!("Integridad:         {}", base.integridad()?);
    println!("Respaldos:          {}", base.respaldos()?.len());
    let mut filas = 0;
    for c in base.catalogos() {
        filas += base.filas_de_catalogo(c)?;
    }
    println!("Catálogos:          {} ({} filas)", base.catalogos().len(), filas);
    bitacora.info("Diagnóstico ejecutado.");
    Ok(())
}

fn leer_sua(ruta: &Path) -> Resultado<bool> {
    let datos = std::fs::read(ruta).map_err(ErrorSua::archivo(ruta))?;
    let a = pago::leer(&datos)?;
    let (p, t, c) = (&a.patron, &a.totales, &a.control);
    println!("Archivo:            {} bytes, {} registros", a.bytes, a.bytes / pago::LARGO_REGISTRO);
    println!("Periodo:            {}   folio SUA {}", p.periodo, p.folio_sua);
    println!("Prima de RT:        {}.{:05} %", p.prima_rt / 100_000, p.prima_rt % 100_000);
    println!("Trabajadores:       {}   movimientos: {}", a.trabajadores.len(), a.movimientos.len());
    println!("Fecha del 06:       {}", c.fecha);
    println!("IMSS:               {:>14}", pesos(c.total_imss));
    println!("  cuota fija        {:>14}", pesos(t.cuota_fija));
    println!("  excedente         {:>14}", pesos(t.excedente));
    println!("  prest. en dinero  {:>14}", pesos(t.prestaciones_dinero));
    println!("  gastos médicos    {:>14}", pesos(t.gastos_medicos));
    println!("  riesgos trabajo   {:>14}", pesos(t.riesgos_trabajo));
    println!("  invalidez y vida  {:>14}", pesos(t.invalidez_vida));
    println!("  guarderías        {:>14}", pesos(t.guarderias));
    println!("RCV:                {:>14}", pesos(c.total_rcv));
    println!("  retiro            {:>14}", pesos(t.retiro));
    println!("  cesantía y vejez  {:>14}", pesos(t.cyv));
    println!("  recargos          {:>14}", pesos(t.recargos_rcv));
    println!("INFONAVIT:          {:>14}", pesos(c.total_infonavit));
    let obrero: i64 = a
        .trabajadores
        .iter()
        .map(|x| {
            [x.excedente_obrero, x.prestaciones_dinero_obrero, x.gastos_medicos_obrero, x.invalidez_vida_obrero, x.cyv_obrera_codificada]
                .iter()
                .map(|v| v.unwrap_or(0))
                .sum::<i64>()
        })
        .sum();
    println!("Cuotas obreras (decodificadas de base 62): {}", pesos(obrero));

    let hallazgos = a.verificar();
    if hallazgos.is_empty() {
        println!("Verificación:       correcta (sumas, conteos, tamaño e importes codificados)");
    } else {
        println!("Verificación:       {} diferencias", hallazgos.len());
        for h in &hallazgos {
            println!("  - {h}");
        }
    }
    if hallazgos.is_empty() {
        // Comprobación contra el motor de cálculo propio (usa los catálogos de la base).
        let (_e, _b, base) = abrir()?;
        let con = comprobar_pago(&a, &base.tasas(&p.periodo, p.prima_rt, true)?.tasas);
        let sin = comprobar_pago(&a, &base.tasas(&p.periodo, p.prima_rt, false)?.tasas);
        let decreto = con.conceptos_iguales > sin.conceptos_iguales;
        let r = if decreto { con } else { sin };
        println!(
            "Cálculo propio:     {} de {} importes coinciden{}",
            r.conceptos_iguales,
            r.conceptos_comparados,
            if decreto { " (el archivo aplica un decreto de exención)" } else { "" }
        );
        for d in &r.diferencias {
            println!("  - {d}");
        }
    }
    Ok(hallazgos.is_empty())
}

fn importar_sua(ruta: &Path) -> Resultado<bool> {
    let datos = std::fs::read(ruta).map_err(ErrorSua::archivo(ruta))?;
    let a = pago::leer(&datos)?;
    let hallazgos = a.verificar();
    if !hallazgos.is_empty() {
        println!("El archivo tiene {} diferencias internas; no se importa. Revíselo con leer-sua.", hallazgos.len());
        return Ok(false);
    }
    let (_e, bitacora, base) = abrir()?;
    base.respaldar("antes-de-importar")?;
    let r = base.importar_pago(&a)?;
    bitacora.info("Importación de archivo de pago desde la línea de comandos.");
    println!("Patrón:        {}", if r.patron_nuevo { "nuevo" } else { "actualizado" });
    println!("Trabajadores:  {} nuevos, {} actualizados", r.trabajadores_nuevos, r.trabajadores_actualizados);
    println!("Movimientos:   {} nuevos, {} ya existían", r.movimientos_nuevos, r.movimientos_ya_existentes);
    println!("Con datos que no pasan la validación: {}", r.con_advertencias);
    Ok(true)
}

fn anonimizar_sua(entrada: &Path, salida: &Path) -> Resultado<bool> {
    if salida.exists() {
        eprintln!("Ya existe {}; elija otro nombre para la copia.", salida.display());
        return Ok(false);
    }
    let datos = std::fs::read(entrada).map_err(ErrorSua::archivo(entrada))?;
    let anonimo = sua_nucleo::io::anonimo::anonimizar(&datos)?;
    let hallazgos = pago::leer(&anonimo)?.verificar();
    std::fs::write(salida, &anonimo).map_err(ErrorSua::archivo(salida))?;
    println!("Copia sin datos personales: {}", salida.display());
    println!("Se conservan periodo, entidad, delegación, folio SUA, fecha de pago, salarios, días, fechas de los movimientos e importes.");
    if !hallazgos.is_empty() {
        println!("El archivo original ya tenía {} diferencias internas; la copia las conserva.", hallazgos.len());
    }
    Ok(true)
}

/// Datos ficticios: el registro patronal, los NSS y los nombres no son reales.
fn ejemplo() -> Resultado<bool> {
    use sua_nucleo::datos::captura::{Patron, Trabajador};
    use sua_nucleo::dominio::validar::luhn;
    let (_e, _b, base) = abrir()?;
    let reg = "Z9900000109";
    base.guardar_patron(&Patron {
        reg_pat: reg.into(),
        rfc_pat: "XAXX010101000".into(),
        nom_pat: "PATRON FICTICIO DE DEMOSTRACION SA DE CV".into(),
        act_pat: "CONSTRUCCION DE OBRAS (FICTICIO)".into(),
        dom_pat: "CALLE FICTICIA 123 COL CENTRO".into(),
        mun_pat: "CIUDAD DE EJEMPLO".into(),
        cpp_pat: "06000".into(),
        ent_pat: "09".into(),
        zon_pat: "A".into(),
        del_pat: "3112".into(),
        ..Patron::default()
    })?;
    base.guardar_prima_rt(reg, "2026-03", 758_875)?;
    let nombres = ["EJEMPLO$UNO$ANA", "EJEMPLO$DOS$LUIS", "MUESTRA$TRES$MARIA", "MUESTRA$$PEDRO", "PRUEBA$CINCO$ROSA"];
    for (i, nombre) in nombres.iter().enumerate() {
        let base10 = format!("{:010}", 9_000_000_001u64 + i as u64 * 7919);
        base.guardar_trabajador(&Trabajador {
            reg_patr: reg.into(),
            num_afil: format!("{base10}{}", luhn(&base10).unwrap()),
            nom_aseg: nombre.to_string(),
            sal_imss: 31_504 + i as i64 * 5_000,
            fec_alt: "2026-01-01".into(),
            tip_tra: "1".into(),
            sem_jord: "0".into(),
            ..Trabajador::default()
        })?;
    }
    println!("Datos de ejemplo cargados: 1 patrón y {} trabajadores ficticios.", nombres.len());
    Ok(true)
}

fn ejecutar(args: &[String]) -> Resultado<bool> {
    match args.first().map(String::as_str) {
        Some("diagnostico") => diagnostico().map(|_| true),
        Some("respaldar") => {
            let (_e, bitacora, base) = abrir()?;
            let r = base.respaldar("manual")?;
            bitacora.info(&format!("Respaldo manual: {}", r.display()));
            println!("Respaldo creado: {}", r.display());
            Ok(true)
        }
        Some("compactar") => {
            let (_e, bitacora, base) = abrir()?;
            base.compactar()?;
            bitacora.info("Base compactada.");
            println!("Base compactada.");
            Ok(true)
        }
        Some("activar-portable") => {
            let ruta = config::activar_portable(&config::carpeta_del_ejecutable()?)?;
            println!("Modo portable activado: {}", ruta.display());
            Ok(true)
        }
        Some("recuperar") => recuperar(),
        Some("clave") => clave(),
        Some("restablecer-usuario") => restablecer_usuario(args.get(1)),
        Some("ejemplo") => ejemplo(),
        Some("anonimizar-sua") => match (args.get(1), args.get(2)) {
            (Some(entrada), Some(salida)) => anonimizar_sua(Path::new(entrada), Path::new(salida)),
            _ => {
                eprintln!("Faltan el archivo de entrada y el de salida.

{AYUDA}");
                Ok(false)
            }
        },
        Some("importar-sua") => match args.get(1) {
            Some(ruta) => importar_sua(Path::new(ruta)),
            None => {
                eprintln!("Falta la ruta del archivo.

{AYUDA}");
                Ok(false)
            }
        },
        Some("leer-sua") => match args.get(1) {
            Some(ruta) => leer_sua(Path::new(ruta)),
            None => {
                eprintln!("Falta la ruta del archivo.\n\n{AYUDA}");
                Ok(false)
            }
        },
        _ => {
            print!("{AYUDA}");
            Ok(args.is_empty())
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match ejecutar(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::from(2)
        }
    }
}
