use crate::{GuiIcons, ToolIcons, UiState};
use std::time::Duration;

use bevy::log::info;
use bevy::math::{Vec2, Vec3};
use bevy::prelude::*;
use bevy_egui::egui::{Align2, Context, Id, InnerResponse, NumExt, pos2, Pos2, Separator, Ui, vec2, Widget};
use bevy_egui::{egui, EguiContext};
use bevy_mouse_tracking_plugin::MainCamera;
use bevy_rapier2d::plugin::{RapierConfiguration, TimestepMode};
use bevy_rapier2d::prelude::*;
use derivative::Derivative;
use lyon_path::commands::CommandsPathSlice;
use icon_button::IconButton;
use separator_custom::SeparatorCustom;


mod icon_button;
mod menu_item;
mod separator_custom;

use menu_item::MenuItem;
use crate::measures::KineticEnergy;

#[derive(Derivative)]
#[derivative(Debug)]
pub struct WindowData {
    entity: Option<Entity>,
    #[derivative(Debug = "ignore")]
    handler: Box<dyn FnMut(&Context, &Time, &mut UiState, &mut Commands) + Sync + Send>,
}

impl WindowData {
    fn new(
        entity: Option<Entity>,
        handler: impl FnMut(&Context, &Time, &mut UiState, &mut Commands) + Sync + Send + 'static,
    ) -> Self {
        Self {
            entity,
            handler: Box::new(handler),
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
        .with_system(process_temporary_windows)
        .with_system(remove_temporary_windows)
        .with_system(MenuWindow::show)
        .with_system(InformationWindow::show)
}

pub fn draw_toolbox(
    mut egui_ctx: ResMut<EguiContext>,
    mut ui_state: ResMut<UiState>,
    tool_icons: Res<ToolIcons>,
    mut clear_tmp: EventWriter<RemoveTemporaryWindowsEvent>
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
                                    clear_tmp.send(RemoveTemporaryWindowsEvent);
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
    mut clear_tmp: EventWriter<RemoveTemporaryWindowsEvent>
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
                        clear_tmp.send(RemoveTemporaryWindowsEvent);
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

trait AsPos2 {
    fn as_pos2(&self) -> egui::Pos2;
}

impl AsPos2 for Vec2 {
    fn as_pos2(&self) -> egui::Pos2 {
        pos2(self.x, self.y)
    }
}

impl AsPos2 for Pos2 {
    fn as_pos2(&self) -> egui::Pos2 {
        *self
    }
}

#[derive(Component)]
struct InitialPos(Pos2, Pos2);

impl InitialPos {
    fn initial(pos: impl AsPos2) -> impl Bundle {
        let pos = pos.as_pos2();
        (Self(pos, pos), TemporaryWindow)
    }

    fn update<T>(&mut self, resp: InnerResponse<T>) {
        self.1 = resp.response.rect.left_top();
    }
}

#[derive(Component)]
pub struct TemporaryWindow;

pub struct ContextMenuEvent {
    pub screen_pos: Vec2,
}

pub fn handle_context_menu(
    mut ev: EventReader<ContextMenuEvent>,
    mut ui: ResMut<UiState>,
    mut commands: Commands,
) {
    for ev in ev.iter() {
        let entity = ui.selected_entity.map(|sel| sel.entity);
        info!("context menu at {:?} for {:?}", ev.screen_pos, entity);
        let wnd = commands
            .spawn((MenuWindow::default(), InitialPos::initial(ev.screen_pos)))
            .id();

        if let Some(id) = entity {
            commands.entity(id).push_children(&[wnd]);
        }
    }
}

fn process_temporary_windows(
    wnds: Query<(Entity, &InitialPos, &TemporaryWindow)>,
    mut commands: Commands,
) {
    for (wnd, pos, _) in wnds.iter() {
        if pos.0 != pos.1 {
            commands.entity(wnd).remove::<TemporaryWindow>();
        }
    }
}

/*
#[derive(Component)]
struct EntityWindow {
    entity: Option<Entity>
}
*/
trait BevyIdThing {
    fn id_bevy(self, id: Entity) -> Self;
}

impl<'a> BevyIdThing for egui::Window<'a> {
    fn id_bevy(mut self, id: Entity) -> Self {
        self.id(Id::new(id))
    }
}

#[derive(Default, Component)]
struct MenuWindow {
    hovered_item: Option<(MenuId, Duration)>,
    selected_item: Option<MenuId>,
    our_child_window: Option<Entity>,
}

impl Into<Pos2> for &InitialPos {
    fn into(self) -> Pos2 {
        self.0
    }
}

trait Subwindow {
    fn subwindow(self, id: Entity, ctx: &Context, initial_pos: &mut InitialPos, commands: &mut Commands, contents: impl FnOnce(&mut Ui, &mut Commands));
}

impl<'a> Subwindow for egui::Window<'a> {
    fn subwindow(mut self, id: Entity, ctx: &Context, initial_pos: &mut InitialPos, commands: &mut Commands, contents: impl FnOnce(&mut Ui, &mut Commands)) {
        let mut open = true;
        self
            .id_bevy(id)
            .default_pos(&*initial_pos)
            .open(&mut open)
            .show(ctx, |ui| {
                contents(ui, commands)
            })
            .map(|resp| initial_pos.1 = resp.response.rect.left_top());
        if !open {
            commands.entity(id).despawn_recursive();
        }
    }
}

impl MenuWindow {
    fn show(
        mut wnds: Query<(Entity, Option<&Parent>, &mut MenuWindow, &mut InitialPos)>,
        time: Res<Time>,
        mut egui_ctx: ResMut<EguiContext>,
        icons: Res<GuiIcons>,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut();
        for (id, entity, mut info_wnd, mut initial_pos) in wnds.iter_mut() {
            let entity = entity.map(Parent::get);
            egui::Window::new("context menu")
                .default_size(vec2(0.0, 0.0))
                .resizable(false)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, commands| {
                    macro_rules! item {
                            (@ $text:literal, $icon:expr) => {
                                ui.add(MenuItem::button($icon, $text.to_string())).clicked()
                            };
                            ($text:literal, $icon:ident) => {
                                item!(@ $text, Some(icons.$icon))
                            };
                            ($text:literal) => {
                                item!(@ $text, None)
                            };
                        }
                    macro_rules! menu {
                            (@ $text: literal, $icon: expr, $wnd:ty) => {
                                let our_id = $text;
                                let us_selected = matches!(info_wnd.selected_item, Some(id) if id == our_id);
                                let menu = ui.add(MenuItem::menu($icon, $text.to_string(), icons.arrow_right).selected(us_selected));

                                if !us_selected {
                                    let selected = match info_wnd.hovered_item {
                                        Some((id, at)) if id == our_id && (time.elapsed() - at) > Duration::from_millis(500) => true,
                                        _ => menu.clicked()
                                    };

                                    if selected {
                                        info!("clicked: {}", $text);
                                        info_wnd.selected_item = Some(our_id);

                                        if let Some(id) = info_wnd.our_child_window {
                                            commands.entity(id).despawn_recursive();
                                        }

                                        info!("rect: {:?}", menu.rect);

                                        let new_wnd = commands.spawn((
                                            <$wnd as Default>::default(),
                                            InitialPos::initial(menu.rect.right_top())
                                        )).id();

                                        if let Some(id) = entity {
                                            commands.entity(id).push_children(&[new_wnd]);
                                        }

                                        info_wnd.our_child_window = Some(new_wnd);
                                    }
                                }

                                let us = matches!(info_wnd.hovered_item, Some((id, _)) if id == our_id);
                                if menu.hovered() && !us { // we're hovering but someone else was
                                    info_wnd.hovered_item = Some((our_id, time.elapsed())); // we're the new hoverer
                                } else if !menu.hovered() && us { // not hovering and we were
                                    info_wnd.hovered_item = None; // now we're not
                                }
                            };
                            ($text:literal, $icon:ident, $wnd:ty) => {
                                menu!(@ $text, Some(icons.$icon), $wnd);
                            };
                            ($text:literal, /, $wnd:ty) => {
                                menu!(@ $text, None, $wnd);
                            };
                        }

                    match entity {
                        Some(id) => {
                            if item!("Erase", erase) {
                                commands.entity(id).despawn_recursive();
                            }
                            if item!("Mirror", mirror) {}
                            if item!("Show plot", plot) {}
                            ui.add(Separator::default().horizontal());

                            menu!("Selection", /, SelectionWindow);
                            menu!("Appearance", color, AppearanceWindow);
                            menu!("Text", text, TextWindow);
                            menu!("Material", material, MaterialWindow);
                            menu!("Velocities", velocity, VelocitiesWindow);
                            menu!("Information", info, InformationWindow);
                            menu!("Collision layers", collisions, CollisionsWindow);
                            menu!("Geometry actions", /, GeometryActionsWindow);
                            menu!("Combine shapes", csg, CombineShapesWindow);
                            menu!("Controller", controller, ControllerWindow);
                            menu!("Script menu", /, ScriptMenuWindow);
                        }
                        None => {
                            if item!("Zoom to scene", zoom2scene) {}
                            if item!("Default view") {}
                            if item!("Background", color) {}
                        }
                    }
                });
        }
    }
}

type MenuId = &'static str;

#[derive(Default, Component)]
struct SelectionWindow;

#[derive(Default, Component)]
struct AppearanceWindow;

#[derive(Default, Component)]
struct TextWindow;

#[derive(Default, Component)]
struct MaterialWindow;

#[derive(Default, Component)]
struct VelocitiesWindow;

#[derive(Default, Component)]
struct InformationWindow;

impl InformationWindow {
    fn show(
        mut wnds: Query<(Entity, &Parent, &mut InitialPos), With<InformationWindow>>,
        ents: Query<(
            Option<&Transform>,
            Option<&ReadMassProperties>,
            Option<&Velocity>,
            Option<&ColliderMassProperties>,
            Option<&KineticEnergy>
        )>,
        rapier_conf: Res<RapierConfiguration>,
        mut egui_ctx: ResMut<EguiContext>,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut();
        for (id, parent, mut initial_pos) in wnds.iter_mut() {
            let (
                xform,
                mass,
                vel,
                coll_mass,
                kine
            ) = ents.get(parent.get()).unwrap();
            egui::Window::new("info")
                .id_bevy(id)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, commands| {
                    fn line(ui: &mut Ui, label: &'static str, val: String) {
                        ui.label(label);
                        ui.label(val);
                        ui.end_row();
                    }
                    egui::Grid::new("info grid").striped(true).show(ui, |ui| {
                        if let Some(ReadMassProperties(mass)) = mass {
                            line(ui, "Mass", format!("{:.3} kg", mass.mass));

                            line(ui, "Moment of inertia", format!("{:.3} kgm²", mass.principal_inertia));
                        }

                        if let Some(props) = coll_mass {
                            line(ui, "Collider mass", format!("{:?}", props));
                        }

                        if let Some(xform) = xform {
                            line(ui, "Position", format!("[x={:.3}, y={:.3}] m", xform.translation.x, xform.translation.y));
                        }

                        if let Some(vel) = vel {
                            line(ui, "Velocity", format!("[x={:.3}, y={:.3}] m/s", vel.linvel.x, vel.linvel.y));

                            line(ui, "Angular velocity", format!("{:.3} rad/s", vel.angvel));
                        }
                    });
                    ui.separator();
                    egui::Grid::new("info grid 2").striped(true).show(ui, |ui| {
                        let mut total = 0.0;

                        if let Some(KineticEnergy { linear, angular }) = kine {
                            line(ui, "Kinetic linear energy", format!("{:.3} J", linear));
                            line(ui, "Kinetic angular energy", format!("{:.3} J", angular));
                            total += linear + angular;
                        }

                        if let Some(ReadMassProperties(mass)) = mass {
                            let pot = mass.mass * -rapier_conf.gravity.y * xform.unwrap().translation.y;
                            line(ui, "Potential energy (gravity)", format!("{:.3} J", pot)); // todo: nonvertical gravity
                            total += pot;
                        }

                        line(ui, "Energy (total)", format!("{:.3} J", total));
                    });
                });
        }
    }
}

#[derive(Default, Component)]
struct CollisionsWindow;

#[derive(Default, Component)]
struct GeometryActionsWindow;

#[derive(Default, Component)]
struct CombineShapesWindow;

#[derive(Default, Component)]
struct ControllerWindow;

#[derive(Default, Component)]
struct ScriptMenuWindow;


pub struct RemoveTemporaryWindowsEvent;

fn remove_temporary_windows(
    mut commands: Commands,
    mut events: EventReader<RemoveTemporaryWindowsEvent>,
    wnds: Query<Entity, With<TemporaryWindow>>,
) {
    for _ in events.iter() {
        for id in wnds.iter() {
            commands.entity(id).despawn_recursive();
        }
    }
}
