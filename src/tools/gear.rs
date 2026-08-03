use std::f32::consts::{PI, TAU};

use bevy::prelude::*;
use bevy_prototype_lyon::prelude::tess::path::Path;

use crate::lyon_compat::{GeometryBuilder, filled_path_boundary_contours};

const MIN_TEETH: usize = 3;
const MAX_TEETH: usize = 4096;
const MIN_CIRCLE_SEGMENTS: usize = 32;
const TOOTH_HEIGHT_RATIO: f32 = 0.5;
const FLANK_ANGLE_FROM_RADIUS: f32 = PI / 6.0;

#[derive(Resource, Copy, Clone, Debug, PartialEq)]
pub struct GearSettings {
    pub teeth_size: f32,
    pub external: bool,
    pub internal: bool,
    pub hollow_thickness: f32,
}

impl Default for GearSettings {
    fn default() -> Self {
        Self {
            teeth_size: 0.2,
            external: true,
            internal: false,
            hollow_thickness: 0.4,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GearOutline {
    pub outer: Vec<Vec2>,
    pub inner: Option<Vec<Vec2>>,
    pub outer_radius: f32,
    pub external_teeth: usize,
    pub internal_teeth: usize,
}

impl GearOutline {
    /// Builds a radial gear whose tooth midline is as close as possible to
    /// `radius` while keeping an integer number of exact-size teeth.
    pub fn from_radius(radius: f32, settings: GearSettings) -> Option<Self> {
        if !radius.is_finite()
            || radius <= 0.0
            || !settings.teeth_size.is_finite()
            || settings.teeth_size <= 0.0
            || !settings.hollow_thickness.is_finite()
        {
            return None;
        }

        let teeth_size = settings.teeth_size;
        let tooth_height = teeth_size * TOOTH_HEIGHT_RATIO;
        let (outer, outer_radius, external_teeth, material_outer_radius) = if settings.external {
            let count = closest_external_tooth_count(radius, teeth_size);
            let outer_radius = external_radius(count, teeth_size);
            (
                toothed_contour(outer_radius - tooth_height, outer_radius, count)?,
                outer_radius,
                count,
                outer_radius - tooth_height,
            )
        } else {
            (circle_contour(radius, teeth_size), radius, 0, radius)
        };

        let (inner, internal_teeth) = if settings.internal {
            let root_radius = material_outer_radius - settings.hollow_thickness.max(0.0);
            let tip_radius = root_radius - tooth_height;
            match maximum_internal_tooth_count(root_radius, tip_radius, teeth_size).and_then(
                |count| {
                    toothed_contour(root_radius, tip_radius, count).map(|contour| (contour, count))
                },
            ) {
                Some((mut contour, count)) => {
                    contour.reverse();
                    (Some(contour), count)
                }
                None => (None, 0),
            }
        } else {
            (None, 0)
        };

        Some(Self {
            outer,
            inner,
            outer_radius,
            external_teeth,
            internal_teeth,
        })
    }

    pub fn path(&self) -> Path {
        path_from_contours(&self.outer, self.inner.as_deref())
    }
}

/// Adds one continuous, centered tooth row around every filled contour.
/// Contours are oriented with material on their left, so the same profile
/// naturally points out of outer boundaries and into holes.
pub fn gearify_path(path: &Path, teeth_size: f32) -> Option<Path> {
    if !teeth_size.is_finite() || teeth_size <= 0.0 {
        return None;
    }
    let contours = filled_path_boundary_contours(path)?;
    let toothed = contours
        .iter()
        .map(|contour| toothed_perimeter_contour(contour, teeth_size))
        .collect::<Option<Vec<_>>>()?;

    let mut builder = GeometryBuilder::new();
    for contour in &toothed {
        builder = add_contour(builder, contour);
    }
    Some(builder.build())
}

fn toothed_perimeter_contour(contour: &[Vec2], teeth_size: f32) -> Option<Vec<Vec2>> {
    let contour = simplified_contour(contour);
    if contour.len() < 3 {
        return None;
    }

    let tooth_height = teeth_size * TOOTH_HEIGHT_RATIO;
    let root_offset = -tooth_height * 0.5;
    let tip_offset = tooth_height * 0.5;
    let flank_run = tooth_height * FLANK_ANGLE_FROM_RADIUS.tan();
    let edge_lengths = (0..contour.len())
        .map(|index| contour[index].distance(contour[(index + 1) % contour.len()]))
        .collect::<Vec<_>>();
    let perimeter = edge_lengths.iter().sum::<f32>();
    let tooth_count = (perimeter / teeth_size).ceil() as usize;
    if !perimeter.is_finite() || perimeter <= f32::EPSILON || tooth_count > MAX_TEETH {
        return None;
    }

    // Tooth pitch remains exactly `teeth_size`. If the perimeter is not a
    // multiple of it, closing the contour creates the expected imperfect seam.
    let mut profile_events = Vec::with_capacity(tooth_count * 4);
    for tooth in 0..tooth_count {
        let start = tooth as f32 * teeth_size;
        for offset in [
            flank_run,
            teeth_size * 0.5,
            teeth_size * 0.5 + flank_run,
            teeth_size,
        ] {
            let distance = start + offset;
            if distance > 0.0 && distance < perimeter {
                profile_events.push(distance);
            }
        }
    }

    let mut points = Vec::new();
    let mut event_index = 0;
    let mut contour_distance = 0.0;

    for edge_index in 0..contour.len() {
        let from = contour[edge_index];
        let to = contour[(edge_index + 1) % contour.len()];
        let edge = to - from;
        let length = edge_lengths[edge_index];
        if !length.is_finite() || length <= f32::EPSILON {
            continue;
        }
        let direction = edge / length;
        let outward = Vec2::new(direction.y, -direction.x);
        let at = |distance: f32, offset: f32| from + direction * distance + outward * offset;
        let edge_end = contour_distance + length;

        points.push(at(
            0.0,
            tooth_profile_offset(
                contour_distance,
                teeth_size,
                flank_run,
                root_offset,
                tip_offset,
            ),
        ));
        while event_index < profile_events.len() && profile_events[event_index] < edge_end {
            let distance = profile_events[event_index];
            if distance > contour_distance {
                points.push(at(
                    distance - contour_distance,
                    tooth_profile_offset(distance, teeth_size, flank_run, root_offset, tip_offset),
                ));
            }
            event_index += 1;
        }
        points.push(at(
            length,
            tooth_profile_offset(edge_end, teeth_size, flank_run, root_offset, tip_offset),
        ));
        contour_distance = edge_end;
    }

    (points.len() >= 3).then_some(points)
}

fn tooth_profile_offset(
    distance: f32,
    teeth_size: f32,
    flank_run: f32,
    root_offset: f32,
    tip_offset: f32,
) -> f32 {
    let phase = distance.rem_euclid(teeth_size);
    if phase < flank_run {
        root_offset + (tip_offset - root_offset) * phase / flank_run
    } else if phase < teeth_size * 0.5 {
        tip_offset
    } else if phase < teeth_size * 0.5 + flank_run {
        tip_offset + (root_offset - tip_offset) * (phase - teeth_size * 0.5) / flank_run
    } else {
        root_offset
    }
}

fn simplified_contour(contour: &[Vec2]) -> Vec<Vec2> {
    let mut points = contour
        .iter()
        .copied()
        .fold(Vec::<Vec2>::new(), |mut points, point| {
            if points
                .last()
                .is_none_or(|previous| previous.distance_squared(point) > f32::EPSILON)
            {
                points.push(point);
            }
            points
        });
    if points.len() > 1 && points[0].distance_squared(*points.last().unwrap()) <= f32::EPSILON {
        points.pop();
    }

    loop {
        let Some(index) = (0..points.len()).find(|index| {
            let previous = points[(*index + points.len() - 1) % points.len()];
            let current = points[*index];
            let next = points[(*index + 1) % points.len()];
            let incoming = current - previous;
            let outgoing = next - current;
            let scale = incoming.length() * outgoing.length();
            scale <= f32::EPSILON
                || (incoming.dot(outgoing) > 0.0
                    && incoming.perp_dot(outgoing).abs() <= scale * 1.0e-5)
        }) else {
            break;
        };
        points.remove(index);
        if points.len() < 3 {
            break;
        }
    }
    points
}

fn path_from_contours(outer: &[Vec2], inner: Option<&[Vec2]>) -> Path {
    let mut builder = add_contour(GeometryBuilder::new(), outer);
    if let Some(inner) = inner {
        builder = add_contour(builder, inner);
    }
    builder.build()
}

fn add_contour(mut builder: GeometryBuilder, points: &[Vec2]) -> GeometryBuilder {
    let Some(first) = points.first().copied() else {
        return builder;
    };
    builder = builder.begin(first);
    for point in points.iter().skip(1).copied() {
        builder = builder.line_to(point);
    }
    builder.close()
}

fn toothed_contour(root_radius: f32, tip_radius: f32, count: usize) -> Option<Vec<Vec2>> {
    let pitch = TAU / count as f32;
    let flank_angle = flank_angle(root_radius, tip_radius)?;
    let flat_angle = (pitch - flank_angle * 2.0) * 0.5;
    if flat_angle <= 0.0 {
        return None;
    }
    let mut points = Vec::with_capacity(count * 4);
    for tooth in 0..count {
        let start = tooth as f32 * pitch;
        points.extend([
            Vec2::from_angle(start) * root_radius,
            Vec2::from_angle(start + flank_angle) * tip_radius,
            Vec2::from_angle(start + flank_angle + flat_angle) * tip_radius,
            Vec2::from_angle(start + flank_angle * 2.0 + flat_angle) * root_radius,
        ]);
    }
    Some(points)
}

fn flank_angle(root_radius: f32, tip_radius: f32) -> Option<f32> {
    if root_radius <= 0.0 || tip_radius <= 0.0 {
        return None;
    }
    let sin_angle = FLANK_ANGLE_FROM_RADIUS.sin();
    let ratio = root_radius / tip_radius * sin_angle;
    if ratio.abs() > 1.0 {
        return None;
    }
    let angle = if tip_radius > root_radius {
        FLANK_ANGLE_FROM_RADIUS - ratio.asin()
    } else {
        ratio.asin() - FLANK_ANGLE_FROM_RADIUS
    };
    angle.is_finite().then_some(angle)
}

fn circle_contour(radius: f32, teeth_size: f32) -> Vec<Vec2> {
    let segments =
        ((TAU * radius / teeth_size).ceil() as usize).clamp(MIN_CIRCLE_SEGMENTS, MAX_TEETH * 2);
    (0..segments)
        .map(|index| Vec2::from_angle(TAU * index as f32 / segments as f32) * radius)
        .collect()
}

fn external_radius(count: usize, teeth_size: f32) -> f32 {
    external_midline_radius(count, teeth_size) + teeth_size * TOOTH_HEIGHT_RATIO * 0.5
}

fn external_midline_radius(count: usize, teeth_size: f32) -> f32 {
    count as f32 * teeth_size / TAU
}

fn closest_external_tooth_count(radius: f32, teeth_size: f32) -> usize {
    let approximate = (radius * TAU / teeth_size).round().max(MIN_TEETH as f32) as usize;
    let approximate = approximate.min(MAX_TEETH);

    let first = approximate.saturating_sub(2).max(MIN_TEETH);
    let last = (approximate + 2).min(MAX_TEETH);
    (first..=last)
        .min_by(|a, b| {
            (external_midline_radius(*a, teeth_size) - radius)
                .abs()
                .total_cmp(&(external_midline_radius(*b, teeth_size) - radius).abs())
        })
        .unwrap_or(MIN_TEETH)
}

fn maximum_internal_tooth_count(
    root_radius: f32,
    tip_radius: f32,
    teeth_size: f32,
) -> Option<usize> {
    let flank_angle = flank_angle(root_radius, tip_radius)?;
    let midline_radius = (root_radius + tip_radius) * 0.5;
    let period_limit = (TAU * midline_radius / teeth_size).floor() as usize;
    let flank_limit = (TAU / (flank_angle * 2.0)).floor() as usize;
    let count = period_limit.min(flank_limit);
    (count >= MIN_TEETH).then_some(count.min(MAX_TEETH))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyon_compat::shapes;
    use crate::tools::polygon::tessellate_path;

    #[test]
    fn defaults_match_the_tool_panel() {
        assert_eq!(
            GearSettings::default(),
            GearSettings {
                teeth_size: 0.2,
                external: true,
                internal: false,
                hollow_thickness: 0.4,
            }
        );
    }

    #[test]
    fn external_radius_snaps_to_the_requested_spatial_period() {
        let settings = GearSettings::default();
        let outline = GearOutline::from_radius(2.03, settings).unwrap();
        let root_radius = outline.outer_radius - settings.teeth_size * TOOTH_HEIGHT_RATIO;
        let midline_radius = (outline.outer_radius + root_radius) * 0.5;
        let period = TAU * midline_radius / outline.external_teeth as f32;
        let rise = outline.outer[1] - outline.outer[0];

        assert!((period - settings.teeth_size).abs() < 1.0e-5);
        assert!((midline_radius - 2.03).abs() <= settings.teeth_size / (TAU * 2.0) + f32::EPSILON);
        assert!((outline.outer_radius - root_radius - settings.teeth_size * 0.5).abs() < 1.0e-5);
        assert!((rise.y.atan2(rise.x) - FLANK_ANGLE_FROM_RADIUS).abs() < 1.0e-5);
        assert_eq!(
            outline.outer_radius,
            external_radius(outline.external_teeth, settings.teeth_size)
        );
    }

    #[test]
    fn internal_outline_makes_a_real_hole_with_the_requested_web_thickness() {
        let settings = GearSettings {
            internal: true,
            ..GearSettings::default()
        };
        let outline = GearOutline::from_radius(3.0, settings).unwrap();
        let external_root = outline.outer_radius - settings.teeth_size * TOOTH_HEIGHT_RATIO;
        let internal_root = outline
            .inner
            .as_ref()
            .unwrap()
            .iter()
            .map(|point| point.length())
            .fold(0.0, f32::max);

        assert!((external_root - internal_root - settings.hollow_thickness).abs() < 1.0e-5);
        let internal_profile = toothed_contour(2.0, 1.9, 20).unwrap();
        let inward_rise = internal_profile[1] - internal_profile[0];
        assert!((inward_rise.y.atan2(-inward_rise.x) - FLANK_ANGLE_FROM_RADIUS).abs() < 1.0e-5);
        let filled = tessellate_path(&outline.path()).unwrap();
        assert!(filled.area < PI * outline.outer_radius.powi(2));
    }

    #[test]
    fn disabling_both_tooth_sets_produces_one_circle_like_contour() {
        let outline = GearOutline::from_radius(
            2.0,
            GearSettings {
                external: false,
                internal: false,
                ..GearSettings::default()
            },
        )
        .unwrap();

        assert!(outline.outer.len() >= MIN_CIRCLE_SEGMENTS);
        assert!(outline.inner.is_none());
        assert_eq!(outline.external_teeth, 0);
        assert_eq!(outline.internal_teeth, 0);
    }

    #[test]
    fn undersized_internal_gear_falls_back_to_a_solid_outer_shape() {
        let outline = GearOutline::from_radius(
            0.1,
            GearSettings {
                external: false,
                internal: true,
                ..GearSettings::default()
            },
        )
        .unwrap();

        assert!(outline.inner.is_none());
        assert_eq!(outline.internal_teeth, 0);
        assert!(!outline.outer.is_empty());
    }

    #[test]
    fn smallest_external_gear_has_three_teeth() {
        let outline = GearOutline::from_radius(0.001, GearSettings::default()).unwrap();

        assert_eq!(outline.external_teeth, 3);
        assert_eq!(outline.outer.len(), 12);
    }

    #[test]
    fn gearified_box_keeps_exact_pitch_and_grows_by_one_tooth_height() {
        let teeth_size = 0.2;
        let path = GeometryBuilder::build_as(&shapes::Rectangle {
            extents: Vec2::ONE,
            ..Default::default()
        });
        let gearified = gearify_path(&path, teeth_size).unwrap();
        let points = gearified
            .iter()
            .flat_map(|event| [event.from(), event.to()])
            .map(|point| Vec2::new(point.x, point.y))
            .collect::<Vec<_>>();
        let minimum = points
            .iter()
            .fold(Vec2::splat(f32::INFINITY), |a, b| a.min(*b));
        let maximum = points
            .iter()
            .fold(Vec2::splat(f32::NEG_INFINITY), |a, b| a.max(*b));

        assert!((minimum.x + 0.55).abs() < 1.0e-5);
        assert!((minimum.y + 0.55).abs() < 1.0e-5);
        assert!((maximum.x - 0.55).abs() < 1.0e-5);
        assert!((maximum.y - 0.55).abs() < 1.0e-5);

        let flank_run = teeth_size * TOOTH_HEIGHT_RATIO * FLANK_ANGLE_FROM_RADIUS.tan();
        let first_flanks = points
            .iter()
            .filter(|point| (point.y + 0.55).abs() < 1.0e-5)
            .map(|point| point.x)
            .filter(|x| *x < 0.0)
            .collect::<Vec<_>>();
        assert!(
            first_flanks
                .iter()
                .any(|x| (*x - (-0.5 + flank_run)).abs() < 1.0e-5)
        );
        assert!(
            first_flanks
                .iter()
                .any(|x| (*x - (-0.5 + teeth_size + flank_run)).abs() < 1.0e-5)
        );
    }

    #[test]
    fn gearification_teethes_outer_boundaries_and_holes() {
        let path = path_from_contours(
            &[
                Vec2::new(-2.0, -2.0),
                Vec2::new(2.0, -2.0),
                Vec2::new(2.0, 2.0),
                Vec2::new(-2.0, 2.0),
            ],
            Some(&[
                Vec2::new(-1.0, -1.0),
                Vec2::new(-1.0, 1.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(1.0, -1.0),
            ]),
        );
        let gearified = gearify_path(&path, 0.2).unwrap();
        let contours = filled_path_boundary_contours(&gearified).unwrap();
        let areas = contours
            .iter()
            .map(|contour| {
                contour
                    .iter()
                    .zip(contour.iter().cycle().skip(1))
                    .map(|(a, b)| a.perp_dot(*b))
                    .sum::<f32>()
                    * 0.5
            })
            .collect::<Vec<_>>();

        let substantial = contours
            .iter()
            .zip(&areas)
            .filter(|(_, area)| area.abs() > 0.1)
            .collect::<Vec<_>>();
        assert_eq!(substantial.len(), 2, "areas={areas:?}");
        assert!(substantial.iter().all(|(contour, _)| contour.len() > 4));
        assert_eq!(
            substantial.iter().filter(|(_, area)| **area > 0.0).count(),
            1
        );
        assert_eq!(
            substantial.iter().filter(|(_, area)| **area < 0.0).count(),
            1
        );
    }

    #[test]
    fn gearification_keeps_fractional_remainder_at_the_seam() {
        let teeth_size = 0.3;
        let tooth_height = teeth_size * TOOTH_HEIGHT_RATIO;
        let flank_run = tooth_height * FLANK_ANGLE_FROM_RADIUS.tan();
        let root = -tooth_height * 0.5;
        let tip = tooth_height * 0.5;
        let offset = |distance| tooth_profile_offset(distance, teeth_size, flank_run, root, tip);

        assert!((offset(0.07) - offset(0.07 + teeth_size)).abs() < 1.0e-6);
        assert!((offset(4.0) - offset(0.0)).abs() > 1.0e-3);
    }

    #[test]
    fn gearification_handles_a_densely_sampled_figure_eight() {
        let sample_count = 96;
        let point_at = |index: usize| {
            let angle = index as f32 * TAU / sample_count as f32;
            Vec2::new(2.0 * angle.sin(), angle.sin() * angle.cos())
        };
        let mut builder = GeometryBuilder::new().begin(point_at(0));
        for index in 1..sample_count {
            builder = builder.line_to(point_at(index));
        }
        let source = builder.close().build();

        let gearified = gearify_path(&source, 0.2).expect("figure eight should gearify");
        let contours = filled_path_boundary_contours(&gearified).unwrap();
        let substantial_contours = contours
            .iter()
            .filter(|contour| {
                contour
                    .iter()
                    .zip(contour.iter().cycle().skip(1))
                    .map(|(from, to)| from.perp_dot(*to))
                    .sum::<f32>()
                    .abs()
                    * 0.5
                    > 1.0
            })
            .count();

        assert_eq!(substantial_contours, 2);
        assert!(crate::tools::polygon::tessellate_path(&gearified).is_some());
    }

    #[test]
    fn gearification_handles_the_polygon_tools_repeated_figure_eight_points() {
        let source = crate::tools::polygon::polygon_path(
            &[
                Vec2::new(-2.0, 0.0),
                Vec2::new(-1.0, 1.0),
                Vec2::new(0.0, 0.0),
                Vec2::new(-1.0, -1.0),
                Vec2::new(-2.0, 0.0),
                Vec2::new(0.0, 0.0),
                Vec2::new(1.0, 1.0),
                Vec2::new(2.0, 0.0),
                Vec2::new(1.0, -1.0),
                Vec2::new(0.0, 0.0),
            ],
            true,
        );

        assert!(gearify_path(&source, 0.2).is_some());
    }

    #[test]
    fn gearification_does_not_abort_on_sampled_self_intersecting_polygons() {
        let sample_count = 64;
        for x_frequency in 1..=4 {
            for y_frequency in 1..=4 {
                for phase_index in 0..4 {
                    let point_at = |index: usize| {
                        let angle = index as f32 * TAU / sample_count as f32;
                        let phase = phase_index as f32 * 0.17;
                        Vec2::new(
                            2.0 * (x_frequency as f32 * angle + phase).sin(),
                            (y_frequency as f32 * angle).sin(),
                        )
                    };
                    let mut builder = GeometryBuilder::new().begin(point_at(0));
                    for index in 1..sample_count {
                        builder = builder.line_to(point_at(index));
                    }
                    let source = builder.close().build();
                    if crate::tools::polygon::tessellate_path(&source).is_some() {
                        assert!(
                            gearify_path(&source, 0.2).is_some(),
                            "frequencies=({x_frequency}, {y_frequency}), phase={phase_index}"
                        );
                    }
                }
            }
        }
    }
}
