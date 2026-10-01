//! The arrange view: marker lane, ruler, LTC lane, track lanes with
//! waveforms, playhead, plus all mouse interaction on them.

use cueline_core::FrameRate;
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CornerRadius, Mesh, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui,
};

use super::ruler::{choose_steps, Step};
use super::widgets::tabular;
use super::{fonts, theme};
use crate::app::{CueLineApp, TrackState};

pub const HEADER_W: f32 = 248.0;
pub const MARKER_H: f32 = 22.0;
pub const RULER_H: f32 = 24.0;
pub const LTC_H: f32 = 40.0;
pub const OVERVIEW_H: f32 = 18.0;
const REGION_TITLE_H: f32 = 16.0;
const MIN_PPS: f32 = 0.02;
const MAX_PPS: f32 = 20_000.0;

#[derive(Clone, Copy, Debug)]
pub enum Drag {
    Clip { id: u64, orig: f64, grab: f64 },
    Marker { idx: usize, orig: f64, grab: f64 },
    Scrub,
}

/// Maps timeline seconds to screen x and back.
#[derive(Clone, Copy)]
pub struct Map {
    pub left: f32,
    pub pps: f32,
    pub scroll: f64,
}

impl Map {
    pub fn x(&self, t: f64) -> f32 {
        self.left + ((t - self.scroll) * self.pps as f64) as f32
    }
    pub fn t(&self, x: f32) -> f64 {
        self.scroll + ((x - self.left) / self.pps) as f64
    }
}

/// Rounds `t` to the nearest frame boundary of the timeline grid.
pub fn snap_to_frame(t: f64, rate: FrameRate) -> f64 {
    let fps = rate.fps();
    (t * fps).round() / fps
}

pub fn draw(app: &mut CueLineApp, ui: &mut Ui) {
    let full = ui.available_rect_before_wrap();
    ui.allocate_rect(full, Sense::hover());
    let lanes_left = full.left() + HEADER_W;
    // `ruler` covers the marker lane and the time ruler; both interact as one.
    let ruler = Rect::from_min_max(pos2(lanes_left, full.top()), pos2(full.right(), full.top() + MARKER_H + RULER_H));
    let ltc = Rect::from_min_max(pos2(lanes_left, ruler.bottom()), pos2(full.right(), ruler.bottom() + LTC_H));
    let overview = Rect::from_min_max(pos2(lanes_left, full.bottom() - OVERVIEW_H), full.right_bottom());
    let lanes = Rect::from_min_max(pos2(lanes_left, ltc.bottom()), pos2(full.right(), overview.top()));
    let headers = Rect::from_min_max(pos2(full.left(), ltc.bottom()), pos2(lanes_left, full.bottom()));
    let corner = Rect::from_min_max(full.left_top(), pos2(lanes_left, ruler.bottom()));
    let ltc_header = Rect::from_min_max(pos2(full.left(), ruler.bottom()), pos2(lanes_left, ltc.bottom()));
    let arrange = ruler.union(lanes);

    if std::mem::take(&mut app.ui.zoom_to_fit) {
        let end = app.project_end_secs().max(10.0);
        let margin = end * 0.04;
        app.view.px_per_sec = (lanes.width() / (end + 2.0 * margin) as f32).clamp(MIN_PPS, MAX_PPS);
        app.view.scroll_secs = -margin;
    }
    handle_wheel(app, ui, arrange, lanes);
    let playhead = app.position_secs();
    follow_playhead(app, lanes, playhead);

    let map = Map { left: lanes_left, pps: app.view.px_per_sec, scroll: app.view.scroll_secs };
    let painter = ui.painter_at(full);
    painter.rect_filled(full, CornerRadius::ZERO, theme::BG_CONTENT);

    paint_lanes(app, &painter.with_clip_rect(lanes), lanes, map);
    paint_ltc_lane(app, &painter.with_clip_rect(ltc), ltc, map);
    paint_ruler(app, &painter.with_clip_rect(ruler), ruler, map);

    lanes_interaction(app, ui, lanes, map);
    overview_bar(app, ui, overview, lanes.width(), playhead);
    ruler_interaction(app, ui, ruler, map);

    // Start position and playhead over everything in the arrange area.
    let p = painter.with_clip_rect(Rect::from_min_max(ruler.left_top(), lanes.right_bottom()));
    if app.is_playing() {
        let x = map.x(app.ui.play_started_at).round() + 0.5;
        p.vline(x, ruler.bottom()..=lanes.bottom(), Stroke::new(1.0, Color32::from_white_alpha(40)));
    }
    let x = map.x(playhead).round() + 0.5;
    p.vline(x + 1.0, (ruler.bottom() - 6.0)..=lanes.bottom(), Stroke::new(1.0, Color32::from_black_alpha(90)));
    p.vline(x, (ruler.bottom() - 6.0)..=lanes.bottom(), Stroke::new(1.0, theme::PLAYHEAD));
    let head_top = ruler.top() + MARKER_H + 3.0;
    let head = vec![
        pos2(x - 5.5, head_top),
        pos2(x + 5.5, head_top),
        pos2(x + 5.5, head_top + 8.0),
        pos2(x, head_top + 13.0),
        pos2(x - 5.5, head_top + 8.0),
    ];
    p.add(Shape::convex_polygon(head, theme::PLAYHEAD, Stroke::NONE));

    super::headers::corner(app, ui, corner);
    super::headers::ltc_header(app, ui, ltc_header);
    super::headers::track_headers(app, ui, headers);

    let hair = Stroke::new(1.0, theme::HAIRLINE);
    painter.vline(lanes_left - 0.5, full.y_range(), hair);
    painter.hline(full.x_range(), ruler.top() + MARKER_H - 0.5, Stroke::new(1.0, theme::SEPARATOR));
    painter.hline(full.x_range(), ruler.bottom() - 0.5, hair);
    painter.hline(full.x_range(), ltc.bottom() - 0.5, hair);
    painter.hline(overview.x_range(), overview.top() + 0.5, hair);
}

fn handle_wheel(app: &mut CueLineApp, ui: &Ui, arrange: Rect, lanes: Rect) {
    let Some(hover) = ui.input(|i| i.pointer.hover_pos()) else { return };
    if !arrange.contains(hover) {
        return;
    }
    let (zoom, scroll) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta()));
    let v = &mut app.view;
    if zoom != 1.0 {
        // Zoom around the mouse position.
        let t = v.scroll_secs + ((hover.x - lanes.left()) / v.px_per_sec) as f64;
        v.px_per_sec = (v.px_per_sec * zoom).clamp(MIN_PPS, MAX_PPS);
        v.scroll_secs = t - ((hover.x - lanes.left()) / v.px_per_sec) as f64;
    }
    let content_h = app.tracks.len() as f32 * v.track_height;
    let max_y = (content_h - lanes.height() + 40.0).max(0.0);
    if scroll.x != 0.0 {
        v.scroll_secs -= (scroll.x / v.px_per_sec) as f64;
    }
    if scroll.y != 0.0 {
        if max_y > 0.0 && lanes.contains(hover) {
            v.scroll_y = (v.scroll_y - scroll.y).clamp(0.0, max_y);
        } else {
            v.scroll_secs -= (scroll.y / v.px_per_sec) as f64;
        }
    }
    v.scroll_y = v.scroll_y.clamp(0.0, max_y);
}

fn follow_playhead(app: &mut CueLineApp, lanes: Rect, playhead: f64) {
    if !app.settings.follow_playhead || !app.is_playing() || app.view.drag.is_some() {
        return;
    }
    let v = &mut app.view;
    let width_secs = (lanes.width() / v.px_per_sec) as f64;
    let rel = (playhead - v.scroll_secs) / width_secs;
    if !(0.0..0.92).contains(&rel) {
        // Page forward, keeping a little context on the left.
        v.scroll_secs = playhead - width_secs * 0.08;
    }
}

fn grid_lines(app: &CueLineApp, map: Map, width: f32, step: Step) -> impl Iterator<Item = f64> {
    let rate = app.project.frame_rate;
    let s = step.secs(rate);
    let t0 = map.scroll;
    let t1 = map.t(map.left + width);
    let k0 = (t0 / s).floor() as i64;
    let k1 = (t1 / s).ceil() as i64;
    (k0..=k1).map(move |k| match step {
        Step::Frames(n) => (k * n) as f64 / rate.fps(),
        Step::Seconds(n) => (k * n) as f64,
    })
}

fn paint_ruler(app: &CueLineApp, p: &egui::Painter, rect: Rect, map: Map) {
    let marker_lane = Rect::from_min_size(rect.min, vec2(rect.width(), MARKER_H));
    let ruler = Rect::from_min_max(pos2(rect.left(), marker_lane.bottom()), rect.max);
    p.rect_filled(marker_lane, CornerRadius::ZERO, theme::BG_PANEL);
    p.rect_filled(ruler, CornerRadius::ZERO, theme::BG_HEADER);
    shade_before_start(p, ruler, map);

    let rate = app.project.frame_rate;
    let (major, minor) = choose_steps(map.pps, rate, 96.0);
    for t in grid_lines(app, map, rect.width(), minor) {
        let x = map.x(t).round() + 0.5;
        p.vline(x, (ruler.bottom() - 4.0)..=ruler.bottom(), Stroke::new(1.0, theme::TEXT_QUATERNARY));
    }
    let font = fonts::text(10.5);
    for t in grid_lines(app, map, rect.width(), major) {
        let x = map.x(t).round() + 0.5;
        p.vline(x, (ruler.bottom() - 9.0)..=ruler.bottom(), Stroke::new(1.0, theme::TEXT_FAINT));
        let tc = app.timecode_at(t + 1e-6).display(rate).to_string();
        tabular(p, pos2(x + 4.0, ruler.center().y - 2.0), Align2::LEFT_CENTER, &tc, font.clone(), theme::TEXT_DIM);
    }

    // Project end.
    let end = app.project_end_secs();
    if end > 0.0 {
        let x = map.x(end).round() + 0.5;
        p.vline(x, ruler.y_range(), Stroke::new(1.0, theme::TEXT_FAINT));
    }

    // Markers as rounded flags in their lane.
    let flag_font = fonts::semibold(10.5);
    for (i, m) in app.project.markers.iter().enumerate() {
        let x = map.x(m.time_secs).round();
        let color = theme::rgb(m.color);
        let selected = app.view.selected_marker == Some(i);
        let label = if m.name.is_empty() { format!("{}", i + 1) } else { format!("{}   {}", i + 1, m.name) };
        let galley = p.layout_no_wrap(label, flag_font.clone(), Color32::BLACK);
        let flag = Rect::from_min_size(pos2(x, marker_lane.top() + 3.0), vec2(galley.size().x + 12.0, MARKER_H - 6.0));
        p.rect_filled(flag, CornerRadius { nw: 0, ne: 5, sw: 0, se: 5 }, color);
        if selected {
            p.rect_stroke(
                flag,
                CornerRadius { nw: 0, ne: 5, sw: 0, se: 5 },
                Stroke::new(1.5, Color32::WHITE),
                StrokeKind::Inside,
            );
        }
        p.galley(
            pos2(flag.left() + 6.0, flag.center().y - galley.size().y / 2.0),
            galley,
            Color32::from_rgb(0x14, 0x14, 0x16),
        );
        p.vline(x + 0.5, marker_lane.top() + 3.0..=ruler.bottom(), Stroke::new(1.0, color));
    }
}

fn paint_ltc_lane(app: &CueLineApp, p: &egui::Painter, rect: Rect, map: Map) {
    p.rect_filled(rect, CornerRadius::ZERO, theme::BG_LANE);
    shade_before_start(p, rect, map);
    let enabled = app.project.ltc.enabled;
    let base = if enabled { theme::LTC } else { theme::TEXT_FAINT };
    // LTC runs continuously from the project start: draw it as one long region.
    let x0 = map.x(0.0);
    let region = Rect::from_min_max(pos2(x0, rect.top() + 5.0), pos2(rect.right() + 8.0, rect.bottom() - 5.0));
    if region.right() <= rect.left() || region.width() <= 0.0 {
        return;
    }
    p.rect_filled(region, CornerRadius::same(5), base.gamma_multiply(0.16));
    p.rect_stroke(region, CornerRadius::same(5), Stroke::new(1.0, base.gamma_multiply(0.45)), StrokeKind::Inside);

    let rate = app.project.frame_rate;
    let px_per_frame = map.pps as f64 / rate.fps();
    let inner = region.shrink2(vec2(0.0, 6.0));
    if px_per_frame >= 3.0 {
        let step = if px_per_frame >= 28.0 { Step::Frames(1) } else { choose_steps(map.pps, rate, 12.0).1 };
        for t in grid_lines(app, map, rect.width(), step).filter(|t| *t > 0.0) {
            let x = map.x(t).round() + 0.5;
            p.vline(x, inner.y_range(), Stroke::new(1.0, base.gamma_multiply(0.35)));
            if px_per_frame >= 84.0 {
                let tc = app.timecode_at(t + 1e-6).display(rate).to_string();
                tabular(p, pos2(x + 5.0, inner.center().y), Align2::LEFT_CENTER, &tc, fonts::text(10.5), base);
            }
        }
    } else {
        // Zoomed out: a stylised biphase trace hints at the signal.
        let (top, bottom) = (inner.top() + 2.0, inner.bottom() - 2.0);
        let mut x = region.left().max(rect.left() - 10.0) + 4.0;
        let mut hi = true;
        let mut pts = Vec::new();
        let mut k = 0u32;
        while x < rect.right() {
            let y = if hi { top } else { bottom };
            pts.push(pos2(x, y));
            k = k.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            x += if (k >> 16).is_multiple_of(3) { 3.0 } else { 6.0 };
            pts.push(pos2(x, y));
            hi = !hi;
        }
        p.add(Shape::line(pts, Stroke::new(1.0, base.gamma_multiply(0.55))));
    }
}

fn paint_lanes(app: &CueLineApp, p: &egui::Painter, rect: Rect, map: Map) {
    let rate = app.project.frame_rate;
    let (major, _) = choose_steps(map.pps, rate, 96.0);
    let h = app.view.track_height;
    for (i, track) in app.tracks.iter().enumerate() {
        let top = rect.top() + i as f32 * h - app.view.scroll_y;
        let row = Rect::from_min_size(pos2(rect.left(), top), vec2(rect.width(), h));
        if !row.intersects(rect) {
            continue;
        }
        let bg = if i % 2 == 0 { theme::BG_LANE } else { theme::BG_LANE_ALT };
        p.rect_filled(row, CornerRadius::ZERO, bg);
        if app.view.selected_track == Some(track.id) {
            p.rect_filled(row, CornerRadius::ZERO, theme::BLUE.gamma_multiply(0.07));
        }
        p.hline(row.x_range(), row.bottom() - 0.5, Stroke::new(1.0, theme::HAIRLINE));
    }
    shade_before_start(p, rect, map);
    for t in grid_lines(app, map, rect.width(), major) {
        let x = map.x(t).round() + 0.5;
        p.vline(x, rect.y_range(), Stroke::new(1.0, theme::GRID));
    }
    for m in &app.project.markers {
        let x = map.x(m.time_secs).round() + 0.5;
        p.vline(x, rect.y_range(), Stroke::new(1.0, theme::rgb(m.color).gamma_multiply(0.35)));
    }

    let title_font = fonts::medium(11.0);
    for (i, track) in app.tracks.iter().enumerate() {
        let top = rect.top() + i as f32 * h - app.view.scroll_y;
        let x0 = map.x(track.def.offset_secs);
        let dur = track.duration_secs().max(if track.state == TrackState::Loading { 2.0 } else { 0.0 });
        let x1 = map.x(track.def.offset_secs + dur);
        let region = Rect::from_min_max(pos2(x0, top + 3.0), pos2(x1.max(x0 + 3.0), top + h - 3.0));
        if !region.intersects(rect) {
            continue;
        }
        let failed = matches!(track.state, TrackState::Failed(_));
        let mut color = theme::rgb(track.def.color);
        if track.def.mute || failed {
            color = theme::mix(color, Color32::from_rgb(0x70, 0x70, 0x74), 0.75);
        }
        let selected = app.view.selected_track == Some(track.id);
        let radius = CornerRadius::same(5);

        // Body and title strip.
        p.rect_filled(region, radius, theme::mix(theme::BG_LANE, color, if selected { 0.34 } else { 0.26 }));
        let title = Rect::from_min_max(region.left_top(), pos2(region.right(), region.top() + REGION_TITLE_H));
        p.rect_filled(title, CornerRadius { nw: 5, ne: 5, sw: 0, se: 0 }, theme::mix(color, Color32::BLACK, 0.08));
        let label = match &track.state {
            TrackState::Loading => format!("{}  —  loading…", track.def.name),
            TrackState::Failed(_) => format!("{}  —  offline", track.def.name),
            TrackState::Ready => track.def.name.clone(),
        };
        p.with_clip_rect(title.shrink2(vec2(4.0, 0.0)).intersect(rect)).text(
            pos2(region.left().max(rect.left()) + 7.0, title.center().y),
            Align2::LEFT_CENTER,
            label,
            title_font.clone(),
            Color32::from_rgb(0x12, 0x12, 0x14),
        );

        if let Some(peaks) = &track.peaks {
            let wave = Rect::from_min_max(
                pos2(region.left(), title.bottom() + 3.0),
                pos2(region.right(), region.bottom() - 3.0),
            );
            let amp = wave.height() * 0.5 * crate::app::db_to_gain(track.def.gain_db).min(4.0);
            let mid = wave.center().y;
            let wave_color = theme::mix(color, Color32::WHITE, 0.2);
            let mut mesh = Mesh::default();
            let xa = wave.left().max(rect.left()).floor() as i32;
            let xb = wave.right().min(rect.right()).ceil() as i32;
            let sr = peaks.sample_rate as f64;
            for px in xa..xb {
                let ta = map.t(px as f32) - track.def.offset_secs;
                let tb = map.t(px as f32 + 1.0) - track.def.offset_secs;
                if let Some((lo, hi)) = peaks.range(ta * sr, tb * sr) {
                    let y0 = (mid - hi.clamp(-1.0, 1.0) * amp).max(wave.top());
                    let y1 = (mid - lo.clamp(-1.0, 1.0) * amp).min(wave.bottom()).max(y0 + 1.0);
                    mesh.add_colored_rect(
                        Rect::from_min_max(pos2(px as f32, y0), pos2(px as f32 + 1.0, y1)),
                        wave_color,
                    );
                }
            }
            p.hline(
                wave.left().max(rect.left())..=wave.right().min(rect.right()),
                mid,
                Stroke::new(1.0, color.gamma_multiply(0.35)),
            );
            p.add(Shape::mesh(mesh));
        }
        let outline = if selected {
            Stroke::new(1.5, Color32::from_white_alpha(220))
        } else {
            Stroke::new(1.0, Color32::from_black_alpha(90))
        };
        p.rect_stroke(region, radius, outline, StrokeKind::Inside);
    }
}

/// Whole-project strip with a draggable view window (acts as a scrollbar).
fn overview_bar(app: &mut CueLineApp, ui: &mut Ui, rect: Rect, view_w: f32, playhead: f64) {
    let p = ui.painter_at(rect);
    p.rect_filled(rect, CornerRadius::ZERO, theme::BG_PANEL);
    let track = rect.shrink2(vec2(8.0, 4.0));
    p.rect_filled(track, CornerRadius::same(5), theme::BG_DEEP);
    let view_secs = (view_w / app.view.px_per_sec) as f64;
    let total = app.project_end_secs().max(app.view.scroll_secs + view_secs).max(10.0) * 1.02;
    let start = app.view.scroll_secs.min(0.0);
    let span = total - start;
    let x_of = |t: f64| track.left() + ((t - start) / span) as f32 * track.width();

    for t in &app.tracks {
        let a = x_of(t.def.offset_secs);
        let b = x_of(t.end_secs()).max(a + 1.0);
        let r = Rect::from_min_max(pos2(a, track.top() + 3.0), pos2(b, track.bottom() - 3.0));
        p.rect_filled(r, CornerRadius::same(2), theme::rgb(t.def.color).gamma_multiply(0.55));
    }
    for m in &app.project.markers {
        p.vline(x_of(m.time_secs), track.y_range(), Stroke::new(1.0, theme::rgb(m.color)));
    }
    let win = Rect::from_min_max(
        pos2(x_of(app.view.scroll_secs).max(track.left()), track.top()),
        pos2(
            x_of(app.view.scroll_secs + view_secs).min(track.right()).max(x_of(app.view.scroll_secs) + 8.0),
            track.bottom(),
        ),
    );
    let resp = ui.interact(rect, ui.id().with("overview"), Sense::click_and_drag());
    let active = resp.hovered() || resp.dragged();
    p.rect_filled(win, CornerRadius::same(5), Color32::from_white_alpha(if active { 34 } else { 20 }));
    p.rect_stroke(win, CornerRadius::same(5), Stroke::new(1.0, Color32::from_white_alpha(70)), StrokeKind::Inside);
    p.vline(x_of(playhead), track.y_range(), Stroke::new(1.0, theme::PLAYHEAD));

    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.clicked() || resp.dragged() {
            // Centre the view on the pointer.
            let t = start + ((pos.x - track.left()) / track.width()) as f64 * span;
            app.view.scroll_secs = t - view_secs / 2.0;
        }
    }
}

/// Darkens the timeline before the project start.
fn shade_before_start(p: &egui::Painter, rect: Rect, map: Map) {
    let x = map.x(0.0);
    if x > rect.left() {
        let r = Rect::from_min_max(rect.left_top(), pos2(x.min(rect.right()), rect.bottom()));
        p.rect_filled(r, CornerRadius::ZERO, Color32::from_black_alpha(60));
    }
}

/// Index of the track row under `pos`, if any.
fn track_at(app: &CueLineApp, lanes: Rect, pos: Pos2) -> Option<usize> {
    let i = ((pos.y - lanes.top() + app.view.scroll_y) / app.view.track_height).floor();
    (i >= 0.0 && (i as usize) < app.tracks.len()).then_some(i as usize)
}

fn lanes_interaction(app: &mut CueLineApp, ui: &mut Ui, lanes: Rect, map: Map) {
    let resp = ui.interact(lanes, ui.id().with("lanes"), Sense::click_and_drag());
    let rate = app.project.frame_rate;
    let shift = ui.input(|i| i.modifiers.shift);

    if resp.drag_started() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let t = map.t(pos.x);
            if let Some(i) = track_at(app, lanes, pos) {
                let tr = &app.tracks[i];
                let (id, orig, end) = (tr.id, tr.def.offset_secs, tr.end_secs());
                app.view.selected_track = Some(id);
                if t >= orig && t <= end.max(orig + 0.01) {
                    app.checkpoint();
                    app.view.drag = Some(Drag::Clip { id, orig, grab: t });
                }
            }
        }
    }
    if resp.dragged() {
        if let (Some(Drag::Clip { id, orig, grab }), Some(pos)) = (app.view.drag, resp.interact_pointer_pos()) {
            let mut offset = orig + map.t(pos.x) - grab;
            if app.settings.snap_to_frames != shift {
                offset = snap_to_frame(offset, rate);
            }
            if let Some(t) = app.tracks.iter_mut().find(|t| t.id == id) {
                t.def.offset_secs = offset;
            }
            app.track_changed(id);
        }
    }
    if resp.drag_stopped() {
        app.view.drag = None;
    }
    if resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            app.view.selected_track = track_at(app, lanes, pos).map(|i| app.tracks[i].id);
            let mut t = map.t(pos.x);
            if app.settings.snap_to_frames != shift {
                t = snap_to_frame(t, rate);
            }
            app.seek(t);
        }
    }
    if resp.secondary_clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            app.view.context_track = track_at(app, lanes, pos).map(|i| app.tracks[i].id);
            app.view.selected_track = app.view.context_track;
        }
    }
    if let Some(Drag::Clip { .. }) = app.view.drag {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    resp.context_menu(|ui| super::menus::track_context_menu(app, ui));
}

fn marker_at(app: &CueLineApp, map: Map, x: f32) -> Option<usize> {
    app.project
        .markers
        .iter()
        .enumerate()
        .filter(|(_, m)| (map.x(m.time_secs) - x).abs() < 6.0)
        .min_by(|a, b| {
            let da = (map.x(a.1.time_secs) - x).abs();
            let db = (map.x(b.1.time_secs) - x).abs();
            da.total_cmp(&db)
        })
        .map(|(i, _)| i)
}

fn ruler_interaction(app: &mut CueLineApp, ui: &mut Ui, ruler: Rect, map: Map) {
    let resp = ui.interact(ruler, ui.id().with("ruler"), Sense::click_and_drag());
    let rate = app.project.frame_rate;
    let snap = app.settings.snap_to_frames != ui.input(|i| i.modifiers.shift);
    let pos = resp.interact_pointer_pos();

    if resp.drag_started() {
        if let Some(pos) = pos {
            app.view.drag = match marker_at(app, map, pos.x) {
                Some(idx) => {
                    app.checkpoint();
                    app.view.selected_marker = Some(idx);
                    Some(Drag::Marker { idx, orig: app.project.markers[idx].time_secs, grab: map.t(pos.x) })
                }
                None => Some(Drag::Scrub),
            };
        }
    }
    if resp.dragged() {
        if let Some(pos) = pos {
            match app.view.drag {
                Some(Drag::Marker { idx, orig, grab }) => {
                    let mut t = (orig + map.t(pos.x) - grab).max(0.0);
                    if snap {
                        t = snap_to_frame(t, rate);
                    }
                    if let Some(m) = app.project.markers.get_mut(idx) {
                        m.time_secs = t;
                        app.dirty = true;
                    }
                }
                Some(Drag::Scrub) => {
                    let mut t = map.t(pos.x);
                    if snap {
                        t = snap_to_frame(t, rate);
                    }
                    app.seek(t);
                }
                _ => {}
            }
        }
    }
    if resp.drag_stopped() {
        if let Some(Drag::Marker { .. }) = app.view.drag {
            app.sort_markers();
        }
        app.view.drag = None;
    }
    if resp.double_clicked() {
        if let Some(idx) = pos.and_then(|p| marker_at(app, map, p.x)) {
            app.view.selected_marker = Some(idx);
            app.ui.rename_marker = Some((idx, app.project.markers[idx].name.clone()));
        }
    } else if resp.clicked() {
        if let Some(pos) = pos {
            match marker_at(app, map, pos.x) {
                Some(idx) => {
                    app.view.selected_marker = Some(idx);
                    let t = app.project.markers[idx].time_secs;
                    app.seek(t);
                }
                None => {
                    let mut t = map.t(pos.x);
                    if snap {
                        t = snap_to_frame(t, rate);
                    }
                    app.seek(t);
                }
            }
        }
    }
    if resp.secondary_clicked() {
        app.view.context_marker = pos.and_then(|p| marker_at(app, map, p.x));
        if let Some(p) = pos {
            app.view.context_time = map.t(p.x);
        }
    }
    resp.context_menu(|ui| super::menus::ruler_context_menu(app, ui, map));
    if resp.hovered() && pos.is_none() {
        if let Some(h) = ui.input(|i| i.pointer.hover_pos()) {
            if marker_at(app, map, h.x).is_some() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
        }
    }
}
