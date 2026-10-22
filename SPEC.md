# Antresol — замысел и этапы

Панель у верхнего края монитора: прячется за край, выезжает, когда мышь касается края или по
горячей клавише. Размер меняется мышью, функции — модули, движение по набору Anvil. Запросы
владельца 22.10.2026: изменяемый размер, прячется, можно добавлять новые функции и быстрые
настройки, красивые анимации; первые функции — разрешение и масштаб экрана, звук.

## Этап 0.1 — первая версия (сделан 22.10.2026)

- Окно-панель у края, автоскрытие, выезд по краю и по клавише, показ при запуске, закрепление.
- Размер тянется за нижний край, бока и нижние углы; запоминается.
- Модуль «Экран»: разрешение, частота, масштаб; «Оставить?» с откатом через 15 с.
- Модуль «Звук»: громкость, без звука, устройство вывода.
- Модуль «Кнопки»: свои кнопки из `buttons.txt`.
- Настройки: монитор, автозапуск, «не выезжать над играми», задержки, тема, язык.

## Этап 0.2 — избранное и частоты (сделан 22.10.2026, запрос владельца)

- До трёх любимых разрешений на монитор: звёздочка в списке, чипы сверху, один щелчок.
- Частота запоминается для каждого разрешения и ставится при переходе на него.
- Частоты и масштабы — чипами с переносом (в 0.1 полоса обрезалась).

## Этап 0.3 — профили экрана с горячими клавишами (сделан 22.10.2026, запрос владельца)

Изменение OpenSpec `display-profiles` (архив `2026-10-22-display-profiles`).
- Профиль: имя, клавиша, режим и масштаб каждого монитора; сохраняется из текущего, ставится щелчком.
- Клавиша профиля назначается в настройках захватом сочетания, работает при спрятанной панели.
- Применение — с «Оставить этот режим?» и откатом всех изменённых мониторов через 15 с.
- Не вошло: расположение мониторов в профиле, клавиши на избранное, выбор клавиши показа панели.

## 0.3.1 — звук убран (22.10.2026, решение владельца)

Владелец: «Исключи всё что связано со звуком. Потом сделаем интеграцию Kamerton.» Карточка «Звук»
(громкость, «без звука», устройство вывода) и её системная часть удалены; громкость по программам и
микрофон из планов сняты. Звук в панели появится через интеграцию с
[Kamerton](https://github.com/AgitAngst/Kamerton) — отдельный этап после согласования с владельцем.

## Кандидаты этапов — по аналогам (22.10.2026, не согласованы, порядок решит владелец)

Собраны по программам, которые обсуждают в r/software и r/Windows11 (сам Reddit прочитать не удалось —
взяты страницы, README и обзоры этих программ). ★ — первыми, по мнению сессии: ближе всего к
назначению панели (быстрые настройки системы).

| | Кандидат | Что | Аналоги |
|---|---|---|---|
| ★ | Профили экрана | Сохранить весь режим (разрешение, частота, масштаб, расположение мониторов) и ставить одним щелчком | [Display Changer X](https://www.softpedia.com/hubs/Resolution-Changer/), [Display Hot Keys](https://alternativeto.net/software/display-hot-keys/about/) |
| ★ | Горячие клавиши на избранное и профили | Сменить режим, не открывая панель; несколько мониторов одной клавишей | [Display Hot Keys](https://alternativeto.net/software/display-hot-keys/about/), [RezSwap](https://github.com/nullvalue0/RezSwap) |
| ★ | Яркость внешних мониторов | DDC/CI: яркость, контраст, вход монитора; выравнивание яркости между мониторами | [Twinkle Tray](https://cdn.jsdelivr.net/gh/xanderfrangos/twinkle-tray@master/README.md) |
| ★ | Медиа | Пауза, следующий трек, название и обложка из Spotify, браузера, плееров (системные медиа-сеансы) | [Isle Notch](https://toolradar.com/tools/isle-notch), [DynamicWin](https://github.com/FlorianButz/DynamicWin), [Command Palette Dock](https://windowslatest.com/2026/02/07/windows-11-is-testing-an-optional-dock-that-sits-on-the-top-of-the-screen-with-music-playback-control-cpu-gpu-ram-stats-and-more) |
| | Полоска в покое | Тонкая метка у края, что панель есть; раскрывается при наведении или событии | [Isle Notch](https://toolradar.com/tools/isle-notch), [Notchify](https://www.windowscentral.com/software-apps/notchify-brings-macos-style-dynamic-island-flair-to-windows-11) |
| | Не выезжать над выбранными программами | Список программ, над которыми панель не выезжает, — не только полный экран | [Isle Notch](https://toolradar.com/tools/isle-notch) |
| | Несколько мониторов | Панель на всех мониторах или на выбранных, у каждой свои настройки | [Isle Notch](https://toolradar.com/tools/isle-notch) |
| | Ресурсы | ЦП, видеокарта, память, сеть, батарея, температура | [Command Palette Dock](https://pureinfotech.com/powertoys-command-palette-dock-windows-11/) |
| | История буфера обмена | Последние скопированные тексты, вставить щелчком | [Notchify](https://www.windowscentral.com/software-apps/want-your-pc-to-be-a-bit-more-like-a-mac-the-dynamic-island-for-windows-just-got-a-big-update), [Command Palette Dock](https://windowslatest.com/2026/02/07/windows-11-is-testing-an-optional-dock-that-sits-on-the-top-of-the-screen-with-music-playback-control-cpu-gpu-ram-stats-and-more) |
| | Лоток файлов | Бросить файл в панель, забрать или открыть позже | [DynamicWin](https://github.com/FlorianButz/DynamicWin) |
| | Bluetooth-устройства | Подключить наушники и мышь, увидеть заряд | [DynamicWin](https://github.com/FlorianButz/DynamicWin) |
| | Режим правки | Перетаскивать модули, включать и выключать, показывать только значки | [Command Palette Dock](https://windowslatest.com/2026/02/07/windows-11-is-testing-an-optional-dock-that-sits-on-the-top-of-the-screen-with-music-playback-control-cpu-gpu-ram-stats-and-more) |
| | Таймеры, напоминания, календарь | Короткий таймер, «помидор», ближайшие события | [Command Palette Dock](https://windowslatest.com/2026/02/07/windows-11-is-testing-an-optional-dock-that-sits-on-the-top-of-the-screen-with-music-playback-control-cpu-gpu-ram-stats-and-more), [DynamicWin](https://github.com/FlorianButz/DynamicWin) |
| | Интеграция Kamerton | Звук в панели — из Kamerton (после согласования с владельцем) | [Kamerton](https://github.com/AgitAngst/Kamerton) |
| | Состояние GitHub | CI и пулл-реквесты своих репозиториев (без запуска CI) | [Command Palette Dock](https://windowslatest.com/2026/02/07/windows-11-is-testing-an-optional-dock-that-sits-on-the-top-of-the-screen-with-music-playback-control-cpu-gpu-ram-stats-and-more) |

## Дальше — идеи из мозгового штурма (не согласованы, порядок решит владелец)

| Раздел | Что |
|---|---|
| Экран | яркость внешних мониторов (DDC/CI), ночной свет, основной монитор, профили «работа/игра» |
| Система | не давать уснуть, план питания, тема Windows, «не беспокоить», Bluetooth-устройства, Wi‑Fi и VPN |
| Окна | раскладки по зонам, «поверх всех», перенос на другой монитор |
| Буфер и снимки | история буфера, заготовки текста, пипетка, снимок области с распознаванием |
| Состояние | ЦП, видеокарта, память, температура, место на E:, сеть, CI на GitHub |
| Медиа и время | плеер, таймер, «помидор», часы других поясов |
| Сама панель | значок программы, порядок и включение модулей в настройках, выбор горячей клавиши |
