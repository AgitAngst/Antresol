## ADDED Requirements

### Requirement: Высота по полке
The panel SHALL size its height on each shelf to what the tallest card of that shelf needs, between 150 and 440 points,
and SHALL animate the change when another shelf is opened; a height the user drags MUST be kept for that shelf only, and a
double click on the bottom edge MUST return the shelf to fitted height.

#### Scenario: Звук не обрезан
- **WHEN** на полке «Экран и звук» в «Звуке» четыре программы, а панель была 340
- **THEN** панель вырастает так, что видны все программы и «Микшер Kamerton», без прокрутки в карточке

#### Scenario: Полупустая полка
- **WHEN** открыта полка, где карточкам хватает 280
- **THEN** панель становится ниже и не держит пустого места

#### Scenario: Своя высота
- **WHEN** пользователь потянул нижний край на полке «Под рукой» до 500
- **THEN** «Под рукой» открывается на 500, другие полки — по полке; двойной щелчок по нижнему краю возвращает «по полке»

#### Scenario: Настройки
- **WHEN** открыты настройки, поиск или правка сцены
- **THEN** высота панели не меняется

### Requirement: Колесо без Shift
Over a row of cards that does not fit, the mouse wheel SHALL scroll the row sideways whenever the card under the pointer
has nothing to scroll in that direction; over the shelf tabs the wheel SHALL switch to the neighbouring shelf.

#### Scenario: Карточка без прокрутки
- **WHEN** ряд не помещается и мышь над «Медиа»
- **THEN** колесо листает ряд вбок

#### Scenario: Карточка со списком
- **WHEN** мышь над буфером обмена, где список длиннее карточки
- **THEN** колесо листает список; дошёл до конца — дальше листается ряд

#### Scenario: Вкладки
- **WHEN** мышь над вкладками и колесо крутят вниз
- **THEN** открывается следующая полка; на последней — ничего
