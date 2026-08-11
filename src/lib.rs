#![deny(clippy::disallowed_methods)]

use crate::config::AppConfig;
use crate::lyon_compat::*;
use crate::mouse_tracking::{MainCamera, MainCameraEntity, prelude::*};
use crate::skin::SkinConfig;
use avian2d::prelude::*;
use bevy::anti_alias::smaa::{Smaa, SmaaPreset};
use bevy::input::InputSystems;
use bevy::math::Vec3Swizzles;
use bevy::prelude::*;
use bevy::window::{PresentMode, WindowPlugin};
use bevy_diagnostic::FrameTimeDiagnosticsPlugin;
use bevy_egui::egui::epaint::{Hsva, Shadow};
use bevy_egui::egui::style::Widgets;
use bevy_egui::egui::{Color32, CornerRadius};
use bevy_egui::{
    EguiContexts, EguiPlugin, EguiPostUpdateSet, EguiPreUpdateSet, EguiStartupSet,
    egui::{self},
};
use mouse::{button, wheel};
use objects::laser::LaserRays;
use objects::{ColorComponent, laser};
use palette::{PaletteConfig, PaletteList, PaletteLoader};
use tools::add_object::{AddObjectEvent, PlaceAttachmentEvent};
use tools::pan::PanEvent;
use tools::rotate::RotateEvent;
use tools::zoom::ZoomEvent;
use tools::{add_object, drag, r#move, pan, rotate, zoom};
use ui::cursor::ToolCursor;
use ui::selection_overlay::OverlayState;
use ui::{ContextMenuEvent, PointerToolState, SceneState, ToolboxState, cursor, selection_overlay};
use update_from::UpdateFrom;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use crate::mouse::r#move::{MouseLongOrMoved, MouseLongOrMovedWriteback};
use crate::mouse::select::{
    SelectEnclosedEvent, SelectEvent, SelectUnderMouseEvent, SelectionConfig,
};
use crate::objects::{CircleAngleMarker, SpriteOnly};
use crate::tools::ToolIcons;
use crate::tools::drag::{DragConfig, DragEvent};
use crate::tools::r#move::MoveEvent;
use crate::ui::RemoveTemporaryWindowsEvent;
use crate::ui::images::{AppIcons, GuiIcons};

mod config;
mod grid;
mod lyon_compat;
mod measures;
mod mouse;
mod mouse_tracking;
mod objects;
mod palette;
mod rng;
mod script;
mod skin;
mod tools;
mod ui;
mod update_from;

/// Standard object-outline width in physical screen pixels.
const BORDER_WIDTH_PX: f32 = 1.0;
const CAMERA_FAR: f32 = 1e6f32;
const CAMERA_Z: f32 = CAMERA_FAR - 0.1;
const FOREGROUND_Z: f32 = CAMERA_Z - 0.2;

pub(crate) fn default_camera_transform() -> Transform {
    Transform::from_xyz(0.0, 0.0, CAMERA_Z).with_scale(Vec3::new(0.01, 0.01, 1.0))
}

pub trait InvTransformPoint {
    fn to_global(&self, point: Vec2) -> Vec2;

    fn to_local(&self, point: Vec2) -> Vec2;
}

impl InvTransformPoint for GlobalTransform {
    fn to_global(&self, point: Vec2) -> Vec2 {
        self.transform_point(point.extend(0.)).xy()
    }

    fn to_local(&self, point: Vec2) -> Vec2 {
        self.affine()
            .inverse()
            .transform_point3(point.extend(0.))
            .xy()
    }
}

pub fn app_main() {
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(0.0, 0.0, 0.0)))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                present_mode: PresentMode::AutoNoVsync,
                ..Default::default()
            }),
            ..Default::default()
        }))
        .add_plugins(EguiPlugin::default())
        .add_plugins(PhysicsPlugins::default())
        .init_asset::<PaletteList>()
        .init_asset_loader::<PaletteLoader>()
        .init_resource::<PaletteConfig>()
        .init_resource::<ToolboxState>()
        .init_resource::<PointerToolState>()
        .init_resource::<SceneState>()
        .init_resource::<AppIcons>()
        .init_resource::<ToolIcons>()
        .init_resource::<GuiIcons>()
        .init_resource::<ui::image_processing::ImagePreparationState>()
        .init_resource::<SkinConfig>()
        .init_resource::<AppConfig>()
        .init_resource::<DragConfig>()
        .init_resource::<tools::gear::GearSettings>()
        .init_resource::<SelectionConfig>()
        .init_resource::<objects::air::AirSettings>()
        .init_resource::<grid::GridSettings>()
        .init_resource::<add_object::DepthSorter>()
        .init_resource::<cursor::ToolCursorCache>()
        .init_resource::<wheel::SmoothZoom>()
        .insert_resource(SubstepCount(50))
        .insert_resource(OverlayState::default())
        .insert_resource(cursor::EguiWantsFocus::default())
        .insert_resource({
            let mut loop_ = Time::<Physics>::default();
            loop_.pause();
            loop_
        })
        .add_plugins(MousePosPlugin)
        .add_plugins(ShapePlugin)
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        .add_message::<AddObjectEvent>()
        .add_message::<PlaceAttachmentEvent>()
        .add_message::<MouseLongOrMoved>()
        .add_message::<MouseLongOrMovedWriteback>()
        .add_message::<PanEvent>()
        .add_message::<ZoomEvent>()
        .add_message::<MoveEvent>()
        .add_message::<UnfreezeEntityEvent>()
        .add_message::<RotateEvent>()
        .add_message::<DragEvent>()
        .add_message::<SelectUnderMouseEvent>()
        .add_message::<SelectEnclosedEvent>()
        .add_message::<SelectEvent>()
        .add_message::<ContextMenuEvent>()
        .add_message::<RemoveTemporaryWindowsEvent>()
        .add_systems(
            PreStartup,
            setup_graphics.before(EguiStartupSet::InitContexts),
        )
        .add_systems(
            Startup,
            (configure_visuals, setup_rng).chain(),
        )
        .add_systems(
            Update,
            (update_from_palette, ui::image_processing::prepare_images),
        );
    script::add_systems(&mut app);
    ui::add_systems(&mut app);
    app.add_systems(
        PreUpdate,
        wheel::smooth_zoom
            .in_set(wheel::CameraZoomSet)
            .after(InputSystems),
    )
    .add_systems(
        PostUpdate,
        wheel::mouse_wheel.after(EguiPostUpdateSet::EndPass),
    )
    .add_systems(
        PreUpdate,
        (
            button::left_pressed,
            button::left_release.after(button::left_pressed),
            add_object::process_add_object.after(button::left_release),
            mouse::r#move::mouse_long_or_moved
                .after(button::left_pressed)
                .before(mouse::select::process_select),
            mouse::r#move::mouse_long_or_moved_writeback.after(mouse::r#move::mouse_long_or_moved),
        )
            .after(MousePositionSet)
            .after(EguiPreUpdateSet::ProcessInput),
    )
    .add_systems(
        PreUpdate,
        (
            pan::process_pan,
            zoom::process_zoom,
            r#move::process_move,
            add_object::process_place_attachment.after(r#move::process_move),
            process_unfreeze_entity,
            rotate::process_rotate,
            drag::update_drag_target,
        )
            .after(mouse::select::process_select),
    )
    .add_systems(
        PreUpdate,
        selection_overlay::process_draw_overlay
            .after(button::left_release)
            .after(MousePositionSet),
    )
    .add_systems(
        PreUpdate,
        mouse::select::process_select_under_mouse
            .after(button::left_release)
            .after(add_object::process_add_object)
            .before(mouse::select::process_select),
    )
    .add_systems(
        PreUpdate,
        mouse::select::process_select_enclosed
            .after(button::left_release)
            .after(add_object::process_add_object)
            .before(mouse::select::process_select),
    )
    .add_systems(
        PreUpdate,
        mouse::select::process_select
            .before(ui::handle_context_menu)
            .after(button::left_release),
    )
    .add_systems(
        PreUpdate,
        ui::handle_context_menu
            .after(mouse::select::process_select_under_mouse)
            .after(mouse::select::process_select),
    )
    .add_systems(
        PreUpdate,
        (ui::apply_ui_scale, cursor::check_egui_wants_focus).after(EguiPreUpdateSet::ProcessInput),
    )
    .add_systems(
        PreUpdate,
        cursor::show_current_tool_icon
            .after(wheel::mouse_wheel)
            .after(cursor::check_egui_wants_focus)
            .after(button::left_pressed)
            .after(MousePositionSet),
    )
    .add_systems(Update, update_draw_modes)
    .add_systems(
        PostUpdate,
        selection_overlay::sync_selection_highlights.before(lyon_compat::BuildShapes),
    )
    .add_systems(Update, laser::draw_lasers)
    .add_systems(Update, despawn_finished_drags)
    .add_systems(
        PhysicsSchedule,
        drag::apply_drag_force.in_set(PhysicsStepSystems::BroadPhase),
    );
    objects::add_systems(&mut app);
    app.run();
}

fn setup_rng(mut commands: Commands) {
    commands.spawn((crate::rng::RngComponent::default(),));
}

#[derive(Component)]
struct DrawObject;

fn update_draw_modes(
    mut draws: Query<(
        Entity,
        Option<&mut Fill>,
        Option<&mut Stroke>,
        &UpdateFrom<ColorComponent>,
        Option<&SpriteOnly>,
        Option<&CircleAngleMarker>,
    )>,
    parents: Query<(Option<&ChildOf>, Option<Ref<ColorComponent>>)>,
    changed_colors: Query<(), Changed<ColorComponent>>,
    changed_sources: Query<(), Changed<UpdateFrom<ColorComponent>>>,
    changed_parents: Query<(), (Changed<ChildOf>, With<UpdateFrom<ColorComponent>>)>,
    changed_sprite_only: Query<(), Changed<SpriteOnly>>,
    changed_angle_markers: Query<(), Changed<CircleAngleMarker>>,
) {
    if changed_colors.is_empty()
        && changed_sources.is_empty()
        && changed_parents.is_empty()
        && changed_sprite_only.is_empty()
        && changed_angle_markers.is_empty()
    {
        return;
    }

    for (entity, fill, stroke, update_source, sprite_only, angle_marker) in draws.iter_mut() {
        let (_entity, color) = update_source
            .find_component(entity, &parents)
            .expect("no color component found");

        let border_color = hsva_to_rgba(Hsva {
            v: color.v * 0.5,
            a: if sprite_only.is_some() { 0.0 } else { 1.0 },
            ..color
        });
        if let Some(mut fill) = fill {
            let color = if angle_marker.is_some() {
                border_color
            } else {
                hsva_to_rgba(color)
            };
            if fill.color != color {
                fill.color = color;
            }
        }
        if let Some(mut stroke) = stroke {
            if stroke.color != border_color {
                stroke.color = border_color;
            }
        }
    }
}

#[derive(Copy, Clone, Message)]
pub struct UnfreezeEntityEvent {
    entity: Entity,
}

fn process_unfreeze_entity(
    mut events: MessageReader<UnfreezeEntityEvent>,
    mut commands: Commands,
    bodies: Query<&ColliderOf>,
    scene: Res<SceneState>,
) {
    for UnfreezeEntityEvent { entity } in events.read().copied() {
        if let Ok(body) = bodies.get(entity)
            && body.body != scene.sky
        {
            commands.entity(body.body).insert(RigidBody::Dynamic);
        }
    }
}

#[derive(Component)]
pub struct UiCamera;

fn setup_graphics(mut commands: Commands) {
    info!("Setting up graphics");
    let camera = commands
        .spawn((
            Camera2d,
            MainCamera,
            Projection::Orthographic(OrthographicProjection {
                far: CAMERA_FAR,
                ..OrthographicProjection::default_2d()
            }),
        ))
        .insert(default_camera_transform())
        .insert((
            Msaa::Off,
            Smaa {
                preset: SmaaPreset::High,
            },
        ))
        .queue(InitWorldTracking)
        .id();
    commands.insert_resource(MainCameraEntity(camera));
    info!("Added main camera with {camera:?}");

    commands.spawn((
        ToolCursor,
        (
            ImageNode::default(),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(32.0),
                height: Val::Px(32.0),
                margin: UiRect::px(12.0, 0.0, 16.0, 0.0),
                ..Default::default()
            },
        ),
    ));

    commands.spawn((
        LaserRays::default(),
        Visibility::Visible,
        ViewVisibility::default(),
        Transform::default(),
    ));
}

fn hsva_to_rgba(hsva: Hsva) -> Color {
    let color = hsva.to_rgba_premultiplied();
    LinearRgba::new(color[0], color[1], color[2], color[3]).into()
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
                color: Color::srgba(0.0, 0.0, 0.0, 0.0),
                options: FillOptions::default().with_tolerance(STROKE_TOLERANCE),
            },
            stroke: Stroke {
                color: Color::srgba(0.0, 0.0, 0.0, 0.0),
                width_px: BORDER_WIDTH_PX,
                alignment: StrokeAlignment::Inward,
            },
        }
    }
}

fn make_stroke(color: Color, thickness: f32) -> Stroke {
    Stroke {
        color,
        width_px: thickness,
        alignment: StrokeAlignment::Center,
    }
}

fn make_inset_stroke(color: Color, thickness: f32) -> Stroke {
    Stroke {
        alignment: StrokeAlignment::Inward,
        ..make_stroke(color, thickness)
    }
}

const STROKE_TOLERANCE: f32 = 0.0001;

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

fn configure_visuals(mut egui_ctx: EguiContexts) -> Result {
    let ctx = egui_ctx.ctx_mut()?;
    let mut visuals = egui::Visuals {
        window_corner_radius: CornerRadius::same(3),
        window_shadow: Shadow::NONE,
        window_fill: Color32::from_rgb(134, 140, 147),
        panel_fill: Color32::from_rgb(134, 140, 147),
        override_text_color: Some(Color32::from_rgb(249, 249, 249)),
        widgets: Widgets {
            ..Default::default()
        },
        ..Default::default()
    };
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(3);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(3);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(3);
    visuals.widgets.active.corner_radius = CornerRadius::same(3);
    visuals.widgets.open.corner_radius = CornerRadius::same(3);
    ctx.set_visuals(visuals);
    ctx.global_style_mut(|s| s.spacing.slider_width = 260.0);
    Ok(())
}

fn update_from_palette(palette: Res<PaletteConfig>, mut clear_color: ResMut<ClearColor>) {
    if palette.is_changed() {
        clear_color.0 = palette.current_palette.sky_color;
    }
}

#[macro_export]
macro_rules! systems {
    (@ [$($($p:path),+$(,)*)?] [$($f:ident),*$(,)*] [$($e:ident),*$(,)*] $(,)?) => {
        $(pub mod $f;)*

        pub fn add_systems(#[allow(unused_variables)] app: &mut bevy::prelude::App) {
            $($f::add_systems(app);)*

            $(app.add_systems(bevy::prelude::Update, ($($p),*));)?

            $(app.add_message::<$e>();)*
        }
    };
    (@ [$($p:tt)*] [$($f:tt)*] [$($e:tt)*] event $system:ident $(, $($x:tt)*)?) => {
        systems!(@ [$($p)*] [$($f)*] [$system, $($e:tt)*] $($($x)*)?);
    };
    (@ [$($p:tt)*] [$($f:tt)*] [$($e:tt)*] mod $system:ident $(, $($x:tt)*)?) => {
        systems!(@ [$($p)*] [$system, $($f)*] [$($e:tt)*] $($($x)*)?);
    };
    (@ [$($p:tt)*] [$($f:tt)*] [$($e:tt)*] $first:ident $(:: $next:ident)* $(, $($x:tt)*)?) => {
        systems!(@ [$first $(:: $next)*, $($p)*] [$($f)*] [$($e:tt)*] $($($x)*)?);
    };
    (@ $($x:tt)*) => {
        compile_error!(stringify!($($x)*));
    };
    ($($x:tt)*) => {
        systems!(@ [] [] [] $($x)*);
    };
}

#[macro_export]
macro_rules! egui_systems {
    (@ [$($($p:path),+$(,)*)?] [$($f:ident),*$(,)*] [$($e:ident),*$(,)*] $(,)?) => {
        $(pub mod $f;)*

        pub fn add_systems(#[allow(unused_variables)] app: &mut bevy::prelude::App) {
            $($f::add_systems(app);)*

            $(app.add_systems(bevy_egui::EguiPrimaryContextPass, ($($p),*));)?

            $(app.add_message::<$e>();)*
        }
    };
    (@ [$($p:tt)*] [$($f:tt)*] [$($e:tt)*] event $system:ident $(, $($x:tt)*)?) => {
        egui_systems!(@ [$($p)*] [$($f)*] [$system, $($e:tt)*] $($($x)*)?);
    };
    (@ [$($p:tt)*] [$($f:tt)*] [$($e:tt)*] mod $system:ident $(, $($x:tt)*)?) => {
        egui_systems!(@ [$($p)*] [$system, $($f)*] [$($e:tt)*] $($($x)*)?);
    };
    (@ [$($p:tt)*] [$($f:tt)*] [$($e:tt)*] $first:ident $(:: $next:ident)* $(, $($x:tt)*)?) => {
        egui_systems!(@ [$first $(:: $next)*, $($p)*] [$($f)*] [$($e:tt)*] $($($x)*)?);
    };
    (@ $($x:tt)*) => {
        compile_error!(stringify!($($x)*));
    };
    ($($x:tt)*) => {
        egui_systems!(@ [] [] [] $($x)*);
    };
}
#[derive(Component)]
struct FinishedDrag;

fn despawn_finished_drags(
    drags: Query<Entity, With<FinishedDrag>>,
    mut commands: Commands,
) {
    for id in drags.iter() {
        commands.entity(id).despawn();
    }
}

#[macro_export]
macro_rules! update_changed {
    ($ui:expr, $target:expr, $range:expr, $settings:expr) => {
        update_changed!($ui, || { $target } => |x| { $target = x; }, $range, $settings)
    };
    ($ui:expr, $getter:expr => $setter:expr, $range:expr, $settings:expr) => {
        {
            use egui::{Slider, Widget};
            let mut current = $getter();
            fn update_slider<'a, T: Widget + 'a>(f: impl FnOnce(Slider<'a>) -> T, s: Slider<'a>) -> T {
                f(s)
            }
            if $ui.add(update_slider($settings, Slider::new(&mut current, $range))).changed() {
                $setter(current);
            }
        }
    };
}
