## ADDED Requirements

### Requirement: Сцена — экран и система одним щелчком
A scene SHALL apply, by a click on its chip or its hotkey, the display part (mode, scale, HDR of every monitor) and the
system settings it has checked — monitor brightness, night light, Quiet, Keep awake, dark mode, Wi‑Fi, power plan —
and MUST leave unchecked settings untouched. Profiles saved before this change SHALL work as scenes that change the
display only.

#### Scenario: Сцена «Кино»
- **WHEN** у «Кино» отмечены экран 3840 × 2160 @ 60 с HDR, «Тишина» и «Не спать», и пользователь жмёт «Кино»
- **THEN** ставится режим с «Оставить этот режим?», включаются HDR, «Тишина» и «Не спать», остальное не меняется

#### Scenario: Старый профиль
- **WHEN** в `config.txt` профиль без системной части
- **THEN** чип «Сцена» ставит только режим, масштаб и HDR, как раньше

### Requirement: Повторный щелчок — вернуть как было
When a scene's undo switch is on, a click on the chip of the active scene or its hotkey SHALL restore every setting the
scene changed to its value before the scene; with the switch off nothing SHALL happen.

#### Scenario: Вернуть
- **WHEN** «Кино» стоит и пользователь снова жмёт «Кино»
- **THEN** режим, HDR, «Тишина» и «Не спать» — как до «Кино», уведомление «Вернул как было»

#### Scenario: Возврат выключен
- **WHEN** у стоящей сцены выключатель «Повторный щелчок — вернуть как было» выключен и пользователь жмёт её
- **THEN** ничего не меняется, уведомление «Эта сцена уже стоит»

#### Scenario: Нечего возвращать
- **WHEN** сцена стоит, но её не ставили (программу перезапустили или после неё ставили другую) и пользователь жмёт её
- **THEN** настройки не меняются, уведомление «Возвращать нечего»

### Requirement: Правка сцены
The panel SHALL offer a scene editor in place of the cards where the user checks what the scene changes, takes current
values, names it, sets a hotkey, switches undo and deletes the scene after an in-place confirmation.

#### Scenario: Новая сцена
- **WHEN** пользователь жмёт «+» в шапке, отмечает «Тишина», вводит имя «Доклад» и жмёт «Готово»
- **THEN** в шапке чип «Доклад»; сцена меняет экран (как сейчас) и «Тишину»

#### Scenario: Удалить
- **WHEN** пользователь жмёт «Удалить сцену» и подтверждает «Удалить»
- **THEN** сцены нет ни в шапке, ни в настройках, её клавиша освобождена; вопрос и обе кнопки видны целиком на любой ширине
  панели

#### Scenario: Удалить из настроек
- **WHEN** пользователь жмёт корзину у сцены в настройках
- **THEN** вместо строки сцены тот же вопрос «Удалить сцену «…»?»; «Отмена» и Esc оставляют сцену, «Удалить» убирает её

#### Scenario: Новая сцена без имени
- **WHEN** пользователь жмёт «+», стирает имя и уходит из правки
- **THEN** новой сцены нет

#### Scenario: Имя занято
- **WHEN** пользователь вводит имя, которое уже у другой сцены
- **THEN** поле в красной рамке, под ним со значком «Такая сцена уже есть», «Готово» недоступна, сцене имя не попадает

#### Scenario: Недоступная строка
- **WHEN** строку нельзя отметить (монитор не отвечает по DDC/CI, нет радио Wi‑Fi, значение не прочиталось)
- **THEN** строка серая, не нажимается, в подсказке причина

### Requirement: Файл сцены
`config.txt` SHALL keep a scene in one `profile=` line — name, hotkey, display, system part and `undo=0`. Lines written
before this change SHALL read as before, unknown keys and bad values SHALL be skipped without losing the scene, and a scene
with nothing to set SHALL NOT be saved.

#### Scenario: Сцена без экрана
- **WHEN** в `config.txt` строка `profile=Тишина | Ctrl+Alt+2 |  | quiet=1; awake=1`
- **THEN** сцена читается и меняет только «Тишину» и «Не спать»

#### Scenario: Кривое значение
- **WHEN** в системной части `bright=300; quiet=1`
- **THEN** яркость пропущена, «Тишина» отмечена, сцена остаётся

### Requirement: Экран сцены ставится с подтверждением
The panel SHALL ask «Оставить этот режим?» after the display part of a scene is applied and MUST revert every changed
mode, scale and HDR after 15 seconds without an answer; a monitor of the scene that is not connected SHALL be skipped with
a notice and the rest applied.

#### Scenario: Нет ответа
- **WHEN** после постановки сцены 15 с никто не нажал «Оставить»
- **THEN** все изменённые режимы, масштаб и HDR возвращаются

#### Scenario: Монитор отключён
- **WHEN** в сцене есть монитор, которого сейчас нет
- **THEN** он пропускается, остальное ставится, пользователь видит уведомление

### Requirement: Отказ от режима — отказ от сцены
Declining the display mode a scene set — «Вернуть», 15 seconds without an answer or closing the program — SHALL decline the
whole scene: the panel MUST restore the system settings the scene changed as well, with the undo switch on or off, and say
«Вернул как было».

#### Scenario: Вернуть
- **WHEN** «Кино» поставлена («Тишина», «Не спать», HDR) и пользователь жмёт «Вернуть» в «Оставить этот режим?»
- **THEN** вернулись режим и HDR, «Тишина» и «Не спать»; уведомление «Вернул как было»

#### Scenario: Нет ответа у сцены без возврата
- **WHEN** у сцены выключен «Повторный щелчок — вернуть как было» и 15 с никто не ответил
- **THEN** сцена возвращается целиком

### Requirement: Возврат не начинается при чужом вопросе
While an «Оставить этот режим?» that the scene did not ask is waiting, a click on the active scene SHALL change nothing and
ask to answer first: a scene MUST NOT be restored by halves.

#### Scenario: Чужой вопрос
- **WHEN** сцена стоит, в карточке «Экран» поменяли режим и не ответили, и пользователь жмёт стоящую сцену
- **THEN** ничего не возвращается, уведомление «Сначала ответьте на «Оставить этот режим?»»

### Requirement: Сцену узнают по номеру
What to restore SHALL belong to the scene, not to its name: renaming a scene MUST keep its undo, and a new scene with the
name of a deleted one MUST NOT inherit its undo.

#### Scenario: Переименовали
- **WHEN** сцену поставили, переименовали и жмут её снова
- **THEN** всё возвращается как было

#### Scenario: Новая сцена со старым именем
- **WHEN** сцену поставили, удалили, завели новую с тем же именем и жмут её
- **THEN** чужой возврат не выполняется, уведомление «Возвращать нечего»

### Requirement: Тема Windows — обе величины
The dark mode of a scene SHALL set both Windows themes, of apps and of the taskbar, and the undo SHALL restore each as it
was, including a mixed state (light apps, dark taskbar).

#### Scenario: Смешанная тема
- **WHEN** программы светлые, панель задач тёмная, сцена ставит «Тёмная тема» и её возвращают
- **THEN** программы снова светлые, панель задач осталась тёмной

### Requirement: Яркость ждёт монитор
When a scene sets brightness right after a mode or HDR change, the panel SHALL ask the monitor again for up to three
seconds, and if it still does not answer MUST warn «Не удалось поставить яркость: монитор не ответил по DDC/CI»; a monitor
that never answered over DDC/CI SHALL be skipped silently. The undo SHALL retry the same way.

#### Scenario: Монитор молчит секунду
- **WHEN** сразу после смены режима монитор не отвечает по DDC/CI, а через секунду отвечает
- **THEN** яркость поставлена, предупреждения нет

#### Scenario: Монитор молчит
- **WHEN** монитор, что отвечал раньше, не отвечает три секунды
- **THEN** предупреждение, остальная сцена стоит

### Requirement: Ошибки словами программы
A failure of a scene part SHALL be told in the program's words («Не получилось: Тёмная тема»), keeping the English service
text for the log; what fails in a module's worker thread (Wi‑Fi, DDC/CI) SHALL be told even when the card is not shown.

#### Scenario: Wi‑Fi не переключился
- **WHEN** сцена выключает Wi‑Fi, Windows не даёт доступ к радио, а карточка «Быстрые настройки» на закрытой полке
- **THEN** уведомление «Не получилось: Wi‑Fi», без английского текста ошибки

### Requirement: Горячая клавиша сцены
The panel SHALL let the user assign an optional global hotkey to a scene; pressing it MUST set or restore the scene even when
the panel is hidden and slide the panel out with the question and the notice.

#### Scenario: Назначить клавишу
- **WHEN** пользователь в настройках или в правке сцены нажимает «Изменить» и сочетание с Ctrl или Alt (Shift — по желанию) и
  буквой, цифрой, F1–F24 или пробелом
- **THEN** сочетание сохраняется и сразу работает; занятое другой программой помечено «Занято другой программой»

#### Scenario: Нажать при спрятанной панели
- **WHEN** панель спрятана и пользователь нажимает клавишу сцены
- **THEN** сцена ставится (или стоящая возвращается), панель выезжает с вопросом «Оставить этот режим?» и уведомлением
