// Медиа: что сейчас играет (системные медиа-сеансы Windows — Spotify, браузер, плееры) и кнопки
// «назад», «пауза», «вперёд». WinRT: GlobalSystemMediaTransportControlsSessionManager.
// Ответы асинхронные — всё это зовётся из потока модуля и ждёт ответа там, но не дольше
// `TIMEOUT`: зависший плеер не должен вешать поток навсегда.
// Обложка — поток картинки сеанса, разбирается WIC в RGBA.

use std::sync::Arc;
use std::time::{Duration, Instant};

use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::IRandomAccessStreamWithContentType;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppRGBA, IWICImagingFactory, WICBitmapDitherTypeNone,
    WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, IStream,
};
use windows::Win32::System::WinRT::CreateStreamOverRandomAccessStream;
use windows::core::{Error, HRESULT, Result};

/// Сколько ждать ответа WinRT.
const TIMEOUT: Duration = Duration::from_secs(2);

/// Дождаться асинхронной операции WinRT, но не дольше `TIMEOUT`; не дождались — отменить.
/// Макрос, а не функция: тип операции — из крейта windows-future, а он у нас не прямой.
macro_rules! wait {
    ($op:expr) => {{
        let op = $op;
        let start = Instant::now();
        loop {
            // 0 — ещё идёт, 1 — готово, 2 — отменено, 3 — ошибка.
            match op.Status()?.0 {
                0 if start.elapsed() < TIMEOUT => std::thread::sleep(Duration::from_millis(15)),
                0 => {
                    let _ = op.Cancel();
                    break Err(Error::new(HRESULT(0x8000_0005u32 as i32), "media request timed out"));
                }
                _ => break op.GetResults(),
            }
        }
    }};
}

/// Что играет сейчас.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    /// Кто играет: AppUserModelId сеанса («Spotify.exe», «Chrome»…).
    pub app: String,
    pub playing: bool,
    /// Обложка: ширина, высота, RGBA. `Arc` — чтобы не копировать картинку на каждом опросе.
    pub cover: Option<(u32, u32, Arc<Vec<u8>>)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Previous,
    PlayPause,
    Next,
}

/// Подготовить поток для WinRT и WIC.
pub fn init_thread() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

fn session() -> Result<Session> {
    let manager = wait!(Manager::RequestAsync()?)?;
    manager.GetCurrentSession()
}

fn read_session(with_cover: bool) -> Result<NowPlaying> {
    let s = session()?;
    let props = wait!(s.TryGetMediaPropertiesAsync()?)?;
    let playing = s.GetPlaybackInfo().and_then(|i| i.PlaybackStatus()).is_ok_and(|st| st == Status::Playing);
    let cover = if with_cover {
        match props.Thumbnail() {
            Ok(t) => decode(wait!(t.OpenReadAsync()?)?),
            Err(_) => None,
        }
    } else {
        None
    };
    Ok(NowPlaying {
        title: props.Title().map(|h| h.to_string()).unwrap_or_default(),
        artist: props.Artist().map(|h| h.to_string()).unwrap_or_default(),
        app: s.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default(),
        playing,
        cover,
    })
}

/// Прочитать текущий сеанс; `None` — ничего не играет (или плеер не ответил). `with_cover` — ещё
/// и обложку (дороже). В демо-режиме трек выдуманный, даже если ничего не играет.
pub fn read(with_cover: bool) -> Option<NowPlaying> {
    if super::demo() {
        return Some(NowPlaying {
            title: "Demo Track".into(),
            artist: "Demo Artist".into(),
            app: "Player".into(),
            playing: true,
            cover: None,
        });
    }
    read_session(with_cover).ok()
}

/// Кнопка плеера. Плеер ответил «не могу» — тоже ошибка. В демо и в тестах плееру ничего не уходит (заслон, как у
/// остальных писателей): кнопка «пауза» из теста не остановила бы настоящий плеер.
pub fn act(action: Action) -> Result<()> {
    if super::blocked(&format!("media::act {action:?}")) {
        return Ok(());
    }
    super::real_write("media::act");
    let s = session()?;
    let done = match action {
        Action::Previous => wait!(s.TrySkipPreviousAsync()?)?,
        Action::PlayPause => wait!(s.TryTogglePlayPauseAsync()?)?,
        Action::Next => wait!(s.TrySkipNextAsync()?)?,
    };
    if done { Ok(()) } else { Err(Error::new(HRESULT(0x8000_4005u32 as i32), "the player refused")) }
}

/// Картинка обложки → RGBA. Размер не трогаем — egui сам сожмёт при показе; больше 4096 — не берём.
fn decode(stream: IRandomAccessStreamWithContentType) -> Option<(u32, u32, Arc<Vec<u8>>)> {
    unsafe {
        let istream: IStream = CreateStreamOverRandomAccessStream(&stream).ok()?;
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        let decoder =
            factory.CreateDecoderFromStream(&istream, std::ptr::null(), WICDecodeMetadataCacheOnDemand).ok()?;
        let frame = decoder.GetFrame(0).ok()?;
        let converter = factory.CreateFormatConverter().ok()?;
        converter
            .Initialize(
                &frame,
                &GUID_WICPixelFormat32bppRGBA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
            .ok()?;
        let (mut w, mut h) = (0u32, 0u32);
        converter.GetSize(&mut w, &mut h).ok()?;
        if w == 0 || h == 0 || w > 4096 || h > 4096 {
            return None;
        }
        let mut buf = vec![0u8; (w * h * 4) as usize];
        converter.CopyPixels(std::ptr::null(), w * 4, &mut buf).ok()?;
        Some((w, h, Arc::new(buf)))
    }
}
