# Genera LICENCIAS-DE-TERCEROS.txt para el paquete: los componentes que entran
# en la compilación de Windows, con su versión y su licencia declarada.
# Uso: python -I licencias.py <carpeta app> <archivo de salida>
#
# La lista sale de `cargo metadata` filtrado a Windows. Incluye también las
# bibliotecas que solo se usan al compilar (no viajan en el .exe): sobra, no falta.
import json
import os
import subprocess
import sys

app, salida = sys.argv[1], sys.argv[2]
# Tercer argumento opcional: la plataforma de destino (por omisión, Windows).
plataforma = sys.argv[3] if len(sys.argv) > 3 else "x86_64-pc-windows-msvc"
cargo = os.path.join(os.environ.get("CARGO_HOME", os.path.join(os.path.expanduser("~"), ".cargo")), "bin", "cargo")
meta = json.loads(subprocess.run(
    [cargo, "metadata", "--format-version", "1", "--filter-platform", plataforma, "--locked"],
    cwd=app, capture_output=True, text=True, encoding="utf-8", check=True).stdout)

# Solo lo que queda en el grafo resuelto para Windows.
usados = {n["id"] for n in meta["resolve"]["nodes"]}
propios = set(meta["workspace_members"])
paquetes = sorted((p for p in meta["packages"] if p["id"] in usados and p["id"] not in propios), key=lambda p: (p["name"].lower(), p["version"]))

por_licencia = {}
for p in paquetes:
    por_licencia.setdefault(p.get("license") or "(no declarada: ver su repositorio)", []).append(p)

with open(os.path.join(app, "crates", "ui", "recursos", "iconos", "LICENSE"), encoding="utf-8") as f:
    lucide = f.read().strip()

# La licencia de SQLCipher pide reproducir su texto en lo que acompaña al programa.
fuente_sqlcipher = next(p for p in paquetes if p["name"] == "sqlcipher-src")
with open(os.path.join(os.path.dirname(fuente_sqlcipher["manifest_path"]), "sqlcipher", "LICENSE-sqlcipher"), encoding="utf-8") as f:
    sqlcipher = f.read().strip()

L = []
L.append("OpenSUA: licencias de componentes de terceros")
L.append("=" * 50)
L.append("")
L.append("Este programa incluye o se construye con los componentes siguientes. Cada uno")
L.append("conserva su propia licencia. El texto completo de cada licencia se encuentra en")
L.append("el repositorio de cada componente (https://crates.io/crates/<nombre>).")
L.append("")
L.append("1. Interfaz: Slint")
L.append("-" * 50)
L.append("La interfaz gráfica usa Slint (https://slint.dev), bajo la licencia gratuita")
L.append("«Slint Royalty-free Desktop, Mobile, and Web Applications License 2.0», que")
L.append("pide mostrar la atribución a Slint: está en la pantalla Ayuda, sección")
L.append("«Acerca de». Texto de la licencia: https://slint.dev/terms-and-conditions")
L.append("")
L.append("2. Iconos: Lucide")
L.append("-" * 50)
L.append(lucide)
L.append("")
L.append("3. Base de datos y cifrado: SQLite, SQLCipher y OpenSSL")
L.append("-" * 50)
L.append("SQLite es de dominio público (https://www.sqlite.org/copyright.html).")
L.append("")
L.append("El cifrado opcional de la base usa SQLCipher (https://www.zetetic.net/sqlcipher/),")
L.append("con esta licencia:")
L.append("")
L.append(sqlcipher)
L.append("")
L.append("SQLCipher cifra con OpenSSL (https://www.openssl.org), que se distribuye bajo la")
L.append("licencia Apache 2.0 (https://www.apache.org/licenses/LICENSE-2.0).")
L.append("Copyright 1995-2026 The OpenSSL Project Authors. All Rights Reserved.")
L.append("")
L.append(f"4. Bibliotecas de Rust ({len(paquetes)})")
L.append("-" * 50)
for licencia in sorted(por_licencia, key=lambda k: (-len(por_licencia[k]), k)):
    L.append("")
    L.append(f"Licencia: {licencia}")
    for p in por_licencia[licencia]:
        L.append(f"  {p['name']} {p['version']}")
L.append("")

with open(salida, "w", encoding="utf-8", newline="\r\n") as f:
    f.write("\n".join(L))
print(f"{len(paquetes)} componentes, {len(por_licencia)} licencias distintas -> {salida}")
