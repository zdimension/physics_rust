use crate::objects::spring::{SpringEndHandle, SpringObject};
use crate::tools::add_object::AddObjectEvent;
use crate::ui::{ContextMenuEvent, Selected, TemporaryWindow, WindowSelectionTarget};

use crate::mouse_tracking::MousePos;
use avian2d::prelude::*;
use bevy::log::info;
use bevy::math::{Vec2, Vec2Swizzles, Vec3Swizzles};
use bevy::prelude::*;
use std::collections::HashSet;

#[derive(Message)]
pub struct SelectEvent {
    pub(crate) entities: Vec<Entity>,
    pub(crate) mode: SelectionMode,
    pub(crate) open_menu: bool,
    /// Direct object picks expand selection groups. Area-selection events leave
    /// this false so an enclosure selects exactly the objects it contains.
    pub(crate) expand_groups: bool,
}

#[derive(Component, Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SelectionGroup(pub(crate) Entity);

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SelectionMode {
    Replace,
    Toggle,
}

#[derive(Resource)]
pub struct SelectionConfig {
    pub select_by_encircling: bool,
}

impl Default for SelectionConfig {
    fn default() -> Self {
        Self {
            select_by_encircling: true,
        }
    }
}

pub fn process_select(
    mut events: MessageReader<SelectEvent>,
    mut commands: Commands,
    mut menu_event: MessageWriter<ContextMenuEvent>,
    screen_pos: Res<MousePos>,
    selected: Query<Entity, With<Selected>>,
    springs: Query<(), With<SpringObject>>,
    spring_handles: Query<(Entity, &SpringEndHandle)>,
    groups: Query<(Entity, &SelectionGroup)>,
) {
    for SelectEvent {
        entities,
        mode,
        open_menu,
        expand_groups,
    } in events.read()
    {
        let direct_toggle_removes = *expand_groups
            && entities
                .first()
                .is_some_and(|entity| selected.contains(*entity));
        let entities = if *expand_groups {
            expand_selection_groups(entities.iter().copied(), &groups)
        } else {
            entities.clone()
        };
        let entities = normalize_selection(entities, &springs, &spring_handles);
        if entities.is_empty() {
            info!("Setting selection to nothing");
        } else {
            info!("Selecting entities: {:?}", entities);
            for entity in &entities {
                commands.entity(*entity).log_components();
            }
        }

        let selected_before = selected.iter().collect::<Vec<_>>();
        let selected_after = match *mode {
            SelectionMode::Replace => {
                for entity in selected_before.iter().copied() {
                    commands.entity(entity).remove::<Selected>();
                }
                for entity in entities.iter().copied() {
                    commands.entity(entity).insert(Selected);
                }
                entities
            }
            SelectionMode::Toggle => {
                if entities.is_empty() {
                    for entity in selected_before.iter().copied() {
                        commands.entity(entity).remove::<Selected>();
                    }
                    Vec::new()
                } else if *expand_groups {
                    let mut selected_after = selected_before;
                    for entity in entities.iter().copied() {
                        if direct_toggle_removes {
                            commands.entity(entity).remove::<Selected>();
                            selected_after.retain(|selected| *selected != entity);
                        } else {
                            commands.entity(entity).insert(Selected);
                            push_unique(&mut selected_after, entity);
                        }
                    }
                    selected_after
                } else {
                    let mut selected_after = selected_before;
                    for entity in entities.iter().copied() {
                        if selected_after.contains(&entity) {
                            commands.entity(entity).remove::<Selected>();
                            selected_after.retain(|selected| *selected != entity);
                        } else {
                            commands.entity(entity).insert(Selected);
                            push_unique(&mut selected_after, entity);
                        }
                    }
                    selected_after
                }
            }
        };

        if *open_menu {
            menu_event.write(ContextMenuEvent {
                screen_pos: screen_pos.xy(),
                target: WindowSelectionTarget::from_entities(selected_after),
            });
        }
    }
}

fn expand_selection_groups(
    entities: impl IntoIterator<Item = Entity>,
    groups: &Query<(Entity, &SelectionGroup)>,
) -> Vec<Entity> {
    let mut expanded = entities.into_iter().fold(Vec::new(), |mut result, entity| {
        push_unique(&mut result, entity);
        result
    });
    let group_ids = expanded
        .iter()
        .filter_map(|entity| groups.get(*entity).ok().map(|(_, group)| group.0))
        .collect::<HashSet<_>>();
    for (entity, group) in groups {
        if group_ids.contains(&group.0) {
            push_unique(&mut expanded, entity);
        }
    }
    expanded
}

fn normalize_selection(
    entities: impl IntoIterator<Item = Entity>,
    springs: &Query<(), With<SpringObject>>,
    spring_handles: &Query<(Entity, &SpringEndHandle)>,
) -> Vec<Entity> {
    let mut normalized = Vec::new();
    for entity in entities {
        push_unique(&mut normalized, entity);
        if springs.contains(entity) {
            for (handle_entity, handle) in spring_handles.iter() {
                if handle.spring == entity {
                    push_unique(&mut normalized, handle_entity);
                }
            }
        }
    }
    normalized
}

fn push_unique(entities: &mut Vec<Entity>, entity: Entity) {
    if !entities.contains(&entity) {
        entities.push(entity);
    }
}

#[derive(Clone, Message)]
pub struct SelectEnclosedEvent {
    pub(crate) start: Vec2,
    pub(crate) end: Vec2,
    pub(crate) mode: SelectionMode,
    pub(crate) open_menu: bool,
    pub(crate) fallback_add_object: Option<AddObjectEvent>,
}

pub fn process_select_enclosed(
    mut events: MessageReader<SelectEnclosedEvent>,
    mut select: MessageWriter<SelectEvent>,
    mut add_object: MessageWriter<AddObjectEvent>,
    colliders: Query<(Entity, &Collider, &GlobalTransform), Without<ColliderDisabled>>,
    mut commands: Commands,
    wnds: Query<Entity, With<TemporaryWindow>>,
) {
    for SelectEnclosedEvent {
        start,
        end,
        mode,
        open_menu,
        fallback_add_object,
    } in events.read()
    {
        for id in wnds.iter() {
            commands.entity(id).despawn();
        }

        let start = *start;
        let end = *end;
        let min = start.min(end);
        let max = start.max(end);
        let mut enclosed = colliders
            .iter()
            .filter_map(|(entity, collider, transform)| {
                collider_is_enclosed(collider, transform, min, max)
                    .then_some((entity, transform.translation_vec3a().z))
            })
            .collect::<Vec<_>>();
        enclosed.sort_by(|a, b| a.1.total_cmp(&b.1));
        let selected = enclosed
            .into_iter()
            .map(|(entity, _)| entity)
            .collect::<Vec<_>>();

        if !selected.is_empty() {
            select.write(SelectEvent {
                entities: selected,
                mode: *mode,
                open_menu: *open_menu,
                expand_groups: false,
            });
        } else if let Some(fallback_add_object) = fallback_add_object.clone() {
            add_object.write(fallback_add_object);
        } else {
            select.write(SelectEvent {
                entities: vec![],
                mode: *mode,
                open_menu: *open_menu,
                expand_groups: false,
            });
        }
    }
}

#[derive(Copy, Clone, Message)]
pub struct SelectUnderMouseEvent {
    pub(crate) pos: Vec2,
    pub(crate) mode: SelectionMode,
    pub(crate) open_menu: bool,
}

pub fn process_select_under_mouse(
    mut events: MessageReader<SelectUnderMouseEvent>,
    mut select: MessageWriter<SelectEvent>,
    mut menu_event: MessageWriter<ContextMenuEvent>,
    colliders: Query<(Entity, &Collider, &GlobalTransform), Without<ColliderDisabled>>,
    selected_entities: Query<Entity, With<Selected>>,
    selection_groups: Query<(Entity, &SelectionGroup)>,
    spring_handles: Query<(), With<SpringEndHandle>>,
    mut commands: Commands,
    screen_pos: Res<MousePos>,
    wnds: Query<Entity, With<TemporaryWindow>>,
) {
    for SelectUnderMouseEvent {
        pos,
        mode,
        open_menu,
    } in events.read().copied()
    {
        for id in wnds.iter() {
            commands.entity(id).despawn();
        }
        let selected = collider_under_point(pos, &colliders);

        let preserve_current_selection = open_menu
            && mode == SelectionMode::Replace
            && selected.is_some_and(|entity| {
                selected_entities.contains(entity) && !spring_handles.contains(entity)
            });
        if preserve_current_selection {
            let clicked = selected.unwrap();
            let group_is_fully_selected = match selection_groups.get(clicked) {
                Ok((_, group)) => selection_groups
                    .iter()
                    .filter(|(_, candidate)| candidate.0 == group.0)
                    .all(|(entity, _)| selected_entities.contains(entity)),
                Err(_) => true,
            };
            if group_is_fully_selected {
                menu_event.write(ContextMenuEvent {
                    screen_pos: screen_pos.xy(),
                    target: WindowSelectionTarget::from_entities(selected_entities.iter()),
                });
                continue;
            }

            select.write(SelectEvent {
                entities: selected_entities.iter().chain(Some(clicked)).collect(),
                mode,
                open_menu,
                expand_groups: true,
            });
            continue;
        }

        select.write(SelectEvent {
            entities: selected.into_iter().collect(),
            mode,
            open_menu,
            expand_groups: true,
        });
    }
}

pub(crate) fn collider_under_point(
    point: Vec2,
    colliders: &Query<(Entity, &Collider, &GlobalTransform), Without<ColliderDisabled>>,
) -> Option<Entity> {
    colliders_under_point(point, colliders).next()
}

pub(crate) fn colliders_under_point(
    point: Vec2,
    colliders: &Query<(Entity, &Collider, &GlobalTransform), Without<ColliderDisabled>>,
) -> impl Iterator<Item = Entity> {
    let mut hits = colliders
        .iter()
        .filter_map(|(entity, collider, transform)| {
            collider_contains_point(collider, transform, point)
                .then_some((entity, transform.translation_vec3a().z))
        })
        .collect::<Vec<_>>();
    hits.sort_by(|a, b| a.1.total_cmp(&b.1));
    hits.into_iter().rev().map(|(entity, _)| entity)
}

fn collider_contains_point(collider: &Collider, transform: &GlobalTransform, point: Vec2) -> bool {
    let (collider, position, rotation) = collider_at_transform(collider, transform);
    collider.contains_point(position, rotation, point)
}

fn collider_is_enclosed(
    collider: &Collider,
    transform: &GlobalTransform,
    min: Vec2,
    max: Vec2,
) -> bool {
    let (collider, position, rotation) = collider_at_transform(collider, transform);
    let aabb = collider.aabb(position, rotation);
    aabb.min.x >= min.x && aabb.max.x <= max.x && aabb.min.y >= min.y && aabb.max.y <= max.y
}

fn collider_at_transform(
    collider: &Collider,
    transform: &GlobalTransform,
) -> (Collider, Vec2, Rotation) {
    let (scale, rotation, translation) = transform.to_scale_rotation_translation();
    let mut transformed = Collider::from(collider.shape().clone());
    transformed.set_scale(scale.xy(), 10);
    let angle = rotation.to_euler(EulerRot::XYZ).2;
    (transformed, translation.xy(), Rotation::radians(angle))
}

#[cfg(test)]
mod selection_tests {
    use super::*;

    fn selection_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(MousePos::default())
            .add_message::<SelectEvent>()
            .add_message::<ContextMenuEvent>()
            .add_systems(Update, process_select);
        app
    }

    #[test]
    fn direct_selection_expands_groups_but_area_selection_does_not() {
        let mut app = selection_app();
        let group_id = app.world_mut().spawn_empty().id();
        let first = app.world_mut().spawn(SelectionGroup(group_id)).id();
        let second = app.world_mut().spawn(SelectionGroup(group_id)).id();

        app.world_mut().write_message(SelectEvent {
            entities: vec![first],
            mode: SelectionMode::Replace,
            open_menu: false,
            expand_groups: true,
        });
        app.update();
        assert!(app.world().entity(first).contains::<Selected>());
        assert!(app.world().entity(second).contains::<Selected>());

        app.world_mut().write_message(SelectEvent {
            entities: vec![first],
            mode: SelectionMode::Replace,
            open_menu: false,
            expand_groups: false,
        });
        app.update();
        assert!(app.world().entity(first).contains::<Selected>());
        assert!(!app.world().entity(second).contains::<Selected>());
    }

    #[test]
    fn direct_toggle_changes_a_partially_selected_group_as_one_unit() {
        let mut app = selection_app();
        let group_id = app.world_mut().spawn_empty().id();
        let first = app
            .world_mut()
            .spawn((SelectionGroup(group_id), Selected))
            .id();
        let second = app.world_mut().spawn(SelectionGroup(group_id)).id();

        app.world_mut().write_message(SelectEvent {
            entities: vec![first],
            mode: SelectionMode::Toggle,
            open_menu: false,
            expand_groups: true,
        });
        app.update();
        assert!(!app.world().entity(first).contains::<Selected>());
        assert!(!app.world().entity(second).contains::<Selected>());

        app.world_mut().write_message(SelectEvent {
            entities: vec![second],
            mode: SelectionMode::Toggle,
            open_menu: false,
            expand_groups: true,
        });
        app.update();
        assert!(app.world().entity(first).contains::<Selected>());
        assert!(app.world().entity(second).contains::<Selected>());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_unique_keeps_first_occurrence_only() {
        let a = Entity::from_raw_u32(1).unwrap();
        let b = Entity::from_raw_u32(2).unwrap();
        let mut entities = vec![a];

        push_unique(&mut entities, a);
        push_unique(&mut entities, b);

        assert_eq!(entities, vec![a, b]);
    }

    #[test]
    fn collider_hit_test_uses_current_transform_without_a_physics_step() {
        let collider = Collider::circle(0.5);
        let transform =
            GlobalTransform::from(Transform::from_xyz(3.0, 4.0, 0.0).with_scale(Vec3::splat(4.0)));

        assert!(collider_contains_point(
            &collider,
            &transform,
            Vec2::new(4.9, 4.0)
        ));
        assert!(!collider_contains_point(
            &collider,
            &transform,
            Vec2::new(5.1, 4.0)
        ));
    }

    #[test]
    fn collider_enclosure_uses_current_rotation() {
        let collider = Collider::rectangle(4.0, 2.0);
        let transform = GlobalTransform::from(Transform::from_rotation(Quat::from_rotation_z(
            std::f32::consts::FRAC_PI_2,
        )));

        assert!(collider_is_enclosed(
            &collider,
            &transform,
            Vec2::new(-1.1, -2.1),
            Vec2::new(1.1, 2.1),
        ));
        assert!(!collider_is_enclosed(
            &collider,
            &transform,
            Vec2::new(-2.1, -1.1),
            Vec2::new(2.1, 1.1),
        ));
    }
}
