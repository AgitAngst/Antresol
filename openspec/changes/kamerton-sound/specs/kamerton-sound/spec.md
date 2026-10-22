## ADDED Requirements

### Requirement: Звук через Kamerton
The panel SHALL show and change the default output device and per-app volume and mute through Kamerton's pipe, and MUST NOT touch audio itself.

#### Scenario: Громкость с панели
- **WHEN** Kamerton запущен и пользователь двигает ползунок устройства
- **THEN** панель шлёт `volume N`, громкость меняет Kamerton

#### Scenario: Kamerton не запущен
- **WHEN** канала нет
- **THEN** карточка пишет «Kamerton не запущен» и предлагает запустить его (путь из настроек) или где его взять
