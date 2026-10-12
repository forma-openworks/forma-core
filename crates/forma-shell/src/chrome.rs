//! Оформление главного окна: верхняя строка с меню и рабочими пространствами, строка
//! состояния, экран приветствия, диалоги и уведомления.
//!
//! Меню программа собирает сама из пунктов оболочки ([`Shell::file_menu_settings`],
//! [`Shell::view_menu_theme`], [`Shell::window_menu`], [`Shell::help_menu`]) и своих.

use eframe::egui::{self, Align, Align2, Color32, CornerRadius, CursorIcon, FontId, Frame, Id, Key, Layout, Margin, Rect, RichText, Sense, Stroke, Ui, vec2};
use forma_i18n::t;
use forma_ui::theme::{self, Origin, Palette};
use forma_ui::widgets;

use crate::actions;
use crate::caption::{self, BUTTONS_W};
use crate::settings::AppSettings;
use crate::shell::{Shell, ShellCmd};
use crate::workspace::Builtin;

/// Id диалога: у оболочки и программы он общий, их диалоги не бывают открыты сразу.
pub const MODAL_ID: &str = "forma-modal";
const MODAL_FADE: &str = "forma-modal-anim";

/// Картинка нужного размера в пикселях экрана — так она остаётся чёткой.
pub fn icon_texture(ctx: &egui::Context, cache: &mut Option<(u32, egui::TextureHandle)>, draw: fn(u32) -> Vec<u8>, points: f32, name: &str) -> egui::TextureHandle {
    let px = (points * ctx.pixels_per_point()).round().max(16.0) as u32;
    if let Some((size, tex)) = cache
        && *size == px
    {
        return tex.clone();
    }
    let img = egui::ColorImage::from_rgba_unmultiplied([px as usize, px as usize], &draw(px));
    let tex = ctx.load_texture(name, img, egui::TextureOptions::LINEAR);
    *cache = Some((px, tex.clone()));
    tex
}

/// Ширина диалога: желаемая, но не шире окна.
pub fn modal_width(ctx: &egui::Context, want: f32) -> f32 {
    want.min(ctx.content_rect().width() - 2.0 * 22.0 - 40.0).max(160.0)
}

/// Наибольшая высота содержимого диалога: что не помещается — прокручивается.
pub fn modal_max_height(ctx: &egui::Context) -> f32 {
    (ctx.content_rect().height() - 2.0 * 22.0 - 40.0).max(120.0)
}

/// Диалог поверх затемнения в общем стиле: плавно проявляется вместе с фоном.
///
/// `sticky` — диалог закрывается только по Escape, а не щелчком мимо (там легко потерять
/// изменения). `margin` — внутренние поля. Возвращает результат `add` и `true`, если
/// диалог пора закрыть (щелчок мимо или Escape).
pub fn modal<R>(ctx: &egui::Context, pal: &Palette, sticky: bool, margin: i8, add: impl FnOnce(&mut Ui) -> R) -> (R, bool) {
    let t = ctx.animate_bool_with_time(Id::new(MODAL_FADE), true, 0.16);
    let frame = Frame::new()
        .fill(pal.modal_bg.gamma_multiply(t))
        .stroke(Stroke::new(1.0, pal.modal_border.gamma_multiply(t)))
        .corner_radius(CornerRadius::same(14))
        .inner_margin(Margin::same(margin));
    let resp = egui::Modal::new(Id::new(MODAL_ID)).frame(frame).backdrop_color(pal.modal_backdrop.gamma_multiply(t)).show(ctx, |ui| {
        ui.multiply_opacity(t);
        add(ui)
    });
    let escape = || resp.is_top_modal && !resp.any_popup_open && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape));
    let close = if sticky { escape() } else { resp.should_close() };
    (resp.inner, close)
}

/// Сбрасывает проявление диалога, когда ни одного не открыто.
pub(crate) fn reset_modal_fade(ctx: &egui::Context) {
    ctx.animate_bool_with_time(Id::new(MODAL_FADE), false, 0.0);
}

/// Заголовок диалога.
pub fn modal_title(ui: &mut Ui, pal: &Palette, text: &str) {
    ui.label(RichText::new(text).size(18.0).family(forma_ui::fonts::semibold()).color(pal.heading));
    ui.add_space(12.0);
}

/// Пункт меню с сочетанием клавиш справа.
pub fn item(text: impl Into<String>, shortcut: String) -> egui::Button<'static> {
    egui::Button::new(text.into()).shortcut_text(shortcut)
}

/// Пункт меню со значком слева и сочетанием клавиш справа.
pub fn icon_item(ui: &Ui, icon: &str, text: impl Into<String>, shortcut: String) -> egui::Button<'static> {
    let img = forma_ui::icons::image(ui.ctx(), icon, forma_ui::icons::SIZE - 2.0, ui.visuals().widgets.inactive.fg_stroke.color);
    egui::Button::new((img, text.into())).shortcut_text(shortcut)
}

/// Меню верхней строки. В узком окне все меню сворачиваются в одно «☰».
pub struct Menus<'a> {
    ui: &'a mut Ui,
    compact: bool,
}

impl Menus<'_> {
    /// Меню `title`. `keep_open` — не закрывается от переключателей внутри (как «Вид»).
    pub fn menu(&mut self, title: &str, keep_open: bool, add: impl FnOnce(&mut Ui)) {
        if self.compact {
            self.ui.menu_button(title, add);
        } else {
            widgets::top_menu(self.ui, title, keep_open, add);
        }
    }
}

/// Меню верхней строки: `add` вызывает [`Menus::menu`] для каждого меню по порядку.
pub fn menus(ui: &mut Ui, compact: bool, add: impl FnOnce(&mut Menus)) {
    if compact {
        widgets::top_menu_icon(ui, "menu", &t!("menu-compact"), false, |ui| add(&mut Menus { ui, compact: true }));
    } else {
        add(&mut Menus { ui, compact: false });
    }
}

/// Ширина вкладки рабочего пространства с названием `title`.
fn workspace_tab_width(ui: &Ui, title: &str) -> f32 {
    ui.painter().layout_no_wrap(title.to_owned(), FontId::proportional(13.0), Color32::WHITE).size().x + 22.0
}

/// Вкладка рабочего пространства: «таблетка», выбранная подсвечена акцентом.
/// Не шире `max_width`: длинное название обрезается многоточием.
fn workspace_tab(ui: &mut Ui, pal: &Palette, id: Id, title: &str, active: bool, max_width: f32) -> egui::Response {
    let font = FontId::proportional(13.0);
    let galley = widgets::elide(ui, title, font, pal.text, max_width - 22.0);
    let size = vec2(galley.size().x + 22.0, 24.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let resp = ui.interact(rect, id, Sense::click());
    let a = ui.ctx().animate_bool_with_time(id.with("active"), active, 0.18);
    let hover = ui.ctx().animate_bool_with_time(id.with("hover"), resp.hovered(), 0.12);
    let painter = ui.painter();
    if hover > 0.0 {
        painter.rect_filled(rect, CornerRadius::same(7), pal.workspace_tab_hover.gamma_multiply(hover * (1.0 - a)));
    }
    if a > 0.0 {
        painter.rect_filled(rect, CornerRadius::same(7), pal.workspace_tab_active.gamma_multiply(a));
    }
    let color = pal.workspace_tab_text.lerp_to_gamma(pal.workspace_tab_active_text, a.max(hover * 0.6));
    painter.galley(rect.center() - galley.size() / 2.0, galley, color);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

impl<S: AppSettings, B: Builtin> Shell<S, B> {
    /// Маленький логотип для шапки.
    pub fn mark_texture(&mut self, ctx: &egui::Context, points: f32) -> egui::TextureHandle {
        icon_texture(ctx, &mut self.mark, self.spec.mark, points, "forma-mark")
    }

    /// Подробный логотип для экрана приветствия и «О программе».
    pub fn logo_texture(&mut self, ctx: &egui::Context, points: f32) -> egui::TextureHandle {
        icon_texture(ctx, &mut self.logo, self.spec.logo, points, "forma-logo")
    }

    /// Верхняя строка: логотип, меню, рабочие пространства и справа — область программы.
    ///
    /// - `menu_titles` — заголовки всех меню: по ним считается, помещаются ли они целиком;
    /// - `right_w` — ширина области программы справа (кнопки запуска и т. п.);
    /// - `menus` рисует меню (третий аргумент — меню свёрнуты в «☰»), обычно через [`menus`];
    /// - `right` рисует область программы справа налево.
    pub fn top_bar(&mut self, ui: &mut Ui, menu_titles: &[String], right_w: f32, menus: impl FnOnce(&mut Ui, &mut Self, bool), right: impl FnOnce(&mut Ui, &mut Self)) {
        let pal = self.pal.clone();
        let mark = self.mark_texture(ui.ctx(), 20.0);
        // Справа отступа нет: там вплотную к краю окна стоят кнопки окна (см. `caption`).
        let frame = Frame::new().fill(pal.menubar_bg).inner_margin(Margin { left: 8, right: 0, top: 4, bottom: 4 });
        let bar = egui::Panel::top("forma-top").frame(frame).show(ui, |ui| {
            // Вся строка — заголовок окна: за пустое место окно перетаскивают.
            let inner = ui.max_rect();
            let whole = Rect::from_min_max(inner.min - vec2(8.0, 4.0), inner.max + vec2(0.0, 4.0));
            caption::drag_area(ui, whole, Id::new("forma-caption"));
            let w = &mut ui.visuals_mut().widgets;
            w.inactive.fg_stroke.color = pal.menubar_text;
            w.hovered.fg_stroke.color = pal.menubar_text;
            w.hovered.weak_bg_fill = pal.menubar_hover;
            w.open.weak_bg_fill = pal.menubar_hover;
            egui::MenuBar::new().style(theme::menu_bar_style).config(theme::menu_config()).ui(ui, |ui| {
                // Место в строке делится по важности: значок и меню, область программы,
                // текущее рабочее пространство, остальные пространства. Что не поместилось —
                // уходит в меню: пространства в «»», а меню программы в одно «☰».
                let total = ui.available_width() - BUTTONS_W - 8.0;
                let gap = ui.spacing().item_spacing.x;
                let menus_w: f32 = menu_titles.iter().map(|t| widgets::button_width(ui, t, 0.0) + gap).sum();
                let active_title = self.layout.workspaces[self.layout.active].title();
                let tabs_min = workspace_tab_width(ui, &active_title) + 60.0;
                let logo_w = 24.0 + gap;
                let compact = logo_w + menus_w + 18.0 + tabs_min + right_w > total;
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(BUTTONS_W + 8.0);
                    ui.horizontal(|ui| right(ui, self));
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.add(egui::Image::new(egui::load::SizedTexture::new(mark.id(), vec2(20.0, 20.0))));
                        ui.add_space(4.0);
                        menus(ui, self, compact);
                        ui.add_space(if compact { 6.0 } else { 18.0 });
                        self.workspace_tabs(ui, &pal);
                    });
                });
            });
        });
        self.caption_h = bar.response.rect.height();
    }

    /// Пункты «Файл» оболочки: настройки, их экспорт и импорт, выход.
    pub fn file_menu_settings(&mut self, ui: &mut Ui) {
        if ui.add(item(t!("action-settings"), self.keys(actions::SETTINGS))).clicked() {
            self.cmds.push(ShellCmd::Settings);
        }
        ui.menu_button(t!("menu-manage-settings"), |ui| {
            if ui.add(item(t!("action-export-settings"), self.keys(actions::EXPORT_SETTINGS))).clicked() {
                self.cmds.push(ShellCmd::ExportSettings);
            }
            if ui.add(item(t!("action-import-settings"), self.keys(actions::IMPORT_SETTINGS))).clicked() {
                self.cmds.push(ShellCmd::ImportSettings);
            }
        });
        ui.separator();
        let quit = self.keys(actions::QUIT);
        if ui.add(item(t!("action-quit"), if quit.is_empty() { "Alt+F4".to_owned() } else { quit })).clicked() {
            self.cmds.push(ShellCmd::Quit);
        }
    }

    /// Пункты «Вид» оболочки: тема и масштаб. Ползунков здесь нет — они живут в настройках.
    pub fn view_menu_theme(&mut self, ui: &mut Ui) {
        ui.menu_button(t!("settings-theme"), |ui| {
            let current = self.theme_id();
            for th in self.themes.list() {
                // Встроенные темы помечены справа, как сочетание клавиш у пункта.
                let tag = if th.origin == Origin::Builtin { t!("theme-default") } else { String::new() };
                if ui.add(egui::Button::new(th.name.as_str()).shortcut_text(tag).selected(current == th.id)).clicked() {
                    self.settings.appearance.theme = th.id.clone();
                }
            }
            ui.separator();
            if ui.add(item(t!("action-quick-theme"), self.keys(actions::QUICK_THEME))).clicked() {
                self.cmds.push(ShellCmd::QuickSwitchTheme);
                ui.close();
            }
        });
        ui.separator();
        if ui.add(item(t!("action-zoom-in"), self.keys(actions::ZOOM_IN))).clicked() {
            self.cmds.push(ShellCmd::ZoomIn);
        }
        if ui.add(item(t!("action-zoom-out"), self.keys(actions::ZOOM_OUT))).clicked() {
            self.cmds.push(ShellCmd::ZoomOut);
        }
        if ui.add(item(t!("action-zoom-reset"), self.keys(actions::ZOOM_RESET))).clicked() {
            self.cmds.push(ShellCmd::ZoomReset);
        }
    }

    /// Меню «Окно»: `open_editor` — пункты подменю «Открыть редактор» программы.
    pub fn window_menu(&mut self, ui: &mut Ui, open_editor: impl FnOnce(&mut Ui)) {
        ui.menu_button(t!("menu-open-editor"), open_editor);
        ui.separator();
        if ui.add(item(t!("dock-maximize"), self.keys(actions::MAXIMIZE))).clicked() {
            self.cmds.push(ShellCmd::ToggleMaximize);
        }
        if ui.add(item(t!("dock-detach"), self.keys(actions::DETACH))).clicked() {
            self.cmds.push(ShellCmd::DetachFocused);
        }
        if ui.add_enabled(!self.layout.dock.windows.is_empty(), item(t!("action-return-windows"), self.keys(actions::RETURN_WINDOWS))).clicked() {
            self.cmds.push(ShellCmd::ReturnAllWindows);
        }
        ui.separator();
        ui.menu_button(t!("menu-new-workspace"), |ui| self.new_workspace_items(ui));
        let builtin = self.layout.workspaces[self.layout.active].builtin.is_some();
        if ui.add_enabled(builtin, item(t!("action-reset-workspace"), String::new())).clicked() {
            self.cmds.push(ShellCmd::ResetWorkspace(self.layout.active));
        }
        if ui.add(item(t!("action-reset-all-workspaces"), String::new())).clicked() {
            self.cmds.push(ShellCmd::ResetAllWorkspaces);
        }
        ui.separator();
        if ui.add(item(t!("layout-save-default"), String::new())).on_hover_text(t!("layout-save-default-hint")).clicked() {
            self.cmds.push(ShellCmd::SaveDefaultLayout);
        }
        if ui.add(item(t!("layout-apply-default"), String::new())).clicked() {
            self.cmds.push(ShellCmd::ApplyDefaultLayout);
        }
        if ui.add(item(t!("layout-reset-default"), String::new())).clicked() {
            self.cmds.push(ShellCmd::ResetDefaultLayout);
        }
    }

    /// Меню «Справка»: «Найти действие», пункты программы (`extra`) и «О программе».
    pub fn help_menu(&mut self, ui: &mut Ui, extra: impl FnOnce(&mut Ui)) {
        if ui.add(item(t!("action-find-action"), self.keys(actions::FIND_ACTION))).clicked() {
            self.cmds.push(ShellCmd::FindAction);
        }
        extra(ui);
        ui.separator();
        if ui.add(item(t!("action-about"), self.keys(actions::ABOUT))).clicked() {
            self.cmds.push(ShellCmd::About);
        }
    }

    fn new_workspace_items(&mut self, ui: &mut Ui) {
        if ui.button(t!("workspace-new-copy")).clicked() {
            self.cmds.push(ShellCmd::NewWorkspace(None));
        }
        ui.separator();
        for &b in B::all() {
            if ui.button(b.title()).clicked() {
                self.cmds.push(ShellCmd::NewWorkspace(Some(b)));
            }
        }
    }

    /// Вкладки рабочих пространств в верхней строке, как в Blender. Те, что не поместились,
    /// открываются из меню «»»; текущее пространство видно всегда.
    fn workspace_tabs(&mut self, ui: &mut Ui, pal: &Palette) {
        let count = self.layout.workspaces.len();
        let active = self.layout.active;
        let titles: Vec<String> = self.layout.workspaces.iter().map(|w| w.title()).collect();
        let gap = ui.spacing().item_spacing.x;
        let widths: Vec<f32> = titles.iter().map(|t| workspace_tab_width(ui, t)).collect();
        let plus_w = 24.0 + gap;
        let more_w = 28.0 + gap;
        let avail = ui.available_width() - plus_w;
        let mut visible = vec![true; count];
        if widths.iter().map(|w| w + gap).sum::<f32>() > avail {
            // Сначала текущее, затем остальные по порядку, пока есть место.
            let room = avail - more_w;
            let mut used = widths[active] + gap;
            for (i, w) in widths.iter().enumerate() {
                visible[i] = i == active || used + w + gap <= room;
                if i != active && visible[i] {
                    used += w + gap;
                }
            }
        }
        // Текущее пространство в совсем узком окне сжимается до оставшегося места.
        let active_max = (avail - if visible.iter().all(|v| *v) { 0.0 } else { more_w }).max(40.0);
        for i in (0..count).filter(|i| visible[*i]) {
            let max_w = if i == active { active_max } else { f32::INFINITY };
            let resp = workspace_tab(ui, pal, Id::new(("forma-workspace", i)), &titles[i], i == active, max_w);
            if resp.double_clicked() {
                self.cmds.push(ShellCmd::RenameWorkspace(i));
            } else if resp.clicked() {
                self.cmds.push(ShellCmd::SwitchWorkspace(i));
            }
            let builtin = self.layout.workspaces[i].builtin.is_some();
            theme::context_menu(&resp, |ui| {
                if ui.button(t!("workspace-rename")).clicked() {
                    self.cmds.push(ShellCmd::RenameWorkspace(i));
                }
                if ui.button(t!("workspace-duplicate")).clicked() {
                    self.cmds.push(ShellCmd::DuplicateWorkspace(i));
                }
                if ui.add_enabled(builtin, egui::Button::new(t!("action-reset-workspace"))).clicked() {
                    self.cmds.push(ShellCmd::ResetWorkspace(i));
                }
                ui.separator();
                if ui.add_enabled(count > 1, egui::Button::new(t!("workspace-delete"))).clicked() {
                    self.cmds.push(ShellCmd::DeleteWorkspace(i));
                }
            });
        }
        let hidden: Vec<usize> = (0..count).filter(|i| !visible[*i]).collect();
        if !hidden.is_empty() {
            let more = forma_ui::icons::flat_button(ui, "overflow", pal.text_muted).on_hover_text(t!("workspace-more", count = hidden.len()));
            egui::Popup::menu(&more).style(theme::menu_style).show(|ui| {
                for i in hidden {
                    if ui.button(&titles[i]).clicked() {
                        self.cmds.push(ShellCmd::SwitchWorkspace(i));
                    }
                }
            });
        }
        let plus = forma_ui::icons::flat_button(ui, "add", pal.text_muted).on_hover_text(t!("menu-new-workspace"));
        egui::Popup::menu(&plus).style(theme::menu_style).show(|ui| self.new_workspace_items(ui));
    }

    /// Строка состояния: слева — то, что рисует программа (`left`), справа — язык и
    /// видеокарта. Сведения о видеокарте уступают место левой части.
    pub fn status_bar(&mut self, ui: &mut Ui, left: impl FnOnce(&mut Ui, &mut Self)) {
        let pal = self.pal.clone();
        egui::Panel::bottom("forma-status").frame(Frame::new().fill(pal.statusbar_bg).inner_margin(Margin::symmetric(12, 3))).show(ui, |ui| {
            let small = |s: String, c: Color32| RichText::new(s).size(12.0).color(c);
            let left_id = Id::new("forma-status-left-width");
            let left_w: f32 = ui.data(|d| d.get_temp(left_id)).unwrap_or(0.0);
            let gpu_w = ui.painter().layout_no_wrap(self.gpu_info.clone(), FontId::proportional(12.0), pal.statusbar_text).size().x + 24.0;
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(small(forma_i18n::language().to_uppercase(), pal.statusbar_text)).on_hover_text(t!("settings-language"));
                    if left_w + gpu_w + 40.0 <= ui.available_width() {
                        ui.label(small("·".into(), pal.statusbar_text));
                        ui.label(small(self.gpu_info.clone(), pal.statusbar_text)).on_hover_text(t!("status-renderer"));
                    }
                    let rest = ui.available_rect_before_wrap();
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rest).layout(Layout::left_to_right(Align::Center)));
                    child.set_clip_rect(rest.shrink2(vec2(0.0, -4.0)).intersect(ui.clip_rect()));
                    left(&mut child, self);
                    let windows = self.layout.dock.windows.len();
                    if windows > 0 {
                        child.label(small("·".into(), pal.statusbar_text));
                        child.label(small(t!("status-detached-windows", count = windows), pal.statusbar_text));
                    }
                    let w = child.min_rect().width();
                    if (w - left_w).abs() > 0.5 {
                        ui.data_mut(|d| d.insert_temp(left_id, w));
                    }
                });
            });
        });
    }

    /// Экран приветствия: логотип, «FORMA», название и подпись программы (`app-tagline`),
    /// затем то, что рисует программа (`content`: большие кнопки шириной `width`, недавнее),
    /// рекомендуемые плагины, настройки и сведения о рендере.
    pub fn welcome_ui(&mut self, ui: &mut Ui, content: impl FnOnce(&mut Ui, &mut Self, f32)) {
        let pal = self.pal.clone();
        let logo = self.logo_texture(ui.ctx(), 104.0);
        egui::CentralPanel::default().frame(Frame::new().fill(pal.welcome_bg)).show(ui, |ui| {
            // Строки меню здесь нет: заголовок окна — полоса вдоль верхнего края.
            let strip = Rect::from_min_size(ui.max_rect().min, vec2(ui.max_rect().width(), caption::WELCOME_H));
            caption::drag_area(ui, strip, Id::new("forma-caption-welcome"));
            // В низком окне экран приветствия компактнее (кнопки видны сразу) и прокручивается.
            let height = ui.available_height();
            let compact = height < 600.0;
            let (logo_size, title_size, gap) = if compact { (64.0, 30.0, 14.0) } else { (104.0, 38.0, 26.0) };
            egui::ScrollArea::vertical().id_salt("forma-welcome").auto_shrink(false).show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(if compact { 16.0 } else { (height * 0.14).max(24.0) });
                    ui.add(egui::Image::new(egui::load::SizedTexture::new(logo.id(), vec2(logo_size, logo_size))));
                    ui.add_space(if compact { 8.0 } else { 14.0 });
                    // «FORMA» вразрядку над названием, как подпись студии.
                    ui.label(RichText::new("F O R M A").size(13.0).family(forma_ui::fonts::semibold()).color(pal.welcome_brand));
                    ui.add_space(2.0);
                    ui.label(RichText::new(self.spec.short_name).size(title_size).family(forma_ui::fonts::bold()).color(pal.welcome_title));
                    ui.add_space(6.0);
                    widgets::centered_text(ui, &t!("app-tagline"), 15.0, pal.text_muted, 460.0);
                    ui.add_space(gap);
                    let width = (ui.available_width() - 32.0).clamp(120.0, 300.0);
                    content(ui, self, width);
                    // Рекомендуемые плагины: ставятся одной кнопкой, пока их нет.
                    let catalog = self.spec.catalog();
                    let mut plugin_actions = Vec::new();
                    ui.add_space(gap);
                    forma_ui::plugins_page::welcome(ui, &pal, &self.plugins, &catalog, (ui.available_width() - 32.0).clamp(240.0, 460.0), &mut plugin_actions);
                    self.cmds.extend(plugin_actions.into_iter().map(crate::dialogs::plugin_cmd));
                    ui.add_space(gap);
                    let gear = forma_ui::icons::image(ui.ctx(), "settings", 15.0, pal.text_muted);
                    if ui.add(egui::Button::new((gear, RichText::new(t!("action-settings")).size(13.0).color(pal.text_muted))).frame(false)).clicked() {
                        self.cmds.push(ShellCmd::Settings);
                    }
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("{} · {}", t!("status-renderer"), self.gpu_info)).size(12.0).color(pal.text_faint));
                    ui.add_space(16.0);
                });
            });
        });
    }

    /// Большая кнопка экрана приветствия: значок и подпись.
    pub fn welcome_button(&self, ui: &mut Ui, icon: &str, text: String, width: f32, primary: bool) -> egui::Response {
        let img = forma_ui::icons::image(ui.ctx(), icon, 18.0, self.pal.text);
        let mut b = egui::Button::new((img, RichText::new(text).size(15.5))).min_size(vec2(width, 40.0)).corner_radius(10).truncate();
        if primary {
            b = b.fill(self.pal.accent_soft);
        }
        ui.add(b)
    }

    pub(crate) fn toast_ui(&mut self, ctx: &egui::Context) {
        let now = self.now();
        let anim_id = Id::new("forma-toast-anim");
        let Some(t) = self.current_toast() else {
            ctx.animate_bool_with_time(anim_id, false, 0.0);
            return;
        };
        // Уведомление выезжает снизу и так же плавно исчезает.
        let visible = now <= t.until;
        let a = ctx.animate_bool_with_time(anim_id, visible, 0.22);
        if !visible && a <= 0.0 {
            self.clear_toast();
            return;
        }
        let pal = &self.pal;
        let color = if t.error { pal.danger } else { pal.accent };
        egui::Area::new(Id::new("forma-toast")).anchor(Align2::CENTER_BOTTOM, vec2(0.0, -40.0 + (1.0 - a) * 18.0)).order(egui::Order::Foreground).show(ctx, |ui| {
            ui.multiply_opacity(a);
            Frame::new()
                .fill(pal.toast_bg)
                .stroke(Stroke::new(1.0, color.gamma_multiply(0.6)))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::symmetric(18, 11))
                .shadow(egui::Shadow { offset: [0, 6], blur: 22, spread: 0, color: pal.shadow })
                .show(ui, |ui| {
                    // Длинное уведомление переносится, а не уходит за края окна.
                    ui.set_max_width((ctx.content_rect().width() - 80.0).clamp(120.0, 640.0));
                    ui.add(egui::Label::new(RichText::new(&t.text).color(pal.toast_text)).wrap());
                });
        });
        ctx.request_repaint_after_secs(if a < 1.0 { 0.0 } else { 0.5 });
    }
}
