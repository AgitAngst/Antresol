## ADDED Requirements

### Requirement: Поиск действий
The panel SHALL let the user find any action of its shelves and cards by typing and run it with Enter or a click; the
action MUST do exactly what the same click in its card does. Typing while the panel is open and no input has focus SHALL
open the search with that character.

#### Scenario: Ночной свет с клавиатуры
- **WHEN** панель открыта, пользователь печатает «ночн» и жмёт Enter
- **THEN** ночной свет включается, вид возвращается к карточкам

#### Scenario: Режим экрана
- **WHEN** пользователь ищет «1080» и выбирает «Экран: 1920 × 1080 · 240 Гц»
- **THEN** режим ставится с «Оставить этот режим?», как щелчком в карточке «Экран»

### Requirement: Калькулятор и таймер из запроса
The search SHALL compute an arithmetic query and copy the result on Enter, and SHALL start a timer from a query like
«чай 5».

#### Scenario: Посчитать
- **WHEN** запрос «2*3,5»
- **THEN** первая строка «2*3,5 = 7», Enter кладёт «7» в буфер обмена

#### Scenario: Таймер
- **WHEN** запрос «чай 5» и Enter
- **THEN** идёт таймер «чай» на 5 минут

### Requirement: Недоступное действие
An action that cannot run now SHALL be listed greyed with the reason and MUST NOT run on Enter.

#### Scenario: VPN без подключения
- **WHEN** VPN-подключений в Windows нет
- **THEN** действий VPN в списке нет; плитка «Wi‑Fi» без радио — серая строка с причиной
