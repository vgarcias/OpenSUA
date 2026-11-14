# -*- coding: utf-8 -*-
"""
Validador de archivos .SUA de prueba.

Uso:
    python3 valida_sua.py                    # valida todos los *.SUA válidos
    python3 valida_sua.py ERRORES_202609.SUA # valida un archivo específico
    python3 valida_sua.py --csv              # además compara contra los CSV esperados

Salida: reporte por archivo y código de salida 0 si todo está correcto.
El archivo ERRORES_202609.SUA DEBE fallar; sus defectos están listados en
el README (sección de pruebas negativas).
"""

import csv
import glob
import os
import sys
from decimal import Decimal, ROUND_HALF_UP

D = Decimal
DIR = os.path.dirname(os.path.abspath(__file__))

UMA = D("117.31")
TOPE = D("25") * UMA
TASAS = dict(cf=D("0.2040"), pd=D("0.0025"), gmp=D("0.0105"), exc=D("0.0040"),
             ivp=D("0.0175"), ivo=D("0.00625"), guard=D("0.0100"),
             ret=D("0.0200"), cvp=D("0.0315"), cvo=D("0.01125"),
             inf=D("0.0500"))

# posiciones (inicio, longitud) de cada registro — ver LAYOUT_SUA.md
H = [(1,1),(2,5),(7,11),(18,13),(31,50),(81,6),(87,7),(94,2),(96,8),(104,6),
     (110,4),(114,7),(121,7),(128,11),(139,11),(150,11),(161,11)]
T = [(1,1),(2,11),(13,5),(18,5),(23,7),(30,13),(43,11),(54,11),(65,11),(76,11)]
D2 = [(1,1),(2,11),(13,11),(24,50),(74,13),(87,18),(105,8),(113,7),(120,3),
      (123,3),(126,9),(135,9),(144,9),(153,9),(162,9),(171,9),(180,9),(189,9),
      (198,9),(207,9),(216,9),(225,9),(234,9),(243,9),(252,9),(261,9),(270,9),
      (279,9),(288,11)]

H_NOMBRES = ["tipo","version","rp","rfc_patron","razon","periodo","bimestre",
             "tipo_doc","fecha_gen","folio","cotizantes","prima_rt","uma",
             "tot_imss","tot_rcv","tot_infonavit","tot_total"]
T_NOMBRES = ["tipo","rp","num_detalle","cotizantes","tot_dias_mes","tot_sbc_mes",
             "tot_imss","tot_rcv","tot_infonavit","tot_total"]
D_NOMBRES = ["tipo","rp","nss","nombre","rfc","curp","fecha_alta","sbc",
             "dias_mes","dias_bim","cf_eym","eym_pd_pat","eym_pd_obr","eym_gmp",
             "eym_exc_pat","eym_exc_obr","rt","iv_pat","iv_obr","guard",
             "sub_imss_pat","sub_imss_obr","total_imss","retiro","cv_pat",
             "cv_obr","sub_rcv","infonavit","total_trab"]

def extraer(linea, spec, nombres):
    out = {}
    for (pos, lon), nom in zip(spec, nombres):
        out[nom] = linea[pos-1:pos-1+lon]
    return out

def num(s, errores, ctx):
    if not s.isdigit():
        errores.append(f"{ctx}: campo numérico inválido {s!r}")
        return None
    return int(s)

def a_pesos(s):
    return D(s) / 100

def parse(path):
    errores = []
    data = {"archivo": os.path.basename(path), "detalles": [], "errores": []}
    with open(path, "r", newline="") as fh:
        lineas = fh.read().split("\r\n")
    lineas = [l for l in lineas if l != ""]

    if len(lineas) < 3:
        errores.append("el archivo debe tener encabezado, al menos un detalle y trailer")
        data["errores"] = errores
        return data

    # --- encabezado ---
    enc = lineas[0]
    if len(enc) != 171:
        errores.append(f"encabezado con longitud {len(enc)} (se esperaban 171)")
    e = extraer(enc, H, H_NOMBRES)
    if e["tipo"] != "1":
        errores.append(f"primer registro no es encabezado (tipo={e['tipo']!r})")
    data["encabezado"] = e
    prima_rt = None
    if e["prima_rt"].isdigit() and len(e["prima_rt"]) == 7:
        # prima RT: 2 enteros y 5 decimales (porcentaje), ej. 0245000 = 2.45 %
        prima_rt = D(e["prima_rt"]) / D(10**7)

    # --- detalles ---
    nss_vistos = set()
    for i, linea in enumerate(lineas[1:-1], start=1):
        if linea[:1] != "2":
            errores.append(f"detalle #{i}: tipo de registro inválido {linea[:1]!r}")
            continue
        if len(linea) != 298:
            errores.append(f"detalle #{i}: longitud {len(linea)} (se esperaban 298)")
            continue
        d = extraer(linea, D2, D_NOMBRES)
        if not d["nss"].isdigit():
            errores.append(f"detalle #{i}: NSS no numérico {d['nss']!r}")
        elif d["nss"] in nss_vistos:
            errores.append(f"detalle #{i}: NSS duplicado {d['nss']}")
        nss_vistos.add(d["nss"])
        for k in ("dias_mes", "dias_bim"):
            d[k] = num(d[k], errores, f"detalle #{i} {k}")
        d["prima_rt"] = prima_rt
        data["detalles"].append(d)

    # --- trailer ---
    tra = lineas[-1]
    if tra[:1] != "3":
        errores.append(f"último registro no es trailer (tipo={tra[:1]!r})")
    elif len(tra) != 86:
        errores.append(f"trailer con longitud {len(tra)} (se esperaban 86)")
    t = extraer(tra, T, T_NOMBRES) if len(tra) >= 76 else {}
    data["trailer"] = t

    # --- recálculo de cuotas por trabajador ---
    tot = dict(imss=D(0), rcv=D(0), inf=D(0), total=D(0), dias=0)
    for i, d in enumerate(data["detalles"], start=1):
        if any(v is None for v in (d.get("dias_mes"), d.get("dias_bim"))):
            continue
        sbc = a_pesos(d["sbc"]); dias = d["dias_mes"]; dbim = d["dias_bim"]
        prt = d.get("prima_rt")
        sbc_cap = min(sbc, TOPE)
        base = sbc_cap * dias
        plena = sbc * dias
        exc = max(D(0), sbc_cap - 3*UMA) * dias
        r2 = lambda x: x.quantize(D("0.01"), rounding=ROUND_HALF_UP)
        esp = dict(
            cf_eym=r2(TASAS["cf"]*UMA*dias),
            eym_pd_pat=r2(TASAS["pd"]*base), eym_pd_obr=r2(TASAS["pd"]*base),
            eym_gmp=r2(TASAS["gmp"]*base),
            eym_exc_pat=r2(TASAS["exc"]*exc), eym_exc_obr=r2(TASAS["exc"]*exc),
            rt=r2(prt*plena) if prt is not None else None,
            iv_pat=r2(TASAS["ivp"]*base), iv_obr=r2(TASAS["ivo"]*base),
            guard=r2(TASAS["guard"]*base))
        pat = (esp["cf_eym"] + esp["eym_pd_pat"] + esp["eym_gmp"] +
               esp["eym_exc_pat"] + esp["rt"] + esp["iv_pat"] + esp["guard"])
        obr = esp["eym_pd_obr"] + esp["eym_exc_obr"] + esp["iv_obr"]
        esp["sub_imss_pat"] = pat
        esp["sub_imss_obr"] = obr
        esp["total_imss"] = pat + obr
        esp["retiro"] = r2(TASAS["ret"]*sbc*dbim)
        esp["cv_pat"] = r2(TASAS["cvp"]*sbc_cap*dbim)
        esp["cv_obr"] = r2(TASAS["cvo"]*sbc_cap*dbim)
        esp["sub_rcv"] = esp["retiro"] + esp["cv_pat"] + esp["cv_obr"]
        esp["infonavit"] = r2(TASAS["inf"]*sbc*dbim)
        esp["total_trab"] = esp["total_imss"] + esp["sub_rcv"] + esp["infonavit"]

        # Archivo IMSS-solo (primer mes del bimestre): RCV e INFONAVIT en
        # ceros; el total del trabajador es solo el total IMSS.
        imss_solo = a_pesos(d["sub_rcv"]) == 0
        if imss_solo:
            esp["retiro"] = esp["cv_pat"] = esp["cv_obr"] = D("0.00")
            esp["sub_rcv"] = esp["infonavit"] = D("0.00")
            esp["total_trab"] = esp["total_imss"]

        for k, v in esp.items():
            if v is None:
                continue
            real = a_pesos(d[k])
            if real != v:
                errores.append(f"detalle #{i} {d['nss']} {k}: archivo={real} esperado={v}")
        tot["imss"] += esp["total_imss"]
        tot["dias"] += dias
        if a_pesos(d["sub_rcv"]) > 0:
            tot["rcv"] += esp["sub_rcv"]
            tot["inf"] += esp["infonavit"]
        tot["total"] += esp["total_trab"]

    # --- consistencia encabezado vs suma de detalles ---
    if e.get("tot_imss","").isdigit() and "detalles" in data:
        if a_pesos(e["tot_imss"]) != tot["imss"]:
            errores.append(f"total IMSS del encabezado ({a_pesos(e['tot_imss'])}) "
                           f"!= suma de detalles ({tot['imss']})")
        if a_pesos(e["tot_rcv"]) != tot["rcv"]:
            errores.append(f"total RCV del encabezado ({a_pesos(e['tot_rcv'])}) "
                           f"!= suma de detalles ({tot['rcv']})")
        if a_pesos(e["tot_infonavit"]) != tot["inf"]:
            errores.append(f"total INFONAVIT del encabezado ({a_pesos(e['tot_infonavit'])}) "
                           f"!= suma de detalles ({tot['inf']})")
        if a_pesos(e["tot_total"]) != tot["total"]:
            errores.append(f"total general del encabezado ({a_pesos(e['tot_total'])}) "
                           f"!= suma de detalles ({tot['total']})")
    # --- trailer vs detalles ---
    if t:
        if t.get("num_detalle","").isdigit():
            nd = int(t["num_detalle"])
            if nd != len(data["detalles"]):
                errores.append(f"trailer: num registros ({nd}) != detalles reales "
                               f"({len(data['detalles'])})")
        if t.get("tot_imss","").isdigit() and "detalles" in data:
            if a_pesos(t["tot_imss"]) != tot["imss"]:
                errores.append(f"total IMSS del trailer ({a_pesos(t['tot_imss'])}) "
                               f"!= suma de detalles ({tot['imss']})")

    data["errores"] = errores
    return data

def main():
    args = [a for a in sys.argv[1:] if a != "--csv"]
    con_csv = "--csv" in sys.argv
    archivos = args or sorted(glob.glob(os.path.join(DIR, "*.SUA")))
    falla_global = False
    for path in archivos:
        data = parse(path)
        base = os.path.basename(path)
        if data["errores"]:
            falla_global = True
            print(f"[FALLA] {base}:")
            for err in data["errores"]:
                print(f"    - {err}")
        else:
            n = len(data["detalles"])
            print(f"[OK]    {base}: {n} trabajadores, totales consistentes")
        if con_csv and not data["errores"]:
            comparar_csv(data)
    sys.exit(1 if falla_global else 0)

def comparar_csv(data):
    """Compara el detalle parseado contra esperados_detalle.csv."""
    ruta = os.path.join(DIR, "esperados_detalle.csv")
    if not os.path.exists(ruta):
        return
    with open(ruta, newline="", encoding="utf-8") as fh:
        filas = [r for r in csv.DictReader(fh) if r["archivo"] == data["archivo"]]
    if not filas:
        return
    por_nss = {f["nss"]: f for f in filas}
    ok = True
    for d in data["detalles"]:
        esp = por_nss.get(d["nss"])
        if not esp:
            print(f"    [csv] NSS {d['nss']} no encontrado en esperados_detalle.csv")
            ok = False
            continue
        if a_pesos(d["sbc"]) != D(esp["sbc"]) or d["dias_mes"] != int(esp["dias_mes"]):
            print(f"    [csv] {d['nss']}: SBC o días no coinciden con CSV")
            ok = False
    if ok:
        print(f"    [csv] detalle coincide con esperados_detalle.csv")

if __name__ == "__main__":
    main()
