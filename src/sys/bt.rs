// Bluetooth: сопряжённые устройства, подключено ли, и заряд — если Windows его знает.
// Список — BluetoothFindFirstDevice (без поиска новых: только запомненные); заряд — свойство
// устройства {104EA319-6EE2-4701-BD47-8DDBF425BBE5} 2 у узлов BTHENUM и BTHLEDEVICE, которые
// сопоставляются с устройством по адресу в их ID (12 шестнадцатеричных знаков).
// Подключать и отключать устройства отсюда нельзя — это делают «Параметры Windows».

use windows::Win32::Devices::Bluetooth::{
    BLUETOOTH_DEVICE_INFO, BLUETOOTH_DEVICE_SEARCH_PARAMS, BluetoothFindDeviceClose, BluetoothFindFirstDevice,
    BluetoothFindNextDevice,
};
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    DIGCF_ALLCLASSES, DIGCF_PRESENT, SP_DEVINFO_DATA, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo,
    SetupDiGetClassDevsW, SetupDiGetDeviceInstanceIdW, SetupDiGetDevicePropertyW,
};
use windows::Win32::Devices::Properties::DEVPROPTYPE;
use windows::Win32::Foundation::DEVPROPKEY;
use windows::core::{BOOL, GUID, w};

const BATTERY: DEVPROPKEY = DEVPROPKEY { fmtid: GUID::from_u128(0x104ea319_6ee2_4701_bd47_8ddbf425bbe5), pid: 2 };

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub name: String,
    pub connected: bool,
    /// Заряд 0..100; `None` — Windows не знает.
    pub battery: Option<u8>,
    pub address: u64,
}

/// Адрес из ID узла устройства: последний кусок из ровно 12 шестнадцатеричных знаков.
/// `BTHENUM\{0000111e-…}_LOCALMFG&0002\7&27e1d9e9&0&A0B1C2D3E4F5_C00000000` → 0xA0B1C2D3E4F5.
pub fn address_in(id: &str) -> Option<u64> {
    // Хвост базового UUID Bluetooth (…-00805f9b34fb) — не адрес.
    id.rsplit(|c: char| !c.is_ascii_hexdigit())
        .find(|part| part.len() == 12 && !part.eq_ignore_ascii_case("00805f9b34fb"))
        .and_then(|hex| u64::from_str_radix(hex, 16).ok())
}

/// Заряд всех узлов, где он есть: (адрес, заряд).
fn batteries() -> Vec<(u64, u8)> {
    let mut out = Vec::new();
    for enumerator in [w!("BTHENUM"), w!("BTHLEDEVICE")] {
        unsafe {
            let Ok(set) = SetupDiGetClassDevsW(None, enumerator, None, DIGCF_PRESENT | DIGCF_ALLCLASSES) else {
                continue;
            };
            let mut i = 0;
            loop {
                let mut data = SP_DEVINFO_DATA { cbSize: size_of::<SP_DEVINFO_DATA>() as u32, ..Default::default() };
                if SetupDiEnumDeviceInfo(set, i, &mut data).is_err() {
                    break;
                }
                i += 1;
                let mut kind = DEVPROPTYPE::default();
                let mut value = [0u8; 1];
                if SetupDiGetDevicePropertyW(set, &data, &BATTERY, &mut kind, Some(&mut value), None, 0).is_err() {
                    continue;
                }
                let mut id = [0u16; 512];
                if SetupDiGetDeviceInstanceIdW(set, &data, Some(&mut id), None).is_err() {
                    continue;
                }
                let end = id.iter().position(|&c| c == 0).unwrap_or(id.len());
                if let Some(addr) = address_in(&String::from_utf16_lossy(&id[..end]))
                    && value[0] <= 100
                {
                    out.push((addr, value[0]));
                }
            }
            let _ = SetupDiDestroyDeviceInfoList(set);
        }
    }
    out
}

/// Сопряжённые устройства: подключённые — первыми.
pub fn devices() -> Vec<Device> {
    if super::demo() {
        return vec![
            Device { name: "Headphones".into(), connected: true, battery: Some(80), address: 1 },
            Device { name: "Mouse".into(), connected: true, battery: Some(45), address: 2 },
            Device { name: "Keyboard".into(), connected: false, battery: None, address: 3 },
        ];
    }
    let mut out = Vec::new();
    let params = BLUETOOTH_DEVICE_SEARCH_PARAMS {
        dwSize: size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32,
        fReturnAuthenticated: BOOL(1),
        fReturnRemembered: BOOL(1),
        fReturnUnknown: BOOL(0),
        fReturnConnected: BOOL(1),
        fIssueInquiry: BOOL(0),
        cTimeoutMultiplier: 0,
        ..Default::default()
    };
    let mut info = BLUETOOTH_DEVICE_INFO { dwSize: size_of::<BLUETOOTH_DEVICE_INFO>() as u32, ..Default::default() };
    unsafe {
        let Ok(find) = BluetoothFindFirstDevice(&params, &mut info) else { return out };
        loop {
            let end = info.szName.iter().position(|&c| c == 0).unwrap_or(info.szName.len());
            out.push(Device {
                name: String::from_utf16_lossy(&info.szName[..end]),
                connected: info.fConnected.as_bool(),
                battery: None,
                address: info.Address.Anonymous.ullLong,
            });
            info = BLUETOOTH_DEVICE_INFO { dwSize: size_of::<BLUETOOTH_DEVICE_INFO>() as u32, ..Default::default() };
            if BluetoothFindNextDevice(find, &mut info).is_err() {
                break;
            }
        }
        let _ = BluetoothFindDeviceClose(find);
    }
    let charge = batteries();
    for d in &mut out {
        // У отключённого Windows хранит старое значение — его не показываем.
        d.battery = charge.iter().find(|(a, _)| *a == d.address && d.connected).map(|(_, b)| *b);
        if d.name.trim().is_empty() {
            d.name = format!("{:012X}", d.address);
        }
    }
    out.sort_by_key(|d| !d.connected);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_this_pc_without_crashing() {
        // Настоящий вызов Windows: адаптера может не быть — тогда список пуст, но без паники.
        for d in devices() {
            assert!(d.battery.is_none_or(|b| b <= 100));
        }
    }

    #[test]
    fn addresses_from_ids() {
        let classic =
            r"BTHENUM\{0000111e-0000-1000-8000-00805f9b34fb}_LOCALMFG&0002\7&27e1d9e9&0&A0B1C2D3E4F5_C00000000";
        assert_eq!(address_in(classic), Some(0xA0B1C2D3E4F5));
        let le = r"BTHLEDEVICE\{0000180f-0000-1000-8000-00805f9b34fb}_DEV_VID&02045E_PID&0B13_REV&0509_112233445566\8&1a2b3c4d&0&0020";
        assert_eq!(address_in(le), Some(0x112233445566));
        assert_eq!(address_in(r"USB\VID_1234&PID_5678\0001"), None);
        assert_eq!(address_in(r"BTHENUM\{0000110b-0000-1000-8000-00805f9b34fb}_X"), None, "хвост UUID — не адрес");
    }
}
