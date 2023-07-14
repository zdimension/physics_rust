use std::time::Duration;

use bevy::log::info;
use bevy::math::{Vec2, Vec2Swizzles, Vec3Swizzles};
use bevy::prelude::*;
use bevy_diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy_egui::egui::{pos2, Context, Id, Pos2, Ui, Align2};
use bevy_egui::{egui, EguiContexts};
use bevy_mouse_tracking_plugin::{MainCamera, MousePos, MousePosWorld};
use bevy_rapier2d::plugin::RapierConfiguration;
use derivative::Derivative;

use crate::objects::laser::LaserRays;
use crate::palette::{PaletteConfig, PaletteList};
use crate::tools::ToolEnum;
use crate::{demo, systems, Despawn, UsedMouseButton};

use self::windows::menu::MenuWindow;

pub mod cursor;
mod icon_button;
pub mod images;
mod menu_item;
pub(crate) mod selection_overlay;
mod separator_custom;
mod text_button;
mod tabs;
mod custom_widget;

systems! {
    mod windows,
    ui_example,
    process_temporary_windows,
    remove_temporary_windows,
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
    mouse_sc: Res<MousePos>,
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

        cmds.entity(ui_state.scene).with_children(|parent| {
            demo::newton_cradle::init(parent);
        });
        *is_initialized = true;
    }

    egui::Window::new("Debug").show(egui_ctx.ctx_mut(), |ui| {
        ui.collapsing("Mouse", |ui| {
            ui.label(format!("World: {:.2} m", mouse.xy()));
            ui.label(format!("Screen: {:.2} px", mouse_sc.xy()));
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
            ui.monospace(format!(
                "{:.2}",
                diag.get(FrameTimeDiagnosticsPlugin::FPS)
                    .unwrap()
                    .value()
                    .unwrap_or(f64::NAN)
            ));
        });
    });
}

trait AsPos2 {
    fn as_pos2(&self) -> Pos2;
}

impl AsPos2 for Vec2 {
    fn as_pos2(&self) -> Pos2 {
        pos2(self.x, self.y)
    }
}

impl AsPos2 for Pos2 {
    fn as_pos2(&self) -> Pos2 {
        *self
    }
}

#[derive(Component)]
pub enum InitialPos {
    Pos(Pos2, Pos2),
    ScreenCenter
}


impl InitialPos {
    fn initial(pos: impl AsPos2) -> impl Bundle {
        (Self::persistent(pos), TemporaryWindow)
    }

    fn persistent(pos: impl AsPos2) -> InitialPos {
        let pos = pos.as_pos2();
        Self::Pos(pos, pos)
    }

    /*fn update<T>(&mut self, resp: InnerResponse<T>) {
        self.1 = resp.response.rect.left_top();
    }*/
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
    existing: Query<Entity, With<MenuWindow>>
) {
    for ev in ev.iter() {
        let entity = ui.selected_entity.map(|sel| sel.entity);
        info!("context menu at {:?} for {:?}", ev.screen_pos, entity);
        if let Ok(existing) = existing.get_single() {
            commands.entity(existing).insert(Despawn::Recursive);
        }
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
        let InitialPos::Pos(begin, current) = *pos else { continue };
        if begin.distance(current) > 1.0 {
            info!(
                "marking window {:?} as persistent (initial {:?} != current {:?})",
                wnd, begin, current
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

/*impl Into<Pos2> for &InitialPos {
    fn into(self) -> Pos2 {
        self.0
    }
}*/

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
        let center = ctx.input(|i| i.screen_rect.size()) / 2.0;
        let (wnd, begin) = match initial_pos {
            InitialPos::Pos(begin, _) => {
                (self.pivot(Align2::LEFT_TOP).default_pos(*begin), *begin) // heu... du coup ça marche pas ?
            },
            InitialPos::ScreenCenter => {
                /*let input = ctx.input(|i| i.screen_rect);*/

                let zero = center.to_pos2();
                // TODO !!
                (self, zero)
            }
        };
        wnd.id_bevy(id)
            .open(&mut open)
            .show(ctx, |ui| contents(ui, commands))
            .map(|resp| { *initial_pos = InitialPos::Pos(begin, resp.response.rect.left_top()); });
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
            scene: _world.spawn((Scene, SpatialBundle::default())).id(),
        }
    }
}
