use std::time::Duration;

use egui::{vec2, Color32, CornerRadius, FontId, Frame, Margin, RichText, Stroke, Ui};
use serde_json::{Map, Value};
use yaist_core::{Output, ParamKind, Risk, Table};
use yaist_ui::{row_action_is_direct, row_args, tone_of, Cmd, Entry, Form, LlmStatus, ModuleView, Panel, Plan, PlanStatus, Session};

use crate::theme::{self, Tokens};
use crate::widgets::{self, Kind};

const SIDEBAR_W: f32 = 264.0;
const ROW_H: f32 = 40.0;

/// What the user did this frame; applied once drawing is done.
enum Act {
    Select(usize),
    Refresh(usize),
    Propose(&'static str, Map<String, Value>),
    Form(Form),
    Confirm(u64),
    Cancel(u64),
    Submit(String),
}

/// What one frame draws, copied out of the shared state so the backend isn't
/// blocked while we lay out.
struct View {
    modules: Vec<ModuleView>,
    selected: usize,
    conversation: Vec<Entry>,
    busy: Option<String>,
    llm: LlmStatus,
    llm_url: String,
    distro: String,
    pending: bool,
}

pub struct App {
    session: Session,
    input: String,
    form: Option<Form>,
    acts: Vec<Act>,
    shot: Option<Shot>,
}

/// `YAIST_SCREENSHOT=file.ppm`: save one frame after `YAIST_SCREENSHOT_DELAY`
/// seconds and quit. `YAIST_MODULE` (number) and `YAIST_DEMO` (a request)
/// set the scene. For docs and visual checks.
struct Shot {
    path: String,
    after: Duration,
    started: std::time::Instant,
    requested: bool,
}

impl App {
    pub fn new(session: Session) -> Self {
        if let Some(i) = std::env::var("YAIST_MODULE").ok().and_then(|m| m.parse::<usize>().ok()) {
            session.select(i.saturating_sub(1));
        }
        if let Ok(demo) = std::env::var("YAIST_DEMO") {
            session.send(Cmd::Submit(demo));
        }
        let shot = std::env::var("YAIST_SCREENSHOT").ok().map(|path| Shot {
            path,
            after: Duration::from_secs_f32(std::env::var("YAIST_SCREENSHOT_DELAY").ok().and_then(|d| d.parse().ok()).unwrap_or(4.0)),
            started: std::time::Instant::now(),
            requested: false,
        });
        Self { session, input: String::new(), form: None, acts: Vec::new(), shot }
    }

    fn screenshot(&mut self, ctx: &egui::Context) {
        let Some(shot) = &mut self.shot else { return };
        ctx.request_repaint_after(Duration::from_millis(200));
        if !shot.requested && shot.started.elapsed() >= shot.after {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            shot.requested = true;
        }
        let image = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = image {
            let mut ppm = format!("P6\n{} {}\n255\n", img.size[0], img.size[1]).into_bytes();
            for p in &img.pixels {
                ppm.extend_from_slice(&[p.r(), p.g(), p.b()]);
            }
            if let Err(e) = std::fs::write(&shot.path, ppm) {
                eprintln!("writing {}: {e}", shot.path);
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn view(&self) -> View {
        let s = self.session.state();
        View {
            modules: s.modules.clone(),
            selected: s.selected,
            conversation: s.conversation.clone(),
            busy: s.busy.clone(),
            llm: s.llm.clone(),
            llm_url: s.llm_url.clone(),
            distro: if s.distro.pretty.is_empty() { s.distro.id.clone() } else { s.distro.pretty.clone() },
            pending: s.pending_plan().is_some(),
        }
    }

    fn apply(&mut self) {
        for act in std::mem::take(&mut self.acts) {
            match act {
                Act::Select(i) => self.session.select(i),
                Act::Refresh(i) => self.session.send(Cmd::Refresh(i)),
                Act::Propose(action, raw) => self.session.send(Cmd::Propose { action, raw }),
                Act::Form(f) => self.form = Some(f),
                Act::Confirm(id) => self.session.send(Cmd::Confirm(id)),
                Act::Cancel(id) => self.session.send(Cmd::Cancel(id)),
                Act::Submit(text) => self.session.send(Cmd::Submit(text)),
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let t = Tokens::get(&ctx);
        let v = self.view();
        if v.busy.is_some() {
            ctx.request_repaint_after(Duration::from_millis(80));
        }

        egui::Panel::top("header").frame(Frame::new().fill(t.bg)).exact_size(theme::HEADER_H).resizable(false).show(ui, |ui| self.header(ui, &v));
        egui::Panel::bottom("footer")
            .frame(Frame::new().fill(t.bg).inner_margin(Margin::symmetric(20, 0)))
            .exact_size(theme::FOOTER_H)
            .resizable(false)
            .show(ui, |ui| footer(ui, &v));
        egui::Panel::left("modules").frame(Frame::new().fill(t.bg)).exact_size(SIDEBAR_W).resizable(false).show(ui, |ui| self.sidebar(ui, &v));
        egui::Panel::right("conversation")
            .frame(Frame::new().fill(t.bg))
            .default_size(470.0)
            .size_range(380.0..=760.0)
            .resizable(true)
            .show(ui, |ui| self.conversation(ui, &v));
        egui::CentralPanel::default()
            .frame(Frame::new().fill(t.bg).inner_margin(Margin { left: 28, right: 28, top: 22, bottom: 0 }))
            .show(ui, |ui| self.module_page(ui, &v));

        self.form_dialog(&ctx);
        widgets::window_edges(&ctx);
        self.apply();
        self.screenshot(&ctx);
    }
}

// ---- header and footer ---------------------------------------------------------------------------

impl App {
    fn header(&mut self, ui: &mut Ui, v: &View) {
        let t = Tokens::get(ui.ctx());
        widgets::title_bar_drag(ui);
        let r = ui.max_rect();
        ui.painter().hline(r.x_range(), r.bottom() - 0.5, t.hairline());
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(22.0);
            wordmark(ui, 21.0);
            ui.add_space(20.0);
            widgets::rule(ui, theme::HEADER_H);
            ui.add_space(18.0);
            widgets::tag(ui, "System", t.invert_bg, t.invert_fg);
            ui.add_space(12.0);
            widgets::hud(ui, &v.distro, t.muted);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                widgets::window_buttons(ui, theme::HEADER_H);
                let dark = ui.ctx().theme() == egui::Theme::Dark;
                if widgets::theme_toggle(ui, dark, theme::HEADER_H).clicked() {
                    ui.ctx().set_theme(if dark { egui::ThemePreference::Light } else { egui::ThemePreference::Dark });
                }
                widgets::rule(ui, theme::HEADER_H);
                ui.add_space(16.0);
                match &v.llm {
                    LlmStatus::Ready(name) => widgets::badge(ui, &format!("model · {name}"), t.good),
                    LlmStatus::Checking => widgets::badge(ui, "model · checking", t.muted),
                    LlmStatus::Offline => widgets::badge(ui, "model · offline", t.warn),
                };
                ui.add_space(10.0);
                widgets::hud(ui, "local ai", t.faint);
            });
        });
    }
}

fn footer(ui: &mut Ui, v: &View) {
    let t = Tokens::get(ui.ctx());
    let r = ui.max_rect();
    ui.painter().hline(r.x_range(), r.top() + 0.5, t.hairline());
    ui.horizontal_centered(|ui| {
        widgets::hud(ui, "yAIst 0.1 · prototype · changes ask polkit", t.faint);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            widgets::hud(ui, &format!("llm · {}", v.llm_url.trim_start_matches("http://")), t.faint);
        });
    });
}

/// "yAIst" with the AI in Geeko green.
fn wordmark(ui: &mut Ui, size: f32) {
    let t = Tokens::get(ui.ctx());
    let mut job = egui::text::LayoutJob::default();
    for (text, color) in [("y", t.ink_strong), ("AI", t.accent), ("st", t.ink_strong)] {
        job.append(text, 0.0, egui::TextFormat { font_id: FontId::new(size, theme::display()), color, ..Default::default() });
    }
    ui.label(job);
}

// ---- sidebar -------------------------------------------------------------------------------------

impl App {
    fn sidebar(&mut self, ui: &mut Ui, v: &View) {
        let t = Tokens::get(ui.ctx());
        let full = ui.max_rect();
        ui.painter().vline(full.right() - 0.5, full.y_range(), t.hairline());
        ui.add_space(22.0);
        ui.horizontal(|ui| {
            ui.add_space(22.0);
            widgets::eyebrow(ui, &format!("{:02}", v.modules.len()), "Modules");
        });
        ui.add_space(14.0);
        let width = SIDEBAR_W - 1.0;
        let top = ui.cursor().top();
        ui.painter().hline(full.left()..=full.left() + width, top, t.hairline());
        for (i, m) in v.modules.iter().enumerate() {
            let (rect, resp) = ui.allocate_exact_size(vec2(width, 66.0), egui::Sense::click());
            let selected = i == v.selected;
            let p = ui.painter();
            if selected || resp.hovered() {
                p.rect_filled(rect, CornerRadius::ZERO, t.sunken);
            }
            if selected {
                p.rect_filled(egui::Rect::from_min_size(rect.min, vec2(3.0, rect.height())), CornerRadius::ZERO, t.module(i));
            }
            p.hline(rect.x_range(), rect.bottom() - 0.5, t.hairline());
            let galley = widgets::caps(ui, &format!("{:02}", i + 1), theme::HUD_SIZE, theme::mono_bold(), theme::HUD_SPACING, t.invert_fg);
            let tag = egui::Rect::from_min_size(rect.min + vec2(22.0, 14.0), galley.size() + vec2(14.0, 8.0));
            p.rect_filled(tag, CornerRadius::ZERO, t.module(i));
            p.galley_with_override_text_color(tag.center() - galley.size() / 2.0, galley, t.invert_fg);
            p.text(
                egui::pos2(tag.right() + 12.0, tag.center().y),
                egui::Align2::LEFT_CENTER,
                m.spec.title,
                FontId::new(15.0, theme::semibold()),
                if selected { t.ink_strong } else { t.ink },
            );
            p.text(
                egui::pos2(rect.left() + 22.0, rect.top() + 47.0),
                egui::Align2::LEFT_CENTER,
                m.spec.tagline,
                FontId::new(12.0, egui::FontFamily::Proportional),
                t.muted,
            );
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.acts.push(Act::Select(i));
            }
        }
    }
}

// ---- module page ---------------------------------------------------------------------------------

impl App {
    fn module_page(&mut self, ui: &mut Ui, v: &View) {
        let t = Tokens::get(ui.ctx());
        let i = v.selected;
        let m = &v.modules[i];
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            widgets::tag(ui, &format!("{:02}", i + 1), t.module(i), t.invert_fg);
            widgets::eyebrow(ui, "module", m.spec.title);
            if let Panel::Ready(out) = &m.panel {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    widgets::hud(ui, &format!("{} items", yaist_ui::output_rows(out)), t.faint);
                });
            }
        });
        ui.add_space(12.0);
        ui.label(RichText::new(m.spec.title).font(FontId::new(32.0, theme::display())).color(t.ink_strong));
        ui.label(RichText::new(m.spec.tagline).size(14.0).color(t.muted));
        ui.add_space(16.0);

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            for (k, a) in m.actions.iter().enumerate() {
                let kind = if k == 0 { Kind::Primary } else { Kind::Secondary };
                if widgets::button(ui, a.title, kind, 0.0).on_hover_text(a.description).clicked() {
                    if a.params.is_empty() {
                        self.acts.push(Act::Propose(a.id, Map::new()));
                    } else {
                        self.acts.push(Act::Form(Form::new(*a, &Map::new())));
                    }
                }
            }
            if widgets::button(ui, "Refresh", Kind::Secondary, 0.0).on_hover_text("Reload this panel").clicked() {
                self.acts.push(Act::Refresh(i));
            }
        });
        ui.add_space(18.0);

        match &m.panel {
            Panel::Loading => {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(14.0).color(t.muted));
                    widgets::hud(ui, "loading", t.muted);
                });
            }
            Panel::Failed(e) => {
                Frame::new().stroke(Stroke::new(1.0, t.bad)).inner_margin(Margin::symmetric(14, 12)).show(ui, |ui| {
                    ui.label(RichText::new(e).color(t.bad));
                });
            }
            Panel::Ready(Output::Table(table)) => self.table(ui, m, table),
            Panel::Ready(Output::Facts(facts)) => facts_grid(ui, facts, 16.0),
            Panel::Ready(Output::Text(s) | Output::Done(s)) => {
                ui.label(s);
            }
        }
    }

    fn table(&mut self, ui: &mut Ui, m: &ModuleView, table: &Table) {
        let t = Tokens::get(ui.ctx());
        let visible_actions = m.row_actions.len().min(2);
        let actions_w =
            if m.row_actions.is_empty() { 0.0 } else { visible_actions as f32 * 90.0 + if m.row_actions.len() > 2 { 40.0 } else { 0.0 } + 8.0 };
        let full_w = ui.available_width();
        let cols_w = full_w - actions_w - 28.0;
        // Badge columns get room for their longest label; the other columns
        // share what is left by how long their content is.
        let longest = |c: usize| table.rows.iter().take(300).map(|r| r[c].chars().count()).max().unwrap_or(0).max(table.columns[c].title.len());
        let is_badge = |c: usize| yaist_ui::is_status_column(table.columns[c].key);
        let fixed: f32 = (0..table.columns.len()).filter(|&c| is_badge(c)).map(|c| longest(c) as f32 * 7.6 + 26.0).sum();
        let weights: Vec<f32> = (0..table.columns.len()).map(|c| if is_badge(c) { 0.0 } else { longest(c).clamp(4, 40) as f32 + 1.0 }).collect();
        let total: f32 = weights.iter().sum::<f32>().max(1.0);
        let widths: Vec<f32> = (0..table.columns.len())
            .map(|c| if is_badge(c) { longest(c) as f32 * 7.6 + 26.0 } else { (cols_w - fixed).max(80.0) * weights[c] / total })
            .collect();

        let (head, _) = ui.allocate_exact_size(vec2(full_w, 34.0), egui::Sense::hover());
        let p = ui.painter();
        p.hline(head.x_range(), head.top() + 0.5, Stroke::new(1.0, t.line_strong));
        p.hline(head.x_range(), head.bottom() - 0.5, t.hairline());
        let mut x = head.left() + 14.0;
        for (col, w) in table.columns.iter().zip(&widths) {
            let g = widgets::caps(ui, col.title, theme::HUD_SIZE, theme::mono(), theme::HUD_SPACING, t.faint);
            ui.painter().galley(egui::pos2(x, head.center().y - g.size().y / 2.0), g, t.faint);
            x += w;
        }

        if table.rows.is_empty() {
            ui.add_space(18.0);
            widgets::hud(ui, "  nothing here", t.faint);
            return;
        }
        egui::ScrollArea::vertical().auto_shrink(false).show_rows(ui, ROW_H, table.rows.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for row in &table.rows[range] {
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), egui::Sense::hover());
                if resp.contains_pointer() {
                    ui.painter().rect_filled(rect, CornerRadius::ZERO, t.sunken);
                }
                ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, t.hairline());
                let mut x = rect.left() + 14.0;
                for (c, (value, w)) in row.iter().zip(&widths).enumerate() {
                    let cell = egui::Rect::from_min_size(egui::pos2(x, rect.top()), vec2((w - 10.0).max(10.0), ROW_H));
                    x += w;
                    let mut cu = ui.new_child(egui::UiBuilder::new().max_rect(cell).layout(egui::Layout::left_to_right(egui::Align::Center)));
                    cu.set_clip_rect(cell.intersect(ui.clip_rect()));
                    if yaist_ui::is_status_column(table.columns[c].key) && !value.is_empty() {
                        widgets::badge(&mut cu, value, t.tone(tone_of(value)));
                    } else {
                        let text =
                            if c == 0 { RichText::new(value).family(theme::medium()).color(t.ink_strong) } else { RichText::new(value).color(t.ink) };
                        cu.add(egui::Label::new(text).truncate());
                    }
                }
                if !m.row_actions.is_empty() {
                    let area = egui::Rect::from_min_max(egui::pos2(rect.right() - actions_w, rect.top()), rect.max - vec2(8.0, 0.0));
                    let mut au = ui.new_child(egui::UiBuilder::new().max_rect(area).layout(egui::Layout::right_to_left(egui::Align::Center)));
                    au.spacing_mut().item_spacing.x = 6.0;
                    if m.row_actions.len() > 2 {
                        au.menu_button(RichText::new("···").color(t.muted), |ui| {
                            for a in &m.row_actions[2..] {
                                if ui.button(a.title).clicked() {
                                    self.row_action(a, table, row);
                                    ui.close();
                                }
                            }
                        });
                    }
                    for a in m.row_actions[..visible_actions].iter().rev() {
                        if widgets::button_sized(&mut au, a.title, Kind::Secondary, 84.0, 26.0).on_hover_text(a.description).clicked() {
                            self.row_action(a, table, row);
                        }
                    }
                }
            }
        });
    }

    fn row_action(&mut self, a: &yaist_core::ActionSpec, table: &Table, row: &[String]) {
        let raw = row_args(a, table, row);
        if row_action_is_direct(a, table) {
            self.acts.push(Act::Propose(a.id, raw));
        } else {
            self.acts.push(Act::Form(Form::new(*a, &raw)));
        }
    }
}

fn facts_grid(ui: &mut Ui, facts: &[(String, String)], gap: f32) {
    let t = Tokens::get(ui.ctx());
    Frame::new().stroke(t.hairline()).inner_margin(Margin::symmetric(16, 14)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::Grid::new(ui.next_auto_id()).num_columns(2).spacing(vec2(24.0, gap / 2.0 + 4.0)).show(ui, |ui| {
            for (k, val) in facts {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| widgets::hud(ui, k, t.faint));
                if val.contains('\n') {
                    ui.label(RichText::new(val).monospace().size(12.0).color(t.ink));
                } else {
                    ui.label(RichText::new(val).color(t.ink_strong));
                }
                ui.end_row();
            }
        });
    });
}

// ---- conversation --------------------------------------------------------------------------------

impl App {
    fn conversation(&mut self, ui: &mut Ui, v: &View) {
        let t = Tokens::get(ui.ctx());
        let full = ui.max_rect();
        ui.painter().vline(full.left() + 0.5, full.y_range(), t.hairline());

        egui::Panel::top("conv-head").frame(Frame::new().inner_margin(Margin { left: 20, right: 16, top: 18, bottom: 12 })).show(ui, |ui| {
            ui.horizontal(|ui| {
                widgets::eyebrow(ui, "ai", "Conversation");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::button_sized(ui, "Clear", Kind::Secondary, 0.0, 26.0).clicked() {
                        self.acts.push(Act::Submit("/clear".into()));
                    }
                });
            });
        });

        egui::Panel::bottom("conv-input").frame(Frame::new().inner_margin(Margin { left: 20, right: 16, top: 10, bottom: 16 })).show(ui, |ui| {
            if let Some(what) = &v.busy {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().size(12.0).color(t.accent));
                    widgets::hud(ui, what, t.muted);
                });
                ui.add_space(6.0);
            } else if v.pending {
                widgets::hud(ui, "a change is waiting for you above", t.warn);
                ui.add_space(6.0);
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let (send_w, h) = (84.0, theme::CONTROL_H + 6.0);
                let (field, _) = ui.allocate_exact_size(vec2(ui.available_width() - send_w - 8.0, h), egui::Sense::hover());
                ui.painter().rect(field, CornerRadius::ZERO, t.sunken, Stroke::new(1.0, t.line_strong), egui::StrokeKind::Inside);
                let mut fu = ui.new_child(
                    egui::UiBuilder::new().max_rect(field.shrink2(vec2(12.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                let edit = fu.add(
                    egui::TextEdit::singleline(&mut self.input)
                        .hint_text(RichText::new("Ask yAIst, or type /help").color(t.faint))
                        .frame(Frame::NONE)
                        .desired_width(field.width() - 24.0)
                        .font(FontId::new(14.0, egui::FontFamily::Proportional)),
                );
                let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let send = widgets::button_sized(ui, "Send", Kind::Primary, send_w, h).clicked();
                if (enter || send) && !self.input.trim().is_empty() {
                    self.acts.push(Act::Submit(std::mem::take(&mut self.input)));
                    edit.request_focus();
                }
            });
        });

        egui::CentralPanel::default().frame(Frame::new().inner_margin(Margin { left: 20, right: 12, top: 0, bottom: 0 })).show(ui, |ui| {
            egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink(false).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                for e in &v.conversation {
                    self.entry(ui, e);
                    ui.add_space(10.0);
                }
            });
        });
    }

    fn entry(&mut self, ui: &mut Ui, e: &Entry) {
        let t = Tokens::get(ui.ctx());
        match e {
            Entry::User(s) => {
                widgets::hud(ui, "you", t.faint);
                Frame::new().fill(t.sunken).inner_margin(Margin::symmetric(12, 10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(s).size(14.0).color(t.ink_strong));
                });
            }
            Entry::Command(s) => {
                Frame::new().fill(t.sunken).stroke(t.hairline()).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(format!("› {s}")).family(theme::mono()).size(13.0).color(t.ink_strong));
                });
            }
            Entry::AgentRan { command, ok } => {
                let (mark, color) = if *ok { ("✓", t.faint) } else { ("✗", t.bad) };
                ui.label(RichText::new(format!("{mark} ran {command}")).family(theme::mono()).size(12.0).color(color));
            }
            Entry::Assistant { text, elapsed } => {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    wordmark(ui, 13.0);
                    ui.add_space(10.0);
                    widgets::hud(ui, &format!("{:.1} s", elapsed.as_secs_f32()), t.faint);
                });
                ui.label(RichText::new(text).size(14.0).color(t.ink));
            }
            Entry::Plan(p) => self.plan_card(ui, p),
            Entry::Result { title, output } => {
                widgets::hud(ui, title, t.faint);
                match output {
                    Output::Table(table) => compact_table(ui, table),
                    Output::Facts(f) => facts_grid(ui, f, 4.0),
                    Output::Text(s) | Output::Done(s) => {
                        let text = if s.contains('\n') { RichText::new(s).monospace().size(12.0) } else { RichText::new(s) };
                        ui.label(text.color(t.ink));
                    }
                }
            }
            Entry::Error(s) => {
                ui.label(RichText::new(format!("✗ {s}")).color(t.bad));
            }
            Entry::Notice(s) => {
                let text = if s.contains('\n') { RichText::new(s).family(theme::mono()).size(12.0) } else { RichText::new(s).size(13.0) };
                ui.label(text.color(t.muted));
            }
        }
    }

    /// A proposed change. Everything on it comes from the action's definition
    /// and validated arguments, never from model-written text.
    fn plan_card(&mut self, ui: &mut Ui, p: &Plan) {
        let t = Tokens::get(ui.ctx());
        let risk = t.risk(p.spec.risk);
        let stroke = if p.status == PlanStatus::Pending { Stroke::new(1.0, t.line_strong) } else { t.hairline() };
        Frame::new().fill(t.bg).stroke(stroke).inner_margin(Margin::same(14)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                widgets::tag(ui, "plan", risk, t.invert_fg);
                widgets::badge(ui, p.spec.risk.label(), risk);
                ui.label(RichText::new(p.spec.title).family(theme::semibold()).size(16.0).color(t.ink_strong));
            });
            ui.add_space(6.0);
            Frame::new().fill(t.sunken).inner_margin(Margin::symmetric(10, 7)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(&p.command).family(theme::mono()).size(12.5).color(t.ink_strong));
            });
            if !p.spec.params.is_empty() {
                ui.add_space(4.0);
                egui::Grid::new(("plan", p.id)).num_columns(2).spacing(vec2(16.0, 4.0)).show(ui, |ui| {
                    for param in p.spec.params {
                        widgets::hud(ui, &param.name.replace('_', " "), t.faint);
                        let v = p.args.display(param.name);
                        ui.label(RichText::new(if v.is_empty() { "default".into() } else { v }).color(t.ink));
                        ui.end_row();
                    }
                });
            }
            ui.add_space(4.0);
            ui.label(RichText::new(p.spec.description).size(12.5).color(t.muted));
            ui.add_space(8.0);
            match &p.status {
                PlanStatus::Pending => {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let kind = if p.spec.risk == Risk::Dangerous { Kind::Danger } else { Kind::Primary };
                        if widgets::button(ui, "Confirm", kind, 120.0).clicked() {
                            self.acts.push(Act::Confirm(p.id));
                        }
                        if widgets::button(ui, "Edit", Kind::Secondary, 0.0).clicked() {
                            self.acts.push(Act::Form(Form::edit(p)));
                        }
                        if widgets::button(ui, "Cancel", Kind::Secondary, 0.0).clicked() {
                            self.acts.push(Act::Cancel(p.id));
                        }
                    });
                }
                PlanStatus::Running => {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().size(12.0).color(t.muted));
                        widgets::hud(ui, "running", t.muted);
                    });
                }
                PlanStatus::Done(m) => status_line(ui, "done", t.good, m, t.ink),
                PlanStatus::Failed(m) => status_line(ui, "failed", t.bad, m, t.bad),
                PlanStatus::Cancelled => status_line(ui, "cancelled", t.faint, "", t.faint),
            }
        });
    }

    fn form_dialog(&mut self, ctx: &egui::Context) {
        let Some(form) = &mut self.form else { return };
        let t = Tokens::get(ctx);
        let modal = egui::Modal::new(egui::Id::new("form").with(form.spec.id))
            .frame(Frame::new().fill(t.bg).stroke(Stroke::new(1.0, t.line_strong)).shadow(theme::shadow(&t)).inner_margin(theme::PAD as i8));
        let mut submit = false;
        let mut close = false;
        let resp = modal.show(ctx, |ui| {
            ui.set_width(460.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                ui.label(RichText::new(form.spec.title).font(FontId::new(22.0, theme::display())).color(t.ink_strong));
                widgets::badge(ui, form.spec.risk.label(), t.risk(form.spec.risk));
            });
            ui.add_space(4.0);
            ui.label(RichText::new(form.spec.description).size(13.0).color(t.muted));
            ui.add_space(14.0);
            for (i, p) in form.spec.params.iter().enumerate() {
                let label = format!("{}{}", p.name.replace('_', " "), if p.required { "" } else { " · optional" });
                widgets::hud(ui, &label, t.faint);
                ui.add_space(2.0);
                match p.kind {
                    ParamKind::Bool => {
                        let mut on = form.values[i] == "true";
                        if ui.checkbox(&mut on, RichText::new(p.help).color(t.ink)).changed() {
                            form.values[i] = on.to_string();
                        }
                    }
                    ParamKind::Choice(opts) => {
                        egui::ComboBox::from_id_salt(("choice", p.name)).selected_text(form.values[i].clone()).width(220.0).show_ui(ui, |ui| {
                            for o in opts {
                                ui.selectable_value(&mut form.values[i], (*o).to_owned(), *o);
                            }
                        });
                        ui.label(RichText::new(p.help).size(12.0).color(t.faint));
                    }
                    ParamKind::Text | ParamKind::Integer => {
                        let edit = ui.add(egui::TextEdit::singleline(&mut form.values[i]).desired_width(f32::INFINITY).margin(vec2(10.0, 8.0)));
                        if i == 0 && !ui.ctx().memory(|m| m.has_focus(edit.id)) && form.values[i].is_empty() {
                            edit.request_focus();
                        }
                        if edit.lost_focus() && ui.input(|inp| inp.key_pressed(egui::Key::Enter)) {
                            submit = true;
                        }
                        ui.label(RichText::new(p.help).size(12.0).color(t.faint));
                    }
                }
                ui.add_space(10.0);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if widgets::button(ui, "Review", Kind::Primary, 120.0).clicked() {
                    submit = true;
                }
                if widgets::button(ui, "Cancel", Kind::Secondary, 0.0).clicked() {
                    close = true;
                }
            });
        });
        if submit {
            if let Some(f) = self.form.take() {
                f.submit(&self.session);
            }
        } else if close || resp.should_close() {
            self.form = None;
        }
    }
}

fn status_line(ui: &mut Ui, label: &str, color: Color32, msg: &str, msg_color: Color32) {
    ui.horizontal_wrapped(|ui| {
        widgets::badge(ui, label, color);
        if !msg.is_empty() {
            ui.label(RichText::new(msg).size(13.0).color(msg_color));
        }
    });
}

/// A result table in the conversation: the first rows, in mono.
fn compact_table(ui: &mut Ui, table: &Table) {
    let t = Tokens::get(ui.ctx());
    const MAX: usize = 12;
    Frame::new().stroke(t.hairline()).inner_margin(Margin::symmetric(10, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::ScrollArea::horizontal().id_salt(ui.next_auto_id()).show(ui, |ui| {
            egui::Grid::new(ui.next_auto_id()).num_columns(table.columns.len()).spacing(vec2(14.0, 3.0)).show(ui, |ui| {
                for c in &table.columns {
                    widgets::hud(ui, c.title, t.faint);
                }
                ui.end_row();
                for row in table.rows.iter().take(MAX) {
                    for (c, v) in row.iter().enumerate() {
                        let color = if yaist_ui::is_status_column(table.columns[c].key) { t.tone(tone_of(v)) } else { t.ink };
                        let short: String = if v.chars().count() > 40 { v.chars().take(39).chain(['…']).collect() } else { v.clone() };
                        ui.label(RichText::new(short).family(theme::mono()).size(12.0).color(color));
                    }
                    ui.end_row();
                }
            });
        });
        if table.rows.len() > MAX {
            widgets::hud(ui, &format!("… {} more", table.rows.len() - MAX), t.faint);
        } else if table.rows.is_empty() {
            widgets::hud(ui, "nothing", t.faint);
        }
    });
}
