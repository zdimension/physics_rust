use crate::egui_systems;
use crate::lyon_compat::Shape;
use crate::objects::body::{self, BodyTransform};
use crate::objects::phy_obj::{CircleVisual, FreeformObject, PhysicalGeometry, PhysicalObject};
use crate::objects::plane::PlaneObject;
use crate::tools::polygon::{surfaces_path, tessellate_path};
use crate::ui::images::GuiIcons;
use crate::ui::{
    InitialPos, Subwindow, WindowSelectionTarget, window_matching_entities, window_title,
};
use avian2d::prelude::Collider;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_prototype_lyon::prelude::tess::path::{Path, PathEvent, iterator::PathIterator};
use i_overlay::core::{fill_rule::FillRule, overlay_rule::OverlayRule};
use i_overlay::float::{relate::FloatRelate, single::SingleFloatOverlay, slice::FloatSlice};

use super::geom_actions::action_button;

type Contour = Vec<[f32; 2]>;
type ShapeData = Vec<Contour>;
type Shapes = Vec<ShapeData>;

const TOLERANCE: f32 = 0.001;

#[derive(Default, Component)]
pub struct CombineShapesWindow;

egui_systems!(CombineShapesWindow::show);

#[derive(Clone, Copy)]
enum Operation {
    Cut,
    Intersect,
    Subtract,
    Add,
}

#[derive(Clone)]
struct Geometry {
    entity: Entity,
    contours: ShapeData,
}

fn contours(path: &Path, pose: (Vec2, avian2d::prelude::Rotation), scale: Vec2) -> ShapeData {
    let point = |point: bevy_prototype_lyon::prelude::tess::math::Point| {
        let point = body::world_point((pose.0, pose.1), Vec2::new(point.x, point.y) * scale);
        [point.x, point.y]
    };
    let mut contours = Vec::new();
    let mut contour = Vec::new();
    for event in path.iter().flattened(TOLERANCE) {
        match event {
            PathEvent::Begin { at } => contour.push(point(at)),
            PathEvent::Line { to, .. } => contour.push(point(to)),
            PathEvent::End { close: true, .. } if contour.len() >= 3 => {
                contours.push(std::mem::take(&mut contour));
            }
            PathEvent::End { .. } => contour.clear(),
            _ => unreachable!("flattened paths contain only lines"),
        }
    }
    contours
}

fn geometry(world: &World, entity: Entity) -> Option<Geometry> {
    Some(Geometry {
        entity,
        contours: contours(
            &world.get::<Shape>(entity)?.path,
            body::pose(world, entity)?,
            world.get::<BodyTransform>(entity)?.scale,
        ),
    })
}

fn union(mut shapes: impl Iterator<Item = ShapeData>) -> Shapes {
    let Some(first) = shapes.next() else {
        return Vec::new();
    };
    let empty = ShapeData::new();
    shapes.fold(
        first.overlay(&empty, OverlayRule::Subject, FillRule::EvenOdd),
        |result, shape| result.overlay(&shape, OverlayRule::Union, FillRule::EvenOdd),
    )
}

fn local_path(world: &World, entity: Entity, shape: &ShapeData) -> Option<Path> {
    let (pos, rotation) = body::pose(world, entity)?;
    let scale = world.get::<BodyTransform>(entity)?.scale;
    Some(surfaces_path(
        &shape
            .iter()
            .map(|contour| {
                contour
                    .iter()
                    .map(|&[x, y]| rotation.inverse() * (Vec2::new(x, y) - pos) / scale)
                    .collect()
            })
            .collect::<Vec<_>>(),
    ))
}

fn set_path(world: &mut World, entity: Entity, path: Path) -> bool {
    let Some(geometry) = tessellate_path(&path) else {
        return false;
    };
    let mut entity = world.entity_mut(entity);
    *entity.get_mut::<Collider>().unwrap() = geometry.collider();
    entity.get_mut::<Shape>().unwrap().path = path;
    entity.get_mut::<CircleVisual>().unwrap().0 = 0.0;
    entity.insert(FreeformObject);
    true
}

fn set_shape(world: &mut World, entity: Entity, shape: &ShapeData) -> bool {
    local_path(world, entity, shape).is_some_and(|path| set_path(world, entity, path))
}

fn replace(world: &mut World, target: Entity, shapes: Shapes) {
    let paths = shapes
        .iter()
        .filter_map(|shape| local_path(world, target, shape))
        .collect::<Vec<_>>();
    let Some((first, rest)) = paths.split_first() else {
        world.despawn(target);
        return;
    };
    for path in rest.iter().cloned() {
        PhysicalObject::fragment(world, target, path);
    }
    set_path(world, target, first.clone());
}

fn apply(world: &mut World, selected: Vec<Entity>, operation: Operation) {
    let cutters = selected
        .iter()
        .filter_map(|&entity| geometry(world, entity))
        .collect::<Vec<_>>();
    if cutters.is_empty() {
        return;
    }
    let selected = selected
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let targets = world
        .query_filtered::<Entity, (With<PhysicalGeometry>, Without<PlaneObject>)>()
        .iter(world)
        .filter(|entity| !selected.contains(entity))
        .filter_map(|entity| geometry(world, entity))
        .filter(|target| {
            cutters
                .iter()
                .any(|cutter| target.contours.interiors_intersect(&cutter.contours))
        })
        .collect::<Vec<_>>();
    if targets.is_empty() {
        return;
    }

    match operation {
        Operation::Cut => {
            let edges = cutters
                .iter()
                .flat_map(|cutter| cutter.contours.iter())
                .map(|contour| {
                    let mut closed = contour.clone();
                    closed.push(contour[0]);
                    closed
                })
                .collect::<Vec<_>>();
            for target in targets {
                let pieces = target.contours.slice_by(&edges, FillRule::EvenOdd);
                if pieces.len() > 1 {
                    replace(world, target.entity, pieces);
                }
            }
        }
        Operation::Intersect => {
            for target in targets {
                replace(
                    world,
                    target.entity,
                    cutters
                        .iter()
                        .flat_map(|cutter| {
                            target.contours.overlay(
                                &cutter.contours,
                                OverlayRule::Intersect,
                                FillRule::EvenOdd,
                            )
                        })
                        .collect(),
                );
            }
        }
        Operation::Subtract => {
            let cutters = union(cutters.into_iter().map(|cutter| cutter.contours));
            for target in targets {
                replace(
                    world,
                    target.entity,
                    target
                        .contours
                        .overlay(&cutters, OverlayRule::Difference, FillRule::EvenOdd),
                );
            }
        }
        Operation::Add => {
            let mut result = union(
                cutters
                    .into_iter()
                    .chain(targets.iter().cloned())
                    .map(|geometry| geometry.contours),
            );
            let source = targets[0].entity;
            let mut targets = targets.into_iter();
            for target in targets.by_ref() {
                if let Some(shape) = result.pop() {
                    set_shape(world, target.entity, &shape);
                } else {
                    world.despawn(target.entity);
                }
            }
            for shape in result {
                if let Some(path) = local_path(world, source, &shape) {
                    PhysicalObject::fragment(world, source, path);
                }
            }
        }
    }
}

impl CombineShapesWindow {
    fn show(
        mut windows: Query<
            (
                Entity,
                Option<&ChildOf>,
                Option<&WindowSelectionTarget>,
                &mut InitialPos,
            ),
            With<Self>,
        >,
        geometries: Query<(), (With<PhysicalGeometry>, Without<PlaneObject>)>,
        icons: Res<GuiIcons>,
        mut egui_ctx: EguiContexts,
        mut commands: Commands,
    ) {
        let ctx = egui_ctx.ctx_mut().expect("primary egui context");
        for (id, parent, target, mut initial_pos) in &mut windows {
            let selected = window_matching_entities(target, parent, &geometries);
            if selected.is_empty() {
                commands.entity(id).despawn();
                continue;
            }
            egui::Window::new(window_title(target, "Combine shapes"))
                .resizable(false)
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, commands| {
                    for (text, icon, operation) in [
                        ("Cut", icons.csg_cut, Operation::Cut),
                        ("Intersect", icons.csg_intersection, Operation::Intersect),
                        ("Subtract", icons.csg_difference, Operation::Subtract),
                        ("Add", icons.csg_union, Operation::Add),
                    ] {
                        if action_button(ui, icon, text) {
                            let selected = selected.clone();
                            commands
                                .queue(move |world: &mut World| apply(world, selected, operation));
                        }
                    }
                });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::add_object::DepthSorter;

    fn rectangle(world: &mut World, scene: Entity, min: Vec2, size: Vec2) -> Entity {
        let mut commands = world.commands();
        let entity = PhysicalObject::rect(size, min.extend(0.0)).spawn(&mut commands, scene);
        world.flush();
        entity
    }

    fn area(world: &World, entity: Entity) -> f32 {
        world
            .get::<Collider>(entity)
            .unwrap()
            .shape()
            .mass_properties(1.0)
            .mass()
    }

    fn geometries(world: &mut World) -> Vec<Entity> {
        world
            .query_filtered::<Entity, With<PhysicalGeometry>>()
            .iter(world)
            .collect()
    }

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<DepthSorter>();
        world
    }

    #[test]
    fn subtract_and_add_leave_the_cutter_intact() {
        for operation in [Operation::Subtract, Operation::Add] {
            let mut world = world();
            let scene = world.spawn_empty().id();
            let cutter = rectangle(&mut world, scene, Vec2::ZERO, Vec2::splat(2.0));
            let target = rectangle(&mut world, scene, Vec2::X, Vec2::splat(2.0));

            apply(&mut world, vec![cutter], operation);
            world.flush();

            assert!((area(&world, cutter) - 4.0).abs() < 1e-3);
            let expected = if matches!(operation, Operation::Add) {
                6.0
            } else {
                2.0
            };
            assert!((area(&world, target) - expected).abs() < 1e-3);
        }
    }

    #[test]
    fn cut_splits_the_target_into_independent_objects() {
        let mut world = world();
        let scene = world.spawn_empty().id();
        let cutter = rectangle(
            &mut world,
            scene,
            Vec2::new(-0.5, -2.0),
            Vec2::new(1.0, 4.0),
        );
        rectangle(
            &mut world,
            scene,
            Vec2::new(-2.0, -1.0),
            Vec2::new(4.0, 2.0),
        );

        apply(&mut world, vec![cutter], Operation::Cut);
        world.flush();

        let geometries = geometries(&mut world);
        assert_eq!(geometries.len(), 4); // cutter + three slices
        assert!(
            geometries
                .iter()
                .copied()
                .filter(|&entity| entity != cutter)
                .all(|entity| world.get::<FreeformObject>(entity).is_some())
        );
        let mut depths = geometries
            .iter()
            .copied()
            .filter(|&entity| entity != cutter)
            .map(|entity| world.get::<Transform>(entity).unwrap().translation.z)
            .collect::<Vec<_>>();
        depths.sort_by(f32::total_cmp);
        depths.dedup();
        assert_eq!(depths.len(), 3);
    }

    #[test]
    fn intersect_applies_each_cutter_separately() {
        let mut world = world();
        let scene = world.spawn_empty().id();
        let a = rectangle(&mut world, scene, Vec2::new(-2.0, -1.0), Vec2::splat(2.0));
        let b = rectangle(&mut world, scene, Vec2::new(-1.0, -1.0), Vec2::splat(2.0));
        rectangle(
            &mut world,
            scene,
            Vec2::new(-2.0, -1.0),
            Vec2::new(4.0, 2.0),
        );

        apply(&mut world, vec![a, b], Operation::Intersect);
        world.flush();

        assert_eq!(geometries(&mut world).len(), 4); // cutters + two intersections
    }

    #[test]
    fn curves_are_flattened_and_holes_tessellate() {
        let mut world = world();
        let scene = world.spawn_empty().id();
        let cutter = {
            let mut commands = world.commands();
            PhysicalObject::ball(1.0, Vec3::ZERO).spawn(&mut commands, scene)
        };
        world.flush();
        let target = rectangle(&mut world, scene, Vec2::splat(-2.0), Vec2::splat(4.0));

        apply(&mut world, vec![cutter], Operation::Subtract);

        let area = area(&world, target);
        assert!(
            (area - (16.0 - std::f32::consts::PI)).abs() < 1e-2,
            "{area}"
        );
    }
}
