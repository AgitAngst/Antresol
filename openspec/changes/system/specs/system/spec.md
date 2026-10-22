## ADDED Requirements

### Requirement: Системные переключатели
The quick settings card SHALL toggle keep-awake, Windows dark mode, Windows notifications and the Wi‑Fi radio, switch the power plan and connect or disconnect Windows VPN connections.

#### Scenario: Не спать
- **WHEN** пользователь включает «Не спать»
- **THEN** компьютер не засыпает и экран не гаснет, пока работает Antresol; выход снимает запрет

#### Scenario: VPN
- **WHEN** в Windows есть VPN-подключение с сохранёнными данными и пользователь жмёт «Подключить»
- **THEN** оно подключается, точка становится зелёной; не удалось — уведомление с кодом
