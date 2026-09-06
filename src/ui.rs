use std::path::PathBuf;
use std::time::Instant;
use windows::Win32::Graphics::Direct2D::Common::*;

use crate::d2d::D2dContext;
use crate::db::ClipItem;
use crate::model::{AppSettings, HitTarget, UiMode, WINDOW_HEIGHT, WINDOW_WIDTH};

pub struct UiState {
    pub mode: UiMode,
    pub selected_index: usize,
    pub search_query: String,
    pub show_favorites: bool,
    pub hover_target: HitTarget,
    pub scroll_offset: f32,
    pub cursor_visible: bool,
    pub last_cursor_blink: Instant,
    pub clips: Vec<ClipItem>,
    pub settings: AppSettings,
    pub settings_message: String,
    pub data_dir: PathBuf,
}

impl UiState {
    pub fn new(data_dir: PathBuf, settings: AppSettings) -> Self {
        Self {
            mode: UiMode::SingleCard,
            selected_index: 0,
            search_query: String::new(),
            show_favorites: false,
            hover_target: HitTarget::None,
            scroll_offset: 0.0,
            cursor_visible: true,
            last_cursor_blink: Instant::now(),
            clips: Vec::new(),
            settings,
            settings_message: String::new(),
            data_dir,
        }
    }

    pub fn set_clips(&mut self, clips: Vec<ClipItem>, select_idx: Option<usize>) {
        self.clips = clips;
        if let Some(idx) = select_idx {
            self.selected_index = if self.clips.is_empty() {
                0
            } else {
                idx.min(self.clips.len() - 1)
            };
        } else if self.selected_index >= self.clips.len() && !self.clips.is_empty() {
            self.selected_index = self.clips.len() - 1;
        }
        self.ensure_selected_visible();
    }

    pub fn selected_clip(&self) -> Option<&ClipItem> {
        self.clips.get(self.selected_index)
    }

    pub fn ensure_selected_visible(&mut self) {
        if self.mode == UiMode::SearchList {
            let row_h = 56.0;
            let list_h = 250.0;
            let target_y = (self.selected_index as f32) * row_h;
            if target_y < self.scroll_offset {
                self.scroll_offset = target_y;
            } else if target_y + row_h > self.scroll_offset + list_h {
                self.scroll_offset = target_y + row_h - list_h;
            }
        }
    }

    pub fn hit_test(&self, x: f32, y: f32) -> HitTarget {
        // Window bounds
        if x < 0.0 || x > WINDOW_WIDTH || y < 0.0 || y > WINDOW_HEIGHT {
            return HitTarget::None;
        }

        // Header controls (y: 8..40)
        if y >= 8.0 && y <= 40.0 {
            // Close button at top right in Settings mode
            if self.mode == UiMode::Settings {
                if x >= 438.0 && x <= 470.0 {
                    return HitTarget::CloseSettings;
                }
            } else {
                let is_search_mode = self.mode == UiMode::SearchList;
                let pill_w = 36.0;
                let star_slot_w = 26.0;
                let gap = 10.0;
                let search_w = 200.0;
                let total_w = if is_search_mode {
                    pill_w + gap + star_slot_w + gap + search_w
                } else {
                    pill_w + gap + star_slot_w
                };
                let start_x = (WINDOW_WIDTH - total_w) * 0.5;

                let star_x = start_x + pill_w + gap;

                // Favorite filter toggle
                if x >= star_x - 2.0 && x <= star_x + star_slot_w + 2.0 {
                    return HitTarget::FavFilter;
                }
                // Search box (only active in search mode)
                if is_search_mode {
                    let search_x = star_x + star_slot_w + gap;
                    if x >= search_x && x <= search_x + search_w {
                        return HitTarget::SearchBox;
                    }
                }
            }
        }

        match self.mode {
            UiMode::Settings => {
                // Launch on boot toggle: x: 30..450, y: 80..110
                if x >= 30.0 && x <= 450.0 && y >= 80.0 && y <= 112.0 {
                    return HitTarget::SettingsBootToggle;
                }
                // Clear non-favorites: x: 30..220, y: 170..202
                if x >= 30.0 && x <= 220.0 && y >= 170.0 && y <= 202.0 {
                    return HitTarget::ClearNonFavorites;
                }
                // Clear all: x: 235..425, y: 170..202
                if x >= 235.0 && x <= 425.0 && y >= 170.0 && y <= 202.0 {
                    return HitTarget::ClearAll;
                }
            }
            UiMode::SearchList => {
                // List rows viewport: y: 48..308
                if y >= 48.0 && y <= 308.0 && x >= 14.0 && x <= 466.0 {
                    let relative_y = y - 48.0 + self.scroll_offset;
                    let row_idx = (relative_y / 56.0).floor() as isize;
                    let row_local_y = relative_y - (row_idx as f32) * 56.0;
                    if row_idx >= 0 && (row_idx as usize) < self.clips.len() && row_local_y <= 52.0 {
                        return HitTarget::ListItem(row_idx as usize);
                    }
                }
            }
            UiMode::SingleCard => {
                // Bottom dock: centered at bottom: x: 201..279, y: 318..348
                if x >= 201.0 && x <= 279.0 && y >= 318.0 && y <= 348.0 && !self.clips.is_empty() {
                    if x <= 239.0 {
                        return HitTarget::DockTrash;
                    } else if x >= 241.0 {
                        return HitTarget::DockStar;
                    }
                }
            }
        }

        HitTarget::None
    }

    pub fn render(&mut self, d2d: &mut D2dContext) {
        if d2d.brushes.is_none() || d2d.formats.is_none() {
            return;
        }
        let brushes = d2d.brushes.as_ref().unwrap();

        // Update caret blink (500ms cycle)
        if self.last_cursor_blink.elapsed().as_millis() >= 500 {
            self.cursor_visible = !self.cursor_visible;
            self.last_cursor_blink = Instant::now();
        }

        // 1. Root background & subtle rounded border
        d2d.draw_rounded_rect(
            0.5,
            0.5,
            WINDOW_WIDTH - 1.0,
            WINDOW_HEIGHT - 1.0,
            12.0,
            &brushes.bg,
            Some((&brushes.border, 1.0)),
        );

        // 2. Header Bar
        self.render_header(d2d);

        // 3. Main Content based on mode
        match self.mode {
            UiMode::Settings => self.render_settings(d2d),
            UiMode::SearchList => self.render_search_list(d2d),
            UiMode::SingleCard => self.render_single_card(d2d),
        }
    }

    fn render_header(&self, d2d: &D2dContext) {
        let brushes = d2d.brushes.as_ref().unwrap();
        let formats = d2d.formats.as_ref().unwrap();

        if self.mode == UiMode::Settings {
            // Settings Title
            let title_rect = D2D_RECT_F {
                left: 18.0,
                top: 12.0,
                right: 200.0,
                bottom: 38.0,
            };
            d2d.draw_text("Settings", &formats.title, &title_rect, &brushes.text_primary);

            // Close button at top right
            let is_hover = self.hover_target == HitTarget::CloseSettings;
            if is_hover {
                d2d.draw_rounded_rect(440.0, 11.0, 26.0, 26.0, 6.0, &brushes.card_hover, None);
            }
            d2d.draw_close_icon(
                453.0,
                24.0,
                11.0,
                if is_hover {
                    &brushes.text_primary
                } else {
                    &brushes.text_secondary
                },
            );
            return;
        }

        // --- Normal / Search Header ---
        let is_search_mode = self.mode == UiMode::SearchList;
        let pill_w = 36.0;
        let pill_h = 24.0;
        let star_slot_w = 26.0;
        let gap = 10.0;
        let search_w = 200.0;
        let total_w = if is_search_mode {
            pill_w + gap + star_slot_w + gap + search_w
        } else {
            pill_w + gap + star_slot_w
        };
        let start_x = (WINDOW_WIDTH - total_w) * 0.5;

        // 1. Clip counter / index pill
        let pill_text = if self.clips.is_empty() {
            "0".to_string()
        } else {
            format!("{}", self.selected_index + 1)
        };
        d2d.draw_rounded_rect(
            start_x,
            12.0,
            pill_w,
            pill_h,
            12.0,
            &brushes.pill_bg,
            Some((&brushes.border, 1.0)),
        );
        let pill_rect = D2D_RECT_F {
            left: start_x,
            top: 12.0,
            right: start_x + pill_w,
            bottom: 12.0 + pill_h,
        };
        d2d.draw_text(&pill_text, &formats.counter, &pill_rect, &brushes.text_secondary);

        // 2. Favorite Filter Button
        let star_x = start_x + pill_w + gap;
        let star_cx = star_x + star_slot_w * 0.5;
        let is_fav_hover = self.hover_target == HitTarget::FavFilter;
        if is_fav_hover {
            d2d.draw_rounded_rect(star_x, 11.0, star_slot_w, 26.0, 6.0, &brushes.card_hover, None);
        }
        d2d.draw_star(
            star_cx,
            24.0,
            7.0,
            if self.show_favorites {
                Some(&brushes.favorite)
            } else {
                None
            },
            if self.show_favorites {
                None
            } else {
                Some((
                    if is_fav_hover {
                        &brushes.text_secondary
                    } else {
                        &brushes.text_muted
                    },
                    1.4,
                ))
            },
        );

        // 3. Search Box - Only visible when typing (search mode active)
        if is_search_mode {
            let search_x = star_x + star_slot_w + gap;
            let search_h = 24.0;
            d2d.draw_rounded_rect(
                search_x,
                12.0,
                search_w,
                search_h,
                6.0,
                &brushes.search_bg,
                Some((&brushes.accent, 1.0)),
            );

            d2d.draw_search_icon(
                search_x + 12.0,
                24.0,
                5.0,
                &brushes.accent,
            );

            let search_text_rect = D2D_RECT_F {
                left: search_x + 24.0,
                top: 12.0,
                right: search_x + search_w - 8.0,
                bottom: 12.0 + search_h,
            };

            if self.search_query.is_empty() {
                d2d.draw_text("Type to search...", &formats.search, &search_text_rect, &brushes.text_muted);
            } else {
                let display_text = if self.cursor_visible {
                    format!("{}|", self.search_query)
                } else {
                    self.search_query.clone()
                };
                d2d.draw_text(&display_text, &formats.search, &search_text_rect, &brushes.text_primary);
            }
        }
    }

    fn render_single_card(&mut self, d2d: &mut D2dContext) {
        let brushes = d2d.brushes.as_ref().unwrap();
        let formats = d2d.formats.as_ref().unwrap();

        let card_x = 14.0;
        let card_y = 48.0;
        let card_w = WINDOW_WIDTH - 28.0;
        let card_h = 254.0;

        let is_fav = self
            .clips
            .get(self.selected_index)
            .map(|c| c.is_favorite)
            .unwrap_or(false);

        // Card container background with yellow border if favorite, else subtle border
        let card_border = if is_fav {
            (&brushes.favorite, 1.5)
        } else {
            (&brushes.border, 1.0)
        };

        d2d.draw_rounded_rect(
            card_x,
            card_y,
            card_w,
            card_h,
            10.0,
            &brushes.card,
            Some(card_border),
        );

        if self.clips.is_empty() {
            let empty_rect = D2D_RECT_F {
                left: card_x,
                top: card_y + 100.0,
                right: card_x + card_w,
                bottom: card_y + 140.0,
            };
            d2d.draw_text(
                "Clipboard history is empty\nCopy text or images to get started",
                &formats.dock,
                &empty_rect,
                &brushes.text_muted,
            );
            return;
        }

        let item = &self.clips[self.selected_index];

        // Image Clip View
        if item.clip_type == "image" {
            let mut img_drawn = false;
            if let Some(ref rel_or_abs) = item.image_path {
                let p = std::path::Path::new(rel_or_abs);
                let full = if p.is_absolute() {
                    p.to_path_buf()
                } else {
                    self.data_dir.join(p)
                };
                if let Some(full_str) = full.to_str() {
                    if let Some(bmp) = d2d.get_or_load_bitmap(full_str) {
                        d2d.draw_bitmap_contain(&bmp, card_x + 14.0, card_y + 14.0, card_w - 28.0, card_h - 48.0);
                        img_drawn = true;
                    }
                }
            }

            if !img_drawn {
                let placeholder_rect = D2D_RECT_F {
                    left: card_x,
                    top: card_y + 80.0,
                    right: card_x + card_w,
                    bottom: card_y + 120.0,
                };
                d2d.draw_text("[Image Preview]", &formats.dock, &placeholder_rect, &brushes.text_muted);
            }

            // Image dimensions pill badge
            let dim_text = match (item.image_width, item.image_height) {
                (Some(w), Some(h)) => format!("IMAGE • {} × {}", w, h),
                _ => "IMAGE".to_string(),
            };
            let badge_rect = D2D_RECT_F {
                left: card_x + 14.0,
                top: card_y + card_h - 22.0,
                right: card_x + 220.0,
                bottom: card_y + card_h - 6.0,
            };
            d2d.draw_text(&dim_text, &formats.small, &badge_rect, &brushes.text_muted);
        } else {
            // Text / Code Clip View
            d2d.push_clip(card_x + 14.0, card_y + 14.0, card_w - 28.0, card_h - 38.0);
            let text_rect = D2D_RECT_F {
                left: card_x + 14.0,
                top: card_y + 14.0,
                right: card_x + card_w - 14.0,
                bottom: card_y + card_h - 26.0,
            };
            d2d.draw_text(&item.text, &formats.card_text, &text_rect, &brushes.text_primary);
            d2d.pop_clip();

            // Metadata footer (character count & type)
            let len = item.text.chars().count();
            let lines = item.text.lines().count();
            let meta_text = format!("{} chars • {} line{}", len, lines, if lines == 1 { "" } else { "s" });
            let meta_rect = D2D_RECT_F {
                left: card_x + 14.0,
                top: card_y + card_h - 22.0,
                right: card_x + 220.0,
                bottom: card_y + card_h - 6.0,
            };
            d2d.draw_text(&meta_text, &formats.small, &meta_rect, &brushes.text_muted);
        }

        // Bottom-Center Action Dock: pill width 78, height 28, centered at bottom
        self.render_action_dock(d2d, is_fav);
    }

    fn render_action_dock(&self, d2d: &D2dContext, is_fav: bool) {
        let brushes = d2d.brushes.as_ref().unwrap();

        let dock_w = 78.0;
        let dock_h = 28.0;
        let dock_x = (WINDOW_WIDTH - dock_w) * 0.5;
        let dock_y = 318.0;

        // Dock background
        d2d.draw_rounded_rect(
            dock_x,
            dock_y,
            dock_w,
            dock_h,
            14.0,
            &brushes.dock_bg,
            Some((&brushes.dock_border, 1.0)),
        );

        // 1. Trash Button (left half)
        let is_trash_hover = self.hover_target == HitTarget::DockTrash;
        if is_trash_hover {
            d2d.draw_rounded_rect(dock_x + 6.0, dock_y + 2.0, 24.0, 24.0, 6.0, &brushes.trash_bg, None);
        }
        d2d.draw_trash_icon(
            dock_x + 18.0,
            dock_y + 14.0,
            12.0,
            if is_trash_hover {
                &brushes.trash
            } else {
                &brushes.text_muted
            },
        );

        // Divider
        d2d.draw_rect(dock_x + 38.5, dock_y + 6.0, 1.0, 16.0, &brushes.border);

        // 2. Favorite Button (right half)
        let is_star_hover = self.hover_target == HitTarget::DockStar;
        if is_star_hover {
            d2d.draw_rounded_rect(dock_x + 48.0, dock_y + 2.0, 24.0, 24.0, 6.0, &brushes.favorite_bg, None);
        }
        d2d.draw_star(
            dock_x + 60.0,
            dock_y + 14.0,
            6.5,
            if is_fav {
                Some(&brushes.favorite)
            } else {
                None
            },
            if is_fav {
                None
            } else {
                Some((
                    if is_star_hover {
                        &brushes.favorite
                    } else {
                        &brushes.text_muted
                    },
                    1.3,
                ))
            },
        );
    }
}

struct MatchSnippet {
    pub primary: String,
    pub primary_highlight: Option<(usize, usize)>,
    pub secondary: String,
}

fn get_match_snippet(item: &crate::db::ClipItem, query: &str) -> MatchSnippet {
    if item.clip_type == "image" {
        let dim = match (item.image_width, item.image_height) {
            (Some(w), Some(h)) => format!("📷 Image • {} × {}", w, h),
            _ => "📷 Image".to_string(),
        };
        let ocr_info = if let Some(ref ocr) = item.ocr_text {
            let ocr_trimmed = ocr.trim();
            if !ocr_trimmed.is_empty() {
                format!("OCR: {}", ocr_trimmed.lines().next().unwrap_or(""))
            } else {
                "Bitmap image".to_string()
            }
        } else {
            "Bitmap image".to_string()
        };
        return MatchSnippet {
            primary: dim,
            primary_highlight: None,
            secondary: ocr_info,
        };
    }

    let trimmed_query = query.trim();
    let lines: Vec<&str> = item.text.lines().collect();

    if trimmed_query.is_empty() {
        let non_empty: Vec<(usize, &str)> = lines
            .iter()
            .enumerate()
            .filter_map(|(i, l)| {
                let t = l.trim();
                if !t.is_empty() {
                    Some((i, t))
                } else {
                    None
                }
            })
            .collect();

        if non_empty.is_empty() {
            return MatchSnippet {
                primary: "(Empty clip)".to_string(),
                primary_highlight: None,
                secondary: "0 chars".to_string(),
            };
        }

        let first_text = non_empty[0].1;
        let primary = if (first_text == "{" || first_text == "[") && non_empty.len() > 1 {
            format!("{} {}", first_text, non_empty[1].1)
        } else {
            first_text.to_string()
        };

        let secondary = if lines.len() > 1 {
            format!("{} chars • {} lines", item.text.chars().count(), lines.len())
        } else {
            format!("{} chars", item.text.chars().count())
        };

        return MatchSnippet {
            primary,
            primary_highlight: None,
            secondary,
        };
    }

    let query_lower = trimmed_query.to_lowercase();
    let query_utf16_len = trimmed_query.encode_utf16().count();

    let mut match_line_idx = None;
    for (i, line) in lines.iter().enumerate() {
        if line.to_lowercase().contains(&query_lower) {
            match_line_idx = Some(i);
            break;
        }
    }

    let line_idx = match_line_idx.unwrap_or(0);
    let matched_raw_line = lines.get(line_idx).map(|l| l.trim()).unwrap_or("");

    let line_lower = matched_raw_line.to_lowercase();
    let (display_text, highlight_range) = if let Some(byte_pos) = line_lower.find(&query_lower) {
        let char_pos = matched_raw_line[..byte_pos].chars().count();
        let total_chars = matched_raw_line.chars().count();

        if total_chars > 65 {
            let window_start = char_pos.saturating_sub(18);
            let window_end = (char_pos + trimmed_query.chars().count() + 35).min(total_chars);
            let prefix = if window_start > 0 { "…" } else { "" };
            let suffix = if window_end < total_chars { "…" } else { "" };
            let sub: String = matched_raw_line
                .chars()
                .skip(window_start)
                .take(window_end - window_start)
                .collect();
            let final_str = format!("{}{}{}", prefix, sub, suffix);

            let prefix_utf16 = prefix.encode_utf16().count();
            let sub_before_match: String = matched_raw_line
                .chars()
                .skip(window_start)
                .take(char_pos - window_start)
                .collect();
            let start_u16 = prefix_utf16 + sub_before_match.encode_utf16().count();
            let end_u16 = start_u16 + query_utf16_len;

            (final_str, Some((start_u16, end_u16)))
        } else {
            let start_u16 = matched_raw_line[..byte_pos].encode_utf16().count();
            let end_u16 = start_u16 + query_utf16_len;
            (matched_raw_line.to_string(), Some((start_u16, end_u16)))
        }
    } else {
        (matched_raw_line.to_string(), None)
    };

    let secondary = if lines.len() > 1 {
        if line_idx > 1 {
            let first_line = lines[0].trim();
            let first_clean = if (first_line == "{" || first_line == "[") && lines.len() > 1 {
                format!("{} {}", first_line, lines[1].trim())
            } else {
                first_line.to_string()
            };
            format!("Line {} • {}", line_idx + 1, first_clean)
        } else {
            format!("{} lines • {} chars", lines.len(), item.text.chars().count())
        }
    } else {
        format!("{} chars", item.text.chars().count())
    };

    MatchSnippet {
        primary: display_text,
        primary_highlight: highlight_range,
        secondary,
    }
}

impl UiState {
    fn render_search_list(&self, d2d: &D2dContext) {
        let brushes = d2d.brushes.as_ref().unwrap();
        let formats = d2d.formats.as_ref().unwrap();

        let list_x = 14.0;
        let list_y = 48.0;
        let list_w = WINDOW_WIDTH - 28.0;
        let list_h = 260.0;
        let row_h = 52.0;
        let row_gap = 4.0;
        let step = row_h + row_gap;

        if self.clips.is_empty() {
            let empty_rect = D2D_RECT_F {
                left: list_x,
                top: list_y + 100.0,
                right: list_x + list_w,
                bottom: list_y + 140.0,
            };
            d2d.draw_text(
                if self.show_favorites {
                    "No favorites saved yet\nClick the star on any clip to favorite"
                } else {
                    "No matching clips found"
                },
                &formats.dock,
                &empty_rect,
                &brushes.text_muted,
            );
            return;
        }

        // Virtualized list range calculation
        let start_idx = ((self.scroll_offset / step).floor() as usize).min(self.clips.len());
        let end_idx = (((self.scroll_offset + list_h) / step).ceil() as usize).min(self.clips.len());

        d2d.push_clip(list_x, list_y, list_w, list_h);

        for idx in start_idx..end_idx {
            let item = &self.clips[idx];
            let row_y = list_y + (idx as f32) * step - self.scroll_offset;
            let is_selected = idx == self.selected_index;
            let is_hover = self.hover_target == HitTarget::ListItem(idx);

            // Row background
            let row_bg = if is_selected {
                if item.is_favorite {
                    &brushes.favorite_selected_bg
                } else {
                    &brushes.card_selected
                }
            } else if is_hover {
                &brushes.card_hover
            } else {
                &brushes.border_subtle
            };

            let row_border = if item.is_favorite {
                Some((&brushes.favorite, 1.0))
            } else {
                None
            };

            d2d.draw_rounded_rect(list_x, row_y, list_w - 8.0, row_h, 8.0, row_bg, row_border);

            // Left accent bar on active row
            if is_selected {
                d2d.draw_rounded_rect(
                    list_x,
                    row_y + 6.0,
                    3.5,
                    row_h - 12.0,
                    1.75,
                    if item.is_favorite {
                        &brushes.favorite
                    } else {
                        &brushes.accent
                    },
                    None,
                );
            }

            // Extract match snippet: exact matching line and context
            let snippet = get_match_snippet(item, &self.search_query);

            let text_left = list_x + 16.0;
            let text_right = list_x + list_w - 38.0;

            // Line 1: Exact matching line (or first meaningful line)
            let primary_rect = D2D_RECT_F {
                left: text_left,
                top: row_y + 7.0,
                right: text_right,
                bottom: row_y + 27.0,
            };

            let primary_brush = if is_selected {
                &brushes.text_primary
            } else {
                &brushes.text_secondary
            };

            d2d.draw_highlighted_text(
                &snippet.primary,
                &formats.row_text,
                &primary_rect,
                primary_brush,
                snippet.primary_highlight,
                &brushes.favorite_bg,
            );

            // Line 2: Context / metadata line
            let secondary_rect = D2D_RECT_F {
                left: text_left,
                top: row_y + 28.0,
                right: text_right,
                bottom: row_y + 46.0,
            };

            d2d.draw_text(
                &snippet.secondary,
                &formats.meta,
                &secondary_rect,
                &brushes.text_muted,
            );

            // Right star indicator if favorite
            if item.is_favorite {
                d2d.draw_star(
                    list_x + list_w - 22.0,
                    row_y + row_h * 0.5,
                    5.5,
                    Some(&brushes.favorite),
                    None,
                );
            }
        }

        d2d.pop_clip();

        // Scrollbar thumb if content exceeds viewport
        let total_content_h = (self.clips.len() as f32) * step;
        if total_content_h > list_h {
            let thumb_h = (list_h / total_content_h * list_h).max(24.0);
            let thumb_y = list_y + (self.scroll_offset / (total_content_h - list_h)) * (list_h - thumb_h);
            d2d.draw_rounded_rect(
                list_x + list_w - 4.0,
                thumb_y,
                3.0,
                thumb_h,
                1.5,
                &brushes.scroll_thumb,
                None,
            );
        }

        // Bottom hints
        let hint_rect = D2D_RECT_F {
            left: list_x,
            top: WINDOW_HEIGHT - 32.0,
            right: WINDOW_WIDTH - 14.0,
            bottom: WINDOW_HEIGHT - 8.0,
        };
        d2d.draw_text(
            "↵ Paste   •   Tab Starred   •   Del Remove   •   Esc Back",
            &formats.small,
            &hint_rect,
            &brushes.text_muted,
        );
    }

    fn render_settings(&self, d2d: &D2dContext) {
        let brushes = d2d.brushes.as_ref().unwrap();
        let formats = d2d.formats.as_ref().unwrap();

        let panel_x = 24.0;
        let panel_w = WINDOW_WIDTH - 48.0;

        // Section 1: Global Shortcut info
        let sc_label_rect = D2D_RECT_F {
            left: panel_x,
            top: 54.0,
            right: panel_x + 200.0,
            bottom: 74.0,
        };
        d2d.draw_text("Global Shortcut:", &formats.label, &sc_label_rect, &brushes.text_primary);

        let sc_box_rect = D2D_RECT_F {
            left: panel_x + 130.0,
            top: 50.0,
            right: panel_x + panel_w,
            bottom: 78.0,
        };
        d2d.draw_rounded_rect(
            panel_x + 130.0,
            50.0,
            panel_w - 130.0,
            28.0,
            6.0,
            &brushes.search_bg,
            Some((&brushes.border, 1.0)),
        );
        d2d.draw_text(
            &self.settings.shortcut,
            &formats.counter,
            &sc_box_rect,
            &brushes.accent,
        );

        // Section 2: Launch on boot toggle
        let boot_y = 92.0;
        let is_boot_hover = self.hover_target == HitTarget::SettingsBootToggle;
        d2d.draw_rounded_rect(
            panel_x,
            boot_y,
            panel_w,
            36.0,
            6.0,
            if is_boot_hover {
                &brushes.card_hover
            } else {
                &brushes.card
            },
            Some((&brushes.border, 1.0)),
        );

        // Checkbox box
        d2d.draw_rounded_rect(
            panel_x + 12.0,
            boot_y + 9.0,
            18.0,
            18.0,
            4.0,
            if self.settings.launch_on_boot {
                &brushes.accent
            } else {
                &brushes.search_bg
            },
            Some((&brushes.border, 1.0)),
        );
        if self.settings.launch_on_boot {
            d2d.draw_check_icon(panel_x + 21.0, boot_y + 18.0, 10.0, &brushes.text_primary);
        }

        let boot_label_rect = D2D_RECT_F {
            left: panel_x + 40.0,
            top: boot_y,
            right: panel_x + panel_w,
            bottom: boot_y + 36.0,
        };
        d2d.draw_text(
            "Launch automatically on system boot",
            &formats.row_text,
            &boot_label_rect,
            &brushes.text_primary,
        );

        // Section 3: History management actions
        let act_y = 150.0;
        let btn_w = (panel_w - 12.0) * 0.5;

        // Button A: Clear Non-Favorites
        let is_clear_nf_hover = self.hover_target == HitTarget::ClearNonFavorites;
        d2d.draw_rounded_rect(
            panel_x,
            act_y,
            btn_w,
            34.0,
            6.0,
            if is_clear_nf_hover {
                &brushes.card_hover
            } else {
                &brushes.card
            },
            Some((&brushes.border, 1.0)),
        );
        let btn_a_rect = D2D_RECT_F {
            left: panel_x,
            top: act_y,
            right: panel_x + btn_w,
            bottom: act_y + 34.0,
        };
        d2d.draw_text(
            "Clear Non-Favorites",
            &formats.counter,
            &btn_a_rect,
            &brushes.text_secondary,
        );

        // Button B: Clear All
        let is_clear_all_hover = self.hover_target == HitTarget::ClearAll;
        d2d.draw_rounded_rect(
            panel_x + btn_w + 12.0,
            act_y,
            btn_w,
            34.0,
            6.0,
            if is_clear_all_hover {
                &brushes.trash_bg
            } else {
                &brushes.card
            },
            Some((
                if is_clear_all_hover {
                    &brushes.trash
                } else {
                    &brushes.border
                },
                1.0,
            )),
        );
        let btn_b_rect = D2D_RECT_F {
            left: panel_x + btn_w + 12.0,
            top: act_y,
            right: panel_x + panel_w,
            bottom: act_y + 34.0,
        };
        d2d.draw_text(
            "Clear Everything",
            &formats.counter,
            &btn_b_rect,
            if is_clear_all_hover {
                &brushes.trash
            } else {
                &brushes.text_muted
            },
        );

        // Section 4: System info & specs
        let info_y = 210.0;
        let info_rect = D2D_RECT_F {
            left: panel_x,
            top: info_y,
            right: panel_x + panel_w,
            bottom: info_y + 80.0,
        };
        d2d.draw_text(
            "Clipped v0.2.0 • Pure Rust Native Architecture\nDirect2D Hardware-Accelerated Rendering\nIdle Memory Target: < 4 MB (Working Set Purge Active)",
            &formats.meta,
            &info_rect,
            &brushes.text_muted,
        );

        if !self.settings_message.is_empty() {
            let msg_rect = D2D_RECT_F {
                left: panel_x,
                top: WINDOW_HEIGHT - 36.0,
                right: panel_x + panel_w,
                bottom: WINDOW_HEIGHT - 12.0,
            };
            d2d.draw_text(
                &self.settings_message,
                &formats.counter,
                &msg_rect,
                &brushes.accent,
            );
        }
    }
}
