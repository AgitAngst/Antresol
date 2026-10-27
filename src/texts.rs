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
    // Буфер: картинки, ссылки, цвета, фильтр и «Исправить раскладку» (2.3.0): один блок.
    pub clip_all: &'static str,
    pub clip_text: &'static str,
    pub clip_links: &'static str,
    pub clip_images: &'static str,
    pub clip_colors: &'static str,
    pub clip_nothing: &'static str,
    pub clip_open: &'static str,
    pub clip_fix: &'static str,
    /// Надпись на самой кнопке «Исправить раскладку» (по-русски короче); полное название — подсказка и диктор.
    pub clip_fix_chip: &'static str,
    // Живые плашки у края (2.4.0): один блок. `{}` — имя, число или размер (см. `plaque_text`).
    pub plaques: &'static str,
    pub plaques_usb: &'static str,
    pub plaques_downloads: &'static str,
    pub plaque_connected: &'static str,
    pub plaque_bt_charge: &'static str,
    pub plaque_low: &'static str,
    pub plaque_battery: &'static str,
    pub plaque_usb: &'static str,
    pub plaque_usb_in: &'static str,
    pub plaque_open: &'static str,
    pub plaque_eject: &'static str,
    pub plaque_safe: &'static str,
    pub plaque_busy: &'static str,
    pub plaque_build_failed: &'static str,
    pub plaque_build_passed: &'static str,
    pub plaque_downloaded: &'static str,
    pub plaque_many: &'static str,
    pub plaque_in_downloads: &'static str,
    pub plaque_to_tray: &'static str,
    pub plaque_close: &'static str,
    pub plaque_gone: &'static str,
    pub plaque_ejecting: &'static str,
    // Встречи (2.5.0): один блок. `{}` в `plaque_meeting` — название и число минут (см. `plaque_text`).
    pub meetings: &'static str,
    pub shelf_day: &'static str,
    pub meetings_join: &'static str,
    pub meetings_later: &'static str,
    pub meetings_none_today: &'static str,
    pub meetings_tomorrow: &'static str,
    pub meetings_link: &'static str,
    pub meetings_add: &'static str,
    pub meetings_refresh: &'static str,
    pub meetings_calendars: &'static str,
    pub meetings_add_calendar: &'static str,
    pub meetings_failed: &'static str,
    pub meetings_not_calendar: &'static str,
    pub meetings_big: &'static str,
    pub meetings_loading: &'static str,
    pub meetings_empty: &'static str,
    pub meetings_google: &'static str,
    pub meetings_outlook: &'static str,
    pub meetings_link_bad: &'static str,
    pub meetings_how: &'static str,
    pub plaque_meeting: &'static str,
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
    clipboard_empty: "Скопируйте текст или картинку — они появятся здесь.",
    clipboard_put: "Положить в буфер обмена",
    clipboard_done: "В буфере — вставляйте",
    clipboard_clear: "Очистить",
    clipboard_remember: "Запоминать скопированное (только в памяти)",
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
    clip_all: "Все",
    clip_text: "Текст",
    clip_links: "Ссылки",
    clip_images: "Картинки",
    clip_colors: "Цвета",
    clip_nothing: "Здесь пока ничего нет",
    clip_open: "Открыть в браузере",
    clip_fix: "Исправить раскладку",
    clip_fix_chip: "Исправить",
    plaques: "Живые плашки",
    plaques_usb: "Флешки",
    plaques_downloads: "Загрузки",
    plaque_connected: "{}: подключено",
    plaque_bt_charge: "Bluetooth · заряд {} %",
    plaque_low: "{}: разряжается",
    plaque_battery: "Заряд {} %",
    plaque_usb: "Флешка «{}»",
    plaque_usb_in: "{} · вставлена",
    plaque_open: "Открыть",
    plaque_eject: "Извлечь",
    plaque_safe: "Можно вынимать",
    plaque_busy: "Не извлечь: диск занят",
    plaque_build_failed: "{}: сборка упала",
    plaque_build_passed: "{}: сборка прошла",
    plaque_downloaded: "{}: скачано",
    plaque_many: "Скачано файлов: {}",
    plaque_in_downloads: "{} · Загрузки",
    plaque_to_tray: "В лоток",
    plaque_close: "Закрыть",
    plaque_gone: "Флешку уже вынули",
    plaque_ejecting: "Извлекаю…",
    meetings: "Встречи",
    shelf_day: "День",
    meetings_join: "Подключиться",
    meetings_later: "Дальше сегодня",
    meetings_none_today: "Сегодня встреч больше нет",
    meetings_tomorrow: "Завтра",
    meetings_link: "Ссылка на календарь",
    meetings_add: "Добавить",
    meetings_refresh: "Обновить",
    meetings_calendars: "Календари",
    meetings_add_calendar: "Добавить календарь",
    meetings_failed: "Календарь не ответил",
    meetings_not_calendar: "Ссылка не на календарь",
    meetings_big: "Календарь большой — прочитана только часть",
    meetings_loading: "Читаю календарь…",
    meetings_empty: "Покажу ближайшие встречи и кнопку «Подключиться» из календаря Google или Outlook по секретной ссылке .ics",
    meetings_google: "Google: Настройки календаря → Секретный адрес в формате iCal",
    meetings_outlook: "Outlook: Параметры → Календарь → Общие календари → Опубликовать календарь → ICS",
    meetings_link_bad: "Нужна ссылка вида https://… или webcal://…",
    meetings_how: "Календарь — по ссылке .ics из Google или Outlook, учётная запись Windows не нужна.",
    plaque_meeting: "{} через {} мин",
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
    clipboard_empty: "Copy some text or an image — it will show up here.",
    clipboard_put: "Put on the clipboard",
    clipboard_done: "On the clipboard — paste away",
    clipboard_clear: "Clear",
    clipboard_remember: "Remember what is copied (in memory only)",
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
    clip_all: "All",
    clip_text: "Text",
    clip_links: "Links",
    clip_images: "Images",
    clip_colors: "Colors",
    clip_nothing: "Nothing here yet",
    clip_open: "Open in the browser",
    clip_fix: "Fix layout",
    clip_fix_chip: "Fix layout",
    plaques: "Live plaques",
    plaques_usb: "USB drives",
    plaques_downloads: "Downloads",
    plaque_connected: "{}: connected",
    plaque_bt_charge: "Bluetooth · battery {}%",
    plaque_low: "{}: running low",
    plaque_battery: "Battery {}%",
    plaque_usb: "USB drive “{}”",
    plaque_usb_in: "{} · plugged in",
    plaque_open: "Open",
    plaque_eject: "Eject",
    plaque_safe: "Safe to remove",
    plaque_busy: "Can’t eject: the drive is busy",
    plaque_build_failed: "{}: build failed",
    plaque_build_passed: "{}: build passed",
    plaque_downloaded: "{}: downloaded",
    plaque_many: "{} files downloaded",
    plaque_in_downloads: "{} · Downloads",
    plaque_to_tray: "To tray",
    plaque_close: "Close",
    plaque_gone: "The drive is already gone",
    plaque_ejecting: "Ejecting…",
    meetings: "Meetings",
    shelf_day: "Today",
    meetings_join: "Join",
    meetings_later: "Later today",
    meetings_none_today: "No more meetings today",
    meetings_tomorrow: "Tomorrow",
    meetings_link: "Calendar link",
    meetings_add: "Add",
    meetings_refresh: "Refresh",
    meetings_calendars: "Calendars",
    meetings_add_calendar: "Add a calendar",
    meetings_failed: "The calendar did not answer",
    meetings_not_calendar: "The link is not a calendar",
    meetings_big: "The calendar is big — only part of it was read",
    meetings_loading: "Reading the calendar…",
    meetings_empty: "I’ll show your next meetings and a “Join” button from a Google or Outlook calendar by its secret .ics link",
    meetings_google: "Google: Calendar settings → Secret address in iCal format",
    meetings_outlook: "Outlook: Settings → Calendar → Shared calendars → Publish a calendar → ICS",
    meetings_link_bad: "A link like https://… or webcal://… is needed",
    meetings_how: "The calendar comes from an .ics link from Google or Outlook; no Windows account needed.",
    plaque_meeting: "{} in {} min",
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

/// Имя полки для вкладки: `@av`, `@sys`, `@day`, `@hand` — готовые (переводятся), остальное — как ввели.
pub fn shelf_title<'a>(t: &'a Tr, name: &'a str) -> &'a str {
    match name {
        "@av" => t.shelf_av,
        "@sys" => t.shelf_sys,
        "@day" => t.shelf_day,
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
pub fn demo_clip_recent() -> [&'static str; 3] {
    // Одинаковые на обоих языках, русских букв в них нет: текст не в той раскладке, ссылка и цвет. Картинку (1280 × 720)
    // рисует сама карточка.
    ["Ghbdtn, rfr ltkf?", "https://example.com/report", "#4f7a12"]
}

pub fn demo_clip_pinned(lang: Lang) -> [&'static str; 1] {
    // Одно закреплённое, как на макете: иначе на карточке 340 высотой недавнему не хватает места.
    match lang {
        Lang::Ru => ["Спасибо, получил!"],
        Lang::En => ["Thanks, got it!"],
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

/// Почему «Добавить пояс» серое: часов уже предел («Часов уже 8»).
pub fn search_clocks_full(lang: Lang, max: usize) -> String {
    match lang {
        Lang::Ru => format!("Часов уже {max}"),
        Lang::En => format!("Already {max} clocks"),
    }
}

// ─── Буфер: картинки, ссылки, цвета, раскладка (2.3.0) ──────────────────────

/// Название картинки в истории (подсказка и имя для диктора): «Картинка · 1280 × 720».
pub fn clip_image(lang: Lang, width: u32, height: u32) -> String {
    match lang {
        Lang::Ru => format!("Картинка · {}", clip_size(width, height)),
        Lang::En => format!("Image · {}", clip_size(width, height)),
    }
}

/// Размер картинки в самой строке: «1280 × 720» (миниатюра и так говорит, что это картинка).
pub fn clip_size(width: u32, height: u32) -> String {
    format!("{width} × {height}")
}

/// Подсказка к «Исправить раскладку»: на что похож текст и что получится.
pub fn clip_fix_tip(lang: Lang, to_russian: bool, fixed: &str) -> String {
    match (lang, to_russian) {
        (Lang::Ru, true) => format!("Похоже на русский текст, набранный в английской раскладке: «{fixed}»"),
        (Lang::Ru, false) => format!("Похоже на английский текст, набранный в русской раскладке: «{fixed}»"),
        (Lang::En, true) => format!("Looks like Russian typed with the English layout: “{fixed}”"),
        (Lang::En, false) => format!("Looks like English typed with the Russian layout: “{fixed}”"),
    }
}

/// Сколько назад скопировано — справа в строке истории: «сейчас», «2 мин», «1 ч».
pub fn clip_age(lang: Lang, secs: u64) -> String {
    match (lang, secs) {
        (Lang::Ru, 0..=59) => "сейчас".into(),
        (Lang::En, 0..=59) => "now".into(),
        (_, 60..=3599) => minutes(lang, secs / 60),
        (Lang::Ru, _) => format!("{} ч", secs / 3600),
        (Lang::En, _) => format!("{} h", secs / 3600),
    }
}

// ─── Встречи (2.5.0) ────────────────────────────────────────────────────────

/// Имя службы видеосвязи (по сайту ссылки): «Zoom», «Meet», «Teams», «Телемост»…
pub fn service_name(lang: Lang, service: crate::ics::Service) -> &'static str {
    use crate::ics::Service;
    match (service, lang) {
        (Service::Zoom, _) => "Zoom",
        (Service::Meet, _) => "Meet",
        (Service::Teams, _) => "Teams",
        (Service::Telemost, Lang::Ru) => "Телемост",
        (Service::Telemost, Lang::En) => "Telemost",
        (Service::Jazz, _) => "Jazz",
        (Service::Jitsi, _) => "Jitsi",
        (Service::Webex, _) => "Webex",
        (Service::Whereby, _) => "Whereby",
        (Service::Discord, _) => "Discord",
    }
}

/// Сколько до встречи или до её конца: «12 мин», «1 ч 20 мин», «2 ч».
pub fn span(lang: Lang, minutes: i64) -> String {
    let (h, m) = (minutes / 60, minutes % 60);
    match (lang, h, m) {
        (Lang::Ru, 0, _) => format!("{m} мин"),
        (Lang::Ru, _, 0) => format!("{h} ч"),
        (Lang::Ru, _, _) => format!("{h} ч {m} мин"),
        (Lang::En, 0, _) => format!("{m} min"),
        (Lang::En, _, 0) => format!("{h} h"),
        (Lang::En, _, _) => format!("{h} h {m} min"),
    }
}

/// «через 12 мин» / «in 12 min».
pub fn meeting_in(lang: Lang, minutes: i64) -> String {
    match lang {
        Lang::Ru => format!("через {}", span(lang, minutes)),
        Lang::En => format!("in {}", span(lang, minutes)),
    }
}

/// «идёт · ещё 20 мин» / «now · 20 min left».
pub fn meeting_left(lang: Lang, minutes: i64) -> String {
    match lang {
        Lang::Ru => format!("идёт · ещё {}", span(lang, minutes)),
        Lang::En => format!("now · {} left", span(lang, minutes)),
    }
}

/// Первая завтрашняя встреча, когда сегодня их больше нет: «Завтра 09:30 · Планёрка».
pub fn meeting_tomorrow(t: &Tr, time: &str, title: &str) -> String {
    format!("{} {time} · {title}", t.meetings_tomorrow)
}

/// Справа в заголовке карточки при одном календаре: «Календарь «Работа»» / “Work” calendar.
pub fn meetings_calendar(lang: Lang, name: &str) -> String {
    match lang {
        Lang::Ru => format!("Календарь «{name}»"),
        Lang::En => format!("“{name}” calendar"),
    }
}

/// То же при нескольких: «Календарей: 2».
pub fn meetings_calendars_n(lang: Lang, n: usize) -> String {
    match lang {
        Lang::Ru => format!("Календарей: {n}"),
        Lang::En => format!("{n} calendars"),
    }
}

/// Крестик у календаря в списке: «Убрать календарь «Работа»» (и подсказка, и имя для диктора).
pub fn meetings_remove(lang: Lang, name: &str) -> String {
    match lang {
        Lang::Ru => format!("Убрать календарь «{name}»"),
        Lang::En => format!("Remove the “{name}” calendar"),
    }
}

/// Действие поиска: «Подключиться к «Планёрка»» / Join “Daily stand-up”.
pub fn search_join(lang: Lang, title: &str) -> String {
    match lang {
        Lang::Ru => format!("Подключиться к «{title}»"),
        Lang::En => format!("Join “{title}”"),
    }
}

// ─── Живые плашки у края (2.4.0) ────────────────────────────────────────────

/// Размер файла или диска в плашке: «32 ГБ», «1,2 МБ» — как `res::bytes`, но без лишнего «,0».
pub fn plaque_size(lang: Lang, bytes: u64) -> String {
    let text = crate::sys::res::bytes(bytes as f64, lang == Lang::Ru);
    text.replace(",0 ", " ").replace(".0 ", " ")
}

/// Слова плашки по языку: ключ и числа из данных собираются здесь, при показе; «сколько назад» — от `now` (секунды с 1970).
pub fn plaque_text_at(lang: Lang, say: &crate::plaque::Say, now: i64) -> String {
    use crate::plaque::Say;
    let t = tr(lang);
    let with = |template: &str, value: &str| template.replace("{}", value);
    match say {
        Say::None => String::new(),
        Say::TimerDone(Some(name)) => with(t.timer_named_done, name),
        Say::TimerDone(None) => t.timer_done.to_string(),
        Say::WorkDone => t.timer_work_done.to_string(),
        Say::RestDone => t.timer_rest_done.to_string(),
        Say::Connected(name) => with(t.plaque_connected, name),
        Say::BtCharge(Some(percent)) => with(t.plaque_bt_charge, &percent.to_string()),
        Say::BtCharge(None) => t.bluetooth.to_string(),
        Say::Low(name) => with(t.plaque_low, name),
        Say::Battery(percent) => with(t.plaque_battery, &percent.to_string()),
        Say::Usb(name) => with(t.plaque_usb, name),
        Say::UsbIn(bytes) => with(t.plaque_usb_in, &plaque_size(lang, *bytes)),
        Say::Safe => t.plaque_safe.to_string(),
        Say::Busy => t.plaque_busy.to_string(),
        Say::Ejecting => t.plaque_ejecting.to_string(),
        Say::Gone => t.plaque_gone.to_string(),
        Say::BuildFailed(repo) => with(t.plaque_build_failed, repo),
        Say::BuildPassed(repo) => with(t.plaque_build_passed, repo),
        Say::RunAge { branch, at } => {
            let ago = at.map(|at| ago(lang, now - at)).unwrap_or_default();
            match (branch.is_empty(), ago.is_empty()) {
                (false, false) => format!("{branch} · {ago}"),
                (false, true) => branch.clone(),
                (true, _) => ago,
            }
        }
        Say::Downloaded(name) => with(t.plaque_downloaded, name),
        Say::DownloadedMany(count) => with(t.plaque_many, &count.to_string()),
        Say::InDownloads(bytes) => with(t.plaque_in_downloads, &plaque_size(lang, *bytes)),
        Say::Failed(name) => format!("{}: {name}", t.failed),
        Say::MeetingIn { title, start } => {
            // «Планёрка через 2 мин»: минуты считаются при показе — висящая плашка не врёт. Шаблон: название, потом число.
            let minutes = (((*start - now).max(0) + 59) / 60).max(1).to_string();
            let title = if title.is_empty() { t.media_untitled } else { title };
            let mut parts = t.plaque_meeting.splitn(3, "{}");
            let (before, middle, after) =
                (parts.next().unwrap_or_default(), parts.next().unwrap_or_default(), parts.next().unwrap_or_default());
            format!("{before}{title}{middle}{minutes}{after}")
        }
        Say::MeetingAt { time, service } => format!("{time} · {}", service_name(lang, *service)),
        Say::Join => t.meetings_join.to_string(),
        Say::Open => t.plaque_open.to_string(),
        Say::Reveal => t.tray_reveal.to_string(),
        Say::Eject => t.plaque_eject.to_string(),
        Say::ToTray => t.plaque_to_tray.to_string(),
        Say::Done => t.done.to_string(),
    }
}

/// То же от сегодняшнего времени.
pub fn plaque_text(lang: Lang, say: &crate::plaque::Say) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default();
    plaque_text_at(lang, say, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_words() {
        assert_eq!(clip_size(1280, 720), "1280 × 720");
        assert_eq!(clip_image(Lang::Ru, 1280, 720), "Картинка · 1280 × 720");
        assert_eq!(clip_image(Lang::En, 1280, 720), "Image · 1280 × 720");
        assert_eq!(clip_age(Lang::Ru, 0), "сейчас");
        assert_eq!(clip_age(Lang::En, 59), "now");
        assert_eq!(clip_age(Lang::Ru, 60), "1 мин");
        assert_eq!(clip_age(Lang::En, 2 * 60 + 59), "2 min");
        assert_eq!(clip_age(Lang::Ru, 3599), "59 мин");
        assert_eq!(clip_age(Lang::Ru, 3600), "1 ч");
        assert_eq!(clip_age(Lang::En, 5 * 3600 + 1800), "5 h");
        assert_eq!(
            clip_fix_tip(Lang::Ru, true, "Привет, как дела?"),
            "Похоже на русский текст, набранный в английской раскладке: «Привет, как дела?»"
        );
        assert_eq!(clip_fix_tip(Lang::En, false, "Hello"), "Looks like English typed with the Russian layout: “Hello”");
    }

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

    /// Слова плашек по дизайну 2.4.0, на обоих языках; у каждого значения подставляется своё.
    #[test]
    fn plaque_words_in_both_languages() {
        use crate::plaque::Say;
        let words = |lang: Lang, say: Say| plaque_text_at(lang, &say, 10_000);
        let gib = 1024u64 * 1024 * 1024;
        let cases: Vec<(Say, &str, &str)> = vec![
            (Say::Connected("Headphones".into()), "Headphones: подключено", "Headphones: connected"),
            (Say::BtCharge(Some(80)), "Bluetooth · заряд 80 %", "Bluetooth · battery 80%"),
            (Say::BtCharge(None), "Bluetooth", "Bluetooth"),
            (Say::Low("Mouse".into()), "Mouse: разряжается", "Mouse: running low"),
            (Say::Battery(12), "Заряд 12 %", "Battery 12%"),
            (Say::Usb("DEMO".into()), "Флешка «DEMO»", "USB drive “DEMO”"),
            (Say::UsbIn(32 * gib), "32 ГБ · вставлена", "32 GB · plugged in"),
            (Say::Safe, "Можно вынимать", "Safe to remove"),
            (Say::Busy, "Не извлечь: диск занят", "Can’t eject: the drive is busy"),
            (Say::Ejecting, "Извлекаю…", "Ejecting…"),
            (Say::Gone, "Флешку уже вынули", "The drive is already gone"),
            (Say::BuildFailed("demo/site".into()), "demo/site: сборка упала", "demo/site: build failed"),
            (Say::BuildPassed("demo/site".into()), "demo/site: сборка прошла", "demo/site: build passed"),
            (Say::Downloaded("report.pdf".into()), "report.pdf: скачано", "report.pdf: downloaded"),
            (Say::DownloadedMany(3), "Скачано файлов: 3", "3 files downloaded"),
            (Say::InDownloads(1_258_291), "1,2 МБ · Загрузки", "1.2 MB · Downloads"),
            (Say::Failed("report.pdf".into()), "Не получилось: report.pdf", "Failed: report.pdf"),
            (Say::Open, "Открыть", "Open"),
            (Say::Reveal, "Показать в папке", "Show in folder"),
            (Say::Eject, "Извлечь", "Eject"),
            (Say::ToTray, "В лоток", "To tray"),
            (Say::Done, "Готово", "Done"),
            (Say::TimerDone(None), "Таймер дошёл", "Timer is up"),
            (Say::TimerDone(Some("tea".into())), "Таймер «tea» дошёл", "Timer “tea” is up"),
            (Say::None, "", ""),
        ];
        for (say, ru, en) in cases {
            assert_eq!(words(Lang::Ru, say.clone()), ru, "{say:?}");
            assert_eq!(words(Lang::En, say.clone()), en, "{say:?}");
        }
        assert_eq!(plaque_size(Lang::Ru, 5 * gib / 2), "2,5 ГБ");
        assert_eq!(plaque_size(Lang::En, 512), "512 B");
        assert_eq!(plaque_size(Lang::En, 10 * 1024 * 1024), "10 MB", "«10.0 MB» без нуля");
    }

    #[test]
    fn the_age_of_a_run_is_counted_when_the_plaque_is_shown() {
        use crate::plaque::Say;
        let run = |branch: &str, at: Option<i64>| Say::RunAge { branch: branch.into(), at };
        // Ветка и «сколько назад» — через « · »; чего нет — того нет.
        assert_eq!(plaque_text_at(Lang::Ru, &run("main", Some(1000)), 1000 + 5 * 60), "main · 5 мин назад");
        assert_eq!(plaque_text_at(Lang::En, &run("main", Some(1000)), 1000 + 30), "main · just now");
        assert_eq!(plaque_text_at(Lang::En, &run("", Some(1000)), 1000 + 7200), "2 h ago");
        assert_eq!(plaque_text_at(Lang::En, &run("main", None), 5), "main");
        assert_eq!(plaque_text_at(Lang::En, &run("", None), 5), "");
    }

    /// «Встречи» (2.5.0): слова карточки, плашки и поиска на обоих языках; полка «День».
    #[test]
    fn meetings_words_in_both_languages() {
        use crate::ics::Service;
        // Сколько до встречи или до её конца.
        for (minutes, ru, en) in [
            (1, "1 мин", "1 min"),
            (12, "12 мин", "12 min"),
            (60, "1 ч", "1 h"),
            (80, "1 ч 20 мин", "1 h 20 min"),
            (125, "2 ч 5 мин", "2 h 5 min"),
        ] {
            assert_eq!(span(Lang::Ru, minutes), ru);
            assert_eq!(span(Lang::En, minutes), en);
        }
        assert_eq!(
            (meeting_in(Lang::Ru, 12).as_str(), meeting_in(Lang::En, 12).as_str()),
            ("через 12 мин", "in 12 min")
        );
        assert_eq!(meeting_in(Lang::Ru, 80), "через 1 ч 20 мин");
        assert_eq!(
            (meeting_left(Lang::Ru, 20).as_str(), meeting_left(Lang::En, 20).as_str()),
            ("идёт · ещё 20 мин", "now · 20 min left")
        );
        assert_eq!(meeting_tomorrow(tr(Lang::Ru), "09:30", "Планёрка"), "Завтра 09:30 · Планёрка");
        assert_eq!(meeting_tomorrow(tr(Lang::En), "09:30", "Stand-up"), "Tomorrow 09:30 · Stand-up");
        assert_eq!(
            (meetings_calendar(Lang::Ru, "Работа").as_str(), meetings_calendar(Lang::En, "Work").as_str()),
            ("Календарь «Работа»", "“Work” calendar")
        );
        assert_eq!(
            (meetings_calendars_n(Lang::Ru, 2).as_str(), meetings_calendars_n(Lang::En, 2).as_str()),
            ("Календарей: 2", "2 calendars")
        );
        assert_eq!(
            (meetings_remove(Lang::Ru, "Работа").as_str(), meetings_remove(Lang::En, "Work").as_str()),
            ("Убрать календарь «Работа»", "Remove the “Work” calendar")
        );
        assert_eq!(
            (search_join(Lang::Ru, "Планёрка").as_str(), search_join(Lang::En, "Stand-up").as_str()),
            ("Подключиться к «Планёрка»", "Join “Stand-up”")
        );
        // Службы: свои имена, а Телемост — по языку.
        for (service, name) in [
            (Service::Zoom, "Zoom"),
            (Service::Meet, "Meet"),
            (Service::Teams, "Teams"),
            (Service::Jazz, "Jazz"),
            (Service::Jitsi, "Jitsi"),
            (Service::Webex, "Webex"),
            (Service::Whereby, "Whereby"),
            (Service::Discord, "Discord"),
        ] {
            assert_eq!((service_name(Lang::Ru, service), service_name(Lang::En, service)), (name, name));
        }
        assert_eq!(
            (service_name(Lang::Ru, Service::Telemost), service_name(Lang::En, Service::Telemost)),
            ("Телемост", "Telemost")
        );
        // Плашка: название режется по шаблону, а у встречи без названия — «Без названия».
        let say = |title: &str| crate::plaque::Say::MeetingIn { title: title.into(), start: 1000 };
        assert_eq!(plaque_text_at(Lang::Ru, &say("Планёрка"), 1000 - 120), "Планёрка через 2 мин");
        assert_eq!(plaque_text_at(Lang::En, &say("Stand-up"), 1000 - 61), "Stand-up in 2 min");
        assert_eq!(plaque_text_at(Lang::En, &say("Stand-up"), 1000 - 60), "Stand-up in 1 min");
        assert_eq!(plaque_text_at(Lang::En, &say("Stand-up"), 1000 + 5), "Stand-up in 1 min", "началась — не «0 мин»");
        assert_eq!(plaque_text_at(Lang::Ru, &say(""), 1000 - 90), "Без названия через 2 мин");
        assert_eq!(
            plaque_text_at(Lang::Ru, &say("{} через {}"), 1000 - 60),
            "{} через {} через 1 мин",
            "фигурные скобки в названии — не шаблон"
        );
        // Полка «День» переводится, как остальные готовые.
        assert_eq!((shelf_title(tr(Lang::Ru), "@day"), shelf_title(tr(Lang::En), "@day")), ("День", "Today"));
        assert_eq!(shelf_title(tr(Lang::En), "Мой стол"), "Мой стол");
        // Тексты подключения: ничего не пропущено ни на одном языке.
        for lang in [Lang::Ru, Lang::En] {
            let t = tr(lang);
            for text in [
                t.meetings,
                t.shelf_day,
                t.meetings_join,
                t.meetings_later,
                t.meetings_none_today,
                t.meetings_tomorrow,
                t.meetings_link,
                t.meetings_add,
                t.meetings_refresh,
                t.meetings_calendars,
                t.meetings_add_calendar,
                t.meetings_failed,
                t.meetings_not_calendar,
                t.meetings_big,
                t.meetings_loading,
                t.meetings_empty,
                t.meetings_google,
                t.meetings_outlook,
                t.meetings_link_bad,
                t.meetings_how,
                t.plaque_meeting,
            ] {
                assert!(!text.is_empty());
            }
            assert_eq!(t.plaque_meeting.matches("{}").count(), 2, "название и число минут");
        }
        let (ru, en) = (tr(Lang::Ru), tr(Lang::En));
        assert_eq!(ru.meetings_google, "Google: Настройки календаря → Секретный адрес в формате iCal");
        assert_eq!(
            ru.meetings_outlook,
            "Outlook: Параметры → Календарь → Общие календари → Опубликовать календарь → ICS"
        );
        assert_eq!(en.meetings_google, "Google: Calendar settings → Secret address in iCal format");
        assert_eq!(en.meetings_outlook, "Outlook: Settings → Calendar → Shared calendars → Publish a calendar → ICS");
    }
}
