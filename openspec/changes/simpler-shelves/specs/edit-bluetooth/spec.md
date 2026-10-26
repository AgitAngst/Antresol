## MODIFIED Requirements

### Requirement: Выбор и порядок карточек
The panel SHALL let the user remove and return cards and change their order and shelf in the shelf edit mode on the
panel itself (see `shelves`); the settings MUST NOT have a separate card list. Removed cards MUST keep working in the
background.

#### Scenario: Выключить карточку
- **WHEN** в правке полок пользователь жмёт крестик на «Медиа»
- **THEN** «Медиа» нет на полке, она в «Убранных карточках», порядок остальных прежний

#### Scenario: Сдвинуть
- **WHEN** пользователь тянет «Кнопки» за ручку левее «Таймеров»
- **THEN** «Кнопки» встают перед «Таймерами», порядок сохраняется после перезапуска
