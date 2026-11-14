//! Tasas y bases vigentes en un periodo, leídas de los catálogos (`SALARIO`,
//! `UMA`, `CEAV_Patronal`).
//!
//! Los catálogos del `SUA.MDB` terminaban en 2018; la migración 3 los extiende
//! hasta 2026. El último renglón de cada uno queda abierto («hasta 2999»): si
//! el periodo pedido es de más de un año después de su inicio, el valor ya no
//! es el vigente y [`Vigencias::desactualizado`] lo indica. Hay que añadir cada
//! año los valores nuevos.

use super::Base;
use crate::dominio::calculo::{tasas_eym, Tasas};
use crate::dominio::extemporaneo::{factor_de_actualizacion, fecha_de_dia, iso_de_dia, mes_anterior, mes_siguiente, meses_de_mora, vencimiento, Extemporaneo};
use crate::dominio::tramos::{dia_de_iso, Periodo};
use crate::error::{ErrorSua, Resultado};

/// Último mes (`AAAAMM`) del catálogo `INPC` que viene con el programa.
pub const ULTIMO_MES_DE_INPC_CARGADO: i64 = 2026_01;

/// `dd/mm/aaaa …` (formato en que Access exportó las fechas) → `aaaammdd`.
fn fecha_catalogo(texto: &str) -> Option<u32> {
    let f = texto.split_whitespace().next()?;
    let mut p = f.split('/');
    let (d, m, a) = (p.next()?.parse::<u32>().ok()?, p.next()?.parse::<u32>().ok()?, p.next()?.parse::<u32>().ok()?);
    Some(a * 10_000 + m * 100 + d)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Vigencias {
    pub tasas: Tasas,
    /// `true` si el periodo cae más allá de la vigencia anual del último valor
    /// registrado de salario mínimo o UMA.
    pub desactualizado: bool,
    /// `true` si la cuota fija se calcula sobre la UMA (desde febrero de 2017).
    pub usa_uma: bool,
}

impl Base {
    fn valor_vigente(&self, catalogo: &str, col_ini: &str, col_fin: &str, col_valor: &str, fecha: u32) -> Resultado<Option<(f64, u32, u32)>> {
        let t = self.leer_catalogo(catalogo)?;
        let pos = |nombre: &str| {
            t.columnas
                .iter()
                .position(|c| c.eq_ignore_ascii_case(nombre))
                .ok_or_else(|| ErrorSua::Semilla { catalogo: catalogo.into(), detalle: format!("falta la columna {nombre}") })
        };
        let (i, f, v) = (pos(col_ini)?, pos(col_fin)?, pos(col_valor)?);
        for fila in &t.filas {
            let (Some(ini), Some(fin)) = (fecha_catalogo(&fila[i]), fecha_catalogo(&fila[f])) else { continue };
            if (ini..=fin).contains(&fecha) {
                return Ok(fila[v].trim().parse::<f64>().ok().map(|valor| (valor, ini, fin)));
            }
        }
        Ok(None)
    }

    /// Registra el INPC y la tasa de recargos de un mes `AAAAMM`. Como en el
    /// SUA, los meses que venían cargados no se modifican y cada mes nuevo
    /// requiere que exista el anterior. `inpc` y `tasa` van como texto
    /// (`143.925`, `2.07`).
    pub fn guardar_inpc(&self, mes: &str, inpc: &str, tasa: &str) -> Resultado<()> {
        self.con_permiso(crate::datos::usuarios::Permiso::Capturar, "Guardó INPC", || mes.to_string(), || self.guardar_inpc_sin_control(mes, inpc, tasa))
    }

    fn guardar_inpc_sin_control(&self, mes: &str, inpc: &str, tasa: &str) -> Resultado<()> {
        let mal = |m: &str| ErrorSua::Validacion(m.to_string());
        let p = Periodo::de_texto(mes.trim()).ok_or_else(|| mal("Mes: use el formato AAAAMM."))?;
        let clave = p.anio * 100 + p.mes;
        if clave <= ULTIMO_MES_DE_INPC_CARGADO {
            return Err(mal("Ese mes venía cargado con el programa y no se modifica."));
        }
        let numero = |texto: &str, max: f64, decimales: usize| {
            let t = texto.trim();
            let forma = t.split_once('.').map_or(true, |(_, d)| !d.is_empty() && d.len() <= decimales) && t.bytes().all(|b| b.is_ascii_digit() || b == b'.');
            t.parse::<f64>().ok().filter(|v| forma && *v >= 0.0 && *v < max).map(|_| t.to_string())
        };
        let inpc = numero(inpc, 100_000.0, 6).filter(|t| t.parse::<f64>().is_ok_and(|v| v > 0.0)).ok_or_else(|| mal("INPC: escriba un número mayor que cero, como 143.925."))?;
        let tasa = numero(tasa, 100.0, 2).ok_or_else(|| mal("Tasa de recargos: escriba el porcentaje mensual, como 2.07."))?;
        let existe = |k: i64| self.conn.query_row("SELECT COUNT(*) FROM \"cat_INPC\" WHERE \"MES_ANO\" = ?1", [k.to_string()], |r| r.get::<_, i64>(0));
        let (a, m) = mes_anterior(p.anio, p.mes);
        if existe(a * 100 + m)? == 0 {
            return Err(mal("Falta el mes anterior: registre los meses en orden."));
        }
        if existe(clave)? > 0 {
            self.conn.execute("UPDATE \"cat_INPC\" SET \"VAL_INP\" = ?2, \"VAL_REC\" = ?3 WHERE \"MES_ANO\" = ?1", [clave.to_string(), inpc, tasa])?;
        } else {
            self.conn.execute("INSERT INTO \"cat_INPC\" (\"MES_ANO\", \"VAL_INP\", \"VAL_REC\") VALUES (?1, ?2, ?3)", [clave.to_string(), inpc, tasa])?;
        }
        Ok(())
    }

    /// Fecha límite de pago de un periodo `AAAAMM`, como `AAAA-MM-DD`.
    pub fn vencimiento(&self, periodo: &str) -> Resultado<String> {
        let p = Periodo::de_texto(periodo).ok_or_else(|| ErrorSua::Validacion(format!("Periodo «{periodo}» inválido: se espera AAAAMM.")))?;
        let inhabiles: Vec<(i64, i64)> =
            self.leer_catalogo("Dias_Inhabiles")?.filas.iter().filter_map(|f| Some((f.first()?.trim().parse().ok()?, f.get(1)?.trim().parse().ok()?))).collect();
        Ok(iso_de_dia(vencimiento(p.anio, p.mes, &inhabiles)))
    }

    /// Datos de un pago extemporáneo: vencimiento, meses de mora, factor de
    /// actualización y tasa de recargos, del catálogo `INPC` (INPC y tasa de
    /// recargos por mes) y de `Dias_Inhabiles`. `None` si la fecha de pago no
    /// pasa del vencimiento. Los importes quedan en cero: los pone el cálculo.
    /// La segunda parte son avisos (valores que faltan en el catálogo).
    pub fn extemporaneo(&self, periodo: &str, fecha_pago: &str) -> Resultado<Option<(Extemporaneo, Vec<String>)>> {
        let mal = |m: String| ErrorSua::Validacion(m);
        let p = Periodo::de_texto(periodo).ok_or_else(|| mal(format!("Periodo «{periodo}» inválido: se espera AAAAMM.")))?;
        let pago = dia_de_iso(fecha_pago.trim()).ok_or_else(|| mal("Fecha de pago: use el formato AAAA-MM-DD.".into()))?;
        let inhabiles: Vec<(i64, i64)> =
            self.leer_catalogo("Dias_Inhabiles")?.filas.iter().filter_map(|f| Some((f.first()?.trim().parse().ok()?, f.get(1)?.trim().parse().ok()?))).collect();
        let vence = vencimiento(p.anio, p.mes, &inhabiles);
        if pago <= vence {
            return Ok(None);
        }
        let (pa, pm, pd) = fecha_de_dia(pago);
        let meses = meses_de_mora((p.anio, p.mes), (pa, pm, pd));

        // (aaaamm, INPC, tasa de recargos en centésimas de punto), en orden.
        let mut tabla: Vec<(i64, f64, i64)> = self
            .leer_catalogo("INPC")?
            .filas
            .iter()
            .filter_map(|f| Some((f.first()?.trim().parse().ok()?, f.get(1)?.trim().parse().ok()?, (f.get(2)?.trim().parse::<f64>().ok()? * 100.0).round() as i64)))
            .collect();
        tabla.sort_by_key(|x| x.0);
        let clave = |(a, m): (i64, i64)| a * 100 + m;
        let de = |k: i64| tabla.iter().find(|x| x.0 == k);
        let mes_texto = |k: i64| format!("{:02}/{}", k % 100, k / 100);
        let mut avisos = Vec::new();

        let antiguo = de(clave((p.anio, p.mes))).ok_or_else(|| mal(format!("No hay INPC registrado para {}.", mes_texto(clave((p.anio, p.mes))))))?;
        let buscado = clave(mes_anterior(pa, pm));
        let reciente = match de(buscado) {
            Some(x) => x,
            None => {
                let ultimo = tabla.iter().rev().find(|x| x.0 < buscado).ok_or_else(|| mal("El catálogo de INPC está vacío.".into()))?;
                avisos.push(format!("El INPC de {} no está en el catálogo: se actualizó con el más reciente registrado, el de {}.", mes_texto(buscado), mes_texto(ultimo.0)));
                ultimo
            }
        };
        let (mut tasa, mut sin_tasa) = (0, 0);
        let mut mes = mes_siguiente(p.anio, p.mes);
        for _ in 0..meses {
            match de(clave(mes)) {
                Some(x) => tasa += x.2,
                None => {
                    sin_tasa += 1;
                    tasa += tabla.last().map_or(0, |x| x.2);
                }
            }
            mes = mes_siguiente(mes.0, mes.1);
        }
        if sin_tasa > 0 {
            avisos.push(format!("El catálogo no tiene tasa de recargos para {sin_tasa} de los {meses} meses de mora: en ellos se usó la última registrada."));
        }
        if meses > 60 {
            avisos.push("Son más de cinco años de mora: los recargos se causan hasta por cinco años y aquí no se limitan.".into());
        }
        Ok(Some((
            Extemporaneo {
                fecha_pago: fecha_pago.trim().to_string(),
                vencimiento: iso_de_dia(vence),
                meses,
                factor: factor_de_actualizacion(reciente.1, antiguo.1),
                tasa,
                ..Extemporaneo::default()
            },
            avisos,
        )))
    }

    /// UMI diaria del año, para los créditos con factor de descuento.
    /// Devuelve `(valor, desactualizado)`: si el año es posterior al último
    /// del catálogo se usa ese último y se marca. `None` si el año es
    /// anterior al catálogo: entonces el factor va sobre el salario mínimo.
    pub fn umi(&self, anio: u32) -> Resultado<Option<(f64, bool)>> {
        let t = self.leer_catalogo(super::migraciones::CATALOGO_UMI)?;
        let mut valores: Vec<(u32, f64)> = t.filas.iter().filter_map(|f| Some((f.first()?.trim().parse().ok()?, f.get(1)?.trim().parse().ok()?))).collect();
        valores.sort_by_key(|v| v.0);
        let (Some(primero), Some(ultimo)) = (valores.first().copied(), valores.last().copied()) else { return Ok(None) };
        if anio < primero.0 {
            return Ok(None);
        }
        Ok(Some(match valores.iter().find(|v| v.0 == anio) {
            Some(v) => (v.1, false),
            None => (ultimo.1, anio > ultimo.0),
        }))
    }

    /// Seguro de daños a la vivienda por bimestre, en centavos (catálogo
    /// `Parametros`, clave 1). Se cobra desde el primer bimestre de 2006.
    pub fn seguro_vivienda(&self, periodo: &str) -> Resultado<i64> {
        if periodo < "200601" {
            return Ok(0);
        }
        let t = self.leer_catalogo("Parametros")?;
        let pesos = t.filas.iter().find(|f| f.first().is_some_and(|c| c.trim() == "1")).and_then(|f| f.get(2)?.trim().parse::<f64>().ok());
        Ok(pesos.map_or(0, |p| (p * 100.0).round() as i64))
    }

    /// Tasas del periodo `AAAAMM`. `prima_cienmilesimas`: prima de RT del
    /// patrón (7.58875 % = 758875). `aplicar_decreto`: aplica el descuento de
    /// 2008 o 2009 si el periodo cae en su vigencia (es opcional para el patrón).
    pub fn tasas(&self, periodo: &str, prima_cienmilesimas: i64, aplicar_decreto: bool) -> Resultado<Vigencias> {
        let invalido = || ErrorSua::Validacion(format!("Periodo «{periodo}» inválido: se espera AAAAMM."));
        if periodo.len() != 6 || !periodo.bytes().all(|b| b.is_ascii_digit()) {
            return Err(invalido());
        }
        let (anio, mes): (u32, u32) = (periodo[..4].parse().map_err(|_| invalido())?, periodo[4..].parse().map_err(|_| invalido())?);
        if !(1..=12).contains(&mes) || anio < 1997 {
            return Err(invalido());
        }
        let fecha = anio * 10_000 + mes * 100 + 1;

        let sin_valor = |que: &str| ErrorSua::Validacion(format!("No hay {que} registrado para el periodo {periodo}."));
        let (minimo, minimo_desde, minimo_hasta) = self.valor_vigente("SALARIO", "FEC_INI", "FEC_TER", "ZONAA", fecha)?.ok_or_else(|| sin_valor("salario mínimo"))?;
        let usa_uma = fecha >= 2017_02_01;
        let (base, base_desde, base_hasta) = if usa_uma {
            self.valor_vigente("UMA", "Fec_Ini", "Fec_Ter", "Valor", fecha)?.ok_or_else(|| sin_valor("valor de la UMA"))?
        } else {
            (minimo, minimo_desde, minimo_hasta)
        };
        // El último renglón de cada catálogo dura «hasta 2999». Cada valor rige
        // un año: pasado ese año desde su inicio, ya no es el vigente.
        let vencido = |desde: u32, hasta: u32| hasta >= 2999_01_01 && fecha >= desde + 1_00_00;
        let desactualizado = vencido(base_desde, base_hasta) || vencido(minimo_desde, minimo_hasta);

        let (cuota_fija, excedente_patronal, excedente_obrero) = tasas_eym(anio);
        let descuento_decreto = match (aplicar_decreto, anio, mes) {
            (true, 2008, 3..=12) => 5,
            (true, 2009, 5..=6) => 20,
            _ => 0,
        };

        let mut cyv_patronal_por_rango = Vec::new();
        if anio >= 2023 {
            let t = self.leer_catalogo("CEAV_Patronal")?;
            let columna = anio.min(2030).to_string();
            let c = t.columnas.iter().position(|x| *x == columna).ok_or_else(|| sin_valor("tabla de cesantía y vejez patronal"))?;
            let limites = [1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, f64::MAX];
            for (fila, limite) in t.filas.iter().zip(limites) {
                let tasa = fila[c].trim().parse::<f64>().map_err(|_| sin_valor("tasa de cesantía y vejez patronal"))?;
                cyv_patronal_por_rango.push((limite, tasa));
            }
        }

        Ok(Vigencias {
            tasas: Tasas {
                base_cuota_fija: base,
                cuota_fija,
                excedente_patronal,
                excedente_obrero,
                prima_rt: prima_cienmilesimas as f64 / 100_000.0,
                cyv_patronal: 3.15,
                descuento_decreto,
                tope_veces: 25.0,
                cyv_patronal_por_rango,
                salario_minimo: minimo,
            },
            desactualizado,
            usa_uma,
        })
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::config::{CarpetaTemporal, Modo};

    fn base(t: &CarpetaTemporal) -> Base {
        Base::abrir_en(&t.0.join("sua.db"), &t.0.join("respaldos"), Modo::Portable, 3).unwrap()
    }

    #[test]
    fn fechas_del_catalogo() {
        assert_eq!(fecha_catalogo("01/02/2017 12:00:00 a. m."), Some(2017_02_01));
        assert_eq!(fecha_catalogo("31/12/2999 12:00:00 a. m."), Some(2999_12_31));
        assert_eq!(fecha_catalogo("sin fecha"), None);
    }

    #[test]
    fn tasas_de_periodos_con_muestra_real() {
        let t = CarpetaTemporal::nueva("tas1");
        let b = base(&t);

        let v = b.tasas("200605", 758_875, true).unwrap();
        assert_eq!((v.tasas.base_cuota_fija, v.tasas.cuota_fija, v.tasas.excedente_obrero), (48.67, 19.1, 0.72));
        assert_eq!((v.tasas.descuento_decreto, v.usa_uma, v.desactualizado), (0, false, false));
        assert_eq!(v.tasas.prima_rt, 7.58875);
        assert!(v.tasas.cyv_patronal_por_rango.is_empty());

        // Febrero de 2008: antes del decreto. Junio: dentro, si el patrón lo aplica.
        assert_eq!(b.tasas("200802", 758_875, true).unwrap().tasas.descuento_decreto, 0);
        let junio = b.tasas("200806", 758_875, true).unwrap();
        assert_eq!((junio.tasas.base_cuota_fija, junio.tasas.cuota_fija, junio.tasas.descuento_decreto), (52.59, 20.4, 5));
        assert_eq!(b.tasas("200806", 758_875, false).unwrap().tasas.descuento_decreto, 0);
        assert_eq!(b.tasas("200905", 0, true).unwrap().tasas.descuento_decreto, 20);

        assert_eq!(b.tasas("201309", 0, false).unwrap().tasas.base_cuota_fija, 64.76);
    }

    #[test]
    fn uma_desde_febrero_de_2017_y_aviso_de_catalogo_vencido() {
        let t = CarpetaTemporal::nueva("tas2");
        let b = base(&t);
        let enero = b.tasas("201701", 0, false).unwrap();
        assert_eq!((enero.usa_uma, enero.tasas.base_cuota_fija), (false, 80.04));
        let febrero = b.tasas("201702", 0, false).unwrap();
        assert_eq!((febrero.usa_uma, febrero.tasas.base_cuota_fija, febrero.desactualizado), (true, 75.49, false));

        // Valores añadidos por la migración 3 (2019–2026).
        let v = b.tasas("201901", 0, false).unwrap();
        assert_eq!((v.tasas.salario_minimo, v.tasas.base_cuota_fija, v.desactualizado), (102.68, 80.6, false));
        let v = b.tasas("201902", 0, false).unwrap();
        assert_eq!((v.tasas.salario_minimo, v.tasas.base_cuota_fija), (102.68, 84.49));
        let v = b.tasas("202212", 0, false).unwrap();
        assert_eq!((v.tasas.salario_minimo, v.tasas.base_cuota_fija), (172.87, 96.22));
        assert!(v.tasas.cyv_patronal_por_rango.is_empty());
        // Enero usa la UMA del año anterior.
        let v = b.tasas("202601", 0, false).unwrap();
        assert_eq!((v.tasas.salario_minimo, v.tasas.base_cuota_fija, v.desactualizado), (315.04, 113.14, false));
        let hoy = b.tasas("202611", 0, false).unwrap();
        assert_eq!((hoy.tasas.salario_minimo, hoy.tasas.base_cuota_fija, hoy.desactualizado), (315.04, 117.31, false));
        // El salario mínimo de 2026 deja de ser vigente en enero de 2027; la UMA, en febrero.
        let v = b.tasas("202701", 0, false).unwrap();
        assert!(v.desactualizado);
        assert_eq!(v.tasas.base_cuota_fija, 117.31);
        assert!(b.tasas("202702", 0, false).unwrap().desactualizado);
        assert_eq!(hoy.tasas.cyv_patronal_por_rango.len(), 8);
        assert_eq!(hoy.tasas.cyv_patronal_por_rango[7].1, 7.513);
        assert_eq!(hoy.tasas.cyv_patronal_por_rango[0], (1.0, 3.15));
    }

    #[test]
    fn periodos_invalidos() {
        let t = CarpetaTemporal::nueva("tas3");
        let b = base(&t);
        for malo in ["2008", "200813", "abcdef", "199001"] {
            assert!(matches!(b.tasas(malo, 0, false), Err(ErrorSua::Validacion(_))), "{malo}");
        }
    }

    #[test]
    fn captura_de_inpc_y_recargos_de_meses_nuevos() {
        let t = crate::config::CarpetaTemporal::nueva("inpc");
        let b = Base::abrir_en(&t.0.join("sua.db"), &t.0.join("respaldos"), crate::config::Modo::Portable, 3).unwrap();
        let rechazado = |r: Resultado<()>, texto: &str| match r {
            Err(ErrorSua::Validacion(m)) => assert!(m.contains(texto), "{m}"),
            otro => panic!("se esperaba rechazo con «{texto}», se obtuvo {otro:?}"),
        };
        rechazado(b.guardar_inpc("2026-02", "143.9", "2.07"), "AAAAMM");
        rechazado(b.guardar_inpc("202601", "143.9", "2.07"), "no se modifica");
        rechazado(b.guardar_inpc("202603", "143.9", "2.07"), "mes anterior");
        rechazado(b.guardar_inpc("202602", "cien", "2.07"), "INPC");
        rechazado(b.guardar_inpc("202602", "0", "2.07"), "INPC");
        rechazado(b.guardar_inpc("202602", "143.9", "2.075"), "Tasa");
        // Sin el INPC del mes del periodo no se puede actualizar.
        assert!(matches!(b.extemporaneo("202602", "2026-04-20"), Err(ErrorSua::Validacion(m)) if m.contains("02/2026")));

        b.guardar_inpc("202602", "144.000", "2.07").unwrap();
        b.guardar_inpc("202603", "144.720", "2").unwrap();
        b.guardar_inpc("202603", "144.720", "2.00").unwrap(); // corregir un mes capturado sí se puede
        assert_eq!(b.filas_de_catalogo("INPC").unwrap(), 344 + 2);
        assert_eq!((b.vencimiento("202602").unwrap().as_str(), b.vencimiento("202512").unwrap().as_str()), ("2026-03-17", "2026-01-19"));
        // Febrero de 2026 vencía el 17 de marzo; pagado el 20 de abril son dos meses (marzo y abril).
        // Abril no tiene tasa: se usa la última y se avisa. Factor: INPC de marzo entre el de febrero.
        let (e, avisos) = b.extemporaneo("202602", "2026-04-20").unwrap().unwrap();
        assert_eq!((e.vencimiento.as_str(), e.meses, e.factor, e.tasa), ("2026-03-17", 2, 10_050, 400));
        assert!(avisos.len() == 1 && avisos[0].contains("1 de los 2 meses"));
        assert_eq!(b.extemporaneo("202602", "2026-03-17").unwrap(), None);
    }
}
