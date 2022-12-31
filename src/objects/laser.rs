use crate::{AsMode, ColorComponent, RefractiveIndex};
use bevy::hierarchy::{BuildChildren, DespawnRecursiveExt};
use bevy::math::{EulerRot, Vec2, Vec3, Vec3Swizzles};
use bevy::prelude::*;
use bevy_egui::egui::ecolor::Hsva;
use bevy_prototype_lyon::geometry::GeometryBuilder;
use bevy_prototype_lyon::shapes;
use bevy_rapier2d::prelude::{QueryFilter, RapierContext, RayIntersection};
use num_traits::float::FloatConst;
#[derive(Component)]
pub struct LaserBundle {
    pub(crate) fade_distance: f32,
}

#[derive(Debug)]
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

    fn end_strength(&self, parent: &LaserBundle) -> f32 {
        0.0f32
            .max(self.strength * (1.0 - self.length / (parent.fade_distance - self.start_distance)))
    }

    fn end_distance(&self) -> f32 {
        self.start_distance + self.length
    }
}

struct LaserCompute<'a, Refr: Fn(Entity) -> f32> {
    laser: &'a LaserBundle,
    rapier: &'a RapierContext,
    refractive_index: Refr,
    rays: Vec<LaserRay>,
}

const MAX_RAYS: usize = 100;

impl<'a, Refr: Fn(Entity) -> f32> LaserCompute<'a, Refr> {
    fn new(laser: &'a LaserBundle, rapier: &'a RapierContext, refr: Refr) -> Self {
        Self {
            laser,
            rapier,
            refractive_index: refr,
            rays: Vec::new(),
        }
    }

    fn shoot_ray(&mut self, mut ray: LaserRay, depth: usize) {
        if depth > MAX_RAYS {
            return;
        }

        ray.length = ray
            .length
            .min(self.laser.fade_distance - ray.start_distance);

        let mut intersection = None;
        let mut min_dist = f32::INFINITY;

        let ray_dir = Vec2::from_angle(ray.angle);

        self.rapier.intersections_with_ray(
            ray.start,
            ray_dir,
            ray.length_clipped(),
            false,
            QueryFilter::new().exclude_sensors(),
            |ent, inter| {
                if inter.toi < min_dist && inter.toi > 0.0001 {
                    intersection = Some((ent, inter));
                    min_dist = inter.toi;
                }
                true
            },
        );

        if let Some((
            ent,
            RayIntersection {
                toi,
                point,
                normal,
                feature,
            },
        )) = intersection
        {
            ray.length = toi;

            let normal_angle = normal.y.atan2(normal.x);
            let mut incidence_angle = ray.angle - normal_angle;
            if incidence_angle > f32::FRAC_PI_2() {
                incidence_angle -= f32::PI();
            } else if incidence_angle < -f32::FRAC_PI_2() {
                incidence_angle += f32::PI();
            }
            let reflected_angle = normal_angle - incidence_angle;

            let obj_index = (self.refractive_index)(ent);

            let opacity_refracted = (-obj_index.log10()).exp();
            let opacity_reflected = 1.0 - opacity_refracted;
            let test = Hsva {
                h: ray.color.h + 20.0,
                ..ray.color
            };
            let reflected_ray = LaserRay {
                start: point,
                angle: reflected_angle,
                length: f32::INFINITY,
                strength: ray.end_strength(self.laser) * opacity_reflected,
                color: test,
                width: ray.width,
                start_distance: ray.end_distance(),
                refractive_index: ray.refractive_index,
            };

            self.shoot_ray(reflected_ray, depth + 1);

            /*if f32::is_finite(obj_index) {
                let new_index =
            }*/
        }

        self.rays.push(ray);
    }

    fn end(self) -> Vec<LaserRay> {
        self.rays
    }
}

const LASER_WIDTH: f32 = 0.2;

pub fn draw_lasers(
    lasers: Query<(&Transform, &LaserBundle, &ColorComponent)>,
    refr: Query<&RefractiveIndex>,
    mut rays: Query<(Entity, &mut LaserRays)>,
    mut commands: Commands,
    rapier: Res<RapierContext>,
) {
    let (rays, mut rays_obj) = rays.single_mut();
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

        let mut compute = LaserCompute::new(laser, &rapier, |ent| refr.get(ent).unwrap().0);

        compute.shoot_ray(initial, 0);

        let ray_list = compute.end();

        let mut debug = String::new();

        commands.entity(rays).add_children(|builder| {
            for ray in ray_list {
                debug.push_str(&format!("{:?} ", ray));
                builder.spawn(GeometryBuilder::build_as(
                    &shapes::Line(ray.start, ray.end()),
                    crate::make_stroke(crate::hsva_to_rgba(ray.color), ray.width).as_mode(),
                    Transform::from_translation(Vec3::new(0.0, 0.0, transform.translation.z - 0.1)),
                ));
            }
        });

        rays_obj.debug = debug;
    }
}

#[derive(Component, Default)]
pub struct LaserRays {
    pub debug: String,
}
