use anaxa_builder::config_io;
use anaxa_builder::parser;
use anaxa_builder::schema::{ConfigItem, ConfigNode};
use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    widgets::ListState,
    Terminal,
};
use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use toml::Value;

pub mod action;
pub mod handler;
pub mod ui;

use action::Action;

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
    pub show_help: bool,
    pub help_scroll: u16,
    pub editor: Option<Editor>,
    pub show_search: bool,
    pub search_query: String,
    pub search_results: Vec<(Vec<usize>, usize)>, // (node_path, index_in_node)
    pub search_list_state: ListState,
}

pub struct App {
    pub root_node: ConfigNode,
    pub values: HashMap<String, Value>,
    pub config_path: PathBuf,
    pub should_quit: bool,
    pub flattened_items: Vec<ConfigItem>,
    pub is_dirty: bool,
    pub evaluator: anaxa_builder::evaluator::Evaluator,
    pub ui: UiState,
}

impl App {
    pub fn new(root_node: ConfigNode, config_path: PathBuf) -> Result<Self> {
        let flattened_items = parser::flatten_configs(&root_node);
        let values = config_io::load_config(&config_path, &flattened_items)?;
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        let mut evaluator = anaxa_builder::evaluator::Evaluator::new();
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
                show_help: false,
                help_scroll: 0,
                editor: None,
                show_search: false,
                search_query: String::new(),
                search_results: Vec::new(),
                search_list_state: ListState::default(),
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
        self.ui.help_scroll = 0;
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
        self.ui.help_scroll = 0;
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
                anaxa_builder::schema::ConfigType::Bool => {
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
                anaxa_builder::schema::ConfigType::Int
                | anaxa_builder::schema::ConfigType::Hex
                | anaxa_builder::schema::ConfigType::String
                | anaxa_builder::schema::ConfigType::Cstr => {
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
                anaxa_builder::schema::ConfigType::Choice => {
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

    pub fn perform_search(&mut self) {
        self.ui.search_results.clear();
        if self.ui.search_query.is_empty() {
            self.ui.search_list_state.select(None);
            return;
        }

        let query = self.ui.search_query.to_lowercase();
        let mut results = Vec::new();
        self.search_recursive(&self.root_node, &mut Vec::new(), &query, &mut results);
        self.ui.search_results = results;
        if !self.ui.search_results.is_empty() {
            self.ui.search_list_state.select(Some(0));
        } else {
            self.ui.search_list_state.select(None);
        }
    }

    fn search_recursive(
        &self,
        node: &ConfigNode,
        current_path: &mut Vec<usize>,
        query: &str,
        results: &mut Vec<(Vec<usize>, usize)>,
    ) {
        for (i, config) in node.configs.iter().enumerate() {
            if config.name.to_lowercase().contains(query)
                || config.desc.to_lowercase().contains(query)
            {
                results.push((current_path.clone(), i));
            }
        }

        for (i, child) in node.children.iter().enumerate() {
            current_path.push(i);
            self.search_recursive(child, current_path, query, results);
            current_path.pop();
        }
    }

    pub fn next_search_result(&mut self) {
        if self.ui.search_results.is_empty() {
            return;
        }
        let i = match self.ui.search_list_state.selected() {
            Some(i) => {
                if i >= self.ui.search_results.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.ui.search_list_state.select(Some(i));
    }

    pub fn previous_search_result(&mut self) {
        if self.ui.search_results.is_empty() {
            return;
        }
        let i = match self.ui.search_list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.ui.search_results.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.ui.search_list_state.select(Some(i));
    }

    pub fn jump_to_search_result(&mut self) {
        if let Some(selected) = self.ui.search_list_state.selected() {
            if let Some((path, index)) = self.ui.search_results.get(selected) {
                self.ui.current_node_path = path.clone();
                // We need to find the index in the VISIBLE items
                // This is tricky because the index in search result is real index in node.configs
                // But the TUI list shows visible configs then visible children.
                let (visible_configs, _) = self.get_visible_items();
                let config_name = &self.get_current_node().configs[*index].name;

                if let Some(pos) = visible_configs.iter().position(|c| &c.name == config_name) {
                    self.ui.list_state.select(Some(pos));
                } else {
                    // Item might be hidden by dependency, but we jumped to the parent node anyway
                    self.ui.list_state.select(Some(0));
                    self.notify(
                        "Note: Target item is currently hidden by dependencies".to_string(),
                    );
                }

                self.ui.show_search = false;
            }
        }
    }

    pub fn help_scroll_up(&mut self) {
        if self.ui.help_scroll > 0 {
            self.ui.help_scroll -= 1;
        }
    }

    pub fn help_scroll_down(&mut self) {
        self.ui.help_scroll += 1;
    }

    pub fn submit_input(&mut self) {
        if let Some(editor) = self.ui.editor.take() {
            let config = editor.config;
            let value = match config.config_type {
                anaxa_builder::schema::ConfigType::Int => match editor.input.parse::<i64>() {
                    Ok(i) => Some(Value::Integer(i)),
                    Err(_) => {
                        self.notify("Invalid integer".to_string());
                        None
                    }
                },
                anaxa_builder::schema::ConfigType::Hex => {
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
                anaxa_builder::schema::ConfigType::String
                | anaxa_builder::schema::ConfigType::Cstr => {
                    Some(Value::String(editor.input.clone()))
                }
                _ => None,
            };

            if let Some(val) = value {
                match config.validate(&val) {
                    Ok(_) => {
                        self.values.insert(config.name, val);
                        self.is_dirty = true;
                        self.update_evaluator();
                        self.notify("Value updated".to_string());
                    }
                    Err(e) => {
                        self.notify(format!("Error: {}", e));
                    }
                }
            }
        }
    }

    pub fn cancel_input(&mut self) {
        self.ui.editor = None;
    }

    pub fn handle_action(&mut self, action: Action) -> bool {
        match action {
            Action::Next => {
                if self.ui.show_search {
                    self.next_search_result();
                } else {
                    self.next();
                }
            }
            Action::Previous => {
                if self.ui.show_search {
                    self.previous_search_result();
                } else {
                    self.previous();
                }
            }
            Action::Enter => {
                if self.ui.show_search {
                    self.jump_to_search_result();
                } else {
                    self.enter();
                }
            }
            Action::Back => self.back(),
            Action::ToggleBool => self.toggle_bool(),
            Action::SubmitChoice => self.submit_choice(),
            Action::NextChoice => self.next_choice(),
            Action::PreviousChoice => self.previous_choice(),
            Action::SubmitInput => self.submit_input(),
            Action::CancelInput => self.cancel_input(),
            Action::InputChar(c) => {
                if self.ui.show_search {
                    self.ui.search_query.push(c);
                    self.perform_search();
                } else if let Some(editor) = &mut self.ui.editor {
                    editor.input.push(c);
                }
            }
            Action::Backspace => {
                if self.ui.show_search {
                    self.ui.search_query.pop();
                    self.perform_search();
                } else if let Some(editor) = &mut self.ui.editor {
                    editor.input.pop();
                }
            }
            Action::Save => {
                let _ = self.save();
            }
            Action::QuitRequest => {
                if self.is_dirty {
                    self.ui.show_quit_confirm = true;
                } else {
                    return true;
                }
            }
            Action::ConfirmQuit => {
                let _ = self.save();
                return true;
            }
            Action::DiscardQuit => return true,
            Action::CancelQuit => self.ui.show_quit_confirm = false,
            Action::ToggleHelp => self.ui.show_help = !self.ui.show_help,
            Action::HelpScrollUp => self.help_scroll_up(),
            Action::HelpScrollDown => self.help_scroll_down(),
            Action::ClearNotification => self.clear_notification(),
            Action::ToggleSearch => {
                if self.ui.show_search {
                    self.ui.show_search = false;
                } else {
                    self.ui.show_search = true;
                    self.ui.search_query.clear();
                    self.ui.search_results.clear();
                    self.ui.search_list_state.select(None);
                }
            }
        }
        false
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

        if let Some(action) = handler::handle_event(&app, event::read()?) {
            if app.handle_action(action) {
                return Ok(());
            }
        }
    }
}
