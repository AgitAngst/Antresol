// «Исправить раскладку»: русский текст, набранный в английской раскладке («Ghbdtn»), и наоборот. Только чистые функции без
// Windows: перевод по клавишам ЙЦУКЕН ↔ QWERTY и распознавание, что текст набран не в той раскладке.
//
// Перевод идёт по клавишам: `q` → `й`, `[` → `х`, `;` → `ж`, `'` → `э`, `,` → `б`, `.` → `ю`, `` ` `` → `ё` (и их пары с
// Shift). Всё остальное — цифры, пробелы, `? ! ( ) -` — остаётся как набрано. Знаки на клавишах букв внутри слова — буквы
// (`cgfcb,j` → «спасибо»); в начале слова — буква, если слово с ней похоже на русское; в конце слова знак остаётся знаком
// («Ghbdtn,» → «Привет,»), кроме `[` и `` ` `` (х, ё) и точки перед словом с маленькой буквы и в конце текста («k.,k. nt,z» →
// «люблю тебя», «Gjplhfdkz.» → «Поздравляю»); запятая после слова, за которой «б» не бывает, выдаёт обычную пунктуацию — тогда
// и точки остаются точками; `/` в конце слова — точка.
//
// Предлагаем, только если текст похож на обычную речь, а перевод — на другой язык заметно больше, чем исходный на свой:
// доля частых пар соседних букв (небольшие списки ниже) у перевода выше, чем у исходного, на запас `MARGIN`. Короткие слова,
// адреса, пути, код (`::`, `{`, `=>`), числа и смесь письменностей не трогаем.

use std::collections::HashMap;
use std::sync::OnceLock;

/// Больше не разбираем: речь, а не файл.
const MAX_CHARS: usize = 1000;
/// Букв во всём тексте не меньше; в тексте из одного слова — не меньше `MIN_LETTERS_ONE_WORD` (`pdf`, `png`, `css`, `ssh`).
const MIN_LETTERS: usize = 3;
const MIN_LETTERS_ONE_WORD: usize = 4;
/// Пар соседних букв в переводе не меньше: по двум-трём парам язык не определить.
const MIN_PAIRS: u32 = 3;
/// Доля частых пар у перевода не меньше…
const MIN_TARGET: f32 = 0.35;
/// …и больше, чем у исходного текста, не меньше чем на столько.
const MARGIN: f32 = 0.25;
/// Букв среди знаков (цифры и пробелы не в счёт) не меньше этой доли: `rgb(79, 122, 18)` — не речь.
const MIN_LETTER_SHARE: f32 = 0.6;

const EN_LOW: &str = "qwertyuiop[]asdfghjkl;'zxcvbnm,.`";
const RU_LOW: &str = "йцукенгшщзхъфывапролджэячсмитьбюё";
const EN_UP: &str = "QWERTYUIOP{}ASDFGHJKL:\"ZXCVBNM<>~";
const RU_UP: &str = "ЙЦУКЕНГШЩЗХЪФЫВАПРОЛДЖЭЯЧСМИТЬБЮЁ";

const VOWELS: &str = "аеёиоуыэюя";

/// Куда переводим.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum To {
    /// Английский набран в русской раскладке — получается английский.
    English,
    /// Русский набран в английской раскладке — получается русский.
    Russian,
}

/// Предложение исправить раскладку: что получится.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fix {
    pub text: String,
    pub to: To,
}

// ─── Частые пары букв ───────────────────────────────────────────────────────
//
// Два списка на язык: частые пары (вес 2) и редкие, но встречающиеся (вес 1). Сняты с большого текста по словарю, чтобы
// не зависеть от того, сколько раз повторяется одно слово; английские — без пар из программистских слов.

const RU_STRONG: &str = "\
    ст но то на ен ов ни ра во ко ро по ор ол ал ли ре ер ом ос ел ан ть от ат ет ва ие ем ка ла ны ти ль ск ам \
    ое ит об ак их ле ик до ту ча ог ся ыл це ля де ир ум ей ию ну ус ыв ки ги го ди да ви ве ив ис ых бо бы же \
    ож ме ми ма мо мы чи чт ше ют ую пр пе па пи ри ру те та тр од ой за се со си ес ез ав ас ад ый им ин ил не \
    нн ло тс ае вы ок ед он кр аз кл че ия сл лю юч бе рв ап";

const RU_WEAK: &str = "\
    ар ая ры оп тк пу вл аю ши ые ты ят ий нт ня ут тв ку оч сп хо ым ек гр дн бр ке са ач зв вк из ьн сь ря ще \
    рт св жи аб ай чн оз ци ег ёт га вн рн лу щи му зн сн ду ён др ии пл еп ах еч ид тн бл ич фи зо уп дк яе би \
    уж ее ба ыт аг еж иг ев гл жд уд уч кт жа дв оя аш ша жн сс уг нк ющ нд зд ьс вс фо уб аж еш чк уз йс сы ур \
    ац зы ям рк иф ву вш бн вт см йт ул ош ун лн лс мн зм рж су иж иц чё дл уе ук ущ кс ои мм ща ды лы вр лк иш \
    ык иа зк кн бу эт яз ср нс шл пк дс ян ыб ащ ём зр дё яю фа ыд дп ял фр ып зи мк зу дя сч яд рм уш жк лё иб \
    ящ ех хр ип бщ яв ох тп зе йд рх рс ью оф ьш нь тл ца ьз тя лч ыш ео мп ьк жб мя йл гд ьц йк эк чо аё еф \
    сц оо жо сб бк её вя вм";

const EN_STRONG: &str = "\
    th he in er an re on at en nd ti es or te of ed is it al ar st to nt ng se ha as ou io le ve co me de hi ri \
    ro ic ne ea ra ce li ch ll be ma si om ur ca el ta la ns di fo ho pe ec pr no ct us ac ot il tr ly nc et ut \
    ss so rs un lo wa ge ie wh ee wi em ad ol rt po we na ul ni ts mo ow pa im mi ai sh ir su id os iv ia am fi \
    ag ci ap ev ex ld pl bl ab mp cl op od if va tt";

const EN_WEAK: &str = "\
    ck sc sp ig ty vi ip pt cr ke ay ov do up lu gr cu bu oc tu pp lt rr ui rm ru ba rd gi ua ds um qu fe ub pu \
    ls oo da ry pi ib rc iz bo sa nu eg au xt av fa rn tc ff ga gs uc fr ew nn tl og gh ht ze ys ph rg gu dr rk \
    du ps wo mu bi dd gn fl oa ud sy mm ei br gg dl fu ks oi yo sl ug ey eo ki gl wn ft ye eb my rl tw ak go sk \
    by ue ef ob ep";

struct Lists {
    ru: HashMap<(char, char), u8>,
    en: HashMap<(char, char), u8>,
}

fn table(strong: &str, weak: &str) -> HashMap<(char, char), u8> {
    let mut map = HashMap::new();
    // Сначала редкие, потом частые: пара из обоих списков — частая.
    for (list, weight) in [(weak, 1), (strong, 2)] {
        for pair in list.split_whitespace() {
            let mut chars = pair.chars();
            if let (Some(a), Some(b)) = (chars.next(), chars.next()) {
                map.insert((a, b), weight);
            }
        }
    }
    map
}

fn lists() -> &'static Lists {
    static LISTS: OnceLock<Lists> = OnceLock::new();
    LISTS.get_or_init(|| Lists { ru: table(RU_STRONG, RU_WEAK), en: table(EN_STRONG, EN_WEAK) })
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Вес пары букв (без учёта регистра): 2 — частая, 1 — редкая, 0 — нет в списках.
fn weight(map: &HashMap<(char, char), u8>, a: char, b: char) -> u8 {
    map.get(&(lower(a), lower(b))).copied().unwrap_or(0)
}

// ─── Клавиши ────────────────────────────────────────────────────────────────

/// Что на этой клавише в русской раскладке (`q` → `й`, `[` → `х`, `Q` → `Й`); цифры и прочие знаки — `None`.
fn key_to_ru(c: char) -> Option<char> {
    for (from, to) in [(EN_LOW, RU_LOW), (EN_UP, RU_UP)] {
        if let Some(i) = from.chars().position(|f| f == c) {
            return to.chars().nth(i);
        }
    }
    None
}

/// Что на этой клавише в английской раскладке (`й` → `q`, `х` → `[`); не буква русской раскладки — `None`.
fn letter_to_key(c: char) -> Option<char> {
    for (from, to) in [(RU_LOW, EN_LOW), (RU_UP, EN_UP)] {
        if let Some(i) = from.chars().position(|f| f == c) {
            return to.chars().nth(i);
        }
    }
    None
}

fn is_ru_letter(c: char) -> bool {
    RU_LOW.contains(c) || RU_UP.contains(c)
}

/// Слова — подряд идущие знаки без пробелов: (начало, конец) в `chars`.
fn tokens(chars: &[char]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        out.push((start, i));
    }
    out
}

/// Парный знак, которым закрывают слово, открытое этим знаком: кавычки и скобки, а не буквы.
fn closer(c: char) -> Option<char> {
    match c {
        '\'' => Some('\''),
        '"' => Some('"'),
        '[' => Some(']'),
        '`' => Some('`'),
        '{' => Some('}'),
        _ => None,
    }
}

/// Слово может начинаться с этой русской буквы перед этой: пара есть в списках, а у б, х, ж — ещё перед гласной, у э, ю, ё —
/// перед согласной (в списках пар «эт», «юг», «бу» мало: слов мало, но каждое частое).
fn word_start_ok(ru: char, next: char, lists: &Lists) -> bool {
    let (r, n) = (lower(ru), lower(next));
    if weight(&lists.ru, r, n) > 0 {
        return true;
    }
    match r {
        'б' | 'х' | 'ж' => VOWELS.contains(n) || "лрв".contains(n),
        'э' | 'ю' | 'ё' => !VOWELS.contains(n) && !"ьъй".contains(n),
        _ => false,
    }
}

/// Одно слово английской раскладки: буквы переводятся по клавишам, знаки в начале и в конце — по правилам из заголовка.
/// `out` и `letter` — то же слово: результат и «этот знак стал буквой»; `dot_is_letter` — точка в конце слова здесь «ю».
fn convert_token(tok: &[char], out: &mut [char], letter: &mut [bool], dot_is_letter: bool, lists: &Lists) {
    let Some(first) = tok.iter().position(|c| c.is_ascii_alphabetic()) else { return };
    let last = tok.iter().rposition(|c| c.is_ascii_alphabetic()).unwrap_or(first);
    // Буквы и всё, что между ними: знаки на клавишах букв — тоже буквы.
    for i in first..=last {
        if let Some(r) = key_to_ru(tok[i]) {
            out[i] = r;
            letter[i] = true;
        }
    }
    // Кавычки и скобки вокруг слова остаются кавычками и скобками.
    let wrapped = tok.len() > 1 && closer(tok[0]) == Some(tok[tok.len() - 1]);
    if !wrapped {
        let mut next = out[first];
        for i in (0..first).rev() {
            let Some(r) = key_to_ru(tok[i]) else { break };
            if !word_start_ok(r, next, lists) {
                break;
            }
            out[i] = r;
            letter[i] = true;
            next = r;
        }
    }
    let mut prev = out[last];
    for i in last + 1..tok.len() {
        let c = tok[i];
        if (c == '[' || c == '`') && !wrapped {
            // «х» и «ё» в конце слова («их», «всё»): `[` и `` ` `` знаком в обычной речи не бывают.
            if let Some(r) = key_to_ru(c).filter(|&r| c == '`' || weight(&lists.ru, prev, r) > 0) {
                out[i] = r;
                letter[i] = true;
                prev = r;
                continue;
            }
        }
        // «ю» в конце слова («люблю»), а не точка: русская точка — на клавише `/`.
        if c == '.' && i == last + 1 && dot_is_letter && weight(&lists.ru, prev, 'ю') > 0 {
            out[i] = 'ю';
            letter[i] = true;
            prev = 'ю';
            continue;
        }
        // Русская точка — на клавише `/`.
        if c == '/' && i == tok.len() - 1 && tok[i - 1].is_ascii_alphabetic() {
            out[i] = '.';
        }
        break;
    }
}

/// Текст с обычной пунктуацией: после слова стоит запятая, точка с запятой или двоеточие, за которыми «б», «ж», «Ж» не бывает
/// («Ghbdtn,» — «тб» в русском нет). Клавишами так не набирают (там запятая — это «б»), поэтому и точки здесь — точки.
fn natural_punctuation(chars: &[char], lists: &Lists) -> bool {
    tokens(chars).into_iter().any(|(start, end)| {
        let tok = &chars[start..end];
        let Some(last) = tok.iter().rposition(|c| c.is_ascii_alphabetic()) else { return false };
        let sign = tok.get(last + 1).copied().filter(|c| matches!(c, ',' | ';' | ':'));
        match (key_to_ru(tok[last]), sign.and_then(key_to_ru)) {
            (Some(prev), Some(letter)) => weight(&lists.ru, prev, letter) == 0,
            _ => false,
        }
    })
}

/// Английская раскладка → русский: результат и пометки «стал буквой» по знакам.
fn latin_to_russian(chars: &[char]) -> (Vec<char>, Vec<bool>) {
    let lists = lists();
    let mut out = chars.to_vec();
    let mut letter = vec![false; chars.len()];
    let natural = natural_punctuation(chars, lists);
    for (start, end) in tokens(chars) {
        // Точка в конце слова — «ю», если после неё слово с маленькой буквы или текст кончился: точку так не ставят.
        let next = chars[end..].iter().find(|c| !c.is_whitespace());
        let dot_is_letter = !natural && next.is_none_or(|c| c.is_ascii_lowercase());
        convert_token(&chars[start..end], &mut out[start..end], &mut letter[start..end], dot_is_letter, lists);
    }
    (out, letter)
}

/// Русская раскладка → английский: каждая русская буква — клавиша, остальное как есть.
fn cyrillic_to_latin(chars: &[char]) -> (Vec<char>, Vec<bool>) {
    let mut out = chars.to_vec();
    let mut letter = vec![false; chars.len()];
    for (i, &c) in chars.iter().enumerate() {
        if let Some(k) = letter_to_key(c) {
            out[i] = k;
            letter[i] = true;
        }
    }
    (out, letter)
}

/// Текст, набранный в английской раскладке, как он вышел бы в русской: «Ghbdtn, rfr ltkf?» → «Привет, как дела?».
#[cfg(test)]
fn to_russian(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    latin_to_russian(&chars).0.into_iter().collect()
}

/// Текст, набранный в русской раскладке, как он вышел бы в английской: «Руддщ цщкдв» → «Hello world».
#[cfg(test)]
fn to_english(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    cyrillic_to_latin(&chars).0.into_iter().collect()
}

// ─── Распознавание ──────────────────────────────────────────────────────────

/// Адрес, путь, код, число, цвет: знаки, каких в речи нет, и их сочетания. Скобки `{ } < > ~` — только в начале слова перед
/// буквой: так выходят заглавные Х, Ъ, Б, Ю, Ё в английской раскладке («{jhjij» — «Хорошо»).
fn looks_technical(chars: &[char]) -> bool {
    const SIGNS: &str = "\\@#$%^&*=+|_";
    if chars.iter().any(|&c| SIGNS.contains(c)) {
        return true;
    }
    let text: String = chars.iter().collect();
    if ["://", "::", "=>", "->", "==", "!="].iter().any(|s| text.contains(s)) {
        return true;
    }
    tokens(chars).into_iter().any(|(start, end)| {
        let tok = &chars[start..end];
        tok.iter()
            .enumerate()
            .any(|(i, &c)| "{}<>~".contains(c) && !(i == 0 && tok.len() > 1 && tok[1].is_ascii_alphabetic()))
    })
}

/// Английский после перевода: знаки на клавишах русских букв — не внутри слова и не перед ним (кроме апострофа и кавычек):
/// `,eath` и `j,tof` — не английские слова.
fn english_plausible(out: &[char], letter: &[bool]) -> bool {
    tokens(out).into_iter().all(|(start, end)| {
        let tok = &out[start..end];
        let Some(last) = tok.iter().rposition(|c| c.is_ascii_alphabetic()) else { return true };
        tok.iter()
            .enumerate()
            .take(last)
            .all(|(i, &c)| !letter[start + i] || c.is_alphabetic() || c == '\'' || c == '"')
    })
}

/// Предложить исправить раскладку: что получится, если текст набран в другой раскладке. `None` — текст на вид нормальный
/// или не речь (адрес, путь, код, число, слишком короткий, смесь письменностей).
pub fn suggest(text: &str) -> Option<Fix> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() > MAX_CHARS || looks_technical(&chars) {
        return None;
    }
    let (mut latin, mut cyrillic) = (0usize, 0usize);
    for &c in &chars {
        if c.is_ascii_alphabetic() {
            latin += 1;
        } else if is_ru_letter(c) {
            cyrillic += 1;
        } else if c.is_alphabetic() {
            // Другая письменность, украинская «і», «é»: раскладки ЙЦУКЕН ↔ QWERTY тут ни при чём.
            return None;
        }
    }
    if latin > 0 && cyrillic > 0 {
        return None;
    }
    let letters = latin + cyrillic;
    let words = tokens(&chars);
    if letters < MIN_LETTERS || (words.len() == 1 && letters < MIN_LETTERS_ONE_WORD) {
        return None;
    }
    let signs = chars.iter().filter(|c| !c.is_whitespace() && !c.is_ascii_digit()).count();
    if (letters as f32) < MIN_LETTER_SHARE * signs as f32 {
        return None;
    }
    let (to, (out, letter)) = if latin > 0 {
        // Слэш — только точкой в конце слова; `camelCase` и `PascalCase` — имена, а не слова.
        let name = words.iter().any(|&(s, e)| {
            let tok = &chars[s..e];
            tok[..tok.len() - 1].contains(&'/')
                || tok.windows(2).any(|w| w[0].is_ascii_lowercase() && w[1].is_ascii_uppercase())
        });
        if name {
            return None;
        }
        (To::Russian, latin_to_russian(&chars))
    } else {
        if chars.contains(&'/') {
            return None;
        }
        let (out, letter) = cyrillic_to_latin(&chars);
        if !english_plausible(&out, &letter) {
            return None;
        }
        (To::English, (out, letter))
    };
    let lists = lists();
    let (source, target) = match to {
        To::Russian => (&lists.en, &lists.ru),
        To::English => (&lists.ru, &lists.en),
    };
    // Пары соседних букв там, где перевод — две буквы подряд; считаем их у исходного текста и у перевода.
    let (mut pairs, mut from_weight, mut to_weight) = (0u32, 0u32, 0u32);
    for i in 0..chars.len().saturating_sub(1) {
        if !(letter[i] && letter[i + 1]) {
            continue;
        }
        let (a, b) = (out[i], out[i + 1]);
        let in_target = match to {
            To::Russian => is_ru_letter(a) && is_ru_letter(b),
            To::English => a.is_ascii_alphabetic() && b.is_ascii_alphabetic(),
        };
        if !in_target {
            continue;
        }
        pairs += 1;
        to_weight += u32::from(weight(target, a, b));
        let (x, y) = (chars[i], chars[i + 1]);
        let in_source = match to {
            To::Russian => x.is_ascii_alphabetic() && y.is_ascii_alphabetic(),
            To::English => is_ru_letter(x) && is_ru_letter(y),
        };
        if in_source {
            from_weight += u32::from(weight(source, x, y));
        }
    }
    if pairs < MIN_PAIRS {
        return None;
    }
    // Вес пары — до 2, поэтому доля — вес, делённый на удвоенное число пар.
    let scale = 2.0 * pairs as f32;
    let (like_to, like_from) = (to_weight as f32 / scale, from_weight as f32 / scale);
    (like_to >= MIN_TARGET && like_to - like_from >= MARGIN).then(|| Fix { text: out.into_iter().collect(), to })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offered(text: &str) -> Option<(String, To)> {
        suggest(text).map(|f| (f.text, f.to))
    }

    /// Русская фраза, как она вышла бы в английской раскладке; английская — как в русской.
    fn typed_in_en(ru: &str) -> String {
        to_english(ru)
    }

    fn typed_in_ru(en: &str) -> String {
        en.chars().map(|c| key_to_ru(c).unwrap_or(c)).collect()
    }

    #[test]
    fn the_tables_are_one_to_one_and_cover_the_keys() {
        for (en, ru) in [(EN_LOW, RU_LOW), (EN_UP, RU_UP)] {
            assert_eq!((en.chars().count(), ru.chars().count()), (33, 33));
            for (e, r) in en.chars().zip(ru.chars()) {
                assert_eq!((key_to_ru(e), letter_to_key(r)), (Some(r), Some(e)), "{e} ↔ {r}");
            }
        }
        assert_eq!(key_to_ru('1'), None);
        assert_eq!(key_to_ru('?'), None, "знаки без букв остаются");
        assert_eq!(letter_to_key('і'), None, "украинской «і» в раскладке нет");
    }

    #[test]
    fn the_lists_are_pairs_of_one_script_without_repeats() {
        let l = lists();
        for (map, ru) in [(&l.ru, true), (&l.en, false)] {
            assert!(map.len() > 200, "списки не пустые: {}", map.len());
            for &(a, b) in map.keys() {
                let ok = if ru {
                    is_ru_letter(a) && is_ru_letter(b)
                } else {
                    a.is_ascii_lowercase() && b.is_ascii_lowercase()
                };
                assert!(ok, "{a}{b}");
                assert_eq!((lower(a), lower(b)), (a, b), "пары — строчными");
            }
        }
        for (strong, weak) in [(RU_STRONG, RU_WEAK), (EN_STRONG, EN_WEAK)] {
            let all: Vec<&str> = strong.split_whitespace().chain(weak.split_whitespace()).collect();
            let mut once = all.clone();
            once.sort_unstable();
            once.dedup();
            assert_eq!(once.len(), all.len(), "пара есть и в частых, и в редких, или дважды");
            assert!(all.iter().all(|p| p.chars().count() == 2));
        }
        assert_eq!((weight(&l.ru, 'С', 'Т'), weight(&l.en, 'T', 'h')), (2, 2), "регистр не важен");
    }

    #[test]
    fn russian_typed_in_the_english_layout_comes_back() {
        assert_eq!(to_russian("ghbdtn"), "привет");
        assert_eq!(to_russian("Ghbdtn, rfr ltkf?"), "Привет, как дела?", "запятая и вопрос остаются знаками");
        assert_eq!(to_russian("GHBDTN"), "ПРИВЕТ");
        assert_eq!(to_russian("Ntcn 123"), "Тест 123", "цифры остаются");
        // Знаки на клавишах букв внутри слова — буквы: ж, б, х, э, ъ.
        assert_eq!(to_russian("gj;fkeqcnf"), "пожалуйста");
        assert_eq!(to_russian("cgfcb,j"), "спасибо");
        assert_eq!(to_russian("[jhjij"), "хорошо");
        assert_eq!(to_russian("j,]zdktybt"), "объявление");
        // В начале слова — буква, если слово с ней похоже на русское: «это», «жизнь», «буфер».
        assert_eq!(to_russian("'nj ntcn"), "это тест");
        assert_eq!(to_russian(";bpym"), "жизнь");
        assert_eq!(to_russian(",eath j,vtyf"), "буфер обмена");
        assert_eq!(to_russian("{jhjij"), "Хорошо", "заглавная Х");
        assert_eq!(to_russian("<eyrth"), "Бункер", "заглавная Б");
        // В конце слова: `[` и `` ` `` — буквы х и ё, `/` — точка, запятая и `?` остаются.
        assert_eq!(to_russian("b[ b ult"), "их и где");
        assert_eq!(to_russian("dc`"), "всё");
        assert_eq!(to_russian("Ghbdtn/"), "Привет.");
        assert_eq!(to_russian("Ghbdtn, ytn"), "Привет, нет");
        // Точка перед словом с маленькой буквы и в конце текста — «ю»; перед заглавной — точка.
        assert_eq!(to_russian("z k.,k. nt,z"), "я люблю тебя");
        assert_eq!(to_russian("Gjplhfdkz."), "Поздравляю");
        assert_eq!(to_russian("yt pyf. nj;t"), "не знаю тоже");
        assert_eq!(to_russian("Ghbdtn. Rfr"), "Привет. Как");
        // Запятая после слова — обычная пунктуация: и точки в таком тексте точки, а не «ю».
        assert_eq!(to_russian("Ghbdtn, rfr ltkf."), "Привет, как дела.");
        assert_eq!(to_russian("Ghbdtn, ytn. rfr ltkf."), "Привет, нет. как дела.");
        // «ю» с такой парой в русском нет («тю» в конце слова): точка остаётся.
        assert_eq!(to_russian("ghbdtn nfr."), "привет так.");
        // Кавычки и скобки вокруг слова остаются; пробелы и переводы строк — как были.
        assert_eq!(to_russian("\"ghbdtn\" [ntcn]"), "\"привет\" [тест]");
        assert_eq!(to_russian("ghbdtn\n  vbh\t"), "привет\n  мир\t");
    }

    #[test]
    fn english_typed_in_the_russian_layout_comes_back() {
        assert_eq!(to_english("Руддщ цщкдв"), "Hello world");
        assert_eq!(to_english("Руддщб цщкдв!"), "Hello, world!", "б — клавиша запятой");
        assert_eq!(to_english("вщтэе"), "don't");
        assert_eq!(to_english("ЙЦУКЕН"), "QWERTY");
        assert_eq!(to_english("Привет, мир 12"), "Ghbdtn, vbh 12", "запятая, цифры и пробелы остаются");
    }

    #[test]
    fn the_wrong_layout_is_offered_with_the_fixed_text() {
        // Образец из задачи: запятая и вопрос остаются знаками.
        assert_eq!(offered("Ghbdtn, rfr ltkf?"), Some(("Привет, как дела?".into(), To::Russian)));
        assert_eq!(offered("ghbdtn"), Some(("привет".into(), To::Russian)));
        assert_eq!(offered("cgfcb,j"), Some(("спасибо".into(), To::Russian)));
        assert_eq!(offered("Ntcn 123"), Some(("Тест 123".into(), To::Russian)), "цифры в счёт не идут");
        assert_eq!(offered("Plhfdcndeqnt"), Some(("Здравствуйте".into(), To::Russian)));
        assert_eq!(offered("lj,hsq ltym"), Some(("добрый день".into(), To::Russian)));
        assert_eq!(offered("{jhjij, ljujdjhbkbcm"), Some(("Хорошо, договорились".into(), To::Russian)));
        assert_eq!(offered("GHBDTN VBH"), Some(("ПРИВЕТ МИР".into(), To::Russian)), "Caps Lock");
        assert_eq!(offered("  ghbdtn vbh\n"), Some(("  привет мир\n".into(), To::Russian)), "пробелы остаются");
        // Наоборот: английский в русской раскладке.
        assert_eq!(offered("Руддщ цщкдв"), Some(("Hello world".into(), To::English)));
        assert_eq!(offered("Руддщб цщкдв!"), Some(("Hello, world!".into(), To::English)));
        assert_eq!(offered("вщтэе дщыу"), Some(("don't lose".into(), To::English)));
    }

    /// Фраза, набранная клавишами не той раскладки, узнаётся и возвращается как была.
    #[test]
    fn phrases_come_back_either_way() {
        for ru in [
            "привет",
            "Спасибо большое",
            "добрый день",
            "Хорошо, договорились",
            "давай встретимся завтра",
            "Мне нужна помощь",
            "Где находится ближайшая аптека",
            "Не получилось подключиться к серверу",
            "Поздравляю",
            "Желаю удачи",
            "Я опоздаю на десять минут",
            "Привет, как дела.",
            "Купи молоко и сыр",
            "Включи музыку погромче",
            "жизнь",
            "объясни пожалуйста",
            "программа",
            "клавиатура",
            "сообщение",
        ] {
            assert_eq!(offered(&typed_in_en(ru)), Some((ru.into(), To::Russian)), "{ru}");
        }
        for en in [
            "Hello, how are you?",
            "Thank you very much",
            "good morning",
            "Please send me the link",
            "Check the network settings",
            "Open in the browser",
            "Meeting is moved to Friday",
            "settings",
            "clipboard history",
            "Don't forget the keys",
        ] {
            assert_eq!(offered(&typed_in_ru(en)), Some((en.into(), To::English)), "{en}");
        }
    }

    #[test]
    fn normal_text_is_not_offered() {
        for text in [
            "Hello, world!",
            "Hello",
            "The quick brown fox jumps over the lazy dog",
            "Thanks, got it!",
            "Meeting tomorrow at 10:00",
            "Clipboard history",
            "Open in the browser",
            "Nothing here yet",
            "Error: file not found",
            "cargo build --release",
            "Привет, как дела?",
            "Спасибо",
            "Отчёт за месяц",
            "Хорошо, договорились",
            "Встреча завтра в 10:00 Комната 3",
            "Пример текста для вставки",
        ] {
            assert_eq!(offered(text), None, "{text}");
        }
    }

    #[test]
    fn short_words_are_not_touched() {
        // Меньше трёх букв — никогда; одно слово короче четырёх — тоже (`pdf`, `png`, `css`, `ssh`, `git`).
        for text in ["ok", "да", "rj", "ghb", "rjl", "pdf", "png", "css", "git", "ntl", "a", "", "   "] {
            assert_eq!(offered(text), None, "{text:?}");
        }
        // Слово из четырёх букв и несколько слов — уже можно.
        assert_eq!(offered("ntcn").map(|(t, _)| t), Some("тест".into()));
        assert_eq!(offered("ntcn ntcn").map(|(t, _)| t), Some("тест тест".into()));
    }

    #[test]
    fn addresses_paths_code_and_numbers_are_not_touched() {
        for text in [
            "https://example.com/ghbdtn",
            "http://localhost:3000/ghbdtn",
            "www.example.com/ghbdtn",
            "user@example.com",
            r"C:\Users\ghbdtn\Documents",
            "/usr/local/bin/ghbdtn",
            "~/ghbdtn/vbh",
            "a/b/c ghbdtn",
            "std::collections::HashMap",
            "fn main() {",
            "{ghbdtn: vbh}",
            "let x = ghbdtn => vbh;",
            "ghbdtn->vbh",
            "a == b ghbdtn",
            "foo_bar_baz",
            "camelCaseName",
            "<div>ghbdtn</div>",
            "rgb(79, 122, 18)",
            "#4f7a12",
            "1234567890",
            "2026-10-27 10:45:30",
            "+7 (999) 123-45-67",
            "192.168.0.1",
            "a3f9c2b7d1e84f6a",
            "SELECT * FROM users WHERE id = 1;",
            "ghbdtn $HOME vbh",
            "100% ghbdtn",
            "Ghbdtn Привет",
            "Привет Hello",
            "Café résumé",
            "こんにちは世界",
            "Привіт, світе",
        ] {
            assert_eq!(offered(text), None, "{text}");
        }
        // Слэш в английской раскладке — только русской точкой в конце слова.
        assert_eq!(offered("Ghbdtn/ rfr ltkf/").map(|(t, _)| t), Some("Привет. как дела.".into()));
        assert_eq!(offered("и/или ntcn"), None);
        // Слишком длинное — не речь.
        assert_eq!(offered(&"ghbdtn ".repeat(200)), None);
    }
}
