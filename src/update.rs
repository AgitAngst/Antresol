// Обновления изнутри программы — `anvil-update` из набора Anvil (GitHub Releases, SHA-256, подмена exe, перезапуск).
// Проверка — при запуске и раз в сутки, если включена в настройках; говорит о новой версии баннер над карточками
// (`anvil_update::ui::banner`), управляет — раздел «Обновления» в настройках. В демо и тестах сети нет: проверяльщика нет.

use anvil_update::{Config, State, Updater};

/// Переменная окружения с номером процесса: новая версия после «Перезапустить» ждёт, пока старая закроется — иначе
/// сочетания клавиш ещё держит старая, и у новой они «заняты».
pub const RESTARTED: &str = "ANTRESOL_RESTARTED";

/// Запуск после «Перезапустить»: подождать, пока прежний процесс выйдет (не дольше 10 с), и записать в окружение свой
/// номер — его унаследует следующая версия. Звать в самом начале `main`, пока потоков нет.
pub fn wait_for_previous() {
    if let Some(pid) = std::env::var(RESTARTED).ok().and_then(|v| v.parse::<u32>().ok()) {
        crate::sys::wait_process_exit(pid, 10_000);
    }
    // Потоков ещё нет: менять окружение безопасно.
    unsafe { std::env::set_var(RESTARTED, std::process::id().to_string()) };
}

/// Проверяльщик: в настоящей Windows — да, в демо и тестах — нет (сети там не бывает). Заодно убирает следы прошлого
/// обновления (старый exe рядом).
pub fn new(ctx: &eframe::egui::Context) -> Option<Updater> {
    if crate::sys::isolated() {
        return None;
    }
    anvil_update::cleanup();
    // Только в debug-сборке: `ANTRESOL_UPDATE_AS=2.7.0` — притвориться старой версией, чтобы проверить весь путь
    // обновления на копии программы, не дожидаясь нового выпуска.
    let version = match std::env::var("ANTRESOL_UPDATE_AS") {
        Ok(v) if cfg!(debug_assertions) && !v.is_empty() => v,
        _ => env!("CARGO_PKG_VERSION").to_string(),
    };
    let config = Config::new("antresol", &version, "AgitAngst/Antresol");
    Some(Updater::new(config, ctx.clone()))
}

/// Что умеет кнопка рядом с состоянием в настройках.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Button {
    Check,
    Install,
    Restart,
    Retry,
    /// Идёт проверка или загрузка — кнопки нет.
    None,
}

/// Одна кнопка по состоянию (общий гайдлайн, «О программе»): «Проверить», «Обновить», «Перезапустить», «Повторить».
pub fn button(state: &State) -> Button {
    match state {
        State::Idle | State::UpToDate => Button::Check,
        State::Available(u) if u.asset.is_some() => Button::Install,
        State::Available(_) => Button::Check,
        State::Ready { .. } => Button::Restart,
        State::Failed { update: Some(_), .. } => Button::Retry,
        State::Failed { update: None, .. } => Button::Check,
        State::Checking | State::Downloading { .. } => Button::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update(asset: bool) -> anvil_update::Update {
        anvil_update::Update {
            version: anvil_update::Version::parse("9.9.9").expect("version"),
            tag: "v9.9.9".into(),
            notes: String::new(),
            page: String::new(),
            asset: asset.then(|| anvil_update::Asset {
                name: "antresol-9.9.9-windows-x64.zip".into(),
                url: String::new(),
                size: 1,
            }),
            sums: None,
        }
    }

    #[test]
    fn one_button_per_state() {
        assert_eq!(button(&State::Idle), Button::Check);
        assert_eq!(button(&State::UpToDate), Button::Check);
        assert_eq!(button(&State::Checking), Button::None);
        assert_eq!(button(&State::Available(update(true))), Button::Install);
        assert_eq!(button(&State::Available(update(false))), Button::Check, "архива нет — только проверить снова");
        assert_eq!(button(&State::Downloading { update: update(true), done: 1, total: 2 }), Button::None);
        assert_eq!(button(&State::Ready { update: update(true), exe: Default::default() }), Button::Restart);
        assert_eq!(button(&State::Failed { update: Some(update(true)), error: String::new() }), Button::Retry);
        assert_eq!(button(&State::Failed { update: None, error: String::new() }), Button::Check);
    }

    #[test]
    fn no_updater_in_tests() {
        assert!(new(&eframe::egui::Context::default()).is_none(), "в тестах сети нет");
    }
}
