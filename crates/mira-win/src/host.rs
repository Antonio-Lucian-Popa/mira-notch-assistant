//! Native GDI host. No browser runtime; all state lives on the UI thread.
use mira_core::{
    pomodoro::PomodoroEngine,
    reminder::{self, Reminder, Urgency},
    store::{self, Config, Store},
    tokens::Tokens,
};
use std::{
    cell::RefCell,
    ptr::{null, null_mut},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{LibraryLoader::GetModuleHandleW, SystemInformation::GetLocalTime},
    UI::{Controls::EM_SETLIMITTEXT, HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
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
    reminders: Vec<Reminder>,
    selected_reminder: usize,
    animation_clock: Instant,
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
        let reminders = db.reminders()?;
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
            reminders,
            selected_reminder: 0,
            animation_clock: Instant::now(),
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
            ((if !self.open && !self.reminders.is_empty() {
                300.
            } else {
                size.width
            }) * self.scale) as i32,
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
        let urgency = self.reminders.first().map(|r| r.urgency(epoch()));
        if matches!(
            urgency,
            Some(Urgency::Soon | Urgency::Urgent | Urgency::Due)
        ) && !self.open
        {
            // Redraw only during the brief agitation beat, with a calm pause
            // between beats. Parent gaze remains independent from this motion.
            let beat = self.animation_clock.elapsed().as_secs_f32() % 1.;
            dirty |= beat < 0.5;
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
            for r in &mut self.reminders {
                if !r.notified && r.due_epoch <= epoch() {
                    match self.store.mark_reminder_notified(r.id) {
                        Ok(()) => r.notified = true,
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
            }
            dirty |= self.open || !self.reminders.is_empty();
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
            self.page = (self.page + 1) % 4;
        } else if (105. ..160.).contains(&y) {
            match self.page {
                3 => {
                    let index = self
                        .selected_reminder
                        .min(self.reminders.len().saturating_sub(1));
                    if (w / 5. ..w * 2. / 5.).contains(&x) {
                        PostMessageW(hwnd, WM_APP + 2, 0, 0);
                    } else if x < w / 5. {
                        self.selected_reminder = index.saturating_sub(1);
                    } else if x >= w * 4. / 5. {
                        self.selected_reminder =
                            (index + 1).min(self.reminders.len().saturating_sub(1));
                    } else if let Some(r) = self.reminders.get(index) {
                        let result = if x < w * 3. / 5. {
                            self.store.snooze_reminder(r.id, epoch() + 300, epoch())
                        } else {
                            self.store.remove_reminder(r.id)
                        };
                        if let Err(e) = result {
                            self.error = Some(e.to_string());
                        }
                        self.reload_reminders(hwnd);
                    }
                }
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
            let next = self.reminders.first();
            let urgency = next.map(|r| r.urgency(epoch()));
            let color = match urgency {
                Some(Urgency::Due | Urgency::Urgent) => rgb(255, 120, 120),
                Some(Urgency::Soon) => rgb(255, 182, 92),
                _ => rgb(143, 176, 255),
            };
            let brush = CreateSolidBrush(color);
            let old_brush = SelectObject(buffer, brush);
            let old_pen = SelectObject(buffer, GetStockObject(NULL_PEN));
            let eye_h = if self.closed_blink { 1.5 } else { 9. };
            let phase = self.animation_clock.elapsed().as_secs_f32();
            let intensity = match urgency {
                Some(Urgency::Due | Urgency::Urgent) => 2.,
                Some(Urgency::Soon) if phase % 3. < 0.5 => 0.6,
                _ => 0.,
            };
            let beat = if phase % 1. < 0.45 {
                (phase * 32.).sin() * intensity
            } else {
                0.
            };
            for offset in [-10., 10.] {
                let cx = if next.is_some() {
                    40. * self.scale
                } else {
                    w as f32 / 2.
                } + (offset + self.gaze.0 + beat) * self.scale;
                let cy = h as f32 / 2. + (self.gaze.1 - beat.abs() * 0.5) * self.scale;
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
            if let Some(r) = next {
                let remaining = r.remaining(epoch());
                let text = format!(
                    "{}  ·  {}",
                    if remaining == 0 {
                        self.tr("Acum", "Now").to_owned()
                    } else {
                        reminder::countdown(remaining)
                    },
                    r.text
                );
                self.text_with_left(buffer, &text, (0, 32), (11, color), 78);
            }
        } else {
            let title = match self.page {
                0 => self.tr("Muzică", "Music"),
                1 => "Pomodoro",
                2 => self.tr("Apă", "Water"),
                _ => self.tr("Remindere", "Reminders"),
            };
            self.text(
                buffer,
                &format!("Mira  •  {}  •  {}    ›", self.clock, title),
                12,
                42,
                15,
                rgb(242, 242, 243),
            );
            if self.page == 3 {
                self.paint_reminders(buffer);
            } else {
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
            }
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
    unsafe fn reload_reminders(&mut self, hwnd: HWND) {
        match self.store.reminders() {
            Ok(items) => self.reminders = items,
            Err(e) => self.error = Some(e.to_string()),
        }
        self.selected_reminder = self
            .selected_reminder
            .min(self.reminders.len().saturating_sub(1));
        self.layout(hwnd);
    }

    unsafe fn paint_reminders(&self, dc: HDC) {
        let current = self.reminders.get(self.selected_reminder);
        let label = current
            .map(|r| {
                format!(
                    "{} / {}  ·  {}",
                    self.selected_reminder + 1,
                    self.reminders.len(),
                    r.text
                )
            })
            .unwrap_or_else(|| {
                self.tr(
                    "Niciun reminder. Adaugă ceva de ținut minte.",
                    "No reminders. Add something to remember.",
                )
                .into()
            });
        self.text(dc, &label, 48, 75, 14, rgb(242, 242, 243));
        let remaining = current.map(|r| r.remaining(epoch()));
        let count = remaining
            .map(|n| {
                if n == 0 {
                    self.tr("Acum", "Now").into()
                } else {
                    reminder::countdown(n)
                }
            })
            .unwrap_or_else(|| "—".into());
        let color = match current.map(|r| r.urgency(epoch())) {
            Some(Urgency::Due | Urgency::Urgent) => rgb(255, 120, 120),
            _ => rgb(255, 182, 92),
        };
        self.text(dc, &count, 76, 111, 30, color);
        self.text(
            dc,
            self.tr(
                "‹          + Nou          +5 min          ✓ Gata          ›",
                "‹          + New          +5 min          ✓ Done          ›",
            ),
            110,
            155,
            16,
            rgb(242, 242, 243),
        );
        if let Some(r) = current {
            let left = (24. * self.scale) as i32;
            let right = self.size().0 - left;
            let y = (155. * self.scale) as i32;
            let base = RECT {
                left,
                right,
                top: y,
                bottom: y + (3. * self.scale) as i32,
            };
            let brush = CreateSolidBrush(rgb(38, 38, 43));
            FillRect(dc, &base, brush);
            DeleteObject(brush);
            let progress = RECT {
                right: left + ((right - left) as f64 * r.fraction(epoch())) as i32,
                ..base
            };
            let brush = CreateSolidBrush(color);
            FillRect(dc, &progress, brush);
            DeleteObject(brush);
        }
    }

    unsafe fn text(&self, dc: HDC, value: &str, top: i32, bottom: i32, size: i32, color: u32) {
        self.text_with_left(dc, value, (top, bottom), (size, color), 14);
    }
    unsafe fn text_with_left(
        &self,
        dc: HDC,
        value: &str,
        vertical: (i32, i32),
        style: (i32, u32),
        left: i32,
    ) {
        let (top, bottom) = vertical;
        let (size, color) = style;
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
            left: (left as f32 * self.scale) as i32,
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
        message if message == WM_APP + 2 => {
            open_reminder_editor(hwnd);
            0
        }
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
    let editor_class = wide("MiraReminderEditor");
    let editor_wc = WNDCLASSW {
        lpfnWndProc: Some(editor_proc),
        lpszClassName: editor_class.as_ptr(),
        hbrBackground: (COLOR_WINDOW + 1) as HBRUSH,
        ..wc
    };
    RegisterClassW(&editor_wc);
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
        let editor = FindWindowW(editor_class.as_ptr(), null());
        if !editor.is_null() && IsDialogMessageW(editor, &message) != 0 {
            continue;
        }
        TranslateMessage(&message);
        DispatchMessageW(&message);
    }
    HOST.with(|s| *s.borrow_mut() = None);
}

fn epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

unsafe fn open_reminder_editor(owner: HWND) {
    let class = wide("MiraReminderEditor");
    let existing = FindWindowW(class.as_ptr(), null());
    if !existing.is_null() {
        SetForegroundWindow(existing);
        return;
    }
    let hwnd = CreateWindowExW(
        WS_EX_TOOLWINDOW,
        class.as_ptr(),
        wide("Mira · Reminder").as_ptr(),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        440,
        230,
        owner,
        null_mut(),
        GetModuleHandleW(null()),
        null(),
    );
    ShowWindow(hwnd, SW_SHOW);
    SetForegroundWindow(hwnd);
    SetFocus(GetDlgItem(hwnd, 101));
}

unsafe fn control(
    hwnd: HWND,
    kind: &str,
    text: &str,
    id: usize,
    rect: (i32, i32, i32, i32),
    style: u32,
) {
    let (x, y, w, h) = rect;
    let child = CreateWindowExW(
        0,
        wide(kind).as_ptr(),
        wide(text).as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        x,
        y,
        w,
        h,
        hwnd,
        id as HMENU,
        GetModuleHandleW(null()),
        null(),
    );
    SendMessageW(
        child,
        WM_SETFONT,
        GetStockObject(DEFAULT_GUI_FONT) as usize,
        1,
    );
}

unsafe fn edit_text(hwnd: HWND, id: i32) -> String {
    let child = GetDlgItem(hwnd, id);
    let mut value = vec![0u16; GetWindowTextLengthW(child) as usize + 1];
    let len = GetWindowTextW(child, value.as_mut_ptr(), value.len() as i32);
    String::from_utf16_lossy(&value[..len as usize])
}

unsafe extern "system" fn editor_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            control(
                hwnd,
                "STATIC",
                "Ce vrei să-ți amintești? / Reminder title",
                100,
                (16, 14, 395, 20),
                0,
            );
            control(
                hwnd,
                "EDIT",
                "",
                101,
                (16, 38, 395, 26),
                WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL as u32,
            );
            SendMessageW(GetDlgItem(hwnd, 101), EM_SETLIMITTEXT, 200, 0);
            control(
                hwnd,
                "STATIC",
                "În câte minute? / Minutes from now",
                102,
                (16, 76, 300, 20),
                0,
            );
            control(
                hwnd,
                "EDIT",
                "15",
                103,
                (16, 100, 100, 26),
                WS_BORDER | WS_TABSTOP | ES_NUMBER as u32,
            );
            control(
                hwnd,
                "BUTTON",
                "Salvează / Save",
                1,
                (145, 140, 140, 28),
                WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
            );
            control(
                hwnd,
                "BUTTON",
                "Anulează / Cancel",
                2,
                (295, 140, 115, 28),
                WS_TABSTOP,
            );
            0
        }
        WM_COMMAND => {
            match wp & 0xffff {
                1 => {
                    let title = edit_text(hwnd, 101);
                    let minutes = edit_text(hwnd, 103)
                        .parse::<i64>()
                        .ok()
                        .filter(|n| (1..=527040).contains(n));
                    let result = if let Some(minutes) = minutes {
                        HOST.with(|state| {
                            let mut state = state.borrow_mut();
                            match state.as_mut() {
                                Some(s) => {
                                    let now = epoch();
                                    let result =
                                        s.store.add_reminder(&title, now + minutes * 60, now);
                                    if result.is_ok() {
                                        s.reload_reminders(GetWindow(hwnd, GW_OWNER));
                                    }
                                    result.map(|_| ())
                                }
                                None => Err(mira_core::Error::other("Mira is closing")),
                            }
                        })
                    } else {
                        Err(mira_core::Error::other(
                            "Alege 1–527040 minute / Choose 1–527040 minutes",
                        ))
                    };
                    match result {
                        Ok(()) => {
                            DestroyWindow(hwnd);
                        }
                        Err(e) => {
                            MessageBoxW(
                                hwnd,
                                wide(&e.to_string()).as_ptr(),
                                wide("Mira").as_ptr(),
                                MB_ICONWARNING,
                            );
                        }
                    }
                }
                2 => {
                    DestroyWindow(hwnd);
                }
                _ => {}
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
