use bevy::prelude::*;
use bevy::math::{EulerRot, Vec2, Vec3, Vec3Swizzles};
use bevy_egui::egui::ecolor::Hsva;
use bevy_prototype_lyon::geometry::GeometryBuilder;
use bevy_prototype_lyon::shapes;
use bevy::hierarchy::{BuildChildren, DespawnRecursiveExt};
use crate::{AsMode, ColorComponent};

#[derive(Component)]
pub struct LaserBundle {
    pub(crate) fade_distance: f32
}

struct LaserRay {
    start: Vec2,
    angle: f32,
    length: f32,
    strength: f32,
    color: Hsva,
    width: f32,
    start_distance: f32,
    refractive_index: f32,
}

impl LaserRay {
    fn length_clipped(&self) -> f32 {
        if self.length == f32::INFINITY {
            1e6f32
        } else {
            self.length
        }
    }

    fn end(&self) -> Vec2 {
        self.start + Vec2::from_angle(self.angle) * self.length_clipped()
    }
}

struct LaserCompute<'a> {
    laser: &'a LaserBundle,
    rays: Vec<LaserRay>
}

impl<'a> LaserCompute<'a> {
    fn new(laser: &'a LaserBundle) -> Self {
        Self {
            laser,
            rays: Vec::new()
        }
    }

    fn shoot_ray(&mut self, ray: LaserRay, depth: usize) {
        self.rays.push(ray);
    }

    fn end(self) -> Vec<LaserRay> {
        self.rays
    }
}

const LASER_WIDTH: f32 = 0.2;

pub fn draw_lasers(
    lasers: Query<(&Transform, &LaserBundle, &ColorComponent)>,
    rays: Query<Entity, With<LaserRays>>,
    mut commands: Commands,
) {
    let rays = rays.single();
    commands.entity(rays).despawn_descendants();

    for (transform, laser, color) in lasers.iter() {
        let ray_width = transform.scale.x * LASER_WIDTH;

        let initial = LaserRay {
            start: transform.transform_point(Vec3::new(0.5, 0.0, 1.0)).xy(),
            angle: transform.rotation.to_euler(EulerRot::XYZ).2,
            length: laser.fade_distance,
            strength: 1.0,
            color: color.0,
            width: ray_width,
            start_distance: 0.0,
            refractive_index: 1.0,
        };

        let mut compute = LaserCompute::new(laser);

        compute.shoot_ray(initial, 0);

        let ray_list = compute.end();

        commands.entity(rays).add_children(|builder| {
            for ray in ray_list {
                builder.spawn(GeometryBuilder::build_as(
                    &shapes::Line(ray.start, ray.end()),
                    crate::make_stroke(crate::hsva_to_rgba(ray.color), ray.width).as_mode(),
                    Transform::from_translation(Vec3::new(0.0, 0.0, transform.translation.z))
                ));
            }
        });
    }
}


#[derive(Component)]
pub struct LaserRays;
