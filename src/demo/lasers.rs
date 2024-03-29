use crate::tools::add_object::AddObjectEvent;
use bevy::prelude::*;

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
}
