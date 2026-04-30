use chrono::Local;
use ab_glyph::Font;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use std::sync::Arc;
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
    Custom(Color32),
}

const PRESET_COLORS: [(LineColor, Color32); 5] = [
    (LineColor::Red, Color32::from_rgb(255, 59, 48)),
    (LineColor::Yellow, Color32::from_rgb(255, 204, 0)),
    (LineColor::Green, Color32::from_rgb(52, 199, 89)),
    (LineColor::Blue, Color32::from_rgb(0, 122, 255)),
    (LineColor::White, Color32::from_rgb(255, 255, 255)),
];

impl LineColor {
    fn to_color32(self) -> Color32 {
        match self {
            LineColor::Custom(c) => c,
            _ => PRESET_COLORS.iter().find(|(c, _)| *c == self).unwrap().1,
        }
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
        bold: bool,
        italic: bool,
        underline: bool,
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
const TOOLBAR_W: f32 = 420.0;
const TOOLBAR_ROW1_H: f32 = 36.0;
const TOOLBAR_ROW2_H: f32 = 32.0;

const BTN_W: f32 = 28.0;
const BTN_GAP: f32 = 6.0;
const TB_PAD: f32 = 12.0;
const ICON_SZ: f32 = 16.0;

fn tb_button_x(i: usize) -> f32 { TB_PAD + i as f32 * (BTN_W + BTN_GAP) }
fn tb_button_rect(tb: Rect, i: usize, row_y: f32) -> Rect {
    Rect::from_min_size(
        Pos2::new(tb.min.x + tb_button_x(i), row_y + 4.0),
        Vec2::new(BTN_W, TOOLBAR_ROW1_H - 8.0),
    )
}

struct ToolbarMetrics {
    divider_x: f32,
    right_start: f32,
}

impl ToolbarMetrics {
    fn new(tb: Rect) -> Self {
        let divider_x = tb.min.x + tb_button_x(7);
        let right_start = divider_x + 8.0;
        Self { divider_x, right_start }
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
    bold: bool,
    italic: bool,
    underline: bool,

    annotations: Vec<Annotation>,

    brush_points: Vec<Pos2>,

    text_input_active: bool,
    text_input_buffer: String,
    text_input_pos: Option<Pos2>,
    ime_preedit: String,

    pending_action: Option<PendingAction>,
    exit_countdown: i32,

    show_color_picker: bool,
    show_size_picker: bool,

    screen_rect: Rect,
    ime_was_enabled: bool,
    font_registered: bool,
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
            bold: false,
            italic: false,
            underline: false,
            annotations: Vec::new(),
            brush_points: Vec::new(),
            text_input_active: false,
            text_input_buffer: String::new(),
            text_input_pos: None,
            ime_preedit: String::new(),
            pending_action: None,
            exit_countdown: 0,
            show_color_picker: false,
            show_size_picker: false,
            screen_rect: Rect::ZERO,
            ime_was_enabled: false,
            font_registered: false,
        }
    }

    fn toolbar_has_row2(&self) -> bool {
        self.current_tool != Tool::None
    }

    fn toolbar_rect(&self) -> Option<Rect> {
        let sel = self.selection?;
        let h = if self.toolbar_has_row2() {
            TOOLBAR_ROW1_H + TOOLBAR_ROW2_H
        } else {
            TOOLBAR_ROW1_H
        };
        let sr = self.screen_rect;

        let x_center = (sel.center().x - TOOLBAR_W / 2.0).max(0.0);

        // 1. Below
        let y_below = sel.max.y;
        if y_below + h <= sr.max.y {
            return Some(Rect::from_min_size(Pos2::new(x_center.min(sr.max.x - TOOLBAR_W), y_below), Vec2::new(TOOLBAR_W, h)));
        }

        // 2. Above
        let y_above = sel.min.y - h;
        if y_above >= sr.min.y {
            return Some(Rect::from_min_size(Pos2::new(x_center.min(sr.max.x - TOOLBAR_W), y_above), Vec2::new(TOOLBAR_W, h)));
        }

        // 3. Inside at bottom
        let y_inside = (sel.max.y - h).max(sel.min.y);
        Some(Rect::from_min_size(
            Pos2::new(x_center.min(sr.max.x - TOOLBAR_W).max(sr.min.x), y_inside.max(sr.min.y)),
            Vec2::new(TOOLBAR_W, h),
        ))
    }

    fn is_in_toolbar(&self, pos: Pos2) -> bool {
        self.toolbar_rect().map_or(false, |r| r.contains(pos))
    }

    fn color_picker_rect(&self) -> Option<Rect> {
        let tb = self.toolbar_rect()?;
        let picker_w = 180.0;
        let picker_h = 230.0;
        let y = tb.max.y + picker_h / 2.0 + 8.0;
        let x = (tb.center().x).max(picker_w / 2.0).min(self.screen_rect.max.x - picker_w / 2.0);
        Some(Rect::from_center_size(
            Pos2::new(x, y),
            Vec2::new(picker_w, picker_h),
        ))
    }

    fn is_in_color_picker(&self, pos: Pos2) -> bool {
        self.color_picker_rect().map_or(false, |r| r.contains(pos))
    }

    fn size_picker_rect(&self) -> Option<Rect> {
        let tb = self.toolbar_rect()?;
        let x = tb.min.x + tb_button_x(0);
        let y = tb.max.y;
        Some(Rect::from_min_size(
            Pos2::new(x, y),
            Vec2::new(BTN_W * 2.0 + BTN_GAP, 80.0),
        ))
    }

    fn is_in_size_picker(&self, pos: Pos2) -> bool {
        self.size_picker_rect().map_or(false, |r| r.contains(pos))
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
                    bold,
                    italic,
                    underline,
                } => self.draw_text_on_image(&mut img, text, *position, sel, *color, *size, *bold, *italic, *underline),
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
        let paths: &[&str] = &[
            "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            "/usr/share/fonts/truetype/noto/NotoSansSC-Regular.ttf",
            "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
            "/usr/share/fonts/opentype/noto/NotoSansSC-Regular.otf",
            "/usr/share/fonts/truetype/arphic/uming.ttc",
            "/usr/share/fonts/truetype/arphic/ukai.ttc",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        ];
        for path in paths {
            if let Ok(data) = std::fs::read(*path) {
                info!("[Font] Loaded: {}", path);
                return Some(data);
            }
        }
        info!("[Font] No font found, text rendering will fail");
        None
    }

    fn draw_text_on_image(&self, img: &mut image::RgbaImage, text: &str, position: Pos2, sel: Rect, color: Color32, size: f32, bold: bool, italic: bool, underline: bool) {
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
        let mut max_height: f32 = 0.0;
        let mut first_glyph_y: f32 = 0.0;
        let mut last_x_end: f32 = px;

        let italic_skew = if italic { -size * 0.2 } else { 0.0 };

        for c in text.chars() {
            let glyph_id = font.glyph_id(c);
            let glyph = glyph_id.with_scale(scale);
            let advance = if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                let h = bounds.height() as f32;
                let bx = (cursor_x + bounds.min.x + italic_skew * (h / size)).round() as i32;
                let by = (py + bounds.min.y).round() as i32;
                last_x_end = cursor_x + bounds.max.x + italic_skew;
                max_height = max_height.max(h);
                if first_glyph_y == 0.0 { first_glyph_y = bounds.max.y; }
                outlined.draw(|x, y, c| {
                    let skew_x = italic_skew * (y as f32 / h.max(1.0));
                    let gx = (bx + x as i32 + skew_x as i32).max(0).min(img.width() as i32 - 1) as u32;
                    let gy = (by + y as i32).max(0).min(img.height() as i32 - 1) as u32;
                    let alpha = (c * 255.0) as u8;
                    if alpha > 0 {
                        let existing = *img.get_pixel(gx, gy);
                        let blended = blend_pixel(existing, color, alpha);
                        img.put_pixel(gx, gy, blended);
                    }
                });

                if bold {
                    for (dx, dy) in &[(1i32, 0i32), (-1, 0), (0, 1), (1, 1)] {
                        outlined.draw(|x, y, c| {
                            let skew_x = italic_skew * (y as f32 / h.max(1.0));
                            let gx = (bx + x as i32 + dx + skew_x as i32).max(0).min(img.width() as i32 - 1) as u32;
                            let gy = (by + y as i32 + dy).max(0).min(img.height() as i32 - 1) as u32;
                            let alpha = (c * 255.0) as u8;
                            if alpha > 0 {
                                let existing = *img.get_pixel(gx, gy);
                                let blended = blend_pixel(existing, color, alpha);
                                img.put_pixel(gx, gy, blended);
                            }
                        });
                    }
                }
                bounds.max.x - bounds.min.x
            } else {
                scale.x
            };
            cursor_x += advance;
        }

        if underline {
            let y = (py + first_glyph_y + 2.0).round() as i32;
            let x0 = px.round() as i32;
            let x1 = (last_x_end + if italic { size * 0.2 } else { 0.0 }).round() as i32;
            let rgba = image::Rgba([color.r(), color.g(), color.b(), 255]);
            for x in x0..=x1 {
                if x >= 0 && (x as u32) < img.width() && y >= 0 && (y as u32) < img.height() {
                    img.put_pixel(x as u32, y as u32, rgba);
                }
            }
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

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let hp = h / (std::f32::consts::PI / 3.0);
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match hp as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

impl eframe::App for ScreenshotApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.font_registered {
            if let Some(font_data) = Self::load_font() {
                let mut fonts = egui::FontDefinitions::default();
                fonts.font_data.insert("cjk".into(), Arc::new(egui::FontData::from_owned(font_data)));
                fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "cjk".into());
                fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().insert(0, "cjk".into());
                ctx.set_fonts(fonts);
                info!("[Font] Registered CJK font with egui for proportional/monospace");
            }
            self.font_registered = true;
        }

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

        self.ensure_ime(ctx);
        self.handle_text_input(ctx);

        // --- Log ALL raw events for debugging ---
        let frame_modifiers = ctx.input(|i| i.modifiers);
        ctx.input(|i| {
            if !i.events.is_empty() {
                let text_events: Vec<String> = i.events.iter().filter_map(|e| {
                    match e {
                        egui::Event::Text(t) => Some(format!("Text({:?})", t)),
                        egui::Event::Ime(ime) => Some(format!("Ime({:?})", ime)),
                        egui::Event::Key { key, pressed, .. } if *pressed => Some(format!("Key({:?})", key)),
                        _ => None,
                    }
                }).collect();
                if !text_events.is_empty() {
                    info!("[Update] Frame events: state={:?} tool={:?} text_active={} modifiers={:?} events={:?}",
                        self.state, self.current_tool, self.text_input_active,
                        frame_modifiers, text_events);
                }
            }
        });

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.text_input_active {
                debug!("Esc 取消文字输入");
                self.text_input_active = false;
                self.text_input_buffer.clear();
                self.text_input_pos = None;
                self.ime_preedit.clear();
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
            info!("[Update] Enter pressed in Adjusting state, switching to Canvas, tool: {:?}, buf: {:?}", self.current_tool, self.text_input_buffer);
            self.state = AppState::Canvas;
        }

        if self.state == AppState::Canvas
            && self.text_input_active
            && ctx.input(|i| i.key_pressed(egui::Key::Enter))
        {
            info!("[Update] Enter pressed with text_input_active, calling finish_text_input, buf: {:?}", self.text_input_buffer);
            self.finish_text_input();
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                let screen_rect = ui.max_rect();
                self.screen_rect = screen_rect;
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
                self.render_size_picker(ctx, screen_rect);
                self.render_color_picker(ctx, screen_rect);
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
            if self.show_size_picker {
                if self.is_in_size_picker(pointer) {
                    return;
                }
                self.show_size_picker = false;
                return;
            }
            if self.show_color_picker {
                if self.is_in_color_picker(pointer) {
                    return;
                }
                self.show_color_picker = false;
                return;
            }

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
                    if self.current_tool == Tool::Text && self.text_input_active {
                        self.finish_text_input();
                    }
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

    fn finish_text_input(&mut self) {
        if !self.text_input_active {
            return;
        }
        if !self.ime_preedit.is_empty() {
            info!("[TextInput] finish_text_input: absorbing preedit {:?} into buf", self.ime_preedit);
            self.text_input_buffer.push_str(&self.ime_preedit);
            self.ime_preedit.clear();
        }
        let txt = self.text_input_buffer.clone();
        info!("[TextInput] finish_text_input: saving text {:?}", txt);
        if !txt.is_empty() {
            if let Some(pos) = self.text_input_pos {
                self.annotations.push(Annotation::Text {
                    text: txt,
                    position: pos,
                    color: self.line_color.to_color32(),
                    size: self.font_size.value(),
                    bold: self.bold,
                    italic: self.italic,
                    underline: self.underline,
                });
            }
        }
        self.text_input_active = false;
        self.text_input_buffer.clear();
        self.text_input_pos = None;
        self.ime_preedit.clear();
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
                info!("[TextInput] Text tool activated at position ({:.0}, {:.0}), enabling IME", pos.x, pos.y);
                self.text_input_pos = Some(pos);
                self.text_input_active = true;
                self.text_input_buffer.clear();
                self.ime_preedit.clear();
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

        let m = ToolbarMetrics::new(tb);
        let col_x = pos.x - tb.min.x;
        let row2_y = tb.min.y + TOOLBAR_ROW1_H;

        if pos.y < row2_y || !self.toolbar_has_row2() {
            if col_x < m.divider_x - tb.min.x {
                let tools = [Tool::Rectangle, Tool::Ellipse, Tool::Arrow, Tool::Brush, Tool::Text, Tool::Mosaic];
                for (i, &tool) in tools.iter().enumerate() {
                    let bx = tb_button_x(i);
                    if col_x >= bx && col_x < bx + BTN_W {
                        info!("选择标注工具: {:?}, 旧状态: {:?}", tool, self.state);
                        self.current_tool = tool;
                        if self.state == AppState::Adjusting {
                            info!("状态转换: Adjusting -> Canvas");
                            self.state = AppState::Canvas;
                        }
                        self.text_input_active = false;
                        self.text_input_buffer.clear();
                        self.ime_preedit.clear();
                        return;
                    }
                }
                let save_bx = tb_button_x(6);
                if col_x >= save_bx && col_x < save_bx + BTN_W {
                    info!("点击下载按钮");
                    self.pending_action = Some(PendingAction::SaveToFile);
                    return;
                }
            } else {
                let right_start = m.right_start - tb.min.x;
                for i in 0..3 {
                    let bx = right_start + i as f32 * (BTN_W + BTN_GAP);
                    if col_x >= bx && col_x < bx + BTN_W {
                        match i {
                            0 => { debug!("点击撤销"); self.annotations.pop(); }
                            1 => { debug!("点击关闭"); self.pending_action = Some(PendingAction::Close); }
                            2 => { info!("点击复制到剪贴板"); self.pending_action = Some(PendingAction::CopyToClipboard); }
                            _ => {}
                        }
                        return;
                    }
                }
            }
        } else {
            let is_draw_tool = matches!(self.current_tool, Tool::Rectangle | Tool::Ellipse | Tool::Arrow | Tool::Brush);
            let is_text_tool = matches!(self.current_tool, Tool::Text);

            if is_draw_tool {
                let widths = [StrokeWidth::Thin, StrokeWidth::Medium, StrokeWidth::Thick];
                for (i, &w) in widths.iter().enumerate() {
                    let bx = tb_button_x(i);
                    if col_x >= bx && col_x < bx + BTN_W { self.stroke_width = w; return; }
                }
                let color_start = tb_button_x(3) + 8.0;
                for (i, &(lc, _)) in PRESET_COLORS.iter().enumerate() {
                    let bx = color_start + i as f32 * (24.0);
                    if col_x >= bx && col_x < bx + 20.0 { self.line_color = lc; return; }
                }
            } else if is_text_tool {
                let sz_bx = tb_button_x(0);
                if col_x >= sz_bx && col_x < sz_bx + BTN_W {
                    self.show_size_picker = !self.show_size_picker;
                    self.show_color_picker = false;
                    return;
                }
                for i in 1..4 {
                    let bx = tb_button_x(i);
                    if col_x >= bx && col_x < bx + BTN_W {
                        match i {
                            1 => { self.bold = !self.bold; return; }
                            2 => { self.italic = !self.italic; return; }
                            3 => { self.underline = !self.underline; return; }
                            _ => {}
                        }
                    }
                }
                let cp_bx = tb_button_x(4);
                if col_x >= cp_bx && col_x < cp_bx + BTN_W {
                    self.show_color_picker = !self.show_color_picker;
                    self.show_size_picker = false;
                    return;
                }
            }
        }
    }

    fn handle_text_input(&mut self, ctx: &egui::Context) {
        if !self.text_input_active {
            return;
        }

        if let Some(pos) = self.text_input_pos {
            let txt = if self.text_input_buffer.is_empty() { " " } else { &self.text_input_buffer };
            let pre = if self.ime_preedit.is_empty() { "" } else { &self.ime_preedit };
            let display = format!("{}{}", txt, pre);
            let galley = ctx.fonts(|f| {
                f.layout_no_wrap(
                    display,
                    egui::FontId::proportional(self.font_size.value()),
                    self.line_color.to_color32(),
                )
            });
            let cursor_pos = pos + Vec2::new(8.0 + galley.size().x, 4.0);
            let cursor_rect = egui::Rect::from_min_size(
                cursor_pos,
                egui::Vec2::new(1.0, self.font_size.value()),
            );
            ctx.send_viewport_cmd(egui::ViewportCommand::IMERect(cursor_rect));
        }

        apply_text_events(
            ctx,
            &mut self.text_input_buffer,
            &mut self.ime_preedit,
        );
    }

    fn ensure_ime(&mut self, ctx: &egui::Context) {
        if self.text_input_active && self.state == AppState::Canvas {
            if !self.ime_was_enabled {
                info!("[IME] First time sending IMEAllowed(true) + IMEPurpose");
                self.ime_was_enabled = true;
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::IMEAllowed(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::IMEPurpose(egui::IMEPurpose::Normal));
        } else if self.ime_was_enabled {
            self.ime_was_enabled = false;
        }
    }

    fn disable_ime(&self, ctx: &egui::Context) {
        info!("[IME] Disabling IME");
        ctx.send_viewport_cmd(egui::ViewportCommand::IMEAllowed(false));
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
                    bold,
                    italic,
                    underline,
                } => {
                    let fid = egui::FontId::proportional(*size);
                    if *bold {
                        painter.text(*position + Vec2::new(1.2, 0.0), egui::Align2::LEFT_TOP, text, fid.clone(), *color);
                        painter.text(*position + Vec2::new(0.0, 1.0), egui::Align2::LEFT_TOP, text, fid.clone(), *color);
                    }
                    if *italic {
                        let italic_offset = *size * 0.15;
                        painter.text(*position + Vec2::new(italic_offset, 0.0), egui::Align2::LEFT_TOP, text, fid, *color);
                    } else {
                        painter.text(*position, egui::Align2::LEFT_TOP, text, fid, *color);
                    }
                    if *underline {
                        let galley = painter.layout_no_wrap(text.to_string(), egui::FontId::proportional(*size), *color);
                        let y = position.y + galley.size().y + 1.0;
                        let offset = if *italic { *size * 0.15 } else { 0.0 };
                        painter.line_segment(
                            [Pos2::new(position.x + offset, y), Pos2::new(position.x + galley.size().x + offset, y)],
                            Stroke::new(1.0, *color),
                        );
                    }
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

        if self.text_input_active {
            if let Some(pos) = self.text_input_pos {
                let txt = &self.text_input_buffer;
                let pre = &self.ime_preedit;
                let display = if txt.is_empty() && pre.is_empty() {
                    " "
                } else {
                    txt.as_str()
                };
                let size = self.font_size.value();
                let color = self.line_color.to_color32();
                let fid = egui::FontId::proportional(size);
                let fid2 = fid.clone();
                let fid3 = fid.clone();
                let fid4 = fid.clone();
                let fid5 = fid.clone();

                let pre_display = if pre.is_empty() {
                    String::new()
                } else {
                    pre.to_string()
                };

                let galley = painter.layout_no_wrap(display.to_string(), fid, color);
                let pre_galley = if !pre_display.is_empty() {
                    painter.layout_no_wrap(
                        display.to_string() + &pre_display,
                        fid2,
                        color,
                    )
                } else {
                    painter.layout_no_wrap(display.to_string(), fid3, color)
                };
                let txt_w = pre_galley.size().x.max(10.0);
                let txt_h = galley.size().y.max(size + 4.0);

                let bg_rect = Rect::from_min_size(pos, Vec2::new(txt_w + 16.0, txt_h + 8.0));
                painter.rect_filled(bg_rect, 0.0, Color32::from_rgba_premultiplied(40, 40, 40, 200));
                painter.rect_stroke(bg_rect, 0.0, Stroke::new(1.0, Color32::from_rgba_premultiplied(0, 122, 255, 200)));

                let text_pos = pos + Vec2::new(8.0, 4.0);
                let italic_offset = if self.italic { size * 0.15 } else { 0.0 };
                let draw_pos = text_pos + Vec2::new(italic_offset, 0.0);
                if self.bold {
                    painter.text(draw_pos + Vec2::new(1.2, 0.0), egui::Align2::LEFT_TOP, display, fid4.clone(), color);
                    painter.text(draw_pos + Vec2::new(0.0, 1.0), egui::Align2::LEFT_TOP, display, fid4, color);
                }
                painter.text(draw_pos, egui::Align2::LEFT_TOP, display, fid5.clone(), color);

                let mut after_x = draw_pos.x + galley.size().x;
                if !pre_display.is_empty() {
                    let pre_draw_pos = draw_pos + Vec2::new(galley.size().x, 0.0);
                    painter.text(pre_draw_pos, egui::Align2::LEFT_TOP, &pre_display, fid5, color);
                    let underline_y = pre_draw_pos.y + txt_h - 6.0;
                    painter.line_segment(
                        [Pos2::new(pre_draw_pos.x, underline_y), Pos2::new(pre_draw_pos.x + pre_galley.size().x - galley.size().x, underline_y)],
                        Stroke::new(1.5, color),
                    );
                    after_x = pre_draw_pos.x + pre_galley.size().x - galley.size().x;
                }

                if self.underline && !txt.is_empty() && pre.is_empty() {
                    let y = text_pos.y + txt_h - 4.0;
                    painter.line_segment(
                        [Pos2::new(text_pos.x + italic_offset, y), Pos2::new(text_pos.x + txt_w + italic_offset, y)],
                        Stroke::new(1.0, color),
                    );
                }

                let caret_x = after_x + 1.0;
                painter.line_segment(
                    [Pos2::new(caret_x, text_pos.y + 2.0), Pos2::new(caret_x, text_pos.y + txt_h - 4.0)],
                    Stroke::new(1.5, color),
                );
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

        let m = ToolbarMetrics::new(tb);
        let painter = ui.painter();
        let bg = Color32::from_rgba_premultiplied(40, 40, 40, 230);
        painter.rect_filled(tb, 4.0, bg);

        let row1_y = tb.min.y;
        let tools = [Tool::Rectangle, Tool::Ellipse, Tool::Arrow, Tool::Brush, Tool::Text, Tool::Mosaic];
        let white = Color32::WHITE;
        let active_bg = Color32::from_rgba_premultiplied(0, 122, 255, 180);

        for (i, &tool) in tools.iter().enumerate() {
            let btn = tb_button_rect(tb, i, row1_y);
            if self.current_tool == tool {
                painter.rect_filled(btn, 4.0, active_bg);
            }
            let center = btn.center();
            let stroke = Stroke::new(1.5, white);
            match tool {
                Tool::Rectangle => {
                    let r = Rect::from_center_size(center, Vec2::new(ICON_SZ, ICON_SZ * 0.75));
                    painter.rect_stroke(r, 2.0, stroke);
                }
                Tool::Ellipse => {
                    painter.circle_stroke(center, ICON_SZ * 0.46, stroke);
                }
                Tool::Arrow => {
                    let yc = center.y;
                    let x0 = center.x - ICON_SZ * 0.45;
                    let x1 = center.x + ICON_SZ * 0.45;
                    painter.line_segment([Pos2::new(x0, yc), Pos2::new(x1, yc)], stroke);
                    let tip = ICON_SZ * 0.25;
                    painter.line_segment([Pos2::new(x1, yc), Pos2::new(x1 - tip, yc - tip)], stroke);
                    painter.line_segment([Pos2::new(x1, yc), Pos2::new(x1 - tip, yc + tip)], stroke);
                }
                Tool::Brush => {
                    let h = ICON_SZ * 0.4;
                    let w = ICON_SZ * 0.35;
                    let pts = [
                        Pos2::new(center.x - w * 0.8, center.y + h),
                        Pos2::new(center.x - w * 0.3, center.y + h * 0.6),
                        Pos2::new(center.x + w * 0.25, center.y + h * 0.1),
                        Pos2::new(center.x + w * 0.7, center.y - h * 0.4),
                        Pos2::new(center.x + w * 1.2, center.y - h * 0.6),
                    ];
                    for pair in pts.windows(2) {
                        painter.line_segment([pair[0], pair[1]], stroke);
                    }
                }
                Tool::Text => {
                    painter.text(center, egui::Align2::CENTER_CENTER, "T", egui::FontId::proportional(14.0), white);
                }
                Tool::Mosaic => {
                    let s = ICON_SZ * 0.28;
                    for r in 0..3 {
                        for c in 0..3 {
                            let fill = if (r + c) % 2 == 0 { Color32::from_gray(180) } else { Color32::from_gray(80) };
                            let sq = Rect::from_min_size(
                                Pos2::new(center.x - s * 1.5 + c as f32 * s, center.y - s * 1.5 + r as f32 * s),
                                Vec2::new(s, s),
                            );
                            painter.rect_filled(sq, 1.0, fill);
                        }
                    }
                }
                _ => {}
            }
        }

        let save_btn = tb_button_rect(tb, 6, row1_y);
        let sc = save_btn.center();
        let stroke = Stroke::new(1.5, white);
        painter.line_segment([Pos2::new(sc.x, sc.y - 6.0), Pos2::new(sc.x, sc.y + 3.0)], stroke);
        painter.line_segment([Pos2::new(sc.x - 5.0, sc.y - 1.0), Pos2::new(sc.x, sc.y + 3.0)], stroke);
        painter.line_segment([Pos2::new(sc.x + 5.0, sc.y - 1.0), Pos2::new(sc.x, sc.y + 3.0)], stroke);
        painter.line_segment([Pos2::new(sc.x - 3.0, sc.y + 6.0), Pos2::new(sc.x + 3.0, sc.y + 6.0)], Stroke::new(2.0, white));

        painter.line_segment(
            [Pos2::new(m.divider_x, row1_y + 6.0), Pos2::new(m.divider_x, row1_y + TOOLBAR_ROW1_H - 6.0)],
            Stroke::new(1.0, Color32::from_gray(120)),
        );

        let right_start = m.right_start;
        let action_colors = [white, Color32::from_rgb(255, 80, 80), Color32::from_rgb(80, 255, 80)];
        for i in 0..3 {
            let bx = right_start + i as f32 * (BTN_W + BTN_GAP);
            let btn = Rect::from_min_size(Pos2::new(bx, row1_y + 4.0), Vec2::new(BTN_W, TOOLBAR_ROW1_H - 8.0));
            let c = btn.center();
            let color = action_colors[i];
            match i {
                0 => {
                    let s = Stroke::new(1.5, color);
                    let arc_r = 5.5;
                    for j in 0..7 {
                        let a0 = std::f32::consts::PI * 0.4 + j as f32 * (std::f32::consts::PI * 1.0 / 6.0);
                        let a1 = std::f32::consts::PI * 0.4 + (j + 1) as f32 * (std::f32::consts::PI * 1.0 / 6.0);
                        painter.line_segment([
                            Pos2::new(c.x + arc_r * a0.cos(), c.y + arc_r * a0.sin()),
                            Pos2::new(c.x + arc_r * a1.cos(), c.y + arc_r * a1.sin()),
                        ], s);
                    }
                    let tip_x = c.x + arc_r * (std::f32::consts::PI * 0.4).cos();
                    let tip_y = c.y + arc_r * (std::f32::consts::PI * 0.4).sin();
                    let tl = 3.0;
                    painter.line_segment([Pos2::new(tip_x, tip_y), Pos2::new(tip_x + tl, tip_y - tl)], s);
                    painter.line_segment([Pos2::new(tip_x, tip_y), Pos2::new(tip_x + tl, tip_y + tl)], s);
                }
                1 => {
                    let s = Stroke::new(2.0, color);
                    let d = 7.0;
                    painter.line_segment([Pos2::new(c.x - d, c.y - d), Pos2::new(c.x + d, c.y + d)], s);
                    painter.line_segment([Pos2::new(c.x + d, c.y - d), Pos2::new(c.x - d, c.y + d)], s);
                }
                2 => {
                    let s = Stroke::new(2.0, color);
                    let d = 6.0;
                    painter.line_segment([Pos2::new(c.x - d, c.y), Pos2::new(c.x - 2.0, c.y + d)], s);
                    painter.line_segment([Pos2::new(c.x - 2.0, c.y + d), Pos2::new(c.x + d, c.y - d + 2.0)], s);
                }
                _ => {}
            }
        }

        if self.toolbar_has_row2() {
            let row2_y = tb.min.y + TOOLBAR_ROW1_H;
            let is_draw_tool = matches!(self.current_tool, Tool::Rectangle | Tool::Ellipse | Tool::Arrow | Tool::Brush);
            let is_text_tool = matches!(self.current_tool, Tool::Text);

        if is_draw_tool {
            let widths = [StrokeWidth::Thin, StrokeWidth::Medium, StrokeWidth::Thick];
            let radii = [2.5, 5.0, 7.0];
            for (i, (&w, &r)) in widths.iter().zip(radii.iter()).enumerate() {
                let bx = tb.min.x + tb_button_x(i);
                let br = Rect::from_min_size(Pos2::new(bx, row2_y + 2.0), Vec2::new(BTN_W, TOOLBAR_ROW2_H - 4.0));
                if self.stroke_width == w {
                    painter.rect_filled(br, 3.0, active_bg);
                }
                painter.circle_filled(br.center(), r, white);
            }
            let color_start = tb.min.x + tb_button_x(3) + 8.0;
            for (i, &(lc, col)) in PRESET_COLORS.iter().enumerate() {
                let bx = color_start + i as f32 * 24.0;
                let cr = Rect::from_min_size(Pos2::new(bx, row2_y + 5.0), Vec2::new(20.0, TOOLBAR_ROW2_H - 10.0));
                painter.rect_filled(cr, 3.0, col);
                if self.line_color == lc {
                    painter.rect_stroke(cr.expand(2.0), 2.0, Stroke::new(2.0, white));
                }
            }
        } else if is_text_tool {
            let sz_btn = tb_button_rect(tb, 0, row2_y);
            if self.show_size_picker {
                painter.rect_filled(sz_btn, 3.0, active_bg);
            }
            let sz_label = match self.font_size {
                FontSize::Small => "小",
                FontSize::Medium => "中",
                FontSize::Large => "大",
            };
            painter.text(sz_btn.center(), egui::Align2::CENTER_CENTER, sz_label, egui::FontId::proportional(11.0), white);
            let arrow = Rect::from_min_size(
                Pos2::new(sz_btn.max.x - 12.0, sz_btn.min.y + 5.0),
                Vec2::new(10.0, 10.0),
            );
            painter.line_segment(
                [Pos2::new(arrow.min.x + 2.0, arrow.min.y + 3.0), Pos2::new(arrow.center().x, arrow.max.y - 2.0)],
                Stroke::new(1.2, white),
            );
            painter.line_segment(
                [Pos2::new(arrow.center().x, arrow.max.y - 2.0), Pos2::new(arrow.max.x - 2.0, arrow.min.y + 3.0)],
                Stroke::new(1.2, white),
            );

            let bold_btn = tb_button_rect(tb, 1, row2_y);
            if self.bold {
                painter.rect_filled(bold_btn, 3.0, active_bg);
            }
            let bc = if self.bold { Color32::from_rgb(100, 200, 255) } else { white };
            painter.text(bold_btn.center(), egui::Align2::CENTER_CENTER, "B", egui::FontId::proportional(13.0), bc);

            let italic_btn = tb_button_rect(tb, 2, row2_y);
            if self.italic {
                painter.rect_filled(italic_btn, 3.0, active_bg);
            }
            let ic = if self.italic { Color32::from_rgb(100, 200, 255) } else { white };
            painter.text(italic_btn.center(), egui::Align2::CENTER_CENTER, "I", egui::FontId::proportional(13.0), ic);

            let underline_btn = tb_button_rect(tb, 3, row2_y);
            if self.underline {
                painter.rect_filled(underline_btn, 3.0, active_bg);
            }
            let uc = if self.underline { Color32::from_rgb(100, 200, 255) } else { white };
            painter.text(underline_btn.center(), egui::Align2::CENTER_CENTER, "U", egui::FontId::proportional(13.0), uc);

            let color_btn_x = tb.min.x + tb_button_x(4);
            let color_btn = Rect::from_min_size(Pos2::new(color_btn_x, row2_y + 2.0), Vec2::new(BTN_W, TOOLBAR_ROW2_H - 4.0));
            if self.show_color_picker {
                painter.rect_filled(color_btn, 3.0, active_bg);
            }
            let cc = self.line_color.to_color32();
            painter.circle_filled(color_btn.center(), 10.0, cc);
            painter.circle_stroke(color_btn.center(), 10.0, Stroke::new(1.5, white));
            }
        }
    }

    fn render_size_picker(&mut self, ctx: &egui::Context, _screen_rect: Rect) {
        if !self.show_size_picker {
            return;
        }

        let picker_rect = match self.size_picker_rect() {
            Some(r) => r,
            None => return,
        };

        let painter = ctx.debug_painter();
        let bg = Color32::from_rgba_premultiplied(30, 30, 30, 250);
        painter.rect_filled(picker_rect, 6.0, bg);
        painter.rect_stroke(picker_rect, 6.0, Stroke::new(1.0, Color32::from_gray(80)));

        let active_bg = Color32::from_rgba_premultiplied(0, 122, 255, 140);
        let active_text = Color32::from_rgb(100, 200, 255);
        let white = Color32::WHITE;

        let items: [(FontSize, &str); 3] = [
            (FontSize::Small, "小 (16px)"),
            (FontSize::Medium, "中 (24px)"),
            (FontSize::Large, "大 (36px)"),
        ];
        let item_h = 22.0;
        let gap = 4.0;
        for (i, &(fs, label)) in items.iter().enumerate() {
            let y = picker_rect.min.y + 8.0 + i as f32 * (item_h + gap);
            let ir = Rect::from_min_size(
                Pos2::new(picker_rect.min.x + 4.0, y),
                Vec2::new(picker_rect.width() - 8.0, item_h),
            );
            if self.font_size == fs {
                painter.rect_filled(ir, 3.0, active_bg);
            }
            let clr = if self.font_size == fs { active_text } else { white };
            painter.text(
                Pos2::new(ir.min.x + 8.0, ir.center().y),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(12.0),
                clr,
            );

            let pointer = ctx.input(|i| i.pointer.interact_pos());
            let pressed = ctx.input(|i| i.pointer.button_down(egui::PointerButton::Primary));
            if let Some(pp) = pointer {
                if pressed && ir.contains(pp) {
                    self.font_size = fs;
                    self.show_size_picker = false;
                }
            }
        }
    }

    fn render_color_picker(&mut self, ctx: &egui::Context, _screen_rect: Rect) {
        if !self.show_color_picker {
            return;
        }

        let picker_rect = match self.color_picker_rect() {
            Some(r) => r,
            None => return,
        };

        let wheel_radius = 65.0;
        let wheel_center = Pos2::new(picker_rect.center().x, picker_rect.min.y + 40.0 + wheel_radius);

        let painter = ctx.debug_painter();
        let bg = Color32::from_rgba_premultiplied(30, 30, 30, 250);
        painter.rect_filled(picker_rect, 8.0, bg);
        painter.rect_stroke(picker_rect, 8.0, Stroke::new(1.0, Color32::from_gray(80)));

        let n_segments = 120;
        let mut mesh = egui::Mesh::with_texture(egui::TextureId::Managed(0));
        let center_idx = mesh.vertices.len() as u32;
        mesh.vertices.push(egui::epaint::Vertex {
            pos: wheel_center,
            uv: Pos2::ZERO,
            color: Color32::WHITE,
        });
        for i in 0..=n_segments {
            let angle = (i as f32 / n_segments as f32) * std::f32::consts::TAU;
            let sat = if i == n_segments { 1.0 } else { 1.0 };
            let (r, g, b) = hsv_to_rgb(angle, sat, 1.0);
            let color = Color32::from_rgb(r, g, b);
            let x = wheel_center.x + wheel_radius * angle.cos();
            let y = wheel_center.y + wheel_radius * angle.sin();
            let idx = mesh.vertices.len() as u32;
            mesh.vertices.push(egui::epaint::Vertex {
                pos: Pos2::new(x, y),
                uv: Pos2::ZERO,
                color,
            });
            if i > 0 {
                mesh.indices.push(center_idx);
                mesh.indices.push(idx - 1);
                mesh.indices.push(idx);
            }
        }
        painter.add(egui::Shape::Mesh(mesh));

        let pointer = ctx.input(|i| i.pointer.interact_pos());
        let pressed = ctx.input(|i| i.pointer.button_down(egui::PointerButton::Primary));
        if let Some(pointer_pos) = pointer {
            let dx = pointer_pos.x - wheel_center.x;
            let dy = pointer_pos.y - wheel_center.y;
            let dist = (dx * dx + dy * dy).sqrt();
            if pressed && dist <= wheel_radius {
                let angle = dy.atan2(dx);
                let hue = if angle < 0.0 { angle + std::f32::consts::TAU } else { angle };
                let sat = (dist / wheel_radius).min(1.0);
                let (r, g, b) = hsv_to_rgb(hue, sat, 1.0);
                self.line_color = LineColor::Custom(Color32::from_rgb(r, g, b));
            }
        }

        let current_color = self.line_color.to_color32();
        let preview_rect = Rect::from_center_size(
            Pos2::new(picker_rect.center().x, picker_rect.max.y - 28.0),
            Vec2::new(100.0, 24.0),
        );
        painter.rect_filled(preview_rect, 4.0, current_color);
        painter.rect_stroke(preview_rect, 4.0, Stroke::new(1.0, Color32::WHITE));

        let label_pos = preview_rect.center();
        let label_color = if current_color.r() as u16 + current_color.g() as u16 + current_color.b() as u16 > 380 {
            Color32::BLACK
        } else {
            Color32::WHITE
        };
        painter.text(
            label_pos,
            egui::Align2::CENTER_CENTER,
            "当前颜色",
            egui::FontId::proportional(11.0),
            label_color,
        );

        let preset_y = wheel_center.y + wheel_radius + 14.0;
        let preset_start_x = picker_rect.center().x - 5.0 * 22.0 / 2.0;
        for (i, &(_, col)) in PRESET_COLORS.iter().enumerate() {
            let bx = preset_start_x + i as f32 * 22.0;
            let pr = Rect::from_min_size(Pos2::new(bx, preset_y), Vec2::new(18.0, 18.0));
            painter.rect_filled(pr, 3.0, col);
            painter.rect_stroke(pr, 3.0, Stroke::new(1.0, Color32::from_gray(120)));
            if let Some(pointer_pos) = pointer {
                if pressed && pr.contains(pointer_pos) {
                    match i {
                        0 => self.line_color = LineColor::Red,
                        1 => self.line_color = LineColor::Yellow,
                        2 => self.line_color = LineColor::Green,
                        3 => self.line_color = LineColor::Blue,
                        4 => self.line_color = LineColor::White,
                        _ => {}
                    }
                }
            }
        }

        let close_btn = Rect::from_center_size(
            Pos2::new(picker_rect.max.x - 16.0, picker_rect.min.y + 16.0),
            Vec2::new(20.0, 20.0),
        );
        let close_color = Color32::from_rgb(200, 200, 200);
        painter.line_segment(
            [Pos2::new(close_btn.min.x + 4.0, close_btn.min.y + 4.0), Pos2::new(close_btn.max.x - 4.0, close_btn.max.y - 4.0)],
            Stroke::new(2.0, close_color),
        );
        painter.line_segment(
            [Pos2::new(close_btn.max.x - 4.0, close_btn.min.y + 4.0), Pos2::new(close_btn.min.x + 4.0, close_btn.max.y - 4.0)],
            Stroke::new(2.0, close_color),
        );

        if let Some(pointer_pos) = pointer {
            if pressed && close_btn.contains(pointer_pos) {
                self.show_color_picker = false;
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

fn apply_text_events(ctx: &egui::Context, buf: &mut String, preedit: &mut String) {
    ctx.input(|i| {
        for event in &i.events {
            // Log every event type for debugging
            let evt_debug = match event {
                egui::Event::Text(t) => format!("Text({:?})", t),
                egui::Event::Ime(ie) => format!("Ime({:?})", ie),
                egui::Event::Key { key, pressed, modifiers, .. } => if *pressed { format!("Key({:?} mod={:?})", key, modifiers) } else { String::new() },
                egui::Event::PointerMoved(_) => "PointerMoved".into(),
                egui::Event::PointerButton { button, pressed, .. } => format!("PtrBtn({:?} {})", button, if *pressed {"down"} else {"up"}),
                egui::Event::PointerGone => "PointerGone".into(),
                egui::Event::MouseWheel { .. } => "MouseWheel".into(),
                other => format!("Other({:?})", other),
            };
            if !evt_debug.is_empty() {
                info!("[TextInput] Event: {}", evt_debug);
            }

            match event {
                egui::Event::Text(text) => {
                    buf.push_str(text);
                }
                egui::Event::Ime(ime_event) => match ime_event {
                    egui::ImeEvent::Enabled => {}
                    egui::ImeEvent::Preedit(text) => {
                        *preedit = text.clone();
                    }
                    egui::ImeEvent::Commit(text) => {
                        buf.push_str(text);
                        preedit.clear();
                    }
                    egui::ImeEvent::Disabled => {
                        preedit.clear();
                    }
                },
                _ => {}
            }
        }
    });

    if ctx.input(|i| i.key_pressed(egui::Key::Backspace)) {
        if !preedit.is_empty() {
            preedit.clear();
        } else {
            buf.pop();
        }
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    info!("截图工具启动");
    info!("[IME] XMODIFIERS={:?} GTK_IM_MODULE={:?} QT_IM_MODULE={:?}",
        std::env::var("XMODIFIERS").ok(),
        std::env::var("GTK_IM_MODULE").ok(),
        std::env::var("QT_IM_MODULE").ok());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_fullscreen(true)
            .with_decorations(false)
            .with_always_on_top(),
        window_builder: Some(Box::new(|builder| {
            #[cfg(target_os = "linux")]
            {
                info!("[IME] Window builder hook: enabling IME on window creation");
            }
            builder
        })),
        ..Default::default()
    };

    eframe::run_native(
        "截图工具",
        options,
        Box::new(|_cc| Ok(Box::new(ScreenshotApp::new()))),
    )
    .expect("启动截图工具失败");
}

#[cfg(test)]
/// Simplified event for testing text input logic (no egui context needed).
#[derive(Clone, Debug, PartialEq)]
enum TextEv {
    Text(String),
    Preedit(String),
    Commit(String),
    Enabled,
    Disabled,
    Backspace,
    Enter,
}

#[cfg(test)]
fn process_text_ev(event: &TextEv, buf: &mut String, preedit: &mut String) {
    match event {
        TextEv::Text(t) => buf.push_str(t),
        TextEv::Enabled => {}
        TextEv::Preedit(t) => *preedit = t.clone(),
        TextEv::Commit(t) => {
            buf.push_str(t);
            preedit.clear();
        }
        TextEv::Disabled => preedit.clear(),
        TextEv::Backspace => {
            if !preedit.is_empty() {
                preedit.clear();
            } else {
                buf.pop();
            }
        }
        TextEv::Enter => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_events(events: &[TextEv]) -> (String, String) {
        let mut buf = String::new();
        let mut preedit = String::new();
        for ev in events {
            process_text_ev(ev, &mut buf, &mut preedit);
        }
        (buf, preedit)
    }

    #[test]
    fn english_typewriter_input() {
        let (buf, pre) = run_events(&[
            TextEv::Text("H".into()),
            TextEv::Text("e".into()),
            TextEv::Text("l".into()),
            TextEv::Text("l".into()),
            TextEv::Text("o".into()),
        ]);
        assert_eq!(buf, "Hello");
        assert!(pre.is_empty());
    }

    #[test]
    fn chinese_via_event_text() {
        // Chinese characters arrive as committed Event::Text
        let (buf, pre) = run_events(&[
            TextEv::Text("你".into()),
            TextEv::Text("好".into()),
            TextEv::Text("世".into()),
            TextEv::Text("界".into()),
        ]);
        assert_eq!(buf, "你好世界");
        assert!(pre.is_empty());
    }

    #[test]
    fn chinese_via_ime_commit() {
        // IME flow: Preedit displays composing text, Commit finalizes
        let (buf, pre) = run_events(&[
            TextEv::Enabled,
            TextEv::Preedit("n".into()),
            TextEv::Preedit("ni".into()),
            TextEv::Preedit("你好".into()),
            TextEv::Commit("你好".into()),
        ]);
        assert_eq!(buf, "你好");
        assert!(pre.is_empty());
    }

    #[test]
    fn chinese_multi_word_ime() {
        // Type two Chinese words through IME
        let (buf, pre) = run_events(&[
            TextEv::Enabled,
            TextEv::Preedit("zhong".into()),
            TextEv::Preedit("中国".into()),
            TextEv::Commit("中国".into()),
            TextEv::Preedit("jia".into()),
            TextEv::Preedit("加油".into()),
            TextEv::Commit("加油".into()),
        ]);
        assert_eq!(buf, "中国加油");
        assert!(pre.is_empty());
    }

    #[test]
    fn chinese_mixed_english() {
        // Mixed Chinese and English input
        let (buf, pre) = run_events(&[
            TextEv::Text("Rust ".into()),
            TextEv::Enabled,
            TextEv::Preedit("zhongwen".into()),
            TextEv::Preedit("中文".into()),
            TextEv::Commit("中文".into()),
            TextEv::Text(" is".into()),
            TextEv::Text(" great".into()),
        ]);
        assert_eq!(buf, "Rust 中文 is great");
        assert!(pre.is_empty());
    }

    #[test]
    fn ime_preedit_canceled_with_backspace() {
        // User types in IME, sees preedit, then hits Backspace to cancel
        let (buf, pre) = run_events(&[
            TextEv::Enabled,
            TextEv::Preedit("ni".into()),
            TextEv::Preedit("你好".into()),
            TextEv::Backspace,          // clears preedit
        ]);
        assert!(buf.is_empty());
        assert!(pre.is_empty());
    }

    #[test]
    fn backspace_deletes_committed_text() {
        // After committing, backspace deletes committed text
        let (buf, pre) = run_events(&[
            TextEv::Text("H".into()),
            TextEv::Text("i".into()),
            TextEv::Backspace,
        ]);
        assert_eq!(buf, "H");
        assert!(pre.is_empty());
    }

    #[test]
    fn chinese_punctuation() {
        // Chinese punctuation input
        let (buf, pre) = run_events(&[
            TextEv::Text("你好".into()),
            TextEv::Text("，".into()),
            TextEv::Text("欢迎".into()),
            TextEv::Text("！".into()),
        ]);
        assert_eq!(buf, "你好，欢迎！");
        assert!(pre.is_empty());
    }

    #[test]
    fn ime_commit_then_preedit_does_not_overflow() {
        // After committing, starting a new preedit should work
        let (buf, pre) = run_events(&[
            TextEv::Enabled,
            TextEv::Preedit("中文".into()),
            TextEv::Commit("中文".into()),
            TextEv::Preedit("编程".into()),
        ]);
        assert_eq!(buf, "中文");
        assert_eq!(pre, "编程");
    }

    #[test]
    fn empty_commit_is_handled() {
        // Some IMEs might send empty commits
        let (buf, pre) = run_events(&[
            TextEv::Enabled,
            TextEv::Commit("".into()),
            TextEv::Commit("测试".into()),
        ]);
        assert_eq!(buf, "测试");
        assert!(pre.is_empty());
    }

    #[test]
    fn chinese_text_and_enter_same_frame() {
        // Critical bug scenario: IME commit ("你好") and Enter key arrive in same frame.
        // handle_text_input must process Event::Text BEFORE Enter key is checked,
        // otherwise the committed text is lost.
        let (buf, _pre) = run_events(&[
            TextEv::Enabled,
            TextEv::Preedit("ni".into()),
            TextEv::Preedit("你好".into()),
            TextEv::Commit("你好".into()),
            TextEv::Enter,  // arrives same frame as Commit
        ]);
        assert_eq!(buf, "你好", "Chinese text should be in buffer before Enter fires finish_text_input");
    }
}