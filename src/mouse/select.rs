use crate::objects::spring::{self, SpringEndHandle, SpringObject};
use crate::ui::{ContextMenuEvent, EntitySelection, TemporaryWindow, UiState};

//use crate::Despawn;
use crate::mouse_tracking::MousePos;
use avian2d::prelude::*;
use bevy::log::info;
use bevy::math::{Vec2, Vec2Swizzles};
use bevy::prelude::*;

#[derive(Message)]
pub struct SelectEvent {
    pub(crate) entity: Option<Entity>,
    pub(crate) open_menu: bool,
}

pub fn process_select(
    mut events: MessageReader<SelectEvent>,
    mut state: ResMut<UiState>,
    mut commands: Commands,
    mut menu_event: MessageWriter<ContextMenuEvent>,
    screen_pos: Res<MousePos>,
) {
    for SelectEvent { entity, open_menu } in events.read() {
        if let Some(entity) = entity {
            info!("Selecting entity: {:?}", entity);
            commands.entity(*entity).log_components();
        } else {
            info!("Setting selection to nothing");
        }

        state.selected_entity = entity.map(|entity| EntitySelection { entity });
        if *open_menu {
            menu_event.write(ContextMenuEvent {
                screen_pos: screen_pos.xy(),
            });
        }
    }
}

pub fn find_under_mouse(
    query: &SpatialQuery,
    pos: Vec2,
    filter: SpatialQueryFilter,
    mut z: impl FnMut(Entity) -> f32,
) -> impl Iterator<Item = Entity> {
    let mut hits = Vec::new();

    query.point_intersections_callback(pos, &filter, |ent| {
        hits.push((ent, z(ent)));
        true
    });

    hits.sort_by(|a, b| a.1.total_cmp(&b.1));
    hits.into_iter().rev().map(|(entity, _)| entity)
}

#[derive(Copy, Clone, Message)]
pub struct SelectUnderMouseEvent {
    pub(crate) pos: Vec2,
    pub(crate) open_menu: bool,
}

pub fn process_select_under_mouse(
    mut events: MessageReader<SelectUnderMouseEvent>,
    spatial_query: SpatialQuery,
    mut select: MessageWriter<SelectEvent>,
    query: Query<&GlobalTransform>,
    spring_objects: Query<(Entity, &SpringObject, &Transform)>,
    spring_ends: Query<(Entity, &SpringEndHandle)>,
    body_positions: Query<(&Position, &Rotation)>,
    mut commands: Commands,
    wnds: Query<Entity, With<TemporaryWindow>>,
) {
    for SelectUnderMouseEvent { pos, open_menu } in events.read().copied() {
        for id in wnds.iter() {
            commands.entity(id).despawn();
        }
        let spatial_selected = find_under_mouse(&spatial_query, pos, Default::default(), |ent| {
            let Ok(transform) = query.get(ent) else {
                panic!("Entity {:?} has no transform", ent)
            };
            transform.translation_vec3a().z
        })
        .next()
        .map(|entity| {
            let z = query
                .get(entity)
                .map(|transform| transform.translation_vec3a().z)
                .unwrap_or(f32::NEG_INFINITY);
            (entity, z)
        });

        let spring_selected =
            spring::find_spring_under_point(pos, &spring_objects, &spring_ends, &body_positions);
        let selected = match (spatial_selected, spring_selected) {
            (Some(spatial), Some(spring)) if spring.1 >= spatial.1 => Some(spring.0),
            (Some(spatial), _) => Some(spatial.0),
            (None, Some(spring)) => Some(spring.0),
            (None, None) => None,
        };

        select.write(SelectEvent {
            entity: selected,
            open_menu,
        });
    }
}
