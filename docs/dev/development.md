# Как разрабатывать

Нужны Rust (1.98) и Windows. Только диск E:.

```bash
cargo build -j 4
python E:/dev_personal/tools/checks.py --repo . -j 4 fmt test clippy
```

Тесты — логика показа/скрытия (`panel.rs`), разбор файлов (`config.rs`), выбор режима и масштаба
(`modules/display.rs`, `sys/display.rs`), профили и сочетания клавиш (`config.rs`, `modules/display.rs`),
коды клавиш (`sys/mod.rs`).

## Живая проверка

- `target\debug\antresol.exe` — в debug видна консоль с журналом (по-английски).
- `ANTRESOL_TRACE=1` — печатать каждую смену места или размера окна (доля выезда, место, размер, тема).
- `ANTRESOL_DEMO=1` — выдуманные имена монитора и звуковых устройств: снимки для открытого репозитория
  (`docs/img`) — только так, на демо-настройках (папка `antresol-scratch\demo`).
- `ANTRESOL_VIEW=settings` — открыть сразу настройки (для снимков без щелчков).
- Своя папка настроек для проверки — `APPDATA=E:\dev_personal\antresol-scratch\appdata` (задавать через
  PowerShell `$env:APPDATA`, из Git Bash путь не доходит); пока запущена панель владельца, собирать копию в
  `CARGO_TARGET_DIR=E:\dev_personal\antresol-scratch\target-test`.
- Снимки — захватом экрана после запуска (панель видна 2 с). Курсор не двигать: за ПК может
  работать владелец. В `docs/img` класть только вырезку самой панели.
- Не нажимать в проверках смену режима экрана, масштаба, устройства звука и автозапуск без команды
  владельца — это его настройки.
