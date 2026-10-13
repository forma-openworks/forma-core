//! Рабочие пространства — вкладки над интерфейсом, как Layout / Modeling / Shading в Blender.
//! У каждого своя раскладка областей; отдельные окна ОС общие для всех пространств.
//!
//! Программа описывает редактор области ([`ShellEditor`]) и свои встроенные пространства
//! ([`Builtin`]): их названия и исходные раскладки.

use std::path::Path;

use eframe::egui::{self, Color32, Frame, Ui};
use forma_i18n::t;
use forma_layout::{AreaId, DockState, DockStyle, EditorViewer, Node, Place, Surface, SurfaceKey, Zone};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Редактор области: у каждого свой номер, уникальный во всей раскладке.
pub trait ShellEditor: Clone + Serialize + DeserializeOwned + 'static {
    fn id(&self) -> u64;
    fn set_id(&mut self, id: u64);
    /// Самый большой номер внутри редактора: если у его вкладок свои номера из того же
    /// счётчика, новые номера не должны их повторять.
    fn max_id(&self) -> u64 {
        self.id()
    }
    /// Новые номера редактору (и его вкладкам) — при копировании рабочего пространства.
    fn renumber(&mut self, next: &mut dyn FnMut() -> u64) {
        self.set_id(next());
    }
    /// Редактор в стандартной раскладке, которая подходит всем: открытые документы обычно
    /// убираются, а сам редактор остаётся.
    fn for_default_layout(&self) -> Self {
        self.clone()
    }
}

/// Встроенное пространство программы: его раскладку можно сбросить к исходной.
pub trait Builtin: Copy + PartialEq + Serialize + DeserializeOwned + 'static {
    type Editor: ShellEditor;
    /// Все встроенные пространства — набор при первом запуске.
    fn all() -> &'static [Self];
    fn title(self) -> String;
    /// Исходная раскладка пространства.
    fn surface(self, ids: &mut Ids<'_, Self::Editor>) -> Surface<Self::Editor>;
}

/// Выдаёт номера областей и редакторов при сборке раскладок.
pub struct Ids<'a, T> {
    pub dock: &'a mut DockState<T>,
    pub next_editor: &'a mut u64,
}

impl<T> Ids<'_, T> {
    /// Новый номер редактора.
    pub fn next_id(&mut self) -> u64 {
        *self.next_editor += 1;
        *self.next_editor
    }

    /// Область с редактором `editor`.
    pub fn area(&mut self, editor: T) -> Node<T> {
        let id: AreaId = self.dock.new_area_id();
        Node::area(id, editor)
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct Workspace<B: Builtin> {
    /// Из какого шаблона создано (для сброса раскладки).
    pub builtin: Option<B>,
    /// Своё имя; без него показывается имя шаблона на текущем языке.
    pub name: Option<String>,
    pub surface: Surface<B::Editor>,
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
pub fn default_workspaces<B: Builtin>(ids: &mut Ids<'_, B::Editor>) -> Vec<Workspace<B>> {
    B::all().iter().map(|&b| Workspace { builtin: Some(b), name: None, surface: b.surface(ids) }).collect()
}

/// Раскладка интерфейса: рабочие пространства, отдельные окна и счётчики номеров.
#[derive(Serialize, Deserialize)]
#[serde(bound = "")]
pub struct Layout<B: Builtin> {
    pub workspaces: Vec<Workspace<B>>,
    pub active: usize,
    pub dock: DockState<B::Editor>,
    #[serde(alias = "next_tab_id")]
    pub next_editor_id: u64,
}

impl<B: Builtin> Layout<B> {
    /// Заводская раскладка: все встроенные пространства.
    pub fn factory() -> Self {
        let mut dock = DockState::default();
        let mut next = 0;
        let workspaces = default_workspaces(&mut Ids { dock: &mut dock, next_editor: &mut next });
        Self { workspaces, active: 0, dock, next_editor_id: next }
    }

    pub fn ids(&mut self) -> Ids<'_, B::Editor> {
        Ids { dock: &mut self.dock, next_editor: &mut self.next_editor_id }
    }

    /// Новый номер редактора.
    pub fn next_id(&mut self) -> u64 {
        self.next_editor_id += 1;
        self.next_editor_id
    }

    /// Раскладка текущего пространства, общее состояние областей и счётчик номеров редакторов —
    /// по отдельности, чтобы рисовать области и выдавать номера новым редакторам одновременно.
    pub fn parts(&mut self) -> (&mut Surface<B::Editor>, &mut DockState<B::Editor>, &mut u64) {
        if self.active >= self.workspaces.len() {
            self.active = 0;
        }
        let active = self.active;
        (&mut self.workspaces[active].surface, &mut self.dock, &mut self.next_editor_id)
    }

    /// Раскладка текущего пространства.
    pub fn surface(&self) -> &Surface<B::Editor> {
        &self.workspaces[self.active.min(self.workspaces.len() - 1)].surface
    }

    pub fn surface_mut(&mut self) -> &mut Surface<B::Editor> {
        let i = self.active.min(self.workspaces.len() - 1);
        &mut self.workspaces[i].surface
    }

    /// Раскладки всех пространств и отдельных окон.
    pub fn surfaces(&self) -> impl Iterator<Item = &Surface<B::Editor>> {
        self.workspaces.iter().map(|w| &w.surface).chain(self.dock.windows.iter().map(|w| &w.surface))
    }

    /// Редакторы текущего пространства и отдельных окон.
    pub fn visible_editors(&self) -> impl Iterator<Item = &B::Editor> {
        std::iter::once(self.surface()).chain(self.dock.windows.iter().map(|w| &w.surface)).flat_map(|s| s.editors().collect::<Vec<_>>())
    }

    /// Все редакторы всех пространств и окон.
    pub fn all_editors(&self) -> impl Iterator<Item = &B::Editor> {
        self.surfaces().flat_map(|s| s.editors().collect::<Vec<_>>())
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
        let max_editor = self.all_editors().map(ShellEditor::max_id).max().unwrap_or(0);
        self.next_editor_id = self.next_editor_id.max(max_editor);
        self.active = self.active.min(self.workspaces.len() - 1);
    }

    /// Первая область текущего пространства (потом — отдельных окон), чей редактор подходит
    /// под `pred`.
    fn find_area(&self, mut pred: impl FnMut(&B::Editor) -> bool) -> Option<(SurfaceKey, AreaId)> {
        let mut find = |key: SurfaceKey, s: &Surface<B::Editor>| s.areas().into_iter().find(|a| pred(&a.editor)).map(|a| (key, a.id));
        if let Some(found) = find(SurfaceKey::Main, self.surface()) {
            return Some(found);
        }
        self.dock.windows.iter().find_map(|w| find(SurfaceKey::Window(w.id), &w.surface))
    }

    /// Делает область с подходящим под `pred` редактором текущей и видимой: сначала в текущем
    /// пространстве, потом в отдельных окнах. `false` — такого редактора нет.
    pub fn show_editor(&mut self, pred: impl FnMut(&B::Editor) -> bool) -> bool {
        match self.find_area(pred) {
            Some((key, area)) => {
                self.reveal_area(key, area);
                true
            }
            None => false,
        }
    }

    /// Изменяет первый подходящий редактор текущего пространства (или отдельного окна)
    /// и делает его область текущей. `false` — такого редактора нет.
    pub fn update_editor(&mut self, pred: impl FnMut(&B::Editor) -> bool, change: impl FnOnce(&mut B::Editor)) -> bool {
        let Some((key, area)) = self.find_area(pred) else { return false };
        if let Some(a) = self.surface_by_key(key).and_then(|s| s.area_mut(area)) {
            change(&mut a.editor);
        }
        self.reveal_area(key, area);
        true
    }

    /// Изменяет все редакторы всех пространств и окон (например, когда файл переименовали).
    pub fn for_each_editor_mut(&mut self, mut change: impl FnMut(&mut B::Editor)) {
        for w in &mut self.workspaces {
            w.surface.for_each_area_mut(|a| change(&mut a.editor));
        }
        for w in &mut self.dock.windows {
            w.surface.for_each_area_mut(|a| change(&mut a.editor));
        }
    }

    /// Открывает редактор в новой области текущего пространства: делит область с редактором,
    /// подходящим под `near`, а если такой нет — самую большую. `side` и `share` — с какого
    /// края и какой долей встанет новая область (см. [`Place`]).
    pub fn open_editor(&mut self, editor: B::Editor, mut near: impl FnMut(&B::Editor) -> bool, side: Option<Zone>, share: f32) {
        let near = self.surface().areas().into_iter().find(|a| near(&a.editor)).map(|a| a.id);
        self.dock.open_editor(editor, Place { near, side, share });
    }

    fn surface_by_key(&mut self, key: SurfaceKey) -> Option<&mut Surface<B::Editor>> {
        match key {
            SurfaceKey::Main => Some(self.surface_mut()),
            SurfaceKey::Window(id) => self.dock.windows.iter_mut().find(|w| w.id == id).map(|w| &mut w.surface),
        }
    }

    /// Делает область текущей и видимой: если развёрнута другая область, раскладка возвращается.
    pub fn reveal_area(&mut self, key: SurfaceKey, area: AreaId) {
        self.dock.focus(key, area);
        if let Some(s) = self.surface_by_key(key)
            && s.maximized.is_some_and(|m| m != area)
        {
            s.maximized = None;
        }
    }

    /// Копия пространства `i` с новыми номерами областей и редакторов.
    pub fn copy_workspace(&mut self, i: usize) -> Workspace<B> {
        let mut ws = self.workspaces[i].clone();
        ws.name = Some(t!("workspace-copy-name", name = ws.title()));
        let dock = &mut self.dock;
        ws.surface.reassign_ids(|| dock.new_area_id());
        let next = &mut self.next_editor_id;
        let mut new_id = || {
            *next += 1;
            *next
        };
        ws.surface.for_each_area_mut(|a| a.editor.renumber(&mut new_id));
        ws
    }

    /// Копия для стандартной раскладки, которая подходит всем: документы из редакторов убраны.
    pub fn for_default(&self) -> Option<Layout<B>> {
        let value = serde_json::to_value(self).ok()?;
        let mut copy: Layout<B> = serde_json::from_value(value).ok()?;
        copy.for_each_editor_mut(|e| *e = e.for_default_layout());
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

/// Версия формата файла раскладки. Третья — области с одним редактором вместо вкладок.
pub const LAYOUT_VERSION: u32 = 3;

/// Самая старая версия, которую ещё можно прочитать: из областей со вкладками остаётся
/// видимая вкладка.
const OLDEST_LAYOUT: u32 = 2;

/// Читает раскладку. Файл неизвестной версии или испорченный — `None` (тогда берётся запасная).
pub fn read_layout<B: Builtin>(path: &Path) -> Option<(Layout<B>, serde_json::Value)> {
    let file: LayoutFile<B> = crate::settings::read_json(path)?;
    if !(OLDEST_LAYOUT..=LAYOUT_VERSION).contains(&file.version) {
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
pub fn show<T, V: EditorViewer<Editor = T>>(ui: &mut Ui, surface: &mut Surface<T>, dock: &mut DockState<T>, style: &DockStyle, bg: Color32, viewer: &mut V) {
    let ctx = ui.ctx().clone();
    egui::CentralPanel::default().frame(Frame::new().fill(bg)).show(ui, |ui| {
        dock.show_main(ui, surface, viewer, style);
    });
    dock.show_windows(&ctx, viewer, style);
    dock.finish(&ctx, surface, viewer);
}
