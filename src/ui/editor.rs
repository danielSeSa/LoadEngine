use std::collections::HashSet;

use egui::{
    pos2,
    text::{CCursor, CCursorRange, LayoutJob, TextFormat},
    text_edit::TextEditState,
    vec2, Align, Align2, Color32, Event, FontId, Id, Key, Modifiers, Rect, RichText, Sense,
};

use crate::editor::{
    console::Level,
    document::Document,
    edit,
    lexer::{self, Tok},
    EditorState,
};
use crate::state::AppState;

// =====================================================================
//  Entry point
// =====================================================================

pub fn code_editor(ctx: &egui::Context, app: &mut AppState) {
    let ed = &mut app.editor;

    // میانبرهای سراسری (قبل از TextEdit مصرف می‌شوند)
    let (save, run, find, new, close) = ctx.input_mut(|i| {
        (
            i.consume_key(Modifiers::COMMAND, Key::S),
            i.consume_key(Modifiers::COMMAND, Key::Enter) | i.consume_key(Modifiers::NONE, Key::F5),
            i.consume_key(Modifiers::COMMAND, Key::F),
            i.consume_key(Modifiers::COMMAND, Key::N),
            i.consume_key(Modifiers::COMMAND, Key::W),
        )
    });
    if save {
        ed.save_active();
    }
    if run {
        ed.run_active();
    }
    if find {
        ed.find.open = true;
        ed.find.focus_query = true;
    }
    if new {
        ed.new_file();
    }
    if close && !ed.docs.is_empty() {
        let i = ed.active;
        ed.close_tab(i);
    }

    // ترتیب مهم است: پنل‌ها از بیرون به داخل
    status_bar(ctx, ed);
    if ed.show_console {
        console_panel(ctx, ed);
    }
    if ed.show_explorer {
        explorer_panel(ctx, ed);
    }
    egui::CentralPanel::default().show(ctx, |ui| central(ui, ed));
}

// =====================================================================
//  Panels
// =====================================================================

fn status_bar(ctx: &egui::Context, ed: &mut EditorState) {
    egui::TopBottomPanel::bottom("editor_status")
        .exact_height(24.0)
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                if let Some(doc) = ed.docs.get(ed.active) {
                    let (l, c) = edit::line_col(&doc.text, doc.sel.1);
                    ui.label(format!("Ln {}, Col {}", l + 1, c + 1));
                    let sel_len = doc.sel.0.abs_diff(doc.sel.1);
                    if sel_len > 0 {
                        ui.label(format!("({sel_len} selected)"));
                    }
                    ui.separator();
                    ui.label(format!("{} lines", doc.text.matches('\n').count() + 1));
                    ui.separator();
                    ui.label("UTF-8");
                    ui.label(format!("Spaces: {}", edit::TAB_WIDTH));
                    ui.separator();
                    let n = doc.diagnostics.len();
                    if n == 0 {
                        ui.colored_label(Color32::from_rgb(152, 195, 121), "✔ No problems");
                    } else {
                        ui.colored_label(Color32::from_rgb(224, 108, 117), format!("⛔ {n} problem(s)"));
                    }
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        ui.weak(doc.path.display().to_string());
                    });
                }
            });
        });
}

fn console_panel(ctx: &egui::Context, ed: &mut EditorState) {
    let mut goto = None;
    egui::TopBottomPanel::bottom("code_console")
        .resizable(true)
        .default_height(160.0)
        .min_height(80.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("📟 Console");
                ui.separator();
                ui.checkbox(&mut ed.console.show_info, "Info");
                ui.checkbox(&mut ed.console.show_warn, "Warnings");
                ui.checkbox(&mut ed.console.show_error, "Errors");
                ui.checkbox(&mut ed.console.auto_scroll, "Auto-scroll");
                if ui.button("🗑 Clear").clicked() {
                    ed.console.clear();
                }
            });
            ui.separator();

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .stick_to_bottom(ed.console.auto_scroll)
                .show(ui, |ui| {
                    for e in &ed.console.entries {
                        if !ed.console.is_visible(e.level) {
                            continue;
                        }
                        let (color, tag) = match e.level {
                            Level::Info => (Color32::from_rgb(171, 178, 191), "[Info]"),
                            Level::Build => (Color32::from_rgb(97, 175, 239), "[Build]"),
                            Level::Success => (Color32::from_rgb(152, 195, 121), "[OK]"),
                            Level::Warn => (Color32::from_rgb(229, 192, 123), "[Warn]"),
                            Level::Error => (Color32::from_rgb(224, 108, 117), "[Error]"),
                        };
                        let text = RichText::new(format!("{tag} {}", e.text)).monospace().color(color);
                        if e.goto.is_some() {
                            let r = ui
                                .add(egui::Label::new(text).sense(Sense::click()))
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            if r.clicked() {
                                goto = e.goto;
                            }
                        } else {
                            ui.label(text);
                        }
                    }
                });
        });
    if let Some((l, c)) = goto {
        ed.goto(l, c);
    }
}

fn explorer_panel(ctx: &egui::Context, ed: &mut EditorState) {
    egui::SidePanel::left("editor_explorer")
        .resizable(true)
        .default_width(190.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("📁 SCRIPTS");
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if ui.small_button("⟳").on_hover_text("Refresh").clicked() {
                        ed.refresh_files();
                    }
                    if ui.small_button("＋").on_hover_text("New script (Ctrl+N)").clicked() {
                        ed.new_file();
                    }
                });
            });
            ui.separator();

            let mut open = None;
            egui::ScrollArea::vertical().show(ui, |ui| {
                for p in &ed.files {
                    let rel = p.strip_prefix(&ed.project_dir).unwrap_or(p).display().to_string();
                    let is_active = ed.docs.get(ed.active).map_or(false, |d| &d.path == p);
                    if ui.selectable_label(is_active, format!("📄 {rel}")).clicked() {
                        open = Some(p.clone());
                    }
                }
            });
            if let Some(p) = open {
                ed.open_file(p);
            }
        });
}

// =====================================================================
//  Central area: tabs + toolbar + find bar + editor
// =====================================================================

fn central(ui: &mut egui::Ui, ed: &mut EditorState) {
    tab_bar(ui, ed);
    toolbar(ui, ed);
    if ed.find.open {
        find_bar(ui, ed);
    }
    ui.separator();

    if ed.docs.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            ui.heading("💻 No script open");
            ui.add_space(8.0);
            if ui.button("＋ New script").clicked() {
                ed.new_file();
            }
        });
        return;
    }
    editor_view(ui, ed);
}

fn tab_bar(ui: &mut egui::Ui, ed: &mut EditorState) {
    let mut select = None;
    let mut close = None;
    let mut new = false;

    egui::ScrollArea::horizontal()
        .id_source("editor_tabs")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for (i, doc) in ed.docs.iter().enumerate() {
                    let label = format!("{}{}", doc.name(), if doc.dirty { " ●" } else { "" });
                    if ui.selectable_label(i == ed.active, label).clicked() {
                        select = Some(i);
                    }
                    if ui.small_button("✕").clicked() {
                        close = Some(i);
                    }
                    ui.add_space(8.0);
                }
                if ui.button("＋").on_hover_text("New script (Ctrl+N)").clicked() {
                    new = true;
                }
            });
        });

    if let Some(i) = select {
        ed.active = i;
    }
    if let Some(i) = close {
        ed.close_tab(i);
    }
    if new {
        ed.new_file();
    }
}

fn toolbar(ui: &mut egui::Ui, ed: &mut EditorState) {
    ui.horizontal(|ui| {
        if ui.button("💾 Save").on_hover_text("Ctrl+S").clicked() {
            ed.save_active();
        }
        if ui.button("▶ Run").on_hover_text("F5 / Ctrl+Enter").clicked() {
            ed.run_active();
        }
        ui.separator();
        if ui.button("🔍 Find").on_hover_text("Ctrl+F").clicked() {
            ed.find.open = true;
            ed.find.focus_query = true;
        }
        ui.separator();
        ui.toggle_value(&mut ed.show_explorer, "📁 Files");
        ui.toggle_value(&mut ed.show_console, "📟 Console");
        ui.separator();
        ui.label("Font");
        ui.add(
            egui::DragValue::new(&mut ed.font_size)
                .clamp_range(10.0..=32.0)
                .speed(0.2)
                .suffix(" px"),
        );
    });
}

fn find_bar(ui: &mut egui::Ui, ed: &mut EditorState) {
    let count_text = if ed.find.query.is_empty() {
        String::new()
    } else {
        format!("{} found", ed.match_count())
    };
    let mut next: Option<bool> = None;
    let (mut do_replace, mut do_all, mut close) = (false, false, false);

    ui.horizontal(|ui| {
        ui.label("🔍");
        let r = ui.add(
            egui::TextEdit::singleline(&mut ed.find.query)
                .hint_text("Find")
                .desired_width(180.0),
        );
        if ed.find.focus_query {
            r.request_focus();
            ed.find.focus_query = false;
        }
        if (r.has_focus() || r.lost_focus()) && ui.input(|i| i.key_pressed(Key::Escape)) {
            close = true;
        }
        if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
            next = Some(!ui.input(|i| i.modifiers.shift));
            r.request_focus();
        }
        ui.label(count_text);
        if ui.small_button("▲").on_hover_text("Previous").clicked() {
            next = Some(false);
        }
        if ui.small_button("▼").on_hover_text("Next").clicked() {
            next = Some(true);
        }
        ui.checkbox(&mut ed.find.case_sensitive, "Aa");
        ui.separator();
        ui.add(
            egui::TextEdit::singleline(&mut ed.find.replace)
                .hint_text("Replace")
                .desired_width(160.0),
        );
        if ui.button("Replace").clicked() {
            do_replace = true;
        }
        if ui.button("All").clicked() {
            do_all = true;
        }
        if ui.small_button("✕").clicked() {
            close = true;
        }
    });

    if let Some(f) = next {
        ed.find_next(f);
    }
    if do_replace {
        ed.replace_current();
    }
    if do_all {
        ed.replace_all();
    }
    if close {
        ed.find.open = false;
    }
}

// =====================================================================
//  Editor view (TextEdit + highlighter + gutter)
// =====================================================================

fn editor_view(ui: &mut egui::Ui, ed: &mut EditorState) {
    let ctx = ui.ctx().clone();
    let dark = ui.visuals().dark_mode;
    let font = FontId::monospace(ed.font_size);
    let active = ed.active;
    let id = Id::new(("code_editor", ed.docs[active].id));

    // ۱) اعمال انتخاب/پرش درخواست‌شده (Find, goto, ...)
    let mut scroll_to: Option<(usize, Option<Align>)> = None;
    if let Some((a, b)) = ed.pending_select.take() {
        set_selection(&ctx, id, a, b);
        ed.docs[active].sel = (a, b);
        ctx.memory_mut(|m| m.request_focus(id));
        scroll_to = Some((b, Some(Align::Center)));
    }

    // ۲) Tab / Enter / auto-close / Ctrl+/
    let key_changed = handle_keys(&ctx, id, &mut ed.docs[active]);
    if key_changed && scroll_to.is_none() {
        scroll_to = Some((ed.docs[active].sel.1, None));
    }

    // ۳) اندازه‌ها
    let avail = ui.available_size();
    let row_h = ui.fonts(|f| f.row_height(&font));
    let rows = ((avail.y / row_h) as usize).max(10);
    let char_w = ui.fonts(|f| f.glyph_width(&font, '0'));

    let doc = &mut ed.docs[active];
    let digits = (doc.text.matches('\n').count() + 1).to_string().len().max(3);
    let gutter_w = (char_w * digits as f32 + 28.0).ceil();
    let diag_lines: HashSet<usize> = doc.diagnostics.iter().map(|d| d.line).collect();

    let (bg, gutter_bg, dim, bright, cur_line_bg) = if dark {
        (
            Color32::from_rgb(40, 44, 52),
            Color32::from_rgb(33, 37, 43),
            Color32::from_rgb(99, 109, 131),
            Color32::from_rgb(171, 178, 191),
            Color32::from_white_alpha(10),
        )
    } else {
        (
            Color32::from_rgb(250, 250, 250),
            Color32::from_rgb(240, 240, 243),
            Color32::from_rgb(140, 149, 159),
            Color32::from_rgb(36, 41, 47),
            Color32::from_black_alpha(12),
        )
    };
    let err_color = Color32::from_rgb(224, 108, 117);

    let mut changed = key_changed;
    let text_width = (avail.x - gutter_w - 8.0).max(100.0);
    let doc_id = doc.id;

    egui::Frame::none().fill(bg).show(ui, |ui| {
        ui.set_min_size(avail);

        // اسکرول عمودی: گاتر و متن با هم حرکت می‌کنند
        egui::ScrollArea::vertical()
            .id_source(("editor_vscroll", doc_id))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let clip = ui.clip_rect();
                let origin = ui.cursor().min;

                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.add_space(gutter_w); // ستون ثابت گاتر

                    let mut layouter = |ui: &egui::Ui, text: &str, _wrap: f32| {
                        ui.fonts(|f| f.layout_job(highlight_job(text, &font, dark)))
                    };

                    // اسکرول افقی: فقط روی متن اثر دارد
                    let out = egui::ScrollArea::horizontal()
                        .id_source(("editor_hscroll", doc_id))
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            let out = egui::TextEdit::multiline(&mut doc.text)
                                .id(id)
                                .font(font.clone())
                                .frame(false)
                                .lock_focus(true)
                                .desired_width(text_width)
                                .desired_rows(rows)
                                .layouter(&mut layouter)
                                .show(ui);

                            // اسکرول به کرسر (بعد از Find/goto/Enter)
                            if let Some((idx, align)) = scroll_to {
                                let cur = out.galley.from_ccursor(CCursor::new(idx));
                                let r = out
                                    .galley
                                    .pos_from_cursor(&cur)
                                    .translate(out.galley_pos.to_vec2());
                                ui.scroll_to_rect(r.expand2(vec2(1.0, 0.0)), align);
                            }
                            out
                        })
                        .inner;

                    let galley = out.galley.clone();
                    let pos = out.galley_pos;
                    if let Some(r) = out.cursor_range {
                        doc.sel = (r.secondary.ccursor.index, r.primary.ccursor.index);
                    }
                    if out.response.changed() {
                        changed = true;
                    }

                    let painter = ui.painter();
                    let full_h = galley.size().y.max(avail.y);

                    // هایلایت خط فعلی (از ابتدای گاتر تا انتهای ناحیه‌ی دید)
                    let caret_row = galley.from_ccursor(CCursor::new(doc.sel.1)).rcursor.row;
                    if let Some(row) = galley.rows.get(caret_row) {
                        let y0 = pos.y + row.rect.min.y;
                        let y1 = pos.y + row.rect.max.y;
                        painter.rect_filled(
                            Rect::from_min_max(pos2(origin.x, y0), pos2(clip.right(), y1)),
                            0.0,
                            cur_line_bg,
                        );
                    }

                    // پس‌زمینه‌ی گاتر: ستون ثابت با عرض gutter_w
                    painter.rect_filled(
                        Rect::from_min_max(
                            pos2(origin.x, origin.y),
                            pos2(origin.x + gutter_w, origin.y + full_h),
                        ),
                        0.0,
                        gutter_bg,
                    );

                    // شماره خط‌ها (فقط ردیف‌های قابل‌مشاهده)
                    for (i, row) in galley.rows.iter().enumerate() {
                        let y = pos.y + row.rect.min.y;
                        if y + row.rect.height() < clip.top() || y > clip.bottom() {
                            continue;
                        }
                        let color = if diag_lines.contains(&i) {
                            err_color
                        } else if i == caret_row {
                            bright
                        } else {
                            dim
                        };
                        painter.text(
                            pos2(origin.x + gutter_w - 12.0, y),
                            Align2::RIGHT_TOP,
                            (i + 1).to_string(),
                            font.clone(),
                            color,
                        );
                    }
                });
            });
    });

    if changed {
        doc.dirty = true;
        doc.refresh();
    }
}

// =====================================================================
//  Keyboard handling (auto-indent, auto-close, tab, comment)
// =====================================================================

fn handle_keys(ctx: &egui::Context, id: Id, doc: &mut Document) -> bool {
    if !ctx.memory(|m| m.has_focus(id)) {
        return false;
    }

    let (shift_tab, tab, enter, comment, typed) = ctx.input_mut(|i| {
        let shift_tab = i.consume_key(Modifiers::SHIFT, Key::Tab);
        let tab = i.consume_key(Modifiers::NONE, Key::Tab);
        let enter = i.consume_key(Modifiers::NONE, Key::Enter);
        let comment = i.consume_key(Modifiers::COMMAND, Key::Slash);

        let mut typed: Vec<char> = Vec::new();
        i.events.retain(|e| match e {
            Event::Text(t) if t.chars().count() == 1 && "([{\")]}".contains(t.as_str()) => {
                typed.push(t.chars().next().unwrap());
                false
            }
            _ => true,
        });
        (shift_tab, tab, enter, comment, typed)
    });

    let mut sel = doc.sel;
    let mut changed = false;

    if shift_tab {
        sel = edit::tab(&mut doc.text, sel, true);
        changed = true;
    } else if tab {
        sel = edit::tab(&mut doc.text, sel, false);
        changed = true;
    }
    if enter {
        let c = edit::newline(&mut doc.text, sel);
        sel = (c, c);
        changed = true;
    }
    if comment {
        sel = edit::toggle_comment(&mut doc.text, sel);
        changed = true;
    }
    for c in typed {
        sel = edit::type_char(&mut doc.text, sel, c);
        changed = true;
    }

    if changed {
        doc.sel = sel;
        set_selection(ctx, id, sel.0, sel.1);
    }
    changed
}

/// تنها نقطه‌ای که به API داخلی TextEditState وابسته است
fn set_selection(ctx: &egui::Context, id: Id, anchor: usize, caret: usize) {
    let mut state = TextEditState::load(ctx, id).unwrap_or_default();
    state
        .cursor
        .set_char_range(Some(CCursorRange::two(CCursor::new(anchor), CCursor::new(caret))));
    state.store(ctx, id);
}

// =====================================================================
//  Syntax highlighting
// =====================================================================

fn highlight_job(text: &str, font: &FontId, dark: bool) -> LayoutJob {
    let mut job = LayoutJob::default();
    for tok in lexer::tokenize(text) {
        let (color, italics) = palette(tok.kind, dark);
        job.append(
            &text[tok.start..tok.end],
            0.0,
            TextFormat { font_id: font.clone(), color, italics, ..Default::default() },
        );
    }
    if job.text.is_empty() {
        job.append("", 0.0, TextFormat { font_id: font.clone(), ..Default::default() });
    }
    job
}

fn palette(kind: Tok, dark: bool) -> (Color32, bool) {
    let c = Color32::from_rgb;
    if dark {
        match kind {
            Tok::Plain => (c(171, 178, 191), false),
            Tok::Keyword => (c(198, 120, 221), false),
            Tok::Type => (c(229, 192, 123), false),
            Tok::Func => (c(97, 175, 239), false),
            Tok::Macro => (c(86, 182, 194), false),
            Tok::Str => (c(152, 195, 121), false),
            Tok::Num => (c(209, 154, 102), false),
            Tok::Comment => (c(92, 99, 112), true),
        }
    } else {
        match kind {
            Tok::Plain => (c(36, 41, 47), false),
            Tok::Keyword => (c(207, 34, 46), false),
            Tok::Type => (c(149, 56, 0), false),
            Tok::Func => (c(130, 80, 223), false),
            Tok::Macro => (c(5, 80, 174), false),
            Tok::Str => (c(10, 48, 105), false),
            Tok::Num => (c(5, 80, 174), false),
            Tok::Comment => (c(110, 119, 129), true),
        }
    }
}