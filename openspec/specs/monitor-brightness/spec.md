# monitor-brightness Specification

## Purpose
TBD - created by archiving change monitor-brightness. Update Purpose after archive.

## Requirements

### Requirement: Яркость и контраст внешнего монитора
The panel SHALL show, for every external monitor answering over DDC/CI, brightness and (if supported) contrast sliders in percent and MUST send the chosen value to the monitor without blocking the window.

#### Scenario: Двигать ползунок
- **WHEN** пользователь двигает ползунок яркости или крутит колесо мыши над ним
- **THEN** монитор получает новое значение, ползунок показывает выбранное, пока монитор не подтвердит

#### Scenario: Монитор не отвечает
- **WHEN** ни один монитор не отвечает по DDC/CI
- **THEN** карточка поясняет, что встроенный экран так не управляется, а у внешнего DDC/CI включают в его меню

### Requirement: Вход монитора
The panel SHALL list the inputs a monitor reports in its capabilities string and MUST ask for confirmation before switching, because the screen may go dark.

#### Scenario: Переключить вход
- **WHEN** пользователь выбирает другой вход и подтверждает «Переключить»
- **THEN** монитор переключается на этот вход; «Отмена» ничего не меняет
