## Why

Владелец 22.10.2026: всё из мозгового штурма, кроме окон. Раздел «Буфер и снимки»: заготовки текста, пипетка,
снимок области с распознаванием (история буфера уже есть с 0.8).

## What Changes

- Карточка «Заготовки»: свои тексты, щелчок — в буфер; добавить из буфера; `snippet=` в config.txt.
- Карточка «Инструменты»: пипетка (тянуть круг на цвет экрана, код в буфер, история) и текст с экрана
  («Ножницы» Windows, распознавание Windows.Media.Ocr, текст в буфер).
- Версия 1.6.0.

## Capabilities

### New Capabilities
- `clip-tools`: заготовки, пипетка, текст с экрана.

### Modified Capabilities
- нет.

## Impact

`src/sys/ocr.rs`, `src/modules/{snippets,tools}.rs` (новые), `src/sys/monitor.rs` (`pixel_at`), `src/config.rs`,
`Cargo.toml` (фичи WinRT OCR), тексты, docs, `scripts/shots.ps1`.
