//! Motor de cuotas: IMSS mensual y RCV / INFONAVIT bimestral, por tramo de días.
//!
//! **Aritmética.** El SUA calcula en punto flotante de doble precisión, no en
//! decimal: se dedujo de los archivos de pago reales, donde aparecen diferencias
//! de un centavo que solo se explican así (`Decompilacion_SUA.md`, sección 15).
//! Para dar los mismos centavos este módulo reproduce esa aritmética a propósito:
//!
//! - importe = `base × días × porcentaje / 100`, en ese orden;
//! - riesgos de trabajo = `base × (prima / 100) × días` (orden distinto);
//! - centavos = `floor(importe × 100 + 0.5)`;
//! - patronal y obrera se redondean por separado y luego se suman;
//! - el descuento por decreto se aplica al importe patronal ya redondeado.
//!
//! Comprobado contra 5 archivos de pago reales de 2006–2008 (28 trabajadores
//! con cuotas mensuales y 39 con bimestrales): 17 de 18 conceptos coinciden en
//! todos los casos; cesantía y vejez patronal en 38 de 39 (un centavo en un
//! trabajador con dos periodos en el bimestre). Y contra una Emisión del IMSS
//! de 2013 (121 importes).
//!
//! **Ausentismos e incapacidades** ([`DiasCotizados`]): se aplican según el
//! artículo 31 de la Ley del Seguro Social, sin ningún caso real que lo
//! confirme.
//!
//! **Sin comprobar:** tope de 25 salarios mínimos o UMA, ausentismos,
//! incapacidades, cesantía patronal por rangos (2023 en adelante),
//! actualización y recargos. Un `.SUA` generado así no es aceptable sin el
//! número de control del registro 06; este motor sirve para comprobar.

/// Tasas y bases vigentes para un periodo. Los porcentajes van como número
/// de por ciento (20.4 = 20.40 %).
#[derive(Debug, Clone, PartialEq)]
pub struct Tasas {
    /// Salario mínimo general (hasta enero de 2017) o UMA diaria.
    pub base_cuota_fija: f64,
    pub cuota_fija: f64,
    pub excedente_patronal: f64,
    pub excedente_obrero: f64,
    /// Prima de riesgos de trabajo del patrón, en por ciento.
    pub prima_rt: f64,
    /// Cesantía y vejez patronal. Tasa única hasta 2022; desde 2023 depende
    /// del salario y la da [`Tasas::cyv_patronal_por_rango`].
    pub cyv_patronal: f64,
    /// Decreto de exención parcial de cuotas patronales del IMSS: 5 en
    /// marzo–diciembre de 2008, 20 en mayo–junio de 2009. `0` = sin decreto.
    pub descuento_decreto: u8,
    /// Veces la base que topa el salario (25).
    pub tope_veces: f64,
    /// (límite superior del rango en veces de UMA, tasa). Vacío hasta 2022.
    pub cyv_patronal_por_rango: Vec<(f64, f64)>,
    /// Salario mínimo del periodo, para el rango 1 de cesantía patronal.
    pub salario_minimo: f64,
}

pub const PD_PATRONAL: f64 = 0.7;
pub const PD_OBRERO: f64 = 0.25;
pub const GMP_PATRONAL: f64 = 1.05;
pub const GMP_OBRERO: f64 = 0.375;
pub const IV_PATRONAL: f64 = 1.75;
pub const IV_OBRERO: f64 = 0.625;
pub const GUARDERIAS: f64 = 1.0;
pub const RETIRO: f64 = 2.0;
pub const CYV_OBRERO: f64 = 1.125;
pub const INFONAVIT: f64 = 5.0;

/// Cuota fija y excedente por año calendario (LSS 1997, transitorio
/// decimonoveno): la cuota fija sube 0.65 puntos por año de 1999 a 2008; el
/// excedente baja 0.49 (patronal) y 0.16 (obrera). Devuelve
/// (cuota fija, excedente patronal, excedente obrero) en por ciento.
///
/// Comprobado con archivos reales para 2006 y 2008 y con la Emisión de 2013.
/// Los demás años salen de la regla.
pub fn tasas_eym(anio: u32) -> (f64, f64, f64) {
    let pasos = anio.clamp(1998, 2008) - 1998;
    // En centésimas, para no arrastrar error de punto flotante en la tasa.
    let centesimas = |inicio: u32, paso: i64| (i64::from(inicio) + paso * i64::from(pasos)) as f64 / 100.0;
    (centesimas(1390, 65), centesimas(600, -49), centesimas(200, -16))
}

fn centavos(importe: f64) -> i64 {
    (importe * 100.0 + 0.5).floor() as i64
}

fn cuota(base: f64, dias: u32, porcentaje: f64) -> i64 {
    centavos(base * f64::from(dias) * porcentaje / 100.0)
}

/// Días de un tramo y cuántos de ellos fueron de ausentismo o de incapacidad.
///
/// Ley del Seguro Social, artículo 31: en los días de ausentismo solo se
/// cotiza en enfermedades y maternidad (fracción I); en los de incapacidad
/// expedida por el IMSS no se cotiza, salvo en el ramo de retiro (fracción
/// IV). La aportación al INFONAVIT se paga en los días de incapacidad y no en
/// los de ausentismo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiasCotizados {
    pub naturales: u32,
    pub ausentismo: u32,
    pub incapacidad: u32,
}

impl DiasCotizados {
    pub fn completos(dias: u32) -> DiasCotizados {
        DiasCotizados { naturales: dias, ausentismo: 0, incapacidad: 0 }
    }

    /// Cuota fija, excedente, prestaciones en dinero y gastos médicos.
    pub fn enfermedades(&self) -> u32 {
        self.naturales.saturating_sub(self.incapacidad)
    }

    /// Riesgos de trabajo, invalidez y vida, guarderías, y cesantía y vejez.
    pub fn otros_ramos(&self) -> u32 {
        self.naturales.saturating_sub(self.incapacidad).saturating_sub(self.ausentismo)
    }

    /// Retiro y aportación al INFONAVIT.
    pub fn retiro_e_infonavit(&self) -> u32 {
        self.naturales.saturating_sub(self.ausentismo)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CuotasMes {
    pub cuota_fija: i64,
    pub excedente_patronal: i64,
    pub excedente_obrero: i64,
    pub pd_patronal: i64,
    pub pd_obrero: i64,
    pub gmp_patronal: i64,
    pub gmp_obrero: i64,
    pub riesgos_trabajo: i64,
    pub iv_patronal: i64,
    pub iv_obrero: i64,
    pub guarderias: i64,
}

impl CuotasMes {
    pub fn patronal(&self) -> i64 {
        self.cuota_fija + self.excedente_patronal + self.pd_patronal + self.gmp_patronal + self.riesgos_trabajo + self.iv_patronal + self.guarderias
    }

    pub fn obrero(&self) -> i64 {
        self.excedente_obrero + self.pd_obrero + self.gmp_obrero + self.iv_obrero
    }

    pub fn total(&self) -> i64 {
        self.patronal() + self.obrero()
    }

    pub fn sumar(&mut self, o: &CuotasMes) {
        self.cuota_fija += o.cuota_fija;
        self.excedente_patronal += o.excedente_patronal;
        self.excedente_obrero += o.excedente_obrero;
        self.pd_patronal += o.pd_patronal;
        self.pd_obrero += o.pd_obrero;
        self.gmp_patronal += o.gmp_patronal;
        self.gmp_obrero += o.gmp_obrero;
        self.riesgos_trabajo += o.riesgos_trabajo;
        self.iv_patronal += o.iv_patronal;
        self.iv_obrero += o.iv_obrero;
        self.guarderias += o.guarderias;
    }

    /// (concepto, importe en centavos)
    pub fn renglones(&self) -> [(&'static str, i64); 11] {
        [
            ("Cuota fija", self.cuota_fija),
            ("Excedente patronal", self.excedente_patronal),
            ("Excedente obrero", self.excedente_obrero),
            ("Prestaciones en dinero, patronal", self.pd_patronal),
            ("Prestaciones en dinero, obrero", self.pd_obrero),
            ("Gastos médicos de pensionados, patronal", self.gmp_patronal),
            ("Gastos médicos de pensionados, obrero", self.gmp_obrero),
            ("Riesgos de trabajo", self.riesgos_trabajo),
            ("Invalidez y vida, patronal", self.iv_patronal),
            ("Invalidez y vida, obrero", self.iv_obrero),
            ("Guarderías y prestaciones sociales", self.guarderias),
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CuotasBimestre {
    pub retiro: i64,
    pub cyv_patronal: i64,
    pub cyv_obrero: i64,
    /// Aportación patronal del 5 %.
    pub infonavit: i64,
    /// Amortización de créditos INFONAVIT, descontada al trabajador. No
    /// incluye el seguro de daños.
    pub amortizacion: i64,
    /// Seguro de daños a la vivienda: importe fijo por bimestre que el SUA
    /// suma a la amortización cuando la hay.
    pub seguro_vivienda: i64,
}

impl CuotasBimestre {
    pub fn total(&self) -> i64 {
        self.retiro + self.cyv_patronal + self.cyv_obrero + self.infonavit + self.amortizacion + self.seguro_vivienda
    }

    pub fn sumar(&mut self, o: &CuotasBimestre) {
        self.retiro += o.retiro;
        self.cyv_patronal += o.cyv_patronal;
        self.cyv_obrero += o.cyv_obrero;
        self.infonavit += o.infonavit;
        self.amortizacion += o.amortizacion;
        self.seguro_vivienda += o.seguro_vivienda;
    }
}

impl Tasas {
    fn salario_topado(&self, sdi: f64) -> f64 {
        sdi.min(self.tope_veces * self.base_cuota_fija)
    }

    fn con_decreto(&self, patronal: i64) -> i64 {
        if self.descuento_decreto == 0 {
            patronal
        } else {
            (patronal * i64::from(100 - self.descuento_decreto) + 50) / 100
        }
    }

    /// Tasa patronal de cesantía y vejez para un salario diario.
    pub fn cyv_patronal_para(&self, sdi: f64) -> f64 {
        if self.cyv_patronal_por_rango.is_empty() {
            return self.cyv_patronal;
        }
        // El rango 1 se decide contra el salario mínimo, no contra la UMA.
        if sdi <= self.salario_minimo {
            return self.cyv_patronal_por_rango[0].1;
        }
        let veces = sdi / self.base_cuota_fija;
        self.cyv_patronal_por_rango[1..]
            .iter()
            .find(|(limite, _)| veces <= *limite)
            .or(self.cyv_patronal_por_rango.last())
            .map(|(_, tasa)| *tasa)
            .unwrap_or(self.cyv_patronal)
    }

    /// Cuotas del IMSS de un tramo de días con salario constante.
    /// `sdi_centavos`: salario diario integrado en centavos.
    pub fn mes(&self, sdi_centavos: i64, dias: u32) -> CuotasMes {
        self.mes_con(sdi_centavos, DiasCotizados::completos(dias))
    }

    /// Como [`Tasas::mes`], descontando ausentismos e incapacidades.
    pub fn mes_con(&self, sdi_centavos: i64, dias_cotizados: DiasCotizados) -> CuotasMes {
        let (dias, otros) = (dias_cotizados.enfermedades(), dias_cotizados.otros_ramos());
        let sdi = self.salario_topado(sdi_centavos as f64 / 100.0);
        let excedente = (sdi - 3.0 * self.base_cuota_fija).max(0.0);
        let d = self.con_decreto_fn();
        CuotasMes {
            cuota_fija: d(cuota(self.base_cuota_fija, dias, self.cuota_fija)),
            excedente_patronal: d(cuota(excedente, dias, self.excedente_patronal)),
            excedente_obrero: cuota(excedente, dias, self.excedente_obrero),
            pd_patronal: d(cuota(sdi, dias, PD_PATRONAL)),
            pd_obrero: cuota(sdi, dias, PD_OBRERO),
            gmp_patronal: d(cuota(sdi, dias, GMP_PATRONAL)),
            gmp_obrero: cuota(sdi, dias, GMP_OBRERO),
            // Orden distinto al del resto: es el que reproduce los archivos reales.
            riesgos_trabajo: d(centavos(sdi * (self.prima_rt / 100.0) * f64::from(otros))),
            iv_patronal: d(cuota(sdi, otros, IV_PATRONAL)),
            iv_obrero: cuota(sdi, otros, IV_OBRERO),
            guarderias: d(cuota(sdi, otros, GUARDERIAS)),
        }
    }

    fn con_decreto_fn(&self) -> impl Fn(i64) -> i64 + '_ {
        move |c| self.con_decreto(c)
    }

    /// Cuotas de RCV e INFONAVIT de un tramo de días del bimestre. Los decretos
    /// de 2008 y 2009 no las tocan.
    pub fn bimestre(&self, sdi_centavos: i64, dias: u32) -> CuotasBimestre {
        self.bimestre_con(sdi_centavos, DiasCotizados::completos(dias))
    }

    /// Como [`Tasas::bimestre`], descontando ausentismos e incapacidades.
    pub fn bimestre_con(&self, sdi_centavos: i64, dias: DiasCotizados) -> CuotasBimestre {
        let sdi_real = sdi_centavos as f64 / 100.0;
        let sdi = self.salario_topado(sdi_real);
        CuotasBimestre {
            retiro: cuota(sdi, dias.retiro_e_infonavit(), RETIRO),
            cyv_patronal: cuota(sdi, dias.otros_ramos(), self.cyv_patronal_para(sdi_real)),
            cyv_obrero: cuota(sdi, dias.otros_ramos(), CYV_OBRERO),
            infonavit: cuota(sdi, dias.retiro_e_infonavit(), INFONAVIT),
            amortizacion: 0,
            seguro_vivienda: 0,
        }
    }

    /// Amortización de un crédito INFONAVIT en `dias` días de un bimestre de
    /// `dias_del_bimestre` días naturales.
    ///
    /// - Tipo 1, porcentaje: `salario × días × porcentaje / 100`, con el salario topado.
    /// - Tipo 2, cuota fija mensual: `cuota × 2 / días del bimestre × días`.
    /// - Tipo 3, factor mensual: `factor × unidad × 2 / días del bimestre × días`;
    ///   `unidad` es la UMI diaria del año (el salario mínimo antes de 2016).
    ///
    /// `valor` va en diezmilésimas. **Sin comprobar con un caso real.**
    pub fn amortizacion(&self, tipo_descuento: u8, valor: i64, sdi_centavos: i64, dias: u32, dias_del_bimestre: u32, unidad: f64) -> i64 {
        let valor = valor as f64 / 10_000.0;
        let proporcion = |mensual: f64| centavos(mensual * 2.0 / f64::from(dias_del_bimestre.max(1)) * f64::from(dias));
        match tipo_descuento {
            1 => cuota(self.salario_topado(sdi_centavos as f64 / 100.0), dias, valor),
            2 => proporcion(valor),
            3 => proporcion(valor * unidad),
            _ => 0,
        }
    }
}

#[cfg(test)]
pub(crate) mod pruebas {
    use super::*;

    pub(crate) fn tasas(anio: u32, base: f64, decreto: u8) -> Tasas {
        let (cuota_fija, excedente_patronal, excedente_obrero) = tasas_eym(anio);
        Tasas {
            base_cuota_fija: base,
            cuota_fija,
            excedente_patronal,
            excedente_obrero,
            prima_rt: 7.58875,
            cyv_patronal: 3.15,
            descuento_decreto: decreto,
            tope_veces: 25.0,
            cyv_patronal_por_rango: Vec::new(),
            salario_minimo: base,
        }
    }

    /// LSS art. 31: el ausentismo solo deja enfermedades y maternidad; la
    /// incapacidad solo deja retiro (y la aportación al INFONAVIT).
    #[test]
    fn ausentismos_e_incapacidades_descuentan_dias_por_ramo() {
        let t = tasas(2008, 52.59, 0);
        let sdi = 20_000; // con excedente de tres salarios mínimos
        let d = DiasCotizados { naturales: 30, ausentismo: 2, incapacidad: 3 };
        assert_eq!((d.enfermedades(), d.otros_ramos(), d.retiro_e_infonavit()), (27, 25, 28));
        let (m, eym, otros) = (t.mes_con(sdi, d), t.mes(sdi, 27), t.mes(sdi, 25));
        assert!(eym.excedente_patronal > 0);
        assert_eq!(
            (m.cuota_fija, m.excedente_patronal, m.excedente_obrero, m.pd_patronal, m.pd_obrero, m.gmp_patronal, m.gmp_obrero),
            (eym.cuota_fija, eym.excedente_patronal, eym.excedente_obrero, eym.pd_patronal, eym.pd_obrero, eym.gmp_patronal, eym.gmp_obrero)
        );
        assert_eq!((m.riesgos_trabajo, m.iv_patronal, m.iv_obrero, m.guarderias), (otros.riesgos_trabajo, otros.iv_patronal, otros.iv_obrero, otros.guarderias));
        let b = t.bimestre_con(sdi, d);
        assert_eq!((b.retiro, b.infonavit), (t.bimestre(sdi, 28).retiro, t.bimestre(sdi, 28).infonavit));
        assert_eq!((b.cyv_patronal, b.cyv_obrero), (t.bimestre(sdi, 25).cyv_patronal, t.bimestre(sdi, 25).cyv_obrero));
        // Sin incidencias es el cálculo de siempre.
        assert_eq!(t.mes_con(sdi, DiasCotizados::completos(30)), t.mes(sdi, 30));
        // Todo el mes incapacitado: no hay cuotas del IMSS; sí retiro e INFONAVIT.
        let todo = DiasCotizados { naturales: 30, ausentismo: 0, incapacidad: 30 };
        assert_eq!(t.mes_con(sdi, todo).total(), 0);
        let b = t.bimestre_con(sdi, todo);
        assert_eq!((b.retiro, b.cyv_patronal, b.cyv_obrero, b.infonavit), (12_000, 0, 0, 30_000));
    }

    #[test]
    fn amortizacion_de_creditos() {
        let t = tasas(2026, 117.31, 0);
        // Porcentaje: 25 % de 400.00 por 61 días.
        assert_eq!(t.amortizacion(1, 250_000, 40_000, 61, 61, 100.81), 610_000);
        // El porcentaje se aplica sobre el salario topado a 25 veces la base.
        assert_eq!(t.amortizacion(1, 250_000, 500_000, 10, 61, 100.81), t.amortizacion(1, 250_000, 293_275, 10, 61, 100.81));
        // Cuota fija de 1,500.00 al mes: 3,000.00 por el bimestre completo; proporcional con menos días.
        assert_eq!(t.amortizacion(2, 15_000_000, 40_000, 61, 61, 100.81), 300_000);
        assert_eq!(t.amortizacion(2, 15_000_000, 40_000, 30, 61, 100.81), 147_541);
        // Factor 28.5320 × UMI 100.81 × 2 = 5,752.62 por el bimestre completo.
        assert_eq!(t.amortizacion(3, 285_320, 40_000, 59, 59, 100.81), 575_262);
        assert_eq!(t.amortizacion(3, 285_320, 40_000, 0, 59, 100.81), 0);
        assert_eq!(t.amortizacion(0, 285_320, 40_000, 59, 59, 100.81), 0);
    }

    #[test]
    fn tasas_por_anio() {
        assert_eq!(tasas_eym(1997), (13.9, 6.0, 2.0));
        assert_eq!(tasas_eym(2006), (19.1, 2.08, 0.72));
        assert_eq!(tasas_eym(2008), (20.4, 1.1, 0.4));
        assert_eq!(tasas_eym(2026), (20.4, 1.1, 0.4));
    }

    /// Renglones de la Emisión del IMSS de septiembre de 2013 (solo importes).
    #[test]
    fn coincide_con_la_emision_del_imss_de_2013() {
        let t = tasas(2013, 64.76, 0);
        let c = t.mes(15_000, 29);
        assert_eq!(
            [c.cuota_fija, c.excedente_patronal, c.pd_patronal, c.pd_obrero, c.gmp_patronal, c.gmp_obrero],
            [38_312, 0, 3_045, 1_088, 4_568, 1_631]
        );
        assert_eq!([c.riesgos_trabajo, c.iv_patronal, c.iv_obrero, c.guarderias, c.total()], [33_011, 7_613, 2_719, 4_350, 96_337]);

        let c = t.mes(10_000, 9);
        assert_eq!([c.cuota_fija, c.pd_patronal, c.pd_obrero, c.gmp_patronal, c.gmp_obrero], [11_890, 630, 225, 945, 338]);
        assert_eq!([c.riesgos_trabajo, c.iv_patronal, c.iv_obrero, c.guarderias, c.total()], [6_830, 1_575, 563, 900, 23_896]);

        let c = t.mes(10_000, 22);
        assert_eq!([c.cuota_fija, c.riesgos_trabajo, c.total()], [29_064, 16_695, 58_409]);
    }

    /// Importes tomados de archivos de pago reales (solo números).
    #[test]
    fn coincide_con_archivos_de_pago_reales() {
        // Febrero de 2008, SDI 100.00, 29 días del mes y 60 del bimestre.
        let t = tasas(2008, 52.59, 0);
        let c = t.mes(10_000, 29);
        assert_eq!(c.cuota_fija, 31_112);
        assert_eq!(c.pd_patronal + c.pd_obrero, 2_755);
        assert_eq!(c.gmp_patronal + c.gmp_obrero, 4_133);
        assert_eq!(c.riesgos_trabajo, 22_007);
        assert_eq!(c.iv_patronal + c.iv_obrero, 6_888);
        assert_eq!(c.guarderias, 2_900);
        assert_eq!([c.pd_obrero, c.gmp_obrero, c.iv_obrero], [725, 1_088, 1_813]);
        let b = t.bimestre(10_000, 60);
        assert_eq!([b.retiro, b.cyv_patronal, b.cyv_obrero, b.infonavit], [12_000, 18_900, 6_750, 30_000]);

        // SDI 90.00 por 22 días: 7.425 de gastos médicos obrera se paga como 7.43.
        assert_eq!(t.mes(9_000, 22).gmp_obrero, 743);

        // Mayo de 2006, SDI 200.00 (hay excedente de 3 salarios mínimos de 48.67).
        let t = tasas(2006, 48.67, 0);
        let c = t.mes(20_000, 26);
        assert_eq!(c.cuota_fija, 24_170);
        assert_eq!(c.excedente_patronal + c.excedente_obrero, 3_931);
        assert_eq!(c.excedente_obrero, 1_011);
        // 200 × 26 × 7.58875 % = 394.615: en decimal sería 394.62; el SUA paga 394.61.
        assert_eq!(c.riesgos_trabajo, 39_461);
        // Junio de 2006, 30 días: 455.325 se paga como 455.32.
        assert_eq!(t.mes(20_000, 30).riesgos_trabajo, 45_532);
    }

    /// Junio de 2008 con el decreto del 5 %: SDI 90.00, 7 días.
    #[test]
    fn decreto_del_cinco_por_ciento() {
        // Ese archivo traía la prima vigente desde marzo de 2008: 6.58875 %.
        let mut t = tasas(2008, 52.59, 5);
        t.prima_rt = 6.58875;
        let c = t.mes(9_000, 7);
        assert_eq!(c.cuota_fija, 7_135);
        assert_eq!(c.pd_patronal + c.pd_obrero, 577);
        assert_eq!(c.gmp_patronal + c.gmp_obrero, 865);
        assert_eq!(c.riesgos_trabajo, 3_943);
        assert_eq!(c.iv_patronal + c.iv_obrero, 1_442);
        assert_eq!(c.guarderias, 599);
        // La parte obrera no lleva descuento.
        assert_eq!([c.pd_obrero, c.gmp_obrero, c.iv_obrero], [158, 236, 394]);
        // RCV e INFONAVIT tampoco.
        let b = tasas(2008, 52.59, 5).bimestre(9_000, 38);
        assert_eq!([b.retiro, b.cyv_patronal, b.cyv_obrero, b.infonavit], [6_840, 10_773, 3_848, 17_100]);
    }

    /// Regla de ley, sin muestra real que la confirme.
    #[test]
    fn tope_de_veinticinco_veces() {
        let t = tasas(2008, 52.59, 0);
        let topado = t.mes(131_475, 30); // 25 × 52.59 = 1,314.75
        assert_eq!(t.mes(500_000, 30), topado);
        assert_eq!(t.bimestre(500_000, 60), t.bimestre(131_475, 60));
        assert!(t.mes(131_474, 30).guarderias < topado.guarderias);
    }

    #[test]
    fn cesantia_patronal_por_rangos() {
        let mut t = tasas(2026, 117.31, 0);
        t.salario_minimo = 315.04;
        t.cyv_patronal_por_rango =
            vec![(1.0, 3.15), (1.5, 3.676), (2.0, 4.851), (2.5, 5.556), (3.0, 6.026), (3.5, 6.361), (4.0, 6.613), (f64::MAX, 7.513)];
        // Quien gana el mínimo paga 3.150 % aunque equivalga a más de 1.5 UMA.
        assert_eq!(t.cyv_patronal_para(315.04), 3.15);
        assert_eq!(t.cyv_patronal_para(330.0), 6.026); // 2.81 UMA → rango 2.51 a 3.00
        assert_eq!(t.cyv_patronal_para(360.0), 6.361); // 3.07 UMA
        assert_eq!(t.cyv_patronal_para(1_000.0), 7.513);
        // Sin tabla (hasta 2022) rige la tasa única.
        assert_eq!(tasas(2022, 96.22, 0).cyv_patronal_para(1_000.0), 3.15);
    }
}
