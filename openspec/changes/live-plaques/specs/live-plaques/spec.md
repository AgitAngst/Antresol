## ADDED Requirements

### Requirement: Плашка события у края
While the panel is hidden, an event SHALL be shown as a plaque at the edge without sliding the panel out and without
taking the focus; an ordinary plaque MUST disappear after 4 s unless the pointer is over it or its action is still running,
an important one SHALL stay until closed, and plaques SHALL come one at a time in order. A plaque's buttons, cross and body
MUST accept a click only 400 ms after the plaque has stood in place.

#### Scenario: Наушники подключились
- **WHEN** панель спрятана и подключаются Bluetooth-наушники
- **THEN** у края плашка «Наушники: подключено» с зарядом, через 4 с уходит, панель не выезжает

#### Scenario: Мышь проснулась
- **WHEN** Bluetooth-мышь или клавиатура снова подключается после сна
- **THEN** плашки «подключено» нет; плашка «Мышь: разряжается» бывает, когда заряд упал до 15 % и ниже

#### Scenario: Сборка упала
- **WHEN** последний прогон GitHub сменился на «упал»
- **THEN** плашка висит, пока её не закроют; «Открыть» открывает прогон в браузере

#### Scenario: Щелчок сразу после появления
- **WHEN** плашка только что выехала и по её кнопке щёлкнули раньше чем через 400 мс
- **THEN** щелчок ничего не делает: кнопка не светится, «руки» нет; позже кнопка работает

#### Scenario: Панель выдвинулась
- **WHEN** выезжает панель, пока плашка висит или ждёт
- **THEN** плашки уходят уведомлениями; та, что простояла на виду дольше секунды, — без пересказа; важные остаются, а если
  панель вызвана клавишей, пересказываются и уходят тоже

### Requirement: Флешка и загрузки
The panel SHALL announce a plugged removable drive with «Открыть» and a safe «Извлечь», and a finished download in the
Downloads folder with «Открыть» and «В лоток»; a file that runs when opened MUST get «Показать в папке» instead of «Открыть»,
several files finished at once MUST be one plaque, and temporary, empty and still-being-written files MUST be ignored.

#### Scenario: Извлечь
- **WHEN** пользователь жмёт «Извлечь» на плашке флешки
- **THEN** диск безопасно извлечён, плашка «Можно вынимать»; занят — «Не извлечь: диск занят»; пока ждём, «Извлекаю…» и время
  плашки стоит

#### Scenario: Флешку уже вынули
- **WHEN** «Извлечь» нажали, а такого тома уже нет или на его букве другой диск
- **THEN** ничего не извлекается, плашка «Флешку уже вынули»

#### Scenario: Карта во встроенном картридере
- **WHEN** вставлена карта на шине SD или MMC
- **THEN** плашка с «Открыть», без «Извлечь»

#### Scenario: Флешка с несколькими разделами
- **WHEN** вставлена флешка с несколькими томами
- **THEN** одна плашка; вынули её — плашка уходит (когда тома нет в двух чтениях подряд)

#### Scenario: Недокачанный файл
- **WHEN** в «Загрузках» появляется `файл.crdownload`, `файл.opdownload` или пустой `файл` с `файл.part` рядом, либо файл
  пишет другая программа
- **THEN** плашки нет, пока не появится готовый файл

#### Scenario: Скачана программа
- **WHEN** скачан файл, что при открытии запускается (`.exe`, `.msi`, `.bat`, `.lnk`, `.ps1`, образ диска…)
- **THEN** у плашки «Показать в папке» вместо «Открыть» (и без «В лоток»); сам файл плашка не запускает

#### Scenario: Распаковали архив
- **WHEN** за одно чтение «Загрузок» готово несколько файлов
- **THEN** одна плашка «Скачано файлов: 7» с «Показать в папке»

### Requirement: Выключатели плашек
The panel settings SHALL let the user turn each kind of plaque on or off with a chip; the timer plaque MUST always show,
and the config file MUST list what is switched off so that a kind added later is on by default.

#### Scenario: Без загрузок
- **WHEN** «Загрузки» выключены в «Живых плашках»
- **THEN** готовый файл не показывается у края, а в `config.txt` строка `plaques_off=downloads`

### Requirement: Плашки не мешают
Plaques SHALL wait while the user is presenting, in a full-screen game or in a do-not-disturb mode, except the timer plaque;
a shown plaque MUST stay on its monitor, and the pointer over a plaque MUST NOT count as the panel's edge.

#### Scenario: Показ экрана
- **WHEN** идёт показ экрана и скачивается файл
- **THEN** плашка ждёт; плашка таймера показывается

### Requirement: Запросы GitHub
The panel SHALL read the runs and the pull requests of a repository apart, each at most once in 5 minutes and the pull
requests only for the card, so that no more than 24 requests an hour go to one repository.

#### Scenario: Плашки без карточки
- **WHEN** плашки GitHub включены, а панель не открывали
- **THEN** читаются только прогоны, раз в 5 минут
