## ADDED Requirements

### Requirement: HDR одним щелчком
When the panel's monitor supports HDR, the «Экран» card SHALL show an «HDR» toggle with its state and MUST switch HDR
for that monitor only; a monitor without HDR SHALL show no toggle, and a monitor where Windows forbids HDR right now MUST
show the toggle disabled with the reason. A change made outside the panel SHALL be shown at the next refresh.

#### Scenario: Включить HDR
- **WHEN** монитор поддерживает HDR и пользователь жмёт «HDR Выкл»
- **THEN** HDR этого монитора включён, кнопка — «HDR Вкл» с подложкой акцента, другие мониторы не тронуты

#### Scenario: Монитор без HDR
- **WHEN** монитор панели HDR не поддерживает
- **THEN** кнопки «HDR» в плашке нет

#### Scenario: Windows не даёт включить
- **WHEN** монитор поддерживает HDR, но Windows сейчас не даёт его включить
- **THEN** кнопка «HDR» недоступна, в подсказке причина

#### Scenario: HDR включили снаружи
- **WHEN** HDR монитора включили Win+Alt+B или в «Параметрах», а панель открыта
- **THEN** при ближайшем чтении (раз в 1,5 с) кнопка показывает «HDR Вкл»

### Requirement: HDR в профилях экрана
A saved display profile SHALL remember HDR of every monitor that supports it and applying the profile MUST set it;
profiles saved before this change MUST leave HDR untouched.

#### Scenario: Профиль «Кино»
- **WHEN** «Кино» сохранён с включённым HDR, а сейчас HDR выключен
- **THEN** щелчок по «Кино» ставит режим и включает HDR; «Вернуть» в «Оставить этот режим?» возвращает и HDR

#### Scenario: Профиль до 2.0
- **WHEN** профиль сохранён до этого изменения (в строке `profile=` нет слов `hdr` / `sdr`)
- **THEN** его применение и откат HDR не трогают

### Requirement: HDR в демо и тестах
In demo mode and tests the HDR toggle SHALL work on the invented monitor only and MUST NOT write to Windows.

#### Scenario: Снимок в демо
- **WHEN** панель запущена с `ANTRESOL_DEMO=1`
- **THEN** у выдуманного монитора есть «HDR», щелчок меняет только его состояние
