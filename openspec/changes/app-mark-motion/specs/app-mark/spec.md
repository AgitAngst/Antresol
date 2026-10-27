## ADDED Requirements

### Requirement: Значок программы
The program SHALL have one own icon, the same in both themes — a light rounded tile with a monitor whose top panel is
bright and the rest dim — used for the exe file, the panel window and the About window; small sizes (16–48 px) MUST be
drawn from the simplified version, from 64 px from the full one.

#### Scenario: Значок exe
- **WHEN** собран `antresol.exe` и значок прочитан из файла
- **THEN** у него кадры 16, 24, 32, 48, 64, 128, 256 и непрозрачный центр

#### Scenario: Пересборка значка
- **WHEN** скрипт значка запущен второй раз без правок SVG
- **THEN** файлы совпадают по хэшам

### Requirement: Знак в интерфейсе
The settings header SHALL show the mark at 28 px and the About window at 56 px, drawn by code from the simplified SVG with
fixed colours independent of the theme and the language.

#### Scenario: Смена темы
- **WHEN** тема переключена со светлой на тёмную
- **THEN** знак не меняется

### Requirement: Анимация знака
The mark SHALL play one scene per event, once: A «Выезд» on launch and when About opens, B «Прятки» following the panel
showing and hiding, C «Модули» while an update check or download runs, D «Пульс» a twice-blink on a reminder or meeting
plaque and, in the error colour, on an error; with animations turned off in Windows the mark MUST stand in its final state.

#### Scenario: Запуск
- **WHEN** программа запущена и окно настроек открыто
- **THEN** панель знака выезжает из-за кромки экрана знака за 240 мс и встаёт на место

#### Scenario: Панель спряталась
- **WHEN** панель уезжает к краю
- **THEN** панель знака сжимается до тонкой полоски; когда панель выехала — раскрывается

#### Scenario: Проверка обновлений
- **WHEN** в «О программе» нажата «Проверить обновления»
- **THEN** три метки в панели знака загораются по очереди, пока нет ответа

#### Scenario: Напоминание
- **WHEN** выезжает плашка напоминания, а шапка настроек на виду
- **THEN** панель знака мигает яркостью два раза и возвращается; ошибка — то же в цвете «ошибка»

#### Scenario: Анимация выключена
- **WHEN** в Windows выключено «Показывать анимацию»
- **THEN** сцены не играют, знак стоит с раскрытой панелью
