use crate::palette::ToRgba;
use crate::update_from::UpdateFrom;
use bevy::hierarchy::Parent;
use bevy::math::Vec3;
use bevy::prelude::{Component, Entity, Query, Sprite, Transform};
use bevy_egui::egui::ecolor::Hsva;
use std::marker::PhantomData;

pub(crate) mod hinge;
pub(crate) mod laser;
pub(crate) mod phy_obj;

pub trait SettingComponent: Component + Sized {
    type Value;

    fn get(&self) -> Self::Value;

    fn update_from_this(self) -> (Self, UpdateFrom<Self>) {
        (self, UpdateFrom::<Self>::This)
    }

    fn update_from_entity(self, entity: Entity) -> (Self, UpdateFrom<Self>) {
        (self, UpdateFrom::<Self>::Entity(entity, PhantomData))
    }
}

pub fn update_sprites_color(
    mut sprites: Query<(Entity, &mut Sprite, &UpdateFrom<ColorComponent>)>,
    parents: Query<(Option<&Parent>, Option<&ColorComponent>)>,
) {
    for (entity, mut sprite, update_source) in sprites.iter_mut() {
        sprite.color = update_source.find_component(entity, &parents).1.to_rgba();
    }
}

// set scale to (size, size, 1)
pub fn update_size_scales(
    mut scales: Query<(Entity, &mut Transform, &UpdateFrom<SizeComponent>)>,
    parents: Query<(Option<&Parent>, Option<&SizeComponent>)>,
) {
    for (entity, mut scale, update_source) in scales.iter_mut() {
        let (_, size) = update_source.find_component(entity, &parents);
        scale.scale = Vec3::new(size, size, 1.0);
    }
}

#[derive(Component)]
pub struct ColorComponent(pub Hsva);

impl SettingComponent for ColorComponent {
    type Value = Hsva;

    fn get(&self) -> Hsva {
        self.0
    }
}

#[derive(Component)]
pub struct SizeComponent(pub f32);

impl SettingComponent for SizeComponent {
    type Value = f32;

    fn get(&self) -> f32 {
        self.0
    }
}
