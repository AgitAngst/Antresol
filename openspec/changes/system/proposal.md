## Why

Владелец 22.10.2026: всё из мозгового штурма, кроме окон. Раздел «Система».

## What Changes

- В «Быстрых настройках»: плитки «Не спать», «Тёмная тема» (Windows), «Без уведомлений», «Wi‑Fi»; чипы
  «План питания»; список VPN Windows с «Подключить» / «Отключить».
- Версия 1.5.0.

## Capabilities

### New Capabilities
- `system`: быстрые системные переключатели.

### Modified Capabilities
- нет.

## Impact

`src/sys/system.rs`, `src/sys/net.rs` (новые), `src/modules/quick.rs`, `Cargo.toml` (фичи WinRT Radios,
Connectivity, RAS), тексты, docs.
