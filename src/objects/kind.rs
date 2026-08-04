use bevy::ecs::{query::QueryData, system::SystemParam};
use bevy::prelude::*;

use crate::objects::axle::{AxleObject, FixObject};
use crate::objects::phy_obj::{CircleVisual, FreeformObject};
use crate::objects::plane::PlaneObject;
use crate::objects::spring::{SpringEndHandle, SpringObject};
use crate::tools::add_object::AttachmentKind;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ObjectKind {
    Polygon,
    Box,
    Circle,
    Plane,
    Spring,
    FixJoint,
    Axle,
    Tracer,
    LaserPen,
    Thruster,
}

impl ObjectKind {
    pub(crate) const ALL: [Self; 10] = [
        Self::Polygon,
        Self::Box,
        Self::Circle,
        Self::Plane,
        Self::Spring,
        Self::FixJoint,
        Self::Axle,
        Self::Tracer,
        Self::LaserPen,
        Self::Thruster,
    ];

    pub(crate) const fn singular(self) -> &'static str {
        match self {
            Self::Polygon => "Polygon",
            Self::Box => "Box",
            Self::Circle => "Circle",
            Self::Plane => "Plane",
            Self::Spring => "Spring",
            Self::FixJoint => "Fix joint",
            Self::Axle => "Axle",
            Self::Tracer => "Tracer",
            Self::LaserPen => "Laser pen",
            Self::Thruster => "Thruster",
        }
    }

    pub(crate) const fn plural(self) -> &'static str {
        match self {
            Self::Polygon => "polygons",
            Self::Box => "boxes",
            Self::Circle => "circles",
            Self::Plane => "planes",
            Self::Spring => "springs",
            Self::FixJoint => "fix joints",
            Self::Axle => "axles",
            Self::Tracer => "tracers",
            Self::LaserPen => "laser pens",
            Self::Thruster => "thrusters",
        }
    }

    pub(crate) const fn is_geometry(self) -> bool {
        matches!(self, Self::Polygon | Self::Box | Self::Circle | Self::Plane)
    }

    pub(crate) fn title(self, count: usize) -> String {
        if count == 1 {
            self.singular().to_owned()
        } else {
            format!("{count} {}", self.plural())
        }
    }
}

#[derive(QueryData)]
struct ObjectKindData {
    attachment: Option<&'static AttachmentKind>,
    spring: Option<&'static SpringObject>,
    spring_end: Option<&'static SpringEndHandle>,
    plane: Option<&'static PlaneObject>,
    polygon: Option<&'static FreeformObject>,
    circle: Option<&'static CircleVisual>,
    axle: Option<&'static AxleObject>,
    fix: Option<&'static FixObject>,
}

#[derive(SystemParam)]
pub(crate) struct ObjectKinds<'w, 's> {
    objects: Query<'w, 's, ObjectKindData>,
}

impl ObjectKinds<'_, '_> {
    pub(crate) fn get(&self, entity: Entity) -> Option<ObjectKind> {
        let object = self.objects.get(entity).ok()?;
        if let Some(attachment) = object.attachment {
            return Some(match attachment {
                AttachmentKind::Fix => ObjectKind::FixJoint,
                AttachmentKind::Axle => ObjectKind::Axle,
                AttachmentKind::Laser => ObjectKind::LaserPen,
                AttachmentKind::Thruster => ObjectKind::Thruster,
                AttachmentKind::Tracer => ObjectKind::Tracer,
            });
        }
        if object.spring.is_some() {
            return Some(ObjectKind::Spring);
        }
        if object.spring_end.is_some() {
            return None;
        }
        if object.axle.is_some() {
            return Some(ObjectKind::Axle);
        }
        if object.fix.is_some() {
            return Some(ObjectKind::FixJoint);
        }
        if object.plane.is_some() {
            return Some(ObjectKind::Plane);
        }
        if object.polygon.is_some() {
            return Some(ObjectKind::Polygon);
        }
        object.circle.map(|circle| {
            if circle.0 > 0.0 {
                ObjectKind::Circle
            } else {
                ObjectKind::Box
            }
        })
    }

    pub(crate) fn selection_title(&self, entities: impl IntoIterator<Item = Entity>) -> String {
        let mut kinds = Vec::new();
        let mut unknown_objects = 0;
        for entity in entities {
            if let Some(kind) = self.get(entity) {
                kinds.push(kind);
            } else if self
                .objects
                .get(entity)
                .is_ok_and(|object| object.spring_end.is_none())
            {
                unknown_objects += 1;
            }
        }
        selection_title(&kinds, unknown_objects)
    }
}

fn selection_title(kinds: &[ObjectKind], unknown_objects: usize) -> String {
    let count = kinds.len() + unknown_objects;
    if count == 0 {
        return "Object".to_owned();
    }
    if unknown_objects == 0 && kinds.iter().all(|kind| *kind == kinds[0]) {
        return kinds[0].title(count);
    }
    if unknown_objects == 0 && kinds.iter().all(|kind| kind.is_geometry()) {
        return format!("{count} geometries");
    }
    if count == 1 {
        "Object".to_owned()
    } else {
        format!("{count} objects")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_use_specific_geometry_and_generic_object_levels() {
        assert_eq!(selection_title(&[ObjectKind::Box], 0), "Box");
        assert_eq!(
            selection_title(&[ObjectKind::Box, ObjectKind::Box], 0),
            "2 boxes"
        );
        assert_eq!(
            selection_title(&[ObjectKind::Box, ObjectKind::Box, ObjectKind::Circle], 0),
            "3 geometries"
        );
        assert_eq!(
            selection_title(&[ObjectKind::Box, ObjectKind::Axle], 0),
            "2 objects"
        );
    }
}
