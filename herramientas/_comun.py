"""Rutas por defecto de los .SUA reales. Se pueden pasar otras rutas como argumentos."""
import glob, os, sys
BASE = r'D:\otros\sua\SUA\Pagos'
POR_DEFECTO = [BASE + r'\W0802051.SUA', BASE + r'\SUA\LectSua.SUA', BASE + r'\SUA\Patron.Sua',
               BASE + r'\SUA\W0706051.SUA', BASE + r'\SUA\ADD\W0605051.SUA', BASE + r'\SUA\ADD\W0606051.SUA']
def archivos(args):
    rutas = [a for a in args if os.path.sep in a or a.upper().endswith('.SUA')]
    resto = [a for a in args if a not in rutas]
    out = []
    for r in rutas: out.extend(glob.glob(r) or [r])
    return (out or POR_DEFECTO), resto
