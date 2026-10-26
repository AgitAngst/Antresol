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

### Requirement: Правка сцены
The panel SHALL offer a scene editor in place of the cards where the user checks what the scene changes, takes current
values, names it, sets a hotkey, switches undo and deletes the scene after an in-place confirmation.

#### Scenario: Новая сцена
- **WHEN** пользователь жмёт «+» в шапке, отмечает «Тишина», вводит имя «Доклад» и жмёт «Готово»
- **THEN** в шапке чип «Доклад»; сцена меняет экран (как сейчас) и «Тишину»

#### Scenario: Удалить
- **WHEN** пользователь жмёт «Удалить сцену» и подтверждает «Удалить»
- **THEN** сцены нет ни в шапке, ни в настройках, её клавиша освобождена
