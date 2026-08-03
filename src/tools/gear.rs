use std::f32::consts::{PI, TAU};

use bevy::prelude::*;
use bevy_prototype_lyon::prelude::tess::path::Path;

use crate::lyon_compat::GeometryBuilder;

const MIN_TEETH: usize = 4;
const MAX_TEETH: usize = 4096;
const MIN_CIRCLE_SEGMENTS: usize = 32;
const TOOTH_DEPTH_RATIO: f32 = 0.25;

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
        let tooth_depth = teeth_size * TOOTH_DEPTH_RATIO;
        let (outer, outer_radius, external_teeth, material_outer_radius) = if settings.external {
            let count = closest_external_tooth_count(radius, teeth_size);
            let outer_radius = external_radius(count, teeth_size);
            (
                toothed_contour(outer_radius - tooth_depth, outer_radius, count, tooth_depth),
                outer_radius,
                count,
                outer_radius - tooth_depth,
            )
        } else {
            (circle_contour(radius, teeth_size), radius, 0, radius)
        };

        let (inner, internal_teeth) = if settings.internal {
            let root_radius = material_outer_radius - settings.hollow_thickness.max(0.0);
            let tip_radius = root_radius - tooth_depth;
            let count = maximum_internal_tooth_count(tip_radius, teeth_size)?;
            let mut contour = toothed_contour(root_radius, tip_radius, count, tooth_depth);
            contour.reverse();
            (Some(contour), count)
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
        let mut builder = add_contour(GeometryBuilder::new(), &self.outer);
        if let Some(inner) = &self.inner {
            builder = add_contour(builder, inner);
        }
        builder.build()
    }
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

fn toothed_contour(root_radius: f32, tip_radius: f32, count: usize, tooth_depth: f32) -> Vec<Vec2> {
    let pitch = TAU / count as f32;
    let top_angle = pitch * 0.5;
    let flank_angle = if tip_radius > root_radius {
        let ratio = (1.0 - tooth_depth / tip_radius) / 2.0_f32.sqrt();
        ratio.clamp(-1.0, 1.0).acos() - PI * 0.25
    } else {
        let ratio = (root_radius / tip_radius) / 2.0_f32.sqrt();
        PI * 0.25 - ratio.clamp(-1.0, 1.0).acos()
    };
    let mut points = Vec::with_capacity(count * 4);
    for tooth in 0..count {
        let start = tooth as f32 * pitch;
        points.extend([
            Vec2::from_angle(start) * root_radius,
            Vec2::from_angle(start + flank_angle) * tip_radius,
            Vec2::from_angle(start + flank_angle + top_angle) * tip_radius,
            Vec2::from_angle(start + flank_angle * 2.0 + top_angle) * root_radius,
        ]);
    }
    points
}

fn circle_contour(radius: f32, teeth_size: f32) -> Vec<Vec2> {
    let segments =
        ((TAU * radius / teeth_size).ceil() as usize).clamp(MIN_CIRCLE_SEGMENTS, MAX_TEETH * 2);
    (0..segments)
        .map(|index| Vec2::from_angle(TAU * index as f32 / segments as f32) * radius)
        .collect()
}

fn external_radius(count: usize, teeth_size: f32) -> f32 {
    teeth_size / (2.0 * (PI / (2.0 * count as f32)).sin())
}

fn closest_external_tooth_count(radius: f32, teeth_size: f32) -> usize {
    let ratio = teeth_size / (2.0 * radius);
    let approximate = if ratio < 1.0 {
        (PI / (2.0 * ratio.asin())).round() as usize
    } else {
        MIN_TEETH
    }
    .clamp(MIN_TEETH, MAX_TEETH);

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

fn maximum_internal_tooth_count(tip_radius: f32, teeth_size: f32) -> Option<usize> {
    if tip_radius <= 0.0 {
        return None;
    }
    let ratio = teeth_size / (2.0 * tip_radius);
    if ratio >= 1.0 {
        return None;
    }
    let count = (PI / (2.0 * ratio.asin())).floor() as usize;
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
    fn external_radius_snaps_and_each_tooth_top_has_the_requested_size() {
        let settings = GearSettings::default();
        let outline = GearOutline::from_radius(2.03, settings).unwrap();
        let first_top = outline.outer[1].distance(outline.outer[2]);
        let first_gap = outline.outer[2].distance(outline.outer[5]);

        assert!((first_top - settings.teeth_size).abs() < 1.0e-5);
        assert!((first_gap - settings.teeth_size).abs() < 1.0e-5);
        let rise = outline.outer[1] - outline.outer[0];
        assert!((rise.x - rise.y).abs() < 1.0e-5);
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
        let external_root = outline.outer_radius - settings.teeth_size * TOOTH_DEPTH_RATIO;
        let internal_root = outline
            .inner
            .as_ref()
            .unwrap()
            .iter()
            .map(|point| point.length())
            .fold(0.0, f32::max);

        assert!((external_root - internal_root - settings.hollow_thickness).abs() < 1.0e-5);
        let internal_profile = toothed_contour(2.0, 1.95, 20, 0.05);
        let inward_rise = internal_profile[1] - internal_profile[0];
        assert!((inward_rise.x.abs() - inward_rise.y).abs() < 1.0e-5);
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
}
