// egui UI toolbar
// Ported from ToolBarViews.cs + CaptionBar.cs

use egui::Context;

/// UI state and callbacks
pub struct UiState {
    /// Whether to show orthographic projection
    pub orthographic: bool,
}

impl Default for UiState {
    fn default() -> Self {
        Self { orthographic: true }
    }
}

/// View direction preset
#[derive(Clone, Copy, PartialEq)]
pub enum ViewPreset {
    Default,
    Front,
    Back,
    Left,
    Right,
    Top,
    Bottom,
}

/// Action requested by UI
#[derive(Clone, Copy, PartialEq)]
pub enum UiAction {
    None,
    ToggleProjection,
    SetView(ViewPreset),
    Quit,
}

impl UiState {
    /// Draw the toolbar UI. Returns the action the user triggered.
    #[allow(deprecated)]
    pub fn draw(&mut self, ctx: &Context) -> UiAction {
        let mut action = UiAction::None;

        egui::Panel::left("toolbar")
            .resizable(false)
            .default_size(50.0)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    // Toggle button
                    let toggle_label = if self.orthographic {
                        "正交视图"
                    } else {
                        "透视视图"
                    };
                    if ui.button(toggle_label).clicked() {
                        action = UiAction::ToggleProjection;
                        self.orthographic = !self.orthographic;
                    }

                    ui.separator();

                    // Reset view
                    if ui.button("重置").clicked() {
                        action = UiAction::SetView(ViewPreset::Default);
                    }

                    ui.separator();

                    // View direction buttons
                    let views = [
                        ("正视图", ViewPreset::Front),
                        ("后视图", ViewPreset::Back),
                        ("左视图", ViewPreset::Left),
                        ("右视图", ViewPreset::Right),
                        ("顶视图", ViewPreset::Top),
                        ("底视图", ViewPreset::Bottom),
                    ];
                    for (label, preset) in &views {
                        if ui.button(*label).clicked() {
                            action = UiAction::SetView(*preset);
                        }
                    }

                    ui.separator();

                    // Quit button
                    if ui.button("  ×  ").clicked() {
                        action = UiAction::Quit;
                    }
                });
            });

        action
    }
}
