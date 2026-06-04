//! Native Webby browser shell.
//!
//! The testable browser state lives in `webby_app::AppState`; this binary is a
//! thin adapter for a native framebuffer window.

use std::io::IsTerminal;
use std::process::ExitCode;

use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};
use webby_app::{AppState, ChromeAction, DEFAULT_WINDOW_HEIGHT, DEFAULT_WINDOW_WIDTH};
use webby_core::{WebbyError, WebbyResult};
use webby_state::ProfileStore;

fn main() -> ExitCode {
    let result = if std::io::stdout().is_terminal()
        || std::env::var_os("WEBBY_APP_FORCE_WINDOW").is_some()
    {
        run_window()
    } else {
        run_non_interactive_smoke()
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn run_non_interactive_smoke() -> WebbyResult<()> {
    let loader = webby_net::DefaultResourceLoader::new()?;
    let store = ProfileStore::default_user()?;
    let mut profile = store.load()?;
    let mut state = AppState::with_profile(&profile)?;
    configure_disk_cache(&mut state, &store, &profile)?;
    state.submit_address(&loader);
    state.record_successful_navigation(&store, &mut profile)?;
    let _frame = state.compose_frame()?;
    clear_on_exit_if_configured(&store, &mut profile)?;
    println!("webby_app: non-interactive startup smoke rendered one frame");
    Ok(())
}

fn run_window() -> WebbyResult<()> {
    let loader = webby_net::DefaultResourceLoader::new()?;
    let store = ProfileStore::default_user()?;
    let mut profile = store.load()?;
    let mut state = AppState::with_profile(&profile)?;
    configure_disk_cache(&mut state, &store, &profile)?;
    state.submit_address(&loader);
    state.record_successful_navigation(&store, &mut profile)?;

    let mut window = match Window::new(
        "Webby",
        DEFAULT_WINDOW_WIDTH,
        DEFAULT_WINDOW_HEIGHT,
        WindowOptions::default(),
    ) {
        Ok(window) => window,
        Err(error) => {
            eprintln!("webby_app: native window unavailable: {error}");
            clear_on_exit_if_configured(&store, &mut profile)?;
            return Ok(());
        }
    };
    let mut previous_left_mouse_down = false;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let (width, height) = window.get_size();
        state.resize(width, height);
        handle_keyboard(&mut state, &loader, &store, &mut profile, &mut window)?;
        handle_mouse_click(
            &mut state,
            &loader,
            &store,
            &mut profile,
            &window,
            &mut previous_left_mouse_down,
        )?;
        handle_scroll(&mut state, &window);
        handle_hover(&mut state, &window);

        update_window_frame(&state, &mut window)?;
    }

    clear_on_exit_if_configured(&store, &mut profile)?;
    Ok(())
}

fn configure_disk_cache(
    state: &mut AppState,
    store: &ProfileStore,
    profile: &webby_state::BrowserProfile,
) -> WebbyResult<()> {
    if profile.config.disk_cache_enabled {
        state.enable_disk_cache(store.cache_dir())?;
    }
    Ok(())
}

fn clear_on_exit_if_configured(
    store: &ProfileStore,
    profile: &mut webby_state::BrowserProfile,
) -> WebbyResult<()> {
    if profile.config.clear_data_on_exit {
        return store.clear_browsing_data(profile);
    }
    if profile.config.clear_cookies_on_exit {
        store.clear_cookies(profile)?;
    }
    Ok(())
}

fn handle_keyboard(
    state: &mut AppState,
    loader: &webby_net::DefaultResourceLoader,
    store: &ProfileStore,
    profile: &mut webby_state::BrowserProfile,
    window: &mut Window,
) -> WebbyResult<()> {
    for key in window.get_keys_pressed(KeyRepeat::Yes) {
        if handle_tab_keyboard_command(state, key, window) {
            continue;
        }
        if handle_ux_keyboard_command(state, key, window) {
            continue;
        }
        match key {
            Key::F1 => state.toggle_shortcut_help(),
            Key::F12 => state.toggle_debug_overlay(),
            Key::Tab => {
                if window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift) {
                    state.focus_previous_page_item();
                } else {
                    state.focus_next_page_item();
                }
            }
            Key::Enter => {
                if state.find_active {
                    state.close_find();
                } else if !state.chrome.address_focused && state.keyboard_focus.is_some() {
                    if state.activate_keyboard_focus(loader) {
                        state.record_successful_navigation(store, profile)?;
                    }
                } else {
                    let before_url = state.navigation.current_url.clone();
                    state.submit_address(loader);
                    if state.navigation.current_url.is_some()
                        && state.navigation.current_url != before_url
                    {
                        state.record_successful_navigation(store, profile)?;
                    }
                }
            }
            Key::Left if has_navigation_modifier(window) => {
                if state.go_back(loader) {
                    state.record_successful_navigation(store, profile)?;
                }
            }
            Key::Right if has_navigation_modifier(window) => {
                if state.go_forward(loader) {
                    state.record_successful_navigation(store, profile)?;
                }
            }
            Key::R if has_command_modifier(window) => {
                if state.reload(loader) {
                    state.record_successful_navigation(store, profile)?;
                }
            }
            Key::Backspace => state.backspace(),
            Key::Left => state.move_address_cursor_left(),
            Key::Right => state.move_address_cursor_right(),
            Key::Home => state.move_address_cursor_home(),
            Key::End => state.move_address_cursor_end(),
            Key::Space => {
                if !state.chrome.address_focused && state.press_space_on_keyboard_focus(loader) {
                    state.record_successful_navigation(store, profile)?;
                } else {
                    state.type_character(' ');
                }
            }
            Key::Up => state.keyboard_scroll_by(-48.0),
            Key::Down => state.keyboard_scroll_by(48.0),
            Key::PageUp => state.keyboard_scroll_by(-(state.page_viewport_height() as f32)),
            Key::PageDown => state.keyboard_scroll_by(state.page_viewport_height() as f32),
            key => {
                if let Some(character) = key_to_address_char(key, window) {
                    state.type_character(character);
                }
            }
        }
    }
    Ok(())
}

fn handle_ux_keyboard_command(state: &mut AppState, key: Key, window: &Window) -> bool {
    if !has_command_modifier(window) {
        return false;
    }
    match key {
        Key::A => {
            state.select_all_address();
            true
        }
        Key::C => state.copy_address_or_current_url(),
        Key::V => state.paste_address(),
        Key::F => {
            state.open_find();
            true
        }
        Key::L | Key::O => {
            state.select_all_address();
            true
        }
        _ => false,
    }
}

fn handle_tab_keyboard_command(state: &mut AppState, key: Key, window: &Window) -> bool {
    if !has_command_modifier(window) {
        return false;
    }

    match key {
        Key::T => apply_tab_command(state, TabCommand::New),
        Key::W => apply_tab_command(state, TabCommand::Close),
        Key::Tab => apply_tab_command(
            state,
            if window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift) {
                TabCommand::Previous
            } else {
                TabCommand::Next
            },
        ),
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TabCommand {
    New,
    Close,
    Next,
    Previous,
}

fn apply_tab_command(state: &mut AppState, command: TabCommand) -> bool {
    match command {
        TabCommand::New => {
            state.new_tab();
            true
        }
        TabCommand::Close => state.close_active_tab(),
        TabCommand::Next => state.next_tab(),
        TabCommand::Previous => state.previous_tab(),
    }
}

fn handle_mouse_click(
    state: &mut AppState,
    loader: &webby_net::DefaultResourceLoader,
    store: &ProfileStore,
    profile: &mut webby_state::BrowserProfile,
    window: &Window,
    previous_left_mouse_down: &mut bool,
) -> WebbyResult<()> {
    let left_down = window.get_mouse_down(MouseButton::Left);
    if left_down
        && !*previous_left_mouse_down
        && let Some((x, y)) = window.get_mouse_pos(MouseMode::Discard)
    {
        if let Some(action) = state.chrome_action_at(x, y) {
            let should_record = match action {
                ChromeAction::BookmarkCurrentPage => {
                    if let Some(current_url) = state.navigation.current_url.clone() {
                        state.add_bookmark(store, profile, &current_url)?;
                    }
                    false
                }
                ChromeAction::Back | ChromeAction::Forward | ChromeAction::Reload => {
                    state.apply_chrome_action(action, loader)
                }
                action => {
                    state.apply_chrome_action(action, loader);
                    false
                }
            };
            if should_record {
                state.record_successful_navigation(store, profile)?;
            }
            *previous_left_mouse_down = left_down;
            return Ok(());
        }

        let before_url = state.navigation.current_url.clone();
        if state.click_at(x, y, loader) {
            let after_url = state.navigation.current_url.clone();
            if after_url.is_some() && after_url != before_url {
                state.record_successful_navigation(store, profile)?;
            }
        }
    }
    *previous_left_mouse_down = left_down;
    Ok(())
}

fn handle_scroll(state: &mut AppState, window: &Window) {
    let Some((_, wheel_y)) = window.get_scroll_wheel() else {
        return;
    };
    let delta_y = -wheel_y * 48.0;
    if let Some((x, y)) = window.get_mouse_pos(MouseMode::Discard) {
        state.scroll_at_window_position(x, y, delta_y);
    } else {
        state.scroll_by(delta_y);
    }
}

fn handle_hover(state: &mut AppState, window: &Window) {
    if let Some((x, y)) = window.get_mouse_pos(MouseMode::Discard) {
        state.update_hover_at(x, y);
    }
}

fn key_to_address_char(key: Key, window: &Window) -> Option<char> {
    let shifted = window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift);
    match key {
        Key::A => Some(letter('a', shifted)),
        Key::B => Some(letter('b', shifted)),
        Key::C => Some(letter('c', shifted)),
        Key::D => Some(letter('d', shifted)),
        Key::E => Some(letter('e', shifted)),
        Key::F => Some(letter('f', shifted)),
        Key::G => Some(letter('g', shifted)),
        Key::H => Some(letter('h', shifted)),
        Key::I => Some(letter('i', shifted)),
        Key::J => Some(letter('j', shifted)),
        Key::K => Some(letter('k', shifted)),
        Key::L => Some(letter('l', shifted)),
        Key::M => Some(letter('m', shifted)),
        Key::N => Some(letter('n', shifted)),
        Key::O => Some(letter('o', shifted)),
        Key::P => Some(letter('p', shifted)),
        Key::Q => Some(letter('q', shifted)),
        Key::R => Some(letter('r', shifted)),
        Key::S => Some(letter('s', shifted)),
        Key::T => Some(letter('t', shifted)),
        Key::U => Some(letter('u', shifted)),
        Key::V => Some(letter('v', shifted)),
        Key::W => Some(letter('w', shifted)),
        Key::X => Some(letter('x', shifted)),
        Key::Y => Some(letter('y', shifted)),
        Key::Z => Some(letter('z', shifted)),
        Key::Key0 => Some(if shifted { ')' } else { '0' }),
        Key::Key1 => Some(if shifted { '!' } else { '1' }),
        Key::Key2 => Some(if shifted { '@' } else { '2' }),
        Key::Key3 => Some(if shifted { '#' } else { '3' }),
        Key::Key4 => Some(if shifted { '$' } else { '4' }),
        Key::Key5 => Some(if shifted { '%' } else { '5' }),
        Key::Key6 => Some(if shifted { '^' } else { '6' }),
        Key::Key7 => Some(if shifted { '&' } else { '7' }),
        Key::Key8 => Some(if shifted { '*' } else { '8' }),
        Key::Key9 => Some(if shifted { '(' } else { '9' }),
        Key::Minus => Some(if shifted { '_' } else { '-' }),
        Key::Equal => Some(if shifted { '+' } else { '=' }),
        Key::Comma => Some(if shifted { '<' } else { ',' }),
        Key::Period => Some(if shifted { '>' } else { '.' }),
        Key::Slash => Some(if shifted { '?' } else { '/' }),
        Key::Backslash => Some(if shifted { '|' } else { '\\' }),
        Key::Semicolon => Some(if shifted { ':' } else { ';' }),
        Key::Apostrophe => Some(if shifted { '"' } else { '\'' }),
        _ => None,
    }
}

fn has_navigation_modifier(window: &Window) -> bool {
    has_command_modifier(window)
        || window.is_key_down(Key::LeftAlt)
        || window.is_key_down(Key::RightAlt)
}

fn has_command_modifier(window: &Window) -> bool {
    window.is_key_down(Key::LeftCtrl)
        || window.is_key_down(Key::RightCtrl)
        || window.is_key_down(Key::LeftSuper)
        || window.is_key_down(Key::RightSuper)
}

fn letter(character: char, shifted: bool) -> char {
    if shifted {
        character.to_ascii_uppercase()
    } else {
        character
    }
}

fn surface_to_argb(surface: &webby_render::Surface) -> WebbyResult<Vec<u32>> {
    let pixel_count =
        surface
            .width
            .checked_mul(surface.height)
            .ok_or_else(|| WebbyError::Render {
                message: "native frame dimensions are too large".to_string(),
            })?;
    let mut buffer = Vec::with_capacity(pixel_count);

    for pixel in surface.pixels.chunks_exact(4) {
        let r = u32::from(pixel[0]);
        let g = u32::from(pixel[1]);
        let b = u32::from(pixel[2]);
        buffer.push((r << 16) | (g << 8) | b);
    }

    Ok(buffer)
}

fn update_window_frame(state: &AppState, window: &mut Window) -> WebbyResult<()> {
    let frame = state.compose_frame()?;
    let buffer = surface_to_argb(&frame)?;
    window.set_title(&format!("{} - Webby", state.window_title()));
    window
        .update_with_buffer(&buffer, frame.width, frame.height)
        .map_err(|error| WebbyError::Render {
            message: format!("failed to update native window: {error}"),
        })
}

#[cfg(test)]
mod tests {
    use super::{TabCommand, apply_tab_command, clear_on_exit_if_configured};
    use webby_app::AppState;
    use webby_core::{WebbyError, WebbyResult};

    #[test]
    fn native_adapter_tab_commands_delegate_to_app_state() {
        let mut state = AppState::with_window_size(200, 120);

        assert!(apply_tab_command(&mut state, TabCommand::New));
        assert_eq!(state.tab_count(), 2);
        assert_eq!(state.active_tab_index, 1);

        assert!(apply_tab_command(&mut state, TabCommand::Previous));
        assert_eq!(state.active_tab_index, 0);

        assert!(apply_tab_command(&mut state, TabCommand::Next));
        assert_eq!(state.active_tab_index, 1);

        assert!(apply_tab_command(&mut state, TabCommand::Close));
        assert_eq!(state.tab_count(), 1);
    }

    #[test]
    fn native_adapter_clear_data_on_exit_uses_profile_store() -> WebbyResult<()> {
        let root =
            std::env::temp_dir().join(format!("webby-app-clear-exit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let store = webby_state::ProfileStore::new(&root);
        let url = url::Url::parse("https://example.test/").map_err(|error| WebbyError::Url {
            message: error.to_string(),
        })?;
        let mut profile = webby_state::BrowserProfile {
            config: webby_state::BrowserConfig {
                clear_data_on_exit: true,
                persist_cookies: true,
                ..webby_state::BrowserConfig::default()
            },
            ..webby_state::BrowserProfile::default()
        };
        store.record_successful_navigation(&mut profile, &url)?;
        store.add_bookmark(&mut profile, &url)?;
        profile.cookies.store_from_headers(
            &url,
            &[(
                "set-cookie".to_string(),
                "sid=abc; Path=/; Max-Age=60".to_string(),
            )],
        );
        profile
            .local_storage
            .set_item("https://example.test:443", "theme", "dark")?;
        store.save(&profile)?;

        clear_on_exit_if_configured(&store, &mut profile)?;
        let loaded = store.load()?;

        assert!(loaded.history.entries.is_empty());
        assert!(loaded.bookmarks.bookmarks.is_empty());
        assert!(loaded.cookies.cookies.is_empty());
        assert!(loaded.local_storage.origins.is_empty());
        Ok(())
    }
}
