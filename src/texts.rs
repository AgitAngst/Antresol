// Тексты интерфейса на двух языках. Язык — общий с набором Anvil (`anvil_ui::Lang`).

use anvil_ui::Lang;

use crate::config::SceneRow;

pub struct Tr {
    pub display: &'static str,
    pub recommended: &'static str,
    pub favorites: &'static str,
    pub favorites_hint: &'static str,
    pub favorites_full: &'static str,
    pub star_add: &'static str,
    pub star_remove: &'static str,
    pub cancel: &'static str,
    pub change: &'static str,
    pub press_keys: &'static str,
    pub key_busy: &'static str,
    pub key_free: &'static str,
    pub no_key: &'static str,
    pub remove_key: &'static str,
    pub key_is_panel: &'static str,
    pub brightness: &'static str,
    pub brightness_short: &'static str,
    pub contrast: &'static str,
    pub ddc_reading: &'static str,
    pub ddc_none: &'static str,
    pub ddc_failed: &'static str,
    pub input_title: &'static str,
    pub input_switch: &'static str,
    pub media: &'static str,
    pub media_none: &'static str,
    pub media_untitled: &'static str,
    pub media_previous: &'static str,
    pub media_next: &'static str,
    pub media_play: &'static str,
    pub media_pause: &'static str,
    pub media_failed: &'static str,
    pub all_monitors: &'static str,
    pub all_monitors_note: &'static str,
    pub strip: &'static str,
    pub exclude_title: &'static str,
    pub exclude_none: &'static str,
    pub exclude_remove: &'static str,
    pub key_auto: &'static str,
    pub resources: &'static str,
    pub cpu: &'static str,
    pub gpu: &'static str,
    pub memory: &'static str,
    pub network: &'static str,
    pub battery: &'static str,
    pub plugged: &'static str,
    pub clipboard: &'static str,
    pub clipboard_off: &'static str,
    pub clipboard_empty: &'static str,
    pub clipboard_put: &'static str,
    pub clipboard_done: &'static str,
    pub clipboard_clear: &'static str,
    pub clipboard_remember: &'static str,
    pub tray: &'static str,
    pub tray_empty: &'static str,
    pub tray_missing: &'static str,
    pub tray_reveal: &'static str,
    pub tray_remove: &'static str,
    pub bluetooth: &'static str,
    pub bt_none: &'static str,
    pub bt_settings: &'static str,
    pub timers: &'static str,
    pub timer: &'static str,
    pub timer_work: &'static str,
    pub timer_rest: &'static str,
    pub timer_done: &'static str,
    pub timer_work_done: &'static str,
    pub timer_rest_done: &'static str,
    pub pomodoro: &'static str,
    pub pomodoro_hint: &'static str,
    pub timers_none: &'static str,
    pub timer_cancel: &'static str,
    pub timer_pause: &'static str,
    pub timer_resume: &'static str,
    pub github: &'static str,
    pub github_none: &'static str,
    pub github_no_runs: &'static str,
    pub github_add: &'static str,
    pub github_remove: &'static str,
    pub answer_first: &'static str,
    pub mode_unavailable: &'static str,
    pub keep_title: &'static str,
    pub keep: &'static str,
    pub revert: &'static str,
    pub scale_note: &'static str,
    pub buttons: &'static str,
    pub buttons_empty: &'static str,
    pub edit_buttons: &'static str,
    pub pin: &'static str,
    pub unpin: &'static str,
    pub settings: &'static str,
    pub back: &'static str,
    pub quit: &'static str,
    pub panel: &'static str,
    pub monitor: &'static str,
    pub autostart: &'static str,
    pub fullscreen_guard: &'static str,
    pub show_delay: &'static str,
    pub hide_delay: &'static str,
    pub hotkey: &'static str,
    pub hotkey_busy: &'static str,
    pub resize_hint: &'static str,
    pub failed: &'static str,
    pub ms: &'static str,
    pub all_res: &'static str,
    pub refresh_hz: &'static str,
    pub scale_pct: &'static str,
    pub hide: &'static str,
    pub color: &'static str,
    pub custom_color: &'static str,
    pub hue: &'static str,
    pub sat_value: &'static str,
    pub color_code: &'static str,
    pub presets: &'static str,
    pub windows_color: &'static str,
    pub windows_color_none: &'static str,
    pub dark_theme: &'static str,
    pub light_theme: &'static str,
    pub contrast_text: &'static str,
    pub contrast_on: &'static str,
    pub adjusted: &'static str,
    pub as_picked: &'static str,
    pub color_note: &'static str,
    pub apply: &'static str,
    pub sample: &'static str,
    pub done: &'static str,
    pub media_strip: &'static str,
    pub open_panel: &'static str,
    pub appearance: &'static str,
    pub more_hint: &'static str,
    pub sound: &'static str,
    pub sound_apps: &'static str,
    pub kamerton_off: &'static str,
    pub kamerton_start: &'static str,
    pub kamerton_get: &'static str,
    pub kamerton_mixer: &'static str,
    pub kamerton_failed: &'static str,
    pub mute: &'static str,
    pub unmute: &'static str,
    pub no_output: &'static str,
    pub quick: &'static str,
    pub night_light: &'static str,
    pub on: &'static str,
    pub off: &'static str,
    pub unavailable: &'static str,
    pub primary: &'static str,
    pub make_primary: &'static str,
    pub keep_awake: &'static str,
    pub dark_theme_win: &'static str,
    pub quiet: &'static str,
    pub power_plan: &'static str,
    pub connect: &'static str,
    pub disconnect: &'static str,
    pub tools: &'static str,
    pub picker: &'static str,
    pub picker_hint: &'static str,
    pub color_copied: &'static str,
    pub screen_text: &'static str,
    pub screen_text_pick: &'static str,
    pub screen_text_wait: &'static str,
    pub screen_text_done: &'static str,
    pub screen_text_none: &'static str,
    pub screen_text_no_lang: &'static str,
    pub clocks: &'static str,
    pub clocks_empty: &'static str,
    pub clock_add: &'static str,
    pub clock_remove: &'static str,
    pub clock_search: &'static str,
    pub clock_unknown: &'static str,
    pub temperature: &'static str,
    // Полки, правка и закреплённое буфера (1.9.0): один блок.
    pub shelf_av: &'static str,
    pub shelf_sys: &'static str,
    pub shelf_hand: &'static str,
    pub shelf_add: &'static str,
    pub shelves_edit: &'static str,
    pub shelves_hint: &'static str,
    pub shelves_undo: &'static str,
    pub grip_hint: &'static str,
    pub removed_cards: &'static str,
    pub removed_none: &'static str,
    pub removed_note: &'static str,
    pub shelf_empty: &'static str,
    pub shelf_rename: &'static str,
    pub shelf_remove: &'static str,
    pub quiet_on: &'static str,
    pub clip_pin: &'static str,
    pub clip_pin_from: &'static str,
    pub clip_no_text: &'static str,
    pub clip_hint: &'static str,
    pub clip_pinned_full: &'static str,
    pub clip_unpinned: &'static str,
    pub shelf_limit: &'static str,
    pub paused: &'static str,
    // HDR монитора в карточке «Экран» (2.0.0): один блок.
    pub hdr: &'static str,
    pub hdr_hint: &'static str,
    pub hdr_blocked: &'static str,
    // Сцены вместо профилей экрана (2.1.0): один блок.
    pub scene: &'static str,
    pub scenes: &'static str,
    pub scene_new: &'static str,
    pub scene_what: &'static str,
    pub scene_untouched: &'static str,
    pub scene_take: &'static str,
    pub scene_name_key: &'static str,
    pub scene_name: &'static str,
    pub scene_name_empty: &'static str,
    pub scene_name_taken: &'static str,
    pub scene_key: &'static str,
    pub scene_exit: &'static str,
    pub scene_undo: &'static str,
    pub scene_undo_note: &'static str,
    pub scene_confirm_note: &'static str,
    pub scene_delete: &'static str,
    pub scene_delete_yes: &'static str,
    pub scene_undone: &'static str,
    pub scene_nothing_undo: &'static str,
    pub scene_already: &'static str,
    pub scene_edit: &'static str,
    pub scene_rows_none: &'static str,
    pub scene_plan_missing: &'static str,
    pub scene_bright_failed: &'static str,
    pub scene_why_wifi: &'static str,
    pub scene_why_read: &'static str,
    pub scenes_none: &'static str,
    // Поиск действий (2.2.0): один блок.
    pub search_field: &'static str,
    pub search_tip: &'static str,
    pub search_clear: &'static str,
    pub search_close: &'static str,
    pub search_keys: &'static str,
    pub search_found: &'static str,
    pub search_recent: &'static str,
    pub search_start: &'static str,
    pub search_try: &'static str,
    pub search_none: &'static str,
    pub search_calc: &'static str,
    pub search_pointer: &'static str,
    pub search_hide: &'static str,
    pub search_undo: &'static str,
    pub search_pick: &'static str,
    pub search_play: &'static str,
    pub search_pause: &'static str,
    pub search_next: &'static str,
    pub search_previous: &'static str,
    pub turn_on: &'static str,
    pub turn_off: &'static str,
    pub scale_word: &'static str,
    /// Таймер со своим именем дошёл; `{}` — имя.
    pub timer_named_done: &'static str,
    /// Примеры запросов справа в поиске: запрос и что он делает.
    pub tips: [(&'static str, &'static str); 6],
    pub wifi: &'static str,
    pub accents: [&'static str; 5],
}

const RU: Tr = Tr {
    display: "Экран",
    recommended: "Рекомендуется",
    favorites: "Избранное",
    favorites_hint: "Звёздочка у разрешения — до трёх любимых здесь, в один щелчок.",
    favorites_full: "Не больше трёх избранных — снимите звёздочку с другого",
    star_add: "В избранное",
    star_remove: "Убрать из избранного",
    cancel: "Отмена",
    change: "Изменить…",
    press_keys: "Нажмите сочетание: Ctrl или Alt и клавишу; Esc — отмена",
    key_busy: "Занято другой программой",
    key_free: "Работает",
    no_key: "Без клавиши",
    remove_key: "Убрать клавишу",
    key_is_panel: "Это сочетание показа панели — выберите другое",
    brightness: "Яркость",
    brightness_short: "Яркость",
    contrast: "Контраст",
    ddc_reading: "Опрашиваю мониторы…",
    ddc_none: "Мониторы не отвечают по DDC/CI. Встроенный экран ноутбука так не управляется; у внешнего DDC/CI включают в меню монитора.",
    ddc_failed: "Монитор не принял значение",
    input_title: "Переключить вход монитора?",
    input_switch: "Переключить",
    media: "Медиа",
    media_none: "Ничего не играет.",
    media_untitled: "Без названия",
    media_previous: "Назад",
    media_next: "Вперёд",
    media_play: "Играть",
    media_pause: "Пауза",
    media_failed: "Плеер не ответил",
    all_monitors: "Все",
    all_monitors_note: "Панель выезжает у того монитора, к краю которого подошла мышь.",
    strip: "Полоска у края, пока панель спрятана",
    exclude_title: "Не выезжать над программами",
    exclude_none: "Список пуст — над любой программой панель выезжает.",
    exclude_remove: "Убрать из списка",
    key_auto: "Как по умолчанию",
    resources: "Ресурсы",
    cpu: "Процессор",
    gpu: "Видеокарта",
    memory: "Память",
    network: "Сеть",
    battery: "Батарея",
    plugged: "от сети",
    clipboard: "Буфер обмена",
    clipboard_off: "Запоминание выключено в настройках.",
    clipboard_empty: "Скопируйте текст — он появится здесь.",
    clipboard_put: "Положить в буфер обмена",
    clipboard_done: "В буфере — вставляйте",
    clipboard_clear: "Очистить",
    clipboard_remember: "Запоминать скопированный текст (только в памяти)",
    tray: "Лоток",
    tray_empty: "Перетащите файлы на панель — они полежат здесь.",
    tray_missing: "нет файла",
    tray_reveal: "Показать в папке",
    tray_remove: "Убрать из лотка",
    bluetooth: "Bluetooth",
    bt_none: "Сопряжённых устройств нет.",
    bt_settings: "Параметры Bluetooth",
    timers: "Таймеры",
    timer: "Таймер",
    timer_work: "Работа",
    timer_rest: "Отдых",
    timer_done: "Таймер дошёл",
    timer_work_done: "Работа закончилась — отдых 5 минут",
    timer_rest_done: "Отдых закончился — снова работа",
    pomodoro: "Помидор",
    pomodoro_hint: "25 минут работы, 5 отдыха — по кругу",
    timers_none: "Таймеров нет — запустите кнопкой выше.",
    timer_cancel: "Убрать таймер",
    timer_pause: "Пауза",
    timer_resume: "Дальше",
    github: "GitHub",
    github_none: "Добавьте репозиторий: «владелец/имя».",
    github_no_runs: "Прогонов CI нет",
    github_add: "Добавить",
    github_remove: "Убрать репозиторий",
    answer_first: "Сначала ответьте на «Оставить этот режим?»",
    mode_unavailable: "Режима из сцены у монитора нет — он пропущен",
    keep_title: "Оставить этот режим?",
    keep: "Оставить",
    revert: "Вернуть",
    scale_note: "Некоторые программы подхватят масштаб только после перезапуска.",
    buttons: "Кнопки",
    buttons_empty: "Своих кнопок пока нет — добавьте строки в файл кнопок.",
    edit_buttons: "Править кнопки",
    pin: "Закрепить: не прятать",
    unpin: "Открепить",
    settings: "Настройки",
    back: "Назад",
    quit: "Закрыть Antresol",
    panel: "Панель",
    monitor: "Монитор",
    autostart: "Запускать вместе с Windows",
    fullscreen_guard: "Не выезжать поверх игр и видео во весь экран",
    show_delay: "Выезжать через",
    hide_delay: "Прятаться через",
    hotkey: "Показать или спрятать",
    hotkey_busy: "Все сочетания заняты другими программами",
    resize_hint: "Тяните нижний край или углы, чтобы изменить размер",
    failed: "Не получилось",
    ms: "мс",
    all_res: "Все разрешения",
    refresh_hz: "Частота, Гц",
    scale_pct: "Масштаб, %",
    hide: "Спрятать (Esc)",
    color: "Цвет",
    custom_color: "Свой цвет…",
    hue: "Оттенок",
    sat_value: "Насыщенность и яркость",
    color_code: "Код цвета",
    presets: "Готовые",
    windows_color: "Взять цвет Windows",
    windows_color_none: "Цвет Windows не прочитать",
    dark_theme: "Тёмная тема",
    light_theme: "Светлая тема",
    contrast_text: "Текст",
    contrast_on: "На кнопке",
    adjusted: "Подправлен для читаемости",
    as_picked: "Как выбран",
    color_note: "Цвет подстраивается под каждую тему, чтобы текст читался (не ниже 4,5:1).",
    apply: "Применить",
    sample: "Пример",
    done: "Готово",
    media_strip: "Медиа на спрятанной полоске",
    open_panel: "Открыть панель",
    appearance: "Оформление",
    more_hint: "Пролистать к остальным карточкам",
    sound: "Звук",
    sound_apps: "Программы",
    kamerton_off: "Kamerton не запущен. Громкость устройства и программ появится здесь, когда он работает.",
    kamerton_start: "Запустить Kamerton",
    kamerton_get: "Где взять Kamerton",
    kamerton_mixer: "Микшер Kamerton",
    kamerton_failed: "Kamerton не ответил",
    mute: "Без звука",
    unmute: "Включить звук",
    no_output: "Нет устройства вывода",
    quick: "Быстрые настройки",
    night_light: "Ночной свет",
    on: "Вкл",
    off: "Выкл",
    unavailable: "Недоступно",
    primary: "основной",
    make_primary: "Сделать основным",
    keep_awake: "Не спать",
    dark_theme_win: "Тёмная тема",
    quiet: "Тишина",
    power_plan: "План питания",
    connect: "Подключить",
    disconnect: "Отключить",
    tools: "Инструменты",
    picker: "Пипетка",
    picker_hint: "Тяните круг на любой цвет экрана — код цвета попадёт в буфер.",
    color_copied: "в буфере",
    screen_text: "Текст с экрана",
    screen_text_pick: "Выделить область",
    screen_text_wait: "Выделите область на экране…",
    screen_text_done: "Текст в буфере обмена.",
    screen_text_none: "Текст на снимке не найден.",
    screen_text_no_lang: "В Windows нет языка для распознавания текста — добавьте его в «Параметры → Время и язык».",
    clocks: "Часы",
    clocks_empty: "Добавьте пояс — будет видно, который там час.",
    clock_add: "Добавить пояс",
    clock_remove: "Убрать часы",
    clock_search: "Пояс, например Tokyo",
    clock_unknown: "Пояс не найден",
    temperature: "Температура",
    shelf_av: "Экран и звук",
    shelf_sys: "Система",
    shelf_hand: "Под рукой",
    shelf_add: "Полка",
    shelves_edit: "Изменить полки",
    shelves_hint: "Тяните карточку за ручку — в ряду или на вкладку другой полки",
    shelves_undo: "Как было",
    grip_hint: "Тянуть мышью или Alt + ←/→; Alt + 1…9 — на другую полку",
    removed_cards: "Убранные карточки",
    removed_none: "Пока ничего не убрано",
    removed_note: "Крестик на карточке убирает её сюда. Щелчок по ней — вернуть на эту полку.",
    shelf_empty: "На этой полке пусто",
    shelf_rename: "Двойной щелчок — переименовать",
    shelf_remove: "Убрать полку — её карточки уйдут в «Убранные»",
    quiet_on: "без всплывающих",
    clip_pin: "Закрепить",
    clip_pin_from: "Закрепить из буфера",
    clip_no_text: "В буфере нет текста",
    clip_hint: "Щелчок — скопировать снова. Булавка — закрепить.",
    clip_pinned_full: "Закреплено 30 — открепите лишнее",
    clip_unpinned: "Откреплено",
    shelf_limit: "Полок не больше девяти — уберите ненужную",
    paused: "на паузе",
    hdr: "HDR",
    hdr_hint: "HDR этого монитора. Win+Alt+B переключает все мониторы сразу",
    hdr_blocked: "Windows сейчас не даёт включить HDR на этом мониторе",
    scene: "Сцена",
    scenes: "Сцены",
    scene_new: "Новая сцена из текущих настроек",
    scene_what: "Что меняет сцена",
    scene_untouched: "не трогать",
    scene_take: "Запомнить текущие",
    scene_name_key: "Имя и клавиша",
    scene_name: "Имя",
    scene_name_empty: "Нужно имя",
    scene_name_taken: "Такая сцена уже есть",
    scene_key: "Клавиша",
    scene_exit: "Выход из сцены",
    scene_undo: "Повторный щелчок — вернуть как было",
    scene_undo_note: "Вернётся всё, что сцена поменяла",
    scene_confirm_note: "Режим экрана, как и сейчас, спросит «Оставить?» и сам вернётся через 15 с.",
    scene_delete: "Удалить сцену",
    scene_delete_yes: "Удалить",
    scene_undone: "Вернул как было",
    scene_nothing_undo: "Возвращать нечего",
    scene_already: "Эта сцена уже стоит",
    scene_edit: "Изменить сцену",
    scene_rows_none: "Отметьте, что менять",
    scene_plan_missing: "Плана питания из сцены нет — он пропущен",
    scene_bright_failed: "Не удалось поставить яркость: монитор не ответил по DDC/CI",
    scene_why_wifi: "Нет радио Wi‑Fi",
    scene_why_read: "Windows не отдала значение",
    scenes_none: "Сцен пока нет — «+» в шапке создаёт сцену из текущих настроек.",
    search_field: "Найти действие",
    search_tip: "Или просто начните печатать, пока панель открыта",
    search_clear: "Стереть",
    search_close: "Закрыть поиск (Esc)",
    search_keys: "↑↓ — выбрать · Enter — выполнить · Esc — назад",
    search_found: "Найдено",
    search_recent: "Недавние",
    search_start: "Начните печатать",
    search_try: "Можно так",
    search_none: "Ничего не нашлось — попробуйте другое слово",
    search_calc: "Посчитать · Enter — скопировать результат",
    search_pointer: "Наведите мышь на цвет вне панели",
    search_hide: "Спрятать панель",
    search_undo: "вернуть как было",
    search_pick: "цвет под мышью",
    search_play: "играть",
    search_pause: "пауза",
    search_next: "следующий трек",
    search_previous: "предыдущий трек",
    turn_on: "включить",
    turn_off: "выключить",
    scale_word: "Масштаб",
    timer_named_done: "Таймер «{}» дошёл",
    tips: [
        ("ночн", "ночной свет"),
        ("1080", "режим экрана"),
        ("чай 5", "таймер на 5 минут"),
        ("2*3,5", "посчитать"),
        ("кино", "сцена"),
        ("тишина", "уведомления"),
    ],
    wifi: "Wi‑Fi",
    accents: ["Лайм", "Фиолетовый", "Индиго", "Фуксия", "Лазурь"],
};

const EN: Tr = Tr {
    display: "Display",
    recommended: "Recommended",
    favorites: "Favorites",
    favorites_hint: "Star a resolution to keep up to three favorites here, one click away.",
    favorites_full: "Up to three favorites — unstar another one first",
    star_add: "Add to favorites",
    star_remove: "Remove from favorites",
    cancel: "Cancel",
    change: "Change…",
    press_keys: "Press a shortcut: Ctrl or Alt and a key; Esc to cancel",
    key_busy: "Taken by another app",
    key_free: "Works",
    no_key: "No shortcut",
    remove_key: "Remove shortcut",
    key_is_panel: "This is the panel shortcut — pick another one",
    brightness: "Brightness",
    brightness_short: "Brightness",
    contrast: "Contrast",
    ddc_reading: "Asking the monitors…",
    ddc_none: "No monitor answers over DDC/CI. A laptop's built-in screen cannot be controlled this way; on an external one, turn DDC/CI on in its menu.",
    ddc_failed: "The monitor did not accept the value",
    input_title: "Switch the monitor input?",
    input_switch: "Switch",
    media: "Media",
    media_none: "Nothing is playing.",
    media_untitled: "Untitled",
    media_previous: "Previous",
    media_next: "Next",
    media_play: "Play",
    media_pause: "Pause",
    media_failed: "The player did not answer",
    all_monitors: "All",
    all_monitors_note: "The panel slides out at the monitor whose edge the mouse touched.",
    strip: "Thin strip at the edge while the panel is hidden",
    exclude_title: "Do not slide out over apps",
    exclude_none: "The list is empty — the panel slides out over any app.",
    exclude_remove: "Remove from the list",
    key_auto: "Use the default",
    resources: "Resources",
    cpu: "CPU",
    gpu: "GPU",
    memory: "Memory",
    network: "Network",
    battery: "Battery",
    plugged: "plugged in",
    clipboard: "Clipboard",
    clipboard_off: "Remembering is off in settings.",
    clipboard_empty: "Copy some text — it will show up here.",
    clipboard_put: "Put on the clipboard",
    clipboard_done: "On the clipboard — paste away",
    clipboard_clear: "Clear",
    clipboard_remember: "Remember copied text (in memory only)",
    tray: "Tray",
    tray_empty: "Drop files on the panel — they will wait here.",
    tray_missing: "missing",
    tray_reveal: "Show in folder",
    tray_remove: "Remove from the tray",
    bluetooth: "Bluetooth",
    bt_none: "No paired devices.",
    bt_settings: "Bluetooth settings",
    timers: "Timers",
    timer: "Timer",
    timer_work: "Work",
    timer_rest: "Break",
    timer_done: "Timer is up",
    timer_work_done: "Work is over — 5 minutes of break",
    timer_rest_done: "Break is over — back to work",
    pomodoro: "Pomodoro",
    pomodoro_hint: "25 minutes of work, 5 of break, over and over",
    timers_none: "No timers — start one with a button above.",
    timer_cancel: "Remove the timer",
    timer_pause: "Pause",
    timer_resume: "Resume",
    github: "GitHub",
    github_none: "Add a repository: “owner/name”.",
    github_no_runs: "No CI runs",
    github_add: "Add",
    github_remove: "Remove the repository",
    answer_first: "Answer “Keep this display mode?” first",
    mode_unavailable: "The monitor has no such mode — skipped",
    keep_title: "Keep this display mode?",
    keep: "Keep",
    revert: "Revert",
    scale_note: "Some apps pick up the new scale only after a restart.",
    buttons: "Buttons",
    buttons_empty: "No custom buttons yet — add lines to the buttons file.",
    edit_buttons: "Edit buttons",
    pin: "Pin: do not hide",
    unpin: "Unpin",
    settings: "Settings",
    back: "Back",
    quit: "Quit Antresol",
    panel: "Panel",
    monitor: "Monitor",
    autostart: "Start with Windows",
    fullscreen_guard: "Do not slide out over full-screen games and video",
    show_delay: "Slide out after",
    hide_delay: "Hide after",
    hotkey: "Show or hide",
    hotkey_busy: "All shortcuts are taken by other apps",
    resize_hint: "Drag the bottom edge or corners to resize",
    failed: "Failed",
    ms: "ms",
    all_res: "All resolutions",
    refresh_hz: "Refresh, Hz",
    scale_pct: "Scale, %",
    hide: "Hide (Esc)",
    color: "Color",
    custom_color: "Custom color…",
    hue: "Hue",
    sat_value: "Saturation and brightness",
    color_code: "Color code",
    presets: "Presets",
    windows_color: "Use Windows color",
    windows_color_none: "Cannot read the Windows color",
    dark_theme: "Dark theme",
    light_theme: "Light theme",
    contrast_text: "Text",
    contrast_on: "On button",
    adjusted: "Adjusted for legibility",
    as_picked: "As picked",
    color_note: "The color adapts to each theme so text stays legible (4.5:1 or more).",
    apply: "Apply",
    sample: "Sample",
    done: "Done",
    media_strip: "Media on the hidden strip",
    open_panel: "Open the panel",
    appearance: "Appearance",
    more_hint: "Scroll to the other cards",
    sound: "Sound",
    sound_apps: "Apps",
    kamerton_off: "Kamerton is not running. Device and app volume will show here once it runs.",
    kamerton_start: "Start Kamerton",
    kamerton_get: "Get Kamerton",
    kamerton_mixer: "Kamerton mixer",
    kamerton_failed: "Kamerton did not answer",
    mute: "Mute",
    unmute: "Unmute",
    no_output: "No output device",
    quick: "Quick settings",
    night_light: "Night light",
    on: "On",
    off: "Off",
    unavailable: "Unavailable",
    primary: "primary",
    make_primary: "Make primary",
    keep_awake: "Keep awake",
    dark_theme_win: "Dark mode",
    quiet: "Quiet",
    power_plan: "Power plan",
    connect: "Connect",
    disconnect: "Disconnect",
    tools: "Tools",
    picker: "Color picker",
    picker_hint: "Drag the circle onto any color on screen — its code goes to the clipboard.",
    color_copied: "copied",
    screen_text: "Text from screen",
    screen_text_pick: "Select an area",
    screen_text_wait: "Select an area on the screen…",
    screen_text_done: "The text is in the clipboard.",
    screen_text_none: "No text found in the snip.",
    screen_text_no_lang: "Windows has no text-recognition language — add one in Settings → Time & language.",
    clocks: "Clocks",
    clocks_empty: "Add a time zone to see the time there.",
    clock_add: "Add time zone",
    clock_remove: "Remove clock",
    clock_search: "Zone, e.g. Tokyo",
    clock_unknown: "Time zone not found",
    temperature: "Temperature",
    shelf_av: "Display & sound",
    shelf_sys: "System",
    shelf_hand: "At hand",
    shelf_add: "Shelf",
    shelves_edit: "Edit shelves",
    shelves_hint: "Drag a card by its handle — within the row or onto another shelf’s tab",
    shelves_undo: "Undo all",
    grip_hint: "Drag with the mouse or Alt + ←/→; Alt + 1…9 moves it to another shelf",
    removed_cards: "Removed cards",
    removed_none: "Nothing removed yet",
    removed_note: "The cross on a card puts it here. Click it to bring it back to this shelf.",
    shelf_empty: "This shelf is empty",
    shelf_rename: "Double-click to rename",
    shelf_remove: "Remove the shelf — its cards go to Removed cards",
    quiet_on: "no pop-ups",
    clip_pin: "Pin",
    clip_pin_from: "Pin from clipboard",
    clip_no_text: "No text in the clipboard",
    clip_hint: "Click to copy again. Pin to keep.",
    clip_pinned_full: "30 pinned — unpin some first",
    clip_unpinned: "Unpinned",
    shelf_limit: "At most nine shelves — remove one first",
    paused: "paused",
    hdr: "HDR",
    hdr_hint: "HDR for this monitor. Win+Alt+B toggles all monitors at once",
    hdr_blocked: "Windows won't turn HDR on for this monitor right now",
    scene: "Scene",
    scenes: "Scenes",
    scene_new: "New scene from current settings",
    scene_what: "What the scene changes",
    scene_untouched: "leave as is",
    scene_take: "Use current values",
    scene_name_key: "Name and shortcut",
    scene_name: "Name",
    scene_name_empty: "A name is needed",
    scene_name_taken: "There is already a scene with this name",
    scene_key: "Shortcut",
    scene_exit: "Leaving the scene",
    scene_undo: "Click again to undo",
    scene_undo_note: "Everything the scene changed comes back",
    scene_confirm_note: "As now, a display mode change asks “Keep?” and reverts by itself after 15 s.",
    scene_delete: "Delete scene",
    scene_delete_yes: "Delete",
    scene_undone: "Undone",
    scene_nothing_undo: "Nothing to undo",
    scene_already: "This scene is already on",
    scene_edit: "Edit scene",
    scene_rows_none: "Check what to change",
    scene_plan_missing: "The power plan from the scene is gone — skipped",
    scene_bright_failed: "Could not set the brightness: the monitor did not answer over DDC/CI",
    scene_why_wifi: "No Wi‑Fi radio",
    scene_why_read: "Windows did not give the value",
    scenes_none: "No scenes yet — “+” in the header makes one from the current settings.",
    search_field: "Find an action",
    search_tip: "Or just start typing while the panel is open",
    search_clear: "Clear",
    search_close: "Close search (Esc)",
    search_keys: "↑↓ select · Enter run · Esc back",
    search_found: "Results",
    search_recent: "Recent",
    search_start: "Start typing",
    search_try: "Try",
    search_none: "Nothing found — try another word",
    search_calc: "Calculate · Enter copies the result",
    search_pointer: "Point the mouse at a color outside the panel",
    search_hide: "Hide the panel",
    search_undo: "undo",
    search_pick: "color under the mouse",
    search_play: "play",
    search_pause: "pause",
    search_next: "next track",
    search_previous: "previous track",
    turn_on: "turn on",
    turn_off: "turn off",
    scale_word: "Scale",
    timer_named_done: "Timer “{}” is up",
    tips: [
        ("night", "night light"),
        ("1080", "display mode"),
        ("tea 5", "5 minute timer"),
        ("2*3.5", "calculate"),
        ("movie", "scene"),
        ("quiet", "notifications"),
    ],
    wifi: "Wi‑Fi",
    accents: ["Lime", "Violet", "Indigo", "Fuchsia", "Sky"],
};

pub fn tr(lang: Lang) -> &'static Tr {
    match lang {
        Lang::Ru => &RU,
        Lang::En => &EN,
    }
}

/// Строка под текущим режимом: «144 Гц · масштаб 125 %».
pub fn mode_line(lang: Lang, hz: u32, scale: Option<u32>) -> String {
    match (lang, scale) {
        (Lang::Ru, Some(s)) => format!("{hz} Гц · масштаб {s} %"),
        (Lang::Ru, None) => format!("{hz} Гц"),
        (Lang::En, Some(s)) => format!("{hz} Hz · scale {s}%"),
        (Lang::En, None) => format!("{hz} Hz"),
    }
}

/// Кнопка у не влезшего ряда карточек: «Ещё 4».
pub fn more_cards(lang: Lang, n: usize) -> String {
    match lang {
        Lang::Ru => format!("Ещё {n}"),
        Lang::En => format!("{n} more"),
    }
}

/// Имя полки для вкладки: `@av`, `@sys`, `@hand` — готовые (переводятся), остальное — как ввели.
pub fn shelf_title<'a>(t: &'a Tr, name: &'a str) -> &'a str {
    match name {
        "@av" => t.shelf_av,
        "@sys" => t.shelf_sys,
        "@hand" => t.shelf_hand,
        other => other,
    }
}

/// Имя новой полки: «Полка 4».
pub fn shelf_n(lang: Lang, n: usize) -> String {
    match lang {
        Lang::Ru => format!("Полка {n}"),
        Lang::En => format!("Shelf {n}"),
    }
}

/// Выдуманные тексты для снимков (демо-режим) — по языку сцены, чтобы в английских не было русских: недавнее и
/// закреплённое в «Буфере обмена», города в «Часах».
pub fn demo_clip_recent(lang: Lang) -> [&'static str; 3] {
    match lang {
        Lang::Ru => ["Пример текста для вставки", "https://example.com/demo", "#2A5FD0"],
        Lang::En => ["Sample text to paste", "https://example.com/demo", "#2A5FD0"],
    }
}

pub fn demo_clip_pinned(lang: Lang) -> [&'static str; 2] {
    match lang {
        Lang::Ru => ["Спасибо, получил!", "Встреча завтра в 10:00\nКомната 3"],
        Lang::En => ["Thanks, got it!", "Meeting tomorrow at 10:00\nRoom 3"],
    }
}

pub fn demo_city(lang: Lang, i: usize) -> &'static str {
    match (lang, i) {
        (Lang::Ru, 0) => "Токио",
        (Lang::Ru, _) => "Нью-Йорк",
        (Lang::En, 0) => "Tokyo",
        (Lang::En, _) => "New York",
    }
}

/// Имя ручки карточки для диктора: «Перетащить «Экран»».
pub fn drag_card(lang: Lang, title: &str) -> String {
    match lang {
        Lang::Ru => format!("Перетащить «{title}»"),
        Lang::En => format!("Drag “{title}”"),
    }
}

/// Крестик на карточке: «Убрать с полки «Экран»».
pub fn remove_card(lang: Lang, title: &str) -> String {
    match lang {
        Lang::Ru => format!("Убрать с полки «{title}»"),
        Lang::En => format!("Remove “{title}” from the shelf"),
    }
}

/// Подсказка метки таймера на вкладке: «Идёт таймер «Чай», осталось 03:42».
pub fn timer_running(lang: Lang, name: &str, left: &str) -> String {
    match lang {
        Lang::Ru => format!("Идёт таймер «{name}», осталось {left}"),
        Lang::En => format!("Timer “{name}” is running, {left} left"),
    }
}

/// Разница поясов словами: «+6 ч», «−1 ч 30 мин», «как здесь».
pub fn time_diff(lang: Lang, minutes: i32) -> String {
    if minutes == 0 {
        return match lang {
            Lang::Ru => "как здесь".into(),
            Lang::En => "same as here".into(),
        };
    }
    let sign = if minutes > 0 { "+" } else { "−" };
    let (h, m) = (minutes.abs() / 60, minutes.abs() % 60);
    match (lang, h, m) {
        (Lang::Ru, _, 0) => format!("{sign}{h} ч"),
        (Lang::Ru, 0, _) => format!("{sign}{m} мин"),
        (Lang::Ru, _, _) => format!("{sign}{h} ч {m} мин"),
        (Lang::En, _, 0) => format!("{sign}{h} h"),
        (Lang::En, 0, _) => format!("{sign}{m} min"),
        (Lang::En, _, _) => format!("{sign}{h} h {m} min"),
    }
}

/// Контраст «6,1:1» / «6.1:1».
pub fn ratio(lang: Lang, r: f32) -> String {
    let s = format!("{r:.1}");
    match lang {
        Lang::Ru => format!("{}:1", s.replace('.', ",")),
        Lang::En => format!("{s}:1"),
    }
}

/// «Оставить этот режим?» с обратным отсчётом.
pub fn revert_in(lang: Lang, secs: u64) -> String {
    match lang {
        Lang::Ru => format!("Вернётся сам через {secs} с"),
        Lang::En => format!("Reverts in {secs} s"),
    }
}

/// Подпись строки сцены — по ней же уведомления называют, что не вышло.
pub fn scene_row_label(t: &Tr, row: SceneRow) -> &'static str {
    match row {
        SceneRow::Screen => t.display,
        SceneRow::Bright => t.brightness,
        SceneRow::Night => t.night_light,
        SceneRow::Quiet => t.quiet,
        SceneRow::Awake => t.keep_awake,
        SceneRow::Dark => t.dark_theme_win,
        SceneRow::Wifi => t.wifi,
        SceneRow::Plan => t.power_plan,
    }
}

/// Часть сцены не поставилась: своими словами, а английский текст ошибки остаётся в журнале — «Не получилось: Тёмная тема».
pub fn scene_failed(t: &Tr, row: SceneRow) -> String {
    format!("{}: {}", t.failed, scene_row_label(t, row))
}

/// Имя новой сцены: «Сцена 2» (первый свободный номер).
pub fn scene_n(lang: Lang, n: usize) -> String {
    match lang {
        Lang::Ru => format!("Сцена {n}"),
        Lang::En => format!("Scene {n}"),
    }
}

/// Уведомление, когда сцену поставили: «Сцена «Кино»».
pub fn scene_set(lang: Lang, name: &str) -> String {
    match lang {
        Lang::Ru => format!("Сцена «{name}»"),
        Lang::En => format!("Scene “{name}”"),
    }
}

/// Вопрос перед удалением: «Удалить сцену «Кино»?».
pub fn scene_delete_q(lang: Lang, name: &str) -> String {
    match lang {
        Lang::Ru => format!("Удалить сцену «{name}»?"),
        Lang::En => format!("Delete scene “{name}”?"),
    }
}

/// Имя в кавычках языка — заголовок экрана правки сцены: «Кино», “Movie”.
pub fn scene_quoted(lang: Lang, name: &str) -> String {
    match lang {
        Lang::Ru => format!("«{name}»"),
        Lang::En => format!("“{name}”"),
    }
}

/// Подсказка чипа сцены: клавиша (если есть), что делает щелчок и — если возврат включён — второй, и правый щелчок.
/// «Ctrl+Alt+3 · щелчок — включить, ещё щелчок — вернуть; правый щелчок — изменить»; без клавиши — с большой буквы.
pub fn scene_tip(lang: Lang, hotkey: Option<&str>, undo: bool) -> String {
    let (click, again, right) = match lang {
        Lang::Ru => ("щелчок — включить", ", ещё щелчок — вернуть", "; правый щелчок — изменить"),
        Lang::En => ("click to switch on", ", click again to undo", "; right-click to edit"),
    };
    let tip = format!("{click}{}{right}", if undo { again } else { "" });
    match hotkey {
        Some(key) => format!("{key} · {tip}"),
        None => {
            let mut chars = tip.chars();
            chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
        }
    }
}

/// Режим одного монитора сцены: «3840 × 2160 · 60 Гц · 150 % · HDR»; `sdr` в файле — «· SDR».
pub fn scene_mode(lang: Lang, width: u32, height: u32, hz: u32, scale: Option<u32>, hdr: Option<bool>) -> String {
    let mut s = format!("{width} × {height} · ");
    s += &match lang {
        Lang::Ru => format!("{hz} Гц"),
        Lang::En => format!("{hz} Hz"),
    };
    if let Some(sc) = scale {
        s += &match lang {
            Lang::Ru => format!(" · {sc} %"),
            Lang::En => format!(" · {sc}%"),
        };
    }
    match hdr {
        Some(true) => s += " · HDR",
        Some(false) => s += " · SDR",
        None => {}
    }
    s
}

/// Экран сцены на нескольких мониторах — коротко, полное — в подсказке: «Мониторов: 2».
pub fn scene_monitors(lang: Lang, n: usize) -> String {
    match lang {
        Lang::Ru => format!("Мониторов: {n}"),
        Lang::En => format!("{n} monitors"),
    }
}

/// Процент у строки «Яркость»: «80 %» / «80%».
pub fn scene_percent(lang: Lang, n: u8) -> String {
    match lang {
        Lang::Ru => format!("{n} %"),
        Lang::En => format!("{n}%"),
    }
}

/// Сколько мониторов сцены сейчас нет.
pub fn monitors_missing(lang: Lang, n: usize) -> String {
    match lang {
        Lang::Ru => format!("Мониторов из сцены нет: {n} — пропущены"),
        Lang::En => format!("Monitors from the scene not found: {n} — skipped"),
    }
}

/// Предупреждение перед сменой входа монитора.
pub fn input_warning(lang: Lang, monitor: &str, input: &str) -> String {
    match lang {
        Lang::Ru => format!(
            "{monitor} перейдёт на {input}. Если там нет сигнала, экран погаснет — вернуть вход можно только кнопками монитора."
        ),
        Lang::En => format!(
            "{monitor} will switch to {input}. If there is no signal there, the screen goes dark — only the monitor's own buttons bring it back."
        ),
    }
}

/// «Добавить «chrome.exe»» — последнюю программу на переднем плане в список «не выезжать».
pub fn exclude_add(lang: Lang, exe: &str) -> String {
    match lang {
        Lang::Ru => format!("Добавить «{exe}»"),
        Lang::En => format!("Add “{exe}”"),
    }
}

/// Сочетание уже у сцены.
pub fn key_is_scene(lang: Lang, name: &str) -> String {
    match lang {
        Lang::Ru => format!("Это сочетание у сцены «{name}» — выберите другое"),
        Lang::En => format!("This shortcut belongs to the scene “{name}” — pick another one"),
    }
}

/// «свободно 120 ГБ из 500 ГБ».
pub fn disk_free(lang: Lang, free: &str, total: &str) -> String {
    match lang {
        Lang::Ru => format!("свободно {free} из {total}"),
        Lang::En => format!("{free} free of {total}"),
    }
}

/// «5 мин».
pub fn minutes(lang: Lang, n: u64) -> String {
    match lang {
        Lang::Ru => format!("{n} мин"),
        Lang::En => format!("{n} min"),
    }
}

/// «PR 2».
pub fn prs(_lang: Lang, n: usize) -> String {
    format!("PR {n}")
}

/// «12 мин назад», «3 ч назад», «2 дн назад».
pub fn ago(lang: Lang, secs: i64) -> String {
    let s = secs.max(0);
    let (n, ru, en) = if s < 60 {
        return match lang {
            Lang::Ru => "только что".into(),
            Lang::En => "just now".into(),
        };
    } else if s < 3600 {
        (s / 60, "мин", "min")
    } else if s < 86400 {
        (s / 3600, "ч", "h")
    } else {
        (s / 86400, "дн", "d")
    };
    match lang {
        Lang::Ru => format!("{n} {ru} назад"),
        Lang::En => format!("{n} {en} ago"),
    }
}

/// Состояние прогона словами.
pub fn run_state(lang: Lang, status: &str, conclusion: &str) -> String {
    let (ru, en) = match (status, conclusion) {
        ("completed", "success") => ("прошёл", "passed"),
        ("completed", "failure") => ("упал", "failed"),
        ("completed", "cancelled") => ("отменён", "cancelled"),
        ("completed", "timed_out") => ("не успел", "timed out"),
        ("completed", "skipped") => ("пропущен", "skipped"),
        ("completed", _) => ("закончен", "finished"),
        ("queued" | "waiting" | "pending" | "requested", _) => ("в очереди", "queued"),
        _ => ("идёт", "running"),
    };
    match lang {
        Lang::Ru => ru.into(),
        Lang::En => en.into(),
    }
}

/// Ошибка чтения репозитория: код ответа GitHub или сеть.
pub fn github_error(lang: Lang, e: &str) -> String {
    match (lang, e) {
        (Lang::Ru, "404") => "Не найден (или закрытый — нужен токен)".into(),
        (Lang::En, "404") => "Not found (or private — a token is needed)".into(),
        (Lang::Ru, "403" | "429") => "GitHub ограничил запросы — позже".into(),
        (Lang::En, "403" | "429") => "GitHub rate limit — later".into(),
        (Lang::Ru, other) => format!("Не прочитать: {other}"),
        (Lang::En, other) => format!("Cannot read: {other}"),
    }
}

// ─── Поиск действий (2.2.0) ─────────────────────────────────────────────────

/// Таймер из запроса: «Таймер «чай» на 5 мин»; без имени — «Таймер на 5 мин».
pub fn search_timer(lang: Lang, name: Option<&str>, minutes: u32) -> String {
    match (lang, name) {
        (Lang::Ru, Some(n)) => format!("Таймер «{n}» на {minutes} мин"),
        (Lang::Ru, None) => format!("Таймер на {minutes} мин"),
        (Lang::En, Some(n)) => format!("Timer “{n}” for {minutes} min"),
        (Lang::En, None) => format!("Timer for {minutes} min"),
    }
}

/// «Открыть полку «Система»».
pub fn search_open_shelf(lang: Lang, name: &str) -> String {
    match lang {
        Lang::Ru => format!("Открыть полку «{name}»"),
        Lang::En => format!("Open the “{name}” shelf"),
    }
}

/// «Открыть «report.pdf»» — файл лотка и репозиторий GitHub.
pub fn search_open(lang: Lang, name: &str) -> String {
    match lang {
        Lang::Ru => format!("Открыть «{name}»"),
        Lang::En => format!("Open “{name}”"),
    }
}

/// «Скопировать «Спасибо, получил!»» — закреплённый текст буфера.
pub fn search_copy(lang: Lang, text: &str) -> String {
    match lang {
        Lang::Ru => format!("Скопировать «{text}»"),
        Lang::En => format!("Copy “{text}”"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ago_words() {
        assert_eq!(ago(Lang::Ru, 30), "только что");
        assert_eq!(ago(Lang::Ru, 12 * 60), "12 мин назад");
        assert_eq!(ago(Lang::En, 3 * 3600 + 5), "3 h ago");
        assert_eq!(ago(Lang::Ru, 2 * 86400), "2 дн назад");
        assert_eq!(time_diff(Lang::Ru, -30), "−30 мин");
        assert_eq!(time_diff(Lang::Ru, 330), "+5 ч 30 мин");
        assert_eq!(time_diff(Lang::En, 0), "same as here");
    }
}
