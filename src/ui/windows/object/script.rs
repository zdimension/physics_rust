use std::collections::HashMap;

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use egui_extras::syntax_highlighting::{CodeTheme, highlight};

use crate::{
    script::thyme::{evaluate_bindings, with_script_engine},
    ui::{InitialPos, Subwindow, WindowSelectionTarget, window_target_entities, window_title},
};

const PROPERTIES_PER_COLUMN: usize = 20;

pub fn add_systems(app: &mut App) {
    app.add_systems(PreUpdate, sync_script_windows.after(evaluate_bindings))
        .add_systems(EguiPrimaryContextPass, ScriptWindow::show);
}

#[derive(Default, Component)]
pub struct ScriptWindow {
    properties: Vec<PropertyEditor>,
    pending: Vec<PendingEdit>,
}

struct PropertyEditor {
    name: &'static str,
    text: String,
    read_only: bool,
    focused: bool,
    dirty: bool,
    error: Option<String>,
}

struct PendingEdit {
    name: &'static str,
    source: String,
}

impl ScriptWindow {
    fn sync(
        &mut self,
        properties: Vec<crate::script::thyme::SceneProperty>,
        results: HashMap<&'static str, Result<(), String>>,
    ) {
        let mut previous = std::mem::take(&mut self.properties)
            .into_iter()
            .map(|property| (property.name, property))
            .collect::<HashMap<_, _>>();
        self.properties = properties
            .into_iter()
            .map(|property| {
                let mut editor = previous.remove(property.name).unwrap_or(PropertyEditor {
                    name: property.name,
                    text: property.value.clone(),
                    read_only: property.read_only,
                    focused: false,
                    dirty: false,
                    error: None,
                });
                editor.read_only = property.read_only;
                match results.get(property.name) {
                    Some(Ok(())) => {
                        editor.text = property.value;
                        editor.error = None;
                    }
                    Some(Err(error)) => editor.error = Some(error.clone()),
                    None if !editor.focused && editor.error.is_none() => {
                        editor.text = property.value
                    }
                    None => {}
                }
                editor
            })
            .collect();
    }

    fn show(
        mut windows: Query<(
            Entity,
            Option<&ChildOf>,
            Option<&WindowSelectionTarget>,
            &mut Self,
            &mut InitialPos,
        )>,
        mut contexts: EguiContexts,
        mut commands: Commands,
    ) {
        let ctx = contexts.ctx_mut().expect("primary egui context");
        for (id, parent, target, mut window, mut initial_pos) in &mut windows {
            let title = window_title(target, "Script");
            let mut pending = Vec::new();
            egui::Window::new(title)
                .auto_sized()
                .subwindow(id, ctx, &mut initial_pos, &mut commands, |ui, _| {
                    let theme = CodeTheme::from_memory(ui.ctx(), ui.style());
                    ui.horizontal_top(|ui| {
                        for (column, properties) in window
                            .properties
                            .chunks_mut(PROPERTIES_PER_COLUMN)
                            .enumerate()
                        {
                            let font = egui::TextStyle::Monospace.resolve(ui.style());
                            let code_width = properties
                                .iter()
                                .flat_map(|property| property.text.lines())
                                .map(|line| {
                                    ui.fonts_mut(|fonts| {
                                        fonts
                                            .layout_no_wrap(
                                                line.to_owned(),
                                                font.clone(),
                                                egui::Color32::WHITE,
                                            )
                                            .size()
                                            .x
                                    })
                                })
                                .fold(48.0, f32::max);
                            ui.push_id(column, |ui| {
                                egui::Grid::new("properties").show(ui, |ui| {
                                    for property in properties {
                                        let mut label = egui::RichText::new(property.name);
                                        if property.error.is_some() {
                                            label = label.color(egui::Color32::RED);
                                        }
                                        ui.add(egui::Label::new(label).extend());
                                        let mut layouter =
                                            |ui: &egui::Ui,
                                             text: &dyn egui::TextBuffer,
                                             _width: f32| {
                                                let mut job = highlight(
                                                    ui.ctx(),
                                                    ui.style(),
                                                    &theme,
                                                    text.as_str(),
                                                    "rs",
                                                );
                                                job.wrap.max_width = f32::INFINITY;
                                                ui.fonts_mut(|fonts| fonts.layout_job(job))
                                            };
                                        let rows = property.text.split('\n').count();
                                        let response = ui.add(
                                            egui::TextEdit::multiline(&mut property.text)
                                                .id_salt(property.name)
                                                .code_editor()
                                                .desired_rows(rows)
                                                .desired_width(code_width)
                                                .interactive(!property.read_only)
                                                .layouter(&mut layouter),
                                        );
                                        let response = if let Some(error) = &property.error {
                                            response.on_hover_text(error)
                                        } else {
                                            response
                                        };
                                        property.focused = response.has_focus();
                                        if response.changed() {
                                            property.dirty = true;
                                            property.error = None;
                                        }
                                        if response.lost_focus() && property.dirty {
                                            pending.push(PendingEdit {
                                                name: property.name,
                                                source: property.text.clone(),
                                            });
                                            property.dirty = false;
                                        }
                                        ui.end_row();
                                    }
                                });
                            });
                        }
                    });
                });
            window.pending.extend(pending);
            if window.properties.is_empty()
                && window_target_entities(target, parent).is_empty()
            {
                commands.entity(id).despawn();
            }
        }
    }
}

fn sync_script_windows(world: &mut World) {
    let snapshots = {
        let mut query = world.query::<(
            Entity,
            Option<&ChildOf>,
            Option<&WindowSelectionTarget>,
            &mut ScriptWindow,
        )>();
        query
            .iter_mut(world)
            .map(|(window, parent, target, mut state)| {
                let entities = target
                    .map(|target| target.entities.clone())
                    .unwrap_or_else(|| parent.map(ChildOf::parent).into_iter().collect());
                (window, entities, std::mem::take(&mut state.pending))
            })
            .collect::<Vec<_>>()
    };
    with_script_engine(world, |engine, world| {
        for (window, entities, pending) in snapshots {
            let results = pending
                .into_iter()
                .map(|edit| {
                    let result = engine.set_selection_property(
                        world,
                        &entities,
                        edit.name,
                        edit.source.trim(),
                    );
                    (edit.name, result)
                })
                .collect();
            let properties = engine.selection_properties(world, &entities);
            if let Some(mut state) = world.get_mut::<ScriptWindow>(window) {
                state.sync(properties, results);
            }
        }
    });
}
