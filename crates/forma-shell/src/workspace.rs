//! Рабочие пространства — вкладки над интерфейсом, как Layout / Modeling / Shading в Blender.
//! У каждого своя раскладка областей; отдельные окна ОС общие для всех пространств.
//!
//! Программа описывает вкладку области ([`ShellTab`]) и свои встроенные пространства
//! ([`Builtin`]): их названия и исходные раскладки.

use std::path::Path;

use eframe::egui::{self, Color32, Frame, Ui};
use forma_i18n::t;
use forma_layout::{AreaId, DockState, DockStyle, Node, Surface, SurfaceKey, TabViewer};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Вкладка области: у каждой свой номер, уникальный во всей раскладке.
pub trait ShellTab: Clone + Serialize + DeserializeOwned + 'static {
    fn id(&self) -> u64;
    fn set_id(&mut self, id: u64);
    /// Вкладка в стандартной раскладке, которая подходит всем: открытые документы обычно
    /// убираются (`None`), а окна инструментов остаются как есть.
    fn for_default_layout(&self) -> Option<Self> {
        Some(self.clone())
    }
}

/// Встроенное пространство программы: его раскладку можно сбросить к исходной.
pub trait Builtin: Copy + PartialEq + Serialize + DeserializeOwned + 'static {
    type Tab: ShellTab;
    /// Все встроенные пространства — набор при первом запуске.
    fn all() -> &'static [Self];
    fn title(self) -> String;
    /// Исходная раскладка пространства.
    fn surface(self, ids: &mut Ids<'_, Self::Tab>) -> Surface<Self::Tab>;
}

/// Выдаёт номера областей и вкладок при сборке раскладок.
pub struct Ids<'a, T> {
    pub dock: &'a mut DockState<T>,
    pub next_tab: &'a mut u64,
}

impl<T> Ids<'_, T> {
    /// Новый номер вкладки.
    pub fn next_id(&mut self) -> u64 {
        *self.next_tab += 1;
        *self.next_tab
    }

    /// Область с вкладками `tabs`.
    pub fn area(&mut self, tabs: Vec<T>) -> Node<T> {
        let id: AreaId = self.dock.new_area_id();
        Node::area(id, tabs)
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct Workspace<B: Builtin> {
    /// Из какого шаблона создано (для сброса раскладки).
    pub builtin: Option<B>,
    /// Своё имя; без него показывается имя шаблона на текущем языке.
    pub name: Option<String>,
    pub surface: Surface<B::Tab>,
}

impl<B: Builtin> Workspace<B> {
    pub fn title(&self) -> String {
        match (&self.name, self.builtin) {
            (Some(name), _) => name.clone(),
            (None, Some(b)) => b.title(),
            (None, None) => t!("workspace-untitled"),
        }
    }
}

/// Набор пространств при первом запуске.
pub fn default_workspaces<B: Builtin>(ids: &mut Ids<'_, B::Tab>) -> Vec<Workspace<B>> {
    B::all().iter().map(|&b| Workspace { builtin: Some(b), name: None, surface: b.surface(ids) }).collect()
}

/// Раскладка интерфейса: рабочие пространства, отдельные окна и счётчики номеров.
#[derive(Serialize, Deserialize)]
#[serde(bound = "")]
pub struct Layout<B: Builtin> {
    pub workspaces: Vec<Workspace<B>>,
    pub active: usize,
    pub dock: DockState<B::Tab>,
    pub next_tab_id: u64,
}

impl<B: Builtin> Layout<B> {
    /// Заводская раскладка: все встроенные пространства.
    pub fn factory() -> Self {
        let mut dock = DockState::default();
        let mut next_tab = 0;
        let workspaces = default_workspaces(&mut Ids { dock: &mut dock, next_tab: &mut next_tab });
        Self { workspaces, active: 0, dock, next_tab_id: next_tab }
    }

    pub fn ids(&mut self) -> Ids<'_, B::Tab> {
        Ids { dock: &mut self.dock, next_tab: &mut self.next_tab_id }
    }

    /// Новый номер вкладки.
    pub fn next_id(&mut self) -> u64 {
        self.next_tab_id += 1;
        self.next_tab_id
    }

    /// Раскладка текущего пространства, общее состояние областей и счётчик номеров вкладок —
    /// по отдельности, чтобы рисовать области и выдавать номера новым вкладкам одновременно.
    pub fn parts(&mut self) -> (&mut Surface<B::Tab>, &mut DockState<B::Tab>, &mut u64) {
        if self.active >= self.workspaces.len() {
            self.active = 0;
        }
        let active = self.active;
        (&mut self.workspaces[active].surface, &mut self.dock, &mut self.next_tab_id)
    }

    /// Раскладка текущего пространства.
    pub fn surface(&self) -> &Surface<B::Tab> {
        &self.workspaces[self.active.min(self.workspaces.len() - 1)].surface
    }

    pub fn surface_mut(&mut self) -> &mut Surface<B::Tab> {
        let i = self.active.min(self.workspaces.len() - 1);
        &mut self.workspaces[i].surface
    }

    /// Раскладки всех пространств и отдельных окон.
    pub fn surfaces(&self) -> impl Iterator<Item = &Surface<B::Tab>> {
        self.workspaces.iter().map(|w| &w.surface).chain(self.dock.windows.iter().map(|w| &w.surface))
    }

    /// Вкладки текущего пространства и отдельных окон.
    pub fn visible_tabs(&self) -> impl Iterator<Item = &B::Tab> {
        std::iter::once(self.surface())
            .chain(self.dock.windows.iter().map(|w| &w.surface))
            .flat_map(|s| s.areas().into_iter().flat_map(|a| a.tabs.iter()))
    }

    /// Все вкладки всех пространств и окон.
    pub fn all_tabs(&self) -> impl Iterator<Item = &B::Tab> {
        self.surfaces().flat_map(|s| s.areas().into_iter().flat_map(|a| a.tabs.iter()))
    }

    /// Чинит загруженную раскладку и выставляет счётчики выше уже занятых номеров.
    pub fn sanitize(&mut self) {
        if self.workspaces.is_empty() {
            self.workspaces = default_workspaces(&mut self.ids());
        }
        for w in &mut self.workspaces {
            w.surface.sanitize();
        }
        self.dock.sanitize();
        let max_area = self.workspaces.iter().map(|w| w.surface.max_area_id()).max().unwrap_or(0);
        self.dock.reserve_ids(max_area);
        let max_tab = self.all_tabs().map(ShellTab::id).max().unwrap_or(0);
        self.next_tab_id = self.next_tab_id.max(max_tab);
        self.active = self.active.min(self.workspaces.len() - 1);
    }

    /// Делает вкладку, подходящую под `pred`, текущей в её области: сначала в текущем
    /// пространстве, потом в отдельных окнах. `false` — такой вкладки нет.
    pub fn show_tab(&mut self, mut pred: impl FnMut(&B::Tab) -> bool) -> bool {
        let active = self.active.min(self.workspaces.len() - 1);
        let mut found: Option<(SurfaceKey, AreaId)> = None;
        let mut check = |key: SurfaceKey, s: &mut Surface<B::Tab>| {
            s.for_each_area_mut(|a| {
                if found.is_none()
                    && let Some(i) = a.tabs.iter().position(&mut pred)
                {
                    a.active = i;
                    found = Some((key, a.id));
                }
            });
        };
        check(SurfaceKey::Main, &mut self.workspaces[active].surface);
        for w in &mut self.dock.windows {
            check(SurfaceKey::Window(w.id), &mut w.surface);
        }
        match found {
            Some((key, area)) => {
                self.reveal_area(key, area);
                true
            }
            None => false,
        }
    }

    /// Изменяет первую подходящую вкладку текущего пространства (или отдельного окна)
    /// и делает её текущей. `false` — такой вкладки нет.
    pub fn update_tab(&mut self, mut pred: impl FnMut(&B::Tab) -> bool, mut change: impl FnMut(&mut B::Tab)) -> bool {
        let active = self.active.min(self.workspaces.len() - 1);
        let mut found: Option<(SurfaceKey, AreaId)> = None;
        let mut visit = |key: SurfaceKey, s: &mut Surface<B::Tab>| {
            s.for_each_area_mut(|a| {
                if found.is_none()
                    && let Some(i) = a.tabs.iter().position(&mut pred)
                {
                    change(&mut a.tabs[i]);
                    a.active = i;
                    found = Some((key, a.id));
                }
            });
        };
        visit(SurfaceKey::Main, &mut self.workspaces[active].surface);
        for w in &mut self.dock.windows {
            visit(SurfaceKey::Window(w.id), &mut w.surface);
        }
        match found {
            Some((key, area)) => {
                self.reveal_area(key, area);
                true
            }
            None => false,
        }
    }

    /// Добавляет вкладку в область, где уже есть вкладка, подходящая под `near`; если такой
    /// нет — в последнюю область, с которой работали.
    pub fn open_near(&mut self, tab: B::Tab, mut near: impl FnMut(&B::Tab) -> bool) {
        let mut placed: Option<AreaId> = None;
        let mut tab = Some(tab);
        self.surface_mut().for_each_area_mut(|a| {
            if placed.is_none()
                && a.tabs.iter().any(&mut near)
                && let Some(t) = tab.take()
            {
                a.tabs.push(t);
                a.active = a.tabs.len() - 1;
                placed = Some(a.id);
            }
        });
        match (placed, tab) {
            (Some(area), _) => self.reveal_area(SurfaceKey::Main, area),
            (None, Some(t)) => {
                let surface = self.surface_mut();
                if surface.maximized.is_some() {
                    surface.maximized = None;
                }
                self.dock.open_tab(t);
            }
            (None, None) => {}
        }
    }

    /// Делает область текущей и видимой: если развёрнута другая область, раскладка возвращается.
    pub fn reveal_area(&mut self, key: SurfaceKey, area: AreaId) {
        self.dock.focus(key, area);
        let active = self.active.min(self.workspaces.len() - 1);
        let surface = match key {
            SurfaceKey::Main => Some(&mut self.workspaces[active].surface),
            SurfaceKey::Window(id) => self.dock.windows.iter_mut().find(|w| w.id == id).map(|w| &mut w.surface),
        };
        if let Some(s) = surface
            && s.maximized.is_some_and(|m| m != area)
        {
            s.maximized = None;
        }
    }

    /// Копия пространства `i` с новыми номерами областей и вкладок.
    pub fn copy_workspace(&mut self, i: usize) -> Workspace<B> {
        let mut ws = self.workspaces[i].clone();
        ws.name = Some(t!("workspace-copy-name", name = ws.title()));
        let dock = &mut self.dock;
        ws.surface.reassign_ids(|| dock.new_area_id());
        let next = &mut self.next_tab_id;
        ws.surface.for_each_area_mut(|a| {
            for tab in &mut a.tabs {
                *next += 1;
                tab.set_id(*next);
            }
        });
        ws
    }

    /// Копия для стандартной раскладки, которая подходит всем: вкладки документов убраны.
    pub fn for_default(&self) -> Option<Layout<B>> {
        let value = serde_json::to_value(self).ok()?;
        let mut copy: Layout<B> = serde_json::from_value(value).ok()?;
        let strip = |s: &mut Surface<B::Tab>| {
            s.for_each_area_mut(|a| {
                // Одинаковые вкладки (кроме номера) схлопываются: три пустые вкладки — одна.
                let mut seen: Vec<serde_json::Value> = Vec::new();
                a.tabs = a
                    .tabs
                    .iter()
                    .filter_map(ShellTab::for_default_layout)
                    .filter(|t| {
                        let mut key = serde_json::to_value(t).unwrap_or_default();
                        if let Some(o) = key.as_object_mut() {
                            o.remove("id");
                        }
                        let fresh = !seen.contains(&key);
                        seen.push(key);
                        fresh
                    })
                    .collect();
                a.clamp_active();
            });
        };
        for w in &mut copy.workspaces {
            strip(&mut w.surface);
        }
        for w in &mut copy.dock.windows {
            strip(&mut w.surface);
        }
        copy.sanitize();
        Some(copy)
    }
}

/// Файл раскладки вместе с тем, что программа ещё помнит об интерфейсе (`view`).
#[derive(Deserialize)]
#[serde(bound = "")]
struct LayoutFile<B: Builtin> {
    version: u32,
    #[serde(flatten)]
    layout: Layout<B>,
    #[serde(default)]
    view: serde_json::Value,
}

#[derive(Serialize)]
#[serde(bound = "")]
struct LayoutFileRef<'a, B: Builtin> {
    version: u32,
    #[serde(flatten)]
    layout: &'a Layout<B>,
    view: &'a serde_json::Value,
}

/// Версия формата файла раскладки (общая с Forma Mycelium).
pub const LAYOUT_VERSION: u32 = 2;

/// Читает раскладку. Файл другой версии или испорченный — `None` (тогда берётся запасная).
pub fn read_layout<B: Builtin>(path: &Path) -> Option<(Layout<B>, serde_json::Value)> {
    let file: LayoutFile<B> = crate::settings::read_json(path)?;
    if file.version != LAYOUT_VERSION {
        return None;
    }
    let mut layout = file.layout;
    layout.sanitize();
    Some((layout, file.view))
}

pub fn write_layout<B: Builtin>(path: &Path, layout: &Layout<B>, view: &serde_json::Value) {
    crate::settings::write_json(path, &LayoutFileRef { version: LAYOUT_VERSION, layout, view });
}

/// Рисует области текущего пространства (`surface`, см. [`Layout::parts`]) и отдельные окна.
/// Вызывать раз за кадр после верхней строки и строки состояния.
pub fn show<T, V: TabViewer<Tab = T>>(ui: &mut Ui, surface: &mut Surface<T>, dock: &mut DockState<T>, style: &DockStyle, bg: Color32, viewer: &mut V) {
    let ctx = ui.ctx().clone();
    egui::CentralPanel::default().frame(Frame::new().fill(bg)).show(ui, |ui| {
        dock.show_main(ui, surface, viewer, style);
    });
    dock.show_windows(&ctx, viewer, style);
    dock.finish(&ctx, surface, viewer);
}
