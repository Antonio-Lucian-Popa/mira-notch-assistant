//! Native GDI host. No browser runtime; all state lives on the UI thread.
use mira_core::{
    pomodoro::PomodoroEngine,
    store::{self, Config, Store},
    tokens::Tokens,
};
use std::{
    cell::RefCell,
    ptr::{null, null_mut},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{LibraryLoader::GetModuleHandleW, SystemInformation::GetLocalTime},
    UI::{HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

struct Host {
    tokens: Tokens,
    config: Config,
    store: Store,
    focus: PomodoroEngine,
    open: bool,
    page: usize,
    gaze: (f32, f32),
    last_second: Instant,
    away: Option<Instant>,
    blink: Instant,
    closed_blink: bool,
    clock: String,
    day: String,
    water: u32,
    scale: f32,
    error: Option<String>,
}
thread_local! { static HOST: RefCell<Option<Host>> = const { RefCell::new(None) }; }
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn rgb(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | ((g as u32) << 8) | ((b as u32) << 16)
}

impl Host {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let config = store::config_path().and_then(|p| Config::load_or_default(&p))?;
        let db = Store::open(&store::db_path()?)?;
        Ok(Self {
            tokens: Tokens::embedded()?,
            focus: PomodoroEngine::new(config.pomodoro_config()),
            config,
            store: db,
            open: false,
            page: 0,
            gaze: (0., 0.),
            last_second: Instant::now() - Duration::from_secs(1),
            away: None,
            blink: Instant::now(),
            closed_blink: false,
            clock: String::new(),
            day: String::new(),
            water: 0,
            scale: 1.,
            error: None,
        })
    }
    fn ro(&self) -> bool {
        matches!(self.config.language, store::Language::Ro)
    }
    fn tr<'a>(&self, ro: &'a str, en: &'a str) -> &'a str {
        if self.ro() {
            ro
        } else {
            en
        }
    }
    fn size(&self) -> (i32, i32) {
        let size = if self.open {
            self.tokens.notch.hover
        } else {
            self.tokens.notch.closed
        };
        (
            (size.width * self.scale) as i32,
            (size.height * self.scale) as i32,
        )
    }
    unsafe fn layout(&mut self, hwnd: HWND) {
        self.scale = GetDpiForWindow(hwnd).max(96) as f32 / 96.;
        let (w, h) = self.size();
        // Windows primary monitor has its origin at (0, 0).
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            (GetSystemMetrics(SM_CXSCREEN) - w) / 2,
            0,
            w,
            h,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        let radius = (if self.open {
            self.tokens.notch.hover.radius_bottom
        } else {
            self.tokens.notch.closed.radius_bottom
        } * self.scale) as i32;
        let region = CreateRoundRectRgn(0, -radius, w + 1, h + 1, radius * 2, radius * 2);
        if SetWindowRgn(hwnd, region, 1) == 0 {
            DeleteObject(region);
        }
        InvalidateRect(hwnd, null(), 0);
    }
    unsafe fn tick(&mut self, hwnd: HWND) {
        let now = Instant::now();
        let mut dirty = false;
        let mut point: POINT = std::mem::zeroed();
        let mut rect: RECT = std::mem::zeroed();
        GetWindowRect(hwnd, &mut rect);
        if GetCursorPos(&mut point) != 0 {
            let inside = point.x >= rect.left
                && point.x < rect.right
                && point.y >= rect.top
                && point.y < rect.bottom;
            if inside {
                self.away = None;
                if !self.open {
                    self.open = true;
                    self.layout(hwnd);
                    dirty = true;
                }
            } else if self.open {
                let since = *self.away.get_or_insert(now);
                if now.duration_since(since).as_millis()
                    >= self.tokens.notch.hover_close_delay_ms as u128
                {
                    self.open = false;
                    self.away = None;
                    self.layout(hwnd);
                    dirty = true;
                }
            }
            if !self.open {
                let dx = (point.x - (rect.left + rect.right) / 2) as f32 / self.scale;
                let dy = (point.y - (rect.top + rect.bottom) / 2) as f32 / self.scale;
                let distance = dx.hypot(dy).max(1.);
                let strength = (distance / 600.).min(1.);
                let target = (dx / distance * strength * 8., dy / distance * strength * 5.);
                let next = (
                    self.gaze.0 + (target.0 - self.gaze.0) * 0.3,
                    self.gaze.1 + (target.1 - self.gaze.1) * 0.3,
                );
                if (next.0 - self.gaze.0).abs() + (next.1 - self.gaze.1).abs() > 0.01 {
                    self.gaze = next;
                    dirty = true;
                }
            }
        }
        let blinking = now.duration_since(self.blink).as_millis() >= 4000;
        if blinking != self.closed_blink {
            self.closed_blink = blinking;
            dirty = true;
        }
        if now.duration_since(self.blink).as_millis() >= 4140 {
            self.blink = now;
        }
        let seconds = now.duration_since(self.last_second).as_secs();
        if seconds > 0 {
            self.last_second += Duration::from_secs(seconds);
            for _ in 0..seconds {
                self.focus.tick();
            }
            let mut time: SYSTEMTIME = std::mem::zeroed();
            GetLocalTime(&mut time);
            self.clock = format!("{:02}:{:02}", time.wHour, time.wMinute);
            let day = format!("{:04}-{:02}-{:02}", time.wYear, time.wMonth, time.wDay);
            if day != self.day {
                self.day = day;
                self.water = self.store.water_glasses(&self.day).unwrap_or(0);
            }
            dirty |= self.open;
        }
        if dirty {
            InvalidateRect(hwnd, null(), 0);
        }
    }
    unsafe fn click(&mut self, hwnd: HWND, x: i32, y: i32) {
        if !self.open {
            return;
        }
        self.error = None;
        let x = x as f32 / self.scale;
        let y = y as f32 / self.scale;
        let w = self.tokens.notch.hover.width;
        if y < 50. {
            self.page = (self.page + 1) % 3;
        } else if (105. ..160.).contains(&y) {
            match self.page {
                0 => {
                    let key = if x < w / 3. {
                        VK_MEDIA_PREV_TRACK
                    } else if x > w * 2. / 3. {
                        VK_MEDIA_NEXT_TRACK
                    } else {
                        VK_MEDIA_PLAY_PAUSE
                    };
                    keybd_event(key as u8, 0, 0, 0);
                    keybd_event(key as u8, 0, KEYEVENTF_KEYUP, 0);
                }
                1 => {
                    if x > w * 2. / 3. {
                        self.focus.reset();
                    } else {
                        self.focus.toggle();
                    }
                }
                _ => {
                    let delta = if x < w / 2. { -1 } else { 1 };
                    match self.store.water_adjust(&self.day, delta) {
                        Ok(value) => self.water = value,
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
            }
        }
        InvalidateRect(hwnd, null(), 0);
    }
    unsafe fn paint(&self, hwnd: HWND) {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let dc = BeginPaint(hwnd, &mut ps);
        let (w, h) = self.size();
        // Double buffering keeps blinking and pointer motion flicker-free.
        let buffer = CreateCompatibleDC(dc);
        let bitmap = CreateCompatibleBitmap(dc, w, h);
        let old_bitmap = SelectObject(buffer, bitmap);
        let bg = CreateSolidBrush(rgb(10, 10, 11));
        let bounds = RECT {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        };
        FillRect(buffer, &bounds, bg);
        DeleteObject(bg);
        SetBkMode(buffer, TRANSPARENT as i32);
        if !self.open {
            let brush = CreateSolidBrush(rgb(143, 176, 255));
            let old_brush = SelectObject(buffer, brush);
            let old_pen = SelectObject(buffer, GetStockObject(NULL_PEN));
            let eye_h = if self.closed_blink { 1.5 } else { 9. };
            for offset in [-10., 10.] {
                let cx = w as f32 / 2. + (offset + self.gaze.0) * self.scale;
                let cy = h as f32 / 2. + self.gaze.1 * self.scale;
                Ellipse(
                    buffer,
                    (cx - 4.5 * self.scale) as i32,
                    (cy - eye_h / 2. * self.scale) as i32,
                    (cx + 4.5 * self.scale) as i32,
                    (cy + eye_h / 2. * self.scale) as i32,
                );
            }
            SelectObject(buffer, old_brush);
            SelectObject(buffer, old_pen);
            DeleteObject(brush);
        } else {
            let title = match self.page {
                0 => self.tr("Muzică", "Music"),
                1 => "Pomodoro",
                _ => self.tr("Apă", "Water"),
            };
            self.text(
                buffer,
                &format!("Mira  •  {}  •  {}    ›", self.clock, title),
                12,
                42,
                15,
                rgb(242, 242, 243),
            );
            let (body, buttons) = match self.page {
                0 => (
                    self.tr(
                        "Controlează playerul activ",
                        "Control the active media player",
                    )
                    .to_owned(),
                    self.tr(
                        "Înapoi                 Redare / Pauză                 Înainte",
                        "Previous                 Play / Pause                 Next",
                    )
                    .to_owned(),
                ),
                1 => {
                    let f = self.focus.snapshot();
                    (
                        format!(
                            "{:?}   {:02}:{:02}",
                            f.phase,
                            f.remaining_secs / 60,
                            f.remaining_secs % 60
                        ),
                        format!(
                            "{}                              Reset",
                            if f.running {
                                self.tr("Pauză", "Pause")
                            } else {
                                self.tr("Pornește", "Start")
                            }
                        ),
                    )
                }
                _ => (
                    format!(
                        "{} / {}   ({} ml)",
                        self.water,
                        self.config.water_goal,
                        self.water * 250
                    ),
                    self.tr(
                        "− un pahar                          + un pahar",
                        "− one glass                          + one glass",
                    )
                    .to_owned(),
                ),
            };
            self.text(buffer, &body, 56, 95, 22, rgb(143, 176, 255));
            self.text(buffer, &buttons, 110, 155, 16, rgb(242, 242, 243));
            self.text(
                buffer,
                self.error.as_deref().unwrap_or_else(|| {
                    self.tr(
                        "Click pe titlu: pagina următoare · Click dreapta: meniu",
                        "Click title: next page · Right-click: menu",
                    )
                }),
                164,
                190,
                11,
                rgb(154, 154, 160),
            );
        }
        BitBlt(dc, 0, 0, w, h, buffer, 0, 0, SRCCOPY);
        SelectObject(buffer, old_bitmap);
        DeleteObject(bitmap);
        DeleteDC(buffer);
        EndPaint(hwnd, &ps);
    }
    unsafe fn text(&self, dc: HDC, value: &str, top: i32, bottom: i32, size: i32, color: u32) {
        let font = CreateFontW(
            -(size as f32 * self.scale) as i32,
            0,
            0,
            0,
            400,
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
        let mut rect = RECT {
            left: (14. * self.scale) as i32,
            top: (top as f32 * self.scale) as i32,
            right: self.size().0 - (14. * self.scale) as i32,
            bottom: (bottom as f32 * self.scale) as i32,
        };
        let value = wide(value);
        DrawTextW(
            dc,
            value.as_ptr(),
            (value.len() - 1) as i32,
            &mut rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
        SelectObject(dc, old);
        DeleteObject(font);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER => {
            HOST.with(|s| {
                if let Some(s) = s.borrow_mut().as_mut() {
                    s.tick(hwnd);
                }
            });
            0
        }
        WM_PAINT => {
            HOST.with(|s| {
                if let Some(s) = s.borrow().as_ref() {
                    s.paint(hwnd);
                }
            });
            0
        }
        WM_ERASEBKGND => 1,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as isize,
        WM_LBUTTONUP => {
            HOST.with(|s| {
                if let Some(s) = s.borrow_mut().as_mut() {
                    s.click(
                        hwnd,
                        (lp & 0xffff) as i16 as i32,
                        ((lp >> 16) & 0xffff) as i16 as i32,
                    );
                }
            });
            0
        }
        WM_CONTEXTMENU => {
            let menu = CreatePopupMenu();
            AppendMenuW(menu, MF_STRING, 1, wide("Mira — Exit / Ieșire").as_ptr());
            let mut pt = std::mem::zeroed();
            GetCursorPos(&mut pt);
            SetForegroundWindow(hwnd);
            let choice = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_NONOTIFY,
                pt.x,
                pt.y,
                0,
                hwnd,
                null(),
            );
            DestroyMenu(menu);
            if choice == 1 {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_DISPLAYCHANGE | WM_DPICHANGED => {
            HOST.with(|s| {
                if let Some(s) = s.borrow_mut().as_mut() {
                    s.layout(hwnd);
                }
            });
            0
        }
        WM_DESTROY => {
            KillTimer(hwnd, 1);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

pub unsafe fn run() {
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    // A named window prevents two copies (including installer auto-launch).
    let class = wide("MiraNotchWindow");
    if !FindWindowW(class.as_ptr(), null()).is_null() {
        return;
    }
    let state = match Host::new() {
        Ok(state) => state,
        Err(e) => {
            MessageBoxW(
                null_mut(),
                wide(&e.to_string()).as_ptr(),
                wide("Mira startup failed").as_ptr(),
                MB_ICONERROR,
            );
            return;
        }
    };
    HOST.with(|s| *s.borrow_mut() = Some(state));
    let instance = GetModuleHandleW(null());
    let wc = WNDCLASSW {
        lpfnWndProc: Some(wndproc),
        hInstance: instance,
        lpszClassName: class.as_ptr(),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        ..std::mem::zeroed()
    };
    if RegisterClassW(&wc) == 0 {
        return;
    }
    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        class.as_ptr(),
        wide("Mira").as_ptr(),
        WS_POPUP,
        0,
        0,
        180,
        32,
        null_mut(),
        null_mut(),
        instance,
        null(),
    );
    if hwnd.is_null() {
        return;
    }
    HOST.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut() {
            s.layout(hwnd);
        }
    });
    SetTimer(hwnd, 1, 16, None);
    let mut message: MSG = std::mem::zeroed();
    while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
        TranslateMessage(&message);
        DispatchMessageW(&message);
    }
    HOST.with(|s| *s.borrow_mut() = None);
}
