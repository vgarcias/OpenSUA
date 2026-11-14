//! Interfaz gráfica de OpenSUA. Toda la lógica vive en `sua-nucleo`; aquí
//! solo se conectan las pantallas con el núcleo.
#![windows_subsystem = "windows"]

use slint::{ComponentHandle, ModelRc, SharedString, StandardListViewItem, TableColumn, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use sua_nucleo::bitacora::Bitacora;
use sua_nucleo::config::{nombre_seguro, servicio_de_sincronizacion, Entorno, Modo};
use sua_nucleo::datos::cifrado;
use sua_nucleo::datos::usuarios::{Permiso, Rol, Usuario};
use sua_nucleo::datos::restaurar::{recuperar, ultimo_respaldo_util};
use sua_nucleo::datos::captura::{centavos_a_texto, texto_a_centavos, Patron, Problema, Trabajador};
use sua_nucleo::datos::periodo::{CalculoPeriodo, MovimientoGuardado, TIPOS_DE_CREDITO, TIPOS_DE_MOVIMIENTO};
use sua_nucleo::dominio::credito::texto_a_valor;
use sua_nucleo::datos::{Base, Tabla};
use sua_nucleo::dominio::comprobar::comprobar_pago;
use sua_nucleo::io::pago::{self, ArchivoPago};
use sua_nucleo::datos::expediente::{Afiliacion, DatosIncapacidad};
use sua_nucleo::dominio::prima_rt::prima_en_texto;
use sua_nucleo::io::txt::Clase;
use sua_nucleo::reportes::listados::{detalle_de_movimiento, movimientos_pdf, trabajadores_pdf};
use sua_nucleo::reportes::cedula::{cedula_pdf, nombre_de_archivo, periodo_en_texto, Cedula};
use sua_nucleo::Resultado;

slint::include_modules!();

mod autoprueba;

#[derive(Default)]
struct Estado {
    /// Lo que pasó al abrir y el usuario debe ver en Sistema (una base recuperada de un respaldo).
    aviso_de_arranque: Option<String>,
    /// Cambio de contraseña pedido en Sistema (actual, nueva): se aplica al cerrar la ventana.
    cambio_de_clave: Option<(Option<String>, Option<String>)>,
    /// Hay que cerrar y volver a abrir el programa (tras restaurar, los usuarios son otros): texto que se mostrará al reabrir.
    reinicio: Option<String>,
    /// Usuarios listados en Sistema, en el orden de la lista (sin la posición 0, «Usuario nuevo»).
    usuarios: Vec<Usuario>,
    patrones: Vec<Patron>,
    trabajadores: Vec<Trabajador>,
    filtro_trabajadores: String,
    movimientos: Vec<MovimientoGuardado>,
    catalogo: Tabla,
    pago: Option<ArchivoPago>,
    /// Último cálculo de todo un patrón (o cálculo guardado abierto): registro patronal y resultado.
    calculo: Option<(String, CalculoPeriodo)>,
    /// Respaldos listados en Sistema, del más reciente al más antiguo.
    respaldos: Vec<std::path::PathBuf>,
    /// Claves de las entidades federativas, en el orden de la lista; la posición 0 es «sin elegir».
    entidades: Vec<String>,
}

pub(crate) struct Contexto {
    pub(crate) entorno: Entorno,
    pub(crate) base: Base,
    pub(crate) bitacora: Bitacora,
    estado: RefCell<Estado>,
}

fn celda(texto: &str) -> StandardListViewItem {
    StandardListViewItem::from(SharedString::from(texto))
}

fn modelo(filas: Vec<Vec<StandardListViewItem>>) -> ModelRc<ModelRc<StandardListViewItem>> {
    let filas: Vec<ModelRc<StandardListViewItem>> = filas.into_iter().map(|f| ModelRc::new(VecModel::from(f))).collect();
    ModelRc::new(VecModel::from(filas))
}

fn pesos(centavos: i64) -> String {
    let entero = (centavos / 100).to_string();
    let mut s = String::new();
    for (i, c) in entero.chars().enumerate() {
        if i > 0 && (entero.len() - i) % 3 == 0 {
            s.push(',');
        }
        s.push(c);
    }
    format!("{s}.{:02}", centavos % 100)
}

fn avisos(problemas: &[Problema]) -> String {
    problemas.iter().map(|p| format!("Advertencia — {}: {}", p.campo, p.mensaje)).collect::<Vec<_>>().join("\n")
}

/// `7.58875` → 758875 cienmilésimas. Hasta 5 decimales.
fn prima_a_cienmilesimas(texto: &str) -> Option<i64> {
    let t = texto.trim();
    let (entero, dec) = t.split_once('.').unwrap_or((t, ""));
    if entero.is_empty() || entero.len() > 2 || dec.len() > 5 || !entero.bytes().chain(dec.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(entero.parse::<i64>().ok()? * 100_000 + format!("{dec:0<5}").parse::<i64>().ok()?)
}

fn cienmilesimas_a_prima(v: i64) -> String {
    format!("{}.{:05}", v / 100_000, v % 100_000)
}

// ---------------------------------------------------------------- Patrones

fn refrescar_patrones(ui: &Principal, c: &Contexto) -> Resultado<()> {
    let patrones = c.base.patrones()?;
    let mut filas = Vec::new();
    let mut lista = Vec::new();
    for p in &patrones {
        let n = c.base.trabajadores_de(&p.reg_pat)?;
        filas.push(vec![celda(&p.reg_pat), celda(&p.nom_pat), celda(&n.to_string())]);
        lista.push(SharedString::from(format!("{} — {}", p.reg_pat, p.nom_pat)));
    }
    ui.set_filas_patrones(modelo(filas));
    let anterior = ui.get_indice_patron();
    ui.set_lista_patrones(ModelRc::new(VecModel::from(lista)));
    let nuevo = if patrones.is_empty() { -1 } else { anterior.clamp(0, patrones.len() as i32 - 1) };
    ui.set_indice_patron(nuevo);
    c.estado.borrow_mut().patrones = patrones;
    Ok(())
}

fn mostrar_patron(ui: &Principal, c: &Contexto, p: Option<&Patron>) {
    let vacio = Patron::default();
    let existe = p.is_some();
    let p = p.unwrap_or(&vacio);
    ui.set_patron_existente(existe);
    // La tabla marca la fila del patrón cargado (ninguna si es nuevo).
    let fila = if existe { c.estado.borrow().patrones.iter().position(|x| x.reg_pat == p.reg_pat).map_or(-1, |i| i as i32) } else { -1 };
    ui.set_p_fila(fila);
    ui.set_p_reg(p.reg_pat.as_str().into());
    ui.set_p_rfc(p.rfc_pat.as_str().into());
    ui.set_p_nombre(p.nom_pat.as_str().into());
    ui.set_p_actividad(p.act_pat.as_str().into());
    ui.set_p_domicilio(p.dom_pat.as_str().into());
    ui.set_p_municipio(p.mun_pat.as_str().into());
    ui.set_p_cp(p.cpp_pat.as_str().into());
    ui.set_p_entidad(indice_de(&c.estado.borrow().entidades, &p.ent_pat));
    ui.set_p_telefono(p.tel_pat.as_str().into());
    ui.set_p_zona(p.zon_pat.as_str().into());
    ui.set_p_delegacion(p.del_pat.as_str().into());
    let prima = if existe { c.base.prima_rt_vigente(&p.reg_pat).ok().flatten() } else { None };
    ui.set_p_prima(prima.as_ref().map(|(_, v)| cienmilesimas_a_prima(*v)).unwrap_or_default().into());
    ui.set_p_prima_desde(prima.map(|(d, _)| d).unwrap_or_default().into());
    actualizar_bloqueos(ui, c);
}

fn guardar_patron(ui: &Principal, c: &Contexto) -> Resultado<String> {
    let p = Patron {
        reg_pat: ui.get_p_reg().into(),
        rfc_pat: ui.get_p_rfc().into(),
        nom_pat: ui.get_p_nombre().into(),
        act_pat: ui.get_p_actividad().into(),
        dom_pat: ui.get_p_domicilio().into(),
        mun_pat: ui.get_p_municipio().into(),
        cpp_pat: ui.get_p_cp().into(),
        ent_pat: {
            let estado = c.estado.borrow();
            let guardada = estado.patrones.iter().find(|x| x.reg_pat == ui.get_p_reg().trim().to_uppercase()).map(|x| x.ent_pat.clone()).unwrap_or_default();
            codigo_de(&estado.entidades, ui.get_p_entidad(), &guardada)
        },
        tel_pat: ui.get_p_telefono().into(),
        zon_pat: ui.get_p_zona().into(),
        del_pat: ui.get_p_delegacion().into(),
    };
    // La prima se valida antes de escribir nada.
    let (prima_txt, desde): (String, String) = (ui.get_p_prima().trim().to_string(), ui.get_p_prima_desde().trim().to_string());
    let prima = if prima_txt.is_empty() && desde.is_empty() {
        None
    } else {
        let valor = prima_a_cienmilesimas(&prima_txt)
            .ok_or_else(|| sua_nucleo::ErrorSua::Validacion("Prima de RT: escriba un porcentaje como 7.58875 (hasta 5 decimales).".into()))?;
        let b = desde.as_bytes();
        let mes_ok = b.len() == 7 && b[4] == b'-' && desde[..4].parse::<u32>().is_ok() && matches!(desde[5..].parse::<u32>(), Ok(1..=12));
        if !mes_ok {
            return Err(sua_nucleo::ErrorSua::Validacion("Vigente desde: use el formato AAAA-MM.".into()));
        }
        Some((desde, valor))
    };
    let advertencias = c.base.guardar_patron(&p)?;
    let reg = p.reg_pat.trim().to_uppercase();
    if let Some((desde, valor)) = prima {
        c.base.guardar_prima_rt(&reg, &desde, valor)?;
    }
    c.bitacora.info("Patrón guardado.");
    refrescar_patrones(ui, c)?;
    let guardado = c.estado.borrow().patrones.iter().find(|x| x.reg_pat == reg).cloned();
    mostrar_patron(ui, c, guardado.as_ref());
    Ok(if advertencias.is_empty() { "Patrón guardado.".to_string() } else { format!("Patrón guardado.\n{}", avisos(&advertencias)) })
}

// ------------------------------------------------------------ Trabajadores

pub(crate) fn patron_activo(ui: &Principal, c: &Contexto) -> Option<String> {
    let i = ui.get_indice_patron();
    c.estado.borrow().patrones.get(usize::try_from(i).ok()?).map(|p| p.reg_pat.clone())
}

fn refrescar_trabajadores(ui: &Principal, c: &Contexto) -> Resultado<()> {
    let lista = match patron_activo(ui, c) {
        Some(reg) => c.base.trabajadores(&reg, &c.estado.borrow().filtro_trabajadores)?,
        None => Vec::new(),
    };
    let filas = lista
        .iter()
        .map(|t| vec![celda(&t.num_afil), celda(&t.nom_aseg.replace('$', " ")), celda(&centavos_a_texto(t.sal_imss)), celda(&t.tip_tra)])
        .collect();
    ui.set_filas_trabajadores(modelo(filas));
    c.estado.borrow_mut().trabajadores = lista;
    Ok(())
}

/// Datos afiliatorios escritos en el formulario de Trabajadores.
fn afiliacion_del_formulario(ui: &Principal, c: &Contexto, reg: &str, nss: &str) -> Afiliacion {
    let guardada = c.base.afiliacion(reg, nss).ok().flatten().unwrap_or_default();
    Afiliacion {
        reg_patr: reg.to_string(),
        num_afil: nss.to_string(),
        cpp_trab: ui.get_a_cp().into(),
        fec_nac: ui.get_a_nacimiento().into(),
        lug_nac: ui.get_a_lugar().into(),
        ent_trab: codigo_de(&c.estado.borrow().entidades, ui.get_a_entidad(), &guardada.ent_trab),
        umf_trab: ui.get_a_umf().into(),
        ocupa: ui.get_a_ocupacion().into(),
        sexo: codigo_de(&SEXOS, ui.get_a_sexo(), &guardada.sexo),
        tip_sal: codigo_de(&TIPOS_DE_SALARIO, ui.get_a_tipo_salario(), &guardada.tip_sal),
        jor_hor: ui.get_a_jornada().into(),
    }
}

fn mostrar_trabajador(ui: &Principal, c: &Contexto, t: Option<&Trabajador>) {
    let a = t.and_then(|t| c.base.afiliacion(&t.reg_patr, &t.num_afil).ok().flatten()).unwrap_or_default();
    ui.set_a_cp(a.cpp_trab.as_str().into());
    ui.set_a_nacimiento(a.fec_nac.as_str().into());
    ui.set_a_lugar(a.lug_nac.as_str().into());
    ui.set_a_entidad(indice_de(&c.estado.borrow().entidades, &a.ent_trab));
    ui.set_a_umf(a.umf_trab.as_str().into());
    ui.set_a_ocupacion(a.ocupa.as_str().into());
    ui.set_a_sexo(indice_de(&SEXOS, &a.sexo));
    ui.set_a_tipo_salario(indice_de(&TIPOS_DE_SALARIO, &a.tip_sal));
    ui.set_a_jornada(a.jor_hor.as_str().into());
    let vacio = Trabajador::default();
    let fila = t.and_then(|t| c.estado.borrow().trabajadores.iter().position(|x| x.num_afil == t.num_afil)).map_or(-1, |i| i as i32);
    ui.set_trabajador_existente(t.is_some());
    ui.set_t_fila(fila);
    let existe = t.is_some();
    let t = t.unwrap_or(&vacio);
    let mut partes = t.nom_aseg.split('$');
    ui.set_t_nss(t.num_afil.as_str().into());
    ui.set_t_paterno(partes.next().unwrap_or("").into());
    ui.set_t_materno(partes.next().unwrap_or("").into());
    ui.set_t_nombre(partes.next().unwrap_or("").into());
    ui.set_t_rfc(t.rfc_curp.as_str().into());
    ui.set_t_curp(t.curp.as_str().into());
    ui.set_t_sdi(if existe { centavos_a_texto(t.sal_imss).into() } else { SharedString::new() });
    ui.set_t_alta(t.fec_alt.as_str().into());
    ui.set_t_tipo(indice_de(&TIPOS_DE_TRABAJADOR, &t.tip_tra));
    ui.set_t_jornada(indice_de(&JORNADAS, &t.sem_jord));
    ui.set_t_credito(t.pag_info.as_str().into());
    actualizar_bloqueos(ui, c);
}

fn guardar_trabajador(ui: &Principal, c: &Contexto) -> Resultado<String> {
    let reg = patron_activo(ui, c).ok_or_else(|| sua_nucleo::ErrorSua::Validacion("Registre primero un patrón.".into()))?;
    let sdi = texto_a_centavos(&ui.get_t_sdi())
        .ok_or_else(|| sua_nucleo::ErrorSua::Validacion("Salario diario integrado: escriba un importe como 315.04.".into()))?;
    let nss_escrito = ui.get_t_nss().trim().to_string();
    let guardado_antes = c.estado.borrow().trabajadores.iter().find(|x| x.num_afil == nss_escrito).cloned().unwrap_or_default();
    let t = Trabajador {
        reg_patr: reg,
        num_afil: ui.get_t_nss().into(),
        curp: ui.get_t_curp().into(),
        rfc_curp: ui.get_t_rfc().into(),
        nom_aseg: format!("{}${}${}", ui.get_t_paterno().trim(), ui.get_t_materno().trim(), ui.get_t_nombre().trim()),
        sal_imss: sdi,
        fec_alt: ui.get_t_alta().into(),
        tip_tra: codigo_de(&TIPOS_DE_TRABAJADOR, ui.get_t_tipo(), &guardado_antes.tip_tra),
        sem_jord: codigo_de(&JORNADAS, ui.get_t_jornada(), &guardado_antes.sem_jord),
        pag_info: ui.get_t_credito().into(),
    };
    // Los datos afiliatorios se leen antes de guardar: al refrescar se recargan de la base.
    let afiliacion = afiliacion_del_formulario(ui, c, &t.reg_patr, t.num_afil.trim());
    let advertencias = c.base.guardar_trabajador(&t)?;
    let ya_tenia = c.base.afiliacion(&afiliacion.reg_patr, &afiliacion.num_afil)?.is_some();
    let afiliacion_mal = if afiliacion.vacia() && !ya_tenia { None } else { c.base.guardar_afiliacion(&afiliacion).err() };
    c.bitacora.info("Trabajador guardado.");
    refrescar_trabajadores(ui, c)?;
    refrescar_patrones(ui, c)?;
    let nss = t.num_afil.trim().to_string();
    let guardado = c.estado.borrow().trabajadores.iter().find(|x| x.num_afil == nss).cloned();
    if let Some(e) = afiliacion_mal {
        // El trabajador quedó guardado; los datos afiliatorios se dejan en pantalla para corregirlos.
        ui.set_trabajador_existente(true);
        return Ok(format!("Trabajador guardado, pero no sus datos afiliatorios.\n{e}"));
    }
    mostrar_trabajador(ui, c, guardado.as_ref());
    Ok(if advertencias.is_empty() { "Trabajador guardado.".to_string() } else { format!("Trabajador guardado.\n{}", avisos(&advertencias)) })
}

// ------------------------------------------------------------- Movimientos

/// Tipos que ofrece la pantalla, en el orden de la lista: afiliatorios,
/// incidencias y crédito INFONAVIT.
fn tipos_de_movimiento() -> impl Iterator<Item = &'static (&'static str, &'static str)> {
    TIPOS_DE_MOVIMIENTO.iter().chain(TIPOS_DE_CREDITO)
}

/// Trabajador cargado en el formulario de Trabajadores: (registro patronal, NSS).
fn trabajador_actual(ui: &Principal, c: &Contexto) -> Option<(String, String)> {
    if !ui.get_trabajador_existente() {
        return None;
    }
    Some((patron_activo(ui, c)?, ui.get_t_nss().to_string()))
}

fn refrescar_movimientos(ui: &Principal, c: &Contexto) -> Resultado<()> {
    let actual = trabajador_actual(ui, c);
    ui.set_m_habilitado(actual.is_some());
    let lista = match &actual {
        Some((reg, nss)) => {
            let nombre = format!("{} {} {}", ui.get_t_paterno(), ui.get_t_materno(), ui.get_t_nombre());
            ui.set_m_trabajador(format!("{nss} — {}", nombre.split_whitespace().collect::<Vec<_>>().join(" ")).into());
            c.base.movimientos(reg, nss)?
        }
        None => {
            ui.set_m_trabajador("Elija un trabajador en la pantalla Trabajadores.".into());
            Vec::new()
        }
    };
    let descripcion = |tipo: &str| {
        let nombre = match tipo {
            "00" | "13" => "Salario al inicio del bimestre",
            otro => tipos_de_movimiento().find(|(t, _)| *t == otro).map(|(_, d)| *d).unwrap_or("Otro"),
        };
        format!("{tipo} {nombre}")
    };
    let filas = lista
        .iter()
        .map(|m| {
            let detalle = detalle_de_movimiento(m).replace("salario ", "");
            vec![celda(&m.fecha), celda(&descripcion(&m.tipo)), celda(&detalle)]
        })
        .collect();
    ui.set_filas_movimientos(modelo(filas));
    ui.set_m_fila(-1);
    c.estado.borrow_mut().movimientos = lista;
    actualizar_bloqueos(ui, c);
    Ok(())
}

fn registrar_movimiento(ui: &Principal, c: &Contexto) -> Resultado<String> {
    let mal = |m: &str| sua_nucleo::ErrorSua::Validacion(m.to_string());
    let (reg, nss) = trabajador_actual(ui, c).ok_or_else(|| mal("Elija primero un trabajador."))?;
    let tipo = usize::try_from(ui.get_m_tipo()).ok().and_then(|i| tipos_de_movimiento().nth(i)).ok_or_else(|| mal("Elija el tipo de movimiento."))?.0;
    if TIPOS_DE_CREDITO.iter().any(|(t, _)| *t == tipo) {
        // Cada movimiento usa solo los datos que le tocan; los demás campos se ignoran.
        // La lista da la posición 0 a 2; los códigos del SUA son 1 a 3.
        let tipo_descuento = if matches!(tipo, "15" | "18") { ui.get_m_tipo_descuento().clamp(0, 2) as u8 + 1 } else { 0 };
        let valor = if matches!(tipo, "15" | "18" | "19") {
            texto_a_valor(&ui.get_m_valor()).ok_or_else(|| mal("Valor del descuento: escriba un número como 25, 1500.00 o 28.5320."))?
        } else {
            0
        };
        c.base.guardar_credito(&reg, &nss, tipo, ui.get_m_fecha().trim(), ui.get_m_credito().trim(), tipo_descuento, valor)?;
        c.bitacora.info("Movimiento de crédito registrado.");
        refrescar_movimientos(ui, c)?;
        refrescar_trabajadores(ui, c)?;
        // El número de crédito del trabajador pudo cambiar.
        let guardado = c.estado.borrow().trabajadores.iter().find(|x| x.num_afil == nss).cloned();
        if let Some(t) = guardado {
            ui.set_t_credito(t.pag_info.as_str().into());
        }
        ui.set_m_fecha(SharedString::new());
        ui.set_m_credito(SharedString::new());
        ui.set_m_valor(SharedString::new());
        return Ok("Movimiento de crédito registrado.".to_string());
    }
    if matches!(tipo, "11" | "12") {
        let dias = ui.get_m_dias().trim().parse::<u32>().map_err(|_| mal("Días: escriba cuántos días dura el ausentismo o la incapacidad."))?;
        // Datos médicos de la incapacidad: opcionales; si se escribe el ramo, se validan antes de guardar nada.
        let medicos = if tipo == "12" && ui.get_m_ramo() > 0 {
            let ramo = ui.get_m_ramo().clamp(1, 3) as u8;
            let de_trabajo = ramo == 1;
            let texto = ui.get_m_porcentaje().trim().to_string();
            let porcentaje = if !de_trabajo || texto.is_empty() { 0 } else { texto.parse::<u8>().map_err(|_| mal("Porcentaje de incapacidad permanente: escriba un número de 0 a 100."))? };
            let d = DatosIncapacidad {
                ramo,
                // El tipo de riesgo solo existe en riesgos de trabajo; la lista da la posición 0 a 2.
                tipo_riesgo: if de_trabajo { ui.get_m_riesgo().clamp(0, 2) as u8 + 1 } else { 0 },
                secuela: ui.get_m_secuela().clamp(0, 9) as u8,
                control: ui.get_m_control().clamp(0, 9) as u8,
                porcentaje,
            };
            sua_nucleo::datos::expediente::validar_datos_de_incapacidad(&d)?;
            Some(d)
        } else {
            None
        };
        c.base.guardar_incidencia(&reg, &nss, tipo, ui.get_m_fecha().trim(), dias, ui.get_m_folio().trim())?;
        if let Some(d) = medicos {
            c.base.guardar_datos_de_incapacidad(&reg, &nss, ui.get_m_folio().trim(), &d)?;
        }
        c.bitacora.info(if tipo == "11" { "Ausentismo registrado." } else { "Incapacidad registrada." });
        refrescar_movimientos(ui, c)?;
        for limpiar in [Principal::set_m_ramo, Principal::set_m_riesgo, Principal::set_m_secuela, Principal::set_m_control] {
            limpiar(ui, 0);
        }
        ui.set_m_porcentaje(SharedString::new());
        ui.set_m_fecha(SharedString::new());
        ui.set_m_dias(SharedString::new());
        ui.set_m_folio(SharedString::new());
        return Ok(if tipo == "11" { "Ausentismo registrado." } else { "Incapacidad registrada." }.to_string());
    }
    let texto_sdi = ui.get_m_sdi().trim().to_string();
    let sdi = if tipo == "02" && texto_sdi.is_empty() {
        0
    } else {
        texto_a_centavos(&texto_sdi).ok_or_else(|| mal("Salario diario integrado: escriba un importe como 315.04."))?
    };
    c.base.guardar_movimiento(&reg, &nss, tipo, ui.get_m_fecha().trim(), sdi)?;
    c.bitacora.info("Movimiento registrado.");
    refrescar_movimientos(ui, c)?;
    refrescar_trabajadores(ui, c)?;
    // El salario vigente del trabajador pudo cambiar.
    let guardado = c.estado.borrow().trabajadores.iter().find(|x| x.num_afil == nss).cloned();
    if let Some(t) = guardado {
        ui.set_t_sdi(centavos_a_texto(t.sal_imss).into());
    }
    ui.set_m_fecha(SharedString::new());
    ui.set_m_sdi(SharedString::new());
    Ok("Movimiento registrado.".to_string())
}

// --------------------------------------------------------------- Catálogos

fn mostrar_catalogo(ui: &Principal, tabla: &Tabla, filtro: &str) {
    let f = filtro.to_lowercase();
    let filas = tabla
        .filas
        .iter()
        .filter(|fila| f.is_empty() || fila.iter().any(|x| x.to_lowercase().contains(&f)))
        .map(|fila| fila.iter().map(|x| celda(x)).collect())
        .collect();
    ui.set_filas_catalogo(modelo(filas));
}

fn cargar_catalogo(ui: &Principal, c: &Contexto, nombre: &str) -> Resultado<()> {
    let tabla = c.base.leer_catalogo(nombre)?;
    let columnas: Vec<TableColumn> = tabla
        .columnas
        .iter()
        .map(|n| {
            let mut col = TableColumn::default();
            col.title = n.as_str().into();
            col.min_width = 90.0;
            col.horizontal_stretch = 1.0;
            col
        })
        .collect();
    ui.set_columnas_catalogo(ModelRc::new(VecModel::from(columnas)));
    mostrar_catalogo(ui, &tabla, "");
    c.estado.borrow_mut().catalogo = tabla;
    ui.set_catalogo_es_inpc(nombre == "INPC");
    Ok(())
}

// --------------------------------------------------------- Archivo de pago

/// Recalcula el archivo con el motor propio y describe el resultado.
fn comprobacion_del_calculo(c: &Contexto, a: &ArchivoPago) -> String {
    let p = &a.patron;
    let calcular = |decreto: bool| c.base.tasas(&p.periodo, p.prima_rt, decreto).map(|v| (comprobar_pago(a, &v.tasas), v));
    let (sin, con) = match (calcular(false), calcular(true)) {
        (Ok(s), Ok(k)) => (s, k),
        (Err(e), _) | (_, Err(e)) => return format!("Comprobación del cálculo: no se pudo hacer. {e}"),
    };
    let con_decreto = con.1.tasas.descuento_decreto > 0 && con.0.conceptos_iguales > sin.0.conceptos_iguales;
    let (r, v) = if con_decreto { con } else { sin };
    let mut s = format!(
        "Comprobación del cálculo: {} de {} importes coinciden con el cálculo propio",
        r.conceptos_iguales, r.conceptos_comparados
    );
    if r.coincide() {
        s.push('.');
    } else {
        s.push_str(&format!(" (diferencia total {}).", pesos(r.diferencia_total)));
    }
    if con_decreto {
        s.push_str(&format!("\n  El archivo aplica el decreto de exención del {} % a las cuotas patronales del IMSS.", v.tasas.descuento_decreto));
    }
    if v.desactualizado {
        s.push_str("\n  Aviso: el catálogo de salario mínimo o UMA no tiene valores para este año; el resultado no es confiable.");
    }
    for d in r.diferencias.iter().take(12) {
        s.push_str(&format!("\n  - {d}"));
    }
    if r.diferencias.len() > 12 {
        s.push_str(&format!("\n  … y {} más.", r.diferencias.len() - 12));
    }
    if !r.coincide() {
        s.push_str("\n  Diferencias de centavos pueden deberse a trabajadores con varios periodos o salarios en el mes.");
    }
    s
}

fn resumen_pago(c: &Contexto, nombre: &str, a: &ArchivoPago) -> (String, bool) {
    let (p, t, k) = (&a.patron, &a.totales, &a.control);
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
    let mut s = format!(
        "Archivo:            {nombre}\n\
         Tamaño:             {} bytes, {} registros\n\
         Patrón:             {}  {}\n\
         Periodo:            {}    Folio SUA: {}    Fecha: {}\n\
         Prima de RT:        {} %\n\
         Trabajadores:       {}    Movimientos: {}\n\n\
         IMSS                {:>14}\n\
         \x20 Cuota fija        {:>14}\n\
         \x20 Excedente         {:>14}\n\
         \x20 Prest. en dinero  {:>14}\n\
         \x20 Gastos médicos    {:>14}\n\
         \x20 Riesgos trabajo   {:>14}\n\
         \x20 Invalidez y vida  {:>14}\n\
         \x20 Guarderías        {:>14}\n\
         RCV                 {:>14}\n\
         \x20 Retiro            {:>14}\n\
         \x20 Cesantía y vejez  {:>14}\n\
         \x20 Recargos          {:>14}\n\
         INFONAVIT           {:>14}\n\
         Total               {:>14}\n\n\
         Cuotas obreras (decodificadas): {}\n\n",
        a.bytes,
        a.bytes / pago::LARGO_REGISTRO,
        p.registro_patronal,
        p.razon_social,
        p.periodo,
        p.folio_sua,
        k.fecha,
        cienmilesimas_a_prima(p.prima_rt),
        a.trabajadores.len(),
        a.movimientos.len(),
        pesos(k.total_imss),
        pesos(t.cuota_fija),
        pesos(t.excedente),
        pesos(t.prestaciones_dinero),
        pesos(t.gastos_medicos),
        pesos(t.riesgos_trabajo),
        pesos(t.invalidez_vida),
        pesos(t.guarderias),
        pesos(k.total_rcv),
        pesos(t.retiro),
        pesos(t.cyv),
        pesos(t.recargos_rcv),
        pesos(k.total_infonavit),
        pesos(k.total_imss + k.total_rcv + k.total_infonavit),
        pesos(obrero),
    );
    let hallazgos = a.verificar();
    if hallazgos.is_empty() {
        s.push_str("Verificación: correcta. Sumas, conteos, tamaño e importes codificados coinciden.\n\n");
        s.push_str(&comprobacion_del_calculo(c, a));
    } else {
        s.push_str(&format!("Verificación: {} diferencias. No se puede importar.\n", hallazgos.len()));
        for h in &hallazgos {
            s.push_str(&format!("  - {h}\n"));
        }
    }
    (s, hallazgos.is_empty())
}

pub(crate) fn abrir_pago(ui: &Principal, c: &Contexto, ruta: &std::path::Path) {
    let nombre = ruta.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let leido = std::fs::read(ruta).map_err(|e| e.to_string()).and_then(|d| pago::leer(&d).map_err(|e| e.to_string()));
    match leido {
        Ok(a) => {
            let (texto, correcto) = resumen_pago(c, &nombre, &a);
            ui.set_pago_resumen(texto.into());
            ui.set_pago_importable(correcto);
            if correcto {
                avisar(ui, Canal::Pago, 1, "Archivo leído y verificado: se puede importar.");
            } else {
                avisar(ui, Canal::Pago, 2, "El archivo tiene diferencias en su verificación: no se puede importar. El detalle está abajo.");
            }
            c.estado.borrow_mut().pago = Some(a);
            c.bitacora.info("Archivo de pago leído.");
        }
        Err(e) => {
            ui.set_pago_resumen(format!("No se pudo leer {nombre}.
{e}").into());
            ui.set_pago_importable(false);
            avisar(ui, Canal::Pago, 3, format!("No se pudo leer {nombre}."));
            c.estado.borrow_mut().pago = None;
        }
    }
}

// ----------------------------------------------------------------- Cálculo

fn calcular(ui: &Principal, c: &Contexto) -> Resultado<String> {
    let mal = |m: &str| sua_nucleo::ErrorSua::Validacion(m.to_string());
    let periodo = ui.get_c_periodo().trim().replace('-', "");
    let sdi = texto_a_centavos(&ui.get_c_sdi())
        .filter(|v| *v > 0)
        .ok_or_else(|| mal("Salario diario integrado: escriba un importe como 315.04."))?;
    let dias = |texto: SharedString, max: u32, nombre: &str| -> Resultado<u32> {
        let t = texto.trim().to_string();
        if t.is_empty() {
            return Ok(0);
        }
        t.parse::<u32>().ok().filter(|d| *d <= max).ok_or_else(|| mal(&format!("{nombre}: escriba un número entre 0 y {max}.")))
    };
    let dias_mes = dias(ui.get_c_dias(), 31, "Días del mes")?;
    let dias_bim = dias(ui.get_c_dias_bim(), 62, "Días del bimestre")?;
    if dias_mes + dias_bim == 0 {
        return Err(mal("Escriba los días del mes, los del bimestre o ambos."));
    }
    let prima = prima_a_cienmilesimas(&ui.get_c_prima()).ok_or_else(|| mal("Prima de RT: escriba un porcentaje como 7.58875."))?;
    let v = c.base.tasas(&periodo, prima, ui.get_c_decreto())?;
    let t = &v.tasas;
    let mut s = format!(
        "Periodo {periodo}   {} diaria: {:.2}   Cuota fija: {:.2} %   Excedente: {:.2} % + {:.2} %\n",
        if v.usa_uma { "UMA" } else { "Salario mínimo" },
        t.base_cuota_fija,
        t.cuota_fija,
        t.excedente_patronal,
        t.excedente_obrero
    );
    if t.descuento_decreto > 0 {
        s.push_str(&format!("Con decreto de exención del {} % sobre las cuotas patronales del IMSS.\n", t.descuento_decreto));
    }
    if v.desactualizado {
        s.push_str("AVISO: el catálogo de salario mínimo o UMA no tiene valores para este año. El resultado NO es el vigente.\n");
    }
    let mut total = 0;
    if dias_mes > 0 {
        let m = t.mes(sdi, dias_mes);
        s.push_str(&format!("\nIMSS, {dias_mes} días\n"));
        for (concepto, importe) in m.renglones() {
            s.push_str(&format!("  {concepto:<42}{:>12}\n", pesos(importe)));
        }
        s.push_str(&format!("  {:<42}{:>12}\n  {:<42}{:>12}\n", "Patronal", pesos(m.patronal()), "Obrero", pesos(m.obrero())));
        total += m.total();
    }
    if dias_bim > 0 {
        let b = t.bimestre(sdi, dias_bim);
        s.push_str(&format!("\nRCV e INFONAVIT, {dias_bim} días (cesantía patronal {:.3} %)\n", t.cyv_patronal_para(sdi as f64 / 100.0)));
        let renglones = [
            ("Retiro", b.retiro),
            ("Cesantía y vejez, patronal", b.cyv_patronal),
            ("Cesantía y vejez, obrero", b.cyv_obrero),
            ("Aportación INFONAVIT", b.infonavit),
        ];
        for (concepto, importe) in renglones {
            s.push_str(&format!("  {concepto:<42}{:>12}\n", pesos(importe)));
        }
        total += b.total();
    }
    s.push_str(&format!("\n  {:<42}{:>12}\n", "Total", pesos(total)));
    s.push_str("\nCálculo de un solo tramo, sin ausentismos, incapacidades, actualización ni recargos.");
    Ok(s)
}

fn calcular_patron(ui: &Principal, c: &Contexto) -> Resultado<String> {
    let mal = |m: &str| sua_nucleo::ErrorSua::Validacion(m.to_string());
    let reg = patron_activo(ui, c).ok_or_else(|| mal("Registre primero un patrón."))?;
    let periodo = ui.get_c_periodo().trim().replace('-', "");
    let fecha_pago = ui.get_c_fecha_pago().trim().to_string();
    let r = c.base.calcular_periodo_con_pago(&reg, &periodo, ui.get_c_decreto(), Some(fecha_pago.as_str()).filter(|f| !f.is_empty()))?;
    let mut s = texto_del_calculo(&reg, &r);
    if let Some(g) = c.base.calculo_guardado(&reg, &r.periodo)? {
        s.push_str(&format!(
            "\n\nHay un cálculo guardado de este periodo ({} UTC), total {}: {}.",
            g.guardado_en,
            pesos(g.calculo.total()),
            if g.calculo == r { "es igual a este" } else { "es distinto de este; «Guardar cálculo» lo sustituye" }
        ));
    }
    mostrar_calculo(ui, c, &r);
    c.estado.borrow_mut().calculo = Some((reg, r));
    Ok(s)
}

/// Llena el resultado con formato de la pantalla Cálculo: total, rubros, avisos y detalle.
fn mostrar_calculo(ui: &Principal, c: &Contexto, r: &CalculoPeriodo) {
    let (m, b) = (&r.mes, &r.bimestre);
    let rcv = b.retiro + b.cyv_patronal + b.cyv_obrero;
    let (mes, bimestre) = periodo_en_texto(&r.periodo);
    let mut rubros = vec![RubroVista { nombre: "IMSS".into(), importe: pesos(m.total()).into() }];
    if r.cierra_bimestre {
        rubros.push(RubroVista { nombre: "Retiro, cesantía y vejez".into(), importe: pesos(rcv).into() });
        rubros.push(RubroVista { nombre: "INFONAVIT patronal".into(), importe: pesos(b.infonavit).into() });
        if b.amortizacion + b.seguro_vivienda > 0 {
            rubros.push(RubroVista { nombre: "Amortización y seguro".into(), importe: pesos(b.amortizacion + b.seguro_vivienda).into() });
        }
    }
    let mut resumen = format!("Periodo: {mes}");
    if r.cierra_bimestre {
        resumen.push_str(&format!(", con el bimestre {bimestre}"));
    }
    resumen.push_str(&format!(" · {} con días cotizados", if r.lineas.len() == 1 { "1 trabajador".to_string() } else { format!("{} trabajadores", r.lineas.len()) }));
    match &r.extemporaneo {
        Some(e) => {
            rubros.push(RubroVista { nombre: "Actualización y recargos".into(), importe: pesos(e.total()).into() });
            resumen.push_str(&format!(
                " · vencía el {}; pago el {}: {} {} de mora, factor {}.{:04}, recargos {}.{:02} %",
                e.vencimiento,
                e.fecha_pago,
                e.meses,
                if e.meses == 1 { "mes" } else { "meses" },
                e.factor / 10_000,
                e.factor % 10_000,
                e.tasa / 100,
                e.tasa % 100
            ));
        }
        None => {
            if let Ok(vence) = c.base.vencimiento(&r.periodo) {
                resumen.push_str(&format!(" · vence el {vence}"));
            }
        }
    }
    if r.decreto > 0 {
        resumen.push_str(&format!(" · con decreto de exención del {} %", r.decreto));
    }
    let mut rotulo = if r.extemporaneo.is_some() { "Total a pagar, con actualización y recargos" } else { "Total a pagar" }.to_string();
    if !r.avisos.is_empty() {
        rotulo.push_str(" · preliminar: revise los avisos");
    }
    ui.set_c_total_rotulo(rotulo.into());
    ui.set_c_sin_prima(r.prima_rt.is_none());
    ui.set_c_total(pesos(r.total()).into());
    ui.set_c_resumen(resumen.into());
    ui.set_c_rubros(ModelRc::new(VecModel::from(rubros)));
    ui.set_c_avisos(ModelRc::new(VecModel::from(r.avisos.iter().map(SharedString::from).collect::<Vec<_>>())));

    let cierra = r.cierra_bimestre;
    let dias = |mes: u32, bim: u32| if cierra { format!("{mes} / {bim}") } else { mes.to_string() };
    let incidencias = |inc: u32, aus: u32| if inc + aus == 0 { String::new() } else { format!("{inc} / {aus}") };
    let o_nada = |centavos: i64| if centavos == 0 { String::new() } else { pesos(centavos) };
    ui.set_c_cabecera(LineaVista {
        nombre: "Trabajador".into(),
        dias: if cierra { "Días mes / bim." } else { "Días del mes" }.into(),
        incidencias: "Incap. / aus.".into(),
        imss: "IMSS".into(),
        rcv: "RCV".into(),
        infonavit: "INFONAVIT patronal".into(),
        amortizacion: "Amort. y seguro".into(),
        total: "Total".into(),
    });
    let lineas: Vec<LineaVista> = r
        .lineas
        .iter()
        .map(|l| {
            let (inc, aus) = if cierra { (l.incapacidad_bimestre, l.ausentismos_bimestre) } else { (l.incapacidad_mes, l.ausentismos_mes) };
            LineaVista {
                nombre: l.nombre.as_str().into(),
                dias: dias(l.dias_mes, l.dias_bimestre).into(),
                incidencias: incidencias(inc, aus).into(),
                imss: pesos(l.mes.total()).into(),
                rcv: if cierra { pesos(l.bimestre.retiro + l.bimestre.cyv_patronal + l.bimestre.cyv_obrero) } else { String::new() }.into(),
                infonavit: if cierra { pesos(l.bimestre.infonavit) } else { String::new() }.into(),
                amortizacion: o_nada(l.bimestre.amortizacion + l.bimestre.seguro_vivienda).into(),
                total: pesos(l.mes.total() + l.bimestre.total()).into(),
            }
        })
        .collect();
    ui.set_c_lineas(ModelRc::new(VecModel::from(lineas)));
    ui.set_c_linea_total(LineaVista {
        nombre: "Suma de cuotas".into(),
        dias: SharedString::new(),
        incidencias: SharedString::new(),
        imss: pesos(m.total()).into(),
        rcv: if cierra { pesos(rcv) } else { String::new() }.into(),
        infonavit: if cierra { pesos(b.infonavit) } else { String::new() }.into(),
        amortizacion: o_nada(b.amortizacion + b.seguro_vivienda).into(),
        total: pesos(r.cuotas()).into(),
    });

    let ramo = |concepto: &str, patronal: Option<i64>, obrero: Option<i64>, destacado: bool| RamoVista {
        concepto: concepto.into(),
        patronal: patronal.map(pesos).unwrap_or_default().into(),
        obrero: obrero.map(pesos).unwrap_or_default().into(),
        total: pesos(patronal.unwrap_or(0) + obrero.unwrap_or(0)).into(),
        destacado,
    };
    let mut ramos = vec![
        ramo("Cuota fija", Some(m.cuota_fija), None, false),
        ramo("Excedente de tres veces la base", Some(m.excedente_patronal), Some(m.excedente_obrero), false),
        ramo("Prestaciones en dinero", Some(m.pd_patronal), Some(m.pd_obrero), false),
        ramo("Gastos médicos de pensionados", Some(m.gmp_patronal), Some(m.gmp_obrero), false),
        ramo("Riesgos de trabajo", Some(m.riesgos_trabajo), None, false),
        ramo("Invalidez y vida", Some(m.iv_patronal), Some(m.iv_obrero), false),
        ramo("Guarderías y prestaciones sociales", Some(m.guarderias), None, false),
        ramo("Suma IMSS", Some(m.patronal()), Some(m.obrero()), true),
    ];
    if cierra {
        ramos.push(ramo("Retiro", Some(b.retiro), None, false));
        ramos.push(ramo("Cesantía en edad avanzada y vejez", Some(b.cyv_patronal), Some(b.cyv_obrero), false));
        ramos.push(ramo("Suma RCV", Some(b.retiro + b.cyv_patronal), Some(b.cyv_obrero), true));
        ramos.push(ramo("Aportación patronal al INFONAVIT", Some(b.infonavit), None, false));
        if b.amortizacion + b.seguro_vivienda > 0 {
            ramos.push(ramo("Amortización de créditos", None, Some(b.amortizacion), false));
            ramos.push(ramo("Seguro de daños a la vivienda", None, Some(b.seguro_vivienda), false));
        }
        ramos.push(ramo("Suma INFONAVIT", Some(b.infonavit), Some(b.amortizacion + b.seguro_vivienda), true));
    }
    if let Some(e) = &r.extemporaneo {
        let cargo = |concepto: &str, importe: i64| RamoVista { concepto: concepto.into(), patronal: SharedString::new(), obrero: SharedString::new(), total: pesos(importe).into(), destacado: false };
        ramos.push(cargo("Actualización", e.actualizacion_imss + e.actualizacion_rcv + e.actualizacion_infonavit));
        ramos.push(cargo("Recargos", e.recargos_imss + e.recargos_rcv + e.recargos_infonavit));
    }
    ramos.push(RamoVista { concepto: "Total a pagar".into(), patronal: SharedString::new(), obrero: SharedString::new(), total: pesos(r.total()).into(), destacado: true });
    ui.set_c_ramos(ModelRc::new(VecModel::from(ramos)));
}

fn texto_del_calculo(reg: &str, r: &CalculoPeriodo) -> String {
    let mut s = format!("Patrón {reg}   Periodo {}   {} trabajadores con días cotizados\n", r.periodo, r.lineas.len());
    if r.decreto > 0 {
        s.push_str(&format!("Con decreto de exención del {} % sobre las cuotas patronales del IMSS.\n", r.decreto));
    }
    for a in &r.avisos {
        s.push_str(&format!("AVISO: {a}\n"));
    }
    s.push_str(&format!("\n{:<32}{:>5}{:>5}{:>13}{:>13}{:>13}{:>13}\n", "Trabajador", "Mes", "Bim", "IMSS", "RCV", "INFONAVIT", "Total"));
    for l in &r.lineas {
        let rcv = l.bimestre.retiro + l.bimestre.cyv_patronal + l.bimestre.cyv_obrero;
        let nombre: String = l.nombre.chars().take(30).collect();
        s.push_str(&format!(
            "{nombre:<32}{:>5}{:>5}{:>13}{:>13}{:>13}{:>13}",
            l.dias_mes,
            l.dias_bimestre,
            pesos(l.mes.total()),
            pesos(rcv),
            pesos(l.bimestre.infonavit),
            pesos(l.mes.total() + l.bimestre.total())
        ));
        // Días de incapacidad y de ausentismo: los del bimestre si se calcula, si no los del mes.
        let (inc, aus) = if r.cierra_bimestre { (l.incapacidad_bimestre, l.ausentismos_bimestre) } else { (l.incapacidad_mes, l.ausentismos_mes) };
        if inc + aus > 0 {
            s.push_str(&format!("   inc. {inc}, aus. {aus}"));
        }
        if l.bimestre.amortizacion > 0 {
            s.push_str(&format!("   amort. {}", pesos(l.bimestre.amortizacion + l.bimestre.seguro_vivienda)));
        }
        s.push('\n');
    }
    let rcv = r.bimestre.retiro + r.bimestre.cyv_patronal + r.bimestre.cyv_obrero;
    s.push_str(&format!("{:<42}{:>13}{:>13}{:>13}{:>13}\n", "Total", pesos(r.mes.total()), pesos(rcv), pesos(r.bimestre.infonavit), pesos(r.cuotas())));
    s.push_str(&format!("\nIMSS por ramo ({} patronal, {} obrero)\n", pesos(r.mes.patronal()), pesos(r.mes.obrero())));
    for (concepto, importe) in r.mes.renglones() {
        s.push_str(&format!("  {concepto:<42}{:>13}\n", pesos(importe)));
    }
    if r.cierra_bimestre {
        s.push_str("\nRCV e INFONAVIT del bimestre\n");
        let renglones = [
            ("Retiro", r.bimestre.retiro),
            ("Cesantía y vejez, patronal", r.bimestre.cyv_patronal),
            ("Cesantía y vejez, obrero", r.bimestre.cyv_obrero),
            ("Aportación INFONAVIT", r.bimestre.infonavit),
            ("Amortización de créditos INFONAVIT", r.bimestre.amortizacion),
            ("Seguro de daños a la vivienda", r.bimestre.seguro_vivienda),
        ];
        for (concepto, importe) in renglones {
            s.push_str(&format!("  {concepto:<42}{:>13}\n", pesos(importe)));
        }
    } else {
        s.push_str("\nMes impar: no cierra bimestre, solo cuotas del IMSS.\n");
    }
    match &r.extemporaneo {
        None => s.push_str("\nPago oportuno: sin actualización ni recargos.\n"),
        Some(e) => {
            s.push_str(&format!(
                "\nPago extemporáneo el {} (vencía el {}): {} {} de mora; factor de actualización {}.{:04}; recargos {}.{:02} %\n",
                e.fecha_pago,
                e.vencimiento,
                e.meses,
                if e.meses == 1 { "mes o fracción" } else { "meses o fracción" },
                e.factor / 10_000,
                e.factor % 10_000,
                e.tasa / 100,
                e.tasa % 100
            ));
            s.push_str(&format!("  {:<28}{:>14}{:>13}\n", "", "Actualización", "Recargos"));
            let mut rubros = vec![("IMSS", e.actualizacion_imss, e.recargos_imss)];
            if r.cierra_bimestre {
                rubros.push(("RCV", e.actualizacion_rcv, e.recargos_rcv));
                rubros.push(("INFONAVIT", e.actualizacion_infonavit, e.recargos_infonavit));
            }
            for (rubro, actualizacion, recargos) in rubros {
                s.push_str(&format!("  {rubro:<28}{:>14}{:>13}\n", pesos(actualizacion), pesos(recargos)));
            }
            s.push_str(&format!("  {:<42}{:>13}\n", "Total a pagar", pesos(r.total())));
            s.push_str("  Regla general del Código Fiscal (artículos 17-A y 21); no incluye multas.\n");
        }
    }
    s.push_str("\nLa columna INFONAVIT es la aportación patronal; la amortización va en el total.");
    s
}

fn guardar_calculo(c: &Contexto) -> Resultado<String> {
    let estado = c.estado.borrow();
    let (reg, r) = estado.calculo.as_ref().ok_or_else(|| sua_nucleo::ErrorSua::Validacion("Calcule primero todo el patrón.".into()))?;
    let cuando = c.base.guardar_calculo(reg, r)?;
    c.bitacora.info(&format!("Cálculo guardado: periodo {}.", r.periodo));
    Ok(format!("Cálculo del periodo {} guardado ({cuando} UTC).", r.periodo))
}

/// Escribe la cédula del cálculo en pantalla en la carpeta de reportes.
fn escribir_cedula(c: &Contexto) -> Resultado<std::path::PathBuf> {
    let estado = c.estado.borrow();
    let (reg, r) = estado.calculo.as_ref().ok_or_else(|| sua_nucleo::ErrorSua::Validacion("Calcule primero todo el patrón.".into()))?;
    let patron = estado.patrones.iter().find(|p| p.reg_pat == *reg).cloned().unwrap_or_else(|| Patron { reg_pat: reg.clone(), ..Patron::default() });
    let nota = format!(
        "OpenSUA {} · generado el {} UTC",
        env!("CARGO_PKG_VERSION"),
        sua_nucleo::bitacora::marca_utc(std::time::SystemTime::now())
    );
    let pdf = cedula_pdf(&Cedula { patron: &patron, calculo: r, nota: &nota });
    std::fs::create_dir_all(&c.entorno.reportes).map_err(sua_nucleo::ErrorSua::archivo(&c.entorno.reportes))?;
    let ruta = c.entorno.reportes.join(nombre_de_archivo(reg, &r.periodo));
    std::fs::write(&ruta, pdf).map_err(sua_nucleo::ErrorSua::archivo(&ruta))?;
    c.bitacora.info(&format!("Cédula en PDF del periodo {}.", r.periodo));
    Ok(ruta)
}

/// Abre el cálculo guardado del periodo escrito; si no lo hay, lista los guardados del patrón.
fn ver_guardado(ui: &Principal, c: &Contexto) -> Resultado<String> {
    c.estado.borrow_mut().calculo = None;
    let reg = patron_activo(ui, c).ok_or_else(|| sua_nucleo::ErrorSua::Validacion("Registre primero un patrón.".into()))?;
    let periodo = ui.get_c_periodo().trim().replace('-', "");
    let mut s = String::new();
    if !periodo.is_empty() {
        if let Some(g) = c.base.calculo_guardado(&reg, &periodo)? {
            let texto = format!("CÁLCULO GUARDADO el {} UTC. No se recalculó: es lo que se guardó ese día.\n\n{}", g.guardado_en, texto_del_calculo(&reg, &g.calculo));
            mostrar_calculo(ui, c, &g.calculo);
            c.estado.borrow_mut().calculo = Some((reg, g.calculo));
            return Ok(texto);
        }
        s.push_str(&format!("No hay un cálculo guardado del periodo {periodo}.\n\n"));
    }
    let lista = c.base.calculos_guardados(&reg)?;
    if lista.is_empty() {
        s.push_str(&format!("El patrón {reg} no tiene cálculos guardados."));
        return Ok(s);
    }
    s.push_str(&format!("Cálculos guardados del patrón {reg}. Escriba un periodo y pulse «Ver guardado» para abrirlo.\n\n"));
    s.push_str(&format!("{:<10}{:>14}{:>16}   {}\n", "Periodo", "Trabajadores", "Total", "Guardado (UTC)"));
    for x in &lista {
        s.push_str(&format!("{:<10}{:>14}{:>16}   {}\n", x.periodo, x.trabajadores, pesos(x.total), x.guardado_en));
    }
    Ok(s)
}

// ------------------------------------------------------- Prima de RT y listados

/// Determina la prima de riesgos de trabajo del año escrito y propone la
/// nueva en los campos de prima del patrón, sin guardarla.
fn determinar_prima(ui: &Principal, c: &Contexto) -> Resultado<String> {
    let mal = |m: &str| sua_nucleo::ErrorSua::Validacion(m.to_string());
    if !ui.get_patron_existente() {
        return Err(mal("Elija primero un patrón de la lista."));
    }
    let reg: String = ui.get_p_reg().into();
    let anio: u32 = ui.get_p_anio().trim().parse().ok().filter(|a| (1997..=2100).contains(a)).ok_or_else(|| mal("Año a determinar: escriba un año, como 2026."))?;
    let s = c.base.siniestralidad(&reg, anio)?;
    let Some(r) = s.determinar() else { return Err(mal("No hay días cotizados en ese año: no se puede determinar la prima.")) };
    let mut t = format!(
        "Prima de riesgos de trabajo, revisión de {anio} (rige desde marzo de {}):\n\
         \x20 Días cotizados: {}   Trabajadores promedio: {:.4}\n\
         \x20 Casos: {}   Días subsidiados: {}   Incapacidad permanente: {} %   Defunciones: {}\n\
         \x20 Factor de prima: {}   Prima anterior: {}\n\
         \x20 Prima de la fórmula: {} %\n\
         \x20 Prima nueva: {} %",
        anio + 1,
        s.dias_cotizados,
        r.trabajadores_promedio,
        s.casos,
        s.dias_subsidiados,
        s.porcentajes,
        s.defunciones,
        s.factor,
        s.prima_anterior.map_or("no registrada".to_string(), |p| format!("{} %", prima_en_texto(p))),
        prima_en_texto(r.calculada),
        prima_en_texto(r.nueva)
    );
    if let Some(limite) = r.limite {
        t.push_str(&format!(" ({limite})"));
    }
    if s.sin_clasificar > 0 {
        t.push_str(&format!("\nAviso: {} incapacidades del año no tienen datos médicos y no se contaron.", s.sin_clasificar));
    }
    t.push_str("\nSe propuso en los campos de prima; revise y pulse Guardar para registrarla. No sustituye a la declaración ante el IMSS.");
    ui.set_p_prima(prima_en_texto(r.nueva).into());
    ui.set_p_prima_desde(format!("{}-03", anio + 1).into());
    Ok(t)
}

fn nota_de_reporte() -> String {
    format!("OpenSUA {} · generado el {} UTC", env!("CARGO_PKG_VERSION"), sua_nucleo::bitacora::marca_utc(std::time::SystemTime::now()))
}

/// Escribe un PDF en la carpeta de reportes y lo abre (salvo en la autoprueba).
fn escribir_reporte(c: &Contexto, nombre: &str, pdf: Vec<u8>) -> Resultado<String> {
    std::fs::create_dir_all(&c.entorno.reportes).map_err(sua_nucleo::ErrorSua::archivo(&c.entorno.reportes))?;
    // El nombre lleva datos de la base: solo se usa su última parte, nunca una ruta.
    let nombre = std::path::Path::new(nombre).file_name().and_then(|n| n.to_str()).ok_or_else(|| sua_nucleo::ErrorSua::Validacion("Nombre de reporte inválido.".into()))?;
    let ruta = c.entorno.reportes.join(nombre);
    std::fs::write(&ruta, pdf).map_err(sua_nucleo::ErrorSua::archivo(&ruta))?;
    c.bitacora.info(&format!("Reporte en PDF: {nombre}."));
    let abierto = std::env::var_os("SUA_AUTOPRUEBA").is_some() || std::process::Command::new("explorer").arg(&ruta).spawn().is_ok();
    Ok(format!("Reporte escrito en {}{}", ruta.display(), if abierto { "" } else { " (no se pudo abrir el visor de PDF)" }))
}

fn patron_del_reporte(ui: &Principal, c: &Contexto) -> Resultado<Patron> {
    let reg = patron_activo(ui, c).ok_or_else(|| sua_nucleo::ErrorSua::Validacion("Registre primero un patrón.".into()))?;
    Ok(c.estado.borrow().patrones.iter().find(|p| p.reg_pat == reg).cloned().unwrap_or_else(|| Patron { reg_pat: reg, ..Patron::default() }))
}

fn reporte_de_trabajadores(ui: &Principal, c: &Contexto) -> Resultado<String> {
    let patron = patron_del_reporte(ui, c)?;
    let lista = c.base.trabajadores(&patron.reg_pat, "")?;
    escribir_reporte(c, &format!("trabajadores_{}.pdf", nombre_seguro(&patron.reg_pat)), trabajadores_pdf(&patron, &lista, &nota_de_reporte()))
}

/// Movimientos del mes del periodo, o del bimestre si el mes es par.
fn reporte_de_movimientos(ui: &Principal, c: &Contexto) -> Resultado<String> {
    let patron = patron_del_reporte(ui, c)?;
    let periodo = ui.get_c_periodo().trim().replace('-', "");
    let p = sua_nucleo::dominio::tramos::Periodo::de_texto(&periodo).ok_or_else(|| sua_nucleo::ErrorSua::Validacion("Periodo: use el formato AAAAMM.".into()))?;
    let (desde, hasta) = if p.cierra_bimestre() { p.bimestre() } else { p.mes_completo() };
    let iso = sua_nucleo::dominio::extemporaneo::iso_de_dia;
    let mut movimientos = Vec::new();
    for t in c.base.trabajadores(&patron.reg_pat, "")? {
        let lista = c.base.movimientos(&patron.reg_pat, &t.num_afil)?;
        movimientos.push((t, lista));
    }
    escribir_reporte(c, &format!("movimientos_{}_{}.pdf", nombre_seguro(&patron.reg_pat), nombre_seguro(&periodo)), movimientos_pdf(&patron, &iso(desde), &iso(hasta), &movimientos, &nota_de_reporte()))
}

// ------------------------------------------------------------- TXT del SUA

fn clase_elegida(ui: &Principal) -> Clase {
    usize::try_from(ui.get_txt_clase()).ok().and_then(|i| Clase::TODAS.get(i).copied()).unwrap_or(Clase::Trabajadores)
}

/// Importa un TXT de la clase elegida, con respaldo previo, y describe el resultado.
pub(crate) fn importar_txt(ui: &Principal, c: &Contexto, ruta: &std::path::Path) -> String {
    let clase = clase_elegida(ui);
    let nombre = ruta.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let resultado = std::fs::read(ruta)
        .map_err(sua_nucleo::ErrorSua::archivo(ruta))
        .and_then(|datos| c.base.respaldar("antes-de-importar-txt").map(|_| datos))
        .and_then(|datos| c.base.importar_txt(clase, &datos));
    let r = match resultado {
        Ok(r) => r,
        Err(e) => {
            avisar(ui, Canal::Txt, 3, format!("No se importó {nombre}."));
            return format!("No se importó {nombre}.\n{e}");
        }
    };
    c.bitacora.info(&format!(
        "Importación de TXT ({}): {} renglones, {} nuevos, {} actualizados, {} rechazados.",
        clase.nombre(),
        r.procesados,
        r.nuevos,
        r.actualizados,
        r.rechazados.len()
    ));
    let _ = refrescar_patrones(ui, c).and_then(|_| refrescar_trabajadores(ui, c));
    let mut s = format!(
        "Importación de {} desde {nombre} (se hizo un respaldo antes).\n\n\
         \x20 Renglones:     {}\n\
         \x20 Nuevos:        {}\n\
         \x20 Actualizados:  {}\n\
         \x20 Ya existían:   {}\n\
         \x20 Rechazados:    {}\n",
        clase.nombre(),
        r.procesados,
        r.nuevos,
        r.actualizados,
        r.ya_existian,
        r.rechazados.len()
    );
    for (titulo, lista) in [("Rechazados", &r.rechazados), ("Avisos", &r.avisos)] {
        if lista.is_empty() {
            continue;
        }
        s.push_str(&format!("\n{titulo}\n"));
        for (n, motivo) in lista.iter().take(60) {
            if *n > 0 {
                s.push_str(&format!("  Renglón {n}: {motivo}\n"));
            } else {
                s.push_str(&format!("  {motivo}\n"));
            }
        }
        if lista.len() > 60 {
            s.push_str(&format!("  … y {} más.\n", lista.len() - 60));
        }
    }
    let resumen = format!("{} nuevos, {} actualizados, {} que ya existían", r.nuevos, r.actualizados, r.ya_existian);
    if r.rechazados.is_empty() {
        avisar(ui, Canal::Txt, 1, format!("Importación terminada: {resumen}."));
    } else {
        avisar(ui, Canal::Txt, 2, format!("Importación terminada con {} renglones rechazados ({resumen}). Revise el detalle abajo.", r.rechazados.len()));
    }
    s
}

/// Exporta la clase elegida del patrón activo a `archivos\<registro patronal>\`.
fn exportar_txt(ui: &Principal, c: &Contexto) -> Resultado<String> {
    let reg = patron_activo(ui, c).ok_or_else(|| sua_nucleo::ErrorSua::Validacion("Registre primero un patrón.".into()))?;
    let clase = clase_elegida(ui);
    let (datos, renglones) = c.base.exportar_txt(clase, &reg)?;
    let carpeta = c.entorno.archivos.join(nombre_seguro(&reg));
    std::fs::create_dir_all(&carpeta).map_err(sua_nucleo::ErrorSua::archivo(&carpeta))?;
    let ruta = carpeta.join(clase.archivo());
    std::fs::write(&ruta, datos).map_err(sua_nucleo::ErrorSua::archivo(&ruta))?;
    c.bitacora.info(&format!("Exportación de TXT ({}): {renglones} renglones.", clase.nombre()));
    Ok(format!(
        "Exportación de {} del patrón {reg}: {renglones} renglones de {} caracteres.\n\nArchivo: {}\n\nSi ya existía, se sustituyó. Contiene datos personales y no va cifrado, aunque la base tenga contraseña: cuide dónde lo copia.",
        clase.nombre(),
        clase.largo(),
        ruta.display()
    ))
}

// ----------------------------------------------------------------- Sistema

fn texto_sistema(c: &Contexto) -> String {
    let e = &c.entorno;
    let linea = |r: Resultado<String>| r.unwrap_or_else(|x| format!("error: {x}"));
    let filas: i64 = c.base.catalogos().iter().filter_map(|n| c.base.filas_de_catalogo(n).ok()).sum();
    format!(
        "Versión:            {}\n\
         Modo:               {}\n\
         Carpeta de datos:   {}\n\
         Base de datos:      {}\n\
         Esquema:            versión {}\n\
         Diario:             {}\n\
         Cifrado:            {}\n\
         Integridad:         {}\n\
         Respaldos:          {}\n\
         Catálogos:          {} ({} filas)\n\
         Patrones:           {}\n\
         Bitácora:           {}\n\
         Motor gráfico:      render por software (sin GPU)",
        env!("CARGO_PKG_VERSION"),
        if e.modo == Modo::Portable { "portable" } else { "instalado" },
        e.raiz.display(),
        c.base.ruta().display(),
        linea(c.base.version_esquema().map(|v| v.to_string())),
        c.base.modo_diario,
        if c.base.cifrada() { "sí, con contraseña" } else { "no" },
        linea(c.base.integridad()),
        linea(c.base.respaldos().map(|r| r.len().to_string())),
        c.base.catalogos().len(),
        filas,
        c.estado.borrow().patrones.len(),
        c.bitacora.ruta().display(),
    )
}

/// Vuelve a listar los respaldos en la pantalla Sistema.
fn refrescar_respaldos(ui: &Principal, c: &Contexto) {
    let mut lista = c.base.respaldos().unwrap_or_default();
    lista.reverse();
    let etiquetas: Vec<SharedString> = lista.iter().map(|r| SharedString::from(etiqueta_local(r))).collect();
    ui.set_sistema_respaldos(ModelRc::new(VecModel::from(etiquetas)));
    // Sin elección previa: restaurar exige elegir a propósito.
    ui.set_sistema_respaldo(-1);
    c.estado.borrow_mut().respaldos = lista;
    refrescar_cifrado(ui, c);
}

/// Restaura el respaldo elegido en la lista y vuelve a cargar todas las pantallas.
pub(crate) fn restaurar_respaldo(ui: &Principal, c: &Contexto) -> String {
    let elegido = usize::try_from(ui.get_sistema_respaldo()).ok().and_then(|i| c.estado.borrow().respaldos.get(i).cloned());
    let Some(ruta) = elegido else {
        avisar(ui, Canal::Sistema, 3, "Elija un respaldo de la lista.");
        return "Elija un respaldo de la lista.".to_string();
    };
    let clave_del_respaldo = ui.get_sistema_clave_respaldo().to_string();
    let restaurado = c.base.restaurar_con_clave(&ruta, Some(clave_del_respaldo.as_str()).filter(|k| !k.is_empty()));
    if matches!(restaurado, Err(sua_nucleo::ErrorSua::ClaveIncorrecta)) {
        // No se tocó nada: se pide la contraseña de ese respaldo y se deja elegido.
        ui.set_sistema_pide_clave_respaldo(true);
        let m = "Ese respaldo no abre con la contraseña actual de la base: está cifrado con otra, o no es un respaldo de este programa. Escriba la contraseña que tenía la base cuando se hizo y vuelva a pulsar «Restaurar el elegido».";
        avisar(ui, Canal::Sistema, 3, m);
        return m.to_string();
    }
    ui.set_sistema_pide_clave_respaldo(false);
    ui.set_sistema_clave_respaldo(SharedString::new());
    let estado = if restaurado.is_ok() { 1 } else { 3 };
    let m = match restaurado {
        Ok(r) => {
            c.bitacora.info(&format!("Respaldo restaurado: {}.", etiqueta_local(&ruta)));
            let mut s = format!(
                "Respaldo restaurado: {}\n\
                 \x20 Contenía {} patrones y {} trabajadores.\n\
                 \x20 La base anterior quedó guardada como: {}",
                etiqueta_local(&ruta),
                r.respaldo.patrones,
                r.respaldo.trabajadores,
                etiqueta_local(&r.respaldo_previo)
            );
            if !r.migraciones.is_empty() {
                s.push_str(&format!("\n  El respaldo era del esquema {}; se actualizó al actual.", r.respaldo.esquema));
            }
            s
        }
        Err(e) => {
            c.bitacora.error(&format!("No se pudo restaurar un respaldo: {e}"));
            format!("No se restauró; la base quedó como estaba.\n{e}")
        }
    };
    // Todo lo que hay en pantalla venía de la base anterior.
    {
        let mut estado = c.estado.borrow_mut();
        estado.calculo = None;
        estado.pago = None;
        estado.filtro_trabajadores.clear();
    }
    let _ = refrescar_patrones(ui, c).and_then(|_| refrescar_trabajadores(ui, c));
    mostrar_patron(ui, c, None);
    mostrar_trabajador(ui, c, None);
    let _ = refrescar_movimientos(ui, c);
    ui.set_c_hay_calculo(false);
    ui.set_c_mensaje(SharedString::new());
    ui.set_calculo_texto("La base se restauró desde un respaldo: vuelva a calcular.".into());
    ui.set_pago_resumen("Abra un archivo de pago .SUA para revisarlo.".into());
    ui.set_pago_importable(false);
    refrescar_respaldos(ui, c);
    refrescar_usuarios(ui, c);
    mostrar_usuario(ui, c, 0);
    avisar(ui, Canal::Sistema, estado, m.as_str());
    if c.base.hay_usuarios() && c.base.sesion().is_none() {
        // El respaldo no conoce al usuario que estaba trabajando: hay que entrar con uno de los suyos.
        c.estado.borrow_mut().reinicio = Some(format!("{m}\n  Los usuarios son ahora los de ese respaldo; por eso se pidió iniciar sesión de nuevo."));
        let _ = ui.hide();
        let _ = slint::quit_event_loop();
    }
    m
}

/// Dice por qué no se puede eliminar el patrón o el trabajador cargados en sus
/// formularios (vacío si se puede) y pone al trabajador en la franja de contexto.
fn actualizar_bloqueos(ui: &Principal, c: &Contexto) {
    let reg: String = ui.get_p_reg().into();
    let del_patron = match (ui.get_patron_existente(), c.base.trabajadores_de(&reg)) {
        (true, Ok(n)) if n > 0 => format!(
            "No se puede eliminar: tiene {n} {}. Un patrón con trabajadores registrados se conserva, aunque ya estén dados de baja: su historial respalda las cuotas pagadas.",
            if n == 1 { "trabajador" } else { "trabajadores" }
        ),
        _ => String::new(),
    };
    ui.set_p_bloqueo(del_patron.into());
    let (mut del_trabajador, mut contexto) = (String::new(), String::new());
    if ui.get_trabajador_existente() {
        let nss: String = ui.get_t_nss().into();
        let nombre = format!("{} {} {}", ui.get_t_paterno(), ui.get_t_materno(), ui.get_t_nombre());
        contexto = format!("{nss} — {}", nombre.split_whitespace().collect::<Vec<_>>().join(" "));
        if let Some(n) = patron_activo(ui, c).and_then(|r| c.base.movimientos_de(&r, &nss).ok()).filter(|n| *n > 0) {
            del_trabajador = format!(
                "No se puede eliminar: tiene {n} {}. Un trabajador con historial se conserva. Si dejó la empresa, registre su baja en Movimientos: deja de cotizar y su historial queda.",
                if n == 1 { "movimiento" } else { "movimientos" }
            );
        }
    }
    ui.set_t_bloqueo(del_trabajador.into());
    ui.set_ctx_trabajador(contexto.into());
}

/// Lugar de la interfaz donde se muestra un mensaje.
#[derive(Clone, Copy)]
enum Canal {
    Patron,
    Trabajador,
    Movimiento,
    Calculo,
    Inpc,
    Sistema,
    Txt,
    Pago,
}

/// Muestra un mensaje con su estado: 1 correcto, 2 advertencia, 3 error, 4 información.
/// El estado lo decide quien llama, no el texto.
fn avisar(ui: &Principal, canal: Canal, estado: i32, texto: impl Into<SharedString>) {
    let texto = texto.into();
    match canal {
        Canal::Patron => (ui.set_estado_patron(estado), ui.set_mensaje_patron(texto)),
        Canal::Trabajador => (ui.set_estado_trabajador(estado), ui.set_mensaje_trabajador(texto)),
        Canal::Movimiento => (ui.set_estado_movimiento(estado), ui.set_mensaje_movimiento(texto)),
        Canal::Calculo => (ui.set_estado_calculo(estado), ui.set_c_mensaje(texto)),
        Canal::Inpc => (ui.set_estado_inpc(estado), ui.set_mensaje_inpc(texto)),
        Canal::Sistema => (ui.set_estado_sistema(estado), ui.set_sistema_mensaje(texto)),
        Canal::Txt => (ui.set_estado_txt(estado), ui.set_txt_mensaje(texto)),
        Canal::Pago => (ui.set_estado_pago(estado), ui.set_pago_mensaje(texto)),
    };
}

/// Resultado de una acción: si falló, error con `al_fallar` delante; si salió
/// bien, correcto, salvo que el propio texto traiga advertencias (que escribe
/// este programa con marcas fijas) o sea un informe.
fn avisar_resultado(ui: &Principal, canal: Canal, resultado: Resultado<String>, al_fallar: &str) {
    match resultado {
        Ok(texto) => {
            let estado = if texto.contains("Advertencia —") || texto.contains(", pero ") {
                2
            } else if texto.starts_with("Prima de riesgos de trabajo") {
                4
            } else {
                1
            };
            avisar(ui, canal, estado, texto);
        }
        Err(e) => avisar(ui, canal, 3, format!("{al_fallar}{e}")),
    }
}

/// Resultado de un cálculo: el texto va al bloque de resultado; un error, al mensaje con icono.
fn mostrar_resultado(ui: &Principal, resultado: Resultado<String>) {
    match resultado {
        Ok(texto) => {
            ui.set_calculo_texto(texto.into());
            ui.set_c_mensaje(SharedString::new());
        }
        Err(e) => {
            ui.set_calculo_texto(SharedString::new());
            avisar(ui, Canal::Calculo, 3, e.to_string());
        }
    }
}

/// Pregunta antes de una acción que borra datos. En la autoprueba no pregunta.
fn confirmar(titulo: &str, pregunta: &str) -> bool {
    if std::env::var_os("SUA_AUTOPRUEBA").is_some() {
        return true;
    }
    rfd::MessageDialog::new().set_level(rfd::MessageLevel::Warning).set_title(titulo).set_description(pregunta).set_buttons(rfd::MessageButtons::YesNo).show() == rfd::MessageDialogResult::Yes
}

// Códigos de las listas del formulario; la posición 0 es «sin elegir».
const TIPOS_DE_TRABAJADOR: [&str; 5] = ["", "1", "2", "3", "4"];
const JORNADAS: [&str; 8] = ["", "0", "1", "2", "3", "4", "5", "6"];
const SEXOS: [&str; 3] = ["", "M", "F"];
const TIPOS_DE_SALARIO: [&str; 4] = ["", "0", "1", "2"];

fn indice_de<S: AsRef<str>>(codigos: &[S], valor: &str) -> i32 {
    codigos.iter().position(|c| c.as_ref() == valor.trim()).unwrap_or(0) as i32
}

/// Código elegido en una lista. Si no se eligió nada y lo guardado es un
/// código que la lista no conoce (venido de una importación), se conserva.
fn codigo_de<S: AsRef<str>>(codigos: &[S], indice: i32, guardado: &str) -> String {
    match usize::try_from(indice).ok().filter(|i| *i > 0).and_then(|i| codigos.get(i)) {
        Some(c) => c.as_ref().to_string(),
        None if !guardado.is_empty() && !codigos.iter().any(|c| c.as_ref() == guardado) => guardado.to_string(),
        None => String::new(),
    }
}

/// Minutos que hay que sumar a la hora UTC para tener la hora local del equipo.
fn minutos_desde_utc() -> i64 {
    #[cfg(windows)]
    {
        // TIME_ZONE_INFORMATION: Bias (0), StandardBias (84) y DaylightBias (168), en minutos.
        #[repr(C)]
        struct Zona {
            bias: i32,
            resto: [u8; 168],
        }
        #[link(name = "kernel32")]
        extern "system" {
            fn GetTimeZoneInformation(zona: *mut Zona) -> u32;
        }
        let mut z = Zona { bias: 0, resto: [0; 168] };
        // SAFETY: la estructura tiene el tamaño de TIME_ZONE_INFORMATION (172 bytes) y la función solo la llena.
        let modo = unsafe { GetTimeZoneInformation(&mut z) };
        let entero = |desde: usize| i32::from_le_bytes([z.resto[desde], z.resto[desde + 1], z.resto[desde + 2], z.resto[desde + 3]]);
        let ajuste = match modo {
            1 => entero(80),
            2 => entero(164),
            _ => 0,
        };
        -i64::from(z.bias + ajuste)
    }
    #[cfg(unix)]
    {
        // SAFETY: localtime_r solo llena la estructura `tm` que se le pasa.
        unsafe {
            let ahora = libc::time(std::ptr::null_mut());
            let mut tm: libc::tm = std::mem::zeroed();
            if libc::localtime_r(&ahora, &mut tm).is_null() {
                0
            } else {
                tm.tm_gmtoff as i64 / 60
            }
        }
    }
    #[cfg(not(any(windows, unix)))]
    {
        0
    }
}

/// Etiqueta de un respaldo para la lista: fecha y hora locales y un motivo legible.
fn etiqueta_local(ruta: &std::path::Path) -> String {
    let nombre = ruta.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let partes: Vec<&str> = nombre.trim_end_matches(".db").splitn(4, '-').collect();
    let (f, h, motivo) = match partes.as_slice() {
        ["sua", f, h, motivo] if f.len() == 8 && h.len() == 6 && f.bytes().chain(h.bytes()).all(|b| b.is_ascii_digit()) => (*f, *h, *motivo),
        _ => return nombre,
    };
    let n = |t: &str| t.parse::<i64>().unwrap_or(0);
    let dia = sua_nucleo::dominio::tramos::dia(n(&f[..4]), n(&f[4..6]), n(&f[6..]));
    let segundos = dia * 86_400 + n(&h[..2]) * 3_600 + n(&h[2..4]) * 60 + n(&h[4..]) + minutos_desde_utc() * 60;
    let local = sua_nucleo::bitacora::marca_utc(std::time::UNIX_EPOCH + std::time::Duration::from_secs(segundos.max(0) as u64));
    // Si hubo varios con el mismo nombre en el mismo segundo, el archivo lleva un sufijo numérico.
    let (base, repetido) = match motivo.rsplit_once('-') {
        Some((b, n)) if n.len() == 1 && n.bytes().all(|x| x.is_ascii_digit()) && !b.ends_with("esquema") => (b, format!(" ({n})")),
        _ => (motivo, String::new()),
    };
    let legible = match base {
        "manual" => "Respaldo manual".to_string(),
        "antes-de-importar" => "Antes de importar un archivo de pago".to_string(),
        "antes-de-importar-txt" => "Antes de importar TXT".to_string(),
        "antes-de-restaurar" => "Antes de restaurar".to_string(),
        otro => match otro.strip_prefix("antes-de-esquema-") {
            Some(version) => format!("Antes de actualizar la base a la versión {version}"),
            None => otro.replace('-', " "),
        },
    };
    format!("{} · {legible}{repetido}", &local[..16])
}

fn error_fatal(mensaje: &str) {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("OpenSUA")
        .set_description(mensaje)
        .show();
}

/// Si la carpeta del programa no se puede escribir (USB protegida, carpeta de
/// solo lectura) no se cambia de sitio en silencio: se avisa y se pide otra
/// carpeta para los datos (especificación, §2.3).
fn preparar_entorno() -> Resultado<Entorno> {
    let mut intento = Entorno::preparar();
    loop {
        let carpeta = match intento {
            Err(sua_nucleo::ErrorSua::CarpetaNoEscribible(carpeta)) => carpeta,
            otro => return otro,
        };
        if std::env::var_os("SUA_AUTOPRUEBA").is_some() {
            return Err(sua_nucleo::ErrorSua::CarpetaNoEscribible(carpeta));
        }
        let pregunta = format!(
            "No se puede escribir en la carpeta\n{}\n\nSuele pasar cuando la memoria USB está protegida contra escritura o la carpeta es de solo lectura.\n\n¿Quiere elegir otra carpeta para guardar los datos? El programa se queda donde está; ahí solo se guardan los datos, en una carpeta llamada {}.",
            carpeta.display(),
            sua_nucleo::config::CARPETA_DE_DATOS
        );
        let respuesta = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title("OpenSUA")
            .set_description(pregunta)
            .set_buttons(rfd::MessageButtons::YesNo)
            .show();
        let elegida = if respuesta == rfd::MessageDialogResult::Yes {
            rfd::FileDialog::new().set_title("Carpeta para los datos de OpenSUA").pick_folder()
        } else {
            None
        };
        match elegida {
            Some(elegida) => intento = Entorno::con_datos_en(&elegida),
            // El usuario ya vio el aviso y decidió no seguir: se cierra sin otro mensaje.
            None => std::process::exit(1),
        }
    }
}

/// Variable de entorno con la contraseña de la base. Evita la ventana de
/// contraseña; sirve para pruebas y para accesos directos.
const VARIABLE_CLAVE: &str = "SUA_CLAVE";
/// Texto que la copia anterior del programa deja para mostrar en Sistema al
/// volver a abrir (resultado de un cambio de contraseña).
const VARIABLE_AVISO: &str = "SUA_AVISO_INICIAL";

fn clave_del_entorno() -> Option<String> {
    std::env::var(VARIABLE_CLAVE).ok().filter(|v| !v.is_empty())
}

#[derive(Clone, Copy, PartialEq)]
enum Respuesta {
    Aceptar,
    Restaurar,
    Cancelar,
}

/// Ventana de contraseña, previa a la principal. Queda a la vista entre
/// intentos; `esperar` devuelve lo que pulsó el usuario (cerrarla es cancelar).
struct DialogoClave {
    ventana: VentanaClave,
    respuesta: Rc<std::cell::Cell<Respuesta>>,
}

impl DialogoClave {
    fn nuevo(titulo: &str, explicacion: &str, nueva: bool, puede_restaurar: bool, aceptar: &str, cancelar: &str) -> Resultado<DialogoClave> {
        let ventana = VentanaClave::new().map_err(|e| sua_nucleo::ErrorSua::Ejecutable(format!("no se pudo crear la ventana de contraseña: {e}")))?;
        ventana.set_titulo(titulo.into());
        ventana.set_explicacion(explicacion.into());
        ventana.set_nueva(nueva);
        ventana.set_puede_restaurar(puede_restaurar);
        ventana.set_texto_aceptar(aceptar.into());
        ventana.set_texto_cancelar(cancelar.into());
        let respuesta = Rc::new(std::cell::Cell::new(Respuesta::Cancelar));
        for (cual, registrar) in [
            (Respuesta::Aceptar, VentanaClave::on_aceptar as fn(&VentanaClave, Box<dyn FnMut()>)),
            (Respuesta::Restaurar, VentanaClave::on_restaurar),
            (Respuesta::Cancelar, VentanaClave::on_cancelar),
        ] {
            let respuesta = respuesta.clone();
            registrar(
                &ventana,
                Box::new(move || {
                    respuesta.set(cual);
                    let _ = slint::quit_event_loop();
                }),
            );
        }
        ventana.show().map_err(|e| sua_nucleo::ErrorSua::Ejecutable(format!("no se pudo mostrar la ventana de contraseña: {e}")))?;
        Ok(DialogoClave { ventana, respuesta })
    }

    fn esperar(&self) -> Respuesta {
        self.respuesta.set(Respuesta::Cancelar);
        if slint::run_event_loop().is_err() {
            return Respuesta::Cancelar;
        }
        self.respuesta.get()
    }

    fn clave(&self) -> String {
        self.ventana.get_clave().to_string()
    }

    fn avisar(&self, estado: i32, texto: &str) {
        self.ventana.set_estado(estado);
        self.ventana.set_mensaje(texto.into());
    }
}

impl Drop for DialogoClave {
    fn drop(&mut self) {
        let _ = self.ventana.hide();
    }
}

/// Texto para Sistema después de recuperar la base de un respaldo al abrir.
fn texto_de_recuperacion(r: &sua_nucleo::datos::restaurar::Recuperacion) -> String {
    format!(
        "Al abrir, la base no se pudo leer y se restauró el respaldo: {}\n\
         \x20 Contenía {} patrones y {} trabajadores. Revise lo capturado después de esa fecha.\n\
         \x20 El archivo anterior quedó guardado como: {}",
        etiqueta_local(&r.respaldo.ruta),
        r.respaldo.patrones,
        r.respaldo.trabajadores,
        r.danada.display()
    )
}

/// En una base nueva en modo portable se propone protegerla con contraseña
/// (especificación, §11). Devuelve la contraseña elegida, o `None` si no quiso.
fn ofrecer_clave_nueva() -> Resultado<Option<String>> {
    let dialogo = DialogoClave::nuevo(
        "Base de datos nueva",
        "Puede proteger la base con una contraseña. El archivo se cifra y sin la contraseña nadie puede leerlo, tampoco si pierde la memoria USB.\n\nSi olvida la contraseña, los datos no se pueden recuperar: no hay forma de restablecerla.\n\nPuede ponerla, cambiarla o quitarla después, en la pantalla Sistema.",
        true,
        false,
        "Proteger con contraseña",
        "Seguir sin contraseña",
    )?;
    loop {
        if dialogo.esperar() != Respuesta::Aceptar {
            return Ok(None);
        }
        let clave = dialogo.clave();
        match cifrado::validar_clave_nueva(&clave, &dialogo.ventana.get_repetida()) {
            Ok(()) => return Ok(Some(clave)),
            Err(e) => dialogo.avisar(3, &e.to_string()),
        }
    }
}

/// Pide la contraseña de una base cifrada hasta que abra. Desde la misma
/// ventana se puede restaurar un respaldo: con contraseña, un archivo dañado
/// y una contraseña equivocada dan el mismo error.
fn abrir_con_ventana(entorno: &Entorno, bitacora: &Bitacora) -> Resultado<(Base, Option<String>)> {
    let dialogo = DialogoClave::nuevo(
        "La base de datos tiene contraseña",
        "Escriba la contraseña para abrirla.",
        false,
        true,
        "Abrir",
        "Salir",
    )?;
    loop {
        match dialogo.esperar() {
            // El usuario decidió no seguir: se cierra sin otro mensaje.
            Respuesta::Cancelar => std::process::exit(1),
            Respuesta::Aceptar => match Base::abrir_con_clave(entorno, Some(&dialogo.clave())) {
                Ok(base) => return Ok((base, None)),
                Err(sua_nucleo::ErrorSua::ClaveIncorrecta) => {
                    bitacora.info("Contraseña de la base rechazada.");
                    dialogo.avisar(3, "La base no abre con esa contraseña. Revise mayúsculas y acentos. Si está seguro de que es la correcta, el archivo puede estar dañado: restaure un respaldo.");
                }
                Err(e) if e.es_base_danada() => dialogo.avisar(3, &format!("{e}. El archivo está dañado: restaure un respaldo.")),
                Err(e) => return Err(e),
            },
            Respuesta::Restaurar => {
                let clave = dialogo.clave();
                let clave = Some(clave.as_str()).filter(|c| !c.is_empty());
                let Some(respaldo) = ultimo_respaldo_util(&entorno.respaldos, clave) else {
                    dialogo.avisar(3, "Ningún respaldo abre con esa contraseña. Escriba la contraseña que tenía la base cuando se hicieron los respaldos y vuelva a pulsar «Restaurar un respaldo».");
                    continue;
                };
                let pregunta = format!(
                    "El respaldo más reciente que abre con esa contraseña es:\n{}\n({} patrones, {} trabajadores)\n\n¿Quiere restaurarlo? Lo capturado después de ese respaldo se pierde. El archivo actual de la base no se borra: se guarda aparte en la carpeta de datos.",
                    etiqueta_local(&respaldo.ruta),
                    respaldo.patrones,
                    respaldo.trabajadores
                );
                if !confirmar("OpenSUA", &pregunta) {
                    continue;
                }
                let r = recuperar(&entorno.ruta_base(), &entorno.respaldos, clave).inspect_err(|e| bitacora.error(&format!("No se pudo recuperar la base: {e}")))?;
                let base = Base::abrir_con_clave(entorno, clave.filter(|_| r.respaldo.cifrado)).inspect_err(|e| bitacora.error(&format!("No se pudo abrir la base recuperada: {e}")))?;
                bitacora.info(&format!("Base recuperada del respaldo {}; el archivo anterior quedó en {}.", etiqueta_local(&r.respaldo.ruta), r.danada.display()));
                return Ok((base, Some(texto_de_recuperacion(&r))));
            }
        }
    }
}

/// Abre la base. Si es nueva y el programa es portable, propone ponerle
/// contraseña; si tiene contraseña, la pide; si el archivo está dañado, ofrece
/// sustituirlo por el respaldo más reciente que sirva (el archivo dañado se
/// aparta, no se borra). Devuelve además el texto que cuenta la recuperación,
/// si la hubo.
fn abrir_base(entorno: &Entorno, bitacora: &Bitacora) -> Resultado<(Base, Option<String>)> {
    let en_prueba = std::env::var_os("SUA_AUTOPRUEBA").is_some();
    let del_entorno = clave_del_entorno();
    if !entorno.ruta_base().exists() {
        let clave = match del_entorno {
            Some(clave) => Some(clave),
            None if entorno.modo == Modo::Portable && !en_prueba => ofrecer_clave_nueva()?,
            None => None,
        };
        if clave.is_some() {
            bitacora.info("Base nueva protegida con contraseña.");
        }
        return Base::abrir_con_clave(entorno, clave.as_deref()).map(|b| (b, None));
    }
    let error = match Base::abrir(entorno) {
        Ok(base) => return Ok((base, None)),
        Err(e) => e,
    };
    // Con la contraseña a la mano se prueba antes de preguntar. Sin la marca de
    // base cifrada, una base con contraseña parece dañada: también se prueba.
    if let Some(clave) = del_entorno.as_deref().filter(|_| matches!(error, sua_nucleo::ErrorSua::FaltaClave) || error.es_base_danada()) {
        match Base::abrir_con_clave(entorno, Some(clave)) {
            Ok(base) => return Ok((base, None)),
            Err(e) if en_prueba => return Err(e),
            Err(_) => {}
        }
    }
    if matches!(error, sua_nucleo::ErrorSua::FaltaClave) {
        return if en_prueba { Err(error) } else { abrir_con_ventana(entorno, bitacora) };
    }
    if !error.es_base_danada() {
        return Err(error);
    }
    bitacora.error(&format!("La base está dañada: {error}"));
    let Some(respaldo) = ultimo_respaldo_util(&entorno.respaldos, None) else {
        return Err(sua_nucleo::ErrorSua::Validacion(format!(
            "{error}\n\nEl archivo de la base está dañado y no hay ningún respaldo sin contraseña que se pueda restaurar en\n{}\n\nSi guardó una copia de esa carpeta en otro lugar, póngala ahí y vuelva a abrir el programa. El archivo dañado no se ha tocado.",
            entorno.respaldos.display()
        )));
    };
    let pregunta = format!(
        "El archivo de la base de datos está dañado y no se puede abrir. Suele pasar al retirar la memoria USB sin expulsarla o si el equipo se apagó a media escritura.\n\n\
         El respaldo más reciente que se puede usar es:\n{}\n({} patrones, {} trabajadores)\n\n\
         ¿Quiere restaurarlo? Lo capturado después de ese respaldo se pierde. El archivo dañado no se borra: se guarda aparte en la carpeta de datos.",
        etiqueta_local(&respaldo.ruta),
        respaldo.patrones,
        respaldo.trabajadores
    );
    if !confirmar("OpenSUA", &pregunta) {
        bitacora.info("El usuario no quiso restaurar un respaldo; la base dañada queda como estaba.");
        return Err(error);
    }
    let r = recuperar(&entorno.ruta_base(), &entorno.respaldos, None).inspect_err(|e| bitacora.error(&format!("No se pudo recuperar la base: {e}")))?;
    let base = Base::abrir(entorno).inspect_err(|e| bitacora.error(&format!("No se pudo abrir la base recuperada: {e}")))?;
    bitacora.info(&format!("Base recuperada del respaldo {}; el archivo dañado quedó en {}.", etiqueta_local(&r.respaldo.ruta), r.danada.display()));
    Ok((base, Some(texto_de_recuperacion(&r))))
}

/// Inicia sesión si la base tiene usuarios: con `SUA_USUARIO` y
/// `SUA_CLAVE_USUARIO` si están (pruebas y accesos directos), o con la ventana.
fn iniciar_sesion(base: &Base, bitacora: &Bitacora) -> Resultado<()> {
    if !base.hay_usuarios() {
        return Ok(());
    }
    let de_variable = |nombre: &str| std::env::var(nombre).ok().filter(|v| !v.is_empty());
    if let (Some(usuario), Some(clave)) = (de_variable("SUA_USUARIO"), de_variable("SUA_CLAVE_USUARIO")) {
        return base.iniciar_sesion(&usuario, &clave).map(|_| ());
    }
    if std::env::var_os("SUA_AUTOPRUEBA").is_some() {
        return Err(sua_nucleo::ErrorSua::SesionRequerida);
    }
    let dialogo = DialogoClave::nuevo("Iniciar sesión", "Escriba su usuario y su contraseña.", false, false, "Entrar", "Salir")?;
    dialogo.ventana.set_con_usuario(true);
    loop {
        // El usuario decidió no seguir: se cierra sin otro mensaje.
        if dialogo.esperar() != Respuesta::Aceptar {
            std::process::exit(1);
        }
        match base.iniciar_sesion(&dialogo.ventana.get_usuario(), &dialogo.clave()) {
            Ok(_) => return Ok(()),
            Err(sua_nucleo::ErrorSua::AccesoDenegado) => {
                bitacora.info("Inicio de sesión rechazado.");
                dialogo.ventana.set_clave(SharedString::new());
                dialogo.avisar(3, "Usuario o contraseña incorrectos. Si un administrador olvidó su contraseña, se restablece con: sua-cli restablecer-usuario <nombre>.");
            }
            Err(e) => return Err(e),
        }
    }
}

/// Hora local, a partir de la marca UTC `AAAA-MM-DD HH:MM:SS` de la bitácora.
fn hora_local(marca_utc: &str) -> String {
    let n = |desde: usize, hasta: usize| marca_utc.get(desde..hasta).and_then(|t| t.parse::<i64>().ok());
    match (n(0, 4), n(5, 7), n(8, 10), n(11, 13), n(14, 16), n(17, 19)) {
        (Some(a), Some(m), Some(d), Some(h), Some(mi), Some(s)) => {
            let segundos = sua_nucleo::dominio::tramos::dia(a, m, d) * 86_400 + h * 3_600 + mi * 60 + s + minutos_desde_utc() * 60;
            sua_nucleo::bitacora::marca_utc(std::time::UNIX_EPOCH + std::time::Duration::from_secs(segundos.max(0) as u64))
        }
        _ => marca_utc.to_string(),
    }
}

/// Pone en pantalla lo que depende del usuario: qué botones puede usar, la
/// lista de usuarios, la bitácora de accesos y el pie de la ventana.
pub(crate) fn refrescar_usuarios(ui: &Principal, c: &Contexto) {
    ui.set_puede_capturar(c.base.puede(Permiso::Capturar));
    ui.set_puede_eliminar(c.base.puede(Permiso::Eliminar));
    ui.set_puede_exportar(c.base.puede(Permiso::Exportar));
    ui.set_puede_respaldar(c.base.puede(Permiso::Respaldar));
    let administra = c.base.puede(Permiso::Administrar);
    ui.set_puede_administrar(administra);
    ui.set_sistema_hay_usuarios(c.base.hay_usuarios());
    let lista = if administra { c.base.usuarios().unwrap_or_default() } else { Vec::new() };
    let mut etiquetas = vec![SharedString::from("Usuario nuevo")];
    etiquetas.extend(lista.iter().map(|u| {
        let nombre = if u.nombre_completo.is_empty() { u.nombre.clone() } else { format!("{} — {}", u.nombre, u.nombre_completo) };
        SharedString::from(format!("{nombre} · {}{}", u.rol.nombre(), if u.activo { "" } else { " · desactivado" }))
    }));
    ui.set_sistema_usuarios(ModelRc::new(VecModel::from(etiquetas)));
    c.estado.borrow_mut().usuarios = lista;
    let bitacora = if administra && c.base.hay_usuarios() {
        c.base
            .accesos(200)
            .map(|accesos| accesos.iter().map(|a| format!("{}  {:<20}  {}{}", &hora_local(&a.momento)[..16], a.usuario, a.accion, if a.detalle.is_empty() { String::new() } else { format!(": {}", a.detalle) })).collect::<Vec<_>>().join("\n"))
            .unwrap_or_else(|e| e.to_string())
    } else {
        String::new()
    };
    ui.set_sistema_bitacora(bitacora.into());
    // El usuario va delante: si el pie no cabe, lo que se recorta es la ruta.
    let usuario = match c.base.sesion() {
        Some(s) if s.rol == Rol::Consulta => format!("{} (Consulta: solo lectura)   ·   ", s.nombre),
        Some(s) => format!("{} ({})   ·   ", s.nombre, s.rol.nombre()),
        None => String::new(),
    };
    ui.set_pie(
        format!(
            "{usuario}{} · datos en {}",
            if c.entorno.modo == Modo::Portable { "Modo portable" } else { "Modo instalado" },
            abreviar_ruta(&c.entorno.raiz.display().to_string(), 60)
        )
        .into(),
    );
}

/// Deja el formulario de usuarios en blanco (posición 0) o con los datos del elegido.
pub(crate) fn mostrar_usuario(ui: &Principal, c: &Contexto, indice: i32) {
    let elegido = usize::try_from(indice - 1).ok().and_then(|i| c.estado.borrow().usuarios.get(i).cloned());
    ui.set_sistema_usuario_indice(if elegido.is_some() { indice } else { 0 });
    ui.set_u_nombre(elegido.as_ref().map(|u| u.nombre.clone()).unwrap_or_default().into());
    ui.set_u_completo(elegido.as_ref().map(|u| u.nombre_completo.clone()).unwrap_or_default().into());
    // El primer usuario tiene que ser administrador; después se propone capturista.
    ui.set_u_rol(match elegido.as_ref().map(|u| u.rol) {
        Some(rol) => Rol::TODOS.iter().position(|r| *r == rol).unwrap_or(0) as i32,
        None if c.base.hay_usuarios() => 1,
        None => 0,
    });
    ui.set_u_activo(elegido.as_ref().is_none_or(|u| u.activo));
    ui.set_u_clave(SharedString::new());
    ui.set_u_repetida(SharedString::new());
}

/// Crea el usuario escrito en Sistema, o guarda los cambios del elegido.
pub(crate) fn guardar_usuario(ui: &Principal, c: &Contexto) -> Resultado<String> {
    let rol = usize::try_from(ui.get_u_rol()).ok().and_then(|i| Rol::TODOS.get(i).copied()).unwrap_or(Rol::Consulta);
    let (clave, repetida) = (ui.get_u_clave().to_string(), ui.get_u_repetida().to_string());
    let elegido = usize::try_from(ui.get_sistema_usuario_indice() - 1).ok().and_then(|i| c.estado.borrow().usuarios.get(i).cloned());
    let hecho = match elegido {
        None => {
            let primero = !c.base.hay_usuarios();
            let u = c.base.crear_usuario(&ui.get_u_nombre(), &ui.get_u_completo(), rol, &clave, &repetida)?;
            c.bitacora.info(&format!("Usuario creado: {} ({}).", u.nombre, u.rol.nombre()));
            if primero {
                format!("Se creó el administrador {}. Desde ahora el programa pide usuario y contraseña al abrir. Su sesión ya está iniciada.", u.nombre)
            } else {
                format!("Se creó el usuario {} ({}).", u.nombre, u.rol.nombre())
            }
        }
        Some(u) => {
            c.base.modificar_usuario(&u.nombre, &ui.get_u_completo(), rol, ui.get_u_activo())?;
            let mut hecho = format!("Se guardaron los cambios de {}.", u.nombre);
            if !clave.is_empty() || !repetida.is_empty() {
                c.base.cambiar_clave_de_usuario(&u.nombre, &clave, &repetida)?;
                hecho.push_str(" Su contraseña se cambió.");
            }
            c.bitacora.info(&format!("Usuario modificado: {}.", u.nombre));
            hecho
        }
    };
    refrescar_usuarios(ui, c);
    mostrar_usuario(ui, c, 0);
    Ok(hecho)
}

/// Estado del cifrado en la pantalla Sistema: si la base tiene contraseña y,
/// si la tiene, cuántos respaldos anteriores quedaron sin cifrar.
fn refrescar_cifrado(ui: &Principal, c: &Contexto) {
    ui.set_sistema_cifrada(c.base.cifrada());
    let sin_cifrar = if c.base.cifrada() { c.base.respaldos_sin_cifrar().map(|r| r.len()).unwrap_or(0) } else { 0 };
    ui.set_sistema_aviso_respaldos(
        match sin_cifrar {
            0 => String::new(),
            1 => "Hay 1 respaldo sin cifrar, hecho antes de poner la contraseña: quien tenga el archivo puede leerlo. Elimínelo si ya no lo necesita.".to_string(),
            n => format!("Hay {n} respaldos sin cifrar, hechos antes de poner la contraseña: quien tenga los archivos puede leerlos. Elimínelos si ya no los necesita."),
        }
        .into(),
    );
}

/// Valida lo escrito en «Contraseña de la base» y deja anotado el cambio. No lo
/// aplica: para sustituir el archivo hay que cerrar la base, así que se hace al
/// salir de la ventana (ver `main`). Devuelve el cambio (actual, nueva).
pub(crate) fn preparar_cambio_de_clave(ui: &Principal, c: &Contexto, quitar: bool) -> Resultado<(Option<String>, Option<String>)> {
    c.base.exigir(Permiso::Administrar)?;
    let validacion = |texto: &str| sua_nucleo::ErrorSua::Validacion(texto.to_string());
    let actual = if c.base.cifrada() {
        let escrita = ui.get_sistema_clave_actual().to_string();
        if !c.base.clave_es(&escrita) {
            return Err(validacion("La contraseña actual no es correcta."));
        }
        Some(escrita)
    } else {
        None
    };
    let nueva = if quitar {
        if actual.is_none() {
            return Err(validacion("La base no tiene contraseña."));
        }
        None
    } else {
        let nueva = ui.get_sistema_clave_nueva().to_string();
        cifrado::validar_clave_nueva(&nueva, &ui.get_sistema_clave_repetida())?;
        if actual.as_deref() == Some(nueva.as_str()) {
            return Err(validacion("La contraseña nueva es igual a la actual."));
        }
        Some(nueva)
    };
    Ok((actual, nueva))
}

/// Cierra la base, aplica el cambio de contraseña anotado y vuelve a abrir el
/// programa. Se llama cuando la ventana principal ya se cerró.
fn aplicar_cambio_de_clave(c: Rc<Contexto>, actual: Option<String>, nueva: Option<String>) {
    let Ok(Contexto { entorno, base, bitacora, .. }) = Rc::try_unwrap(c) else {
        return error_fatal("No se pudo cerrar la base para cambiar la contraseña. No se cambió nada.");
    };
    drop(base);
    let aviso = match cifrado::cambiar_clave(&entorno.ruta_base(), actual.as_deref(), nueva.as_deref()) {
        Ok(()) => {
            let hecho = match (&actual, &nueva) {
                (None, Some(_)) => "La base quedó protegida con contraseña. A partir de ahora se pide al abrir el programa.",
                (Some(_), Some(_)) => "La contraseña de la base se cambió. Los respaldos hechos antes siguen con la contraseña anterior.",
                _ => "Se quitó la contraseña: la base ya no está cifrada. Los respaldos hechos antes siguen cifrados con la contraseña que tenían.",
            };
            bitacora.info(hecho);
            hecho.to_string()
        }
        Err(e) => {
            bitacora.error(&format!("No se pudo cambiar la contraseña: {e}"));
            format!("No se pudo cambiar la contraseña; la base quedó como estaba.\n{e}")
        }
    };
    // Se suelta el candado de instancia única antes de abrir la copia nueva.
    drop(entorno);
    relanzar(&aviso);
}

/// Abre otra copia del programa, que mostrará `aviso` en Sistema. La copia
/// vuelve a pedir contraseña y usuario: no hereda los de las variables de prueba.
fn relanzar(aviso: &str) {
    let relanzado = std::env::current_exe().and_then(|exe| {
        std::process::Command::new(exe).env(VARIABLE_AVISO, aviso).env_remove(VARIABLE_CLAVE).env_remove("SUA_USUARIO").env_remove("SUA_CLAVE_USUARIO").env_remove("SUA_ESCENA").spawn()
    });
    if relanzado.is_err() {
        error_fatal(&format!("{aviso}\n\nNo se pudo volver a abrir el programa: ábralo usted."));
    }
}

/// Texto de advertencia cuando la carpeta de datos está dentro de una carpeta sincronizada.
fn aviso_de_sincronizacion(servicio: &str) -> String {
    format!(
        "La carpeta de datos está dentro de {servicio}. Un servicio de sincronización puede copiar la base mientras el programa la usa y dejarla dañada o duplicada, sobre todo si se abre en dos equipos. \
         Conviene cerrar el programa y mover su carpeta a un lugar que no se sincronice (por ejemplo C:\\OpenSUA-Portable o una memoria USB). Para tener copias en la nube, copie ahí la carpeta data\\respaldos."
    )
}

/// Ruta para el pie de la ventana: si es muy larga se le quita el centro,
/// porque lo que orienta es la unidad y el final. Completa está en Sistema.
fn abreviar_ruta(ruta: &str, maximo: usize) -> String {
    let letras: Vec<char> = ruta.chars().collect();
    if letras.len() <= maximo || maximo < 8 {
        return ruta.to_string();
    }
    let inicio = maximo / 3;
    let fin = maximo - inicio - 1;
    format!("{}…{}", letras[..inicio].iter().collect::<String>(), letras[letras.len() - fin..].iter().collect::<String>())
}

fn preparar() -> Resultado<Contexto> {
    let entorno = preparar_entorno()?;
    let bitacora = Bitacora::en(&entorno.logs);
    let (base, recuperacion) = abrir_base(&entorno, &bitacora).inspect_err(|e| bitacora.error(&format!("No se pudo abrir la base: {e}")))?;
    let recuperacion = recuperacion.or_else(|| std::env::var(VARIABLE_AVISO).ok().filter(|v| !v.is_empty()));
    iniciar_sesion(&base, &bitacora).inspect_err(|e| bitacora.error(&format!("No se pudo iniciar sesión: {e}")))?;
    if let Some(r) = &base.respaldo_previo {
        bitacora.info(&format!("Respaldo previo a la migración: {}", r.display()));
    }
    if !base.migraciones_aplicadas.is_empty() {
        bitacora.info(&format!("Migraciones aplicadas: {:?}", base.migraciones_aplicadas));
    }
    Ok(Contexto { entorno, base, bitacora, estado: RefCell::new(Estado { aviso_de_arranque: recuperacion, ..Estado::default() }) })
}

fn main() {
    // Render por software: sin GPU ni controlador gráfico.
    if std::env::var_os("SLINT_BACKEND").is_none() {
        std::env::set_var("SLINT_BACKEND", "winit-software");
    }
    let c = match preparar() {
        Ok(c) => Rc::new(c),
        Err(e) => return error_fatal(&e.to_string()),
    };
    let ui = match Principal::new() {
        Ok(ui) => ui,
        Err(e) => {
            c.bitacora.error(&format!("No se pudo crear la ventana: {e}"));
            return error_fatal(&format!("No se pudo crear la ventana: {e}"));
        }
    };

    // Estado inicial.
    let nombres: Vec<SharedString> = c.base.catalogos().into_iter().map(SharedString::from).collect();
    let pos = nombres.iter().position(|n| n == "Municipios").unwrap_or(0);
    ui.set_catalogos(ModelRc::new(VecModel::from(nombres)));
    ui.set_indice_catalogo(pos as i32);
    // Entidades federativas, del catálogo Estados: «09 CIUDAD DE MEXICO».
    {
        let estados = c.base.leer_catalogo("Estados").map(|t| t.filas).unwrap_or_default();
        let mut claves = vec![String::new()];
        let mut nombres = vec![SharedString::from("Sin elegir")];
        for fila in estados.iter().filter(|f| f.len() >= 3) {
            claves.push(fila[0].trim().to_string());
            nombres.push(SharedString::from(format!("{} {}", fila[0].trim(), fila[2].trim())));
        }
        ui.set_lista_entidades(ModelRc::new(VecModel::from(nombres)));
        c.estado.borrow_mut().entidades = claves;
    }
    let inicial = (|| -> Resultado<()> {
        cargar_catalogo(&ui, &c, "Municipios")?;
        refrescar_patrones(&ui, &c)?;
        refrescar_trabajadores(&ui, &c)
    })();
    if let Err(e) = inicial {
        c.bitacora.error(&format!("Carga inicial: {e}"));
        return error_fatal(&e.to_string());
    }
    mostrar_patron(&ui, &c, None);
    mostrar_trabajador(&ui, &c, None);
    let tipos: Vec<SharedString> = tipos_de_movimiento().map(|(t, d)| SharedString::from(format!("{t} {d}"))).collect();
    let familia = |desde: usize, hasta: usize| ModelRc::new(VecModel::from(tipos[desde..hasta].to_vec()));
    ui.set_tipos_empleo(familia(0, 4));
    ui.set_tipos_incidencia(familia(4, 6));
    ui.set_tipos_credito(familia(6, 12));
    ui.set_tipos_movimiento(ModelRc::new(VecModel::from(tipos)));
    let clases: Vec<SharedString> = Clase::TODAS.iter().map(|k| SharedString::from(format!("{} ({}, {} caracteres)", k.nombre(), k.archivo(), k.largo()))).collect();
    ui.set_txt_clases(ModelRc::new(VecModel::from(clases)));
    ui.set_txt_clase(1);
    ui.set_sistema_texto(texto_sistema(&c).into());
    refrescar_respaldos(&ui, &c);
    if let Some(texto) = c.estado.borrow_mut().aviso_de_arranque.take() {
        let estado = if texto.starts_with("Al abrir") || texto.starts_with("No se pudo") { 2 } else { 1 };
        avisar(&ui, Canal::Sistema, estado, texto);
        // Es lo primero que debe ver quien acaba de recuperar la base.
        ui.set_seccion(4);
    }
    if let Some(servicio) = servicio_de_sincronizacion(&c.entorno.raiz) {
        let texto = aviso_de_sincronizacion(servicio);
        ui.set_sistema_aviso_carpeta(texto.as_str().into());
        // El aviso queda fijo en Sistema; en ventana aparte se da una sola vez por carpeta de datos.
        let visto = c.entorno.config.join("aviso-sincronizacion.txt");
        if !visto.exists() {
            c.bitacora.info(&format!("La carpeta de datos está dentro de {servicio}."));
            if std::env::var_os("SUA_AUTOPRUEBA").is_none() {
                rfd::MessageDialog::new().set_level(rfd::MessageLevel::Warning).set_title("OpenSUA").set_description(format!("{texto}\n\nEste aviso no se repite; queda en la pantalla Sistema.")).show();
                let _ = std::fs::write(&visto, "Ya se mostró el aviso de carpeta sincronizada. Si se borra este archivo, el aviso vuelve a salir.\r\n");
            }
        }
    }
    refrescar_usuarios(&ui, &c);
    mostrar_usuario(&ui, &c, 0);

    // Se propone el mes anterior al actual, que es el que normalmente toca calcular.
    {
        let hoy = sua_nucleo::bitacora::marca_utc(std::time::SystemTime::now());
        let (anio, mes): (i64, i64) = (hoy[..4].parse().unwrap_or(2026), hoy[5..7].parse().unwrap_or(1));
        let (a, m) = sua_nucleo::dominio::extemporaneo::mes_anterior(anio, mes);
        ui.set_c_periodo(format!("{a:04}{m:02}").into());
    }

    // Para pruebas y capturas: abrir directamente en una sección (0 a 4).
    if let Some(n) = std::env::var("SUA_SECCION").ok().and_then(|v| v.parse::<i32>().ok()) {
        ui.set_seccion(n.clamp(0, 8));
    }

    // Conecta un callback: recibe la ventana y el contexto ya resueltos.
    macro_rules! conectar {
        ($registrar:ident, |$ui:ident, $c:ident $(, $arg:ident)*| $cuerpo:block) => {{
            let (debil, ctx) = (ui.as_weak(), c.clone());
            ui.$registrar(move |$($arg),*| {
                let Some($ui) = debil.upgrade() else { return };
                let $c = &*ctx;
                $cuerpo
            });
        }};
    }

    ui.on_periodo_con_decreto(|texto| {
        let t = texto.trim();
        t.len() == 6 && (t.starts_with("2008") || t.starts_with("2009"))
    });
    ui.set_acerca_version(format!("OpenSUA, versión {}.", env!("CARGO_PKG_VERSION")).into());
    ui.on_fecha_valida(|texto| sua_nucleo::datos::captura::es_fecha_iso(texto.trim()));
    ui.on_periodo_valido(|texto| sua_nucleo::dominio::tramos::Periodo::de_texto(texto.trim()).is_some());
    ui.on_nss_valido(|texto| sua_nucleo::dominio::validar::nss(texto.trim()).formato_valido && {
        let t = texto.trim();
        t.len() == 11 && sua_nucleo::dominio::validar::luhn(&t[..10]).is_some_and(|d| t.ends_with(d))
    });
    conectar!(on_patron_elegido, |ui, c, fila| {
        let p = usize::try_from(fila).ok().and_then(|i| c.estado.borrow().patrones.get(i).cloned());
        if p.is_some() {
            mostrar_patron(&ui, c, p.as_ref());
            ui.set_mensaje_patron(SharedString::new());
            // La fila elegida pasa a ser el patrón activo de toda la aplicación.
            if ui.get_indice_patron() != fila {
                ui.set_indice_patron(fila);
                if let Err(e) = refrescar_trabajadores(&ui, c) {
                    avisar(&ui, Canal::Trabajador, 3, e.to_string());
                }
                mostrar_trabajador(&ui, c, None);
            }
        }
    });
    conectar!(on_patron_nuevo, |ui, c| {
        mostrar_patron(&ui, c, None);
        ui.set_mensaje_patron(SharedString::new());
    });
    conectar!(on_patron_guardar, |ui, c| {
        avisar_resultado(&ui, Canal::Patron, guardar_patron(&ui, c), "No se guardó.\n");
    });
    conectar!(on_patron_determinar_prima, |ui, c| {
        avisar_resultado(&ui, Canal::Patron, determinar_prima(&ui, c), "No se determinó la prima.\n");
    });
    conectar!(on_trabajadores_pdf, |ui, c| {
        avisar_resultado(&ui, Canal::Trabajador, reporte_de_trabajadores(&ui, c), "No se escribió el reporte. Si el PDF está abierto en otro programa, ciérrelo.\n");
    });
    conectar!(on_movimientos_pdf, |ui, c| {
        avisar_resultado(&ui, Canal::Movimiento, reporte_de_movimientos(&ui, c), "No se escribió el reporte. ");
    });
    conectar!(on_patron_eliminar, |ui, c| {
        let reg: String = ui.get_p_reg().into();
        let guardados = c.base.calculos_guardados(&reg).map(|l| l.len()).unwrap_or(0);
        let mut pregunta = format!("Se eliminará el patrón:\n\n{reg} — {}\n", ui.get_p_nombre());
        if guardados > 0 {
            pregunta.push_str(&format!("\nTambién se eliminarán sus {guardados} cálculos guardados.\n"));
        }
        pregunta.push_str("\nNo se puede deshacer. Si quiere poder recuperarlo, haga antes un respaldo en Sistema. ¿Continuar?");
        // Si no se puede eliminar no se pregunta: se explica por qué.
        if ui.get_p_bloqueo().is_empty() && !confirmar("Eliminar patrón", &pregunta) {
            return;
        }
        let r = c.base.eliminar_patron(&reg).and_then(|_| refrescar_patrones(&ui, c)).and_then(|_| refrescar_trabajadores(&ui, c));
        match r {
            Ok(()) => {
                c.bitacora.info("Patrón eliminado.");
                mostrar_patron(&ui, c, None);
                avisar(&ui, Canal::Patron, 1, "Patrón eliminado.");
            }
            Err(e) => avisar(&ui, Canal::Patron, 3, format!("No se eliminó.\n{e}")),
        }
    });

    conectar!(on_patron_activo_cambiado, |ui, c, _indice| {
        if let Err(e) = refrescar_trabajadores(&ui, c) {
            avisar(&ui, Canal::Trabajador, 3, e.to_string());
        }
        mostrar_trabajador(&ui, c, None);
    });
    conectar!(on_trabajador_buscar, |ui, c, texto| {
        c.estado.borrow_mut().filtro_trabajadores = texto.into();
        if let Err(e) = refrescar_trabajadores(&ui, c) {
            avisar(&ui, Canal::Trabajador, 3, e.to_string());
        }
    });
    conectar!(on_trabajador_elegido, |ui, c, fila| {
        let t = usize::try_from(fila).ok().and_then(|i| c.estado.borrow().trabajadores.get(i).cloned());
        if t.is_some() {
            mostrar_trabajador(&ui, c, t.as_ref());
            ui.set_mensaje_trabajador(SharedString::new());
        }
    });
    conectar!(on_trabajador_nuevo, |ui, c| {
        mostrar_trabajador(&ui, c, None);
        ui.set_mensaje_trabajador(SharedString::new());
    });
    conectar!(on_trabajador_guardar, |ui, c| {
        avisar_resultado(&ui, Canal::Trabajador, guardar_trabajador(&ui, c), "No se guardó.\n");
    });
    conectar!(on_trabajador_eliminar, |ui, c| {
        let nss: String = ui.get_t_nss().into();
        let nombre = format!("{} {} {}", ui.get_t_paterno(), ui.get_t_materno(), ui.get_t_nombre());
        let pregunta = format!(
            "Se eliminará al trabajador:\n\n{nss} — {}\n\nCon él se eliminan sus datos afiliatorios. No se puede deshacer; si quiere poder recuperarlo, haga antes un respaldo en Sistema. ¿Continuar?",
            nombre.split_whitespace().collect::<Vec<_>>().join(" ")
        );
        if ui.get_t_bloqueo().is_empty() && !confirmar("Eliminar trabajador", &pregunta) {
            return;
        }
        let r = match patron_activo(&ui, c) {
            Some(reg) => c.base.eliminar_trabajador(&reg, &nss).and_then(|_| refrescar_trabajadores(&ui, c)).and_then(|_| refrescar_patrones(&ui, c)),
            None => Ok(()),
        };
        match r {
            Ok(()) => {
                c.bitacora.info("Trabajador eliminado.");
                mostrar_trabajador(&ui, c, None);
                avisar(&ui, Canal::Trabajador, 1, "Trabajador eliminado.");
            }
            Err(e) => avisar(&ui, Canal::Trabajador, 3, format!("No se eliminó.\n{e}")),
        }
    });

    conectar!(on_catalogo_elegido, |ui, c, nombre| {
        if let Err(e) = cargar_catalogo(&ui, c, nombre.as_str()) {
            ui.set_pie(e.to_string().into());
        }
    });
    conectar!(on_inpc_guardar, |ui, c| {
        let r = c.base.guardar_inpc(&ui.get_i_mes(), &ui.get_i_inpc(), &ui.get_i_tasa()).and_then(|_| cargar_catalogo(&ui, c, "INPC"));
        match r {
            Ok(()) => {
                c.bitacora.info("INPC y tasa de recargos registrados.");
                let mes = ui.get_i_mes();
                ui.set_i_mes(SharedString::new());
                ui.set_i_inpc(SharedString::new());
                ui.set_i_tasa(SharedString::new());
                avisar(&ui, Canal::Inpc, 1, format!("Mes {mes} guardado."));
            }
            Err(e) => avisar(&ui, Canal::Inpc, 3, format!("No se guardó. {e}")),
        }
    });
    conectar!(on_catalogo_buscar, |ui, c, texto| {
        mostrar_catalogo(&ui, &c.estado.borrow().catalogo, texto.as_str());
    });

    conectar!(on_pago_abrir, |ui, c| {
        let elegido = rfd::FileDialog::new()
            .set_title("Abrir archivo de pago")
            .add_filter("Archivo de pago (*.SUA)", &["sua", "SUA"])
            .add_filter("Todos los archivos", &["*"])
            .pick_file();
        if let Some(ruta) = elegido {
            abrir_pago(&ui, c, &ruta);
        }
    });
    conectar!(on_pago_importar, |ui, c| {
        let Some(a) = c.estado.borrow().pago.clone() else { return };
        // Respaldo antes de una carga masiva.
        let r = c.base.respaldar("antes-de-importar").and_then(|_| c.base.importar_pago(&a));
        let texto = match r {
            Ok(r) => {
                c.bitacora.info(&format!(
                    "Importación de archivo de pago: {} trabajadores nuevos, {} actualizados, {} movimientos nuevos.",
                    r.trabajadores_nuevos, r.trabajadores_actualizados, r.movimientos_nuevos
                ));
                format!(
                    "\n\nImportación terminada (se hizo un respaldo antes).\n\
                     \x20 Patrón:        {}\n\
                     \x20 Trabajadores:  {} nuevos, {} actualizados\n\
                     \x20 Movimientos:   {} nuevos, {} ya existían\n\
                     \x20 Con datos que no pasan la validación (NSS, RFC, CURP o nombre): {}",
                    if r.patron_nuevo { "nuevo" } else { "actualizado" },
                    r.trabajadores_nuevos,
                    r.trabajadores_actualizados,
                    r.movimientos_nuevos,
                    r.movimientos_ya_existentes,
                    r.con_advertencias
                )
            }
            Err(e) => format!("\n\nNo se importó nada.\n{e}"),
        };
        if texto.contains("Importación terminada") {
            avisar(&ui, Canal::Pago, 1, "Importación terminada; antes se hizo un respaldo. El detalle está al final del texto.");
        } else {
            avisar(&ui, Canal::Pago, 3, "No se importó nada. El motivo está al final del texto.");
        }
        ui.set_pago_resumen(format!("{}{texto}", ui.get_pago_resumen()).into());
        ui.set_pago_importable(false);
        let _ = refrescar_patrones(&ui, c).and_then(|_| refrescar_trabajadores(&ui, c));
    });

    conectar!(on_calcular, |ui, c| {
        // Un tramo suelto no se guarda ni lleva cédula.
        c.estado.borrow_mut().calculo = None;
        ui.set_c_hay_calculo(false);
        mostrar_resultado(&ui, calcular(&ui, c));
    });
    conectar!(on_calcular_patron, |ui, c| {
        c.estado.borrow_mut().calculo = None;
        let resultado = calcular_patron(&ui, c);
        ui.set_c_hay_calculo(c.estado.borrow().calculo.is_some());
        mostrar_resultado(&ui, resultado);
    });
    conectar!(on_calculo_guardar, |ui, c| {
        // Guardar sustituye al cálculo que hubiera del mismo periodo: se pregunta si es distinto.
        let distinto = c.estado.borrow().calculo.as_ref().and_then(|(reg, r)| {
            let guardado = c.base.calculo_guardado(reg, &r.periodo).ok().flatten()?;
            (guardado.calculo != *r).then(|| (r.periodo.clone(), guardado.guardado_en, guardado.calculo.total()))
        });
        if let Some((periodo, cuando, total)) = distinto {
            let pregunta = format!("Ya hay un cálculo guardado del periodo {periodo} (del {cuando} UTC, total {}), distinto de este.\n\nGuardar lo sustituye. ¿Continuar?", pesos(total));
            if !confirmar("Sustituir el cálculo guardado", &pregunta) {
                return;
            }
        }
        avisar_resultado(&ui, Canal::Calculo, guardar_calculo(c), "No se guardó. ");
    });
    conectar!(on_calculo_pdf, |ui, c| {
        match escribir_cedula(c) {
            Ok(ruta) => {
                // En la autoprueba no se abre el visor.
                let abierto = std::env::var_os("SUA_AUTOPRUEBA").is_some() || std::process::Command::new("explorer").arg(&ruta).spawn().is_ok();
                let texto = format!("Cédula escrita en {}{}", ruta.display(), if abierto { "" } else { ", pero no se pudo abrir el visor de PDF" });
                avisar(&ui, Canal::Calculo, if abierto { 1 } else { 2 }, texto);
            }
            Err(e) => avisar(&ui, Canal::Calculo, 3, format!("No se escribió la cédula. Si el PDF está abierto en otro programa, ciérrelo y repita.\n{e}")),
        }
    });
    conectar!(on_calculo_ver_guardado, |ui, c| {
        let resultado = ver_guardado(&ui, c);
        ui.set_c_hay_calculo(c.estado.borrow().calculo.is_some());
        mostrar_resultado(&ui, resultado);
    });
    conectar!(on_movimientos_abiertos, |ui, c| {
        ui.set_mensaje_movimiento(SharedString::new());
        if let Err(e) = refrescar_movimientos(&ui, c) {
            avisar(&ui, Canal::Movimiento, 3, e.to_string());
        }
    });
    conectar!(on_movimiento_registrar, |ui, c| {
        avisar_resultado(&ui, Canal::Movimiento, registrar_movimiento(&ui, c), "No se registró.\n");
    });
    conectar!(on_movimiento_eliminar, |ui, c| {
        let id = usize::try_from(ui.get_m_fila()).ok().and_then(|i| c.estado.borrow().movimientos.get(i).map(|m| m.id));
        let Some(id) = id else { return };
        // Se dice qué movimiento es antes de borrarlo.
        let descripcion = c.estado.borrow().movimientos.iter().find(|m| m.id == id).map(|m| {
            let detalle = detalle_de_movimiento(m);
            format!("{} del {}{}", sua_nucleo::reportes::listados::descripcion_de_movimiento(&m.tipo), m.fecha, if detalle.is_empty() { String::new() } else { format!(" ({detalle})") })
        });
        if !confirmar("Eliminar movimiento", &format!("Se eliminará este movimiento:\n\n{}\n\nEl cálculo de los periodos que toca cambiará. ¿Continuar?", descripcion.unwrap_or_default())) {
            return;
        }
        let r = c.base.eliminar_movimiento(id).and_then(|_| refrescar_movimientos(&ui, c));
        match r {
            Ok(()) => avisar(&ui, Canal::Movimiento, 1, "Movimiento eliminado."),
            Err(e) => avisar(&ui, Canal::Movimiento, 3, format!("No se eliminó.\n{e}")),
        }
    });
    conectar!(on_calculo_abierto, |ui, c| {
        let nombre = usize::try_from(ui.get_indice_patron())
            .ok()
            .and_then(|i| c.estado.borrow().patrones.get(i).map(|p| format!("{} — {}", p.reg_pat, p.nom_pat)));
        ui.set_c_patron(nombre.unwrap_or_else(|| "sin patrón".to_string()).into());
        // Propone la prima del patrón activo si el campo está vacío.
        if ui.get_c_prima().is_empty() {
            if let Some((_, v)) = patron_activo(&ui, c).and_then(|reg| c.base.prima_rt_vigente(&reg).ok().flatten()) {
                ui.set_c_prima(cienmilesimas_a_prima(v).into());
            }
        }
    });

    conectar!(on_txt_importar, |ui, c| {
        let elegido = rfd::FileDialog::new()
            .set_title("Importar TXT del SUA")
            .add_filter("Texto (*.txt)", &["txt", "TXT", "Txt"])
            .add_filter("Todos los archivos", &["*"])
            .pick_file();
        if let Some(ruta) = elegido {
            let texto = importar_txt(&ui, c, &ruta);
            ui.set_txt_texto(texto.into());
        }
    });
    conectar!(on_txt_abrir_carpeta, |ui, c| {
        let carpeta = patron_activo(&ui, c).map(|reg| c.entorno.archivos.join(nombre_seguro(&reg))).filter(|d| d.is_dir()).unwrap_or_else(|| c.entorno.archivos.clone());
        let texto = match std::fs::create_dir_all(&carpeta).map_err(|e| e.to_string()).and_then(|_| std::process::Command::new("explorer").arg(&carpeta).spawn().map_err(|e| e.to_string())) {
            Ok(_) => format!("Carpeta de exportación: {}", carpeta.display()),
            Err(e) => format!("No se pudo abrir {}.\n{e}", carpeta.display()),
        };
        ui.set_txt_texto(texto.into());
    });
    conectar!(on_txt_exportar, |ui, c| {
        match exportar_txt(&ui, c) {
            Ok(texto) => {
                avisar(&ui, Canal::Txt, 1, "Exportación terminada. El archivo contiene datos personales y no va cifrado.");
                ui.set_txt_texto(texto.into());
            }
            Err(e) => {
                avisar(&ui, Canal::Txt, 3, format!("No se exportó. {e}"));
                ui.set_txt_texto(SharedString::new());
            }
        }
    });
    conectar!(on_txt_abierto, |ui, c| {
        let nombre = usize::try_from(ui.get_indice_patron())
            .ok()
            .and_then(|i| c.estado.borrow().patrones.get(i).map(|p| format!("{} — {}", p.reg_pat, p.nom_pat)));
        ui.set_c_patron(nombre.unwrap_or_else(|| "sin patrón".to_string()).into());
    });

    conectar!(on_sistema_actualizar, |ui, c| {
        ui.set_sistema_texto(texto_sistema(c).into());
        refrescar_respaldos(&ui, c);
        refrescar_usuarios(&ui, c);
    });
    conectar!(on_sistema_restaurar, |ui, c| {
        let etiqueta = usize::try_from(ui.get_sistema_respaldo()).ok().and_then(|i| c.estado.borrow().respaldos.get(i).map(|r| etiqueta_local(r)));
        let Some(etiqueta) = etiqueta else {
            avisar(&ui, Canal::Sistema, 3, "Elija un respaldo de la lista.");
            return;
        };
        let confirma = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title("Restaurar respaldo")
            .set_description(format!(
                "Se va a sustituir toda la información actual por la del respaldo:\n\n{etiqueta}\n\nAntes se guardará una copia de la base actual. ¿Continuar?"
            ))
            .set_buttons(rfd::MessageButtons::YesNo)
            .show();
        if confirma == rfd::MessageDialogResult::Yes {
            restaurar_respaldo(&ui, c);
            ui.set_sistema_texto(texto_sistema(c).into());
        }
    });
    conectar!(on_sistema_clave_aplicar, |ui, c, quitar| {
        let cambio = match preparar_cambio_de_clave(&ui, c, quitar) {
            Ok(cambio) => cambio,
            Err(e) => return ui.set_sistema_mensaje_clave(e.to_string().into()),
        };
        ui.set_sistema_mensaje_clave(SharedString::new());
        let pregunta = match &cambio {
            (None, Some(_)) => "Se va a cifrar la base con la contraseña que escribió. A partir de ahora se pedirá al abrir el programa.\n\nSi la olvida, los datos no se pueden recuperar: no hay forma de restablecerla.",
            (Some(_), Some(_)) => "Se va a cambiar la contraseña de la base.\n\nSi olvida la nueva, los datos no se pueden recuperar. Los respaldos hechos hasta hoy siguen con la contraseña anterior.",
            _ => "Se va a quitar la contraseña: la base quedará sin cifrar y quien tenga el archivo podrá leer los datos.",
        };
        if !confirmar("Contraseña de la base", &format!("{pregunta}\n\nPara aplicarlo el programa se cierra y vuelve a abrir. ¿Continuar?")) {
            return;
        }
        c.base.anotar("Pidió cambiar la contraseña de la base", "");
        c.estado.borrow_mut().cambio_de_clave = Some(cambio);
        let _ = ui.hide();
        let _ = slint::quit_event_loop();
    });
    conectar!(on_sistema_usuario_elegido, |ui, c, indice| {
        mostrar_usuario(&ui, c, indice);
        ui.set_sistema_mensaje_usuarios(SharedString::new());
    });
    conectar!(on_sistema_usuario_guardar, |ui, c| {
        if !c.base.hay_usuarios() && !confirmar("Usuarios", "Se va a crear el primer administrador. Desde ese momento el programa pedirá usuario y contraseña cada vez que se abra.\n\nSi olvida la contraseña del administrador, se restablece desde la línea de comandos (sua-cli restablecer-usuario). ¿Continuar?") {
            return;
        }
        match guardar_usuario(&ui, c) {
            Ok(hecho) => {
                ui.set_estado_usuarios(1);
                ui.set_sistema_mensaje_usuarios(hecho.into());
            }
            Err(e) => {
                ui.set_estado_usuarios(3);
                ui.set_sistema_mensaje_usuarios(e.to_string().into());
            }
        }
    });
    conectar!(on_sistema_mi_clave, |ui, c| {
        match c.base.cambiar_mi_clave(&ui.get_mi_clave_actual(), &ui.get_mi_clave_nueva(), &ui.get_mi_clave_repetida()) {
            Ok(()) => {
                ui.set_mi_clave_actual(SharedString::new());
                ui.set_mi_clave_nueva(SharedString::new());
                ui.set_mi_clave_repetida(SharedString::new());
                ui.set_estado_mi_clave(1);
                ui.set_sistema_mensaje_mi_clave("Su contraseña se cambió.".into());
            }
            Err(e) => {
                ui.set_estado_mi_clave(3);
                ui.set_sistema_mensaje_mi_clave(e.to_string().into());
            }
        }
        refrescar_usuarios(&ui, c);
    });
    conectar!(on_sistema_borrar_sin_cifrar, |ui, c| {
        if let Err(e) = c.base.exigir(Permiso::Administrar) {
            return avisar(&ui, Canal::Sistema, 3, e.to_string());
        }
        let lista = c.base.respaldos_sin_cifrar().unwrap_or_default();
        if lista.is_empty() || !confirmar("Respaldos sin cifrar", &format!("Se van a eliminar {} respaldos sin cifrar. Los respaldos cifrados se conservan. ¿Continuar?", lista.len())) {
            return;
        }
        let borrados = lista.iter().filter(|r| std::fs::remove_file(r).is_ok()).count();
        c.bitacora.info(&format!("Respaldos sin cifrar eliminados: {borrados} de {}.", lista.len()));
        if borrados == lista.len() {
            c.base.anotar("Eliminó respaldos sin cifrar", &borrados.to_string());
            avisar(&ui, Canal::Sistema, 1, format!("Se eliminaron {borrados} respaldos sin cifrar."));
        } else {
            avisar(&ui, Canal::Sistema, 3, format!("Se eliminaron {borrados} de {} respaldos sin cifrar; el resto no se pudo borrar.", lista.len()));
        }
        ui.set_sistema_texto(texto_sistema(c).into());
        refrescar_respaldos(&ui, c);
    });
    conectar!(on_sistema_respaldar, |ui, c| {
        match c.base.respaldar("manual") {
            Ok(r) => {
                c.bitacora.info("Respaldo manual.");
                avisar(&ui, Canal::Sistema, 1, format!("Respaldo creado: {}", etiqueta_local(&r)));
            }
            Err(e) => avisar(&ui, Canal::Sistema, 3, format!("No se pudo respaldar: {e}")),
        }
        ui.set_sistema_texto(texto_sistema(c).into());
        refrescar_respaldos(&ui, c);
    });
    conectar!(on_sistema_compactar, |ui, c| {
        match c.base.compactar() {
            Ok(()) => avisar(&ui, Canal::Sistema, 1, "Base compactada."),
            Err(e) => avisar(&ui, Canal::Sistema, 3, format!("No se pudo compactar: {e}")),
        }
        ui.set_sistema_texto(texto_sistema(c).into());
    });

    // Para capturas: `SUA_VENTANA=900x560` fija el tamaño de la ventana.
    if let Some((ancho, alto)) = std::env::var("SUA_VENTANA").ok().and_then(|v| v.split_once('x').and_then(|(a, b)| Some((a.parse::<f32>().ok()?, b.parse::<f32>().ok()?)))) {
        ui.window().set_size(slint::LogicalSize::new(ancho, alto));
    }
    // Para capturas: deja una pantalla en un estado con datos.
    // `SUA_ESCENA=movimientos:<tipo>` elige al primer trabajador y un tipo de movimiento (0 a 11);
    // `SUA_ESCENA=calculo:<AAAAMM>` calcula todo el patrón en ese periodo.
    let mut escena_de_cierre = false;
    if let Ok(escena) = std::env::var("SUA_ESCENA") {
        match escena.split_once(':') {
            Some(("movimientos", tipo)) => {
                ui.invoke_trabajador_elegido(0);
                ui.set_seccion(6);
                ui.invoke_movimientos_abiertos();
                ui.set_m_tipo(tipo.parse().unwrap_or(0));
            }
            Some(("calculo", periodo)) => {
                ui.set_seccion(5);
                ui.invoke_calculo_abierto();
                ui.set_c_periodo(periodo.into());
                ui.invoke_calcular_patron();
            }
            // `SUA_ESCENA=ayuda:fin` abre Ayuda desplazada al final («Acerca de»).
            Some(("ayuda", "fin")) => {
                ui.set_seccion(8);
                ui.set_ayuda_al_final(true);
            }
            // `SUA_ESCENA=sistema:fin` abre Sistema desplazada al final (usuarios y bitácora de accesos).
            Some(("sistema", "fin")) => {
                ui.set_seccion(4);
                ui.set_sistema_al_final(true);
            }
            // `SUA_ESCENA=clave:<nueva>` (solo pruebas) hace lo que el botón de contraseña de Sistema,
            // sin preguntar: pone o cambia la contraseña, o la quita si <nueva> va vacía. La actual
            // se toma de `SUA_CLAVE`. El programa se cierra, aplica el cambio y vuelve a abrir.
            Some(("clave", nueva)) => {
                ui.set_sistema_clave_actual(clave_del_entorno().unwrap_or_default().into());
                ui.set_sistema_clave_nueva(nueva.into());
                ui.set_sistema_clave_repetida(nueva.into());
                match preparar_cambio_de_clave(&ui, &c, nueva.is_empty()) {
                    Ok(cambio) => {
                        c.estado.borrow_mut().cambio_de_clave = Some(cambio);
                        escena_de_cierre = true;
                    }
                    Err(e) => ui.set_sistema_mensaje_clave(e.to_string().into()),
                }
                ui.set_seccion(4);
            }
            _ => {}
        }
    }

    if std::env::var_os("SUA_AUTOPRUEBA").is_some() {
        let codigo = autoprueba::ejecutar(&ui, &c);
        c.base.cerrar_sesion();
        std::process::exit(codigo);
    }

    if escena_de_cierre {
        // Nada que mostrar: se pasa directo a aplicar el cambio.
    } else if let Err(e) = ui.run() {
        c.bitacora.error(&format!("La interfaz terminó con error: {e}"));
        error_fatal(&e.to_string());
    }
    c.base.cerrar_sesion();
    // Un cambio de contraseña sustituye el archivo de la base: se hace con todo cerrado.
    let cambio = c.estado.borrow_mut().cambio_de_clave.take();
    let reinicio = c.estado.borrow_mut().reinicio.take();
    if let Some((actual, nueva)) = cambio {
        drop(ui);
        aplicar_cambio_de_clave(c, actual, nueva);
    } else if let Some(aviso) = reinicio {
        drop(ui);
        // Se suelta la base y el candado de instancia única antes de abrir la copia nueva.
        drop(c);
        relanzar(&aviso);
    }
}
