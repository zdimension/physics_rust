use std::time::Duration;

use bevy::math::Vec3Swizzles;
use bevy::{input::mouse::MouseWheel, prelude::*};
use bevy_egui::egui::TextureId;
use bevy_egui::{
    egui::{self, Align2},
    EguiContext, EguiPlugin,
};
use bevy_mouse_tracking_plugin::{prelude::*, MainCamera, MousePos, MousePosWorld};
use bevy_prototype_lyon::prelude::*;
use bevy_prototype_lyon::{
    entity::ShapeBundle,
    prelude::{DrawMode, FillMode, GeometryBuilder, ShapePlugin},
};
use bevy_rapier2d::prelude::*;

mod palette;

use palette::{PaletteLoader, PaletteList};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[derive(Resource)]
struct Images {
    hinge_background: Handle<Image>,
    hinge_balls: Handle<Image>,
}

const BORDER_THICKNESS: f32 = 0.03;

impl FromWorld for Images {
    fn from_world(world: &mut World) -> Self {
        let asset_server = world.get_resource_mut::<AssetServer>().unwrap();

        Self {
            hinge_background: asset_server.load("app/hinge_background.png"),
            hinge_balls: asset_server.load("app/hinge_balls.png"),
        }
    }
}

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
        .add_asset::<PaletteList>()
        .init_asset_loader::<PaletteLoader>()
        .init_resource::<UiState>()
        .init_resource::<Images>()
        .insert_resource(RapierConfiguration {
            gravity: Vect::Y * -9.81,
            physics_pipeline_active: false,
            ..Default::default()
        })
        .insert_resource(PhysicsHooksWithQueryResource(Box::new(CollideHooks)))
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
        .add_startup_system(configure_visuals)
        .add_startup_system(configure_ui_state)
        .add_startup_system(setup_graphics)
        .add_startup_system(setup_physics)
        //.add_startup_system(setup_palettes)
        .add_system(ui_example)
        .add_system(mouse_wheel)
        .add_system(mouse_button)
        .run();


    println!("caca");
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
            outline_mode: StrokeMode::new(
                if selected { Color::WHITE } else { Color::BLACK },
                BORDER_THICKNESS,
            ),
        },
        _ => unreachable!("shouldn't happen"),
    };
}

#[derive(Component)]
struct DrawObject;

fn mouse_button(
    mouse_button_input: Res<Input<MouseButton>>,
    mut ui_state: ResMut<UiState>,
    rapier: Res<RapierContext>,
    mouse_pos: Res<MousePosWorld>,
    screen_pos: Res<MousePos>,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
    mut query: Query<(&mut Transform, &mut RigidBody), Without<MainCamera>>,
    mut draw_mode: Query<&mut DrawMode>,
    mut commands: Commands,
    mut egui_ctx: ResMut<EguiContext>,
    images: Res<Images>,
    time: Res<Time>,
) {
    let screen_pos = **screen_pos;

    let ToolDef(_, builder) = ui_state.toolbox_selected;
    let hover_tool = builder();

    use ToolEnum::*;

    let left = mouse_button_input.pressed(MouseButton::Left);
    let pos = mouse_pos.xy();
    if left {
        if let Some((at, click_pos, click_pos_screen)) = ui_state.mouse_left_pos {
            match ui_state.mouse_left {
                Some(Pan(Some(PanState { orig_camera_pos }))) => {
                    let mut camera = cameras.single_mut();
                    camera.translation = (orig_camera_pos
                        + (click_pos_screen - screen_pos) * camera.scale.xy())
                    .extend(0.0);
                }
                Some(Move(Some(state))) => {
                    if let Some(EntitySelection { entity }) = ui_state.selected_entity {
                        let mut transform = query.get_mut(entity).unwrap().0;
                        transform.translation = (pos + state.obj_delta).extend(0.0);
                    } else {
                        ui_state.mouse_left = None;
                        ui_state.mouse_left_pos = None;
                    }
                }
                Some(Rotate(Some(state))) => {
                    if let Some(EntitySelection { entity }) = ui_state.selected_entity {
                        let mut transform = query.get_mut(entity).unwrap().0;
                        let start = click_pos - transform.translation.xy();
                        let current = pos - transform.translation.xy();
                        let angle = start.angle_between(current);
                        transform.rotation = state.orig_obj_rot * Quat::from_rotation_z(angle);
                    } else {
                        ui_state.mouse_left = None;
                        ui_state.mouse_left_pos = None;
                    }
                }
                Some(Box(Some(draw_ent))) => {
                    let camera = cameras.single();
                    commands.entity(draw_ent).insert(GeometryBuilder::build_as(
                        &shapes::Rectangle {
                            extents: pos - click_pos,
                            origin: RectangleOrigin::BottomLeft,
                        },
                        DrawMode::Stroke(StrokeMode::new(Color::WHITE, 5.0 * camera.scale.x)),
                        Transform::from_translation(click_pos.extend(0.0)),
                    ));
                }
                Some(Circle(Some(draw_ent))) => {
                    let camera = cameras.single();
                    commands.entity(draw_ent).insert(GeometryBuilder::build_as(
                        &shapes::Circle {
                            radius: (pos - click_pos).length(),
                            ..Default::default()
                        },
                        DrawMode::Stroke(StrokeMode::new(Color::WHITE, 5.0 * camera.scale.x)),
                        Transform::from_translation(click_pos.extend(0.0)),
                    ));
                }
                _ => {
                    let long_press = time.elapsed() - at > Duration::from_millis(200);
                    let moved = (click_pos - pos).length() > 0.0;
                    let long_or_moved = long_press || moved;

                    if long_or_moved {
                        info!("long or moved!");

                        match hover_tool {
                            Pan(None) => {
                                info!("panning");
                                ui_state.mouse_left = Some(Pan(Some(PanState {
                                    orig_camera_pos: cameras.single_mut().translation.xy(),
                                })));
                            }
                            Zoom(None) => {
                                todo!()
                            }
                            _ => {
                                let mut under_mouse = None;
                                rapier.intersections_with_point(
                                    pos,
                                    QueryFilter::default(),
                                    |ent| {
                                        under_mouse = Some(ent);
                                        false
                                    },
                                );

                                if matches!(
                                    hover_tool,
                                    Move(None) | Rotate(None) | Fix(()) | Hinge(()) | Tracer(())
                                ) {
                                    ui_state.set_selected(under_mouse, &mut draw_mode);
                                }

                                match (
                                    hover_tool,
                                    under_mouse,
                                    ui_state.selected_entity.map(|s| s.entity),
                                ) {
                                    (Spring(None), _, _) => todo!(),
                                    (Drag(None), Some(ent), _) => {
                                        ui_state.mouse_left = Some(Drag(Some(DragState {
                                            entity: ent,
                                            orig_obj_pos: pos
                                                - query.get_mut(ent).unwrap().0.translation.xy(),
                                        })));
                                    }
                                    (Rotate(None), Some(under), _) => {
                                        let (transform, mut body) = query.get_mut(under).unwrap();
                                        ui_state.mouse_left = Some(Rotate(Some(RotateState {
                                            orig_obj_rot: transform.rotation,
                                        })));
                                        *body = RigidBody::Fixed;
                                    }
                                    (_, Some(under), Some(sel)) if under == sel => {
                                        let (transform, mut body) = query.get_mut(under).unwrap();
                                        ui_state.mouse_left = Some(Move(Some(MoveState {
                                            obj_delta: transform.translation.xy() - pos,
                                        })));
                                        *body = RigidBody::Fixed;
                                    }
                                    (Box(None), _, _) => {
                                        ui_state.mouse_left =
                                            Some(Box(Some(commands.spawn(DrawObject).id())));
                                    }
                                    (Circle(None), _, _) => {
                                        ui_state.mouse_left =
                                            Some(Circle(Some(commands.spawn(DrawObject).id())));
                                    }
                                    (tool, _, _) => {
                                        dbg!(tool);
                                        todo!()
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            if !egui_ctx.ctx_mut().is_pointer_over_area() {
                info!("egui doesn't want pointer input");
                ui_state.mouse_left = Some(hover_tool);
                ui_state.mouse_left_pos = Some((time.elapsed(), pos, screen_pos));
                ui_state.mouse_button = Some(UsedMouseButton::Left);
            }
        }
    } else {
        if let Some((_at, click_pos, click_pos_screen)) = ui_state.mouse_left_pos {
            if let Some(tool) = &ui_state.mouse_left {
                match tool {
                    Box(Some(ent)) => {
                        commands.entity(*ent).despawn();
                    }
                    Circle(Some(ent)) => {
                        commands.entity(*ent).despawn();
                    }
                    _ => {}
                }
                match tool {
                    Move(Some(_)) | Rotate(Some(_)) => {
                        if let Some(EntitySelection { entity }) = ui_state.selected_entity {
                            let mut body = query.get_mut(entity).unwrap().1;
                            *body = RigidBody::Dynamic;
                        }
                    }
                    Box(Some(_ent)) if screen_pos.distance(click_pos_screen) > 6.0 => {
                        commands
                            .spawn(PhysicalObject::rect(pos - click_pos, click_pos))
                            .log_components();
                    }
                    Circle(Some(_ent)) if screen_pos.distance(click_pos_screen) > 6.0 => {
                        commands
                            .spawn(PhysicalObject::ball((pos - click_pos).length(), click_pos))
                            .log_components();
                    }
                    Spring(Some(_)) => {
                        todo!()
                    }
                    Thruster(_) => {
                        todo!()
                    }
                    Fix(()) => {
                        let mut entity1 = None;
                        let mut entity2 = None;
                        rapier.intersections_with_point(pos, QueryFilter::default(), |ent| {
                            if entity1 == None {
                                entity1 = Some(ent);
                                true
                            } else {
                                entity2 = Some(ent);
                                false
                            }
                        });
                        if let Some(entity1) = entity1 {
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
                    Hinge(()) => {
                        let mut entity1 = None;
                        let mut entity2 = None;
                        rapier.intersections_with_point(pos, QueryFilter::default(), |ent| {
                            if entity1 == None {
                                entity1 = Some(ent);
                                true
                            } else {
                                entity2 = Some(ent);
                                false
                            }
                        });
                        if let Some(entity1) = entity1 {
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
                                )).add_children(|builder| {
                                    builder.spawn(SpriteBundle {
                                        texture: images.hinge_background.clone(),
                                        transform: Transform::from_scale(Vec3::new(0.001, 0.001, 1.0)).with_translation(anchor2.extend(1.0)),
                                        ..Default::default()
                                    });
                                });
                                commands.entity(entity1).add_children(|builder| {
                                    builder.spawn(SpriteBundle {
                                        texture: images.hinge_balls.clone(),
                                        transform: Transform::from_scale(Vec3::new(0.001, 0.001, 1.0)).with_translation(anchor1.extend(1.0)),
                                        ..Default::default()
                                    });
                                })
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
                        }
                    }
                    Tracer(()) => {
                        todo!()
                    }
                    Pan(Some(_)) | Zoom(Some(_)) | Drag(Some(_)) => {
                        //
                    }
                    _ => {
                        info!("selecting under mouse");
                        ui_state.select_under_mouse(pos, &rapier, &mut draw_mode);
                    }
                }
            }
            info!("resetting state");
            ui_state.mouse_left_pos = None;
            ui_state.mouse_left = None;
        }
    }
}

fn setup_graphics(mut commands: Commands) {
    // Add a camera so we can see the debug-render.
    commands
        .spawn((Camera2dBundle::default(), MainCamera))
        .add_world_tracking();
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

impl PhysicalObject {
    fn ball(radius: f32, pos: Vec2) -> Self {
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
                    fill_mode: FillMode {
                        color: Color::CYAN,
                        options: FillOptions::default().with_tolerance(0.0001),
                    },
                    outline_mode: StrokeMode {
                        color: Color::BLACK,
                        options: StrokeOptions::default()
                            .with_tolerance(0.0001)
                            .with_line_width(BORDER_THICKNESS),
                    },
                },
                Transform::from_translation(pos.extend(0.0)),
            ),
        }
    }

    fn rect(mut size: Vec2, mut pos: Vec2) -> Self {
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
                    fill_mode: FillMode::color(Color::CYAN),
                    outline_mode: StrokeMode::new(Color::BLACK, BORDER_THICKNESS),
                },
                Transform::from_translation((pos + size / 2.0).extend(0.0)),
            ),
        }
    }
}

#[derive(Component)]
struct HingeObject;

fn setup_physics(mut commands: Commands) {
    /* Create the ground. */
    let ground = PhysicalObject::rect(Vec2::new(8.0, 0.5), Vec2::new(0.0, -3.0));
    commands.spawn(ground).insert(RigidBody::Fixed);
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

#[derive(Debug)]
enum ToolEnum {
    Move(Option<MoveState>),
    Drag(Option<DragState>),
    Rotate(Option<RotateState>),
    Box(Option<Entity>),
    Circle(Option<Entity>),
    Spring(Option<()>),
    Thruster(Option<()>),
    Fix(()),
    Hinge(()),
    Tracer(()),
    Pan(Option<PanState>),
    Zoom(Option<()>),
}

#[derive(Debug)]
struct PanState {
    orig_camera_pos: Vec2,
}

#[derive(Debug)]
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

#[derive(Copy, Clone, PartialEq)]
struct EntitySelection {
    entity: Entity,
}

enum UsedMouseButton {
    Left,
    Right,
}

#[derive(Resource)]
struct UiState {
    selected_entity: Option<EntitySelection>,
    toolbox: Vec<Vec<ToolDef>>,
    toolbox_bottom: Vec<ToolDef>,
    toolbox_selected: ToolDef,
    mouse_left: Option<ToolEnum>,
    mouse_left_pos: Option<(Duration, Vec2, Vec2)>,
    mouse_right: Option<ToolEnum>,
    mouse_right_pos: Option<Vec2>,
    mouse_button: Option<UsedMouseButton>,
}

#[derive(Resource)]
struct PaletteConfig {
    palettes: Handle<PaletteList>
}

fn setup_palettes(mut PaletteConfig: ResMut<PaletteConfig>, asset_server: Res<AssetServer>) {
    PaletteConfig.palettes = asset_server.load("palettes.ron");
}

impl UiState {
    fn set_selected(&mut self, ent: Option<Entity>, query: &mut Query<&mut DrawMode>) {
        if let Some(ent) = self.selected_entity {
            let dm = query.get_mut(ent.entity).unwrap();
            set_selected(dm, false);
        }

        self.selected_entity = ent.map(|ent| {
            let dm = query.get_mut(ent).unwrap();
            set_selected(dm, true);
            EntitySelection { entity: ent }
        });
    }

    fn select_under_mouse(
        &mut self,
        pos: Vec2,
        rapier: &Res<RapierContext>,
        query: &mut Query<&mut DrawMode>,
    ) {
        let mut selected = None;
        rapier.intersections_with_point(pos, QueryFilter::default(), |ent| {
            selected = Some(ent);
            false
        });
        self.set_selected(selected, query);
    }
}

impl FromWorld for UiState {
    fn from_world(world: &mut World) -> Self {
        let mut egui_ctx = unsafe { world.get_resource_unchecked_mut::<EguiContext>().unwrap() };
        let assets = world.get_resource::<AssetServer>().unwrap();
        macro_rules! tool {
            ($img:literal, $ty:ident) => {
                ToolDef(
                    egui_ctx.add_image(assets.load(concat!("tools/", $img, ".png"))),
                    || ToolEnum::$ty(Default::default()),
                )
            };
        }

        let pan = tool!("pan", Pan);

        Self {
            selected_entity: None,
            toolbox: vec![
                vec![
                    tool!("move", Move),
                    tool!("drag", Drag),
                    tool!("rotate", Rotate),
                ],
                vec![tool!("box", Box), tool!("circle", Circle)],
                vec![
                    tool!("spring", Spring),
                    tool!("fixjoint", Fix),
                    tool!("hinge", Hinge),
                    tool!("thruster", Thruster),
                    tool!("tracer", Tracer),
                ],
            ],
            toolbox_bottom: vec![tool!("zoom", Zoom), pan],
            toolbox_selected: pan,
            mouse_left: None,
            mouse_left_pos: None,
            mouse_right: None,
            mouse_right_pos: None,
            mouse_button: None,
        }
    }
}

fn configure_visuals(mut egui_ctx: ResMut<EguiContext>) {
    egui_ctx.ctx_mut().set_visuals(egui::Visuals {
        window_rounding: 0.0.into(),
        ..Default::default()
    });
}

fn configure_ui_state(_ui_state: ResMut<UiState>) {}

#[derive(Copy, Clone)]
struct ToolDef(TextureId, fn() -> ToolEnum);

impl PartialEq for ToolDef {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

fn ui_example(
    mut egui_ctx: ResMut<EguiContext>,
    mut ui_state: ResMut<UiState>,
    mut is_initialized: Local<bool>,
    mut rapier: ResMut<RapierConfiguration>,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
    assets: Res<AssetServer>,
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

    egui::Window::new("Tools")
        .anchor(Align2::LEFT_BOTTOM, [0.0, 0.0])
        .title_bar(false)
        .resizable(false)
        //.resize(|r| r.default_width(0.0))
        .show(egui_ctx.clone().ctx_mut(), |ui| {
            ui.vertical(|ui| {
                let ui_state = &mut *ui_state;
                for (i, category) in ui_state.toolbox.iter().enumerate() {
                    if i > 0 {
                        // todo size
                        //ui.separator();
                    }
                    for chunk in category.chunks(2) {
                        ui.horizontal(|ui| {
                            for def @ ToolDef(image, _) in chunk {
                                if ui
                                    .add(
                                        egui::ImageButton::new(*image, [24.0, 24.0])
                                            .selected(ui_state.toolbox_selected == *def),
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

    egui::Window::new("Tools2")
        .anchor(Align2::CENTER_BOTTOM, [0.0, 0.0])
        .title_bar(false)
        .resizable(false)
        .show(egui_ctx.clone().ctx_mut(), |ui| {
            ui.horizontal(|ui| {
                let ui_state = &mut *ui_state;
                for def @ ToolDef(image, _) in ui_state.toolbox_bottom.iter() {
                    if ui
                        .add(
                            egui::ImageButton::new(*image, [32.0, 32.0])
                                .selected(ui_state.toolbox_selected == *def),
                        )
                        .clicked()
                    {
                        ui_state.toolbox_selected = *def;
                    }
                }

                let pause = egui_ctx.add_image(assets.load("gui/pause.png"));
                let play = egui_ctx.add_image(assets.load("gui/play.png"));

                let playpause = ui.add(egui::ImageButton::new(
                    if rapier.physics_pipeline_active {
                        pause
                    } else {
                        play
                    },
                    [32.0, 32.0],
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
            })
        });
}
