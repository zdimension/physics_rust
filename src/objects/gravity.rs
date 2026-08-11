use avian2d::prelude::Gravity;
use bevy::prelude::*;

#[derive(Resource, Copy, Clone)]
pub(crate) struct GravitySetting {
    pub(crate) strength: f32,
    pub(crate) direction: f32,
    pub(crate) enabled: bool,
}

impl GravitySetting {
    pub(crate) fn vector(self) -> Vec2 {
        if self.enabled {
            Vec2::from_angle(self.direction) * self.strength
        } else {
            Vec2::ZERO
        }
    }
}

impl Default for GravitySetting {
    fn default() -> Self {
        Self {
            strength: 9.81,
            direction: -std::f32::consts::FRAC_PI_2,
            enabled: true,
        }
    }
}

fn sync(settings: Res<GravitySetting>, mut gravity: ResMut<Gravity>) {
    if settings.is_changed() {
        gravity.0 = settings.vector();
    }
}

pub(super) fn add_systems(app: &mut App) {
    app.init_resource::<GravitySetting>()
        .init_resource::<Gravity>()
        .add_systems(
            PreUpdate,
            sync.after(crate::script::thyme::evaluate_bindings),
        );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_drives_avian_gravity() {
        let mut app = App::new();
        add_systems(&mut app);
        app.update();
        assert!((app.world().resource::<Gravity>().0 - Vec2::NEG_Y * 9.81).length() < 1e-5);

        app.world_mut().resource_mut::<GravitySetting>().enabled = false;
        app.update();
        assert_eq!(app.world().resource::<Gravity>().0, Vec2::ZERO);
    }
}
