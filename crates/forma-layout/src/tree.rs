//! Дерево разбиений одного окна: области с вкладками, разделённые по горизонтали
//! или вертикали, как экран в Blender. Здесь только данные и операции над ними —
//! без отрисовки, чтобы всё можно было проверить тестами.

use serde::{Deserialize, Serialize};

/// Уникальный номер области (во всех окнах и рабочих пространствах сразу).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AreaId(pub u64);

/// Направление разбиения.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitDir {
    /// Дети стоят рядом: левый и правый.
    Horizontal,
    /// Дети друг над другом: верхний и нижний.
    Vertical,
}

/// Область: набор вкладок, одна из которых показана.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Area<T> {
    pub id: AreaId,
    pub tabs: Vec<T>,
    pub active: usize,
}

impl<T> Area<T> {
    pub fn new(id: AreaId, tabs: Vec<T>) -> Self {
        Self { id, tabs, active: 0 }
    }

    pub fn active_tab(&self) -> Option<&T> {
        self.tabs.get(self.active.min(self.tabs.len().saturating_sub(1)))
    }

    /// Держит номер активной вкладки в допустимых пределах.
    pub fn clamp_active(&mut self) {
        self.active = self.active.min(self.tabs.len().saturating_sub(1));
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Node<T> {
    Area(Area<T>),
    Split {
        dir: SplitDir,
        /// Доля первого ребёнка (0..1).
        ratio: f32,
        children: Box<[Node<T>; 2]>,
    },
}

impl<T> Node<T> {
    pub fn split(dir: SplitDir, ratio: f32, first: Node<T>, second: Node<T>) -> Self {
        Node::Split { dir, ratio, children: Box::new([first, second]) }
    }

    pub fn area(id: AreaId, tabs: Vec<T>) -> Self {
        Node::Area(Area::new(id, tabs))
    }

    /// Первая (верхняя левая) область поддерева.
    pub fn first_area(&self) -> &Area<T> {
        match self {
            Node::Area(a) => a,
            Node::Split { children, .. } => children[0].first_area(),
        }
    }

    fn visit<'a>(&'a self, out: &mut Vec<&'a Area<T>>) {
        match self {
            Node::Area(a) => out.push(a),
            Node::Split { children, .. } => {
                children[0].visit(out);
                children[1].visit(out);
            }
        }
    }

    pub(crate) fn find_mut(&mut self, id: AreaId) -> Option<&mut Area<T>> {
        match self {
            Node::Area(a) => (a.id == id).then_some(a),
            Node::Split { children, .. } => {
                let [a, b] = &mut **children;
                a.find_mut(id).or_else(|| b.find_mut(id))
            }
        }
    }

    fn for_each_mut(&mut self, f: &mut impl FnMut(&mut Area<T>)) {
        match self {
            Node::Area(a) => f(a),
            Node::Split { children, .. } => {
                children[0].for_each_mut(f);
                children[1].for_each_mut(f);
            }
        }
    }
}

/// Убирает область из дерева; разбиение, у которого остался один ребёнок, схлопывается.
fn remove_area<T>(node: Node<T>, id: AreaId, out: &mut Option<Area<T>>) -> Option<Node<T>> {
    match node {
        Node::Area(a) if a.id == id => {
            *out = Some(a);
            None
        }
        Node::Area(a) => Some(Node::Area(a)),
        Node::Split { dir, ratio, children } => {
            let [a, b] = *children;
            match (remove_area(a, id, out), remove_area(b, id, out)) {
                (Some(a), Some(b)) => Some(Node::split(dir, ratio, a, b)),
                (Some(x), None) | (None, Some(x)) => Some(x),
                (None, None) => None,
            }
        }
    }
}

/// Заменяет область с номером `id` результатом `f` (вызывается не больше одного раза).
fn replace_area<T, F: FnOnce(Area<T>) -> Node<T>>(node: Node<T>, id: AreaId, f: &mut Option<F>) -> Node<T> {
    match node {
        Node::Area(a) if a.id == id => match f.take() {
            Some(f) => f(a),
            None => Node::Area(a),
        },
        Node::Area(a) => Node::Area(a),
        Node::Split { dir, ratio, children } => {
            let [a, b] = *children;
            Node::split(dir, ratio, replace_area(a, id, f), replace_area(b, id, f))
        }
    }
}

/// Куда положить вкладку относительно области.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    /// Ещё одной вкладкой в конец.
    Center,
    Left,
    Right,
    Top,
    Bottom,
    /// В строку вкладок перед вкладкой с этим номером.
    Tab(usize),
}

impl Zone {
    /// Разбиение, которое создаёт эта зона: направление и «новая область идёт первой».
    pub fn split(self) -> Option<(SplitDir, bool)> {
        match self {
            Zone::Left => Some((SplitDir::Horizontal, true)),
            Zone::Right => Some((SplitDir::Horizontal, false)),
            Zone::Top => Some((SplitDir::Vertical, true)),
            Zone::Bottom => Some((SplitDir::Vertical, false)),
            Zone::Center | Zone::Tab(_) => None,
        }
    }
}

/// Содержимое одного окна: дерево областей (или пусто) и развёрнутая область.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Surface<T> {
    pub root: Option<Node<T>>,
    #[serde(default)]
    pub maximized: Option<AreaId>,
}

impl<T> Default for Surface<T> {
    fn default() -> Self {
        Self { root: None, maximized: None }
    }
}

impl<T> Surface<T> {
    pub fn new(root: Node<T>) -> Self {
        Self { root: Some(root), maximized: None }
    }

    pub fn areas(&self) -> Vec<&Area<T>> {
        let mut out = Vec::new();
        if let Some(root) = &self.root {
            root.visit(&mut out);
        }
        out
    }

    pub fn area(&self, id: AreaId) -> Option<&Area<T>> {
        self.areas().into_iter().find(|a| a.id == id)
    }

    pub fn area_mut(&mut self, id: AreaId) -> Option<&mut Area<T>> {
        self.root.as_mut()?.find_mut(id)
    }

    pub fn contains(&self, id: AreaId) -> bool {
        self.area(id).is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    pub fn tab_count(&self) -> usize {
        self.areas().iter().map(|a| a.tabs.len()).sum()
    }

    pub fn first_area_id(&self) -> Option<AreaId> {
        self.root.as_ref().map(|r| r.first_area().id)
    }

    pub fn for_each_area_mut(&mut self, mut f: impl FnMut(&mut Area<T>)) {
        if let Some(root) = &mut self.root {
            root.for_each_mut(&mut f);
        }
    }

    /// Убирает область целиком и возвращает её.
    pub fn remove_area(&mut self, id: AreaId) -> Option<Area<T>> {
        let mut out = None;
        self.root = self.root.take().and_then(|r| remove_area(r, id, &mut out));
        if self.maximized == Some(id) {
            self.maximized = None;
        }
        out
    }

    /// Вынимает вкладку. Опустевшая область исчезает, соседняя занимает её место.
    pub fn remove_tab(&mut self, id: AreaId, index: usize) -> Option<T> {
        let area = self.area_mut(id)?;
        if index >= area.tabs.len() {
            return None;
        }
        let tab = area.tabs.remove(index);
        if index < area.active || area.active >= area.tabs.len() {
            area.active = area.active.saturating_sub(1);
        }
        area.clamp_active();
        if area.tabs.is_empty() {
            self.remove_area(id);
        }
        Some(tab)
    }

    /// Кладёт вкладку в область `target`. Для краевых зон область делится пополам,
    /// и вкладка попадает в новую область с номером `new_id`.
    /// Если такой области нет, вкладка добавляется как в [`Self::add_tab`].
    pub fn insert(&mut self, target: AreaId, zone: Zone, tab: T, new_id: AreaId) {
        if !self.contains(target) {
            self.add_tab(tab, None, new_id);
            return;
        }
        match zone.split() {
            Some((dir, new_first)) => {
                let new_area = Node::area(new_id, vec![tab]);
                let mut f = Some(move |old: Area<T>| {
                    if new_first {
                        Node::split(dir, 0.5, new_area, Node::Area(old))
                    } else {
                        Node::split(dir, 0.5, Node::Area(old), new_area)
                    }
                });
                self.root = self.root.take().map(|r| replace_area(r, target, &mut f));
                if self.maximized == Some(target) {
                    self.maximized = None;
                }
            }
            None => {
                let area = self.area_mut(target).expect("область проверена выше");
                let index = match zone {
                    Zone::Tab(i) => i.min(area.tabs.len()),
                    _ => area.tabs.len(),
                };
                area.tabs.insert(index, tab);
                area.active = index;
            }
        }
    }

    /// Добавляет вкладку в область `prefer` (если она есть), иначе в первую область,
    /// а в пустое окно — новой областью с номером `new_id`.
    pub fn add_tab(&mut self, tab: T, prefer: Option<AreaId>, new_id: AreaId) {
        let target = prefer.filter(|id| self.contains(*id)).or_else(|| self.first_area_id());
        match target {
            Some(id) => {
                let area = self.area_mut(id).expect("область существует");
                area.tabs.push(tab);
                area.active = area.tabs.len() - 1;
            }
            None => self.root = Some(Node::area(new_id, vec![tab])),
        }
    }

    /// Все вкладки окна по порядку (окно при этом пустеет).
    pub fn take_all_tabs(&mut self) -> Vec<T> {
        let mut tabs = Vec::new();
        self.for_each_area_mut(|a| tabs.append(&mut a.tabs));
        self.root = None;
        self.maximized = None;
        tabs
    }

    /// Самый большой номер области — чтобы после загрузки выдавать новые без повторов.
    pub fn max_area_id(&self) -> u64 {
        self.areas().iter().map(|a| a.id.0).max().unwrap_or(0)
    }

    /// Выдаёт всем областям новые номера (например, при копировании рабочего пространства).
    pub fn reassign_ids(&mut self, mut next: impl FnMut() -> AreaId) {
        let old_max = self.maximized;
        let mut new_max = None;
        self.for_each_area_mut(|a| {
            let id = next();
            if old_max == Some(a.id) {
                new_max = Some(id);
            }
            a.id = id;
        });
        self.maximized = new_max;
    }

    /// Чинит то, что могло сломаться в файле раскладки: пустые области, неверные доли.
    pub fn sanitize(&mut self) {
        let mut empty = Vec::new();
        self.for_each_area_mut(|a| {
            a.clamp_active();
            if a.tabs.is_empty() {
                empty.push(a.id);
            }
        });
        for id in empty {
            self.remove_area(id);
        }
        fn fix<T>(node: &mut Node<T>) {
            if let Node::Split { ratio, children, .. } = node {
                if !ratio.is_finite() {
                    *ratio = 0.5;
                }
                *ratio = ratio.clamp(0.05, 0.95);
                fix(&mut children[0]);
                fix(&mut children[1]);
            }
        }
        if let Some(root) = &mut self.root {
            fix(root);
        }
        if let Some(id) = self.maximized {
            if !self.contains(id) {
                self.maximized = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(s: &Surface<&'static str>) -> Vec<(u64, Vec<&'static str>)> {
        s.areas().iter().map(|a| (a.id.0, a.tabs.clone())).collect()
    }

    fn two_areas() -> Surface<&'static str> {
        Surface::new(Node::split(
            SplitDir::Horizontal,
            0.3,
            Node::area(AreaId(1), vec!["проект"]),
            Node::area(AreaId(2), vec!["код", "консоль"]),
        ))
    }

    #[test]
    fn removing_last_tab_collapses_split() {
        let mut s = two_areas();
        assert_eq!(s.remove_tab(AreaId(1), 0), Some("проект"));
        assert!(matches!(s.root, Some(Node::Area(_))), "разбиение схлопнулось");
        assert_eq!(ids(&s), vec![(2, vec!["код", "консоль"])]);
        assert_eq!(s.remove_tab(AreaId(2), 1), Some("консоль"));
        assert_eq!(s.remove_tab(AreaId(2), 0), Some("код"));
        assert!(s.is_empty());
    }

    #[test]
    fn insert_into_edge_splits_area() {
        let mut s = two_areas();
        s.insert(AreaId(2), Zone::Bottom, "исключения", AreaId(3));
        assert_eq!(ids(&s), vec![(1, vec!["проект"]), (2, vec!["код", "консоль"]), (3, vec!["исключения"])]);
        let Some(Node::Split { children, .. }) = &s.root else { panic!() };
        assert!(matches!(&children[1], Node::Split { dir: SplitDir::Vertical, .. }));

        s.insert(AreaId(1), Zone::Left, "карта", AreaId(4));
        assert_eq!(s.areas()[0].id, AreaId(4), "новая область слева идёт первой");
    }

    #[test]
    fn insert_into_tab_bar_keeps_order() {
        let mut s = two_areas();
        s.insert(AreaId(2), Zone::Tab(1), "карта", AreaId(9));
        assert_eq!(ids(&s)[1], (2, vec!["код", "карта", "консоль"]));
        assert_eq!(s.area(AreaId(2)).unwrap().active, 1, "вставленная вкладка становится активной");
        s.insert(AreaId(2), Zone::Tab(99), "конец", AreaId(9));
        assert_eq!(s.area(AreaId(2)).unwrap().tabs.last(), Some(&"конец"));
    }

    #[test]
    fn active_tab_stays_valid_after_removal() {
        let mut s = Surface::new(Node::area(AreaId(1), vec!["a", "b", "c"]));
        s.area_mut(AreaId(1)).unwrap().active = 2;
        s.remove_tab(AreaId(1), 2);
        assert_eq!(s.area(AreaId(1)).unwrap().active, 1);
        s.area_mut(AreaId(1)).unwrap().active = 1;
        s.remove_tab(AreaId(1), 0);
        assert_eq!(s.area(AreaId(1)).unwrap().active_tab(), Some(&"b"), "показана та же вкладка");
    }

    #[test]
    fn add_tab_prefers_given_area_and_fills_empty_surface() {
        let mut s = Surface::default();
        s.add_tab("код", Some(AreaId(5)), AreaId(7));
        assert_eq!(ids(&s), vec![(7, vec!["код"])]);
        let mut s = two_areas();
        s.add_tab("карта", Some(AreaId(2)), AreaId(8));
        assert_eq!(ids(&s)[1].1, vec!["код", "консоль", "карта"]);
        s.add_tab("ещё", Some(AreaId(42)), AreaId(8));
        assert_eq!(ids(&s)[0].1, vec!["проект", "ещё"], "нет такой области — в первую");
    }

    #[test]
    fn maximized_is_cleared_with_its_area() {
        let mut s = two_areas();
        s.maximized = Some(AreaId(1));
        s.remove_tab(AreaId(1), 0);
        assert_eq!(s.maximized, None);
    }

    #[test]
    fn sanitize_fixes_broken_layout() {
        let mut s: Surface<&str> = Surface::new(Node::split(
            SplitDir::Vertical,
            f32::NAN,
            Node::area(AreaId(1), vec![]),
            Node::area(AreaId(2), vec!["код"]),
        ));
        s.maximized = Some(AreaId(77));
        s.sanitize();
        assert_eq!(ids(&s), vec![(2, vec!["код"])]);
        assert_eq!(s.maximized, None);
    }

    #[test]
    fn layout_roundtrips_through_json() {
        let mut s = two_areas();
        s.maximized = Some(AreaId(2));
        let json = serde_json::to_string(&s).unwrap();
        let back: Surface<String> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.areas().len(), 2);
        assert_eq!(back.maximized, Some(AreaId(2)));
    }

    #[test]
    fn reassign_ids_keeps_maximized() {
        let mut s = two_areas();
        s.maximized = Some(AreaId(2));
        let mut n = 100;
        s.reassign_ids(|| {
            n += 1;
            AreaId(n)
        });
        assert_eq!(ids(&s).iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![101, 102]);
        assert_eq!(s.maximized, Some(AreaId(102)));
        assert_eq!(s.max_area_id(), 102);
    }
}
