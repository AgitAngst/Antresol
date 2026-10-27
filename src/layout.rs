// «Исправить раскладку»: русский текст, набранный в английской раскладке («Ghbdtn»), и наоборот. Только чистые функции без
// Windows: перевод по клавишам ЙЦУКЕН ↔ QWERTY и распознавание, что текст набран не в той раскладке.
//
// Перевод идёт по клавишам: `q` → `й`, `[` → `х`, `;` → `ж`, `'` → `э`, `,` → `б`, `.` → `ю`, `` ` `` → `ё` (и их пары с
// Shift). Знаки на клавишах букв внутри слова — буквы (`cgfcb,j` → «спасибо»); в начале слова — буква, если слово с ней
// похоже на русское; в конце слова знак остаётся знаком («Ghbdtn,» → «Привет,»), кроме `[` и `` ` `` (х, ё) и точки перед
// словом с маленькой буквы и в конце текста («k.,k. nt,z» → «люблю тебя», «Gjplhfdkz.» → «Поздравляю»); запятая после слова,
// за которой «б» не бывает, выдаёт обычную пунктуацию — тогда и точки остаются точками.
//
// Русская пунктуация набирается другими клавишами, чем английская (так выходит, когда пишут не в той раскладке): точка —
// `/`, запятая — Shift+`/` (`?`), вопрос — Shift+7 (`&`), двоеточие — Shift+6 (`^`), точка с запятой — Shift+4 (`$`),
// кавычка — Shift+2 (`@`), «№» — Shift+3 (`#`). После слова это знаки: «Ghbdtn? rfr ltkf&» → «Привет, как дела?» (`?` —
// запятая, если за ней идёт ещё слово; последний `?` текста остаётся вопросом, как и в «Ghbdtn, rfr ltkf?», где запятая
// — запятая). В таком тексте запятая, точка с запятой и точка в конце слова — буквы б, ж, ю («масштаб», «муж», «вживую»).
// Обратно: запятая русской раскладки — `?` английской («Рщц фку нщг,» → «How are you?»). Всё остальное — цифры, пробелы,
// `! ( ) -` — остаётся как набрано.
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
/// Одно слово заглавными не длиннее этого — сокращение (`LGTM`, `HTTP`, `UEFI`), а не слово с включённым Caps Lock.
const MAX_ACRONYM: usize = 5;
/// Русская пунктуация, набранная клавишами английской раскладки (`ltkf&`), в английском тексте почти не бывает: тогда
/// сравнивать перевод с исходным не нужно — хватает, чтобы он был похож на русский не меньше чем на столько.
const MIN_TARGET_TYPED: f32 = 0.5;
/// Что может стоять после последней буквы слова: знаки русской пунктуации в английской раскладке и их соседи.
const TAIL: &str = "&^$@?!)/.,;:'\"][";

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

/// Как читать знаки в конце слов текста.
#[derive(Clone, Copy)]
struct Reading {
    /// Точка в конце слова здесь — «ю».
    dot_is_letter: bool,
    /// `?` после слова — запятая русской раскладки (Shift+`/`): за словом идёт ещё слово, а запятая в тексте не обычная.
    question_is_comma: bool,
    /// Текст набран настоящими клавишами (в нём есть `?` вместо запятой или `&`, `^`, `$`): запятая в конце слова — «б», точка
    /// с запятой — «ж», точка — «ю»; обычной пунктуации на этих клавишах там нет.
    real_keys: bool,
}

/// Одно слово английской раскладки: буквы переводятся по клавишам, знаки в начале и в конце — по правилам из заголовка.
/// `out` и `letter` — то же слово: результат и «этот знак стал буквой».
fn convert_token(tok: &[char], out: &mut [char], letter: &mut [bool], reading: Reading, lists: &Lists) {
    let Some(first) = tok.iter().position(|c| c.is_ascii_alphabetic()) else {
        // Число в конце предложения: «28/» → «28.», «34)/» → «34).»
        let n = tok.len();
        if n > 1
            && tok[n - 1] == '/'
            && tok[..n - 1].iter().all(|c| c.is_ascii_digit() || *c == ')')
            && tok[0].is_ascii_digit()
        {
            out[n - 1] = '.';
        }
        return;
    };
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
            // После гласной («двух», «трёх», «случаях») — всегда; после согласной («их» → `b[`) — по паре букв.
            let ok = |r: char| c == '`' || VOWELS.contains(lower(prev)) || weight(&lists.ru, prev, r) > 0;
            if let Some(r) = key_to_ru(c).filter(|&r| ok(r)) {
                out[i] = r;
                letter[i] = true;
                prev = r;
                continue;
            }
        }
        // Настоящими клавишами запятая, точка с запятой и точка — буквы б, ж, ю («масштаб», «муж», «вживую»).
        if reading.real_keys && i == last + 1 {
            let key = match c {
                ',' => Some('б'),
                ';' => Some('ж'),
                '.' => Some('ю'),
                _ => None,
            };
            if let Some(r) = key {
                out[i] = r;
                letter[i] = true;
                prev = r;
                continue;
            }
        }
        // «ю» в конце слова («люблю»), а не точка: русская точка — на клавише `/`.
        if c == '.' && i == last + 1 && reading.dot_is_letter && weight(&lists.ru, prev, 'ю') > 0 {
            out[i] = 'ю';
            letter[i] = true;
            prev = 'ю';
            continue;
        }
        // Запятая русской раскладки — Shift+`/`: «Ghbdtn? rfr» → «Привет, как».
        if c == '?' && reading.question_is_comma {
            out[i] = ',';
            continue;
        }
        // Вопрос, двоеточие, точку с запятой и кавычку ставит `typed_signs`; они не мешают знакам за ними («@lf@?»).
        if "&^$@!)".contains(c) {
            continue;
        }
        // Русская точка — на клавише `/`; до неё в хвосте только знаки, что принимались выше («...экрана)/»).
        if c == '/' && i == tok.len() - 1 {
            out[i] = '.';
        }
        break;
    }
}

/// Знак русской пунктуации, набранный клавишей английской раскладки: где он в тексте и чем стал.
struct Typed {
    at: usize,
    sign: char,
    /// После слова, а не между цифрами или перед числом: так английский текст не пишут (`ltkf&`, `Dhtvz^`).
    after_word: bool,
}

/// Русская пунктуация, набранная клавишами английской раскладки: `&` — «?», `^` — «:», `$` — «;», `@` — кавычка (парная),
/// `#` перед числом — «№», `^` и `?` между цифрами — «:» и «,» (время, десятичная запятая). Только там, где знак может быть знаком: после слова (за ним — лишь такие
/// же знаки и пробел) и перед числом; `a@b.c`, `#tag`, `$HOME`, `x^2`, `AT&T` остаются кодом. `?` (запятая) и `/` (точка)
/// разбирает `convert_token`.
fn typed_signs(chars: &[char]) -> Vec<Typed> {
    let mut found = Vec::new();
    // Открывающие кавычки, что ждут закрывающей.
    let mut open: Vec<usize> = Vec::new();
    for (start, end) in tokens(chars) {
        let tok = &chars[start..end];
        // `#15` — «№15».
        if tok.len() > 1 && tok[0] == '#' && tok[1..].iter().all(char::is_ascii_digit) {
            found.push(Typed { at: start, sign: '№', after_word: false });
            continue;
        }
        // `15^00` — «15:00», `1?5` — «1,5»: между цифрами.
        for i in 1..tok.len().saturating_sub(1) {
            let between_digits = tok[i - 1].is_ascii_digit() && tok[i + 1].is_ascii_digit();
            match tok[i] {
                '^' if between_digits => found.push(Typed { at: start + i, sign: ':', after_word: false }),
                '?' if between_digits => found.push(Typed { at: start + i, sign: ',', after_word: false }),
                _ => {}
            }
        }
        // Открывающая кавычка — `@` в начале слова перед клавишей буквы.
        if tok.len() > 1 && tok[0] == '@' && key_to_ru(tok[1]).is_some() {
            open.push(start);
        }
        let Some(last) = tok.iter().rposition(|c| c.is_ascii_alphabetic()) else { continue };
        let tail = &tok[last + 1..];
        if !tail.iter().all(|c| TAIL.contains(*c)) {
            continue;
        }
        for (k, &c) in tail.iter().enumerate() {
            let at = start + last + 1 + k;
            match c {
                '&' => found.push(Typed { at, sign: '?', after_word: true }),
                '^' => found.push(Typed { at, sign: ':', after_word: true }),
                '$' => found.push(Typed { at, sign: ';', after_word: true }),
                '@' => {
                    // Закрывающая кавычка — если есть открывающая.
                    if let Some(opening) = open.pop() {
                        found.push(Typed { at: opening, sign: '"', after_word: true });
                        found.push(Typed { at, sign: '"', after_word: true });
                    }
                }
                _ => {}
            }
        }
    }
    found
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
fn latin_to_russian(chars: &[char], typed: &[Typed]) -> (Vec<char>, Vec<bool>) {
    let lists = lists();
    let mut out = chars.to_vec();
    let mut letter = vec![false; chars.len()];
    // Запятая после слова, за которой «б» не бывает, — обычная пунктуация: и `?`, и точки в таком тексте остаются как есть.
    let natural = natural_punctuation(chars, lists);
    let words = tokens(chars);
    // `?` после слова, за которым есть ещё слово, — запятая; вместе со знаками вроде `&` это признак настоящего набора.
    let comma_questions = words.iter().take(words.len().saturating_sub(1)).any(|&(start, end)| {
        let tok = &chars[start..end];
        tok.iter().rposition(|c| c.is_ascii_alphabetic()).is_some_and(|last| tok[last + 1..].contains(&'?'))
    });
    let real_keys = !natural && (comma_questions || typed.iter().any(|t| t.after_word));
    for (start, end) in words {
        // Точка в конце слова — «ю», если после неё слово с маленькой буквы или текст кончился: точку так не ставят.
        // `?` после слова — запятая, если за ним идёт ещё слово; последний `?` текста остаётся вопросом.
        let next = chars[end..].iter().find(|c| !c.is_whitespace());
        let reading = Reading {
            dot_is_letter: !natural && next.is_none_or(|c| c.is_ascii_lowercase()),
            question_is_comma: !natural && next.is_some(),
            real_keys,
        };
        convert_token(&chars[start..end], &mut out[start..end], &mut letter[start..end], reading, lists);
    }
    for t in typed {
        out[t.at] = t.sign;
    }
    (out, letter)
}

/// Знак русской раскладки на клавише английской (Shift+`/` — запятая, Shift+7 — вопрос и т. д.); `None` — клавиша та же.
fn sign_to_key(c: char) -> Option<char> {
    Some(match c {
        ',' => '?',
        '.' => '/',
        '?' => '&',
        '"' => '@',
        '№' => '#',
        ';' => '$',
        ':' => '^',
        _ => return None,
    })
}

/// Русская раскладка → английский: каждая русская буква — клавиша; знаки русской раскладки — клавиши английской («Рщц фку нщг,»
/// → «How are you?»); цифры, пробелы и прочее — как есть.
fn cyrillic_to_latin(chars: &[char]) -> (Vec<char>, Vec<bool>) {
    let mut out = chars.to_vec();
    let mut letter = vec![false; chars.len()];
    for (i, &c) in chars.iter().enumerate() {
        if let Some(k) = letter_to_key(c) {
            out[i] = k;
            letter[i] = true;
        } else if let Some(k) = sign_to_key(c) {
            out[i] = k;
        }
    }
    (out, letter)
}

/// Текст, набранный в английской раскладке, как он вышел бы в русской: «Ghbdtn? rfr ltkf&» → «Привет, как дела?».
#[cfg(test)]
fn to_russian(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    latin_to_russian(&chars, &typed_signs(&chars)).0.into_iter().collect()
}

/// Текст, набранный в русской раскладке, как он вышел бы в английской: «Руддщ цщкдв» → «Hello world».
#[cfg(test)]
fn to_english(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    cyrillic_to_latin(&chars).0.into_iter().collect()
}

// ─── Распознавание ──────────────────────────────────────────────────────────

/// Адрес, путь, код, число, цвет: знаки, каких в речи нет, и их сочетания. `@ # $ ^ &` — кроме русской пунктуации, набранной
/// клавишами английской раскладки (`typed_signs`). Скобки `{ } < > ~` — только в начале слова перед буквой: так выходят
/// заглавные Х, Ъ, Б, Ю, Ё в английской раскладке («{jhjij» — «Хорошо»).
fn looks_technical(chars: &[char], typed: &[Typed]) -> bool {
    const SIGNS: &str = "\\%*=+|_";
    if chars.iter().any(|&c| SIGNS.contains(c)) {
        return true;
    }
    if chars.iter().enumerate().any(|(i, &c)| "@#$^&".contains(c) && !typed.iter().any(|t| t.at == i)) {
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
    if chars.len() > MAX_CHARS {
        return None;
    }
    let typed = typed_signs(&chars);
    if looks_technical(&chars, &typed) {
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
    // Одно короткое слово заглавными — сокращение (`LGTM`, `HTTP`), а не набранное в не той раскладке.
    if words.len() == 1
        && letters <= MAX_ACRONYM
        && chars.iter().filter(|c| c.is_alphabetic()).all(|c| c.is_uppercase())
    {
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
        (To::Russian, latin_to_russian(&chars, &typed))
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
    // Знак вроде `ltkf&` в английском тексте не бывает: короткое «Ult ns&» (английские пары `ul`, `lt`, `ns` частые)
    // узнаётся и без сравнения с исходным.
    let typed_after_word = to == To::Russian && typed.iter().any(|t| t.after_word);
    let sure = like_to >= MIN_TARGET && like_to - like_from >= MARGIN;
    (sure || (typed_after_word && like_to >= MIN_TARGET_TYPED)).then(|| Fix { text: out.into_iter().collect(), to })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offered(text: &str) -> Option<(String, To)> {
        suggest(text).map(|f| (f.text, f.to))
    }

    /// Русская фраза, как она выходит, если набрать её клавишами при английской раскладке: точка — `/`, запятая — Shift+`/`,
    /// вопрос — Shift+7, кавычка — Shift+2, «№» — Shift+3, точка с запятой — Shift+4, двоеточие — Shift+6. Таблица своя, не
    /// из кода: тест не проверяет код им же.
    fn typed_in_en(ru: &str) -> String {
        ru.chars()
            .map(|c| match c {
                '.' => '/',
                ',' => '?',
                '?' => '&',
                '"' => '@',
                '№' => '#',
                ';' => '$',
                ':' => '^',
                c => letter_to_key(c).unwrap_or(c),
            })
            .collect()
    }

    /// Английская фраза, набранная клавишами при русской раскладке (знаки — как выше, в обратную сторону).
    fn typed_in_ru(en: &str) -> String {
        en.chars()
            .map(|c| match c {
                '/' => '.',
                '?' => ',',
                '&' => '?',
                '@' => '"',
                '#' => '№',
                '$' => ';',
                '^' => ':',
                c => key_to_ru(c).unwrap_or(c),
            })
            .collect()
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
        // Знаки русской раскладки — клавиши английской: запятая — Shift+/ (`?`), вопрос — Shift+7 (`&`), точка — `/`.
        assert_eq!(to_english("Рщц фку нщг,"), "How are you?", "запятая в конце — это вопрос");
        assert_eq!(to_english("Привет, мир 12"), "Ghbdtn? vbh 12", "запятая — Shift+/, цифры и пробелы остаются");
        assert_eq!(to_english("Ыуу нщг фе 5ю"), "See you at 5.", "точка английской раскладки — буква «ю»");
        assert_eq!(to_english("фтв.щк"), "and/or", "точка русской раскладки — это `/`");
        assert_eq!(to_english("Сфдд ьуЖ тщц"), "Call me: now", "Shift+; — «Ж»");
        assert_eq!(to_english("Ру ыфшв ЭнуыЭ"), "He said \"yes\"", "Shift+' — «Э»");
        assert_eq!(
            to_english("№5 \"а\"; б: в?"),
            "#5 @f@$ ,^ d&",
            "№ Shift+3, \" Shift+2, ; Shift+4, : Shift+6, ? Shift+7"
        );
    }

    #[test]
    fn russian_punctuation_typed_with_the_english_layout_comes_back() {
        // Запятая — Shift+/ (`?`), вопрос — Shift+7 (`&`), кавычка — Shift+2 (`@`), двоеточие — Shift+6 (`^`).
        assert_eq!(to_russian("Ghbdtn? rfr ltkf&"), "Привет, как дела?");
        assert_eq!(to_russian("Jr? gjyzk/"), "Ок, понял.");
        assert_eq!(to_russian("Ult ns&"), "Где ты?");
        assert_eq!(to_russian("Crf;b @lf@"), "Скажи \"да\"");
        assert_eq!(to_russian("@Ghbdtn vbh@ - crfpfk jy"), "\"Привет мир\" - сказал он", "кавычки на несколько слов");
        assert_eq!(to_russian("Dhtvz^ 10 vbyen"), "Время: 10 минут");
        assert_eq!(to_russian("Dcnhtxf d 15^00"), "Встреча в 15:00", "двоеточие между цифрами");
        assert_eq!(to_russian("Gbnm 1?5 kbnhf"), "Пить 1,5 литра", "запятая между цифрами");
        // Настоящими клавишами запятая, точка с запятой и точка в конце слова — буквы б, ж, ю; без них — знаки.
        assert_eq!(to_russian("Vfcinf, df;ty? ve; ljvf"), "Масштаб важен, муж дома");
        assert_eq!(to_russian("D;bde. ghjdthtyj? yt ghjdthtyj"), "Вживую проверено, не проверено");
        assert_eq!(
            to_russian("Ve; ljvf"),
            "Му; дома",
            "без признаков настоящего набора точка с запятой остаётся знаком"
        );
        assert_eq!(to_russian("(ghjdthtyj ntcnjd 34)/"), "(проверено тестов 34).", "точка после скобки");
        assert_eq!(to_russian("Ljujdjh #15"), "Договор №15", "`#` перед числом — «№»");
        assert_eq!(to_russian("Vjkjrj$ csh$ vfckj"), "Молоко; сыр; масло", "точка с запятой — Shift+4");
        assert_eq!(to_russian("Xnj&! Rfr&&"), "Что?! Как??", "знаки подряд");
        assert_eq!(to_russian("Ghbdtn (rfr ltkf)? z"), "Привет (как дела), я", "запятая после скобки");
        assert_eq!(to_russian("Crf;b @lf@? b"), "Скажи \"да\", и", "запятая после кавычки");
        // Запятая: за ней идёт слово, большая буква ему не мешает (Вася); последний `?` текста — вопрос.
        assert_eq!(to_russian("Ghbdtn? dfcz"), "Привет, вася");
        assert_eq!(to_russian("Ghbdtn? Dfcz"), "Привет, Вася");
        assert_eq!(to_russian("Ghbdtn?"), "Привет?");
        assert_eq!(to_russian("Ghbdtn?\nrfr ltkf&"), "Привет,\nкак дела?");
        // Обычная запятая в тексте — тогда `?` остаётся вопросом, как в образце из задачи.
        assert_eq!(to_russian("Ghbdtn, rfr ltkf?"), "Привет, как дела?");
        assert_eq!(to_russian("Ghbdtn, rfr ltkf&"), "Привет, как дела?", "смесь: запятая — запятая, `&` — вопрос");
    }

    #[test]
    fn signs_of_code_stay_code_even_in_a_russian_looking_text() {
        for text in [
            "user@example.com ghbdtn vbh",
            "@username ghbdtn vbh",
            "ghbdtn @vbh",
            "ghbdtn vbh@",
            "ghbdtn #tag vbh",
            "ghbdtn $HOME vbh",
            "ghbdtn $5 vbh",
            "x^2 ghbdtn vbh",
            "ghbdtn a&&b vbh",
            "AT&T ghbdtn vbh",
            "^start ghbdtn vbh",
            "foo(&bar) ghbdtn vbh",
            "ghbdtn 10^ vbh",
            "ghbdtn &amp; vbh",
            "ghbdtn#15 vbh",
        ] {
            assert_eq!(offered(text), None, "{text}");
        }
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

    /// Вопросы, набранные по-настоящему — клавишами не той раскладки (запятая `?`, вопрос `&`), — узнаются.
    #[test]
    fn typed_questions_and_punctuation_are_offered() {
        for (typed, fixed) in [
            ("Ghbdtn? rfr ltkf&", "Привет, как дела?"),
            ("Jr? gjyzk/", "Ок, понял."),
            ("Ult ns&", "Где ты?"),
            ("Crf;b @lf@", "Скажи \"да\""),
            ("Dhtvz^ 10 vbyen", "Время: 10 минут"),
            ("Dcnhtxf d 15^00", "Встреча в 15:00"),
            ("Xnj yjdjuj&", "Что нового?"),
        ] {
            assert_eq!(offered(typed), Some((fixed.into(), To::Russian)), "{typed}");
        }
        for (typed, fixed) in [
            ("Рщц фку нщг,", "How are you?"),
            ("Руддщб рщц фку нщг,", "Hello, how are you?"),
            ("Ыуу нщг фе 5ю", "See you at 5."),
            ("Сфдд ьуЖ тщц", "Call me: now"),
            ("Ру ыфшв ЭнуыЭ", "He said \"yes\""),
        ] {
            assert_eq!(offered(typed), Some((fixed.into(), To::English)), "{typed}");
        }
        // Вопрос из одного слова.
        assert_eq!(offered("Ghbdtn&"), Some(("Привет?".into(), To::Russian)));
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
            // С настоящей пунктуацией: запятая, вопрос, точка, двоеточие, кавычки, номер.
            "Привет, как дела?",
            "Привет, как дела",
            "Как дела?",
            "Я дома.",
            "Где ты?",
            "Ок, понял.",
            "Скажи \"да\"",
            "Время: 10 минут",
            "Встреча в 15:00",
            "Привет, Вася",
            "Что нового?",
            "Мы дома",
            "Когда придёшь, позвони",
            "Договор №15 подписан",
            "Молоко; сыр; масло",
            "Ты дома? Я скоро буду.",
            "Спасибо, всё получилось!",
        ] {
            assert_eq!(offered(&typed_in_en(ru)), Some((ru.into(), To::Russian)), "{ru} → {}", typed_in_en(ru));
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
            // С настоящей пунктуацией: `?` выходит запятой, `/` — точкой, `:` и `"` — «Ж» и «Э».
            "How are you?",
            "Is it ready?",
            "See you at 5.",
            "Call me: now",
            "He said \"yes\"",
            "Wait, what?",
            "Where is the meeting?",
            "Thanks! See you tomorrow.",
        ] {
            assert_eq!(offered(&typed_in_ru(en)), Some((en.into(), To::English)), "{en} → {}", typed_in_ru(en));
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
        // Одно слово заглавными до пяти букв — сокращение, а не слово с Caps Lock: «LGTM» не «ДПЕЬ».
        for text in ["LGTM", "HTTP", "ASAP", "NASA", "UEFI", "HDMI", "ГОСТ", "ЖКХ"] {
            assert_eq!(offered(text), None, "{text:?}");
        }
        assert_eq!(
            offered("GHBDTN").map(|(t, _)| t),
            Some("ПРИВЕТ".into()),
            "а слово подлиннее — Caps Lock, предлагаем"
        );
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
