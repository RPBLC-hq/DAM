use super::*;
use std::{
    fs,
    process::{Child, Command, Stdio},
};

use muda::{Menu, MenuEvent, MenuItem};
use tao::{
    dpi::{LogicalSize, PhysicalPosition},
    event::{Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::{Window, WindowBuilder},
};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use wry::{NewWindowResponse, WebViewBuilder};

const INITIAL_POPOVER_WIDTH: f64 = 430.0;
const INITIAL_POPOVER_HEIGHT: f64 = 720.0;
const POPOVER_MARGIN: f64 = 8.0;
const RPBLC_HOME_URL: &str = "https://rpblc.com";
const TRAY_OPEN_RPBLC_MESSAGE: &str = "dam-tray:open-rpblc";
const TRAY_OPEN_DAM_WEB_MESSAGE: &str = "dam-tray:open-dam-web";
const TRAY_CONNECT_MESSAGE: &str = "dam-tray:connect";
const TRAY_QUIT_MESSAGE: &str = "dam-tray:quit";

#[derive(Debug)]
enum UserEvent {
    TrayIcon(TrayIconEvent),
    Menu(MenuEvent),
    OpenRpblc,
    OpenDamWeb,
    ConnectRequested,
    QuitRequested,
}

pub(super) fn run(cli: CliArgs) -> Result<(), String> {
    if cli.activate_system_extension.is_some() || cli.deactivate_system_extension.is_some() {
        return Err(
            "System Extension activation is only available in the macOS DAM tray".to_string(),
        );
    }

    let addr = choose_web_addr(cli.addr.as_deref())?;
    let url = connect_url(&addr);
    let data_paths = data_paths(&cli)?;
    fs::create_dir_all(&data_paths.state_dir).map_err(|error| {
        format!(
            "failed to create DAM state directory {}: {error}",
            data_paths.state_dir.display()
        )
    })?;

    let dam_bin = sibling_or_path(cli.dam_bin.clone(), DAM_BIN_ENV, "dam");
    let dam_web_bin = sibling_or_path(cli.dam_web_bin.clone(), DAM_WEB_BIN_ENV, "dam-web");
    let tray_post_token = generate_tray_post_token()?;
    let mut web_child = WebChild::spawn(
        &dam_web_bin,
        &dam_bin,
        &addr,
        &data_paths,
        cli.config_path.as_ref(),
        &tray_post_token,
    )?;
    wait_for_tcp(&addr, Duration::from_secs(8))?;

    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    TrayIconEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::TrayIcon(event));
    }));
    let menu_proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(UserEvent::Menu(event));
    }));
    let ipc_proxy = event_loop.create_proxy();
    let navigation_authority = addr.clone();
    let ipc_authority = addr.clone();
    let web_home_url = format!("http://{addr}/");

    let window = WindowBuilder::new()
        .with_title("DAM")
        .with_inner_size(LogicalSize::new(
            INITIAL_POPOVER_WIDTH,
            INITIAL_POPOVER_HEIGHT,
        ))
        .with_resizable(true)
        .with_minimizable(false)
        .with_maximizable(false)
        .with_closable(false)
        .with_visible(false)
        .with_decorations(false)
        .with_always_on_top(true)
        .build(&event_loop)
        .map_err(|error| format!("failed to create DAM window: {error}"))?;

    let webview = WebViewBuilder::new()
        .with_url(&url)
        .with_navigation_handler(move |target| url_has_local_origin(&target, &navigation_authority))
        .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
        .with_ipc_handler(move |request| {
            if !url_has_local_origin(&request.uri().to_string(), &ipc_authority) {
                return;
            }
            match request.body().trim() {
                TRAY_OPEN_RPBLC_MESSAGE => {
                    let _ = ipc_proxy.send_event(UserEvent::OpenRpblc);
                }
                TRAY_OPEN_DAM_WEB_MESSAGE => {
                    let _ = ipc_proxy.send_event(UserEvent::OpenDamWeb);
                }
                TRAY_CONNECT_MESSAGE => {
                    let _ = ipc_proxy.send_event(UserEvent::ConnectRequested);
                }
                TRAY_QUIT_MESSAGE => {
                    let _ = ipc_proxy.send_event(UserEvent::QuitRequested);
                }
                _ => {}
            }
        })
        .build(&window)
        .map_err(|error| format!("failed to create DAM webview: {error}"))?;

    let quit_item = MenuItem::with_id("dam-tray.quit", "Quit DAM", true, None);
    let quit_item_id = quit_item.id().clone();
    let tray_menu = Menu::new();
    if let Err(error) = tray_menu.append(&quit_item) {
        eprintln!("failed to build tray menu: {error}");
    }
    let dam_bin_for_connect = dam_bin.clone();
    let data_paths_for_connect = data_paths.clone();
    let config_path_for_connect = cli.config_path.clone();
    let mut tray_icon = None;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::NewEvents(StartCause::Init) => {
                if tray_icon.is_none() {
                    match build_tray(tray_menu.clone()) {
                        Ok(icon) => tray_icon = Some(icon),
                        Err(error) => {
                            eprintln!("{error}");
                            web_child.stop();
                            *control_flow = ControlFlow::Exit;
                        }
                    }
                }
            }
            Event::UserEvent(UserEvent::TrayIcon(TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            })) => show_popover(&window),
            Event::UserEvent(UserEvent::TrayIcon(_)) => {}
            Event::UserEvent(UserEvent::Menu(event)) => {
                if event.id == quit_item_id {
                    web_child.stop();
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::UserEvent(UserEvent::OpenRpblc) => {
                if let Err(error) = open_in_browser(RPBLC_HOME_URL) {
                    eprintln!("{error}");
                }
            }
            Event::UserEvent(UserEvent::OpenDamWeb) => {
                if let Err(error) = open_in_browser(&web_home_url) {
                    eprintln!("{error}");
                }
            }
            Event::UserEvent(UserEvent::ConnectRequested) => {
                let redirect = connect_result_redirect(connect_dam(
                    &dam_bin_for_connect,
                    &data_paths_for_connect,
                    config_path_for_connect.as_ref(),
                ));
                let script = format!("window.location.href = {}", js_string_literal(&redirect));
                if let Err(error) = webview.evaluate_script(&script) {
                    eprintln!("failed to refresh DAM tray view: {error}");
                }
            }
            Event::UserEvent(UserEvent::QuitRequested) => {
                web_child.stop();
                *control_flow = ControlFlow::Exit;
            }
            Event::WindowEvent {
                event: WindowEvent::Focused(false),
                ..
            }
            | Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                window.set_visible(false);
            }
            _ => {}
        }
    });
}

fn build_tray(menu: Menu) -> Result<tray_icon::TrayIcon, String> {
    TrayIconBuilder::new()
        .with_tooltip("DAM")
        .with_title("[R:]")
        .with_icon(dam_icon()?)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .map_err(|error| format!("failed to create tray icon: {error}"))
}

fn dam_icon() -> Result<Icon, String> {
    // Minimal high-contrast 32x32 bracket-mark placeholder. The canonical
    // branded icon should come from RPBLC.Design later; this keeps the Linux
    // tray visible in AppIndicator/status areas.
    let size = 32_u32;
    let mut rgba = vec![0_u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let idx = ((y * size + x) * 4) as usize;
            let border = x < 3 || x >= size - 3 || y < 3 || y >= size - 3;
            let mark = (x == 10 && (8..=24).contains(&y))
                || (x == 21 && (8..=24).contains(&y))
                || ((10..=21).contains(&x) && (y == 8 || y == 24));
            if border || mark {
                rgba[idx..idx + 4].copy_from_slice(&[0x2d, 0xe2, 0x7d, 0xff]);
            } else {
                rgba[idx..idx + 4].copy_from_slice(&[0x12, 0x17, 0x1f, 0xff]);
            }
        }
    }
    Icon::from_rgba(rgba, size, size)
        .map_err(|error| format!("failed to build DAM tray icon: {error}"))
}

fn show_popover(window: &Window) {
    position_popover(window);
    window.set_visible(true);
    window.set_focus();
}

fn position_popover(window: &Window) {
    let monitor = window
        .current_monitor()
        .or_else(|| window.primary_monitor());
    let scale = monitor
        .as_ref()
        .map(|monitor| monitor.scale_factor())
        .unwrap_or_else(|| window.scale_factor());
    let margin = (POPOVER_MARGIN * scale).round() as i32;
    let Some(monitor) = monitor else {
        window.set_outer_position(PhysicalPosition::new(margin, margin));
        return;
    };
    let position = monitor.position();
    let size = monitor.size();
    let popover = window.outer_size();
    let x = position.x + size.width as i32 - popover.width as i32 - margin;
    let y = position.y + margin;
    window.set_outer_position(PhysicalPosition::new(x.max(position.x + margin), y));
}

fn connect_result_redirect(result: Result<(), String>) -> String {
    match result {
        Ok(()) => format!(
            "/connect?notice={}",
            form_url_encode_component("DAM connected")
        ),
        Err(error) => {
            eprintln!("{error}");
            format!(
                "/connect?error={}",
                form_url_encode_component(&format!("Connect failed: {error}"))
            )
        }
    }
}

fn connect_dam(
    dam_bin: &PathBuf,
    data_paths: &DataPaths,
    config_path: Option<&PathBuf>,
) -> Result<(), String> {
    let has_active_profile = enabled_profile_selected(dam_bin, data_paths)?;
    run_dam_command(
        dam_bin,
        data_paths,
        &connect_args(data_paths, config_path, has_active_profile),
        "connect DAM",
    )
}

fn connect_args(
    data_paths: &DataPaths,
    config_path: Option<&PathBuf>,
    has_active_profile: bool,
) -> Vec<String> {
    let mut args = vec!["connect".to_string()];
    if has_active_profile {
        args.push("--apply".to_string());
    }
    if let Some(config_path) = config_path {
        args.extend(["--config".to_string(), config_path.display().to_string()]);
    }
    args.extend([
        "--db".to_string(),
        data_paths.vault_path.display().to_string(),
        "--log".to_string(),
        data_paths.log_path.display().to_string(),
        "--consent-db".to_string(),
        data_paths.consent_path.display().to_string(),
        "--network-mode".to_string(),
        "explicit_proxy".to_string(),
        "--trust-mode".to_string(),
        "disabled".to_string(),
    ]);
    args
}

fn enabled_profile_selected(dam_bin: &PathBuf, data_paths: &DataPaths) -> Result<bool, String> {
    let output = Command::new(dam_bin)
        .arg("profile")
        .arg("status")
        .env(DAM_STATE_DIR_ENV, &data_paths.state_dir)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("failed to inspect active profile: {error}"))?;
    if !output.status.success() {
        return Err(command_error(
            "inspect active profile",
            &["profile".to_string(), "status".to_string()],
            &output,
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(profile_status_has_enabled_profile(&stdout))
}

fn profile_status_has_enabled_profile(output: &str) -> bool {
    output
        .lines()
        .find_map(|line| line.strip_prefix("enabled_profiles: "))
        .map(|profiles| profiles.trim() != "none")
        .unwrap_or_else(|| {
            output
                .lines()
                .find_map(|line| line.strip_prefix("active_profile: "))
                .map(|profile| profile.trim() != "none")
                .unwrap_or(false)
        })
}

fn run_dam_command(
    dam_bin: &PathBuf,
    data_paths: &DataPaths,
    args: &[String],
    label: &str,
) -> Result<(), String> {
    let output = Command::new(dam_bin)
        .args(args)
        .env(DAM_STATE_DIR_ENV, &data_paths.state_dir)
        .env(DAM_CONSENT_PATH_ENV, &data_paths.consent_path)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("failed to {label}: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_error(label, args, &output))
    }
}

fn command_error(label: &str, args: &[String], output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let message = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    if message.is_empty() {
        format!(
            "failed to {label}: dam {} exited with {}",
            args.join(" "),
            output.status
        )
    } else {
        format!("failed to {label}: {message}")
    }
}

fn open_in_browser(url: &str) -> Result<(), String> {
    let status = Command::new("xdg-open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("failed to open {url}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "failed to open {url}: command exited with {status}"
        ))
    }
}

fn form_url_encode_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char)
            }
            b' ' => encoded.push('+'),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn js_string_literal(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '<' => escaped.push_str("\\u003c"),
            '>' => escaped.push_str("\\u003e"),
            '&' => escaped.push_str("\\u0026"),
            _ => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}

fn url_has_local_origin(candidate: &str, allowed_authority: &str) -> bool {
    let Some(rest) = candidate.strip_prefix("http://") else {
        return false;
    };
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .filter(|authority| !authority.is_empty());
    authority == Some(allowed_authority)
}

fn generate_tray_post_token() -> Result<String, String> {
    use std::io::Read as _;
    let mut bytes = [0_u8; 24];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .map_err(|error| format!("failed to generate tray session token: {error}"))?;
    Ok(hex_encode(&bytes))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

struct WebChild {
    child: Option<Child>,
}

impl WebChild {
    fn spawn(
        dam_web_bin: &PathBuf,
        dam_bin: &PathBuf,
        addr: &str,
        data_paths: &DataPaths,
        config_path: Option<&PathBuf>,
        tray_post_token: &str,
    ) -> Result<Self, String> {
        let mut command = Command::new(dam_web_bin);
        if let Some(path) = config_path {
            command.arg("--config").arg(path);
        }
        command
            .arg("--addr")
            .arg(addr)
            .arg("--db")
            .arg(&data_paths.vault_path)
            .arg("--log")
            .arg(&data_paths.log_path)
            .arg("--consent-db")
            .arg(&data_paths.consent_path)
            .env(DAM_BIN_ENV, dam_bin)
            .env(DAM_STATE_DIR_ENV, &data_paths.state_dir)
            .env(DAM_CONSENT_PATH_ENV, &data_paths.consent_path)
            .env(DAM_WEB_SHELL_ENV, DAM_WEB_SHELL_TRAY)
            .env(DAM_WEB_TRAY_POST_TOKEN_ENV, tray_post_token)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        let child = command.spawn().map_err(|error| {
            format!(
                "failed to start dam-web from {}: {error}",
                dam_web_bin.display()
            )
        })?;
        Ok(Self { child: Some(child) })
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for WebChild {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
#[path = "linux_tests.rs"]
mod tests;
