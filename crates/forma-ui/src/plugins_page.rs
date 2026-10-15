//! Страница «Плагины»: «Рекомендуемое» — каталог программы с установкой в одну кнопку,
//! «Установленные» — что стоит, с включением, удалением, установкой из файла и папками
//! плагинов в разработке. У неработающего плагина написано, чего ему не хватает (итог
//! [`forma_plugins::check`]), — как во вкладке модов у загрузчика Minecraft.
//!
//! Установка и удаление выполняются сразу (страница только сообщает, что нажали),
//! а включение и выключение — часть черновика настроек: применяются кнопками окна.

use std::path::PathBuf;

use egui::{CornerRadius, Margin, RichText, Sense, Stroke, Ui, vec2};
use forma_i18n::t;
use forma_plugins::{CatalogEntry, Installed, Kind, Manifest, Permissions, PluginSettings, Plugins, Problem, Progress, Report, Source};

use crate::settings_window::matches;
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
    /// Подключить папку плагина в разработке.
    AddDevFolder,
    RemoveDevFolder(PathBuf),
    /// Перечитать плагины (после правок в папке разработчика).
    Reload,
}

/// Что не так с плагином — одной строкой.
pub fn problem_text(p: &Problem) -> String {
    match p {
        Problem::OtherApps(apps) => t!("plugin-problem-apps", apps = apps.join(", ")),
        Problem::Api(need) => t!("plugin-problem-api", need = need.clone(), have = forma_plugins::manifest::api_version().to_string()),
        Problem::Missing { dep, need } => t!("plugin-problem-missing", dep = dep.clone(), need = need.clone()),
        Problem::Version { dep, need, found } => t!("plugin-problem-version", dep = dep.clone(), need = need.clone(), found = found.clone()),
        Problem::Disabled(dep) => t!("plugin-problem-disabled", dep = dep.clone()),
        Problem::Broken(dep) => t!("plugin-problem-broken", dep = dep.clone()),
        Problem::Cycle(chain) => t!("plugin-problem-cycle", chain = chain.join(" → ")),
        Problem::Duplicate => t!("plugin-problem-duplicate"),
    }
}

/// Что плагин приносит — строками для окна установки.
pub fn brings(m: &Manifest) -> Vec<String> {
    let c = &m.contributes;
    let list = |items: Vec<String>| items.join(", ");
    let mut out = Vec::new();
    if c.themes.is_some() {
        out.push(t!("plugin-brings-themes"));
    }
    if c.icons.is_some() {
        out.push(t!("plugin-brings-icons"));
    }
    if c.locales.is_some() {
        out.push(t!("plugin-brings-locales"));
    }
    if !c.blocks.is_empty() {
        out.push(t!("plugin-brings-blocks", list = list(c.blocks.iter().map(|b| if b.title.is_empty() { b.lang.clone() } else { format!("{} ({})", b.title, b.lang) }).collect())));
    }
    if !c.embeds.is_empty() {
        out.push(t!("plugin-brings-embeds", list = list(c.embeds.iter().flat_map(|e| e.extensions.iter().map(|x| format!(".{x}"))).collect())));
    }
    if !c.commands.is_empty() {
        out.push(t!("plugin-brings-commands", list = list(c.commands.iter().map(|c| c.title.clone()).collect())));
    }
    if !c.panels.is_empty() {
        out.push(t!("plugin-brings-panels", list = list(c.panels.iter().map(|p| p.title.clone()).collect())));
    }
    if !c.hovers.is_empty() {
        out.push(t!("plugin-brings-hovers", list = list(c.hovers.iter().map(|h| h.title.clone()).collect())));
    }
    if m.code.is_some() {
        out.push(t!("plugin-brings-code"));
    }
    out
}

/// Что плагин просит — строками для окна установки.
pub fn asks(p: &Permissions) -> Vec<String> {
    let mut out = Vec::new();
    if !p.process.is_empty() {
        out.push(t!("plugin-perm-process", list = p.process.join(", ")));
    }
    for f in &p.files {
        out.push(match f.as_str() {
            "vault:read" => t!("plugin-perm-vault-read"),
            "vault:write" => t!("plugin-perm-vault-write"),
            "data" => t!("plugin-perm-data"),
            other => t!("plugin-perm-other", what = other.to_owned()),
        });
    }
    out
}

/// Подпись вида плагина.
fn kind_text(kind: Kind) -> String {
    match kind {
        Kind::Tool => t!("plugins-kind-tool"),
        Kind::Themes => t!("plugins-kind-themes"),
        Kind::Icons => t!("plugins-kind-icons"),
        Kind::Script => t!("plugins-kind-script"),
        Kind::Extension => t!("plugins-kind-extension"),
    }
}

#[derive(Debug, Default)]
pub struct PluginsPage {
    /// 0 — «Рекомендуемое», 1 — «Установленные».
    tab: usize,
}

/// Значок плагина: из каталога, иначе по виду.
fn icon_of(entry: Option<&CatalogEntry>, kind: Kind) -> &'static str {
    entry.map(|e| e.icon).unwrap_or(match kind {
        Kind::Tool | Kind::Extension => "plugin",
        Kind::Themes | Kind::Icons => "palette",
        Kind::Script => "file-code",
    })
}

fn size_text(bytes: u64) -> String {
    format!("≈ {}", forma_plugins::human_size(bytes, &t!("unit-mb"), &t!("unit-kb")))
}

impl PluginsPage {
    #[allow(clippy::too_many_arguments)]
    /// `failures` — плагины, чей code программа остановила (упал или не скомпилировался), и
    /// почему.
    pub fn ui(&mut self, ui: &mut Ui, pal: &Palette, search: &str, plugins: &Plugins, report: &Report, failures: &[(String, String)], catalog: &[CatalogEntry], settings: &mut PluginSettings, actions: &mut Vec<Action>) {
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
            _ => installed(ui, pal, filter, plugins, report, failures, catalog, settings, actions),
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
                (Source::Download { .. } | Source::Bundled { .. }, false, None) => {
                    if crate::icons::text_button(ui, "download", t!("plugins-install")).clicked() {
                        actions.push(Action::Install(e.id.to_owned()));
                    }
                }
            },
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn installed(ui: &mut Ui, pal: &Palette, filter: &str, plugins: &Plugins, report: &Report, failures: &[(String, String)], catalog: &[CatalogEntry], settings: &mut PluginSettings, actions: &mut Vec<Action>) {
    ui.horizontal_wrapped(|ui| {
        if crate::icons::text_button(ui, "add", t!("plugins-install-file")).on_hover_text(t!("plugins-install-file-hint")).clicked() {
            actions.push(Action::InstallFile);
        }
        if plugins.root().is_some() && crate::icons::text_button(ui, "open", t!("plugins-open-folder")).clicked() {
            actions.push(Action::OpenFolder);
        }
        if crate::icons::text_button(ui, "folder", t!("plugins-add-dev")).on_hover_text(t!("plugins-add-dev-hint")).clicked() {
            actions.push(Action::AddDevFolder);
        }
        if crate::icons::text_button(ui, "refresh", t!("plugins-reload")).clicked() {
            actions.push(Action::Reload);
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
        let kind = kind_text(p.manifest.kind());
        let mut enabled = settings.enabled(&id);
        let problems = report.problems(&id);
        let broken = enabled && !problems.is_empty();
        let failure = failures.iter().find(|(f, _)| *f == id).map(|(_, e)| e.clone()).filter(|_| enabled);
        let dev = p.dev;
        let dir = p.dir.clone();
        card(
            ui,
            pal,
            if dev { "installed-dev" } else { "installed" },
            icon_of(entry, p.manifest.kind()),
            &title,
            |ui| {
                if !description.is_empty() {
                    ui.add(egui::Label::new(RichText::new(&description).size(12.5).color(pal.text)).wrap());
                    ui.add_space(2.0);
                }
                // Плагины каталога ставятся «последней сборкой» — номера версии у них нет.
                let version = if meta.version.is_empty() || meta.version == "latest" { String::new() } else { t!("plugins-version", version = meta.version.clone()) };
                let authors = p.manifest.authors().join(", ");
                let dev_label = if dev { t!("plugins-dev") } else { String::new() };
                meta_line(ui, pal, &[kind, version, authors, meta.license.clone(), dev_label], &meta.homepage);
                ui.add(egui::Label::new(RichText::new(dir.display().to_string()).size(11.0).color(pal.text_faint)).truncate()).on_hover_text(if dev { t!("plugins-dev-hint") } else { dir.display().to_string() });
                // Включённый плагин, который не работает: чего ему не хватает.
                if broken {
                    ui.add_space(2.0);
                    for problem in problems {
                        crate::widgets::icon_label(ui, "error", format!("{} {}", t!("plugins-not-working"), problem_text(problem)), 11.5, pal.danger);
                    }
                }
                // Code плагина остановлен: ошибка одной строкой (у wasmtime она бывает в
                // несколько), целиком — в подсказке.
                if let Some(error) = &failure {
                    ui.add_space(2.0);
                    let short = error.split_whitespace().collect::<Vec<_>>().join(" ");
                    let short = match short.char_indices().nth(200) {
                        Some((i, _)) => format!("{}…", &short[..i]),
                        None => short,
                    };
                    crate::widgets::icon_label(ui, "error", t!("plugins-code-failed", error = short), 11.5, pal.danger)
                        .on_hover_text(format!("{error}\n\n{}", t!("plugins-code-failed-hint")));
                }
            },
            |ui| {
                if dev {
                    if crate::icons::button(ui, "close", false).on_hover_text(t!("plugins-remove-dev")).clicked() {
                        actions.push(Action::RemoveDevFolder(dir.clone()));
                    }
                } else if crate::icons::button(ui, "trash", false).on_hover_text(t!("plugins-uninstall")).clicked() {
                    actions.push(Action::Uninstall(id.clone()));
                }
                if crate::widgets::checkbox(ui, pal, &mut enabled, t!("plugins-enabled")).changed() {
                    settings.set_enabled(&id, enabled);
                }
            },
        );
    }
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
