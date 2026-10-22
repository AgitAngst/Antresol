## Why

Кандидаты «История буфера обмена» и «Лоток файлов» из SPEC.md (Notchify, Command Palette Dock, DynamicWin).

## What Changes

- Карточка «Буфер обмена»: последние 20 скопированных текстов (только в памяти, на диск не пишутся;
  приватные копии менеджеров паролей пропускаются), щелчок — положить текст обратно в буфер, «Очистить».
  Запоминание выключается в настройках.
- Карточка «Лоток»: файлы, брошенные на панель (до 30, строки `tray=`); щелчок — открыть, «Показать в
  папке», убрать. Перетащить из лотка в другую программу пока нельзя.
- Модули получают `background` — вызов каждый кадр, даже у спрятанной панели.
- Версия 0.8.0.

## Capabilities

### New Capabilities
- `clipboard-tray`: история буфера и лоток.

### Modified Capabilities
- нет.

## Impact

`src/sys/clip.rs`, `src/modules/clipboard.rs`, `src/modules/tray.rs` (новые), `src/modules/mod.rs`, `src/app.rs`,
`src/config.rs` (`clipboard`, `tray=`), `src/main.rs` (перетаскивание), `Cargo.toml` (DataExchange, Memory, Ole), docs.
