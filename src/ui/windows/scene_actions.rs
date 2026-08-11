use std::path::PathBuf;

use crate::palette::{PaletteConfig, PaletteList};
use crate::{ egui_systems};
use bevy::prelude::*;
use bevy_egui::egui::Align2;
use bevy_egui::{egui, EguiContexts};

use crate::ui::icon_button::IconButton;
use crate::ui::images::GuiIcons;
use crate::ui::{InitialPos, Subwindow, WindowExt, image_radio};

egui_systems!(draw_scene_actions);

enum SceneWindows {
    NewScene,
    Open,
    Save,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum FileSort {
    ByName,
    LatestFirst,
}

struct OpenSceneWindow {
    filter: String,
    current_path: PathBuf,
    file_sort: FileSort,
}

impl Default for OpenSceneWindow {
    fn default() -> Self {
        Self {
            filter: String::new(),
            current_path: PathBuf::from("scenes"),
            file_sort: FileSort::LatestFirst,
        }
    }
}

pub fn draw_scene_actions(
    mut egui_ctx: EguiContexts,
    gui_icons: Res<GuiIcons>,
    mut commands: Commands,
    mut open_window: Local<Option<SceneWindows>>,
    mut palette_config: ResMut<PaletteConfig>,
    assets: Res<Assets<PaletteList>>,
    mut open_state: Local<OpenSceneWindow>,
) {
    let ctx = egui_ctx.ctx_mut().expect("primary egui context");

    let toolbar = egui::Window::new("Scene actions")
        .anchor(Align2::LEFT_TOP, [1.0, 36.0])
        .title_bar(false)
        .auto_sized()
        .show_translucent(ctx, |ui| {
            ui.vertical(|ui| {
                if ui.add(IconButton::new(gui_icons.new, 32.0)).clicked() {
                    *open_window = Some(SceneWindows::NewScene);
                }
                if ui.add(IconButton::new(gui_icons.save, 32.0)).clicked() {
                    *open_window = Some(SceneWindows::Save);
                }
                if ui.add(IconButton::new(gui_icons.open, 32.0)).clicked() {
                    *open_window = Some(SceneWindows::Open);
                }
            });
        }).expect("toolbar should always be open");

    let mut open = true;
    let make_window = |title: &str| {
        egui::Window::new(title)
            .fixed_pos(toolbar.response.rect.right_top() + egui::Vec2::new(1.0, 0.0))
            .auto_sized()
    };
    match *open_window {
        Some(SceneWindows::NewScene) => {
            make_window("New scene")
                .open(&mut open)
                .show_translucent(ctx, |ui| {
                    ui.style_mut().spacing.item_spacing = egui::Vec2::new(3.0, 3.0);
                    ui.vertical(|ui| {
                        for (name, palette) in assets
                            .get(&palette_config.palettes)
                            .unwrap()
                            .0
                            .iter()
                            .chain(std::iter::once((&"Default".into(), &Default::default())))
                        {
                            if ui.button(name).clicked() {
                                palette_config.current_palette = *palette;
                                commands.queue(crate::script::thyme::scene::queue_new);
                                *open_window = None;
                            }
                        }
                    });
                });
        }
        Some(SceneWindows::Open) => {
            make_window("My scenes")
                .open(&mut open)
                .default_height(580.0)
                .show_translucent(ctx, |ui| {
                    ui.vertical(|ui| {
                        ui.style_mut().spacing.item_spacing = egui::Vec2::new(10.0, 3.0);
                        ui.horizontal(|ui| {
                            ui.label("Filter:");
                            ui.text_edit_singleline(&mut open_state.filter);
                            if ui.add_enabled(!open_state.filter.is_empty(), IconButton::new(gui_icons.clear, 16.0)).clicked() {
                                open_state.filter.clear();
                            }
                        });
                        ui.horizontal(|ui| {
                            if ui.add(IconButton::new(gui_icons.home, 16.0)).clicked() {
                                // todo
                            }
                            let mut new_path = None;
                            for (i, comp) in open_state.current_path.components().enumerate() {
                                if let std::path::Component::Normal(name) = comp {
                                    if ui.button(name.to_string_lossy()).clicked() {
                                        new_path = Some(i);
                                    }
                                    ui.label("/");
                                }
                            }
                            if let Some(i) = new_path {
                                open_state.current_path = open_state
                                    .current_path
                                    .components()
                                    .take(i + 1)
                                    .collect();
                            }
                            if ui.add(IconButton::new(gui_icons.open, 16.0)).clicked() {
                                // todo: folder choose dialog
                            }
                            if ui.add(IconButton::new(gui_icons.redo, 16.0)).clicked() {
                                // todo: open current folder in explorer
                            }
                        });
                        ui.horizontal(|ui| {
                            if image_radio(ui, &gui_icons, open_state.file_sort == FileSort::ByName, "Sort by name") {
                                open_state.file_sort = FileSort::ByName;
                            }
                            if image_radio(ui, &gui_icons, open_state.file_sort == FileSort::LatestFirst, "Latest first") {
                                open_state.file_sort = FileSort::LatestFirst;
                            }
                        });
                        // scroll area containing:
                        // 100px column for folders then fill the rest with the file list
                        egui::ScrollArea::vertical()
                            .max_height(400.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        // folders
                                        for entry in std::fs::read_dir(&open_state.current_path).unwrap() {
                                            let entry = entry.unwrap();
                                            if entry.file_type().unwrap().is_dir() {
                                                if ui.button(entry.file_name().to_string_lossy()).clicked() {
                                                    open_state.current_path.push(entry.file_name());
                                                }
                                            }
                                        }
                                    });
                                    ui.vertical(|ui| {
                                        // files
                                        let mut entries: Vec<_> = std::fs::read_dir(&open_state.current_path)
                                            .unwrap()
                                            .filter_map(|entry| {
                                                let entry = entry.ok()?;
                                                if entry.file_type().ok()?.is_file() {
                                                    Some((entry.path(), entry))
                                                } else {
                                                    None
                                                }
                                            })
                                            .filter(|(path, _)| {
                                                path.extension()
                                                    .map(|ext| ext == "phz" || ext == "phn")
                                                    .unwrap_or(false)
                                            })
                                            .collect();
                                        match open_state.file_sort {
                                            FileSort::ByName => entries.sort_by_key(|(_, e)| e.file_name()),
                                            FileSort::LatestFirst => entries.sort_by_key(|(_, e)| std::cmp::Reverse(e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH))),
                                        }
                                        for (path, entry) in entries {
                                            let file_name = entry.file_name();
                                            if !open_state.filter.is_empty() && !file_name.to_string_lossy().contains(&open_state.filter) {
                                                continue;
                                            }
                                            if ui.button(file_name.to_string_lossy()).clicked() {
                                                commands.queue(move |world: &mut World| {
                                                    crate::script::thyme::scene::queue_path(world, path, false)
                                                });
                                                *open_window = None;
                                            }
                                        }
                                    });
                                });
                            });
                    });
                });
        }
        Some(SceneWindows::Save) => {
            // Save window logic here
        }
        None => {}
    }
    if !open {
        *open_window = None;
    }
}
