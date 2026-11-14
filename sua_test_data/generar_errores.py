# -*- coding: utf-8 -*-
"""
Genera ERRORES_202609.SUA: archivo CON DEFECTOS INTENCIONALES para probar
que el importador rechaza datos inválidos. Ver README.md para la lista
exacta de defectos introducidos.
"""

with open("ALPHA_202609.SUA", "r", newline="") as fh:
    lineas = fh.read().split("\r\n")
lineas = [l for l in lineas if l != ""]

enc = lineas[0]
det = lineas[1:-1]
trailer = lineas[-1]

# Defecto 1: total IMSS del encabezado alterado (+100.00 en posiciones 128-138)
enc_malo = enc[:127] + "00003350246".zfill(11) + enc[138:]

# Defecto 2: NSS con letra en el registro 4 (posición 13-23)
r4 = det[3]
det[3] = r4[:12] + "11058423A45" + r4[23:]

# Defecto 3: registro 7 truncado a 250 caracteres
det[6] = det[6][:250]

# Defecto 4: tipo de registro inválido ("9") en el registro 9
r9 = det[8]
det[8] = "9" + r9[1:]

# Defecto 5: NSS duplicado (registro 10 = copia del registro 2)
det.append(det[1])

# Defecto 6: trailer con número de registros alterado
trailer_malo = trailer[:12] + "00099" + trailer[17:]

contenido = [enc_malo] + det + [trailer_malo]
with open("ERRORES_202609.SUA", "w", newline="") as fh:
    fh.write("\r\n".join(contenido) + "\r\n")

print("ERRORES_202609.SUA generado con 6 defectos intencionales.")
