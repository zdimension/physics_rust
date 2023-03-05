use crate::tools::ToolIcons;
use bevy::prelude::*;
use bevy_diagnostic::FrameTimeDiagnosticsPlugin;
use bevy_egui::egui::epaint::Hsva;
use bevy_egui::{
    egui::{self},
    EguiContext, EguiPlugin,
};
use bevy_mouse_tracking_plugin::{prelude::*, MainCamera};
use bevy_prototype_lyon::prelude::*;
use bevy_prototype_lyon::prelude::{DrawMode, FillMode, ShapePlugin};
use bevy_rapier2d::prelude::*;

mod demo;
mod measures;
mod mouse;
mod objects;
mod palette;
mod tools;
mod ui;
mod update_from;

pub use egui::egui_assert;

use crate::mouse::r#move::{MouseLongOrMoved, MouseLongOrMovedWriteback};
use crate::mouse::select::{SelectEvent, SelectUnderMouseEvent};


use crate::tools::r#move::MoveEvent;
use crate::ui::RemoveTemporaryWindowsEvent;
use bevy_turborand::{DelegatedRng, GlobalRng, RngComponent, RngPlugin};
use mouse::{button, wheel};
use objects::laser::LaserRays;
use objects::{laser, ColorComponent, SettingComponent};
use palette::{PaletteConfig, PaletteList, PaletteLoader};
use ui::cursor::ToolCursor;

use objects::hinge::HingeObject;
use tools::add_object::AddObjectEvent;
use tools::pan::PanEvent;

use tools::rotate::RotateEvent;

use crate::ui::images::{AppIcons, GuiIcons};
use tools::{add_object, pan, r#move, rotate};
use ui::selection_overlay::OverlayState;
use ui::{cursor, selection_overlay, ContextMenuEvent, EntitySelection, UiState};
use update_from::UpdateFrom;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

const BORDER_THICKNESS: f32 = 0.03;
const CAMERA_FAR: f32 = 1e6f32;
const CAMERA_Z: f32 = CAMERA_FAR - 0.1;
const FOREGROUND_Z: f32 = CAMERA_Z - 0.2;

struct CollideHooks;

type CollideHookData<'a> = (&'a HingeObject, &'a MultibodyJoint);

impl<'a> PhysicsHooksWithQuery<CollideHookData<'a>> for CollideHooks {
    fn filter_contact_pair(
        &self,
        context: PairFilterContextView,
        user_data: &Query<CollideHookData<'a>>,
    ) -> Option<SolverFlags> {
        fn check_hinge_contains(
            query: &Query<CollideHookData<'_>>,
            first: Entity,
            second: Entity,
        ) -> bool {
            let Ok((_, joint)) = query.get(first) else {
                return false;
            };

            joint.parent == second
        }

        let first = context.collider1();
        let second = context.collider2();

        let hinge_between = check_hinge_contains(user_data, first, second)
            || check_hinge_contains(user_data, second, first);

        if hinge_between {
            None
        } else {
            Some(SolverFlags::COMPUTE_IMPULSES)
        }
    }
}

mod stages {
    pub(crate) const MAIN: &str = "main";

    pub(crate) const DESPAWN: &str = "despawn";
}

struct BevyAppExtHelper<'a, L: StageLabel + Copy> {
    app: &'a mut App,
    stage: L
}

impl<'a, L: StageLabel + Copy> BevyAppExtHelper<'a, L> {
    fn add_system<Params>(&mut self, system: impl IntoSystemDescriptor<Params>) -> &mut Self {
        self.app.add_system_to_stage(self.stage, system);
        self
    }

    fn add_system_set(&mut self, system_set: SystemSet) -> &mut Self {
        self.app.add_system_set_to_stage(self.stage, system_set);
        self
    }
}

trait BevyAppExt {
    fn add_stage_after_with<S: Stage, L: StageLabel + Copy>(
        &mut self,
        target: impl StageLabel,
        label: L,
        stage: S,
        content: for<'a> fn(BevyAppExtHelper<'a, L>)
    ) -> &mut Self;
}

impl BevyAppExt for App {
    fn add_stage_after_with<S: Stage, L: StageLabel + Copy>(
        &mut self,
        target: impl StageLabel,
        label: L,
        stage: S,
        content: for<'a> fn(BevyAppExtHelper<'a, L>)
    ) -> &mut Self {
        self.add_stage_after(target, label, stage);
        content(BevyAppExtHelper {
            app: self,
            stage: label
        });
        self
    }
}

trait ToRot {
    fn to_rot(&self) -> f32;
}

impl ToRot for Quat {
    fn to_rot(&self) -> f32 {
        let ang = self.to_euler(EulerRot::XYZ);
        ang.2
    }
}

pub fn app_main() {
    App::new()
        .insert_resource(ClearColor(Color::rgb(0.0, 0.0, 0.0)))
        .insert_resource(Msaa { samples: 4 })
        .add_plugins(DefaultPlugins)
        .add_plugin(EguiPlugin)
        .add_plugin(RngPlugin::default())
        .add_asset::<PaletteList>()
        .init_asset_loader::<PaletteLoader>()
        .init_resource::<PaletteConfig>()
        .init_resource::<UiState>()
        .init_resource::<AppIcons>()
        .init_resource::<ToolIcons>()
        .init_resource::<GuiIcons>()
        .insert_resource(RapierConfiguration {
            gravity: Vect::Y * -9.81,
            physics_pipeline_active: false,
            ..Default::default()
        })
        .insert_resource(PhysicsHooksWithQueryResource(Box::new(CollideHooks)))
        .insert_resource(OverlayState::default())
        .add_plugin(RapierPhysicsPlugin::<CollideHookData>::pixels_per_meter(
            1.0,
        ))
        .add_plugin(RapierDebugRenderPlugin {
            style: DebugRenderStyle {
                rigid_body_axes_length: 1.0,
                ..Default::default()
            },
            ..Default::default()
        })
        .add_plugin(MousePosPlugin)
        .add_plugin(ShapePlugin)
        .add_plugin(FrameTimeDiagnosticsPlugin::default())
        .add_event::<AddObjectEvent>()
        .add_event::<MouseLongOrMoved>()
        .add_event::<MouseLongOrMovedWriteback>()
        .add_event::<PanEvent>()
        .add_event::<MoveEvent>()
        .add_event::<UnfreezeEntityEvent>()
        .add_event::<RotateEvent>()
        .add_event::<SelectUnderMouseEvent>()
        .add_event::<SelectEvent>()
        .add_event::<ContextMenuEvent>()
        .add_event::<RemoveTemporaryWindowsEvent>()
        .add_startup_system(configure_visuals)
        .add_startup_system(setup_graphics)
        .add_startup_system(setup_physics.after(setup_graphics))
        .add_startup_system(setup_rng)
        .add_system(update_from_palette)
        .add_system_set(ui::draw_ui())
        .add_system_set(measures::compute_measures())
        .add_system_set(
            SystemSet::new()
                .with_system(wheel::mouse_wheel)
                .with_system(button::left_pressed)
                .with_system(button::left_release)
                .with_system(add_object::process_add_object)
                .with_system(mouse::r#move::mouse_long_or_moved)
                .with_system(mouse::r#move::mouse_long_or_moved_writeback),
        )
        .add_system(pan::process_pan)
        .add_system(r#move::process_move)
        .add_system(process_unfreeze_entity)
        .add_system(rotate::process_rotate)
        .add_system(selection_overlay::process_draw_overlay.after(button::left_release))
        .add_system(mouse::select::process_select_under_mouse.before(mouse::select::process_select))
        .add_system(
            mouse::select::process_select
                .before(ui::handle_context_menu)
                .after(button::left_release),
        )
        .add_system(
            ui::handle_context_menu
                .after(mouse::select::process_select_under_mouse)
                .after(mouse::select::process_select),
        )
        .add_system(cursor::show_current_tool_icon.after(wheel::mouse_wheel))
        .add_system(objects::update_sprites_color)
        .add_system(update_draw_modes)
        .add_system(laser::draw_lasers)
        .add_system(objects::update_size_scales)
        .add_stage_after(CoreStage::Update, stages::DESPAWN, SystemStage::single(despawn_entities))
        /*.add_stage_after_with(CoreStage::Update, stages::MAIN, SystemStage::parallel(),
        |mut stage| {
            stage
                .add_system(update_from_palette)
                .add_system_set(ui::draw_ui())
                .add_system_set(measures::compute_measures())
                .add_system_set(
                    SystemSet::new()
                        .with_system(wheel::mouse_wheel)
                        .with_system(button::left_pressed)
                        .with_system(button::left_release)
                        .with_system(add_object::process_add_object)
                        .with_system(mouse::r#move::mouse_long_or_moved)
                        .with_system(mouse::r#move::mouse_long_or_moved_writeback),
                )
                .add_system(pan::process_pan)
                .add_system(r#move::process_move)
                .add_system(process_unfreeze_entity)
                .add_system(rotate::process_rotate)
                .add_system(selection_overlay::process_draw_overlay.after(button::left_release))
                .add_system(mouse::select::process_select_under_mouse.before(mouse::select::process_select))
                .add_system(
                    mouse::select::process_select
                        .before(ui::handle_context_menu)
                        .after(button::left_release),
                )
                .add_system(
                    ui::handle_context_menu
                        .after(mouse::select::process_select_under_mouse)
                        .after(mouse::select::process_select),
                )
                .add_system(cursor::show_current_tool_icon.after(wheel::mouse_wheel))
                .add_system(objects::update_sprites_color)
                .add_system(update_draw_modes)
                .add_system(laser::draw_lasers)
                .add_system(objects::update_size_scales);
        })*/
        .run();
}

fn setup_rng(mut commands: Commands, mut global_rng: ResMut<GlobalRng>) {
    commands.spawn((RngComponent::from(&mut global_rng),));
}

#[derive(Component)]
struct DrawObject;

trait DrawModeExt {
    fn get_fill_color(&self) -> Color;
    fn get_outline_color(&self) -> Color;
}

impl DrawModeExt for DrawMode {
    fn get_fill_color(&self) -> Color {
        match *self {
            DrawMode::Fill(FillMode { color, .. }) => color,
            DrawMode::Stroke(_) => Color::rgba(0.0, 0.0, 0.0, 0.0),
            DrawMode::Outlined {
                fill_mode: FillMode { color, .. },
                ..
            } => color,
        }
    }

    fn get_outline_color(&self) -> Color {
        match *self {
            DrawMode::Fill(_) => Color::rgba(0.0, 0.0, 0.0, 0.0),
            DrawMode::Stroke(StrokeMode { color, .. }) => color,
            DrawMode::Outlined {
                outline_mode: StrokeMode { color, .. },
                ..
            } => color,
        }
    }
}

#[derive(Component)]
pub enum Despawn {
    Single,
    Recursive,
    Descendants
}

fn despawn_entities(
    entities: Query<(Entity, &Despawn)>,
    mut commands: Commands,
) {
    for (entity, despawn) in entities.iter() {
        match despawn {
            Despawn::Single => {
                commands.entity(entity).despawn();
            }
            Despawn::Recursive => {
                commands.entity(entity).despawn_recursive();
            }
            Despawn::Descendants => {
                commands.entity(entity).despawn_descendants();
                commands.entity(entity).remove::<Despawn>();
            }
        }
    }
}

fn update_draw_modes(
    mut draws: Query<(Entity, &mut DrawMode, &UpdateFrom<ColorComponent>)>,
    parents: Query<(Option<&Parent>, Option<&ColorComponent>)>,
    ui_state: Res<UiState>,
) {
    for (entity, mut draw, update_source) in draws.iter_mut() {
        let (entity, color) = update_source.find_component(entity, &parents);

        *draw = match *draw {
            DrawMode::Outlined { .. } | DrawMode::Fill(_) => DrawMode::Outlined {
                fill_mode: make_fill(hsva_to_rgba(color)),
                outline_mode: {
                    let stroke = if ui_state.selected_entity == Some(EntitySelection { entity }) {
                        Color::WHITE
                    } else {
                        hsva_to_rgba(Hsva {
                            v: color.v * 0.5,
                            a: 1.0,
                            ..color
                        })
                    };
                    make_stroke(stroke, BORDER_THICKNESS)
                },
            },
            DrawMode::Stroke(_) => {
                let stroke = if ui_state.selected_entity == Some(EntitySelection { entity }) {
                    Color::WHITE
                } else {
                    Color::rgba(0.0, 0.0, 0.0, 0.0)
                };
                make_stroke(stroke, BORDER_THICKNESS).as_mode()
            }
        }
    }
}

#[derive(Copy, Clone)]
pub struct UnfreezeEntityEvent {
    entity: Entity,
}

fn process_unfreeze_entity(
    mut events: EventReader<UnfreezeEntityEvent>,
    mut query: Query<&mut RigidBody>,
) {
    for UnfreezeEntityEvent { entity } in events.iter().copied() {
        let Ok(mut body) = query.get_mut(entity) else { continue; };
        *body = RigidBody::Dynamic;
    }
}

trait AsMode {
    fn as_mode(&self) -> DrawMode;
}

impl AsMode for StrokeMode {
    fn as_mode(&self) -> DrawMode {
        DrawMode::Stroke(*self)
    }
}

impl AsMode for FillMode {
    fn as_mode(&self) -> DrawMode {
        DrawMode::Fill(*self)
    }
}

fn setup_graphics(mut commands: Commands, _egui_ctx: ResMut<EguiContext>) {
    // Add a camera so we can see the debug-render.
    // note: camera's scale means meters per pixel
    commands
        .spawn((Camera2dBundle::new_with_far(CAMERA_FAR), MainCamera))
        .insert(TransformBundle::from(
            Transform::default()
                .with_translation(Vec3::new(0.0, 0.0, CAMERA_FAR - 0.1))
                .with_scale(Vec3::new(0.01, 0.01, 1.0)),
        ))
        .add_world_tracking();

    commands.spawn((ToolCursor, SpriteBundle::default()));

    commands.spawn((
        LaserRays::default(),
        Visibility::VISIBLE,
        ComputedVisibility::default(),
        TransformBundle::default(),
    ));
}

fn hsva_to_rgba(hsva: Hsva) -> Color {
    let color = hsva.to_rgba_premultiplied();
    Color::rgba_linear(color[0], color[1], color[2], color[3])
}

fn make_fill(color: Color) -> FillMode {
    FillMode {
        color,
        options: FillOptions::default().with_tolerance(STROKE_TOLERANCE),
    }
}

fn make_stroke(color: Color, thickness: f32) -> StrokeMode {
    StrokeMode {
        color,
        options: StrokeOptions::default()
            .with_tolerance(STROKE_TOLERANCE)
            .with_line_width(thickness),
    }
}

const STROKE_TOLERANCE: f32 = 0.0001;

fn setup_physics(_commands: Commands) {
    /* Create the ground. */
    //demo::newton_cradle::init(&mut commands);
    //demo::lasers::init(&mut commands);
    /*  commands
        .spawn(Collider::cuboid(4.0, 0.5))
        .insert(TransformBundle::from(Transform::from_xyz(0.0, -3.0, 0.0)));

    let circle = PhysicalObject::ball(0.5, Vec2::new(0.0, 3.0));
    commands.spawn(circle);

    let rect1 = PhysicalObject::rect(Vec2::new(2.0, 0.5), Vec2::new(-1.0, 0.0));
    let collision_groups = CollisionGroups::new(Group::GROUP_2, Group::GROUP_3);
    let collision_groups = CollisionGroups::default();
    let rect1 = commands.spawn((rect1, collision_groups)).id();

    let rect2 = PhysicalObject::rect(Vec2::new(0.5, 2.0), Vec2::new(-0.25, -1.5));
    let mut rect2 = commands.spawn((rect2, collision_groups));

    rect2.insert((
        HingeObject,
        MultibodyJoint::new(
            rect1,
            RevoluteJointBuilder::new()
                .local_anchor1(Vec2::ZERO)
                .local_anchor2(Vec2::ZERO),
        ),
        ActiveHooks::FILTER_CONTACT_PAIRS,
    ));*/
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
fn wasm_main() {
    app_main();
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum UsedMouseButton {
    Left,
    Right,
}

impl From<UsedMouseButton> for MouseButton {
    fn from(button: UsedMouseButton) -> Self {
        match button {
            UsedMouseButton::Left => MouseButton::Left,
            UsedMouseButton::Right => MouseButton::Right,
        }
    }
}

fn configure_visuals(mut egui_ctx: ResMut<EguiContext>) {
    let ctx = egui_ctx.ctx_mut();
    ctx.set_visuals(egui::Visuals {
        window_rounding: 4.0.into(),
        ..Default::default()
    });
    let mut style: egui::Style = (*ctx.style()).clone();
    style.spacing.slider_width = 260.0;
    ctx.set_style(style);
}

fn update_from_palette(palette: Res<PaletteConfig>, mut clear_color: ResMut<ClearColor>) {
    clear_color.0 = palette.current_palette.sky_color;
}
