use std::time::Duration;
use crate::{GuiIcons, ToolIcons, UiState};

use bevy::log::info;
use bevy::math::{Vec2, Vec3};
use bevy::prelude::{
    Commands, Entity, EventReader, Local, Query, Res, ResMut, SystemSet, Time, Transform, With,
};
use bevy_egui::egui::{
    pos2, vec2, Align2, Context, Id, NumExt, Response, Sense, Separator, TextStyle, TextureId, Ui,
    Widget, WidgetInfo, WidgetText, WidgetType,
};
use bevy_egui::{egui, EguiContext};
use bevy_mouse_tracking_plugin::MainCamera;
use bevy_rapier2d::plugin::{RapierConfiguration, TimestepMode};
use derivative::Derivative;

struct IconButton {
    icon: egui::widgets::Image,
    selected: bool,
}

impl IconButton {
    fn new(icon: TextureId, size: f32) -> Self {
        Self {
            icon: egui::widgets::Image::new(icon, Vec2::splat(size).to_array()),
            selected: false,
        }
    }

    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl Widget for IconButton {
    fn ui(self, ui: &mut Ui) -> Response {
        let Self { icon, selected } = self;
        let desired_size = icon.size() + vec2(2.0, 2.0);

        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());
        response.widget_info(|| WidgetInfo::new(WidgetType::ImageButton));

        if ui.is_rect_visible(rect) {
            let visuals = ui.style().interact(&response);

            if response.hovered() {
                ui.painter().rect(
                    rect.expand(visuals.expansion),
                    visuals.rounding,
                    visuals.bg_fill,
                    visuals.bg_stroke,
                );
            }
            if selected {
                let selection = ui.visuals().selection;
                ui.painter().rect(
                    rect.expand(visuals.expansion),
                    visuals.rounding,
                    selection.bg_fill,
                    selection.stroke,
                );
            }

            let image_rect =
                egui::Rect::from_min_size(pos2(rect.min.x + 1.0, rect.min.y + 1.0), icon.size());
            icon.paint_at(ui, image_rect);
        }

        response
    }
}

pub struct MenuItem {
    icon: Option<egui::widgets::Image>,
    text: String,
    icon_right: Option<egui::widgets::Image>,
    selected: bool
}

impl MenuItem {
    const ICON_SIZE: f32 = 16.0;

    fn gen_image(icon: TextureId) -> egui::widgets::Image {
        egui::widgets::Image::new(icon, Vec2::splat(Self::ICON_SIZE).to_array())
    }

    fn button(icon: Option<TextureId>, text: String) -> Self {
        Self {
            icon: icon.map(Self::gen_image),
            text,
            icon_right: None,
            selected: false
        }
    }

    fn menu(icon: Option<TextureId>, text: String, icon_right: TextureId) -> Self {
        Self {
            icon_right: Some(Self::gen_image(icon_right)),
            ..Self::button(icon, text)
        }
    }

    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl Widget for MenuItem {
    fn ui(self, ui: &mut Ui) -> Response {
        let Self {
            icon,
            text,
            icon_right,
            selected
        } = self;
        let button_padding = ui.spacing().button_padding;
        let icon_count = 1 + icon_right.is_some() as usize;
        let icon_width = Self::ICON_SIZE * ui.spacing().icon_spacing;
        let icon_width_total = icon_width * icon_count as f32;
        let text_wrap_width = ui.available_width() - button_padding.x * 2.0 - icon_width_total;

        let text: WidgetText = text.into();
        let text = text.into_galley(ui, Some(false), text_wrap_width, TextStyle::Button);
        let mut desired_size = text.size();
        desired_size.x += icon_width_total;
        desired_size.y = desired_size.y.max(Self::ICON_SIZE);
        desired_size.y = desired_size.y.at_least(ui.spacing().interact_size.y);
        desired_size += button_padding * 2.0;

        desired_size.x = desired_size.x.at_least(ui.available_width());

        let (rect, response) = ui.allocate_at_least(desired_size, Sense::click());
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, text.text()));

        if ui.is_rect_visible(rect) {
            let visuals = ui.style().interact(&response);

            if response.hovered() {
                ui.painter().rect(
                    rect.expand(visuals.expansion),
                    visuals.rounding,
                    visuals.bg_fill,
                    visuals.bg_stroke,
                );
            }
            if selected {
                let selection = ui.visuals().selection;
                ui.painter().rect(
                    rect.expand(visuals.expansion),
                    visuals.rounding,
                    selection.bg_fill,
                    selection.stroke,
                );
            }

            let text_pos = {
                let icon_spacing = ui.spacing().icon_spacing;
                pos2(
                    rect.min.x + button_padding.x + Self::ICON_SIZE + icon_spacing,
                    rect.center().y - text.size().y / 2.0,
                )
            };
            text.paint_with_visuals(ui.painter(), text_pos, visuals);

            if let Some(icon) = icon {
                let image_rect = egui::Rect::from_min_size(
                    pos2(rect.min.x, rect.center().y - 0.5 - (Self::ICON_SIZE / 2.0)),
                    vec2(Self::ICON_SIZE, Self::ICON_SIZE),
                );
                icon.paint_at(ui, image_rect);
            }

            if let Some(icon) = icon_right {
                let image_rect = egui::Rect::from_min_size(
                    pos2(rect.max.x - Self::ICON_SIZE, rect.center().y - 0.5 - (Self::ICON_SIZE / 2.0)),
                    vec2(Self::ICON_SIZE, Self::ICON_SIZE),
                );
                icon.paint_at(ui, image_rect);
            }
        }

        response
    }
}


#[derive(Derivative)]
#[derivative(Debug)]
pub struct WindowData {
    entity: Option<Entity>,
    #[derivative(Debug = "ignore")]
    handler: Box<dyn FnMut(&Context, &Time, &mut UiState) + Sync + Send>,
}

impl WindowData {
    fn new(entity: Option<Entity>, handler: impl FnMut(&Context, &Time, &mut UiState) + Sync + Send + 'static) -> Self {
        Self {
            entity,
            handler: Box::new(handler),
        }
    }
}

pub fn show_subwindows(
    mut ui_state: ResMut<UiState>,
    mut commands: Commands,
    egui_ctx: ResMut<EguiContext>,
    time: Res<Time>
) {
    ui_state.windows.retain(|_, w| {
        w.entity
            .map_or(true, |ent| commands.get_entity(ent).is_some())
    });

    let ids: Vec<Id> = ui_state.windows.keys().copied().collect();
    for id in ids {
        let val = ui_state.windows.remove(&id);
        if let Some(mut wnd) = val {
            (wnd.handler)(egui_ctx.clone().ctx_mut(), &time, &mut ui_state);
            ui_state.windows.insert(id, wnd);
        }
    }
}

pub struct GravitySetting {
    value: Vec2,
    enabled: bool,
}

impl Default for GravitySetting {
    fn default() -> Self {
        Self {
            value: Vec2::new(0.0, -9.81),
            enabled: true,
        }
    }
}

pub struct SeparatorCustom {
    spacing: f32,
    is_horizontal_line: Option<bool>,
}

impl Default for SeparatorCustom {
    fn default() -> Self {
        Self {
            spacing: 6.0,
            is_horizontal_line: None,
        }
    }
}

impl SeparatorCustom {
    /// How much space we take up. The line is painted in the middle of this.
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }

    /// Explicitly ask for a horizontal line.
    /// By default you will get a horizontal line in vertical layouts,
    /// and a vertical line in horizontal layouts.
    pub fn horizontal(mut self) -> Self {
        self.is_horizontal_line = Some(true);
        self
    }

    /// Explicitly ask for a vertical line.
    /// By default you will get a horizontal line in vertical layouts,
    /// and a vertical line in horizontal layouts.
    pub fn vertical(mut self) -> Self {
        self.is_horizontal_line = Some(false);
        self
    }
}

impl Widget for SeparatorCustom {
    fn ui(self, ui: &mut Ui) -> Response {
        let Self {
            spacing,
            is_horizontal_line,
        } = self;

        let is_horizontal_line =
            is_horizontal_line.unwrap_or_else(|| !ui.layout().main_dir().is_horizontal());

        let available_space = ui.min_size();

        let size = if is_horizontal_line {
            vec2(available_space.x, spacing)
        } else {
            vec2(spacing, available_space.y)
        };

        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());

        if ui.is_rect_visible(response.rect) {
            let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
            let painter = ui.painter();
            if is_horizontal_line {
                painter.hline(
                    rect.x_range(),
                    painter.round_to_pixel(rect.center().y),
                    stroke,
                );
            } else {
                painter.vline(
                    painter.round_to_pixel(rect.center().x),
                    rect.y_range(),
                    stroke,
                );
            }
        }

        response
    }
}

pub fn ui_example(
    mut egui_ctx: ResMut<EguiContext>,
    ui_state: ResMut<UiState>,
    mut is_initialized: Local<bool>,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
) {
    if !*is_initialized {
        let mut camera = cameras.single_mut();
        camera.scale = Vec3::new(0.01, 0.01, 1.0);
        *is_initialized = true;
    }

    egui::TopBottomPanel::top("top_panel").show(egui_ctx.ctx_mut(), |ui| {
        // The top panel is often a good place for a menu bar:
        egui::menu::bar(ui, |ui| {
            egui::menu::menu_button(ui, "File", |ui| {
                if ui.button("Quit").clicked() {
                    std::process::exit(0);
                }
            });
        });
    });

    egui::Window::new("Debug").show(egui_ctx.clone().ctx_mut(), |ui| {
        ui.monospace(format!("{:#?}", ui_state));
    });
}

pub fn draw_ui() -> SystemSet {
    SystemSet::new()
        .with_system(ui_example)
        .with_system(draw_toolbox)
        .with_system(draw_bottom_toolbar)
        .with_system(show_subwindows)
}

pub fn draw_toolbox(
    mut egui_ctx: ResMut<EguiContext>,
    mut ui_state: ResMut<UiState>,
    tool_icons: Res<ToolIcons>,
) {
    egui::Window::new("Tools")
        .anchor(Align2::LEFT_BOTTOM, [0.0, 0.0])
        .title_bar(false)
        .resizable(false)
        .default_size(egui::Vec2::ZERO)
        .show(egui_ctx.clone().ctx_mut(), |ui| {
            ui.vertical(|ui| {
                let ui_state = &mut *ui_state;
                for (i, category) in ui_state.toolbox.iter().enumerate() {
                    if i > 0 {
                        ui.add(SeparatorCustom::default().horizontal());
                    }
                    for chunk in category.chunks(2) {
                        ui.horizontal(|ui| {
                            for def in chunk {
                                if ui
                                    .add(
                                        IconButton::new(
                                            egui_ctx.add_image(def.icon(&tool_icons)),
                                            24.0,
                                        )
                                        .selected(ui_state.toolbox_selected.is_same(def)),
                                    )
                                    .clicked()
                                {
                                    ui_state.toolbox_selected = *def;
                                }
                            }
                        });
                    }
                }
            });
        });
}

pub fn draw_bottom_toolbar(
    mut egui_ctx: ResMut<EguiContext>,
    mut ui_state: ResMut<UiState>,
    mut rapier: ResMut<RapierConfiguration>,
    mut gravity_conf: Local<GravitySetting>,
    tool_icons: Res<ToolIcons>,
    gui_icons: Res<GuiIcons>,
) {
    egui::Window::new("Tools2")
        .anchor(Align2::CENTER_BOTTOM, [0.0, 0.0])
        .title_bar(false)
        .resizable(false)
        .show(egui_ctx.clone().ctx_mut(), |ui| {
            ui.horizontal(|ui| {
                let ui_state = &mut *ui_state;
                for def in ui_state.toolbox_bottom.iter() {
                    if ui
                        .add(
                            IconButton::new(egui_ctx.add_image(def.icon(&tool_icons)), 32.0)
                                .selected(ui_state.toolbox_selected.is_same(def)),
                        )
                        .clicked()
                    {
                        ui_state.toolbox_selected = *def;
                    }
                }

                ui.separator();

                let playpause = ui.add(IconButton::new(
                    if rapier.physics_pipeline_active {
                        gui_icons.pause
                    } else {
                        gui_icons.play
                    },
                    32.0,
                ));

                if playpause.clicked() {
                    rapier.physics_pipeline_active = !rapier.physics_pipeline_active;
                }
                playpause.context_menu(|ui| {
                    let (max_dt, mut time_scale, substeps) = match rapier.timestep_mode {
                        TimestepMode::Variable {
                            max_dt,
                            time_scale,
                            substeps,
                        } => (max_dt, time_scale, substeps),
                        _ => unreachable!("Shouldn't happen"),
                    };
                    ui.add(
                        egui::Slider::new(&mut time_scale, 0.1..=10.0)
                            .logarithmic(true)
                            .text("Simulation speed"),
                    );
                    rapier.timestep_mode = TimestepMode::Variable {
                        max_dt,
                        time_scale,
                        substeps,
                    };
                });

                ui.separator();

                let gravity =
                    ui.add(IconButton::new(gui_icons.gravity, 32.0).selected(gravity_conf.enabled));
                if gravity.clicked() {
                    gravity_conf.enabled = !gravity_conf.enabled;
                    if gravity_conf.enabled {
                        rapier.gravity = gravity_conf.value;
                    } else {
                        rapier.gravity = Vec2::ZERO;
                    }
                }
            })
        });
}

pub struct ContextMenuEvent {
    pub screen_pos: Vec2,
}

pub fn handle_context_menu(
    mut ev: EventReader<ContextMenuEvent>,
    time: Res<Time>,
    mut ui: ResMut<UiState>,
    icons: Res<GuiIcons>,
) {
    fn item(ui: &mut Ui, text: &'static str, icon: Option<TextureId>) -> bool {
        ui.add(MenuItem::button(icon, text.to_string())).clicked()
    }

    let icons = *icons;
    for ev in ev.iter() {
        info!("context menu at {:?}", ev.screen_pos);

        let id = Id::new(time.elapsed());
        let pos = ev.screen_pos.to_array();
        //let pos = [ev.screen_pos.x, egui]
        let entity = ui.selected_entity.map(|sel| sel.entity);
        ui.window_temp = Some(id);
        type MenuId = &'static str;
        let mut hovered_item: Option<(MenuId, Duration)> = None;
        let mut selected_item: Option<MenuId> = None;
        let mut our_child_window: Option<Id> = None;
        ui.windows.insert(
            id,
            WindowData::new(entity, move |ctx, time, ui_state| {
                egui::Window::new("context menu")
                    .id(id)
                    .default_pos(pos)
                    .default_size(vec2(0.0, 0.0))
                    .resizable(false)
                    .show(ctx, |ui| {
                        macro_rules! item {
                            ($text:literal, $icon:ident) => {
                                item(ui, $text, Some(icons.$icon))
                            };
                            ($text:literal) => {
                                item(ui, $text, None)
                            };
                        }
                        let child_id = Id::new(time.elapsed());
                        let child_wnd = egui::Window::new("child window")
                            .id(child_id)
                            .default_size(vec2(0.0, 0.0))
                            .resizable(false);
                        macro_rules! menu {
                            (@ $text: literal, $icon: expr, $wnd: expr) => {
                                let our_id = $text;
                                let us_selected = matches!(selected_item, Some(id) if id == our_id);
                                let menu = ui.add(MenuItem::menu($icon, $text.to_string(), icons.arrow_right).selected(us_selected));

                                if !us_selected {
                                    let selected = match hovered_item {
                                        Some((id, at)) if id == our_id && (time.elapsed() - at) > Duration::from_millis(500) => true,
                                        _ => menu.clicked()
                                    };

                                    if selected {
                                        info!("clicked: {}", $text);
                                        selected_item = Some(our_id);

                                        let child_id = Id::new(time.elapsed());
                                        ui_state.windows.insert(child_id, WindowData::new(entity, move |ctx, time, ui_state| {
                                            egui::Window::new("child window")
                                                .id(child_id)
                                                .default_size(vec2(0.0, 0.0))
                                                .resizable(false)
                                                .show(ctx, $wnd);
                                        }));

                                        if let Some(id) = our_child_window {
                                            ui_state.windows.remove(id);
                                        }

                                        our_child_window = Some(child_id);
                                    }
                                }

                                let us = matches!(hovered_item, Some((id, _)) if id == our_id);
                                if menu.hovered() && !us { // we're hovering but someone else was
                                    hovered_item = Some((our_id, time.elapsed())); // we're the new hoverer
                                } else if !menu.hovered() && us { // not hovering and we were
                                    hovered_item = None; // now we're not
                                }
                            };
                            ($text: literal, $icon: ident, $wnd: expr) => {
                                menu!(@ $text, Some(icons.$icon), $wnd);
                            };
                            ($text: literal, /, $wnd: expr) => {
                                menu!(@ $text, None, $wnd);
                            };
                        }

                        match entity {
                            Some(_) => {
                                if item!("Erase", erase) {}
                                if item!("Mirror", mirror) {}
                                if item!("Show plot", plot) {}
                                ui.add(Separator::default().horizontal());

                                menu!("Selection", /, |ui| { ui.label("Hello"); });
                                menu!("Appearance", color, |ui| { ui.label("Hello"); });
                                menu!("Text", text, |ui| { ui.label("Hello"); });
                                menu!("Material", material, |ui| { ui.label("Hello"); });
                                menu!("Velocities", velocity, |ui| { ui.label("Hello"); });
                                menu!("Information", info, |ui| { ui.label("Hello"); });
                                menu!("Collision layers", collisions, |ui| { ui.label("Hello"); });
                                menu!("Geometry actions", /, |ui| { ui.label("Hello"); });
                                menu!("Combine shapes", csg, |ui| { ui.label("Hello"); });
                                menu!("Controller", controller, |ui| { ui.label("Hello"); });
                                menu!("Script menu", /, |ui| { ui.label("Hello"); });
                            }
                            None => {
                                if item!("Zoom to scene", zoom2scene) {}
                                if item!("Default view") {}
                                if item!("Background", color) {}
                            }
                        }
                    });
            }),
        );
    }
}
