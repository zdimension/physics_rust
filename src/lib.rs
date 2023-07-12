use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use bevy_diagnostic::FrameTimeDiagnosticsPlugin;
use bevy_egui::egui::epaint::{Hsva, Shadow};
use bevy_egui::egui::Color32;
use bevy_egui::{
    egui::{self},
    EguiContexts, EguiPlugin,
};
use bevy_egui::egui::style::Widgets;
use bevy_mouse_tracking_plugin::{prelude::*, MainCamera};
use bevy_prototype_lyon::prelude::*;
use bevy_rapier2d::prelude::*;
//use bevy_prototype_lyon::prelude::{DrawMode, FillMode, ShapePlugin};
use bevy_turborand::prelude::*;
pub use egui::egui_assert;

use mouse::{button, wheel};
use objects::hinge::HingeObject;
use objects::laser::LaserRays;
use objects::{laser, ColorComponent, SettingComponent};
use palette::{PaletteConfig, PaletteList, PaletteLoader};
use tools::add_object::AddObjectEvent;
use tools::pan::PanEvent;
use tools::rotate::RotateEvent;
use tools::{add_object, pan, r#move, rotate};
use ui::cursor::ToolCursor;
use ui::selection_overlay::OverlayState;
use ui::{cursor, selection_overlay, ContextMenuEvent, EntitySelection, UiState};
use update_from::UpdateFrom;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use crate::mouse::r#move::{MouseLongOrMoved, MouseLongOrMovedWriteback};
use crate::mouse::select::{SelectEvent, SelectUnderMouseEvent};
use crate::objects::SpriteOnly;
use crate::tools::r#move::MoveEvent;
use crate::tools::ToolIcons;
use crate::ui::images::{AppIcons, GuiIcons};
use crate::ui::RemoveTemporaryWindowsEvent;

mod demo;
mod measures;
mod mouse;
mod objects;
mod palette;
mod tools;
mod ui;
mod update_from;
//mod grid;

const BORDER_THICKNESS: f32 = 0.03;
const CAMERA_FAR: f32 = 1e6f32;
const CAMERA_Z: f32 = CAMERA_FAR - 0.1;
const FOREGROUND_Z: f32 = CAMERA_Z - 0.2;

#[derive(SystemParam)]
struct CollideHooks<'w, 's> {
    query: Query<'w, 's, CollideHookData<'static>>,
}

type CollideHookData<'a> = (&'a HingeObject, &'a MultibodyJoint);

impl<'w, 's> BevyPhysicsHooks for CollideHooks<'w, 's> {
    fn filter_contact_pair(&self, context: PairFilterContextView) -> Option<SolverFlags> {
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

        let hinge_between = check_hinge_contains(&self.query, first, second)
            || check_hinge_contains(&self.query, second, first);

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
/*
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
*/
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
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::rgb(0.0, 0.0, 0.0)))
        .insert_resource(Msaa::Sample4)
        .add_plugins(DefaultPlugins)
        .add_plugins(EguiPlugin)
        .add_plugins(RngPlugin::default())
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
        .insert_resource(OverlayState::default())
        .insert_resource(cursor::EguiWantsFocus::default())
        .add_plugins(RapierPhysicsPlugin::<CollideHooks>::pixels_per_meter(1.0))
        .add_plugins(RapierDebugRenderPlugin {
            style: DebugRenderStyle {
                rigid_body_axes_length: 1.0,
                ..Default::default()
            },
            ..Default::default()
        })
        .add_plugins(MousePosPlugin)
        .add_plugins(ShapePlugin)
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
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
        .add_systems(
            Startup,
            (
                configure_visuals,
                setup_graphics,
                (setup_physics, setup_rng),
            )
                .chain(),
        )
        .add_systems(Update, update_from_palette);
    ui::add_systems(&mut app);
    measures::add_systems(&mut app);
    app.add_systems(
        Update,
        (
            wheel::mouse_wheel,
            button::left_pressed,
            button::left_release,
            add_object::process_add_object,
            mouse::r#move::mouse_long_or_moved,
            mouse::r#move::mouse_long_or_moved_writeback,
        )
            .chain(),
    )
    .add_systems(
        Update,
        (
            pan::process_pan,
            r#move::process_move,
            process_unfreeze_entity,
            rotate::process_rotate,
        ),
    )
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
    .add_system(cursor::check_egui_wants_focus)
    .add_system(
        cursor::show_current_tool_icon
            .after(wheel::mouse_wheel)
            .after(cursor::check_egui_wants_focus),
    )
    .add_system(update_draw_modes)
    .add_system(laser::draw_lasers)
    .add_systems(PostUpdate, despawn_entities);
    objects::add_update_systems(&mut app);
    app.run();
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemSet)]
//#[system_set(base)]
pub struct AfterUpdate;

fn setup_rng(mut commands: Commands, mut global_rng: ResMut<GlobalRng>) {
    commands.spawn((RngComponent::from(&mut global_rng),));
}

#[derive(Component)]
struct DrawObject;

#[derive(Component)]
pub enum Despawn {
    Single,
    Recursive,
    Descendants,
}

fn despawn_entities(entities: Query<(Entity, &Despawn)>, mut commands: Commands) {
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
    mut draws: Query<(
        Entity,
        Option<&mut Fill>,
        &mut Stroke,
        &UpdateFrom<ColorComponent>,
        Option<&SpriteOnly>,
    )>,
    parents: Query<(Option<&Parent>, Option<Ref<ColorComponent>>)>,
    ui_state: Res<UiState>,
) {
    for (entity, fill, mut stroke, update_source, sprite_only) in draws.iter_mut() {
        let (entity, color) = update_source
            .find_component(entity, &parents)
            .expect("no color component found");

        if let Some(mut fill) = fill {
            fill.color = hsva_to_rgba(color);
        }
        stroke.color = if ui_state.selected_entity == Some(EntitySelection { entity }) {
            Color::WHITE
        } else {
            hsva_to_rgba(Hsva {
                v: color.v * 0.5,
                a: if sprite_only.is_some() { 0.0 } else { 1.0 },
                ..color
            })
        };
    }
}

#[derive(Copy, Clone, Event)]
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

#[derive(Component)]
pub struct UiCamera;

fn setup_graphics(mut commands: Commands) {
    info!("Setting up graphics");
    // Add a camera so we can see the debug-render.
    // note: camera's scale means meters per pixel
    commands
        .spawn((Camera2dBundle::new_with_far(CAMERA_FAR), MainCamera))
        .insert(TransformBundle::from(
            Transform::default()
                .with_translation(Vec3::new(0.0, 0.0, CAMERA_FAR - 0.1))
                .with_scale(Vec3::new(0.01, 0.01, 1.0)),
        ))
        .add(InitWorldTracking)
        .add(|id: Entity, _world: &mut World| {
            info!("Added main camera with {id:?}");
        });

    let mut cursor_bundle = ImageBundle::default();
    cursor_bundle.style.position_type = PositionType::Absolute;
    cursor_bundle.style.width = Val::Px(32.0);
    cursor_bundle.style.height = Val::Px(32.0);
    cursor_bundle.style.margin = UiRect::px(12.0, 0.0, 16.0, 0.0);
    commands.spawn((ToolCursor, cursor_bundle));

    commands.spawn((
        LaserRays::default(),
        Visibility::Visible,
        ComputedVisibility::default(),
        TransformBundle::default(),
    ));
}

fn hsva_to_rgba(hsva: Hsva) -> Color {
    let color = hsva.to_rgba_premultiplied();
    Color::rgba_linear(color[0], color[1], color[2], color[3])
}

fn make_fill(color: Color) -> Fill {
    Fill {
        color,
        options: FillOptions::default().with_tolerance(STROKE_TOLERANCE),
    }
}

#[derive(Bundle)]
struct FillStroke {
    fill: Fill,
    stroke: Stroke,
}

impl Default for FillStroke {
    fn default() -> Self {
        Self {
            fill: Fill {
                color: Color::rgba(0.0, 0.0, 0.0, 0.0),
                options: FillOptions::default().with_tolerance(STROKE_TOLERANCE),
            },
            stroke: Stroke {
                color: Color::rgba(0.0, 0.0, 0.0, 0.0),
                options: StrokeOptions::default()
                    .with_tolerance(STROKE_TOLERANCE)
                    .with_line_width(BORDER_THICKNESS),
            },
        }
    }
}

fn make_stroke(color: Color, thickness: f32) -> Stroke {
    Stroke {
        color,
        options: StrokeOptions::default()
            .with_tolerance(STROKE_TOLERANCE)
            .with_line_width(thickness),
    }
}

const STROKE_TOLERANCE: f32 = 0.0001;

fn setup_physics(mut images: ResMut<Assets<Image>>) {
    for img in images.iter_mut() {
        print!(
            "{:?} {:?}\n",
            img.1.texture_descriptor.label, img.1.texture_descriptor.format
        );
        //img.1.texture_descriptor.format = TextureFormat::Rgba8Unorm;
    }
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

fn configure_visuals(mut egui_ctx: EguiContexts) {
    let ctx = egui_ctx.ctx_mut();
    ctx.set_visuals(egui::Visuals {
        window_rounding: 2.0.into(),
        window_shadow: Shadow {
            extrusion: 10.0,
            color: Color32::from_black_alpha(96),
        },
        window_fill: Color32::from_rgb(134, 140, 147),
        panel_fill: Color32::from_rgb(134, 140, 147),
        override_text_color: Some(Color32::from_rgb(249, 249, 249)),
        widgets: Widgets {

            ..Default::default()
        },
        ..Default::default()
    });
    let mut style: egui::Style = (*ctx.style()).clone();
    style.spacing.slider_width = 260.0;
    ctx.set_style(style);
}

fn update_from_palette(palette: Res<PaletteConfig>, mut clear_color: ResMut<ClearColor>) {
    if palette.is_changed() {
        clear_color.0 = palette.current_palette.sky_color;
    }
}

#[macro_export]
macro_rules! systems {
    (@ [$($($p:path),+$(,)*)?] [$($f:ident),*$(,)*] $(,)?) => {
        $(pub mod $f;)*

        pub fn add_systems(app: &mut bevy::prelude::App) {
            $($f::add_systems(app);)*

            $(app.add_systems(bevy::prelude::Update, ($($p),*));)?
        }
    };
    (@ [$($p:tt)*] [$($f:tt)*] mod $system:ident $(, $($x:tt)*)?) => {
        systems!(@ [$($p)*] [$system, $($f)*] $($($x)*)?);
    };
    (@ [$($p:tt)*] [$($f:tt)*] $first:ident $(:: $next:ident)* $(, $($x:tt)*)?) => {
        systems!(@ [$first $(:: $next)*, $($p)*] [$($f)*] $($($x)*)?);
    };
    ($($x:tt)*) => {
        systems!(@ [] [] $($x)*);
    };
}