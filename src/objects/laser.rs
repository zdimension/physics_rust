use std::fmt::{Debug, Formatter};

use bevy::hierarchy::{BuildChildren, DespawnRecursiveExt};
use bevy::math::{EulerRot, Vec2, Vec3, Vec3Swizzles};
use bevy::prelude::*;
use bevy_egui::egui::ecolor::Hsva;
use bevy_prototype_lyon::geometry::GeometryBuilder;
use bevy_prototype_lyon::prelude::ShapeBundle;
use bevy_prototype_lyon::shapes;
use bevy_rapier2d::prelude::{QueryFilter, RapierContext, RayIntersection};
use num_traits::float::FloatConst;

use crate::objects::phy_obj::RefractiveIndex;
use crate::objects::ColorComponent;

#[derive(Component)]
pub struct LaserBundle {
    pub(crate) fade_distance: f32,
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
    kind: RayKind,
    num: usize,
    source: usize,
    start_angle: f32,
    end_angle: f32,
}

#[derive(Debug)]
enum RayKind {
    Laser,
    Reflected,
    Refracted,
}

impl Debug for LaserRay {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:2} ({:2}): {{ {:.3}, {:.1}°, L={:.1}m, {:.1}%, w: {:.1}m, n: {:?}, {:?}, S: {:.1}°, E: {:.1}° }}",
            self.num, self.source, self.start, self.angle.to_degrees(), self.length, self.strength * 100.0, self.width, self.refractive_index, self.kind,
            self.start_angle.to_degrees(), self.end_angle.to_degrees()
        )
    }
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
        assert!(self.start.is_finite());
        assert!(self.angle.is_finite());
        self.start + Vec2::from_angle(self.angle) * self.length_clipped()
    }

    fn end_strength(&self, parent: &LaserBundle) -> f32 {
        0.0f32
            .max(self.strength * (1.0 - self.length / (parent.fade_distance - self.start_distance)))
    }

    fn end_distance(&self) -> f32 {
        self.start_distance + self.length
    }

    fn color_blended(&self) -> Hsva {
        Hsva::new(self.color.h, self.color.s, self.color.v, self.strength)
    }
}

struct ObjectInfo {
    refractive_index: f32,
    color: Hsva,
}

struct LaserCompute<'a, ObjInfo: Fn(Entity) -> ObjectInfo> {
    laser: &'a LaserBundle,
    rapier: &'a RapierContext,
    object_info: ObjInfo,
    rays: Vec<LaserRay>,
}

const MAX_RAYS: usize = 1000;

impl<'a, ObjInfo: Fn(Entity) -> ObjectInfo> LaserCompute<'a, ObjInfo> {
    fn new(laser: &'a LaserBundle, rapier: &'a RapierContext, object_info: ObjInfo) -> Self {
        Self {
            laser,
            rapier,
            object_info,
            rays: Vec::new(),
        }
    }

    fn shoot_ray(&mut self, mut ray: LaserRay, ray_count: &mut usize) {
        if *ray_count > MAX_RAYS {
            return;
        }

        if ray.strength < STRENGTH_EPSILON {
            return;
        }

        *ray_count += 1;

        ray.length = ray
            .length
            .min(self.laser.fade_distance - ray.start_distance);

        let mut intersection = None;
        let mut min_dist = f32::INFINITY;

        let ray_dir = Vec2::from_angle(ray.angle);

        self.rapier.intersections_with_ray(
            ray.start + 0.0001 * ray_dir,
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
                feature: _,
            },
        )) = intersection
        {
            ray.length = toi;

            let normal = if normal.dot(ray_dir) > 0.0 {
                -normal
            } else {
                normal
            };

            let normal_angle = normal.y.atan2(normal.x);

            let mut inside_object = false;
            self.rapier.intersections_with_point(
                ray.start + (toi / 2.0) * ray_dir,
                QueryFilter::default().predicate(&|scrutinee| scrutinee == ent),
                |_| {
                    inside_object = true;
                    false
                },
            );

            let incidence_angle = (f32::PI() + ray.angle) - normal_angle;

            let reflected_angle = normal_angle - incidence_angle;

            ray.end_angle = incidence_angle;

            let ObjectInfo {
                refractive_index: obj_index,
                color: obj_color,
            } = (self.object_info)(ent);

            let opacity_refracted = (-obj_index.log10()).exp();
            let opacity_reflected = 1.0 - opacity_refracted;
            let reflected_ray = LaserRay {
                start: point,
                angle: reflected_angle,
                length: f32::INFINITY,
                strength: ray.end_strength(self.laser) * opacity_reflected,
                color: ray.color_blended(),
                width: ray.width,
                start_distance: ray.end_distance(),
                refractive_index: ray.refractive_index,
                kind: RayKind::Reflected,
                num: *ray_count,
                source: ray.num,
                start_angle: -incidence_angle,
                end_angle: 0.0,
            };

            self.shoot_ray(reflected_ray, ray_count);

            if f32::is_finite(obj_index) {
                let obj_index = if inside_object {
                    let mut object_other = None;
                    self.rapier.intersections_with_point(
                        point,
                        QueryFilter::default()
                            .exclude_sensors()
                            .predicate(&|scrutinee| scrutinee != ent),
                        |ent| {
                            object_other = Some(ent);
                            false
                        },
                    );
                    match object_other {
                        Some(ent) => (self.object_info)(ent).refractive_index,
                        None => 1.0,
                    }
                } else {
                    obj_index
                };

                // todo: make sure total color strength is bounded by the incident ray
                let strength = ray.end_strength(self.laser) * opacity_refracted;

                let alpha_inv = 1.0 - obj_color.a;
                let color_strength = |_hue| alpha_inv;

                let rainbow_strength = strength * (1.0 - ray.color.s) * RAINBOW_SPLIT_MULT;
                let refraction_strength = strength * ray.color.s;

                let mut shoot_refracted_ray = |color: Hsva, base_strength| {
                    let ref_index = adjust_index(obj_index, color.h);
                    if let Some(ref_angle) =
                        compute_new_angle(incidence_angle, ray.refractive_index, obj_index)
                    {
                        let refracted_ray = LaserRay {
                            start: point,
                            angle: (normal_angle - f32::PI()) + ref_angle,
                            length: f32::INFINITY,
                            strength: base_strength * color_strength(color.h),
                            color: color,
                            width: refraction_thickness(ray.width, incidence_angle, ref_angle),
                            start_distance: ray.end_distance(),
                            refractive_index: ref_index,
                            kind: RayKind::Refracted,
                            num: *ray_count,
                            source: ray.num,
                            start_angle: ref_angle,
                            end_angle: 0.0,
                        };

                        self.shoot_ray(refracted_ray, ray_count);
                    }
                };

                if refraction_strength > 0.0 {
                    shoot_refracted_ray(ray.color, refraction_strength);
                }

                if rainbow_strength > 0.0 {
                    let mut color = Hsva::new(0.0, 1.0, 1.0, 1.0);

                    for i in 0..COLORS_IN_RAINBOW {
                        color.h = 0.5 * (2.0 * i as f32 + 1.0) / COLORS_IN_RAINBOW as f32;
                        shoot_refracted_ray(color, rainbow_strength);
                    }
                }
            }
        }

        self.rays.push(ray);
    }

    fn end(self) -> Vec<LaserRay> {
        self.rays
    }
}

const STRENGTH_EPSILON: f32 = 0.9 / 255.0;
const RAINBOW_SPLIT_MULT: f32 = 1.0 / 3.0;
const COLORS_IN_RAINBOW: usize = 12;

fn refraction_thickness(thickness: f32, angle_in: f32, angle_out: f32) -> f32 {
    let miter_width = thickness / angle_in.cos();
    miter_width * angle_out.cos()
    //thickness * (angle - side_angle).sin() / (side_angle - angle + f32::FRAC_PI_2()).cos()
}

fn adjust_index(base_index: f32, hue: f32) -> f32 {
    let hue_360 = hue * 360.0;
    base_index + (1.206e-4 * (hue_360 - 180.0) * (base_index * base_index))
}

fn compute_new_angle(incidence: f32, index_ray: f32, index_new: f32) -> Option<f32> {
    let new_sin = incidence.sin() * index_ray / index_new;
    if !(-1.0..=1.0).contains(&new_sin) {
        None // total internal reflection
    } else {
        let new_angle = new_sin.asin();
        assert!(
            new_angle.is_finite(),
            "new_sin: {}, incidence: {}, index_ray: {}, index_new: {}",
            new_sin,
            incidence,
            index_ray,
            index_new
        );
        Some(new_angle)
    }
}

const LASER_WIDTH: f32 = 0.2;

pub fn draw_lasers(
    lasers: Query<(&Transform, &LaserBundle, &ColorComponent)>,
    refr: Query<(&RefractiveIndex, &ColorComponent), Without<LaserBundle>>,
    mut rays: Query<(Entity, &mut LaserRays)>,
    mut commands: Commands,
    rapier: Res<RapierContext>,
) {
    let (rays, mut rays_obj) = rays.single_mut();
    commands.entity(rays).despawn_descendants();

    for (transform, laser, color) in lasers.iter() {
        let ray_width = transform.scale.x * LASER_WIDTH;

        let start = transform.transform_point(Vec3::new(0.5, 0.0, 1.0)).xy();
        let mut object_other = None;
        rapier.intersections_with_point(start, QueryFilter::default().exclude_sensors(), |ent| {
            object_other = Some(ent);
            false
        });
        let start_index = match object_other {
            Some(ent) => refr.get(ent).unwrap().0 .0,
            None => 1.0,
        };

        let initial = LaserRay {
            start,
            angle: transform.rotation.to_euler(EulerRot::XYZ).2,
            length: laser.fade_distance,
            strength: 1.0,
            color: color.0,
            width: ray_width / 2.0,
            start_distance: 0.0,
            refractive_index: start_index,
            kind: RayKind::Laser,
            num: 0,
            source: 0,
            start_angle: 0.0,
            end_angle: 0.0,
        };

        let mut compute = LaserCompute::new(laser, &rapier, |ent| {
            let (refr, col) = refr.get(ent).unwrap();
            ObjectInfo {
                refractive_index: refr.0,
                color: col.0,
            }
        });

        let mut ray_count = 0;
        compute.shoot_ray(initial, &mut ray_count);

        let ray_list = compute.end();

        let mut debug = String::new();

        for ray in ray_list {
            debug.push_str(&format!("{:?}\n", ray));
            let start = ray.start;
            let end = ray.end();
            let thick = ray.width; // todo: lazer_fuzziness
            let halfthick = thick / 2.0;
            let dir = (end - start).normalize();
            let norm = dir.perp() * halfthick;
            let diff_start = dir * halfthick * ray.start_angle.tan();
            let diff_end = dir * halfthick * ray.end_angle.tan();
            let poly = shapes::Polygon {
                points: vec![
                    start + norm + diff_start,
                    start - norm - diff_start,
                    end - norm - diff_end,
                    end + norm + diff_end,
                ],
                closed: true,
            };
            commands
                .spawn((
                    ShapeBundle {
                        path: GeometryBuilder::build_as(&poly),
                        transform: Transform::from_translation(Vec3::new(
                            0.0,
                            0.0,
                            transform.translation.z - 0.1,
                        )),
                        ..Default::default()
                    },
                    crate::make_fill(crate::hsva_to_rgba(ray.color_blended())),
                ))
                .set_parent(rays);
        }

        rays_obj.debug = debug;
    }
}

#[derive(Component, Default)]
pub struct LaserRays {
    pub debug: String,
}
