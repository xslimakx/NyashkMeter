//! Shared Nyashk widgets: palette, knob (drag / shift-fine / wheel / double-click reset / click-to-type),
//! pill buttons, section titles.
#![allow(dead_code)]
use nih_plug::prelude::*;
use nih_plug_egui::egui::{self, epaint::PathStroke, text::LayoutJob, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, TextFormat, Ui, UiBuilder, Vec2};

pub const BG: Color32 = Color32::from_rgb(0x0a, 0x0a, 0x0a);
pub const PANEL: Color32 = Color32::from_rgb(0x11, 0x11, 0x11);
pub const PANEL2: Color32 = Color32::from_rgb(0x1c, 0x1c, 0x1c);
pub const LINE: Color32 = Color32::from_rgb(0x2e, 0x2e, 0x2e);
pub const WHITE: Color32 = Color32::from_rgb(0xf2, 0xf2, 0xf2);
pub const DIM: Color32 = Color32::from_rgb(0x80, 0x80, 0x80);
pub const MID: Color32 = Color32::from_rgb(0x7a, 0x7a, 0x7a);

pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

pub fn set_norm<P: Param>(setter: &ParamSetter, p: &P, v: f32) {
    setter.begin_set_parameter(p);
    setter.set_parameter_normalized(p, v.clamp(0.0, 1.0));
    setter.end_set_parameter(p);
}
pub fn set_f(setter: &ParamSetter, p: &FloatParam, v: f32) {
    setter.begin_set_parameter(p);
    setter.set_parameter(p, v);
    setter.end_set_parameter(p);
}
pub fn set_b(setter: &ParamSetter, p: &BoolParam, v: bool) {
    setter.begin_set_parameter(p);
    setter.set_parameter(p, v);
    setter.end_set_parameter(p);
}
pub fn set_e<E: Enum + PartialEq>(setter: &ParamSetter, p: &EnumParam<E>, v: E) {
    setter.begin_set_parameter(p);
    setter.set_parameter(p, v);
    setter.end_set_parameter(p);
}
pub fn reset<P: Param>(setter: &ParamSetter, p: &P) {
    set_norm(setter, p, p.default_normalized_value());
}

#[derive(Clone, Copy, Default)]
pub struct KnobEv {
    pub delta: f32,
    pub started: bool,
    pub total: Option<f32>,
}

pub fn knob<P: Param>(ui: &mut Ui, setter: &ParamSetter, p: &P, label: &str, cell: Rect, tooltip: &str, bipolar: bool) -> KnobEv {
    knob_r(ui, setter, p, label, cell, tooltip, bipolar, 30.0)
}

/// Band colours (muted so they sit well on the black UI): used by the EQ and the channel strip.
pub const C_LF: Color32 = Color32::from_rgb(0xd8, 0x62, 0x5c);
pub const C_LMF: Color32 = Color32::from_rgb(0x86, 0xc2, 0x6e);
pub const C_HMF: Color32 = Color32::from_rgb(0x5f, 0xa3, 0xe4);
pub const C_HF: Color32 = Color32::from_rgb(0xc0, 0x92, 0xe0);
pub const C_DYN: Color32 = Color32::from_rgb(0xe2, 0xb0, 0x58);
pub const C_GATE: Color32 = Color32::from_rgb(0x58, 0xc4, 0xb8);

#[allow(clippy::too_many_arguments)]
pub fn knob_r<P: Param>(ui: &mut Ui, setter: &ParamSetter, p: &P, label: &str, cell: Rect, tooltip: &str, bipolar: bool, r: f32) -> KnobEv {
    knob_ac(ui, setter, p, label, cell, tooltip, bipolar, r, WHITE)
}

/// Knob with an accent colour for the value arc and pointer.
#[allow(clippy::too_many_arguments)]
pub fn knob_ac<P: Param>(ui: &mut Ui, setter: &ParamSetter, p: &P, label: &str, cell: Rect, tooltip: &str, bipolar: bool, r: f32, accent: Color32) -> KnobEv {
    let c = Pos2::new(cell.center().x, cell.top() + r + 4.0);
    let hit = Rect::from_center_size(c, Vec2::splat(2.0 * r + 8.0));
    let resp = ui.allocate_rect(hit, Sense::click_and_drag());
    let resp = if tooltip.is_empty() { resp } else { resp.on_hover_text(tooltip) };

    let norm = p.unmodulated_normalized_value();
    let mut delta = 0.0f32;
    let mut ev = KnobEv::default();
    let raw_id = egui::Id::new(("knob_raw", label, p.name()));
    if resp.drag_started() {
        setter.begin_set_parameter(p);
        ui.data_mut(|d| d.insert_temp(raw_id, (norm, norm)));
        ev.started = true;
    }
    if resp.dragged() {
        let fine = ui.input(|i| i.modifiers.shift);
        let d = -resp.drag_delta().y / if fine { 1200.0 } else { 220.0 };
        let (raw0, raw) = ui.data(|dd| dd.get_temp::<(f32, f32)>(raw_id)).unwrap_or((norm, norm));
        let raw = raw + d;
        ui.data_mut(|dd| dd.insert_temp(raw_id, (raw0, raw)));
        let new = raw.clamp(0.0, 1.0);
        delta = new - norm;
        setter.set_parameter_normalized(p, new);
        ev.total = Some(raw - raw0);
    }
    if resp.drag_stopped() {
        setter.end_set_parameter(p);
    }
    if resp.double_clicked() {
        delta = p.default_normalized_value() - norm;
        set_norm(setter, p, p.default_normalized_value());
        ev.started = false;
    }
    if resp.hovered() {
        let sc = ui.input(|i| i.smooth_scroll_delta.y);
        if sc != 0.0 {
            let new = (norm + sc / 1500.0).clamp(0.0, 1.0);
            delta = new - norm;
            setter.begin_set_parameter(p);
            setter.set_parameter_normalized(p, new);
            setter.end_set_parameter(p);
        }
    }

    let norm = p.unmodulated_normalized_value();
    let painter = ui.painter();
    let a0 = std::f32::consts::PI * 0.75;
    let sweep = std::f32::consts::PI * 1.5;
    let ring = r - 2.5;
    let pt = |rad: f32, t: f32| -> Pos2 {
        let a = a0 + t * sweep;
        Pos2::new(c.x + rad * a.cos(), c.y + rad * a.sin())
    };
    let arc = |from: f32, to: f32| -> Vec<Pos2> {
        let n = (((to - from).abs() / sweep) * 120.0).ceil().max(2.0) as usize;
        (0..=n).map(|i| pt(ring, from + (to - from) * i as f32 / n as f32)).collect()
    };
    // track + value arc with round caps
    painter.add(egui::Shape::line(arc(0.0, 1.0), PathStroke::new(4.0, LINE)));
    painter.circle_filled(pt(ring, 0.0), 2.0, LINE);
    painter.circle_filled(pt(ring, 1.0), 2.0, LINE);
    let from = if bipolar { 0.5 } else { 0.0 };
    if (norm - from).abs() > 0.002 {
        painter.add(egui::Shape::line(arc(from, norm), PathStroke::new(4.0, accent)));
        painter.circle_filled(pt(ring, from), 2.0, accent);
        painter.circle_filled(pt(ring, norm), 2.0, accent);
    }
    // round knob body
    let body = r * 0.72;
    painter.circle_filled(c + Vec2::new(0.0, 1.5), body + 1.0, Color32::from_rgb(5, 5, 5));
    painter.circle_filled(c, body, Color32::from_rgb(34, 34, 34));
    painter.circle_filled(c, body * 0.84, Color32::from_rgb(24, 24, 24));
    painter.circle_stroke(c, body, Stroke::new(1.0, Color32::from_rgb(70, 70, 70)));
    // pointer
    painter.line_segment([pt(body * 0.30, norm), pt(body * 0.80, norm)], Stroke::new(3.0, accent));
    painter.circle_filled(pt(body * 0.80, norm), 1.5, accent);
    painter.circle_filled(pt(body * 0.30, norm), 1.5, accent);

    // value + label (click the value box to type a number)
    let val = p.normalized_value_to_string(norm, true);
    let vr = Rect::from_center_size(Pos2::new(cell.center().x, c.y + r + 14.0), Vec2::new(cell.width().min(84.0), 18.0));
    let edit_id = egui::Id::new(("knob_edit", label, p.name()));
    let editing: Option<String> = ui.data(|d| d.get_temp::<String>(edit_id));
    painter_label(ui, cell, vr, label);
    if let Some(mut buf) = editing {
        let te = egui::TextEdit::singleline(&mut buf)
            .font(mono(11.0))
            .horizontal_align(egui::Align::Center)
            .desired_width(vr.width())
            .margin(egui::Margin::symmetric(2, 1));
        let r_te = ui.put(vr, te);
        if !r_te.has_focus() && !r_te.lost_focus() {
            r_te.request_focus();
        }
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if esc {
            ui.data_mut(|d| d.remove::<String>(edit_id));
        } else if enter || r_te.lost_focus() {
            let txt = buf.trim().replace(',', ".");
            if let Some(nv) = p.string_to_normalized_value(&txt) {
                let nv = nv.clamp(0.0, 1.0);
                delta = nv - norm;
                set_norm(setter, p, nv);
            }
            ui.data_mut(|d| d.remove::<String>(edit_id));
        } else {
            ui.data_mut(|d| d.insert_temp(edit_id, buf));
        }
    } else {
        let vresp = ui.allocate_rect(vr, Sense::click()).on_hover_text("Click to type a value");
        let painter = ui.painter();
        painter.rect_stroke(vr, CornerRadius::same(4), Stroke::new(1.0, if vresp.hovered() { DIM } else { LINE }), StrokeKind::Inside);
        painter.text(vr.center(), Align2::CENTER_CENTER, &val, mono(11.0), WHITE);
        if vresp.clicked() {
            let plain = val.trim().split(' ').next().unwrap_or("").to_string();
            ui.data_mut(|d| d.insert_temp(edit_id, plain));
        }
    }
    ev.delta = delta;
    ev
}

pub fn painter_label(ui: &Ui, cell: Rect, vr: Rect, label: &str) {
    ui.painter().text(Pos2::new(cell.center().x, vr.bottom() + 9.0), Align2::CENTER_CENTER, label, mono(10.0), DIM);
}

pub fn pill(ui: &mut Ui, rect: Rect, text: &str, on: bool) -> bool {
    pill_tip(ui, rect, text, on, "")
}

pub fn pill_tip(ui: &mut Ui, rect: Rect, text: &str, on: bool, tip: &str) -> bool {
    let mut resp = ui.allocate_rect(rect, Sense::click());
    if !tip.is_empty() {
        resp = resp.on_hover_text(tip);
    }
    let painter = ui.painter();
    let hov = resp.hovered();
    painter.rect_filled(rect, CornerRadius::same(6), if on { WHITE } else if hov { PANEL2 } else { Color32::TRANSPARENT });
    painter.rect_stroke(rect, CornerRadius::same(6), Stroke::new(1.0, if on { WHITE } else if hov { DIM } else { LINE }), StrokeKind::Inside);
    painter.text(rect.center(), Align2::CENTER_CENTER, text, mono(11.0), if on { Color32::BLACK } else { WHITE });
    resp.clicked()
}

pub fn enum_combo<E: Enum + PartialEq + 'static>(ui: &mut Ui, setter: &ParamSetter, p: &EnumParam<E>, id: &str, rect: Rect) {
    let names = E::variants();
    let idx = p.value().to_index();
    ui.allocate_new_ui(UiBuilder::new().max_rect(rect), |ui| {
        egui::ComboBox::from_id_salt(id)
            .width(rect.width() - 22.0)
            .selected_text(egui::RichText::new(names[idx]).font(mono(12.0)))
            .show_ui(ui, |ui| {
                for (i, n) in names.iter().enumerate() {
                    if ui.selectable_label(i == idx, egui::RichText::new(*n).font(mono(12.0))).clicked() {
                        set_e(setter, p, E::from_index(i));
                    }
                }
            });
    });
}

pub fn section_title(ui: &Ui, pos: Pos2, num: &str, title: &str) {
    let mut job = LayoutJob::default();
    job.append("\u{2116}", 0.0, TextFormat { font_id: FontId::proportional(11.0), color: DIM, ..Default::default() });
    job.append(&format!(" {num}  {title}"), 0.0, TextFormat { font_id: mono(10.5), color: DIM, ..Default::default() });
    let galley = ui.fonts(|f| f.layout_job(job));
    ui.painter().galley(pos, galley, DIM);
}

// ---------------------------------------------------------------------------- shared chrome

pub fn apply_theme(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.extreme_bg_color = PANEL;
    v.override_text_color = Some(WHITE);
    v.selection.bg_fill = Color32::from_rgb(64, 64, 64);
    v.selection.stroke = Stroke::new(1.0, WHITE);
    ctx.set_visuals(v);
}

/// Panel with the standard Nyashk outline.
pub fn panel(ui: &Ui, rect: Rect) {
    ui.painter().rect_filled(rect, CornerRadius::same(6), PANEL);
    ui.painter().rect_stroke(rect, CornerRadius::same(6), Stroke::new(1.0, LINE), StrokeKind::Inside);
}

/// Logo line: "Nyashk" + dim product name + blinking cursor + subtitle, and the header rule.
pub fn header(ui: &Ui, o: Pos2, width: f32, product: &str, subtitle: &str) {
    let painter = ui.painter();
    let time = ui.input(|i| i.time);
    painter.text(Pos2::new(o.x + 18.0, o.y + 34.0), Align2::LEFT_CENTER, "Nyashk", mono(19.0), WHITE);
    painter.text(Pos2::new(o.x + 18.0 + 6.0 * 11.4, o.y + 34.0), Align2::LEFT_CENTER, product, mono(19.0), DIM);
    if ((time * 2.0) as i64) % 2 == 0 {
        painter.text(Pos2::new(o.x + 18.0 + (6 + product.len()) as f32 * 11.4, o.y + 34.0), Align2::LEFT_CENTER, "_", mono(19.0), WHITE);
    }
    painter.text(Pos2::new(o.x + 28.0 + (6 + product.len()) as f32 * 11.4 + 14.0, o.y + 36.0), Align2::LEFT_CENTER, subtitle, mono(10.5), DIM);
    painter.hline(o.x + 14.0..=o.x + width - 14.0, o.y + 58.0, Stroke::new(1.0, LINE));
}

/// Bottom-right grip that resizes the window keeping its proportions.
pub fn resize_grip(ui: &mut Ui, ctx: &egui::Context, size_state: &nih_plug_egui::EguiState, full: Rect, base_w: f32, base_h: f32, min_w: f32) {
    let grip = Rect::from_min_size(full.max - Vec2::splat(22.0), Vec2::splat(22.0));
    let r = ui.interact(grip, egui::Id::new("nyashk_resize_grip"), Sense::drag()).on_hover_text("Drag to resize");
    if r.dragged() {
        if let Some(pos) = r.interact_pointer_pos() {
            let ppp = if cfg!(target_os = "macos") { size_state.size().0 as f32 / base_w } else { ctx.pixels_per_point() };
            let w = ((pos.x - full.min.x + 11.0) * ppp).clamp(min_w, 2800.0);
            size_state.set_requested_size((w.round() as u32, (w * base_h / base_w).round() as u32));
        }
    }
    let gc = if r.hovered() || r.dragged() { WHITE } else { DIM };
    for k in [6.0f32, 11.0, 16.0] {
        ui.painter().line_segment([Pos2::new(full.max.x - 3.0, full.max.y - k), Pos2::new(full.max.x - k, full.max.y - 3.0)], Stroke::new(1.0, gc));
    }
}

/// Vertical level bar. `lo..hi` in dB, `peak` draws a hold tick.
pub fn vbar(ui: &Ui, rect: Rect, level_db: f32, peak_db: f32, lo: f32, hi: f32) {
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(2), Color32::from_rgb(0x07, 0x07, 0x07));
    let t = |v: f32| ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
    let h = rect.height() * t(level_db);
    if h > 0.5 {
        let fill = Rect::from_min_max(Pos2::new(rect.left() + 1.0, rect.bottom() - h), Pos2::new(rect.right() - 1.0, rect.bottom()));
        p.rect_filled(fill, CornerRadius::same(1), Color32::from_rgb(0xc8, 0xc8, 0xc8));
        // top 6 dB brighter
        let hot = t(hi - 6.0);
        if t(level_db) > hot {
            let hr = Rect::from_min_max(Pos2::new(rect.left() + 1.0, rect.bottom() - h), Pos2::new(rect.right() - 1.0, rect.bottom() - rect.height() * hot));
            p.rect_filled(hr, CornerRadius::same(1), WHITE);
        }
    }
    let py = rect.bottom() - rect.height() * t(peak_db);
    p.hline(rect.left()..=rect.right(), py, Stroke::new(1.5, WHITE));
    p.rect_stroke(rect, CornerRadius::same(2), Stroke::new(1.0, LINE), StrokeKind::Inside);
}

/// Small round indicator.
pub fn led(ui: &Ui, c: Pos2, on: bool) {
    ui.painter().circle_filled(c, 3.5, if on { WHITE } else { Color32::from_rgb(0x2a, 0x2a, 0x2a) });
    ui.painter().circle_stroke(c, 3.5, Stroke::new(1.0, LINE));
}

/// Fill the area between a polyline and the horizontal line `base_y`.
/// egui fills only convex polygons, so a spectrum has to be drawn as a strip of quads.
pub fn fill_under(painter: &egui::Painter, pts: &[Pos2], base_y: f32, color: Color32) {
    if pts.len() < 2 {
        return;
    }
    let mut mesh = egui::epaint::Mesh::default();
    let uv = egui::epaint::WHITE_UV;
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let i = mesh.vertices.len() as u32;
        for p in [a, b, Pos2::new(b.x, base_y), Pos2::new(a.x, base_y)] {
            mesh.vertices.push(egui::epaint::Vertex { pos: p, uv, color });
        }
        mesh.indices.extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
    }
    painter.add(egui::Shape::mesh(mesh));
}
