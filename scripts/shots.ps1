# Снимки для docs/img своим окном в демо-режиме, на выдуманных настройках (своя папка APPDATA):
# powershell -File scripts\shots.ps1 [-Exe antresol.exe] [-Out папка] [-Only кадр] [-App папка настроек]; потом squeeze-png.ps1.
# Правило демо: клавиш сцен нет, монитор несуществующий (\\.\DISPLAY9 — выдуманный экран демо-режима), в Windows
# ничего не пишется. Сцены, избранное и кнопки — выдуманные, по языку кадра.
# Кадры: полки (`shelf` — номер полки, `ANTRESOL_SHELF`), правка полок (`view = "edit"`), правка сцены (`view = "scene"`,
# `scene` — её номер, `ANTRESOL_SCENE`; `scene-ask` — с вопросом «Удалить сцену?», `scene-name` — с негодным именем),
# поиск действий (`view = "search"`, `query` — запрос, `ANTRESOL_QUERY`), настройки (`settings-ask` — с тем же вопросом у
# сцены), «Свой цвет…», полоска у края, живые плашки (`view = "strip-plaque-<вид>"`: bluetooth, battery, usb, ejecting, eject, busy, gone,
# github, downloads, downloads-many, downloads-danger, timer, meeting — выдуманные данные на языке кадра, панель спрятана, снимается только окно плашки). Тёмная RU и светлая EN.
# Карточка «Bluetooth» на полке «Система» (`shelf = "2"`): обычная (`shelf-system-*`: наушники и мышь подключены, колонка с «Подключить», клавиатура без
# кнопки), «Подключение…» (`view = "bluetooth-connecting"`: колонка в работе, поток не заводится) и «Bluetooth выключен» (`view = "bluetooth-off"`: «Подключить» серая).
# Полка «День» (`shelf = "3"`): с календарём (`calendar=` — выдуманная ссылка; встречи — выдуманные, из самой программы), список календарей
# (`view = "meetings-manage"`, два календаря) и без календаря.
# У кадра может быть своя высота панели (`height`, по умолчанию 340): «Под рукой» снимается на 400 — буферу с чипами нужны
# строки, настройки — на 480: «Живые плашки» в карточке «Панель» иначе ниже края.
param([string]$Exe = "E:\dev_personal\antresol-scratch\target-shot\debug\antresol.exe",
      [string]$Out = "E:\dev_personal\antresol-scratch\shots", [string]$Only = "",
      [string]$App = "E:\dev_personal\antresol-scratch\appdata-shot")
$exe = $Exe
$app = $App
New-Item -ItemType Directory -Force "$app\Antresol" | Out-Null
New-Item -ItemType Directory -Force $Out | Out-Null
$dev = '\\.\DISPLAY9'
# Сцены — без клавиш и на выдуманном мониторе; первая совпадает с тем, что стоит в выдуманном мире (тёмная тема, 70 %,
# 4K 120 Гц 150 %) — её чип выделен. «Кино» — с HDR (`hdr` после масштаба) и «Тишиной», «Не спать», яркостью, ночным светом.
function Get-Profiles($lang) {
  if ($lang -eq "en") { $n = @("Work", "Game", "Movie", "Slides") } else { $n = @("Работа", "Игра", "Кино", "Слайды") }
  return @("profile=$($n[0]) |  | $dev 3840x2160@120 150% | dark=1; bright=70",
          "profile=$($n[1]) |  | $dev 2560x1440@144 100% | quiet=1; awake=1; bright=100",
          "profile=$($n[2]) |  | $dev 3840x2160@60 150% hdr | night=0; quiet=1; awake=1; bright=80",
          "profile=$($n[3]) |  | $dev 1920x1080@60 100% | dark=0; quiet=1")
}
# Свои кнопки: образец по языку сцены (демо файл не заводит сам).
function Get-Buttons($lang) {
  if ($lang -eq "en") {
    return @("Downloads | %USERPROFILE%\Downloads", "Notepad | notepad.exe", "Terminal | powershell.exe | -NoExit | terminal")
  }
  return @("Загрузки | %USERPROFILE%\Downloads", "Блокнот | notepad.exe", "Терминал | powershell.exe | -NoExit | terminal")
}
# Календари для «Дня»: выдуманные ссылки (сети в демо нет — встречи выдуманные, из самой программы).
$calendar = "calendar=https://calendar.example.com/work/demo.ics"
$calendar2 = "calendar=webcal://calendar.example.com/home/home.ics"
$scenes = @(
  @{ n = "panel-dark-ru";    lang = "ru"; theme = "dark";  view = "" },
  @{ n = "panel-light-en";   lang = "en"; theme = "light"; view = "" },
  @{ n = "shelf-system-dark-ru";  lang = "ru"; theme = "dark";  view = ""; shelf = "2" },
  @{ n = "shelf-system-light-en"; lang = "en"; theme = "light"; view = ""; shelf = "2" },
  @{ n = "bluetooth-connecting-dark-ru";  lang = "ru"; theme = "dark";  view = "bluetooth-connecting"; shelf = "2" },
  @{ n = "bluetooth-connecting-light-en"; lang = "en"; theme = "light"; view = "bluetooth-connecting"; shelf = "2" },
  @{ n = "bluetooth-off-dark-ru";  lang = "ru"; theme = "dark";  view = "bluetooth-off"; shelf = "2" },
  @{ n = "bluetooth-off-light-en"; lang = "en"; theme = "light"; view = "bluetooth-off"; shelf = "2" },
  @{ n = "shelf-day-dark-ru";     lang = "ru"; theme = "dark";  view = ""; shelf = "3"; extra = @($calendar) },
  @{ n = "shelf-day-light-en";    lang = "en"; theme = "light"; view = ""; shelf = "3"; extra = @($calendar) },
  @{ n = "shelf-day-manage-dark-ru";  lang = "ru"; theme = "dark";  view = "meetings-manage"; shelf = "3"; extra = @($calendar, $calendar2) },
  @{ n = "shelf-day-manage-light-en"; lang = "en"; theme = "light"; view = "meetings-manage"; shelf = "3"; extra = @($calendar, $calendar2) },
  @{ n = "shelf-day-empty-dark-ru";   lang = "ru"; theme = "dark";  view = ""; shelf = "3" },
  @{ n = "shelf-day-empty-light-en";  lang = "en"; theme = "light"; view = ""; shelf = "3" },
  @{ n = "shelf-hand-dark-ru";    lang = "ru"; theme = "dark";  view = ""; shelf = "4"; height = 400 },
  @{ n = "shelf-hand-light-en";   lang = "en"; theme = "light"; view = ""; shelf = "4"; height = 400 },
  @{ n = "edit-dark-ru";     lang = "ru"; theme = "dark";  view = "edit"; extra = @("hidden=media") },
  @{ n = "edit-light-en";    lang = "en"; theme = "light"; view = "edit"; extra = @("hidden=media") },
  @{ n = "scene-dark-ru";    lang = "ru"; theme = "dark";  view = "scene"; scene = "3" },
  @{ n = "scene-light-en";   lang = "en"; theme = "light"; view = "scene"; scene = "3" },
  @{ n = "scene-ask-dark-ru";  lang = "ru"; theme = "dark";  view = "scene-ask"; scene = "3" },
  @{ n = "scene-ask-light-en"; lang = "en"; theme = "light"; view = "scene-ask"; scene = "3" },
  @{ n = "scene-name-dark-ru";  lang = "ru"; theme = "dark";  view = "scene-name"; scene = "3" },
  @{ n = "scene-name-light-en"; lang = "en"; theme = "light"; view = "scene-name"; scene = "3" },
  @{ n = "search-dark-ru";   lang = "ru"; theme = "dark";  view = "search"; query = "1080" },
  @{ n = "search-light-en";  lang = "en"; theme = "light"; view = "search"; query = "1080" },
  @{ n = "settings-dark-ru"; lang = "ru"; theme = "dark";  view = "settings"; height = 480 },
  @{ n = "settings-ask-dark-ru";  lang = "ru"; theme = "dark";  view = "settings-ask"; scene = "3" },
  @{ n = "settings-ask-light-en"; lang = "en"; theme = "light"; view = "settings-ask"; scene = "3" },
  @{ n = "settings-light-en"; lang = "en"; theme = "light"; view = "settings"; height = 480 },
  @{ n = "color-dark-ru";    lang = "ru"; theme = "dark";  view = "color" },
  @{ n = "color-light-en";   lang = "en"; theme = "light"; view = "color" },
  @{ n = "strip-media-dark"; lang = "ru"; theme = "dark";  view = "strip-media" },
  @{ n = "plaque-bluetooth-dark-ru";   lang = "ru"; theme = "dark";  view = "strip-plaque-bluetooth" },
  @{ n = "plaque-bluetooth-light-en";  lang = "en"; theme = "light"; view = "strip-plaque-bluetooth" },
  @{ n = "plaque-battery-dark-ru";     lang = "ru"; theme = "dark";  view = "strip-plaque-battery" },
  @{ n = "plaque-battery-light-en";    lang = "en"; theme = "light"; view = "strip-plaque-battery" },
  @{ n = "plaque-usb-dark-ru";         lang = "ru"; theme = "dark";  view = "strip-plaque-usb" },
  @{ n = "plaque-usb-light-en";        lang = "en"; theme = "light"; view = "strip-plaque-usb" },
  @{ n = "plaque-ejecting-dark-ru";    lang = "ru"; theme = "dark";  view = "strip-plaque-ejecting" },
  @{ n = "plaque-ejecting-light-en";   lang = "en"; theme = "light"; view = "strip-plaque-ejecting" },
  @{ n = "plaque-eject-dark-ru";       lang = "ru"; theme = "dark";  view = "strip-plaque-eject" },
  @{ n = "plaque-eject-light-en";      lang = "en"; theme = "light"; view = "strip-plaque-eject" },
  @{ n = "plaque-busy-dark-ru";        lang = "ru"; theme = "dark";  view = "strip-plaque-busy" },
  @{ n = "plaque-busy-light-en";       lang = "en"; theme = "light"; view = "strip-plaque-busy" },
  @{ n = "plaque-gone-dark-ru";        lang = "ru"; theme = "dark";  view = "strip-plaque-gone" },
  @{ n = "plaque-gone-light-en";       lang = "en"; theme = "light"; view = "strip-plaque-gone" },
  @{ n = "plaque-github-dark-ru";      lang = "ru"; theme = "dark";  view = "strip-plaque-github" },
  @{ n = "plaque-github-light-en";     lang = "en"; theme = "light"; view = "strip-plaque-github" },
  @{ n = "plaque-downloads-dark-ru";   lang = "ru"; theme = "dark";  view = "strip-plaque-downloads" },
  @{ n = "plaque-downloads-light-en";  lang = "en"; theme = "light"; view = "strip-plaque-downloads" },
  @{ n = "plaque-downloads-many-dark-ru";    lang = "ru"; theme = "dark";  view = "strip-plaque-downloads-many" },
  @{ n = "plaque-downloads-many-light-en";   lang = "en"; theme = "light"; view = "strip-plaque-downloads-many" },
  @{ n = "plaque-downloads-danger-dark-ru";  lang = "ru"; theme = "dark";  view = "strip-plaque-downloads-danger" },
  @{ n = "plaque-downloads-danger-light-en"; lang = "en"; theme = "light"; view = "strip-plaque-downloads-danger" },
  @{ n = "plaque-timer-dark-ru";       lang = "ru"; theme = "dark";  view = "strip-plaque-timer" },
  @{ n = "plaque-timer-light-en";      lang = "en"; theme = "light"; view = "strip-plaque-timer" },
  @{ n = "plaque-meeting-dark-ru";     lang = "ru"; theme = "dark";  view = "strip-plaque-meeting" },
  @{ n = "plaque-meeting-light-en";    lang = "en"; theme = "light"; view = "strip-plaque-meeting" }
)
foreach ($s in $scenes) {
  if ($Only -and $s.n -ne $Only) { continue }
  $cfg = @("lang=$($s.lang)", "theme=$($s.theme)", "width=1300", "height=$(if ($s.height) { $s.height } else { 340 })", "monitor=\\.\DISPLAY1",
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
  $env:ANTRESOL_SCENE = $s.scene
  $env:ANTRESOL_QUERY = $s.query
  $p = Start-Process -FilePath $exe -PassThru -WindowStyle Normal -RedirectStandardError "$Out\$($s.n).log"
  if (-not $p.WaitForExit(30000)) { $p.Kill(); "timeout $($s.n)" } else { "done $($s.n)" }
}
