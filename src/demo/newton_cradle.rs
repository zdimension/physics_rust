use crate::objects::hinge::HingeObject;
use crate::objects::phy_obj::PhysicalObject;
use bevy::prelude::*;
use bevy_rapier2d::prelude::*;

pub fn init(commands: &mut ChildBuilder) {
    let mut z = 1.0;
    let mut z = || {
        z += 0.1;
        z
    };
    let ground = PhysicalObject::rect(Vec2::new(8.0, 0.5), Vec3::new(-4.0, -3.0, z()));
    commands.spawn(ground).insert(RigidBody::Fixed);

    for i in 0..5 {
        let stick = PhysicalObject::rect(
            Vec2::new(0.4, 2.4),
            Vec3::new(-1.0 + i as f32 * 0.8, 1.8, z()),
        );
        let ball = PhysicalObject::ball(0.4, Vec3::new(-1.0 + i as f32 * 0.8 + 0.2, 2.0, z()));
        let stick_id = commands.spawn(stick).insert(ColliderMassProperties::MassProperties(MassProperties {
            local_center_of_mass: Default::default(),
            mass: 0.0,
            principal_inertia: 0.0,
        })).id();
        commands.spawn(ball).insert((
            HingeObject,
            MultibodyJoint::new(
                stick_id,
                RevoluteJointBuilder::new()
                    .local_anchor1(Vec2::new(0.0, -1.0))
                    .local_anchor2(Vec2::new(0.0, 0.0)),
            ),
            Restitution::coefficient(1.0),
            ActiveHooks::FILTER_CONTACT_PAIRS,
        ));
        commands.spawn((
            ImpulseJoint::new(
                stick_id,
                RevoluteJointBuilder::new()
                    .local_anchor1(Vec2::new(0.0, 1.0))
                    .local_anchor2(Vec2::new(-1.0 + i as f32 * 0.8 + 0.2, 4.0)),
            ),
            RigidBody::Dynamic,
        ));
    }

    let stick = PhysicalObject::rect(Vec2::new(2.4, 0.4), Vec3::new(-3.8, 3.8, z()));
    let ball = PhysicalObject::ball(0.4, Vec3::new(-3.6, 4.0, z()));
    let stick_id = commands.spawn(stick).insert(ColliderMassProperties::MassProperties(MassProperties {
        local_center_of_mass: Default::default(),
        mass: 0.0,
        principal_inertia: 0.0,
    })).id();
    commands.spawn(ball).insert((
        HingeObject,
        MultibodyJoint::new(
            stick_id,
            RevoluteJointBuilder::new()
                .local_anchor1(Vec2::new(-1.0, 0.0))
                .local_anchor2(Vec2::new(0.0, 0.0)),
        ),
        Restitution::coefficient(1.0),
        ActiveHooks::FILTER_CONTACT_PAIRS,
    ));
    commands.spawn((
        ImpulseJoint::new(
            stick_id,
            RevoluteJointBuilder::new()
                .local_anchor1(Vec2::new(1.0, 0.0))
                .local_anchor2(Vec2::new(-1.6, 4.0)),
        ),
        RigidBody::Dynamic,
    ));
}
