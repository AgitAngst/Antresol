// Тексты интерфейса на двух языках. Язык — общий с набором Anvil (`anvil_ui::Lang`).

use anvil_ui::Lang;

pub struct Tr {
    pub display: &'static str,
    pub recommended: &'static str,
    pub favorites: &'static str,
    pub favorites_hint: &'static str,
    pub favorites_full: &'static str,
    pub star_add: &'static str,
    pub star_remove: &'static str,
    pub profile_save: &'static str,
    pub profile_name: &'static str,
    pub profile_already: &'static str,
    pub save: &'static str,
    pub cancel: &'static str,
    pub profiles_settings: &'static str,
    pub profiles_none: &'static str,
    pub change: &'static str,
    pub press_keys: &'static str,
    pub key_busy: &'static str,
    pub key_free: &'static str,
    pub no_key: &'static str,
    pub remove_key: &'static str,
    pub delete: &'static str,
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
    pub cards_title: &'static str,
    pub cards_note: &'static str,
    pub no_cards: &'static str,
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
    pub edit_cards: &'static str,
    pub profile: &'static str,
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
    pub drag: &'static str,
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
    profile_save: "Сохранить текущий экран профилем",
    profile_name: "Имя профиля",
    profile_already: "Этот профиль уже стоит",
    save: "Сохранить",
    cancel: "Отмена",
    profiles_settings: "Профили экрана",
    profiles_none: "Профилей пока нет — «+» в карточке «Экран» сохраняет текущий экран.",
    change: "Изменить…",
    press_keys: "Нажмите сочетание: Ctrl или Alt и клавишу; Esc — отмена",
    key_busy: "Занято другой программой",
    key_free: "Работает",
    no_key: "Без клавиши",
    remove_key: "Убрать клавишу",
    delete: "Удалить профиль",
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
    cards_title: "Карточки панели",
    cards_note: "Выключенные карточки не видны и ничего не опрашивают; буфер обмена запоминает и без карточки.",
    no_cards: "Все карточки выключены — включите нужные в настройках.",
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
    mode_unavailable: "Режима из профиля у монитора нет — он пропущен",
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
    edit_cards: "Изменить карточки",
    profile: "Профиль",
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
    drag: "Перетащите, чтобы поменять порядок",
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
    profile_save: "Save the current display as a profile",
    profile_name: "Profile name",
    profile_already: "This profile is already on",
    save: "Save",
    cancel: "Cancel",
    profiles_settings: "Display profiles",
    profiles_none: "No profiles yet — “+” on the Display card saves the current display.",
    change: "Change…",
    press_keys: "Press a shortcut: Ctrl or Alt and a key; Esc to cancel",
    key_busy: "Taken by another app",
    key_free: "Works",
    no_key: "No shortcut",
    remove_key: "Remove shortcut",
    delete: "Delete profile",
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
    cards_title: "Panel cards",
    cards_note: "Hidden cards are not shown and poll nothing; the clipboard remembers even without its card.",
    no_cards: "All cards are off — turn the ones you need on in settings.",
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
    edit_cards: "Edit cards",
    profile: "Profile",
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
    drag: "Drag to reorder",
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

/// Имя нового профиля, если его не ввели: «Профиль 2».
pub fn profile_default(lang: Lang, n: usize) -> String {
    match lang {
        Lang::Ru => format!("Профиль {n}"),
        Lang::En => format!("Profile {n}"),
    }
}

/// Сколько мониторов профиля сейчас нет.
pub fn monitors_missing(lang: Lang, n: usize) -> String {
    match lang {
        Lang::Ru => format!("Мониторов из профиля нет: {n} — пропущены"),
        Lang::En => format!("Monitors from the profile not found: {n} — skipped"),
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

/// Сочетание уже у профиля.
pub fn key_is_profile(lang: Lang, name: &str) -> String {
    match lang {
        Lang::Ru => format!("Это сочетание у профиля «{name}» — выберите другое"),
        Lang::En => format!("This shortcut belongs to the profile “{name}” — pick another one"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ago_words() {
        assert_eq!(ago(Lang::Ru, 30), "только что");
        assert_eq!(ago(Lang::Ru, 12 * 60), "12 мин назад");
        assert_eq!(ago(Lang::En, 3 * 3600 + 5), "3 h ago");
        assert_eq!(ago(Lang::Ru, 2 * 86400), "2 дн назад");
    }
}
