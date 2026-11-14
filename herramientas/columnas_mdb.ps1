# Lista columnas y anchos de tablas de SUA.MDB. Requiere PowerShell de 32 bits (Jet OLEDB 4.0):
#   C:\Windows\SysWOW64\WindowsPowerShell\v1.0\powershell.exe -NoProfile -ExecutionPolicy Bypass -File columnas_mdb.ps1 -Clave '<contraseña>'
# La contraseña de la base esta en FAQ_IMSS_SUA.md seccion 5; no se guarda aqui.
param(
  [Parameter(Mandatory=$true)][string]$Clave,
  [string]$Mdb = 'D:\otros\sua\SUA\SUA.MDB',
  [string[]]$Tablas = @('Registro_02','Registro_03','Registro_04','Registro_05','Pago1')
)
$ErrorActionPreference = 'Stop'
$c = New-Object System.Data.OleDb.OleDbConnection "Provider=Microsoft.Jet.OLEDB.4.0;Data Source=$Mdb;Mode=Read;User ID=Admin;Jet OLEDB:Database Password=$Clave"
$c.Open()
foreach ($t in $Tablas) {
  $s = $c.GetOleDbSchemaTable([System.Data.OleDb.OleDbSchemaGuid]::Columns, @($null,$null,$t,$null))
  $rows = $s.Select('', 'ORDINAL_POSITION')
  "== $t ($($rows.Count) columnas)"
  foreach ($r in $rows) { "{0,3} {1,-32} tipo={2,-4} len={3}" -f $r['ORDINAL_POSITION'], $r['COLUMN_NAME'], $r['DATA_TYPE'], $r['CHARACTER_MAXIMUM_LENGTH'] }
}
$c.Close()
