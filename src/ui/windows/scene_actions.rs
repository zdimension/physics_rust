use std::{path::PathBuf, time::SystemTime};

use crate::palette::{PaletteConfig, PaletteList};
use crate::{ egui_systems};
use bevy::prelude::*;
use bevy_egui::egui::Align2;
use bevy_egui::{egui, EguiContexts};

use crate::ui::icon_button::IconButton;
use crate::ui::images::GuiIcons;
use crate::ui::{InitialPos, Subwindow, WindowExt, image_radio};

egui_systems!(draw_scene_actions);

#[derive(Copy, Clone, PartialEq, Eq)]
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
    folders: Vec<(String, PathBuf)>,
    files: Vec<SceneFile>,
}

struct SceneFile {
    name: String,
    path: PathBuf,
    modified: SystemTime,
}

impl Default for OpenSceneWindow {
    fn default() -> Self {
        Self {
            filter: String::new(),
            current_path: PathBuf::from("scenes"),
            file_sort: FileSort::LatestFirst,
            folders: Vec::new(),
            files: Vec::new(),
        }
    }
}

impl OpenSceneWindow {
    fn open(&mut self, path: PathBuf) {
        self.current_path = path;
        self.refresh();
    }

    fn refresh(&mut self) {
        self.folders.clear();
        self.files.clear();
        let Ok(entries) = std::fs::read_dir(&self.current_path) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else { continue };
            let name = entry.file_name().to_string_lossy().into_owned();
            if kind.is_dir() {
                self.folders.push((name, path));
            } else if kind.is_file()
                && path
                    .extension()
                    .is_some_and(|ext| ext == "phz" || ext == "phn")
            {
                self.files.push(SceneFile {
                    name,
                    path,
                    modified: entry
                        .metadata()
                        .and_then(|metadata| metadata.modified())
                        .unwrap_or(SystemTime::UNIX_EPOCH),
                });
            }
        }
        self.sort();
    }

    fn sort(&mut self) {
        match self.file_sort {
            FileSort::ByName => self.files.sort_by(|a, b| a.name.cmp(&b.name)),
            FileSort::LatestFirst => self.files.sort_by_key(|file| std::cmp::Reverse(file.modified)),
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
                for (icon, window) in [
                    (gui_icons.new, SceneWindows::NewScene),
                    (gui_icons.save, SceneWindows::Save),
                    (gui_icons.open, SceneWindows::Open),
                ] {
                    if ui.add(IconButton::new(icon, 32.0).selected(*open_window == Some(window))).clicked() {
                        if window == SceneWindows::Open {
                            open_state.refresh();
                        }
                        *open_window = Some(window);
                    }
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
                                let path = open_state
                                    .current_path
                                    .components()
                                    .take(i + 1)
                                    .collect();
                                open_state.open(path);
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
                                open_state.sort();
                            }
                            if image_radio(ui, &gui_icons, open_state.file_sort == FileSort::LatestFirst, "Latest first") {
                                open_state.file_sort = FileSort::LatestFirst;
                                open_state.sort();
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
                                        let next = open_state.folders.iter().find_map(|(name, path)| {
                                            ui.button(name).clicked().then(|| path.clone())
                                        });
                                        if let Some(path) = next {
                                            open_state.open(path);
                                        }
                                    });
                                    ui.vertical(|ui| {
                                        // files
                                        for file in &open_state.files {
                                            if !open_state.filter.is_empty() && !file.name.contains(&open_state.filter) {
                                                continue;
                                            }
                                            if ui.button(&file.name).clicked() {
                                                let path = file.path.clone();
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
