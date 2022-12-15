use bevy::input::mouse::MouseButtonInput;
use bevy::input::ButtonState;
use bevy::math::Vec3Swizzles;
use bevy::{
    input::mouse::{MouseMotion, MouseWheel},
    prelude::*,
    render::camera::RenderTarget,
};
use bevy_egui::{
    egui::{self, Align2},
    EguiContext, EguiPlugin, EguiSettings,
};
use bevy_mouse_tracking_plugin::{prelude::*, MainCamera, MousePosWorld};
use bevy_prototype_lyon::prelude::*;
use bevy_prototype_lyon::{
    entity::ShapeBundle,
    prelude::{DrawMode, FillMode, GeometryBuilder, ShapePlugin},
};
use bevy_rapier2d::prelude::*;
use std::ops::DerefMut;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
struct Images {
    circle: Handle<Image>,
    coil: Handle<Image>,
    console: Handle<Image>,
    drag: Handle<Image>,
    fix: Handle<Image>,
    gravity: Handle<Image>,
    grid: Handle<Image>,
    hinge: Handle<Image>,
    laser: Handle<Image>,
    move_: Handle<Image>,
    options: Handle<Image>,
    pause: Handle<Image>,
    play: Handle<Image>,
    rectangle: Handle<Image>,
    reset: Handle<Image>,
    thruster: Handle<Image>,
    tracer: Handle<Image>,
    wind: Handle<Image>,
}

impl FromWorld for Images {
    fn from_world(world: &mut World) -> Self {
        let asset_server = world.get_resource_mut::<AssetServer>().unwrap();

        Self {
            circle: asset_server.load("circle.png"),
            coil: asset_server.load("coil.png"),
            console: asset_server.load("console.png"),
            drag: asset_server.load("drag.png"),
            fix: asset_server.load("fix.png"),
            gravity: asset_server.load("gravity.png"),
            grid: asset_server.load("grid.png"),
            hinge: asset_server.load("hinge.png"),
            laser: asset_server.load("laser.png"),
            move_: asset_server.load("move.png"),
            options: asset_server.load("options.png"),
            pause: asset_server.load("pause.png"),
            play: asset_server.load("play.png"),
            rectangle: asset_server.load("rectangle.png"),
            reset: asset_server.load("reset.png"),
            thruster: asset_server.load("thruster.png"),
            tracer: asset_server.load("tracer.png"),
            wind: asset_server.load("wind.png"),
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
        .init_resource::<UiState>()
        .add_plugins(DefaultPlugins)
        .add_plugin(EguiPlugin)
        .add_plugin(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(100.0))
        .add_plugin(RapierDebugRenderPlugin::default())
        .add_plugin(MousePosPlugin)
        .add_plugin(ShapePlugin)
        .add_startup_system(configure_visuals)
        .add_startup_system(configure_ui_state)
        .add_startup_system(setup_graphics)
        .add_startup_system(setup_physics)
        .add_system(update_ui_scale_factor)
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
    if mouse_button_input.pressed(MouseButton::Right) {
        if ui_state.rotating.is_none() {
            if let Some(EntitySelection { entity, .. }) = ui_state.selected_entity {
                commands.entity(entity).insert(TemporarilyFrozen);
                let (mut entity, mut body) = entities.get_mut(entity).unwrap();
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
            let (mut entity, mut body) = entities.get_mut(*entity).unwrap();
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
                let mut cam = cameras.single_mut();
                let scale = cam.scale;
                for event in mouse_motion_events.iter() {
                    entity.translation += Vec3::new(event.delta.x, -event.delta.y, 0.0) * scale;
                }
            }
        }
    }
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
            outline_mode,
        } => DrawMode::Outlined {
            fill_mode,
            outline_mode: StrokeMode::new(if selected { Color::RED } else { Color::BLACK }, 3.0),
        },
        _ => unreachable!("shouldn't happen"),
    };
}

fn mouse_button(
    mut mouse_button_events: EventReader<MouseButtonInput>,
    mut ui_state: ResMut<UiState>,
    rapier_context: Res<RapierContext>,
    mouse_pos: Res<MousePosWorld>,
    mut query: Query<(&mut DrawMode, &Transform, &mut RigidBody)>,
    frozen: Query<&TemporarilyFrozen>,
    mut commands: Commands,
) {
    for event in mouse_button_events.iter() {
        match (event.button, event.state) {
            (MouseButton::Left | MouseButton::Right, ButtonState::Pressed) => {
                let pos = mouse_pos.xy();
                if let Some(EntitySelection { entity, .. }) = ui_state.selected_entity {
                    let (mut shape, transform, _) = query.get_mut(entity).unwrap();
                    set_selected(shape, false);
                    ui_state.selected_entity = None;
                }
                rapier_context.intersections_with_point(pos, QueryFilter::default(), |ent| {
                    let (mut shape, transform, _) = query.get_mut(ent).unwrap();
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

        if event.button == MouseButton::Left {
            if event.state == ButtonState::Pressed {
                let pos = mouse_pos.xy();
                if let Some(EntitySelection { entity, .. }) = ui_state.selected_entity {
                    let (mut shape, transform, _) = query.get_mut(entity).unwrap();
                    set_selected(shape, false);
                }
                rapier_context.intersections_with_point(pos, QueryFilter::default(), |ent| {
                    let (mut shape, transform, _) = query.get_mut(ent).unwrap();
                    set_selected(shape, true);
                    ui_state.selected_entity = Some(EntitySelection {
                        entity: ent,
                        rel_pos: pos - transform.translation.truncate(),
                    });
                    false
                });
            } else {
                if ui_state.current_tool == MouseTool::Move {
                    if let Some(EntitySelection { entity, .. }) = ui_state.selected_entity {
                        let (_, _, mut body) = query.get_mut(entity).unwrap();
                        *body = RigidBody::Dynamic;
                    }
                }
            }
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

    /* Create the bouncing ball. */
    /*     commands
    .spawn(RigidBody::Dynamic)
    .insert(Collider::ball(50.0))
    .insert(Restitution::coefficient(0.7))
    .insert(TransformBundle::from(Transform::from_xyz(0.0, 400.0, 0.0)));*/
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
fn wasm_main() {
    app_main();
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum MouseTool {
    Drag,
    Move,
    DrawShape(ShapeTool),
}

impl Default for MouseTool {
    fn default() -> Self {
        Self::Drag
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum ShapeTool {
    Circle,
    Rectangle,
}

struct EntitySelection {
    entity: Entity,
    rel_pos: Vec2,
}

#[derive(Default, Resource)]
struct UiState {
    current_tool: MouseTool,
    rotating: Option<Quat>,
    is_window_open: bool,
    selected_entity: Option<EntitySelection>,
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

fn update_ui_scale_factor(
    keyboard_input: Res<Input<KeyCode>>,
    mut toggle_scale_factor: Local<Option<bool>>,
    mut egui_settings: ResMut<EguiSettings>,
    windows: Res<Windows>,
) {
    if keyboard_input.just_pressed(KeyCode::Slash) || toggle_scale_factor.is_none() {
        *toggle_scale_factor = Some(!toggle_scale_factor.unwrap_or(true));

        if let Some(window) = windows.get_primary() {
            let scale_factor = if toggle_scale_factor.unwrap() {
                1.0
            } else {
                1.0 / window.scale_factor()
            };
            egui_settings.scale_factor = scale_factor;
        }
    }
}

fn ui_example(
    mut egui_ctx: ResMut<EguiContext>,
    mut ui_state: ResMut<UiState>,
    mut is_initialized: Local<bool>,
    mut rapier: ResMut<RapierConfiguration>,
    // If you need to access the ids from multiple systems, you can also initialize the `Images`
    // resource while building the app and use `Res<Images>` instead.
    images: Local<Images>,
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

    egui::Window::new("Window")
        .vscroll(true)
        .open(&mut ui_state.is_window_open)
        .show(egui_ctx.ctx_mut(), |ui| {
            ui.label("Windows can be moved by dragging them.");
            ui.label("They are automatically sized based on contents.");
            ui.label("You can turn on resizing and scrolling if you like.");
            ui.label("You would normally chose either panels OR windows.");
        });

    let drag = egui_ctx.add_image(images.drag.clone());
    let move_ = egui_ctx.add_image(images.move_.clone());
    let rectangle = egui_ctx.add_image(images.rectangle.clone());
    let circle = egui_ctx.add_image(images.circle.clone());
    let play = egui_ctx.add_image(images.play.clone());
    let pause = egui_ctx.add_image(images.pause.clone());
    let cur_tool = ui_state.current_tool;
    egui::Window::new("Tools")
        .anchor(Align2::CENTER_BOTTOM, [0.0, 0.0])
        .title_bar(false)
        .resizable(false)
        .show(egui_ctx.clone().ctx_mut(), |ui| {
            ui.horizontal(|ui| {
                for (image, tool) in [
                    (drag, MouseTool::Drag),
                    (move_, MouseTool::Move),
                    (rectangle, MouseTool::DrawShape(ShapeTool::Rectangle)),
                    (circle, MouseTool::DrawShape(ShapeTool::Circle)),
                ] {
                    if ui
                        .add(egui::ImageButton::new(image, [32.0, 32.0]).selected(cur_tool == tool))
                        .clicked()
                    {
                        ui_state.current_tool = tool;
                    }
                }

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
