// Bluetooth-звук: подключить и отключить сопряжённое звуковое устройство (наушники, гарнитуру, колонку) из панели.
// Простого вызова «подключи наушники» у Windows нет. «Параметры Windows» идут через драйвер: у каждого звукового устройства
// Bluetooth есть фильтры KS (музыка — `bthenum`, звонки — `bthhfenum`), и они понимают две команды набора
// `KSPROPSETID_BtAudio` из `ksmedia.h` — «переподключить» (`KSPROPERTY_ONESHOT_RECONNECT`, 0) и «отключить»
// (`KSPROPERTY_ONESHOT_DISCONNECT`, 1). Образец пути — ToothTray (BSD-2); кода из него здесь нет.
// Путь: звуковые точки Windows во всех состояниях (у отключённого устройства они «не подключены») → у каждой `IDeviceTopology`
// → соединители → к чему подключён соединитель (`IConnector::GetConnectedTo` → `IPart::GetTopologyObject` → `GetDeviceId`):
// это id фильтра, и у Bluetooth-аудио он начинается с `{2}.\\?\bth` → `IMMDeviceEnumerator::GetDevice(id)` → `IKsControl` →
// `KsProperty` с `KSPROPERTY { Set: KSPROPSETID_BtAudio, Id: 0 | 1, Flags: KSPROPERTY_TYPE_GET }`. Запрос уходит всем фильтрам
// устройства (у гарнитуры их два). Устройство и его точки связывает ContainerId: у всех частей одного физического устройства он
// один — у точки это `PKEY_Device_ContainerId`, у узла `BTHENUM` с адресом устройства — `DEVPKEY_Device_ContainerId`
// (`bt::containers_of`). Не `bt::address_in` по id фильтра: в пути интерфейса есть GUID, чей хвост — тоже 12 шестнадцатеричных
// знаков, а у `bthhfenum` (звонки) адреса в id нет вовсе. Права администратора, по замыслу, не нужны (подтвердить вживую).
// Писатель — за заслоном (`sys::blocked`): в демо и в тестах Windows не трогается, подключение выдумано (`bt::fake_connect`).
// Всё идёт в потоке с COM (MTA, [`Com`]); объекты COM из потока не выходят.

use std::time::Duration;

use windows::Win32::Devices::Properties::DEVPKEY_Device_ContainerId;
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::{
    DEVICE_STATE, DEVICE_STATEMASK_ALL, IConnector, IDeviceTopology, IMMDevice, IMMDeviceEnumerator, IPart,
    MMDeviceEnumerator, eAll,
};
use windows::Win32::Media::KernelStreaming::{
    IKsControl, KSIDENTIFIER, KSIDENTIFIER_0, KSIDENTIFIER_0_0, KSPROPERTY_ONESHOT_DISCONNECT,
    KSPROPERTY_ONESHOT_RECONNECT, KSPROPERTY_TYPE_GET, KSPROPSETID_BtAudio,
};
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToGUID};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, STGM_READ,
};
use windows::core::{Error, HRESULT, Interface, PCWSTR, PWSTR};

use super::bt;

/// ContainerId точки: тот же ключ, что у узла устройства (`DEVPKEY_Device_ContainerId`), только как ключ свойства точки.
const CONTAINER: PROPERTYKEY =
    PROPERTYKEY { fmtid: DEVPKEY_Device_ContainerId.fmtid, pid: DEVPKEY_Device_ContainerId.pid };

/// «Нет контейнера» у Windows: `{00000000-0000-0000-FFFF-FFFFFFFFFFFF}` — у встроенных устройств и корня. По нему точки не
/// сопоставляют: иначе подошли бы чужие устройства.
const NO_CONTAINER: u128 = 0x0000_0000_0000_0000_ffff_ffff_ffff_ffff;

/// Больше фильтров Bluetooth-аудио у одного устройства не бывает (музыка и звонки, по одному; запас — на LE Audio). Нашлось больше —
/// сопоставление ошиблось, и запрос не уходит ни одному: он мог бы отключить чужое устройство.
const MOST_FILTERS: usize = 6;

/// `ERROR_MORE_DATA`: драйвер принял запрос, но у него есть что отдать, а буфер нулевой, — запрос ушёл.
const MORE_DATA: HRESULT = HRESULT(0x8007_00EAu32 as i32);

/// Что за фильтр у звуковой точки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    /// `bthenum`: музыка (A2DP).
    Music,
    /// `bthhfenum`: звонки (гарнитура, «hands-free»).
    Calls,
    /// Другой фильтр Bluetooth.
    Other,
}

/// Фильтр Bluetooth-аудио по id устройства KS (`IDeviceTopology::GetDeviceId`): `{2}.\\?\bthenum#…` — музыка,
/// `{2}.\\?\bthhfenum#…` — звонки. Не Bluetooth — `None`. Регистр не важен.
pub fn bluetooth_filter(id: &str) -> Option<Filter> {
    let id = id.to_ascii_lowercase();
    let rest = id.strip_prefix(r"{2}.\\?\bth")?;
    Some(if rest.starts_with("enum#") {
        Filter::Music
    } else if rest.starts_with("hfenum#") {
        Filter::Calls
    } else {
        Filter::Other
    })
}

/// Настоящий ли контейнер: не пустой и не «нет контейнера».
pub fn is_real_container(container: u128) -> bool {
    container != 0 && container != NO_CONTAINER
}

/// Звуковая точка Windows: чей она (контейнер) и к какому фильтру подключена.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Point {
    /// ContainerId точки; `None` — Windows не сказала.
    pub container: Option<u128>,
    /// id фильтра, к которому подключён соединитель точки (`IDeviceTopology::GetDeviceId`).
    pub filter: String,
    /// Состояние точки (`DEVICE_STATE_*`: 1 — работает, 2 — выключена, 4 — нет, 8 — не подключена); для проверки, что у отключённого
    /// устройства фильтр находится тоже.
    pub state: u32,
}

/// Фильтры Bluetooth-аудио устройства: у каждой точки с контейнером из `containers` — её фильтр, если он Bluetooth. Без повторов
/// (у гарнитуры «слушать» и «говорить» могут делить фильтр), по порядку точек.
pub fn filters_of(points: &[Point], containers: &[u128]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for point in points {
        let Some(container) = point.container else { continue };
        if !is_real_container(container)
            || !containers.contains(&container)
            || bluetooth_filter(&point.filter).is_none()
        {
            continue;
        }
        if !out.iter().any(|f| f.eq_ignore_ascii_case(&point.filter)) {
            out.push(point.filter.clone());
        }
    }
    out
}

/// COM на время жизни потока (MTA): `IMMDevice`, топология и WinRT живут только в нём. В выдуманном мире (демо, тесты) Windows
/// не трогается — и COM тоже.
pub struct Com {
    init: bool,
}

impl Com {
    pub fn enter() -> Com {
        if super::fake_reads() {
            return Com { init: false };
        }
        // S_OK и S_FALSE (поток уже в COM) — оба просят парного `CoUninitialize`; `RPC_E_CHANGED_MODE` (поток уже STA) — нет.
        let init = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        Com { init }
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.init {
            unsafe { CoUninitialize() };
        }
    }
}

/// Строка из COM-памяти: прочитать и вернуть память Windows.
unsafe fn take(text: PWSTR) -> Option<String> {
    if text.is_null() {
        return None;
    }
    let s = unsafe { text.to_string() }.ok();
    unsafe { CoTaskMemFree(Some(text.0 as *const _)) };
    s
}

/// ContainerId точки (`PKEY_Device_ContainerId` из хранилища свойств).
unsafe fn container_of(endpoint: &IMMDevice) -> Option<u128> {
    unsafe {
        let store = endpoint.OpenPropertyStore(STGM_READ).ok()?;
        let mut value = store.GetValue(&CONTAINER).ok()?;
        let guid = PropVariantToGUID(&value).ok();
        let _ = PropVariantClear(&mut value);
        guid.map(|g| g.to_u128())
    }
}

/// id фильтра, к которому подключён соединитель: по цепочке «подключён к → часть → устройство», а не вышло — как отдаёт сам
/// соединитель (`GetDeviceIdConnectedTo`).
unsafe fn filter_behind(connector: &IConnector) -> Option<String> {
    unsafe {
        let chain = || -> windows::core::Result<Option<String>> {
            let other = connector.GetConnectedTo()?;
            let part: IPart = other.cast()?;
            Ok(take(part.GetTopologyObject()?.GetDeviceId()?))
        };
        match chain() {
            Ok(Some(id)) => Some(id),
            _ => connector.GetDeviceIdConnectedTo().ok().and_then(|id| take(id)),
        }
    }
}

/// Все звуковые точки Windows в любом состоянии, за которыми стоит фильтр Bluetooth: их контейнер и фильтр. Только чтение.
/// Звать из потока с [`Com`]. Выдуманный мир (демо, тесты) — точек нет: подключение там выдумано целиком (`set_connected`).
pub fn points() -> Result<Vec<Point>, String> {
    if super::fake_reads() {
        return Ok(Vec::new());
    }
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| e.message())?;
        let all = enumerator.EnumAudioEndpoints(eAll, DEVICE_STATE(DEVICE_STATEMASK_ALL)).map_err(|e| e.message())?;
        let count = all.GetCount().map_err(|e| e.message())?;
        let mut out = Vec::new();
        for i in 0..count {
            let Ok(endpoint) = all.Item(i) else { continue };
            let Ok(topology) = endpoint.Activate::<IDeviceTopology>(CLSCTX_ALL, None) else { continue };
            let Ok(connectors) = topology.GetConnectorCount() else { continue };
            let container = container_of(&endpoint);
            let state = endpoint.GetState().map_or(0, |state| state.0);
            for n in 0..connectors {
                let Ok(connector) = topology.GetConnector(n) else { continue };
                if let Some(filter) = filter_behind(&connector).filter(|id| bluetooth_filter(id).is_some()) {
                    out.push(Point { container, filter, state });
                }
            }
        }
        Ok(out)
    }
}

/// Одна команда одному фильтру: открыть его (`GetDevice`), взять `IKsControl` и послать `KsProperty`.
unsafe fn ask(enumerator: &IMMDeviceEnumerator, filter: &str, on: bool) -> windows::core::Result<()> {
    unsafe {
        let id = super::wide(filter);
        let device = enumerator.GetDevice(PCWSTR(id.as_ptr()))?;
        let control: IKsControl = device.Activate(CLSCTX_ALL, None)?;
        let command = if on { KSPROPERTY_ONESHOT_RECONNECT } else { KSPROPERTY_ONESHOT_DISCONNECT };
        let request = KSIDENTIFIER {
            Anonymous: KSIDENTIFIER_0 {
                Anonymous: KSIDENTIFIER_0_0 {
                    Set: KSPROPSETID_BtAudio,
                    Id: command.0 as u32,
                    Flags: KSPROPERTY_TYPE_GET,
                },
            },
        };
        let mut returned = 0u32;
        super::real_write("IKsControl::KsProperty");
        control.KsProperty(&request, size_of::<KSIDENTIFIER>() as u32, std::ptr::null_mut(), 0, &mut returned)
    }
}

/// Попросить драйвер подключить (`on`) или отключить устройство — каждому его фильтру Bluetooth-аудио. Ответ — сколько фильтров
/// приняли запрос; ошибка — по-английски, для журнала (пользователю говорит модуль своими словами). Ничего не принял — ошибка:
/// подключаться нечему. Принял — подключилось ли, видно только из чтения устройств (`bt::devices`): драйвер отвечает сразу, а
/// соединение занимает секунды. Звать из потока с [`Com`].
pub fn set_connected(address: u64, name: &str, on: bool) -> Result<usize, String> {
    if super::blocked(&format!("bt {} {name}", if on { "connect" } else { "disconnect" })) {
        return fake_set(address, on);
    }
    let containers: Vec<u128> = bt::containers_of(address).into_iter().filter(|c| is_real_container(*c)).collect();
    if containers.is_empty() {
        return Err("the device has no container id".into());
    }
    let filters = filters_of(&points()?, &containers);
    if filters.is_empty() {
        return Err("no Bluetooth audio filter behind the device's endpoints".into());
    }
    if filters.len() > MOST_FILTERS {
        return Err(format!("{} filters matched one device: refusing to send", filters.len()));
    }
    let kinds = |kind: Filter| filters.iter().filter(|f| bluetooth_filter(f) == Some(kind)).count();
    let (music, calls) = (kinds(Filter::Music), kinds(Filter::Calls));
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.map_err(|e| e.message())?;
    let mut accepted = 0;
    let mut refused: Option<Error> = None;
    for filter in &filters {
        match unsafe { ask(&enumerator, filter, on) } {
            Ok(()) => accepted += 1,
            // Драйвер хотел отдать данные, а буфер нулевой: действие «в один выстрел» всё равно выполнено.
            Err(e) if e.code() == MORE_DATA => accepted += 1,
            Err(e) => {
                eprintln!("bluetooth audio: a filter refused the request: {:#010x}", e.code().0);
                refused = Some(e);
            }
        }
    }
    eprintln!(
        "bluetooth audio: {} to {} filters (music {music}, calls {calls}), {accepted} accepted",
        if on { "connect" } else { "disconnect" },
        filters.len()
    );
    if accepted > 0 {
        Ok(accepted)
    } else {
        Err(refused.map(|e| e.message()).unwrap_or_else(|| "no filter accepted the request".into()))
    }
}

/// Выдуманное подключение (демо и тесты): занимает столько же, сколько в макете, и меняет выдуманный мир (`bt`). Устройства
/// без выдуманных точек нет — как у настоящего, у которого точек не нашлось.
fn fake_set(address: u64, on: bool) -> Result<usize, String> {
    if !bt::fake_has_points(address) {
        return Err("no Bluetooth audio filter behind the device's endpoints".into());
    }
    std::thread::sleep(fake_delay(on));
    bt::fake_connect(address, on);
    // Как у гарнитуры: два фильтра, музыка и звонки.
    Ok(2)
}

/// Сколько «занимает» выдуманное подключение: 1,4 с, как в макете; отключение быстрее. В тестах — доли секунды.
fn fake_delay(on: bool) -> Duration {
    if cfg!(test) {
        Duration::from_millis(30)
    } else if on {
        Duration::from_millis(1400)
    } else {
        Duration::from_millis(600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Id фильтров, как их отдаёт Windows (устройство и адрес выдуманы).
    const MUSIC: &str = r"{2}.\\?\bthenum#{0000110b-0000-1000-8000-00805f9b34fb}_vid&0001ffff_pid&0001#7&1a2b3c4d&0&a0b1c2d3e4f5_c00000000#{6994ad04-93ef-11d0-a3cc-00a0c9223196}\audio";
    const CALLS: &str = r"{2}.\\?\bthhfenum#{0000111e-0000-1000-8000-00805f9b34fb}_vid&0001ffff_pid&0001#9&5e6f7a8b&0&bthhfaudio#{6994ad04-93ef-11d0-a3cc-00a0c9223196}\audio";

    #[test]
    fn bluetooth_filters_are_told_by_the_prefix_of_their_id() {
        assert_eq!(bluetooth_filter(MUSIC), Some(Filter::Music));
        assert_eq!(bluetooth_filter(CALLS), Some(Filter::Calls));
        // Регистр не важен; другой фильтр Bluetooth — всё равно Bluetooth.
        assert_eq!(bluetooth_filter(&MUSIC.to_uppercase()), Some(Filter::Music));
        assert_eq!(
            bluetooth_filter(r"{2}.\\?\bthleaudio#{00001850-0000-1000-8000-00805f9b34fb}#1&2&3"),
            Some(Filter::Other)
        );
        // Не Bluetooth: встроенный звук, USB, точка (не фильтр), пусто, префикс не в начале.
        for id in [
            r"{2}.\\?\hdaudio#func_01&ven_10ec&dev_0000#4&1&0&0001#{6994ad04-93ef-11d0-a3cc-00a0c9223196}\audio",
            r"{2}.\\?\usb#vid_1234&pid_5678#0001#{6994ad04-93ef-11d0-a3cc-00a0c9223196}",
            "{0.0.0.00000000}.{a1b2c3d4-0000-0000-0000-000000000001}",
            r"\\?\bthenum#{0000110b-0000-1000-8000-00805f9b34fb}",
            r"x{2}.\\?\bthenum#",
            "",
        ] {
            assert_eq!(bluetooth_filter(id), None, "{id}");
        }
    }

    #[test]
    fn the_empty_container_matches_nothing() {
        assert!(is_real_container(0xa1b2_c3d4_0000_0000_0000_0000_0000_0001));
        assert!(!is_real_container(0));
        assert!(!is_real_container(NO_CONTAINER));
        // `{00000000-0000-0000-FFFF-FFFFFFFFFFFF}` как GUID — то самое значение.
        assert_eq!(windows::core::GUID::from_u128(NO_CONTAINER).data4, [0xff; 8]);
        let points = [Point { container: Some(NO_CONTAINER), filter: MUSIC.into(), state: 1 }];
        assert!(filters_of(&points, &[NO_CONTAINER]).is_empty(), "встроенные устройства — не чьё-то устройство");
    }

    /// Устройство ↔ точки — по ContainerId: свои фильтры (музыка и звонки), чужие и не Bluetooth не попадают, повторы — один раз.
    #[test]
    fn a_device_gets_the_filters_behind_its_endpoints_by_container() {
        const MINE: u128 = 0x1111_2222_3333_4444_5555_6666_7777_8888;
        const THEIRS: u128 = 0x9999_aaaa_bbbb_cccc_dddd_eeee_ffff_0001;
        let other_music = r"{2}.\\?\bthenum#{0000110b-0000-1000-8000-00805f9b34fb}_vid&0001ffff_pid&0002#7&aaaaaaaa&0&b0b1b2b3b4b5_c00000000#{6994ad04-93ef-11d0-a3cc-00a0c9223196}\audio";
        let at = |container: Option<u128>, filter: &str| Point { container, filter: filter.to_string(), state: 1 };
        let points = [
            at(Some(MINE), MUSIC),
            at(Some(MINE), CALLS),
            at(Some(MINE), CALLS),                 // «слушать» и «говорить» делят фильтр
            at(Some(MINE), &CALLS.to_uppercase()), // тот же фильтр другим регистром
            at(Some(THEIRS), other_music),
            at(None, other_music),
            at(Some(MINE), r"{2}.\\?\hdaudio#func_01&ven_10ec&dev_0000#4&1&0&0001"),
        ];
        assert_eq!(filters_of(&points, &[MINE]), [MUSIC, CALLS]);
        assert_eq!(filters_of(&points, &[THEIRS]), [other_music]);
        // Несколько контейнеров у одного устройства (классика и LE) — фильтры обоих; ни одного — ничего.
        assert_eq!(filters_of(&points, &[MINE, THEIRS]), [MUSIC, CALLS, other_music]);
        assert!(filters_of(&points, &[]).is_empty());
        assert!(filters_of(&[], &[MINE]).is_empty());
    }

    /// Выдуманное подключение: заслон остановил запись (метка в журнале), выдуманный мир запомнил, и ничего настоящего не было.
    #[test]
    fn the_invented_connection_is_remembered_and_stays_behind_the_guard() {
        let _world = super::super::WORLD_TEST.lock().unwrap_or_else(|e| e.into_inner());
        super::super::reset_fake_world();
        let marked =
            |what: &str| super::super::BLOCKED.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|w| w == what);
        let speaker = |list: Vec<bt::Device>| list.into_iter().find(|d| d.address == bt::FAKE_SPEAKER).unwrap();
        assert!(!speaker(bt::devices()).connected);
        assert_eq!(set_connected(bt::FAKE_SPEAKER, "Speaker", true), Ok(2));
        assert!(marked("bt connect Speaker"));
        assert!(speaker(bt::devices()).connected, "выдуманный мир помнит");
        assert_eq!(set_connected(bt::FAKE_SPEAKER, "Speaker", false), Ok(2));
        assert!(marked("bt disconnect Speaker"));
        assert!(!speaker(bt::devices()).connected);
        // «Выключенное» устройство: запрос принят, а подключения нет. Устройство без точек — ошибка, как у настоящего.
        bt::fake_set_unreachable(bt::FAKE_SPEAKER, true);
        assert_eq!(set_connected(bt::FAKE_SPEAKER, "Speaker", true), Ok(2));
        assert!(!speaker(bt::devices()).connected);
        bt::fake_set_no_points(bt::FAKE_SPEAKER, true);
        assert!(set_connected(bt::FAKE_SPEAKER, "Speaker", true).is_err());
        assert!(set_connected(0xdead, "Nobody", true).is_err(), "такого устройства нет");
        super::super::reset_fake_world();
    }

    /// Живое чтение, ничего не посылает: звуковые точки этого ПК, их фильтры Bluetooth и сопоставление с сопряжёнными
    /// устройствами. В журнал — только числа (ни имён, ни адресов). Не в общем прогоне — `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn reads_the_bluetooth_filters_of_this_pc() {
        crate::sys::LIVE_READS.store(true, std::sync::atomic::Ordering::Relaxed);
        let _com = Com::enter();
        let points = points().expect("перечисление точек");
        let paired = bt::devices();
        let audio: Vec<_> = paired.iter().filter(|d| d.audio).collect();
        let mut with_container = 0;
        let mut with_filters = 0;
        let mut found = 0;
        // Главное для «Подключить»: у НЕ подключённого устройства точки и фильтры тоже находятся.
        let (mut off_audio, mut off_with_filters) = (0, 0);
        for d in &audio {
            let containers = bt::containers_of(d.address);
            with_container += usize::from(!containers.is_empty());
            let filters = filters_of(&points, &containers);
            with_filters += usize::from(!filters.is_empty());
            found += filters.len();
            if !d.connected {
                off_audio += 1;
                off_with_filters += usize::from(!filters.is_empty());
            }
            assert!(filters.len() <= MOST_FILTERS, "у одного устройства слишком много фильтров");
            assert!(filters.iter().all(|f| bluetooth_filter(f).is_some()));
        }
        let by_state = |state: u32| points.iter().filter(|p| p.state == state).count();
        eprintln!(
            "bluetooth radio: {:?}; points with a Bluetooth filter: {} ({} active, {} disabled, {} not present, {} unplugged), with a container: {}",
            crate::sys::net::bluetooth_on(),
            points.len(),
            by_state(1),
            by_state(2),
            by_state(4),
            by_state(8),
            points.iter().filter(|p| p.container.is_some_and(is_real_container)).count(),
        );
        eprintln!(
            "bluetooth devices: paired {}, audio {} ({off_audio} not connected); audio with a container: {with_container}, with filters: {with_filters} ({off_with_filters} of the not connected), filters: {found}",
            paired.len(),
            audio.len(),
        );
    }
}
