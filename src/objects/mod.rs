use crate::palette::ToRgba;
use crate::update_from::UpdateFrom;
use bevy::hierarchy::Parent;
use bevy::math::Vec3;
use bevy::prelude::{App, Component, Entity, Query, Ref, Sprite, Transform};
use bevy_egui::egui::ecolor::Hsva;
use bevy_rapier2d::prelude::ImpulseJoint;
use bevy_rapier2d::rapier::dynamics::JointAxis;
use bevy_rapier2d::rapier::prelude::MotorModel;
use num_traits::FloatConst;
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
    parents: Query<(Option<&Parent>, Option<Ref<ColorComponent>>)>,
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
    parents: Query<(Option<&Parent>, Option<Ref<SizeComponent>>)>,
) {
    for (entity, mut scale, update_source) in scales.iter_mut() {
        let (_, size) = update_source
            .find_component(entity, &parents)
            .expect("size not found");
        scale.scale = Vec3::new(size, size, 1.0);
    }
}

pub fn update_motors(
    mut motors: Query<(Entity, &mut ImpulseJoint, &UpdateFrom<MotorComponent>)>,
    parents: Query<(Option<&Parent>, Option<Ref<MotorComponent>>)>,
) {
    for (entity, mut motor, update_source) in motors.iter_mut() {
        let (_, motor_component) = update_source
            .find_component(entity, &parents)
            .expect("motor not found");
        motor.data.set_motor(
            JointAxis::AngX,
            0.0,
            {
                let vel = motor_component.vel * f32::PI() / 30.0;
                if motor_component.reversed {
                    -vel
                } else {
                    vel
                }
            },
            0.0,
            motor_component.torque,
        );
        motor
            .data
            .raw
            .set_motor_model(JointAxis::AngX.into(), MotorModel::ForceBased);
        motor
            .data
            .raw
            .motor_axes
            .set(JointAxis::AngX.into(), motor_component.enabled);
    }
}

pub fn add_update_systems(app: &mut App) {
    app.add_system(update_sprites_color)
        .add_system(update_size_scales)
        .add_system(update_motors);
}

#[derive(Component)]
pub struct ColorComponent(pub Hsva);

#[derive(Component)]
pub struct SpriteOnly;

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
