//! Lector del archivo de pago `.SUA`.
//!
//! Layout obtenido de seis archivos reales de 2006–2008; tablas completas y
//! nivel de certeza de cada campo en `Decompilacion_SUA.md`, sección 14.
//! Los campos con sufijo `_inf` son inferidos: en las muestras iban en ceros o
//! en blanco, así que su posición no está comprobada.
//!
//! Solo lectura. Escribir un `.SUA` aceptable exige el número de control del
//! registro 06, cuyo algoritmo no se conoce (`ESPECIFICACION_SUA.md`, §7.4).
//!
//! Importes en centavos (`i64`). Posiciones 1‑based inclusivas, como en la
//! documentación.

use std::collections::HashMap;

pub const LARGO_REGISTRO: usize = 295;
const ALFABETO: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ErrorPago {
    #[error("el tamaño ({0} bytes) no es múltiplo de 295; no es un archivo de pago o está truncado")]
    Tamano(usize),
    #[error("el archivo está vacío")]
    Vacio,
    #[error("registro {registro}: tipo «{tipo}» desconocido")]
    TipoDesconocido { registro: usize, tipo: String },
    #[error("registro {registro} (tipo {tipo}): el campo «{campo}» (posiciones {desde}-{hasta}) no es numérico")]
    CampoNumerico { registro: usize, tipo: &'static str, campo: &'static str, desde: usize, hasta: usize },
    #[error("registro {registro}: el importe codificado «{campo}» no es base 62 válida")]
    Base62 { registro: usize, campo: &'static str },
    #[error("estructura inesperada: {0}")]
    Estructura(String),
}

/// Importe en centavos escrito en base 62 con el alfabeto `0-9 A-Z a-z`.
/// Devuelve `None` si hay un carácter fuera del alfabeto.
pub fn base62_a_centavos(texto: &str) -> Option<i64> {
    texto.bytes().try_fold(0i64, |acc, b| {
        let d = ALFABETO.iter().position(|&a| a == b)? as i64;
        acc.checked_mul(62)?.checked_add(d)
    })
}

/// Inverso de [`base62_a_centavos`], con `ancho` caracteres y ceros a la
/// izquierda. `None` si el importe es negativo o no cabe.
pub fn centavos_a_base62(mut centavos: i64, ancho: usize) -> Option<String> {
    if centavos < 0 {
        return None;
    }
    let mut salida = vec![b'0'; ancho];
    for casilla in salida.iter_mut().rev() {
        *casilla = ALFABETO[(centavos % 62) as usize];
        centavos /= 62;
    }
    (centavos == 0).then(|| String::from_utf8(salida).unwrap())
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Patron02 {
    pub registro_patronal: String,
    pub rfc: String,
    pub periodo: String,
    pub folio_sua: String,
    pub razon_social: String,
    pub calle_colonia: String,
    pub poblacion: String,
    pub entidad: String,
    pub codigo_postal: String,
    pub telefono_inf: String,
    /// Cienmilésimas de punto porcentual: 7.58875 % = 758875.
    pub prima_rt: i64,
    pub fecha_prima_rt: String,
    pub actividad: String,
    pub delegacion: String,
    pub subdelegacion: String,
    pub area_geografica: String,
    pub porcentaje_infonavit_inf: String,
    pub convenio_reembolso_inf: String,
    pub tipo_cotizacion_inf: String,
    pub dias_cotizados: i64,
    pub cotizantes: i64,
    pub tipo_documento_inf: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Trabajador03 {
    pub nss: String,
    pub rfc: String,
    pub curp: String,
    pub credito_infonavit_inf: String,
    pub fecha_inicio_descuento_inf: String,
    pub movimientos: i64,
    /// `PATERNO$MATERNO$NOMBRE`
    pub nombre: String,
    pub sdi: i64,
    pub tipo_trabajador: String,
    pub jornada_reducida: String,
    pub dias_mes: i64,
    pub dias_incapacidad_mes_inf: i64,
    pub dias_ausentismo_mes_inf: i64,
    pub cuota_fija: i64,
    /// Patronal + obrera, igual que los tres siguientes.
    pub excedente: i64,
    pub prestaciones_dinero: i64,
    pub gastos_medicos: i64,
    pub riesgos_trabajo: i64,
    pub invalidez_vida: i64,
    pub guarderias: i64,
    pub act_rec_imss_inf: i64,
    pub dias_bimestre: i64,
    pub dias_incapacidad_bim_inf: i64,
    pub dias_ausentismo_bim_inf: i64,
    pub retiro: i64,
    pub act_rec_retiro: i64,
    pub cyv_patronal: i64,
    pub cyv_obrera: i64,
    pub act_rec_cyv: i64,
    pub aportacion_voluntaria_inf: i64,
    pub aportacion_patronal: i64,
    pub amortizacion_inf: i64,
    /// Importes obreros decodificados de base 62. `None` = campo en blanco: el
    /// archivo no incluye ese ramo.
    pub excedente_obrero: Option<i64>,
    pub prestaciones_dinero_obrero: Option<i64>,
    pub gastos_medicos_obrero: Option<i64>,
    pub invalidez_vida_obrero: Option<i64>,
    pub cyv_obrera_codificada: Option<i64>,
    pub cola: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Movimiento04 {
    pub nss: String,
    /// `00` salario al inicio del bimestre, `01` alta, `02` baja, `08` reingreso, `13` (sin identificar).
    pub tipo: String,
    /// `AAAAMMDD`
    pub fecha: String,
    pub folio_incapacidad_inf: String,
    pub dias_inf: i64,
    pub sdi: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Totales05 {
    pub folio_sua: String,
    pub tasa_actualizacion_inf: i64,
    /// Diezmilésimas de punto porcentual: 1.13 % = 11300.
    pub tasa_recargos: i64,
    pub cuota_fija: i64,
    pub excedente: i64,
    pub prestaciones_dinero: i64,
    pub gastos_medicos: i64,
    pub riesgos_trabajo: i64,
    pub invalidez_vida: i64,
    pub guarderias: i64,
    pub suma_imss: i64,
    pub actualizacion_imss_inf: i64,
    pub recargos_imss_inf: i64,
    pub retiro: i64,
    pub cyv: i64,
    pub suma_rcv: i64,
    pub actualizacion_rcv_inf: i64,
    pub recargos_rcv: i64,
    /// Posiciones 198–280 sin interpretar: los límites de campo del bloque
    /// INFONAVIT no se conocen.
    pub bloque_infonavit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Control06 {
    pub folio_sua: String,
    pub tasa_recargos: i64,
    pub tamano_archivo: i64,
    /// `AAAAMMDD`; se presume fecha de pago.
    pub fecha: String,
    pub marca: String,
    pub total_imss: i64,
    pub total_rcv: i64,
    pub total_infonavit: i64,
    /// 19 dígitos; algoritmo desconocido.
    pub numero_control: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchivoPago {
    pub bytes: usize,
    pub patron: Patron02,
    pub trabajadores: Vec<Trabajador03>,
    pub movimientos: Vec<Movimiento04>,
    pub totales: Totales05,
    pub control: Control06,
}

struct Reg<'a> {
    datos: &'a [u8],
    numero: usize,
    tipo: &'static str,
}

impl Reg<'_> {
    fn crudo(&self, desde: usize, hasta: usize) -> &[u8] {
        &self.datos[desde - 1..hasta]
    }

    /// Texto Windows‑1252 (se decodifica como Latin‑1: coinciden en las letras
    /// acentuadas y la Ñ) sin espacios al final.
    fn txt(&self, desde: usize, hasta: usize) -> String {
        let s: String = self.crudo(desde, hasta).iter().map(|&b| b as char).collect();
        s.trim_end().to_string()
    }

    /// Número con ceros a la izquierda. Un campo en blanco vale 0.
    fn num(&self, desde: usize, hasta: usize, campo: &'static str) -> Result<i64, ErrorPago> {
        let c = self.crudo(desde, hasta);
        if c.iter().all(|&b| b == b' ') {
            return Ok(0);
        }
        if !c.iter().all(u8::is_ascii_digit) {
            return Err(ErrorPago::CampoNumerico { registro: self.numero, tipo: self.tipo, campo, desde, hasta });
        }
        Ok(c.iter().fold(0i64, |acc, &b| acc * 10 + i64::from(b - b'0')))
    }

    fn b62(&self, desde: usize, campo: &'static str) -> Result<Option<i64>, ErrorPago> {
        let c = self.crudo(desde, desde + 2);
        if c.iter().all(|&b| b == b' ') {
            return Ok(None);
        }
        std::str::from_utf8(c)
            .ok()
            .and_then(base62_a_centavos)
            .map(Some)
            .ok_or(ErrorPago::Base62 { registro: self.numero, campo })
    }
}

fn patron(r: &Reg) -> Result<Patron02, ErrorPago> {
    Ok(Patron02 {
        registro_patronal: r.txt(3, 13),
        rfc: r.txt(14, 26),
        periodo: r.txt(27, 32),
        folio_sua: r.txt(33, 38),
        razon_social: r.txt(39, 88),
        calle_colonia: r.txt(89, 128),
        poblacion: r.txt(129, 168),
        entidad: r.txt(169, 170),
        codigo_postal: r.txt(171, 175),
        telefono_inf: r.txt(176, 190),
        prima_rt: r.num(191, 197, "prima de RT")?,
        fecha_prima_rt: r.txt(198, 203),
        actividad: r.txt(204, 243),
        delegacion: r.txt(244, 245),
        subdelegacion: r.txt(246, 247),
        area_geografica: r.txt(248, 248),
        porcentaje_infonavit_inf: r.txt(249, 250),
        convenio_reembolso_inf: r.txt(251, 251),
        tipo_cotizacion_inf: r.txt(252, 252),
        dias_cotizados: r.num(253, 259, "días cotizados")?,
        cotizantes: r.num(260, 268, "cotizantes")?,
        tipo_documento_inf: r.txt(269, 270),
    })
}

fn trabajador(r: &Reg) -> Result<Trabajador03, ErrorPago> {
    Ok(Trabajador03 {
        nss: r.txt(33, 43),
        rfc: r.txt(44, 56),
        curp: r.txt(57, 74),
        credito_infonavit_inf: r.txt(75, 84),
        fecha_inicio_descuento_inf: r.txt(85, 92),
        movimientos: r.num(93, 94, "número de movimientos")?,
        nombre: r.txt(95, 144),
        sdi: r.num(145, 151, "SDI")?,
        tipo_trabajador: r.txt(152, 152),
        jornada_reducida: r.txt(153, 153),
        dias_mes: r.num(154, 155, "días del mes")?,
        dias_incapacidad_mes_inf: r.num(156, 157, "días de incapacidad")?,
        dias_ausentismo_mes_inf: r.num(158, 159, "días de ausentismo")?,
        cuota_fija: r.num(160, 166, "cuota fija")?,
        excedente: r.num(167, 173, "excedente")?,
        prestaciones_dinero: r.num(174, 180, "prestaciones en dinero")?,
        gastos_medicos: r.num(181, 187, "gastos médicos")?,
        riesgos_trabajo: r.num(188, 194, "riesgos de trabajo")?,
        invalidez_vida: r.num(195, 201, "invalidez y vida")?,
        guarderias: r.num(202, 208, "guarderías")?,
        act_rec_imss_inf: r.num(209, 215, "actualización y recargos IMSS")?,
        dias_bimestre: r.num(216, 217, "días del bimestre")?,
        dias_incapacidad_bim_inf: r.num(218, 219, "días de incapacidad del bimestre")?,
        dias_ausentismo_bim_inf: r.num(220, 221, "días de ausentismo del bimestre")?,
        retiro: r.num(222, 228, "retiro")?,
        act_rec_retiro: r.num(229, 235, "actualización y recargos de retiro")?,
        cyv_patronal: r.num(236, 242, "CyV patronal")?,
        cyv_obrera: r.num(243, 249, "CyV obrera")?,
        act_rec_cyv: r.num(250, 256, "actualización y recargos de CyV")?,
        aportacion_voluntaria_inf: r.num(257, 263, "aportación voluntaria")?,
        aportacion_patronal: r.num(264, 270, "aportación patronal")?,
        amortizacion_inf: r.num(271, 277, "amortización")?,
        excedente_obrero: r.b62(278, "excedente obrero")?,
        prestaciones_dinero_obrero: r.b62(281, "prestaciones en dinero obrera")?,
        gastos_medicos_obrero: r.b62(284, "gastos médicos obrera")?,
        invalidez_vida_obrero: r.b62(287, "invalidez y vida obrera")?,
        cyv_obrera_codificada: r.b62(290, "CyV obrera")?,
        cola: r.txt(293, 295),
    })
}

fn movimientos(r: &Reg, salida: &mut Vec<Movimiento04>) -> Result<(), ErrorPago> {
    for g in 0..7 {
        let base = 14 + g * 38;
        if r.crudo(base, base + 37).iter().all(|&b| b == b' ') {
            continue;
        }
        salida.push(Movimiento04 {
            nss: r.txt(base, base + 10),
            tipo: r.txt(base + 11, base + 12),
            fecha: r.txt(base + 13, base + 20),
            folio_incapacidad_inf: r.txt(base + 21, base + 28),
            dias_inf: r.num(base + 29, base + 30, "días del movimiento")?,
            sdi: r.num(base + 31, base + 37, "SDI del movimiento")?,
        });
    }
    Ok(())
}

fn totales(r: &Reg) -> Result<Totales05, ErrorPago> {
    Ok(Totales05 {
        folio_sua: r.txt(33, 38),
        tasa_actualizacion_inf: r.num(39, 48, "tasa de actualización")?,
        tasa_recargos: r.num(49, 58, "tasa de recargos")?,
        cuota_fija: r.num(59, 67, "cuota fija")?,
        excedente: r.num(68, 76, "excedente")?,
        prestaciones_dinero: r.num(77, 85, "prestaciones en dinero")?,
        gastos_medicos: r.num(86, 94, "gastos médicos")?,
        riesgos_trabajo: r.num(95, 103, "riesgos de trabajo")?,
        invalidez_vida: r.num(104, 112, "invalidez y vida")?,
        guarderias: r.num(113, 121, "guarderías")?,
        suma_imss: r.num(122, 132, "suma IMSS")?,
        actualizacion_imss_inf: r.num(133, 141, "actualización IMSS")?,
        recargos_imss_inf: r.num(142, 150, "recargos IMSS")?,
        retiro: r.num(151, 159, "retiro")?,
        cyv: r.num(160, 168, "cesantía y vejez")?,
        suma_rcv: r.num(169, 179, "suma RCV")?,
        actualizacion_rcv_inf: r.num(180, 188, "actualización RCV")?,
        recargos_rcv: r.num(189, 197, "recargos RCV")?,
        bloque_infonavit: r.txt(198, 280),
    })
}

fn control(r: &Reg) -> Result<Control06, ErrorPago> {
    Ok(Control06 {
        folio_sua: r.txt(33, 38),
        tasa_recargos: r.num(39, 48, "tasa de recargos")?,
        tamano_archivo: r.num(49, 58, "tamaño del archivo")?,
        fecha: r.txt(59, 66),
        marca: r.txt(69, 70),
        total_imss: r.num(73, 84, "total IMSS")?,
        total_rcv: r.num(85, 96, "total RCV")?,
        total_infonavit: r.num(97, 108, "total INFONAVIT")?,
        numero_control: r.txt(121, 139),
    })
}

/// Interpreta el contenido completo de un archivo de pago.
pub fn leer(datos: &[u8]) -> Result<ArchivoPago, ErrorPago> {
    if datos.is_empty() {
        return Err(ErrorPago::Vacio);
    }
    if datos.len() % LARGO_REGISTRO != 0 {
        return Err(ErrorPago::Tamano(datos.len()));
    }
    let (mut p, mut t5, mut c6) = (None, None, None);
    let mut trabajadores = Vec::new();
    let mut movs = Vec::new();
    let mut orden = String::new();

    for (i, bloque) in datos.chunks(LARGO_REGISTRO).enumerate() {
        let numero = i + 1;
        let tipo: &'static str = match &bloque[..2] {
            b"02" => "02",
            b"03" => "03",
            b"04" => "04",
            b"05" => "05",
            b"06" => "06",
            otro => {
                return Err(ErrorPago::TipoDesconocido { registro: numero, tipo: String::from_utf8_lossy(otro).into_owned() })
            }
        };
        if !orden.ends_with(&tipo[1..]) {
            orden.push_str(&tipo[1..]);
        }
        let r = Reg { datos: bloque, numero, tipo };
        let unico = |ya: bool| {
            if ya {
                Err(ErrorPago::Estructura(format!("hay más de un registro {tipo}")))
            } else {
                Ok(())
            }
        };
        match tipo {
            "02" => {
                unico(p.is_some())?;
                p = Some(patron(&r)?)
            }
            "03" => trabajadores.push(trabajador(&r)?),
            "04" => movimientos(&r, &mut movs)?,
            "05" => {
                unico(t5.is_some())?;
                t5 = Some(totales(&r)?)
            }
            _ => {
                unico(c6.is_some())?;
                c6 = Some(control(&r)?)
            }
        }
    }

    // Orden observado: 02, 03…, 04…, 05, 06. Los 04 faltan si no hay movimientos.
    if orden != "23456" && orden != "2356" {
        let legible: Vec<String> = orden.chars().map(|c| format!("0{c}")).collect();
        return Err(ErrorPago::Estructura(format!(
            "orden de registros {} (se esperaba 02, 03, 04, 05, 06)",
            legible.join(", ")
        )));
    }
    let falta = |t: &str| ErrorPago::Estructura(format!("falta el registro {t}"));
    Ok(ArchivoPago {
        bytes: datos.len(),
        patron: p.ok_or_else(|| falta("02"))?,
        trabajadores,
        movimientos: movs,
        totales: t5.ok_or_else(|| falta("05"))?,
        control: c6.ok_or_else(|| falta("06"))?,
    })
}

impl ArchivoPago {
    /// Comprobaciones de consistencia interna. Lista vacía = todo cuadra.
    /// Los mensajes no contienen datos personales (el trabajador se identifica
    /// por su número de orden en el archivo).
    pub fn verificar(&self) -> Vec<String> {
        let mut h = Vec::new();
        let mut comparar = |nombre: &str, obtenido: i64, esperado: i64| {
            if obtenido != esperado {
                h.push(format!("{nombre}: el archivo dice {obtenido} y se esperaba {esperado}"));
            }
        };
        let t = &self.trabajadores;
        let suma = |f: fn(&Trabajador03) -> i64| t.iter().map(f).sum::<i64>();

        comparar("registro 06, tamaño del archivo", self.control.tamano_archivo, self.bytes as i64);
        comparar("registro 02, cotizantes", self.patron.cotizantes, t.len() as i64);
        let dias_bim = suma(|x| x.dias_bimestre);
        let dias = if dias_bim > 0 { dias_bim } else { suma(|x| x.dias_mes) };
        comparar("registro 02, días cotizados", self.patron.dias_cotizados, dias);

        let s = &self.totales;
        comparar("registro 05, cuota fija", s.cuota_fija, suma(|x| x.cuota_fija));
        comparar("registro 05, excedente", s.excedente, suma(|x| x.excedente));
        comparar("registro 05, prestaciones en dinero", s.prestaciones_dinero, suma(|x| x.prestaciones_dinero));
        comparar("registro 05, gastos médicos", s.gastos_medicos, suma(|x| x.gastos_medicos));
        comparar("registro 05, riesgos de trabajo", s.riesgos_trabajo, suma(|x| x.riesgos_trabajo));
        comparar("registro 05, invalidez y vida", s.invalidez_vida, suma(|x| x.invalidez_vida));
        comparar("registro 05, guarderías", s.guarderias, suma(|x| x.guarderias));
        comparar(
            "registro 05, suma IMSS",
            s.suma_imss,
            s.cuota_fija + s.excedente + s.prestaciones_dinero + s.gastos_medicos + s.riesgos_trabajo + s.invalidez_vida + s.guarderias,
        );
        comparar("registro 05, retiro", s.retiro, suma(|x| x.retiro));
        comparar("registro 05, cesantía y vejez", s.cyv, suma(|x| x.cyv_patronal + x.cyv_obrera));
        comparar("registro 05, suma RCV", s.suma_rcv, s.retiro + s.cyv);
        comparar("registro 05, recargos RCV", s.actualizacion_rcv_inf + s.recargos_rcv, suma(|x| x.act_rec_retiro + x.act_rec_cyv));

        let c = &self.control;
        comparar("registro 06, total IMSS", c.total_imss, s.suma_imss + s.actualizacion_imss_inf + s.recargos_imss_inf);
        comparar("registro 06, total RCV", c.total_rcv, s.suma_rcv + s.actualizacion_rcv_inf + s.recargos_rcv);
        comparar("registro 06, total INFONAVIT", c.total_infonavit, suma(|x| x.aportacion_patronal + x.amortizacion_inf));

        for (nombre, folio) in [("05", &s.folio_sua), ("06", &c.folio_sua)] {
            if *folio != self.patron.folio_sua {
                h.push(format!("registro {nombre}: el folio SUA no coincide con el del registro 02"));
            }
        }

        let mut por_nss: HashMap<&str, i64> = HashMap::new();
        for m in &self.movimientos {
            *por_nss.entry(m.nss.as_str()).or_default() += 1;
        }
        for (i, x) in t.iter().enumerate() {
            let n = i + 1;
            let en_04 = por_nss.remove(x.nss.as_str()).unwrap_or(0);
            if en_04 != x.movimientos {
                h.push(format!("trabajador {n}: declara {} movimientos y hay {en_04} en los registros 04", x.movimientos));
            }
            if x.cyv_obrera_codificada.unwrap_or(0) != x.cyv_obrera {
                h.push(format!("trabajador {n}: la CyV obrera codificada no coincide con la que va en claro"));
            }
            for (ramo, obrero, total) in [
                ("excedente", x.excedente_obrero, x.excedente),
                ("prestaciones en dinero", x.prestaciones_dinero_obrero, x.prestaciones_dinero),
                ("gastos médicos", x.gastos_medicos_obrero, x.gastos_medicos),
                ("invalidez y vida", x.invalidez_vida_obrero, x.invalidez_vida),
            ] {
                if obrero.unwrap_or(0) > total {
                    h.push(format!("trabajador {n}: la parte obrera de {ramo} es mayor que el total del ramo"));
                }
            }
        }
        if !por_nss.is_empty() {
            h.push(format!("hay movimientos de {} NSS que no aparecen en los registros 03", por_nss.len()));
        }
        h
    }
}

#[cfg(test)]
pub(crate) mod pruebas {
    use super::*;

    // Todo lo que sigue es ficticio: patrón, NSS, RFC, CURP, nombres e importes.
    const REG_PAT: &str = "Z9900000109";
    const RFC_PAT: &str = "XAXX010101000";

    fn registro(tipo: &str, periodo: bool) -> Vec<u8> {
        let mut r = vec![b' '; LARGO_REGISTRO];
        pon(&mut r, 1, tipo);
        pon(&mut r, 3, REG_PAT);
        if periodo {
            pon(&mut r, 14, RFC_PAT);
            pon(&mut r, 27, "202602");
        }
        r
    }

    fn pon(r: &mut [u8], pos: usize, texto: &str) {
        r[pos - 1..pos - 1 + texto.len()].copy_from_slice(texto.as_bytes());
    }

    fn num(r: &mut [u8], desde: usize, hasta: usize, valor: i64) {
        pon(r, desde, &format!("{valor:0ancho$}", ancho = hasta - desde + 1));
    }

    struct Fila {
        nss: &'static str,
        sdi: i64,
        dias: i64,
        dias_bim: i64,
    }

    /// Importes de un trabajador con las tasas vigentes en 2008 (las de las
    /// muestras reales): (cf, pd, gmp, rt, iv, gua, retiro, cvp, cvo, infonavit, pd_obr, gmp_obr, iv_obr).
    fn importes(f: &Fila) -> [i64; 13] {
        let base = f.sdi * f.dias; // centavos × días
        let bim = f.sdi * f.dias_bim;
        let pct = |b: i64, diezmilesimas: i64| (b * diezmilesimas + 500_000) / 1_000_000;
        [
            pct(5259 * f.dias, 204_000),
            pct(base, 9_500),
            pct(base, 14_250),
            pct(base, 75_888),
            pct(base, 23_750),
            pct(base, 10_000),
            pct(bim, 20_000),
            pct(bim, 31_500),
            pct(bim, 11_250),
            pct(bim, 50_000),
            pct(base, 2_500),
            pct(base, 3_750),
            pct(base, 6_250),
        ]
    }

    fn archivo(filas: &[Fila]) -> Vec<u8> {
        let mut salida = Vec::new();
        let total_registros = 1 + filas.len() + 1 + 2;

        let mut r = registro("02", true);
        pon(&mut r, 33, "123456");
        pon(&mut r, 39, "PATRON FICTICIO DE PRUEBA SA DE CV");
        pon(&mut r, 169, "09");
        pon(&mut r, 171, "06000");
        num(&mut r, 191, 197, 758_880);
        pon(&mut r, 198, "202503");
        pon(&mut r, 244, "3112A50N3");
        num(&mut r, 253, 259, filas.iter().map(|f| f.dias_bim).sum());
        num(&mut r, 260, 268, filas.len() as i64);
        pon(&mut r, 269, "01000000000");
        salida.extend(r);

        let mut tot = [0i64; 13];
        for (i, f) in filas.iter().enumerate() {
            let v = importes(f);
            for k in 0..13 {
                tot[k] += v[k];
            }
            let mut r = registro("03", true);
            pon(&mut r, 33, f.nss);
            pon(&mut r, 44, "XEXX010101000");
            pon(&mut r, 57, "XEXX010101HNEXXXA4");
            num(&mut r, 93, 94, 1);
            pon(&mut r, 95, &format!("FICTICIO$DE PRUEBA$TRABAJADOR {}", i + 1));
            num(&mut r, 145, 151, f.sdi);
            pon(&mut r, 152, "10");
            num(&mut r, 154, 155, f.dias);
            pon(&mut r, 156, "0000");
            for (k, desde) in [(0, 160), (1, 174), (2, 181), (3, 188), (4, 195), (5, 202)] {
                num(&mut r, desde, desde + 6, v[k]);
            }
            num(&mut r, 167, 173, 0);
            num(&mut r, 209, 215, 0);
            num(&mut r, 216, 217, f.dias_bim);
            pon(&mut r, 218, "0000");
            num(&mut r, 222, 228, v[6]);
            num(&mut r, 229, 235, 0);
            num(&mut r, 236, 242, v[7]);
            num(&mut r, 243, 249, v[8]);
            num(&mut r, 250, 256, 0);
            num(&mut r, 257, 263, 0);
            num(&mut r, 264, 270, v[9]);
            num(&mut r, 271, 277, 0);
            pon(&mut r, 278, "000");
            for (k, desde) in [(10, 281), (11, 284), (12, 287), (8, 290)] {
                pon(&mut r, desde, &centavos_a_base62(v[k], 3).unwrap());
            }
            pon(&mut r, 293, &REG_PAT[..3]);
            salida.extend(r);
        }

        let mut r = registro("04", false);
        for (g, f) in filas.iter().enumerate() {
            let base = 14 + g * 38;
            pon(&mut r, base, f.nss);
            pon(&mut r, base + 11, "0020260101");
            pon(&mut r, base + 29, "00");
            num(&mut r, base + 31, base + 37, f.sdi);
        }
        salida.extend(r);

        let mut r = registro("05", true);
        pon(&mut r, 33, "123456");
        num(&mut r, 39, 58, 0);
        for (k, desde) in [(0, 59), (1, 77), (2, 86), (3, 95), (4, 104), (5, 113)] {
            num(&mut r, desde, desde + 8, tot[k]);
        }
        num(&mut r, 68, 76, 0);
        let imss: i64 = tot[..6].iter().sum();
        num(&mut r, 122, 132, imss);
        num(&mut r, 133, 150, 0);
        num(&mut r, 151, 159, tot[6]);
        num(&mut r, 160, 168, tot[7] + tot[8]);
        num(&mut r, 169, 179, tot[6] + tot[7] + tot[8]);
        num(&mut r, 180, 197, 0);
        num(&mut r, 198, 217, tot[9]);
        pon(&mut r, 218, &"0".repeat(63));
        salida.extend(r);

        let mut r = registro("06", true);
        pon(&mut r, 33, "123456");
        num(&mut r, 39, 48, 0);
        num(&mut r, 49, 58, (total_registros * LARGO_REGISTRO) as i64);
        pon(&mut r, 59, "2026031701W300");
        num(&mut r, 73, 84, imss);
        num(&mut r, 85, 96, tot[6] + tot[7] + tot[8]);
        num(&mut r, 97, 108, tot[9]);
        num(&mut r, 109, 120, 0);
        pon(&mut r, 121, "1234567890123456789");
        salida.extend(r);

        assert_eq!(salida.len(), total_registros * LARGO_REGISTRO);
        salida
    }

    pub(crate) fn ejemplo() -> Vec<u8> {
        archivo(&[
            Fila { nss: "00000000019", sdi: 10_000, dias: 29, dias_bim: 60 },
            Fila { nss: "00000000027", sdi: 9_000, dias: 22, dias_bim: 43 },
        ])
    }

    #[test]
    fn base62_reproduce_los_valores_observados() {
        // Pares observados en archivos reales (Decompilacion_SUA.md, 14.4):
        // 100.00 de SDI por 29 días: 7.25, 10.88 y 18.13; CyV obrera 67.50.
        for (codigo, centavos) in [("0Bh", 725), ("0HY", 1088), ("0TF", 1813), ("1ks", 6750), ("000", 0), ("zzz", 238_327)] {
            assert_eq!(base62_a_centavos(codigo), Some(centavos), "{codigo}");
            assert_eq!(centavos_a_base62(centavos, 3).as_deref(), Some(codigo));
        }
        assert_eq!(base62_a_centavos("0-1"), None);
        assert_eq!(centavos_a_base62(238_328, 3), None);
        assert_eq!(centavos_a_base62(-1, 3), None);
    }

    #[test]
    fn lee_un_archivo_completo_y_todo_cuadra() {
        let a = leer(&ejemplo()).unwrap();
        assert_eq!(a.patron.registro_patronal, REG_PAT);
        assert_eq!(a.patron.periodo, "202602");
        assert_eq!(a.patron.prima_rt, 758_880);
        assert_eq!(a.patron.area_geografica, "A");
        assert_eq!(a.trabajadores.len(), 2);
        assert_eq!(a.movimientos.len(), 2);
        assert_eq!(a.movimientos[0].tipo, "00");
        assert_eq!(a.movimientos[0].fecha, "20260101");

        let t = &a.trabajadores[0];
        assert_eq!(t.nombre, "FICTICIO$DE PRUEBA$TRABAJADOR 1");
        assert_eq!((t.sdi, t.dias_mes, t.dias_bimestre), (10_000, 29, 60));
        assert_eq!(t.cuota_fija, 31_112); // 52.59 × 20.40 % × 29, como en la muestra real
        assert_eq!(t.prestaciones_dinero, 2_755);
        assert_eq!(t.prestaciones_dinero_obrero, Some(725));
        assert_eq!(t.gastos_medicos_obrero, Some(1_088));
        assert_eq!(t.invalidez_vida_obrero, Some(1_813));
        assert_eq!(t.cyv_obrera, 6_750);
        assert_eq!(t.cyv_obrera_codificada, Some(6_750));
        assert_eq!(t.aportacion_patronal, 30_000);
        assert_eq!(a.control.numero_control, "1234567890123456789");
        assert_eq!(a.control.marca, "W3");

        assert_eq!(a.verificar(), Vec::<String>::new());
    }

    #[test]
    fn detecta_importes_alterados_y_archivo_recortado() {
        let bueno = ejemplo();

        // Riesgos de trabajo del primer trabajador, alterado.
        let mut malo = bueno.clone();
        num(&mut malo[LARGO_REGISTRO..2 * LARGO_REGISTRO], 188, 194, 1);
        let h = leer(&malo).unwrap().verificar();
        assert!(h.iter().any(|m| m.contains("riesgos de trabajo")), "{h:?}");

        // CyV obrera codificada distinta de la que va en claro.
        let mut malo = bueno.clone();
        pon(&mut malo[LARGO_REGISTRO..2 * LARGO_REGISTRO], 290, "001");
        let h = leer(&malo).unwrap().verificar();
        assert!(h.iter().any(|m| m.contains("trabajador 1") && m.contains("CyV")), "{h:?}");

        // Se quita un trabajador: fallan cotizantes, tamaño, sumas y movimientos.
        let mut malo = bueno[..LARGO_REGISTRO].to_vec();
        malo.extend(&bueno[2 * LARGO_REGISTRO..]);
        let h = leer(&malo).unwrap().verificar();
        for esperado in ["cotizantes", "tamaño", "cuota fija", "NSS que no aparecen"] {
            assert!(h.iter().any(|m| m.contains(esperado)), "falta «{esperado}» en {h:?}");
        }

        assert_eq!(leer(&bueno[..bueno.len() - 1]), Err(ErrorPago::Tamano(bueno.len() - 1)));
        assert_eq!(leer(&[]), Err(ErrorPago::Vacio));
    }

    #[test]
    fn rechaza_estructura_y_campos_invalidos() {
        let bueno = ejemplo();

        let mut malo = bueno.clone();
        pon(&mut malo[LARGO_REGISTRO..], 1, "09");
        assert!(matches!(leer(&malo), Err(ErrorPago::TipoDesconocido { registro: 2, .. })));

        let mut malo = bueno.clone();
        pon(&mut malo[LARGO_REGISTRO..], 145, "00A0000");
        assert!(matches!(leer(&malo), Err(ErrorPago::CampoNumerico { registro: 2, campo: "SDI", .. })));

        let mut malo = bueno.clone();
        pon(&mut malo[LARGO_REGISTRO..], 281, "0$1");
        assert!(matches!(leer(&malo), Err(ErrorPago::Base62 { registro: 2, .. })));

        // Sin el registro 06.
        let malo = &bueno[..bueno.len() - LARGO_REGISTRO];
        assert!(matches!(leer(malo), Err(ErrorPago::Estructura(_))));

        // Un 03 después del 05.
        let mut malo = bueno.clone();
        malo.extend(bueno[LARGO_REGISTRO..2 * LARGO_REGISTRO].to_vec());
        assert!(matches!(leer(&malo), Err(ErrorPago::Estructura(_))));
    }

    #[test]
    fn campos_codificados_en_blanco_significan_ramo_no_incluido() {
        let mut datos = ejemplo();
        pon(&mut datos[LARGO_REGISTRO..2 * LARGO_REGISTRO], 278, "            ");
        let a = leer(&datos).unwrap();
        let t = &a.trabajadores[0];
        assert_eq!(t.excedente_obrero, None);
        assert_eq!(t.invalidez_vida_obrero, None);
        assert_eq!(t.cyv_obrera_codificada, Some(6_750));
    }

    /// Prueba contra archivos de pago reales. Solo corre si la variable de
    /// entorno `SUA_MUESTRAS` apunta a una carpeta con archivos `.SUA`; los
    /// archivos reales no forman parte del repositorio. No imprime su contenido.
    #[test]
    fn muestras_reales_si_estan_disponibles() {
        let Some(carpeta) = std::env::var_os("SUA_MUESTRAS") else { return };
        let mut pendientes = vec![std::path::PathBuf::from(carpeta)];
        let mut leidos = 0;
        while let Some(dir) = pendientes.pop() {
            for e in std::fs::read_dir(&dir).unwrap().flatten() {
                let ruta = e.path();
                if ruta.is_dir() {
                    pendientes.push(ruta);
                } else if ruta.extension().is_some_and(|x| x.eq_ignore_ascii_case("sua")) {
                    let nombre = ruta.file_name().unwrap().to_string_lossy().into_owned();
                    let a = leer(&std::fs::read(&ruta).unwrap()).unwrap_or_else(|e| panic!("{nombre}: {e}"));
                    assert_eq!(a.verificar(), Vec::<String>::new(), "{nombre}");
                    leidos += 1;
                }
            }
        }
        assert!(leidos > 0, "SUA_MUESTRAS no contiene archivos .SUA");
    }
}
