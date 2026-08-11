use std::collections::HashSet;
use std::f32::consts::PI;

use crate::lyon_compat::Shape;
use crate::objects::axle::{FixObject, JointGeometry};
use crate::objects::phy_obj::{
    CircleVisual, FreeformObject, PhysicalGeometry, set_box_geometry, set_circle_geometry,
};
use crate::objects::plane::PlaneObject;
use crate::objects::spring::{SpringEnd, SpringObject};
use crate::tools::ToolIcons;
use crate::tools::add_object::{
    AddAxleEvent, AddObjectEvent, AttachmentLinks, despawn_attachment_links,
};
use crate::tools::gear::{GearOutline, GearSettings, gearify_path};
use crate::tools::polygon::tessellate_path;
use crate::ui::images::GuiIcons;
use crate::ui::windows::menu::MenuWindow;
use crate::ui::{
    InitialPos, SceneState, Subwindow, WindowSelectionTarget, window_target_entities, window_title,
};
use avian2d::parry::shape::TypedShape;
use avian2d::prelude::*;
use bevy::prelude::*;
use bevy_egui::egui::load::SizedTexture;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

pub fn add_systems(app: &mut App) {
    app.add_message::<GeometryActionEvent>()
        .add_systems(
            EguiPrimaryContextPass,
            GeometryActionsWindow::show.after(MenuWindow::show),
        )
        .add_systems(Update, process_geometry_actions);
}

#[derive(Default, Component)]
pub struct GeometryActionsWindow;

#[derive(Clone, Debug, Message)]
enum GeometryActionEvent {
    GlueToBackground(Vec<Entity>),
    GlueTogether(Vec<Entity>),
    Loosen(Vec<Entity>),
    Gearify {
        targets: Vec<Entity>,
        teeth_size: f32,
    },
    ShapesToCircles(Vec<Entity>),
    ShapesToBoxes(Vec<Entity>),
}

/// Marks a fixed joint created by a glue action. Unlike an ordinary fixpoint,
/// this joint deliberately has no selectable or rendered visual entity.
#[derive(Copy, Clone, Debug, Component)]
struct VirtualFixpoint;

impl GeometryActionsWindow {
    #[allow(clippy::too_many_arguments)]
    fn show(
        mut wnds: Query<
            (
                Entity,
                Option<&ChildOf>,
                Option<&WindowSelectionTarget>,
                &mut InitialPos,
            ),
            With<GeometryActionsWindow>,
        >,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
        mut add_obj: MessageWriter<AddObjectEvent>,
        mut actions: MessageWriter<GeometryActionEvent>,
        gui_icons: Res<GuiIcons>,
        tool_icons: Res<ToolIcons>,
        gear_settings: Res<GearSettings>,
        physical_objects: Query<&Collider, (With<PhysicalGeometry>, Without<PlaneObject>)>,
        freeform_objects: Query<(), With<FreeformObject>>,
        springs: Query<&SpringObject>,
        joints: Query<&JointGeometry>,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, parent, target, mut initial_pos) in wnds.iter_mut() {
            let bodies = window_target_entities(target, parent)
                .into_iter()
                .filter(|entity| physical_objects.contains(*entity))
                .collect::<HashSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            if bodies.is_empty() {
                commands.entity(id).despawn();
                continue;
            }

            let contains_box_or_polygon = bodies.iter().any(|entity| {
                freeform_objects.contains(*entity)
                    || physical_objects.get(*entity).is_ok_and(collider_is_box)
            });
            let contains_circle_or_polygon = bodies.iter().any(|entity| {
                freeform_objects.contains(*entity)
                    || physical_objects.get(*entity).is_ok_and(collider_is_circle)
            });
            let can_gearify = bodies.iter().any(|entity| {
                freeform_objects.contains(*entity)
                    || physical_objects.get(*entity).is_ok_and(|collider| {
                        collider_is_box(collider) || collider_is_circle(collider)
                    })
            });
            let can_loosen = bodies
                .iter()
                .copied()
                .any(|entity| object_has_attachment(entity, &springs, &joints));

            egui::Window::new(window_title(target, "Geom actions"))
                .resizable(false)
                .default_size(egui::Vec2::ZERO)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _commands| {
                    if action_button(ui, tool_icons.egui_icon_fixjoint, "Glue to background") {
                        actions.write(GeometryActionEvent::GlueToBackground(bodies.clone()));
                    }
                    if bodies.len() >= 2
                        && action_button(ui, tool_icons.egui_icon_fixjoint, "Glue together")
                    {
                        actions.write(GeometryActionEvent::GlueTogether(bodies.clone()));
                    }
                    if can_loosen && action_button(ui, gui_icons.loosen, "Loosen") {
                        actions.write(GeometryActionEvent::Loosen(bodies.clone()));
                    }
                    if action_button(ui, tool_icons.egui_icon_hinge, "Add center axle") {
                        for entity in bodies.iter().copied() {
                            add_obj.write(AddObjectEvent::Axle(AddAxleEvent::AddCenter(entity)));
                        }
                    }
                    if action_button(ui, tool_icons.egui_icon_thruster, "Add center thruster") {
                        for entity in bodies.iter().copied() {
                            add_obj.write(AddObjectEvent::CenterThruster(entity));
                        }
                    }
                    if action_button(ui, tool_icons.egui_icon_tracer, "Attach tracer") {
                        for entity in bodies.iter().copied() {
                            add_obj.write(AddObjectEvent::CenterTracer(entity));
                        }
                    }
                    if can_gearify && action_button(ui, gui_icons.gearify, "Gearify") {
                        actions.write(GeometryActionEvent::Gearify {
                            targets: bodies.clone(),
                            teeth_size: gear_settings.teeth_size,
                        });
                    }
                    if contains_box_or_polygon
                        && action_button(ui, tool_icons.egui_icon_circle, "Transform into circle")
                    {
                        actions.write(GeometryActionEvent::ShapesToCircles(bodies.clone()));
                    }
                    if contains_circle_or_polygon
                        && action_button(ui, tool_icons.egui_icon_box, "Transform into box")
                    {
                        actions.write(GeometryActionEvent::ShapesToBoxes(bodies.clone()));
                    }
                });
        }
    }
}

fn action_button(ui: &mut egui::Ui, icon: egui::TextureId, text: &str) -> bool {
    ui.add(
        egui::Button::image_and_text(SizedTexture::new(icon, [16.0, 16.0]), text)
            .wrap_mode(egui::TextWrapMode::Extend),
    )
    .clicked()
}

fn object_has_attachment(
    entity: Entity,
    springs: &Query<&SpringObject>,
    joints: &Query<&JointGeometry>,
) -> bool {
    springs.iter().any(|spring| spring_mentions(spring, entity))
        || joints
            .iter()
            .any(|joint| joint.geoms.contains(&Some(entity)))
}

fn spring_mentions(spring: &SpringObject, entity: Entity) -> bool {
    matches!(spring.end_a, SpringEnd::Body { entity: body, .. } if body == entity)
        || matches!(spring.end_b, SpringEnd::Body { entity: body, .. } if body == entity)
}

#[allow(clippy::too_many_arguments)]
fn process_geometry_actions(
    mut events: MessageReader<GeometryActionEvent>,
    mut commands: Commands,
    scene_state: Res<SceneState>,
    bodies: Query<(&Position, &Rotation), With<PhysicalGeometry>>,
    springs: Query<(Entity, &SpringObject)>,
    joints: Query<(Entity, &JointGeometry, Option<&AttachmentLinks>)>,
    mut geometry: Query<
        (
            &mut Collider,
            &mut Shape,
            &mut CircleVisual,
            Has<FreeformObject>,
        ),
        Without<PlaneObject>,
    >,
) {
    for event in events.read() {
        match event {
            GeometryActionEvent::GlueToBackground(targets) => {
                for entity in unique_valid_targets(targets, &bodies) {
                    spawn_background_glue(
                        &mut commands,
                        scene_state.scene,
                        scene_state.sky,
                        entity,
                        &bodies,
                    );
                }
            }
            GeometryActionEvent::GlueTogether(targets) => {
                let targets = unique_valid_targets(targets, &bodies);
                if let Some((&first, rest)) = targets.split_first() {
                    for other in rest.iter().copied() {
                        spawn_body_glue(&mut commands, scene_state.scene, first, other, &bodies);
                    }
                }
            }
            GeometryActionEvent::Loosen(targets) => {
                loosen_objects(targets, &mut commands, &springs, &joints)
            }
            GeometryActionEvent::Gearify {
                targets,
                teeth_size,
            } => {
                for entity in targets.iter().copied() {
                    if let Ok((mut collider, mut shape, mut circle, is_polygon)) =
                        geometry.get_mut(entity)
                        && (is_polygon
                            || collider_is_box(&collider)
                            || collider_is_circle(&collider))
                        && gearify_geometry(&mut collider, &mut shape, &mut circle, *teeth_size)
                    {
                        commands.entity(entity).insert(FreeformObject);
                    }
                }
            }
            GeometryActionEvent::ShapesToCircles(targets) => {
                for entity in targets.iter().copied() {
                    if let Ok((mut collider, mut shape, mut circle, is_polygon)) =
                        geometry.get_mut(entity)
                        && (is_polygon || collider_is_box(&collider))
                        && transform_to_circle(&mut collider, &mut shape, &mut circle)
                        && is_polygon
                    {
                        commands.entity(entity).remove::<FreeformObject>();
                    }
                }
            }
            GeometryActionEvent::ShapesToBoxes(targets) => {
                for entity in targets.iter().copied() {
                    if let Ok((mut collider, mut shape, mut circle, is_polygon)) =
                        geometry.get_mut(entity)
                        && (is_polygon || collider_is_circle(&collider))
                        && transform_to_box(&mut collider, &mut shape, &mut circle)
                        && is_polygon
                    {
                        commands.entity(entity).remove::<FreeformObject>();
                    }
                }
            }
        }
    }
}

fn unique_valid_targets(
    targets: &[Entity],
    bodies: &Query<(&Position, &Rotation), With<PhysicalGeometry>>,
) -> Vec<Entity> {
    let mut seen = HashSet::new();
    targets
        .iter()
        .copied()
        .filter(|entity| seen.insert(*entity) && bodies.contains(*entity))
        .collect()
}

fn spawn_background_glue(
    commands: &mut Commands,
    scene: Entity,
    _sky: Entity,
    body: Entity,
    bodies: &Query<(&Position, &Rotation), With<PhysicalGeometry>>,
) {
    let Ok((position, _)) = bodies.get(body) else {
        return;
    };
    commands.spawn((
        FixObject,
        VirtualFixpoint,
        JointGeometry {
            geoms: [Some(body), None],
            positions: [Vec2::ZERO, position.0],
        },
        ChildOf(scene),
    ));
}

fn spawn_body_glue(
    commands: &mut Commands,
    scene: Entity,
    body1: Entity,
    body2: Entity,
    bodies: &Query<(&Position, &Rotation), With<PhysicalGeometry>>,
) {
    let Ok((position, _)) = bodies.get(body1) else {
        return;
    };
    let Ok((position2, rotation2)) = bodies.get(body2) else {
        return;
    };
    commands.spawn((
        FixObject,
        VirtualFixpoint,
        JointGeometry {
            geoms: [Some(body1), Some(body2)],
            positions: [Vec2::ZERO, rotation2.inverse() * (position.0 - position2.0)],
        },
        ChildOf(scene),
    ));
}

fn loosen_objects(
    targets: &[Entity],
    commands: &mut Commands,
    springs: &Query<(Entity, &SpringObject)>,
    joints: &Query<(Entity, &JointGeometry, Option<&AttachmentLinks>)>,
) {
    let targets = targets.iter().copied().collect::<HashSet<_>>();

    for (spring_entity, spring) in springs.iter() {
        if targets
            .iter()
            .any(|target| spring_mentions(spring, *target))
        {
            commands.entity(spring_entity).despawn();
        }
    }

    for (joint, geometry, links) in joints.iter() {
        if !geometry
            .geoms
            .iter()
            .flatten()
            .any(|entity| targets.contains(entity))
        {
            continue;
        }
        despawn_attachment_links(commands, links);
        commands.entity(joint).despawn();
    }
}

fn collider_is_box(collider: &Collider) -> bool {
    matches!(collider.shape().as_typed_shape(), TypedShape::Cuboid(_))
}

fn collider_is_circle(collider: &Collider) -> bool {
    matches!(collider.shape().as_typed_shape(), TypedShape::Ball(_))
}

fn gearify_geometry(
    collider: &mut Collider,
    shape: &mut Shape,
    circle: &mut CircleVisual,
    teeth_size: f32,
) -> bool {
    let path = match collider.shape().as_typed_shape() {
        // Preserve exact visual equivalence with the radial gear tool. Only
        // tooth size applies to Gearify; existing holes are handled below.
        TypedShape::Ball(ball) => GearOutline::from_radius(
            ball.radius,
            GearSettings {
                teeth_size,
                external: true,
                internal: false,
                ..Default::default()
            },
        )
        .map(|outline| outline.path()),
        _ => gearify_path(&shape.path, teeth_size),
    };
    let Some(path) = path else {
        return false;
    };
    let Some(geometry) = tessellate_path(&path) else {
        return false;
    };

    *collider = geometry.collider();
    shape.path = path;
    circle.0 = 0.0;
    true
}

fn transform_to_circle(
    collider: &mut Collider,
    shape: &mut Shape,
    circle: &mut CircleVisual,
) -> bool {
    let area = collider.shape().mass_properties(1.0).mass();
    if !area.is_finite() || area <= 0.0 {
        return false;
    }
    let radius = (area / PI).sqrt();
    set_circle_geometry(collider, shape, circle, radius);
    true
}

fn transform_to_box(collider: &mut Collider, shape: &mut Shape, circle: &mut CircleVisual) -> bool {
    let area = collider.shape().mass_properties(1.0).mass();
    if !area.is_finite() || area <= 0.0 {
        return false;
    }
    let side = area.sqrt();
    set_box_geometry(collider, shape, Vec2::splat(side));
    circle.0 = 0.0;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyon_compat::{GeometryBuilder, shapes};

    fn geometry_action_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<SceneState>()
            .init_resource::<avian2d::dynamics::solver::joint_graph::JointGraph>()
            .add_message::<GeometryActionEvent>()
            .add_systems(Update, process_geometry_actions);
        app
    }

    fn spawn_body(app: &mut App, position: Vec2) -> Entity {
        let scene = app.world().resource::<SceneState>().scene;
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let entity = crate::objects::phy_obj::PhysicalObject::ball(1.0, position.extend(0.0))
            .spawn(&mut Commands::new(&mut queue, app.world()), scene);
        queue.apply(app.world_mut());
        entity
    }

    #[test]
    fn box_to_circle_preserves_area() {
        let mut collider = Collider::rectangle(8.0, 2.0);
        let mut shape = Shape::default();
        let mut circle = CircleVisual(0.0);

        assert!(transform_to_circle(&mut collider, &mut shape, &mut circle,));
        let TypedShape::Ball(ball) = collider.shape().as_typed_shape() else {
            panic!("box was not transformed into a circle");
        };
        assert!((PI * ball.radius.powi(2) - 16.0).abs() < 1.0e-5);
        assert_eq!(circle.0, ball.radius);
    }

    #[test]
    fn circle_to_square_preserves_area() {
        let mut collider = Collider::circle(3.0);
        let mut shape = Shape::default();
        let mut circle = CircleVisual(3.0);

        assert!(transform_to_box(&mut collider, &mut shape, &mut circle,));
        let TypedShape::Cuboid(square) = collider.shape().as_typed_shape() else {
            panic!("circle was not transformed into a box");
        };
        let side = square.half_extents.x * 2.0;
        assert!((side.powi(2) - PI * 9.0).abs() < 1.0e-5);
        assert_eq!(square.half_extents.x, square.half_extents.y);
        assert_eq!(circle.0, 0.0);
    }

    #[test]
    fn gearify_geometry_changes_a_repeated_figure_eight_polygon() {
        let source = crate::tools::polygon::polygon_path(
            &[
                Vec2::new(-2.0, 0.0),
                Vec2::new(-1.0, 1.0),
                Vec2::ZERO,
                Vec2::new(-1.0, -1.0),
                Vec2::new(-2.0, 0.0),
                Vec2::ZERO,
                Vec2::new(1.0, 1.0),
                Vec2::new(2.0, 0.0),
                Vec2::new(1.0, -1.0),
                Vec2::ZERO,
            ],
            true,
        );
        let source_events = source.iter().count();
        let geometry = tessellate_path(&source).unwrap();
        let mut collider = geometry.collider();
        let mut shape = Shape {
            path: source.clone(),
        };
        let mut circle = CircleVisual(0.0);

        assert!(gearify_geometry(
            &mut collider,
            &mut shape,
            &mut circle,
            0.2,
        ));
        assert!(shape.path.iter().count() > source_events);
        assert!(matches!(
            collider.shape().as_typed_shape(),
            TypedShape::Compound(_)
        ));
    }

    #[test]
    fn gearified_circle_matches_a_gear_drawn_at_the_same_radius() {
        let radius = 1.0;
        let settings = GearSettings::default();
        let expected = GearOutline::from_radius(radius, settings).unwrap().path();
        let mut collider = Collider::circle(radius);
        let mut shape = Shape {
            path: GeometryBuilder::build_as(&shapes::Circle {
                radius,
                ..Default::default()
            }),
        };
        let mut circle = CircleVisual(radius);

        assert!(gearify_geometry(
            &mut collider,
            &mut shape,
            &mut circle,
            settings.teeth_size,
        ));
        let actual_segments = path_segments(&shape.path);
        let expected_segments = path_segments(&expected);
        assert_eq!(actual_segments.len(), expected_segments.len());
        for (actual, expected) in actual_segments.iter().zip(&expected_segments) {
            assert!(actual.0.distance_squared(expected.0) < 1.0e-10);
            assert!(actual.1.distance_squared(expected.1) < 1.0e-10);
        }
        assert_eq!(circle.0, 0.0);
    }

    #[test]
    fn gearify_action_converts_a_circle_to_a_freeform_collider() {
        let mut app = geometry_action_app();
        let radius = 1.0;
        let entity = app
            .world_mut()
            .spawn((
                RigidBody::Dynamic,
                Position::default(),
                Rotation::default(),
                Collider::circle(radius),
                Shape {
                    path: GeometryBuilder::build_as(&shapes::Circle {
                        radius,
                        ..Default::default()
                    }),
                },
                CircleVisual(radius),
            ))
            .id();

        app.world_mut().write_message(GeometryActionEvent::Gearify {
            targets: vec![entity],
            teeth_size: 0.2,
        });
        app.update();

        assert!(app.world().get::<FreeformObject>(entity).is_some());
        assert_eq!(app.world().get::<CircleVisual>(entity).unwrap().0, 0.0);
        assert!(!collider_is_circle(
            app.world().get::<Collider>(entity).unwrap()
        ));
    }

    #[test]
    fn parry_computes_area_without_manual_shape_cases() {
        let mut collider =
            Collider::convex_hull(vec![Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0), Vec2::Y])
                .unwrap();
        let mut shape = Shape::default();
        let mut circle = CircleVisual(0.0);

        assert!(transform_to_circle(&mut collider, &mut shape, &mut circle,));
        let TypedShape::Ball(ball) = collider.shape().as_typed_shape() else {
            panic!("convex shape was not transformed into a circle");
        };
        assert!((PI * ball.radius.powi(2) - 1.0).abs() < 1.0e-5);

        assert!(transform_to_box(&mut collider, &mut shape, &mut circle,));
        let TypedShape::Cuboid(square) = collider.shape().as_typed_shape() else {
            panic!("circle was not transformed into a box");
        };
        assert!(((square.half_extents.x * 2.0).powi(2) - 1.0).abs() < 1.0e-5);
    }

    #[test]
    fn polygon_transforms_preserve_area_and_remove_polygon_marker() {
        let mut app = geometry_action_app();
        let expected_area = 3.0;
        let polygon = crate::tools::polygon::tessellate_polygon(&[
            Vec2::ZERO,
            Vec2::new(2.0, 0.0),
            Vec2::new(2.0, 2.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 2.0),
        ])
        .unwrap();
        let entity = app
            .world_mut()
            .spawn((
                RigidBody::Dynamic,
                Position::default(),
                Rotation::default(),
                polygon.collider(),
                Shape::default(),
                CircleVisual(0.0),
                FreeformObject,
            ))
            .id();

        app.world_mut()
            .write_message(GeometryActionEvent::ShapesToCircles(vec![entity]));
        app.update();

        let collider = app.world().get::<Collider>(entity).unwrap();
        let TypedShape::Ball(circle) = collider.shape().as_typed_shape() else {
            panic!("polygon was not transformed into a circle");
        };
        assert!((PI * circle.radius.powi(2) - expected_area).abs() < 1.0e-5);
        assert!(app.world().get::<FreeformObject>(entity).is_none());
    }

    #[test]
    fn polygon_can_transform_directly_into_an_equal_area_square() {
        let polygon = crate::tools::polygon::tessellate_polygon(&[
            Vec2::ZERO,
            Vec2::new(2.0, 0.0),
            Vec2::new(2.0, 2.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 2.0),
        ])
        .unwrap();
        let expected_area = polygon.area;
        let mut collider = polygon.collider();
        let mut shape = Shape::default();
        let mut circle = CircleVisual(0.0);

        assert!(transform_to_box(&mut collider, &mut shape, &mut circle,));
        let TypedShape::Cuboid(square) = collider.shape().as_typed_shape() else {
            panic!("polygon was not transformed into a box");
        };
        let side = square.half_extents.x * 2.0;
        assert!((side.powi(2) - expected_area).abs() < 1.0e-5);
    }

    #[test]
    fn background_glue_reuses_the_sky_body_and_loosen_preserves_it() {
        let mut app = geometry_action_app();
        let body = spawn_body(&mut app, Vec2::new(2.0, 3.0));
        let sky = app.world().resource::<SceneState>().sky;

        app.world_mut()
            .write_message(GeometryActionEvent::GlueToBackground(vec![body]));
        app.update();

        let joint_entity = {
            let world = app.world_mut();
            let mut fixes = world.query::<(Entity, &JointGeometry, &VirtualFixpoint)>();
            let (joint_entity, joint, _) = fixes.single(world).unwrap();
            assert_eq!(joint.geoms, [Some(body), None]);
            joint_entity
        };

        app.world_mut()
            .write_message(GeometryActionEvent::Loosen(vec![body]));
        app.update();

        assert!(app.world().get_entity(body).is_ok());
        assert!(app.world().get_entity(joint_entity).is_err());
        assert!(app.world().get_entity(sky).is_ok());
    }

    fn path_segments(path: &bevy_prototype_lyon::prelude::tess::path::Path) -> Vec<(Vec2, Vec2)> {
        path.iter()
            .map(|event| {
                (
                    Vec2::new(event.from().x, event.from().y),
                    Vec2::new(event.to().x, event.to().y),
                )
            })
            .collect()
    }

    #[test]
    fn glue_together_connects_every_selected_body() {
        let mut app = geometry_action_app();
        let bodies = [
            spawn_body(&mut app, Vec2::ZERO),
            spawn_body(&mut app, Vec2::X),
            spawn_body(&mut app, Vec2::Y),
        ];

        app.world_mut()
            .write_message(GeometryActionEvent::GlueTogether(bodies.to_vec()));
        app.update();

        let world = app.world_mut();
        let mut fixes = world.query::<(&JointGeometry, &VirtualFixpoint)>();
        let fixes = fixes.iter(world).collect::<Vec<_>>();
        assert_eq!(fixes.len(), bodies.len() - 1);
        assert!(
            fixes
                .iter()
                .all(|(joint, _)| joint.geoms[0] == Some(bodies[0]))
        );
    }

    #[test]
    fn loosen_removes_springs_and_visible_joint_attachments() {
        let mut app = geometry_action_app();
        let scene = app.world().resource::<SceneState>().scene;
        let body = spawn_body(&mut app, Vec2::ZERO);
        let other = spawn_body(&mut app, Vec2::X);
        let spring = app
            .world_mut()
            .spawn(SpringObject::placement(
                SpringEnd::Body {
                    entity: body,
                    local_anchor: Vec2::ZERO,
                },
                SpringEnd::Sky {
                    world_anchor: Vec2::X,
                },
                1.0,
                1.0,
            ))
            .id();
        let visual = app
            .world_mut()
            .spawn((
                AttachmentLinks::default(),
                JointGeometry {
                    geoms: [Some(body), Some(other)],
                    positions: [Vec2::ZERO; 2],
                },
                FixObject,
                ChildOf(scene),
            ))
            .id();

        app.world_mut()
            .write_message(GeometryActionEvent::Loosen(vec![body]));
        app.update();

        assert!(app.world().get_entity(body).is_ok());
        assert!(app.world().get_entity(other).is_ok());
        assert!(app.world().get_entity(spring).is_err());
        assert!(app.world().get_entity(visual).is_err());
    }
}
