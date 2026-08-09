use std::{collections::VecDeque, fmt::Write};

use bevy::{ecs::world::World, prelude::Resource};

mod native;

pub(crate) use native::ScriptEngine;

#[derive(Default, Resource)]
pub(crate) struct Console {
    pub(crate) open: bool,
    pub(crate) input: String,
    pub(crate) output: String,
    pending: VecDeque<String>,
}

impl Console {
    pub(crate) fn submit(&mut self) {
        let source = std::mem::take(&mut self.input);
        if !source.trim().is_empty() {
            self.pending.push_back(source);
        }
    }

    fn push_line(&mut self, line: impl std::fmt::Display) {
        if !self.output.is_empty() {
            self.output.push('\n');
        }
        write!(self.output, "{line}").unwrap();
    }
}

pub(crate) fn execute_console(world: &mut World) {
    let source = world.resource_mut::<Console>().pending.pop_front();
    let Some(source) = source else { return };
    let engine = world
        .remove_non_send::<ScriptEngine>()
        .expect("Thyme engine");
    let result = engine.eval(world, source.trim());
    world.insert_non_send(engine);

    let console = &mut *world.resource_mut::<Console>();
    console.push_line(format_args!("> {}", source.trim()));
    match result {
        Ok(value) => console.push_line(value),
        Err(error) => console.push_line(format_args!("ERROR: {error}")),
    }
}

pub(crate) fn evaluate_bindings(world: &mut World) {
    let engine = world
        .remove_non_send::<ScriptEngine>()
        .expect("Thyme engine");
    let errors = engine.evaluate_bindings(world);
    world.insert_non_send(engine);
    for error in errors {
        world
            .resource_mut::<Console>()
            .push_line(format_args!("ERROR: {error}"));
    }
}
