use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::egui_systems;
use crate::palette::{PaletteConfig, PaletteList};
use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::tasks::{IoTaskPool, Task, block_on, poll_once};
use bevy_egui::egui::{Align2, Atom, AtomLayout, Direction, TextureHandle, load::SizedTexture};
use bevy_egui::{EguiContexts, egui};

use crate::ui::icon_button::IconButton;
use crate::ui::images::GuiIcons;
use crate::ui::{WindowExt, image_radio};

egui_systems!(draw_scene_actions);

#[derive(Copy, Clone, PartialEq, Eq)]
pub(super) enum SceneWindows {
    NewScene,
    Open,
    Save,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum FileSort {
    ByName,
    LatestFirst,
}

pub(super) struct OpenSceneWindow {
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
    thumbnail: Thumbnail,
}

enum Thumbnail {
    Loading(Task<Option<egui::ColorImage>>),
    Ready(Option<TextureHandle>),
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
            let Ok(kind) = entry.file_type() else {
                continue;
            };
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
                    thumbnail: Thumbnail::load(&path),
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
            FileSort::LatestFirst => self
                .files
                .sort_by_key(|file| std::cmp::Reverse(file.modified)),
        }
    }
}

impl Thumbnail {
    fn load(path: &Path) -> Self {
        if !path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("phz"))
        {
            return Self::Ready(None);
        }
        let path = path.to_owned();
        Self::Loading(IoTaskPool::get().spawn(async move { load_thumbnail(&path) }))
    }

    fn poll(&mut self, ctx: &egui::Context, name: &str) {
        let image = match self {
            Self::Loading(task) => block_on(poll_once(task)),
            Self::Ready(_) => return,
        };
        if let Some(image) = image {
            *self = Self::Ready(
                image.map(|image| ctx.load_texture(name, image, egui::TextureOptions::LINEAR)),
            );
        }
    }
}

fn load_thumbnail(path: &Path) -> Option<egui::ColorImage> {
    let mut archive = zip::ZipArchive::new(File::open(path).ok()?).ok()?;
    let mut bytes = Vec::new();
    archive
        .by_name("thumb.png")
        .ok()?
        .read_to_end(&mut bytes)
        .ok()?;
    let image = Image::from_buffer(
        &bytes,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::linear(),
        RenderAssetUsages::default(),
    )
    .ok()?
    .convert(TextureFormat::Rgba8UnormSrgb)?;
    egui::ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.data.as_deref()?,
    )
    .into()
}

fn image_button(
    ui: &mut egui::Ui,
    image: Atom<'_>,
    text: &str,
    spinner: Option<egui::Id>,
    size: Option<egui::Vec2>,
) -> egui::Response {
    let mut layout = AtomLayout::new((image, text))
        .direction(Direction::TopDown)
        .align2(Align2::CENTER_CENTER)
        .wrap_mode(if size.is_some() {
            egui::TextWrapMode::Wrap
        } else {
            egui::TextWrapMode::Extend
        });
    if let Some(size) = size {
        layout = layout.max_width(size.x - 15.0);
    }
    let mut content = Atom::layout(layout);
    content.grow = true;
    let mut button = egui::Button::new(content);
    if let Some(size) = size {
        button = button.min_size(size);
    }
    let response = button.atom_ui(ui);
    if let Some(rect) = spinner.and_then(|id| response.rect(id)) {
        egui::Spinner::new().size(32.0).paint_at(
            ui,
            egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(32.0)),
        );
    }
    response.response
}

fn icon_atom(icon: egui::TextureId, size: f32) -> Atom<'static> {
    egui::Image::new(SizedTexture::new(icon, [size; 2])).into()
}

pub(super) fn draw_scene_actions(
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
                    if ui
                        .add(IconButton::new(icon, 32.0).selected(*open_window == Some(window)))
                        .clicked()
                    {
                        if window == SceneWindows::Open {
                            open_state.refresh();
                        }
                        *open_window = Some(window);
                    }
                }
            });
        })
        .expect("toolbar should always be open");

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
                            if ui
                                .add_enabled(
                                    !open_state.filter.is_empty(),
                                    IconButton::new(gui_icons.clear, 16.0),
                                )
                                .clicked()
                            {
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
                                let path =
                                    open_state.current_path.components().take(i + 1).collect();
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
                            if image_radio(
                                ui,
                                &gui_icons,
                                open_state.file_sort == FileSort::ByName,
                                "Sort by name",
                            ) {
                                open_state.file_sort = FileSort::ByName;
                                open_state.sort();
                            }
                            if image_radio(
                                ui,
                                &gui_icons,
                                open_state.file_sort == FileSort::LatestFirst,
                                "Latest first",
                            ) {
                                open_state.file_sort = FileSort::LatestFirst;
                                open_state.sort();
                            }
                        });
                        // scroll area containing:
                        // 100px column for folders then fill the rest with the file list
                        egui::ScrollArea::vertical()
                            .max_height(400.0)
                            .show(ui, |ui| {
                                ui.horizontal_top(|ui| {
                                    ui.vertical(|ui| {
                                        // folders
                                        let folder_size = Some(egui::Vec2::new(90.0, 0.0));
                                        let parent = open_state
                                            .current_path
                                            .parent()
                                            .unwrap_or(&open_state.current_path)
                                            .to_owned();
                                        let next = image_button(
                                            ui,
                                            icon_atom(gui_icons.open_up, 32.0),
                                            "..",
                                            None,
                                            folder_size,
                                        )
                                        .clicked()
                                        .then_some(parent)
                                        .or_else(|| {
                                            open_state.folders.iter().find_map(|(name, path)| {
                                                image_button(
                                                    ui,
                                                    icon_atom(gui_icons.browse, 32.0),
                                                    name,
                                                    None,
                                                    folder_size,
                                                )
                                                .clicked()
                                                .then(|| path.clone())
                                            })
                                        });
                                        if let Some(path) = next {
                                            open_state.open(path);
                                        }
                                    });
                                    egui::Grid::new("scene files")
                                        .num_columns(4)
                                        .show(ui, |ui| {
                                            // files
                                            let filter = open_state.filter.clone();
                                            let mut column = 0;
                                            for file in &mut open_state.files {
                                                if !filter.is_empty()
                                                    && !file.name.contains(&filter)
                                                {
                                                    continue;
                                                }
                                                let texture_name = file.path.to_string_lossy();
                                                file.thumbnail.poll(ctx, &texture_name);
                                                let spinner = ui.make_persistent_id((
                                                    "scene thumbnail",
                                                    &file.path,
                                                ));
                                                let (image, spinner) = match &file.thumbnail {
                                                    Thumbnail::Loading(_) => (
                                                        Atom::custom(
                                                            spinner,
                                                            egui::Vec2::splat(100.0),
                                                        ),
                                                        Some(spinner),
                                                    ),
                                                    Thumbnail::Ready(Some(texture)) => {
                                                        (icon_atom(texture.id(), 100.0), None)
                                                    }
                                                    Thumbnail::Ready(None) => {
                                                        (icon_atom(gui_icons.new, 100.0), None)
                                                    }
                                                };
                                                if image_button(
                                                    ui,
                                                    image,
                                                    &file.name,
                                                    spinner,
                                                    Some(egui::Vec2::new(115.0, 150.0)),
                                                )
                                                .clicked()
                                                {
                                                    let path = file.path.clone();
                                                    commands.queue(move |world: &mut World| {
                                                        crate::script::thyme::scene::queue_path(
                                                            world, path, false,
                                                        )
                                                    });
                                                    *open_window = None;
                                                }
                                                column += 1;
                                                if column == 4 {
                                                    ui.end_row();
                                                    column = 0;
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
