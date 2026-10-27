//! Native Windows workspace: temporary references, edit controls and worker search.
use super::host::wide;
use mira_core::workspace::{SearchResult, Workspace};
use std::{
    ptr::{null, null_mut},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{DataExchange::*, Memory::*},
    UI::{Controls::EM_SETLIMITTEXT, Shell::*, WindowsAndMessaging::*},
};

pub struct Desktop {
    pub state: Workspace,
    pub shelf_offset: usize,
    pub mode: String,
    pub mascot_scale: f32,
    pub width: f32,
    pub height: f32,
    pub position: Option<(i32, i32)>,
    pub drag: Option<Drag>,
    pub menu_open: bool,
    pub clipboard: bool,
    pub suppress_up: bool,
    pub search_result: Option<SearchResult>,
    pub results: Arc<Mutex<Option<SearchResult>>>,
    pub generation: Arc<AtomicU64>,
    pub query: HWND,
    pub note: HWND,
    pub list: HWND,
    pub status: String,
    pub trail: std::collections::VecDeque<(Instant, i32, i32)>,
    pub next_roam: Instant,
    pub roam_target: Option<(i32, i32)>,
    pub thought: Option<(String, Instant)>,
    pub folder_result: Arc<Mutex<Option<String>>>,
    pub cpu_previous: Option<(u64, u64)>,
}
pub struct Drag {
    pub start: POINT,
    pub origin: POINT,
    pub at: Instant,
    pub last: POINT,
    pub vy: f32,
    pub moved: bool,
    pub resize: bool,
    pub width: f32,
    pub height: f32,
}
impl Desktop {
    pub fn new() -> Self {
        Self {
            state: Workspace::new(),
            shelf_offset: 0,
            mode: "follow".into(),
            mascot_scale: 1.,
            width: 640.,
            height: 420.,
            position: None,
            drag: None,
            menu_open: false,
            clipboard: true,
            suppress_up: false,
            search_result: None,
            results: Arc::new(Mutex::new(None)),
            generation: Arc::new(AtomicU64::new(0)),
            query: null_mut(),
            note: null_mut(),
            list: null_mut(),
            status: String::new(),
            trail: std::collections::VecDeque::new(),
            next_roam: Instant::now(),
            roam_target: None,
            thought: None,
            folder_result: Arc::new(Mutex::new(None)),
            cpu_previous: None,
        }
    }
    pub unsafe fn controls(&mut self, hwnd: HWND, open: bool, page: usize, scale: f32) {
        if self.note.is_null() {
            self.note = CreateWindowExW(
                WS_EX_CLIENTEDGE,
                wide("EDIT").as_ptr(),
                null(),
                WS_CHILD
                    | WS_VSCROLL
                    | ES_MULTILINE as u32
                    | ES_AUTOVSCROLL as u32
                    | ES_WANTRETURN as u32,
                0,
                0,
                0,
                0,
                hwnd,
                401usize as HMENU,
                null_mut(),
                null(),
            );
            SendMessageW(self.note, EM_SETLIMITTEXT, 65536, 0);
            self.query = CreateWindowExW(
                WS_EX_CLIENTEDGE,
                wide("EDIT").as_ptr(),
                null(),
                WS_CHILD | ES_AUTOHSCROLL as u32,
                0,
                0,
                0,
                0,
                hwnd,
                402usize as HMENU,
                null_mut(),
                null(),
            );
            SendMessageW(self.query, EM_SETLIMITTEXT, 1024, 0);
            self.list = CreateWindowExW(
                0,
                wide("LISTBOX").as_ptr(),
                null(),
                WS_CHILD | WS_VSCROLL | LBS_NOTIFY as u32 | LBS_NOINTEGRALHEIGHT as u32,
                0,
                0,
                0,
                0,
                hwnd,
                403usize as HMENU,
                null_mut(),
                null(),
            );
            let font = GetStockObject(DEFAULT_GUI_FONT);
            for control in [self.note, self.query, self.list] {
                SendMessageW(control, WM_SETFONT, font as usize, 1);
            }
        }
        let (w, h) = (self.width, self.height);
        MoveWindow(
            self.note,
            (22. * scale) as i32,
            (119. * scale) as i32,
            ((w - 44.) * scale) as i32,
            ((h - 177.) * scale) as i32,
            1,
        );
        MoveWindow(
            self.query,
            (22. * scale) as i32,
            (120. * scale) as i32,
            ((w - 160.) * scale) as i32,
            (34. * scale) as i32,
            1,
        );
        let list_y = if page == 8 { 172. } else { 122. };
        MoveWindow(
            self.list,
            (22. * scale) as i32,
            (list_y * scale) as i32,
            ((w - 44.) * scale) as i32,
            ((h - list_y - 52.) * scale) as i32,
            1,
        );
        ShowWindow(self.note, if open && page == 6 { SW_SHOW } else { SW_HIDE });
        ShowWindow(
            self.query,
            if open && page == 8 { SW_SHOW } else { SW_HIDE },
        );
        ShowWindow(
            self.list,
            if open && (page == 7 || page == 8) {
                SW_SHOW
            } else {
                SW_HIDE
            },
        );
        if open && page == 7 {
            self.populate_clips();
        }
    }
    pub unsafe fn populate_clips(&self) {
        SendMessageW(self.list, LB_RESETCONTENT, 0, 0);
        for clip in &self.state.clips {
            let s: String = clip.replace(['\r', '\n'], " ").chars().take(180).collect();
            SendMessageW(self.list, LB_ADDSTRING, 0, wide(&s).as_ptr() as isize);
        }
    }
    pub fn add(&mut self, paths: Vec<String>) {
        self.shelf_offset = 0;
        self.state.add_paths(&paths);
        self.status = self.state.message.clone();
    }
    pub fn search(&mut self) {
        let query = unsafe { super::host::window_text(self.query) };
        let roots = self.state.roots.clone();
        let generation = self.generation.clone();
        let result = self.results.clone();
        let id = generation.fetch_add(1, Ordering::Relaxed) + 1;
        self.status = "Caut… (maxim 3 secunde, 200 rezultate)".into();
        std::thread::spawn(move || {
            let r = mira_core::workspace::search(&roots, &query, &generation, id);
            if generation.load(Ordering::Relaxed) == id {
                if let Ok(mut out) = result.lock() {
                    *out = Some(r);
                }
            }
        });
    }
    pub unsafe fn clipboard_changed(&mut self, hwnd: HWND) {
        if !self.clipboard || OpenClipboard(hwnd) == 0 {
            return;
        }
        let handle = GetClipboardData(13); // CF_UNICODETEXT
        if !handle.is_null() {
            let ptr = GlobalLock(handle) as *const u16;
            if !ptr.is_null() {
                let mut len = 0;
                let bound = (GlobalSize(handle) / 2).min(32768);
                while len < bound && *ptr.add(len) != 0 {
                    len += 1;
                }
                let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
                self.state.push_clip(&text);
                GlobalUnlock(handle);
            }
        } else if IsClipboardFormatAvailable(15) != 0 {
            // CF_HDROP
            let h = GetClipboardData(15);
            let paths = drop_paths(h as HDROP);
            self.state.push_clip(&paths.join("\n"));
        }
        CloseClipboard();
    }
    pub unsafe fn copy(&self, hwnd: HWND, text: &str) {
        if OpenClipboard(hwnd) == 0 {
            return;
        }
        let data = wide(text);
        let memory = GlobalAlloc(GMEM_MOVEABLE, data.len() * 2);
        if !memory.is_null() {
            let target = GlobalLock(memory) as *mut u16;
            if !target.is_null() {
                std::ptr::copy_nonoverlapping(data.as_ptr(), target, data.len());
                GlobalUnlock(memory);
                EmptyClipboard();
                if SetClipboardData(13, memory).is_null() {
                    GlobalFree(memory);
                }
            } else {
                GlobalFree(memory);
            }
        }
        CloseClipboard();
    }
    pub unsafe fn open_selected(&self, hwnd: HWND, page: usize) {
        let index = SendMessageW(self.list, LB_GETCURSEL, 0, 0);
        if index < 0 {
            return;
        }
        if page == 7 {
            if let Some(text) = self.state.clips.get(index as usize) {
                self.copy(hwnd, text);
            }
        } else if let Some(hit) = self
            .search_result
            .as_ref()
            .and_then(|r| r.hits.get(index as usize))
        {
            open_path(hwnd, &hit.path);
        }
    }
    pub unsafe fn poll(&mut self) -> bool {
        let result = self.results.lock().ok().and_then(|mut r| r.take());
        if let Some(result) = result {
            self.status = result.error.clone().unwrap_or_else(|| {
                format!(
                    "{} potriviri · {} fișiere{}",
                    result.hits.len(),
                    result.files,
                    if result.limited { " · limitat" } else { "" }
                )
            });
            SendMessageW(self.list, LB_RESETCONTENT, 0, 0);
            for hit in &result.hits {
                let file = std::path::Path::new(&hit.path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                let row = format!("{file}:{}  {}", hit.line, hit.text);
                SendMessageW(self.list, LB_ADDSTRING, 0, wide(&row).as_ptr() as isize);
            }
            self.search_result = Some(result);
            return true;
        }
        false
    }
    pub unsafe fn clear(&mut self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.state.clear();
        self.shelf_offset = 0;
        self.search_result = None;
        if let Ok(mut r) = self.results.lock() {
            *r = None;
        }
        SetWindowTextW(self.note, wide("").as_ptr());
        SetWindowTextW(self.query, wide("").as_ptr());
        SendMessageW(self.list, LB_RESETCONTENT, 0, 0);
        self.status = "Memoria temporară a fost golită. Originalele sunt intacte.".into();
    }
    pub unsafe fn paint(&self, dc: HDC, scale: f32, gaze: (f32, f32), page: usize, t: f32) {
        let w = self.width;
        let h = self.height;
        // All rendering is in logical pixels, with GDI mapped once for DPI.
        SetMapMode(dc, MM_ANISOTROPIC);
        SetWindowExtEx(dc, 100, 100, null_mut());
        SetViewportExtEx(dc, (100. * scale) as i32, (100. * scale) as i32, null_mut());
        let background = CreateSolidBrush(color(13, 20, 29));
        let old = SelectObject(dc, background);
        let pen = SelectObject(dc, GetStockObject(NULL_PEN));
        RoundRect(dc, 0, 0, w as i32, h as i32, 36, 36);
        SelectObject(dc, old);
        DeleteObject(background);
        SelectObject(dc, pen);
        let brush = CreateSolidBrush(color(133, 239, 183));
        let old = SelectObject(dc, brush);
        let pen = SelectObject(dc, GetStockObject(NULL_PEN));
        for x in [28., 48.] {
            let x = (x + gaze.0 * 0.5) as i32;
            let y = (31. + gaze.1 * 0.5) as i32;
            RoundRect(dc, x, y, x + 8, y + 13, 8, 8);
        }
        SelectObject(dc, old);
        DeleteObject(brush);
        SelectObject(dc, pen);
        label(
            dc,
            "mira · spațiul tău temporar",
            76,
            20,
            w as i32 - 20,
            45,
            15,
            color(233, 245, 243),
        );
        label(
            dc,
            "Click dreapta: moduri · dublu click pe ramă: acasă · trage colțul ◢",
            22,
            52,
            w as i32 - 20,
            72,
            10,
            color(144, 170, 187),
        );
        for (i, name) in ["Raft", "Notițe", "Clipboard", "Căutare"]
            .iter()
            .enumerate()
        {
            let left = 22 + i as i32 * 116;
            label(
                dc,
                name,
                left,
                82,
                left + 110,
                108,
                12,
                if page == 5 + i {
                    color(133, 239, 183)
                } else {
                    color(157, 181, 199)
                },
            );
        }
        if page == 5 {
            if self.state.links.is_empty() {
                label(
                    dc,
                    "⇩ Trage fișiere sau foldere peste mine",
                    32,
                    140,
                    w as i32 - 30,
                    178,
                    20,
                    color(173, 231, 205),
                );
                label(
                    dc,
                    "Shortcuturi vizuale · originalele rămân la locul lor",
                    32,
                    187,
                    w as i32 - 30,
                    218,
                    12,
                    color(151, 175, 192),
                );
            }
            for (index, link) in self
                .state
                .links
                .iter()
                .skip(self.shelf_offset)
                .take(((h - 170.) / 55.).max(1.) as usize * 2)
                .enumerate()
            {
                let columns = 2;
                let col = index % columns;
                let row = index / columns;
                let card_w = (w - 58.) / 2.;
                let left = 22. + col as f32 * (card_w + 14.);
                let bounce = (t * 1.8 + index as f32 * 0.7).sin() * 2.;
                let top = 124. + row as f32 * 55. + bounce;
                if top + 48. > h - 45. {
                    break;
                }
                let b = CreateSolidBrush(color(28, 44, 58));
                let old = SelectObject(dc, b);
                let p = SelectObject(dc, GetStockObject(NULL_PEN));
                RoundRect(
                    dc,
                    left as i32,
                    top as i32,
                    (left + card_w) as i32,
                    (top + 47.) as i32,
                    18,
                    18,
                );
                SelectObject(dc, old);
                SelectObject(dc, p);
                DeleteObject(b);
                label(
                    dc,
                    if link.directory { "▣" } else { "▤" },
                    left as i32 + 12,
                    top as i32 + 10,
                    left as i32 + 40,
                    top as i32 + 38,
                    18,
                    color(133, 239, 183),
                );
                label(
                    dc,
                    &link.name,
                    left as i32 + 44,
                    top as i32 + 7,
                    (left + card_w - 8.) as i32,
                    top as i32 + 38,
                    11,
                    color(229, 240, 246),
                );
            }
        } else if page == 8 {
            label(
                dc,
                "Caută →",
                w as i32 - 127,
                124,
                w as i32 - 20,
                152,
                12,
                color(133, 239, 183),
            );
        }
        label(
            dc,
            if self.status.is_empty() {
                "Doar în RAM · Ctrl+V în notițe · click pe fișier pentru a-l deschide"
            } else {
                &self.status
            },
            22,
            h as i32 - 37,
            w as i32 - 26,
            h as i32 - 12,
            10,
            color(151, 175, 192),
        );
        label(
            dc,
            "◢",
            w as i32 - 24,
            h as i32 - 24,
            w as i32 - 3,
            h as i32,
            14,
            color(133, 239, 183),
        );
        SetMapMode(dc, MM_TEXT);
    }
    pub fn folder_under(&self, x: i32, y: i32) {
        let result = self.folder_result.clone();
        // Only after an explicit robot drag. No persistent process/window scraping.
        std::thread::spawn(move || {
            use std::os::windows::process::CommandExt;
            let script = include_str!("folder-under-pointer.ps1");
            if let Ok(out) = std::process::Command::new("powershell.exe")
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    &format!("& {{ {script} }} {x} {y}"),
                ])
                .creation_flags(0x08000000)
                .output()
            {
                if out.status.success() {
                    let path = String::from_utf8_lossy(&out.stdout).trim().to_owned();
                    if std::path::Path::new(&path).is_dir() {
                        if let Ok(mut r) = result.lock() {
                            *r = Some(path);
                        }
                    }
                }
            }
        });
    }
}
pub unsafe fn drop_paths(drop: HDROP) -> Vec<String> {
    let count = DragQueryFileW(drop, u32::MAX, null_mut(), 0).min(64);
    let mut paths = Vec::new();
    for i in 0..count {
        let size = DragQueryFileW(drop, i, null_mut(), 0);
        let mut buf = vec![0; size as usize + 1];
        DragQueryFileW(drop, i, buf.as_mut_ptr(), size + 1);
        paths.push(String::from_utf16_lossy(&buf[..size as usize]));
    }
    paths
}
pub unsafe fn open_path(hwnd: HWND, path: &str) {
    ShellExecuteW(
        hwnd,
        wide("open").as_ptr(),
        wide(path).as_ptr(),
        null(),
        null(),
        SW_SHOWNORMAL,
    );
}
#[allow(clippy::too_many_arguments)] // Low-level GDI text primitive: rectangle, font size and color.
unsafe fn label(
    dc: HDC,
    text: &str,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    size: i32,
    color: u32,
) {
    let font = CreateFontW(
        -size,
        0,
        0,
        0,
        FW_NORMAL as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        0,
        0,
        CLEARTYPE_QUALITY as u32,
        0,
        wide("Segoe UI").as_ptr(),
    );
    let old = SelectObject(dc, font);
    SetTextColor(dc, color);
    SetBkMode(dc, TRANSPARENT as i32);
    let mut rect = RECT {
        left,
        top,
        right,
        bottom,
    };
    let text = wide(text);
    DrawTextW(
        dc,
        text.as_ptr(),
        (text.len() - 1) as i32,
        &mut rect,
        DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );
    SelectObject(dc, old);
    DeleteObject(font);
}
fn color(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | ((g as u32) << 8) | ((b as u32) << 16)
}
