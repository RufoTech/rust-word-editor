#![windows_subsystem = "windows"]

mod app;
mod backstage;
mod commands;
mod docx;
mod dropdown;
mod editor;
mod format;
mod ribbon;
mod shape;
mod statusbar;
mod theme;
mod util;

use windows::Win32::Foundation::HINSTANCE;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;

fn main() {
    unsafe {
        let hinst = HINSTANCE(GetModuleHandleW(None).unwrap_or_default().0);
        if let Err(e) = app::run(hinst) {
            let msg = format!("Startup failed: {:?}", e);
            let w = util::to_w(&msg);
            windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                windows::Win32::Foundation::HWND::default(),
                windows::core::PCWSTR(w.as_ptr()),
                windows::core::w!("Rust Word"),
                windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
            );
        }
    }
}
