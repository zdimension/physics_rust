use crate::ui::{ContextMenuEvent, EntitySelection, TemporaryWindow, UiState};

//use crate::Despawn;
use bevy::log::info;
use bevy::math::{Vec2, Vec2Swizzles};
use bevy::prelude::*;
use crate::mouse_tracking::MousePos;
use avian2d::{math::*, prelude::*};
use avian2d::{math::*, prelude::*};

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
    query: Query<&Transform>,
    mut commands: Commands,
    wnds: Query<Entity, With<TemporaryWindow>>,
) {
    for SelectUnderMouseEvent { pos, open_menu } in events.read().copied() {
        for id in wnds.iter() {
            commands.entity(id).despawn();
        }
        let selected = find_under_mouse(&spatial_query, pos, Default::default(), |ent| {
            let Ok(transform) = query.get(ent) else {
                panic!("Entity {:?} has no transform", ent)
            };
            transform.translation.z
        })
        .next();
        select.write(SelectEvent {
            entity: selected,
            open_menu,
        });
    }
}
