## Why

Владелец: «Делай интеграцию с камертон». Звук из панели убран в 0.3.1 — его даёт Kamerton.

## What Changes

- Kamerton 0.2.1 слушает канал `\\.\pipe\kamerton-ipc` (протокол версии 1: `state`, `volume`, `mute`,
  `app-volume`, `app-mute`, `open`).
- Карточка «Звук»: громкость и «без звука» устройства вывода по умолчанию и программ на нём, «Микшер
  Kamerton»; нет Kamerton — «Запустить» (путь `kamerton=` в config.txt) или «Где взять».
- Версия 1.3.0.

## Capabilities

### New Capabilities
- `kamerton-sound`: звук через Kamerton.

### Modified Capabilities
- нет.

## Impact

`src/sys/kamerton.rs`, `src/modules/sound.rs` (новые), `src/modules/mod.rs`, `src/config.rs`, `src/texts.rs`, docs.
