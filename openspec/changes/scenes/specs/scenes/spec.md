## ADDED Requirements

### Requirement: Сцена — экран и система одним щелчком
A scene SHALL apply, by a click on its chip or its hotkey, the display part (mode, scale, HDR of every monitor) and the
system settings it has checked — monitor brightness, night light, Quiet, Keep awake, dark mode, Wi‑Fi, power plan —
and MUST leave unchecked settings untouched. Profiles saved before this change SHALL work as scenes that change the
display only.

#### Scenario: Сцена «Кино»
- **WHEN** у «Кино» отмечены экран 3840 × 2160 @ 60 с HDR, «Тишина» и «Не спать», и пользователь жмёт «Кино»
- **THEN** ставится режим с «Оставить этот режим?», включаются HDR, «Тишина» и «Не спать», остальное не меняется

#### Scenario: Старый профиль
- **WHEN** в `config.txt` профиль без системной части
- **THEN** чип «Сцена» ставит только режим, масштаб и HDR, как раньше

### Requirement: Повторный щелчок — вернуть как было
When a scene's undo switch is on, a click on the chip of the active scene or its hotkey SHALL restore every setting the
scene changed to its value before the scene; with the switch off nothing SHALL happen.

#### Scenario: Вернуть
- **WHEN** «Кино» стоит и пользователь снова жмёт «Кино»
- **THEN** режим, HDR, «Тишина» и «Не спать» — как до «Кино», уведомление «Вернул как было»

#### Scenario: Возврат выключен
- **WHEN** у стоящей сцены выключатель «Повторный щелчок — вернуть как было» выключен и пользователь жмёт её
- **THEN** ничего не меняется, уведомление «Эта сцена уже стоит»

#### Scenario: Нечего возвращать
- **WHEN** сцена стоит, но её не ставили (программу перезапустили или после неё ставили другую) и пользователь жмёт её
- **THEN** настройки не меняются, уведомление «Возвращать нечего»

### Requirement: Правка сцены
The panel SHALL offer a scene editor in place of the cards where the user checks what the scene changes, takes current
values, names it, sets a hotkey, switches undo and deletes the scene after an in-place confirmation.

#### Scenario: Новая сцена
- **WHEN** пользователь жмёт «+» в шапке, отмечает «Тишина», вводит имя «Доклад» и жмёт «Готово»
- **THEN** в шапке чип «Доклад»; сцена меняет экран (как сейчас) и «Тишину»

#### Scenario: Удалить
- **WHEN** пользователь жмёт «Удалить сцену» и подтверждает «Удалить»
- **THEN** сцены нет ни в шапке, ни в настройках, её клавиша освобождена

#### Scenario: Новая сцена без имени
- **WHEN** пользователь жмёт «+», стирает имя и уходит из правки
- **THEN** новой сцены нет

#### Scenario: Имя занято
- **WHEN** пользователь вводит имя, которое уже у другой сцены
- **THEN** под полем «Такая сцена уже есть», «Готово» недоступна, сцене имя не попадает

### Requirement: Файл сцены
`config.txt` SHALL keep a scene in one `profile=` line — name, hotkey, display, system part and `undo=0`. Lines written
before this change SHALL read as before, unknown keys and bad values SHALL be skipped without losing the scene, and a scene
with nothing to set SHALL NOT be saved.

#### Scenario: Сцена без экрана
- **WHEN** в `config.txt` строка `profile=Тишина | Ctrl+Alt+2 |  | quiet=1; awake=1`
- **THEN** сцена читается и меняет только «Тишину» и «Не спать»

#### Scenario: Кривое значение
- **WHEN** в системной части `bright=300; quiet=1`
- **THEN** яркость пропущена, «Тишина» отмечена, сцена остаётся
