use chrono::Local;
use ab_glyph::Font;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use tracing::{debug, info};
use xcap::Monitor;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Tool {
    None,
    Rectangle,
    Ellipse,
    Arrow,
    Brush,
    Text,
    Mosaic,
}

#[derive(Clone, Copy, PartialEq)]
enum StrokeWidth {
    Thin,
    Medium,
    Thick,
}

impl StrokeWidth {
    fn value(self) -> f32 {
        match self {
            StrokeWidth::Thin => 2.0,
            StrokeWidth::Medium => 4.0,
            StrokeWidth::Thick => 6.0,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum FontSize {
    Small,
    Medium,
    Large,
}

impl FontSize {
    fn value(self) -> f32 {
        match self {
            FontSize::Small => 16.0,
            FontSize::Medium => 24.0,
            FontSize::Large => 36.0,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum LineColor {
    Red,
    Yellow,
    Green,
    Blue,
    White,
}

const COLORS: [(LineColor, Color32); 5] = [
    (LineColor::Red, Color32::from_rgb(255, 59, 48)),
    (LineColor::Yellow, Color32::from_rgb(255, 204, 0)),
    (LineColor::Green, Color32::from_rgb(52, 199, 89)),
    (LineColor::Blue, Color32::from_rgb(0, 122, 255)),
    (LineColor::White, Color32::from_rgb(255, 255, 255)),
];

impl LineColor {
    fn to_color32(self) -> Color32 {
        COLORS.iter().find(|(c, _)| *c == self).unwrap().1
    }
}

#[derive(Clone)]
enum Annotation {
    Rectangle {
        rect: Rect,
        color: Color32,
        width: f32,
    },
    Ellipse {
        rect: Rect,
        color: Color32,
        width: f32,
    },
    Arrow {
        start: Pos2,
        end: Pos2,
        color: Color32,
        width: f32,
    },
    FreeDraw {
        points: Vec<Pos2>,
        color: Color32,
        width: f32,
    },
    Text {
        text: String,
        position: Pos2,
        color: Color32,
        size: f32,
    },
    Mosaic {
        rect: Rect,
        block_size: u32,
    },
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum AppState {
    Selecting,
    Adjusting,
    Canvas,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum DragMode {
    None,
    NewSelection,
    ResizeNW,
    ResizeNE,
    ResizeSW,
    ResizeSE,
    CanvasDraw,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum PendingAction {
    SaveToFile,
    CopyToClipboard,
    Close,
}

const HANDLE_SIZE: f32 = 8.0;
const TOOLBAR_ROW1_H: f32 = 36.0;
const TOOLBAR_ROW2_H: f32 = 30.0;

struct ToolbarMetrics {
    btn_w: f32,
    padding: f32,
    color_btn_w: f32,
}

impl ToolbarMetrics {
    fn new(tb_width: f32) -> Self {
        let btn_w = (tb_width / 14.0).clamp(20.0, 36.0);
        let padding = (btn_w * 0.13).round();
        let color_btn_w = (btn_w * 0.55).round();
        Self {
            btn_w,
            padding,
            color_btn_w,
        }
    }
}

struct ScreenshotApp {
    screenshot: Vec<u8>,
    img_w: u32,
    img_h: u32,
    texture: Option<egui::TextureHandle>,

    selection: Option<Rect>,
    state: AppState,
    drag_mode: DragMode,
    drag_start: Option<Pos2>,
    drag_current: Option<Pos2>,

    current_tool: Tool,
    stroke_width: StrokeWidth,
    line_color: LineColor,
    font_size: FontSize,

    annotations: Vec<Annotation>,

    brush_points: Vec<Pos2>,

    text_input_active: bool,
    text_input_buffer: String,
    text_input_pos: Option<Pos2>,

    pending_action: Option<PendingAction>,
    exit_countdown: i32,
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
            selection: None,
            state: AppState::Selecting,
            drag_mode: DragMode::None,
            drag_start: None,
            drag_current: None,
            current_tool: Tool::None,
            stroke_width: StrokeWidth::Thin,
            line_color: LineColor::Red,
            font_size: FontSize::Medium,
            annotations: Vec::new(),
            brush_points: Vec::new(),
            text_input_active: false,
            text_input_buffer: String::new(),
            text_input_pos: None,
            pending_action: None,
            exit_countdown: 0,
        }
    }

    fn toolbar_rect(&self) -> Option<Rect> {
        let sel = self.selection?;
        let h = TOOLBAR_ROW1_H + TOOLBAR_ROW2_H;
        let y = sel.max.y.min(self.img_h as f32 - h);
        Some(Rect::from_min_size(
            Pos2::new(sel.min.x, y),
            Vec2::new(sel.width(), h),
        ))
    }

    fn is_in_toolbar(&self, pos: Pos2) -> bool {
        self.toolbar_rect().map_or(false, |r| r.contains(pos))
    }

    fn is_in_selection(&self, pos: Pos2) -> bool {
        self.selection.map_or(false, |r| r.contains(pos))
    }

    fn is_on_handle(&self, pos: Pos2) -> DragMode {
        let sel = match self.selection {
            Some(s) => s,
            None => return DragMode::None,
        };
        let h = HANDLE_SIZE;
        let corners = [
            (sel.min, DragMode::ResizeNW),
            (Pos2::new(sel.max.x, sel.min.y), DragMode::ResizeNE),
            (Pos2::new(sel.min.x, sel.max.y), DragMode::ResizeSW),
            (sel.max, DragMode::ResizeSE),
        ];
        for (corner, mode) in corners {
            let handle = Rect::from_center_size(corner, Vec2::new(h, h));
            if handle.contains(pos) {
                return mode;
            }
        }
        DragMode::None
    }

    fn crop_and_save(&self) {
        info!("开始合成截图并保存到文件");
        let img = self.compose();
        let filename = format!("screenshot_{}.png", Local::now().format("%Y%m%d_%H%M%S"));
        img.save(&filename).expect("无法保存截图");
        info!("截图已保存: {}", filename);
    }

    fn copy_to_clipboard(&self) {
        info!("开始合成截图并复制到剪贴板");
        let img = self.compose();
        let (w, h) = (img.width() as usize, img.height() as usize);
        let rgba = img.into_raw();
        info!("图片尺寸: {}x{}, 数据大小: {} bytes", w, h, rgba.len());
        let mut clipboard =
            arboard::Clipboard::new().expect("无法访问剪贴板");
        let img_data = arboard::ImageData {
            width: w,
            height: h,
            bytes: std::borrow::Cow::from(rgba),
        };
        clipboard.set_image(img_data).expect("无法写入剪贴板");
        info!("截图已复制到剪贴板");
    }

    fn compose(&self) -> image::RgbaImage {
        let sel = match self.selection {
            Some(s) => s,
            None => {
                let empty: Vec<u8> = vec![0; (self.img_w * self.img_h * 4) as usize];
                return image::RgbaImage::from_raw(self.img_w, self.img_h, empty).unwrap();
            }
        };

        let sx = sel.min.x as u32;
        let sy = sel.min.y as u32;
        let sw = sel.width() as u32;
        let sh = sel.height() as u32;

        let mut pixels = Vec::with_capacity((sw * sh * 4) as usize);
        for row in sy..sy + sh {
            let start = (row * self.img_w + sx) as usize * 4;
            let end = start + (sw as usize * 4);
            pixels.extend_from_slice(&self.screenshot[start..end]);
        }
        let mut img = image::RgbaImage::from_raw(sw, sh, pixels).unwrap();
        info!("合成底图尺寸: {}x{}", sw, sh);

        let mut mosaic_anns: Vec<&Annotation> = Vec::new();
        let mut draw_anns: Vec<&Annotation> = Vec::new();
        for ann in &self.annotations {
            if matches!(ann, Annotation::Mosaic { .. }) {
                mosaic_anns.push(ann);
            } else {
                draw_anns.push(ann);
            }
        }
        info!(
            "合成标注: 马赛克={}个, 图形/文字={}个",
            mosaic_anns.len(),
            draw_anns.len()
        );

        for ann in &mosaic_anns {
            if let Annotation::Mosaic { rect, block_size } = ann {
                self.draw_mosaic_on_image(&mut img, *rect, sel, *block_size);
            }
        }

        for ann in &draw_anns {
            match ann {
                Annotation::Rectangle {
                    rect, color, width, ..
                } => self.draw_rect_on_image(&mut img, *rect, sel, *color, *width),
                Annotation::Ellipse {
                    rect, color, width, ..
                } => self.draw_ellipse_on_image(&mut img, *rect, sel, *color, *width),
                Annotation::Arrow {
                    start,
                    end,
                    color,
                    width,
                } => self.draw_arrow_on_image(&mut img, *start, *end, sel, *color, *width),
                Annotation::FreeDraw {
                    points,
                    color,
                    width,
                } => self.draw_free_on_image(&mut img, points, sel, *color, *width),
                Annotation::Text {
                    text,
                    position,
                    color,
                    size,
                } => self.draw_text_on_image(&mut img, text, *position, sel, *color, *size),
                _ => {}
            }
        }

        img
    }

    fn draw_mosaic_on_image(&self, img: &mut image::RgbaImage, rect: Rect, sel: Rect, block_size: u32) {
        let x0 = ((rect.min.x - sel.min.x).max(0.0) as u32).min(img.width());
        let y0 = ((rect.min.y - sel.min.y).max(0.0) as u32).min(img.height());
        let x1 = ((rect.max.x - sel.min.x).max(0.0) as u32).min(img.width());
        let y1 = ((rect.max.y - sel.min.y).max(0.0) as u32).min(img.height());
        let bs = block_size.max(2);

        let mut by = y0;
        while by < y1 {
            let ey = (by + bs).min(y1);
            let mut bx = x0;
            while bx < x1 {
                let ex = (bx + bs).min(x1);
                let (mut sr, mut sg, mut sb) = (0u64, 0u64, 0u64);
                let mut count = 0u64;
                for py in by..ey {
                    for px in bx..ex {
                        let p = img.get_pixel(px, py);
                        sr += p[0] as u64;
                        sg += p[1] as u64;
                        sb += p[2] as u64;
                        count += 1;
                    }
                }
                if count > 0 {
                    let ar = (sr / count) as u8;
                    let ag = (sg / count) as u8;
                    let ab_ = (sb / count) as u8;
                    let avg = image::Rgba([ar, ag, ab_, 255]);
                    for py in by..ey {
                        for px in bx..ex {
                            img.put_pixel(px, py, avg);
                        }
                    }
                }
                bx = ex;
            }
            by = ey;
        }
    }

    fn draw_rect_on_image(&self, img: &mut image::RgbaImage, rect: Rect, sel: Rect, color: Color32, width: f32) {
        let x0 = ((rect.min.x - sel.min.x).max(0.0) as i32).min(img.width() as i32 - 1);
        let y0 = ((rect.min.y - sel.min.y).max(0.0) as i32).min(img.height() as i32 - 1);
        let x1 = ((rect.max.x - sel.min.x).max(0.0) as i32).min(img.width() as i32 - 1);
        let y1 = ((rect.max.y - sel.min.y).max(0.0) as i32).min(img.height() as i32 - 1);
        let rgba = image::Rgba([color.r(), color.g(), color.b(), 255]);
        let hw = (width / 2.0).ceil() as i32;

        for dy in -hw..=hw {
            for x in x0 - hw..=x1 + hw {
                if x >= 0 && x < img.width() as i32 {
                    let top = y0 + dy;
                    let bot = y1 + dy;
                    if top >= 0 && top < img.height() as i32 {
                        img.put_pixel(x as u32, top as u32, rgba);
                    }
                    if bot >= 0 && bot < img.height() as i32 {
                        img.put_pixel(x as u32, bot as u32, rgba);
                    }
                }
            }
            for y in y0 - hw..=y1 + hw {
                if y >= 0 && y < img.height() as i32 {
                    let left = x0 + dy;
                    let right = x1 + dy;
                    if left >= 0 && left < img.width() as i32 {
                        img.put_pixel(left as u32, y as u32, rgba);
                    }
                    if right >= 0 && right < img.width() as i32 {
                        img.put_pixel(right as u32, y as u32, rgba);
                    }
                }
            }
        }
    }

    fn draw_ellipse_on_image(&self, img: &mut image::RgbaImage, rect: Rect, sel: Rect, color: Color32, width: f32) {
        let cx = rect.center().x - sel.min.x;
        let cy = rect.center().y - sel.min.y;
        let rx = rect.width() / 2.0;
        let ry = rect.height() / 2.0;
        if rx <= 1.0 || ry <= 1.0 {
            return;
        }
        let rgba = image::Rgba([color.r(), color.g(), color.b(), 255]);
        let hw = width / 2.0;

        let x0 = ((rect.min.x - sel.min.x).max(0.0) as i32).min(img.width() as i32 - 1);
        let y0 = ((rect.min.y - sel.min.y).max(0.0) as i32).min(img.height() as i32 - 1);
        let x1 = ((rect.max.x - sel.min.x).max(0.0) as i32).min(img.width() as i32 - 1);
        let y1 = ((rect.max.y - sel.min.y).max(0.0) as i32).min(img.height() as i32 - 1);

        for py in y0..=y1 {
            for px in x0..=x1 {
                if rx > 0.0 && ry > 0.0 {
                    let dx = px as f32 - cx;
                    let dy = py as f32 - cy;
                    let dist = ((dx * dx) / (rx * rx) + (dy * dy) / (ry * ry)).sqrt();
                    let inner_dist = (dx * dx) / ((rx - hw).max(1.0).powi(2))
                        + (dy * dy) / ((ry - hw).max(1.0).powi(2));
                    if dist <= 1.0 && inner_dist >= 1.0 {
                        img.put_pixel(px as u32, py as u32, rgba);
                    }
                }
            }
        }
    }

    fn draw_arrow_on_image(&self, img: &mut image::RgbaImage, start: Pos2, end: Pos2, sel: Rect, color: Color32, width: f32) {
        let sx = start.x - sel.min.x;
        let sy = start.y - sel.min.y;
        let ex = end.x - sel.min.x;
        let ey = end.y - sel.min.y;
        let rgba = image::Rgba([color.r(), color.g(), color.b(), 255]);
        self.draw_line_pixels(img, sx as i32, sy as i32, ex as i32, ey as i32, rgba, width);

        let dx = ex - sx;
        let dy = ey - sy;
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1.0 {
            return;
        }
        let ux = dx / len;
        let uy = dy / len;
        let arrow_len = 14.0;
        let arrow_angle = 0.45f32;
        let ax1 = ex - arrow_len * (ux * arrow_angle.cos() - uy * arrow_angle.sin());
        let ay1 = ey - arrow_len * (uy * arrow_angle.cos() + ux * arrow_angle.sin());
        let ax2 = ex - arrow_len * (ux * arrow_angle.cos() + uy * arrow_angle.sin());
        let ay2 = ey - arrow_len * (uy * arrow_angle.cos() - ux * arrow_angle.sin());
        self.draw_line_pixels(img, ex as i32, ey as i32, ax1 as i32, ay1 as i32, rgba, width);
        self.draw_line_pixels(img, ex as i32, ey as i32, ax2 as i32, ay2 as i32, rgba, width);
    }

    fn draw_free_on_image(&self, img: &mut image::RgbaImage, points: &[Pos2], sel: Rect, color: Color32, width: f32) {
        let rgba = image::Rgba([color.r(), color.g(), color.b(), 255]);
        for pair in points.windows(2) {
            let sx = pair[0].x - sel.min.x;
            let sy = pair[0].y - sel.min.y;
            let ex = pair[1].x - sel.min.x;
            let ey = pair[1].y - sel.min.y;
            self.draw_line_pixels(img, sx as i32, sy as i32, ex as i32, ey as i32, rgba, width);
        }
    }

    fn load_font() -> Option<Vec<u8>> {
        let paths = [
            "/usr/share/fonts/truetype/noto/NotoSansSC-Regular.ttf",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
            "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        ];
        for path in paths {
            if let Ok(data) = std::fs::read(path) {
                return Some(data);
            }
        }
        None
    }

    fn draw_text_on_image(&self, img: &mut image::RgbaImage, text: &str, position: Pos2, sel: Rect, color: Color32, size: f32) {
        let font_data = match Self::load_font() {
            Some(d) => d,
            None => return,
        };
        let font = ab_glyph::FontRef::try_from_slice(&font_data);
        let font = match font {
            Ok(f) => f,
            Err(_) => return,
        };
        let scale = ab_glyph::PxScale::from(size);
        let px = (position.x - sel.min.x).max(0.0);
        let py = (position.y - sel.min.y).max(0.0);

        let mut cursor_x = px;
        for c in text.chars() {
            let glyph_id = font.glyph_id(c);
            let glyph = glyph_id.with_scale(scale);
            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                let bx = (cursor_x + bounds.min.x).round() as i32;
                let by = (py + bounds.min.y).round() as i32;
                outlined.draw(|x, y, c| {
                    let gx = (bx + x as i32).max(0).min(img.width() as i32 - 1) as u32;
                    let gy = (by + y as i32).max(0).min(img.height() as i32 - 1) as u32;
                    let alpha = (c * 255.0) as u8;
                    if alpha > 0 {
                        let existing = *img.get_pixel(gx, gy);
                        let blended = blend_pixel(existing, color, alpha);
                        img.put_pixel(gx, gy, blended);
                    }
                });
            }
            cursor_x += scale.x;
        }
    }

    fn draw_line_pixels(&self, img: &mut image::RgbaImage, x0: i32, y0: i32, x1: i32, y1: i32, color: image::Rgba<u8>, width: f32) {
        let w = img.width() as i32;
        let h = img.height() as i32;
        let hw = (width / 2.0).ceil() as i32;

        if (x1 - x0).abs() >= (y1 - y0).abs() {
            let (sx, ex, sy, ey) = if x0 <= x1 {
                (x0, x1, y0, y1)
            } else {
                (x1, x0, y1, y0)
            };
            for x in sx..=ex {
                let t = if ex != sx {
                    (x - sx) as f32 / (ex - sx) as f32
                } else {
                    0.5
                };
                let y = (sy as f32 + t * (ey - sy) as f32).round() as i32;
                for dy in -hw..=hw {
                    let py = y + dy;
                    let px = x;
                    if px >= 0 && px < w && py >= 0 && py < h {
                        img.put_pixel(px as u32, py as u32, color);
                    }
                }
            }
        } else {
            let (sy, ey, sx, ex) = if y0 <= y1 {
                (y0, y1, x0, x1)
            } else {
                (y1, y0, x1, x0)
            };
            for y in sy..=ey {
                let t = if ey != sy {
                    (y - sy) as f32 / (ey - sy) as f32
                } else {
                    0.5
                };
                let x = (sx as f32 + t * (ex - sx) as f32).round() as i32;
                for dx in -hw..=hw {
                    let px = x + dx;
                    let py = y;
                    if px >= 0 && px < w && py >= 0 && py < h {
                        img.put_pixel(px as u32, py as u32, color);
                    }
                }
            }
        }
    }
}

fn blend_pixel(bg: image::Rgba<u8>, fg: Color32, alpha: u8) -> image::Rgba<u8> {
    let fa = alpha as f32 / 255.0;
    let fr = fg.r() as f32;
    let fg_v = fg.g() as f32;
    let fb = fg.b() as f32;
    let br = bg[0] as f32;
    let bg_v = bg[1] as f32;
    let bb = bg[2] as f32;
    image::Rgba([
        (fr * fa + br * (1.0 - fa)) as u8,
        (fg_v * fa + bg_v * (1.0 - fa)) as u8,
        (fb * fa + bb * (1.0 - fa)) as u8,
        255,
    ])
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
            if self.text_input_active {
                debug!("Esc 取消文字输入");
                self.text_input_active = false;
                self.text_input_buffer.clear();
                self.text_input_pos = None;
            } else {
                debug!("Esc 关闭工具");
                self.pending_action = Some(PendingAction::Close);
            }
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Z) && i.modifiers.ctrl) {
            if self.state == AppState::Canvas {
                let removed = self.annotations.pop();
                debug!("撤销: {:?}", removed.as_ref().map(|_| "已移除"));
            }
        }

        if self.state == AppState::Adjusting
            && self.current_tool != Tool::None
            && ctx.input(|i| i.key_pressed(egui::Key::Enter))
        {
            info!("Enter 键: Adjusting -> Canvas, 工具: {:?}", self.current_tool);
            self.state = AppState::Canvas;
        }

        self.handle_text_input(ctx);

        if self.state == AppState::Canvas
            && self.text_input_active
            && ctx.input(|i| i.key_pressed(egui::Key::Enter))
        {
            if let Some(pos) = self.text_input_pos {
                let txt = self.text_input_buffer.clone();
                if !txt.is_empty() {
                    self.annotations.push(Annotation::Text {
                        text: txt,
                        position: pos,
                        color: self.line_color.to_color32(),
                        size: self.font_size.value(),
                    });
                }
            }
            self.text_input_active = false;
            self.text_input_buffer.clear();
            self.text_input_pos = None;
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                let screen_rect = ui.max_rect();
                let _pointer_pos = ctx.input(|i| i.pointer.interact_pos());

                self.handle_mouse_events(ctx, screen_rect);

                let painter = ui.painter();
                self.render_screenshot(&painter, screen_rect);
                if self.drag_mode == DragMode::NewSelection {
                    self.render_selecting_preview(&painter, screen_rect);
                } else {
                    self.render_darken_outside(&painter, screen_rect);
                }
                self.render_annotations(&painter);
                self.render_current_drawing(&painter, screen_rect);
                self.render_selection_border(&painter, screen_rect);
                self.render_handles(&painter, screen_rect);
                self.render_toolbar(ui, screen_rect);
            });

        if self.exit_countdown > 0 {
            self.exit_countdown -= 1;
            if self.exit_countdown == 0 {
                debug!("倒计时结束，关闭窗口");
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        if let Some(action) = self.pending_action.take() {
            match action {
                PendingAction::SaveToFile => {
                    info!("执行保存到文件");
                    self.crop_and_save();
                    self.exit_countdown = 30;
                }
                PendingAction::CopyToClipboard => {
                    info!("执行复制到剪贴板，延迟退出以确保 X11 剪贴板持久化");
                    self.copy_to_clipboard();
                    self.exit_countdown = 30;
                }
                PendingAction::Close => {
                    info!("关闭工具");
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }

        ctx.request_repaint();
    }
}

impl ScreenshotApp {
    fn handle_mouse_events(&mut self, ctx: &egui::Context, _screen_rect: Rect) {
        let pointer = match ctx.input(|i| i.pointer.interact_pos()) {
            Some(p) => p,
            None => return,
        };

        let pressed = ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary));
        let released = ctx.input(|i| i.pointer.button_released(egui::PointerButton::Primary));

        if pressed {
            if self.is_in_toolbar(pointer) {
                debug!("工具栏点击: ({:.0}, {:.0})", pointer.x, pointer.y);
                self.handle_toolbar_click(pointer);
                self.drag_mode = DragMode::None;
                return;
            }

            match self.state {
                AppState::Selecting => {
                    debug!("开始框选屏幕区域: ({:.0}, {:.0})", pointer.x, pointer.y);
                    self.drag_start = Some(pointer);
                    self.drag_current = Some(pointer);
                    self.drag_mode = DragMode::NewSelection;
                }
                AppState::Adjusting => {
                    let handle = self.is_on_handle(pointer);
                    if handle != DragMode::None {
                        debug!("拖拽手柄调整选区: {:?}", handle);
                        self.drag_start = Some(pointer);
                        self.drag_mode = handle;
                    } else if self.is_in_selection(pointer) {
                        debug!("选区内按下: 重新框选");
                        self.drag_start = Some(pointer);
                        self.drag_current = Some(pointer);
                        self.drag_mode = DragMode::NewSelection;
                    }
                }
                AppState::Canvas => {
                    if !self.text_input_active {
                        debug!(
                            "画布绘制: 起点=({:.0},{:.0}), 工具={:?}",
                            pointer.x, pointer.y, self.current_tool
                        );
                        self.drag_start = Some(pointer);
                        self.drag_current = Some(pointer);
                        self.drag_mode = DragMode::CanvasDraw;
                        self.brush_points.clear();
                        self.brush_points.push(pointer);
                    }
                }
            }
        }

        if self.drag_mode != DragMode::None {
            self.drag_current = Some(pointer);
        }

        if self.drag_mode == DragMode::CanvasDraw && self.current_tool == Tool::Brush {
            self.brush_points.push(pointer);
        }

        if released {
            match self.drag_mode {
                DragMode::NewSelection => {
                    if let (Some(start), Some(end)) = (self.drag_start, self.drag_current) {
                        let r = Rect::from_two_pos(start, end);
                        if r.width() > 4.0 && r.height() > 4.0 {
                            info!("选区确认: ({:.0},{:.0})-({:.0},{:.0}), 状态: Adjusting", r.min.x, r.min.y, r.max.x, r.max.y);
                            self.selection = Some(r);
                            self.state = AppState::Adjusting;
                        }
                    }
                }
                DragMode::ResizeNW
                | DragMode::ResizeNE
                | DragMode::ResizeSW
                | DragMode::ResizeSE => {
                    if let (Some(start), Some(end)) = (self.drag_start, self.drag_current) {
                        if let Some(sel) = self.selection {
                            let new_sel = self.compute_resize(sel, self.drag_mode, start, end);
                            if new_sel.width() > 4.0 && new_sel.height() > 4.0 {
                                self.selection = Some(new_sel);
                            }
                        }
                    }
                }
                DragMode::CanvasDraw => {
                    self.finish_canvas_draw();
                }
                _ => {}
            }
            self.drag_start = None;
            self.drag_current = None;
            self.drag_mode = DragMode::None;
        }
    }

    fn compute_resize(&self, sel: Rect, mode: DragMode, start: Pos2, end: Pos2) -> Rect {
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        match mode {
            DragMode::ResizeNW => {
                Rect::from_min_max(
                    Pos2::new(sel.min.x + dx, sel.min.y + dy).clamp(
                        Pos2::new(0.0, 0.0),
                        Pos2::new(sel.max.x - 4.0, sel.max.y - 4.0),
                    ),
                    sel.max,
                )
            }
            DragMode::ResizeNE => {
                Rect::from_min_max(
                    Pos2::new(sel.min.x, sel.min.y + dy).clamp(
                        Pos2::new(0.0, 0.0),
                        Pos2::new(sel.max.x - 4.0, sel.max.y - 4.0),
                    ),
                    Pos2::new(sel.max.x + dx, sel.max.y)
                        .clamp(
                            Pos2::new(sel.min.x + 4.0, sel.min.y + 4.0),
                            Pos2::new(self.img_w as f32 - 1.0, self.img_h as f32 - 1.0),
                        ),
                )
            }
            DragMode::ResizeSW => {
                Rect::from_min_max(
                    Pos2::new(sel.min.x + dx, sel.min.y).clamp(
                        Pos2::new(0.0, 0.0),
                        Pos2::new(sel.max.x - 4.0, sel.max.y - 4.0),
                    ),
                    Pos2::new(sel.max.x, sel.max.y + dy).clamp(
                        Pos2::new(sel.min.x + 4.0, sel.min.y + 4.0),
                        Pos2::new(self.img_w as f32 - 1.0, self.img_h as f32 - 1.0),
                    ),
                )
            }
            DragMode::ResizeSE => {
                Rect::from_min_max(
                    sel.min,
                    Pos2::new(sel.max.x + dx, sel.max.y + dy).clamp(
                        Pos2::new(sel.min.x + 4.0, sel.min.y + 4.0),
                        Pos2::new(self.img_w as f32 - 1.0, self.img_h as f32 - 1.0),
                    ),
                )
            }
            _ => sel,
        }
    }

    fn finish_canvas_draw(&mut self) {
        let start = match self.drag_start {
            Some(s) => s,
            None => return,
        };
        let end = match self.drag_current {
            Some(e) => e,
            None => return,
        };

        match self.current_tool {
            Tool::Rectangle => {
                let r = Rect::from_two_pos(start, end);
                if r.width() > 2.0 && r.height() > 2.0 {
                    info!("添加矩形标注: ({:.0},{:.0})-({:.0},{:.0})", r.min.x, r.min.y, r.max.x, r.max.y);
                    self.annotations.push(Annotation::Rectangle {
                        rect: r,
                        color: self.line_color.to_color32(),
                        width: self.stroke_width.value(),
                    });
                }
            }
            Tool::Ellipse => {
                let r = Rect::from_two_pos(start, end);
                if r.width() > 2.0 && r.height() > 2.0 {
                    info!("添加椭圆标注");
                    self.annotations.push(Annotation::Ellipse {
                        rect: r,
                        color: self.line_color.to_color32(),
                        width: self.stroke_width.value(),
                    });
                }
            }
            Tool::Arrow => {
                let dist = start.distance(end);
                if dist > 2.0 {
                    info!("添加箭头标注: 长度={:.0}", dist);
                    self.annotations.push(Annotation::Arrow {
                        start,
                        end,
                        color: self.line_color.to_color32(),
                        width: self.stroke_width.value(),
                    });
                }
            }
            Tool::Brush => {
                if self.brush_points.len() >= 2 {
                    info!("添加画笔标注: {} 个点", self.brush_points.len());
                    self.annotations.push(Annotation::FreeDraw {
                        points: self.brush_points.clone(),
                        color: self.line_color.to_color32(),
                        width: self.stroke_width.value(),
                    });
                }
            }
            Tool::Mosaic => {
                let r = Rect::from_two_pos(start, end);
                if r.width() > 4.0 && r.height() > 4.0 {
                    info!("添加马赛克标注");
                    self.annotations.push(Annotation::Mosaic {
                        rect: r,
                        block_size: 10,
                    });
                }
            }
            Tool::Text => {
                let pos = end;
                info!("文字工具激活: 位置 ({:.0}, {:.0})", pos.x, pos.y);
                self.text_input_pos = Some(pos);
                self.text_input_active = true;
                self.text_input_buffer.clear();
            }
            Tool::None => {}
        }
        self.brush_points.clear();
    }

    fn handle_toolbar_click(&mut self, pos: Pos2) {
        let tb = match self.toolbar_rect() {
            Some(r) => r,
            None => return,
        };

        let m = ToolbarMetrics::new(tb.width());
        let row2_y = tb.min.y + TOOLBAR_ROW1_H;

        let col_x = pos.x - tb.min.x;
        let divider_x = tb.width() * 0.7;

        if pos.y < row2_y {
            if col_x < divider_x {
                let tools = [
                    Tool::Rectangle,
                    Tool::Ellipse,
                    Tool::Arrow,
                    Tool::Brush,
                    Tool::Text,
                    Tool::Mosaic,
                ];
                for (i, &tool) in tools.iter().enumerate() {
                    let bx = m.padding + i as f32 * (m.btn_w + 2.0);
                    if col_x >= bx && col_x < bx + m.btn_w {
                        info!("选择标注工具: {:?}, 旧状态: {:?}", tool, self.state);
                        self.current_tool = tool;
                        if self.state == AppState::Adjusting {
                            info!("状态转换: Adjusting -> Canvas");
                            self.state = AppState::Canvas;
                        }
                        self.text_input_active = false;
                        self.text_input_buffer.clear();
                        return;
                    }
                }
                let save_x = m.padding + 6.0 * (m.btn_w + 2.0);
                if col_x >= save_x && col_x < save_x + m.btn_w {
                    info!("点击下载按钮");
                    self.pending_action = Some(PendingAction::SaveToFile);
                    return;
                }
            } else {
                let right_start = divider_x + m.padding;
                for i in 0..3 {
                    let bx = right_start + i as f32 * (m.btn_w + 2.0);
                    if col_x >= bx && col_x < bx + m.btn_w {
                        match i {
                            0 => {
                                debug!("点击撤销");
                                self.annotations.pop();
                            }
                            1 => {
                                debug!("点击关闭");
                                self.pending_action = Some(PendingAction::Close);
                            }
                            2 => {
                                info!("点击复制到剪贴板");
                                self.pending_action = Some(PendingAction::CopyToClipboard);
                            }
                            _ => {}
                        }
                        return;
                    }
                }
            }
        } else {
            let is_draw_tool = matches!(
                self.current_tool,
                Tool::Rectangle | Tool::Ellipse | Tool::Arrow | Tool::Brush
            );
            let is_text_tool = matches!(self.current_tool, Tool::Text);

            if is_draw_tool {
                let widths = [StrokeWidth::Thin, StrokeWidth::Medium, StrokeWidth::Thick];
                for (i, &w) in widths.iter().enumerate() {
                    let bx = m.padding + i as f32 * (m.btn_w + 2.0);
                    if col_x >= bx && col_x < bx + m.btn_w {
                        self.stroke_width = w;
                        return;
                    }
                }
                let color_start = m.padding + 3.0 * (m.btn_w + 2.0) + 8.0;
                for (i, &(lc, _)) in COLORS.iter().enumerate() {
                    let bx = color_start + i as f32 * (m.color_btn_w + 6.0);
                    if col_x >= bx && col_x < bx + m.color_btn_w {
                        self.line_color = lc;
                        return;
                    }
                }
            } else if is_text_tool {
                let sizes = [FontSize::Small, FontSize::Medium, FontSize::Large];
                for (i, &fs) in sizes.iter().enumerate() {
                    let bx = m.padding + i as f32 * (m.btn_w + 2.0);
                    if col_x >= bx && col_x < bx + m.btn_w {
                        self.font_size = fs;
                        return;
                    }
                }
                let color_start = m.padding + 3.0 * (m.btn_w + 2.0) + 8.0;
                for (i, &(lc, _)) in COLORS.iter().enumerate() {
                    let bx = color_start + i as f32 * (m.color_btn_w + 6.0);
                    if col_x >= bx && col_x < bx + m.color_btn_w {
                        self.line_color = lc;
                        return;
                    }
                }
            }
        }
    }

    fn handle_text_input(&mut self, ctx: &egui::Context) {
        if !self.text_input_active {
            return;
        }

        ctx.input(|i| {
            for event in &i.events {
                if let egui::Event::Text(text) = event {
                    self.text_input_buffer.push_str(text);
                }
            }
        });

        // Also handle Backspace
        if ctx.input(|i| i.key_pressed(egui::Key::Backspace)) {
            self.text_input_buffer.pop();
        }
    }

    fn render_screenshot(&self, painter: &egui::Painter, screen_rect: Rect) {
        if let Some(texture) = &self.texture {
            let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
            painter.image(texture.id(), screen_rect, uv, Color32::WHITE);
        }
    }

    fn render_selecting_preview(&self, painter: &egui::Painter, screen_rect: Rect) {
        let (start, end) = match (self.drag_start, self.drag_current) {
            (Some(s), Some(e)) => (s, e),
            _ => return,
        };
        let sel = Rect::from_two_pos(start, end);
        if sel.width() < 2.0 || sel.height() < 2.0 {
            return;
        }

        let dark = Color32::from_black_alpha(160);
        painter.rect_filled(
            Rect::from_min_max(screen_rect.min, Pos2::new(screen_rect.max.x, sel.min.y)),
            0.0,
            dark,
        );
        painter.rect_filled(
            Rect::from_min_max(Pos2::new(screen_rect.min.x, sel.max.y), screen_rect.max),
            0.0,
            dark,
        );
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(screen_rect.min.x, sel.min.y),
                Pos2::new(sel.min.x, sel.max.y),
            ),
            0.0,
            dark,
        );
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(sel.max.x, sel.min.y),
                Pos2::new(screen_rect.max.x, sel.max.y),
            ),
            0.0,
            dark,
        );

        painter.rect_stroke(sel, 0.0, Stroke::new(2.0, Color32::from_rgb(0, 140, 255)));
    }

    fn render_darken_outside(&self, painter: &egui::Painter, screen_rect: Rect) {
        let sel = match self.selection {
            Some(s) => s,
            None => return,
        };
        let dark = Color32::from_black_alpha(160);

        painter.rect_filled(
            Rect::from_min_max(screen_rect.min, Pos2::new(screen_rect.max.x, sel.min.y)),
            0.0,
            dark,
        );
        painter.rect_filled(
            Rect::from_min_max(Pos2::new(screen_rect.min.x, sel.max.y), screen_rect.max),
            0.0,
            dark,
        );
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(screen_rect.min.x, sel.min.y),
                Pos2::new(sel.min.x, sel.max.y),
            ),
            0.0,
            dark,
        );
        painter.rect_filled(
            Rect::from_min_max(
                Pos2::new(sel.max.x, sel.min.y),
                Pos2::new(screen_rect.max.x, sel.max.y),
            ),
            0.0,
            dark,
        );
    }

    fn render_annotations(&self, painter: &egui::Painter) {
        for ann in &self.annotations {
            match ann {
                Annotation::Rectangle { rect, color, width } => {
                    let fill = Color32::from_rgba_premultiplied(
                        color.r(),
                        color.g(),
                        color.b(),
                        40,
                    );
                    painter.rect_filled(*rect, 0.0, fill);
                    painter.rect_stroke(*rect, 0.0, Stroke::new(*width, *color));
                }
                Annotation::Ellipse { rect, color, width } => {
                    self.paint_ellipse(painter, *rect, *color, *width);
                }
                Annotation::Arrow { start, end, color, width } => {
                    painter.line_segment([*start, *end], Stroke::new(*width, *color));
                    self.paint_arrowhead(painter, *start, *end, *color, *width);
                }
                Annotation::FreeDraw { points, color, width } => {
                    if points.len() >= 2 {
                        for pair in points.windows(2) {
                            painter.line_segment([pair[0], pair[1]], Stroke::new(*width, *color));
                        }
                    }
                }
                Annotation::Text {
                    text,
                    position,
                    color,
                    size,
                } => {
                    painter.text(
                        *position,
                        egui::Align2::LEFT_TOP,
                        text,
                        egui::FontId::proportional(*size),
                        *color,
                    );
                }
                Annotation::Mosaic { rect, .. } => {
                    let mosaic_color = Color32::from_black_alpha(100);
                    painter.rect_filled(*rect, 0.0, mosaic_color);
                    painter.rect_stroke(*rect, 0.0, Stroke::new(1.0, Color32::GRAY));

                    let label = "马赛克";
                    let center = rect.center();
                    painter.text(
                        center,
                        egui::Align2::CENTER_CENTER,
                        label,
                        egui::FontId::proportional(14.0),
                        Color32::WHITE,
                    );
                }
            }
        }
    }

    fn render_current_drawing(&self, painter: &egui::Painter, _screen_rect: Rect) {
        if self.drag_mode != DragMode::CanvasDraw {
            return;
        }
        let start = match self.drag_start {
            Some(s) => s,
            None => return,
        };
        let end = match self.drag_current {
            Some(e) => e,
            None => return,
        };
        let color = self.line_color.to_color32();
        let width = self.stroke_width.value();

        match self.current_tool {
            Tool::Rectangle => {
                let r = Rect::from_two_pos(start, end);
                let fill = Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), 40);
                painter.rect_filled(r, 0.0, fill);
                painter.rect_stroke(r, 0.0, Stroke::new(width, color));
            }
            Tool::Ellipse => {
                let r = Rect::from_two_pos(start, end);
                self.paint_ellipse(painter, r, color, width);
            }
            Tool::Arrow => {
                painter.line_segment([start, end], Stroke::new(width, color));
                self.paint_arrowhead(painter, start, end, color, width);
            }
            Tool::Brush => {
                if self.brush_points.len() >= 2 {
                    for pair in self.brush_points.windows(2) {
                        painter.line_segment([pair[0], pair[1]], Stroke::new(width, color));
                    }
                }
            }
            Tool::Mosaic => {
                let r = Rect::from_two_pos(start, end);
                painter.rect_filled(r, 0.0, Color32::from_black_alpha(100));
                painter.rect_stroke(r, 0.0, Stroke::new(2.0, Color32::GRAY));
            }
            _ => {}
        }
    }

    fn render_selection_border(&self, painter: &egui::Painter, _screen_rect: Rect) {
        let sel = match self.selection {
            Some(s) => s,
            None => return,
        };

        let color = if self.state == AppState::Canvas {
            Color32::from_rgb(0, 200, 100)
        } else {
            Color32::from_rgb(0, 140, 255)
        };

        painter.rect_stroke(sel, 0.0, Stroke::new(2.0, color));
    }

    fn render_handles(&self, painter: &egui::Painter, _screen_rect: Rect) {
        let sel = match self.selection {
            Some(s) => s,
            None => return,
        };

        let h = HANDLE_SIZE;
        let corners = [
            sel.min,
            Pos2::new(sel.max.x, sel.min.y),
            Pos2::new(sel.min.x, sel.max.y),
            sel.max,
        ];
        for corner in corners {
            let handle = Rect::from_center_size(corner, Vec2::new(h, h));
            painter.rect_filled(handle, 0.0, Color32::WHITE);
            painter.rect_stroke(handle, 0.0, Stroke::new(1.0, Color32::BLACK));
        }
    }

    fn render_toolbar(&self, ui: &mut egui::Ui, _screen_rect: Rect) {
        let tb = match self.toolbar_rect() {
            Some(r) => r,
            None => return,
        };

        let m = ToolbarMetrics::new(tb.width());
        let painter = ui.painter();
        let bg = Color32::from_rgba_premultiplied(40, 40, 40, 230);
        painter.rect_filled(tb, 4.0, bg);

        let divider_x = tb.min.x + tb.width() * 0.7;

        let row1_y = tb.min.y;

        let tool_labels = ["口", "○", "↗", "✎", "T", "▩"];
        let tools = [
            Tool::Rectangle,
            Tool::Ellipse,
            Tool::Arrow,
            Tool::Brush,
            Tool::Text,
            Tool::Mosaic,
        ];

        let font_scaled = |s: f32| -> egui::FontId {
            let scale = m.btn_w / 30.0;
            egui::FontId::proportional(s * scale)
        };

        for (i, label) in tool_labels.iter().enumerate() {
            let bx = tb.min.x + m.padding + i as f32 * (m.btn_w + 2.0);
            let btn_rect = Rect::from_min_size(
                Pos2::new(bx, row1_y + 3.0),
                Vec2::new(m.btn_w, TOOLBAR_ROW1_H - 6.0),
            );

            if self.current_tool == tools[i] {
                painter.rect_filled(btn_rect, 4.0, Color32::from_rgba_premultiplied(0, 122, 255, 180));
            }

            painter.text(
                btn_rect.center(),
                egui::Align2::CENTER_CENTER,
                *label,
                font_scaled(16.0),
                Color32::WHITE,
            );
        }

        let save_x = tb.min.x + m.padding + 6.0 * (m.btn_w + 2.0);
        let save_rect = Rect::from_min_size(
            Pos2::new(save_x, row1_y + 3.0),
            Vec2::new(m.btn_w, TOOLBAR_ROW1_H - 6.0),
        );
        painter.text(
            save_rect.center(),
            egui::Align2::CENTER_CENTER,
            "↓",
            font_scaled(16.0),
            Color32::WHITE,
        );

        painter.line_segment(
            [Pos2::new(divider_x, row1_y + 4.0), Pos2::new(divider_x, row1_y + TOOLBAR_ROW1_H - 4.0)],
            Stroke::new(1.0, Color32::from_gray(120)),
        );

        let action_labels = ["↩", "✕", "✓"];
        let right_start = divider_x + m.padding;
        for (i, label) in action_labels.iter().enumerate() {
            let bx = right_start + i as f32 * (m.btn_w + 2.0);
            let btn_rect = Rect::from_min_size(
                Pos2::new(bx, row1_y + 3.0),
                Vec2::new(m.btn_w, TOOLBAR_ROW1_H - 6.0),
            );
            let color = match i {
                1 => Color32::from_rgb(255, 80, 80),
                2 => Color32::from_rgb(80, 255, 80),
                _ => Color32::WHITE,
            };
            painter.text(
                btn_rect.center(),
                egui::Align2::CENTER_CENTER,
                *label,
                font_scaled(16.0),
                color,
            );
        }

        let row2_y = tb.min.y + TOOLBAR_ROW1_H;
        let is_draw_tool = matches!(
            self.current_tool,
            Tool::Rectangle | Tool::Ellipse | Tool::Arrow | Tool::Brush
        );
        let is_text_tool = matches!(self.current_tool, Tool::Text);

        if is_draw_tool {
            let widths = [StrokeWidth::Thin, StrokeWidth::Medium, StrokeWidth::Thick];
            let radii = [3.0, 5.0, 7.0];
            for (i, (&w, &r)) in widths.iter().zip(radii.iter()).enumerate() {
                let bx = tb.min.x + m.padding + i as f32 * (m.btn_w + 2.0);
                let btn_rect = Rect::from_min_size(
                    Pos2::new(bx, row2_y + 2.0),
                    Vec2::new(m.btn_w, TOOLBAR_ROW2_H - 4.0),
                );

                if self.stroke_width == w {
                    painter.rect_filled(btn_rect, 3.0, Color32::from_rgba_premultiplied(0, 122, 255, 150));
                }

                painter.circle_filled(btn_rect.center(), r, Color32::WHITE);
            }

            let color_start = tb.min.x + m.padding + 3.0 * (m.btn_w + 2.0) + 8.0;
            for (i, &(lc, col)) in COLORS.iter().enumerate() {
                let bx = color_start + i as f32 * (m.color_btn_w + 6.0);
                let cr = Rect::from_min_size(
                    Pos2::new(bx, row2_y + 4.0),
                    Vec2::new(m.color_btn_w, TOOLBAR_ROW2_H - 8.0),
                );
                painter.rect_filled(cr, 3.0, col);

                if self.line_color == lc {
                    painter.rect_stroke(cr.expand(2.0), 2.0, Stroke::new(2.0, Color32::WHITE));
                }
            }
        } else if is_text_tool {
            let sizes = [FontSize::Small, FontSize::Medium, FontSize::Large];
            let size_labels = ["小A", "中A", "大A"];
            for (i, (&fs, label)) in sizes.iter().zip(size_labels.iter()).enumerate() {
                let bx = tb.min.x + m.padding + i as f32 * (m.btn_w + 2.0);
                let btn_rect = Rect::from_min_size(
                    Pos2::new(bx, row2_y + 2.0),
                    Vec2::new(m.btn_w, TOOLBAR_ROW2_H - 4.0),
                );

                if self.font_size == fs {
                    painter.rect_filled(btn_rect, 3.0, Color32::from_rgba_premultiplied(0, 122, 255, 150));
                }

                painter.text(
                    btn_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    *label,
                    font_scaled(13.0),
                    Color32::WHITE,
                );
            }

            let color_start = tb.min.x + m.padding + 3.0 * (m.btn_w + 2.0) + 8.0;
            for (i, &(lc, col)) in COLORS.iter().enumerate() {
                let bx = color_start + i as f32 * (m.color_btn_w + 6.0);
                let cr = Rect::from_min_size(
                    Pos2::new(bx, row2_y + 4.0),
                    Vec2::new(m.color_btn_w, TOOLBAR_ROW2_H - 8.0),
                );
                painter.rect_filled(cr, 3.0, col);

                if self.line_color == lc {
                    painter.rect_stroke(cr.expand(2.0), 2.0, Stroke::new(2.0, Color32::WHITE));
                }
            }
        }
    }

    fn paint_ellipse(&self, painter: &egui::Painter, rect: Rect, color: Color32, width: f32) {
        let n = 64;
        let cx = rect.center().x;
        let cy = rect.center().y;
        let rx = rect.width() / 2.0;
        let ry = rect.height() / 2.0;
        let mut points = Vec::with_capacity(n + 1);
        for i in 0..=n {
            let angle = (i as f32 / n as f32) * std::f32::consts::TAU;
            points.push(Pos2::new(cx + rx * angle.cos(), cy + ry * angle.sin()));
        }
        for pair in points.windows(2) {
            painter.line_segment([pair[0], pair[1]], Stroke::new(width, color));
        }
    }

    fn paint_arrowhead(&self, painter: &egui::Painter, start: Pos2, end: Pos2, color: Color32, width: f32) {
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1.0 {
            return;
        }
        let ux = dx / len;
        let uy = dy / len;
        let arrow_len = 14.0;
        let angle = 0.45f32;
        let ax1 = end.x - arrow_len * (ux * angle.cos() - uy * angle.sin());
        let ay1 = end.y - arrow_len * (uy * angle.cos() + ux * angle.sin());
        let ax2 = end.x - arrow_len * (ux * angle.cos() + uy * angle.sin());
        let ay2 = end.y - arrow_len * (uy * angle.cos() - ux * angle.sin());
        painter.line_segment([end, Pos2::new(ax1, ay1)], Stroke::new(width, color));
        painter.line_segment([end, Pos2::new(ax2, ay2)], Stroke::new(width, color));
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    info!("截图工具启动");

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