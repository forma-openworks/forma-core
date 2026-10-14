//! Отрисовка составного интерфейса в духе Blender: области с одним редактором, разделители,
//! меню вида редактора в заголовке, жесты за уголки области (разделить, объединить, поменять
//! местами, перенести), меню границ и заголовков, отдельные окна ОС.
//!
//! Перенос области за заголовок работает в общих экранных координатах: каждое окно запоминает,
//! где на экране лежат его области, а цель броска вычисляется в [`DockState::finish`] после
//! того, как нарисованы все окна. Пока кнопка мыши зажата, Windows присылает координаты курсора
//! окну-источнику даже за его пределами, поэтому область можно бросить в другое окно или просто
//! на рабочий стол — тогда она становится отдельным окном.
//!
//! Жесты за уголки и разделение из меню работают внутри одного окна.

use std::collections::HashMap;
use std::sync::Arc;

use egui::{
    Align2, Color32, Context, CornerRadius, CursorIcon, FontId, Frame, Id, Key, KeyboardShortcut, Modifiers, Pos2, Rect,
    Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2, ViewportBuilder, ViewportId, pos2, vec2,
};
use forma_i18n::t;
use serde::{Deserialize, Serialize};

use crate::tree::{Area, AreaId, Node, SplitDir, Surface, Zone};

/// Высота заголовка области.
const HEADER_H: f32 = 30.0;
/// Зазор между областями (и от краёв окна).
const GAP: f32 = 4.0;
/// Скругление углов области.
const RADIUS: u8 = 8;
/// Меньше этого область не сжимается разделителем и не делится.
const MIN_AREA: f32 = 72.0;
/// Размер нового отдельного окна, если размер исходной области неизвестен.
const DEFAULT_WINDOW: Vec2 = vec2(720.0, 480.0);
/// Сторона уголка, за который тянут жесты.
const CORNER: f32 = 12.0;
/// На сколько надо отвести мышь от уголка внутрь области, чтобы выбрать направление разреза.
const CORNER_THRESHOLD: f32 = 10.0;

/// Куда поставить новый редактор (см. [`DockState::open_editor`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    /// Какую область делить; `None` (или такой области нет) — самую большую.
    pub near: Option<AreaId>,
    /// С какого края встанет новая область; `None` — вдоль длинной стороны, справа или снизу.
    pub side: Option<Zone>,
    /// Доля новой области.
    pub share: f32,
}

impl Default for Place {
    fn default() -> Self {
        Self { near: None, side: None, share: 0.5 }
    }
}

/// Вид редактора в меню выбора.
#[derive(Clone, Debug)]
pub struct EditorKind {
    pub id: String,
    pub title: String,
    /// Имя значка (см. [`DockStyle::paint_icon`]).
    pub icon: String,
    /// Подпись сочетания клавиш; пусто — сочетания нет.
    pub shortcut: String,
}

/// Столбец меню выбора вида: группа редакторов, как «Общие» или «Анимация» в Blender.
#[derive(Clone, Debug)]
pub struct KindGroup {
    pub title: String,
    pub kinds: Vec<EditorKind>,
}

/// Что приложение рассказывает о своих редакторах и как их рисует.
pub trait EditorViewer {
    type Editor;

    /// Виды редакторов по группам — столбцы меню выбора вида.
    fn kinds(&mut self) -> Vec<KindGroup>;

    /// Вид редактора (`id` из [`Self::kinds`]).
    fn kind(&mut self, editor: &Self::Editor) -> String;

    /// Новый редактор вида `kind`.
    fn create(&mut self, kind: &str) -> Option<Self::Editor>;

    /// Меняет вид редактора области. По умолчанию — новый редактор вместо прежнего.
    fn switch(&mut self, editor: &mut Self::Editor, kind: &str) {
        if let Some(new) = self.create(kind) {
            let old = std::mem::replace(editor, new);
            self.on_close(old);
        }
    }

    /// Подпись редактора: в заголовке отдельного окна и у курсора при переносе.
    fn title(&mut self, editor: &Self::Editor) -> String;

    /// Значок вида редактора — его имя (см. [`DockStyle::paint_icon`]) — и цвет.
    fn icon(&mut self, _editor: &Self::Editor) -> Option<(String, Color32)> {
        None
    }

    /// Постоянный идентификатор редактора (не меняется при переносе между областями).
    fn id(&mut self, editor: &Self::Editor) -> Id;

    /// Заголовок области правее меню вида. По умолчанию — подпись редактора.
    fn header(&mut self, ui: &mut Ui, editor: &mut Self::Editor) {
        let title = self.title(editor);
        ui.label(egui::RichText::new(title).size(13.0));
    }

    /// Содержимое области.
    fn ui(&mut self, ui: &mut Ui, editor: &mut Self::Editor);

    /// Копия редактора — для разделения области и копии в новом окне.
    fn duplicate(&mut self, editor: &Self::Editor) -> Self::Editor;

    /// Редактор закрыли (область закрыта, объединена с соседней или заменена).
    fn on_close(&mut self, _editor: Self::Editor) {}

    /// Начало кадра отдельного окна ОС: у него свой ввод, и общие сочетания клавиш
    /// программа ловит здесь (в главном окне она делает это сама).
    fn window_input(&mut self, _ui: &mut Ui) {}
}

/// Цвета и оформление, которые задаёт приложение.
#[derive(Clone)]
pub struct DockStyle {
    /// Фон в зазорах между областями.
    pub gap_bg: Color32,
    /// Фон содержимого области.
    pub area_bg: Color32,
    /// Фон заголовка области.
    pub header_bg: Color32,
    pub hover_bg: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub text_strong: Color32,
    pub accent: Color32,
    pub border: Color32,
    /// Стиль всплывающих меню.
    pub menu_style: fn(&mut egui::Style),
    /// Рисует значок по имени в прямоугольнике заданным цветом. `false` — такого значка
    /// нет, и раскладка нарисует вместо него символ.
    pub paint_icon: fn(&egui::Painter, Rect, &str, Color32) -> bool,
    /// Иконка отдельных окон.
    pub icon: Option<Arc<egui::IconData>>,
    /// Название программы — в заголовках отдельных окон.
    pub app_title: String,
    /// Сочетания «развернуть область» из раскладки клавиш программы. Их ловит сама
    /// раскладка: разворачивается область под курсором, а не только последняя.
    pub maximize_keys: Vec<KeyboardShortcut>,
    /// Подпись сочетания в меню.
    pub maximize_text: String,
}

/// Какое окно: главное или одно из отдельных.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SurfaceKey {
    Main,
    Window(u64),
}

/// Отдельное окно ОС со своими областями.
#[derive(Serialize, Deserialize)]
#[serde(bound(serialize = "T: Serialize", deserialize = "T: Deserialize<'de>"))]
pub struct FloatWindow<T> {
    pub id: u64,
    pub surface: Surface<T>,
    /// Положение левого верхнего угла окна на экране.
    pub pos: Option<[f32; 2]>,
    /// Размер содержимого окна.
    pub size: [f32; 2],
    /// С какими положением и размером окно создаётся. Не меняется, пока окно открыто,
    /// иначе egui на каждом кадре двигал бы окно обратно.
    #[serde(skip)]
    spawn: Option<(Option<Pos2>, Vec2)>,
}

/// Состояние всех окон: отдельные окна сохраняются, остальное живёт только в памяти.
#[derive(Serialize, Deserialize)]
#[serde(bound(serialize = "T: Serialize", deserialize = "T: Deserialize<'de>"))]
pub struct DockState<T> {
    pub windows: Vec<FloatWindow<T>>,
    next_id: u64,
    #[serde(skip)]
    rt: Runtime<T>,
}

impl<T> Default for DockState<T> {
    fn default() -> Self {
        Self { windows: Vec::new(), next_id: 0, rt: Runtime::default() }
    }
}

struct Runtime<T> {
    /// Область переносят за заголовок.
    drag: Option<Drag>,
    /// Куда упадёт переносимая область (вычислено в прошлом кадре).
    target: Target,
    /// Жест за уголок области.
    corner: Option<CornerDrag>,
    /// Разделение из меню: линия идёт за курсором, щелчок — разрезать.
    pick: Option<SplitPick>,
    /// Где на экране нарисованы области в этом кадре.
    rects: Vec<SurfaceRects>,
    /// То же для прошлого кадра — для действий между кадрами (например, «вынести в окно»).
    prev_rects: Vec<SurfaceRects>,
    /// Последняя область, по которой щёлкали, в каждом окне.
    focused: HashMap<SurfaceKey, AreaId>,
    ops: Vec<Op<T>>,
}

impl<T> Default for Runtime<T> {
    fn default() -> Self {
        Self {
            drag: None,
            target: Target::Nothing,
            corner: None,
            pick: None,
            rects: Vec::new(),
            prev_rects: Vec::new(),
            focused: HashMap::new(),
            ops: Vec::new(),
        }
    }
}

struct Drag {
    src: SurfaceKey,
    area: AreaId,
    title: String,
    /// Курсор в экранных координатах.
    pointer: Option<Pos2>,
    released: bool,
    /// Размер исходной области — такого размера будет новое окно.
    size: Vec2,
}

struct CornerDrag {
    key: SurfaceKey,
    area: AreaId,
    /// Уголок, за который потянули (координаты окна).
    corner: Pos2,
    /// Направление разреза: выбирается первым движением внутрь, Tab меняет.
    dir: Option<SplitDir>,
    action: CornerAction,
}

/// Что сделает жест за уголок, если отпустить мышь сейчас.
#[derive(Clone, Copy, Debug, PartialEq)]
enum CornerAction {
    None,
    /// Разделить область: копия встаёт с края `zone` и занимает долю `ratio`.
    Split { zone: Zone, ratio: f32 },
    /// Область поглощает соседнюю.
    Join(AreaId),
    /// Поменять редакторы двух областей местами (с Ctrl).
    Swap(AreaId),
    /// Перенести область к краю другой, а в середину — на её место.
    Dock(AreaId, Zone),
}

struct SplitPick {
    key: SurfaceKey,
    dir: SplitDir,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    Area { key: SurfaceKey, area: AreaId, zone: Zone },
    /// Пустое окно.
    Empty(SurfaceKey),
    /// Мимо всех окон — новое окно в этой точке экрана.
    NewWindow(Pos2),
    Nothing,
}

struct SurfaceRects {
    key: SurfaceKey,
    /// Окно на экране.
    window: Rect,
    areas: Vec<AreaRects>,
}

/// Прямоугольники области.
struct AreaRects {
    id: AreaId,
    header: Rect,
    body: Rect,
}

impl AreaRects {
    fn full(&self) -> Rect {
        self.header.union(self.body)
    }
}

enum Op<T> {
    /// Перенести область в другое место (`Center` — на место другой области).
    Move { from: (SurfaceKey, AreaId), to: Target },
    /// Разделить область, положив в новую часть копию редактора.
    Split { key: SurfaceKey, area: AreaId, zone: Zone, ratio: f32 },
    Join { key: SurfaceKey, keep: AreaId, remove: AreaId },
    Swap { key: SurfaceKey, a: AreaId, b: AreaId },
    /// Поменять местами половины разбиения (меню границы).
    SwapSplit { key: SurfaceKey, first: AreaId, second: AreaId },
    /// Новый редактор вида `kind` в пустом окне.
    Create { key: SurfaceKey, kind: String },
    /// Добавить редактор: разделить область `near` (или самую большую).
    Add { key: SurfaceKey, editor: T, place: Place },
    CloseArea { key: SurfaceKey, area: AreaId },
    Detach { key: SurfaceKey, area: AreaId, pos: Option<Pos2>, size: Vec2 },
    DuplicateWindow { key: SurfaceKey, area: AreaId, size: Vec2 },
    Return(u64),
    ToggleMaximize { key: SurfaceKey, area: AreaId },
}

fn surface_mut<'a, T>(main: &'a mut Surface<T>, windows: &'a mut [FloatWindow<T>], key: SurfaceKey) -> Option<&'a mut Surface<T>> {
    match key {
        SurfaceKey::Main => Some(main),
        SurfaceKey::Window(id) => windows.iter_mut().find(|w| w.id == id).map(|w| &mut w.surface),
    }
}

fn surface_ref<'a, T>(main: &'a Surface<T>, windows: &'a [FloatWindow<T>], key: SurfaceKey) -> Option<&'a Surface<T>> {
    match key {
        SurfaceKey::Main => Some(main),
        SurfaceKey::Window(id) => windows.iter().find(|w| w.id == id).map(|w| &w.surface),
    }
}

impl<T> DockState<T> {
    /// Новый номер для области или окна.
    pub fn new_area_id(&mut self) -> AreaId {
        self.next_id += 1;
        AreaId(self.next_id)
    }

    /// Следит, чтобы новые номера не повторяли уже занятые (после загрузки раскладки).
    pub fn reserve_ids(&mut self, used_max: u64) {
        let windows_max = self.windows.iter().map(|w| w.id.max(w.surface.max_area_id())).max().unwrap_or(0);
        self.next_id = self.next_id.max(used_max).max(windows_max);
    }

    /// Последняя область, с которой работали в окне.
    pub fn focused(&self, key: SurfaceKey) -> Option<AreaId> {
        self.rt.focused.get(&key).copied()
    }

    /// Делает область текущей (например, когда в ней открыли файл).
    pub fn focus(&mut self, key: SurfaceKey, area: AreaId) {
        self.rt.focused.insert(key, area);
    }

    /// Идёт перенос области, жест за уголок или разделение из меню.
    pub fn is_dragging(&self) -> bool {
        !self.idle()
    }

    fn idle(&self) -> bool {
        self.rt.drag.is_none() && self.rt.corner.is_none() && self.rt.pick.is_none()
    }

    /// Чинит загруженные окна и убирает пустые.
    pub fn sanitize(&mut self) {
        for w in &mut self.windows {
            w.surface.sanitize();
        }
        self.windows.retain(|w| !w.surface.is_empty());
    }

    /// Открывает редактор в главном окне: делит область по правилам `place`.
    /// В пустом окне редактор займёт всё окно.
    pub fn open_editor(&mut self, editor: T, place: Place) {
        self.rt.ops.push(Op::Add { key: SurfaceKey::Main, editor, place });
    }

    /// Выносит последнюю область главного окна в отдельное окно.
    pub fn detach_focused(&mut self, main: &Surface<T>) {
        let Some(id) = self.focused(SurfaceKey::Main).filter(|id| main.contains(*id)).or_else(|| main.first_area_id()) else {
            return;
        };
        let size = self.area_size(SurfaceKey::Main, id);
        self.rt.ops.push(Op::Detach { key: SurfaceKey::Main, area: id, pos: None, size });
    }

    /// Разворачивает (или возвращает) последнюю область главного окна.
    pub fn toggle_maximize_focused(&mut self, main: &Surface<T>) {
        if let Some(area) = main.maximized.or_else(|| self.focused(SurfaceKey::Main).filter(|id| main.contains(*id))).or_else(|| main.first_area_id()) {
            self.rt.ops.push(Op::ToggleMaximize { key: SurfaceKey::Main, area });
        }
    }

    /// Возвращает все отдельные окна в главное.
    pub fn return_all(&mut self) {
        for w in &self.windows {
            self.rt.ops.push(Op::Return(w.id));
        }
    }

    fn area_rect(&self, key: SurfaceKey, area: AreaId) -> Option<Rect> {
        self.rt.prev_rects.iter().filter(|s| s.key == key).flat_map(|s| &s.areas).find(|a| a.id == area).map(AreaRects::full)
    }

    fn area_size(&self, key: SurfaceKey, area: AreaId) -> Vec2 {
        self.area_rect(key, area).map_or(DEFAULT_WINDOW, |r| r.size())
    }

    /// Где поставить новый редактор: какую область делить и с какого края.
    /// `None` — окно пустое, редактор займёт его целиком.
    fn place_spot(&self, key: SurfaceKey, s: &Surface<T>, place: Place) -> Option<(AreaId, Zone)> {
        let largest = || {
            s.areas()
                .iter()
                .filter_map(|a| self.area_rect(key, a.id).map(|r| (a.id, r.area())))
                .max_by(|x, y| x.1.total_cmp(&y.1))
                .map(|(id, _)| id)
        };
        let target = place
            .near
            .filter(|a| s.contains(*a))
            .or_else(largest)
            .or_else(|| self.focused(key).filter(|a| s.contains(*a)))
            .or_else(|| s.first_area_id())?;
        let zone = match (place.side.filter(|z| z.split().is_some()), self.area_rect(key, target)) {
            (Some(side), _) => side,
            (None, Some(r)) if r.height() > r.width() * 1.1 => Zone::Bottom,
            (None, _) => Zone::Right,
        };
        Some((target, zone))
    }
}

impl<T> DockState<T> {
    /// Рисует главное окно. Вызывать внутри центральной панели.
    pub fn show_main(&mut self, ui: &mut Ui, main: &mut Surface<T>, viewer: &mut impl EditorViewer<Editor = T>, style: &DockStyle) {
        self.show_surface(ui, SurfaceKey::Main, main, viewer, style);
    }

    /// Рисует отдельные окна (каждое — своё окно ОС).
    pub fn show_windows(&mut self, ctx: &Context, viewer: &mut impl EditorViewer<Editor = T>, style: &DockStyle) {
        for idx in 0..self.windows.len() {
            let w = &mut self.windows[idx];
            let id = w.id;
            let (spawn_pos, spawn_size) = *w.spawn.get_or_insert_with(|| (w.pos.map(|p| pos2(p[0], p[1])), vec2(w.size[0], w.size[1])));
            let title = w
                .surface
                .areas()
                .first()
                .map(|a| format!("{} — {}", viewer.title(&a.editor), style.app_title))
                .unwrap_or_else(|| style.app_title.clone());
            let mut surface = std::mem::take(&mut w.surface);

            let mut builder = ViewportBuilder::default()
                .with_title(title)
                .with_inner_size(spawn_size)
                .with_min_inner_size([320.0, 200.0]);
            if let Some(p) = spawn_pos {
                builder = builder.with_position(p);
            }
            if let Some(icon) = &style.icon {
                builder = builder.with_icon(icon.clone());
            }
            let viewport = ViewportId::from_hash_of(("forma-dock-window", id));
            let (outer, inner, close) = ctx.show_viewport_immediate(viewport, builder, |ui, _class| {
                egui::CentralPanel::default().frame(Frame::new().fill(style.gap_bg)).show(ui, |ui| {
                    self.show_surface(ui, SurfaceKey::Window(id), &mut surface, viewer, style);
                });
                ui.ctx().input(|i| {
                    let vp = i.viewport();
                    (vp.outer_rect, vp.inner_rect, vp.close_requested())
                })
            });

            let w = &mut self.windows[idx];
            w.surface = surface;
            if let Some(r) = outer {
                w.pos = Some([r.min.x, r.min.y]);
            }
            if let Some(r) = inner {
                w.size = [r.width(), r.height()];
            }
            if close {
                // Закрытое окно не теряет редакторы: они возвращаются в главное.
                self.rt.ops.push(Op::Return(id));
            }
        }
    }

    /// Завершает кадр: применяет действия, вычисляет цель переноса.
    /// Вызывать после [`Self::show_main`] и [`Self::show_windows`].
    pub fn finish(&mut self, ctx: &Context, main: &mut Surface<T>, viewer: &mut impl EditorViewer<Editor = T>) {
        let rects = std::mem::take(&mut self.rt.rects);
        if let Some(drag) = &self.rt.drag {
            // Окно-источник пропало (например, закрылось) — перенос отменяется.
            if !rects.iter().any(|s| s.key == drag.src) {
                self.rt.drag = None;
            }
        }
        if let Some(g) = &self.rt.corner
            && !rects.iter().any(|s| s.key == g.key)
        {
            self.rt.corner = None;
        }
        if let Some(p) = &self.rt.pick
            && !rects.iter().any(|s| s.key == p.key)
        {
            self.rt.pick = None;
        }
        self.rt.target = Target::Nothing;
        if let Some(drag) = &self.rt.drag {
            let target = drag.pointer.map_or(Target::Nothing, |p| compute_target(&rects, p));
            let target = match target {
                Target::Area { key, area, .. } if key == drag.src && area == drag.area => Target::Nothing,
                t => t,
            };
            if drag.released {
                let drag = self.rt.drag.take().expect("проверено выше");
                match target {
                    Target::NewWindow(p) => self.rt.ops.push(Op::Detach {
                        key: drag.src,
                        area: drag.area,
                        pos: Some(p - vec2(24.0, 14.0)),
                        size: drag.size,
                    }),
                    Target::Nothing => {}
                    to => self.rt.ops.push(Op::Move { from: (drag.src, drag.area), to }),
                }
            } else {
                self.rt.target = target;
                ctx.request_repaint_of(ViewportId::ROOT);
            }
        }
        // Следующий кадр собирает прямоугольники заново.
        self.rt.prev_rects = rects;

        for op in std::mem::take(&mut self.rt.ops) {
            self.apply(op, main, viewer);
        }
        self.windows.retain(|w| !w.surface.is_empty());
        let open: Vec<SurfaceKey> = self.windows.iter().map(|w| SurfaceKey::Window(w.id)).collect();
        self.rt.focused.retain(|k, _| *k == SurfaceKey::Main || open.contains(k));
    }

    fn apply(&mut self, op: Op<T>, main: &mut Surface<T>, viewer: &mut impl EditorViewer<Editor = T>) {
        match op {
            Op::Move { from: (key, area), to } => {
                let valid = match to {
                    Target::Area { key: tk, area: ta, .. } => {
                        !(tk == key && ta == area) && surface_ref(main, &self.windows, tk).is_some_and(|s| s.contains(ta))
                    }
                    Target::Empty(tk) => surface_ref(main, &self.windows, tk).is_some_and(Surface::is_empty),
                    Target::NewWindow(_) => true,
                    Target::Nothing => false,
                };
                if !valid {
                    return;
                }
                let Some(moved) = surface_mut(main, &mut self.windows, key).and_then(|s| s.remove_area(area)) else { return };
                match to {
                    Target::Area { key: tk, area: ta, zone } => {
                        let s = surface_mut(main, &mut self.windows, tk).expect("проверено выше");
                        s.maximized = None;
                        if zone == Zone::Center {
                            if let Ok(old) = s.replace_editor(ta, moved.editor) {
                                viewer.on_close(old);
                            }
                            self.rt.focused.insert(tk, ta);
                        } else if let Err(a) = s.split_area(ta, zone, moved, 0.5) {
                            viewer.on_close(a.editor);
                        } else {
                            self.rt.focused.insert(tk, area);
                        }
                    }
                    Target::Empty(tk) => {
                        let s = surface_mut(main, &mut self.windows, tk).expect("проверено выше");
                        s.root = Some(Node::Area(moved));
                        self.rt.focused.insert(tk, area);
                    }
                    Target::NewWindow(p) => self.spawn_window(moved.editor, Some(p), DEFAULT_WINDOW),
                    Target::Nothing => {}
                }
            }
            Op::Split { key, area, zone, ratio } => {
                let id = self.new_area_id();
                let Some(s) = surface_mut(main, &mut self.windows, key) else { return };
                let Some(copy) = s.area(area).map(|a| viewer.duplicate(&a.editor)) else { return };
                match s.split_area(area, zone, Area::new(id, copy), ratio) {
                    Ok(()) => {
                        self.rt.focused.insert(key, id);
                    }
                    Err(a) => viewer.on_close(a.editor),
                }
            }
            Op::Join { key, keep, remove } => {
                if let Some(editor) = surface_mut(main, &mut self.windows, key).and_then(|s| s.join(keep, remove)) {
                    viewer.on_close(editor);
                    self.rt.focused.insert(key, keep);
                }
            }
            Op::Swap { key, a, b } => {
                if surface_mut(main, &mut self.windows, key).is_some_and(|s| s.swap_editors(a, b)) {
                    self.rt.focused.insert(key, b);
                }
            }
            Op::SwapSplit { key, first, second } => {
                if let Some(s) = surface_mut(main, &mut self.windows, key) {
                    s.swap_split(first, second);
                }
            }
            Op::Create { key, kind } => {
                if let Some(editor) = viewer.create(&kind) {
                    self.add_editor(main, key, editor, Place::default(), viewer);
                }
            }
            Op::Add { key, editor, place } => self.add_editor(main, key, editor, place, viewer),
            Op::CloseArea { key, area } => {
                if let Some(a) = surface_mut(main, &mut self.windows, key).and_then(|s| s.remove_area(area)) {
                    viewer.on_close(a.editor);
                }
            }
            Op::Detach { key, area, pos, size } => {
                if let Some(a) = surface_mut(main, &mut self.windows, key).and_then(|s| s.remove_area(area)) {
                    self.spawn_window(a.editor, pos, size);
                }
            }
            Op::DuplicateWindow { key, area, size } => {
                if let Some(copy) = surface_ref(main, &self.windows, key).and_then(|s| s.area(area)).map(|a| viewer.duplicate(&a.editor)) {
                    self.spawn_window(copy, None, size);
                }
            }
            Op::Return(id) => {
                let Some(i) = self.windows.iter().position(|w| w.id == id) else { return };
                let mut w = self.windows.remove(i);
                for editor in w.surface.take_all() {
                    self.add_editor(main, SurfaceKey::Main, editor, Place::default(), viewer);
                }
            }
            Op::ToggleMaximize { key, area } => {
                if let Some(s) = surface_mut(main, &mut self.windows, key) {
                    s.maximized = if s.maximized == Some(area) { None } else { Some(area) };
                }
            }
        }
    }

    /// Кладёт редактор в окно `key` (если окна уже нет — в главное).
    fn add_editor(&mut self, main: &mut Surface<T>, key: SurfaceKey, editor: T, place: Place, viewer: &mut impl EditorViewer<Editor = T>) {
        let key = if surface_ref(main, &self.windows, key).is_some() { key } else { SurfaceKey::Main };
        let spot = self.place_spot(key, surface_ref(main, &self.windows, key).expect("окно есть"), place);
        let id = self.new_area_id();
        let s = surface_mut(main, &mut self.windows, key).expect("окно есть");
        s.maximized = None;
        match spot {
            None => s.root = Some(Node::area(id, editor)),
            Some((target, zone)) => {
                if let Err(a) = s.split_area(target, zone, Area::new(id, editor), place.share) {
                    viewer.on_close(a.editor);
                    return;
                }
            }
        }
        self.rt.focused.insert(key, id);
    }

    fn spawn_window(&mut self, editor: T, pos: Option<Pos2>, size: Vec2) {
        let id = self.new_area_id().0;
        let area = self.new_area_id();
        let size = vec2(size.x.max(360.0), size.y.max(240.0));
        self.windows.push(FloatWindow {
            id,
            surface: Surface::new(Node::area(area, editor)),
            pos: pos.map(|p| [p.x, p.y]),
            size: [size.x, size.y],
            spawn: None,
        });
    }
}

/// Куда упадёт область, если отпустить её в точке `p` (экранные координаты).
fn compute_target(rects: &[SurfaceRects], p: Pos2) -> Target {
    // Отдельные окна обычно лежат поверх главного, поэтому проверяем их первыми.
    let ordered = rects.iter().rev().filter(|s| s.key != SurfaceKey::Main).chain(rects.iter().filter(|s| s.key == SurfaceKey::Main));
    for s in ordered {
        if !s.window.contains(p) {
            continue;
        }
        for a in &s.areas {
            if a.full().contains(p) {
                return Target::Area { key: s.key, area: a.id, zone: zone_at(a.full(), p) };
            }
        }
        return if s.areas.is_empty() { Target::Empty(s.key) } else { Target::Nothing };
    }
    Target::NewWindow(p)
}

/// Зона области под курсором: середина — заменить, края — встать рядом.
fn zone_at(rect: Rect, p: Pos2) -> Zone {
    let rel = (p - rect.min) / rect.size().max(vec2(1.0, 1.0));
    let dx = rel.x.min(1.0 - rel.x);
    let dy = rel.y.min(1.0 - rel.y);
    if dx.min(dy) > 0.28 {
        Zone::Center
    } else if dx < dy {
        if rel.x < 0.5 { Zone::Left } else { Zone::Right }
    } else if rel.y < 0.5 {
        Zone::Top
    } else {
        Zone::Bottom
    }
}

/// Прямоугольник, который подсвечивается для зоны области.
fn zone_rect(r: Rect, zone: Zone) -> Rect {
    match zone {
        Zone::Center => r,
        Zone::Left => Rect::from_min_max(r.min, pos2(r.center().x, r.max.y)),
        Zone::Right => Rect::from_min_max(pos2(r.center().x, r.min.y), r.max),
        Zone::Top => Rect::from_min_max(r.min, pos2(r.max.x, r.center().y)),
        Zone::Bottom => Rect::from_min_max(pos2(r.min.x, r.center().y), r.max),
    }
}

fn split_rect(rect: Rect, dir: SplitDir, ratio: f32) -> (Rect, Rect, Rect) {
    match dir {
        SplitDir::Horizontal => {
            let w = rect.width() - GAP;
            let x = rect.left() + w * ratio;
            (
                Rect::from_min_max(rect.min, pos2(x, rect.max.y)),
                Rect::from_min_max(pos2(x, rect.min.y), pos2(x + GAP, rect.max.y)),
                Rect::from_min_max(pos2(x + GAP, rect.min.y), rect.max),
            )
        }
        SplitDir::Vertical => {
            let h = rect.height() - GAP;
            let y = rect.top() + h * ratio;
            (
                Rect::from_min_max(rect.min, pos2(rect.max.x, y)),
                Rect::from_min_max(pos2(rect.min.x, y), pos2(rect.max.x, y + GAP)),
                Rect::from_min_max(pos2(rect.min.x, y + GAP), rect.max),
            )
        }
    }
}

/// Доля первой части, если разрезать `rect` в направлении `dir` через точку `p`.
/// `None` — одна из частей вышла бы меньше допустимого.
fn cut_ratio(rect: Rect, dir: SplitDir, p: Pos2) -> Option<f32> {
    let (len, at) = match dir {
        SplitDir::Horizontal => (rect.width(), p.x - rect.left()),
        SplitDir::Vertical => (rect.height(), p.y - rect.top()),
    };
    let avail = len - GAP;
    let first = at - GAP / 2.0;
    (first >= MIN_AREA && avail - first >= MIN_AREA).then(|| first / avail)
}

/// Что сделает жест за уголок `corner` области `src`, если отпустить мышь в точке `p`
/// (всё — в координатах окна). Внутри своей области — разрез (направление выбирается первым
/// движением и запоминается в `dir`), в соседней — объединение, с Ctrl — обмен, в любой
/// другой — перенос к её краю или на её место.
fn corner_action(areas: &[AreaRects], src: AreaId, corner: Pos2, p: Pos2, dir: &mut Option<SplitDir>, joinable: &[AreaId], ctrl: bool) -> CornerAction {
    let Some(own) = areas.iter().find(|a| a.id == src) else { return CornerAction::None };
    let full = own.full();
    match areas.iter().find(|a| a.full().contains(p)) {
        Some(a) if a.id == src => {
            let d = p - corner;
            if dir.is_none() {
                if d.length() < CORNER_THRESHOLD {
                    return CornerAction::None;
                }
                // Ведут вбок — линия разреза вертикальная, области встанут рядом.
                *dir = Some(if d.x.abs() >= d.y.abs() { SplitDir::Horizontal } else { SplitDir::Vertical });
            }
            let dir = dir.expect("выбрано выше");
            let Some(first) = cut_ratio(full, dir, p) else { return CornerAction::None };
            // Новая часть — со стороны уголка.
            match dir {
                SplitDir::Horizontal if corner.x < full.center().x => CornerAction::Split { zone: Zone::Left, ratio: first },
                SplitDir::Horizontal => CornerAction::Split { zone: Zone::Right, ratio: 1.0 - first },
                SplitDir::Vertical if corner.y < full.center().y => CornerAction::Split { zone: Zone::Top, ratio: first },
                SplitDir::Vertical => CornerAction::Split { zone: Zone::Bottom, ratio: 1.0 - first },
            }
        }
        Some(a) if ctrl => CornerAction::Swap(a.id),
        Some(a) if joinable.contains(&a.id) => CornerAction::Join(a.id),
        Some(a) => CornerAction::Dock(a.id, zone_at(a.full(), p)),
        None => CornerAction::None,
    }
}

// ============================================================================================
// Отрисовка
// ============================================================================================

/// Что нарисовано в одном окне в этом кадре (в координатах окна).
struct Frame0 {
    key: SurfaceKey,
    areas: Vec<AreaRects>,
}

impl<T> DockState<T> {
    fn show_surface(&mut self, ui: &mut Ui, key: SurfaceKey, surface: &mut Surface<T>, viewer: &mut impl EditorViewer<Editor = T>, style: &DockStyle) {
        let ctx = ui.ctx().clone();
        if matches!(key, SurfaceKey::Window(_)) {
            viewer.window_input(ui);
        }
        let full = ui.available_rect_before_wrap();
        let rect = full.shrink(GAP);
        // Зазоры — цветом зазора (им же закрашиваются углы областей).
        ui.painter().rect_filled(full, CornerRadius::ZERO, style.gap_bg);
        let mut frame = Frame0 { key, areas: Vec::new() };

        match &mut surface.root {
            None => self.empty_surface(ui, key, rect, viewer, style),
            Some(root) => {
                let maximized = surface.maximized;
                match maximized.and_then(|id| root.find_mut(id)) {
                    Some(area) => self.show_area(ui, &mut frame, area, rect, true, viewer, style),
                    None => {
                        surface.maximized = None;
                        self.show_node(ui, &mut frame, root, rect, viewer, style);
                    }
                }
            }
        }

        // Сочетание разворачивает область под курсором (или последнюю, с которой работали).
        if style.maximize_keys.iter().any(|k| ui.input_mut(|i| i.consume_shortcut(k))) {
            let hovered = ui.input(|i| i.pointer.hover_pos()).and_then(|p| frame.areas.iter().find(|a| a.full().contains(p)).map(|a| a.id));
            let area = surface.maximized.or(hovered).or_else(|| self.focused(key).filter(|id| surface.contains(*id))).or_else(|| surface.first_area_id());
            if let Some(area) = area {
                self.rt.ops.push(Op::ToggleMaximize { key, area });
            }
        }

        self.track_corner(ui, key, surface, &frame, rect, style);
        self.track_pick(ui, key, &frame, rect, style);

        // Экранные координаты окна: от них считаются броски между окнами.
        let window = ctx.input(|i| i.viewport().inner_rect);
        if let Some(window) = window {
            let offset = window.min.to_vec2();
            let shift = |r: Rect| r.translate(offset);
            self.rt.rects.push(SurfaceRects {
                key,
                window,
                areas: frame.areas.iter().map(|a| AreaRects { id: a.id, header: shift(a.header), body: shift(a.body) }).collect(),
            });
        }

        self.track_drag(ui, key, window.map(|w| w.min.to_vec2()), style);
        self.paint_drop_overlay(ui, &frame, rect, style);
    }

    /// Жест за уголок: каждый кадр пересчитывает, что произойдёт, рисует подсказку,
    /// а когда кнопку отпускают — выполняет.
    fn track_corner(&mut self, ui: &mut Ui, key: SurfaceKey, surface: &Surface<T>, frame: &Frame0, surface_rect: Rect, style: &DockStyle) {
        let Some(g) = &mut self.rt.corner else { return };
        if g.key != key {
            return;
        }
        let (p, down, cancel, ctrl, tab) = ui.input_mut(|i| {
            (
                i.pointer.latest_pos(),
                i.pointer.primary_down(),
                i.consume_key(Modifiers::NONE, Key::Escape) || i.pointer.secondary_pressed(),
                i.modifiers.ctrl,
                i.consume_key(Modifiers::NONE, Key::Tab),
            )
        });
        if cancel || !frame.areas.iter().any(|a| a.id == g.area) {
            self.rt.corner = None;
            return;
        }
        if tab && let Some(d) = &mut g.dir {
            *d = d.flip();
        }
        let joinable: Vec<AreaId> = surface.siblings().into_iter().filter(|(a, _, _)| *a == g.area).map(|(_, b, _)| b).collect();
        if let Some(p) = p {
            g.action = corner_action(&frame.areas, g.area, g.corner, p, &mut g.dir, &joinable, ctrl);
        }
        if !down {
            let (area, action) = (g.area, g.action);
            self.rt.corner = None;
            let op = match action {
                CornerAction::None => None,
                CornerAction::Split { zone, ratio } => Some(Op::Split { key, area, zone, ratio }),
                CornerAction::Join(other) => Some(Op::Join { key, keep: area, remove: other }),
                CornerAction::Swap(other) => Some(Op::Swap { key, a: area, b: other }),
                CornerAction::Dock(other, zone) => Some(Op::Move { from: (key, area), to: Target::Area { key, area: other, zone } }),
            };
            self.rt.ops.extend(op);
            return;
        }

        ui.ctx().set_cursor_icon(match g.action {
            CornerAction::Dock(..) | CornerAction::Swap(_) => CursorIcon::Move,
            _ => CursorIcon::Crosshair,
        });
        let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new(("forma-dock-gesture", key))));
        let Some(own) = frame.areas.iter().find(|a| a.id == g.area).map(AreaRects::full) else { return };
        let rect_of = |id: AreaId| frame.areas.iter().find(|a| a.id == id).map(AreaRects::full);
        let fill = style.accent.gamma_multiply(0.18);
        let stroke = Stroke::new(1.5, style.accent.gamma_multiply(0.9));
        let hint = match g.action {
            CornerAction::None => t!("dock-gesture-none"),
            CornerAction::Split { zone, ratio } => {
                let (dir, new_first) = zone.split().expect("у разреза есть край");
                let first = if new_first { ratio } else { 1.0 - ratio };
                let (r1, sep, r2) = split_rect(own, dir, first);
                painter.rect_filled(if new_first { r1 } else { r2 }, CornerRadius::same(RADIUS), style.accent.gamma_multiply(0.08));
                paint_cut(&painter, sep, dir, style);
                t!("dock-gesture-split")
            }
            CornerAction::Join(other) => {
                if let Some(r) = rect_of(other) {
                    painter.rect_filled(r, CornerRadius::same(RADIUS), Color32::from_black_alpha(110));
                    painter.rect_stroke(own, CornerRadius::same(RADIUS), stroke, StrokeKind::Inside);
                    // Стрелка из остающейся области в ту, что исчезнет.
                    let d = r.center() - own.center();
                    let d = if d.x.abs() > d.y.abs() { vec2(d.x.signum(), 0.0) } else { vec2(0.0, d.y.signum()) };
                    let len = 48.0_f32.min(r.width().min(r.height()) * 0.4);
                    painter.arrow(r.center() - d * len / 2.0, d * len, Stroke::new(3.0, style.text_strong));
                }
                t!("dock-gesture-join")
            }
            CornerAction::Swap(other) => {
                for r in [Some(own), rect_of(other)].into_iter().flatten() {
                    painter.rect(r, CornerRadius::same(RADIUS), fill, stroke, StrokeKind::Inside);
                }
                t!("dock-gesture-swap")
            }
            CornerAction::Dock(other, zone) => {
                if let Some(r) = rect_of(other) {
                    painter.rect(zone_rect(r, zone).shrink(3.0), CornerRadius::same(RADIUS), fill, stroke, StrokeKind::Inside);
                }
                if zone == Zone::Center { t!("dock-gesture-replace") } else { t!("dock-gesture-dock") }
            }
        };
        paint_hint(&painter, surface_rect, &format!("{hint}   ·   {}", t!("dock-gesture-keys")), style);
    }

    /// Разделение из меню: линия разреза идёт за курсором по области под ним.
    fn track_pick(&mut self, ui: &mut Ui, key: SurfaceKey, frame: &Frame0, surface_rect: Rect, style: &DockStyle) {
        let Some(pick) = &mut self.rt.pick else { return };
        if pick.key != key {
            return;
        }
        // Ловушка поверх областей: щелчок для разреза не должен достаться редактору.
        let resp = ui.interact(surface_rect, Id::new(("forma-dock-pick", key)), Sense::click());
        let (p, cancel, tab) = ui.input_mut(|i| {
            (i.pointer.latest_pos(), i.consume_key(Modifiers::NONE, Key::Escape), i.consume_key(Modifiers::NONE, Key::Tab))
        });
        if cancel || resp.secondary_clicked() {
            self.rt.pick = None;
            return;
        }
        if tab {
            pick.dir = pick.dir.flip();
        }
        let dir = pick.dir;
        let cut = p.and_then(|p| {
            let a = frame.areas.iter().find(|a| a.full().contains(p))?;
            Some((a.id, a.full(), cut_ratio(a.full(), dir, p)?))
        });
        ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
        let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new(("forma-dock-gesture", key))));
        if let Some((_, rect, first)) = cut {
            let (_, sep, _) = split_rect(rect, dir, first);
            paint_cut(&painter, sep, dir, style);
        }
        paint_hint(&painter, surface_rect, &t!("dock-split-hint"), style);
        if resp.clicked() {
            self.rt.pick = None;
            if let Some((area, _, first)) = cut {
                let zone = if dir == SplitDir::Horizontal { Zone::Right } else { Zone::Bottom };
                self.rt.ops.push(Op::Split { key, area, zone, ratio: 1.0 - first });
            }
        }
    }

    /// Обновляет положение переносимой области, если её тащат из этого окна.
    fn track_drag(&mut self, ui: &Ui, key: SurfaceKey, offset: Option<Vec2>, style: &DockStyle) {
        let Some(drag) = &mut self.rt.drag else { return };
        if drag.src != key {
            return;
        }
        let (local, down, cancel) = ui.input_mut(|i| {
            (i.pointer.latest_pos().or(i.pointer.interact_pos()), i.pointer.primary_down(), i.consume_key(Modifiers::NONE, Key::Escape))
        });
        if cancel {
            self.rt.drag = None;
            return;
        }
        if let (Some(p), Some(off)) = (local, offset) {
            drag.pointer = Some(p + off);
        }
        if !down {
            drag.released = true;
        }
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        // «Призрак» области у курсора, пока курсор над окном-источником.
        if let Some(p) = local.filter(|p| ui.ctx().content_rect().contains(*p)) {
            let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, Id::new("forma-dock-ghost")));
            let galley = painter.layout_no_wrap(drag.title.clone(), FontId::proportional(13.0), style.text_strong);
            let r = Rect::from_min_size(p + vec2(14.0, 10.0), galley.size() + vec2(20.0, 10.0));
            painter.rect(r, CornerRadius::same(7), style.header_bg, Stroke::new(1.0, style.accent), StrokeKind::Inside);
            painter.galley(r.min + vec2(10.0, 5.0), galley, style.text_strong);
        }
    }

    fn paint_drop_overlay(&self, ui: &Ui, frame: &Frame0, surface_rect: Rect, style: &DockStyle) {
        if self.rt.drag.is_none() {
            return;
        }
        let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("forma-dock-overlay")));
        let fill = style.accent.gamma_multiply(0.22);
        let stroke = Stroke::new(1.5, style.accent.gamma_multiply(0.9));
        match self.rt.target {
            Target::Area { key, area, zone } if key == frame.key => {
                if let Some(a) = frame.areas.iter().find(|a| a.id == area) {
                    painter.rect(zone_rect(a.full(), zone).shrink(3.0), CornerRadius::same(RADIUS), fill, stroke, StrokeKind::Inside);
                }
            }
            Target::Empty(key) if key == frame.key => {
                painter.rect(surface_rect, CornerRadius::same(RADIUS), fill, stroke, StrokeKind::Inside);
            }
            _ => {}
        }
    }

    fn empty_surface(&mut self, ui: &mut Ui, key: SurfaceKey, rect: Rect, viewer: &mut impl EditorViewer<Editor = T>, style: &DockStyle) {
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(RADIUS), style.area_bg);
        let mut child = ui.new_child(UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Center)));
        child.add_space((rect.height() * 0.38 - 40.0).max(8.0));
        child.label(egui::RichText::new(t!("dock-empty-title")).size(16.0).color(style.text_strong));
        child.add_space(4.0);
        child.label(egui::RichText::new(t!("dock-empty-hint")).size(13.0).color(style.text_muted));
        child.add_space(12.0);
        let resp = child.button(t!("dock-empty-button"));
        let picked = egui::Popup::menu(&resp)
            .style(style.menu_style)
            .show(|ui| {
                let groups = viewer.kinds();
                kind_menu(ui, &groups, None, style)
            })
            .and_then(|r| r.inner);
        if let Some(kind) = picked {
            self.rt.ops.push(Op::Create { key, kind });
        }
    }

    fn show_node(&mut self, ui: &mut Ui, frame: &mut Frame0, node: &mut Node<T>, rect: Rect, viewer: &mut impl EditorViewer<Editor = T>, style: &DockStyle) {
        match node {
            Node::Area(area) => self.show_area(ui, frame, area, rect, false, viewer, style),
            Node::Split { dir, ratio, children } => {
                let (r1, sep, r2) = split_rect(rect, *dir, *ratio);
                let key = frame.key;
                let first = children[0].first_area().id;
                let second = children[1].first_area().id;
                let both_areas = matches!(&**children, [Node::Area(_), Node::Area(_)]);
                let id = Id::new(("forma-dock-sep", key, first, second));
                let resp = ui.interact(sep.expand(2.0), id, Sense::click_and_drag());
                let horizontal = *dir == SplitDir::Horizontal;
                let idle = self.idle();
                if idle && (resp.hovered() || resp.dragged()) {
                    ui.ctx().set_cursor_icon(if horizontal { CursorIcon::ResizeHorizontal } else { CursorIcon::ResizeVertical });
                }
                if idle
                    && resp.dragged()
                    && let Some(p) = resp.interact_pointer_pos()
                {
                    let (pos, start, len) = if horizontal { (p.x, rect.left(), rect.width() - GAP) } else { (p.y, rect.top(), rect.height() - GAP) };
                    let min = (MIN_AREA / len.max(1.0)).min(0.45);
                    let mut r = (pos - start - GAP / 2.0) / len.max(1.0);
                    // С Ctrl граница прилипает к четвертям и третям.
                    if ui.input(|i| i.modifiers.ctrl) {
                        r = [0.25, 1.0 / 3.0, 0.5, 2.0 / 3.0, 0.75].into_iter().min_by(|a, b| (a - r).abs().total_cmp(&(b - r).abs())).unwrap_or(r);
                    }
                    *ratio = r.clamp(min, 1.0 - min);
                }
                let dir_now = *dir;
                egui::Popup::context_menu(&resp).style(style.menu_style).show(|ui| {
                    self.border_menu(ui, key, dir_now, first, second, both_areas);
                });
                let t = ui.ctx().animate_bool_with_time(id, idle && (resp.hovered() || resp.dragged()), 0.12);
                if t > 0.0 {
                    let line = if horizontal {
                        Rect::from_center_size(sep.center(), vec2(2.0, sep.height() - 16.0))
                    } else {
                        Rect::from_center_size(sep.center(), vec2(sep.width() - 16.0, 2.0))
                    };
                    ui.painter().rect_filled(line, CornerRadius::same(1), style.accent.gamma_multiply(0.7 * t));
                }
                let [a, b] = &mut **children;
                self.show_node(ui, frame, a, r1, viewer, style);
                self.show_node(ui, frame, b, r2, viewer, style);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn show_area(&mut self, ui: &mut Ui, frame: &mut Frame0, area: &mut Area<T>, rect: Rect, maximized: bool, viewer: &mut impl EditorViewer<Editor = T>, style: &DockStyle) {
        let key = frame.key;
        let ctx = ui.ctx().clone();
        let header = Rect::from_min_size(rect.min, vec2(rect.width(), HEADER_H));
        let body = Rect::from_min_max(pos2(rect.left(), header.bottom()), rect.max);
        let editor_id = viewer.id(&area.editor);

        // Щелчок в любом месте области делает её «текущей».
        let pressed_here = ui.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| rect.contains(p)));
        if pressed_here {
            self.rt.focused.insert(key, area.id);
        }
        let focused = self.rt.focused.get(&key) == Some(&area.id);

        let painter = ui.painter().clone();
        painter.rect_filled(rect, CornerRadius::same(RADIUS), style.area_bg);
        painter.rect_filled(header, CornerRadius { nw: RADIUS, ne: RADIUS, sw: 0, se: 0 }, style.header_bg);
        let focus_t = ctx.animate_bool_with_time(Id::new(("forma-dock-focus", area.id)), focused, 0.2);
        let outline = style.border.lerp_to_gamma(style.accent.gamma_multiply(0.55), focus_t);

        // Пустое место заголовка: правая кнопка — меню области, двойной щелчок — развернуть,
        // перетаскивание — перенести область в другое место или окно.
        let head = ui.interact(header, Id::new(("forma-dock-header", area.id)), Sense::click_and_drag());
        if head.drag_started() && self.idle() {
            self.rt.drag = Some(Drag { src: key, area: area.id, title: viewer.title(&area.editor), pointer: None, released: false, size: rect.size() });
        }
        if head.double_clicked() {
            self.rt.ops.push(Op::ToggleMaximize { key, area: area.id });
        }
        egui::Popup::context_menu(&head).style(style.menu_style).show(|ui| {
            self.header_menu(ui, key, area.id, maximized, style);
        });

        // Меню вида редактора — слева в заголовке, как в Blender.
        let type_rect = Rect::from_min_size(pos2(header.left() + 10.0, header.top() + 4.0), vec2(38.0, HEADER_H - 8.0));
        let type_resp = ui.interact(type_rect, Id::new(("forma-dock-type", area.id)), Sense::click());
        let open = egui::Popup::is_id_open(&ctx, egui::Popup::default_response_id(&type_resp));
        let hover_t = ctx.animate_bool_with_time(type_resp.id.with("hover"), type_resp.hovered() || open, 0.12);
        if hover_t > 0.0 {
            painter.rect_filled(type_rect, CornerRadius::same(6), style.hover_bg.gamma_multiply(hover_t));
        }
        let icon_rect = Rect::from_center_size(pos2(type_rect.left() + 13.0, type_rect.center().y), vec2(16.0, 16.0));
        match viewer.icon(&area.editor) {
            Some((name, color)) if (style.paint_icon)(&painter, icon_rect, &name, color) => {}
            _ => {
                painter.text(icon_rect.center(), Align2::CENTER_CENTER, "▦", FontId::proportional(14.0), style.text);
            }
        }
        let chevron = Rect::from_center_size(pos2(type_rect.right() - 9.0, type_rect.center().y), vec2(10.0, 10.0));
        let chevron_color = style.text_muted.lerp_to_gamma(style.text_strong, hover_t);
        if !(style.paint_icon)(&painter, chevron, "chevron-down", chevron_color) {
            painter.text(chevron.center(), Align2::CENTER_CENTER, "▾", FontId::proportional(11.0), chevron_color);
        }
        let type_resp = type_resp.on_hover_text(t!("dock-editor-type")).on_hover_cursor(CursorIcon::PointingHand);
        let picked = egui::Popup::menu(&type_resp)
            .style(style.menu_style)
            .show(|ui| {
                let groups = viewer.kinds();
                let current = viewer.kind(&area.editor);
                kind_menu(ui, &groups, Some(&current), style)
            })
            .and_then(|r| r.inner);
        if let Some(kind) = picked {
            if kind != viewer.kind(&area.editor) {
                viewer.switch(&mut area.editor, &kind);
            }
            self.rt.focused.insert(key, area.id);
        }

        // Своё содержимое заголовка — правее меню вида.
        let content = Rect::from_min_max(pos2(type_rect.right() + 6.0, header.top()), pos2(header.right() - CORNER, header.bottom()));
        if content.width() > 8.0 {
            let mut child = ui.new_child(
                UiBuilder::new()
                    .max_rect(content)
                    .id_salt(("forma-dock-head", area.id, editor_id))
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            child.set_clip_rect(content.intersect(ui.clip_rect()));
            // Подписи не выделяются мышью: иначе за них не потянуть заголовок.
            child.style_mut().interaction.selectable_labels = false;
            viewer.header(&mut child, &mut area.editor);
        }

        // Содержимое редактора.
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(body)
                .id_salt(("forma-dock-body", area.id, editor_id))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(body.intersect(ui.clip_rect()));
        viewer.ui(&mut child, &mut area.editor);

        // Рамка — поверх содержимого: редактор закрашивает свой прямоугольник целиком, и без
        // этого рамка оставалась бы только у заголовка, а углы были бы квадратными.
        round_corners(&painter, rect, style.gap_bg);
        painter.rect_stroke(rect, CornerRadius::same(RADIUS), Stroke::new(1.0, outline), StrokeKind::Inside);

        // Уголки — поверх всего, как в Blender. У развёрнутой области их нет.
        if !maximized {
            self.corners(ui, key, area.id, rect, style);
        }
        frame.areas.push(AreaRects { id: area.id, header, body });
    }

    /// Четыре уголка области: за них тянут, чтобы разделить, объединить, поменять местами
    /// (с Ctrl) или открыть копию в новом окне (с Shift).
    fn corners(&mut self, ui: &mut Ui, key: SurfaceKey, area: AreaId, rect: Rect, style: &DockStyle) {
        let idle = self.idle();
        for (i, c) in [rect.left_top(), rect.right_top(), rect.left_bottom(), rect.right_bottom()].into_iter().enumerate() {
            let inward = vec2(if c.x < rect.center().x { 1.0 } else { -1.0 }, if c.y < rect.center().y { 1.0 } else { -1.0 });
            let id = Id::new(("forma-dock-corner", area, i));
            let resp = ui.interact(Rect::from_two_pos(c, c + inward * CORNER), id, Sense::drag());
            let active = self.rt.corner.as_ref().is_some_and(|g| g.area == area && g.corner == c);
            let hot = (idle && resp.hovered()) || active;
            if hot {
                ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
            }
            let t = ui.ctx().animate_bool_with_time(id, hot, 0.12);
            let color = style.text_muted.gamma_multiply(0.35).lerp_to_gamma(style.accent, t);
            for k in [6.0, 10.0] {
                let a = c + vec2(inward.x * k, inward.y * 2.5);
                let b = c + vec2(inward.x * 2.5, inward.y * k);
                ui.painter().line_segment([a, b], Stroke::new(1.0, color));
            }
            if idle && resp.drag_started() {
                if ui.input(|i| i.modifiers.shift) {
                    self.rt.ops.push(Op::DuplicateWindow { key, area, size: rect.size() });
                } else {
                    self.rt.corner = Some(CornerDrag { key, area, corner: c, dir: None, action: CornerAction::None });
                }
            }
            if idle {
                resp.on_hover_text(t!("dock-corner-hint"));
            }
        }
    }

    /// Пункты «Вертикальное / горизонтальное разделение»: дальше линия идёт за курсором.
    fn split_items(&mut self, ui: &mut Ui, key: SurfaceKey) {
        if ui.button(t!("dock-split-vertical")).clicked() {
            self.rt.pick = Some(SplitPick { key, dir: SplitDir::Horizontal });
        }
        if ui.button(t!("dock-split-horizontal")).clicked() {
            self.rt.pick = Some(SplitPick { key, dir: SplitDir::Vertical });
        }
    }

    /// Меню границы между областями (правая кнопка): разделить, объединить, поменять местами.
    fn border_menu(&mut self, ui: &mut Ui, key: SurfaceKey, dir: SplitDir, first: AreaId, second: AreaId, both_areas: bool) {
        self.split_items(ui, key);
        ui.separator();
        let (into_first, into_second) = match dir {
            SplitDir::Horizontal => (t!("dock-join-left"), t!("dock-join-right")),
            SplitDir::Vertical => (t!("dock-join-up"), t!("dock-join-down")),
        };
        for (label, keep, remove) in [(into_first, first, second), (into_second, second, first)] {
            let resp = ui.add_enabled(both_areas, egui::Button::new(label)).on_disabled_hover_text(t!("dock-join-unavailable"));
            if resp.clicked() {
                self.rt.ops.push(Op::Join { key, keep, remove });
            }
        }
        ui.separator();
        if ui.button(t!("dock-swap")).clicked() {
            self.rt.ops.push(Op::SwapSplit { key, first, second });
        }
    }

    /// Меню заголовка области (правая кнопка).
    fn header_menu(&mut self, ui: &mut Ui, key: SurfaceKey, area: AreaId, maximized: bool, style: &DockStyle) {
        if !maximized {
            self.split_items(ui, key);
            ui.separator();
        }
        let max_label = if maximized { t!("dock-restore") } else { t!("dock-maximize") };
        if ui.add(egui::Button::new(max_label).shortcut_text(style.maximize_text.as_str())).clicked() {
            self.rt.ops.push(Op::ToggleMaximize { key, area });
        }
        let size = self.area_size(key, area);
        match key {
            SurfaceKey::Main => {
                if ui.button(t!("dock-detach")).clicked() {
                    self.rt.ops.push(Op::Detach { key, area, pos: None, size });
                }
            }
            SurfaceKey::Window(w) => {
                if ui.button(t!("dock-attach-window")).clicked() {
                    self.rt.ops.push(Op::Return(w));
                }
            }
        }
        if ui.button(t!("dock-duplicate-window")).clicked() {
            self.rt.ops.push(Op::DuplicateWindow { key, area, size });
        }
        ui.separator();
        if ui.button(t!("dock-close-area")).clicked() {
            self.rt.ops.push(Op::CloseArea { key, area });
        }
    }
}

/// Меню выбора вида редактора: столбцы-группы, в каждом — значок, название и сочетание.
fn kind_menu(ui: &mut Ui, groups: &[KindGroup], current: Option<&str>, style: &DockStyle) -> Option<String> {
    const COLUMN_W: f32 = 200.0;
    let mut picked = None;
    ui.horizontal_top(|ui| {
        for (gi, group) in groups.iter().enumerate() {
            if gi > 0 {
                ui.add_space(8.0);
            }
            ui.vertical(|ui| {
                ui.set_width(COLUMN_W);
                ui.add_space(2.0);
                ui.label(egui::RichText::new(&group.title).size(11.5).color(style.text_muted));
                ui.add_space(2.0);
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                    for kind in &group.kinds {
                        let selected = current == Some(kind.id.as_str());
                        // Место под значок слева от названия.
                        let button = egui::Button::new(format!("      {}", kind.title)).shortcut_text(kind.shortcut.as_str()).selected(selected);
                        let resp = ui.add(button);
                        let r = Rect::from_center_size(resp.rect.left_center() + vec2(ui.spacing().button_padding.x + 8.0, 0.0), vec2(15.0, 15.0));
                        let color = if selected { style.accent } else { style.text };
                        (style.paint_icon)(ui.painter(), r, &kind.icon, color);
                        if resp.clicked() {
                            picked = Some(kind.id.clone());
                        }
                    }
                });
            });
        }
    });
    picked
}

/// Закрашивает цветом зазора всё, что содержимое нарисовало за скруглёнными углами области.
fn round_corners(painter: &egui::Painter, rect: Rect, color: Color32) {
    const STEPS: usize = 8;
    let r = RADIUS as f32;
    let corners = [
        (rect.left_top(), pos2(rect.left() + r, rect.top() + r), 180.0_f32),
        (rect.right_top(), pos2(rect.right() - r, rect.top() + r), 270.0),
        (rect.right_bottom(), pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (rect.left_bottom(), pos2(rect.left() + r, rect.bottom() - r), 90.0),
    ];
    let mut mesh = egui::Mesh::default();
    for (corner, center, start) in corners {
        // Веер треугольников от угла прямоугольника к дуге скругления.
        let base = mesh.vertices.len() as u32;
        mesh.colored_vertex(corner, color);
        for i in 0..=STEPS {
            let a = (start + 90.0 * i as f32 / STEPS as f32).to_radians();
            mesh.colored_vertex(center + r * vec2(a.cos(), a.sin()), color);
        }
        for i in 0..STEPS as u32 {
            mesh.add_triangle(base, base + 1 + i, base + 2 + i);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Линия будущего разреза в промежутке `sep`.
fn paint_cut(painter: &egui::Painter, sep: Rect, dir: SplitDir, style: &DockStyle) {
    let line = match dir {
        SplitDir::Horizontal => Rect::from_center_size(sep.center(), vec2(2.0, sep.height())),
        SplitDir::Vertical => Rect::from_center_size(sep.center(), vec2(sep.width(), 2.0)),
    };
    painter.rect_filled(line, CornerRadius::same(1), style.accent);
}

/// Подсказка внизу окна, пока идёт жест.
fn paint_hint(painter: &egui::Painter, surface_rect: Rect, text: &str, style: &DockStyle) {
    let galley = painter.layout_no_wrap(text.to_owned(), FontId::proportional(12.5), style.text_strong);
    let r = Rect::from_center_size(pos2(surface_rect.center().x, surface_rect.bottom() - 26.0), galley.size() + vec2(24.0, 12.0));
    painter.rect(r, CornerRadius::same(8), style.header_bg, Stroke::new(1.0, style.border), StrokeKind::Inside);
    painter.galley(r.min + vec2(12.0, 6.0), galley, style.text_strong);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area_rects(id: u64, body: Rect) -> AreaRects {
        let header = Rect::from_min_size(body.min - vec2(0.0, HEADER_H), vec2(body.width(), HEADER_H));
        AreaRects { id: AreaId(id), header, body }
    }

    #[test]
    fn zones_by_pointer_position() {
        let body = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0));
        assert_eq!(zone_at(body, pos2(200.0, 150.0)), Zone::Center);
        assert_eq!(zone_at(body, pos2(20.0, 150.0)), Zone::Left);
        assert_eq!(zone_at(body, pos2(390.0, 150.0)), Zone::Right);
        assert_eq!(zone_at(body, pos2(200.0, 10.0)), Zone::Top);
        assert_eq!(zone_at(body, pos2(200.0, 290.0)), Zone::Bottom);
    }

    #[test]
    fn target_prefers_floating_windows_and_detects_desktop() {
        let main = SurfaceRects {
            key: SurfaceKey::Main,
            window: Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 800.0)),
            areas: vec![area_rects(1, Rect::from_min_size(pos2(10.0, 40.0), vec2(980.0, 750.0)))],
        };
        let float = SurfaceRects {
            key: SurfaceKey::Window(9),
            window: Rect::from_min_size(pos2(500.0, 300.0), vec2(400.0, 300.0)),
            areas: vec![area_rects(2, Rect::from_min_size(pos2(500.0, 330.0), vec2(400.0, 270.0)))],
        };
        let rects = vec![main, float];
        assert!(matches!(compute_target(&rects, pos2(700.0, 450.0)), Target::Area { key: SurfaceKey::Window(9), .. }));
        assert!(matches!(compute_target(&rects, pos2(100.0, 400.0)), Target::Area { key: SurfaceKey::Main, .. }));
        assert_eq!(compute_target(&rects, pos2(1500.0, 400.0)), Target::NewWindow(pos2(1500.0, 400.0)));
        assert_eq!(
            compute_target(&rects, pos2(160.0, 20.0)),
            Target::Area { key: SurfaceKey::Main, area: AreaId(1), zone: Zone::Top },
            "заголовок — часть области"
        );
        assert_eq!(compute_target(&rects, pos2(2.0, 2.0)), Target::Nothing, "над рамкой окна — ничего");
    }

    /// Две области рядом: 1 слева (0..400), 2 справа (404..800), обе высотой 600.
    fn side_by_side() -> Vec<AreaRects> {
        vec![
            area_rects(1, Rect::from_min_max(pos2(0.0, HEADER_H), pos2(400.0, 600.0))),
            area_rects(2, Rect::from_min_max(pos2(404.0, HEADER_H), pos2(800.0, 600.0))),
        ]
    }

    #[test]
    fn corner_drag_inside_splits_toward_the_corner() {
        let areas = side_by_side();
        let corner = pos2(400.0, 600.0); // правый нижний уголок первой области
        let mut dir = None;
        assert_eq!(corner_action(&areas, AreaId(1), corner, pos2(396.0, 597.0), &mut dir, &[], false), CornerAction::None, "мышь ещё у уголка");
        assert_eq!(dir, None);

        // Повели влево — вертикальная линия, новая часть справа (со стороны уголка).
        let a = corner_action(&areas, AreaId(1), corner, pos2(300.0, 590.0), &mut dir, &[], false);
        assert_eq!(dir, Some(SplitDir::Horizontal));
        let CornerAction::Split { zone, ratio } = a else { panic!("{a:?}") };
        assert_eq!(zone, Zone::Right);
        assert!((ratio - 0.25).abs() < 0.01, "новая часть — от линии до уголка: {ratio}");

        // Направление запомнилось: дальше вверх — всё ещё вертикальная линия.
        assert!(matches!(corner_action(&areas, AreaId(1), corner, pos2(200.0, 100.0), &mut dir, &[], false), CornerAction::Split { zone: Zone::Right, .. }));
        // Слишком близко к краю — части вышли бы крошечными.
        assert_eq!(corner_action(&areas, AreaId(1), corner, pos2(20.0, 300.0), &mut dir, &[], false), CornerAction::None);

        // Сверху вниз от верхнего левого уголка — горизонтальная линия, новая часть сверху.
        let mut dir = None;
        let a = corner_action(&areas, AreaId(1), pos2(0.0, 0.0), pos2(10.0, 200.0), &mut dir, &[], false);
        assert_eq!(dir, Some(SplitDir::Vertical));
        assert!(matches!(a, CornerAction::Split { zone: Zone::Top, .. }), "{a:?}");
    }

    #[test]
    fn corner_drag_into_another_area() {
        let areas = side_by_side();
        let corner = pos2(400.0, 0.0);
        let mut dir = None;
        assert_eq!(corner_action(&areas, AreaId(1), corner, pos2(600.0, 300.0), &mut dir, &[AreaId(2)], false), CornerAction::Join(AreaId(2)));
        assert_eq!(corner_action(&areas, AreaId(1), corner, pos2(600.0, 300.0), &mut dir, &[AreaId(2)], true), CornerAction::Swap(AreaId(2)), "Ctrl — обмен");
        assert_eq!(corner_action(&areas, AreaId(1), corner, pos2(600.0, 300.0), &mut dir, &[], false), CornerAction::Dock(AreaId(2), Zone::Center), "не сосед — перенос");
        assert_eq!(corner_action(&areas, AreaId(1), corner, pos2(780.0, 300.0), &mut dir, &[], false), CornerAction::Dock(AreaId(2), Zone::Right));
        assert_eq!(corner_action(&areas, AreaId(1), corner, pos2(402.0, 300.0), &mut dir, &[AreaId(2)], false), CornerAction::None, "в зазоре — ничего");
    }

    struct V {
        closed: Vec<&'static str>,
    }
    impl EditorViewer for V {
        type Editor = &'static str;
        fn kinds(&mut self) -> Vec<KindGroup> {
            Vec::new()
        }
        fn kind(&mut self, editor: &Self::Editor) -> String {
            editor.to_string()
        }
        fn create(&mut self, kind: &str) -> Option<Self::Editor> {
            (kind == "консоль").then_some("консоль")
        }
        fn title(&mut self, editor: &Self::Editor) -> String {
            editor.to_string()
        }
        fn id(&mut self, editor: &Self::Editor) -> Id {
            Id::new(editor)
        }
        fn ui(&mut self, _ui: &mut Ui, _editor: &mut Self::Editor) {}
        fn duplicate(&mut self, editor: &Self::Editor) -> Self::Editor {
            editor
        }
        fn on_close(&mut self, editor: Self::Editor) {
            self.closed.push(editor);
        }
    }

    fn v() -> V {
        V { closed: Vec::new() }
    }

    fn editors(s: &Surface<&'static str>) -> Vec<&'static str> {
        s.editors().copied().collect()
    }

    /// Прямоугольники прошлых кадров не должны копиться: иначе цель броска считалась
    /// по старому положению окна (и память росла без конца).
    #[test]
    fn screen_rects_do_not_pile_up_between_frames() {
        let ctx = Context::default();
        let mut dock: DockState<&'static str> = DockState::default();
        let mut main = Surface::default();
        for frame in 0..3 {
            let window = Rect::from_min_size(pos2(frame as f32 * 100.0, 0.0), vec2(800.0, 600.0));
            dock.rt.rects.push(SurfaceRects { key: SurfaceKey::Main, window, areas: Vec::new() });
            dock.finish(&ctx, &mut main, &mut v());
        }
        assert!(dock.rt.rects.is_empty());
        assert_eq!(dock.rt.prev_rects.len(), 1);
        assert_eq!(dock.rt.prev_rects[0].window.min.x, 200.0, "остался только последний кадр");
    }

    #[test]
    fn split_join_swap_and_replace() {
        let ctx = Context::default();
        let mut dock: DockState<&'static str> = DockState::default();
        dock.reserve_ids(10);
        let mut main = Surface::new(Node::area(AreaId(1), "код"));
        let mut viewer = v();

        dock.rt.ops.push(Op::Split { key: SurfaceKey::Main, area: AreaId(1), zone: Zone::Right, ratio: 0.3 });
        dock.finish(&ctx, &mut main, &mut viewer);
        assert_eq!(editors(&main), vec!["код", "код"], "копия редактора в новой части");
        let new = main.areas()[1].id;
        assert_eq!(dock.focused(SurfaceKey::Main), Some(new));

        dock.rt.ops.push(Op::Join { key: SurfaceKey::Main, keep: AreaId(1), remove: new });
        dock.finish(&ctx, &mut main, &mut viewer);
        assert_eq!(main.count(), 1);
        assert_eq!(viewer.closed, vec!["код"], "объединённый редактор закрыт");

        // В пустое окно — новый редактор; потом перенос на место другого.
        let mut empty = Surface::default();
        dock.rt.ops.push(Op::Create { key: SurfaceKey::Main, kind: "консоль".into() });
        dock.finish(&ctx, &mut empty, &mut viewer);
        assert_eq!(editors(&empty), vec!["консоль"]);

        main.split_area(AreaId(1), Zone::Bottom, Area::new(AreaId(50), "консоль"), 0.3).unwrap();
        dock.rt.ops.push(Op::Swap { key: SurfaceKey::Main, a: AreaId(1), b: AreaId(50) });
        dock.finish(&ctx, &mut main, &mut viewer);
        assert_eq!(editors(&main), vec!["консоль", "код"]);

        dock.rt.ops.push(Op::Move { from: (SurfaceKey::Main, AreaId(50)), to: Target::Area { key: SurfaceKey::Main, area: AreaId(1), zone: Zone::Center } });
        dock.finish(&ctx, &mut main, &mut viewer);
        assert_eq!(editors(&main), vec!["код"], "перенесённый редактор занял место другого");
        assert_eq!(viewer.closed, vec!["код", "консоль"]);
    }

    #[test]
    fn moving_areas_between_surfaces_and_windows() {
        let ctx = Context::default();
        let mut dock: DockState<&'static str> = DockState::default();
        dock.reserve_ids(10);
        let mut main = Surface::new(Node::split(SplitDir::Horizontal, 0.5, Node::area(AreaId(1), "код"), Node::area(AreaId(2), "консоль")));
        let mut viewer = v();

        // Вынести «консоль» в отдельное окно.
        dock.rt.ops.push(Op::Detach { key: SurfaceKey::Main, area: AreaId(2), pos: None, size: vec2(500.0, 400.0) });
        dock.finish(&ctx, &mut main, &mut viewer);
        assert_eq!(dock.windows.len(), 1);
        assert_eq!(editors(&main), vec!["код"]);
        let win = dock.windows[0].id;

        // Перетащить её обратно под «код».
        let win_area = dock.windows[0].surface.first_area_id().unwrap();
        dock.rt.ops.push(Op::Move {
            from: (SurfaceKey::Window(win), win_area),
            to: Target::Area { key: SurfaceKey::Main, area: AreaId(1), zone: Zone::Bottom },
        });
        dock.finish(&ctx, &mut main, &mut viewer);
        assert!(dock.windows.is_empty(), "опустевшее окно закрывается");
        assert_eq!(editors(&main), vec!["код", "консоль"]);

        // Копия в новом окне, потом все окна — обратно.
        dock.rt.ops.push(Op::DuplicateWindow { key: SurfaceKey::Main, area: AreaId(1), size: DEFAULT_WINDOW });
        dock.finish(&ctx, &mut main, &mut viewer);
        assert_eq!(dock.windows.len(), 1);
        assert_eq!(main.count(), 2, "копия не убирает область");
        dock.return_all();
        dock.finish(&ctx, &mut main, &mut viewer);
        assert!(dock.windows.is_empty());
        assert_eq!(main.count(), 3);
        assert!(viewer.closed.is_empty(), "при переносах ничего не закрывается");
    }
}
