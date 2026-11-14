"""Lee archivos de pago .SUA y verifica el layout de Decompilacion_SUA.md seccion 14:
decodificacion base 62 del registro 03, sumas 03 contra 05, registro 06 y movimientos 04.
No imprime nombres, RFC, CURP ni NSS.
Uso: python -I verifica_sua.py [archivo.SUA ...]   (sin argumentos usa las muestras de SUA/Pagos)"""
import os, sys
from decimal import Decimal as D, ROUND_HALF_UP
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from _comun import archivos
files, _ = archivos(sys.argv[1:])
ALF = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz'
def b62(s):
    if s.strip() == '': return None
    n = 0
    for ch in s: n = n * 62 + ALF.index(ch)
    return n
def f(r, a, b): return r[a - 1:b]          # posiciones 1-based inclusivas
def n(r, a, b):
    s = f(r, a, b)
    return int(s) if s.strip() else 0
L03 = [('sdi',145,151),('tipo',152,152),('jor',153,153),('dm',154,155),('im',156,157),('am',158,159),
       ('cf',160,166),('exc',167,173),('pd',174,180),('gmp',181,187),('rt',188,194),('iv',195,201),('gua',202,208),
       ('arimss',209,215),('db',216,217),('ib',218,219),('ab',220,221),('ret',222,228),('arret',229,235),
       ('cvp',236,242),('cvo',243,249),('arcv',250,256),('f1',257,263),('f2',264,270),('f3',271,277)]
L05 = [('tact',39,48),('trec',49,58),('cf',59,67),('exc',68,76),('pd',77,85),('gmp',86,94),('rt',95,103),('iv',104,112),
       ('gua',113,121),('s4',122,132),('actimss',133,141),('recimss',142,150),('ret',151,159),('cv',160,168),('srcv',169,179),
       ('actrcv',180,188),('recrcv',189,197),('g1',198,206),('g2',207,215),('g3',216,224),('g4',225,233),('g5',234,242),
       ('g6',243,253),('g7',254,262),('g8',263,271),('g9',272,280)]
def rnd(x): return int((x).quantize(D('1'), rounding=ROUND_HALF_UP))
for fn in files:
    d = open(fn, 'rb').read().decode('cp1252')
    recs = [d[i:i + 295] for i in range(0, len(d), 295)]
    print('=====', os.path.basename(fn), 'bytes', len(d))
    r02 = recs[0]
    print('02 tail 244-295:', repr(f(r02, 244, 295)))
    tot = {}
    okc = bad = 0
    nss03 = []
    for r in recs:
        if r[:2] != '03': continue
        v = {k: n(r, a, b) for k, a, b in L03}
        nss03.append((f(r, 33, 43), n(r, 93, 94)))
        for k in v: tot[k] = tot.get(k, 0) + v[k]
        c = [b62(f(r, p, p + 2)) for p in (278, 281, 284, 287, 290)]
        basem = D(v['sdi']) * (v['dm'] - v['am'])   # centavos*dias
        exp = [None, rnd(basem * D('0.0025')), rnd(basem * D('0.00375')), rnd(basem * D('0.00625')), v['cvo']]
        chk = []
        for i in (1, 2, 3, 4):
            got = c[i] if c[i] is not None else 0
            chk.append(got == exp[i] or (i < 4 and v['pd'] == 0 and got == 0))
        if all(chk): okc += 1
        else:
            bad += 1
            print('  NO CUADRA cod:', c, 'esp:', exp, {k: v[k] for k in ('sdi','dm','im','am','db','pd','exc','cvo')})
        if c[0]: print('  exc total', v['exc'], 'exc obr cod', c[0], 'ratio', round(c[0] / v['exc'], 4))
        if f(r, 293, 295) != f(r, 3, 5): print('  cola distinta', f(r, 293, 295))
    print('03: n=%d codificados OK=%d mal=%d' % (okc + bad, okc, bad))
    print('03 sumas:', {k: tot[k] for k in tot if k not in ('sdi','tipo','jor')})
    r05 = [r for r in recs if r[:2] == '05'][0]
    v5 = {k: n(r05, a, b) for k, a, b in L05}
    print('05:', {k: v for k, v in v5.items() if v})
    print('05 cola 281-295:', repr(f(r05, 281, 295)))
    r06 = [r for r in recs if r[:2] == '06'][0]
    print('06:', f(r06,39,48), f(r06,49,58), f(r06,59,66), f(r06,67,68), f(r06,69,70), f(r06,71,72), f(r06,73,84), f(r06,85,96), f(r06,97,108), f(r06,109,120), f(r06,121,139), '| resto', repr(f(r06,140,295).replace('0'*20,'~')))
    # movimientos 04
    movs = []
    for r in recs:
        if r[:2] != '04': continue
        assert f(r, 280, 295).strip() == ''
        for g in range(7):
            s = r[13 + g * 38: 13 + (g + 1) * 38]
            if s.strip(): movs.append(s)
    cnt = {}
    for m in movs: cnt[m[:11]] = cnt.get(m[:11], 0) + 1
    print('04: movimientos=%d  coincide con contador 93-94 del 03: %s' % (len(movs), all(cnt.get(a, 0) == b for a, b in nss03)))
    tipos = {}
    for m in movs:
        k = m[11:13]; tipos.setdefault(k, []).append((m[13:21], m[21:29].strip(), m[29:31], m[31:38]))
    for k in sorted(tipos):
        ej = tipos[k]
        print('   mov', k, 'n=%d' % len(ej), 'fechas', sorted(set(e[0] for e in ej))[:6], 'folio', set(e[1] for e in ej), 'dias', set(e[2] for e in ej), 'sdi', sorted(set(e[3] for e in ej)))
    print('   ordenado por NSS:', [m[:11] for m in movs] == sorted(m[:11] for m in movs))
