use std::time::Duration;

use bevy::log::info;
use bevy::math::{Vec2, Vec3Swizzles};
use bevy::prelude::*;
use bevy_diagnostic::{Diagnostics, DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy_egui::{egui, EguiContexts};
use bevy_egui::egui::{Context, Id, InnerResponse, pos2, Pos2, Ui};
use bevy_mouse_tracking_plugin::{MainCamera, MousePosWorld};
use bevy_rapier2d::plugin::RapierConfiguration;
use derivative::Derivative;

use windows::object::plot::PlotWindow;

use crate::{demo, Despawn, UsedMouseButton};
use crate::objects::laser::LaserRays;
use crate::palette::{PaletteConfig, PaletteList};
use crate::tools::ToolEnum;
use crate::ui::windows::{scene_actions, toolbar, toolbox};
use crate::ui::windows::object::appearance::AppearanceWindow;
use crate::ui::windows::object::hinge::HingeWindow;
use crate::ui::windows::object::laser::LaserWindow;
use crate::ui::windows::object::material::MaterialWindow;
use crate::ui::windows::scene::background::BackgroundWindow;

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
    mut egui_ctx: EguiContexts,
    ui_state: ResMut<UiState>,
    mut is_initialized: Local<bool>,
    cameras: Query<&mut Transform, With<MainCamera>>,
    mc: Query<Entity, With<MainCamera>>,
    _palette_config: ResMut<PaletteConfig>,
    _assets: Res<Assets<PaletteList>>,
    laser: Query<&LaserRays>,
    mut cmds: Commands,
    mouse: Res<MousePosWorld>,
    rapier: Res<RapierConfiguration>,
    diag: Res<DiagnosticsStore>,
) {
    if !*is_initialized {
        /*palette_config.current_palette = *assets
            .get(&palette_config.palettes)
            .unwrap()
            .0
            .get("Optics")
            .unwrap();*/

        cmds.entity(ui_state.scene)
            .with_children(|parent| {
                demo::newton_cradle::init(parent);
            });
        *is_initialized = true;
    }

    egui::Window::new("Debug").show(egui_ctx.ctx_mut(), |ui| {
        ui.collapsing("Mouse", |ui| {
            ui.label(format!("{:.2} m", mouse.xy()));
        });
        ui.collapsing("Laser", |ui| {
            ui.monospace(&laser.single().debug);
        });
        let Ok(tr) = cameras.get_single() else {
            // dump all components

            panic!("cams found={:#?}", mc.iter().count());
        };
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

pub fn add_ui_systems(app: &mut App) {
    app.add_systems(Update, (
        ui_example,
        toolbox::draw_toolbox,
        toolbar::draw_bottom_toolbar,
        scene_actions::draw_scene_actions,
        scene_actions::NewSceneWindow::show,
        process_temporary_windows,
        remove_temporary_windows
    ));
    windows::object::add_ui_systems(app);
    windows::scene::add_ui_systems(app);
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

#[derive(Event)]
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

#[derive(Event)]
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
