use crate::config_io;
use crate::parser;
use crate::schema::{ConfigItem, ConfigNode};
use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::{Backend, CrosstermBackend},
    widgets::ListState,
};
use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use toml::Value;

pub mod ui;

pub struct App {
    pub root_node: ConfigNode,
    pub current_node_path: Vec<usize>,
    pub values: HashMap<String, Value>,
    pub list_state: ListState,
    pub config_path: PathBuf,
    pub should_quit: bool,
    pub flattened_items: Vec<ConfigItem>,
    pub input_buffer: String,
    pub editing_config: Option<ConfigItem>,
    pub choice_state: ListState,
    pub notification: Option<String>,
    pub is_dirty: bool,
    pub show_quit_confirm: bool,
}

impl App {
    pub fn new(root_node: ConfigNode, config_path: PathBuf) -> Result<Self> {
        let flattened_items = parser::flatten_configs(&root_node);
        let values = config_io::load_config(&config_path, &flattened_items)?;
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        Ok(Self {
            root_node,
            current_node_path: Vec::new(),
            values,
            list_state,
            config_path,
            should_quit: false,
            flattened_items,
            input_buffer: String::new(),
            editing_config: None,
            choice_state: ListState::default(),
            notification: None,
            is_dirty: false,
            show_quit_confirm: false,
        })
    }

    pub fn get_current_node(&self) -> &ConfigNode {
        let mut node = &self.root_node;
        for &index in &self.current_node_path {
            node = &node.children[index];
        }
        node
    }

    pub fn get_path_string(&self) -> String {
        let mut path = vec![self.root_node.desc.clone()];
        let mut node = &self.root_node;
        for &index in &self.current_node_path {
            node = &node.children[index];
            path.push(node.desc.clone());
        }
        path.join(" > ")
    }

    pub fn next(&mut self) {
        let node = self.get_current_node();
        let total = node.configs.len() + node.children.len();
        if total == 0 {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= total - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let node = self.get_current_node();
        let total = node.configs.len() + node.children.len();
        if total == 0 {
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    total - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn enter(&mut self) {
        let selected = self.list_state.selected().unwrap_or(0);
        let node = self.get_current_node();
        if selected >= node.configs.len() {
            let child_index = selected - node.configs.len();
            self.current_node_path.push(child_index);
            self.list_state.select(Some(0));
        }
    }

    pub fn back(&mut self) {
        if !self.current_node_path.is_empty() {
            self.current_node_path.pop();
            self.list_state.select(Some(0));
        }
    }

    pub fn toggle_bool(&mut self) {
        let selected = self.list_state.selected().unwrap_or(0);
        let config = {
            let node = self.get_current_node();
            if selected < node.configs.len() {
                Some(node.configs[selected].clone())
            } else {
                None
            }
        };

        if let Some(config) = config {
            match config.config_type {
                crate::schema::ConfigType::Bool => {
                    let current_val = self
                        .values
                        .get(&config.name)
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    self.values
                        .insert(config.name.clone(), Value::Boolean(!current_val));
                    self.is_dirty = true;
                }
                crate::schema::ConfigType::Int
                | crate::schema::ConfigType::Hex
                | crate::schema::ConfigType::String => {
                    self.input_buffer = self
                        .values
                        .get(&config.name)
                        .map(|v| match v {
                            Value::Integer(i) => i.to_string(),
                            Value::String(s) => s.clone(),
                            _ => String::new(),
                        })
                        .unwrap_or_default();
                    self.editing_config = Some(config);
                }
                crate::schema::ConfigType::Choice => {
                    self.editing_config = Some(config);
                    self.choice_state = ListState::default();
                    self.choice_state.select(Some(0));
                }
            }
        }
    }

    pub fn submit_choice(&mut self) {
        if let Some(config) = self.editing_config.take() {
            if let Some(options) = &config.options {
                if let Some(selected) = self.choice_state.selected() {
                    if let Some(opt) = options.get(selected) {
                        self.values.insert(config.name, Value::String(opt.clone()));
                        self.is_dirty = true;
                        self.notify(format!("Selected: {}", opt));
                    }
                }
            }
        }
    }

    pub fn next_choice(&mut self) {
        if let Some(config) = &self.editing_config {
            if let Some(options) = &config.options {
                let i = match self.choice_state.selected() {
                    Some(i) => {
                        if i >= options.len() - 1 {
                            0
                        } else {
                            i + 1
                        }
                    }
                    None => 0,
                };
                self.choice_state.select(Some(i));
            }
        }
    }

    pub fn previous_choice(&mut self) {
        if let Some(config) = &self.editing_config {
            if let Some(options) = &config.options {
                let i = match self.choice_state.selected() {
                    Some(i) => {
                        if i == 0 {
                            options.len() - 1
                        } else {
                            i - 1
                        }
                    }
                    None => 0,
                };
                self.choice_state.select(Some(i));
            }
        }
    }

    pub fn notify(&mut self, message: String) {
        self.notification = Some(message);
    }

    pub fn clear_notification(&mut self) {
        self.notification = None;
    }

    pub fn submit_input(&mut self) {
        if let Some(config) = self.editing_config.take() {
            let value = match config.config_type {
                crate::schema::ConfigType::Int => match self.input_buffer.parse::<i64>() {
                    Ok(i) => Some(Value::Integer(i)),
                    Err(_) => {
                        self.notify("Invalid integer".to_string());
                        None
                    }
                },
                crate::schema::ConfigType::Hex => {
                    let res = if self.input_buffer.starts_with("0x")
                        || self.input_buffer.starts_with("0X")
                    {
                        i64::from_str_radix(&self.input_buffer[2..], 16)
                    } else {
                        i64::from_str_radix(&self.input_buffer, 16)
                    };
                    match res {
                        Ok(i) => Some(Value::Integer(i)),
                        Err(_) => {
                            self.notify("Invalid hex value".to_string());
                            None
                        }
                    }
                }
                crate::schema::ConfigType::String => Some(Value::String(self.input_buffer.clone())),
                _ => None,
            };

            if let Some(val) = value {
                self.values.insert(config.name, val);
                self.is_dirty = true;
                self.notify("Value updated".to_string());
            }
            self.input_buffer.clear();
        }
    }

    pub fn cancel_input(&mut self) {
        self.editing_config = None;
        self.input_buffer.clear();
    }

    pub fn save(&mut self) -> Result<()> {
        config_io::save_config(&self.config_path, &self.values)?;
        self.is_dirty = false;
        self.notify(format!("Config saved to {:?}", self.config_path));
        Ok(())
    }
}

pub fn run(root_node: ConfigNode, config_path: PathBuf) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app = App::new(root_node, config_path)?;
    let res = run_app(&mut terminal, app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;

        if let Event::Key(key) = event::read()? {
            if app.notification.is_some() {
                app.clear_notification();
                continue;
            }

            if app.show_quit_confirm {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => return Ok(()),
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        app.show_quit_confirm = false;
                    }
                    _ => {}
                }
                continue;
            }

            if let Some(config) = &app.editing_config {
                if config.config_type == crate::schema::ConfigType::Choice {
                    match key.code {
                        KeyCode::Enter => app.submit_choice(),
                        KeyCode::Esc => app.cancel_input(),
                        KeyCode::Down | KeyCode::Char('j') => app.next_choice(),
                        KeyCode::Up | KeyCode::Char('k') => app.previous_choice(),
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Enter => app.submit_input(),
                        KeyCode::Esc => app.cancel_input(),
                        KeyCode::Backspace => {
                            app.input_buffer.pop();
                        }
                        KeyCode::Char(c) => {
                            app.input_buffer.push(c);
                        }
                        _ => {}
                    }
                }
            } else {
                match key.code {
                    KeyCode::Char('q') => {
                        if app.is_dirty {
                            app.show_quit_confirm = true;
                        } else {
                            return Ok(());
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => app.next(),
                    KeyCode::Up | KeyCode::Char('k') => app.previous(),
                    KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => app.enter(),
                    KeyCode::Esc | KeyCode::Left | KeyCode::Char('h') => app.back(),
                    KeyCode::Char(' ') | KeyCode::Char('y') | KeyCode::Char('i') => {
                        app.toggle_bool()
                    }
                    KeyCode::Char('s') => {
                        let _ = app.save();
                    }
                    _ => {}
                }
            }
        }
    }
}
