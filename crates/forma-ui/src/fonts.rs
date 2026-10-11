//! Загрузка системных шрифтов. Если нужного шрифта нет, используются встроенные шрифты egui.

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

struct Candidate {
    path: &'static str,
    index: u32,
}

const fn c(path: &'static str) -> Candidate {
    Candidate { path, index: 0 }
}

fn load_first(candidates: &[Candidate]) -> Option<FontData> {
    candidates.iter().find_map(|cand| {
        let bytes = std::fs::read(cand.path).ok()?;
        let mut data = FontData::from_owned(bytes);
        data.index = cand.index;
        Some(data)
    })
}

pub fn install(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let default_prop = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let default_mono = defs.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();

    let mut add = |name: &str, candidates: &[Candidate]| -> bool {
        match load_first(candidates) {
            Some(data) => {
                defs.font_data.insert(name.to_owned(), Arc::new(data));
                true
            }
            None => false,
        }
    };

    let regular = add(
        "ui-regular",
        &[
            c("C:/Windows/Fonts/segoeui.ttf"),
            c("/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf"),
            c("/usr/share/fonts/noto/NotoSans-Regular.ttf"),
            c("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
        ],
    );
    let bold = add(
        "ui-bold",
        &[
            c("C:/Windows/Fonts/segoeuib.ttf"),
            c("/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf"),
            c("/usr/share/fonts/noto/NotoSans-Bold.ttf"),
            c("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"),
        ],
    );
    let semibold = add(
        "ui-semibold",
        &[
            c("C:/Windows/Fonts/seguisb.ttf"),
            c("/usr/share/fonts/truetype/noto/NotoSans-SemiBold.ttf"),
            c("/usr/share/fonts/noto/NotoSans-SemiBold.ttf"),
        ],
    );
    let italic = add(
        "ui-italic",
        &[
            c("C:/Windows/Fonts/segoeuii.ttf"),
            c("/usr/share/fonts/truetype/noto/NotoSans-Italic.ttf"),
            c("/usr/share/fonts/noto/NotoSans-Italic.ttf"),
            c("/usr/share/fonts/truetype/dejavu/DejaVuSans-Oblique.ttf"),
        ],
    );
    let bold_italic = add(
        "ui-bolditalic",
        &[
            c("C:/Windows/Fonts/segoeuiz.ttf"),
            c("/usr/share/fonts/truetype/noto/NotoSans-BoldItalic.ttf"),
            c("/usr/share/fonts/noto/NotoSans-BoldItalic.ttf"),
        ],
    );
    let mono = add(
        "ui-mono",
        &[
            c("C:/Windows/Fonts/CascadiaMono.ttf"),
            c("C:/Windows/Fonts/consola.ttf"),
            c("/usr/share/fonts/truetype/jetbrains-mono/JetBrainsMono-Regular.ttf"),
            c("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"),
        ],
    );
    let symbols = add(
        "ui-symbols",
        &[c("C:/Windows/Fonts/seguisym.ttf"), c("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")],
    );
    // Цепочка запасных шрифтов: символы и эмодзи из встроенных шрифтов egui.
    let mut fallback: Vec<String> = Vec::new();
    if symbols {
        fallback.push("ui-symbols".into());
    }
    fallback.extend(default_prop.iter().cloned());

    let chain = |first: &[(&str, bool)]| -> Vec<String> {
        let mut v: Vec<String> = first.iter().filter(|(_, ok)| *ok).map(|(n, _)| n.to_string()).collect();
        for f in &fallback {
            if !v.contains(f) {
                v.push(f.clone());
            }
        }
        v
    };

    let proportional = chain(&[("ui-regular", regular)]);
    defs.families.insert(FontFamily::Proportional, proportional);
    defs.families.insert(self::bold(), chain(&[("ui-bold", bold), ("ui-semibold", semibold), ("ui-regular", regular)]));
    defs.families.insert(
        self::semibold(),
        chain(&[("ui-semibold", semibold), ("ui-bold", bold), ("ui-regular", regular)]),
    );
    defs.families.insert(FontFamily::Name("italic".into()), chain(&[("ui-italic", italic), ("ui-regular", regular)]));
    defs.families.insert(
        FontFamily::Name("bolditalic".into()),
        chain(&[("ui-bolditalic", bold_italic), ("ui-bold", bold), ("ui-regular", regular)]),
    );
    let mut monospace: Vec<String> = Vec::new();
    if mono {
        monospace.push("ui-mono".into());
    }
    monospace.extend(default_mono.iter().cloned());
    if regular {
        monospace.push("ui-regular".into());
    }
    if symbols {
        monospace.push("ui-symbols".into());
    }
    defs.families.insert(FontFamily::Monospace, monospace);

    ctx.set_fonts(defs);
}

