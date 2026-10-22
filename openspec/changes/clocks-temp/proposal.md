## Why

Владелец 22.10.2026: всё из мозгового штурма, кроме окон. Остались: температура и часы других поясов.

## What Changes

- «Ресурсы»: строка «Температура» — самая горячая тепловая зона ACPI (PDH, без прав администратора).
- Карточка «Часы»: время в выбранных поясах Windows, смещение и разница с ПК; добавить поиском, убрать;
  `clock=` в config.txt.
- Версия 1.7.0.

## Capabilities

### New Capabilities
- `clocks-temp`: температура, часы поясов.

### Modified Capabilities
- нет.

## Impact

`src/sys/tz.rs`, `src/modules/clocks.rs` (новые), `src/sys/res.rs`, `src/modules/resources.rs`, `src/config.rs`,
`Cargo.toml` (`Win32_System_Time`), тексты, docs.
