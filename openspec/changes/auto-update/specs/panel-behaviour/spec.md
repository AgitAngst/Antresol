## ADDED Requirements

### Requirement: Обновления изнутри программы
The panel SHALL check GitHub Releases for a newer version at start and once a day when «Проверять обновления» is on, SHALL
announce it with a banner above the cards offering to update, and SHALL install it only after the SHA-256 check passes,
then restart on request.

#### Scenario: Вышла новая версия
- **WHEN** на GitHub выпуск новее, а проверка включена
- **THEN** над карточками баннер «Вышла vX.Y.Z» с «Обновить», «Позже», «Пропустить», «Что нового»

#### Scenario: Перезапуск
- **WHEN** обновление скачано и пользователь нажал «Перезапустить»
- **THEN** старая панель закрывается, новая ждёт её выхода и берёт свои сочетания клавиш

#### Scenario: Проверка выключена
- **WHEN** «Проверять обновления» выключено
- **THEN** сама панель в сеть за обновлениями не ходит; «Проверить» в настройках проверяет по просьбе
