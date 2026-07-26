use crate::palette::ToRgba;
use crate::update_from::UpdateFrom;
use avian2d::collision::narrow_phase::CollisionEventSystems;
use avian2d::{math::*, prelude::*};
use avian2d::{math::*, prelude::*};
use avian2d::{math::*, prelude::*};
use bevy::app::Update;
use bevy::math::Vec3;
use bevy::prelude::ChildOf;
use bevy::prelude::{App, Component, Entity, IntoScheduleConfigs, Query, Ref, Sprite, Transform};
use bevy_egui::egui::ecolor::Hsva;
use num_traits::FloatConst;
use std::marker::PhantomData;

pub(crate) mod axle;
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
    parents: Query<(Option<&ChildOf>, Option<Ref<ColorComponent>>)>,
) {
    for (entity, mut sprite, update_source) in sprites.iter_mut() {
        sprite.color = update_source
            .find_component(entity, &parents)
            .expect("no color found")
            .1
            .to_rgba();
    }
}

pub fn update_size_scales(
    mut scales: Query<(Entity, &mut Transform, &UpdateFrom<SizeComponent>)>,
    parents: Query<(Option<&ChildOf>, Option<Ref<SizeComponent>>)>,
) {
    for (entity, mut scale, update_source) in scales.iter_mut() {
        let (_, size) = update_source
            .find_component(entity, &parents)
            .expect("size not found");
        scale.scale = Vec3::new(size, size, 1.0);
    }
}

pub mod spring;
pub mod tracer;

pub fn add_systems(app: &mut App) {
    spring::add_systems(app);
    tracer::add_systems(app);

    app.add_systems(
        Update,
        (
            update_sprites_color,
            update_size_scales,
            axle::sync_hinge_motors,
            axle::update_hinge_motor_visuals,
            phy_obj::spawn_circle_angle_markers,
        ),
    )
    .add_systems(
        PhysicsSchedule,
        axle::break_hinges
            .after(SolverSystems::Finalize)
            .after(CollisionEventSystems),
    );
}

#[derive(Component)]
pub struct ColorComponent(pub Hsva);

#[derive(Component)]
pub struct SpriteOnly;

#[derive(Component)]
pub struct CircleAngleMarker;

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

#[derive(Component, Copy, Clone, Debug)]
pub struct MotorComponent {
    pub enabled: bool,
    pub reversed: bool,
    /// rpm
    pub vel: f32,
    /// Nm
    pub torque: f32,
    /// Ns
    pub break_limit: f32,
}

impl Default for MotorComponent {
    fn default() -> Self {
        Self {
            enabled: false,
            reversed: false,
            vel: 15.0,
            torque: 100.0,
            break_limit: f32::INFINITY,
        }
    }
}

impl SettingComponent for MotorComponent {
    type Value = Self;

    fn get(&self) -> Self {
        *self
    }
}
