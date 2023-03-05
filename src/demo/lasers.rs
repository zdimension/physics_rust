use crate::tools::add_object::AddObjectEvent;
use bevy::prelude::*;
use bevy_prototype_lyon::entity::ShapeBundle;
use lyon_path::Path;
use lyon_path::path::Builder;
use crate::AsMode;
use crate::ui::selection_overlay::CircleSector;

pub fn init(commands: &mut Commands) {
    commands.add(|w: &mut World| {
        let mut ev = w.resource_mut::<Events<_>>();
        ev.send(AddObjectEvent::Laser(Vec2::new(-1.0, 6.5)));

        ev.send(AddObjectEvent::Polygon {
            pos: Vec2::new(1.0, 5.66),
            points: vec![
                Vec2::new(-1.0, -1.0),
                Vec2::new(1.0, -1.0),
                Vec2::new(0.0, 1.0),
            ],
        });

        ev.send(AddObjectEvent::Box {
            pos: Default::default(),
            size: Vec2::new(1.0, 1.0),
        });
    });
    use bevy_prototype_lyon::prelude::Path;
   /* commands.spawn(ShapeBundle {
        path: Path(CircleSector {
            radius: 10.0,
            center: Vec2::ZERO,
            start_angle: 0.0,
            end_angle: -2.0,
        }.add_geometry(Builder::new())),
        mode: crate::make_fill(Color::rgb_u8(0xff, 0xa0, 0xff)).as_mode(),
        transform: Transform::from_translation(Vec3::new(1.0, 1.0, 1.0)),
        ..Default::default()
    });*/
}
