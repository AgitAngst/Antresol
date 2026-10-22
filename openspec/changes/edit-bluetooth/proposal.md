## Why

Кандидаты «Режим правки» (Command Palette Dock) и «Bluetooth-устройства» (DynamicWin) из SPEC.md.
Карточек стало восемь — нужно выбирать, какие видны и в каком порядке.

## What Changes

- Настройки: карточка «Карточки панели» — включить или выключить каждую, «Выше» и «Ниже». Выключенные
  не видны, но работают (буфер обмена запоминает). Строки `modules=` и `hidden=` в `config.txt`.
- Карточка «Bluetooth»: сопряжённые устройства, подключено ли, заряд (если Windows его знает), кнопка
  «Параметры Bluetooth». Подключать из панели нельзя.
- Значение в строке «Ресурсов» уходит под подпись, если вместе не влезают; наименьшая ширина «Ресурсов» — 260.
- Версия 0.9.0.

## Capabilities

### New Capabilities
- `edit-bluetooth`: выбор и порядок карточек, Bluetooth-устройства.

### Modified Capabilities
- нет.

## Impact

`src/sys/bt.rs`, `src/modules/bluetooth.rs` (новые), `src/app.rs`, `src/config.rs` (`order`, `hidden`,
`arrange`), `src/modules/resources.rs`, `Cargo.toml` (Bluetooth, SetupAPI, Properties), docs.
