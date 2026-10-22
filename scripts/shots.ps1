# Снимки для docs/img своим окном в демо-режиме, на выдуманных настройках (своя папка APPDATA):
# powershell -File scripts\shots.ps1 [-Exe antresol.exe] [-Out папка] [-Only сцена]; потом squeeze-png.ps1.
param([string]$Exe = "E:\dev_personal\antresol-scratch\target-shot\debug\antresol.exe",
      [string]$Out = "E:\dev_personal\antresol-scratch\shots", [string]$Only = "")
$exe = $Exe
$app = "E:\dev_personal\antresol-scratch\appdata-shot"
New-Item -ItemType Directory -Force "$app\Antresol" | Out-Null
New-Item -ItemType Directory -Force $Out | Out-Null
$profiles = @(
  'profile=Работа | Ctrl+Alt+1 | \\.\DISPLAY1 2560x1440@144 125%',
  'profile=Игры | Ctrl+Alt+2 | \\.\DISPLAY1 1920x1080@240 100%',
  'profile=Кино |  | \\.\DISPLAY1 3840x2160@60 150%'
)
$scenes = @(
  @{ n = "panel-dark-ru";    lang = "ru"; theme = "dark";  view = "" },
  @{ n = "panel-light-en";   lang = "en"; theme = "light"; view = "" },
  @{ n = "settings-dark-ru"; lang = "ru"; theme = "dark";  view = "settings" },
  @{ n = "settings-light-en"; lang = "en"; theme = "light"; view = "settings" },
  @{ n = "color-dark-ru";    lang = "ru"; theme = "dark";  view = "color" },
  @{ n = "color-light-en";   lang = "en"; theme = "light"; view = "color" },
  @{ n = "strip-media-dark"; lang = "ru"; theme = "dark";  view = "strip-media" },
  @{ n = "strip-banner-light"; lang = "en"; theme = "light"; view = "strip-banner" },
  @{ n = "cards2-dark-ru"; lang = "ru"; theme = "dark"; view = ""; extra = @("modules=quick,sound,snippets,tools,display") },
  @{ n = "cards2-light-en"; lang = "en"; theme = "light"; view = ""; extra = @("modules=quick,sound,snippets,tools,display") }
)
foreach ($s in $scenes) {
  if ($Only -and $s.n -ne $Only) { continue }
  $cfg = @("lang=$($s.lang)", "theme=$($s.theme)", "width=1300", "height=340", "monitor=\\.\DISPLAY1",
    "favorite=\\.\DISPLAY1 2560x1440", "favorite=\\.\DISPLAY1 1920x1080", "favorite=\\.\DISPLAY1 3840x2160",
    "rate=\\.\DISPLAY1 1920x1080 240", "rate=\\.\DISPLAY1 2560x1440 144") + $profiles
  if ($s.extra) { $cfg += $s.extra }
  [IO.File]::WriteAllLines("$app\Antresol\config.txt", [string[]]$cfg, (New-Object Text.UTF8Encoding($false)))
  $env:APPDATA = $app
  $env:ANTRESOL_DEMO = "1"
  $env:ANTRESOL_SHOT = "$Out\$($s.n).png"
  $env:ANTRESOL_VIEW = $s.view
  $p = Start-Process -FilePath $exe -PassThru -WindowStyle Normal -RedirectStandardError "$Out\$($s.n).log"
  if (-not $p.WaitForExit(30000)) { $p.Kill(); "timeout $($s.n)" } else { "done $($s.n)" }
}
