//! Отрисовка составного интерфейса: области с вкладками, разделители, перетаскивание
//! вкладок между областями и окнами, отдельные окна ОС.
//!
//! Перетаскивание работает в общих экранных координатах: каждое окно запоминает, где на
//! экране лежат его области, а цель броска вычисляется в [`DockState::finish`] после того,
//! как нарисованы все окна. Пока кнопка мыши зажата, Windows присылает координаты курсора
//! окну-источнику даже за его пределами, поэтому вкладку можно бросить в другое окно
//! или просто на рабочий стол — тогда она становится отдельным окном.

use std::collections::HashMap;
use std::sync::Arc;

use egui::{
    Align2, Color32, Context, CornerRadius, CursorIcon, FontId, Frame, Id, KeyboardShortcut, Pos2, Rect,
    Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2, ViewportBuilder, ViewportId, pos2, vec2,
};
use forma_i18n::t;
use serde::{Deserialize, Serialize};

use crate::tree::{Area, AreaId, Node, SplitDir, Surface, Zone};

/// Высота строки вкладок.
const HEADER_H: f32 = 30.0;
/// Зазор между областями (и от краёв окна).
const GAP: f32 = 4.0;
/// Скругление углов области.
const RADIUS: u8 = 8;
/// Меньше этого область не сжимается разделителем.
const MIN_AREA: f32 = 72.0;
/// Размер нового отдельного окна, если размер исходной области неизвестен.
const DEFAULT_WINDOW: Vec2 = vec2(720.0, 480.0);
/// Уже этого вкладка не сжимается: лишние вкладки уходят в меню «»».
const MIN_TAB: f32 = 76.0;
/// Уже этого область прячет кнопку «+» в меню области.
const ADD_BUTTON_MIN_W: f32 = 150.0;

/// Что приложение рассказывает о своих вкладках и как рисует их содержимое.
pub trait TabViewer {
    type Tab;

    /// Подпись вкладки.
    fn title(&mut self, tab: &Self::Tab) -> String;

    /// Значок перед подписью — его имя (см. [`DockStyle::paint_icon`]) — и цвет.
    fn icon(&mut self, _tab: &Self::Tab) -> Option<(String, Color32)> {
        None
    }

    /// Постоянный идентификатор вкладки (не меняется при переносе между областями).
    fn id(&mut self, tab: &Self::Tab) -> Id;

    /// Содержимое вкладки.
    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab);

    /// Пункты меню «+»: какую вкладку добавить в область.
    fn add_menu(&mut self, ui: &mut Ui) -> Option<Self::Tab>;

    /// Копия вкладки того же вида — для разделения области.
    fn duplicate(&mut self, tab: &Self::Tab) -> Self::Tab;

    /// Вкладку закрыли.
    fn on_close(&mut self, _tab: Self::Tab) {}

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
    /// Фон строки вкладок.
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
    drag: Option<Drag>,
    /// Куда упадёт перетаскиваемая вкладка (вычислено в прошлом кадре).
    target: Target,
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
        Self { drag: None, target: Target::Nothing, rects: Vec::new(), prev_rects: Vec::new(), focused: HashMap::new(), ops: Vec::new() }
    }
}

struct Drag {
    src: SurfaceKey,
    area: AreaId,
    index: usize,
    title: String,
    /// Курсор в экранных координатах.
    pointer: Option<Pos2>,
    released: bool,
    /// Размер исходной области — такого размера будет новое окно.
    size: Vec2,
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

/// Прямоугольники области в экранных координатах.
struct AreaRects {
    id: AreaId,
    header: Rect,
    body: Rect,
    tabs: Vec<Rect>,
}

enum Op<T> {
    Move { from: (SurfaceKey, AreaId, usize), to: Target },
    Close { key: SurfaceKey, area: AreaId, index: usize },
    CloseOthers { key: SurfaceKey, area: AreaId, index: usize },
    CloseArea { key: SurfaceKey, area: AreaId },
    Detach { key: SurfaceKey, area: AreaId, index: usize, pos: Option<Pos2>, size: Vec2 },
    Return(u64),
    /// Перенести вкладку в новую половину своей же области.
    MoveToSplit { key: SurfaceKey, area: AreaId, index: usize, zone: Zone },
    /// Разделить область, положив во вторую половину копию активной вкладки.
    SplitDuplicate { key: SurfaceKey, area: AreaId, zone: Zone },
    Add { key: SurfaceKey, area: Option<AreaId>, tab: T },
    ToggleMaximize { key: SurfaceKey, area: AreaId },
}

fn surface_mut<'a, T>(main: &'a mut Surface<T>, windows: &'a mut [FloatWindow<T>], key: SurfaceKey) -> Option<&'a mut Surface<T>> {
    match key {
        SurfaceKey::Main => Some(main),
        SurfaceKey::Window(id) => windows.iter_mut().find(|w| w.id == id).map(|w| &mut w.surface),
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

    pub fn is_dragging(&self) -> bool {
        self.rt.drag.is_some()
    }

    /// Чинит загруженные окна и убирает пустые.
    pub fn sanitize(&mut self) {
        for w in &mut self.windows {
            w.surface.sanitize();
        }
        self.windows.retain(|w| !w.surface.is_empty());
    }

    /// Открывает вкладку в главном окне — в области, с которой работали последней.
    pub fn open_tab(&mut self, tab: T) {
        let area = self.focused(SurfaceKey::Main);
        self.rt.ops.push(Op::Add { key: SurfaceKey::Main, area, tab });
    }

    /// Выносит активную вкладку последней области главного окна в отдельное окно.
    pub fn detach_focused(&mut self, main: &Surface<T>) {
        let Some(id) = self.focused(SurfaceKey::Main).filter(|id| main.contains(*id)).or_else(|| main.first_area_id()) else {
            return;
        };
        let Some(area) = main.area(id) else { return };
        let size = self.area_size(SurfaceKey::Main, id);
        self.rt.ops.push(Op::Detach { key: SurfaceKey::Main, area: id, index: area.active, pos: None, size });
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

    fn area_size(&self, key: SurfaceKey, area: AreaId) -> Vec2 {
        self.rt
            .prev_rects
            .iter()
            .filter(|s| s.key == key)
            .flat_map(|s| &s.areas)
            .find(|a| a.id == area)
            .map(|a| vec2(a.body.width(), a.body.height() + HEADER_H))
            .unwrap_or(DEFAULT_WINDOW)
    }
}

impl<T> DockState<T> {
    /// Рисует главное окно. Вызывать внутри центральной панели.
    pub fn show_main(&mut self, ui: &mut Ui, main: &mut Surface<T>, viewer: &mut impl TabViewer<Tab = T>, style: &DockStyle) {
        self.show_surface(ui, SurfaceKey::Main, main, viewer, style);
    }

    /// Рисует отдельные окна (каждое — своё окно ОС).
    pub fn show_windows(&mut self, ctx: &Context, viewer: &mut impl TabViewer<Tab = T>, style: &DockStyle) {
        for idx in 0..self.windows.len() {
            let w = &mut self.windows[idx];
            let id = w.id;
            let (spawn_pos, spawn_size) = *w.spawn.get_or_insert_with(|| (w.pos.map(|p| pos2(p[0], p[1])), vec2(w.size[0], w.size[1])));
            let title = w
                .surface
                .areas()
                .first()
                .and_then(|a| a.active_tab())
                .map(|tab| format!("{} — {}", viewer.title(tab), style.app_title))
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
            let viewport = ViewportId::from_hash_of(("mycelium-dock-window", id));
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
                // Закрытое окно не теряет вкладки: они возвращаются в главное.
                self.rt.ops.push(Op::Return(id));
            }
        }
    }

    /// Завершает кадр: применяет действия, вычисляет цель перетаскивания.
    /// Вызывать после [`Self::show_main`] и [`Self::show_windows`].
    pub fn finish(&mut self, ctx: &Context, main: &mut Surface<T>, viewer: &mut impl TabViewer<Tab = T>) {
        let rects = std::mem::take(&mut self.rt.rects);
        if let Some(drag) = &self.rt.drag {
            // Окно-источник пропало (например, закрылось) — перетаскивание отменяется.
            if !rects.iter().any(|s| s.key == drag.src) {
                self.rt.drag = None;
            }
        }
        self.rt.target = Target::Nothing;
        if let Some(drag) = &self.rt.drag {
            let target = drag.pointer.map_or(Target::Nothing, |p| compute_target(&rects, p));
            let target = self.filter_noop(main, drag, target);
            if drag.released {
                let drag = self.rt.drag.take().expect("проверено выше");
                let to = match target {
                    Target::NewWindow(p) => {
                        self.rt.ops.push(Op::Detach {
                            key: drag.src,
                            area: drag.area,
                            index: drag.index,
                            pos: Some(p - vec2(24.0, 14.0)),
                            size: drag.size,
                        });
                        Target::Nothing
                    }
                    t => t,
                };
                if to != Target::Nothing {
                    self.rt.ops.push(Op::Move { from: (drag.src, drag.area, drag.index), to });
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

    /// Бросок, который ничего не изменит, не подсвечиваем и не выполняем.
    fn filter_noop(&self, main: &Surface<T>, drag: &Drag, target: Target) -> Target {
        let Target::Area { key, area, zone } = target else { return target };
        if key != drag.src || area != drag.area {
            return target;
        }
        let tabs = match drag.src {
            SurfaceKey::Main => main.area(area).map(|a| a.tabs.len()),
            SurfaceKey::Window(id) => self.windows.iter().find(|w| w.id == id).and_then(|w| w.surface.area(area)).map(|a| a.tabs.len()),
        }
        .unwrap_or(0);
        let noop = match zone {
            Zone::Center => true,
            Zone::Tab(i) => i == drag.index || i == drag.index + 1,
            _ => tabs <= 1,
        };
        if noop { Target::Nothing } else { target }
    }

    fn apply(&mut self, op: Op<T>, main: &mut Surface<T>, viewer: &mut impl TabViewer<Tab = T>) {
        match op {
            Op::Move { from: (key, area, index), to } => {
                let Some(src) = surface_mut(main, &mut self.windows, key) else { return };
                let Some(tab) = src.remove_tab(area, index) else { return };
                let new_id = self.new_area_id();
                match to {
                    Target::Area { key: tkey, area: tarea, zone } => {
                        // Внутри одной строки вкладок номера после удаления сдвигаются.
                        let zone = match zone {
                            Zone::Tab(i) if tkey == key && tarea == area && index < i => Zone::Tab(i - 1),
                            z => z,
                        };
                        match surface_mut(main, &mut self.windows, tkey) {
                            Some(dst) => {
                                dst.insert(tarea, zone, tab, new_id);
                                let focus = if zone.split().is_some() { new_id } else { tarea };
                                self.rt.focused.insert(tkey, focus);
                            }
                            None => main.add_tab(tab, None, new_id),
                        }
                    }
                    Target::Empty(tkey) => match surface_mut(main, &mut self.windows, tkey) {
                        Some(dst) => {
                            dst.add_tab(tab, None, new_id);
                            self.rt.focused.insert(tkey, new_id);
                        }
                        None => main.add_tab(tab, None, new_id),
                    },
                    Target::NewWindow(p) => self.spawn_window(tab, Some(p), DEFAULT_WINDOW),
                    Target::Nothing => main.add_tab(tab, None, new_id),
                }
            }
            Op::Close { key, area, index } => {
                if let Some(tab) = surface_mut(main, &mut self.windows, key).and_then(|s| s.remove_tab(area, index)) {
                    viewer.on_close(tab);
                }
            }
            Op::CloseOthers { key, area, index } => {
                let Some(a) = surface_mut(main, &mut self.windows, key).and_then(|s| s.area_mut(area)) else { return };
                if index >= a.tabs.len() {
                    return;
                }
                let keep = a.tabs.remove(index);
                let others = std::mem::replace(&mut a.tabs, vec![keep]);
                a.active = 0;
                for tab in others {
                    viewer.on_close(tab);
                }
            }
            Op::CloseArea { key, area } => {
                if let Some(a) = surface_mut(main, &mut self.windows, key).and_then(|s| s.remove_area(area)) {
                    for tab in a.tabs {
                        viewer.on_close(tab);
                    }
                }
            }
            Op::Detach { key, area, index, pos, size } => {
                if let Some(tab) = surface_mut(main, &mut self.windows, key).and_then(|s| s.remove_tab(area, index)) {
                    self.spawn_window(tab, pos, size);
                }
            }
            Op::Return(id) => {
                let Some(i) = self.windows.iter().position(|w| w.id == id) else { return };
                let mut w = self.windows.remove(i);
                let prefer = self.focused(SurfaceKey::Main);
                for tab in w.surface.take_all_tabs() {
                    let new_id = self.new_area_id();
                    main.add_tab(tab, prefer, new_id);
                }
            }
            Op::MoveToSplit { key, area, index, zone } => {
                let new_id = self.new_area_id();
                let Some(s) = surface_mut(main, &mut self.windows, key) else { return };
                if s.area(area).is_none_or(|a| a.tabs.len() < 2) {
                    return;
                }
                if let Some(tab) = s.remove_tab(area, index) {
                    s.insert(area, zone, tab, new_id);
                    self.rt.focused.insert(key, new_id);
                }
            }
            Op::SplitDuplicate { key, area, zone } => {
                let new_id = self.new_area_id();
                let Some(s) = surface_mut(main, &mut self.windows, key) else { return };
                let Some(tab) = s.area(area).and_then(|a| a.active_tab()).map(|t| viewer.duplicate(t)) else { return };
                s.insert(area, zone, tab, new_id);
                self.rt.focused.insert(key, new_id);
            }
            Op::Add { key, area, tab } => {
                let new_id = self.new_area_id();
                // Окно могли закрыть в этом же кадре — тогда вкладка идёт в главное.
                let exists = match key {
                    SurfaceKey::Main => true,
                    SurfaceKey::Window(id) => self.windows.iter().any(|w| w.id == id),
                };
                let key = if exists { key } else { SurfaceKey::Main };
                let s = surface_mut(main, &mut self.windows, key).expect("окно существует");
                s.add_tab(tab, area, new_id);
                let focus = area.filter(|a| s.contains(*a)).unwrap_or_else(|| s.first_area_id().unwrap_or(new_id));
                self.rt.focused.insert(key, focus);
            }
            Op::ToggleMaximize { key, area } => {
                if let Some(s) = surface_mut(main, &mut self.windows, key) {
                    s.maximized = if s.maximized == Some(area) { None } else { Some(area) };
                }
            }
        }
    }

    fn spawn_window(&mut self, tab: T, pos: Option<Pos2>, size: Vec2) {
        let id = self.new_area_id().0;
        let area = self.new_area_id();
        let size = vec2(size.x.max(360.0), size.y.max(240.0));
        self.windows.push(FloatWindow {
            id,
            surface: Surface::new(Node::area(area, vec![tab])),
            pos: pos.map(|p| [p.x, p.y]),
            size: [size.x, size.y],
            spawn: None,
        });
    }
}

/// Куда упадёт вкладка, если отпустить её в точке `p` (экранные координаты).
fn compute_target(rects: &[SurfaceRects], p: Pos2) -> Target {
    // Отдельные окна обычно лежат поверх главного, поэтому проверяем их первыми.
    let ordered = rects.iter().rev().filter(|s| s.key != SurfaceKey::Main).chain(rects.iter().filter(|s| s.key == SurfaceKey::Main));
    for s in ordered {
        if !s.window.contains(p) {
            continue;
        }
        for a in &s.areas {
            if a.header.contains(p) {
                let index = a.tabs.iter().filter(|r| r.center().x < p.x).count();
                return Target::Area { key: s.key, area: a.id, zone: Zone::Tab(index) };
            }
            if a.body.contains(p) {
                return Target::Area { key: s.key, area: a.id, zone: zone_at(a.body, p) };
            }
        }
        return if s.areas.is_empty() { Target::Empty(s.key) } else { Target::Nothing };
    }
    Target::NewWindow(p)
}

/// Зона области под курсором: середина — вкладкой, края — разделить.
fn zone_at(body: Rect, p: Pos2) -> Zone {
    let rel = (p - body.min) / body.size().max(vec2(1.0, 1.0));
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

/// Прямоугольник, который подсвечивается при броске в зону.
fn zone_rect(a: &AreaRects, zone: Zone) -> Rect {
    let b = a.body;
    match zone {
        Zone::Center => b,
        Zone::Left => Rect::from_min_max(b.min, pos2(b.center().x, b.max.y)),
        Zone::Right => Rect::from_min_max(pos2(b.center().x, b.min.y), b.max),
        Zone::Top => Rect::from_min_max(b.min, pos2(b.max.x, b.center().y)),
        Zone::Bottom => Rect::from_min_max(pos2(b.min.x, b.center().y), b.max),
        Zone::Tab(i) => {
            let x = match (a.tabs.get(i), a.tabs.last()) {
                (Some(r), _) => r.left(),
                (None, Some(last)) => last.right(),
                (None, None) => a.header.left() + 6.0,
            };
            Rect::from_center_size(pos2(x, a.header.center().y), vec2(3.0, a.header.height() - 8.0))
        }
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

// ============================================================================================
// Отрисовка
// ============================================================================================

/// Что нарисовано в одном окне в этом кадре (в координатах окна).
struct Frame0 {
    key: SurfaceKey,
    areas: Vec<AreaRects>,
}

impl<T> DockState<T> {
    fn show_surface(&mut self, ui: &mut Ui, key: SurfaceKey, surface: &mut Surface<T>, viewer: &mut impl TabViewer<Tab = T>, style: &DockStyle) {
        let ctx = ui.ctx().clone();
        if matches!(key, SurfaceKey::Window(_)) {
            viewer.window_input(ui);
        }
        let full = ui.available_rect_before_wrap();
        let rect = full.shrink(GAP);
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
            let hovered = ui.input(|i| i.pointer.hover_pos()).and_then(|p| frame.areas.iter().find(|a| a.header.union(a.body).contains(p)).map(|a| a.id));
            let area = surface.maximized.or(hovered).or_else(|| self.focused(key).filter(|id| surface.contains(*id))).or_else(|| surface.first_area_id());
            if let Some(area) = area {
                self.rt.ops.push(Op::ToggleMaximize { key, area });
            }
        }

        // Экранные координаты окна: от них считаются броски между окнами.
        let window = ctx.input(|i| i.viewport().inner_rect);
        if let Some(window) = window {
            let offset = window.min.to_vec2();
            let shift = |r: Rect| r.translate(offset);
            self.rt.rects.push(SurfaceRects {
                key,
                window,
                areas: frame
                    .areas
                    .iter()
                    .map(|a| AreaRects { id: a.id, header: shift(a.header), body: shift(a.body), tabs: a.tabs.iter().map(|r| shift(*r)).collect() })
                    .collect(),
            });
        }

        self.track_drag(ui, key, window.map(|w| w.min.to_vec2()), style);
        self.paint_drop_overlay(ui, &frame, rect, style);
    }

    /// Обновляет положение перетаскиваемой вкладки, если её тащат из этого окна.
    fn track_drag(&mut self, ui: &Ui, key: SurfaceKey, offset: Option<Vec2>, style: &DockStyle) {
        let Some(drag) = &mut self.rt.drag else { return };
        if drag.src != key {
            return;
        }
        let (local, down) = ui.input(|i| (i.pointer.latest_pos().or(i.pointer.interact_pos()), i.pointer.primary_down()));
        if let (Some(p), Some(off)) = (local, offset) {
            drag.pointer = Some(p + off);
        }
        if !down {
            drag.released = true;
        }
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        // «Призрак» вкладки у курсора, пока курсор над окном-источником.
        if let Some(p) = local.filter(|p| ui.ctx().content_rect().contains(*p)) {
            let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, Id::new("mycelium-dock-ghost")));
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
        let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("mycelium-dock-overlay")));
        let fill = style.accent.gamma_multiply(0.22);
        let stroke = Stroke::new(1.5, style.accent.gamma_multiply(0.9));
        match self.rt.target {
            Target::Area { key, area, zone } if key == frame.key => {
                if let Some(a) = frame.areas.iter().find(|a| a.id == area) {
                    let r = zone_rect(a, zone);
                    let radius = if matches!(zone, Zone::Tab(_)) { 2 } else { RADIUS };
                    painter.rect(r.shrink(if matches!(zone, Zone::Tab(_)) { 0.0 } else { 3.0 }), CornerRadius::same(radius), fill, stroke, StrokeKind::Inside);
                }
            }
            Target::Empty(key) if key == frame.key => {
                painter.rect(surface_rect, CornerRadius::same(RADIUS), fill, stroke, StrokeKind::Inside);
            }
            _ => {}
        }
    }

    fn empty_surface(&mut self, ui: &mut Ui, key: SurfaceKey, rect: Rect, viewer: &mut impl TabViewer<Tab = T>, style: &DockStyle) {
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(RADIUS), style.area_bg);
        let mut child = ui.new_child(UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Center)));
        child.add_space((rect.height() * 0.38 - 40.0).max(8.0));
        child.label(egui::RichText::new(t!("dock-empty-title")).size(16.0).color(style.text_strong));
        child.add_space(4.0);
        child.label(egui::RichText::new(t!("dock-empty-hint")).size(13.0).color(style.text_muted));
        child.add_space(12.0);
        let resp = child.button(t!("dock-add-editor-button"));
        let menu = egui::Popup::menu(&resp).style(style.menu_style).show(|ui| viewer.add_menu(ui));
        if let Some(tab) = menu.and_then(|r| r.inner) {
            self.rt.ops.push(Op::Add { key, area: None, tab });
        }
    }

    fn show_node(&mut self, ui: &mut Ui, frame: &mut Frame0, node: &mut Node<T>, rect: Rect, viewer: &mut impl TabViewer<Tab = T>, style: &DockStyle) {
        match node {
            Node::Area(area) => self.show_area(ui, frame, area, rect, false, viewer, style),
            Node::Split { dir, ratio, children } => {
                let (r1, sep, r2) = split_rect(rect, *dir, *ratio);
                let id = Id::new(("mycelium-dock-sep", frame.key, children[0].first_area().id, children[1].first_area().id));
                let resp = ui.interact(sep.expand(2.0), id, Sense::drag());
                let horizontal = *dir == SplitDir::Horizontal;
                if resp.hovered() || resp.dragged() {
                    ui.ctx().set_cursor_icon(if horizontal { CursorIcon::ResizeHorizontal } else { CursorIcon::ResizeVertical });
                }
                if resp.dragged() {
                    if let Some(p) = resp.interact_pointer_pos() {
                        let (pos, start, len) = if horizontal { (p.x, rect.left(), rect.width() - GAP) } else { (p.y, rect.top(), rect.height() - GAP) };
                        let min = (MIN_AREA / len.max(1.0)).min(0.45);
                        *ratio = ((pos - start - GAP / 2.0) / len.max(1.0)).clamp(min, 1.0 - min);
                    }
                }
                let t = ui.ctx().animate_bool_with_time(id, resp.hovered() || resp.dragged(), 0.12);
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
    fn show_area(&mut self, ui: &mut Ui, frame: &mut Frame0, area: &mut Area<T>, rect: Rect, maximized: bool, viewer: &mut impl TabViewer<Tab = T>, style: &DockStyle) {
        let key = frame.key;
        let ctx = ui.ctx().clone();
        let header = Rect::from_min_size(rect.min, vec2(rect.width(), HEADER_H));
        let body = Rect::from_min_max(pos2(rect.left(), header.bottom()), rect.max);
        area.clamp_active();

        // Щелчок в любом месте области делает её «текущей».
        let pressed_here = ui.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| rect.contains(p)));
        if pressed_here {
            self.rt.focused.insert(key, area.id);
        }
        let focused = self.rt.focused.get(&key) == Some(&area.id);

        let painter = ui.painter().clone();
        painter.rect_filled(rect, CornerRadius::same(RADIUS), style.area_bg);
        painter.rect_filled(header, CornerRadius { nw: RADIUS, ne: RADIUS, sw: 0, se: 0 }, style.header_bg);
        let focus_t = ctx.animate_bool_with_time(Id::new(("mycelium-dock-focus", area.id)), focused, 0.2);
        let outline = style.border.lerp_to_gamma(style.accent.gamma_multiply(0.55), focus_t);
        painter.rect_stroke(rect, CornerRadius::same(RADIUS), Stroke::new(1.0, outline), StrokeKind::Inside);

        // Кнопки справа в строке вкладок: «+» и меню области. В узкой области «+» прячется
        // в меню области, чтобы вкладкам осталось место.
        let btn = 24.0;
        let show_add = rect.width() >= ADD_BUTTON_MIN_W;
        let menu_rect = Rect::from_center_size(pos2(header.right() - 6.0 - btn / 2.0, header.center().y), vec2(btn, btn));
        let add_rect = menu_rect.translate(vec2(-btn - 2.0, 0.0));
        let buttons_left = if show_add { add_rect.left() } else { menu_rect.left() };
        let tabs_rect = Rect::from_min_max(header.min + vec2(4.0, 0.0), pos2(buttons_left - 4.0, header.bottom()));

        if show_add {
            let add = icon_button(ui, add_rect, Id::new(("mycelium-dock-add", area.id)), ("add", "+"), style).on_hover_text(t!("dock-add-editor"));
            if let Some(tab) = egui::Popup::menu(&add).style(style.menu_style).show(|ui| viewer.add_menu(ui)).and_then(|r| r.inner) {
                self.rt.ops.push(Op::Add { key, area: Some(area.id), tab });
            }
        }
        let menu = icon_button(ui, menu_rect, Id::new(("mycelium-dock-menu", area.id)), ("more", "⋯"), style).on_hover_text(t!("dock-area-menu"));
        egui::Popup::menu(&menu).style(style.menu_style).show(|ui| {
            self.area_menu(ui, key, area, maximized, style, (!show_add).then_some(&mut *viewer));
        });

        // Вкладки.
        let font = FontId::proportional(13.0);
        let titles: Vec<(String, Option<(String, Color32)>, Id)> =
            area.tabs.iter().map(|tab| (viewer.title(tab), viewer.icon(tab), viewer.id(tab))).collect();
        let desired: Vec<f32> = titles
            .iter()
            .map(|(title, icon, _)| {
                let text_w = ui.painter().layout_no_wrap(title.clone(), font.clone(), Color32::WHITE).size().x;
                let icon_w = if icon.is_some() { 20.0 } else { 0.0 };
                (12.0 + icon_w + text_w + 26.0).clamp(64.0, 240.0)
            })
            .collect();
        let plan = plan_tabs(&desired, area.active, tabs_rect.width());
        let mut x = tabs_rect.left();
        let mut tab_rects = Vec::with_capacity(titles.len());
        for (i, (title, icon, tab_id)) in titles.iter().enumerate() {
            let Some(w) = plan.widths[i] else {
                // Спрятанная вкладка: пустой прямоугольник у края — по нему считается место броска.
                let edge = if i < plan.first { tabs_rect.left() - 1.0 } else { header.right() + 1.0 };
                tab_rects.push(Rect::from_min_size(pos2(edge, header.top() + 4.0), vec2(0.0, HEADER_H - 4.0)));
                continue;
            };
            let r = Rect::from_min_max(pos2(x, header.top() + 4.0), pos2((x + w).min(tabs_rect.right()), header.bottom()));
            x += w + 2.0;
            tab_rects.push(r);
            let id = tab_id.with("mycelium-dock-tab");
            let active = i == area.active;
            let dragging_this = self.rt.drag.as_ref().is_some_and(|d| d.src == key && d.area == area.id && d.index == i);
            let resp = ui.interact(r, id, Sense::click_and_drag());
            let (close_hit, elided) = paint_tab(ui, &resp, r, id, title, icon.as_ref(), active, dragging_this, style);
            // У обрезанной вкладки полное название — во всплывающей подсказке.
            let resp = if elided && self.rt.drag.is_none() { resp.on_hover_text(title) } else { resp };

            if resp.drag_started() && self.rt.drag.is_none() {
                self.rt.drag = Some(Drag {
                    src: key,
                    area: area.id,
                    index: i,
                    title: title.clone(),
                    pointer: None,
                    released: false,
                    size: vec2(rect.width(), rect.height()),
                });
            }
            if resp.middle_clicked() || (resp.clicked() && close_hit) {
                self.rt.ops.push(Op::Close { key, area: area.id, index: i });
            } else if resp.double_clicked() {
                self.rt.ops.push(Op::ToggleMaximize { key, area: area.id });
            } else if resp.clicked() {
                area.active = i;
                self.rt.focused.insert(key, area.id);
            }
            egui::Popup::context_menu(&resp).style(style.menu_style).show(|ui| {
                self.tab_menu(ui, key, area, i, maximized, style);
            });
        }

        // Не поместившиеся вкладки — в меню «»» справа от видимых.
        if plan.hidden() > 0 {
            let more_rect = Rect::from_center_size(pos2(x + 12.0, header.center().y + 2.0), vec2(24.0, 24.0));
            let more = icon_button(ui, more_rect, Id::new(("mycelium-dock-more", area.id)), ("overflow", "»"), style).on_hover_text(t!("dock-more-tabs", count = plan.hidden()));
            egui::Popup::menu(&more).style(style.menu_style).show(|ui| {
                for (i, (title, icon, _)) in titles.iter().enumerate() {
                    // Место под значок слева от названия; видимые вкладки — приглушённо.
                    let color = if plan.widths[i].is_some() { style.text_muted } else { style.text };
                    let resp = ui.add(egui::Button::new(egui::RichText::new(format!("      {title}")).color(color)).selected(i == area.active));
                    if let Some((name, tint)) = icon {
                        let r = Rect::from_center_size(resp.rect.left_center() + vec2(ui.spacing().button_padding.x + 8.0, 0.0), vec2(14.0, 14.0));
                        (style.paint_icon)(ui.painter(), r, name, if plan.widths[i].is_some() { tint.gamma_multiply(0.6) } else { *tint });
                    }
                    if resp.clicked() {
                        area.active = i;
                        self.rt.focused.insert(key, area.id);
                    }
                }
            });
        }

        // Содержимое активной вкладки.
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(body)
                .id_salt(("mycelium-dock-body", area.id))
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        child.set_clip_rect(body.intersect(ui.clip_rect()));
        if let Some(tab) = area.tabs.get_mut(area.active) {
            viewer.ui(&mut child, tab);
        }

        frame.areas.push(AreaRects { id: area.id, header, body, tabs: tab_rects });
    }

    /// Меню области. `add` — в узкой области сюда переезжает меню кнопки «+».
    fn area_menu(&mut self, ui: &mut Ui, key: SurfaceKey, area: &Area<T>, maximized: bool, style: &DockStyle, add: Option<&mut impl TabViewer<Tab = T>>) {
        let id = area.id;
        if let Some(viewer) = add {
            let mut picked = None;
            ui.menu_button(t!("dock-add-editor-button"), |ui| picked = viewer.add_menu(ui));
            if let Some(tab) = picked {
                self.rt.ops.push(Op::Add { key, area: Some(id), tab });
            }
            ui.separator();
        }
        if ui.button(t!("dock-split-right")).clicked() {
            self.rt.ops.push(Op::SplitDuplicate { key, area: id, zone: Zone::Right });
        }
        if ui.button(t!("dock-split-down")).clicked() {
            self.rt.ops.push(Op::SplitDuplicate { key, area: id, zone: Zone::Bottom });
        }
        ui.separator();
        let max_label = if maximized { t!("dock-restore") } else { t!("dock-maximize") };
        if ui.add(egui::Button::new(max_label).shortcut_text(style.maximize_text.as_str())).clicked() {
            self.rt.ops.push(Op::ToggleMaximize { key, area: id });
        }
        match key {
            SurfaceKey::Main => {
                if ui.button(t!("dock-detach")).clicked() {
                    let size = vec2(ui.ctx().content_rect().width() * 0.5, ui.ctx().content_rect().height() * 0.6);
                    self.rt.ops.push(Op::Detach { key, area: id, index: area.active, pos: None, size });
                }
            }
            SurfaceKey::Window(w) => {
                if ui.button(t!("dock-attach-window")).clicked() {
                    self.rt.ops.push(Op::Return(w));
                }
            }
        }
        ui.separator();
        if ui.button(t!("dock-close-area")).clicked() {
            self.rt.ops.push(Op::CloseArea { key, area: id });
        }
    }

    fn tab_menu(&mut self, ui: &mut Ui, key: SurfaceKey, area: &Area<T>, index: usize, maximized: bool, style: &DockStyle) {
        let id = area.id;
        if ui.button(t!("dock-close")).clicked() {
            self.rt.ops.push(Op::Close { key, area: id, index });
        }
        if ui.add_enabled(area.tabs.len() > 1, egui::Button::new(t!("dock-close-others"))).clicked() {
            self.rt.ops.push(Op::CloseOthers { key, area: id, index });
        }
        ui.separator();
        let several = area.tabs.len() > 1;
        if ui.add_enabled(several, egui::Button::new(t!("dock-move-right"))).clicked() {
            self.rt.ops.push(Op::MoveToSplit { key, area: id, index, zone: Zone::Right });
        }
        if ui.add_enabled(several, egui::Button::new(t!("dock-move-down"))).clicked() {
            self.rt.ops.push(Op::MoveToSplit { key, area: id, index, zone: Zone::Bottom });
        }
        ui.separator();
        match key {
            SurfaceKey::Main => {
                if ui.button(t!("dock-detach-tab")).clicked() {
                    let size = vec2(ui.ctx().content_rect().width() * 0.5, ui.ctx().content_rect().height() * 0.6);
                    self.rt.ops.push(Op::Detach { key, area: id, index, pos: None, size });
                }
            }
            SurfaceKey::Window(w) => {
                if ui.button(t!("dock-attach-window")).clicked() {
                    self.rt.ops.push(Op::Return(w));
                }
            }
        }
        let max_label = if maximized { t!("dock-restore") } else { t!("dock-maximize") };
        if ui.add(egui::Button::new(max_label).shortcut_text(style.maximize_text.as_str())).clicked() {
            self.rt.ops.push(Op::ToggleMaximize { key, area: id });
        }
    }
}

/// Небольшая кнопка-значок в строке вкладок: имя значка и символ на случай, если значка нет.
fn icon_button(ui: &mut Ui, rect: Rect, id: Id, (name, glyph): (&str, &str), style: &DockStyle) -> egui::Response {
    let resp = ui.interact(rect, id, Sense::click());
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&resp));
    let t = ui.ctx().animate_bool_with_time(id.with("hover"), resp.hovered() || open, 0.12);
    if t > 0.0 {
        ui.painter().rect_filled(rect, CornerRadius::same(6), style.hover_bg.gamma_multiply(t));
    }
    let color = style.text_muted.lerp_to_gamma(style.text_strong, t);
    if !(style.paint_icon)(ui.painter(), Rect::from_center_size(rect.center(), vec2(16.0, 16.0)), name, color) {
        ui.painter().text(rect.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(16.0), color);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Как разложить вкладки в строке: ширина каждой видимой (`None` — вкладка спрятана
/// в меню «»») и номер первой видимой.
#[derive(Debug, PartialEq)]
struct TabPlan {
    widths: Vec<Option<f32>>,
    first: usize,
}

impl TabPlan {
    fn hidden(&self) -> usize {
        self.widths.iter().filter(|w| w.is_none()).count()
    }
}

/// Вкладки желаемой ширины `desired` в строке шириной `avail`: если не помещаются, сжимаются
/// поровну, а если и так тесно — видны только соседние с активной, остальные уходят в меню.
fn plan_tabs(desired: &[f32], active: usize, avail: f32) -> TabPlan {
    const GAP: f32 = 2.0;
    let n = desired.len();
    if n == 0 {
        return TabPlan { widths: Vec::new(), first: 0 };
    }
    let gaps = GAP * (n - 1) as f32;
    let sum: f32 = desired.iter().sum();
    if sum + gaps <= avail {
        return TabPlan { widths: desired.iter().map(|w| Some(*w)).collect(), first: 0 };
    }
    let scale = ((avail - gaps) / sum).max(0.0);
    let shrunk: Vec<f32> = desired.iter().map(|d| (d * scale).max(MIN_TAB.min(*d))).collect();
    if shrunk.iter().sum::<f32>() + gaps <= avail + 0.5 {
        return TabPlan { widths: shrunk.into_iter().map(Some).collect(), first: 0 };
    }
    // Место под кнопку «»» и столько вкладок наименьшей ширины, сколько влезет (хотя бы одна).
    let room = (avail - 28.0).max(0.0);
    let k = (((room + GAP) / (MIN_TAB + GAP)).floor() as usize).clamp(1, n);
    let active = active.min(n.saturating_sub(1));
    let first = if active < k { 0 } else { active + 1 - k };
    let w = ((room - GAP * (k - 1) as f32) / k as f32).max(0.0);
    let widths = (0..n).map(|i| (first..first + k).contains(&i).then_some(w)).collect();
    TabPlan { widths, first }
}

/// Название вкладки в одну строку не шире `max_width`, с «…» на конце, если не влезло.
fn elided_title(ui: &Ui, title: &str, color: Color32, max_width: f32) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(title.to_owned(), FontId::proportional(13.0), color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_width.max(0.0));
    ui.painter().layout_job(job)
}

/// Рисует вкладку. Возвращает, наведён ли курсор на крестик закрытия и обрезано ли название.
#[allow(clippy::too_many_arguments)]
fn paint_tab(
    ui: &Ui,
    resp: &egui::Response,
    rect: Rect,
    id: Id,
    title: &str,
    icon: Option<&(String, Color32)>,
    active: bool,
    dragging: bool,
    style: &DockStyle,
) -> (bool, bool) {
    let ctx = ui.ctx();
    let a = ctx.animate_bool_with_time(id.with("active"), active, 0.18);
    let hover = ctx.animate_bool_with_time(id.with("hover"), resp.hovered(), 0.12);
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    let fade = if dragging { 0.45 } else { 1.0 };

    if hover > 0.0 && !active {
        painter.rect_filled(rect.shrink2(vec2(1.0, 2.0)), CornerRadius::same(6), style.hover_bg.gamma_multiply(hover * (1.0 - a)));
    }
    if a > 0.0 {
        painter.rect_filled(rect, CornerRadius { nw: 7, ne: 7, sw: 0, se: 0 }, style.area_bg.gamma_multiply(a));
        // Акцентная полоска сверху «вырастает» из центра.
        let half = (rect.width() / 2.0 - 8.0).max(0.0) * a;
        painter.hline(rect.center().x - half..=rect.center().x + half, rect.top() + 1.0, Stroke::new(2.0, style.accent));
    }

    let close_rect = Rect::from_center_size(pos2(rect.right() - 13.0, rect.center().y), Vec2::splat(16.0));
    // Совсем узкой вкладке крестик не нужен: закрыть её можно средней кнопкой или из меню.
    let closable = rect.width() >= 44.0;
    let mut x = rect.left() + 10.0;
    if let Some((name, color)) = icon {
        let r = Rect::from_center_size(pos2(x + 7.0, rect.center().y), vec2(14.0, 14.0));
        if !(style.paint_icon)(&painter, r, name, color.gamma_multiply(fade)) {
            painter.text(r.center(), Align2::CENTER_CENTER, name, FontId::proportional(13.0), color.gamma_multiply(fade));
        }
        x += 20.0;
    }
    let text_color = style.text_muted.lerp_to_gamma(style.text_strong, a.max(hover * 0.6)).gamma_multiply(fade);
    let text_right = if closable { close_rect.left() - 3.0 } else { rect.right() - 6.0 };
    let room = text_right - x;
    let elided = if room >= 12.0 {
        let galley = elided_title(ui, title, text_color, room);
        let elided = galley.elided;
        painter.galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley, text_color);
        elided
    } else {
        true
    };

    let on_close = closable && resp.hover_pos().is_some_and(|p| close_rect.contains(p));
    if closable && (resp.hovered() || active) {
        if on_close {
            painter.rect_filled(close_rect, CornerRadius::same(4), style.hover_bg);
        }
        let color = style.text_muted.gamma_multiply(a.max(hover) * fade);
        if !(style.paint_icon)(&painter, close_rect.shrink(3.0), "close", color) {
            painter.text(close_rect.center(), Align2::CENTER_CENTER, "×", FontId::proportional(14.0), color);
        }
    }
    if resp.hovered() {
        ctx.set_cursor_icon(CursorIcon::PointingHand);
    }
    (on_close, elided)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area_rects(id: u64, body: Rect) -> AreaRects {
        let header = Rect::from_min_size(body.min - vec2(0.0, HEADER_H), vec2(body.width(), HEADER_H));
        let tabs = vec![
            Rect::from_min_size(header.min, vec2(100.0, HEADER_H)),
            Rect::from_min_size(header.min + vec2(100.0, 0.0), vec2(100.0, HEADER_H)),
        ];
        AreaRects { id: AreaId(id), header, body, tabs }
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
            Target::Area { key: SurfaceKey::Main, area: AreaId(1), zone: Zone::Tab(1) },
            "над строкой вкладок — вставка между вкладками"
        );
        assert_eq!(compute_target(&rects, pos2(2.0, 2.0)), Target::Nothing, "над рамкой окна — ничего");
    }

    #[test]
    fn tabs_shrink_then_overflow_keeping_active_visible() {
        let desired = [120.0, 120.0, 120.0, 120.0];
        let all = plan_tabs(&desired, 0, 600.0);
        assert_eq!(all.hidden(), 0);
        assert_eq!(all.widths[0], Some(120.0));

        let shrunk = plan_tabs(&desired, 0, 400.0);
        assert_eq!(shrunk.hidden(), 0, "сначала вкладки сжимаются");
        assert!(shrunk.widths.iter().all(|w| w.is_some_and(|w| (MIN_TAB..120.0).contains(&w))));

        let tight = plan_tabs(&desired, 3, 200.0);
        assert!(tight.hidden() > 0, "дальше лишние уходят в меню");
        assert!(tight.widths[3].is_some(), "активная вкладка видна");
        assert_eq!(tight.first + tight.widths.iter().flatten().count(), 4, "видны соседние с активной");

        let tiny = plan_tabs(&desired, 1, 30.0);
        assert_eq!(tiny.widths.iter().flatten().count(), 1, "хотя бы одна вкладка видна всегда");
        assert!(tiny.widths[1].is_some());

        assert_eq!(plan_tabs(&[], 0, -10.0).hidden(), 0, "пустая область при нулевой ширине не падает");
        assert_eq!(plan_tabs(&desired, 0, -10.0).widths.iter().flatten().count(), 1);
    }

    struct V;
    impl TabViewer for V {
        type Tab = &'static str;
        fn title(&mut self, tab: &Self::Tab) -> String {
            tab.to_string()
        }
        fn id(&mut self, tab: &Self::Tab) -> Id {
            Id::new(tab)
        }
        fn ui(&mut self, _ui: &mut Ui, _tab: &mut Self::Tab) {}
        fn add_menu(&mut self, _ui: &mut Ui) -> Option<Self::Tab> {
            None
        }
        fn duplicate(&mut self, tab: &Self::Tab) -> Self::Tab {
            tab
        }
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
            dock.finish(&ctx, &mut main, &mut V);
        }
        assert!(dock.rt.rects.is_empty());
        assert_eq!(dock.rt.prev_rects.len(), 1);
        assert_eq!(dock.rt.prev_rects[0].window.min.x, 200.0, "остался только последний кадр");
    }

    #[test]
    fn moving_tabs_between_surfaces_and_windows() {
        let ctx = Context::default();
        let mut dock: DockState<&'static str> = DockState::default();
        dock.reserve_ids(10);
        let mut main = Surface::new(Node::area(AreaId(1), vec!["код", "консоль"]));

        // Вынести «консоль» в отдельное окно.
        dock.rt.ops.push(Op::Detach { key: SurfaceKey::Main, area: AreaId(1), index: 1, pos: None, size: vec2(500.0, 400.0) });
        dock.finish(&ctx, &mut main, &mut V);
        assert_eq!(dock.windows.len(), 1);
        assert_eq!(main.tab_count(), 1);
        let win = dock.windows[0].id;

        // Перетащить её обратно справа от «кода».
        let win_area = dock.windows[0].surface.first_area_id().unwrap();
        dock.rt.ops.push(Op::Move {
            from: (SurfaceKey::Window(win), win_area, 0),
            to: Target::Area { key: SurfaceKey::Main, area: AreaId(1), zone: Zone::Right },
        });
        dock.finish(&ctx, &mut main, &mut V);
        assert!(dock.windows.is_empty(), "опустевшее окно закрывается");
        assert_eq!(main.areas().len(), 2);

        // Снова вынести и закрыть окно — вкладка возвращается.
        let right = main.areas()[1].id;
        dock.rt.ops.push(Op::Detach { key: SurfaceKey::Main, area: right, index: 0, pos: None, size: DEFAULT_WINDOW });
        dock.finish(&ctx, &mut main, &mut V);
        dock.return_all();
        dock.finish(&ctx, &mut main, &mut V);
        assert!(dock.windows.is_empty());
        assert_eq!(main.tab_count(), 2);
    }
}
