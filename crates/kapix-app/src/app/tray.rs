//! Notification-area icon and its context menu.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyIcon, DestroyMenu, GetSystemMetrics, HMENU, MF_CHECKED,
    MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, PostMessageW, SM_CXSMICON, SetForegroundWindow,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WM_NULL,
};
use windows::core::PCWSTR;

use super::winutil::{cursor_pos, wide};
use super::{WM_TRAY, icon, with};

pub const IDM_REGION: u32 = 1;
pub const IDM_OCR: u32 = 2;
pub const IDM_FULLSCREEN: u32 = 3;
pub const IDM_WINDOW: u32 = 4;
pub const IDM_EDIT_CONFIG: u32 = 5;
pub const IDM_RELOAD: u32 = 6;
pub const IDM_STARTUP: u32 = 7;
pub const IDM_EXIT: u32 = 8;
/// Recent item `i` (0 = newest) is `IDM_RECENT + i`.
pub const IDM_RECENT: u32 = 100;

fn data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: with(|a| a.icon),
        ..Default::default()
    };
    let tip = format!("Kapix — {}", with(|a| a.cfg.hotkeys.region.clone()));
    for (d, s) in nid.szTip.iter_mut().zip(tip.encode_utf16().take(127)) {
        *d = s;
    }
    nid
}

/// Add the icon (also used to re-add it after Explorer restarts: `TaskbarCreated`).
pub fn add(hwnd: HWND) {
    if with(|a| a.icon.is_invalid()) {
        let icon = icon::make_icon(unsafe { GetSystemMetrics(SM_CXSMICON) });
        with(|a| a.icon = icon.unwrap_or_default());
    }
    unsafe {
        let _ = Shell_NotifyIconW(NIM_ADD, &data(hwnd));
    }
}

pub fn update_tip() {
    let hwnd = with(|a| a.hwnd);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &data(hwnd));
    }
}

pub fn remove(hwnd: HWND) {
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &data(hwnd));
        let icon = with(|a| std::mem::take(&mut a.icon));
        if !icon.is_invalid() {
            let _ = DestroyIcon(icon);
        }
    }
}

fn item(
    menu: HMENU,
    flags: windows::Win32::UI::WindowsAndMessaging::MENU_ITEM_FLAGS,
    id: usize,
    text: &str,
) {
    let w = wide(text);
    unsafe {
        let _ = AppendMenuW(menu, flags, id, PCWSTR(w.as_ptr()));
    }
}

/// Show the context menu at the cursor; returns the chosen command id (0 = dismissed).
/// No state borrow is held while the menu's modal loop runs.
pub fn show_menu(hwnd: HWND) -> u32 {
    let (cfg, startup) = (super::cfg(), kapix_win::startup::is_enabled());
    let recents: Vec<String> = with(|a| {
        a.recent
            .iter()
            .map(|r| {
                let t = r.taken;
                format!(
                    "{:02}:{:02}:{:02} — {}×{}",
                    t.hour, t.minute, t.second, r.width, r.height
                )
            })
            .collect()
    });
    unsafe {
        let (Ok(menu), Ok(sub)) = (CreatePopupMenu(), CreatePopupMenu()) else {
            return 0;
        };
        let hk = &cfg.hotkeys;
        item(
            menu,
            MF_STRING,
            IDM_REGION as usize,
            &format!("Capture region\t{}", hk.region),
        );
        item(
            menu,
            MF_STRING,
            IDM_OCR as usize,
            &format!("OCR text\t{}", hk.ocr),
        );
        item(
            menu,
            MF_STRING,
            IDM_FULLSCREEN as usize,
            &format!("Capture fullscreen\t{}", hk.fullscreen),
        );
        item(
            menu,
            MF_STRING,
            IDM_WINDOW as usize,
            &format!("Capture window\t{}", hk.active_window),
        );
        item(menu, MF_SEPARATOR, 0, "");
        if recents.is_empty() {
            item(sub, MF_STRING | MF_GRAYED, 0, "(none yet)");
        }
        for (i, label) in recents.iter().enumerate() {
            item(sub, MF_STRING, IDM_RECENT as usize + i, label);
        }
        item(menu, MF_POPUP, sub.0 as usize, "Recent captures");
        item(menu, MF_SEPARATOR, 0, "");
        item(menu, MF_STRING, IDM_EDIT_CONFIG as usize, "Edit config");
        item(menu, MF_STRING, IDM_RELOAD as usize, "Reload config");
        let flags = if startup {
            MF_STRING | MF_CHECKED
        } else {
            MF_STRING
        };
        item(menu, flags, IDM_STARTUP as usize, "Launch at startup");
        item(menu, MF_SEPARATOR, 0, "");
        item(menu, MF_STRING, IDM_EXIT as usize, "Exit");

        // The foreground trick makes the menu dismiss when the user clicks elsewhere.
        let _ = SetForegroundWindow(hwnd);
        let p = cursor_pos();
        let cmd = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD,
            p.x,
            p.y,
            None,
            hwnd,
            None,
        )
        .0 as u32;
        let _ = PostMessageW(Some(hwnd), WM_NULL, Default::default(), Default::default());
        let _ = DestroyMenu(menu); // also destroys the submenu
        cmd
    }
}
