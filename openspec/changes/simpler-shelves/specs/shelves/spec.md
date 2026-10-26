## ADDED Requirements

### Requirement: Полки во вкладках
The panel SHALL group cards into shelves shown as tabs in the header and MUST show only the cards of the open shelf;
by default there are three shelves — «Экран и звук», «Система», «Под рукой». The open shelf SHALL be remembered while
the program runs. Default shelf names MUST follow the interface language.

#### Scenario: Открыть полку
- **WHEN** пользователь щёлкает вкладку «Система»
- **THEN** в панели карточки «Быстрые настройки», «Ресурсы», «Bluetooth», «GitHub», «Часы»; вкладка выделена чертой акцента

#### Scenario: Полка помнится
- **WHEN** панель спряталась на полке «Под рукой» и выехала снова
- **THEN** открыта «Под рукой»

#### Scenario: Сменили язык
- **WHEN** пользователь переключил язык на английский
- **THEN** вкладки по умолчанию — "Display & sound", "System", "At hand"; свои имена полок не меняются

### Requirement: Переход со старых настроек
On a `config.txt` without shelves the panel SHALL build the default shelves, keep cards from `hidden=` removed and
order cards inside each shelf by the old `modules=` list. A known card that is neither on a shelf nor removed MUST be
added to the end of its default shelf.

#### Scenario: Старый файл
- **WHEN** в `config.txt` от 1.7 `hidden=media` и «Кнопки» стояли раньше «Буфера обмена»
- **THEN** «Медиа» убрана, на «Под рукой» «Кнопки» стоят перед «Буфером обмена»

### Requirement: Правка полок на месте
The panel SHALL offer an edit mode from the header where cards can be reordered within a shelf, moved to another
shelf by dropping them on its tab, removed and returned, and shelves can be added, renamed and removed; the same
moves MUST be possible from the keyboard. «Как было» SHALL restore the layout from the start of the edit.

#### Scenario: Перенести на другую полку
- **WHEN** в правке пользователь тянет «Медиа» за ручку и бросает на вкладку «Система»
- **THEN** «Медиа» в конце полки «Система», раскладка сохраняется после перезапуска

#### Scenario: Убрать и вернуть
- **WHEN** пользователь жмёт крестик на «Яркости», затем чип «Яркость» в «Убранных карточках»
- **THEN** «Яркость» сначала уходит в «Убранные», потом встаёт в конец открытой полки

#### Scenario: С клавиатуры
- **WHEN** ручка «Звука» в фокусе и нажато Alt + ←
- **THEN** «Звук» встаёт на место левее

#### Scenario: Содержимое под вуалью не отвечает
- **WHEN** в правке нажаты Tab, Shift+Tab, стрелки, Enter или пробел, а мышь и колесо — над карточкой
- **THEN** фокус не заходит в содержимое карточек, ничего в них не нажимается и не листается

#### Scenario: Двойной щелчок вне правки
- **WHEN** вне правки пользователь дважды щёлкает по вкладке
- **THEN** поле имени не открывается (полка просто открыта); в правке — открывается

### Requirement: Ряд карточек в правке
In edit mode the cards SHALL shrink to a smallest width of 150 points and the removed-cards block to 220 so that the
three default shelves with the block fit a 1300-point panel; a row that still does not fit MUST scroll, and the
«Ещё N ›» button and Shift + wheel MUST work above the edit layers.

#### Scenario: Помещается
- **WHEN** на панели 1300 точек открыта любая из трёх готовых полок в правке
- **THEN** «Убранные карточки» видны целиком, ряд не листается

#### Scenario: Не помещается
- **WHEN** на панели 780 точек в правке щёлкают «Ещё N ›» или крутят колесо с Shift над карточкой
- **THEN** ряд листается

### Requirement: Шапка не наезжает
When the header lacks room it SHALL give up, in order, the side labels, the word «Antresol», the label of «Полка»,
the width of tab labels (ellipsis, full name in the tooltip) and the profiles and monitor picker; its parts MUST NOT
overlap at any width from 560 points.

#### Scenario: Узкая панель
- **WHEN** панель 560 точек, три полки и три профиля
- **THEN** вкладки и значки не перекрывают друг друга, подписи вкладок обрезаны многоточием

### Requirement: Профиль и правка
A profile hotkey pressed in edit mode SHALL first leave it, and when «Оставить этот режим?» waits on a card of another
shelf that shelf MUST be opened.

#### Scenario: Клавиша профиля в правке
- **WHEN** открыта правка полки «Под рукой» и нажата клавиша профиля, меняющая режим
- **THEN** правка закрыта, открыта полка «Экран и звук» с вопросом «Оставить этот режим?»

### Requirement: Метка на вкладке
A module SHALL be able to show a short live badge on the tab of its shelf while another shelf is open; the running
timer MUST show its time left there.

#### Scenario: Идёт таймер
- **WHEN** идёт таймер «5 мин», открыта полка «Экран и звук»
- **THEN** на вкладке полки с таймерами — «03:42», подсказка «Идёт таймер «5 мин», осталось 03:42»

### Requirement: Быстрые настройки без обрезки
Quick setting tiles SHALL use short positive names — «Не спать», «Тёмная тема», «Тишина», «Wi‑Fi» — and MUST show
the Wi‑Fi network name in the state line, not in the name; «Ночной свет» SHALL be a switch in the «Яркость» card.

#### Scenario: Тишина
- **WHEN** пользователь включает «Тишину»
- **THEN** уведомления Windows не всплывают, плитка выделена, состояние — «без всплывающих»

#### Scenario: Ночной свет без DDC/CI
- **WHEN** монитор не отвечает по DDC/CI
- **THEN** в «Яркости» пояснение про DDC/CI и выключатель «Ночной свет», который работает

### Requirement: Читаемый текст
Any text a user is meant to read SHALL have a contrast of at least 4.5:1 in both themes; the `faint` color MUST be
used only for unavailable controls and drawings.

#### Scenario: Таймер на паузе
- **WHEN** таймер поставлен на паузу
- **THEN** время цветом `weak`, к названию добавлено «на паузе»
