## ADDED Requirements

### Requirement: Цвет программы
The panel SHALL use lime as the default accent and MUST let the user pick one of five presets or a custom color, adjusted per theme so accent text keeps a contrast of at least 4.5:1.

#### Scenario: Свой цвет
- **WHEN** пользователь выбрал в «Свой цвет…» тёмно-синий и нажал «Применить»
- **THEN** в тёмной теме акцент осветлён до читаемого, в светлой — как выбран; цвет сохраняется в `config.txt`

### Requirement: Профили в шапке
The panel header SHALL show display profiles as chips; a click applies the profile with the usual «Оставить?» revert.

#### Scenario: Профиль из шапки
- **WHEN** пользователь щёлкает «Игры» в шапке
- **THEN** профиль ставится, в карточке «Экран» — баннер «Оставить?»

### Requirement: Медиа на спрятанной полоске
With the setting on and something playing, the hidden strip SHALL grow to show the track and previous, play/pause and next buttons that work without sliding the panel out.

#### Scenario: Пауза с полоски
- **WHEN** панель спрятана, играет музыка, пользователь жмёт «пауза» на полоске
- **THEN** плеер встаёт на паузу, панель не выезжает

#### Scenario: Ничего не играет
- **WHEN** плеер закрыт
- **THEN** полоска обычная, 120 × 5

### Requirement: Плашка таймера
When a timer ends while the panel is hidden and the strip is on, the panel SHALL show a slim banner at the edge instead of sliding out entirely.

#### Scenario: Таймер дошёл
- **WHEN** таймер дошёл, панель спрятана
- **THEN** у края плашка с текстом и «Готово»; щелчок по тексту открывает панель
