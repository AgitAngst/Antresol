## ADDED Requirements

### Requirement: Ночной свет
The quick settings card SHALL toggle Windows night light and MUST leave the setting untouched when its stored format is unknown.

#### Scenario: Включить
- **WHEN** ночной свет выключен и пользователь щёлкает плитку
- **THEN** ночной свет включается, плитка — «Вкл»

#### Scenario: Незнакомый формат
- **WHEN** значение «Параметров» не похоже на знакомое
- **THEN** плитка «Недоступно» и не нажимается

### Requirement: Основной монитор
The display card SHALL make the shown monitor primary keeping the arrangement, with the usual 15-second revert.

#### Scenario: Сделать основным
- **WHEN** мониторов два и пользователь жмёт «Сделать основным» у второго
- **THEN** второй становится основным, появляется «Оставить этот режим?»; без ответа через 15 с основной прежний
