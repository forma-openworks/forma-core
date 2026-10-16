//! Проверка плагинов до запуска, как у загрузчика модов: подходит ли плагин программе и
//! версии API, всё ли есть из зависимостей и тех ли версий, нет ли циклов. Плагин с
//! проблемами не подключается (ни его темы, ни code), а страница «Плагины» показывает,
//! чего ему не хватает. Плагины, которые зависят от неработающего, тоже не подключаются.

use std::collections::{BTreeMap, BTreeSet};

use crate::Installed;
use crate::manifest::parse_version;

/// Почему плагин не подключён.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// Плагин для других программ (`apps`).
    OtherApps(Vec<String>),
    /// Нужна другая версия API плагинов.
    Api(String),
    /// Нет плагина, от которого этот зависит.
    Missing { dep: String, need: String },
    /// Зависимость есть, но не той версии.
    Version { dep: String, need: String, found: String },
    /// Зависимость выключена.
    Disabled(String),
    /// Зависимость сама не работает.
    Broken(String),
    /// Плагины зависят друг от друга по кругу.
    Cycle(Vec<String>),
    /// Плагин с этим id найден дважды (установлен и подключён из папки разработчика).
    Duplicate,
}

/// Итог проверки: проблемы каждого плагина и порядок подключения работающих (зависимости —
/// раньше тех, кто от них зависит).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub problems: BTreeMap<String, Vec<Problem>>,
    pub order: Vec<String>,
}

impl Report {
    /// Работает ли плагин: включён и без проблем.
    pub fn works(&self, id: &str) -> bool {
        self.order.iter().any(|o| o == id)
    }

    pub fn problems(&self, id: &str) -> &[Problem] {
        self.problems.get(id).map_or(&[], Vec::as_slice)
    }
}

/// Проверяет плагины программы `app`. `enabled` — включён ли плагин в настройках.
pub fn check(plugins: &[Installed], app: &str, enabled: impl Fn(&str) -> bool) -> Report {
    let mut report = Report::default();
    let mut seen = BTreeSet::new();
    let mut own: BTreeMap<String, Vec<Problem>> = BTreeMap::new();
    for p in plugins {
        let id = p.id().to_owned();
        let list = own.entry(id.clone()).or_default();
        if !seen.insert(id.clone()) {
            list.push(Problem::Duplicate);
            continue;
        }
        let m = &p.manifest;
        if !m.for_app(app) {
            list.push(Problem::OtherApps(m.plugin.apps.clone()));
        }
        if !m.api_ok() {
            list.push(Problem::Api(m.plugin.api.clone().unwrap_or_default()));
        }
        for (dep, d) in &m.dependencies {
            let need = d.version().to_owned();
            match plugins.iter().find(|q| q.id() == dep) {
                None if d.optional() => {}
                None => list.push(Problem::Missing { dep: dep.clone(), need }),
                Some(q) => {
                    let req = semver::VersionReq::parse(&need).ok();
                    let found = q.manifest.version();
                    let fits = matches!((&req, &found), (Some(r), Some(v)) if r.matches(v));
                    if !fits {
                        list.push(Problem::Version { dep: dep.clone(), need, found: q.manifest.plugin.version.clone() });
                    } else if !enabled(dep) && !d.optional() {
                        list.push(Problem::Disabled(dep.clone()));
                    }
                }
            }
        }
    }
    // Циклы: обход в глубину по обязательным и присутствующим зависимостям.
    let deps = |id: &str| -> Vec<String> {
        plugins.iter().find(|p| p.id() == id).map(|p| p.manifest.dependencies.keys().filter(|d| plugins.iter().any(|q| q.id() == d.as_str())).cloned().collect()).unwrap_or_default()
    };
    for p in plugins {
        let mut path = vec![p.id().to_owned()];
        if let Some(cycle) = find_cycle(&mut path, &deps)
            && let Some(list) = own.get_mut(p.id())
            && !list.iter().any(|x| matches!(x, Problem::Cycle(_)))
        {
            list.push(Problem::Cycle(cycle));
        }
    }
    // Порядок подключения: сначала то, от чего зависят. Плагин работает, если он включён,
    // без своих проблем и все его зависимости работают.
    let mut placed: BTreeSet<String> = BTreeSet::new();
    let mut broken: BTreeSet<String> = own.iter().filter(|(id, list)| !list.is_empty() || !enabled(id)).map(|(id, _)| id.clone()).collect();
    loop {
        let mut progress = false;
        for p in plugins {
            let id = p.id();
            if placed.contains(id) || broken.contains(id) {
                continue;
            }
            let present: Vec<&String> = p.manifest.dependencies.keys().filter(|d| plugins.iter().any(|q| q.id() == d.as_str())).collect();
            if let Some(bad) = present.iter().find(|d| broken.contains(d.as_str()) && !p.manifest.dependencies[d.as_str()].optional()) {
                own.entry(id.to_owned()).or_default().push(Problem::Broken((*bad).clone()));
                broken.insert(id.to_owned());
                progress = true;
            } else if present.iter().all(|d| placed.contains(d.as_str()) || broken.contains(d.as_str())) {
                placed.insert(id.to_owned());
                report.order.push(id.to_owned());
                progress = true;
            }
        }
        if !progress {
            break;
        }
    }
    report.problems = own.into_iter().filter(|(_, l)| !l.is_empty()).collect();
    report
}

/// Ищет цикл зависимостей от последнего плагина в `path`.
fn find_cycle(path: &mut Vec<String>, deps: &impl Fn(&str) -> Vec<String>) -> Option<Vec<String>> {
    let last = path.last()?.clone();
    for d in deps(&last) {
        if d == path[0] {
            let mut cycle = path.clone();
            cycle.push(d);
            return Some(cycle);
        }
        if path.contains(&d) || path.len() > 32 {
            continue;
        }
        path.push(d);
        if let Some(c) = find_cycle(path, deps) {
            return Some(c);
        }
        path.pop();
    }
    None
}

/// Версия зависимости подходит под строку версий.
pub fn fits(need: &str, version: &str) -> bool {
    matches!((semver::VersionReq::parse(need), parse_version(version)), (Ok(r), Some(v)) if r.matches(&v))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Manifest;
    use std::path::PathBuf;

    fn plugin(text: &str) -> Installed {
        let manifest = Manifest::parse(text).unwrap();
        Installed { dir: PathBuf::from(&manifest.plugin.id), manifest, dev: false, builtin: false }
    }

    #[test]
    fn dependencies_like_a_mod_loader() {
        let plugins = vec![
            plugin("[plugin]\nid = \"myuri\"\nversion = \"0.1.2\""),
            plugin("[plugin]\nid = \"blender\"\nversion = \"0.3.0\"\napps = [\"holotype\"]"),
            plugin("[plugin]\nid = \"myuri-graphics\"\nversion = \"0.1.0\"\n[dependencies]\nmyuri = \"^0.1\"\nblender = { version = \"^0.3\", optional = true }"),
            plugin("[plugin]\nid = \"old-addon\"\n[dependencies]\nmyuri = \"^0.2\""),
            plugin("[plugin]\nid = \"lonely\"\n[dependencies]\nmissing = \"^1\""),
            plugin("[plugin]\nid = \"future\"\napi = \"^2.0\""),
        ];
        let report = check(&plugins, "holotype", |_| true);
        assert_eq!(report.order[..3], ["myuri", "blender", "myuri-graphics"], "зависимости — раньше");
        assert!(report.works("myuri-graphics"));
        assert_eq!(report.problems("old-addon"), [Problem::Version { dep: "myuri".into(), need: "^0.2".into(), found: "0.1.2".into() }]);
        assert_eq!(report.problems("lonely"), [Problem::Missing { dep: "missing".into(), need: "^1".into() }]);
        assert_eq!(report.problems("future"), [Problem::Api("^2.0".into())]);

        // В Instar «Мост к Blender» не работает — но дополнение Мьюри работает: зависимость
        // необязательная.
        let instar = check(&plugins, "instar", |_| true);
        assert!(!instar.works("blender") && instar.works("myuri-graphics"));

        // Выключили Мьюри — дополнение к ней тоже не работает.
        let off = check(&plugins, "holotype", |id| id != "myuri");
        assert!(off.problems("myuri-graphics").contains(&Problem::Disabled("myuri".into())));
        assert!(!off.works("myuri-graphics"));
    }

    #[test]
    fn cycles_and_duplicates() {
        let plugins = vec![
            plugin("[plugin]\nid = \"a\"\nversion = \"1.0.0\"\n[dependencies]\nb = \"^1\""),
            plugin("[plugin]\nid = \"b\"\nversion = \"1.0.0\"\n[dependencies]\na = \"^1\""),
            plugin("[plugin]\nid = \"c\"\nversion = \"1.0.0\""),
            plugin("[plugin]\nid = \"c\"\nversion = \"1.0.1\""),
        ];
        let report = check(&plugins, "holotype", |_| true);
        assert!(matches!(report.problems("a").first(), Some(Problem::Cycle(c)) if c == &["a", "b", "a"]));
        assert!(!report.works("a") && !report.works("b"));
        assert!(report.problems("c").contains(&Problem::Duplicate));
        assert!(fits("^0.1", "0.1.5") && !fits("^0.1", "0.2.0") && fits(">=1", "1.0"));
    }
}
