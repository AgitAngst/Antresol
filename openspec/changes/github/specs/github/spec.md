## ADDED Requirements

### Requirement: Состояние репозиториев
The panel SHALL show, for each configured GitHub repository, the latest CI run state and the number of open pull requests, read-only, at most every 5 minutes while open; it MUST NOT start CI runs.

#### Scenario: CI упал
- **WHEN** последний прогон репозитория закончился с ошибкой
- **THEN** красная точка и «CI · упал · … назад»

#### Scenario: Закрытый без токена
- **WHEN** репозиторий закрытый, а токена нет
- **THEN** «Не найден (или закрытый — нужен токен)»
