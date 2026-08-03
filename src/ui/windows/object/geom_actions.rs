use std::collections::HashSet;
use std::f32::consts::PI;

use crate::lyon_compat::{GeometryBuilder, RectangleOrigin, Shape, shapes};
use crate::objects::phy_obj::{CircleVisual, CollisionOutline, FreeformObject};
use crate::objects::plane::PlaneObject;
use crate::objects::spring::{SpringEnd, SpringObject};
use crate::tools::ToolIcons;
use crate::tools::add_object::{
    AddAxleEvent, AddObjectEvent, AttachmentJoint, AttachmentLinks, despawn_attachment_links,
};
use crate::ui::images::GuiIcons;
use crate::ui::{
    InitialPos, SceneState, Subwindow, WindowSelectionTarget, window_target_entities,
};
use avian2d::parry::shape::TypedShape;
use avian2d::prelude::*;
use bevy::prelude::*;
use bevy_egui::egui::load::SizedTexture;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

pub fn add_systems(app: &mut App) {
    app.add_message::<GeometryActionEvent>()
        .add_systems(EguiPrimaryContextPass, GeometryActionsWindow::show)
        .add_systems(Update, process_geometry_actions);
}

#[derive(Default, Component)]
pub struct GeometryActionsWindow;

#[derive(Clone, Debug, Message)]
enum GeometryActionEvent {
    GlueToBackground(Vec<Entity>),
    GlueTogether(Vec<Entity>),
    Loosen(Vec<Entity>),
    ShapesToCircles(Vec<Entity>),
    ShapesToBoxes(Vec<Entity>),
}

/// Marks a fixed joint created by a glue action. Unlike an ordinary fixpoint,
/// this joint deliberately has no selectable or rendered visual entity.
#[derive(Copy, Clone, Debug, Component)]
struct VirtualFixpoint {
    sky_anchor: Option<Entity>,
}

type JointData<'a> = (
    Entity,
    Option<&'a AttachmentJoint>,
    Option<&'a FixedJoint>,
    Option<&'a RevoluteJoint>,
    Option<&'a VirtualFixpoint>,
);
type JointFilter = Or<(With<FixedJoint>, With<RevoluteJoint>)>;

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
        physical_objects: Query<&Collider, (With<RigidBody>, Without<PlaneObject>)>,
        freeform_objects: Query<(), With<FreeformObject>>,
        springs: Query<&SpringObject>,
        fixed_joints: Query<&FixedJoint>,
        revolute_joints: Query<&RevoluteJoint>,
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
                    || physical_objects
                        .get(*entity)
                        .is_ok_and(collider_is_box)
            });
            let contains_circle_or_polygon = bodies.iter().any(|entity| {
                freeform_objects.contains(*entity)
                    || physical_objects
                        .get(*entity)
                        .is_ok_and(collider_is_circle)
            });
            let can_loosen = bodies.iter().copied().any(|entity| {
                object_has_attachment(entity, &springs, &fixed_joints, &revolute_joints)
            });

            egui::Window::new("Geom actions")
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
                    if contains_box_or_polygon
                        && action_button(
                            ui,
                            tool_icons.egui_icon_circle,
                            "Transform into circle",
                        )
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
    fixed_joints: &Query<&FixedJoint>,
    revolute_joints: &Query<&RevoluteJoint>,
) -> bool {
    springs
        .iter()
        .any(|spring| spring_mentions(spring, entity))
        || fixed_joints
            .iter()
            .any(|joint| joint.body1 == entity || joint.body2 == entity)
        || revolute_joints
            .iter()
            .any(|joint| joint.body1 == entity || joint.body2 == entity)
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
    bodies: Query<(&Position, &Rotation), With<RigidBody>>,
    springs: Query<(Entity, &SpringObject)>,
    joints: Query<JointData, JointFilter>,
    attachment_links: Query<&AttachmentLinks>,
    mut geometry: Query<
        (&mut Collider, &mut Shape, &mut CircleVisual, Has<FreeformObject>),
        Without<PlaneObject>,
    >,
) {
    for event in events.read() {
        match event {
            GeometryActionEvent::GlueToBackground(targets) => {
                for entity in unique_valid_targets(targets, &bodies) {
                    spawn_background_glue(&mut commands, scene_state.scene, entity, &bodies);
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
            GeometryActionEvent::Loosen(targets) => loosen_objects(
                targets,
                &mut commands,
                &springs,
                &joints,
                &attachment_links,
            ),
            GeometryActionEvent::ShapesToCircles(targets) => {
                for entity in targets.iter().copied() {
                    if let Ok((mut collider, mut shape, mut circle, is_polygon)) =
                        geometry.get_mut(entity)
                        && (is_polygon || collider_is_box(&collider))
                        && transform_to_circle(&mut collider, &mut shape, &mut circle)
                        && is_polygon
                    {
                        commands
                            .entity(entity)
                            .remove::<(FreeformObject, CollisionOutline)>();
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
                        commands
                            .entity(entity)
                            .remove::<(FreeformObject, CollisionOutline)>();
                    }
                }
            }
        }
    }
}

fn unique_valid_targets(
    targets: &[Entity],
    bodies: &Query<(&Position, &Rotation), With<RigidBody>>,
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
    body: Entity,
    bodies: &Query<(&Position, &Rotation), With<RigidBody>>,
) {
    let Ok((position, _)) = bodies.get(body) else {
        return;
    };
    let sky_anchor = commands
        .spawn((
            RigidBody::Kinematic,
            *position,
            Transform::from_translation(position.0.extend(0.0)),
            ChildOf(scene),
        ))
        .id();
    commands.spawn((
        VirtualFixpoint {
            sky_anchor: Some(sky_anchor),
        },
        FixedJoint::new(body, sky_anchor)
            .with_anchor(position.0)
            .with_basis(Rotation::default()),
        JointCollisionDisabled,
        ChildOf(scene),
    ));
}

fn spawn_body_glue(
    commands: &mut Commands,
    scene: Entity,
    body1: Entity,
    body2: Entity,
    bodies: &Query<(&Position, &Rotation), With<RigidBody>>,
) {
    let Ok((position, _)) = bodies.get(body1) else {
        return;
    };
    if !bodies.contains(body2) {
        return;
    }
    commands.spawn((
        VirtualFixpoint { sky_anchor: None },
        FixedJoint::new(body1, body2)
            .with_anchor(position.0)
            .with_basis(Rotation::default()),
        JointCollisionDisabled,
        ChildOf(scene),
    ));
}

fn loosen_objects(
    targets: &[Entity],
    commands: &mut Commands,
    springs: &Query<(Entity, &SpringObject)>,
    joints: &Query<JointData, JointFilter>,
    attachment_links: &Query<&AttachmentLinks>,
) {
    let targets = targets.iter().copied().collect::<HashSet<_>>();

    for (spring_entity, spring) in springs.iter() {
        if targets.iter().any(|target| spring_mentions(spring, *target)) {
            commands.entity(spring_entity).despawn();
        }
    }

    let mut removed_visuals = HashSet::new();
    for (joint_entity, attachment, fixed, revolute, virtual_fix) in joints.iter() {
        let attached = fixed
            .map(|joint| targets.contains(&joint.body1) || targets.contains(&joint.body2))
            .unwrap_or(false)
            || revolute
                .map(|joint| targets.contains(&joint.body1) || targets.contains(&joint.body2))
                .unwrap_or(false);
        if !attached {
            continue;
        }

        if let Some(attachment) = attachment {
            if !removed_visuals.insert(attachment.visual) {
                continue;
            }
            if let Ok(links) = attachment_links.get(attachment.visual) {
                despawn_attachment_links(commands, Some(links));
            } else {
                commands.entity(joint_entity).despawn();
            }
            commands.entity(attachment.visual).despawn();
        } else {
            commands.entity(joint_entity).despawn();
            if let Some(anchor) = virtual_fix.and_then(|fix| fix.sky_anchor) {
                commands.entity(anchor).despawn();
            }
        }
    }
}

fn collider_is_box(collider: &Collider) -> bool {
    matches!(collider.shape().as_typed_shape(), TypedShape::Cuboid(_))
}

fn collider_is_circle(collider: &Collider) -> bool {
    matches!(collider.shape().as_typed_shape(), TypedShape::Ball(_))
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
    *collider = Collider::circle(radius);
    shape.path = GeometryBuilder::build_as(&shapes::Circle {
        radius: (radius - crate::BORDER_THICKNESS * 0.5).max(radius * 0.5),
        ..Default::default()
    });
    circle.0 = radius;
    true
}

fn transform_to_box(
    collider: &mut Collider,
    shape: &mut Shape,
    circle: &mut CircleVisual,
) -> bool {
    let area = collider.shape().mass_properties(1.0).mass();
    if !area.is_finite() || area <= 0.0 {
        return false;
    }
    let side = area.sqrt();
    *collider = Collider::rectangle(side, side);
    shape.path = GeometryBuilder::build_as(&shapes::Rectangle {
        extents: (Vec2::splat(side) - Vec2::splat(crate::BORDER_THICKNESS))
            .max(Vec2::splat(f32::EPSILON)),
        origin: RectangleOrigin::Center,
        radii: None,
    });
    circle.0 = 0.0;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

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
        app.world_mut()
            .spawn((RigidBody::Dynamic, Position(position), Rotation::default()))
            .id()
    }

    #[test]
    fn box_to_circle_preserves_area() {
        let mut collider = Collider::rectangle(8.0, 2.0);
        let mut shape = Shape::default();
        let mut circle = CircleVisual(0.0);

        assert!(transform_to_circle(
            &mut collider,
            &mut shape,
            &mut circle,
        ));
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

        assert!(transform_to_box(
            &mut collider,
            &mut shape,
            &mut circle,
        ));
        let TypedShape::Cuboid(square) = collider.shape().as_typed_shape() else {
            panic!("circle was not transformed into a box");
        };
        let side = square.half_extents.x * 2.0;
        assert!((side.powi(2) - PI * 9.0).abs() < 1.0e-5);
        assert_eq!(square.half_extents.x, square.half_extents.y);
        assert_eq!(circle.0, 0.0);
    }

    #[test]
    fn parry_computes_area_without_manual_shape_cases() {
        let mut collider = Collider::convex_hull(vec![
            Vec2::new(-1.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::Y,
        ])
        .unwrap();
        let mut shape = Shape::default();
        let mut circle = CircleVisual(0.0);

        assert!(transform_to_circle(
            &mut collider,
            &mut shape,
            &mut circle,
        ));
        let TypedShape::Ball(ball) = collider.shape().as_typed_shape() else {
            panic!("convex shape was not transformed into a circle");
        };
        assert!((PI * ball.radius.powi(2) - 1.0).abs() < 1.0e-5);

        assert!(transform_to_box(
            &mut collider,
            &mut shape,
            &mut circle,
        ));
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

        assert!(transform_to_box(
            &mut collider,
            &mut shape,
            &mut circle,
        ));
        let TypedShape::Cuboid(square) = collider.shape().as_typed_shape() else {
            panic!("polygon was not transformed into a box");
        };
        let side = square.half_extents.x * 2.0;
        assert!((side.powi(2) - expected_area).abs() < 1.0e-5);
    }

    #[test]
    fn background_glue_is_invisible_and_loosen_removes_it_and_its_anchor() {
        let mut app = geometry_action_app();
        let body = spawn_body(&mut app, Vec2::new(2.0, 3.0));

        app.world_mut()
            .write_message(GeometryActionEvent::GlueToBackground(vec![body]));
        app.update();

        let (joint_entity, anchor) = {
            let world = app.world_mut();
            let mut fixes = world.query::<(Entity, &FixedJoint, &VirtualFixpoint)>();
            let (joint_entity, joint, virtual_fix) = fixes.single(world).unwrap();
            assert_eq!(joint.body1, body);
            (joint_entity, virtual_fix.sky_anchor.unwrap())
        };
        assert!(app.world().get::<AttachmentJoint>(joint_entity).is_none());

        app.world_mut()
            .write_message(GeometryActionEvent::Loosen(vec![body]));
        app.update();

        assert!(app.world().get_entity(body).is_ok());
        assert!(app.world().get_entity(joint_entity).is_err());
        assert!(app.world().get_entity(anchor).is_err());
    }

    #[test]
    fn glue_together_connects_every_selected_body_without_sky_anchors() {
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
        let mut fixes = world.query::<(&FixedJoint, &VirtualFixpoint)>();
        let fixes = fixes.iter(world).collect::<Vec<_>>();
        assert_eq!(fixes.len(), bodies.len() - 1);
        assert!(fixes.iter().all(|(_, fix)| fix.sky_anchor.is_none()));
        assert!(fixes.iter().all(|(joint, _)| joint.body1 == bodies[0]));
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
        let visual = app.world_mut().spawn(AttachmentLinks::default()).id();
        let joint = app
            .world_mut()
            .spawn((
                AttachmentJoint { visual },
                FixedJoint::new(body, other),
                ChildOf(scene),
            ))
            .id();
        let raw_hinge = app
            .world_mut()
            .spawn((RevoluteJoint::new(body, other), ChildOf(scene)))
            .id();
        app.world_mut().entity_mut(visual).insert(AttachmentLinks {
            joint: Some(joint),
            sky_anchor: None,
        });

        app.world_mut()
            .write_message(GeometryActionEvent::Loosen(vec![body]));
        app.update();

        assert!(app.world().get_entity(body).is_ok());
        assert!(app.world().get_entity(other).is_ok());
        assert!(app.world().get_entity(spring).is_err());
        assert!(app.world().get_entity(visual).is_err());
        assert!(app.world().get_entity(joint).is_err());
        assert!(app.world().get_entity(raw_hinge).is_err());
    }
}
