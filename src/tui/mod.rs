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

pub struct Editor {
    pub config: ConfigItem,
    pub input: String,
    pub choice_state: ListState,
}

pub struct UiState {
    pub current_node_path: Vec<usize>,
    pub list_state: ListState,
    pub notification: Option<String>,
    pub show_quit_confirm: bool,
    pub editor: Option<Editor>,
}

pub struct App {
    pub root_node: ConfigNode,
    pub values: HashMap<String, Value>,
    pub config_path: PathBuf,
    pub should_quit: bool,
    pub flattened_items: Vec<ConfigItem>,
    pub is_dirty: bool,
    pub evaluator: crate::evaluator::Evaluator,
    pub ui: UiState,
}

impl App {
    pub fn new(root_node: ConfigNode, config_path: PathBuf) -> Result<Self> {
        let flattened_items = parser::flatten_configs(&root_node);
        let values = config_io::load_config(&config_path, &flattened_items)?;
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        let mut evaluator = crate::evaluator::Evaluator::new();
        for (name, val) in &values {
            let _ = evaluator.set_variable(name, val);
        }

        Ok(Self {
            root_node,
            values,
            config_path,
            should_quit: false,
            flattened_items,
            is_dirty: false,
            evaluator,
            ui: UiState {
                current_node_path: Vec::new(),
                list_state,
                notification: None,
                show_quit_confirm: false,
                editor: None,
            },
        })
    }

    pub fn update_evaluator(&mut self) {
        for (name, val) in &self.values {
            let _ = self.evaluator.set_variable(name, val);
        }
    }

    pub fn get_current_node(&self) -> &ConfigNode {
        let mut node = &self.root_node;
        for &index in &self.ui.current_node_path {
            node = &node.children[index];
        }
        node
    }

    pub fn get_path_string(&self) -> String {
        let mut path = vec![self.root_node.desc.clone()];
        let mut node = &self.root_node;
        for &index in &self.ui.current_node_path {
            node = &node.children[index];
            path.push(node.desc.clone());
        }
        path.join(" > ")
    }

    pub fn is_visible_config(&self, config: &ConfigItem) -> bool {
        config
            .depends_on
            .as_ref()
            .map(|expr| self.evaluator.check_dependency(expr).unwrap_or(true))
            .unwrap_or(true)
    }

    pub fn is_visible_node(&self, node: &ConfigNode) -> bool {
        node.depends_on
            .as_ref()
            .map(|expr| self.evaluator.check_dependency(expr).unwrap_or(true))
            .unwrap_or(true)
    }

    pub fn get_visible_items(&self) -> (Vec<&ConfigItem>, Vec<&ConfigNode>) {
        let node = self.get_current_node();
        let configs: Vec<&ConfigItem> = node
            .configs
            .iter()
            .filter(|c| self.is_visible_config(c))
            .collect();
        let children: Vec<&ConfigNode> = node
            .children
            .iter()
            .filter(|n| self.is_visible_node(n))
            .collect();
        (configs, children)
    }

    pub fn next(&mut self) {
        let (configs, children) = self.get_visible_items();
        let total = configs.len() + children.len();
        if total == 0 {
            return;
        }
        let i = match self.ui.list_state.selected() {
            Some(i) => {
                if i >= total - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.ui.list_state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let (configs, children) = self.get_visible_items();
        let total = configs.len() + children.len();
        if total == 0 {
            return;
        }
        let i = match self.ui.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    total - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.ui.list_state.select(Some(i));
    }

    pub fn enter(&mut self) {
        let selected = self.ui.list_state.selected().unwrap_or(0);
        let (configs, children) = self.get_visible_items();

        if selected >= configs.len() {
            let child_index_in_visible = selected - configs.len();
            if let Some(target_node) = children.get(child_index_in_visible) {
                let parent_node = self.get_current_node();
                let real_index = parent_node
                    .children
                    .iter()
                    .position(|n| std::ptr::eq(n, *target_node));

                if let Some(idx) = real_index {
                    self.ui.current_node_path.push(idx);
                    self.ui.list_state.select(Some(0));
                }
            }
        }
    }

    pub fn back(&mut self) {
        if !self.ui.current_node_path.is_empty() {
            self.ui.current_node_path.pop();
            self.ui.list_state.select(Some(0));
        }
    }

    pub fn toggle_bool(&mut self) {
        let selected = self.ui.list_state.selected().unwrap_or(0);
        let (visible_configs, _) = self.get_visible_items();

        let config = if selected < visible_configs.len() {
            Some(visible_configs[selected].clone())
        } else {
            None
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
                    self.update_evaluator();
                }
                crate::schema::ConfigType::Int
                | crate::schema::ConfigType::Hex
                | crate::schema::ConfigType::String => {
                    let input = self
                        .values
                        .get(&config.name)
                        .map(|v| match v {
                            Value::Integer(i) => i.to_string(),
                            Value::String(s) => s.clone(),
                            _ => String::new(),
                        })
                        .unwrap_or_default();
                    self.ui.editor = Some(Editor {
                        config,
                        input,
                        choice_state: ListState::default(),
                    });
                }
                crate::schema::ConfigType::Choice => {
                    let mut choice_state = ListState::default();
                    choice_state.select(Some(0));
                    self.ui.editor = Some(Editor {
                        config,
                        input: String::new(),
                        choice_state,
                    });
                }
            }
        }
    }

    pub fn submit_choice(&mut self) {
        if let Some(editor) = self.ui.editor.take() {
            let config = editor.config;
            if let Some(options) = &config.options {
                if let Some(selected) = editor.choice_state.selected() {
                    if let Some(opt) = options.get(selected) {
                        self.values.insert(config.name, Value::String(opt.clone()));
                        self.is_dirty = true;
                        self.update_evaluator();
                        self.notify(format!("Selected: {}", opt));
                    }
                }
            }
        }
    }

    pub fn next_choice(&mut self) {
        if let Some(editor) = &mut self.ui.editor {
            if let Some(options) = &editor.config.options {
                let i = match editor.choice_state.selected() {
                    Some(i) => {
                        if i >= options.len() - 1 {
                            0
                        } else {
                            i + 1
                        }
                    }
                    None => 0,
                };
                editor.choice_state.select(Some(i));
            }
        }
    }

    pub fn previous_choice(&mut self) {
        if let Some(editor) = &mut self.ui.editor {
            if let Some(options) = &editor.config.options {
                let i = match editor.choice_state.selected() {
                    Some(i) => {
                        if i == 0 {
                            options.len() - 1
                        } else {
                            i - 1
                        }
                    }
                    None => 0,
                };
                editor.choice_state.select(Some(i));
            }
        }
    }

    pub fn notify(&mut self, message: String) {
        self.ui.notification = Some(message);
    }

    pub fn clear_notification(&mut self) {
        self.ui.notification = None;
    }

    pub fn submit_input(&mut self) {
        if let Some(editor) = self.ui.editor.take() {
            let config = editor.config;
            let value = match config.config_type {
                crate::schema::ConfigType::Int => match editor.input.parse::<i64>() {
                    Ok(i) => Some(Value::Integer(i)),
                    Err(_) => {
                        self.notify("Invalid integer".to_string());
                        None
                    }
                },
                crate::schema::ConfigType::Hex => {
                    let res = if editor.input.starts_with("0x") || editor.input.starts_with("0X") {
                        i64::from_str_radix(&editor.input[2..], 16)
                    } else {
                        i64::from_str_radix(&editor.input, 16)
                    };
                    match res {
                        Ok(i) => Some(Value::Integer(i)),
                        Err(_) => {
                            self.notify("Invalid hex value".to_string());
                            None
                        }
                    }
                }
                crate::schema::ConfigType::String => Some(Value::String(editor.input.clone())),
                _ => None,
            };

            if let Some(val) = value {
                self.values.insert(config.name, val);
                self.is_dirty = true;
                self.update_evaluator();
                self.notify("Value updated".to_string());
            }
        }
    }

    pub fn cancel_input(&mut self) {
        self.ui.editor = None;
    }

    pub fn save(&mut self) -> Result<()> {
        config_io::save_config(&self.config_path, &self.values)?;
        self.is_dirty = false;
        self.notify(format!("Config saved to {:?}", self.config_path));
        Ok(())
    }

    pub fn handle_event(&mut self, event: Event) -> io::Result<bool> {
        if let Event::Key(key) = event {
            return self.handle_key_event(key);
        }
        Ok(false)
    }

    fn handle_key_event(&mut self, key: event::KeyEvent) -> io::Result<bool> {
        if self.ui.notification.is_some() {
            self.clear_notification();
            return Ok(false);
        }

        if self.ui.show_quit_confirm {
            return self.handle_quit_confirm(key);
        }

        if self.ui.editor.is_some() {
            self.handle_editing_key(key);
        } else {
            return self.handle_main_key(key);
        }
        Ok(false)
    }

    fn handle_quit_confirm(&mut self, key: event::KeyEvent) -> io::Result<bool> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let _ = self.save();
                Ok(true)
            }
            KeyCode::Char('n') | KeyCode::Char('N') => Ok(true),
            KeyCode::Esc => {
                self.ui.show_quit_confirm = false;
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    fn handle_editing_key(&mut self, key: event::KeyEvent) {
        let is_choice = self
            .ui
            .editor
            .as_ref()
            .map(|e| e.config.config_type == crate::schema::ConfigType::Choice)
            .unwrap_or(false);

        if is_choice {
            match key.code {
                KeyCode::Enter => self.submit_choice(),
                KeyCode::Esc => self.cancel_input(),
                KeyCode::Down | KeyCode::Char('j') => self.next_choice(),
                KeyCode::Up | KeyCode::Char('k') => self.previous_choice(),
                _ => {}
            }
        } else {
            match key.code {
                KeyCode::Enter => self.submit_input(),
                KeyCode::Esc => self.cancel_input(),
                KeyCode::Backspace => {
                    if let Some(editor) = &mut self.ui.editor {
                        editor.input.pop();
                    }
                }
                KeyCode::Char(c) => {
                    if let Some(editor) = &mut self.ui.editor {
                        editor.input.push(c);
                    }
                }
                _ => {}
            }
        }
    }

    fn handle_main_key(&mut self, key: event::KeyEvent) -> io::Result<bool> {
        match key.code {
            KeyCode::Char('q') => {
                if self.is_dirty {
                    self.ui.show_quit_confirm = true;
                } else {
                    return Ok(true);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => self.next(),
            KeyCode::Up | KeyCode::Char('k') => self.previous(),
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => self.enter(),
            KeyCode::Esc | KeyCode::Left | KeyCode::Char('h') => self.back(),
            KeyCode::Char(' ') | KeyCode::Char('y') | KeyCode::Char('i') => self.toggle_bool(),
            KeyCode::Char('s') => {
                let _ = self.save();
            }
            _ => {}
        }
        Ok(false)
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

        if app.handle_event(event::read()?)? {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{ConfigItem, ConfigNode, ConfigType};
    use std::path::PathBuf;

    fn mock_app() -> App {
        let root = ConfigNode {
            desc: "Root".to_string(),
            configs: vec![ConfigItem {
                name: "cfg1".to_string(),
                config_type: ConfigType::Bool,
                default: Some(toml::Value::Boolean(false)),
                desc: "Desc 1".to_string(),
                depends_on: None,
                help: None,
                options: None,
                feature: None,
            }],
            children: vec![ConfigNode {
                desc: "Child".to_string(),
                configs: vec![],
                children: vec![],
                path: "root.child".to_string(),
                depends_on: None,
            }],
            path: "root".to_string(),
            depends_on: None,
        };
        App::new(root, PathBuf::from("dummy.toml")).unwrap()
    }

    #[test]
    fn test_navigation_next_prev() {
        let mut app = mock_app();
        app.ui.list_state.select(Some(0));

        // 1 config + 1 child = 2 items
        app.next();
        assert_eq!(app.ui.list_state.selected(), Some(1));
        app.next();
        assert_eq!(app.ui.list_state.selected(), Some(0)); // Wrap

        app.previous();
        assert_eq!(app.ui.list_state.selected(), Some(1)); // Wrap back
    }

    #[test]
    fn test_navigation_enter_back() {
        let mut app = mock_app();
        app.ui.list_state.select(Some(1)); // Select "Child"
        app.enter();
        assert_eq!(app.ui.current_node_path.len(), 1);
        assert_eq!(app.ui.list_state.selected(), Some(0));

        app.back();
        assert_eq!(app.ui.current_node_path.len(), 0);
    }
}
