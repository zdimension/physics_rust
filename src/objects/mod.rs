use crate::palette::ToRgba;
use crate::update_from::UpdateFrom;
use avian2d::collision::narrow_phase::CollisionEventSystems;
use avian2d::prelude::*;
use bevy::app::Update;
use bevy::prelude::ChildOf;
use bevy::prelude::{
    App, Changed, Component, Entity, IntoScheduleConfigs, Query, Ref, Sprite, With,
};
use bevy_egui::egui::ecolor::Hsva;

pub(crate) mod air;
pub(crate) mod attraction;
pub(crate) mod axle;
pub(crate) mod laser;
pub(crate) mod phy_obj;
pub(crate) mod plane;
pub(crate) mod thruster;

pub trait SettingComponent: Component + Sized {
    type Value;

    fn get(&self) -> Self::Value;

    fn update_from_this(self) -> (Self, UpdateFrom<Self>) {
        (self, UpdateFrom::<Self>::This)
    }
}

pub fn update_sprites_color(
    mut sprites: Query<(Entity, &mut Sprite, &UpdateFrom<ColorComponent>)>,
    parents: Query<(Option<&ChildOf>, Option<Ref<ColorComponent>>)>,
    changed_colors: Query<(), Changed<ColorComponent>>,
    changed_sources: Query<(), Changed<UpdateFrom<ColorComponent>>>,
    changed_parents: Query<(), (Changed<ChildOf>, With<UpdateFrom<ColorComponent>>)>,
) {
    if changed_colors.is_empty() && changed_sources.is_empty() && changed_parents.is_empty() {
        return;
    }

    for (entity, mut sprite, update_source) in sprites.iter_mut() {
        let color = update_source
            .find_component(entity, &parents)
            .expect("no color found")
            .1
            .to_rgba();
        if sprite.color != color {
            sprite.color = color;
        }
    }
}

pub mod spring;
pub mod tracer;

pub fn add_systems(app: &mut App) {
    air::add_systems(app);
    attraction::add_systems(app);
    spring::add_systems(app);
    thruster::add_systems(app);
    tracer::add_systems(app);

    app.add_systems(
        Update,
        (
            update_sprites_color,
            laser::sync_laser_size.before(laser::draw_lasers),
            axle::sync_hinge_motors,
            axle::update_hinge_motor_visuals,
            phy_obj::spawn_circle_angle_markers,
            plane::update_plane_visuals,
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
