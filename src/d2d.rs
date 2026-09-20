use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use windows::core::{w, Interface};
use windows::Foundation::Numerics::Matrix3x2;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM};

pub struct Brushes {
    pub bg: ID2D1SolidColorBrush,
    pub card: ID2D1SolidColorBrush,
    pub card_hover: ID2D1SolidColorBrush,
    pub card_selected: ID2D1SolidColorBrush,
    pub accent: ID2D1SolidColorBrush,
    pub favorite: ID2D1SolidColorBrush,
    pub favorite_bg: ID2D1SolidColorBrush,
    pub favorite_selected_bg: ID2D1SolidColorBrush,
    pub border: ID2D1SolidColorBrush,
    pub border_subtle: ID2D1SolidColorBrush,
    pub text_primary: ID2D1SolidColorBrush,
    pub text_secondary: ID2D1SolidColorBrush,
    pub text_muted: ID2D1SolidColorBrush,
    pub badge_bg: ID2D1SolidColorBrush,
    pub dock_bg: ID2D1SolidColorBrush,
    pub dock_border: ID2D1SolidColorBrush,
    pub trash: ID2D1SolidColorBrush,
    pub trash_bg: ID2D1SolidColorBrush,
    pub search_bg: ID2D1SolidColorBrush,
    pub pill_bg: ID2D1SolidColorBrush,
    pub scroll_thumb: ID2D1SolidColorBrush,
    pub syn_keyword: ID2D1SolidColorBrush,
    pub syn_string: ID2D1SolidColorBrush,
    pub syn_number: ID2D1SolidColorBrush,
    pub syn_comment: ID2D1SolidColorBrush,
    pub syn_type: ID2D1SolidColorBrush,
    pub syn_function: ID2D1SolidColorBrush,
    pub syn_property: ID2D1SolidColorBrush,
}

impl Brushes {
    pub fn new(target: &ID2D1RenderTarget) -> windows::core::Result<Self> {
        unsafe {
            Ok(Self {
                bg: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 24.0 / 255.0, g: 24.0 / 255.0, b: 37.0 / 255.0, a: 1.0 }, None)?,
                card: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 30.0 / 255.0, g: 30.0 / 255.0, b: 46.0 / 255.0, a: 1.0 }, None)?,
                card_hover: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 38.0 / 255.0, g: 38.0 / 255.0, b: 58.0 / 255.0, a: 1.0 }, None)?,
                card_selected: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 49.0 / 255.0, g: 50.0 / 255.0, b: 68.0 / 255.0, a: 1.0 }, None)?,
                accent: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 100.0 / 255.0, g: 108.0 / 255.0, b: 1.0, a: 1.0 }, None)?,
                favorite: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 251.0 / 255.0, g: 191.0 / 255.0, b: 36.0 / 255.0, a: 1.0 }, None)?,
                favorite_bg: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 251.0 / 255.0, g: 191.0 / 255.0, b: 36.0 / 255.0, a: 0.15 }, None)?,
                favorite_selected_bg: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 251.0 / 255.0, g: 191.0 / 255.0, b: 36.0 / 255.0, a: 0.22 }, None)?,
                border: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 0.08 }, None)?,
                border_subtle: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 0.04 }, None)?,
                text_primary: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 241.0 / 255.0, g: 245.0 / 255.0, b: 249.0 / 255.0, a: 1.0 }, None)?,
                text_secondary: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 203.0 / 255.0, g: 213.0 / 255.0, b: 225.0 / 255.0, a: 1.0 }, None)?,
                text_muted: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 108.0 / 255.0, g: 112.0 / 255.0, b: 134.0 / 255.0, a: 1.0 }, None)?,
                badge_bg: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.35 }, None)?,
                dock_bg: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 20.0 / 255.0, g: 20.0 / 255.0, b: 34.0 / 255.0, a: 0.95 }, None)?,
                dock_border: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 0.15 }, None)?,
                trash: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 248.0 / 255.0, g: 113.0 / 255.0, b: 113.0 / 255.0, a: 1.0 }, None)?,
                trash_bg: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 248.0 / 255.0, g: 113.0 / 255.0, b: 113.0 / 255.0, a: 0.15 }, None)?,
                search_bg: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.30 }, None)?,
                pill_bg: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.30 }, None)?,
                scroll_thumb: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 0.20 }, None)?,
                syn_keyword: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 203.0 / 255.0, g: 166.0 / 255.0, b: 247.0 / 255.0, a: 1.0 }, None)?,
                syn_string: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 166.0 / 255.0, g: 227.0 / 255.0, b: 161.0 / 255.0, a: 1.0 }, None)?,
                syn_number: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 250.0 / 255.0, g: 179.0 / 255.0, b: 135.0 / 255.0, a: 1.0 }, None)?,
                syn_comment: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 127.0 / 255.0, g: 132.0 / 255.0, b: 156.0 / 255.0, a: 1.0 }, None)?,
                syn_type: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 137.0 / 255.0, g: 220.0 / 255.0, b: 235.0 / 255.0, a: 1.0 }, None)?,
                syn_function: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 249.0 / 255.0, g: 226.0 / 255.0, b: 175.0 / 255.0, a: 1.0 }, None)?,
                syn_property: target.CreateSolidColorBrush(&D2D1_COLOR_F { r: 137.0 / 255.0, g: 180.0 / 255.0, b: 250.0 / 255.0, a: 1.0 }, None)?,
            })
        }
    }
}

pub struct TextFormats {
    pub counter: IDWriteTextFormat,
    pub search: IDWriteTextFormat,
    pub card_text: IDWriteTextFormat,
    pub row_text: IDWriteTextFormat,
    pub meta: IDWriteTextFormat,
    pub dock: IDWriteTextFormat,
    pub title: IDWriteTextFormat,
    pub label: IDWriteTextFormat,
    pub small: IDWriteTextFormat,
}

pub struct D2dContext {
    pub factory: ID2D1Factory,
    pub dwrite_factory: IDWriteFactory,
    pub hwnd_target: Option<ID2D1HwndRenderTarget>,
    pub render_target: Option<ID2D1RenderTarget>,
    pub brushes: Option<Brushes>,
    pub formats: Option<TextFormats>,
    pub bitmap_cache: RefCell<HashMap<(String, u32), ID2D1Bitmap>>,
    pub current_size: (u32, u32),
}

impl D2dContext {
    pub fn new() -> windows::core::Result<Self> {
        unsafe {
            let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dwrite_factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;

            let formats = Self::create_text_formats(&dwrite_factory)?;

            Ok(Self {
                factory,
                dwrite_factory,
                hwnd_target: None,
                render_target: None,
                brushes: None,
                formats: Some(formats),
                bitmap_cache: RefCell::new(HashMap::new()),
                current_size: (0, 0),
            })
        }
    }

    fn create_text_formats(dwrite: &IDWriteFactory) -> windows::core::Result<TextFormats> {
        unsafe {
            let locale = w!("en-us");
            let segoe = w!("Segoe UI");
            let consolas = w!("Consolas");

            let counter = dwrite.CreateTextFormat(
                segoe,
                None,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                13.0,
                locale,
            )?;
            counter.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            counter.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;

            let search = dwrite.CreateTextFormat(
                segoe,
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                13.0,
                locale,
            )?;
            search.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;

            let card_text = dwrite.CreateTextFormat(
                consolas,
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                13.0,
                locale,
            )?;
            card_text.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP)?;

            let row_text = dwrite.CreateTextFormat(
                segoe,
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                13.0,
                locale,
            )?;
            row_text.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            row_text.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;

            let meta = dwrite.CreateTextFormat(
                segoe,
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                11.0,
                locale,
            )?;
            meta.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            meta.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;

            let dock = dwrite.CreateTextFormat(
                segoe,
                None,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                11.0,
                locale,
            )?;
            dock.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            dock.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;

            let title = dwrite.CreateTextFormat(
                segoe,
                None,
                DWRITE_FONT_WEIGHT_BOLD,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                15.0,
                locale,
            )?;

            let label = dwrite.CreateTextFormat(
                segoe,
                None,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                12.5,
                locale,
            )?;

            let small = dwrite.CreateTextFormat(
                segoe,
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                10.0,
                locale,
            )?;

            Ok(TextFormats {
                counter,
                search,
                card_text,
                row_text,
                meta,
                dock,
                title,
                label,
                small,
            })
        }
    }

    pub fn ensure_target(&mut self, hwnd: HWND, width: u32, height: u32) -> windows::core::Result<()> {
        if let Some(ref target) = self.hwnd_target {
            if self.current_size != (width, height) {
                unsafe {
                    if let Err(err) = target.Resize(&D2D_SIZE_U { width, height }) {
                        self.discard_target();
                        return Err(err);
                    }
                }
                self.current_size = (width, height);
            }
            return Ok(());
        }

        unsafe {
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_R8G8B8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };

            let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd,
                pixelSize: D2D_SIZE_U { width, height },
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            };

            let hwnd_target = self.factory.CreateHwndRenderTarget(&props, &hwnd_props)?;
            let target: ID2D1RenderTarget = hwnd_target.cast()?;
            target.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE);

            let brushes = Brushes::new(&target)?;

            self.hwnd_target = Some(hwnd_target);
            self.render_target = Some(target);
            self.brushes = Some(brushes);
            self.current_size = (width, height);
            Ok(())
        }
    }

    pub fn set_dpi_scale(&self, dpi_scale: f32) {
        if let Some(ref target) = self.render_target {
            let matrix = Matrix3x2 {
                M11: dpi_scale,
                M12: 0.0,
                M21: 0.0,
                M22: dpi_scale,
                M31: 0.0,
                M32: 0.0,
            };
            unsafe {
                target.SetTransform(&matrix);
            }
        }
    }

    pub fn discard_target(&mut self) {
        self.clear_bitmap_cache();
        self.brushes = None;
        self.render_target = None;
        self.hwnd_target = None;
        self.current_size = (0, 0);
    }

    pub fn begin_draw(&self) {
        if let Some(ref target) = self.render_target {
            unsafe {
                target.BeginDraw();
            }
        }
    }

    pub fn end_draw(&self) -> windows::core::Result<()> {
        if let Some(ref target) = self.render_target {
            unsafe {
                target.EndDraw(None, None)?;
            }
        }
        Ok(())
    }

    pub fn clear(&self, color: &D2D1_COLOR_F) {
        if let Some(ref target) = self.render_target {
            unsafe {
                target.Clear(Some(color as *const _));
            }
        }
    }

    pub fn push_clip(&self, x: f32, y: f32, w: f32, h: f32) {
        if let Some(ref target) = self.render_target {
            let rect = D2D_RECT_F {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            };
            unsafe {
                target.PushAxisAlignedClip(&rect, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            }
        }
    }

    pub fn pop_clip(&self) {
        if let Some(ref target) = self.render_target {
            unsafe {
                target.PopAxisAlignedClip();
            }
        }
    }

    pub fn get_or_load_bitmap(&self, path_str: &str, max_edge: u32) -> Option<ID2D1Bitmap> {
        let key = (path_str.to_owned(), max_edge);
        if let Some(bmp) = self.bitmap_cache.borrow().get(&key) {
            return Some(bmp.clone());
        }

        let target = self.render_target.as_ref()?;
        let path = Path::new(path_str);
        if !path.exists() {
            return None;
        }

        let dynamic_img = image::open(path).ok()?;
        // Retain display-sized previews, not full screenshots on the GPU.
        let mut rgba = dynamic_img.thumbnail(max_edge, max_edge).into_rgba8();
        premultiply_alpha(rgba.as_mut());
        let (width, height) = rgba.dimensions();

        unsafe {
            let size = D2D_SIZE_U { width, height };
            let props = D2D1_BITMAP_PROPERTIES {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_R8G8B8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
            };

            let bmp = target
                .CreateBitmap(
                    size,
                    Some(rgba.as_raw().as_ptr() as *const _),
                    width * 4,
                    &props,
                )
                .ok()?;
            let mut cache = self.bitmap_cache.borrow_mut();
            let retained_bytes: u64 = cache.values().map(|bitmap| {
                let size = bitmap.GetPixelSize();
                u64::from(size.width) * u64::from(size.height) * 4
            }).sum();
            if cache.len() >= 8 || retained_bytes + u64::from(width) * u64::from(height) * 4 > 8 * 1024 * 1024 {
                cache.clear();
            }
            cache.insert(key, bmp.clone());
            Some(bmp)
        }
    }

    pub fn clear_bitmap_cache(&self) {
        self.bitmap_cache.borrow_mut().clear();
    }

    #[allow(clippy::too_many_arguments)] // Direct drawing primitive; keep coordinates explicit.
    pub fn draw_rounded_rect(
        &self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        fill: &ID2D1SolidColorBrush,
        stroke: Option<(&ID2D1SolidColorBrush, f32)>,
    ) {
        if let Some(ref target) = self.render_target {
            let rect = D2D1_ROUNDED_RECT {
                rect: D2D_RECT_F {
                    left: x,
                    top: y,
                    right: x + w,
                    bottom: y + h,
                },
                radiusX: r,
                radiusY: r,
            };
            unsafe {
                target.FillRoundedRectangle(&rect, fill);
                if let Some((stroke_brush, stroke_w)) = stroke {
                    target.DrawRoundedRectangle(&rect, stroke_brush, stroke_w, None);
                }
            }
        }
    }

    pub fn draw_rect(&self, x: f32, y: f32, w: f32, h: f32, fill: &ID2D1SolidColorBrush) {
        if let Some(ref target) = self.render_target {
            let rect = D2D_RECT_F {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            };
            unsafe {
                target.FillRectangle(&rect, fill);
            }
        }
    }

    pub fn draw_star(
        &self,
        cx: f32,
        cy: f32,
        radius: f32,
        fill: Option<&ID2D1SolidColorBrush>,
        stroke: Option<(&ID2D1SolidColorBrush, f32)>,
    ) {
        if let Some(ref target) = self.render_target {
            unsafe {
                if let Ok(geometry) = self.factory.CreatePathGeometry() {
                    if let Ok(sink) = geometry.Open() {
                        let inner_radius = radius * 0.45;
                        let points = 5;

                        for i in 0..(points * 2) {
                            let angle = (i as f32) * std::f32::consts::PI / (points as f32)
                                - std::f32::consts::FRAC_PI_2;
                            let r = if i % 2 == 0 { radius } else { inner_radius };
                            let x = cx + angle.cos() * r;
                            let y = cy + angle.sin() * r;
                            let pt = D2D_POINT_2F { x, y };

                            if i == 0 {
                                sink.BeginFigure(pt, D2D1_FIGURE_BEGIN_FILLED);
                            } else {
                                sink.AddLine(pt);
                            }
                        }

                        sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                        let _ = sink.Close();

                        if let Some(fill_brush) = fill {
                            target.FillGeometry(&geometry, fill_brush, None);
                        }
                        if let Some((stroke_brush, stroke_w)) = stroke {
                            target.DrawGeometry(&geometry, stroke_brush, stroke_w, None);
                        }
                    }
                }
            }
        }
    }

    pub fn draw_trash_icon(&self, cx: f32, cy: f32, size: f32, stroke: &ID2D1SolidColorBrush) {
        if let Some(ref target) = self.render_target {
            unsafe {
                // Lid horizontal line
                target.DrawLine(
                    D2D_POINT_2F {
                        x: cx - size * 0.45,
                        y: cy - size * 0.32,
                    },
                    D2D_POINT_2F {
                        x: cx + size * 0.45,
                        y: cy - size * 0.32,
                    },
                    stroke,
                    1.4,
                    None,
                );
                // Top handle
                target.DrawLine(
                    D2D_POINT_2F {
                        x: cx - size * 0.18,
                        y: cy - size * 0.46,
                    },
                    D2D_POINT_2F {
                        x: cx + size * 0.18,
                        y: cy - size * 0.46,
                    },
                    stroke,
                    1.4,
                    None,
                );
                // Can body
                let can_rect = D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: cx - size * 0.35,
                        top: cy - size * 0.22,
                        right: cx + size * 0.35,
                        bottom: cy + size * 0.48,
                    },
                    radiusX: 1.5,
                    radiusY: 1.5,
                };
                target.DrawRoundedRectangle(&can_rect, stroke, 1.4, None);
                // Two inner vertical ribs (Lucide trash-2 style)
                let rib_x1 = cx - size * 0.13;
                let rib_x2 = cx + size * 0.13;
                let rib_y1 = cy - size * 0.08;
                let rib_y2 = cy + size * 0.34;
                target.DrawLine(
                    D2D_POINT_2F { x: rib_x1, y: rib_y1 },
                    D2D_POINT_2F { x: rib_x1, y: rib_y2 },
                    stroke,
                    1.2,
                    None,
                );
                target.DrawLine(
                    D2D_POINT_2F { x: rib_x2, y: rib_y1 },
                    D2D_POINT_2F { x: rib_x2, y: rib_y2 },
                    stroke,
                    1.2,
                    None,
                );
            }
        }
    }

    pub fn draw_close_icon(&self, cx: f32, cy: f32, size: f32, stroke: &ID2D1SolidColorBrush) {
        if let Some(ref target) = self.render_target {
            unsafe {
                let half = size * 0.5;
                target.DrawLine(
                    D2D_POINT_2F {
                        x: cx - half,
                        y: cy - half,
                    },
                    D2D_POINT_2F {
                        x: cx + half,
                        y: cy + half,
                    },
                    stroke,
                    1.6,
                    None,
                );
                target.DrawLine(
                    D2D_POINT_2F {
                        x: cx - half,
                        y: cy + half,
                    },
                    D2D_POINT_2F {
                        x: cx + half,
                        y: cy - half,
                    },
                    stroke,
                    1.6,
                    None,
                );
            }
        }
    }

    pub fn draw_search_icon(&self, cx: f32, cy: f32, radius: f32, stroke: &ID2D1SolidColorBrush) {
        if let Some(ref target) = self.render_target {
            unsafe {
                let ellipse = D2D1_ELLIPSE {
                    point: D2D_POINT_2F {
                        x: cx - 1.2,
                        y: cy - 1.2,
                    },
                    radiusX: radius * 0.62,
                    radiusY: radius * 0.62,
                };
                target.DrawEllipse(&ellipse, stroke, 1.4, None);
                target.DrawLine(
                    D2D_POINT_2F {
                        x: cx + radius * 0.32,
                        y: cy + radius * 0.32,
                    },
                    D2D_POINT_2F {
                        x: cx + radius * 0.95,
                        y: cy + radius * 0.95,
                    },
                    stroke,
                    1.5,
                    None,
                );
            }
        }
    }

    pub fn draw_check_icon(&self, cx: f32, cy: f32, size: f32, stroke: &ID2D1SolidColorBrush) {
        if let Some(ref target) = self.render_target {
            unsafe {
                target.DrawLine(
                    D2D_POINT_2F {
                        x: cx - size * 0.4,
                        y: cy,
                    },
                    D2D_POINT_2F {
                        x: cx - size * 0.1,
                        y: cy + size * 0.35,
                    },
                    stroke,
                    1.8,
                    None,
                );
                target.DrawLine(
                    D2D_POINT_2F {
                        x: cx - size * 0.1,
                        y: cy + size * 0.35,
                    },
                    D2D_POINT_2F {
                        x: cx + size * 0.45,
                        y: cy - size * 0.35,
                    },
                    stroke,
                    1.8,
                    None,
                );
            }
        }
    }

    pub fn draw_bitmap_contain(&self, bitmap: &ID2D1Bitmap, x: f32, y: f32, max_w: f32, max_h: f32) {
        if let Some(ref target) = self.render_target {
            unsafe {
                let size = bitmap.GetSize();
                if size.width <= 0.0 || size.height <= 0.0 {
                    return;
                }

                let scale_w = max_w / size.width;
                let scale_h = max_h / size.height;
                let scale = scale_w.min(scale_h).min(1.0); // Don't upscale tiny images beyond 100%

                let draw_w = size.width * scale;
                let draw_h = size.height * scale;
                let draw_x = x + (max_w - draw_w) * 0.5;
                let draw_y = y + (max_h - draw_h) * 0.5;

                let dest = D2D_RECT_F {
                    left: draw_x,
                    top: draw_y,
                    right: draw_x + draw_w,
                    bottom: draw_y + draw_h,
                };

                target.DrawBitmap(
                    bitmap,
                    Some(&dest),
                    1.0,
                    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                    None,
                );
            }
        }
    }

    pub fn draw_text(
        &self,
        text: &str,
        format: &IDWriteTextFormat,
        rect: &D2D_RECT_F,
        brush: &ID2D1SolidColorBrush,
    ) {
        if let Some(ref target) = self.render_target {
            let utf16: Vec<u16> = text.encode_utf16().collect();
            unsafe {
                target.DrawText(
                    &utf16,
                    format,
                    rect,
                    brush,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
        }
    }

    pub fn draw_highlighted_text(
        &self,
        text: &str,
        format: &IDWriteTextFormat,
        rect: &D2D_RECT_F,
        text_brush: &ID2D1SolidColorBrush,
        highlight_range: Option<(usize, usize)>,
        highlight_brush: &ID2D1SolidColorBrush,
    ) {
        if let Some(ref target) = self.render_target {
            let utf16: Vec<u16> = text.encode_utf16().collect();
            let w = rect.right - rect.left;
            let h = rect.bottom - rect.top;
            if w <= 0.0 || h <= 0.0 {
                return;
            }
            unsafe {
                if let Ok(layout) = self.dwrite_factory.CreateTextLayout(&utf16, format, w, h) {
                    if let Some((start, end)) = highlight_range {
                        if start < end && end <= utf16.len() {
                            let text_range = DWRITE_TEXT_RANGE {
                                startPosition: start as u32,
                                length: (end - start) as u32,
                            };
                            let _ = layout.SetFontWeight(DWRITE_FONT_WEIGHT_BOLD, text_range);
                            let mut metrics = [DWRITE_HIT_TEST_METRICS::default(); 8];
                            let mut actual_count = 0;
                            if layout
                                .HitTestTextRange(
                                    text_range.startPosition,
                                    text_range.length,
                                    rect.left,
                                    rect.top,
                                    Some(&mut metrics),
                                    &mut actual_count,
                                )
                                .is_ok()
                            {
                                for m in &metrics[..actual_count as usize] {
                                    let hl_rect = D2D1_ROUNDED_RECT {
                                        rect: D2D_RECT_F {
                                            left: m.left - 2.0,
                                            top: m.top + 1.0,
                                            right: m.left + m.width + 2.0,
                                            bottom: m.top + m.height - 1.0,
                                        },
                                        radiusX: 3.0,
                                        radiusY: 3.0,
                                    };
                                    target.FillRoundedRectangle(&hl_rect, highlight_brush);
                                }
                            }
                        }
                    }
                    target.DrawTextLayout(
                        D2D_POINT_2F {
                            x: rect.left,
                            y: rect.top,
                        },
                        &layout,
                        text_brush,
                        D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    );
                    return;
                }
                target.DrawText(
                    &utf16,
                    format,
                    rect,
                    text_brush,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
        }
    }

    pub fn draw_syntax_highlighted_text(
        &self,
        text: &str,
        format: &IDWriteTextFormat,
        rect: &D2D_RECT_F,
        default_brush: &ID2D1SolidColorBrush,
    ) {
        if let Some(ref target) = self.render_target {
            let text_slice = if text.len() > 4000 {
                let mut end = 4000;
                while !text.is_char_boundary(end) && end > 0 {
                    end -= 1;
                }
                &text[..end]
            } else {
                text
            };
            let utf16: Vec<u16> = text_slice.encode_utf16().collect();
            let w = rect.right - rect.left;
            let h = rect.bottom - rect.top;
            if w <= 0.0 || h <= 0.0 {
                return;
            }
            unsafe {
                if let Ok(layout) = self.dwrite_factory.CreateTextLayout(&utf16, format, w, h) {
                    if let Some(ref brushes) = self.brushes {
                        let tokens = crate::syntax::tokenize(text_slice);
                        for token in tokens {
                            if token.start_u16 + token.length_u16 <= utf16.len() as u32 {
                                let range = DWRITE_TEXT_RANGE {
                                    startPosition: token.start_u16,
                                    length: token.length_u16,
                                };
                                let brush = match token.kind {
                                    crate::syntax::TokenKind::Keyword => &brushes.syn_keyword,
                                    crate::syntax::TokenKind::String => &brushes.syn_string,
                                    crate::syntax::TokenKind::Number => &brushes.syn_number,
                                    crate::syntax::TokenKind::Comment => &brushes.syn_comment,
                                    crate::syntax::TokenKind::Type => &brushes.syn_type,
                                    crate::syntax::TokenKind::Function => &brushes.syn_function,
                                    crate::syntax::TokenKind::Property => &brushes.syn_property,
                                };
                                let _ = layout.SetDrawingEffect(brush, range);
                            }
                        }
                    }
                    target.DrawTextLayout(
                        D2D_POINT_2F {
                            x: rect.left,
                            y: rect.top,
                        },
                        &layout,
                        default_brush,
                        D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    );
                    return;
                }
                target.DrawText(
                    &utf16,
                    format,
                    rect,
                    default_brush,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    DWRITE_MEASURING_MODE_NATURAL,
                );
            }
        }
    }

    pub fn export_ui_to_png(
        &mut self,
        ui: &mut crate::ui::UiState,
        width: u32,
        height: u32,
        output_path: &Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use windows::Win32::Graphics::Gdi::*;
        struct Surface {
            dc: HDC,
            bitmap: HBITMAP,
            old: HGDIOBJ,
        }
        impl Drop for Surface {
            fn drop(&mut self) {
                unsafe {
                    if !self.old.is_invalid() {
                        SelectObject(self.dc, self.old);
                    }
                    if !self.bitmap.is_invalid() {
                        let _ = DeleteObject(self.bitmap);
                    }
                    let _ = DeleteDC(self.dc);
                }
            }
        }
        let byte_len = (width as usize).checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4)).filter(|&n| n > 0 && n <= 128 * 1024 * 1024)
            .ok_or("Invalid export dimensions")?;
        unsafe {
            let hdc = CreateCompatibleDC(None);
            if hdc.is_invalid() {
                return Err(windows::core::Error::from_win32().into());
            }
            let mut surface = Surface { dc: hdc, bitmap: HBITMAP::default(), old: HGDIOBJ::default() };
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width as i32,
                    biHeight: -(height as i32), // top-down
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };

            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let hbitmap = CreateDIBSection(
                hdc,
                &bmi,
                DIB_RGB_COLORS,
                &mut bits,
                None,
                0,
            )?;
            surface.bitmap = hbitmap;
            surface.old = SelectObject(hdc, hbitmap);
            if bits.is_null() || surface.old.is_invalid() {
                return Err("Could not select export bitmap".into());
            }

            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };

            let dc_target = self.factory.CreateDCRenderTarget(&props)?;
            let target: ID2D1RenderTarget = dc_target.cast()?;
            target.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE);

            let sub_rect = windows::Win32::Foundation::RECT {
                left: 0,
                top: 0,
                right: width as i32,
                bottom: height as i32,
            };
            dc_target.BindDC(hdc, &sub_rect)?;

            let brushes = Brushes::new(&target)?;
            // Export resources belong to a separate render target and cache.
            let mut export = Self::new()?;
            export.factory = self.factory.clone();
            export.render_target = Some(target.clone());
            export.brushes = Some(brushes);

            target.BeginDraw();
            ui.render(&mut export);
            target.EndDraw(None, None)?;

            let total_pixels = byte_len / 4;
            let slice = std::slice::from_raw_parts(bits as *const u8, total_pixels * 4);
            let mut rgba_buf = vec![0u8; total_pixels * 4];
            for i in 0..total_pixels {
                let b = slice[i * 4];
                let g = slice[i * 4 + 1];
                let r = slice[i * 4 + 2];
                let a = slice[i * 4 + 3];
                rgba_buf[i * 4] = r;
                rgba_buf[i * 4 + 1] = g;
                rgba_buf[i * 4 + 2] = b;
                rgba_buf[i * 4 + 3] = a;
            }

            image::save_buffer(
                output_path,
                &rgba_buf,
                width,
                height,
                image::ExtendedColorType::Rgba8,
            )?;

            Ok(())
        }
    }
}

fn premultiply_alpha(bytes: &mut [u8]) {
    for pixel in bytes.as_chunks_mut::<4>().0 {
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmap_alpha_is_premultiplied() {
        let mut rgba = [255, 128, 0, 128, 255, 255, 255, 0];
        premultiply_alpha(&mut rgba);
        assert_eq!(rgba, [128, 64, 0, 128, 0, 0, 0, 0]);
    }

    #[test]
    fn failed_export_does_not_replace_render_resources() {
        let mut d2d = D2dContext::new().unwrap();
        let mut ui = crate::ui::UiState::new(std::path::PathBuf::new(), crate::model::AppSettings::default());
        assert!(d2d.export_ui_to_png(&mut ui, 0, 360, Path::new("unused.png")).is_err());
        assert!(d2d.render_target.is_none());
        assert!(d2d.brushes.is_none());
    }
}
