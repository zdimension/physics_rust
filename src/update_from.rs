use crate::objects::SettingComponent;
use bevy::hierarchy::Parent;
use bevy::prelude::{Component, Entity, Query, Ref};
use std::marker::PhantomData;

#[derive(Component)]
pub enum UpdateFrom<T: SettingComponent> {
    This,
    Entity(Entity, PhantomData<T>),
}

impl<T: SettingComponent> UpdateFrom<T> {
    pub fn entity(ent: Entity) -> Self {
        UpdateFrom::Entity(ent, PhantomData)
    }

    pub fn find_component(
        &self,
        base: Entity,
        parents: &Query<(Option<&Parent>, Option<Ref<T>>)>,
    ) -> Option<(Entity, T::Value)> {
        let mut root = match self {
            UpdateFrom::This => base,
            UpdateFrom::Entity(e, _) => *e,
        };
        loop {
            let Ok((p, col)) = parents.get(root) else { return None; };
            if let Some(col) = col {
                return Some((root, col.get()));
            }
            root = match p {
                Some(p) => p.get(),
                None => return None,
            };
        }
    }
}
