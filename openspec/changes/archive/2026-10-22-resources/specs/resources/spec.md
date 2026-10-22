## ADDED Requirements

### Requirement: Нагрузка и место
The panel SHALL show CPU load with a one-minute graph, GPU 3D load, memory used of total, network receive and send rates, battery charge if present, and free space on fixed drives, refreshed while the panel is open.

#### Scenario: Панель открыта
- **WHEN** панель выехала
- **THEN** карточка «Ресурсы» показывает текущие значения и обновляет их раз в 1,5 с

#### Scenario: Нет батареи
- **WHEN** у ПК нет батареи
- **THEN** строки «Батарея» нет
