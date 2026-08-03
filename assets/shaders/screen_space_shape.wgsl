#import bevy_sprite::{
    mesh2d_functions as mesh_functions,
    mesh2d_view_bindings::view,
}

#ifdef TONEMAP_IN_SHADER
#import bevy_core_pipeline::tonemapping
#endif
#ifdef SRGB_OUTPUT
#import bevy_render::color_operations::linear_to_srgb
#endif
#ifdef OKLAB_OUTPUT
#import bevy_render::color_operations::linear_rgb_to_oklab
#endif

struct Vertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) screen_offset: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let local_position = vec4<f32>(vertex.position, 1.0);
    let world_position = mesh_functions::mesh2d_position_local_to_world(
        world_from_local,
        local_position,
    );
    var clip_position = mesh_functions::mesh2d_position_world_to_clip(world_position);

    let requested_pixels = length(vertex.screen_offset);
    if requested_pixels > 0.0 {
        // The app uses an orthographic 2D camera, so a direction can be
        // transformed directly instead of projecting a second position.
        let clip_direction = view.clip_from_world
            * world_from_local
            * vec4<f32>(vertex.screen_offset, 0.0, 0.0);
        let pixel_direction = clip_direction.xy * view.viewport.zw * 0.5;
        let pixel_length_squared = dot(pixel_direction, pixel_direction);
        if pixel_length_squared > 0.0 {
            let pixel_offset = pixel_direction
                * inverseSqrt(pixel_length_squared)
                * requested_pixels;
            clip_position = vec4<f32>(
                clip_position.xy
                    + pixel_offset * 2.0 / view.viewport.zw * clip_position.w,
                clip_position.zw,
            );
        }
    }

    var out: VertexOutput;
    out.position = clip_position;
    out.color = vertex.color;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    var color = in.color;
#ifdef TONEMAP_IN_SHADER
    color = tonemapping::tone_mapping(color, view.color_grading);
#endif
#ifdef SRGB_OUTPUT
    color = vec4(linear_to_srgb(color.rgb), color.a);
#endif
#ifdef OKLAB_OUTPUT
    color = vec4(linear_rgb_to_oklab(color.rgb), color.a);
#endif
    return color;
}
