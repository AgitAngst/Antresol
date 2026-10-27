## ADDED Requirements

### Requirement: Ближайшая встреча
With one or more calendar links (.ics) the «Встречи» card SHALL show the current or next meeting of today with its time
and service and a «Подключиться» button when the event has a meeting link; recurring events, exceptions, cancelled
events and time zones from the file MUST be honoured.

#### Scenario: Планёрка через 12 минут
- **WHEN** в календаре повторяющаяся по будням «Планёрка» в 10:30 с ссылкой Zoom, сейчас 10:18
- **THEN** в карточке «Планёрка», «10:30–10:45 · через 12 мин», значок «Zoom» и «Подключиться»

#### Scenario: Отменённый повтор
- **WHEN** сегодняшний повтор «Планёрки» отменён в календаре
- **THEN** его нет, ближайшей стоит следующая встреча

### Requirement: Секретная ссылка
The calendar link SHALL be stored only in `config.txt`, MUST be shown shortened in the window and MUST NOT be written to
logs, notifications or screenshots.

#### Scenario: Ошибка загрузки
- **WHEN** календарь не отвечает
- **THEN** под календарём «Календарь не ответил», в журнале — без адреса, прежние встречи остаются

### Requirement: Плашка перед встречей
Two minutes before a meeting with a link starts the panel SHALL show a plaque with «Подключиться» that stays until the
meeting begins.

#### Scenario: Через 2 минуты
- **WHEN** до «Планёрки» 2 минуты, панель спрятана
- **THEN** у края плашка «Планёрка через 2 мин» с «Подключиться»
