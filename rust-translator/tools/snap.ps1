# Dev helper: launch the app and grab a PNG of one of its windows (no desktop capture).
param(
  [string]$Exe = "target\debug\floating-translator.exe",
  [string]$Class = "FloatingTranslator",
  [string]$Out = "shot.png",
  [int]$WaitMs = 2000,
  [string]$AppArgs = "",
  [string]$Click = "",
  [switch]$Right,
  [switch]$NoStart
)
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinApi {
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr p);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassNameW(IntPtr h, System.Text.StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
  public struct RECT { public int Left, Top, Right, Bottom; }
}
"@
function Find-Window([int]$processId, [string]$className) {
  $script:found = [IntPtr]::Zero
  $cb = [WinApi+EnumProc]{
    param($h, $p)
    $pid2 = 0
    [WinApi]::GetWindowThreadProcessId($h, [ref]$pid2) | Out-Null
    if ($pid2 -eq $processId) {
      $sb = New-Object System.Text.StringBuilder 256
      [WinApi]::GetClassNameW($h, $sb, 256) | Out-Null
      if ($sb.ToString() -eq $className) { $script:found = $h; return $false }
    }
    return $true
  }
  [WinApi]::EnumWindows($cb, [IntPtr]::Zero) | Out-Null
  return $script:found
}
$proc = $null
if (-not $NoStart) {
  $proc = if ($AppArgs) { Start-Process -FilePath $Exe -ArgumentList $AppArgs -PassThru } else { Start-Process -FilePath $Exe -PassThru }
  Start-Sleep -Milliseconds $WaitMs
}
$targetPid = if ($proc) { $proc.Id } else { 0 }
if ($Click) {
  $parts = $Click.Split(',')
  $lx = [int]$parts[0]; $ly = [int]$parts[1]
  $main = Find-Window $targetPid "FloatingTranslator"
  if ($main -eq [IntPtr]::Zero) { Write-Output "main window not found for click"; exit 1 }
  $lp = [IntPtr](($ly -shl 16) -bor ($lx -band 0xFFFF))
  $down = if ($Right) { 0x0204 } else { 0x0201 }
  $up = if ($Right) { 0x0205 } else { 0x0202 }
  [WinApi]::PostMessage($main, 0x0200, [IntPtr]::Zero, $lp) | Out-Null
  [WinApi]::PostMessage($main, $down, [IntPtr]1, $lp) | Out-Null
  [WinApi]::PostMessage($main, $up, [IntPtr]::Zero, $lp) | Out-Null
  Start-Sleep -Milliseconds 700
}
$found = Find-Window $targetPid $Class
if ($found -eq [IntPtr]::Zero) {
  Write-Output "window not found (class=$Class pid=$targetPid)"
  if ($proc) { Stop-Process -Id $proc.Id -Force }
  exit 1
}
$r = New-Object WinApi+RECT
[WinApi]::GetWindowRect($found, [ref]$r) | Out-Null
$w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top
$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
$ok = [WinApi]::PrintWindow($found, $hdc, 2)
$g.ReleaseHdc($hdc)
$g.Dispose()
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Output "saved $Out ($w x $h) ok=$ok pid=$targetPid"
if ($proc) { Stop-Process -Id $proc.Id -Force }

