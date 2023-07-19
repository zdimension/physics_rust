use crate::ui::{ContextMenuEvent, EntitySelection, TemporaryWindow, UiState};
use std::collections::btree_set::BTreeSet;

//use crate::Despawn;
use bevy::log::info;
use bevy::math::{Vec2, Vec2Swizzles};
use bevy::prelude::*;
use bevy_egui::egui::epaint::util::{FloatOrd, OrderedFloat};
use bevy_mouse_tracking_plugin::MousePos;
use bevy_xpbd_2d::{math::*, prelude::*};
use bevy_xpbd_2d::{math::*, prelude::*};
use derivative::Derivative;

#[derive(Event)]
pub struct SelectEvent {
    pub(crate) entity: Option<Entity>,
    pub(crate) open_menu: bool,
}

pub fn process_select(
    mut events: EventReader<SelectEvent>,
    mut state: ResMut<UiState>,
    mut commands: Commands,
    mut menu_event: EventWriter<ContextMenuEvent>,
    screen_pos: Res<MousePos>,
) {
    for SelectEvent { entity, open_menu } in events.iter() {
        if let Some(entity) = entity {
            info!("Selecting entity: {:?}", entity);
            commands.entity(*entity).log_components();
        } else {
            info!("Setting selection to nothing");
        }

        state.selected_entity = entity.map(|entity| EntitySelection { entity });
        if *open_menu {
            menu_event.send(ContextMenuEvent {
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
    #[derive(Derivative)]
    #[derivative(PartialEq, PartialOrd, Eq, Ord)]
    struct EntityZ {
        #[derivative(PartialEq = "ignore", PartialOrd = "ignore")]
        entity: Entity,
        z: OrderedFloat<f32>,
    }

    let mut set = BTreeSet::new();

    query.point_intersections_callback(pos, filter, |ent| {
        set.insert(EntityZ {
            entity: ent,
            z: z(ent).ord(),
        });
        true
    });

    set.into_iter().rev().map(|EntityZ { entity, .. }| entity)
}

#[derive(Copy, Clone, Event)]
pub struct SelectUnderMouseEvent {
    pub(crate) pos: Vec2,
    pub(crate) open_menu: bool,
}

pub fn process_select_under_mouse(
    mut events: EventReader<SelectUnderMouseEvent>,
    spatial_query: SpatialQuery,
    mut select: EventWriter<SelectEvent>,
    query: Query<&Transform>,
    mut commands: Commands,
    wnds: Query<Entity, With<TemporaryWindow>>,
) {
    for SelectUnderMouseEvent { pos, open_menu } in events.iter().copied() {
        for id in wnds.iter() {
            commands.entity(id).despawn_recursive();
        }
        let selected = find_under_mouse(&spatial_query, pos, Default::default(), |ent| {
            let Ok(transform) = query.get(ent) else {
                panic!("Entity {:?} has no transform", ent)
            };
            transform.translation.z
        })
        .next();
        select.send(SelectEvent {
            entity: selected,
            open_menu,
        });
    }
}
