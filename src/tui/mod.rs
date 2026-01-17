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
        })
    }

    pub fn get_current_node(&self) -> &ConfigNode {
        let mut node = &self.root_node;
        for &index in &self.current_node_path {
            node = &node.children[index];
        }
        node
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
        let node = self.get_current_node();
        if selected < node.configs.len() {
            let config = &node.configs[selected];
            if config.config_type == crate::schema::ConfigType::Bool {
                let current_val = self
                    .values
                    .get(&config.name)
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                self.values
                    .insert(config.name.clone(), Value::Boolean(!current_val));
            }
        }
    }

    pub fn save(&self) -> Result<()> {
        config_io::save_config(&self.config_path, &self.values)
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
            match key.code {
                KeyCode::Char('q') => return Ok(()),
                KeyCode::Down | KeyCode::Char('j') => app.next(),
                KeyCode::Up | KeyCode::Char('k') => app.previous(),
                KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => app.enter(),
                KeyCode::Esc | KeyCode::Left | KeyCode::Char('h') => app.back(),
                KeyCode::Char(' ') | KeyCode::Char('y') => app.toggle_bool(),
                KeyCode::Char('s') => {
                    let _ = app.save();
                }
                _ => {}
            }
        }
    }
}
