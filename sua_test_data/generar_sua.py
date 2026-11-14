# -*- coding: utf-8 -*-
"""
Generador de archivos .SUA ficticios para pruebas de un importador SUA.

Los archivos son de longitud fija (ASCII, mayúsculas, CRLF) con tres tipos
de registro: 1 = encabezado, 2 = detalle (trabajador), 3 = trailer.

Los importes se calculan con la LSS 1997 vigente y valores 2026:
  - UMA diaria: 117.31 (tope SBC 25 UMA = 2,932.75)
  - SMG: 315.04
Cuotas IMSS (mensuales) y RCV/INFONAVIT (bimestrales).
Ver LAYOUT_SUA.md para la especificación completa de posiciones.
"""

import csv
import os
from decimal import Decimal, ROUND_HALF_UP

D = Decimal

# ----------------------------------------------------------------------------
# Parámetros 2026
# ----------------------------------------------------------------------------
UMA_DIARIA = D("117.31")
TOPE_SBC = D("25") * UMA_DIARIA            # 2,932.75
SMG = D("315.04")
TASA_CF_EYM = D("0.2040")                   # cuota fija sobre UMA (patronal)
TASA_EYM_PD = D("0.0025")                   # prestaciones en dinero pat/obr
TASA_EYM_GMP = D("0.0105")                  # gastos médicos pensionados (pat)
TASA_EYM_EXC = D("0.0040")                  # excedente 3 UMA (pat/obr)
TASA_IV_PAT = D("0.0175")
TASA_IV_OBR = D("0.00625")
TASA_GUARD = D("0.0100")
TASA_RETIRO = D("0.0200")                   # RCV, sin tope
TASA_CV_PAT = D("0.0315")                   # RCV, tope 25 UMA
TASA_CV_OBR = D("0.01125")
TASA_INFONAVIT = D("0.0500")                # aportación, sin tope

def r2(x):
    return D(x).quantize(D("0.01"), rounding=ROUND_HALF_UP)

def fnum(valor, ent, dec=2):
    """Formatea un Decimal como entero sin punto, relleno de ceros a la izquierda."""
    escala = D(10) ** dec
    n = int((D(valor) * escala).to_integral_value(rounding=ROUND_HALF_UP))
    return str(n).zfill(ent + dec)

def ftxt(s, n):
    s = str(s).upper().replace("Ñ", "N")
    assert len(s) <= n, f"campo demasiado largo: {s!r} ({len(s)} > {n})"
    return s.ljust(n)

def calc_cuotas(sbc, dias_mes, dias_bim, prima_rt):
    """Calcula todas las cuotas de un trabajador. Devuelve dict de Decimals."""
    sbc_cap = min(sbc, TOPE_SBC)
    base_cap = sbc_cap * dias_mes
    base_plena = sbc * dias_mes
    excedente = max(D(0), sbc_cap - 3 * UMA_DIARIA) * dias_mes

    c = {}
    c["cf_eym"] = r2(TASA_CF_EYM * UMA_DIARIA * dias_mes)
    c["eym_pd_pat"] = r2(TASA_EYM_PD * base_cap)
    c["eym_pd_obr"] = r2(TASA_EYM_PD * base_cap)
    c["eym_gmp"] = r2(TASA_EYM_GMP * base_cap)
    c["eym_exc_pat"] = r2(TASA_EYM_EXC * excedente)
    c["eym_exc_obr"] = r2(TASA_EYM_EXC * excedente)
    c["rt"] = r2(prima_rt * base_plena)
    c["iv_pat"] = r2(TASA_IV_PAT * base_cap)
    c["iv_obr"] = r2(TASA_IV_OBR * base_cap)
    c["guard"] = r2(TASA_GUARD * base_cap)

    c["sub_imss_pat"] = (c["cf_eym"] + c["eym_pd_pat"] + c["eym_gmp"] +
                         c["eym_exc_pat"] + c["rt"] + c["iv_pat"] + c["guard"])
    c["sub_imss_obr"] = c["eym_pd_obr"] + c["eym_exc_obr"] + c["iv_obr"]
    c["total_imss"] = c["sub_imss_pat"] + c["sub_imss_obr"]

    base_bim_cap = sbc_cap * dias_bim
    base_bim_plena = sbc * dias_bim
    c["retiro"] = r2(TASA_RETIRO * base_bim_plena)
    c["cv_pat"] = r2(TASA_CV_PAT * base_bim_cap)
    c["cv_obr"] = r2(TASA_CV_OBR * base_bim_cap)
    c["sub_rcv"] = c["retiro"] + c["cv_pat"] + c["cv_obr"]
    c["infonavit"] = r2(TASA_INFONAVIT * base_bim_plena)
    c["total_trab"] = c["total_imss"] + c["sub_rcv"] + c["infonavit"]
    return c

# ----------------------------------------------------------------------------
# Catálogo ficticio de empleadores y trabajadores
# ----------------------------------------------------------------------------
def T(nss, nombre, rfc, curp, alta, sbc, dias_mes, dias_bim):
    return dict(nss=nss, nombre=nombre, rfc=rfc, curp=curp,
                alta=alta, sbc=D(sbc), dias_mes=dias_mes, dias_bim=dias_bim)

EMPLEADORES = {
    "alpha": dict(
        rp="A4623891107", rfc="ACO160315K12",
        razon="ALPHA CONSTRUCCIONES DE PRUEBA SA DE CV", prima_rt=D("0.02450"),
        folio="000001", gen="20261005"),
    "beta": dict(
        rp="B7319052208", rfc="BSE200718Q3A",
        razon="BETA SERVICIOS INTEGRales DEMO S DE RL DE CV", prima_rt=D("0.00825"),
        folio="000002", gen="20261006"),
    "gamma": dict(
        rp="C1104578903", rfc="GTX940902HJ7",
        razon="GAMMA TEXTIL DEMOSTRACION SA DE CV", prima_rt=D("0.03050"),
        folio="000003", gen="20261107"),
    "delta": dict(
        rp="D5682031401", rfc="DHC180612P9B",
        razon="DELTA HACIENDAS DEL CARIBE DEMO SA DE CV", prima_rt=D("0.01675"),
        folio="000004", gen="20261205"),
    "epsilon": dict(
        rp="E9043126705", rfc="EPS250108R2C",
        razon="EPSILON COMERCIAL FICTICIA S DE RL DE CV", prima_rt=D("0.01100"),
        folio="000005", gen="20261005"),
}

# Cada archivo: (clave_empleador, periodo_mes, bimestre, tipo_doc, con_rcv, [trabajadores])
# con_rcv = True -> el archivo incluye RCV e INFONAVIT del bimestre completo
#               (mes de cierre del bimestre); False -> solo cuotas IMSS del mes.
ARCHIVOS = [
    # --- Bimestre sep-oct 2026 (bimestre 5) -------------------------------
    ("alpha", "202609", "202605", "01", False, [
        # variedad: SMG exacto, salarios medios, tope exacto, alta a mitad de mes
        T("11058423014", "RAMIREZ COBOS MARIO ALBERTO", "RACM870312HDF", "RACM870312HDFRRL09", "20200316", "315.04", 30, 60),
        T("11058423021", "GOMEZ PATRON LUCIA TERESA", "GOPL900821MDF", "GOPL900821MDFHGN03", "20190701", "485.60", 30, 60),
        T("11058423038", "CANUL PUC JORGE LUIS", "CAPU850419QRO", "CAPU850419QROXPR07", "20180212", "612.35", 30, 59),
        T("11058423045", "MARTINEZ SOLIS ANA BEATRIZ", "MASA920607MQR", "MASA920607MQRMMD11", "20211115", "780.00", 30, 61),
        T("11058423052", "PEREZ CANCHE FERNANDO", "PECF881230YUC", "PECF881230YUCRRT05", "20170904", "925.80", 30, 60),
        T("11058423069", "CETINA BAAS ROCIO DEL CARMEN", "CEBR950318CYU", "CEBR950318CYUNNL08", "20230301", "1180.50", 30, 61),
        T("11058423076", "UCHIM MAY PEDRO PABLO", "UMPP820710CQR", "UMPP820710CQRZLR06", "20160523", "1560.25", 30, 60),
        T("11058423083", "SOSA ARANDA GUADALUPE", "SOAG790925MQR", "SOAG790925MQRTDP04", "20150209", "2100.00", 30, 61),
        T("11058423090", "NOH CUIS RICARDO", "NOCR860514CQR", "NOCR860514CQRBBM10", "20240610", "2932.75", 30, 60),   # tope exacto 25 UMA
        T("11058423106", "BRICENO LOPEZ SILVIA ELENA", "BRLS931102MQR", "BRLS931102MQRLDN12", "20260916", "540.00", 15, 46),  # alta 16-sep
    ]),
    ("alpha", "202610", "202605", "01", True, [
        T("11058423014", "RAMIREZ COBOS MARIO ALBERTO", "RACM870312HDF", "RACM870312HDFRRL09", "20200316", "315.04", 30, 60),
        T("11058423021", "GOMEZ PATRON LUCIA TERESA", "GOPL900821MDF", "GOPL900821MDFHGN03", "20190701", "485.60", 30, 60),
        T("11058423038", "CANUL PUC JORGE LUIS", "CAPU850419QRO", "CAPU850419QROXPR07", "20180212", "612.35", 29, 59),
        T("11058423045", "MARTINEZ SOLIS ANA BEATRIZ", "MASA920607MQR", "MASA920607MQRMMD11", "20211115", "780.00", 31, 61),
        T("11058423052", "PEREZ CANCHE FERNANDO", "PECF881230YUC", "PECF881230YUCRRT05", "20170904", "925.80", 30, 60),
        T("11058423069", "CETINA BAAS ROCIO DEL CARMEN", "CEBR950318CYU", "CEBR950318CYUNNL08", "20230301", "1180.50", 31, 61),
        T("11058423076", "UCHIM MAY PEDRO PABLO", "UMPP820710CQR", "UMPP820710CQRZLR06", "20160523", "1560.25", 30, 60),
        T("11058423083", "SOSA ARANDA GUADALUPE", "SOAG790925MQR", "SOAG790925MQRTDP04", "20150209", "2100.00", 31, 61),
        T("11058423090", "NOH CUIS RICARDO", "NOCR860514CQR", "NOCR860514CQRBBM10", "20240610", "2932.75", 30, 60),
        T("11058423106", "BRICENO LOPEZ SILVIA ELENA", "BRLS931102MQR", "BRLS931102MQRLDN12", "20260916", "540.00", 31, 46),
    ]),
    ("beta", "202610", "202605", "01", True, [
        T("31096748012", "ALVARADO ZETINA JOSE MANUEL", "AAZJ840208CQR", "AAZJ840208CQRMPS06", "20170110", "350.00", 30, 61),
        T("31096748029", "BAZAR ALONSO MARIA DE LOS ANGELES", "BAAM880517YUC", "BAAM880517YUCRRL01", "20190422", "520.45", 30, 60),
        T("31096748036", "COLLI CANUL VICTOR HUGO", "CACV910930CQR", "CACV910930CQRNDL05", "20200803", "689.90", 30, 60),
        T("31096748043", "DZUL AKAL CAROLINA", "DIAC930412CYU", "DIAC930412CYUHHT07", "20181015", "845.30", 30, 61),
        T("31096748050", "ESPADAS LOPEZ RAUL", "ELOR860727CAM", "ELOR860727CAMPPR02", "20160307", "1020.75", 30, 61),
        T("31096748067", "FUENTES MENDEZ ADRIAN", "FUMA890119QRO", "FUMA890119QROZZS09", "20220118", "1290.60", 30, 60),
        T("31096748074", "GONGORA PECH LAURA PATRICIA", "GOPL950823YUC", "GOPL950823YUCMMN04", "20171102", "1675.40", 30, 61),
        T("31096748081", "HUCHIM BECERRA JAVIER", "HUBJ820605CQR", "HUBJ820605CQRTTT08", "20150928", "2932.75", 30, 61),  # tope
    ]),
    # --- Bimestre nov-dic 2026 (bimestre 6) -------------------------------
    ("gamma", "202611", "202606", "01", False, [
        T("66120378019", "IBARRA QUINTAL ROSA MARIA", "IQRM860214YUC", "IQRM860214YUCLLL05", "20180611", "410.20", 30, 61),
        T("66120378026", "JIMENEZ SANTOS LUIS FERNANDO", "JISL830809CQR", "JISL830809CQRNNR07", "20170220", "555.75", 30, 61),
        T("66120378033", "KOYOC MAY DIANA LAURA", "KOMD910523CAM", "KOMD910523CAMPPB02", "20191001", "698.25", 30, 60),
        T("66120378040", "LOPEZ UCAN ESTEBAN", "LOUE870716CYU", "LOUE870716CYUXXX03", "20160425", "875.10", 30, 60),
        T("66120378057", "MEX COUOH ANAIS SOFIA", "MECA940928QRO", "MECA940928QROHHR09", "20210913", "1050.00", 30, 61),
        T("66120378064", "NAVARRO TUN JORGE ANTONIO", "NATJ880331CQR", "NATJ880331CQRDDD06", "20180808", "1345.80", 30, 61),
        T("66120378071", "OCAMPO POOT MARIO ERNESTO", "OAPM850117YUC", "OAPM850117YUCRRR01", "20170314", "1780.35", 30, 60),
        T("66120378088", "PUC XOOL VERONICA", "PUXV920624CAM", "PUXV920624CAMSSS10", "20220601", "2450.00", 30, 61),
        T("66120378095", "QUIJANO ROSADO PABLO", "QURP840910QRO", "QURP840910QROFFF08", "20151130", "4500.00", 30, 61),  # arriba del tope
    ]),
    ("gamma", "202612", "202606", "01", True, [
        T("66120378019", "IBARRA QUINTAL ROSA MARIA", "IQRM860214YUC", "IQRM860214YUCLLL05", "20180611", "410.20", 31, 61),
        T("66120378026", "JIMENEZ SANTOS LUIS FERNANDO", "JISL830809CQR", "JISL830809CQRNNR07", "20170220", "555.75", 31, 61),
        T("66120378033", "KOYOC MAY DIANA LAURA", "KOMD910523CAM", "KOMD910523CAMPPB02", "20191001", "698.25", 30, 60),
        T("66120378040", "LOPEZ UCAN ESTEBAN", "LOUE870716CYU", "LOUE870716CYUXXX03", "20160425", "875.10", 30, 60),
        T("66120378057", "MEX COUOH ANAIS SOFIA", "MECA940928QRO", "MECA940928QROHHR09", "20210913", "1050.00", 31, 61),
        T("66120378064", "NAVARRO TUN JORGE ANTONIO", "NATJ880331CQR", "NATJ880331CQRDDD06", "20180808", "1345.80", 31, 61),
        T("66120378071", "OCAMPO POOT MARIO ERNESTO", "OAPM850117YUC", "OAPM850117YUCRRR01", "20170314", "1780.35", 30, 60),
        T("66120378088", "PUC XOOL VERONICA", "PUXV920624CAM", "PUXV920624CAMSSS10", "20220601", "2450.00", 31, 61),
        T("66120378095", "QUIJANO ROSADO PABLO", "QURP840910QRO", "QURP840910QROFFF08", "20151130", "4500.00", 31, 61),  # arriba del tope
    ]),
    ("delta", "202611", "202606", "01", False, [
        T("84157690015", "ROSADO VARGAS CLAUDIA", "ROVC890405CQR", "ROVC890405CQRMMM02", "20190506", "400.00", 30, 61),
        T("84157690022", "SANCHEZ AKAL MANUEL JOSE", "SAAM820919YUC", "SAAM820919YUCNNN07", "20171211", "530.60", 30, 61),
        T("84157690039", "TUN CANUL BEATRIZ ADRIANA", "TUCB910803CYU", "TUCB910803CYUQQS04", "20210315", "660.15", 30, 60),
        T("84157690046", "UCAN PECH SERGIO", "UCPS880627CQR", "UCPS880627CQRZZA06", "20180129", "915.85", 30, 61),
        T("84157690053", "VAZQUEZ BAAS MONICA", "VABM931110QRO", "VABM931110QROTTT09", "20200720", "1330.40", 30, 61),
        T("84157690060", "XOOL COHUO JUAN CARLOS", "XOCJ860422CAM", "XOCJ860422CAMHHH03", "20161017", "1885.90", 30, 60),
    ]),
    ("epsilon", "202611", "202606", "02", False, [   # pago extemporáneo, 1 solo trabajador
        T("57031842017", "ZAPATA ALVAREZ RICARDO", "ZAAR840515QRO", "ZAAR840515QROBBB05", "20200901", "2760.50", 30, 61),
    ]),
]

NOMBRES_ARCHIVO = {
    ("alpha", "202609"): "ALPHA_202609.SUA",
    ("alpha", "202610"): "ALPHA_202610.SUA",
    ("beta", "202610"): "BETA_202610.SUA",
    ("gamma", "202611"): "GAMMA_202611.SUA",
    ("gamma", "202612"): "GAMMA_202612.SUA",
    ("delta", "202611"): "DELTA_202611.SUA",
    ("epsilon", "202611"): "EPSILON_202611.SUA",
}

OUT = os.path.dirname(os.path.abspath(__file__))

# ----------------------------------------------------------------------------
# Construcción de registros
# ----------------------------------------------------------------------------
def registro_encabezado(emp, periodo, bimestre, tipo_doc, tot):
    r = (ftxt("1", 1) + ftxt("3.7.1", 5) + ftxt(emp["rp"], 11) +
         ftxt(emp["rfc"], 13) + ftxt(emp["razon"], 50) +
         ftxt(periodo, 6) + ftxt(bimestre, 7) + ftxt(tipo_doc, 2) +
         ftxt(emp["gen"], 8) + ftxt(emp["folio"], 6) +
         fnum(tot["cotizantes"], 4, 0) +
         fnum(emp["prima_rt"] * 100, 2, 5) +
         fnum(UMA_DIARIA, 5) +
         fnum(tot["imss"], 9) + fnum(tot["rcv"], 9) +
         fnum(tot["infonavit"], 9) + fnum(tot["total"], 9))
    assert len(r) == 171, len(r)
    return r

def registro_detalle(emp, t, c):
    r = (ftxt("2", 1) + ftxt(emp["rp"], 11) + ftxt(t["nss"], 11) +
         ftxt(t["nombre"], 50) + ftxt(t["rfc"], 13) + ftxt(t["curp"], 18) +
         ftxt(t["alta"], 8) +
         fnum(t["sbc"], 5) +
         fnum(t["dias_mes"], 3, 0) + fnum(t["dias_bim"], 3, 0) +
         fnum(c["cf_eym"], 7) + fnum(c["eym_pd_pat"], 7) + fnum(c["eym_pd_obr"], 7) +
         fnum(c["eym_gmp"], 7) + fnum(c["eym_exc_pat"], 7) + fnum(c["eym_exc_obr"], 7) +
         fnum(c["rt"], 7) + fnum(c["iv_pat"], 7) + fnum(c["iv_obr"], 7) +
         fnum(c["guard"], 7) +
         fnum(c["sub_imss_pat"], 7) + fnum(c["sub_imss_obr"], 7) + fnum(c["total_imss"], 7) +
         fnum(c["retiro"], 7) + fnum(c["cv_pat"], 7) + fnum(c["cv_obr"], 7) +
         fnum(c["sub_rcv"], 7) +
         fnum(c["infonavit"], 7) +
         fnum(c["total_trab"], 9))
    assert len(r) == 298, len(r)
    return r

def registro_trailer(emp, n_detalle, tot):
    r = (ftxt("3", 1) + ftxt(emp["rp"], 11) +
         fnum(n_detalle, 5, 0) + fnum(tot["cotizantes"], 5, 0) +
         fnum(tot["dias_mes"], 7, 0) +
         fnum(tot["sbc_mes"], 11) +
         fnum(tot["imss"], 9) + fnum(tot["rcv"], 9) +
         fnum(tot["infonavit"], 9) + fnum(tot["total"], 9))
    assert len(r) == 86, len(r)
    return r

# ----------------------------------------------------------------------------
# Generación
# ----------------------------------------------------------------------------
detalle_rows = []
totales_rows = []

for clave_emp, periodo, bimestre, tipo_doc, con_rcv, trabajadores in ARCHIVOS:
    emp = EMPLEADORES[clave_emp]
    lineas = []
    d_rows = []

    tot = dict(cotizantes=0, dias_mes=0, sbc_mes=D(0), imss=D(0),
               rcv=D(0), infonavit=D(0), total=D(0))
    for t in trabajadores:
        c = calc_cuotas(t["sbc"], t["dias_mes"], t["dias_bim"], emp["prima_rt"])
        if not con_rcv:
            for k in ("retiro", "cv_pat", "cv_obr", "sub_rcv", "infonavit"):
                c[k] = D("0.00")
            c["total_trab"] = c["total_imss"]
        lineas.append(registro_detalle(emp, t, c))
        tot["cotizantes"] += 1
        tot["dias_mes"] += t["dias_mes"]
        tot["sbc_mes"] += t["sbc"] * t["dias_mes"]
        tot["imss"] += c["total_imss"]
        tot["rcv"] += c["sub_rcv"]
        tot["infonavit"] += c["infonavit"]
        tot["total"] += c["total_trab"]
        d_rows.append({
            "archivo": NOMBRES_ARCHIVO[(clave_emp, periodo)],
            "registro_patronal": emp["rp"], "nss": t["nss"], "nombre": t["nombre"],
            "rfc": t["rfc"], "curp": t["curp"], "fecha_alta": t["alta"],
            "sbc": str(t["sbc"]), "dias_mes": t["dias_mes"], "dias_bim": t["dias_bim"],
            **{k: str(v) for k, v in c.items()},
        })

    contenido = ([registro_encabezado(emp, periodo, bimestre, tipo_doc, tot)] +
                 lineas + [registro_trailer(emp, len(lineas), tot)])
    nombre = NOMBRES_ARCHIVO[(clave_emp, periodo)]
    with open(os.path.join(OUT, nombre), "w", newline="", encoding="ascii") as fh:
        fh.write("\r\n".join(contenido) + "\r\n")

    detalle_rows.extend(d_rows)
    totales_rows.append({
        "archivo": nombre, "registro_patronal": emp["rp"],
        "razon_social": emp["razon"], "periodo_imss": periodo,
        "bimestre_rcv": bimestre, "tipo_documento": tipo_doc,
        "num_cotizantes": tot["cotizantes"], "total_dias_mes": tot["dias_mes"],
        "total_sbc_mes": str(tot["sbc_mes"]),
        "total_cuotas_imss": str(tot["imss"]), "total_rcv": str(tot["rcv"]),
        "total_infonavit": str(tot["infonavit"]), "total_general": str(tot["total"]),
        "registros_detalle": len(lineas),
    })
    print(f"OK {nombre}: {len(lineas)} trabajadores, total {tot['total']}")

# ----------------------------------------------------------------------------
# CSV de valores esperados
# ----------------------------------------------------------------------------
cols_det = ["archivo", "registro_patronal", "nss", "nombre", "rfc", "curp",
            "fecha_alta", "sbc", "dias_mes", "dias_bim", "cf_eym",
            "eym_pd_pat", "eym_pd_obr", "eym_gmp", "eym_exc_pat", "eym_exc_obr",
            "rt", "iv_pat", "iv_obr", "guard", "sub_imss_pat", "sub_imss_obr",
            "total_imss", "retiro", "cv_pat", "cv_obr", "sub_rcv", "infonavit",
            "total_trab"]
with open(os.path.join(OUT, "esperados_detalle.csv"), "w", newline="", encoding="utf-8") as fh:
    w = csv.DictWriter(fh, fieldnames=cols_det)
    w.writeheader()
    w.writerows(detalle_rows)

cols_tot = ["archivo", "registro_patronal", "razon_social", "periodo_imss",
            "bimestre_rcv", "tipo_documento", "num_cotizantes", "total_dias_mes",
            "total_sbc_mes", "total_cuotas_imss", "total_rcv", "total_infonavit",
            "total_general", "registros_detalle"]
with open(os.path.join(OUT, "esperados_totales.csv"), "w", newline="", encoding="utf-8") as fh:
    w = csv.DictWriter(fh, fieldnames=cols_tot)
    w.writeheader()
    w.writerows(totales_rows)

print("\nCSV de valores esperados generados.")
print("Resumen general:")
for r in totales_rows:
    print(f"  {r['archivo']}: IMSS {r['total_cuotas_imss']} | RCV {r['total_rcv']} | "
          f"INFONAVIT {r['total_infonavit']} | TOTAL {r['total_general']}")
