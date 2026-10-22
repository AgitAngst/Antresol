# display-profiles Specification

## Purpose
TBD - created by archiving change display-profiles. Update Purpose after archive.

## Requirements

### Requirement: Профиль экрана сохраняется из текущего состояния
The panel SHALL save a named display profile holding, for every active monitor, its resolution, refresh rate and scale, taken from the current state.

#### Scenario: Сохранить текущее
- **WHEN** пользователь в карточке «Экран» нажимает «＋», вводит имя и подтверждает
- **THEN** появляется профиль с режимом и масштабом каждого активного монитора, он виден чипом и записан в `config.txt`

### Requirement: Профиль ставится с подтверждением
The panel SHALL apply a profile to all its monitors at once and MUST ask «Оставить этот режим?», reverting every changed mode and scale after 15 seconds without an answer.

#### Scenario: Щелчок по профилю
- **WHEN** пользователь щёлкает чип профиля
- **THEN** режимы и масштаб мониторов становятся как в профиле и показывается вопрос с обратным отсчётом

#### Scenario: Нет ответа
- **WHEN** после применения профиля 15 с никто не нажал «Оставить»
- **THEN** все изменённые режимы и масштабы возвращаются к прежним

#### Scenario: Монитор отключён
- **WHEN** в профиле есть монитор, которого сейчас нет
- **THEN** он пропускается, остальные ставятся, пользователь видит уведомление

### Requirement: Горячая клавиша профиля
The panel SHALL let the user assign an optional global hotkey to a profile; pressing it MUST apply the profile even when the panel is hidden and slide the panel out with the confirmation.

#### Scenario: Назначить клавишу
- **WHEN** пользователь в настройках нажимает «Изменить» у профиля и нажимает сочетание с Ctrl или Alt (Shift — по желанию) и буквой, цифрой, F1–F24 или пробелом
- **THEN** сочетание сохраняется и сразу работает; занятое другой программой помечено «занято»

#### Scenario: Нажать при спрятанной панели
- **WHEN** панель спрятана и пользователь нажимает клавишу профиля
- **THEN** профиль ставится, панель выезжает с вопросом «Оставить этот режим?»
