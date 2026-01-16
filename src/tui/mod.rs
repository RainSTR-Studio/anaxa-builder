pub mod state;

use crate::config_io;
use crate::parser::ParsedConfig;
use crate::schema::ConfigType;
use crate::tui::state::AppState;
use cursive::align::HAlign;
use cursive::traits::*;
use cursive::views::{Dialog, EditView, LinearLayout, Panel, ScrollView, SelectView, TextView};
use cursive::{Cursive, CursiveExt};
use cursive_tree_view::{Placement, TreeView};
use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
enum TreeItem {
    Menu(String),
    Config(usize),
}

impl fmt::Display for TreeItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TreeItem::Menu(s) => write!(f, "{}", s),
            TreeItem::Config(_) => Ok(()),
        }
    }
}

#[derive(Debug, Clone)]
struct NamedTreeItem {
    label: String,
    item: TreeItem,
}

impl fmt::Display for NamedTreeItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label)
    }
}

pub fn run(parsed: ParsedConfig, config_path: PathBuf) -> anyhow::Result<()> {
    let items = parsed.items;
    let mut siv = Cursive::default();

    let values = config_io::load_config(&config_path, &items)?;
    let state = AppState::new(items.clone(), values);
    siv.set_user_data(state);

    siv.add_global_callback('q', |s| s.quit());

    let path_clone = config_path.clone();
    siv.add_global_callback('s', move |s| {
        save_action(s, &path_clone);
    });

    let mut tree = TreeView::<NamedTreeItem>::new();

    let mut dir_rows: HashMap<PathBuf, usize> = HashMap::new();
    let mut menu_titles: HashMap<PathBuf, String> = HashMap::new();
    for (path, menu) in &parsed.menus {
        menu_titles.insert(path.clone(), menu.title.clone());
    }

    let mut sorted_files: Vec<_> = parsed.file_map.iter().collect();
    sorted_files.sort_by_key(|(path, _)| path.components().count());

    let mut last_top_level_row: Option<usize> = None;

    for (path, file_items) in sorted_files {
        let dir = path.parent().unwrap_or(Path::new("."));

        let mut current_path = PathBuf::new();
        let mut current_parent_row: Option<usize> = None;

        for component in dir.components() {
            if component.as_os_str() == "." {
                continue;
            }
            current_path.push(component);

            if let Some(&row) = dir_rows.get(&current_path) {
                current_parent_row = Some(row);
            } else {
                let kconfig_path = current_path.join("Kconfig.toml");
                let title = menu_titles
                    .get(&kconfig_path)
                    .cloned()
                    .unwrap_or_else(|| component.as_os_str().to_string_lossy().to_string());

                let named_item = NamedTreeItem {
                    label: title,
                    item: TreeItem::Menu(current_path.to_string_lossy().to_string()),
                };

                let row = if let Some(parent) = current_parent_row {
                    tree.insert_container_item(named_item, Placement::LastChild, parent)
                } else if let Some(last_root) = last_top_level_row {
                    tree.insert_container_item(named_item, Placement::After, last_root)
                } else {
                    tree.insert_container_item(named_item, Placement::LastChild, 0)
                }
                .unwrap_or(0);

                dir_rows.insert(current_path.clone(), row);
                if current_parent_row.is_none() {
                    last_top_level_row = Some(row);
                }
                current_parent_row = Some(row);
            }
        }

        for item in file_items {
            if let Some(global_idx) = items.iter().position(|i| i.name == item.name) {
                let label = if item.desc.is_empty() {
                    item.name.clone()
                } else {
                    item.desc.clone()
                };

                let named_item = NamedTreeItem {
                    label,
                    item: TreeItem::Config(global_idx),
                };

                let row = if let Some(parent) = current_parent_row {
                    tree.insert_item(named_item, Placement::LastChild, parent)
                } else if let Some(last_root) = last_top_level_row {
                    tree.insert_item(named_item, Placement::After, last_root)
                } else {
                    tree.insert_item(named_item, Placement::LastChild, 0)
                }
                .unwrap_or(0);

                if current_parent_row.is_none() {
                    last_top_level_row = Some(row);
                }
            }
        }
    }

    tree.set_on_submit(move |s, row| {
        let tree_item = {
            let tree: cursive::views::ViewRef<TreeView<NamedTreeItem>> =
                s.find_name("config_tree").unwrap();
            tree.borrow_item(row).cloned()
        };

        if let Some(named_item) = tree_item {
            if let TreeItem::Config(idx) = named_item.item {
                on_tree_submit(s, idx);
            }
        }
    });

    let layout = LinearLayout::vertical()
        .child(
            TextView::new("Anaxa Config - Press <Enter> to edit, <s> to save, <q> to quit")
                .h_align(HAlign::Center),
        )
        .child(Panel::new(ScrollView::new(tree.with_name("config_tree"))).full_screen());

    siv.add_layer(layout);
    siv.run();
    Ok(())
}

fn on_tree_submit(s: &mut Cursive, idx: usize) {
    let state = s.user_data::<AppState>().unwrap().clone();
    let item = &state.items[idx];

    if !state.is_visible(item) {
        s.add_layer(Dialog::info("This option is disabled by dependencies."));
        return;
    }

    let name = item.name.clone();
    let current_val = state.values.get(&name).cloned();

    match item.config_type {
        ConfigType::Bool => {
            let is_true = current_val.and_then(|v| v.as_bool()).unwrap_or(false);
            s.with_user_data(|state: &mut AppState| {
                state.update_value(&name, toml::Value::Boolean(!is_true));
            });
            let new_val = !is_true;
            s.add_layer(Dialog::info(format!("Set {} to {}", name, new_val)));
        }
        ConfigType::String => {
            let val = current_val
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_default();
            s.add_layer(
                Dialog::new()
                    .title(&item.desc)
                    .content(EditView::new().content(val).on_submit(move |s, text| {
                        let name_clone = name.clone();
                        s.with_user_data(|state: &mut AppState| {
                            state.update_value(&name_clone, toml::Value::String(text.to_string()));
                        });
                        s.pop_layer();
                    }))
                    .button("Ok", |s| {
                        s.pop_layer();
                    }),
            );
        }
        ConfigType::Int => {
            let val = current_val
                .and_then(|v| v.as_integer())
                .unwrap_or(0)
                .to_string();
            s.add_layer(
                Dialog::new()
                    .title(&item.desc)
                    .content(EditView::new().content(val).on_submit(move |s, text| {
                        if let Ok(i) = text.parse::<i64>() {
                            let name_clone = name.clone();
                            s.with_user_data(|state: &mut AppState| {
                                state.update_value(&name_clone, toml::Value::Integer(i));
                            });
                            s.pop_layer();
                        } else {
                            s.add_layer(Dialog::info("Invalid integer"));
                        }
                    }))
                    .button("Ok", |s| {
                        s.pop_layer();
                    }),
            );
        }
        ConfigType::Choice => {
            let opts = item.options.clone().unwrap_or_default();
            let mut select = SelectView::new();
            for opt in &opts {
                select.add_item_str(opt);
            }
            let _current_sel = current_val
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_default();

            select.set_on_submit(move |s, val: &str| {
                let name_clone = name.clone();
                let val_string = val.to_string();
                s.with_user_data(|state: &mut AppState| {
                    state.update_value(&name_clone, toml::Value::String(val_string));
                });
                s.pop_layer();
            });

            s.add_layer(
                Dialog::new()
                    .title(&item.desc)
                    .content(select)
                    .button("Cancel", |s| {
                        s.pop_layer();
                    }),
            );
        }
        _ => {
            s.add_layer(Dialog::info("Editing this type not implemented yet"));
        }
    }
}

fn save_action(s: &mut Cursive, path: &Path) {
    let state = s.user_data::<AppState>().unwrap();
    if let Err(e) = config_io::save_config(path, &state.values) {
        s.add_layer(Dialog::info(format!("Failed to save: {}", e)));
    } else {
        s.add_layer(Dialog::info("Configuration saved successfully!"));
    }
}
