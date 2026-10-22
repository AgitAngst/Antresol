## ADDED Requirements

### Requirement: Температура
The resources card SHALL show the hottest ACPI thermal zone in °C when Windows exposes thermal zones, and nothing otherwise.

#### Scenario: Зоны есть
- **WHEN** Windows отдаёт тепловые зоны
- **THEN** строка «Температура» с самой горячей зоной в °C

### Requirement: Часы поясов
The clocks card SHALL show the current time in each chosen Windows time zone with its UTC offset and difference from this PC.

#### Scenario: Добавить Токио
- **WHEN** пользователь ищет «Tokyo» и щёлкает «Tokyo Standard Time»
- **THEN** появляются часы «Tokyo» с временем UTC+9
