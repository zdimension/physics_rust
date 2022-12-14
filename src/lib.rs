use bevy::{
    input::mouse::{MouseMotion, MouseWheel},
    prelude::*,
    render::camera::RenderTarget,
};
use bevy_egui::{egui, EguiContext, EguiPlugin, EguiSettings};
use bevy_inspector_egui::WorldInspectorPlugin;
use bevy_mouse_tracking_plugin::{prelude::*, MainCamera, MousePosWorld};
use bevy_prototype_lyon::{entity::ShapeBundle, prelude::{ShapePlugin, GeometryBuilder, DrawMode, FillMode}};
use bevy_rapier2d::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
use bevy_prototype_lyon::prelude::*;
struct Images {
    bevy_icon: Handle<Image>,
    bevy_icon_inverted: Handle<Image>,
}

impl FromWorld for Images {
    fn from_world(world: &mut World) -> Self {
        let asset_server = world.get_resource_mut::<AssetServer>().unwrap();
        Self {
            bevy_icon: asset_server.load("icon.png"),
            bevy_icon_inverted: asset_server.load("icon_inverted.png"),
        }
    }
}

/// This example demonstrates the following functionality and use-cases of bevy_egui:
/// - rendering loaded assets;
/// - toggling hidpi scaling (by pressing '/' button);
/// - configuring egui contexts during the startup.

pub fn main() {
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
        .run();
}

fn mouse_moved(
    mut mouse_motion_events: EventReader<MouseMotion>,
    mut cameras: Query<&mut Transform, With<MainCamera>>,
    mouse_button_input: Res<Input<MouseButton>>,
) {
    if mouse_button_input.pressed(MouseButton::Right) {
        let mut pos = cameras.single_mut();
        let scale = pos.scale;
        for event in mouse_motion_events.iter() {
            pos.translation += Vec3::new(-event.delta.x, event.delta.y, 0.0) * scale;
        }
    }
}

fn mouse_wheel(
    windows: Res<Windows>,
    mut mouse_wheel_events: EventReader<MouseWheel>,
    mut cameras: Query<&mut Transform, With<MainCamera>>
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

fn setup_graphics(mut commands: Commands) {
    // Add a camera so we can see the debug-render.
    commands
        .spawn((Camera2dBundle::default(), MainCamera))
        .add_world_tracking();
}

#[derive(Bundle)]
struct PhysicalObject {
    rigid_body: RigidBody,
    collider: Collider,
    friction: Friction,
    restitution: Restitution,
    mass_props: ColliderMassProperties,
    shape: ShapeBundle
}

impl PhysicalObject {
    fn ball(radius: f32, transform: Transform) -> Self {
        Self {
            rigid_body: RigidBody::Dynamic,
            collider: Collider::ball(radius),
            friction: Friction::default(),
            restitution: Restitution::coefficient(0.7),
            mass_props: ColliderMassProperties::Density(1.0),
            shape: GeometryBuilder::build_as(
                &shapes::Circle { radius, ..Default::default() },
                DrawMode::Outlined {
                    fill_mode: FillMode::color(Color::CYAN),
                    outline_mode: StrokeMode::new(Color::BLACK, 10.0),
                },
                transform
            )
        }
    }
}

fn setup_physics(mut commands: Commands) {
    /* Create the ground. */
    commands
        .spawn(Collider::cuboid(500.0, 50.0))
        .insert(TransformBundle::from(Transform::from_xyz(0.0, -100.0, 0.0)));

    let circle = PhysicalObject::ball(50.0, Transform::from_xyz(0.0, 200.0, 0.0));
    commands
        .spawn(circle);

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
    main();
}

#[derive(Default, Resource)]
struct UiState {
    label: String,
    value: f32,
    painting: Painting,
    inverted: bool,
    is_window_open: bool,
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
    // You are not required to store Egui texture ids in systems. We store this one here just to
    // demonstrate that rendering by using a texture id of a removed image is handled without
    // making bevy_egui panic.
    mut rendered_texture_id: Local<egui::TextureId>,
    mut is_initialized: Local<bool>,
    // If you need to access the ids from multiple systems, you can also initialize the `Images`
    // resource while building the app and use `Res<Images>` instead.
    images: Local<Images>,
) {
    let mut load = false;
    let mut remove = false;
    let mut invert = false;

    if !*is_initialized {
        *is_initialized = true;
        *rendered_texture_id = egui_ctx.add_image(images.bevy_icon.clone_weak());
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

    if invert {
        ui_state.inverted = !ui_state.inverted;
    }
    if load || invert {
        // If an image is already added to the context, it'll return an existing texture id.
        if ui_state.inverted {
            *rendered_texture_id = egui_ctx.add_image(images.bevy_icon_inverted.clone_weak());
        } else {
            *rendered_texture_id = egui_ctx.add_image(images.bevy_icon.clone_weak());
        };
    }
    if remove {
        egui_ctx.remove_image(&images.bevy_icon);
        egui_ctx.remove_image(&images.bevy_icon_inverted);
    }
}

struct Painting {
    lines: Vec<Vec<egui::Vec2>>,
    stroke: egui::Stroke,
}

impl Default for Painting {
    fn default() -> Self {
        Self {
            lines: Default::default(),
            stroke: egui::Stroke::new(1.0, egui::Color32::LIGHT_BLUE),
        }
    }
}

impl Painting {
    pub fn ui_control(&mut self, ui: &mut egui::Ui) -> egui::Response {
        ui.horizontal(|ui| {
            egui::stroke_ui(ui, &mut self.stroke, "Stroke");
            ui.separator();
            if ui.button("Clear Painting").clicked() {
                self.lines.clear();
            }
        })
        .response
    }

    pub fn ui_content(&mut self, ui: &mut egui::Ui) {
        let (response, painter) =
            ui.allocate_painter(ui.available_size_before_wrap(), egui::Sense::drag());
        let rect = response.rect;

        if self.lines.is_empty() {
            self.lines.push(vec![]);
        }

        let current_line = self.lines.last_mut().unwrap();

        if let Some(pointer_pos) = response.interact_pointer_pos() {
            let canvas_pos = pointer_pos - rect.min;
            if current_line.last() != Some(&canvas_pos) {
                current_line.push(canvas_pos);
            }
        } else if !current_line.is_empty() {
            self.lines.push(vec![]);
        }

        for line in &self.lines {
            if line.len() >= 2 {
                let points: Vec<egui::Pos2> = line.iter().map(|p| rect.min + *p).collect();
                painter.add(egui::Shape::line(points, self.stroke));
            }
        }
    }
}
