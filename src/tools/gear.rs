use std::f32::consts::{PI, TAU};

use bevy::prelude::*;
use bevy_prototype_lyon::prelude::tess::path::Path;

use crate::lyon_compat::GeometryBuilder;

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

    /// Builds the painted outline with a centered stroke kept wholly inside the
    /// collision outline.
    pub fn visual_path(&self, stroke_width: f32) -> Path {
        let inset = stroke_width.max(0.0) * 0.5;
        let outer = offset_contour(&self.outer, inset);
        let inner = self
            .inner
            .as_deref()
            .map(|contour| offset_contour(contour, inset));
        path_from_contours(&outer, inner.as_deref())
    }
}

fn path_from_contours(outer: &[Vec2], inner: Option<&[Vec2]>) -> Path {
    let mut builder = add_contour(GeometryBuilder::new(), outer);
    if let Some(inner) = inner {
        builder = add_contour(builder, inner);
    }
    builder.build()
}

/// Offsets toward the left side of every directed edge. Gear contours are
/// wound so that this is always toward their material: inward for the outer
/// contour and outward for a hole.
fn offset_contour(points: &[Vec2], distance: f32) -> Vec<Vec2> {
    if points.len() < 3 || distance == 0.0 {
        return points.to_vec();
    }

    (0..points.len())
        .map(|index| {
            let previous = points[(index + points.len() - 1) % points.len()];
            let point = points[index];
            let next = points[(index + 1) % points.len()];
            let previous_direction = (point - previous).normalize_or_zero();
            let next_direction = (next - point).normalize_or_zero();
            let previous_offset = previous + previous_direction.perp() * distance;
            let next_offset = point + next_direction.perp() * distance;
            let denominator = cross(previous_direction, next_direction);

            if denominator.abs() <= 1.0e-6 {
                point + next_direction.perp() * distance
            } else {
                previous_offset
                    + previous_direction
                        * (cross(next_offset - previous_offset, next_direction) / denominator)
            }
        })
        .collect()
}

fn cross(left: Vec2, right: Vec2) -> f32 {
    left.x * right.y - left.y * right.x
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
    count as f32 * teeth_size / TAU + teeth_size * TOOTH_HEIGHT_RATIO * 0.5
}

fn closest_external_tooth_count(radius: f32, teeth_size: f32) -> usize {
    let approximate = ((radius - teeth_size * TOOTH_HEIGHT_RATIO * 0.5) * TAU / teeth_size)
        .round()
        .max(MIN_TEETH as f32) as usize;
    let approximate = approximate.min(MAX_TEETH);

    let first = approximate.saturating_sub(2).max(MIN_TEETH);
    let last = (approximate + 2).min(MAX_TEETH);
    (first..=last)
        .min_by(|a, b| {
            (external_radius(*a, teeth_size) - radius)
                .abs()
                .total_cmp(&(external_radius(*b, teeth_size) - radius).abs())
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
    fn contour_offset_moves_both_boundaries_into_the_material() {
        let outer = [
            Vec2::new(-1.0, -1.0),
            Vec2::new(1.0, -1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(-1.0, 1.0),
        ];
        let inner = outer.into_iter().rev().collect::<Vec<_>>();

        let inset_outer = offset_contour(&outer, 0.1);
        let inset_inner = offset_contour(&inner, 0.1);

        assert!(
            inset_outer
                .iter()
                .all(|point| point.abs().max_element() < 1.0)
        );
        assert!(
            inset_inner
                .iter()
                .all(|point| point.abs().max_element() > 1.0)
        );
    }
}
