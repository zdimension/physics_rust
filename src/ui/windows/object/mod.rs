use crate::ui::windows::menu::MenuWindow;
use crate::ui::windows::object::appearance::AppearanceWindow;
use crate::ui::windows::object::collisions::CollisionsWindow;
use crate::ui::windows::object::geom_actions::GeometryActionsWindow;
use crate::ui::windows::object::hinge::HingeWindow;
use crate::ui::windows::object::information::InformationWindow;
use crate::ui::windows::object::laser::LaserWindow;
use crate::ui::windows::object::material::MaterialWindow;
use crate::ui::windows::object::plot::PlotWindow;
use bevy::prelude::{App, Update};

pub mod appearance;
pub mod collisions;
pub mod combine_shapes;
pub mod controller;
pub mod geom_actions;
pub mod hinge;
pub mod information;
pub mod laser;
pub mod material;
pub mod plot;
pub mod script;
pub mod selection;
pub mod text;
pub mod velocities;

pub fn add_ui_systems(app: &mut App) {
    app.add_systems(
        Update,
        (
            MenuWindow::show,
            InformationWindow::show,
            PlotWindow::show,
            CollisionsWindow::show,
            LaserWindow::show,
            MaterialWindow::show,
            AppearanceWindow::show,
            HingeWindow::show,
            GeometryActionsWindow::show,
        ),
    );
}
