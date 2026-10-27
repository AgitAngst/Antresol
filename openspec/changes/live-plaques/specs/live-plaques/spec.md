## ADDED Requirements

### Requirement: Плашка события у края
While the panel is hidden, an event SHALL be shown as a plaque at the edge without sliding the panel out and without
taking the focus; an ordinary plaque MUST disappear after 4 s unless the pointer is over it, an important one SHALL stay
until closed, and plaques SHALL come one at a time in order.

#### Scenario: Наушники подключились
- **WHEN** панель спрятана и подключаются Bluetooth-наушники
- **THEN** у края плашка «Наушники подключены» с зарядом, через 4 с уходит, панель не выезжает

#### Scenario: Сборка упала
- **WHEN** последний прогон GitHub сменился на «упал»
- **THEN** плашка висит, пока её не закроют; «Открыть» открывает прогон в браузере

### Requirement: Флешка и загрузки
The panel SHALL announce a plugged removable drive with «Открыть» and a safe «Извлечь», and a finished download in the
Downloads folder with «Открыть» and «В лоток»; temporary download files MUST be ignored.

#### Scenario: Извлечь
- **WHEN** пользователь жмёт «Извлечь» на плашке флешки
- **THEN** диск безопасно извлечён, плашка «Можно вынимать»; занят — «Не извлечь: диск занят»

#### Scenario: Недокачанный файл
- **WHEN** в «Загрузках» появляется `файл.crdownload`
- **THEN** плашки нет, пока не появится готовый файл

### Requirement: Выключатели плашек
The panel settings SHALL let the user turn each kind of plaque on or off; the timer plaque MUST always show.

#### Scenario: Без загрузок
- **WHEN** «Загрузки» выключены в «Живых плашках»
- **THEN** готовый файл не показывается у края
