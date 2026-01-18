use crate::tui::action::Action;
use crate::tui::App;
use crossterm::event::{Event, KeyCode, MouseButton, MouseEventKind};

pub fn handle_event(app: &App, event: Event) -> Option<Action> {
    match event {
        Event::Key(key) => handle_key_event(app, key.code),
        Event::Mouse(mouse) => handle_mouse_event(app, mouse.kind),
        _ => None,
    }
}

fn handle_mouse_event(app: &App, kind: MouseEventKind) -> Option<Action> {
    match kind {
        MouseEventKind::ScrollUp => {
            if app.ui.show_help {
                Some(Action::HelpScrollUp)
            } else {
                Some(Action::Previous)
            }
        }
        MouseEventKind::ScrollDown => {
            if app.ui.show_help {
                Some(Action::HelpScrollDown)
            } else {
                Some(Action::Next)
            }
        }
        MouseEventKind::Down(MouseButton::Left) => {
            if app.ui.notification.is_some() {
                Some(Action::ClearNotification)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn handle_key_event(app: &App, code: KeyCode) -> Option<Action> {
    if app.ui.notification.is_some() {
        return Some(Action::ClearNotification);
    }

    if app.ui.show_quit_confirm {
        return match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => Some(Action::ConfirmQuit),
            KeyCode::Char('n') | KeyCode::Char('N') => Some(Action::DiscardQuit),
            KeyCode::Esc => Some(Action::CancelQuit),
            _ => None,
        };
    }

    if let Some(editor) = &app.ui.editor {
        let is_choice = editor.config.config_type == crate::schema::ConfigType::Choice;
        if is_choice {
            match code {
                KeyCode::Enter => return Some(Action::SubmitChoice),
                KeyCode::Esc => return Some(Action::CancelInput),
                KeyCode::Down | KeyCode::Char('j') => return Some(Action::NextChoice),
                KeyCode::Up | KeyCode::Char('k') => return Some(Action::PreviousChoice),
                _ => return None,
            }
        } else {
            match code {
                KeyCode::Enter => return Some(Action::SubmitInput),
                KeyCode::Esc => return Some(Action::CancelInput),
                KeyCode::Backspace => return Some(Action::Backspace),
                KeyCode::Char(c) => return Some(Action::InputChar(c)),
                _ => return None,
            }
        }
    }

    match code {
        KeyCode::Char('q') => Some(Action::QuitRequest),
        KeyCode::Down | KeyCode::Char('j') => Some(Action::Next),
        KeyCode::Up | KeyCode::Char('k') => Some(Action::Previous),
        KeyCode::Enter | KeyCode::Right => Some(Action::Enter),
        KeyCode::Esc | KeyCode::Left => Some(Action::Back),
        KeyCode::Char(' ') => Some(Action::ToggleBool),
        KeyCode::Char('s') => Some(Action::Save),
        KeyCode::Char('?') => Some(Action::ToggleHelp),
        KeyCode::PageUp | KeyCode::Char('K') => Some(Action::HelpScrollUp),
        KeyCode::PageDown | KeyCode::Char('J') => Some(Action::HelpScrollDown),
        _ => None,
    }
}
