//! Autoprueba de la interfaz: acciona los mismos callbacks que los botones y
//! comprueba el resultado en pantalla y en la base. Se activa con la variable
//! `SUA_AUTOPRUEBA`; no abre el bucle de eventos. Úsese sobre una carpeta de
//! datos vacía: crea y borra un patrón y un trabajador ficticios.
//!
//! Si `SUA_AUTOPRUEBA_SUA` apunta a un archivo de pago, prueba además su
//! lectura e importación. El resultado queda en `logs/autoprueba.log`; no se
//! escriben datos personales.

use crate::{abrir_pago, guardar_usuario, importar_txt, mostrar_usuario, patron_activo, preparar_cambio_de_clave, refrescar_usuarios, restaurar_respaldo, Contexto, Principal};
use slint::Model;
use sua_nucleo::dominio::validar::luhn;

const REG: &str = "Z9900000208";

pub fn ejecutar(ui: &Principal, c: &Contexto) -> i32 {
    let mut pasos: Vec<(String, bool)> = Vec::new();
    let mut paso = |nombre: &str, ok: bool| pasos.push((nombre.to_string(), ok));
    let nss = {
        let base10 = "9000007919";
        format!("{base10}{}", luhn(base10).unwrap())
    };

    if !c.base.patrones().map(|p| p.is_empty()).unwrap_or(false) {
        paso("la carpeta de datos está vacía (requisito de la autoprueba)", false);
        return terminar(c, &pasos);
    }

    // --- Patrones
    ui.invoke_patron_nuevo();
    ui.set_p_reg("corto".into());
    ui.set_p_rfc("XAXX010101000".into());
    ui.set_p_nombre("patrón de autoprueba".into());
    ui.invoke_patron_guardar();
    paso("patrón con registro inválido se rechaza", ui.get_mensaje_patron().starts_with("No se guardó") && ui.get_filas_patrones().row_count() == 0);

    ui.set_p_reg(REG.to_lowercase().into());
    ui.set_p_cp("06000".into());
    ui.set_p_prima("siete".into());
    ui.set_p_prima_desde("2026-03".into());
    ui.invoke_patron_guardar();
    paso("prima de RT no numérica se rechaza sin guardar el patrón", ui.get_mensaje_patron().contains("Prima de RT") && ui.get_filas_patrones().row_count() == 0);

    ui.set_p_prima("2.5".into());
    ui.invoke_patron_guardar();
    paso("alta de patrón", ui.get_mensaje_patron().starts_with("Patrón guardado") && ui.get_filas_patrones().row_count() == 1);
    paso("el formulario pasa a modo modificar y normaliza a mayúsculas", ui.get_patron_existente() && ui.get_p_reg() == REG && ui.get_p_nombre() == "PATRÓN DE AUTOPRUEBA");
    paso("prima de RT guardada", c.base.prima_rt_vigente(REG).ok().flatten() == Some(("2026-03".into(), 250_000)) && ui.get_p_prima() == "2.50000");

    ui.invoke_patron_nuevo();
    paso("«Nuevo» limpia el formulario", !ui.get_patron_existente() && ui.get_p_reg().is_empty() && ui.get_p_prima().is_empty());
    ui.invoke_patron_elegido(0);
    paso("elegir una fila carga el patrón", ui.get_patron_existente() && ui.get_p_reg() == REG && ui.get_p_cp() == "06000");
    ui.set_p_domicilio("Calle ficticia 1".into());
    ui.invoke_patron_guardar();
    paso("cambio de patrón", c.base.patrones().map(|p| p.len() == 1 && p[0].dom_pat == "CALLE FICTICIA 1").unwrap_or(false));

    // --- Trabajadores
    paso("el patrón nuevo queda activo en Trabajadores", patron_activo(ui, c).as_deref() == Some(REG) && ui.get_lista_patrones().row_count() == 1);
    ui.invoke_trabajador_nuevo();
    ui.set_t_nss(format!("{}{}", &nss[..10], if nss.ends_with('0') { '1' } else { '0' }).into());
    ui.set_t_paterno("autoprueba".into());
    ui.set_t_materno("".into());
    ui.set_t_nombre("uno".into());
    ui.set_t_sdi("315.04".into());
    ui.invoke_trabajador_guardar();
    paso("NSS con dígito verificador incorrecto se rechaza", ui.get_mensaje_trabajador().contains("NSS") && ui.get_filas_trabajadores().row_count() == 0);

    ui.set_t_nss(nss.as_str().into());
    ui.set_t_sdi("trescientos".into());
    ui.invoke_trabajador_guardar();
    paso("salario no numérico se rechaza", ui.get_mensaje_trabajador().contains("Salario") && ui.get_filas_trabajadores().row_count() == 0);

    ui.set_t_sdi("315.04".into());
    ui.set_t_alta("2026-01-15".into());
    ui.set_a_cp("600".into());
    ui.invoke_trabajador_guardar();
    paso(
        "datos afiliatorios inválidos no impiden guardar al trabajador, pero se avisa",
        ui.get_mensaje_trabajador().contains("no sus datos afiliatorios") && ui.get_mensaje_trabajador().contains("Código postal") && ui.get_estado_trabajador() == 2 && ui.get_filas_trabajadores().row_count() == 1,
    );
    ui.set_a_cp("".into());
    ui.set_a_sexo(2); // F
    ui.set_t_tipo(1); // permanente
    ui.set_a_entidad(9); // novena de la lista
    ui.set_a_nacimiento("1990-05-17".into());
    ui.set_a_ocupacion("albañil".into());
    ui.invoke_trabajador_guardar();
    paso("alta de trabajador", ui.get_mensaje_trabajador().starts_with("Trabajador guardado") && ui.get_filas_trabajadores().row_count() == 1);
    paso("la lista de patrones refleja el conteo", c.base.trabajadores_de(REG).ok() == Some(1));

    ui.invoke_trabajador_nuevo();
    ui.invoke_trabajador_elegido(0);
    paso("elegir una fila carga también sus datos afiliatorios", ui.get_a_sexo() == 2 && ui.get_t_tipo() == 1 && ui.get_a_entidad() == 9 && c.base.afiliacion(REG, &nss).ok().flatten().is_some_and(|a| a.sexo == "F" && a.ent_trab == "09") && c.base.trabajadores(REG, "").is_ok_and(|t| t[0].tip_tra == "1") && ui.get_a_nacimiento() == "1990-05-17" && ui.get_a_ocupacion() == "ALBAÑIL");
    ui.invoke_trabajadores_pdf();
    let relacion = std::fs::read(c.entorno.reportes.join(format!("trabajadores_{REG}.pdf"))).unwrap_or_default();
    paso(
        "relación de trabajadores en PDF",
        ui.get_mensaje_trabajador().starts_with("Reporte escrito en") && relacion.starts_with(b"%PDF-1.4") && relacion.windows(16).any(|v| v == b"(AUTOPRUEBA UNO)") && relacion.windows(14).any(|v| v == b"(Renglones: 1)"),
    );
    paso(
        "elegir una fila carga el trabajador",
        ui.get_trabajador_existente() && ui.get_t_nss() == nss.as_str() && ui.get_t_paterno() == "AUTOPRUEBA" && ui.get_t_nombre() == "UNO" && ui.get_t_sdi() == "315.04",
    );
    ui.set_t_sdi("400".into());
    ui.invoke_trabajador_guardar();
    paso("cambio de salario", c.base.trabajadores(REG, "").map(|t| t.len() == 1 && t[0].sal_imss == 40_000).unwrap_or(false));

    ui.invoke_trabajador_buscar("no existe".into());
    let sin = ui.get_filas_trabajadores().row_count();
    ui.invoke_trabajador_buscar("autoprueba".into());
    let con = ui.get_filas_trabajadores().row_count();
    ui.invoke_trabajador_buscar("".into());
    paso("búsqueda de trabajadores", sin == 0 && con == 1 && ui.get_filas_trabajadores().row_count() == 1);

    // --- Movimientos y cálculo de todo el patrón
    ui.invoke_trabajador_nuevo();
    ui.invoke_movimientos_abiertos();
    paso("sin trabajador elegido no se pueden registrar movimientos", !ui.get_m_habilitado() && ui.get_filas_movimientos().row_count() == 0);
    ui.invoke_trabajador_elegido(0);
    ui.invoke_movimientos_abiertos();
    paso("la pantalla de movimientos muestra al trabajador elegido", ui.get_m_habilitado() && ui.get_m_trabajador().contains("AUTOPRUEBA UNO"));
    ui.set_m_tipo(1); // 02 baja
    ui.set_m_fecha("2026-02-10".into());
    ui.invoke_movimiento_registrar();
    paso("baja de quien no está activo se rechaza", ui.get_mensaje_movimiento().contains("no está activo") && ui.get_filas_movimientos().row_count() == 0);
    ui.set_m_tipo(0); // 01 alta
    ui.set_m_fecha("2026-01-01".into());
    ui.set_m_sdi("315.04".into());
    ui.invoke_movimiento_registrar();
    ui.set_m_tipo(1);
    ui.set_m_fecha("2026-02-10".into());
    ui.set_m_sdi("".into());
    ui.invoke_movimiento_registrar();
    paso("alta y baja registradas", ui.get_mensaje_movimiento() == "Movimiento registrado." && ui.get_filas_movimientos().row_count() == 2);
    paso("el salario del trabajador sigue al movimiento", ui.get_t_sdi() == "315.04" && c.base.trabajadores(REG, "").map(|t| t[0].sal_imss == 31_504).unwrap_or(false));

    ui.invoke_calculo_abierto();
    ui.invoke_calculo_guardar();
    paso("sin un cálculo en pantalla no hay nada que guardar", !ui.get_c_hay_calculo() && ui.get_c_mensaje().contains("Calcule primero"));
    ui.set_c_periodo("202602".into());
    ui.set_c_decreto(false);
    ui.invoke_calcular_patron();
    let texto = ui.get_calculo_texto();
    // 10 días de febrero y 41 del bimestre; cuota fija = 117.31 × 10 × 20.40 % = 239.31; retiro = 315.04 × 41 × 2 % = 258.33.
    paso(
        "cálculo de todo el patrón por tramos",
        ui.get_c_patron().contains(REG) && texto.contains("1 trabajadores") && texto.contains("   10   41") && texto.contains("239.31") && texto.contains("258.33"),
    );

    // --- Guardar el cálculo y cédula en PDF
    ui.invoke_calculo_guardar();
    let guardados = c.base.calculos_guardados(REG).unwrap_or_default();
    paso(
        "guardar el cálculo",
        ui.get_c_hay_calculo() && ui.get_c_mensaje().contains("202602 guardado") && guardados.len() == 1 && guardados[0].trabajadores == 1 && guardados[0].total > 0,
    );
    ui.invoke_calcular_patron();
    paso("al recalcular se avisa que el guardado es igual", ui.get_calculo_texto().contains("es igual a este") && ui.get_c_mensaje().is_empty());
    ui.invoke_calculo_pdf();
    let cedula = std::fs::read(c.entorno.reportes.join(format!("cedula_{REG}_202602.pdf"))).unwrap_or_default();
    let contiene = |texto: &[u8]| cedula.windows(texto.len()).any(|v| v == texto);
    paso(
        "cédula en PDF con los importes del cálculo",
        ui.get_c_mensaje().starts_with("Cédula escrita en")
            && cedula.starts_with(b"%PDF-1.4")
            && cedula.ends_with(b"%%EOF\n")
            && contiene(REG.as_bytes())
            && contiene(b"(239.31) Tj")
            && contiene(b"(258.33) Tj")
            && contiene(b"AUTOPRUEBA UNO"),
    );
    ui.set_c_periodo("202601".into());
    ui.invoke_calcular_patron();
    paso("un mes impar no calcula bimestre", ui.get_calculo_texto().contains("no cierra bimestre") && !ui.get_calculo_texto().contains("cálculo guardado"));
    ui.invoke_calculo_ver_guardado();
    paso(
        "sin guardado del periodo se listan los que hay",
        !ui.get_c_hay_calculo() && ui.get_calculo_texto().starts_with("No hay un cálculo guardado del periodo 202601") && ui.get_calculo_texto().contains("202602"),
    );
    ui.set_c_periodo("202602".into());
    ui.invoke_calculo_ver_guardado();
    paso(
        "abrir un cálculo guardado",
        ui.get_c_hay_calculo() && ui.get_calculo_texto().starts_with("CÁLCULO GUARDADO") && ui.get_calculo_texto().contains("239.31"),
    );
    ui.set_c_sdi("100".into());
    ui.set_c_dias("10".into());
    ui.set_c_prima("2.5".into());
    ui.invoke_calcular();
    paso("un tramo suelto no se puede guardar", !ui.get_c_hay_calculo());

    // --- Pago extemporáneo. Enero de 2026 vencía el martes 17 de febrero.
    ui.set_c_periodo("202601".into());
    ui.set_c_fecha_pago("2026-02-17".into());
    ui.invoke_calcular_patron();
    paso("pagar el día del vencimiento es oportuno", ui.get_calculo_texto().contains("Pago oportuno") && !ui.get_calculo_texto().contains("Total a pagar"));
    ui.set_c_fecha_pago("20/02/2026".into());
    ui.invoke_calcular_patron();
    paso("fecha de pago mal escrita se rechaza", ui.get_c_mensaje().starts_with("Fecha de pago") && ui.get_estado_calculo() == 3 && ui.get_calculo_texto().is_empty() && !ui.get_c_hay_calculo());
    ui.set_c_fecha_pago("2026-02-20".into());
    ui.invoke_calcular_patron();
    let texto = ui.get_calculo_texto();
    let esperado = c.base.calcular_periodo_con_pago(REG, "202601", false, Some("2026-02-20")).ok();
    let recargos = esperado.as_ref().and_then(|r| r.extemporaneo.as_ref()).map_or(0, |e| e.recargos_imss);
    // Un mes de mora (febrero), sin actualización; febrero no tiene tasa en el catálogo: se usa la última, 2.07 %, y se avisa.
    paso(
        "pago extemporáneo con recargos",
        texto.contains("vencía el 2026-02-17") && texto.contains("1 mes o fracción") && texto.contains("factor de actualización 1.0000; recargos 2.07 %")
            && texto.contains("tasa de recargos")
            && recargos > 0
            && esperado.is_some_and(|r| texto.contains(&format!("Total a pagar{:>42}", crate::pesos(r.total()))) && r.total() == r.cuotas() + recargos),
    );
    ui.invoke_calculo_pdf();
    let cedula = std::fs::read(c.entorno.reportes.join(format!("cedula_{REG}_202601.pdf"))).unwrap_or_default();
    paso("la cédula de un pago extemporáneo lleva el total a pagar", cedula.windows(18).any(|v| v == b"(Total a pagar) Tj") && cedula.windows(21).any(|v| v == b"pago del 2026-02-20) "));
    ui.set_c_fecha_pago("".into());
    ui.set_c_periodo("202602".into());

    // --- Ausentismos e incapacidades (alta el 1 de enero, baja el 10 de febrero)
    ui.invoke_trabajador_elegido(0);
    ui.invoke_movimientos_abiertos();
    ui.set_m_tipo(4); // 11 ausentismo
    ui.set_m_fecha("2026-01-12".into());
    ui.set_m_dias("8".into());
    ui.invoke_movimiento_registrar();
    paso("ausentismo de más de 7 días se rechaza", ui.get_mensaje_movimiento().contains("entre 1 y 7") && ui.get_filas_movimientos().row_count() == 2);
    ui.set_m_dias("2".into());
    ui.invoke_movimiento_registrar();
    paso("ausentismo registrado", ui.get_mensaje_movimiento() == "Ausentismo registrado." && ui.get_filas_movimientos().row_count() == 3 && ui.get_m_dias().is_empty());
    ui.set_m_tipo(5); // 12 incapacidad
    ui.set_m_fecha("2026-01-20".into());
    ui.set_m_dias("3".into());
    ui.set_m_folio("123".into());
    ui.invoke_movimiento_registrar();
    paso("incapacidad con folio inválido se rechaza", ui.get_mensaje_movimiento().contains("Folio") && ui.get_filas_movimientos().row_count() == 3);
    ui.set_m_folio("ab123456".into());
    ui.set_m_ramo(1); // riesgos de trabajo
    ui.set_m_porcentaje("mucho".into());
    ui.invoke_movimiento_registrar();
    paso("datos médicos inválidos se rechazan sin registrar la incapacidad", ui.get_mensaje_movimiento().contains("Porcentaje") && ui.get_filas_movimientos().row_count() == 3);
    ui.set_m_porcentaje("".into());
    ui.set_m_riesgo(0); // accidente de trabajo
    ui.set_m_secuela(1);
    ui.set_m_control(1);
    ui.invoke_movimiento_registrar();
    let medicos = c.base.incapacidades(REG).ok().and_then(|l| l.first().and_then(|i| i.datos));
    paso("la incapacidad guarda sus datos médicos", medicos.map(|d| (d.ramo, d.tipo_riesgo, d.secuela, d.control)) == Some((1, 1, 1, 1)) && ui.get_m_ramo() == 0);
    let incapacidad = c.base.movimientos(REG, &nss).unwrap_or_default().into_iter().find(|m| m.tipo == "12");
    paso(
        "incapacidad registrada",
        ui.get_mensaje_movimiento() == "Incapacidad registrada." && incapacidad.map(|m| (m.dias, m.folio)) == Some((3, "AB123456".to_string())),
    );
    ui.set_m_tipo(4);
    ui.set_m_fecha("2026-01-21".into());
    ui.set_m_dias("1".into());
    ui.invoke_movimiento_registrar();
    paso("un ausentismo en días de incapacidad se rechaza", ui.get_mensaje_movimiento().contains("Ya hay") && ui.get_filas_movimientos().row_count() == 4);
    ui.set_c_periodo("202602".into());
    ui.invoke_calcular_patron();
    let texto = ui.get_calculo_texto();
    // 41 días del bimestre menos 2 de ausentismo: retiro = 315.04 × 39 × 2 % = 245.73; INFONAVIT = 315.04 × 39 × 5 % = 614.33.
    // Las incidencias son de enero: la cuota fija de febrero no cambia.
    paso(
        "el cálculo descuenta ausentismos e incapacidades",
        texto.contains("inc. 3, aus. 2") && texto.contains("245.73") && texto.contains("614.33") && texto.contains("239.31") && texto.contains("es distinto de este"),
    );

    // --- Crédito INFONAVIT
    ui.invoke_movimientos_abiertos();
    ui.set_m_tipo(6); // 15 inicio de crédito
    ui.set_m_fecha("2026-01-01".into());
    ui.set_m_credito("123".into());
    ui.set_m_tipo_descuento(1); // cuota fija mensual
    ui.set_m_valor("1500".into());
    ui.invoke_movimiento_registrar();
    paso("crédito con número inválido se rechaza", ui.get_mensaje_movimiento().contains("10 dígitos") && ui.get_filas_movimientos().row_count() == 4);
    ui.set_m_credito("0912345678".into());
    ui.set_m_valor("mucho".into());
    ui.invoke_movimiento_registrar();
    paso("valor de descuento inválido se rechaza", ui.get_mensaje_movimiento().contains("Valor del descuento") && ui.get_filas_movimientos().row_count() == 4);
    ui.set_m_valor("1500".into());
    ui.invoke_movimiento_registrar();
    paso(
        "inicio de crédito registrado",
        ui.get_mensaje_movimiento() == "Movimiento de crédito registrado." && ui.get_filas_movimientos().row_count() == 5 && ui.get_t_credito() == "0912345678" && ui.get_m_valor().is_empty(),
    );
    ui.set_m_tipo(8); // 17 reinicio
    ui.set_m_fecha("2026-01-15".into());
    ui.invoke_movimiento_registrar();
    paso("reinicio sin suspensión se rechaza", ui.get_mensaje_movimiento().contains("suspendido") && ui.get_filas_movimientos().row_count() == 5);
    ui.invoke_calcular_patron();
    let texto = ui.get_calculo_texto();
    // 41 días trabajados menos 2 de ausentismo y 3 de incapacidad: 1,500 × 2 / 59 × 36 = 1,830.51; más 13.00 de seguro.
    let lineas: Vec<crate::LineaVista> = ui.get_c_lineas().iter().collect();
    let rubros: Vec<String> = ui.get_c_rubros().iter().map(|r| format!("{} {}", r.nombre, r.importe)).collect();
    paso(
        "el resultado con formato muestra total, rubros y detalle por trabajador",
        ui.get_c_total_rotulo() == "Total a pagar · preliminar: revise los avisos" && ui.get_c_sin_prima()
            && !ui.get_c_total().is_empty()
            && texto.contains(ui.get_c_total().as_str())
            && ui.get_c_resumen().contains("febrero de 2026") && ui.get_c_resumen().contains("vence el 2026-03-17")
            && rubros.len() == 4 && rubros[2].starts_with("INFONAVIT patronal") && rubros[3].starts_with("Amortización y seguro 1,843.51")
            && lineas.len() == 1 && lineas[0].nombre == "AUTOPRUEBA UNO" && lineas[0].dias == "10 / 41" && lineas[0].incidencias == "3 / 2" && lineas[0].amortizacion == "1,843.51"
            && ui.get_c_ramos().iter().any(|r| r.concepto == "Amortización de créditos" && r.obrero == "1,830.51")
            && ui.get_c_linea_total().total == lineas[0].total,
    );
    paso(
        "el cálculo incluye la amortización del crédito",
        texto.contains("amort. 1,843.51") && texto.contains("1,830.51") && texto.contains("Seguro de daños"),
    );
    ui.invoke_calculo_pdf();
    let cedula = std::fs::read(c.entorno.reportes.join(format!("cedula_{REG}_202602.pdf"))).unwrap_or_default();
    let contiene = |texto: &[u8]| cedula.windows(texto.len()).any(|v| v == texto);
    paso("la cédula lleva crédito, amortización e incidencias", contiene(b"(0912345678) Tj") && contiene(b"(1,830.51) Tj") && contiene(b"(Inc.) Tj"));

    // --- Prima de riesgos de trabajo y movimientos en PDF
    ui.invoke_patron_elegido(0);
    ui.set_p_anio("dos mil".into());
    ui.invoke_patron_determinar_prima();
    paso("año inválido para la prima se rechaza", ui.get_mensaje_patron().contains("Año a determinar"));
    ui.set_p_anio("2026".into());
    ui.invoke_patron_determinar_prima();
    let texto = ui.get_mensaje_patron();
    // Activo del 1 de enero al 10 de febrero (41 días) menos 3 de incapacidad y 2 de ausentismo: 36 días cotizados.
    // Un accidente de trabajo de 3 días con un trabajador promedio de 0.0986 dispara la fórmula; la prima anterior
    // es 2.5 %, así que la nueva solo puede subir a 3.5 %.
    paso(
        "determinación de la prima de riesgos de trabajo",
        texto.contains("Días cotizados: 36") && texto.contains("Casos: 1   Días subsidiados: 3") && ui.get_p_prima_desde() == "2027-03" && !ui.get_p_prima().is_empty(),
    );
    ui.invoke_patron_elegido(0);
    ui.set_c_periodo("202602".into());
    ui.invoke_movimientos_pdf();
    let movs = std::fs::read(c.entorno.reportes.join(format!("movimientos_{REG}_202602.pdf"))).unwrap_or_default();
    paso(
        "movimientos del periodo en PDF",
        ui.get_mensaje_movimiento().starts_with("Reporte escrito en") && ui.get_estado_movimiento() == 1 && movs.windows(17).any(|v| v == b"(12 Incapacidad) ") && movs.windows(14).any(|v| v == b"(Renglones: 5)"),
    );

    // --- Exportar e importar TXT del SUA (un trabajador con baja, ausentismo, incapacidad y crédito)
    let carpeta_txt = c.entorno.archivos.join(REG);
    let mut tamanos = Vec::new();
    const TXT: [&str; 6] = ["Patron.txt", "Aseg.txt", "Movt.txt", "Cred.txt", "Afil.txt", "Incap.txt"];
    for clase in 0..6 {
        ui.set_txt_clase(clase);
        ui.invoke_txt_exportar();
        let nombre = TXT[clase as usize];
        tamanos.push(std::fs::metadata(carpeta_txt.join(nombre)).map(|m| m.len()).unwrap_or(0));
    }
    // Renglones de 313, 168, 49 y 52 caracteres más CRLF: 1 patrón, 1 trabajador, 3 movimientos, 1 de crédito.
    // Además, un renglón de datos afiliatorios (80) y uno de incapacidad con datos médicos (57).
    paso("exportar los seis TXT", tamanos == [315, 170, 3 * 51, 54, 82, 59] && ui.get_txt_texto().starts_with("Exportación de Incapacidades"));
    ui.set_txt_clase(2);
    ui.set_txt_texto("".into());
    let texto = importar_txt(ui, c, &carpeta_txt.join("Aseg.txt"));
    paso("un TXT de otra clase se rechaza renglón por renglón", texto.contains("Rechazados:    1") && texto.contains("caracteres") && texto.contains("Nuevos:        0") && ui.get_estado_txt() == 2 && ui.get_txt_mensaje().contains("1 renglones rechazados"));
    let mut textos = Vec::new();
    for (clase, nombre) in TXT.iter().enumerate() {
        ui.set_txt_clase(clase as i32);
        textos.push(importar_txt(ui, c, &carpeta_txt.join(nombre)));
    }
    paso(
        "reimportar los TXT actualiza sin duplicar",
        textos[0].contains("Actualizados:  1")
            && textos[1].contains("Actualizados:  1")
            && textos[2].contains("Ya existían:   3")
            && textos[3].contains("Actualizados:  1")
            && textos[4].contains("Actualizados:  1")
            && textos[5].contains("Actualizados:  1")
            && textos.iter().all(|t| t.contains("Rechazados:    0") && t.contains("Nuevos:        0"))
            && c.base.movimientos(REG, &nss).map(|m| m.len() == 5).unwrap_or(false)
            && c.base.trabajadores_de(REG).ok() == Some(1),
    );
    ui.invoke_calcular_patron();
    paso("tras reimportar, el cálculo es el mismo", ui.get_calculo_texto().contains("amort. 1,843.51"));
    let _ = std::fs::remove_dir_all(&carpeta_txt);

    ui.invoke_trabajador_elegido(0);
    paso(
        "se dice por qué no se puede eliminar, antes de intentarlo",
        ui.get_t_bloqueo().contains("movimientos") && ui.get_t_bloqueo().contains("registre su baja") && ui.get_ctx_trabajador().contains("AUTOPRUEBA UNO") && {
            ui.invoke_patron_elegido(0);
            ui.get_p_bloqueo().contains("1 trabajador")
        },
    );
    ui.invoke_trabajador_eliminar();
    paso("un trabajador con movimientos no se elimina", ui.get_mensaje_trabajador().starts_with("No se eliminó") && ui.get_filas_trabajadores().row_count() == 1);
    ui.invoke_movimientos_abiertos();
    for _ in 0..5 {
        ui.set_m_fila(0);
        ui.invoke_movimiento_eliminar();
    }
    paso("eliminar movimientos", ui.get_mensaje_movimiento() == "Movimiento eliminado." && ui.get_filas_movimientos().row_count() == 0);

    ui.invoke_patron_elegido(0);
    ui.invoke_patron_eliminar();
    paso("un patrón con trabajadores no se elimina", ui.get_mensaje_patron().starts_with("No se eliminó") && ui.get_filas_patrones().row_count() == 1);

    ui.invoke_trabajador_elegido(0);
    paso("sin movimientos ya se puede eliminar al trabajador", ui.get_t_bloqueo().is_empty());
    ui.invoke_trabajador_eliminar();
    paso("baja de trabajador", ui.get_mensaje_trabajador() == "Trabajador eliminado." && ui.get_filas_trabajadores().row_count() == 0);
    ui.invoke_patron_elegido(0);
    ui.invoke_patron_eliminar();
    paso("baja de patrón", ui.get_mensaje_patron() == "Patrón eliminado." && ui.get_filas_patrones().row_count() == 0 && patron_activo(ui, c).is_none());
    paso("al eliminar el patrón se eliminan sus cálculos guardados", c.base.calculos_guardados(REG).map(|l| l.is_empty()).unwrap_or(false));

    // --- Catálogos
    paso("catálogo inicial (Municipios)", ui.get_filas_catalogo().row_count() == 1580 && ui.get_columnas_catalogo().row_count() == 3);
    ui.invoke_catalogo_elegido("INPC".into());
    paso("cambio de catálogo (INPC)", ui.get_filas_catalogo().row_count() == 344);
    ui.invoke_catalogo_buscar("texto que no existe en el catálogo".into());
    let vacio = ui.get_filas_catalogo().row_count();
    ui.invoke_catalogo_buscar("".into());
    paso("búsqueda en catálogo", vacio == 0 && ui.get_filas_catalogo().row_count() == 344);
    ui.set_i_mes("202603".into());
    ui.set_i_inpc("144.72".into());
    ui.set_i_tasa("2.07".into());
    ui.invoke_inpc_guardar();
    paso("un mes de INPC fuera de orden se rechaza", ui.get_catalogo_es_inpc() && ui.get_mensaje_inpc().contains("mes anterior") && ui.get_filas_catalogo().row_count() == 344);
    ui.set_i_mes("202602".into());
    ui.invoke_inpc_guardar();
    paso("captura del INPC de un mes nuevo", ui.get_mensaje_inpc() == "Mes 202602 guardado." && ui.get_filas_catalogo().row_count() == 345 && ui.get_i_mes().is_empty());

    // --- Cálculo (importes comprobados con archivos de pago reales)
    ui.set_c_periodo("200802".into());
    ui.set_c_sdi("100".into());
    ui.set_c_dias("29".into());
    ui.set_c_dias_bim("60".into());
    ui.set_c_prima("7.58875".into());
    ui.invoke_calcular();
    let texto = ui.get_calculo_texto();
    paso(
        "cálculo de febrero de 2008",
        ["311.12", "220.07", "189.00", "67.50", "300.00", "Salario mínimo diaria: 52.59"].iter().all(|v| texto.contains(v)),
    );
    ui.set_c_periodo("200806".into());
    ui.set_c_sdi("90".into());
    ui.set_c_dias("7".into());
    ui.set_c_dias_bim("".into());
    ui.set_c_prima("6.58875".into());
    ui.set_c_decreto(true);
    ui.invoke_calcular();
    let texto = ui.get_calculo_texto();
    paso("cálculo con decreto del 5 %", texto.contains("71.35") && texto.contains("39.43") && texto.contains("decreto de exención del 5 %"));
    // Noviembre de 2026: UMA 117.31, salario mínimo 315.04 (migración 3).
    ui.set_c_periodo("202611".into());
    ui.set_c_sdi("315.04".into());
    ui.set_c_dias("30".into());
    ui.set_c_dias_bim("61".into());
    ui.set_c_prima("0.5".into());
    ui.set_c_decreto(false);
    ui.invoke_calcular();
    let texto = ui.get_calculo_texto();
    paso(
        "cálculo de 2026 con UMA y cesantía del rango de salario mínimo",
        texto.contains("UMA diaria: 117.31") && texto.contains("717.94") && texto.contains("cesantía patronal 3.150 %") && !texto.contains("AVISO"),
    );
    ui.set_c_periodo("202811".into());
    ui.invoke_calcular();
    paso("periodo sin valores en catálogo avisa que no es vigente", ui.get_calculo_texto().contains("AVISO"));
    ui.set_c_sdi("mucho".into());
    ui.invoke_calcular();
    paso("salario no numérico en cálculo se rechaza", ui.get_c_mensaje().starts_with("Salario diario integrado") && ui.get_estado_calculo() == 3);

    // --- Patrón activo: elegir una fila en Patrones lo activa en toda la aplicación
    {
        let segundo = "Z9900000307";
        ui.invoke_patron_nuevo();
        ui.set_p_reg(REG.into());
        ui.set_p_rfc("XAXX010101000".into());
        ui.set_p_nombre("Primero".into());
        ui.invoke_patron_guardar();
        ui.invoke_patron_nuevo();
        ui.set_p_reg(segundo.into());
        ui.set_p_rfc("XAXX010101000".into());
        ui.set_p_nombre("Segundo".into());
        ui.invoke_patron_guardar();
        let orden: Vec<String> = c.base.patrones().unwrap_or_default().into_iter().map(|p| p.reg_pat).collect();
        let fila_del_segundo = orden.iter().position(|r| r == segundo).unwrap_or(0) as i32;
        let fila_del_primero = orden.iter().position(|r| r == REG).unwrap_or(0) as i32;
        ui.invoke_patron_elegido(fila_del_segundo);
        let activo_segundo = patron_activo(ui, c).as_deref() == Some(segundo) && ui.get_indice_patron() == fila_del_segundo;
        ui.invoke_patron_elegido(fila_del_primero);
        paso("elegir un patrón en la lista lo deja como patrón activo", ui.get_lista_patrones().row_count() == 2 && activo_segundo && patron_activo(ui, c).as_deref() == Some(REG));
        for fila in [0, 0] {
            ui.invoke_patron_elegido(fila);
            ui.invoke_patron_eliminar();
        }
        paso("sin patrones no hay patrón activo", ui.get_lista_patrones().row_count() == 0 && patron_activo(ui, c).is_none());
    }

    // --- Iconos de estado de los mensajes
    // El estado de cada mensaje lo fija quien lo produce, no su texto.
    ui.invoke_patron_nuevo();
    ui.set_p_reg("corto".into());
    ui.invoke_patron_guardar();
    let error = ui.get_estado_patron();
    ui.set_p_reg(REG.into());
    ui.set_p_rfc("XAXX010101000".into());
    ui.set_p_nombre("Estados".into());
    ui.set_p_entidad(9);
    ui.invoke_patron_guardar();
    let correcto = ui.get_estado_patron();
    let entidad = c.base.patrones().ok().and_then(|p| p.first().map(|x| x.ent_pat.clone()));
    ui.set_p_anio("2026".into());
    ui.invoke_patron_determinar_prima();
    let sin_dias = ui.get_estado_patron();
    ui.invoke_patron_elegido(0);
    ui.invoke_patron_eliminar();
    paso(
        "cada mensaje lleva su estado: error, correcto",
        error == 3 && correcto == 1 && sin_dias == 3 && ui.get_estado_patron() == 1 && entidad.as_deref() == Some("09") && ui.get_filas_patrones().row_count() == 0,
    );
    paso(
        "validación al escribir: fecha, periodo y NSS",
        ui.invoke_fecha_valida("2026-02-28".into()) && !ui.invoke_fecha_valida("2026-02-30".into()) && !ui.invoke_fecha_valida("28/02/2026".into())
            && ui.invoke_periodo_valido("202602".into()) && !ui.invoke_periodo_valido("202613".into())
            && ui.invoke_nss_valido(nss.as_str().into()) && !ui.invoke_nss_valido("12345678901".into()) && !ui.invoke_nss_valido("123".into()),
    );

    // --- Sistema
    ui.invoke_sistema_respaldar();
    paso("respaldo manual", ui.get_sistema_mensaje().contains("Respaldo creado") && ui.get_estado_sistema() == 1 && c.base.respaldos().map(|r| r.len() == 8).unwrap_or(false));
    // --- Restaurar un respaldo. La base está vacía; los respaldos previos a importar TXT tienen un patrón y un trabajador.
    ui.invoke_sistema_actualizar();
    let etiquetas: Vec<String> = ui.get_sistema_respaldos().iter().map(|s| s.to_string()).collect();
    let posicion = |texto: &str, lista: &[String]| lista.iter().position(|e| e.contains(texto)).map_or(-1, |i| i as i32);
    paso("la lista de respaldos muestra fecha y motivo", etiquetas.len() == 8 && etiquetas.iter().all(|e| e.contains(" · ") && !e.contains("UTC") && !e.contains("antes-de")) && ui.get_sistema_respaldo() == -1);
    ui.set_sistema_respaldo(posicion("Antes de importar TXT", &etiquetas));
    let texto = restaurar_respaldo(ui, c);
    paso(
        "restaurar un respaldo recupera sus datos",
        texto.starts_with("Respaldo restaurado") && texto.contains("1 patrones y 1 trabajadores") && ui.get_filas_patrones().row_count() == 1 && c.base.trabajadores_de(REG).ok() == Some(1),
    );
    let etiquetas: Vec<String> = ui.get_sistema_respaldos().iter().map(|s| s.to_string()).collect();
    paso("antes de restaurar se guardó la base anterior", etiquetas.len() == 9 && posicion("Antes de restaurar", &etiquetas) >= 0);
    // Se vuelve a la base vacía con el respaldo manual.
    ui.set_sistema_respaldo(posicion("Respaldo manual", &etiquetas));
    let texto = restaurar_respaldo(ui, c);
    paso(
        "restaurar el respaldo manual deja la base como estaba",
        texto.starts_with("Respaldo restaurado") && ui.get_filas_patrones().row_count() == 0 && c.base.patrones().map(|p| p.is_empty()).unwrap_or(false) && c.base.integridad().ok().as_deref() == Some("ok"),
    );
    ui.set_sistema_respaldo(-1);
    paso("sin respaldo elegido no se restaura", restaurar_respaldo(ui, c) == "Elija un respaldo de la lista." && ui.get_estado_sistema() == 3);
    paso("la tabla marca la fila del registro cargado", {
        ui.invoke_patron_nuevo();
        ui.get_p_fila() == -1 && ui.get_t_fila() == -1
    });
    ui.invoke_sistema_compactar();
    paso("compactar", ui.get_sistema_mensaje().contains("Base compactada") && ui.get_estado_sistema() == 1 && c.base.integridad().ok().as_deref() == Some("ok"));

    // --- Contraseña de la base. Aquí solo se valida lo escrito: el cambio se aplica al cerrar la ventana.
    let cifrada = c.base.cifrada();
    paso(
        "Sistema dice si la base está cifrada",
        ui.get_sistema_cifrada() == cifrada && ui.get_sistema_texto().contains(if cifrada { "Cifrado:            sí" } else { "Cifrado:            no" }),
    );
    let actual = std::env::var("SUA_CLAVE").unwrap_or_default();
    ui.set_sistema_clave_actual(actual.as_str().into());
    let intento = |nueva: &str, repetida: &str, quitar: bool| {
        ui.set_sistema_clave_nueva(nueva.into());
        ui.set_sistema_clave_repetida(repetida.into());
        preparar_cambio_de_clave(ui, c, quitar)
    };
    paso(
        "una contraseña nueva mal escrita o corta se rechaza",
        intento("contraseña larga", "contraseña larg", false).is_err_and(|e| e.to_string().contains("iguales")) && intento("corta", "corta", false).is_err_and(|e| e.to_string().contains("al menos")),
    );
    paso(
        "una contraseña nueva válida queda lista para aplicarse",
        intento("otra contraseña", "otra contraseña", false).is_ok_and(|(a, n)| a.is_some() == cifrada && n.as_deref() == Some("otra contraseña")),
    );
    paso("quitar la contraseña solo se ofrece si la hay", intento("", "", true).is_ok() == cifrada);
    if cifrada {
        ui.set_sistema_clave_actual("no es la actual".into());
        paso("sin la contraseña actual no se cambia", intento("otra contraseña", "otra contraseña", false).is_err_and(|e| e.to_string().contains("actual")));
        paso(
            "los respaldos de una base cifrada salen cifrados",
            c.base.respaldos_sin_cifrar().map(|r| r.is_empty()).unwrap_or(false) && ui.get_sistema_aviso_respaldos().is_empty(),
        );
    }
    ui.set_sistema_clave_actual(slint::SharedString::new());
    ui.set_sistema_clave_nueva(slint::SharedString::new());
    ui.set_sistema_clave_repetida(slint::SharedString::new());

    // --- Archivo de pago (opcional)
    let no_es_sua = c.entorno.logs.join("app.log");
    abrir_pago(ui, c, &no_es_sua);
    paso("un archivo que no es de pago se rechaza", ui.get_pago_resumen().starts_with("No se pudo leer") && !ui.get_pago_importable());
    if let Some(ruta) = std::env::var_os("SUA_AUTOPRUEBA_SUA") {
        abrir_pago(ui, c, std::path::Path::new(&ruta));
        paso("lectura de un archivo de pago real", ui.get_pago_importable() && ui.get_pago_resumen().contains("Verificación: correcta"));
        paso(
            "comprobación del cálculo del archivo real",
            ui.get_pago_resumen().contains("Comprobación del cálculo:") && ui.get_pago_resumen().contains("importes coinciden"),
        );
        ui.invoke_pago_importar();
        let patrones = c.base.patrones().unwrap_or_default();
        let trabajadores = patrones.first().and_then(|p| c.base.trabajadores_de(&p.reg_pat).ok()).unwrap_or(0);
        paso(
            "importación del archivo de pago",
            ui.get_pago_resumen().contains("Importación terminada") && patrones.len() == 1 && trabajadores > 0 && !ui.get_pago_importable(),
        );
        paso("la importación se refleja en las pantallas", ui.get_filas_patrones().row_count() == 1 && ui.get_filas_trabajadores().row_count() as i64 == trabajadores);
        paso("se hizo un respaldo antes de importar", c.base.respaldos().map(|r| r.len() == 10).unwrap_or(false));
    }

    // --- Usuarios y roles. Va al final: desde que se crea el primer administrador la base exige sesión.
    const CLAVE_USUARIO: &str = "clave de usuario 1";
    let alta = |nombre: &str, rol: i32, clave: &str, repetida: &str| {
        mostrar_usuario(ui, c, 0);
        ui.set_u_nombre(nombre.into());
        ui.set_u_rol(rol);
        ui.set_u_clave(clave.into());
        ui.set_u_repetida(repetida.into());
        guardar_usuario(ui, c)
    };
    paso(
        "sin usuarios todo está permitido y no se pide sesión",
        !ui.get_sistema_hay_usuarios() && ui.get_puede_capturar() && ui.get_puede_eliminar() && ui.get_puede_administrar() && c.base.sesion().is_none() && !ui.get_pie().contains('('),
    );
    paso(
        "el primer usuario tiene que ser administrador y con contraseña válida",
        alta("captura", 1, CLAVE_USUARIO, CLAVE_USUARIO).is_err_and(|e| e.to_string().contains("primer usuario")) && alta("admin", 0, "corta", "corta").is_err() && !c.base.hay_usuarios(),
    );
    paso(
        "crear el primer administrador deja su sesión iniciada",
        alta("Admin", 0, CLAVE_USUARIO, CLAVE_USUARIO).is_ok_and(|m| m.contains("administrador admin"))
            && ui.get_sistema_hay_usuarios()
            && c.base.sesion().is_some_and(|s| s.nombre == "admin")
            && ui.get_pie().contains("admin (Administrador)")
            && ui.get_puede_administrar(),
    );
    paso(
        "alta de un capturista y de un usuario de consulta",
        alta("captura", 1, CLAVE_USUARIO, CLAVE_USUARIO).is_ok() && alta("consulta", 2, CLAVE_USUARIO, CLAVE_USUARIO).is_ok() && ui.get_sistema_usuarios().row_count() == 4 && alta("consulta", 2, CLAVE_USUARIO, CLAVE_USUARIO).is_err(),
    );
    // El único administrador no se puede desactivar (posición 1 de la lista: admin).
    mostrar_usuario(ui, c, 1);
    ui.set_u_activo(false);
    paso("el único administrador no se puede desactivar", ui.get_u_nombre() == "admin" && guardar_usuario(ui, c).is_err_and(|e| e.to_string().contains("único administrador")));

    // Consulta: ve, pero no cambia nada; los botones quedan deshabilitados y el núcleo se niega.
    let entrar = |nombre: &str| {
        let entro = c.base.iniciar_sesion(nombre, CLAVE_USUARIO).is_ok();
        refrescar_usuarios(ui, c);
        entro
    };
    let patrones_antes = c.base.patrones().map(|p| p.len()).unwrap_or(usize::MAX);
    paso(
        "consulta: botones deshabilitados, sin lista de usuarios ni bitácora",
        entrar("consulta")
            && !ui.get_puede_capturar()
            && !ui.get_puede_eliminar()
            && !ui.get_puede_exportar()
            && !ui.get_puede_respaldar()
            && !ui.get_puede_administrar()
            && ui.get_sistema_usuarios().row_count() == 1
            && ui.get_sistema_bitacora().is_empty()
            && ui.get_pie().contains("consulta (Consulta: solo lectura)"),
    );
    ui.invoke_patron_nuevo();
    ui.set_p_reg("Z9900000208".into());
    ui.set_p_rfc("XAXX010101000".into());
    ui.set_p_nombre("PATRON FICTICIO DOS".into());
    ui.invoke_patron_guardar();
    paso(
        "consulta: guardar se rechaza con su razón y no cambia la base",
        ui.get_mensaje_patron().contains("consulta") && ui.get_estado_patron() == 3 && c.base.patrones().map(|p| p.len()).ok() == Some(patrones_antes),
    );
    ui.invoke_sistema_respaldar();
    paso("consulta: tampoco puede respaldar ni cambiar la contraseña de la base", ui.get_estado_sistema() == 3 && preparar_cambio_de_clave(ui, c, false).is_err_and(|e| e.to_string().contains("administrador")));

    // Capturista: captura y respalda; no elimina patrones ni administra.
    paso(
        "capturista: puede capturar, exportar y respaldar, no eliminar ni administrar",
        entrar("captura") && ui.get_puede_capturar() && ui.get_puede_exportar() && ui.get_puede_respaldar() && !ui.get_puede_eliminar() && !ui.get_puede_administrar(),
    );
    ui.invoke_patron_guardar();
    paso("capturista: guardar un patrón funciona", ui.get_estado_patron() != 3 && c.base.patrones().map(|p| p.len()).ok() == Some(patrones_antes + 1));
    paso("capturista: el núcleo no le deja eliminar el patrón", c.base.eliminar_patron("Z9900000208").is_err_and(|e| e.to_string().contains("administrador")));
    ui.set_mi_clave_actual(CLAVE_USUARIO.into());
    ui.set_mi_clave_nueva("otra de captura 2".into());
    ui.set_mi_clave_repetida("otra de captura 2".into());
    ui.invoke_sistema_mi_clave();
    paso(
        "cada usuario cambia su propia contraseña",
        ui.get_estado_mi_clave() == 1 && c.base.iniciar_sesion("captura", CLAVE_USUARIO).is_err() && c.base.iniciar_sesion("captura", "otra de captura 2").is_ok(),
    );

    // Administrador: todo, y ve en la bitácora lo que hicieron los demás.
    paso("administrador: elimina el patrón", entrar("admin") && ui.get_puede_eliminar() && c.base.eliminar_patron("Z9900000208").is_ok());
    refrescar_usuarios(ui, c);
    let bitacora = ui.get_sistema_bitacora().to_string();
    paso(
        "la bitácora dice quién hizo qué, sin nombres de personas",
        ["consulta", "Entró", "captura               Guardó patrón: Z9900000208", "Cambió su contraseña", "Intento de entrada rechazado", "admin                 Eliminó patrón: Z9900000208"].iter().all(|t| bitacora.contains(t)) && !bitacora.contains("FICTICIO"),
    );
    ui.invoke_patron_nuevo();
    mostrar_usuario(ui, c, 0);

    terminar(c, &pasos)
}

fn terminar(c: &Contexto, pasos: &[(String, bool)]) -> i32 {
    let fallos = pasos.iter().filter(|(_, ok)| !ok).count();
    let mut texto = String::new();
    for (nombre, ok) in pasos {
        texto.push_str(&format!("{} {nombre}\n", if *ok { "OK   " } else { "FALLO" }));
    }
    texto.push_str(&format!("\n{} pasos, {fallos} fallos\n", pasos.len()));
    let _ = std::fs::write(c.entorno.logs.join("autoprueba.log"), texto);
    c.bitacora.info(&format!("Autoprueba de la interfaz: {} pasos, {fallos} fallos.", pasos.len()));
    i32::from(fallos > 0)
}
