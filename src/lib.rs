use bevy::input::mouse::MouseButtonInput;
use bevy::input::ButtonState;
use bevy::math::Vec3Swizzles;
use bevy::{
    input::mouse::{MouseMotion, MouseWheel},
    prelude::*,
};
use bevy_egui::egui::{TextureId, Ui};
use bevy_egui::{
    egui::{self, Align2},
    EguiContext, EguiPlugin, EguiSettings,
};
use bevy_mouse_tracking_plugin::{prelude::*, MainCamera, MousePos, MousePosWorld};
use bevy_prototype_lyon::prelude::*;
use bevy_prototype_lyon::{
    entity::ShapeBundle,
    prelude::{DrawMode, FillMode, GeometryBuilder, ShapePlugin},
};
use bevy_rapier2d::prelude::*;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
struct Images {
    box_: Handle<Image>,
    brush: Handle<Image>,
    chain: Handle<Image>,
    circle: Handle<Image>,
    cut: Handle<Image>,
    drag: Handle<Image>,
    eraser: Handle<Image>,
    fixjoint: Handle<Image>,
    gear: Handle<Image>,
    hinge: Handle<Image>,
    laserpen: Handle<Image>,
    move_: Handle<Image>,
    pan: Handle<Image>,
    plane: Handle<Image>,
    polygon: Handle<Image>,
    rotate: Handle<Image>,
    scale: Handle<Image>,
    sketch: Handle<Image>,
    spring: Handle<Image>,
    texture: Handle<Image>,
    thruster: Handle<Image>,
    tracer: Handle<Image>,
    zoom: Handle<Image>,
}

impl FromWorld for Images {
    fn from_world(world: &mut World) -> Self {
        let asset_server = world.get_resource_mut::<AssetServer>().unwrap();

        Self {
            box_: asset_server.load("tools/box.png"),
            brush: asset_server.load("tools/brush.png"),
            chain: asset_server.load("tools/chain.png"),
            circle: asset_server.load("tools/circle.png"),
            cut: asset_server.load("tools/cut.png"),
            drag: asset_server.load("tools/drag.png"),
            eraser: asset_server.load("tools/eraser.png"),
            fixjoint: asset_server.load("tools/fixjoint.png"),
            gear: asset_server.load("tools/gear.png"),
            hinge: asset_server.load("tools/hinge.png"),
            laserpen: asset_server.load("tools/laserpen.png"),
            move_: asset_server.load("tools/move.png"),
            pan: asset_server.load("tools/pan.png"),
            plane: asset_server.load("tools/plane.png"),
            polygon: asset_server.load("tools/polygon.png"),
            rotate: asset_server.load("tools/rotate.png"),
            scale: asset_server.load("tools/scale.png"),
            sketch: asset_server.load("tools/sketch.png"),
            spring: asset_server.load("tools/spring.png"),
            texture: asset_server.load("tools/texture.png"),
            thruster: asset_server.load("tools/thruster.png"),
            tracer: asset_server.load("tools/tracer.png"),
            zoom: asset_server.load("tools/zoom.png"),
        }
    }
}

/// This example demonstrates the following functionality and use-cases of bevy_egui:
/// - rendering loaded assets;
/// - toggling hidpi scaling (by pressing '/' button);
/// - configuring egui contexts during the startup.

pub fn app_main() {
    App::new()
        .insert_resource(ClearColor(Color::rgb(0.0, 0.0, 0.0)))
        .insert_resource(Msaa { samples: 4 })
        .add_plugins(DefaultPlugins)
        .add_plugin(EguiPlugin)
        .init_resource::<UiState>()
        .add_plugin(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0))
        .add_plugin(RapierDebugRenderPlugin::default())
        .add_plugin(MousePosPlugin)
        .add_plugin(ShapePlugin)
        .add_startup_system(configure_visuals)
        .add_startup_system(configure_ui_state)
        .add_startup_system(setup_graphics)
        .add_startup_system(setup_physics)
        .add_system(ui_example)
        .add_system(mouse_moved)
        .add_system(mouse_wheel)
        .add_system(mouse_button)
        .run();
}

#[derive(Component)]
struct TemporarilyFrozen;

fn mouse_moved(
    mut mouse_motion_events: EventReader<MouseMotion>,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
    mouse_button_input: Res<Input<MouseButton>>,
    mut entities: Query<(&mut Transform, &mut RigidBody), Without<MainCamera>>,
    mut ui_state: ResMut<UiState>,
    mouse_pos: Res<MousePosWorld>,
    mut commands: Commands,
) {
    /*if mouse_button_input.pressed(MouseButton::Right) {
        if ui_state.rotating.is_none() {
            if let Some(EntitySelection { entity, .. }) = ui_state.selected_entity {
                commands.entity(entity).insert(TemporarilyFrozen);
                let (entity, mut body) = entities.get_mut(entity).unwrap();
                *body = RigidBody::Fixed;
                ui_state.rotating = Some(entity.rotation);
            } else {
                let mut cam = cameras.single_mut();
                let scale = cam.scale;
                for event in mouse_motion_events.iter() {
                    cam.translation += Vec3::new(-event.delta.x, event.delta.y, 0.0) * scale;
                }
            }
        }

        if let Some(rot) = ui_state.rotating {
            let EntitySelection { entity, rel_pos } = &ui_state.selected_entity.as_ref().unwrap();
            let (mut entity, _body) = entities.get_mut(*entity).unwrap();
            let rel = (**mouse_pos - entity.translation).truncate();
            if rel != *rel_pos {
                let rel_angle = -rel.angle_between(*rel_pos);
                entity.rotation = rot * Quat::from_rotation_z(rel_angle);
            }
        }
    }

    if mouse_button_input.pressed(MouseButton::Left) {
        if ui_state.current_tool == MouseTool::Move {
            if let Some(EntitySelection { entity, .. }) = ui_state.selected_entity {
                commands.entity(entity).insert(TemporarilyFrozen);
                let (mut entity, mut body) = entities.get_mut(entity).unwrap();
                *body = RigidBody::Fixed;
                let cam = cameras.single_mut();
                let scale = cam.scale;
                for event in mouse_motion_events.iter() {
                    entity.translation += Vec3::new(event.delta.x, -event.delta.y, 0.0) * scale;
                }
            }
        }
    }*/
}

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
            outline_mode: StrokeMode::new(if selected { Color::RED } else { Color::BLACK }, 3.0),
        },
        _ => unreachable!("shouldn't happen"),
    };
}

fn check_click(click_pos: &mut Option<Vec2>, cur_pos: Vec2) -> bool {
    if let Some(pos) = *click_pos {
        *click_pos = None;
        if cur_pos == pos {
            return true;
        }
    }
    false
}

fn mouse_button(
    mut mouse_button_events: EventReader<MouseButtonInput>,
    mut ui_state: ResMut<UiState>,
    rapier_context: Res<RapierContext>,
    mouse_pos: Res<MousePosWorld>,
    screen_pos: Res<MousePos>,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
    mut query: Query<(&mut DrawMode, &mut Transform, &mut RigidBody), Without<MainCamera>>,
    frozen: Query<&TemporarilyFrozen>,
    mut commands: Commands,
) {
    let screen_pos = **screen_pos;
    for event in mouse_button_events.iter() {
        if event.state == ButtonState::Pressed {
            match event.button {
                MouseButton::Left => ui_state.mouse_left_pos = Some(screen_pos),
                MouseButton::Right => ui_state.mouse_right_pos = Some(screen_pos),
                _ => {}
            }
        } else if event.state == ButtonState::Released {
            match event.button {
                MouseButton::Left => if check_click(&mut ui_state.mouse_left_pos, screen_pos) {
                    // perform left click
                },
                MouseButton::Right => if check_click(&mut ui_state.mouse_left_pos, screen_pos) {
                    // perform left click
                },
                _ => {}
            }
        }





        
        match (event.button, event.state) {
            (MouseButton::Left | MouseButton::Right, ButtonState::Pressed) => {
                let pos = mouse_pos.xy();
                match ui_state.current_tool {
                    MouseTool::DrawShape(shp) => {}
                    _ => {}
                };
                if let Some(EntitySelection { entity, .. }) = ui_state.selected_entity {
                    let (shape, _transform, _) = query.get_mut(entity).unwrap();
                    set_selected(shape, false);
                    ui_state.selected_entity = None;
                }
                rapier_context.intersections_with_point(pos, QueryFilter::default(), |ent| {
                    let (shape, transform, _) = query.get_mut(ent).unwrap();
                    set_selected(shape, true);
                    ui_state.selected_entity = Some(EntitySelection {
                        entity: ent,
                        rel_pos: pos - transform.translation.truncate(),
                    });
                    false
                });
            }
            (MouseButton::Left, ButtonState::Released) => {
                if ui_state.current_tool == MouseTool::Move {
                    if let Some(EntitySelection { entity, .. }) = ui_state.selected_entity {
                        if frozen.get(entity).is_ok() {
                            commands.entity(entity).remove::<TemporarilyFrozen>();
                            let (_, _, mut body) = query.get_mut(entity).unwrap();
                            *body = RigidBody::Dynamic;
                        }
                    }
                }
            }
            (MouseButton::Right, ButtonState::Released) => {
                if ui_state.rotating.is_some() {
                    if let Some(EntitySelection { entity, .. }) = ui_state.selected_entity {
                        if frozen.get(entity).is_ok() {
                            commands.entity(entity).remove::<TemporarilyFrozen>();
                            let (_, _, mut body) = query.get_mut(entity).unwrap();
                            *body = RigidBody::Dynamic;
                        }
                    }
                    ui_state.rotating = None;
                }
            }
            _ => {}
        };
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
    fn ball(radius: f32, transform: Transform) -> Self {
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
                    fill_mode: FillMode::color(Color::CYAN),
                    outline_mode: StrokeMode::new(Color::BLACK, 3.0),
                },
                transform,
            ),
        }
    }
}

fn setup_physics(mut commands: Commands) {
    /* Create the ground. */
    commands
        .spawn(Collider::cuboid(500.0, 50.0))
        .insert(TransformBundle::from(Transform::from_xyz(0.0, -100.0, 0.0)));

    let circle = PhysicalObject::ball(50.0, Transform::from_xyz(0.0, 200.0, 0.0));
    commands.spawn(circle);
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
fn wasm_main() {
    app_main();
}

enum ToolEnum {
    Pan(Option<PanState>),
    Move(Option<MoveState>),
    Rotate(Option<RotateState>),
}

struct PanState {
    orig_camera_pos: Vec2,
}

struct MoveState {
    orig_obj_pos: Vec2,
}

struct RotateState {
    orig_obj_pos: Vec2,
    orig_obj_rot: Quat,
}

#[derive(Copy, Clone, PartialEq)]
struct EntitySelection {
    entity: Entity,
}

#[derive(Resource)]
struct UiState {
    selected_entity: Option<EntitySelection>,
    toolbox: Vec<Vec<ToolDef>>,
    toolbox_bottom: Vec<ToolDef>,
    toolbox_selected: ToolDef,
    mouse_left: Option<ToolEnum>,
    mouse_left_pos: Option<Vec2>,
    mouse_right: Option<ToolEnum>,
    mouse_right_pos: Option<Vec2>,
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
            toolbox: vec![vec![tool!("move", Move), tool!("rotate", Rotate)]],
            toolbox_bottom: vec![pan],
            toolbox_selected: pan,
            ..Default::default()
        }
    }
}

fn configure_visuals(mut egui_ctx: ResMut<EguiContext>) {
    egui_ctx.ctx_mut().set_visuals(egui::Visuals {
        window_rounding: 0.0.into(),
        ..Default::default()
    });
}

fn configure_ui_state(mut ui_state: ResMut<UiState>) {
    ui_state.is_window_open = true;
}

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
    assets: Res<AssetServer>,
) {
    if !*is_initialized {
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
        .show(egui_ctx.clone().ctx_mut(), |ui| {
            egui::Grid::new("toolsgrid")
                .min_col_width(0.0)
                .show(ui, |ui| {
                    let ui_state = &mut *ui_state;
                    for category in ui_state.toolbox.iter() {
                        for (i, def @ ToolDef(image, _)) in category.iter().enumerate() {
                            if ui
                                .add(
                                    egui::ImageButton::new(*image, [26.0, 26.0])
                                        .selected(ui_state.toolbox_selected == *def),
                                )
                                .clicked()
                            {
                                ui_state.toolbox_selected = *def;
                            }
                        }
                        ui.end_row();
                    }
                })
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
