# media Specification

## Purpose
TBD - created by archiving change media. Update Purpose after archive.

## Requirements

### Requirement: Что играет
The panel SHALL show the current system media session — cover, title, artist and source app — and MUST read it without blocking the window.

#### Scenario: Играет музыка
- **WHEN** в Spotify, браузере или плеере что-то играет
- **THEN** карточка «Медиа» показывает обложку, название, исполнителя и программу

#### Scenario: Ничего не играет
- **WHEN** медиа-сеанса нет
- **THEN** карточка пишет «Ничего не играет.»

### Requirement: Управление плеером
The panel SHALL offer Previous, Play/Pause and Next buttons that act on the current session.

#### Scenario: Пауза
- **WHEN** трек играет и пользователь нажимает «Пауза»
- **THEN** сеанс встаёт на паузу, кнопка становится «Играть»
