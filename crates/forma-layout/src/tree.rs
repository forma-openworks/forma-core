//! Дерево разбиений одного окна: области, разделённые по горизонтали или вертикали, как
//! экран в Blender. В каждой области — один редактор. Здесь только данные и операции над
//! ними — без отрисовки, чтобы всё можно было проверить тестами.

use serde::{Deserialize, Serialize};

/// Уникальный номер области (во всех окнах и рабочих пространствах сразу).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AreaId(pub u64);

/// Направление разбиения.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitDir {
    /// Дети стоят рядом: левый и правый (граница между ними вертикальная).
    Horizontal,
    /// Дети друг над другом: верхний и нижний (граница горизонтальная).
    Vertical,
}

impl SplitDir {
    pub fn flip(self) -> Self {
        match self {
            SplitDir::Horizontal => SplitDir::Vertical,
            SplitDir::Vertical => SplitDir::Horizontal,
        }
    }
}

/// Область: один редактор.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "AreaRepr<T>", bound(serialize = "T: Serialize", deserialize = "T: Deserialize<'de>"))]
pub struct Area<T> {
    pub id: AreaId,
    pub editor: T,
}

/// Как область лежит в файле раскладки — нынешняя и прежняя, с вкладками.
#[derive(Deserialize)]
#[serde(untagged, bound(deserialize = "T: Deserialize<'de>"))]
enum AreaRepr<T> {
    Current {
        id: AreaId,
        editor: T,
    },
    /// Раскладки до областей в духе Blender: вкладки, из которых видна одна.
    Tabs {
        id: AreaId,
        tabs: Vec<T>,
        #[serde(default)]
        active: usize,
    },
}

impl<T> TryFrom<AreaRepr<T>> for Area<T> {
    type Error = String;

    fn try_from(r: AreaRepr<T>) -> Result<Self, String> {
        match r {
            AreaRepr::Current { id, editor } => Ok(Area { id, editor }),
            AreaRepr::Tabs { id, mut tabs, active } => {
                if tabs.is_empty() {
                    return Err("область без вкладок".into());
                }
                let i = active.min(tabs.len() - 1);
                Ok(Area { id, editor: tabs.swap_remove(i) })
            }
        }
    }
}

impl<T> Area<T> {
    pub fn new(id: AreaId, editor: T) -> Self {
        Self { id, editor }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound(serialize = "T: Serialize", deserialize = "T: Deserialize<'de>"))]
pub enum Node<T> {
    Area(Area<T>),
    Split {
        dir: SplitDir,
        /// Доля первого ребёнка (0..1).
        ratio: f32,
        children: Box<[Node<T>; 2]>,
    },
}

/// Куда относительно области: середина или один из краёв.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Center,
    Left,
    Right,
    Top,
    Bottom,
}

impl Zone {
    /// Разбиение, которое создаёт эта зона: направление и «новая область идёт первой».
    pub fn split(self) -> Option<(SplitDir, bool)> {
        match self {
            Zone::Left => Some((SplitDir::Horizontal, true)),
            Zone::Right => Some((SplitDir::Horizontal, false)),
            Zone::Top => Some((SplitDir::Vertical, true)),
            Zone::Bottom => Some((SplitDir::Vertical, false)),
            Zone::Center => None,
        }
    }

    pub fn opposite(self) -> Zone {
        match self {
            Zone::Left => Zone::Right,
            Zone::Right => Zone::Left,
            Zone::Top => Zone::Bottom,
            Zone::Bottom => Zone::Top,
            Zone::Center => Zone::Center,
        }
    }
}

impl<T> Node<T> {
    pub fn split(dir: SplitDir, ratio: f32, first: Node<T>, second: Node<T>) -> Self {
        Node::Split { dir, ratio, children: Box::new([first, second]) }
    }

    pub fn area(id: AreaId, editor: T) -> Self {
        Node::Area(Area::new(id, editor))
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

    fn visit_mut<'a>(&'a mut self, out: &mut Vec<&'a mut Area<T>>) {
        match self {
            Node::Area(a) => out.push(a),
            Node::Split { children, .. } => {
                let [a, b] = &mut **children;
                a.visit_mut(out);
                b.visit_mut(out);
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

    /// Соседи-области: пары (область, её сосед по разбиению и где он стоит), если оба ребёнка
    /// разбиения — области. Только такие соседи объединяются, как в Blender.
    fn siblings(&self, out: &mut Vec<(AreaId, AreaId, Zone)>) {
        if let Node::Split { dir, children, .. } = self {
            if let (Node::Area(a), Node::Area(b)) = (&children[0], &children[1]) {
                let (after, before) = match dir {
                    SplitDir::Horizontal => (Zone::Right, Zone::Left),
                    SplitDir::Vertical => (Zone::Bottom, Zone::Top),
                };
                out.push((a.id, b.id, after));
                out.push((b.id, a.id, before));
            }
            children[0].siblings(out);
            children[1].siblings(out);
        }
    }

    fn split_mut(&mut self, first: AreaId, second: AreaId) -> Option<&mut Node<T>> {
        let here = match self {
            Node::Area(_) => return None,
            Node::Split { children, .. } => children[0].first_area().id == first && children[1].first_area().id == second,
        };
        if here {
            return Some(self);
        }
        let Node::Split { children, .. } = self else { return None };
        let [a, b] = &mut **children;
        a.split_mut(first, second).or_else(|| b.split_mut(first, second))
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

/// Объединяет соседей `keep` и `remove`: их разбиение становится областью `keep`.
fn join<T>(node: Node<T>, keep: AreaId, remove: AreaId, out: &mut Option<T>) -> Node<T> {
    match node {
        Node::Split { dir, ratio, children } => {
            let [a, b] = *children;
            match (a, b) {
                (Node::Area(x), Node::Area(y)) if (x.id == keep && y.id == remove) || (x.id == remove && y.id == keep) => {
                    let (kept, gone) = if x.id == keep { (x, y) } else { (y, x) };
                    *out = Some(gone.editor);
                    Node::Area(kept)
                }
                (a, b) => Node::split(dir, ratio, join(a, keep, remove, out), join(b, keep, remove, out)),
            }
        }
        n => n,
    }
}

/// Содержимое одного окна: дерево областей (или пусто) и развёрнутая область.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound(serialize = "T: Serialize", deserialize = "T: Deserialize<'de>"))]
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

    pub fn editors(&self) -> impl Iterator<Item = &T> {
        self.areas().into_iter().map(|a| &a.editor)
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

    pub fn count(&self) -> usize {
        self.areas().len()
    }

    pub fn first_area_id(&self) -> Option<AreaId> {
        self.root.as_ref().map(|r| r.first_area().id)
    }

    pub fn for_each_area_mut(&mut self, mut f: impl FnMut(&mut Area<T>)) {
        if let Some(root) = &mut self.root {
            root.for_each_mut(&mut f);
        }
    }

    /// Соседи, которые можно объединить: (область, сосед, где стоит сосед).
    pub fn siblings(&self) -> Vec<(AreaId, AreaId, Zone)> {
        let mut out = Vec::new();
        if let Some(root) = &self.root {
            root.siblings(&mut out);
        }
        out
    }

    /// Где стоит сосед `other` относительно `id`, если их можно объединить.
    pub fn sibling_side(&self, id: AreaId, other: AreaId) -> Option<Zone> {
        self.siblings().into_iter().find(|(a, b, _)| *a == id && *b == other).map(|(_, _, z)| z)
    }

    /// Убирает область целиком и возвращает её; сосед занимает её место.
    pub fn remove_area(&mut self, id: AreaId) -> Option<Area<T>> {
        let mut out = None;
        self.root = self.root.take().and_then(|r| remove_area(r, id, &mut out));
        if self.maximized == Some(id) {
            self.maximized = None;
        }
        out
    }

    /// Делит область `target`: новая область `area` встаёт с края `zone` и занимает `ratio`
    /// (долю) от прежней. Для середины ничего не делает и возвращает `area` обратно.
    pub fn split_area(&mut self, target: AreaId, zone: Zone, area: Area<T>, ratio: f32) -> Result<(), Area<T>> {
        let Some((dir, new_first)) = zone.split() else { return Err(area) };
        if !self.contains(target) {
            return Err(area);
        }
        let ratio = if new_first { ratio } else { 1.0 - ratio }.clamp(0.05, 0.95);
        let mut f = Some(move |old: Area<T>| {
            if new_first { Node::split(dir, ratio, Node::Area(area), Node::Area(old)) } else { Node::split(dir, ratio, Node::Area(old), Node::Area(area)) }
        });
        self.root = self.root.take().map(|r| replace_area(r, target, &mut f));
        if self.maximized == Some(target) {
            self.maximized = None;
        }
        Ok(())
    }

    /// Меняет редактор области; возвращает прежний.
    pub fn replace_editor(&mut self, target: AreaId, editor: T) -> Result<T, T> {
        match self.area_mut(target) {
            Some(a) => Ok(std::mem::replace(&mut a.editor, editor)),
            None => Err(editor),
        }
    }

    /// Объединяет соседние области: `keep` занимает место `remove`. Возвращает редактор убранной.
    pub fn join(&mut self, keep: AreaId, remove: AreaId) -> Option<T> {
        self.sibling_side(keep, remove)?;
        let mut out = None;
        self.root = self.root.take().map(|r| join(r, keep, remove, &mut out));
        if self.maximized == Some(remove) {
            self.maximized = None;
        }
        out
    }

    /// Меняет местами редакторы двух областей.
    pub fn swap_editors(&mut self, a: AreaId, b: AreaId) -> bool {
        if a == b {
            return false;
        }
        let Some(root) = &mut self.root else { return false };
        let mut all = Vec::new();
        root.visit_mut(&mut all);
        let mut pair = all.into_iter().filter(|x| x.id == a || x.id == b);
        match (pair.next(), pair.next()) {
            (Some(x), Some(y)) => {
                std::mem::swap(&mut x.editor, &mut y.editor);
                true
            }
            _ => false,
        }
    }

    /// Меняет местами половины разбиения, первая половина которого начинается с области
    /// `first`, а вторая — с `second` (меню границы «Поменять местами»).
    pub fn swap_split(&mut self, first: AreaId, second: AreaId) -> bool {
        match self.root.as_mut().and_then(|r| r.split_mut(first, second)) {
            Some(Node::Split { ratio, children, .. }) => {
                children.swap(0, 1);
                *ratio = 1.0 - *ratio;
                true
            }
            _ => false,
        }
    }

    /// Все редакторы окна по порядку (окно при этом пустеет).
    pub fn take_all(&mut self) -> Vec<T> {
        let mut out = Vec::new();
        if let Some(root) = self.root.take() {
            fn collect<T>(node: Node<T>, out: &mut Vec<T>) {
                match node {
                    Node::Area(a) => out.push(a.editor),
                    Node::Split { children, .. } => {
                        let [a, b] = *children;
                        collect(a, out);
                        collect(b, out);
                    }
                }
            }
            collect(root, &mut out);
        }
        self.maximized = None;
        out
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

    /// Чинит то, что могло сломаться в файле раскладки: неверные доли, пропавшую развёрнутую область.
    pub fn sanitize(&mut self) {
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
        if let Some(id) = self.maximized
            && !self.contains(id)
        {
            self.maximized = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(s: &Surface<&'static str>) -> Vec<(u64, &'static str)> {
        s.areas().iter().map(|a| (a.id.0, a.editor)).collect()
    }

    fn two_areas() -> Surface<&'static str> {
        Surface::new(Node::split(SplitDir::Horizontal, 0.3, Node::area(AreaId(1), "проект"), Node::area(AreaId(2), "код")))
    }

    #[test]
    fn removing_area_collapses_split() {
        let mut s = two_areas();
        assert_eq!(s.remove_area(AreaId(1)).map(|a| a.editor), Some("проект"));
        assert!(matches!(s.root, Some(Node::Area(_))), "разбиение схлопнулось");
        assert_eq!(ids(&s), vec![(2, "код")]);
        s.remove_area(AreaId(2));
        assert!(s.is_empty());
    }

    #[test]
    fn split_area_puts_new_area_on_the_side() {
        let mut s = two_areas();
        s.split_area(AreaId(2), Zone::Bottom, Area::new(AreaId(3), "консоль"), 0.3).unwrap();
        assert_eq!(ids(&s), vec![(1, "проект"), (2, "код"), (3, "консоль")]);
        let Some(Node::Split { children, .. }) = &s.root else { panic!() };
        let Node::Split { dir, ratio, .. } = &children[1] else { panic!() };
        assert_eq!(*dir, SplitDir::Vertical);
        assert!((ratio - 0.7).abs() < 1e-6, "новая область снизу занимает 30%");
        s.split_area(AreaId(1), Zone::Left, Area::new(AreaId(4), "карта"), 0.5).unwrap();
        assert_eq!(s.areas()[0].id, AreaId(4), "новая область слева идёт первой");
        assert!(s.split_area(AreaId(1), Zone::Center, Area::new(AreaId(5), "x"), 0.5).is_err());
        assert!(s.split_area(AreaId(99), Zone::Left, Area::new(AreaId(5), "x"), 0.5).is_err());
    }

    #[test]
    fn join_only_neighbours() {
        let mut s = two_areas();
        s.split_area(AreaId(2), Zone::Bottom, Area::new(AreaId(3), "консоль"), 0.3).unwrap();
        assert_eq!(s.sibling_side(AreaId(2), AreaId(3)), Some(Zone::Bottom));
        assert_eq!(s.sibling_side(AreaId(1), AreaId(2)), None, "у проекта сосед — целое разбиение");
        assert_eq!(s.join(AreaId(1), AreaId(2)), None);
        assert_eq!(s.join(AreaId(3), AreaId(2)), Some("код"));
        assert_eq!(ids(&s), vec![(1, "проект"), (3, "консоль")]);
        assert_eq!(s.sibling_side(AreaId(1), AreaId(3)), Some(Zone::Right));
    }

    #[test]
    fn swaps() {
        let mut s = two_areas();
        assert!(s.swap_editors(AreaId(1), AreaId(2)));
        assert_eq!(ids(&s), vec![(1, "код"), (2, "проект")]);
        assert!(s.swap_split(AreaId(1), AreaId(2)));
        assert_eq!(ids(&s), vec![(2, "проект"), (1, "код")]);
        let Some(Node::Split { ratio, .. }) = &s.root else { panic!() };
        assert!((ratio - 0.7).abs() < 1e-6, "размеры половин сохранились");
    }

    #[test]
    fn replace_and_take_all() {
        let mut s = two_areas();
        assert_eq!(s.replace_editor(AreaId(2), "консоль"), Ok("код"));
        assert_eq!(s.take_all(), vec!["проект", "консоль"]);
        assert!(s.is_empty());
    }

    #[test]
    fn maximized_is_cleared_with_its_area() {
        let mut s = two_areas();
        s.maximized = Some(AreaId(1));
        s.remove_area(AreaId(1));
        assert_eq!(s.maximized, None);
    }

    #[test]
    fn sanitize_fixes_broken_layout() {
        let mut s: Surface<&str> = Surface::new(Node::split(SplitDir::Vertical, f32::NAN, Node::area(AreaId(1), "a"), Node::area(AreaId(2), "b")));
        s.maximized = Some(AreaId(77));
        s.sanitize();
        let Some(Node::Split { ratio, .. }) = &s.root else { panic!() };
        assert_eq!(*ratio, 0.5);
        assert_eq!(s.maximized, None);
    }

    #[test]
    fn layout_roundtrips_and_reads_old_tabs() {
        let mut s = two_areas();
        s.maximized = Some(AreaId(2));
        let json = serde_json::to_string(&s).unwrap();
        let back: Surface<String> = serde_json::from_str(&json).unwrap();
        assert_eq!(back.areas().len(), 2);
        assert_eq!(back.maximized, Some(AreaId(2)));

        // Раскладка до областей в духе Blender: видна была вторая вкладка.
        let old = r#"{"root":{"Area":{"id":1,"tabs":["код","консоль"],"active":1}},"maximized":null}"#;
        let back: Surface<String> = serde_json::from_str(old).unwrap();
        assert_eq!(back.areas()[0].editor, "консоль");
        let broken = r#"{"root":{"Area":{"id":1,"tabs":[],"active":0}}}"#;
        assert!(serde_json::from_str::<Surface<String>>(broken).is_err());
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
