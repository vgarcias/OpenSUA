# Lanza un ejecutable con interfaz, espera, mide memoria y lo cierra.
# Uso: powershell -File medir_ui.ps1 -Exe ruta.exe [-Entorno @{VAR='valor'}] [-Etiqueta texto] [-Captura ruta.png] [-Repeticiones 3]
param(
  [Parameter(Mandatory=$true)][string]$Exe,
  [hashtable]$Entorno = @{},
  [string]$Etiqueta = '',
  [string]$Captura = '',
  [int]$Repeticiones = 3,
  [int]$EsperaSegundos = 6
)
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System; using System.Runtime.InteropServices;
public static class Win {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
}
'@
# Sin esto, en pantallas con escala (125 %, 150 %) el tamaño de la ventana llega reducido y la captura sale recortada.
[void][Win]::SetProcessDPIAware()
foreach ($k in $Entorno.Keys) { Set-Item -Path "Env:$k" -Value $Entorno[$k] }
$res = @()
for ($i = 1; $i -le $Repeticiones; $i++) {
  $p = Start-Process -FilePath $Exe -WorkingDirectory (Split-Path $Exe) -PassThru
  Start-Sleep -Seconds $EsperaSegundos
  $p.Refresh()
  if ($p.HasExited) { "[$Etiqueta] el proceso terminó solo (código $($p.ExitCode))"; continue }
  # Si la aplicación se relanzó con otro motor, el proceso vivo es otro con el mismo nombre.
  $vivo = Get-Process -Name $p.ProcessName -ErrorAction SilentlyContinue | Sort-Object StartTime | Select-Object -Last 1
  $res += [pscustomobject]@{ WS = $vivo.WorkingSet64; Priv = $vivo.PrivateMemorySize64; Hilos = $vivo.Threads.Count; Ventana = ($vivo.MainWindowHandle -ne 0) }
  if ($Captura -and $i -eq 1 -and $vivo.MainWindowHandle -ne 0) {
    # PrintWindow pide a la propia ventana que se dibuje (solo el área cliente): no captura lo que haya encima en la pantalla.
    $r = New-Object Win+RECT; [void][Win]::GetClientRect($vivo.MainWindowHandle, [ref]$r)
    $w = $r.R - $r.L; $h = $r.B - $r.T
    if ($w -gt 0 -and $h -gt 0) {
      $bmp = New-Object System.Drawing.Bitmap $w, $h
      $g = [System.Drawing.Graphics]::FromImage($bmp); $hdc = $g.GetHdc()
      $ok = [Win]::PrintWindow($vivo.MainWindowHandle, $hdc, 3)
      $g.ReleaseHdc($hdc); $g.Dispose()
      if ($ok) { $bmp.Save($Captura, [System.Drawing.Imaging.ImageFormat]::Png) }
      $bmp.Dispose()
    }
  }
  Get-Process -Name $p.ProcessName -ErrorAction SilentlyContinue | Stop-Process -Force
  Start-Sleep -Milliseconds 700
}
if ($res.Count) {
  $ws = ($res | Measure-Object WS -Average -Minimum -Maximum); $pv = ($res | Measure-Object Priv -Average)
  "[{0}] memoria de trabajo {1:N0} MB (min {2:N0}, max {3:N0}) | privada {4:N0} MB | hilos {5} | ventana {6} | n={7}" -f $Etiqueta, ($ws.Average/1MB), ($ws.Minimum/1MB), ($ws.Maximum/1MB), ($pv.Average/1MB), $res[0].Hilos, $res[0].Ventana, $res.Count
}
