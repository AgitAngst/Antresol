# Снимки для docs/img своим окном в демо-режиме, на выдуманных настройках (своя папка APPDATA):
# powershell -File scripts\shots.ps1 [-Exe antresol.exe] [-Out папка] [-Only сцена]; потом squeeze-png.ps1.
# Правило демо: клавиш профилей нет, монитор несуществующий (\\.\DISPLAY9 — выдуманный экран демо-режима), в Windows
# ничего не пишется. Профили, избранное и кнопки — выдуманные, по языку сцены.
# Сцены: полки (`shelf` — номер полки, `ANTRESOL_SHELF`), правка полок (`view = "edit"`), настройки, «Свой цвет…»,
# полоска у края. Тёмная RU и светлая EN.
param([string]$Exe = "E:\dev_personal\antresol-scratch\target-shot\debug\antresol.exe",
      [string]$Out = "E:\dev_personal\antresol-scratch\shots", [string]$Only = "")
$exe = $Exe
$app = "E:\dev_personal\antresol-scratch\appdata-shot"
New-Item -ItemType Directory -Force "$app\Antresol" | Out-Null
New-Item -ItemType Directory -Force $Out | Out-Null
$dev = '\\.\DISPLAY9'
# Профили — без клавиш и на выдуманном мониторе; первый совпадает с режимом выдуманного экрана (его чип выделен).
function Get-Profiles($lang) {
  if ($lang -eq "en") { $n = @("Work", "Games", "Movies") } else { $n = @("Работа", "Игры", "Кино") }
  return @("profile=$($n[0]) |  | $dev 3840x2160@120 150%",
          "profile=$($n[1]) |  | $dev 2560x1440@144 100%",
          "profile=$($n[2]) |  | $dev 3840x2160@60 175%")
}
# Свои кнопки: образец по языку сцены (демо файл не заводит сам).
function Get-Buttons($lang) {
  if ($lang -eq "en") {
    return @("Downloads | %USERPROFILE%\Downloads", "Notepad | notepad.exe", "Terminal | powershell.exe | -NoExit | terminal")
  }
  return @("Загрузки | %USERPROFILE%\Downloads", "Блокнот | notepad.exe", "Терминал | powershell.exe | -NoExit | terminal")
}
$scenes = @(
  @{ n = "panel-dark-ru";    lang = "ru"; theme = "dark";  view = "" },
  @{ n = "panel-light-en";   lang = "en"; theme = "light"; view = "" },
  @{ n = "shelf-system-dark-ru";  lang = "ru"; theme = "dark";  view = ""; shelf = "2" },
  @{ n = "shelf-system-light-en"; lang = "en"; theme = "light"; view = ""; shelf = "2" },
  @{ n = "shelf-hand-dark-ru";    lang = "ru"; theme = "dark";  view = ""; shelf = "3" },
  @{ n = "shelf-hand-light-en";   lang = "en"; theme = "light"; view = ""; shelf = "3" },
  @{ n = "edit-dark-ru";     lang = "ru"; theme = "dark";  view = "edit"; extra = @("hidden=media") },
  @{ n = "edit-light-en";    lang = "en"; theme = "light"; view = "edit"; extra = @("hidden=media") },
  @{ n = "settings-dark-ru"; lang = "ru"; theme = "dark";  view = "settings" },
  @{ n = "settings-light-en"; lang = "en"; theme = "light"; view = "settings" },
  @{ n = "color-dark-ru";    lang = "ru"; theme = "dark";  view = "color" },
  @{ n = "color-light-en";   lang = "en"; theme = "light"; view = "color" },
  @{ n = "strip-media-dark"; lang = "ru"; theme = "dark";  view = "strip-media" },
  @{ n = "strip-banner-light"; lang = "en"; theme = "light"; view = "strip-banner" }
)
foreach ($s in $scenes) {
  if ($Only -and $s.n -ne $Only) { continue }
  $cfg = @("lang=$($s.lang)", "theme=$($s.theme)", "width=1300", "height=340", "monitor=\\.\DISPLAY1",
    "favorite=$dev 3840x2160", "favorite=$dev 2560x1440", "favorite=$dev 1920x1080",
    "rate=$dev 2560x1440 144", "rate=$dev 1920x1080 144") + (Get-Profiles $s.lang)
  if ($s.extra) { $cfg += $s.extra }
  $utf8 = New-Object Text.UTF8Encoding($false)
  [IO.File]::WriteAllLines("$app\Antresol\config.txt", [string[]]$cfg, $utf8)
  [IO.File]::WriteAllLines("$app\Antresol\buttons.txt", [string[]](Get-Buttons $s.lang), $utf8)
  $env:APPDATA = $app
  $env:ANTRESOL_DEMO = "1"
  $env:ANTRESOL_SHOT = "$Out\$($s.n).png"
  $env:ANTRESOL_VIEW = $s.view
  $env:ANTRESOL_SHELF = $s.shelf
  $p = Start-Process -FilePath $exe -PassThru -WindowStyle Normal -RedirectStandardError "$Out\$($s.n).log"
  if (-not $p.WaitForExit(30000)) { $p.Kill(); "timeout $($s.n)" } else { "done $($s.n)" }
}
