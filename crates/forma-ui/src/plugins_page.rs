//! Страница «Плагины»: «Рекомендуемое» — каталог программы с установкой в одну кнопку,
//! «Установленные» — что стоит, с включением, удалением и установкой из файла.
//!
//! Установка и удаление выполняются сразу (страница только сообщает, что нажали),
//! а включение и выключение — часть черновика настроек: применяются кнопками окна.

use egui::{CornerRadius, Margin, RichText, Sense, Stroke, Ui, vec2};
use forma_i18n::t;
use forma_plugins::{CatalogEntry, Installed, Kind, PluginSettings, Plugins, Progress, Source};

use crate::settings_window::{self as sw, matches};
use crate::theme::Palette;

pub const PAGE: &str = "plugins";

/// Подписи страницы — для поиска по настройкам.
pub fn keywords() -> Vec<String> {
    vec![t!("plugins-recommended"), t!("plugins-installed"), t!("plugins-install-file")]
}

/// Что нажали на странице.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Install(String),
    InstallFile,
    Uninstall(String),
    OpenFolder,
}

#[derive(Debug, Default)]
pub struct PluginsPage {
    /// 0 — «Рекомендуемое», 1 — «Установленные».
    tab: usize,
}

/// Значок плагина: из каталога, иначе по виду.
fn icon_of(entry: Option<&CatalogEntry>, kind: Kind) -> &'static str {
    entry.map(|e| e.icon).unwrap_or(match kind {
        Kind::Tool => "plugin",
        Kind::Themes => "palette",
        Kind::Script => "file-code",
    })
}

fn size_text(bytes: u64) -> String {
    format!("≈ {}", forma_plugins::human_size(bytes, &t!("unit-mb"), &t!("unit-kb")))
}

impl PluginsPage {
    #[allow(clippy::too_many_arguments)]
    pub fn ui(&mut self, ui: &mut Ui, pal: &Palette, search: &str, plugins: &Plugins, catalog: &[CatalogEntry], settings: &mut PluginSettings, actions: &mut Vec<Action>) {
        let labels = [t!("plugins-recommended"), t!("plugins-installed")];
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        if let Some(i) = crate::widgets::segmented(ui, pal, egui::Id::new("forma-plugins-tab"), &refs, self.tab, false) {
            self.tab = i;
        }
        ui.add_space(10.0);
        // Страница нашлась не по названию — показываем только подходящие плагины.
        let filter = if matches(&t!("settings-page-plugins"), search) { "" } else { search };
        match self.tab {
            0 => recommended(ui, pal, filter, plugins, catalog, actions),
            _ => installed(ui, pal, filter, plugins, catalog, settings, actions),
        }
    }
}

/// Карточка плагина: значок, название и подписи слева, кнопки справа.
/// `place` различает одинаковые карточки в разных местах (страница настроек и экран приветствия).
fn card(ui: &mut Ui, pal: &Palette, place: &str, icon: &str, title: &str, body: impl FnOnce(&mut Ui), side: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(pal.bg_elevated)
        .stroke(Stroke::new(1.0, pal.border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
                crate::icons::paint(ui.painter(), r, icon, pal.accent);
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    // Справа кнопки, слева — название и подписи, которые переносятся.
                    crate::widgets::bar(
                        ui,
                        egui::Id::new(("forma-plugin-card", place, title)),
                        0.0,
                        |ui| {
                            ui.label(RichText::new(title).size(14.0).family(crate::fonts::semibold()).color(pal.heading));
                        },
                        side,
                    );
                    body(ui);
                });
            });
        });
    ui.add_space(8.0);
}

/// Подпись под названием плагина.
fn meta_line(ui: &mut Ui, pal: &Palette, parts: &[String], homepage: &str) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let text = parts.iter().filter(|p| !p.is_empty()).cloned().collect::<Vec<_>>().join(" · ");
        if !text.is_empty() {
            ui.label(RichText::new(text).size(11.5).color(pal.text_faint));
        }
        if !homepage.is_empty() {
            ui.hyperlink_to(RichText::new(t!("plugins-homepage")).size(11.5), homepage);
        }
    });
}

fn progress_bar(ui: &mut Ui, p: &Progress) {
    let (fraction, text) = match p {
        Progress::Downloading { done, total: Some(total) } if *total > 0 => {
            let mb = |b: u64| format!("{:.1}", b as f64 / 1_048_576.0);
            (*done as f32 / *total as f32, t!("plugins-downloading-of", done = mb(*done), total = mb(*total)))
        }
        Progress::Downloading { done, .. } => (0.0, t!("plugins-downloading", done = format!("{:.1}", *done as f64 / 1_048_576.0))),
        Progress::Extracting => (1.0, t!("plugins-extracting")),
    };
    ui.add(egui::ProgressBar::new(fraction).desired_width(180.0_f32.min(ui.available_width())).text(RichText::new(text).size(11.5)));
}

fn recommended(ui: &mut Ui, pal: &Palette, filter: &str, plugins: &Plugins, catalog: &[CatalogEntry], actions: &mut Vec<Action>) {
    let shown: Vec<&CatalogEntry> = catalog.iter().filter(|e| matches(&e.name, filter) || matches(&e.description, filter)).collect();
    if shown.is_empty() {
        ui.label(RichText::new(t!("settings-nothing-found")).color(pal.text_faint));
    }
    for e in shown {
        let installed = plugins.get(e.id).is_some();
        let progress = plugins.progress(e.id);
        card(
            ui,
            pal,
            "recommended",
            e.icon,
            &e.name,
            |ui| {
                ui.add(egui::Label::new(RichText::new(&e.description).size(12.5).color(pal.text)).wrap());
                ui.add_space(2.0);
                let size = e.size.map(size_text).unwrap_or_default();
                meta_line(ui, pal, &[size, e.author.to_owned(), e.license.to_owned()], e.homepage);
                if let Some(err) = plugins.error(e.id) {
                    crate::widgets::icon_label(ui, "error", t!("plugins-failed", error = err.to_owned()), 11.5, pal.danger);
                }
            },
            |ui| match (&e.source, installed, &progress) {
                (_, _, Some(p)) => progress_bar(ui, p),
                (_, true, None) => {
                    crate::widgets::icon_label(ui, "success", t!("plugins-is-installed"), 12.5, pal.success);
                }
                (Source::Later, false, None) => {
                    ui.label(RichText::new(t!("plugins-coming")).size(12.0).color(pal.text_faint));
                }
                (Source::Download { .. }, false, None) => {
                    if crate::icons::text_button(ui, "download", t!("plugins-install")).clicked() {
                        actions.push(Action::Install(e.id.to_owned()));
                    }
                }
            },
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn installed(ui: &mut Ui, pal: &Palette, filter: &str, plugins: &Plugins, catalog: &[CatalogEntry], settings: &mut PluginSettings, actions: &mut Vec<Action>) {
    ui.horizontal_wrapped(|ui| {
        if crate::icons::text_button(ui, "add", t!("plugins-install-file")).on_hover_text(t!("plugins-install-file-hint")).clicked() {
            actions.push(Action::InstallFile);
        }
        if plugins.root().is_some() && crate::icons::text_button(ui, "open", t!("plugins-open-folder")).clicked() {
            actions.push(Action::OpenFolder);
        }
    });
    ui.add_space(8.0);
    let list: Vec<&Installed> = plugins.installed().iter().filter(|p| matches(p.manifest.title(), filter) || matches(&p.manifest.plugin.description, filter)).collect();
    if list.is_empty() {
        ui.label(RichText::new(t!("plugins-none-installed")).color(pal.text_faint));
        return;
    }
    for p in list {
        let id = p.id().to_owned();
        let entry = catalog.iter().find(|e| e.id == id);
        let meta = &p.manifest.plugin;
        // Название и описание — из каталога на языке интерфейса, иначе из plugin.toml.
        let title = entry.map(|e| e.name.clone()).unwrap_or_else(|| p.manifest.title().to_owned());
        let description = entry.map(|e| e.description.clone()).unwrap_or_else(|| meta.description.clone());
        let kind = match meta.kind {
            Kind::Tool => t!("plugins-kind-tool"),
            Kind::Themes => t!("plugins-kind-themes"),
            Kind::Script => t!("plugins-kind-script"),
        };
        let mut enabled = settings.enabled(&id);
        card(
            ui,
            pal,
            "installed",
            icon_of(entry, meta.kind),
            &title,
            |ui| {
                if !description.is_empty() {
                    ui.add(egui::Label::new(RichText::new(&description).size(12.5).color(pal.text)).wrap());
                    ui.add_space(2.0);
                }
                // Плагины каталога ставятся «последней сборкой» — номера версии у них нет.
                let version = if meta.version.is_empty() || meta.version == "latest" { String::new() } else { t!("plugins-version", version = meta.version.clone()) };
                meta_line(ui, pal, &[kind, version, meta.author.clone(), meta.license.clone()], &meta.homepage);
                ui.add(egui::Label::new(RichText::new(p.dir.display().to_string()).size(11.0).color(pal.text_faint)).truncate()).on_hover_text(p.dir.display().to_string());
            },
            |ui| {
                if crate::icons::button(ui, "trash", false).on_hover_text(t!("plugins-uninstall")).clicked() {
                    actions.push(Action::Uninstall(id.clone()));
                }
                if ui.checkbox(&mut enabled, t!("plugins-enabled")).changed() {
                    settings.set_enabled(&id, enabled);
                }
            },
        );
    }
    sw::hint(ui, pal, &t!("plugins-enable-hint"));
}

/// Блок «Рекомендуемые плагины» для экрана приветствия: плагины, которые можно поставить
/// и которые ещё не стоят. Возвращает нажатые действия.
pub fn welcome(ui: &mut Ui, pal: &Palette, plugins: &Plugins, catalog: &[CatalogEntry], width: f32, actions: &mut Vec<Action>) {
    let wanted: Vec<&CatalogEntry> = catalog.iter().filter(|e| e.recommended && matches!(e.source, Source::Download { .. }) && plugins.get(e.id).is_none()).collect();
    if wanted.is_empty() {
        return;
    }
    ui.label(RichText::new(t!("plugins-welcome-title")).size(13.0).color(pal.text_faint));
    ui.add_space(4.0);
    for e in wanted {
        ui.allocate_ui_with_layout(vec2(width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            let progress = plugins.progress(e.id);
            card(
                ui,
                pal,
                "welcome",
                e.icon,
                &e.name,
                |ui| {
                    ui.add(egui::Label::new(RichText::new(&e.description).size(12.0).color(pal.text_muted)).wrap());
                    if let Some(size) = e.size {
                        ui.label(RichText::new(size_text(size)).size(11.5).color(pal.text_faint));
                    }
                    if let Some(err) = plugins.error(e.id) {
                        crate::widgets::icon_label(ui, "error", t!("plugins-failed", error = err.to_owned()), 11.5, pal.danger);
                    }
                },
                |ui| match &progress {
                    Some(p) => progress_bar(ui, p),
                    None => {
                        if crate::icons::text_button(ui, "download", t!("plugins-install")).clicked() {
                            actions.push(Action::Install(e.id.to_owned()));
                        }
                    }
                },
            );
        });
    }
}
