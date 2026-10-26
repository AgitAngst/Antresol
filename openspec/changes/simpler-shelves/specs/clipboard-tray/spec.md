## MODIFIED Requirements

### Requirement: История буфера обмена
The panel SHALL remember the last 20 copied texts in memory only, MUST skip copies marked private by password managers,
and SHALL put a text back on the clipboard on click. Pinned texts (stored as `snippet=`, up to 30) SHALL be listed
above the history in the same card; a pin button MUST pin a history text and unpin a pinned one.

#### Scenario: Скопировали текст
- **WHEN** пользователь копирует текст в любой программе (панель может быть спрятана)
- **THEN** текст появляется наверху недавнего в карточке «Буфер обмена»

#### Scenario: Вернуть текст
- **WHEN** пользователь щёлкает строку истории или закреплённую
- **THEN** текст снова в буфере обмена, уведомление «В буфере — вставляйте»

#### Scenario: Закрепить
- **WHEN** пользователь жмёт булавку у строки истории
- **THEN** текст вверху закреплённого и остаётся после перезапуска; булавка у закреплённой — открепляет
