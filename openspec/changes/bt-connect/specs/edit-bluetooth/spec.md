## ADDED Requirements

### Requirement: Подключение одним щелчком
The «Bluetooth» card SHALL connect a paired Bluetooth audio device (headphones, headset, speaker) with one click on its
row or its «Подключить» button and disconnect a connected one with one click, showing «Подключение…» while it is in
progress; devices that cannot be connected this way MUST have no button.

#### Scenario: Подключить наушники
- **WHEN** сопряжённые наушники включены, но не подключены, и пользователь щёлкает «Подключить»
- **THEN** строка показывает «Подключение…», затем «Подключено» и заряд

#### Scenario: Наушники выключены
- **WHEN** наушники выключены и пользователь щёлкает «Подключить»
- **THEN** через 15 с строка снова «Не подключено» и уведомление «Не удалось подключить «Наушники»: включите устройство
  и поднесите ближе»

#### Scenario: Мышь
- **WHEN** сопряжённая мышь не подключена
- **THEN** у её строки нет кнопки, подсказка «Подключается само, когда устройство включают»
