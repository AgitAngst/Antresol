## Why

Кандидат «Ресурсы» из SPEC.md (аналог — Command Palette Dock): нагрузка ПК одним взглядом.

## What Changes

- Карточка «Ресурсы»: процессор с графиком за минуту, видеокарта (движки 3D, PDH), память, сеть
  (приём и отдача в секунду), батарея (если есть), свободное место на несъёмных дисках.
- Замер — при перечитывании панели (раз в 1,5 с, пока она открыта). Температуры нет: Windows отдаёт её
  только через WMI с правами администратора или драйвер.
- Демо-режим: все значения выдуманные (как и яркость, входы монитора) — для снимков в открытый репозиторий.
- Версия 0.7.0.

## Capabilities

### New Capabilities
- `resources`: нагрузка и место.

### Modified Capabilities
- нет.

## Impact

`src/sys/res.rs`, `src/modules/resources.rs` (новые), `src/sys/ddc.rs` (демо), `Cargo.toml` (функции
IpHelper, Ndis, FileSystem, Performance, Power, SystemInformation крейта `windows`), docs.
