use std::time::Duration;

use bevy::log::info;
use bevy::math::{Vec2, Vec3Swizzles};
use bevy::prelude::*;
use bevy_diagnostic::{Diagnostics, FrameTimeDiagnosticsPlugin};
use bevy_egui::{egui, EguiContext};
use bevy_egui::egui::{Context, Id, InnerResponse, pos2, Pos2, Ui};
use bevy_mouse_tracking_plugin::{MainCamera, MousePosWorld};
use bevy_rapier2d::plugin::RapierConfiguration;
use derivative::Derivative;

use windows::object::plot::PlotWindow;

use crate::{demo, Despawn, UsedMouseButton};
use crate::objects::laser::LaserRays;
use crate::palette::{PaletteConfig, PaletteList};
use crate::tools::ToolEnum;
use crate::ui::windows::object::appearance::AppearanceWindow;
use crate::ui::windows::object::laser::LaserWindow;
use crate::ui::windows::object::material::MaterialWindow;
use crate::ui::windows::scene::background::BackgroundWindow;
use crate::ui::windows::{scene_actions, toolbar, toolbox};

use self::windows::menu::MenuWindow;
use self::windows::object::collisions::CollisionsWindow;
use self::windows::object::information::InformationWindow;

pub mod cursor;
mod icon_button;
pub mod images;
mod menu_item;
pub(crate) mod selection_overlay;
mod separator_custom;
mod windows;

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

#[derive(Component)]
pub struct Scene;

pub fn ui_example(
    mut egui_ctx: ResMut<EguiContext>,
    ui_state: ResMut<UiState>,
    mut is_initialized: Local<bool>,
    cameras: Query<&mut Transform, With<MainCamera>>,
    mut palette_config: ResMut<PaletteConfig>,
    assets: Res<Assets<PaletteList>>,
    laser: Query<&LaserRays>,
    mut cmds: Commands,
    mouse: Res<MousePosWorld>,
    rapier: Res<RapierConfiguration>,
    diag: Res<Diagnostics>,
) {
    if !*is_initialized {
        palette_config.current_palette = *assets
            .get(&palette_config.palettes)
            .unwrap()
            .0
            .get("Optics")
            .unwrap();

        demo::lasers::init(&mut cmds);
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
        ui.collapsing("Mouse", |ui| {
            ui.label(format!("{:.2} m", mouse.xy()));
        });
        ui.collapsing("Laser", |ui| {
            ui.monospace(&laser.single().debug);
        });
        let tr = cameras.single();
        ui.collapsing("Camera", |ui| {
            ui.monospace(format!(
                "pos = {:.2} m\nscale = {:.2} m\n",
                tr.translation, tr.scale
            ));
        });
        ui.collapsing("UI state", |ui| {
            ui.monospace(format!("{:#?}", ui_state));
        });
        ui.collapsing("Rapier", |ui| {
            ui.monospace(format!("{:#?}", rapier));
        });
        ui.collapsing("FPS", |ui| {
            ui.monospace(format!("{:.2}", diag.get(FrameTimeDiagnosticsPlugin::FPS).unwrap().value().unwrap_or(f64::NAN)));
        });
    });
}

pub fn draw_ui() -> SystemSet {
    SystemSet::new()
        .with_system(ui_example)
        .with_system(toolbox::draw_toolbox)
        .with_system(toolbar::draw_bottom_toolbar)
        .with_system(scene_actions::draw_scene_actions)
        .with_system(scene_actions::NewSceneWindow::show)
        .with_system(process_temporary_windows)
        .with_system(remove_temporary_windows)
        .with_system(MenuWindow::show)
        .with_system(InformationWindow::show)
        .with_system(PlotWindow::show)
        .with_system(CollisionsWindow::show)
        .with_system(LaserWindow::show)
        .with_system(MaterialWindow::show)
        .with_system(AppearanceWindow::show)
        .with_system(BackgroundWindow::show)
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
pub struct InitialPos(Pos2, Pos2);

impl InitialPos {
    fn initial(pos: impl AsPos2) -> impl Bundle {
        (Self::persistent(pos), TemporaryWindow)
    }

    fn persistent(pos: impl AsPos2) -> InitialPos {
        let pos = pos.as_pos2();
        Self(pos, pos)
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
    ui: ResMut<UiState>,
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
        // todo: really detect whether window was moved
        if pos.0.distance(pos.1) > 1.0 {
            info!(
                "marking window {:?} as persistent (initial {:?} != current {:?})",
                wnd, pos.0, pos.1
            );
            commands.entity(wnd).remove::<TemporaryWindow>();
        }
    }
}

trait BevyIdThing {
    fn id_bevy(self, id: Entity) -> Self;
}

impl<'a> BevyIdThing for egui::Window<'a> {
    fn id_bevy(self, id: Entity) -> Self {
        self.id(Id::new(id))
    }
}

impl Into<Pos2> for &InitialPos {
    fn into(self) -> Pos2 {
        self.0
    }
}

trait Subwindow {
    fn subwindow(
        self,
        id: Entity,
        ctx: &Context,
        initial_pos: &mut InitialPos,
        commands: &mut Commands,
        contents: impl FnOnce(&mut Ui, &mut Commands),
    );
}

impl<'a> Subwindow for egui::Window<'a> {
    fn subwindow(
        self,
        id: Entity,
        ctx: &Context,
        initial_pos: &mut InitialPos,
        commands: &mut Commands,
        contents: impl FnOnce(&mut Ui, &mut Commands),
    ) {
        let mut open = true;
        self.id_bevy(id)
            .default_pos(&*initial_pos)
            .open(&mut open)
            .show(ctx, |ui| contents(ui, commands))
            .map(|resp| initial_pos.1 = resp.response.rect.left_top());
        if !open {
            info!("closing window");
            commands.entity(id).insert(Despawn::Recursive);
        }
    }
}

pub struct RemoveTemporaryWindowsEvent;

fn remove_temporary_windows(
    mut commands: Commands,
    mut events: EventReader<RemoveTemporaryWindowsEvent>,
    wnds: Query<Entity, With<TemporaryWindow>>,
) {
    for _ in events.iter() {
        for id in wnds.iter() {
            commands.entity(id).insert(Despawn::Recursive);
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct EntitySelection {
    pub(crate) entity: Entity,
}

#[derive(Resource, Derivative)]
#[derivative(Debug)]
pub struct UiState {
    pub(crate) selected_entity: Option<EntitySelection>,
    #[derivative(Debug = "ignore")]
    toolbox: Vec<Vec<ToolEnum>>,
    #[derivative(Debug = "ignore")]
    toolbox_bottom: Vec<ToolEnum>,
    pub toolbox_selected: ToolEnum,
    pub mouse_left: Option<ToolEnum>,
    pub mouse_left_pos: Option<(Duration, Vec2, Vec2)>,
    pub mouse_right: Option<ToolEnum>,
    pub mouse_right_pos: Option<(Duration, Vec2, Vec2)>,
    pub mouse_button: Option<UsedMouseButton>,
    pub scene: Entity,
}

impl UiState {}

impl FromWorld for UiState {
    fn from_world(_world: &mut World) -> Self {
        macro_rules! tool {
            ($ty:ident) => {
                ToolEnum::$ty(Default::default())
            };
        }

        let pan = tool!(Pan);

        Self {
            selected_entity: None,
            toolbox: vec![
                vec![tool!(Move), tool!(Drag), tool!(Rotate)],
                vec![tool!(Box), tool!(Circle)],
                vec![
                    tool!(Spring),
                    tool!(Fix),
                    tool!(Hinge),
                    tool!(Thruster),
                    tool!(Laser),
                    tool!(Tracer),
                ],
            ],
            toolbox_bottom: vec![tool!(Zoom), pan],
            toolbox_selected: pan,
            mouse_left: None,
            mouse_left_pos: None,
            mouse_right: None,
            mouse_right_pos: None,
            mouse_button: None,
            scene: _world.spawn((
                Scene,
                SpatialBundle::default()
            )).id(),
        }
    }
}
