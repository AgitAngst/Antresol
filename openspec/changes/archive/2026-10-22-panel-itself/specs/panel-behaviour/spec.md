## ADDED Requirements

### Requirement: Полоска у края
The panel SHALL show a thin strip at the top edge while hidden (unless turned off, a full-screen app or an excluded app is in front) and MUST tint it with the accent while the edge dwell is counting.

#### Scenario: Панель спрятана
- **WHEN** панель уехала за край и полоска включена
- **THEN** у края посередине видна полоска 120 × 5

### Requirement: Исключённые программы
The panel MUST NOT slide out from the edge while a program from the exclusion list is in the foreground.

#### Scenario: Игра в списке
- **WHEN** на переднем плане `game.exe` из списка и мышь у края
- **THEN** панель не выезжает; горячая клавиша по-прежнему показывает её

### Requirement: Все мониторы
With the monitor set to «Все», the panel SHALL slide out at the monitor whose top edge the mouse touched.

#### Scenario: Второй монитор
- **WHEN** мышь упирается в верхний край второго монитора
- **THEN** панель выезжает у этого монитора

### Requirement: Своя клавиша показа панели
The user SHALL be able to capture a custom show/hide shortcut, applied at once; a profile's shortcut MUST be refused.

#### Scenario: Назначить
- **WHEN** пользователь жмёт «Изменить…» и сочетание с Ctrl или Alt
- **THEN** оно сразу показывает и прячет панель; «Как по умолчанию» возвращает первое свободное
