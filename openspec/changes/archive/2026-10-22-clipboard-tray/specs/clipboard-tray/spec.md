## ADDED Requirements

### Requirement: История буфера обмена
The panel SHALL remember the last 20 copied texts in memory only, MUST skip copies marked private by password managers, and SHALL put a text back on the clipboard on click.

#### Scenario: Скопировали текст
- **WHEN** пользователь копирует текст в любой программе (панель может быть спрятана)
- **THEN** текст появляется наверху карточки «Буфер обмена»

#### Scenario: Вернуть текст
- **WHEN** пользователь щёлкает строку истории
- **THEN** текст снова в буфере обмена, уведомление «В буфере — вставляйте»

### Requirement: Лоток файлов
The panel SHALL keep files dropped on it (up to 30) and open a file on click or show it in its folder.

#### Scenario: Бросили файл
- **WHEN** пользователь бросает файл на панель
- **THEN** файл появляется наверху карточки «Лоток» и остаётся там после перезапуска
