//! Movimientos de los trabajadores y cálculo de un patrón completo para un
//! periodo, por tramos de días.
//!
//! Comprobado con archivos de pago reales: importando el archivo y calculando
//! el periodo, los totales coinciden con los del archivo salvo un centavo en
//! un caso (`Decompilacion_SUA.md`, sección 15.5).

use super::Base;
use crate::dominio::calculo::{CuotasBimestre, CuotasMes};
use crate::dominio::extemporaneo::Extemporaneo;
use crate::dominio::credito::{creditos, validar_descuento, Estado, MovCredito};
use crate::dominio::tramos::{dia_de_iso, dias_del_mes, tramos, Dia, Incidencia, Movimiento, Periodo};
use crate::error::{ErrorSua, Resultado};
use rusqlite::params;

/// Tipos de movimiento que la captura acepta, con su descripción.
pub const TIPOS_DE_MOVIMIENTO: &[(&str, &str)] =
    &[("01", "Alta"), ("02", "Baja"), ("07", "Modificación de salario"), ("08", "Reingreso"), ("11", "Ausentismo"), ("12", "Incapacidad")];

/// Movimientos de crédito INFONAVIT, con su descripción.
pub const TIPOS_DE_CREDITO: &[(&str, &str)] = &[
    ("15", "Inicio de crédito INFONAVIT"),
    ("16", "Suspensión de descuento"),
    ("17", "Reinicio de descuento"),
    ("18", "Modificación de tipo de descuento"),
    ("19", "Modificación de valor de descuento"),
    ("20", "Modificación de número de crédito"),
];

fn es_de_credito(tipo: &str) -> bool {
    TIPOS_DE_CREDITO.iter().any(|(t, _)| *t == tipo)
}

/// Máximo de días de ausentismo por mes (desde el 21 de diciembre de 2001).
pub const MAX_AUSENTISMOS_POR_MES: u32 = 7;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovimientoGuardado {
    pub id: i64,
    pub tipo: String,
    /// `AAAA-MM-DD`
    pub fecha: String,
    pub sdi: i64,
    /// Días del ausentismo o de la incapacidad; 0 en los demás.
    pub dias: u32,
    /// Folio de la incapacidad; vacío en los demás.
    pub folio: String,
    /// Número de crédito INFONAVIT (movimientos 15 y 20).
    pub credito: String,
    /// 1 porcentaje, 2 cuota fija, 3 factor (movimientos 15 y 18); 0 en los demás.
    pub tipo_descuento: u8,
    /// Valor del descuento en diezmilésimas (movimientos 15, 18 y 19).
    pub valor_descuento: i64,
}

impl MovimientoGuardado {
    fn para_tramos(&self) -> Option<Movimiento> {
        Some(Movimiento { tipo: self.tipo.clone(), fecha: dia_de_iso(&self.fecha)?, sdi: self.sdi, dias: self.dias })
    }

    fn de_credito(&self) -> Option<MovCredito> {
        if !es_de_credito(&self.tipo) {
            return None;
        }
        Some(MovCredito {
            tipo: self.tipo.parse().ok()?,
            fecha: dia_de_iso(&self.fecha)?,
            numero: self.credito.clone(),
            tipo_descuento: self.tipo_descuento,
            valor: self.valor_descuento,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineaCalculo {
    pub nss: String,
    pub nombre: String,
    /// Salario diario integrado del último tramo con días en el periodo.
    pub sdi: i64,
    pub dias_mes: u32,
    pub dias_bimestre: u32,
    pub ausentismos_mes: u32,
    pub incapacidad_mes: u32,
    pub ausentismos_bimestre: u32,
    pub incapacidad_bimestre: u32,
    /// Número de crédito INFONAVIT con descuento en el bimestre; vacío si no hay.
    pub credito: String,
    pub mes: CuotasMes,
    pub bimestre: CuotasBimestre,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CalculoPeriodo {
    pub periodo: String,
    pub cierra_bimestre: bool,
    pub decreto: u8,
    /// Prima de riesgos de trabajo aplicada, en cienmilésimas; `None` si el
    /// patrón no tenía prima vigente en el periodo.
    pub prima_rt: Option<i64>,
    pub lineas: Vec<LineaCalculo>,
    pub mes: CuotasMes,
    pub bimestre: CuotasBimestre,
    /// Actualización y recargos, si se calculó con una fecha de pago posterior al vencimiento.
    pub extemporaneo: Option<Extemporaneo>,
    pub avisos: Vec<String>,
}

impl CalculoPeriodo {
    pub fn total(&self) -> i64 {
        self.cuotas() + self.extemporaneo.as_ref().map_or(0, Extemporaneo::total)
    }

    /// Cuotas, aportaciones y amortizaciones, sin actualización ni recargos.
    pub fn cuotas(&self) -> i64 {
        self.mes.total() + self.bimestre.total()
    }
}

impl Base {
    pub fn movimientos(&self, reg_pat: &str, nss: &str) -> Resultado<Vec<MovimientoGuardado>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, tip_movs, fec_inic, sal_movt, num_dias, fol_inc, num_cre, tip_des, val_des FROM movtos WHERE reg_patr = ?1 AND num_afil = ?2 ORDER BY fec_inic, tip_movs <> '02', id",
        )?;
        let filas = stmt.query_map([reg_pat, nss], |r| Ok(MovimientoGuardado { id: r.get(0)?, tipo: r.get(1)?, fecha: r.get(2)?, sdi: r.get(3)?, dias: r.get(4)?, folio: r.get(5)?, credito: r.get(6)?, tipo_descuento: r.get::<_, String>(7)?.trim().parse().unwrap_or(0), valor_descuento: r.get(8)? }))?;
        Ok(filas.collect::<Result<_, _>>()?)
    }

    /// Registra un movimiento afiliatorio. Valida el tipo, la fecha, el salario
    /// y la secuencia: no se da de baja a quien no está activo ni se da de alta
    /// o reingresa a quien ya lo está.
    pub fn guardar_movimiento(&self, reg_pat: &str, nss: &str, tipo: &str, fecha: &str, sdi: i64) -> Resultado<()> {
        self.con_permiso(crate::datos::usuarios::Permiso::Capturar, "Registró movimiento", || format!("{reg_pat} {nss} tipo {tipo} del {fecha}"), || self.guardar_movimiento_sin_control(reg_pat, nss, tipo, fecha, sdi))
    }

    fn guardar_movimiento_sin_control(&self, reg_pat: &str, nss: &str, tipo: &str, fecha: &str, sdi: i64) -> Resultado<()> {
        let mal = |m: &str| ErrorSua::Validacion(m.to_string());
        if !matches!(tipo, "01" | "02" | "07" | "08") {
            return Err(mal("Tipo de movimiento no admitido: use 01 alta, 02 baja, 07 modificación de salario u 08 reingreso."));
        }
        let fecha_dia = dia_de_iso(fecha).ok_or_else(|| mal("Fecha: use el formato AAAA-MM-DD."))?;
        if tipo == "02" {
            if sdi != 0 {
                return Err(mal("La baja no lleva salario."));
            }
        } else if sdi <= 0 || sdi > 9_999_999 {
            return Err(mal("Salario diario integrado: debe ser mayor que cero y no pasar de 99,999.99."));
        }
        let existe: i64 = self.conn.query_row("SELECT COUNT(*) FROM asegura WHERE reg_patr = ?1 AND num_afil = ?2", [reg_pat, nss], |r| r.get(0))?;
        if existe == 0 {
            return Err(mal("Elija primero un trabajador registrado."));
        }
        let previos = self.movimientos(reg_pat, nss)?;
        if previos.iter().any(|m| m.tipo == tipo && m.fecha == fecha) {
            return Err(mal("Ya existe un movimiento de ese tipo en esa fecha."));
        }
        // Estado del trabajador justo antes del movimiento nuevo.
        let anteriores: Vec<Movimiento> = previos
            .iter()
            .filter_map(MovimientoGuardado::para_tramos)
            .filter(|m| !matches!(m.tipo.as_str(), "11" | "12") && !es_de_credito(&m.tipo))
            .filter(|m| m.fecha < fecha_dia || (m.fecha == fecha_dia && m.tipo == "02"))
            .collect();
        // Un tramo abierto llega hasta el día centinela (el siguiente); uno cerrado por baja, no.
        let activo = tramos(&anteriores, fecha_dia + 1).tramos.last().is_some_and(|t| t.hasta > fecha_dia);
        match (tipo, activo) {
            ("02", false) => return Err(mal("No se puede dar de baja: el trabajador no está activo en esa fecha.")),
            ("07", false) => return Err(mal("No se puede modificar el salario: el trabajador no está activo en esa fecha.")),
            ("01" | "08", true) => return Err(mal("El trabajador ya está activo en esa fecha; registre antes la baja.")),
            _ => {}
        }
        if tipo == "02" {
            let incapacitado = previos.iter().filter(|m| m.tipo == "12").filter_map(|m| Some((dia_de_iso(&m.fecha)?, m.dias))).any(|(ini, dias)| (ini..ini + Dia::from(dias)).contains(&fecha_dia));
            if incapacitado {
                return Err(mal("No se puede dar de baja durante una incapacidad."));
            }
        }
        let tx = self.transaccion()?;
        tx.execute(
            "INSERT INTO movtos (reg_patr, num_afil, tip_movs, fec_inic, sal_movt) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![reg_pat, nss, tipo, fecha, sdi],
        )?;
        // El salario vigente del trabajador sigue al último movimiento con salario.
        tx.execute(
            "UPDATE asegura SET sal_imss = COALESCE((SELECT sal_movt FROM movtos WHERE reg_patr = ?1 AND num_afil = ?2 AND sal_movt > 0
                                                     ORDER BY fec_inic DESC, id DESC LIMIT 1), sal_imss)
             WHERE reg_patr = ?1 AND num_afil = ?2",
            [reg_pat, nss],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Registra un ausentismo (`11`) o una incapacidad (`12`) de `dias` días
    /// contados desde `fecha`. El trabajador debe estar activo todos esos
    /// días y no tener otra incidencia en ellos. Un ausentismo no cruza de
    /// mes y no puede pasar de siete días en el mes; la incapacidad lleva
    /// folio de dos letras y seis números, sin repetir.
    pub fn guardar_incidencia(&self, reg_pat: &str, nss: &str, tipo: &str, fecha: &str, dias: u32, folio: &str) -> Resultado<()> {
        self.con_permiso(crate::datos::usuarios::Permiso::Capturar, "Registró ausentismo o incapacidad", || format!("{reg_pat} {nss} tipo {tipo} del {fecha}"), || self.guardar_incidencia_sin_control(reg_pat, nss, tipo, fecha, dias, folio))
    }

    fn guardar_incidencia_sin_control(&self, reg_pat: &str, nss: &str, tipo: &str, fecha: &str, dias: u32, folio: &str) -> Resultado<()> {
        let mal = |m: &str| ErrorSua::Validacion(m.to_string());
        let ausentismo = match tipo {
            "11" => true,
            "12" => false,
            _ => return Err(mal("Tipo de incidencia no admitido: use 11 ausentismo o 12 incapacidad.")),
        };
        let inicio = dia_de_iso(fecha).ok_or_else(|| mal("Fecha: use el formato AAAA-MM-DD."))?;
        let folio = folio.trim().to_uppercase();
        if ausentismo {
            if !(1..=MAX_AUSENTISMOS_POR_MES).contains(&dias) {
                return Err(mal("Días de ausentismo: escriba un número entre 1 y 7."));
            }
            if !folio.is_empty() {
                return Err(mal("El ausentismo no lleva folio."));
            }
        } else {
            if !(1..=99).contains(&dias) {
                return Err(mal("Días de incapacidad: escriba un número entre 1 y 99."));
            }
            let b = folio.as_bytes();
            if !(b.len() == 8 && b[..2].iter().all(u8::is_ascii_uppercase) && b[2..].iter().all(u8::is_ascii_digit)) {
                return Err(mal("Folio de incapacidad: dos letras y seis números, como AB123456."));
            }
        }
        let fin = inicio + Dia::from(dias) - 1;
        let existe: i64 = self.conn.query_row("SELECT COUNT(*) FROM asegura WHERE reg_patr = ?1 AND num_afil = ?2", [reg_pat, nss], |r| r.get(0))?;
        if existe == 0 {
            return Err(mal("Elija primero un trabajador registrado."));
        }
        let previos = self.movimientos(reg_pat, nss)?;
        let estado = tramos(&previos.iter().filter_map(MovimientoGuardado::para_tramos).collect::<Vec<_>>(), fin);
        if !(inicio..=fin).all(|d| estado.activo_el(d)) {
            return Err(mal("El trabajador no está activo todos esos días."));
        }
        if (inicio..=fin).any(|d| estado.incidencia_del_dia(d).is_some()) {
            return Err(mal("Ya hay un ausentismo o una incapacidad en esos días."));
        }
        if ausentismo {
            // `fecha` ya pasó por `dia_de_iso`: tiene la forma AAAA-MM-DD.
            let (anio, mes): (i64, i64) = (fecha[..4].parse().unwrap_or(0), fecha[5..7].parse().unwrap_or(0));
            let dia_del_mes: i64 = fecha[8..].parse().unwrap_or(0);
            if dia_del_mes + i64::from(dias) - 1 > dias_del_mes(anio, mes) {
                return Err(mal("El ausentismo no puede pasar al mes siguiente: registre uno por cada mes."));
            }
            let primero = inicio - (dia_del_mes - 1);
            let ya = (primero..primero + dias_del_mes(anio, mes)).filter(|d| estado.incidencia_del_dia(*d) == Some(Incidencia::Ausentismo)).count() as u32;
            if ya + dias > MAX_AUSENTISMOS_POR_MES {
                return Err(mal(&format!("Con este serían {} días de ausentismo en el mes; el máximo es {MAX_AUSENTISMOS_POR_MES}.", ya + dias)));
            }
        } else if previos.iter().any(|m| m.tipo == "12" && m.folio == folio) {
            return Err(mal("Ya existe una incapacidad con ese folio."));
        }
        self.conn.execute(
            "INSERT INTO movtos (reg_patr, num_afil, tip_movs, fec_inic, num_dias, fol_inc) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![reg_pat, nss, tipo, fecha, dias, folio],
        )?;
        Ok(())
    }

    /// Registra un movimiento de crédito INFONAVIT (`15` a `20`). `valor` va
    /// en diezmilésimas ([`crate::dominio::credito::texto_a_valor`]). Cada
    /// movimiento usa solo los datos que le tocan: el inicio, número, tipo y
    /// valor; la modificación de tipo, tipo y valor; la de valor, el valor;
    /// la de número, el número. No valida el dígito verificador del crédito.
    pub fn guardar_credito(&self, reg_pat: &str, nss: &str, tipo: &str, fecha: &str, numero: &str, tipo_descuento: u8, valor: i64) -> Resultado<()> {
        self.con_permiso(crate::datos::usuarios::Permiso::Capturar, "Registró movimiento de crédito", || format!("{reg_pat} {nss} tipo {tipo} del {fecha}"), || self.guardar_credito_sin_control(reg_pat, nss, tipo, fecha, numero, tipo_descuento, valor))
    }

    fn guardar_credito_sin_control(&self, reg_pat: &str, nss: &str, tipo: &str, fecha: &str, numero: &str, tipo_descuento: u8, valor: i64) -> Resultado<()> {
        let mal = |m: &str| ErrorSua::Validacion(m.to_string());
        if !es_de_credito(tipo) {
            return Err(mal("Tipo de movimiento de crédito no admitido: use 15 a 20."));
        }
        let dia = dia_de_iso(fecha).ok_or_else(|| mal("Fecha: use el formato AAAA-MM-DD."))?;
        let existe: i64 = self.conn.query_row("SELECT COUNT(*) FROM asegura WHERE reg_patr = ?1 AND num_afil = ?2", [reg_pat, nss], |r| r.get(0))?;
        if existe == 0 {
            return Err(mal("Elija primero un trabajador registrado."));
        }
        let previos = self.movimientos(reg_pat, nss)?;
        let de_credito: Vec<MovCredito> = previos.iter().filter_map(MovimientoGuardado::de_credito).collect();
        if de_credito.iter().any(|m| m.fecha > dia) {
            return Err(mal("Hay movimientos de crédito posteriores a esa fecha: regístrelos en orden."));
        }
        if previos.iter().any(|m| m.tipo == tipo && m.fecha == fecha) {
            return Err(mal("Ya existe un movimiento de ese tipo en esa fecha."));
        }
        let estado = creditos(&de_credito, dia);
        let numero = numero.trim();
        let numero_valido = || {
            if crate::dominio::validar::credito_infonavit(numero).formato_valido {
                Ok(())
            } else {
                Err(mal("Número de crédito: deben ser 10 dígitos, distintos de cero."))
            }
        };
        let descuento_valido = |td: u8| validar_descuento(td, valor).map_err(mal);
        let (numero, tipo_descuento, valor) = match (tipo, estado.estado) {
            ("15", Estado::Activo) => return Err(mal("El trabajador ya tiene un crédito con descuento activo; registre antes la suspensión.")),
            ("15", _) => {
                numero_valido()?;
                descuento_valido(tipo_descuento)?;
                (numero, tipo_descuento, valor)
            }
            ("16", Estado::Activo) => ("", 0, 0),
            ("16", _) => return Err(mal("No hay un descuento activo que suspender en esa fecha.")),
            ("17", Estado::Suspendido) => ("", 0, 0),
            ("17", _) => return Err(mal("Solo se reinicia un descuento suspendido.")),
            (_, Estado::SinCredito) => return Err(mal("El trabajador no tiene crédito en esa fecha; registre antes el inicio.")),
            ("18", _) => {
                descuento_valido(tipo_descuento)?;
                ("", tipo_descuento, valor)
            }
            ("19", _) => {
                descuento_valido(estado.vigente.as_ref().map_or(0, |d| d.tipo_descuento))?;
                ("", 0, valor)
            }
            _ => {
                numero_valido()?;
                (numero, 0, 0)
            }
        };
        if matches!(tipo, "15" | "17") {
            let empleo = tramos(&previos.iter().filter_map(MovimientoGuardado::para_tramos).collect::<Vec<_>>(), dia);
            if !empleo.activo_el(dia) {
                return Err(mal("El trabajador no está activo en esa fecha."));
            }
        }
        let tx = self.transaccion()?;
        tx.execute(
            "INSERT INTO movtos (reg_patr, num_afil, tip_movs, fec_inic, num_cre, tip_des, val_des) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![reg_pat, nss, tipo, fecha, numero, if tipo_descuento == 0 { String::new() } else { tipo_descuento.to_string() }, valor],
        )?;
        if !numero.is_empty() {
            tx.execute("UPDATE asegura SET pag_info = ?3 WHERE reg_patr = ?1 AND num_afil = ?2", [reg_pat, nss, numero])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn eliminar_movimiento(&self, id: i64) -> Resultado<()> {
        self.con_permiso(crate::datos::usuarios::Permiso::Capturar, "Eliminó movimiento", || format!("número {id}"), || self.eliminar_movimiento_sin_control(id))
    }

    fn eliminar_movimiento_sin_control(&self, id: i64) -> Resultado<()> {
        // Con una incapacidad se van sus datos médicos.
        self.conn.execute(
            "DELETE FROM incapacidades WHERE (reg_pat, num_afi, fol_inc) IN (SELECT reg_patr, num_afil, fol_inc FROM movtos WHERE id = ?1 AND tip_movs = '12')",
            [id],
        )?;
        self.conn.execute("DELETE FROM movtos WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Calcula las cuotas de todos los trabajadores de un patrón en el periodo
    /// `AAAAMM`. En los meses pares incluye RCV e INFONAVIT del bimestre.
    ///
    /// Un trabajador sin ningún movimiento se toma como activo desde su fecha
    /// de alta (o todo el periodo si no la tiene) con su salario registrado.
    pub fn calcular_periodo(&self, reg_pat: &str, periodo: &str, aplicar_decreto: bool) -> Resultado<CalculoPeriodo> {
        self.calcular_periodo_con_pago(reg_pat, periodo, aplicar_decreto, None)
    }

    /// Como [`Base::calcular_periodo`]; si `fecha_pago` (`AAAA-MM-DD`) es
    /// posterior al vencimiento, añade actualización y recargos, calculados
    /// por trabajador y por separado para la parte patronal y la obrera. El
    /// seguro de daños a la vivienda no lleva actualización ni recargos.
    pub fn calcular_periodo_con_pago(&self, reg_pat: &str, periodo: &str, aplicar_decreto: bool, fecha_pago: Option<&str>) -> Resultado<CalculoPeriodo> {
        let p = Periodo::de_texto(periodo).ok_or_else(|| ErrorSua::Validacion(format!("Periodo «{periodo}» inválido: se espera AAAAMM.")))?;
        // La prima vigente es la más reciente que no sea posterior al periodo.
        let prima: Option<i64> = self
            .conn
            .query_row(
                "SELECT prima_cienmilesimas FROM patron_prima_rt WHERE reg_pat = ?1 AND vigente_desde <= ?2 ORDER BY vigente_desde DESC LIMIT 1",
                params![reg_pat, format!("{}-{}", &periodo[..4], &periodo[4..])],
                |r| r.get(0),
            )
            .ok();
        let mut r = CalculoPeriodo { periodo: periodo.to_string(), cierra_bimestre: p.cierra_bimestre(), prima_rt: prima, ..CalculoPeriodo::default() };
        if prima.is_none() {
            r.avisos.push("El patrón no tiene prima de riesgos de trabajo vigente en el periodo: ese ramo sale en cero.".into());
        }
        let v = self.tasas(periodo, prima.unwrap_or(0), aplicar_decreto)?;
        r.decreto = v.tasas.descuento_decreto;
        if v.desactualizado {
            r.avisos.push("El catálogo de salario mínimo o UMA no tiene valores para este año: el resultado no es el vigente.".into());
        }
        let (mes, bim) = (p.mes_completo(), p.bimestre());
        let (mut sin_movimientos, mut no_aplicados, mut con_ausentismos_de_mas, mut con_factor) = (0, 0, 0, 0);
        // Para los créditos con factor de descuento: UMI del año, o salario mínimo antes de que existiera.
        let anio: u32 = periodo[..4].parse().unwrap_or(0);
        let (unidad_de_factor, umi_desactualizada) = self.umi(anio)?.unwrap_or((v.tasas.salario_minimo, false));
        let seguro_vivienda = self.seguro_vivienda(periodo)?;
        let dias_del_bimestre = (bim.1 - bim.0 + 1) as u32;

        for t in self.trabajadores(reg_pat, "")? {
            let guardados = self.movimientos(reg_pat, &t.num_afil)?;
            let mut movs: Vec<Movimiento> = guardados.iter().filter_map(MovimientoGuardado::para_tramos).collect();
            if !movs.iter().any(|m| !matches!(m.tipo.as_str(), "11" | "12") && !es_de_credito(&m.tipo)) {
                sin_movimientos += 1;
                movs.push(Movimiento { tipo: "01".into(), fecha: dia_de_iso(&t.fec_alt).unwrap_or(bim.0), sdi: t.sal_imss, dias: 0 });
            }
            let ts = tramos(&movs, bim.1);
            no_aplicados += ts.no_aplicados;
            let mut linea = LineaCalculo {
                nss: t.num_afil.clone(),
                // «PATERNO$MATERNO$NOMBRE»; sin apellido materno no debe quedar un espacio doble.
                nombre: t.nom_aseg.split('$').map(str::trim).filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" "),
                sdi: 0,
                dias_mes: 0,
                dias_bimestre: 0,
                ausentismos_mes: 0,
                incapacidad_mes: 0,
                ausentismos_bimestre: 0,
                incapacidad_bimestre: 0,
                credito: String::new(),
                mes: CuotasMes::default(),
                bimestre: CuotasBimestre::default(),
            };
            for tramo in &ts.tramos {
                let d = ts.dias_cotizados(tramo, mes);
                if d.naturales > 0 {
                    linea.sdi = tramo.sdi;
                    linea.dias_mes += d.naturales;
                    linea.ausentismos_mes += d.ausentismo;
                    linea.incapacidad_mes += d.incapacidad;
                    linea.mes.sumar(&v.tasas.mes_con(tramo.sdi, d));
                }
                let d = ts.dias_cotizados(tramo, bim);
                if d.naturales > 0 && p.cierra_bimestre() {
                    linea.sdi = tramo.sdi;
                    linea.dias_bimestre += d.naturales;
                    linea.ausentismos_bimestre += d.ausentismo;
                    linea.incapacidad_bimestre += d.incapacidad;
                    linea.bimestre.sumar(&v.tasas.bimestre_con(tramo.sdi, d));
                }
            }
            // Amortización de créditos INFONAVIT: por cada tramo de salario y cada
            // tramo de crédito, los días con descuento sin ausentismos ni incapacidades.
            let de_credito: Vec<MovCredito> = guardados.iter().filter_map(MovimientoGuardado::de_credito).collect();
            if p.cierra_bimestre() && !de_credito.is_empty() {
                let cs = creditos(&de_credito, bim.1);
                no_aplicados += cs.no_aplicados;
                for tramo in &ts.tramos {
                    for c in &cs.tramos {
                        let rango = (bim.0.max(c.desde), bim.1.min(c.hasta));
                        let dias = ts.dias_cotizados(tramo, rango).otros_ramos();
                        if dias == 0 {
                            continue;
                        }
                        let d = &c.descuento;
                        if d.tipo_descuento == 3 {
                            con_factor += 1;
                        }
                        linea.bimestre.amortizacion += v.tasas.amortizacion(d.tipo_descuento, d.valor, tramo.sdi, dias, dias_del_bimestre, unidad_de_factor);
                        linea.credito = d.numero.clone();
                    }
                }
                if linea.bimestre.amortizacion > 0 {
                    linea.bimestre.seguro_vivienda = seguro_vivienda;
                }
            }
            if linea.dias_mes + linea.dias_bimestre == 0 {
                continue;
            }
            if linea.ausentismos_mes > MAX_AUSENTISMOS_POR_MES {
                con_ausentismos_de_mas += 1;
            }
            r.mes.sumar(&linea.mes);
            r.bimestre.sumar(&linea.bimestre);
            r.lineas.push(linea);
        }
        if sin_movimientos > 0 {
            r.avisos.push(format!("{sin_movimientos} trabajadores no tienen movimientos: se calcularon con su fecha de alta y su salario registrado."));
        }
        if no_aplicados > 0 {
            r.avisos.push(format!("{no_aplicados} movimientos no se aplicaron (de un tipo no manejado o fuera de secuencia)."));
        }
        if con_factor > 0 && umi_desactualizada {
            r.avisos.push("El catálogo de la UMI no tiene valor para este año: los créditos con factor de descuento se calcularon con el último valor registrado.".into());
        }
        if con_ausentismos_de_mas > 0 {
            r.avisos.push(format!("{con_ausentismos_de_mas} trabajadores tienen más de {MAX_AUSENTISMOS_POR_MES} días de ausentismo en el mes: se descontaron todos, pero el SUA no admite más de {MAX_AUSENTISMOS_POR_MES}."));
        }
        if let Some(fecha) = fecha_pago.map(str::trim).filter(|f| !f.is_empty()) {
            if let Some((mut e, avisos)) = self.extemporaneo(periodo, fecha)? {
                let suma = |importes: &[i64]| importes.iter().fold((0, 0), |(a, r), i| {
                    let (x, y) = e.cargos(*i);
                    (a + x, r + y)
                });
                let (mut imss, mut rcv, mut infonavit) = ((0, 0), (0, 0), (0, 0));
                for l in &r.lineas {
                    let (b, x) = (&l.bimestre, suma(&[l.mes.patronal(), l.mes.obrero()]));
                    imss = (imss.0 + x.0, imss.1 + x.1);
                    let x = suma(&[b.retiro, b.cyv_patronal, b.cyv_obrero]);
                    rcv = (rcv.0 + x.0, rcv.1 + x.1);
                    let x = suma(&[b.infonavit, b.amortizacion]);
                    infonavit = (infonavit.0 + x.0, infonavit.1 + x.1);
                }
                (e.actualizacion_imss, e.recargos_imss) = imss;
                (e.actualizacion_rcv, e.recargos_rcv) = rcv;
                (e.actualizacion_infonavit, e.recargos_infonavit) = infonavit;
                r.avisos.extend(avisos);
                r.extemporaneo = Some(e);
            }
        }
        Ok(r)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::config::{CarpetaTemporal, Modo};
    use crate::datos::captura::{Patron, Trabajador};
    use crate::dominio::validar::luhn;

    const REG: &str = "Z9900000109";

    fn base(t: &CarpetaTemporal) -> Base {
        let b = Base::abrir_en(&t.0.join("sua.db"), &t.0.join("respaldos"), Modo::Portable, 3).unwrap();
        b.guardar_patron(&Patron { reg_pat: REG.into(), rfc_pat: "XAXX010101000".into(), nom_pat: "PATRON FICTICIO".into(), ..Patron::default() })
            .unwrap();
        b
    }

    fn alta(b: &Base, n: u64, sdi: i64, fec_alt: &str) -> String {
        let base10 = format!("{:010}", 9_000_000_000u64 + n);
        let nss = format!("{base10}{}", luhn(&base10).unwrap());
        b.guardar_trabajador(&Trabajador {
            reg_patr: REG.into(),
            num_afil: nss.clone(),
            nom_aseg: format!("FICTICIO$$TRABAJADOR {n}"),
            sal_imss: sdi,
            fec_alt: fec_alt.into(),
            ..Trabajador::default()
        })
        .unwrap();
        nss
    }

    #[test]
    fn captura_de_movimientos_valida_la_secuencia() {
        let t = CarpetaTemporal::nueva("per1");
        let b = base(&t);
        let nss = alta(&b, 1, 30_000, "");
        let rechazado = |r: Resultado<()>, texto: &str| match r {
            Err(ErrorSua::Validacion(m)) => assert!(m.contains(texto), "{m}"),
            otro => panic!("se esperaba rechazo con «{texto}», se obtuvo {otro:?}"),
        };
        rechazado(b.guardar_movimiento(REG, &nss, "02", "2026-01-10", 0), "no está activo");
        rechazado(b.guardar_movimiento(REG, &nss, "07", "2026-01-10", 35_000), "no está activo");
        rechazado(b.guardar_movimiento(REG, &nss, "99", "2026-01-10", 35_000), "Tipo de movimiento");
        rechazado(b.guardar_movimiento(REG, &nss, "01", "10/01/2026", 35_000), "Fecha");
        rechazado(b.guardar_movimiento(REG, &nss, "01", "2026-01-10", 0), "Salario");
        rechazado(b.guardar_movimiento(REG, "00000000000", "01", "2026-01-10", 30_000), "trabajador registrado");

        b.guardar_movimiento(REG, &nss, "01", "2026-01-10", 30_000).unwrap();
        rechazado(b.guardar_movimiento(REG, &nss, "01", "2026-01-10", 30_000), "Ya existe");
        rechazado(b.guardar_movimiento(REG, &nss, "08", "2026-01-20", 30_000), "ya está activo");
        rechazado(b.guardar_movimiento(REG, &nss, "02", "2026-01-20", 100), "no lleva salario");
        b.guardar_movimiento(REG, &nss, "07", "2026-01-16", 35_000).unwrap();
        b.guardar_movimiento(REG, &nss, "02", "2026-01-20", 0).unwrap();
        rechazado(b.guardar_movimiento(REG, &nss, "02", "2026-01-25", 0), "no está activo");
        // Reingreso el mismo día de la baja: permitido.
        b.guardar_movimiento(REG, &nss, "08", "2026-01-20", 40_000).unwrap();

        let m = b.movimientos(REG, &nss).unwrap();
        assert_eq!(m.iter().map(|x| x.tipo.as_str()).collect::<Vec<_>>(), ["01", "07", "02", "08"]);
        // El salario vigente del trabajador sigue al último movimiento.
        assert_eq!(b.trabajadores(REG, "").unwrap()[0].sal_imss, 40_000);
        b.eliminar_movimiento(m[3].id).unwrap();
        assert_eq!(b.movimientos(REG, &nss).unwrap().len(), 3);
    }

    #[test]
    fn calculo_de_un_patron_por_tramos() {
        let t = CarpetaTemporal::nueva("per2");
        let b = base(&t);
        b.guardar_prima_rt(REG, "2007-03", 758_875).unwrap();
        // Tres casos con días e importes iguales a los de un archivo real de febrero de 2008.
        let a = alta(&b, 1, 10_000, "");
        b.guardar_movimiento(REG, &a, "01", "2008-01-01", 10_000).unwrap();
        let c = alta(&b, 2, 9_000, "");
        b.guardar_movimiento(REG, &c, "01", "2008-01-11", 9_000).unwrap();
        b.guardar_movimiento(REG, &c, "02", "2008-02-18", 0).unwrap();
        b.guardar_movimiento(REG, &c, "08", "2008-02-26", 9_000).unwrap();
        // Sin movimientos: se usa su fecha de alta.
        alta(&b, 3, 9_000, "2008-02-12");
        // Dado de baja antes del bimestre: no aparece.
        let d = alta(&b, 4, 9_000, "");
        b.guardar_movimiento(REG, &d, "01", "2007-11-01", 9_000).unwrap();
        b.guardar_movimiento(REG, &d, "02", "2007-12-15", 0).unwrap();

        let r = b.calcular_periodo(REG, "200802", false).unwrap();
        assert!(r.cierra_bimestre);
        assert_eq!(r.lineas.len(), 3);
        let l = &r.lineas[0];
        assert_eq!((l.dias_mes, l.dias_bimestre, l.sdi), (29, 60, 10_000));
        assert_eq!(r.prima_rt, Some(758_875));
        assert_eq!((l.mes.cuota_fija, l.mes.riesgos_trabajo, l.mes.guarderias), (31_112, 22_007, 2_900));
        assert_eq!((l.bimestre.retiro, l.bimestre.cyv_patronal, l.bimestre.cyv_obrero, l.bimestre.infonavit), (12_000, 18_900, 6_750, 30_000));
        let l = &r.lineas[1];
        assert_eq!((l.dias_mes, l.dias_bimestre), (22, 43));
        // Importes del archivo real para SDI 90.00 con 22 y 43 días.
        assert_eq!((l.mes.cuota_fija, l.mes.gmp_patronal + l.mes.gmp_obrero, l.bimestre.retiro, l.bimestre.cyv_patronal), (23_602, 2_822, 7_740, 12_191));
        let l = &r.lineas[2];
        assert_eq!((l.dias_mes, l.dias_bimestre), (18, 18));
        assert_eq!(r.mes.cuota_fija, r.lineas.iter().map(|x| x.mes.cuota_fija).sum::<i64>());
        assert_eq!(r.total(), r.mes.total() + r.bimestre.total());
        assert!(r.avisos.iter().any(|a| a.contains("1 trabajadores no tienen movimientos")));

        // Enero no cierra bimestre: solo IMSS.
        let r = b.calcular_periodo(REG, "200801", false).unwrap();
        assert!(!r.cierra_bimestre);
        assert_eq!(r.bimestre, CuotasBimestre::default());
        assert_eq!((r.lineas[0].dias_mes, r.lineas[1].dias_mes), (31, 21));
        // Antes de la vigencia de la prima: se avisa y riesgos de trabajo sale en cero.
        let r = b.calcular_periodo(REG, "200702", false).unwrap();
        assert!(r.avisos.iter().any(|a| a.contains("prima")));
        assert!(matches!(b.calcular_periodo(REG, "2008", false), Err(ErrorSua::Validacion(_))));
    }

    #[test]
    fn captura_de_ausentismos_e_incapacidades() {
        let t = CarpetaTemporal::nueva("per4");
        let b = base(&t);
        let nss = alta(&b, 1, 30_000, "");
        let rechazado = |r: Resultado<()>, texto: &str| match r {
            Err(ErrorSua::Validacion(m)) => assert!(m.contains(texto), "{m}"),
            otro => panic!("se esperaba rechazo con «{texto}», se obtuvo {otro:?}"),
        };
        rechazado(b.guardar_incidencia(REG, &nss, "11", "2026-01-12", 1, ""), "no está activo");
        b.guardar_movimiento(REG, &nss, "01", "2026-01-05", 30_000).unwrap();
        rechazado(b.guardar_incidencia(REG, &nss, "07", "2026-01-12", 1, ""), "Tipo de incidencia");
        rechazado(b.guardar_incidencia(REG, &nss, "11", "12/01/2026", 1, ""), "Fecha");
        rechazado(b.guardar_incidencia(REG, &nss, "11", "2026-01-12", 0, ""), "entre 1 y 7");
        rechazado(b.guardar_incidencia(REG, &nss, "11", "2026-01-12", 8, ""), "entre 1 y 7");
        rechazado(b.guardar_incidencia(REG, &nss, "11", "2026-01-12", 1, "AB123456"), "no lleva folio");
        rechazado(b.guardar_incidencia(REG, &nss, "11", "2026-01-03", 3, ""), "no está activo");
        rechazado(b.guardar_incidencia(REG, &nss, "11", "2026-01-30", 3, ""), "mes siguiente");
        rechazado(b.guardar_incidencia(REG, &nss, "12", "2026-01-20", 3, "A1234567"), "Folio");
        rechazado(b.guardar_incidencia(REG, &nss, "12", "2026-01-20", 100, "AB123456"), "entre 1 y 99");
        rechazado(b.guardar_incidencia(REG, "00000000000", "11", "2026-01-12", 1, ""), "trabajador registrado");

        b.guardar_incidencia(REG, &nss, "11", "2026-01-12", 4, "").unwrap();
        rechazado(b.guardar_incidencia(REG, &nss, "11", "2026-01-15", 1, ""), "Ya hay");
        rechazado(b.guardar_incidencia(REG, &nss, "11", "2026-01-20", 4, ""), "serían 8 días");
        b.guardar_incidencia(REG, &nss, "11", "2026-01-20", 3, "").unwrap();
        // El límite es por mes: en febrero se puede otra vez.
        b.guardar_incidencia(REG, &nss, "11", "2026-02-02", 7, "").unwrap();
        // La incapacidad sí cruza de mes; el folio se guarda en mayúsculas y no se repite.
        b.guardar_incidencia(REG, &nss, "12", "2026-02-25", 10, "ab123456").unwrap();
        rechazado(b.guardar_incidencia(REG, &nss, "12", "2026-03-20", 2, "AB123456"), "ese folio");
        rechazado(b.guardar_incidencia(REG, &nss, "12", "2026-03-05", 2, "AB123457"), "Ya hay");
        rechazado(b.guardar_movimiento(REG, &nss, "02", "2026-03-02", 0), "durante una incapacidad");
        b.guardar_movimiento(REG, &nss, "02", "2026-03-10", 0).unwrap();
        rechazado(b.guardar_incidencia(REG, &nss, "12", "2026-03-09", 3, "AB123458"), "no está activo");

        let m = b.movimientos(REG, &nss).unwrap();
        let inc = m.iter().find(|x| x.tipo == "12").unwrap();
        assert_eq!((inc.dias, inc.folio.as_str(), inc.sdi), (10, "AB123456", 0));
        // El salario vigente no cambia por las incidencias.
        assert_eq!(b.trabajadores(REG, "").unwrap()[0].sal_imss, 30_000);
    }

    #[test]
    fn el_calculo_descuenta_ausentismos_e_incapacidades() {
        let t = CarpetaTemporal::nueva("per5");
        let b = base(&t);
        b.guardar_prima_rt(REG, "2007-03", 758_875).unwrap();
        let nss = alta(&b, 1, 10_000, "");
        b.guardar_movimiento(REG, &nss, "01", "2008-01-01", 10_000).unwrap();
        let limpio = b.calcular_periodo(REG, "200802", false).unwrap();
        // Dos ausentismos en febrero y una incapacidad de 6 días: 2 de enero y 4 de febrero.
        b.guardar_incidencia(REG, &nss, "11", "2008-02-20", 2, "").unwrap();
        b.guardar_incidencia(REG, &nss, "12", "2008-01-30", 6, "XY000001").unwrap();
        let r = b.calcular_periodo(REG, "200802", false).unwrap();
        let (l, antes) = (&r.lineas[0], &limpio.lineas[0]);
        assert_eq!((l.dias_mes, l.ausentismos_mes, l.incapacidad_mes), (29, 2, 4));
        assert_eq!((l.dias_bimestre, l.ausentismos_bimestre, l.incapacidad_bimestre), (60, 2, 6));
        // SDI 100.00 en febrero de 2008. Enfermedades: 25 días; los demás ramos: 23.
        // Cuota fija = 52.59 × 25 × 20.40 %; guarderías = 100 × 23 × 1 %.
        assert_eq!((l.mes.cuota_fija, l.mes.pd_patronal, l.mes.guarderias, l.mes.iv_obrero), (26_821, 1_750, 2_300, 1_438));
        // Retiro e INFONAVIT: 58 días; cesantía y vejez: 52.
        assert_eq!((l.bimestre.retiro, l.bimestre.infonavit, l.bimestre.cyv_patronal, l.bimestre.cyv_obrero), (11_600, 29_000, 16_380, 5_850));
        assert!(l.mes.total() < antes.mes.total() && r.total() == l.mes.total() + l.bimestre.total());
        assert!(r.avisos.iter().all(|a| !a.contains("no se aplicaron")));

        // Enero: solo cuentan los 2 días de incapacidad de ese mes.
        let enero = b.calcular_periodo(REG, "200801", false).unwrap();
        assert_eq!((enero.lineas[0].dias_mes, enero.lineas[0].ausentismos_mes, enero.lineas[0].incapacidad_mes), (31, 0, 2));
        // Un trabajador con incidencias pero sin altas se sigue calculando desde su fecha de alta.
        let otro = alta(&b, 2, 9_000, "2008-02-01");
        b.conexion()
            .execute("INSERT INTO movtos (reg_patr, num_afil, tip_movs, fec_inic, num_dias) VALUES (?1, ?2, '11', '2008-02-05', 9)", [REG, otro.as_str()])
            .unwrap();
        let r = b.calcular_periodo(REG, "200802", false).unwrap();
        assert_eq!((r.lineas[1].dias_mes, r.lineas[1].ausentismos_mes), (29, 9));
        assert!(r.avisos.iter().any(|a| a.contains("más de 7 días de ausentismo")));
    }

    #[test]
    fn captura_de_creditos_infonavit() {
        let t = CarpetaTemporal::nueva("per6");
        let b = base(&t);
        let nss = alta(&b, 1, 40_000, "");
        let rechazado = |r: Resultado<()>, texto: &str| match r {
            Err(ErrorSua::Validacion(m)) => assert!(m.contains(texto), "{m}"),
            otro => panic!("se esperaba rechazo con «{texto}», se obtuvo {otro:?}"),
        };
        const CRED: &str = "0912345678";
        rechazado(b.guardar_credito(REG, &nss, "15", "2026-01-01", CRED, 1, 250_000), "no está activo");
        b.guardar_movimiento(REG, &nss, "01", "2026-01-01", 40_000).unwrap();
        rechazado(b.guardar_credito(REG, &nss, "01", "2026-01-01", CRED, 1, 250_000), "15 a 20");
        rechazado(b.guardar_credito(REG, &nss, "15", "01/01/2026", CRED, 1, 250_000), "Fecha");
        rechazado(b.guardar_credito(REG, &nss, "15", "2026-01-01", "123", 1, 250_000), "10 dígitos");
        rechazado(b.guardar_credito(REG, &nss, "15", "2026-01-01", CRED, 4, 250_000), "Tipo de descuento");
        rechazado(b.guardar_credito(REG, &nss, "15", "2026-01-01", CRED, 1, 1_000_000), "Porcentaje");
        rechazado(b.guardar_credito(REG, &nss, "16", "2026-01-15", "", 0, 0), "que suspender");
        rechazado(b.guardar_credito(REG, &nss, "17", "2026-01-15", "", 0, 0), "suspendido");
        rechazado(b.guardar_credito(REG, &nss, "19", "2026-01-15", "", 0, 300_000), "no tiene crédito");
        rechazado(b.guardar_credito(REG, "00000000000", "15", "2026-01-01", CRED, 1, 250_000), "trabajador registrado");

        b.guardar_credito(REG, &nss, "15", "2026-01-01", CRED, 1, 250_000).unwrap();
        assert_eq!(b.trabajadores(REG, "").unwrap()[0].pag_info, CRED);
        rechazado(b.guardar_credito(REG, &nss, "15", "2026-01-10", "0912345686", 1, 250_000), "ya tiene un crédito");
        rechazado(b.guardar_credito(REG, &nss, "17", "2026-01-10", "", 0, 0), "suspendido");
        // La modificación de valor se valida contra el tipo vigente (porcentaje).
        rechazado(b.guardar_credito(REG, &nss, "19", "2026-03-01", "", 0, 15_000_000), "Porcentaje");
        b.guardar_credito(REG, &nss, "19", "2026-03-01", "", 0, 300_000).unwrap();
        rechazado(b.guardar_credito(REG, &nss, "16", "2026-02-10", "", 0, 0), "en orden");
        b.guardar_credito(REG, &nss, "18", "2026-05-01", "", 2, 15_000_000).unwrap();
        b.guardar_credito(REG, &nss, "20", "2026-05-01", "0912345686", 0, 0).unwrap();
        assert_eq!(b.trabajadores(REG, "").unwrap()[0].pag_info, "0912345686");
        b.guardar_credito(REG, &nss, "16", "2026-06-30", "", 0, 0).unwrap();
        rechazado(b.guardar_credito(REG, &nss, "16", "2026-07-15", "", 0, 0), "que suspender");
        b.guardar_credito(REG, &nss, "17", "2026-08-01", "", 0, 0).unwrap();

        let m = b.movimientos(REG, &nss).unwrap();
        let de = |tipo: &str| m.iter().find(|x| x.tipo == tipo).unwrap();
        assert_eq!((de("15").credito.as_str(), de("15").tipo_descuento, de("15").valor_descuento), (CRED, 1, 250_000));
        assert_eq!((de("18").tipo_descuento, de("18").valor_descuento, de("19").valor_descuento), (2, 15_000_000, 300_000));
        // Los movimientos de crédito no alteran los días ni cuentan como no aplicados.
        let r = b.calcular_periodo(REG, "202602", false).unwrap();
        assert_eq!((r.lineas[0].dias_mes, r.lineas[0].dias_bimestre), (28, 59));
        assert!(r.avisos.iter().all(|a| !a.contains("no se aplicaron")));
    }

    #[test]
    fn el_calculo_incluye_la_amortizacion() {
        let t = CarpetaTemporal::nueva("per7");
        let b = base(&t);
        b.guardar_prima_rt(REG, "2007-03", 758_875).unwrap();
        // Cuota fija de 1,500.00 al mes, todo el bimestre de 59 días: 3,000.00.
        let fija = alta(&b, 1, 40_000, "");
        b.guardar_movimiento(REG, &fija, "01", "2026-01-01", 40_000).unwrap();
        b.guardar_credito(REG, &fija, "15", "2026-01-01", "0912345678", 2, 15_000_000).unwrap();
        // Factor 28.5320 desde el 1 de febrero: 28.5320 × 100.81 × 2 / 59 × 28 = 2,730.06.
        let factor = alta(&b, 2, 40_000, "");
        b.guardar_movimiento(REG, &factor, "01", "2026-01-01", 40_000).unwrap();
        b.guardar_credito(REG, &factor, "15", "2026-02-01", "0912345686", 3, 285_320).unwrap();
        // 25 % suspendido el 31 de enero: 400.00 × 31 × 25 % = 3,100.00.
        let pct = alta(&b, 3, 40_000, "");
        b.guardar_movimiento(REG, &pct, "01", "2026-01-01", 40_000).unwrap();
        b.guardar_credito(REG, &pct, "15", "2026-01-01", "0912345694", 1, 250_000).unwrap();
        b.guardar_credito(REG, &pct, "16", "2026-01-31", "", 0, 0).unwrap();
        // Sin crédito.
        let sin = alta(&b, 4, 40_000, "");
        b.guardar_movimiento(REG, &sin, "01", "2026-01-01", 40_000).unwrap();

        let r = b.calcular_periodo(REG, "202602", false).unwrap();
        let amortizaciones: Vec<(i64, i64, &str)> = r.lineas.iter().map(|l| (l.bimestre.amortizacion, l.bimestre.seguro_vivienda, l.credito.as_str())).collect();
        assert_eq!(amortizaciones, [(300_000, 1_300, "0912345678"), (273_006, 1_300, "0912345686"), (310_000, 1_300, "0912345694"), (0, 0, "")]);
        // La aportación patronal del 5 % no cambia por tener crédito.
        assert!(r.lineas.iter().all(|l| l.bimestre.infonavit == r.lineas[3].bimestre.infonavit));
        assert_eq!((r.bimestre.amortizacion, r.bimestre.seguro_vivienda), (883_006, 3_900));
        assert_eq!(r.total(), r.mes.total() + r.bimestre.total());
        assert_eq!(r.lineas[0].bimestre.total() - r.lineas[3].bimestre.total(), 301_300);

        // Ausentismos e incapacidades no llevan descuento: 59 − 2 − 3 = 54 días; 1,500 × 2 / 59 × 54 = 2,745.76.
        b.guardar_incidencia(REG, &fija, "11", "2026-01-12", 2, "").unwrap();
        b.guardar_incidencia(REG, &fija, "12", "2026-02-02", 3, "AB123456").unwrap();
        let r = b.calcular_periodo(REG, "202602", false).unwrap();
        assert_eq!(r.lineas[0].bimestre.amortizacion, 274_576);
        // En un mes impar no hay bimestre ni amortización.
        let r = b.calcular_periodo(REG, "202601", false).unwrap();
        assert!(r.lineas.iter().all(|l| l.bimestre.total() == 0 && l.credito.is_empty()));
        // Un año sin UMI en el catálogo: se avisa.
        let r = b.calcular_periodo(REG, "202802", false).unwrap();
        assert!(r.avisos.iter().any(|a| a.contains("UMI")));

        // Antes de la UMI el factor va sobre el salario mínimo: 10 × 52.59 × 2 = 1,051.80 en febrero de 2008.
        let viejo = alta(&b, 5, 10_000, "");
        b.guardar_movimiento(REG, &viejo, "01", "2008-01-01", 10_000).unwrap();
        b.guardar_credito(REG, &viejo, "15", "2008-01-01", "0912345702", 3, 100_000).unwrap();
        let r = b.calcular_periodo(REG, "200802", false).unwrap();
        assert_eq!((r.lineas.len(), r.lineas[0].bimestre.amortizacion, r.lineas[0].bimestre.seguro_vivienda), (1, 105_180, 1_300));
    }

    #[test]
    fn pago_extemporaneo_con_actualizacion_y_recargos() {
        let t = CarpetaTemporal::nueva("per8");
        let b = base(&t);
        b.guardar_prima_rt(REG, "2007-03", 758_875).unwrap();
        let nss = alta(&b, 1, 10_000, "");
        b.guardar_movimiento(REG, &nss, "01", "2008-01-01", 10_000).unwrap();
        let oportuno = b.calcular_periodo(REG, "200802", false).unwrap();
        assert_eq!(oportuno.extemporaneo, None);

        // Febrero de 2008 vencía el lunes 17 de marzo. Pagar ese día es oportuno.
        let r = b.calcular_periodo_con_pago(REG, "200802", false, Some("2008-03-17")).unwrap();
        assert_eq!(r, oportuno);
        // Pagado el 25 de marzo: un mes de mora al 1.13 % y factor 1, como en el archivo real.
        let r = b.calcular_periodo_con_pago(REG, "200802", false, Some("2008-03-25")).unwrap();
        let e = r.extemporaneo.clone().unwrap();
        assert_eq!((e.vencimiento.as_str(), e.meses, e.factor, e.tasa), ("2008-03-17", 1, 10_000, 113));
        // Retiro 120.00 → 1.36; cesantía patronal 189.00 → 2.14 y obrera 67.50 → 0.76; INFONAVIT 300.00 → 3.39.
        assert_eq!((e.actualizacion_rcv, e.recargos_rcv, e.actualizacion_infonavit, e.recargos_infonavit), (0, 136 + 214 + 76, 0, 339));
        let m = &r.lineas[0].mes;
        assert_eq!(e.recargos_imss, (m.patronal() * 113 + 5_000) / 10_000 + (m.obrero() * 113 + 5_000) / 10_000);
        assert_eq!((r.cuotas(), r.total()), (oportuno.total(), oportuno.total() + e.total()));
        assert!(r.avisos.is_empty());

        // Pagado el 20 de mayo: tres meses (marzo, abril y mayo) y actualización con el INPC de abril entre el de febrero.
        let r = b.calcular_periodo_con_pago(REG, "200802", false, Some("2008-05-20")).unwrap();
        let e = r.extemporaneo.unwrap();
        assert_eq!((e.meses, e.tasa), (3, 339));
        assert!(e.factor > 10_000 && e.actualizacion_rcv > 0 && e.actualizacion_imss > 0);
        let retiro = 12_000;
        let actualizacion = (retiro * (e.factor - 10_000) + 5_000) / 10_000;
        assert_eq!(e.cargos(retiro), (actualizacion, ((retiro + actualizacion) * 339 + 5_000) / 10_000));

        // El seguro de daños no lleva actualización ni recargos: con crédito, la base es la amortización sola.
        b.guardar_credito(REG, &nss, "15", "2008-01-01", "0912345678", 2, 15_000_000).unwrap();
        let r = b.calcular_periodo_con_pago(REG, "200802", false, Some("2008-03-25")).unwrap();
        assert_eq!((r.bimestre.amortizacion, r.bimestre.seguro_vivienda), (300_000, 1_300));
        assert_eq!(r.extemporaneo.unwrap().recargos_infonavit, 339 + 3_390);

        // Fecha mal escrita, y una fecha tan lejana que el catálogo no alcanza.
        assert!(matches!(b.calcular_periodo_con_pago(REG, "200802", false, Some("25/03/2008")), Err(ErrorSua::Validacion(_))));
        let r = b.calcular_periodo_con_pago(REG, "200802", false, Some("2030-01-10")).unwrap();
        assert!(r.avisos.iter().any(|a| a.contains("INPC")) && r.avisos.iter().any(|a| a.contains("tasa de recargos")) && r.avisos.iter().any(|a| a.contains("cinco años")));
    }

    /// Importa cada archivo real y calcula su periodo: los totales deben
    /// coincidir con los del archivo, salvo un centavo conocido.
    #[test]
    fn muestras_reales_si_estan_disponibles() {
        let Some(carpeta) = std::env::var_os("SUA_MUESTRAS") else { return };
        let mut pendientes = vec![std::path::PathBuf::from(carpeta)];
        let (mut archivos, mut diferencia) = (0, 0i64);
        while let Some(dir) = pendientes.pop() {
            for e in std::fs::read_dir(&dir).unwrap().flatten() {
                let ruta = e.path();
                if ruta.is_dir() {
                    pendientes.push(ruta);
                    continue;
                }
                if !ruta.extension().is_some_and(|x| x.eq_ignore_ascii_case("sua")) {
                    continue;
                }
                let a = crate::io::pago::leer(&std::fs::read(&ruta).unwrap()).unwrap();
                let t = CarpetaTemporal::nueva("per3");
                let b = Base::abrir_en(&t.0.join("sua.db"), &t.0.join("respaldos"), Modo::Portable, 3).unwrap();
                b.importar_pago(&a).unwrap();
                let reg = &a.patron.registro_patronal;
                let s = &a.totales;
                let imss_archivo = s.suma_imss;
                // El decreto es opcional: se toma el cálculo que más se acerca.
                let r = [false, true]
                    .into_iter()
                    .map(|d| b.calcular_periodo(reg, &a.patron.periodo, d).unwrap())
                    .min_by_key(|r| (r.mes.total() - imss_archivo).abs())
                    .unwrap();
                // Días: deben coincidir trabajador por trabajador.
                for x in &a.trabajadores {
                    let l = r.lineas.iter().find(|l| l.nss == x.nss);
                    if x.dias_mes > 0 {
                        assert_eq!(l.map(|l| l.dias_mes), Some(x.dias_mes as u32));
                    }
                    if x.dias_bimestre > 0 {
                        assert_eq!(l.map(|l| l.dias_bimestre), Some(x.dias_bimestre as u32));
                    }
                }
                if imss_archivo > 0 {
                    diferencia += (r.mes.total() - imss_archivo).abs();
                }
                if s.suma_rcv > 0 {
                    diferencia += (r.bimestre.retiro + r.bimestre.cyv_patronal + r.bimestre.cyv_obrero - s.suma_rcv).abs();
                }
                if a.control.total_infonavit > 0 {
                    diferencia += (r.bimestre.infonavit - a.control.total_infonavit).abs();
                }
                archivos += 1;
            }
        }
        assert!(archivos > 0);
        assert!(diferencia <= 2, "diferencia total de {diferencia} centavos");
    }
}
