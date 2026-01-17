use crate::schema::ConfigType;
use crate::tui::App;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(f.area());

    draw_main(f, app, chunks[0]);
    draw_footer(f, app, chunks[1]);
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

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(node.desc.clone()),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn draw_footer(f: &mut Frame, _app: &App, area: Rect) {
    let text = vec![Line::from(vec![
        Span::raw(" [Enter] Enter Menu  "),
        Span::raw(" [Esc/H] Back  "),
        Span::raw(" [Space/Y] Toggle  "),
        Span::raw(" [S] Save  "),
        Span::raw(" [Q] Quit "),
    ])];
    let help = Paragraph::new(text).block(Block::default().borders(Borders::ALL));
    f.render_widget(help, area);
}
