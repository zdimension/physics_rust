use crate::ui::{ContextMenuEvent, TemporaryWindow};
use crate::{EntitySelection, UiState};
use bevy::hierarchy::DespawnRecursiveExt;
use bevy::log::info;
use bevy::math::Vec2;
use bevy::prelude::{
    Commands, Component, Entity, EventReader, EventWriter, Query, Res, ResMut, Transform, Windows,
    With,
};
use bevy_egui::egui::epaint::util::{FloatOrd, OrderedFloat};
use bevy_mouse_tracking_plugin::MousePos;
use bevy_prototype_lyon::draw::DrawMode;
use bevy_rapier2d::pipeline::QueryFilter;
use bevy_rapier2d::plugin::RapierContext;
use derivative::Derivative;
use std::collections::BTreeSet;

pub struct SelectEvent {
    pub(crate) entity: Option<Entity>,
    pub(crate) open_menu: bool,
}

#[derive(Component)]
struct UnselectedDrawMode {
    draw_mode: DrawMode,
}

pub fn process_select(
    mut events: EventReader<SelectEvent>,
    mut state: ResMut<UiState>,
    mut commands: Commands,
    mut menu_event: EventWriter<ContextMenuEvent>,
    screen_pos: Res<MousePos>,
    windows: Res<Windows>,
) {
    let screen_pos = Vec2::new(
        screen_pos.x,
        windows.get_primary().unwrap().height() - screen_pos.y,
    );

    for SelectEvent { entity, open_menu } in events.iter() {
        if let Some(entity) = entity {
            info!("Selecting entity: {:?}", entity);
            commands.entity(*entity).log_components();
        } else {
            info!("Setting selection to nothing");
        }

        state.selected_entity = entity.map(|entity| EntitySelection { entity });
        if *open_menu {
            menu_event.send(ContextMenuEvent { screen_pos });
        }
    }
}

pub fn find_under_mouse(
    rapier: &RapierContext,
    pos: Vec2,
    filter: QueryFilter,
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

    rapier.intersections_with_point(pos, filter, |ent| {
        set.insert(EntityZ {
            entity: ent,
            z: z(ent).ord(),
        });
        true
    });

    set.into_iter().rev().map(|EntityZ { entity, .. }| entity)
}

#[derive(Copy, Clone)]
pub struct SelectUnderMouseEvent {
    pub(crate) pos: Vec2,
    pub(crate) open_menu: bool,
}

pub fn process_select_under_mouse(
    mut events: EventReader<SelectUnderMouseEvent>,
    rapier: Res<RapierContext>,
    mut select: EventWriter<SelectEvent>,
    query: Query<&Transform>,
    mut commands: Commands,
    wnds: Query<Entity, With<TemporaryWindow>>,
) {
    for SelectUnderMouseEvent { pos, open_menu } in events.iter().copied() {
        for id in wnds.iter() {
            commands.entity(id).despawn_recursive();
        }
        let selected = find_under_mouse(&rapier, pos, QueryFilter::default(), |ent| {
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
