use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

use bevy::math::Vec3Swizzles;
use bevy::{input::mouse::MouseWheel, prelude::*};
use bevy_egui::egui::epaint::Hsva;
use bevy_egui::egui::{
    Id, TextureId,
};
use bevy_egui::{
    egui::{self},
    EguiContext, EguiPlugin,
};
use bevy_egui::egui::epaint::util::{FloatOrd, OrderedFloat};
use bevy_mouse_tracking_plugin::{prelude::*, MainCamera, MousePos, MousePosWorld};
use bevy_prototype_lyon::prelude::*;
use bevy_prototype_lyon::{
    entity::ShapeBundle,
    prelude::{DrawMode, FillMode, GeometryBuilder, ShapePlugin},
};
use bevy_rapier2d::prelude::*;

mod demo;
mod palette;
mod ui;

use bevy_turborand::{DelegatedRng, GlobalRng, RngComponent, RngPlugin};
use derivative::Derivative;
use lyon_path::builder::Build;
use palette::{Palette, PaletteList, PaletteLoader};
use paste::paste;
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
        arrow_right,
        collisions,
        color,
        controller,
        csg,
        erase,
        gravity,
        info,
        material,
        mirror,
        pause,
        play,
        plot,
        text,
        velocity,
        zoom2scene
    ]
);

image_set!(
    Images,
    "app/",
    [hinge_background, hinge_balls, hinge_inner,]
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
        .init_resource::<Images>()
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
        .add_startup_system(configure_visuals)
        .add_startup_system(configure_ui_state)
        .add_startup_system(setup_graphics)
        .add_startup_system(setup_physics)
        .add_startup_system(setup_palettes)
        .add_startup_system(setup_rng)
        .add_system_set(ui::draw_ui())
        .add_system_set(
            SystemSet::new()
                .with_system(mouse_wheel)
                .with_system(left_pressed)
                .with_system(left_release)
                .with_system(process_add_object)
                .with_system(mouse_long_or_moved)
                .with_system(mouse_long_or_moved_writeback),
        )
        .add_system(process_pan)
        .add_system(process_move)
        .add_system(process_unfreeze_entity)
        .add_system(process_rotate)
        .add_system(process_draw_overlay.after(left_release))
        .add_system(process_select_under_mouse.before(process_select))
        .add_system(
            ui::handle_context_menu
                .after(process_select_under_mouse)
                .after(process_select),
        )
        .add_system(process_select)
        .add_system(show_current_tool_icon.after(mouse_wheel))
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
    mut query: Query<(&mut Transform, &mut RigidBody), Without<MainCamera>>,
    mut commands: Commands,
    rapier: Res<RapierContext>,
    mut select_mouse: EventWriter<SelectEvent>,
) {
    use ToolEnum::*;
    for MouseLongOrMoved(hover_tool, pos, button) in events.iter() {
        let pos = *pos;
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
                let under_mouse = find_under_mouse(&rapier, pos, QueryFilter::default(), |ent| {
                    let (transform, _) = query.get(ent).unwrap();
                    transform.translation.z
                }).next();

                if matches!(
                    hover_tool,
                    Move(None) | Rotate(None) | Fix(()) | Hinge(()) | Tracer(())
                ) {
                    select_mouse.send(SelectEvent {
                        entity: under_mouse,
                    });
                }

                match (hover_tool, under_mouse, selected_entity.map(|s| s.entity)) {
                    (Spring(None), _, _) => todo!(),
                    (Drag(None), Some(ent), _) => {
                        *ui_button = Some(Drag(Some(DragState {
                            entity: ent,
                            orig_obj_pos: pos - query.get_mut(ent).unwrap().0.translation.xy(),
                        })));
                    }
                    (Rotate(None), Some(under), _) => {
                        let (transform, mut body) = query.get_mut(under).unwrap();
                        info!("start rotate {:?}", under);
                        *ui_button = Some(Rotate(Some(RotateState {
                            orig_obj_rot: transform.rotation,
                        })));
                        *body = RigidBody::Fixed;
                    }
                    (Rotate(None), None, _) => {
                        ev_writeback.send(MouseLongOrMoved(Pan(None), pos, *button).into());
                    }
                    (_, Some(under), Some(sel)) if under == sel => {
                        let (transform, mut body) = query.get_mut(under).unwrap();
                        *ui_button = Some(Move(Some(MoveState {
                            obj_delta: transform.translation.xy() - pos,
                        })));
                        *body = RigidBody::KinematicPositionBased;
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
    mut context_menu: EventWriter<ContextMenuEvent>,
    mut overlay: ResMut<OverlayState>,
    windows: Res<Windows>,
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
                    _ => {}
                }
                match tool {
                    Move(Some(_)) | Rotate(Some(_)) => {
                        if let Some(EntitySelection { entity }) = ui_state.selected_entity {
                            unfreeze.send(UnfreezeEntityEvent { entity });
                        }
                    }
                    Box(Some(_ent)) if screen_pos.distance(click_pos_screen) > 6.0 => {
                        add_obj.send(AddObjectEvent::Box(click_pos, pos - click_pos));
                    }
                    Circle(Some(_ent)) if screen_pos.distance(click_pos_screen) > 6.0 => {
                        add_obj.send(AddObjectEvent::Circle(
                            click_pos,
                            (pos - click_pos).length(),
                        ));
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
            select_mouse.send(SelectUnderMouseEvent { pos });
        }
    );
    process_button!(
        UsedMouseButton::Right,
        ui_state.mouse_right_pos,
        ui_state.mouse_right,
        {
            info!("selecting under mouse");
            select_mouse.send(SelectUnderMouseEvent { pos });

            let screen_pos = Vec2::new(
                screen_pos.x,
                windows.get_primary().unwrap().height() - screen_pos.y,
            );
            context_menu.send(ContextMenuEvent { screen_pos });
        }
    );
}

enum AddObjectEvent {
    Hinge(Vec2),
    Fix(Vec2),
    Circle(Vec2, f32),
    Box(Vec2, Vec2),
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

fn process_add_object(
    mut events: EventReader<AddObjectEvent>,
    rapier: Res<RapierContext>,
    mut query: Query<(&mut Transform, &mut RigidBody), Without<MainCamera>>,
    images: Res<Images>,
    mut commands: Commands,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
    draw_mode: Query<&DrawMode>,
    palette_config: Res<PaletteConfig>,
    mut z: Local<DepthSorter>,
    mut rng: Query<&mut RngComponent>,
    mut select_mouse: EventWriter<SelectUnderMouseEvent>,
    sensor: Query<&Sensor>,
) {
    let palette = &palette_config.current_palette;
    use AddObjectEvent::*;
    for ev in events.iter() {
        match *ev {
            Box(pos, size) => {
                commands
                    .spawn(PhysicalObject::rect(size, z.pos(pos)))
                    .insert(palette.get_draw_mode(&mut *rng.single_mut()))
                    .log_components();
            }
            Circle(center, radius) => {
                commands
                    .spawn(PhysicalObject::ball(radius, z.pos(center)))
                    .insert(palette.get_draw_mode(&mut *rng.single_mut()))
                    .log_components();
            }
            Fix(pos) => {
                let (entity1, entity2) = {
                    let mut entities = find_under_mouse(&rapier, pos, QueryFilter::only_dynamic(), |ent| {
                        let (transform, _) = query.get(ent).unwrap();
                        transform.translation.z
                    });
                    (entities.next(), entities.next())
                };

                if let Some(entity1) = entity1 {
                    if sensor.get(entity1).is_ok() {
                        select_mouse.send(SelectUnderMouseEvent { pos });
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
                    let mut entities = find_under_mouse(&rapier, pos, QueryFilter::only_dynamic(), |ent| {
                        let (transform, _) = query.get(ent).unwrap();
                        transform.translation.z
                    });
                    (entities.next(), entities.next())
                };

                if let Some(entity1) = entity1 {
                    if sensor.get(entity1).is_ok() {
                        select_mouse.send(SelectUnderMouseEvent { pos });
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
                    let mut back_color = palette.sky_color;
                    if let Some(entity2) = entity2 {
                        back_color = draw_mode.get(entity2).unwrap().get_fill_color();
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
                        commands
                            .entity(entity2)
                            .insert((
                                HingeObject,
                                MultibodyJoint::new(
                                    entity1,
                                    RevoluteJointBuilder::new()
                                        .local_anchor1(anchor1)
                                        .local_anchor2(anchor2),
                                ),
                                ActiveHooks::FILTER_CONTACT_PAIRS,
                            ))
                        /*.add_children(|builder| {
                            builder.spawn(SpriteBundle {
                                texture: images.hinge_background.clone(),
                                transform: Transform::from_scale(Vec3::new(scale, scale, 1.0))
                                    .with_translation(anchor2.extend(0.1)),
                                ..Default::default()
                            });
                        })*/;
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

                    let scale = cameras.single_mut().scale.x;
                    // group the three sprites in an entity containing the transform
                    commands.entity(entity1).add_children(|builder| {
                        builder
                            .spawn((
                                GeometryBuilder::build_as(
                                    &shapes::Circle {
                                        radius: scale * 36.0,
                                        ..Default::default()
                                    },
                                    make_stroke(Color::rgba(0.0, 0.0, 0.0, 0.0), BORDER_THICKNESS)
                                        .as_mode(),
                                    Transform::from_translation(hinge_pos),
                                ),
                                Collider::ball(scale * 36.0),
                                Sensor,
                            ))
                            .add_children(|builder| {
                                builder
                                    .spawn(SpatialBundle::from_transform(Transform::from_scale(
                                        Vec3::new(scale, scale, 1.0) * 0.28,
                                    )))
                                    .with_children(|builder| {
                                        builder.spawn(SpriteBundle {
                                            texture: images.hinge_balls.clone(),
                                            sprite: Sprite {
                                                color: draw_mode
                                                    .get(entity1)
                                                    .unwrap()
                                                    .get_fill_color(),
                                                ..Default::default()
                                            },
                                            ..Default::default()
                                        });
                                    })
                                    .with_children(|builder| {
                                        builder.spawn(SpriteBundle {
                                            texture: images.hinge_background.clone(),
                                            sprite: Sprite {
                                                color: palette.get_color(&mut *rng.single_mut()),
                                                ..Default::default()
                                            },
                                            ..Default::default()
                                        });
                                    })
                                    .with_children(|builder| {
                                        builder.spawn(SpriteBundle {
                                            texture: images.hinge_inner.clone(),
                                            sprite: Sprite {
                                                color: back_color,
                                                ..Default::default()
                                            },
                                            ..Default::default()
                                        });
                                    });
                            });
                    });
                }
            }
        }
    }
}

#[derive(Component)]
struct ColorComponent(Hsva);

#[derive(Bundle)]
struct HingeBundle {}

#[derive(Copy, Clone)]
struct MouseLongOrMoved(ToolEnum, Vec2, UsedMouseButton);

#[derive(Copy, Clone)]
struct PanEvent {
    orig_camera_pos: Vec2,
    delta: Vec2,
}

fn process_pan(
    mut events: EventReader<PanEvent>,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
) {
    for PanEvent {
        orig_camera_pos,
        delta,
    } in events.iter().copied()
    {
        let mut camera = cameras.single_mut();
        camera.translation = (orig_camera_pos + delta * camera.scale.xy()).extend(CAMERA_Z);
    }
}

#[derive(Copy, Clone)]
struct MoveEvent {
    entity: Entity,
    pos: Vec2,
}

fn process_move(mut events: EventReader<MoveEvent>, mut query: Query<&mut Transform>) {
    for MoveEvent { entity, pos } in events.iter().copied() {
        let mut transform = query.get_mut(entity).unwrap();
        transform.translation = pos.extend(transform.translation.z);
    }
}

#[derive(Copy, Clone)]
struct UnfreezeEntityEvent {
    entity: Entity,
}

fn process_unfreeze_entity(
    mut events: EventReader<UnfreezeEntityEvent>,
    mut query: Query<&mut RigidBody>,
) {
    for UnfreezeEntityEvent { entity } in events.iter().copied() {
        let mut body = query.get_mut(entity).unwrap();
        *body = RigidBody::Dynamic;
    }
}

#[derive(Copy, Clone)]
struct RotateEvent {
    entity: Entity,
    orig_obj_rot: Quat,
    click_pos: Vec2,
    mouse_pos: Vec2,
}

fn process_rotate(mut events: EventReader<RotateEvent>, mut query: Query<&mut Transform>) {
    for RotateEvent {
        entity,
        orig_obj_rot,
        click_pos,
        mouse_pos,
    } in events.iter().copied()
    {
        let mut transform = query.get_mut(entity).unwrap();
        let start = click_pos - transform.translation.xy();
        let current = mouse_pos - transform.translation.xy();
        let angle = start.angle_between(current);
        transform.rotation = orig_obj_rot * Quat::from_rotation_z(angle);
    }
}

#[derive(Copy, Clone)]
enum Overlay {
    Rectangle(Vec2),
    Circle(f32),
}

#[derive(Copy, Clone)]
struct DrawOverlayEvent {
    draw_ent: Entity,
    shape: Overlay,
    pos: Vec2,
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

fn process_draw_overlay(
    cameras: Query<&mut Transform, With<MainCamera>>,
    overlay: ResMut<OverlayState>,
    mut commands: Commands,
) {
    if let Some((draw_ent, shape, pos)) = overlay.draw_ent {
        let camera = cameras.single();
        let builder = GeometryBuilder::new();
        let builder = match shape {
            Overlay::Rectangle(size) => builder.add(&shapes::Rectangle {
                extents: size,
                origin: RectangleOrigin::BottomLeft,
            }),
            Overlay::Circle(radius) => builder.add(&shapes::Circle {
                radius,
                ..Default::default()
            }),
        };
        commands.entity(draw_ent).insert(builder.build(
            make_stroke(Color::WHITE, 5.0 * camera.scale.x).as_mode(),
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
                                ev_long_or_moved.send(MouseLongOrMoved(tool, pos, $button));
                            }
                        }
                    }
                } else if mouse_button_input.just_pressed(button.into())
                    && !egui_ctx.ctx_mut().is_using_pointer()
                    && !egui_ctx.ctx_mut().is_pointer_over_area()
                {
                    info!("egui doesn't want pointer input");
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
    commands
        .spawn((Camera2dBundle::new_with_far(CAMERA_FAR), MainCamera))
        .add_world_tracking();

    commands.spawn((ToolCursor, SpriteBundle::default()));
}

#[derive(Component)]
struct ToolCursor;

fn show_current_tool_icon(
    ui_state: Res<UiState>,
    mouse_pos: Res<MousePosWorld>,
    mut icon: Query<(&mut Handle<Image>, &mut Transform, &mut Visibility), With<ToolCursor>>,
    camera: Query<&Transform, (With<MainCamera>, Without<ToolCursor>)>,
    tool_icons: Res<ToolIcons>,
    mut egui_ctx: ResMut<EguiContext>,
) {
    let (mut icon, mut transform, mut vis) = icon.single_mut();
    if egui_ctx.ctx_mut().wants_pointer_input() {
        *vis = Visibility::INVISIBLE;
    } else {
        *vis = Visibility::VISIBLE;
        let current_tool = match ui_state.mouse_button {
            Some(UsedMouseButton::Left) => ui_state.mouse_left,
            Some(UsedMouseButton::Right) => ui_state.mouse_right,
            None => None,
        }
        .unwrap_or(ui_state.toolbox_selected);
        let icon_handle = current_tool.icon(tool_icons);
        let cam_scale = camera.single().scale.xy();
        *icon = icon_handle;
        transform.translation =
            (mouse_pos.xy() + cam_scale * 30.0 * Vec2::new(1.0, -1.0)).extend(FOREGROUND_Z);
        transform.scale = (cam_scale * 0.26).extend(1.0);
    }
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
}

fn hsva_to_rgba(hsva: Hsva) -> Color {
    let color = hsva.to_rgba_unmultiplied();
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

impl Palette {
    fn get_color(&self, rng: &mut impl DelegatedRng) -> Color {
        self.color_range.rand(rng)
    }

    fn get_draw_mode(&self, rng: &mut impl DelegatedRng) -> DrawMode {
        let color = self.color_range.rand_hsva(rng);
        let darkened = Hsva {
            v: color.v * 0.5,
            ..color
        };
        DrawMode::Outlined {
            fill_mode: make_fill(hsva_to_rgba(color)),
            outline_mode: make_stroke(hsva_to_rgba(darkened), BORDER_THICKNESS),
        }
    }
}

impl PhysicalObject {
    fn ball(radius: f32, pos: Vec3) -> Self {
        let radius = radius.abs();
        Self {
            rigid_body: RigidBody::Dynamic,
            velocity: Velocity::default(),
            collider: Collider::ball(radius),
            friction: Friction::default(),
            restitution: Restitution::coefficient(0.7),
            mass_props: ColliderMassProperties::Density(1.0),
            shape: GeometryBuilder::build_as(
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
        }
    }

    fn rect(mut size: Vec2, mut pos: Vec3) -> Self {
        if size.x < 0.0 {
            pos.x += size.x;
            size.x = -size.x;
        }
        if size.y < 0.0 {
            pos.y += size.y;
            size.y = -size.y;
        }
        Self {
            rigid_body: RigidBody::Dynamic,
            velocity: Velocity::default(),
            collider: Collider::cuboid(size.x / 2.0, size.y / 2.0),
            friction: Friction::default(),
            restitution: Restitution::coefficient(0.7),
            mass_props: ColliderMassProperties::Density(1.0),
            shape: GeometryBuilder::build_as(
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
        }
    }
}

#[derive(Component)]
struct HingeObject;

fn setup_physics(mut commands: Commands) {
    /* Create the ground. */
    demo::newton_cradle::init(&mut commands);

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

#[derive(Resource, Default)]
struct PaletteConfig {
    palettes: Handle<PaletteList>,
    current_palette: Palette,
}

fn setup_palettes(mut palette_config: ResMut<PaletteConfig>, asset_server: Res<AssetServer>) {
    palette_config.palettes = asset_server.load("palettes.ron");
}

struct SelectEvent {
    entity: Option<Entity>,
}

#[derive(Component)]
struct UnselectedDrawMode {
    draw_mode: DrawMode,
}

fn process_select(
    mut events: EventReader<SelectEvent>,
    mut state: ResMut<UiState>,
    mut query: Query<&mut DrawMode>,
    query_backup: Query<&UnselectedDrawMode>,
    mut commands: Commands,
) {
    let mut set_selected = move |entity, selected| {
        let Ok(mut current) = query.get_mut(entity) else { return };
        if selected {
            commands.entity(entity).insert(UnselectedDrawMode {
                draw_mode: current.clone(),
            });
            let stroke = make_stroke(Color::WHITE, BORDER_THICKNESS);
            *current = match *current {
                DrawMode::Outlined {
                    fill_mode,
                    outline_mode: _,
                } => DrawMode::Outlined {
                    fill_mode,
                    outline_mode: stroke,
                },
                DrawMode::Fill(fill_mode) => DrawMode::Outlined {
                    fill_mode,
                    outline_mode: stroke,
                },
                DrawMode::Stroke(_) => DrawMode::Stroke(stroke),
            };
        } else {
            let backup = query_backup.get(entity).unwrap();
            *current = backup.draw_mode;
            commands.entity(entity).remove::<UnselectedDrawMode>();
        }
    };

    for SelectEvent { entity } in events.iter() {
        if let Some(EntitySelection { entity }) = state.selected_entity {
            set_selected(entity, false);
        }

        if let Some(entity) = entity {
            info!("Selecting entity: {:?}", entity);
        } else {
            info!("Deselecting entity");
        }

        state.selected_entity = entity.map(|entity| {
            set_selected(entity, true);
            EntitySelection { entity }
        });
    }
}

fn find_under_mouse(
    rapier: &RapierContext,
    pos: Vec2,
    filter: QueryFilter,
    z: impl Fn(Entity) -> f32,
) -> impl Iterator<Item = Entity> {
    #[derive(Derivative)]
    #[derivative(PartialEq, PartialOrd, Eq, Ord)]
    struct EntityZ {
        #[derivative(PartialEq="ignore", PartialOrd="ignore")]
        entity: Entity,
        z: OrderedFloat<f32>,
    }

    let mut set = BTreeSet::new();

    rapier.intersections_with_point(pos, filter, |ent| {
        set.insert(EntityZ {
            entity: ent,
            z: z(ent).ord(),
        });
        true
    });

    set.into_iter().rev().map(|EntityZ { entity, .. }| entity)
}

#[derive(Copy, Clone)]
struct SelectUnderMouseEvent {
    pos: Vec2,
}

fn process_select_under_mouse(
    mut events: EventReader<SelectUnderMouseEvent>,
    rapier: Res<RapierContext>,
    mut select: EventWriter<SelectEvent>,
    query: Query<&Transform, With<RigidBody>>
) {
    for SelectUnderMouseEvent { pos } in events.iter().copied() {
        let selected = find_under_mouse(&rapier, pos, QueryFilter::default(), |ent| {
            let transform = query.get(ent).unwrap();
            transform.translation.z
        }).next();
        select.send(SelectEvent { entity: selected });
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

fn configure_visuals(
    mut egui_ctx: ResMut<EguiContext>,
    palette: Res<PaletteConfig>,
    mut clear_color: ResMut<ClearColor>,
) {
    egui_ctx.ctx_mut().set_visuals(egui::Visuals {
        window_rounding: 0.0.into(),
        ..Default::default()
    });
    clear_color.0 = palette.current_palette.sky_color;
}

fn configure_ui_state(_ui_state: ResMut<UiState>) {}
