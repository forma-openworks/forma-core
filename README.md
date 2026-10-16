<h1 align="center">Forma Core</h1>

<p align="center">
  <b>The shared core of FORMA desktop apps.</b><br>
  Different apps, one base.
</p>

<p align="center">
  <img alt="Status: early development" src="https://img.shields.io/badge/status-early%20development-8a6cf0">
  <img alt="Rust 2024" src="https://img.shields.io/badge/Rust-2024-f46623?logo=rust&logoColor=white">
  <img alt="Rendering: wgpu + Vulkan" src="https://img.shields.io/badge/render-wgpu%20%2B%20Vulkan-a41e22">
  <img alt="Languages: English, Russian" src="https://img.shields.io/badge/UI-English%20%7C%20%D0%A0%D1%83%D1%81%D1%81%D0%BA%D0%B8%D0%B9-4b8bbe">
</p>

<p align="center">
  <a href="#english">English</a> · <a href="#русский">Русский</a>
</p>

---

## English

### What it is

Forma Core is the base that FORMA desktop apps are built on. It gives an app its window with its own frame, a layout of areas in the spirit of Blender (one editor per area, an editor-type menu, split and join by dragging area corners), workspaces, themes and fonts, settings in the spirit of JetBrains IDEs, a keymap and Find Action, plugins and translations. An app brings only its own work, and every improvement to the core reaches all apps at once.

Apps built on Forma Core:

- [Forma Mycelium](https://github.com/forma-openworks/forma-mycelium), an IDE made for Minecraft modding;
- Forma: Holotype, notes, study notes and documentation in the spirit of Obsidian;
- Forma: Instar, a full-featured IDE (being designed).

### Crates

| Crate | What it does |
|---|---|
| `forma-shell` | The app shell: window frame, menus, workspaces, status bar, welcome screen, settings, Find Action, plugins and themes, plugin panels and command contexts shared by all apps (`plugin_panels`). An app describes itself with a `Spec` and starts with `forma_shell::run` |
| `forma-layout` | The composable UI: area tree with one editor per area, editor-type menu, corner gestures, separate OS windows |
| `forma-ui` | The theme engine, icons, fonts, widgets, the settings window and its pages, theme tools. Ships the Mycelium Dark and Light themes (`forma_ui::theme::MYCELIUM_THEMES`) |
| `forma-actions` | Actions and the keymap |
| `forma-settings` | Settings components, their files, export and import; the plugin settings schema and the user `settings.json` (JSON with comments, edited by the settings window and by hand alike, like VS Code) |
| `forma-plugins` | Plugins: the `.fpl` format and `forma-plugin.toml` manifest, dependency checks like a mod loader, installing, removing, turning on and off; built-in plugins an app ships with |
| `forma-plugin-host` | Runs plugin code: WebAssembly components in a wasmtime sandbox, API `forma:plugin@1.2.0` described in WIT; background programs a plugin starts live in a Windows Job Object and die with it |
| `forma-plugin-api` | The SDK for plugin authors: implement `Plugin`, call `register_plugin!`, build for `wasm32-wasip2` |
| `forma-plugin-ui` | The element tree a plugin describes its UI with (blocks, panels), like `UILayout` in Blender add-ons, plus a canvas of shapes, a before/after compare and a 3D model view; the app draws it with `forma_ui::elements` |
| `forma-i18n` | Localization with Fluent, the `t!` macro, translation completeness checks; the platform's own strings |
| `forma-build` | Build-script helpers: embedding `.ftl` translations, the Windows icon and version info |

### Using it in an app

Add the crates an app needs as git dependencies:

```toml
[dependencies]
forma-shell = { git = "https://github.com/forma-openworks/forma-core", branch = "main" }
forma-ui = { git = "https://github.com/forma-openworks/forma-core", branch = "main" }

[build-dependencies]
forma-build = { git = "https://github.com/forma-openworks/forma-core", branch = "main" }
```

`Cargo.lock` pins the exact commit. `cargo update -p forma-shell` moves the app to the latest one.

To change the core and an app together, point Cargo at a local copy instead of GitHub. Put this in a `.cargo/config.toml` above both repositories:

```toml
[patch."https://github.com/forma-openworks/forma-core"]
forma-shell = { path = "FormaCore/crates/forma-shell" }
forma-ui = { path = "FormaCore/crates/forma-ui" }
```

### Building

You need stable Rust (edition 2024) and a GPU driver with Vulkan support. Development and testing currently happen on Windows.

```bash
cargo test
```

### About

Forma Core is part of FORMA. Idea and design: Holo. The code is written entirely by Claude (Anthropic). License: MIT.

---

## Русский

### Что это

Forma Core — ядро, на котором построены программы FORMA. Она даёт программе окно со своей рамкой, раскладку из областей в духе Blender (в каждой области один редактор, меню вида редактора, деление и объединение за уголки), рабочие пространства, темы и шрифты, настройки в духе JetBrains, раскладку клавиш и «Найти действие», плагины и переводы. Программа приносит только своё, а каждое улучшение ядра сразу доходит до всех программ.

Программы на Forma Core:

- [Forma Mycelium](https://github.com/forma-openworks/forma-mycelium) — среда разработки для модов Minecraft;
- Forma: Holotype — заметки, конспекты и документация в духе Obsidian;
- Forma: Instar — полноценная среда разработки (проектируется).

### Крейты

| Крейт | Что делает |
|---|---|
| `forma-shell` | Оболочка программы: рамка окна, меню, рабочие пространства, строка состояния, экран приветствия, настройки, «Найти действие», плагины и темы, общие для всех программ панели плагинов и контекст их команд (`plugin_panels`). Программа описывает себя в `Spec` и запускается через `forma_shell::run` |
| `forma-layout` | Составной интерфейс: дерево областей с одним редактором в каждой, меню вида редактора, жесты за уголки, отдельные окна ОС |
| `forma-ui` | Движок тем, значки, шрифты, виджеты, окно настроек и его страницы, инструменты для тем. Везёт темы Mycelium Dark и Light (`forma_ui::theme::MYCELIUM_THEMES`) |
| `forma-actions` | Действия и раскладка клавиш |
| `forma-settings` | Компоненты настроек, их файлы, экспорт и импорт; схема настроек плагинов и пользовательский `settings.json` (JSON с комментариями, его правят и окно настроек, и человек, как в VS Code) |
| `forma-plugins` | Плагины: формат `.fpl` и манифест `forma-plugin.toml`, проверка зависимостей, как у загрузчика модов, установка, удаление, включение и выключение; встроенные плагины программы |
| `forma-plugin-host` | Запуск code плагинов: компоненты WebAssembly в песочнице wasmtime, API `forma:plugin@1.2.0` на WIT; программы, запущенные плагином в фоне, живут в Windows Job Object и умирают вместе с ним |
| `forma-plugin-api` | SDK для авторов плагинов: реализовать `Plugin`, вызвать `register_plugin!`, собрать под `wasm32-wasip2` |
| `forma-plugin-ui` | Дерево элементов, которым плагин описывает интерфейс (блоки, панели), как `UILayout` у аддонов Blender, а ещё холст с фигурами, сравнение «до и после» и просмотр 3D-модели; рисует его программа через `forma_ui::elements` |
| `forma-i18n` | Локализация на Fluent, макрос `t!`, проверка полноты переводов; строки самой платформы |
| `forma-build` | Помощники сборочных скриптов: вшивание переводов `.ftl`, значка и сведений о версии для Windows |

### Как подключить

Нужные программе крейты подключаются как зависимости из git:

```toml
[dependencies]
forma-shell = { git = "https://github.com/forma-openworks/forma-core", branch = "main" }
forma-ui = { git = "https://github.com/forma-openworks/forma-core", branch = "main" }

[build-dependencies]
forma-build = { git = "https://github.com/forma-openworks/forma-core", branch = "main" }
```

Точный коммит записывается в `Cargo.lock`. `cargo update -p forma-shell` переводит программу на самый новый.

Чтобы менять ядро и программу вместе, Cargo можно направить на локальную копию вместо GitHub. Для этого положите `.cargo/config.toml` в папку над обоими репозиториями:

```toml
[patch."https://github.com/forma-openworks/forma-core"]
forma-shell = { path = "FormaCore/crates/forma-shell" }
forma-ui = { path = "FormaCore/crates/forma-ui" }
```

### Сборка

Нужны Rust (stable, edition 2024) и драйвер видеокарты с поддержкой Vulkan. Разработка и проверка пока идут на Windows.

```bash
cargo test
```

### О проекте

Forma Core — часть FORMA. Идея и дизайн: Holo. Код полностью написан Claude (Anthropic). Лицензия: MIT.
