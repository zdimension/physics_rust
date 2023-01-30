
use std::collections::HashMap;
use std::marker::PhantomData;
use std::time::Duration;

use bevy::math::Vec3Swizzles;
use bevy::{input::mouse::MouseWheel, prelude::*};

use bevy_egui::egui::epaint::Hsva;
use bevy_egui::egui::{Id, TextureId};
use bevy_egui::{
    egui::{self},
    EguiContext, EguiPlugin,
};
use bevy_mouse_tracking_plugin::{prelude::*, MainCamera, MousePos, MousePosWorld};
use bevy_prototype_lyon::prelude::*;
use bevy_prototype_lyon::{
    entity::ShapeBundle,
    prelude::{DrawMode, FillMode, GeometryBuilder, ShapePlugin},
};
use bevy_rapier2d::prelude::*;

mod cursor;
mod demo;
mod measures;
mod mouse_select;
mod objects;
mod palette;
mod tools;
mod ui;

pub use egui::egui_assert;

use crate::palette::ToRgba;
use crate::tools::r#move::MoveEvent;
use crate::ui::RemoveTemporaryWindowsEvent;
use bevy_turborand::{DelegatedRng, GlobalRng, RngComponent, RngPlugin};
use cursor::ToolCursor;
use derivative::Derivative;
use num_traits::FloatConst;
use mouse_select::{SelectEvent, SelectUnderMouseEvent};
use objects::laser;
use objects::laser::LaserRays;
use palette::{Palette, PaletteList, PaletteLoader};
use paste::paste;
use tools::pan::PanEvent;
use tools::rotate::RotateEvent;
use tools::{pan, r#move, rotate};
use ui::{ContextMenuEvent, WindowData};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

const BORDER_THICKNESS: f32 = 0.03;
const CAMERA_FAR: f32 = 1e6f32;
const CAMERA_Z: f32 = CAMERA_FAR - 0.1;
const FOREGROUND_Z: f32 = CAMERA_Z - 0.2;

struct LoadedImage {
    bevy: Handle<Image>,
    egui: TextureId,
}

impl LoadedImage {
    fn clone(&self) -> Handle<Image> {
        self.bevy.clone()
    }
}

macro_rules! icon_set {
    ($type:ident, $root:literal, [$($name:ident),*$(,)?]) => {
        #[derive(Resource, Copy, Clone)]
        pub struct $type {
            $(
                $name: TextureId,
            )*
        }

        impl FromWorld for $type {
            fn from_world(world: &mut World) -> Self {
                let mut egui_ctx = unsafe { world.get_resource_unchecked_mut::<EguiContext>().unwrap() };
                let asset_server = world.get_resource::<AssetServer>().unwrap();
                Self {
                    $(
                        $name: {
                            let handle = asset_server.load(concat!($root, stringify!($name), ".png"));
                            let egui_id = egui_ctx.add_image(handle);
                            egui_id
                        },
                    )*
                }
            }
        }
    }
}

macro_rules! image_set {
    ($type:ident, $root:literal, [$($name:ident),*$(,)?]) => {
        #[derive(Resource)]
        pub struct $type {
            $(
                $name: LoadedImage,
            )*
        }

        impl FromWorld for $type {
            fn from_world(world: &mut World) -> Self {
                let mut egui_ctx = unsafe { world.get_resource_unchecked_mut::<EguiContext>().unwrap() };
                let asset_server = world.get_resource::<AssetServer>().unwrap();
                Self {
                    $(
                        $name: {
                            let handle = asset_server.load(concat!($root, stringify!($name), ".png"));
                            let egui_id = egui_ctx.add_image(handle.clone());
                            LoadedImage {
                                bevy: handle,
                                egui: egui_id,
                            }
                        },
                    )*
                }
            }
        }
    }
}

icon_set!(
    GuiIcons,
    "gui/",
    [
        arrow_down,
        arrow_right,
        arrow_up,
        collisions,
        color,
        controller,
        csg,
        erase,
        gravity,
        info,
        lasermenu,
        material,
        mirror,
        new,
        open,
        pause,
        play,
        plot,
        plot_clear,
        save,
        text,
        velocity,
        zoom2scene
    ]
);

image_set!(
    AppIcons,
    "app/",
    [hinge_background, hinge_balls, hinge_inner, laserpen]
);

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
                .with_system(mouse_wheel)
                .with_system(left_pressed)
                .with_system(left_release)
                .with_system(process_add_object)
                .with_system(mouse_long_or_moved)
                .with_system(mouse_long_or_moved_writeback),
        )
        .add_system(pan::process_pan)
        .add_system(r#move::process_move)
        .add_system(process_unfreeze_entity)
        .add_system(rotate::process_rotate)
        .add_system(process_draw_overlay.after(left_release))
        .add_system(mouse_select::process_select_under_mouse.before(mouse_select::process_select))
        .add_system(
            mouse_select::process_select
                .before(ui::handle_context_menu)
                .after(left_release),
        )
        .add_system(
            ui::handle_context_menu
                .after(mouse_select::process_select_under_mouse)
                .after(mouse_select::process_select),
        )
        .add_system(cursor::show_current_tool_icon.after(mouse_wheel))
        .add_system(update_sprites_color)
        .add_system(update_draw_modes)
        .add_system(laser::draw_lasers)
        .add_system(update_size_scales)
        .run();
}

fn setup_rng(mut commands: Commands, mut global_rng: ResMut<GlobalRng>) {
    commands.spawn((RngComponent::from(&mut global_rng),));
}

#[derive(Component)]
struct TemporarilyFrozen;

fn mouse_wheel(
    windows: Res<Windows>,
    mut mouse_wheel_events: EventReader<MouseWheel>,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
) {
    let prim = windows.get_primary().unwrap();
    let pos = match prim.cursor_position() {
        Some(pos) => pos,
        None => return,
    };
    let win_size = Vec2::new(prim.width(), prim.height());
    let mut transform = cameras.single_mut();

    for event in mouse_wheel_events.iter() {
        const FACTOR: f32 = 0.1;
        let factor = if event.y < 0.0 {
            1.0 + FACTOR
        } else {
            1.0 / (1.0 + FACTOR)
        };
        let off = pos - win_size / 2.0;
        let old = transform.transform_point(off.extend(1.0));
        transform.scale *= Vec3::new(factor, factor, 1.0);
        let new = transform.transform_point(off.extend(1.0));
        let diff = new - old;
        transform.translation -= diff;
    }
}

fn set_selected(mut draw_mode: Mut<DrawMode>, selected: bool) {
    *draw_mode = match *draw_mode {
        DrawMode::Outlined {
            fill_mode,
            outline_mode: _,
        } => DrawMode::Outlined {
            fill_mode,
            outline_mode: make_stroke(
                if selected { Color::WHITE } else { Color::BLACK },
                BORDER_THICKNESS,
            ),
        },
        _ => unreachable!("shouldn't happen"),
    };
}

struct MouseLongOrMovedWriteback {
    event: MouseLongOrMoved,
}

impl From<MouseLongOrMoved> for MouseLongOrMovedWriteback {
    fn from(event: MouseLongOrMoved) -> Self {
        Self { event }
    }
}

fn mouse_long_or_moved_writeback(
    mut read: EventReader<MouseLongOrMovedWriteback>,
    mut write: EventWriter<MouseLongOrMoved>,
) {
    for event in read.iter() {
        write.send(event.event);
    }
}

fn mouse_long_or_moved(
    mut events: EventReader<MouseLongOrMoved>,
    mut ev_writeback: EventWriter<MouseLongOrMovedWriteback>,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
    mut ui_state: ResMut<UiState>,
    mut query: Query<(&mut Transform, Option<&mut RigidBody>), Without<MainCamera>>,
    mut commands: Commands,
    rapier: Res<RapierContext>,
    mut select_mouse: EventWriter<SelectEvent>,
    mouse_pos: Res<MousePosWorld>,
) {
    use ToolEnum::*;
    for MouseLongOrMoved(hover_tool, pos, button) in events.iter() {
        let clickpos = *pos;
        let curpos = mouse_pos.xy();
        info!("long or moved!");

        let selected_entity = ui_state.selected_entity;

        /*let (ui_button, other_button) = match button {
            UsedMouseButton::Left => (&ui_state.mouse_left, &ui_state.mouse_right),
            UsedMouseButton::Right => (&ui_state.mouse_right, &ui_state.mouse_left)
        };

        if Some(button) == ui_state.mouse_button.as_ref() && other_button.is_some() {
            continue;
        }*/
        // todo: is this really needed?

        let ui_button = match button {
            UsedMouseButton::Left => &mut ui_state.mouse_left,
            UsedMouseButton::Right => &mut ui_state.mouse_right,
        };

        match hover_tool {
            Pan(None) => {
                info!("panning");
                *ui_button = Some(Pan(Some(PanState {
                    orig_camera_pos: cameras.single_mut().translation.xy(),
                })));
            }
            Zoom(None) => {
                todo!()
            }
            _ => {
                let under_mouse = mouse_select::find_under_mouse(
                    &rapier,
                    clickpos,
                    QueryFilter::default(),
                    |ent| {
                        let (transform, _) = query.get(ent).unwrap();
                        transform.translation.z
                    },
                )
                .next();

                if matches!(
                    hover_tool,
                    Move(None) | Rotate(None) | Fix(()) | Hinge(()) | Tracer(())
                ) {
                    select_mouse.send(SelectEvent {
                        entity: under_mouse,
                        open_menu: false,
                    });
                }

                match (hover_tool, under_mouse, selected_entity.map(|s| s.entity)) {
                    (Spring(None), _, _) => todo!(),
                    (Drag(None), Some(ent), _) => {
                        *ui_button = Some(Drag(Some(DragState {
                            entity: ent,
                            orig_obj_pos: curpos - query.get_mut(ent).unwrap().0.translation.xy(),
                        })));
                    }
                    (Rotate(None), Some(under), _) => {
                        let (transform, body) = query.get_mut(under).unwrap();
                        info!("start rotate {:?}", under);
                        *ui_button = Some(Rotate(Some(RotateState {
                            orig_obj_rot: transform.rotation,
                            overlay_ent: commands.spawn(DrawObject).id(),
                            scale: cameras.single_mut().scale.x
                        })));
                        if let Some(mut body) = body {
                            *body = RigidBody::Fixed;
                        }
                    }
                    (Rotate(None) | Move(None), None, _) => {
                        ev_writeback.send(MouseLongOrMoved(Pan(None), clickpos, *button).into());
                    }
                    (_, Some(under), Some(sel)) if under == sel => {
                        let (transform, body) = query.get_mut(under).unwrap();
                        *ui_button = Some(Move(Some(MoveState {
                            obj_delta: transform.translation.xy() - curpos,
                        })));
                        if let Some(mut body) = body {
                            *body = RigidBody::KinematicPositionBased;
                        }
                    }
                    (Box(None), _, _) => {
                        *ui_button = Some(Box(Some(commands.spawn(DrawObject).id())));
                    }
                    (Circle(None), _, _) => {
                        *ui_button = Some(Circle(Some(commands.spawn(DrawObject).id())));
                    }
                    (tool, _, _) => {
                        dbg!(tool);
                        //todo!()
                    }
                }
            }
        }
    }
}

#[derive(Component)]
struct DrawObject;

fn left_release(
    mouse_button_input: Res<Input<MouseButton>>,
    mut commands: Commands,
    screen_pos: Res<MousePos>,
    mut ui_state: ResMut<UiState>,
    mouse_pos: Res<MousePosWorld>,
    mut add_obj: EventWriter<AddObjectEvent>,
    mut unfreeze: EventWriter<UnfreezeEntityEvent>,
    mut select_mouse: EventWriter<SelectUnderMouseEvent>,
    _context_menu: EventWriter<ContextMenuEvent>,
    mut overlay: ResMut<OverlayState>,
    _windows: Res<Windows>,
) {
    use ToolEnum::*;
    let screen_pos = **screen_pos;
    let pos = mouse_pos.xy();

    macro_rules! process_button {
        ($button: expr, $state_pos:expr, $state_button:expr, $click_act:expr) => {
            'thing: {
                let pressed = mouse_button_input.pressed($button.into());
                if pressed {
                    break 'thing;
                }
                let Some((_at, click_pos, click_pos_screen)) = $state_pos else { break 'thing; };
                let selected = std::mem::replace(&mut $state_button, None);
                info!("resetting state");
                $state_pos = None;
                let Some(tool) = selected else { break 'thing };
                // remove selection overlays
                if ui_state.mouse_button == Some($button) {
                    ui_state.mouse_button = None;
                }
                *overlay = OverlayState { draw_ent: None };
                match tool {
                    Box(Some(ent)) => {
                        commands.entity(ent).despawn();
                    }
                    Circle(Some(ent)) => {
                        commands.entity(ent).despawn();
                    }
                    Rotate(Some(state)) => {
                        commands.entity(state.overlay_ent).despawn();
                    }
                    _ => {}
                }
                match tool {
                    Move(Some(_)) | Rotate(Some(_)) => {
                        if let Some(EntitySelection { entity }) = ui_state.selected_entity {
                            unfreeze.send(UnfreezeEntityEvent { entity });
                        }
                    }
                    Box(Some(_ent)) if screen_pos.distance(click_pos_screen) > 6.0 => {
                        add_obj.send(AddObjectEvent::Box {
                            pos: click_pos,
                            size: pos - click_pos,
                        });
                    }
                    Circle(Some(_ent)) if screen_pos.distance(click_pos_screen) > 6.0 => {
                        add_obj.send(AddObjectEvent::Circle {
                            center: click_pos,
                            radius: (pos - click_pos).length(),
                        });
                    }
                    Spring(Some(_)) => {
                        todo!()
                    }
                    Thruster(_) => {
                        todo!()
                    }
                    Fix(()) => {
                        add_obj.send(AddObjectEvent::Fix(pos));
                    }
                    Hinge(()) => {
                        add_obj.send(AddObjectEvent::Hinge(pos));
                    }
                    Laser(()) => {
                        add_obj.send(AddObjectEvent::Laser(pos));
                    }
                    Tracer(()) => {
                        todo!()
                    }
                    Pan(Some(_)) | Zoom(Some(_)) | Drag(Some(_)) => {
                        //
                    }
                    _ => $click_act,
                }
            }
        };
    }

    process_button!(
        UsedMouseButton::Left,
        ui_state.mouse_left_pos,
        ui_state.mouse_left,
        {
            info!("selecting under mouse");
            select_mouse.send(SelectUnderMouseEvent {
                pos,
                open_menu: false,
            });
        }
    );
    process_button!(
        UsedMouseButton::Right,
        ui_state.mouse_right_pos,
        ui_state.mouse_right,
        {
            info!("selecting under mouse");
            select_mouse.send(SelectUnderMouseEvent {
                pos,
                open_menu: true,
            });
        }
    );
}

#[derive(Debug)]
enum AddObjectEvent {
    Hinge(Vec2),
    Fix(Vec2),
    Circle { center: Vec2, radius: f32 },
    Box { pos: Vec2, size: Vec2 },
    Laser(Vec2),
    Polygon { pos: Vec2, points: Vec<Vec2> },
}

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

#[derive(Default)]
struct DepthSorter {
    current_depth: f32,
}

impl DepthSorter {
    fn next(&mut self) -> f32 {
        self.current_depth += 1.0;
        self.current_depth
    }

    fn pos(&mut self, pos: Vec2) -> Vec3 {
        pos.extend(self.next())
    }
}
const DEFAULT_OBJ_SIZE: f32 = 66.0;
fn process_add_object(
    mut events: EventReader<AddObjectEvent>,
    rapier: Res<RapierContext>,
    mut query: Query<(&mut Transform, &mut RigidBody), Without<MainCamera>>,
    images: Res<AppIcons>,
    mut commands: Commands,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
    _draw_mode: Query<&DrawMode>,
    palette_config: Res<PaletteConfig>,
    mut z: Local<DepthSorter>,
    mut rng: Query<&mut RngComponent>,
    mut select_mouse: EventWriter<SelectUnderMouseEvent>,
    sensor: Query<&Sensor>,
) {
    let palette = &palette_config.current_palette;
    use objects::laser::LaserBundle;
    use AddObjectEvent::*;
    for ev in events.iter() {
        match *ev {
            Box {
                pos,
                size,
            } => {
                commands
                    .spawn(PhysicalObject::rect(size, z.pos(pos)))
                    .insert(
                        ColorComponent(palette.get_color_hsva(&mut *rng.single_mut()))
                            .update_from_this(),
                    )
                    .log_components();
            }
            Circle {
                center,
                radius,
            } => {
                commands
                    .spawn(PhysicalObject::ball(radius, z.pos(center)))
                    .insert(
                        ColorComponent(palette.get_color_hsva(&mut *rng.single_mut()))
                            .update_from_this(),
                    )
                    .log_components();
            }
            Polygon { pos, ref points } => {
                commands
                    .spawn(PhysicalObject::poly(points.clone(), z.pos(pos)))
                    .insert(
                        ColorComponent(palette.get_color_hsva(&mut *rng.single_mut()))
                            .update_from_this(),
                    )
                    .log_components();
            }
            Fix(pos) => {
                let (entity1, entity2) = {
                    let mut entities = mouse_select::find_under_mouse(
                        &rapier,
                        pos,
                        QueryFilter::only_dynamic(),
                        |ent| {
                            let (transform, _) = query.get(ent).unwrap();
                            transform.translation.z
                        },
                    );
                    (entities.next(), entities.next())
                };

                if let Some(entity1) = entity1 {
                    if sensor.get(entity1).is_ok() {
                        select_mouse.send(SelectUnderMouseEvent {
                            pos,
                            open_menu: false,
                        });
                        return;
                    }

                    let (transform, _) = query.get_mut(entity1).unwrap();
                    let anchor1 = transform
                        .compute_affine()
                        .inverse()
                        .transform_point3(pos.extend(0.0))
                        .xy();

                    if let Some(entity2) = entity2 {
                        let (transform, _) = query.get_mut(entity2).unwrap();
                        let anchor2 = transform
                            .compute_affine()
                            .inverse()
                            .transform_point3(pos.extend(0.0))
                            .xy();
                        commands.entity(entity2).insert(MultibodyJoint::new(
                            entity1,
                            FixedJointBuilder::new()
                                .local_anchor1(anchor1)
                                .local_anchor2(anchor2),
                        ));
                    } else {
                        commands.spawn((
                            ImpulseJoint::new(
                                entity1,
                                FixedJointBuilder::new()
                                    .local_anchor1(anchor1)
                                    .local_anchor2(pos),
                            ),
                            RigidBody::Dynamic,
                        ));
                    }
                }
            }
            Hinge(pos) => {
                let (entity1, entity2) = {
                    let mut entities = mouse_select::find_under_mouse(
                        &rapier,
                        pos,
                        QueryFilter::only_dynamic(),
                        |ent| {
                            let (transform, _) = query.get(ent).unwrap();
                            transform.translation.z
                        },
                    );
                    (entities.next(), entities.next())
                };

                if let Some(entity1) = entity1 {
                    if sensor.get(entity1).is_ok() {
                        select_mouse.send(SelectUnderMouseEvent {
                            pos,
                            open_menu: false,
                        });
                        return;
                    }

                    let Ok((transform, _)) = query.get_mut(entity1) else {
                        commands.entity(entity1).log_components();
                        continue;
                    };
                    let anchor1 = transform
                        .compute_affine()
                        .inverse()
                        .transform_point3(pos.extend(0.0))
                        .xy();
                    let hinge_z = z.next();
                    let hinge_delta = hinge_z - transform.translation.z;
                    let hinge_pos = anchor1.extend(hinge_delta);
                    if let Some(entity2) = entity2 {
                        let (transform, _) = query.get_mut(entity2).unwrap();
                        let anchor2 = transform
                            .compute_affine()
                            .inverse()
                            .transform_point3(pos.extend(0.0))
                            .xy();
                        info!(
                            "hinge: {:?} {:?} {:?} {:?}",
                            entity1, anchor1, entity2, anchor2
                        );
                        commands.entity(entity2).insert((
                            HingeObject,
                            MultibodyJoint::new(
                                entity1,
                                RevoluteJointBuilder::new()
                                    .local_anchor1(anchor1)
                                    .local_anchor2(anchor2),
                            ),
                            ActiveHooks::FILTER_CONTACT_PAIRS,
                        ));
                    } else {
                        commands.spawn((
                            ImpulseJoint::new(
                                entity1,
                                RevoluteJointBuilder::new()
                                    .local_anchor1(anchor1)
                                    .local_anchor2(pos),
                            ),
                            RigidBody::Dynamic,
                        ));
                    }

                    const HINGE_RADIUS: f32 = DEFAULT_OBJ_SIZE / 2.0;
                    let scale = cameras.single_mut().scale.x * DEFAULT_OBJ_SIZE;
                    const IMAGE_SCALE: f32 = 1.0 / 256.0;
                    const IMAGE_SCALE_VEC: Vec3 = Vec3::new(IMAGE_SCALE, IMAGE_SCALE, 1.0);
                    // group the three sprites in an entity containing the transform
                    commands.entity(entity1).add_children(|builder| {
                        builder
                            .spawn((
                                GeometryBuilder::build_as(
                                    &shapes::Circle {
                                        radius: 0.5 * 1.1, // make selection display a bit bigger
                                        ..Default::default()
                                    },
                                    make_stroke(Color::rgba(0.0, 0.0, 0.0, 0.0), BORDER_THICKNESS)
                                        .as_mode(),
                                    Transform::from_translation(hinge_pos)
                                        .with_scale(Vec3::new(scale, scale, 1.0)),
                                ),
                                Collider::ball(0.5),
                                Sensor,
                                ColorComponent(palette.get_color_hsva_opaque(&mut *rng.single_mut()))
                                    .update_from_this(),
                            ))
                            .add_children(|builder| {
                                builder
                                    .spawn(SpatialBundle::from_transform(Transform::from_scale(
                                        IMAGE_SCALE_VEC,
                                    )))
                                    .with_children(|builder| {
                                        builder.spawn((
                                            SpriteBundle {
                                                texture: images.hinge_balls.clone(),
                                                sprite: Sprite {
                                                    ..Default::default()
                                                },
                                                ..Default::default()
                                            },
                                            UpdateFrom::<ColorComponent>::entity(entity1),
                                        ));
                                    })
                                    .with_children(|builder| {
                                        builder.spawn((
                                            SpriteBundle {
                                                texture: images.hinge_background.clone(),
                                                sprite: Sprite {
                                                    ..Default::default()
                                                },
                                                ..Default::default()
                                            },
                                            UpdateFrom::<ColorComponent>::This,
                                        ));
                                    })
                                    .with_children(|builder| {
                                        let mut sprite = builder.spawn(SpriteBundle {
                                            texture: images.hinge_inner.clone(),
                                            sprite: Sprite {
                                                color: palette.sky_color,
                                                ..Default::default()
                                            },
                                            ..Default::default()
                                        });
                                        if let Some(entity2) = entity2 {
                                            sprite.insert(UpdateFrom::<ColorComponent>::entity(
                                                entity2,
                                            ));
                                        }
                                    });
                            });
                    });
                }
            }
            Laser(pos) => {
                let entity = mouse_select::find_under_mouse(
                    &rapier,
                    pos,
                    QueryFilter::only_dynamic(),
                    |ent| query.get(ent).unwrap().0.translation.z,
                )
                .next();

                let scale = cameras.single_mut().scale.x * DEFAULT_OBJ_SIZE;
                let laser = commands
                    .spawn((
                        LaserBundle {
                            fade_distance: 10.0,
                        },
                        ColorComponent(palette.get_color_hsva_opaque(&mut *rng.single_mut()))
                            .update_from_this(),
                        Collider::cuboid(0.5, 0.25),
                        SizeComponent(scale),
                        Sensor,
                    ))
                    .id();

                let laser_pos = if let Some(entity) = entity {
                    commands.entity(entity).add_child(laser);
                    pos - query.get(entity).unwrap().0.translation.xy()
                } else {
                    pos
                };
                commands
                    .entity(laser)
                    .insert((
                        GeometryBuilder::build_as(
                            &shapes::Rectangle {
                                extents: Vec2::new(1.0, 0.5) * 1.1, // make selection display a bit bigger
                                ..Default::default()
                            },
                            make_stroke(Color::rgba(0.0, 0.0, 0.0, 0.0), BORDER_THICKNESS)
                                .as_mode(),
                            Transform::from_translation(z.pos(laser_pos)),
                        ),
                        UpdateFrom::<SizeComponent>::This,
                    ))
                    .with_children(|builder| {
                        builder.spawn((
                            SpriteBundle {
                                texture: images.laserpen.clone(),
                                transform: Transform::from_scale(Vec3::new(
                                    1.0 / 256.0,
                                    1.0 / 256.0,
                                    1.0,
                                )),
                                ..Default::default()
                            },
                            UpdateFrom::<ColorComponent>::This,
                        ));
                    });
            }
            ref x @ _ => unimplemented!("unimplemented tool {:?}", x),
        }
    }
}

#[derive(Component)]
enum UpdateFrom<T: SettingComponent> {
    This,
    Entity(Entity, PhantomData<T>),
}

impl<T: SettingComponent> UpdateFrom<T> {
    fn entity(ent: Entity) -> Self {
        UpdateFrom::Entity(ent, PhantomData)
    }

    fn find_component(
        &self,
        base: Entity,
        parents: &Query<(Option<&Parent>, Option<&T>)>,
    ) -> (Entity, T::Value) {
        let mut root = match self {
            UpdateFrom::This => base,
            UpdateFrom::Entity(e, _) => *e,
        };
        loop {
            let (p, col) = parents.get(root).unwrap();
            if let Some(col) = col {
                return (root, col.get());
            }
            root = p.expect("No parent").get();
        }
    }
}

fn update_sprites_color(
    mut sprites: Query<(Entity, &mut Sprite, &UpdateFrom<ColorComponent>)>,
    parents: Query<(Option<&Parent>, Option<&ColorComponent>)>,
) {
    for (entity, mut sprite, update_source) in sprites.iter_mut() {
        sprite.color = update_source.find_component(entity, &parents).1.to_rgba();
    }
}

// set scale to (size, size, 1)
fn update_size_scales(
    mut scales: Query<(Entity, &mut Transform, &UpdateFrom<SizeComponent>)>,
    parents: Query<(Option<&Parent>, Option<&SizeComponent>)>,
) {
    for (entity, mut scale, update_source) in scales.iter_mut() {
        let (_, size) = update_source.find_component(entity, &parents);
        scale.scale = Vec3::new(size, size, 1.0);
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

trait SettingComponent: Component + Sized {
    type Value;

    fn get(&self) -> Self::Value;

    fn update_from_this(self) -> (Self, UpdateFrom<Self>) {
        (self, UpdateFrom::<Self>::This)
    }

    fn update_from_entity(self, entity: Entity) -> (Self, UpdateFrom<Self>) {
        (self, UpdateFrom::<Self>::Entity(entity, PhantomData))
    }
}

#[derive(Component)]
pub struct ColorComponent(Hsva);

impl SettingComponent for ColorComponent {
    type Value = Hsva;

    fn get(&self) -> Hsva {
        self.0
    }
}

#[derive(Component)]
pub struct SizeComponent(f32);

impl SettingComponent for SizeComponent {
    type Value = f32;

    fn get(&self) -> f32 {
        self.0
    }
}

#[derive(Component)]
pub struct RefractiveIndex(f32);

impl Default for RefractiveIndex {
    fn default() -> Self {
        RefractiveIndex(1.5)
    }
}

#[derive(Bundle)]
struct HingeBundle {}

#[derive(Copy, Clone)]
struct MouseLongOrMoved(ToolEnum, Vec2, UsedMouseButton);

#[derive(Copy, Clone)]
struct UnfreezeEntityEvent {
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

#[derive(Copy, Clone)]
enum Overlay {
    Rectangle(Vec2),
    Circle(f32),
    Rotate(f32, f32)
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

#[derive(Resource, Default)]
struct OverlayState {
    draw_ent: Option<(Entity, Overlay, Vec2)>,
}

const ROTATE_HELPER_RADIUS: f32 = 136.0;
const ROTATE_HELPER_ROUND_TO: f32 = 15.0f32 * std::f32::consts::PI / 180.0;

fn process_draw_overlay(
    cameras: Query<&mut Transform, With<MainCamera>>,
    overlay: ResMut<OverlayState>,
    mut commands: Commands,
) {
    if let Some((draw_ent, shape, pos)) = overlay.draw_ent {
        let camera = cameras.single();
        let builder = GeometryBuilder::new();
        let (thickness, color, builder) = match shape {
            Overlay::Rectangle(size) => (5.0, Color::WHITE, builder.add(&shapes::Rectangle {
                extents: size,
                origin: RectangleOrigin::BottomLeft,
            })),
            Overlay::Circle(radius) => (5.0, Color::WHITE, builder.add(&shapes::Circle {
                radius,
                ..Default::default()
            })),
            Overlay::Rotate(rot_value, scale) => (3.0, Color::rgba(1.0, 1.0, 1.0, 0.4), builder.add(&shapes::Circle {
                radius: scale * ROTATE_HELPER_RADIUS,
                ..Default::default()
            }))
        };
        // todo: rotate helper 2
        commands.entity(draw_ent).insert(builder.build(
            make_stroke(color, thickness * camera.scale.x).as_mode(),
            Transform::from_translation(pos.extend(FOREGROUND_Z)),
        ));
    }
}

fn left_pressed(
    mouse_button_input: Res<Input<MouseButton>>,
    mut ui_state: ResMut<UiState>,
    mouse_pos: Res<MousePosWorld>,
    screen_pos: Res<MousePos>,
    mut egui_ctx: ResMut<EguiContext>,
    mut ev_long_or_moved: EventWriter<MouseLongOrMoved>,
    mut ev_pan: EventWriter<PanEvent>,
    mut ev_move: EventWriter<MoveEvent>,
    mut ev_rotate: EventWriter<RotateEvent>,
    mut overlay: ResMut<OverlayState>,
    time: Res<Time>,
    xform: Query<&Transform>
) {
    let screen_pos = **screen_pos;

    use ToolEnum::*;

    enum HandleStatus {
        Handled,
        HandledAndStop,
        NotHandled,
    }

    let pos = mouse_pos.xy();

    macro_rules! process_button {
        ($button:expr, $tool:expr, $state_pos:expr, $state_button:expr) => {
            'thing: {
                let button = $button;
                let tool = $tool;
                let pressed = mouse_button_input.pressed(button.into());

                if !pressed {
                    break 'thing;
                }
                if let Some((at, click_pos, click_pos_screen)) = $state_pos {
                    match $state_button {
                        Some(Pan(Some(PanState { orig_camera_pos }))) => {
                            ev_pan.send(PanEvent {
                                orig_camera_pos,
                                delta: click_pos_screen - screen_pos,
                            });
                        }
                        Some(Move(Some(state))) => {
                            if let Some(EntitySelection { entity }) = ui_state.selected_entity {
                                ev_move.send(MoveEvent {
                                    entity,
                                    pos: pos + state.obj_delta,
                                });
                            } else {
                                $state_pos = None;
                                $state_button = None;
                            }
                        }
                        Some(Rotate(Some(state))) => {
                            if let Some(EntitySelection { entity }) = ui_state.selected_entity {
                                ev_rotate.send(RotateEvent {
                                    entity,
                                    orig_obj_rot: state.orig_obj_rot,
                                    click_pos,
                                    mouse_pos: pos,
                                });
                                let xf = xform.get(entity).unwrap();
                                *overlay = OverlayState {
                                    draw_ent: Some((
                                        state.overlay_ent,
                                        Overlay::Rotate(2.0 * xf.rotation.z.asin(), state.scale),
                                        xf.translation.xy(),
                                    )),
                                };
                            } else {
                                $state_pos = None;
                                $state_button = None;
                            }
                        }
                        Some(Box(Some(draw_ent))) => {
                            *overlay = OverlayState {
                                draw_ent: Some((
                                    draw_ent,
                                    Overlay::Rectangle(pos - click_pos),
                                    click_pos,
                                )),
                            };
                        }
                        Some(Circle(Some(draw_ent))) => {
                            *overlay = OverlayState {
                                draw_ent: Some((
                                    draw_ent,
                                    Overlay::Circle((pos - click_pos).length()),
                                    click_pos,
                                )),
                            };
                        }
                        _ => {
                            let long_press = time.elapsed() - at > Duration::from_millis(200);
                            let moved = (click_pos - pos).length() > 0.0;
                            let long_or_moved = long_press || moved;
                            if long_or_moved {
                                info!("sending long/moved (button was {:?})", $state_button);
                                ev_long_or_moved.send(MouseLongOrMoved(tool, click_pos, $button));
                            }
                        }
                    }
                } else if mouse_button_input.just_pressed(button.into())
                    && !egui_ctx.ctx_mut().is_using_pointer()
                    && !egui_ctx.ctx_mut().is_pointer_over_area()
                {
                    info!("button pressed ({:?})", button);
                    if let Some(id) = ui_state.window_temp {
                        ui_state.windows.remove(&id);
                        ui_state.window_temp = None;
                    }
                    $state_button = Some(tool);
                    $state_pos = Some((time.elapsed(), pos, screen_pos));
                    if ui_state.mouse_button == None {
                        ui_state.mouse_button = Some(button);
                    }
                }
            }
        };
    }

    process_button!(
        UsedMouseButton::Left,
        match ui_state.mouse_right {
            Some(_x) => Pan(None),
            None => ui_state.toolbox_selected,
        },
        ui_state.mouse_left_pos,
        ui_state.mouse_left
    );
    process_button!(
        UsedMouseButton::Right,
        match ui_state.mouse_left {
            Some(_x) => Pan(None),
            None => Rotate(None),
        },
        ui_state.mouse_right_pos,
        ui_state.mouse_right
    );
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

#[derive(Bundle)]
struct PhysicalObject {
    rigid_body: RigidBody,
    velocity: Velocity,
    collider: Collider,
    friction: Friction,
    restitution: Restitution,
    mass_props: ColliderMassProperties,
    shape: ShapeBundle,
    read_props: ReadMassProperties,
    groups: CollisionGroups,
    refractive_index: RefractiveIndex,
    color: ColorComponent,
    color_upd: UpdateFrom<ColorComponent>,
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

impl PhysicalObject {
    pub fn make(collider: Collider, shape: ShapeBundle) -> Self {
        Self {
            rigid_body: RigidBody::Dynamic,
            velocity: Velocity::default(),
            collider,
            friction: Friction::default(),
            restitution: Restitution::coefficient(0.7),
            mass_props: ColliderMassProperties::Density(1.0),
            shape,
            read_props: ReadMassProperties::default(),
            groups: CollisionGroups::new(Group::GROUP_1, Group::GROUP_1),
            refractive_index: RefractiveIndex::default(),
            color: ColorComponent(Hsva::new(0.0, 1.0, 1.0, 1.0)),
            color_upd: UpdateFrom::This,
        }
    }

    pub fn ball(radius: f32, pos: Vec3) -> Self {
        let radius = radius.abs();
        Self::make(
            Collider::ball(radius),
            GeometryBuilder::build_as(
                &shapes::Circle {
                    radius,
                    ..Default::default()
                },
                DrawMode::Outlined {
                    fill_mode: make_fill(Color::CYAN),
                    outline_mode: make_stroke(Color::BLACK, BORDER_THICKNESS),
                },
                Transform::from_translation(pos),
            ),
        )
    }

    pub fn rect(mut size: Vec2, mut pos: Vec3) -> Self {
        if size.x < 0.0 {
            pos.x += size.x;
            size.x = -size.x;
        }
        if size.y < 0.0 {
            pos.y += size.y;
            size.y = -size.y;
        }
        Self::make(
            Collider::cuboid(size.x / 2.0, size.y / 2.0),
            GeometryBuilder::build_as(
                &shapes::Rectangle {
                    extents: size,
                    origin: RectangleOrigin::Center,
                },
                DrawMode::Outlined {
                    fill_mode: make_fill(Color::CYAN),
                    outline_mode: make_stroke(Color::BLACK, BORDER_THICKNESS),
                },
                Transform::from_translation(pos + (size / 2.0).extend(0.0)),
            ),
        )
    }

    pub fn poly(points: Vec<Vec2>, pos: Vec3) -> Self {
        Self::make(
            Collider::convex_hull(&points).unwrap(),
            GeometryBuilder::build_as(
                &shapes::Polygon {
                    points,
                    closed: true,
                },
                DrawMode::Outlined {
                    fill_mode: make_fill(Color::CYAN),
                    outline_mode: make_stroke(Color::BLACK, BORDER_THICKNESS),
                },
                Transform::from_translation(pos),
            ),
        )
    }
}

#[derive(Component)]
struct HingeObject;

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

macro_rules! tools_enum {
    ($($pic:ident => $name:ident($data:ty)),*$(,)?) => {
        #[derive(Debug, Copy, Clone)]
        enum ToolEnum {
            $($name($data)),*
        }

        paste! {
            #[derive(Resource)]
            pub struct ToolIcons {
                $(
                    [<icon_ $pic>]: Handle<Image>
                ),*
            }

            impl FromWorld for ToolIcons {
                fn from_world(world: &mut World) -> Self {
                    let asset_server = world.get_resource_mut::<AssetServer>().unwrap();
                    Self {
                        $(
                            [<icon_ $pic>]: asset_server.load(concat!("tools/", stringify!($pic), ".png"))
                        ),*
                    }
                }
            }

            impl ToolEnum {
                fn icon(&self, icons: impl AsRef<ToolIcons>) -> Handle<Image> {
                    let icons = icons.as_ref();
                    match self {
                        $(
                            Self::$name(_) => icons.[<icon_ $pic>].clone()
                        ),*
                    }
                }
            }
        }
    }
}

tools_enum! {
    move => Move(Option<MoveState>),
    drag => Drag(Option<DragState>),
    rotate => Rotate(Option<RotateState>),
    box => Box(Option<Entity>),
    circle => Circle(Option<Entity>),
    spring => Spring(Option<()>),
    thruster => Thruster(Option<()>),
    fixjoint => Fix(()),
    hinge => Hinge(()),
    laserpen => Laser(()),
    tracer => Tracer(()),
    pan => Pan(Option<PanState>),
    zoom => Zoom(Option<()>),
}

impl ToolEnum {
    fn is_same(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

#[derive(Copy, Clone, Debug)]
struct PanState {
    orig_camera_pos: Vec2,
}

#[derive(Copy, Clone, Debug)]
struct DragState {
    entity: Entity,
    orig_obj_pos: Vec2,
}

#[derive(Copy, Clone, Debug)]
struct MoveState {
    obj_delta: Vec2,
}

#[derive(Copy, Clone, Debug)]
struct RotateState {
    orig_obj_rot: Quat,
    overlay_ent: Entity,
    scale: f32
}

#[derive(Copy, Clone, PartialEq, Debug)]
struct EntitySelection {
    entity: Entity,
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum UsedMouseButton {
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

#[derive(Resource, Derivative)]
#[derivative(Debug)]
pub struct UiState {
    selected_entity: Option<EntitySelection>,
    #[derivative(Debug = "ignore")]
    toolbox: Vec<Vec<ToolEnum>>,
    #[derivative(Debug = "ignore")]
    toolbox_bottom: Vec<ToolEnum>,
    toolbox_selected: ToolEnum,
    mouse_left: Option<ToolEnum>,
    mouse_left_pos: Option<(Duration, Vec2, Vec2)>,
    mouse_right: Option<ToolEnum>,
    mouse_right_pos: Option<(Duration, Vec2, Vec2)>,
    mouse_button: Option<UsedMouseButton>,
    windows: HashMap<Id, WindowData>,
    window_temp: Option<Id>,
}

#[derive(Resource)]
pub struct PaletteConfig {
    pub palettes: Handle<PaletteList>,
    pub current_palette: Palette,
}

impl FromWorld for PaletteConfig {
    fn from_world(world: &mut World) -> Self {
        let asset_server = world.get_resource_mut::<AssetServer>().unwrap();
        let palettes = asset_server.load("palettes.ron");
        Self {
            palettes,
            current_palette: Palette::default(),
        }
    }
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
            windows: Default::default(),
            window_temp: None,
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
