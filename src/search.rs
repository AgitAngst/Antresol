// Поиск действий: чистые функции без окна — совпадение запроса с названиями, порядок найденного, подсветка, калькулятор
// и таймер из запроса. Окно (`app.rs`) только рисует то, что посчитано здесь, и выполняет выбранное.

use anvil_ui::Lang;

/// Сколько строк показывает поиск.
pub const SHOWN: usize = 7;
/// Сколько недавних действий помнит программа (показывается не больше [`SHOWN`]: часть могла пропасть из списка).
pub const KEEP_RECENT: usize = 12;
/// Наибольшее число минут таймера из запроса.
pub const MAX_MINUTES: u32 = 999;
/// Имя таймера из запроса не длиннее, в символах (как имя сцены или полки: иначе оно не влезет в карточку).
pub const MAX_TIMER_NAME: usize = 24;
/// Запрос длиннее калькулятор не разбирает (вложенные скобки не должны переполнить стек).
const MAX_EXPR: usize = 200;

// ─── Совпадение ─────────────────────────────────────────────────────────────

/// Буква для сравнения: без регистра, «ё» = «е». Одна буква в одну, поэтому места в строке при этом не съезжают.
fn norm_char(c: char) -> char {
    let c = c.to_lowercase().next().unwrap_or(c);
    if c == 'ё' { 'е' } else { c }
}

fn norm(s: &str) -> Vec<char> {
    s.chars().map(norm_char).collect()
}

/// Слова запроса через пробел, в виде, в котором их сравнивают.
fn words(query: &str) -> Vec<Vec<char>> {
    query.split_whitespace().map(norm).collect()
}

/// Где слово стоит в названии: 0 — с начала названия, 1 — с начала слова, 2 — внутри слова.
fn place_of(title: &[char], at: usize) -> u8 {
    if at == 0 {
        0
    } else if title.get(at - 1).is_some_and(|c| !c.is_alphanumeric()) {
        1
    } else {
        2
    }
}

/// Лучшее место слова в названии: `(как стоит, с какой буквы)`; нет — `None`.
fn best_in(title: &[char], word: &[char]) -> Option<(u8, usize)> {
    if word.is_empty() || word.len() > title.len() {
        return None;
    }
    (0..=title.len() - word.len())
        .filter(|&at| title[at..at + word.len()] == *word)
        .map(|at| (place_of(title, at), at))
        .min()
}

/// Как слово нашлось у действия: 0–2 — в названии (см. [`place_of`]), 3 — только в словах для поиска; нет — `None`.
fn word_rank(title: &[char], keywords: &[char], word: &[char]) -> Option<u8> {
    if let Some((rank, _)) = best_in(title, word) {
        return Some(rank);
    }
    best_in(keywords, word).map(|_| 3)
}

/// Найденное по порядку: каждое слово запроса (через пробел) должно найтись в названии или в словах действия
/// (`items`: название и слова для поиска). Порядок — по худшему из слов: с начала названия, с начала слова, внутри
/// слова, только в словах; при равенстве — недавние выше (`recent`: номера в `items`, самое недавнее первым); дальше — как
/// шли в `items`. Пустой запрос ничего не находит. Ответ — номера в `items`.
pub fn find(query: &str, items: &[(&str, &str)], recent: &[usize]) -> Vec<usize> {
    let words = words(query);
    if words.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<(u8, usize, usize)> = Vec::new();
    for (i, (title, keywords)) in items.iter().enumerate() {
        let (title, keywords) = (norm(title), norm(keywords));
        let ranks: Option<Vec<u8>> = words.iter().map(|w| word_rank(&title, &keywords, w)).collect();
        if let Some(worst) = ranks.and_then(|r| r.into_iter().max()) {
            hits.push((worst, recent.iter().position(|&r| r == i).unwrap_or(usize::MAX), i));
        }
    }
    hits.sort_unstable();
    hits.into_iter().map(|(_, _, i)| i).collect()
}

/// Куда в названии попали слова запроса: куски `(с, по)` в буквах (не в байтах), по порядку и без пересечений — их
/// выделяют в строке. Слово, что нашлось только в словах для поиска, ничего не выделяет.
pub fn marks(query: &str, title: &str) -> Vec<(usize, usize)> {
    let title = norm(title);
    let mut spans: Vec<(usize, usize)> =
        words(query).iter().filter_map(|w| best_in(&title, w).map(|(_, at)| (at, at + w.len()))).collect();
    spans.sort_unstable();
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (from, to) in spans {
        match out.last_mut() {
            Some(last) if from <= last.1 => last.1 = last.1.max(to),
            _ => out.push((from, to)),
        }
    }
    out
}

// ─── Калькулятор ────────────────────────────────────────────────────────────

/// Разбор арифметики сверху вниз: `+ − * /`, скобки, минус перед числом. `eval` и чужих разборщиков здесь нет: на входе
/// только цифры, знаки, пробелы и скобки.
struct Parser<'a> {
    src: &'a [char],
    at: usize,
    /// Сколько действий между числами встретилось (минус перед числом не считается).
    ops: usize,
    /// Глубина скобок и знаков перед числом: больше [`MAX_DEPTH`] — не разбираем.
    depth: usize,
    /// Язык интерфейса: по-русски запятая — десятичная («1,5»), по-английски — разделитель тысяч («1,000»).
    lang: Lang,
}

/// Глубже скобки и знаки подряд не разбираются: стек не должен переполниться от длинного запроса.
const MAX_DEPTH: usize = 64;
/// Сколько значащих цифр в ответе: дальше у двойной точности уже шум (`0,1 + 0,2` — `0,30000000000000004`).
const DIGITS: usize = 12;
/// Разность слагаемых меньше этой доли от них самих — они взаимно уничтожились: остаток есть шум точности, а не ответ
/// (`0,1 + 0,2 − 0,3` — это 0, а не `5,5e-17`).
const CANCELS: f64 = 1e-13;

impl Parser<'_> {
    /// Следующая буква, пробелы пропущены.
    fn peek(&mut self) -> Option<char> {
        while self.src.get(self.at).is_some_and(|c| c.is_whitespace()) {
            self.at += 1;
        }
        self.src.get(self.at).copied()
    }

    /// Слагаемые: `a + b − c`.
    fn sum(&mut self) -> Option<f64> {
        let mut value = self.product()?;
        while let Some(op @ ('+' | '-' | '−')) = self.peek() {
            self.at += 1;
            self.ops += 1;
            let rhs = self.product()?;
            let scale = value.abs().max(rhs.abs());
            value = if op == '+' { value + rhs } else { value - rhs };
            value.is_finite().then_some(())?;
            if value.abs() <= scale * CANCELS {
                value = 0.0;
            }
        }
        Some(value)
    }

    /// Множители: `a * b / c`.
    fn product(&mut self) -> Option<f64> {
        let mut value = self.unary()?;
        while let Some(op @ ('*' | '/')) = self.peek() {
            self.at += 1;
            self.ops += 1;
            let rhs = self.unary()?;
            if op == '*' {
                value *= rhs;
            } else if rhs == 0.0 {
                return None;
            } else {
                value /= rhs;
            }
            value.is_finite().then_some(())?;
        }
        Some(value)
    }

    /// Минус и плюс перед числом или скобкой.
    fn unary(&mut self) -> Option<f64> {
        let Some(sign @ ('-' | '−' | '+')) = self.peek() else { return self.primary() };
        self.at += 1;
        self.depth += 1;
        let inner = if self.depth < MAX_DEPTH { self.unary() } else { None };
        self.depth -= 1;
        inner.map(|v| if sign == '+' { v } else { -v })
    }

    /// Число или выражение в скобках.
    fn primary(&mut self) -> Option<f64> {
        if self.peek()? != '(' {
            return self.number();
        }
        self.at += 1;
        self.depth += 1;
        let inner = if self.depth < MAX_DEPTH { self.sum() } else { None };
        self.depth -= 1;
        let value = inner?;
        if self.peek()? != ')' {
            return None;
        }
        self.at += 1;
        Some(value)
    }

    /// Сколько цифр подряд — и пройти их.
    fn digits(&mut self) -> usize {
        let from = self.at;
        while self.src.get(self.at).is_some_and(|c| c.is_ascii_digit()) {
            self.at += 1;
        }
        self.at - from
    }

    /// `12`, `3,5`, `3.5` (по-русски запятая и точка — десятичные) и `1,000.5` (по-английски запятая — разделитель
    /// тысяч: одна–три цифры, а после каждой запятой ровно три; десятичная только точка). Запятая не там, где ей место
    /// («1,5» по-английски, «1000,000»), — не число.
    fn number(&mut self) -> Option<f64> {
        let from = self.at;
        let int_len = self.digits();
        if int_len == 0 {
            return None;
        }
        let mut text: String = self.src[from..self.at].iter().collect();
        if self.lang == Lang::En && self.src.get(self.at) == Some(&',') {
            if int_len > 3 {
                return None;
            }
            while self.src.get(self.at) == Some(&',') {
                self.at += 1;
                let group = self.at;
                if self.digits() != 3 {
                    return None;
                }
                text.extend(&self.src[group..self.at]);
            }
        }
        let point = match self.lang {
            Lang::Ru => matches!(self.src.get(self.at), Some('.' | ',')),
            Lang::En => self.src.get(self.at) == Some(&'.'),
        };
        if point {
            self.at += 1;
            let fraction = self.at;
            if self.digits() == 0 {
                return None;
            }
            text.push('.');
            text.extend(&self.src[fraction..self.at]);
        }
        text.parse().ok()
    }
}

/// Запрос — арифметика: только цифры, `+ − * /`, скобки, пробелы и `,` / `.`, и хоть одно действие между числами
/// (`-5` и `(5)` не считаются). Запятая в числе — по языку (`lang`): по-русски десятичная, по-английски разделитель
/// тысяч. Ответ — значение; деление на ноль, неверные скобки, два числа подряд и переполнение (от 1e15) — `None`.
pub fn calc(query: &str, lang: Lang) -> Option<f64> {
    let src: Vec<char> = query.trim().chars().collect();
    let allowed = |c: &char| c.is_ascii_digit() || c.is_whitespace() || "+-−*/(),.".contains(*c);
    if src.is_empty() || src.len() > MAX_EXPR || !src.iter().all(allowed) {
        return None;
    }
    let mut parser = Parser { src: &src, at: 0, ops: 0, depth: 0, lang };
    let value = parser.sum()?;
    // Всё прочитано: после выражения остались лишь пробелы.
    parser.peek();
    (parser.at == src.len() && parser.ops > 0 && value.abs() < 1e15).then_some(value)
}

/// Число словами языка: до [`DIGITS`] значащих цифр, без хвоста нулей — `7`, `3,5` (RU), `3.5` (EN), `0,333333333333`.
/// Совсем малое (меньше 1e-6) — с порядком: `3,33333333333e-7`, а не `0`.
pub fn number(value: f64, lang: Lang) -> String {
    if value == 0.0 || !value.is_finite() {
        return "0".into();
    }
    // Двенадцать значащих цифр кладёт сам формат: `3.33333333333e-7`; ниже — та же запись без порядка (если можно).
    let sci = format!("{value:.prec$e}", prec = DIGITS - 1);
    let (mantissa, exp) = sci.split_once('e').unwrap_or((sci.as_str(), "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let all: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let digits = all.trim_end_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let sign = if value < 0.0 { "-" } else { "" };
    let body = if exp < -6 {
        match digits.split_at(1) {
            (first, "") => format!("{first}e{exp}"),
            (first, rest) => format!("{first}.{rest}e{exp}"),
        }
    } else if exp < 0 {
        format!("0.{}{digits}", "0".repeat((-exp - 1) as usize))
    } else {
        let whole = exp as usize + 1;
        if digits.len() <= whole {
            format!("{digits}{}", "0".repeat(whole - digits.len()))
        } else {
            format!("{}.{}", &digits[..whole], &digits[whole..])
        }
    };
    let text = format!("{sign}{body}");
    match lang {
        Lang::Ru => text.replace('.', ","),
        Lang::En => text,
    }
}

// ─── Таймер ─────────────────────────────────────────────────────────────────

/// Таймер из запроса: «чай 5» — `чай` и 5 минут; «5 мин» — без имени.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimerQuery {
    pub name: Option<String>,
    pub minutes: u32,
}

/// Запрос — таймер: «<слово> <число>» или «<число> мин» (число 1–999; после числа можно «мин», «min», «м», «m»). Имя —
/// слова до числа: буквы и знаки без цифр, хоть одна буква, не длиннее [`MAX_TIMER_NAME`].
pub fn timer(query: &str) -> Option<TimerQuery> {
    let chars: Vec<char> = query.trim().chars().collect();
    // Слово после числа («5 мин», «5min»): отрезаем, но только если перед ним цифра.
    let mut len = chars.len();
    let mut unit = false;
    for u in ["мин", "min", "м", "m"] {
        let k = u.chars().count();
        if chars.len() > k && chars[chars.len() - k..].iter().collect::<String>().to_lowercase() == u {
            let mut head = chars.len() - k;
            while head > 0 && chars[head - 1].is_whitespace() {
                head -= 1;
            }
            if head > 0 && chars[head - 1].is_ascii_digit() {
                (len, unit) = (head, true);
                break;
            }
        }
    }
    let rest = &chars[..len];
    let digits = rest.iter().rev().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || digits > 3 {
        return None;
    }
    let split = rest.len() - digits;
    let minutes: u32 = rest[split..].iter().collect::<String>().parse().ok()?;
    if !(1..=MAX_MINUTES).contains(&minutes) {
        return None;
    }
    let before = &rest[..split];
    let name: String = before.iter().collect::<String>().trim().to_string();
    if name.is_empty() {
        // Только число — таймер, если после него сказано «мин».
        return unit.then_some(TimerQuery { name: None, minutes });
    }
    // Слова отделены от числа пробелом, без цифр, и среди них есть буква («чай5» и «2+2 5» — не таймеры).
    let spaced = before.last().is_some_and(|c| c.is_whitespace());
    if !spaced || !name.chars().any(char::is_alphabetic) || name.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    let name: String = name.chars().take(MAX_TIMER_NAME).collect();
    Some(TimerQuery { name: Some(name.trim().to_string()), minutes })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles<'a>(items: &[(&'a str, &'a str)], found: &[usize]) -> Vec<&'a str> {
        found.iter().map(|&i| items[i].0).collect()
    }

    #[test]
    fn matching_ignores_case_and_yo() {
        let items = [("Ночной свет: включить", ""), ("Зелёный чай", ""), ("Экран", "")];
        assert_eq!(find("НОЧН", &items, &[]), [0]);
        assert_eq!(find("зеленый", &items, &[]), [1], "«е» находит «ё»");
        assert_eq!(find("ЗЕЛЁНЫЙ", &items, &[]), [1], "«Ё» находит «ё»");
        assert_eq!(find("  ", &items, &[]), Vec::<usize>::new(), "пустой запрос ничего не находит");
        assert_eq!(find("", &items, &[]), Vec::<usize>::new());
    }

    #[test]
    fn every_word_must_be_found_in_the_title_or_the_words() {
        let items = [("Ночной свет: включить", "night warm"), ("Яркость: 50 %", "brightness"), ("Свет в комнате", "")];
        assert_eq!(find("свет ночной", &items, &[]), [0], "оба слова, в любом порядке");
        assert_eq!(find("night", &items, &[]), [0], "слова для поиска — другой язык");
        assert_eq!(find("ночной night", &items, &[]), [0], "одно в названии, другое в словах");
        assert_eq!(find("свет 50", &items, &[]), Vec::<usize>::new(), "второго слова нигде нет");
        assert_eq!(find("яркость 50", &items, &[]), [1]);
    }

    #[test]
    fn order_is_title_start_then_word_start_then_inside_then_words() {
        let items = [
            ("Подсветка", "light"), // «свет» внутри слова
            ("Выключить свет", ""), // с начала слова
            ("Свет и звук", ""),    // с начала названия
            ("Яркость", "свет"),    // только в словах
        ];
        assert_eq!(
            titles(&items, &find("свет", &items, &[])),
            ["Свет и звук", "Выключить свет", "Подсветка", "Яркость"]
        );
    }

    #[test]
    fn recent_come_first_at_equal_rank_then_the_given_order() {
        let items =
            [("Экран: 1920 × 1080 · 60 Гц", ""), ("Экран: 1920 × 1080 · 144 Гц", ""), ("Экран: 1280 × 720", "")];
        assert_eq!(find("экран", &items, &[]), [0, 1, 2], "нет недавних — как шли");
        assert_eq!(find("экран", &items, &[2, 1]), [2, 1, 0], "недавние выше, самое недавнее первым");
        // Недавнее не перебивает лучшее совпадение.
        let items = [("Свет и звук", ""), ("Выключить свет", "")];
        assert_eq!(find("свет", &items, &[1]), [0, 1]);
    }

    #[test]
    fn marks_point_at_the_words_in_letters() {
        assert_eq!(marks("1080", "Экран: 1920 × 1080 · 144 Гц"), [(14, 18)]);
        assert_eq!(marks("ночн свет", "Ночной свет: включить"), [(0, 4), (7, 11)]);
        assert_eq!(marks("зелё", "Зелёный"), [(0, 4)], "«ё» и «е» — одна буква");
        assert_eq!(marks("елка", "Ёлка"), [(0, 4)]);
        // Слова пересекаются — один кусок; слова нет в названии — ничего.
        assert_eq!(marks("ноч ночной", "Ночной свет"), [(0, 6)]);
        assert!(marks("night", "Ночной свет").is_empty());
        assert!(marks("", "Ночной свет").is_empty());
    }

    #[test]
    fn calculator_computes_with_its_own_parser() {
        let c = |q: &str| calc(q, Lang::Ru);
        assert_eq!(c("2*3,5"), Some(7.0));
        assert_eq!(c("2*3.5"), Some(7.0));
        assert_eq!(c("1 + 2 * 3"), Some(7.0), "умножение раньше сложения");
        assert_eq!(c("(1 + 2) * 3"), Some(9.0));
        assert_eq!(c("10 / 4"), Some(2.5));
        assert_eq!(c("10 − 4"), Some(6.0), "типографский минус");
        assert_eq!(c("-3 + 5"), Some(2.0), "минус перед числом");
        assert_eq!(c("2 * -3"), Some(-6.0));
        assert_eq!(c("2 - -3"), Some(5.0));
        assert_eq!(c("((2))*(3)"), Some(6.0));
        assert_eq!(c("100/3"), Some(100.0 / 3.0));
        // Не арифметика.
        assert_eq!(c("1080"), None, "одно число — не выражение");
        assert_eq!(c("-5"), None, "минус перед числом — не действие");
        assert_eq!(c("(5)"), None);
        assert_eq!(c("2+"), None);
        assert_eq!(c("*2"), None);
        assert_eq!(c("(2+3"), None);
        assert_eq!(c("2+3)"), None);
        assert_eq!(c("1.2.3+1"), None);
        assert_eq!(c("3.+1"), None);
        assert_eq!(c(".5+1"), None);
        assert_eq!(c("2 3"), None, "два числа подряд — не выражение");
        assert_eq!(c("2 3 + 1"), None);
        assert_eq!(c("3 ,5 + 1"), None, "пробел внутри числа");
        assert_eq!(c("2x+3"), None);
        assert_eq!(c("чай 5"), None);
        assert_eq!(c(""), None);
        assert_eq!(c("2(3)"), None, "умножения без знака нет");
    }

    #[test]
    fn calculator_gives_nothing_for_zero_division_and_overflow() {
        for lang in [Lang::Ru, Lang::En] {
            let calc = |q: &str| calc(q, lang);
            assert_eq!(calc("1/0"), None);
            assert_eq!(calc("5/(2-2)"), None);
            assert_eq!(calc("0*(1/0)"), None);
            assert_eq!(calc("99999999999999*99999999999999"), None, "переполнение");
            assert_eq!(calc("1e3+1"), None, "буква «e» — не число");
            assert_eq!(calc(&format!("{}+1", "9".repeat(400))), None, "слишком длинный");
            let nested = format!("{}1+1{}", "(".repeat(100), ")".repeat(100));
            assert_eq!(calc(&nested), None, "скобок слишком много — без падения");
            let minuses = format!("{}1+1", "-".repeat(100));
            assert_eq!(calc(&minuses), None);
            assert_eq!(calc("999999999999999+1"), None, "от 1e15 строки нет");
            assert!(calc("99999999999999+1").is_some());
        }
    }

    /// Запятая в числе — по языку: по-русски десятичная, по-английски разделитель тысяч (и только на своём месте).
    #[test]
    fn a_comma_is_decimal_in_russian_and_thousands_in_english() {
        let (ru, en) = (|q: &str| calc(q, Lang::Ru), |q: &str| calc(q, Lang::En));
        assert_eq!(ru("2,5+1"), Some(3.5));
        assert_eq!(ru("1,000*2"), Some(2.0), "по-русски 1,000 — это единица с тремя нулями после запятой");
        assert_eq!(ru("2.5+1"), Some(3.5), "точка тоже десятичная");
        assert_eq!(ru("1,5,5+1"), None);
        assert_eq!(en("1,000*2"), Some(2000.0));
        assert_eq!(en("12,345,678+1"), Some(12_345_679.0));
        assert_eq!(en("1,000.5*2"), Some(2001.0));
        assert_eq!(en("2.5+1"), Some(3.5));
        assert_eq!(en("1000*2"), Some(2000.0), "без запятых — как есть");
        assert_eq!(en("(1,000+1,000)/4"), Some(500.0));
        // Запятая не на месте — не число, а не молчаливая ошибка в тысячу раз.
        for bad in ["1,5+1", "1,00+1", "1,0000+1", "1000,000+1", ",5+1", "1,+1", "1,000,+1", "1,000.+1", "1.5,5+1"] {
            assert_eq!(en(bad), None, "{bad}");
        }
    }

    /// «Без шума»: двоичная точность не лезет в ответ, а то, что взаимно уничтожилось, — ровно ноль.
    #[test]
    fn answers_have_no_floating_point_noise() {
        let say = |q: &str, lang| calc(q, lang).map(|v| number(v, lang));
        assert_eq!(say("0.1+0.2", Lang::En).as_deref(), Some("0.3"));
        assert_eq!(say("0,1+0,2", Lang::Ru).as_deref(), Some("0,3"));
        assert_eq!(say("0.1+0.2-0.3", Lang::En).as_deref(), Some("0"), "взаимно уничтожилось — не 5.5e-17");
        assert_eq!(say("0,3-0,1-0,2", Lang::Ru).as_deref(), Some("0"));
        assert_eq!(say("0.1*3", Lang::En).as_deref(), Some("0.3"));
        assert_eq!(say("100/3", Lang::En).as_deref(), Some("33.3333333333"), "двенадцать значащих цифр");
        assert_eq!(say("2/3", Lang::Ru).as_deref(), Some("0,666666666667"));
        assert_eq!(say("1/3*3", Lang::En).as_deref(), Some("1"));
        // Шум за двенадцатой цифрой не показан.
        assert_eq!(say("999999999999.99*1", Lang::En).as_deref(), Some("1000000000000"));
        assert_eq!(say("99999999999999.9+0", Lang::En).as_deref(), Some("100000000000000"));
        assert_eq!(say("12345678901*10+7", Lang::En).as_deref(), Some("123456789017"), "двенадцать цифр — как есть");
        assert_eq!(say("123456789012*10+7", Lang::En).as_deref(), Some("1234567890130"), "тринадцатая — округлена");
        // Разность близких чисел, что не шум, остаётся.
        assert_eq!(say("1000.5-1000", Lang::En).as_deref(), Some("0.5"));
        assert_eq!(say("0.000003-0.000001", Lang::En).as_deref(), Some("0.000002"));
        // Очень малый ответ — не «0»: с порядком.
        assert_eq!(say("1/3000000", Lang::En).as_deref(), Some("3.33333333333e-7"));
        assert_eq!(say("1/3000000", Lang::Ru).as_deref(), Some("3,33333333333e-7"));
        assert_eq!(say("1/10000000", Lang::En).as_deref(), Some("1e-7"));
        assert_eq!(say("0-1/10000000", Lang::En).as_deref(), Some("-1e-7"));
        assert_eq!(say("1/1000000", Lang::En).as_deref(), Some("0.000001"), "до 1e-6 — обычной записью");
    }

    #[test]
    fn numbers_read_by_language() {
        assert_eq!(number(7.0, Lang::Ru), "7");
        assert_eq!(number(3.5, Lang::Ru), "3,5");
        assert_eq!(number(3.5, Lang::En), "3.5");
        assert_eq!(number(0.1 + 0.2, Lang::En), "0.3", "без хвоста погрешности");
        assert_eq!(number(100.0 / 3.0, Lang::En), "33.3333333333", "двенадцать значащих цифр");
        assert_eq!(number(-2.0, Lang::Ru), "-2");
        assert_eq!(number(-0.0, Lang::Ru), "0", "«минус ноль» не пишем");
        assert_eq!(number(0.0, Lang::En), "0");
        assert_eq!(number(1234567.0, Lang::Ru), "1234567");
        assert_eq!(number(2000.0, Lang::En), "2000", "без разделителя тысяч: ответ можно вставить дальше");
        assert_eq!(number(1e14, Lang::En), "100000000000000");
        assert_eq!(number(-12.5, Lang::Ru), "-12,5");
        assert_eq!(number(0.000123, Lang::En), "0.000123");
        assert_eq!(number(1.234e-7, Lang::En), "1.234e-7", "меньше 1e-6 — с порядком, а не нулём");
        assert_eq!(number(-1.234e-7, Lang::Ru), "-1,234e-7");
    }

    #[test]
    fn a_word_and_a_number_make_a_timer() {
        let t = |q: &str| timer(q);
        assert_eq!(t("чай 5"), Some(TimerQuery { name: Some("чай".into()), minutes: 5 }));
        assert_eq!(t("  tea   15 "), Some(TimerQuery { name: Some("tea".into()), minutes: 15 }));
        assert_eq!(t("зелёный чай 3"), Some(TimerQuery { name: Some("зелёный чай".into()), minutes: 3 }));
        assert_eq!(t("чай 5 мин"), Some(TimerQuery { name: Some("чай".into()), minutes: 5 }));
        assert_eq!(t("tea 5min"), Some(TimerQuery { name: Some("tea".into()), minutes: 5 }));
        assert_eq!(t("5 мин"), Some(TimerQuery { name: None, minutes: 5 }));
        assert_eq!(t("5мин"), Some(TimerQuery { name: None, minutes: 5 }));
        assert_eq!(t("20 min"), Some(TimerQuery { name: None, minutes: 20 }));
        assert_eq!(t("7 m"), Some(TimerQuery { name: None, minutes: 7 }));
        assert_eq!(t("чай 999").map(|q| q.minutes), Some(999));
        assert_eq!(t("чай 1").map(|q| q.minutes), Some(1));
    }

    #[test]
    fn not_every_query_with_a_number_is_a_timer() {
        let t = |q: &str| timer(q);
        assert_eq!(t("5"), None, "одно число — не таймер");
        assert_eq!(t("1080"), None);
        assert_eq!(t("чай 0"), None);
        assert_eq!(t("чай 1000"), None, "больше 999");
        assert_eq!(t("чай"), None);
        assert_eq!(t("чай5"), None, "число отделено пробелом");
        assert_eq!(t("5 чай"), None, "число — в конце");
        assert_eq!(t("a1 5"), None, "в имени нет цифр");
        assert_eq!(t("2+2 5"), None);
        assert_eq!(t("- 5"), None, "в имени нужна буква");
        assert_eq!(t("0 мин"), None);
        assert_eq!(t("1000 мин"), None);
        assert_eq!(t(""), None);
        assert_eq!(t("   "), None);
    }

    #[test]
    fn a_long_timer_name_is_cut() {
        let long = "а".repeat(40);
        let q = timer(&format!("{long} 5")).expect("таймер");
        assert_eq!(q.name.as_deref().map(|n| n.chars().count()), Some(MAX_TIMER_NAME));
    }

    #[test]
    fn a_calculation_is_not_a_timer_and_a_timer_is_not_a_calculation() {
        for q in ["2*3,5", "10 - 5", "(1+2)*3"] {
            assert!(calc(q, Lang::Ru).is_some() && timer(q).is_none(), "{q}");
        }
        for q in ["чай 5", "tea 10 min", "5 мин"] {
            assert!(timer(q).is_some() && calc(q, Lang::Ru).is_none() && calc(q, Lang::En).is_none(), "{q}");
        }
    }
}
