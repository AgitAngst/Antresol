# timers Specification

## Purpose
TBD - created by archiving change timers. Update Purpose after archive.

## Requirements

### Requirement: Таймеры
The panel SHALL run countdown timers that keep going while the panel is hidden, and MUST slide the panel out with a notification when one is up.

#### Scenario: Таймер дошёл
- **WHEN** пользователь запустил «5 мин», спрятал панель и прошло 5 минут
- **THEN** панель выезжает с уведомлением «Таймер дошёл», таймер из списка уходит

### Requirement: Помидор
A pomodoro SHALL alternate 25 minutes of work and 5 minutes of break until removed.

#### Scenario: Работа закончилась
- **WHEN** 25 минут «Работы» прошли
- **THEN** уведомление «Работа закончилась — отдых 5 минут» и сам запускается «Отдых»
