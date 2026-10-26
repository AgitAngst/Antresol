## ADDED Requirements

### Requirement: Картинки, ссылки и цвета в истории
The clipboard history SHALL keep copied images (up to 5, each up to 16 MB, memory only), links and colors besides plain
text, MUST skip private copies of password managers for every kind, and SHALL put any entry back on the clipboard on
click. Colors taken with the color picker SHALL appear in the history too.

#### Scenario: Снимок экрана
- **WHEN** пользователь копирует снимок экрана
- **THEN** наверху истории строка с миниатюрой и размером; щелчок кладёт картинку обратно в буфер

#### Scenario: Цвет пипетки
- **WHEN** пользователь берёт цвет «Пипеткой»
- **THEN** в истории строка с кружком и кодом цвета

### Requirement: Фильтр истории
The clipboard card SHALL filter its entries by kind with chips «Все», «Текст», «Ссылки», «Картинки», «Цвета».

#### Scenario: Только картинки
- **WHEN** пользователь выбирает «Картинки»
- **THEN** видны только картинки; нет картинок — «Здесь пока ничего нет»

### Requirement: Исправить раскладку
For a text that looks typed with the wrong keyboard layout (Russian with the English one or the other way round) the
card SHALL offer «Исправить раскладку», which MUST put the converted text on the clipboard and at the top of the
history; addresses, links, paths, code and short words MUST NOT be offered.

#### Scenario: Русский в английской раскладке
- **WHEN** в буфере «Ghbdtn, rfr ltkf?»
- **THEN** у строки кнопка «Исправить раскладку»; щелчок — в буфере «Привет, как дела?»

#### Scenario: Ссылка
- **WHEN** в буфере «https://example.com/ghbdtn»
- **THEN** кнопки нет
