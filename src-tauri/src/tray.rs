use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};

const MAIN_WINDOW: &str = "main";
const TRAY_SYNC_EVENT: &str = "tray-sync";

#[derive(Debug, PartialEq, Eq)]
pub enum TrayMenuAction {
    Show,
    Sync,
    Exit,
    Ignore,
}

pub fn menu_action(id: &str) -> TrayMenuAction {
    match id {
        "show" => TrayMenuAction::Show,
        "sync" => TrayMenuAction::Sync,
        "quit" => TrayMenuAction::Exit,
        _ => TrayMenuAction::Ignore,
    }
}

pub fn show_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let sync = MenuItem::with_id(app, "sync", "立即同步", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出 LLM Usage", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &sync, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id("llm-usage-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("LLM Usage · 在线用量监控")
        .on_menu_event(|app, event| match menu_action(event.id().as_ref()) {
            TrayMenuAction::Show => show_main_window(app),
            TrayMenuAction::Sync => {
                show_main_window(app);
                let _ = app.emit(TRAY_SYNC_EVENT, ());
            }
            TrayMenuAction::Exit => app.exit(0),
            TrayMenuAction::Ignore => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let tray = builder.build(app)?;
    app.manage(tray);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_tray_menu_ids_to_actions() {
        assert_eq!(menu_action("show"), TrayMenuAction::Show);
        assert_eq!(menu_action("sync"), TrayMenuAction::Sync);
        assert_eq!(menu_action("quit"), TrayMenuAction::Exit);
        assert_eq!(menu_action("unexpected"), TrayMenuAction::Ignore);
    }
}
