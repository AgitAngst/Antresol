## ADDED Requirements

### Requirement: Картинки, ссылки и цвета в истории
The clipboard history SHALL keep copied images (up to 5 and up to 80 MB together, each up to 40 MB, memory only), links and
colors besides plain text, MUST skip private copies of password managers for every kind (the formats
`ExcludeClipboardContentFromMonitorProcessing`, `Clipboard Viewer Ignore` and `CanIncludeInClipboardHistory` = 0), and SHALL
put any entry back on the clipboard on click, a color as the text it was copied with. Colors taken with the color picker
SHALL appear in the history too. The clipboard SHALL be read and the thumbnail made outside the window thread.

#### Scenario: Снимок экрана
- **WHEN** пользователь копирует снимок экрана
- **THEN** наверху истории строка с миниатюрой и размером; щелчок кладёт картинку обратно в буфер

#### Scenario: Снимок 4K целиком
- **WHEN** пользователь копирует полный снимок экрана 4K (33 МБ)
- **THEN** он попадает в историю; снимок двух мониторов 4K (больше 40 МБ) — нет

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
history; the punctuation MUST be read as typed with that layout (comma `?`, question mark `&`, colon `^`, semicolon `$`,
quote `@`, number sign `#`, full stop `/`, and back); addresses, links, paths, code, short words and short capital
abbreviations MUST NOT be offered.

#### Scenario: Русский в английской раскладке
- **WHEN** в буфере «Ghbdtn, rfr ltkf?»
- **THEN** у строки кнопка «Исправить раскладку»; щелчок — в буфере «Привет, как дела?»

#### Scenario: Вопрос, набранный клавишами
- **WHEN** в буфере «Ghbdtn? rfr ltkf&» (русское «Привет, как дела?» в английской раскладке)
- **THEN** у строки кнопка; щелчок — в буфере «Привет, как дела?»

#### Scenario: Ссылка
- **WHEN** в буфере «https://example.com/ghbdtn»
- **THEN** кнопки нет

## MODIFIED Requirements

### Requirement: История буфера обмена
The panel SHALL remember the last 20 copied entries of any kind (texts, links, colors, images) in memory only, MUST skip
copies marked private by password managers, and SHALL put an entry back on the clipboard on click. Pinned texts (stored as
`snippet=`, up to 30) SHALL be listed above the history in the same card; a pin button MUST pin a history text and unpin a
pinned one. Turning remembering off MUST erase the recent entries, images included; pinned texts stay.

#### Scenario: Скопировали текст
- **WHEN** пользователь копирует текст в любой программе (панель может быть спрятана)
- **THEN** текст появляется наверху недавнего в карточке «Буфер обмена»

#### Scenario: Вернуть текст
- **WHEN** пользователь щёлкает строку истории или закреплённую
- **THEN** запись (текст, ссылка, цвет, картинка) снова в буфере обмена, уведомление «В буфере — вставляйте»

#### Scenario: Закрепить
- **WHEN** пользователь жмёт булавку у строки истории
- **THEN** текст вверху закреплённого и остаётся после перезапуска; булавка у закреплённой — открепляет

#### Scenario: Выключили запоминание
- **WHEN** пользователь выключает «Запоминать скопированное» в настройках
- **THEN** недавнее (и картинки) стирается, закреплённое остаётся; включили снова — история начинается заново
