//! Turns a running process into the app a person recognises: its name, its icon,
//! and one identity shared by all of its processes.

use std::{collections::HashMap, path::Path};

use base64::{Engine, engine::general_purpose::STANDARD};
use windows::{
    Win32::{
        Foundation::{CloseHandle, MAX_PATH},
        Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
            DeleteObject, GetDIBits,
        },
        Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW},
        System::Threading::{
            OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
            QueryFullProcessImageNameW,
        },
        UI::WindowsAndMessaging::{
            DestroyIcon, GetIconInfo, HICON, ICONINFO, PrivateExtractIconsW,
        },
    },
    core::{HSTRING, PCWSTR, PWSTR},
};

/// Large enough to stay sharp on a high resolution display.
const ICON_SIZE: i32 = 64;

#[derive(Clone)]
pub struct App {
    /// The program's path. Every process of an app shares it.
    pub id: String,
    pub name: String,
    /// A PNG as a data URL, when the program has an icon.
    pub icon: Option<String>,
}

/// Reading a name and icon means opening the program, so each is read once.
#[derive(Default)]
pub struct Apps(HashMap<String, App>);

impl Apps {
    pub fn of_process(&mut self, process_id: u32) -> Option<&App> {
        let path = path_of(process_id)?;
        let id = path.to_lowercase();
        Some(self.0.entry(id.clone()).or_insert_with(|| App {
            name: description_of(&path).unwrap_or_else(|| file_stem(&path)),
            icon: icon_of(&path),
            id,
        }))
    }
}

fn file_stem(path: &str) -> String {
    Path::new(path).file_stem().map_or_else(
        || path.to_owned(),
        |stem| stem.to_string_lossy().into_owned(),
    )
}

fn path_of(process_id: u32) -> Option<String> {
    let mut buffer = [0u16; 1024];
    let mut length = buffer.len() as u32;
    // SAFETY: the handle is closed here, and the buffer outlives the call that
    // fills it and reports how much it wrote.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).ok()?;
        let read = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        );
        let _ = CloseHandle(process);
        read.ok()?;
    }
    Some(String::from_utf16_lossy(&buffer[..length as usize]))
}

/// The name a program gives itself, like "Google Chrome" for chrome.exe.
fn description_of(path: &str) -> Option<String> {
    let wide = HSTRING::from(path);
    // SAFETY: `data` is sized as Windows asked, and every pointer read back
    // points inside it, with the length Windows reported.
    unsafe {
        let size = GetFileVersionInfoSizeW(&wide, None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(&wide, None, size, data.as_mut_ptr().cast()).ok()?;

        let mut pointer = std::ptr::null_mut();
        let mut length = 0;
        let found = VerQueryValueW(
            data.as_ptr().cast(),
            &HSTRING::from("\\VarFileInfo\\Translation"),
            &mut pointer,
            &mut length,
        );
        if !found.as_bool() || length < 4 {
            return None;
        }
        let languages = pointer.cast::<u16>();
        let (language, code_page) = (*languages, *languages.add(1));

        let key = format!("\\StringFileInfo\\{language:04x}{code_page:04x}\\FileDescription");
        let found = VerQueryValueW(
            data.as_ptr().cast(),
            &HSTRING::from(key),
            &mut pointer,
            &mut length,
        );
        if !found.as_bool() || length == 0 {
            return None;
        }
        let text = PCWSTR(pointer.cast()).to_string().ok()?;
        let text = text.trim();
        (!text.is_empty()).then(|| text.to_owned())
    }
}

fn icon_of(path: &str) -> Option<String> {
    let mut name = [0u16; MAX_PATH as usize];
    let wide: Vec<u16> = path.encode_utf16().collect();
    if wide.len() >= name.len() {
        return None;
    }
    name[..wide.len()].copy_from_slice(&wide);

    let mut icons = [HICON::default()];
    // SAFETY: the name is terminated, and the icon returned is released below.
    let found =
        unsafe { PrivateExtractIconsW(&name, 0, ICON_SIZE, ICON_SIZE, Some(&mut icons), None, 0) };
    if found == 0 || found == u32::MAX || icons[0].is_invalid() {
        return None;
    }
    let pixels = pixels_of(icons[0]);
    // SAFETY: this icon came from the call above and is not used again.
    let _ = unsafe { DestroyIcon(icons[0]) };
    png_data_url(&pixels?)
}

/// The icon as top-to-bottom RGBA.
fn pixels_of(icon: HICON) -> Option<Vec<u8>> {
    let mut info = ICONINFO::default();
    let mut bitmap = BITMAPINFO::default();
    bitmap.bmiHeader = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: ICON_SIZE,
        biHeight: -ICON_SIZE,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };
    let mut pixels = vec![0u8; (ICON_SIZE * ICON_SIZE * 4) as usize];

    // SAFETY: the buffer matches the size and format described above, and the
    // bitmaps and device context made here are released here.
    let copied = unsafe {
        GetIconInfo(icon, &mut info).ok()?;
        let device = CreateCompatibleDC(None);
        let copied = GetDIBits(
            device,
            info.hbmColor,
            0,
            ICON_SIZE as u32,
            Some(pixels.as_mut_ptr().cast()),
            &mut bitmap,
            DIB_RGB_COLORS,
        );
        let _ = DeleteDC(device);
        let _ = DeleteObject(info.hbmColor.into());
        let _ = DeleteObject(info.hbmMask.into());
        copied
    };
    if copied == 0 {
        return None;
    }

    // An old icon has no transparency at all, which would read as invisible.
    let has_alpha = pixels.chunks_exact(4).any(|pixel| pixel[3] != 0);
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        if !has_alpha {
            pixel[3] = 255;
        }
    }
    Some(pixels)
}

fn png_data_url(pixels: &[u8]) -> Option<String> {
    let mut encoded = Vec::new();
    let mut encoder = png::Encoder::new(&mut encoded, ICON_SIZE as u32, ICON_SIZE as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().ok()?.write_image_data(pixels).ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(encoded)
    ))
}
