use bevy::prelude::*;
use bevy::reflect::TypeUuid;
use bevy::render::render_resource::*;

static PLANE_RENDER: &str = include_str!("plane_render.wgsl");

const SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 15204473893972682982);

#[derive(Component)]
struct ExtractedInfiniteGrid {
    transform: GlobalTransform,
    //grid: InfiniteGrid,
}

#[derive(Debug, ShaderType)]
pub struct InfiniteGridUniform {
    rot_matrix: Mat3,
    offset: Vec3,
    normal: Vec3,
    scale: f32,
    // 1 / fadeout_distance
    dist_fadeout_const: f32,

    dot_fadeout_const: f32,

    x_axis_color: Vec3,
    z_axis_color: Vec3,
    minor_line_color: Vec4,
    major_line_color: Vec4,
}

#[derive(Resource, Default)]
struct InfiniteGridUniforms {
    uniforms: DynamicUniformBuffer<InfiniteGridUniform>,
}

#[derive(Component)]
struct InfiniteGridUniformOffset {
    offset: u32,
}

#[derive(Resource)]
struct InfiniteGridBindGroup {
    value: BindGroup,
}