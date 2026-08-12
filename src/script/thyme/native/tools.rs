use super::*;

native_class!(pub(super) TOOLS = "Tools", []);

native_class!(
    pub(super) DRAG_TOOL = "DragTool",
    [
        resource_property!("centerOfMass", DragConfig, drag_center_of_mass, bool),
        resource_property!("maxForce", DragConfig, max_force, float),
        resource_property!("strength", DragConfig, strength, float),
    ]
);

native_class!(
    pub(super) GEAR_TOOL = "GearTool",
    [
        resource_property!("cogSize", GearSettings, teeth_size, float),
        resource_property!("inside", GearSettings, internal, bool),
        resource_property!("outside", GearSettings, external, bool),
        resource_property!("thickness", GearSettings, hollow_thickness, float),
    ]
);
