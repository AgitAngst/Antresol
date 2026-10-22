## ADDED Requirements

### Requirement: Выбор и порядок карточек
The settings SHALL let the user turn each card on or off and move it up or down; hidden cards MUST keep working in the background.

#### Scenario: Выключить карточку
- **WHEN** пользователь выключает «Медиа» в «Карточки панели»
- **THEN** карточки «Медиа» нет в панели, порядок остальных прежний

#### Scenario: Сдвинуть
- **WHEN** пользователь жмёт «Выше» у «Кнопки»
- **THEN** «Кнопки» встают перед соседней карточкой, порядок сохраняется после перезапуска

### Requirement: Bluetooth-устройства
The panel SHALL list paired Bluetooth devices with their connection state and battery charge when Windows knows it.

#### Scenario: Наушники подключены
- **WHEN** сопряжённые наушники подключены и отдают заряд
- **THEN** строка с зелёной точкой и зарядом «80%»
