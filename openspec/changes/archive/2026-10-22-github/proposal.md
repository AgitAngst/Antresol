## Why

Кандидат «Состояние GitHub» из SPEC.md (Command Palette Dock): CI и пулл-реквесты своих репозиториев
одним взглядом — только чтение, без запуска CI.

## What Changes

- Карточка «GitHub»: репозитории `владелец/имя` — точка по последнему прогону CI (прошёл, упал, идёт),
  «CI · прошёл · 12 мин назад», «PR 2»; щелчок по имени — Actions в браузере; добавить и убрать.
- REST API через WinHTTP, без своих библиотек; не чаще раза в 5 минут, пока панель открыта. Свой токен
  (`github_token=` в `config.txt`, вписывает владелец) — для закрытых репозиториев.
- Версия 1.1.0.

## Capabilities

### New Capabilities
- `github`: состояние CI и пулл-реквестов.

### Modified Capabilities
- нет.

## Impact

`src/sys/gh.rs`, `src/modules/github.rs` (новые), `src/config.rs` (`github`, `github_token`), `src/texts.rs`,
`Cargo.toml` (WinHttp крейта `windows`), docs.
