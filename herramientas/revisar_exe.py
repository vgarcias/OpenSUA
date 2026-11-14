# Revisa un ejecutable de Windows para la fase de portabilidad:
#   1. DLL que importa (tabla de importaciones y de carga diferida).
#   2. Rutas absolutas incrustadas (del equipo donde se compilo).
# Uso: python -I revisar_exe.py <archivo.exe> [...]
# Codigo de salida: 0 sin hallazgos, 1 con alguna DLL que no es del sistema o alguna ruta incrustada.
import os
import re
import struct
import sys

# DLL que trae cualquier Windows 10 u 11 de 64 bits.
DEL_SISTEMA = {
    "kernel32.dll", "user32.dll", "gdi32.dll", "advapi32.dll", "shell32.dll", "ole32.dll",
    "oleaut32.dll", "comctl32.dll", "comdlg32.dll", "shlwapi.dll", "imm32.dll", "dwmapi.dll",
    "uxtheme.dll", "ntdll.dll", "bcrypt.dll", "bcryptprimitives.dll", "ws2_32.dll", "userenv.dll",
    "version.dll", "winmm.dll", "opengl32.dll", "dwrite.dll", "d2d1.dll", "d3d11.dll", "dxgi.dll",
    "propsys.dll", "crypt32.dll", "secur32.dll", "setupapi.dll", "cfgmgr32.dll", "powrprof.dll",
    "msimg32.dll", "usp10.dll", "rpcrt4.dll", "combase.dll", "shcore.dll", "psapi.dll",
    "iphlpapi.dll", "netapi32.dll", "wtsapi32.dll", "winspool.drv", "hid.dll", "synchronization.dll",
}


def leer_pe(datos):
    pe = struct.unpack_from("<I", datos, 0x3C)[0]
    assert datos[pe:pe + 4] == b"PE\0\0", "no es un ejecutable PE"
    n_secciones = struct.unpack_from("<H", datos, pe + 6)[0]
    tam_opcional = struct.unpack_from("<H", datos, pe + 20)[0]
    opcional = pe + 24
    magia = struct.unpack_from("<H", datos, opcional)[0]
    assert magia == 0x20B, "se esperaba un ejecutable de 64 bits"
    directorios = opcional + 112
    secciones = []
    base = opcional + tam_opcional
    for i in range(n_secciones):
        s = base + 40 * i
        tam_virtual, rva, tam_crudo, crudo = struct.unpack_from("<IIII", datos, s + 8)
        secciones.append((rva, max(tam_virtual, tam_crudo), crudo))
    return directorios, secciones


def a_archivo(rva, secciones):
    for inicio, largo, crudo in secciones:
        if inicio <= rva < inicio + largo:
            return rva - inicio + crudo
    return None


def cadena(datos, pos):
    fin = datos.index(b"\0", pos)
    return datos[pos:fin].decode("ascii", "replace")


def importaciones(datos):
    directorios, secciones = leer_pe(datos)
    nombres = []
    # Directorio 1: importaciones (entradas de 20 bytes, nombre en +12).
    # Directorio 13: carga diferida (entradas de 32 bytes, nombre en +4).
    for indice, tam, campo in ((1, 20, 12), (13, 32, 4)):
        rva = struct.unpack_from("<I", datos, directorios + 8 * indice)[0]
        if not rva:
            continue
        pos = a_archivo(rva, secciones)
        while pos is not None:
            entrada = datos[pos:pos + tam]
            if not any(entrada):
                break
            rva_nombre = struct.unpack_from("<I", entrada, campo)[0]
            p = a_archivo(rva_nombre, secciones)
            if p is not None:
                nombres.append(cadena(datos, p))
            pos += tam
    return sorted(set(n.lower() for n in nombres))


def es_del_sistema(nombre):
    return nombre in DEL_SISTEMA or nombre.startswith("api-ms-win-") or nombre.startswith("ext-ms-")


RUTA = re.compile(rb"(?<![A-Za-z0-9])[A-Za-z]:[\\/][\x20-\x7e]{3,200}")
RUTA_ANCHA = re.compile(rb"(?<![A-Za-z0-9]\x00)(?:[A-Za-z]\x00):\x00[\\/]\x00(?:[\x20-\x7e]\x00){3,200}")
# Rutas que el programa nombra a proposito, no restos de la compilacion.
PERMITIDAS = ("c:\\windows", "c:/windows")


USUARIO = os.environ.get("USERNAME", "").lower()


def rutas_incrustadas(datos):
    halladas = set()
    for m in RUTA.finditer(datos):
        halladas.add(m.group().decode("ascii"))
    for m in RUTA_ANCHA.finditer(datos):
        halladas.add(m.group().decode("utf-16-le"))
    # Solo interesan las que parecen del equipo de desarrollo: una letra de unidad
    # seguida de una carpeta con nombre (no secuencias binarias casuales).
    limpias = set()
    for r in halladas:
        if not re.match(r"^[A-Za-z]:[\\/][A-Za-z0-9_.\-]{2,}[\\/]", r):
            continue
        if r.lower().startswith(PERMITIDAS):
            continue
        limpias.add(r)
    return sorted(limpias)


def raiz(ruta):
    partes = re.split(r"[\\/]", ruta)
    return "\\".join(partes[:4])


def main():
    hallazgos = 0
    for archivo in sys.argv[1:]:
        with open(archivo, "rb") as f:
            datos = f.read()
        print(f"== {archivo} ({len(datos) / 1048576:.1f} MB)")
        dll = importaciones(datos)
        ajenas = [d for d in dll if not es_del_sistema(d)]
        print(f"   DLL importadas: {len(dll)}; que no son del sistema: {len(ajenas)}")
        print("   " + ", ".join(dll))
        for d in ajenas:
            print(f"   NO ES DEL SISTEMA: {d}")
        rutas = rutas_incrustadas(datos)
        # OpenSSL (el cifrado) deja siempre las rutas de su propia compilacion: carpetas de
        # configuracion que el programa no lee (ver cifrado::aislar_openssl) y nombres de
        # archivos fuente. Se toleran mientras no revelen la carpeta de un usuario.
        de_openssl = [r for r in rutas if "openssl-build" in r.lower()]
        delatoras = [r for r in de_openssl if "\\users\\" in r.lower().replace("/", "\\") or (USUARIO and USUARIO in r.lower())]
        if de_openssl and not delatoras:
            rutas = [r for r in rutas if r not in de_openssl]
            print(f"   Rutas de la compilacion de OpenSSL, toleradas (sin carpeta de usuario): {len(de_openssl)}")
        print(f"   Rutas absolutas incrustadas: {len(rutas)}")
        grupos = {}
        for r in rutas:
            grupos[raiz(r)] = grupos.get(raiz(r), 0) + 1
        for g, n in sorted(grupos.items(), key=lambda x: -x[1])[:15]:
            print(f"   {n:5d}  {g}...")
        hallazgos += len(ajenas) + len(rutas)
    return 1 if hallazgos else 0


if __name__ == "__main__":
    sys.exit(main())
