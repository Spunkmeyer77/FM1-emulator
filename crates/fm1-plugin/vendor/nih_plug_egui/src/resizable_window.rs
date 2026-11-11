//! Resizable window wrapper for Egui editor.

use egui_baseview::egui::emath::GuiRounding;
use egui_baseview::egui::{InnerResponse, UiBuilder};

use crate::egui::{pos2, CentralPanel, Context, Id, Rect, Response, Sense, Ui, Vec2};
use crate::EguiState;

/// Adds a corner to the plugin window that can be dragged in order to resize it.
/// Resizing happens through plugin API, hence a custom implementation is needed.
pub struct ResizableWindow {
    id: Id,
    min_size: Vec2,
    max_size: Option<Vec2>,
    aspect: Option<(f32, f32)>,
}

impl ResizableWindow {
    pub fn new(id_source: impl std::hash::Hash) -> Self {
        Self {
            id: Id::new(id_source),
            min_size: Vec2::splat(16.0),
            max_size: None,
            aspect: None,
        }
    }

    /// Won't grow larger than this (FM-1 emulator: a host stops at the screen's edge, and a
    /// window grown past it leaves its corner out of reach)
    #[inline]
    pub fn max_size(mut self, max_size: impl Into<Vec2>) -> Self {
        self.max_size = Some(max_size.into());
        self
    }

    /// Keep the window's shape while dragging (FM-1 emulator): height = width / `ratio` + `extra`
    /// (content of that ratio under a bar `extra` high)
    #[inline]
    pub fn keep_aspect(mut self, ratio: f32, extra: f32) -> Self {
        self.aspect = Some((ratio, extra));
        self
    }

    /// Won't shrink to smaller than this
    #[inline]
    pub fn min_size(mut self, min_size: impl Into<Vec2>) -> Self {
        self.min_size = min_size.into();
        self
    }

    pub fn show<R>(
        self,
        context: &Context,
        egui_state: &EguiState,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> InnerResponse<R> {
        CentralPanel::default().show(context, move |ui| {
            let ui_rect = ui.clip_rect();
            let mut content_ui =
                ui.new_child(UiBuilder::new().max_rect(ui_rect).layout(*ui.layout()));

            let ret = add_contents(&mut content_ui);

            let corner_size = Vec2::splat(ui.visuals().resize_corner_size);
            let corner_rect = Rect::from_min_size(ui_rect.max - corner_size, corner_size);

            let corner_response = ui.interact(corner_rect, self.id.with("corner"), Sense::drag());

            // (FM-1 emulator) the window changes size once, when the drag ends: resizing it on
            // every frame of the drag made the content jump (on macOS the view grows from its
            // bottom-left corner, so the pointer moved against it). While dragging, an outline
            // shows the new size.
            let origin = context.input(|i| i.pointer.press_origin());
            if let (Some(pointer_pos), Some(origin)) = (corner_response.interact_pointer_pos(), origin) {
                let mut desired_size = (ui_rect.size() + (pointer_pos - origin)).max(self.min_size);
                if let Some(max) = self.max_size {
                    desired_size = desired_size.min(max);
                }
                if let Some((r, extra)) = self.aspect {
                    // the width follows the pointer, or the height, whichever asks for less
                    let w = desired_size.x.min((desired_size.y - extra) * r).max(self.min_size.x);
                    desired_size = Vec2::new(w, w / r + extra);
                }

                if corner_response.dragged() {
                    let painter = content_ui.ctx().layer_painter(crate::egui::LayerId::new(
                        crate::egui::Order::Foreground,
                        self.id.with("outline"),
                    ));
                    let r = Rect::from_min_size(ui_rect.min, desired_size);
                    painter.rect_filled(ui_rect, 0.0, crate::egui::Color32::from_black_alpha(90));
                    painter.rect_stroke(
                        r.shrink(1.0),
                        4.0,
                        crate::egui::Stroke::new(2.0, crate::egui::Color32::from_rgb(0xff, 0x8a, 0x1f)),
                        crate::egui::StrokeKind::Inside,
                    );
                    painter.text(
                        r.center().min(ui_rect.center() + ui_rect.size() * 0.4),
                        crate::egui::Align2::CENTER_CENTER,
                        format!("{} x {}  (release to resize)", desired_size.x.round(), desired_size.y.round()),
                        crate::egui::FontId::proportional(16.0),
                        crate::egui::Color32::WHITE,
                    );
                }
                if corner_response.drag_stopped() {
                    egui_state.set_requested_size((
                        desired_size.x.round() as u32,
                        desired_size.y.round() as u32,
                    ));
                }
            }

            paint_resize_corner(&content_ui, &corner_response);

            ret
        })
    }
}

pub fn paint_resize_corner(ui: &Ui, response: &Response) {
    let stroke = ui.style().interact(response).fg_stroke;

    let painter = ui.painter();
    let rect = response.rect.translate(-Vec2::splat(2.0)); // move away from the corner
    let cp = rect.max.round_to_pixels(painter.pixels_per_point());

    let mut w = 2.0;

    while w <= rect.width() && w <= rect.height() {
        painter.line_segment([pos2(cp.x - w, cp.y), pos2(cp.x, cp.y - w)], stroke);
        w += 4.0;
    }
}
