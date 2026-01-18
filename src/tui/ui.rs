use crate::schema::ConfigType;
use crate::tui::App;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.area());

    draw_header(f, chunks[0], &app.get_path_string());

    if app.ui.show_help {
        let main_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(40)])
            .split(chunks[1]);
        draw_main(f, app, main_chunks[0]);
        draw_help_sidebar(f, app, main_chunks[1]);
    } else {
        draw_main(f, app, chunks[1]);
    }

    draw_footer(f, chunks[2], app);

    if app.ui.editor.is_some() {
        let is_choice = app
            .ui
            .editor
            .as_ref()
            .map(|e| e.config.config_type == crate::schema::ConfigType::Choice)
            .unwrap_or(false);
        if is_choice {
            draw_choice_popup(f, app);
        } else {
            draw_input_popup(f, app);
        }
    }

    if let Some(msg) = &app.ui.notification {
        draw_notification(f, msg);
    }

    if app.ui.show_quit_confirm {
        draw_quit_confirm(f);
    }
}

fn draw_header(f: &mut Frame, area: Rect, breadcrumbs: &str) {
    let header_text = vec![Line::from(vec![
        Span::styled(
            " ANAXA BUILDER ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | "),
        Span::styled(breadcrumbs, Style::default().fg(Color::Gray)),
    ])];

    let header = Paragraph::new(header_text).block(Block::default().borders(Borders::ALL));
    f.render_widget(header, area);
}

fn draw_main(f: &mut Frame, app: &mut App, area: Rect) {
    let (configs, children) = app.get_visible_items();
    let mut items = Vec::new();

    for config in configs {
        let val = app.values.get(&config.name);
        let (val_str, val_style) = match config.config_type {
            ConfigType::Bool => {
                if val.and_then(|v| v.as_bool()).unwrap_or(false) {
                    (
                        "[X]".to_string(),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    ("[ ]".to_string(), Style::default().fg(Color::DarkGray))
                }
            }
            ConfigType::Int => (
                val.and_then(|v| v.as_integer()).unwrap_or(0).to_string(),
                Style::default().fg(Color::Yellow),
            ),
            ConfigType::Hex => (
                format!("0x{:x}", val.and_then(|v| v.as_integer()).unwrap_or(0)),
                Style::default().fg(Color::Yellow),
            ),
            ConfigType::String | ConfigType::Choice => (
                val.and_then(|v| v.as_str()).unwrap_or("").to_string(),
                Style::default().fg(Color::Green),
            ),
        };

        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                format!("{:<30}", config.name),
                Style::default().fg(Color::White),
            ),
            Span::styled(format!(" {} ", val_str), val_style),
            Span::styled(
                format!(" - {}", config.desc),
                Style::default().fg(Color::Gray),
            ),
        ])));
    }

    for child in children {
        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                format!("{:<30}", child.desc),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ➔ ", Style::default().fg(Color::Blue)),
        ])));
    }

    let title = format!(" Configuration {} ", if app.is_dirty { "*" } else { "" });
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .bg(Color::Indexed(237))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, area, &mut app.ui.list_state);
}

fn draw_input_popup(f: &mut Frame, app: &App) {
    if let Some(editor) = &app.ui.editor {
        let area = centered_rect(60, 20, f.area());
        f.render_widget(Clear, area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .title(format!(
                " Edit {} ({}) ",
                editor.config.name, editor.config.config_type
            ));

        let mut lines = vec![Line::from(editor.input.as_str()), Line::from("")];

        if let Some((min, max)) = editor.config.range {
            lines.push(Line::from(Span::styled(
                format!("Range: [{}, {}]", min, max),
                Style::default()
                    .fg(Color::Gray)
                    .add_modifier(Modifier::ITALIC),
            )));
        }

        if let Some(regex) = &editor.config.regex {
            lines.push(Line::from(Span::styled(
                format!("Regex: {}", regex),
                Style::default()
                    .fg(Color::Gray)
                    .add_modifier(Modifier::ITALIC),
            )));
        }

        let text = Paragraph::new(lines)
            .block(block)
            .style(Style::default().fg(Color::Yellow));

        f.render_widget(text, area);
    }
}

fn draw_choice_popup(f: &mut Frame, app: &mut App) {
    if let Some(editor) = &mut app.ui.editor {
        let area = centered_rect(50, 40, f.area());
        f.render_widget(Clear, area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green))
            .title(format!(" Select Option for {} ", editor.config.name));

        let default_options = Vec::new();
        let options = editor.config.options.as_ref().unwrap_or(&default_options);
        let items: Vec<ListItem> = options
            .iter()
            .map(|opt| ListItem::new(opt.as_str()))
            .collect();

        let list = List::new(items)
            .block(block)
            .highlight_style(
                Style::default()
                    .bg(Color::Indexed(237))
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        f.render_stateful_widget(list, area, &mut editor.choice_state);
    }
}

fn draw_notification(f: &mut Frame, msg: &str) {
    let area = centered_rect(60, 20, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Notification ")
        .border_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    let text = Paragraph::new(format!("\n  {}\n\n  Press any key to close", msg))
        .block(block)
        .alignment(ratatui::layout::Alignment::Center);

    f.render_widget(text, area);
}

fn draw_quit_confirm(f: &mut Frame) {
    let area = centered_rect(50, 25, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Unsaved Changes ")
        .border_style(
            Style::default()
                .fg(Color::LightRed)
                .add_modifier(Modifier::BOLD),
        );

    let text = Paragraph::new(
        "\n  You have unsaved changes.\n\n  [Y] Save and Quit\n  [N] Discard and Quit\n  [Esc] Cancel",
    )
    .block(block)
    .alignment(ratatui::layout::Alignment::Center);

    f.render_widget(text, area);
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(15)])
        .split(area);

    let help_text = if app.ui.show_quit_confirm {
        " [Y] Save & Quit  [N] Discard & Quit  [Esc] Stay ".to_string()
    } else if app.ui.notification.is_some() {
        " [Any Key] Close Notification ".to_string()
    } else if let Some(editor) = &app.ui.editor {
        if editor.config.config_type == crate::schema::ConfigType::Choice {
            " [Enter] Select  [Esc] Cancel  [J/K] Navigate ".to_string()
        } else {
            " [Enter] Confirm  [Esc] Cancel  [Backspace] Delete ".to_string()
        }
    } else {
        let help_base =
            " [Enter/L] Enter  [Esc/H] Back  [Space/Y/I] Edit  [S] Save  [?] Help  [Q] Quit ";
        if app.ui.show_help {
            format!("{} [J/K] Scroll Help ", help_base)
        } else {
            help_base.to_string()
        }
    };

    let status_text = if app.is_dirty {
        " MODIFIED "
    } else {
        " SAVED "
    };
    let status_style = if app.is_dirty {
        Style::default().fg(Color::Black).bg(Color::Yellow)
    } else {
        Style::default().fg(Color::Black).bg(Color::Green)
    };

    let help = Paragraph::new(help_text).block(Block::default().borders(Borders::ALL));
    let status = Paragraph::new(status_text)
        .block(Block::default().borders(Borders::ALL))
        .style(status_style)
        .alignment(ratatui::layout::Alignment::Center);

    f.render_widget(help, chunks[0]);
    f.render_widget(status, chunks[1]);
}

fn draw_help_sidebar(f: &mut Frame, app: &App, area: Rect) {
    let selected = app.ui.list_state.selected().unwrap_or(0);
    let (configs, children) = app.get_visible_items();

    let mut help_text = Vec::new();

    if selected < configs.len() {
        let config = configs[selected];
        help_text.push(Line::from(vec![
            Span::styled("Name: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(&config.name),
        ]));
        help_text.push(Line::from(vec![
            Span::styled("Type: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("{}", config.config_type)),
        ]));
        if let Some(default) = &config.default {
            help_text.push(Line::from(vec![
                Span::styled("Default: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!("{}", default)),
            ]));
        }
        help_text.push(Line::from(""));
        help_text.push(Line::from(vec![Span::styled(
            "Help:",
            Style::default().add_modifier(Modifier::BOLD),
        )]));
        let help = config.help.as_deref().unwrap_or("No help available.");
        for line in help.lines() {
            help_text.push(Line::from(format!("  {}", line)));
        }
    } else {
        let child_index = selected - configs.len();
        if let Some(child) = children.get(child_index) {
            help_text.push(Line::from(vec![
                Span::styled("Submenu: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&child.desc),
            ]));
            help_text.push(Line::from(vec![
                Span::styled("Path: ", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&child.path),
            ]));
            if let Some(help) = &child.help {
                help_text.push(Line::from(""));
                help_text.push(Line::from(vec![Span::styled(
                    "Help:",
                    Style::default().add_modifier(Modifier::BOLD),
                )]));
                for line in help.lines() {
                    help_text.push(Line::from(format!("  {}", line)));
                }
            }
        }
    }

    let block = Block::default().borders(Borders::ALL).title(" Help ");
    let paragraph = Paragraph::new(help_text)
        .block(block)
        .wrap(ratatui::widgets::Wrap { trim: true })
        .scroll((app.ui.help_scroll, 0));
    f.render_widget(paragraph, area);
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
