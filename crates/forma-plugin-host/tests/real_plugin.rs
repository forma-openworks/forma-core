//! Проверка с настоящим плагином: `FORMA_TEST_PLUGIN=<папка собранного плагина «terms»>`
//! (`cargo xtask build terms` в forma-plugins кладёт её в `target/fpl/terms`). Без
//! переменной проверка пропускается.

use std::time::Duration;

use forma_plugin_host::Runtime;
use forma_plugins::{Installed, check, read_manifest};

#[test]
fn terms_plugin_reads_the_glossary_and_answers_hovers() {
    let Some(dir) = std::env::var_os("FORMA_TEST_PLUGIN").map(std::path::PathBuf::from) else {
        eprintln!("FORMA_TEST_PLUGIN не задан — пропускаю");
        return;
    };
    let vault = std::env::temp_dir().join(format!("forma-host-vault-{}", std::process::id()));
    std::fs::create_dir_all(vault.join(".model-kit")).unwrap();
    std::fs::write(vault.join("Глоссарий.md"), "- **edge loop** — кольцо рёбер вокруг сетки. → [[2.3 Петли#Edge loop]]\n- **UV** — развёртка поверхности на плоскость. *preview* → [[1.1 Знакомство#UV]]\n").unwrap();
    std::fs::write(vault.join(".model-kit").join("terms.md"), "| Term | Статус | Глава | Зависит от | Подробно — |\n|---|---|---|---|---|\n| UV | preview | 1.1 | — | Stage 6 |\n").unwrap();

    let plugin = Installed { manifest: read_manifest(&dir).unwrap(), dir: dir.clone(), dev: true };
    let plugins = vec![plugin];
    let report = check(&plugins, "holotype", |_| true);
    assert!(report.works("terms"), "{:?}", report.problems("terms"));

    let mut rt = Runtime::new("holotype", Some(vault.join(".data"))).unwrap();
    rt.set_language("ru");
    rt.set_root(Some(&vault));
    rt.sync(&plugins, &report, false);
    assert!(rt.wait_compiled(Duration::from_secs(60)), "модуль не скомпилировался");
    assert!(rt.has_hovers());

    let text = "Добавь edge loop у крыла, а UV разберём потом.";
    let at = |w: &str| text[..text.find(w).unwrap()].chars().count();
    let hover = rt.hover(text, at("loop"), "1.3 Режим правки.md").expect("подсказка про edge loop");
    assert_eq!(hover.range, at("edge")..at("edge") + "edge loop".chars().count());
    assert!(hover.markdown.contains("кольцо рёбер") && hover.markdown.contains("2.3 Петли › Edge loop"), "{}", hover.markdown);
    let uv = rt.hover(text, at("UV"), "1.3 Режим правки.md").expect("подсказка про UV");
    assert!(uv.markdown.contains("preview") && uv.markdown.contains("Stage 6"), "{}", uv.markdown);
    assert!(rt.hover(text, at("крыла"), "x.md").is_none(), "не term — нет подсказки");
    assert!(rt.hover(text, at("loop"), "Глоссарий.md").is_none(), "на самом глоссарии подсказки не нужны");
    assert!(rt.failure("terms").is_none());
    let _ = std::fs::remove_dir_all(&vault);
}
