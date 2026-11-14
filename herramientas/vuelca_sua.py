"""Vuelca registros de un .SUA con regla de posiciones y datos de identidad enmascarados
(letras -> X; NSS -> N; RFC y CURP -> X/9). Los importes y fechas se dejan en claro.
Uso: python -I vuelca_sua.py [tipos...] [all] [archivo.SUA ...]
     tipos = 02 03 04 05 06 (por defecto 02 05 06); 'all' muestra todos los 03 (si no, 4 por archivo)."""
import os, re, sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from _comun import archivos
files, which = archivos(sys.argv[1:])
which = which or ['02', '05', '06']
def xs(s): return re.sub(r'[A-Za-z]', 'X', s)
def x9(s): return re.sub(r'[A-Za-z0-9]', lambda m: 'X' if m.group().isalpha() else '9', s)
ruler1 = ''.join(str((i // 10) % 10) if i % 10 == 0 else ' ' for i in range(1, 296))
ruler2 = ''.join(str(i % 10) for i in range(1, 296))
for f in files:
    d = open(f, 'rb').read().decode('cp1252')
    recs = [d[i:i + 295] for i in range(0, len(d), 295)]
    print('==', os.path.basename(f), len(d), 'bytes')
    print(ruler1); print(ruler2)
    n03 = 0
    for r in recs:
        t = r[:2]
        if t not in which: continue
        if t == '04':
            r = r[:13] + ''.join(('N' * 11 + r[13 + g * 38 + 11:13 + (g + 1) * 38]) if r[13 + g * 38:13 + (g + 1) * 38].strip() else ' ' * 38 for g in range(7)) + r[279:]
        else:
            r = r[:13] + x9(r[13:26]) + r[26:]          # RFC del patron
        if t == '03':
            n03 += 1
            if n03 > 4 and 'all' not in which: continue
            r = r[:32] + 'N' * 11 + x9(r[43:74]) + r[74:94] + xs(r[94:144]) + r[144:]
        elif t == '02':
            r = r[:38] + xs(r[38:168]) + r[168:175] + x9(r[175:190]) + r[190:203] + xs(r[203:243]) + r[243:]
        print(r)
