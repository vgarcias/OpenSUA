# Compila y arma el paquete portable: carpeta OpenSUA-Portable, .zip y su SHA-256.
# Uso (desde PowerShell): .\herramientas\empaquetar.ps1
#
# Compila en target\paquete con las rutas del equipo de desarrollo sustituidas
# (--remap-path-prefix), para que el .exe no lleve el nombre del usuario ni la
# carpeta del proyecto. Despues revisa el resultado con revisar_exe.py: si el
# .exe importa una DLL que no es del sistema o conserva una ruta absoluta, falla.
# (Sin acentos a proposito: Windows PowerShell 5.1 lee este archivo como ANSI.)
$ErrorActionPreference = 'Stop'
$app = (Resolve-Path (Join-Path $PSScriptRoot '..\app')).Path
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
$rustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $env:USERPROFILE '.rustup' }
$env:PATH = (Join-Path $cargoHome 'bin') + ';' + $env:PATH
# El cifrado (SQLCipher) compila OpenSSL desde su codigo: hace falta Strawberry Perl
# (el perl de Git Bash no sirve) y, si esta instalado, se usa NASM. Van delante en el PATH.
foreach ($d in 'C:\Program Files\NASM', 'C:\Strawberry\perl\bin') { if (Test-Path $d) { $env:PATH = $d + ';' + $env:PATH } }
if (-not ((Get-Command perl -ErrorAction SilentlyContinue).Source -like '*Strawberry*')) {
  throw 'Hace falta Strawberry Perl para compilar OpenSSL (winget install StrawberryPerl.StrawberryPerl).'
}

$version = (Select-String -Path (Join-Path $app 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value

# CARGO_ENCODED_RUSTFLAGS separa con 0x1F, asi una ruta con espacios no se parte.
# Sustituye a los rustflags de .cargo\config.toml, por eso se repite crt-static.
$banderas = @(
  '-C', 'target-feature=+crt-static',
  "--remap-path-prefix=$cargoHome=cargo",
  "--remap-path-prefix=$rustupHome=rustup",
  "--remap-path-prefix=$app=sua"
)
$env:CARGO_ENCODED_RUSTFLAGS = $banderas -join [char]0x1F

Push-Location $app
try {
  cargo build --release --target-dir target\paquete
  if ($LASTEXITCODE -ne 0) { throw 'La compilacion fallo.' }
} finally {
  Pop-Location
  Remove-Item Env:CARGO_ENCODED_RUSTFLAGS
}

$salida = Join-Path $app 'target\paquete\release'
python -I (Join-Path $PSScriptRoot 'revisar_exe.py') (Join-Path $salida 'sua.exe') (Join-Path $salida 'sua-cli.exe')
if ($LASTEXITCODE -ne 0) { throw 'La revision de los ejecutables encontro DLL ajenas o rutas incrustadas.' }

# Firma opcional (Authenticode). Solo si SUA_FIRMA_HUELLA trae la huella de un
# certificado de firma de codigo instalado en el almacen del usuario y hay signtool.
function Firmar([string[]]$archivos) {
  if (-not $env:SUA_FIRMA_HUELLA) { return $false }
  $signtool = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin' -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\x64\\' } | Sort-Object FullName | Select-Object -Last 1
  if (-not $signtool) { throw 'SUA_FIRMA_HUELLA esta definida pero no se encontro signtool.exe (Windows SDK).' }
  & $signtool.FullName sign /sha1 $env:SUA_FIRMA_HUELLA /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 $archivos
  if ($LASTEXITCODE -ne 0) { throw 'La firma fallo.' }
  return $true
}

# El paquete lleva el marcador de modo portable y ningun dato.
$dist = Join-Path $app 'dist\paquete'
$carpeta = Join-Path $dist 'OpenSUA-Portable'
$fuentes = Join-Path $app 'paquete'
if (Test-Path $dist) { Remove-Item -Recurse -Force $dist }
New-Item -ItemType Directory -Force (Join-Path $carpeta 'config') | Out-Null
Copy-Item (Join-Path $salida 'sua.exe'), (Join-Path $salida 'sua-cli.exe') $carpeta
Copy-Item (Join-Path $fuentes 'LEEME.txt') $carpeta
[IO.File]::WriteAllText((Join-Path $carpeta 'config\settings.toml'), "portable = true`r`nrespaldos_a_conservar = 10`r`n")
$firmado = Firmar @((Join-Path $carpeta 'sua.exe'), (Join-Path $carpeta 'sua-cli.exe'))

# Licencias de terceros y manual (HTML en un solo archivo, con las capturas dentro).
python -I (Join-Path $PSScriptRoot 'licencias.py') $app (Join-Path $carpeta 'LICENCIAS-DE-TERCEROS.txt')
if ($LASTEXITCODE -ne 0) { throw 'No se pudo generar la lista de licencias.' }
# Desde la carpeta de fuentes y con rutas relativas: pandoc toma una ruta "D:\..." por una direccion web.
Push-Location $fuentes
try {
  pandoc Manual.md -o (Join-Path $carpeta 'Manual.html') --standalone --embed-resources --toc --toc-depth=2 `
    --css=manual.css --metadata ("date=Versi" + [char]0xF3 + "n $version")
  if ($LASTEXITCODE -ne 0) { throw 'No se pudo generar el manual (hace falta pandoc).' }
} finally { Pop-Location }

$zip = Join-Path $dist "OpenSUA-Portable-$version.zip"
Compress-Archive -Path $carpeta -DestinationPath $zip
$entregables = @($zip)

# Instalador .msi opcional, por usuario. Necesita WiX 5 en target\herramientas\wix:
#   dotnet tool install wix --version 5.0.2 --tool-path app\target\herramientas\wix
$wix = Join-Path $app 'target\herramientas\wix\wix.exe'
if (Test-Path $wix) {
  $msi = Join-Path $dist "OpenSUA-$version.msi"
  & $wix build (Join-Path $fuentes 'instalador.wxs') -arch x64 -b $carpeta -d "Version=$version" `
    -d ("Icono=" + (Join-Path $app 'crates\ui\recursos\app.ico')) -o $msi
  if ($LASTEXITCODE -ne 0) { throw 'No se pudo armar el instalador.' }
  Remove-Item (Join-Path $dist "OpenSUA-$version.wixpdb") -ErrorAction SilentlyContinue
  if ($firmado) { [void](Firmar @($msi)) }
  $entregables += $msi
} else {
  'Sin instalador .msi: no esta WiX en target\herramientas\wix.'
}

$sumas = foreach ($e in $entregables) { '{0}  {1}' -f (Get-FileHash $e -Algorithm SHA256).Hash.ToLower(), (Split-Path $e -Leaf) }
[IO.File]::WriteAllLines((Join-Path $dist 'SHA256SUMS.txt'), $sumas)

''
'Entregables en ' + $dist
foreach ($e in $entregables) { '  {0,-28} {1,5:N1} MB' -f (Split-Path $e -Leaf), ((Get-Item $e).Length / 1MB) }
$sumas
if (-not $firmado) { 'Ejecutables SIN FIRMAR (defina SUA_FIRMA_HUELLA para firmarlos).' }
