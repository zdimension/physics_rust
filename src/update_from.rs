use crate::objects::SettingComponent;
use bevy::hierarchy::Parent;
use bevy::prelude::{Component, Entity, Query};
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
        parents: &Query<(Option<&Parent>, Option<&T>)>,
    ) -> (Entity, T::Value) {
        let mut root = match self {
            UpdateFrom::This => base,
            UpdateFrom::Entity(e, _) => *e,
        };
        loop {
            let (p, col) = parents.get(root).unwrap();
            if let Some(col) = col {
                return (root, col.get());
            }
            root = p.expect("No parent").get();
        }
    }
}
