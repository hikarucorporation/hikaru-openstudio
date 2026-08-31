/*
 * Hikaru OpenStudio / OpenLive - About Dialog
 * License: AGPL-3.0-or-later
 */

use egui::{Hyperlink, Ui};
use crate::app::AppMode;

pub fn show(ui: &mut Ui, mode: AppMode) {
    ui.vertical_centered(|ui| {
        ui.add_space(10.0);

        // Título e Identificador del DAW
        match mode {
            AppMode::OpenLive => {
                ui.heading("🎛 Hikaru OpenLive");
            }
            AppMode::OpenStudio => {
                ui.heading("🎚 Hikaru OpenStudio");
            }
        }

        ui.add_space(8.0);
        ui.label("An Advanced Digital Audio Workstation for UNIX sysadmins.");
        ui.add_space(6.0);
        ui.label("Copyright © Hikaru Corporation - 2026");

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);

        ui.label("This is a free software protected by GNU AGPLv3:");
        ui.add_space(6.0);

        // Link a la licencia AGPLv3
        ui.add(Hyperlink::from_label_and_url(
            "GNU AGPLv3 License",
            "https://www.gnu.org/licenses/agpl-3.0.en.html"
        ));

        ui.add_space(12.0);
        ui.separator();
        ui.add_space(8.0);

        // Nota sobre plataformas
        ui.strong("Coming Soon");
        ui.add_space(4.0);
        ui.label("Available on Windows, Linux, BSD distros like Free/Open/NetBSD, microwaves, calculator, potatoes, your mother, apache helicopter, anything shit that run Rust, etc.");

        ui.add_space(10.0);
    });
}