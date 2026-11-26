# Dev helper: run the app with args, wait with a timeout, dump exit code + translate.log.
param([string]$Exe = "target\debug\floating-translator.exe", [string]$AppArgs = "--selftest", [int]$TimeoutMs = 60000)
$ErrorActionPreference = "Continue"
$p = Start-Process -FilePath $Exe -ArgumentList $AppArgs -PassThru
"started pid=$($p.Id)"
$exited = $p.WaitForExit($TimeoutMs)
"exited=$exited"
if ($exited) { "exitcode=$($p.ExitCode)" } else { "killing"; $p.Kill() }
$dir = Split-Path -Parent (Resolve-Path $Exe)
foreach ($f in @("translate.log", "selftest.log")) {
  $path = Join-Path $dir $f
  if (Test-Path $path) { "--- $f ---"; Get-Content $path }
}