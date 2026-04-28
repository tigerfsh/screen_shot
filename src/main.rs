use chrono::Local;
use eframe::egui;
use xcap::Monitor;

#[derive(PartialEq)]
enum AppState {
    Selecting,
    Selected,
}

struct ScreenshotApp {
    screenshot: Vec<u8>,
    img_w: u32,
    img_h: u32,
    texture: Option<egui::TextureHandle>,
    drag_start: Option<egui::Pos2>,
    drag_end: Option<egui::Pos2>,
    state: AppState,
}

impl ScreenshotApp {
    fn new() -> Self {
        let monitors = Monitor::all().expect("无法获取显示器列表");
        if monitors.is_empty() {
            eprintln!("未检测到显示器");
            std::process::exit(1);
        }

        let screenshot = monitors[0].capture_image().expect("无法截取屏幕");
        let (img_w, img_h) = (screenshot.width(), screenshot.height());
        let screenshot = screenshot.into_raw();

        Self {
            screenshot,
            img_w,
            img_h,
            texture: None,
            drag_start: None,
            drag_end: None,
            state: AppState::Selecting,
        }
    }

    fn selection_rect(&self) -> Option<egui::Rect> {
        match (self.drag_start, self.drag_end) {
            (Some(s), Some(e)) => {
                let mut r = egui::Rect::from_two_pos(s, e);
                r.min.x = r.min.x.max(0.0).min(self.img_w as f32 - 1.0);
                r.min.y = r.min.y.max(0.0).min(self.img_h as f32 - 1.0);
                r.max.x = r.max.x.max(0.0).min(self.img_w as f32 - 1.0);
                r.max.y = r.max.y.max(0.0).min(self.img_h as f32 - 1.0);
                if r.width() > 1.0 && r.height() > 1.0 {
                    Some(r)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn crop_and_save(&self) {
        let rect = match self.selection_rect() {
            Some(r) => r,
            None => {
                eprintln!("未选择截图区域");
                return;
            }
        };

        let x = rect.min.x as u32;
        let y = rect.min.y as u32;
        let w = rect.width() as u32;
        let h = rect.height() as u32;

        let mut cropped = Vec::with_capacity((w * h * 4) as usize);
        for row in y..y + h {
            let start = (row * self.img_w + x) as usize * 4;
            let end = start + (w as usize * 4);
            cropped.extend_from_slice(&self.screenshot[start..end]);
        }

        let img =
            image::RgbaImage::from_raw(w, h, cropped).expect("无法创建裁剪图像");
        let filename = format!(
            "screenshot_{}.png",
            Local::now().format("%Y%m%d_%H%M%S")
        );
        img.save(&filename).expect("无法保存截图");
        println!("截图已保存: {}", filename);
    }
}

impl eframe::App for ScreenshotApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.texture.is_none() {
            let color_image = egui::ColorImage::from_rgba_unmultiplied(
                [self.img_w as usize, self.img_h as usize],
                &self.screenshot,
            );
            self.texture = Some(ctx.load_texture(
                "screenshot",
                color_image,
                egui::TextureOptions::LINEAR,
            ));
            ctx.request_repaint();
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            std::process::exit(0);
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Enter))
            && matches!(self.state, AppState::Selected)
        {
            self.crop_and_save();
            std::process::exit(0);
        }

        if ctx.input(|i| i.key_pressed(egui::Key::S))
            && matches!(self.state, AppState::Selected)
        {
            self.crop_and_save();
            std::process::exit(0);
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                let screen_rect = ui.max_rect();
                let response = ui.interact(
                    screen_rect,
                    ui.next_auto_id(),
                    egui::Sense::click_and_drag(),
                );

                if response.drag_started() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        self.drag_start = Some(pos);
                        self.drag_end = Some(pos);
                        self.state = AppState::Selecting;
                    }
                }
                if response.dragged() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        self.drag_end = Some(pos);
                    }
                }
                if response.drag_stopped() {
                    self.state = AppState::Selected;
                }

                let painter = ui.painter();

                if let Some(texture) = &self.texture {
                    let uv = egui::Rect::from_min_max(
                        egui::Pos2::ZERO,
                        egui::Pos2::new(1.0, 1.0),
                    );
                    painter.image(
                        texture.id(),
                        screen_rect,
                        uv,
                        egui::Color32::WHITE,
                    );
                }

                if let Some(sel) = self.selection_rect() {
                    let dark = egui::Color32::from_black_alpha(140);
                    painter.rect_filled(
                        egui::Rect::from_min_max(
                            screen_rect.min,
                            egui::pos2(screen_rect.max.x, sel.min.y),
                        ),
                        0.0,
                        dark,
                    );
                    painter.rect_filled(
                        egui::Rect::from_min_max(
                            egui::pos2(screen_rect.min.x, sel.max.y),
                            screen_rect.max,
                        ),
                        0.0,
                        dark,
                    );
                    painter.rect_filled(
                        egui::Rect::from_min_max(
                            egui::pos2(screen_rect.min.x, sel.min.y),
                            egui::pos2(sel.min.x, sel.max.y),
                        ),
                        0.0,
                        dark,
                    );
                    painter.rect_filled(
                        egui::Rect::from_min_max(
                            egui::pos2(sel.max.x, sel.min.y),
                            egui::pos2(screen_rect.max.x, sel.max.y),
                        ),
                        0.0,
                        dark,
                    );

                    let border_color = if matches!(self.state, AppState::Selected) {
                        egui::Color32::from_rgb(0, 200, 100)
                    } else {
                        egui::Color32::from_rgb(0, 140, 255)
                    };
                    painter.rect_stroke(
                        sel,
                        0.0,
                        egui::Stroke::new(2.0, border_color),
                    );

                    let size_text = format!(
                        "{} × {}",
                        sel.width() as u32,
                        sel.height() as u32
                    );

                    let text_pos = if sel.center().y > screen_rect.center().y {
                        egui::pos2(sel.min.x, sel.min.y - 24.0)
                    } else {
                        egui::pos2(sel.max.x, sel.max.y + 4.0)
                    };

                    let tooltip_rect = egui::Rect::from_min_size(
                        text_pos,
                        egui::vec2(160.0, 22.0),
                    );
                    painter.rect_filled(
                        tooltip_rect,
                        4.0,
                        egui::Color32::from_black_alpha(200),
                    );

                    painter.text(
                        tooltip_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        &size_text,
                        egui::FontId::proportional(14.0),
                        egui::Color32::WHITE,
                    );
                }

                let hint = match self.state {
                    AppState::Selecting => "拖拽鼠标选择截图区域 · Esc 取消",
                    AppState::Selected => "Enter / S 保存 · Esc 取消 · 重新拖拽选区",
                };

                let hint_bg = egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(screen_rect.width(), 32.0),
                );
                painter.rect_filled(
                    hint_bg,
                    0.0,
                    egui::Color32::from_black_alpha(160),
                );
                painter.text(
                    egui::pos2(12.0, 8.0),
                    egui::Align2::LEFT_TOP,
                    hint,
                    egui::FontId::proportional(15.0),
                    egui::Color32::from_rgb(220, 220, 220),
                );
            });

        ctx.request_repaint();
    }
}

fn main() {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_fullscreen(true)
            .with_decorations(false)
            .with_always_on_top(),
        ..Default::default()
    };

    eframe::run_native(
        "截图工具",
        options,
        Box::new(|_cc| Ok(Box::new(ScreenshotApp::new()))),
    )
    .expect("启动截图工具失败");
}
