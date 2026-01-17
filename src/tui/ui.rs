use crate::schema::ConfigType;
use crate::tui::App;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(f.area());

    draw_main(f, app, chunks[0]);
    draw_footer(f, app, chunks[1]);

    if let Some(config) = app.editing_config.clone() {
        if config.config_type == crate::schema::ConfigType::Choice {
            draw_choice_popup(f, app, &config);
        } else {
            draw_input_popup(f, app, &config);
        }
    }

    if let Some(msg) = &app.notification {
        draw_notification(f, msg);
    }

    if app.show_quit_confirm {
        draw_quit_confirm(f);
    }
}

fn draw_main(f: &mut Frame, app: &mut App, area: Rect) {
    let node = app.get_current_node();
    let mut items = Vec::new();

    for config in &node.configs {
        let val = app.values.get(&config.name);
        let val_str = match config.config_type {
            ConfigType::Bool => {
                if val.and_then(|v| v.as_bool()).unwrap_or(false) {
                    "[*]".to_string()
                } else {
                    "[ ]".to_string()
                }
            }
            ConfigType::Int => val.and_then(|v| v.as_integer()).unwrap_or(0).to_string(),
            ConfigType::Hex => format!("0x{:x}", val.and_then(|v| v.as_integer()).unwrap_or(0)),
            ConfigType::String | ConfigType::Choice => {
                val.and_then(|v| v.as_str()).unwrap_or("").to_string()
            }
        };

        items.push(ListItem::new(Line::from(vec![
            Span::styled(format!("{:<30}", config.name), Style::default()),
            Span::styled(format!(" {} ", val_str), Style::default().fg(Color::Yellow)),
            Span::styled(
                format!(" - {}", config.desc),
                Style::default().fg(Color::Gray),
            ),
        ])));
    }

    for child in &node.children {
        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                format!("{:<30}", child.desc),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ---> ", Style::default().fg(Color::Blue)),
        ])));
    }

    let title = format!(
        " {} {} ",
        app.get_path_string(),
        if app.is_dirty { "*" } else { "" }
    );
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn draw_input_popup(f: &mut Frame, app: &App, config: &crate::schema::ConfigItem) {
    let area = centered_rect(60, 20, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Edit {} ({}) ", config.name, config.config_type));

    let text = Paragraph::new(app.input_buffer.as_str())
        .block(block)
        .style(Style::default().fg(Color::Yellow));

    f.render_widget(text, area);
}

fn draw_choice_popup(f: &mut Frame, app: &mut App, config: &crate::schema::ConfigItem) {
    let area = centered_rect(50, 40, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Select Option for {} ", config.name));

    let default_options = Vec::new();
    let options = config.options.as_ref().unwrap_or(&default_options);
    let items: Vec<ListItem> = options
        .iter()
        .map(|opt| ListItem::new(opt.as_str()))
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, area, &mut app.choice_state);
}

fn draw_notification(f: &mut Frame, msg: &str) {
    let area = centered_rect(60, 20, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Notification ")
        .border_style(Style::default().fg(Color::Cyan));

    let text = Paragraph::new(format!("\n  {}\n\n  Press any key to close", msg))
        .block(block)
        .alignment(ratatui::layout::Alignment::Center);

    f.render_widget(text, area);
}

fn draw_quit_confirm(f: &mut Frame) {
    let area = centered_rect(50, 20, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Unsaved Changes ")
        .border_style(Style::default().fg(Color::Red));

    let text =
        Paragraph::new("\n  You have unsaved changes.\n\n  Do you really want to quit? (y/n)")
            .block(block)
            .alignment(ratatui::layout::Alignment::Center);

    f.render_widget(text, area);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let help_text = if app.show_quit_confirm {
        " [Y] Yes, Quit without saving  [N/Esc] No, Stay "
    } else if app.notification.is_some() {
        " [Any Key] Close Notification "
    } else if let Some(config) = &app.editing_config {
        if config.config_type == crate::schema::ConfigType::Choice {
            " [Enter] Select  [Esc] Cancel  [J/K] Navigate "
        } else {
            " [Enter] Confirm  [Esc] Cancel  [Backspace] Delete "
        }
    } else {
        " [Enter/L] Enter  [Esc/H] Back  [Space/Y/I] Edit  [S] Save  [Q] Quit "
    };

    let text = vec![Line::from(vec![Span::raw(help_text)])];
    let help = Paragraph::new(text).block(Block::default().borders(Borders::ALL));
    f.render_widget(help, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ]
            .as_ref(),
        )
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
}
