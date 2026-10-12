# The language's own name — shown in the settings.
language-name = English

### Платформа Forma: окно настроек, внешний вид, раскладка клавиш,
### «Найти действие», экспорт и импорт настроек.

settings-section-platform = Forma platform
settings-search-hint = Search settings
settings-nothing-found = Nothing found
settings-import = Import…
settings-export = Export…
settings-ok = OK
settings-apply = Apply
settings-cancel = Cancel
settings-reset = Reset to default

settings-page-appearance = Appearance
settings-page-keymap = Keymap
settings-group-language = Language
settings-group-look = Look
settings-language = Interface language
settings-language-system = System default
settings-theme = Theme
theme-dark = Dark
theme-light = Light
theme-default = Default
settings-zoom = Interface zoom

keymap-search-hint = Search by name or shortcut
keymap-reset-all = Reset all changes
keymap-add = Add a shortcut
keymap-remove = Remove { $shortcut }
keymap-reset = Restore the default shortcuts
keymap-press = Press a key combination…
keymap-conflict = Already assigned to: { $actions }
keymap-conflict-remove = Add and remove from others
keymap-add-confirm = Add
keymap-none = no shortcut

find-action-hint = Find an action or a setting…
find-action-settings = Settings

settings-export-title = Export settings
settings-export-hint = Choose what goes into the archive. You can import it into { $app } on another computer or into another Forma app: it applies whatever it supports.
settings-export-button = Export…
dialog-export-settings = Save settings as
toast-settings-exported = Settings saved to { $path }
toast-settings-export-failed = Couldn't save the settings: { $error }

settings-import-title = Import settings
settings-import-from = Archive from { $app } { $version }, { $date }
settings-import-apply = Will be applied
settings-import-partial = Will be applied in part. Skipped: { $items }
settings-import-applied = Applied
settings-import-applied-partial = Applied in part. Skipped: { $items }
settings-import-unsupported = Not supported by this app
settings-import-invalid = Damaged: { $error }
settings-import-missing = the file is missing from the archive
settings-import-button = Import
settings-import-done-title = Settings imported
dialog-import-settings = Settings archive
toast-settings-import-failed = Couldn't read the settings archive: { $error }

settings-background = Background
settings-background-image = Image
settings-background-from-theme = from the theme
settings-background-choose = Choose…
settings-background-opacity = Opacity
settings-background-fit = Fit
fit-cover = Cover the window
fit-contain = Fit inside
fit-stretch = Stretch
fit-tile = Tile
fit-center = Center

settings-page-fonts = Fonts
fonts-ui = Interface
fonts-editor = Code editor
fonts-console = Console
fonts-family = Font
fonts-size = Size
fonts-line-height = Line height
fonts-default = Default
fonts-loading = loading the font list…
fonts-custom = Use my own font instead of { $font }
fonts-ctrl-wheel = Ctrl+mouse wheel changes the font size in the editor and console
fonts-ui-size = Interface text

theme-switcher-title = Theme
theme-switcher-hint = Hover or use ↑↓ to preview. Enter keeps the theme, Esc brings the old one back.

settings-page-plugins = Plugins
plugins-recommended = Recommended
plugins-installed = Installed
plugins-install = Install
plugins-install-file = Install from disk…
plugins-install-file-hint = A .zip with plugin.toml inside, for example a theme pack
plugins-open-folder = Open plugins folder
plugins-none-installed = No plugins installed yet
plugins-is-installed = Installed
plugins-coming = Coming later
plugins-homepage = Homepage
plugins-failed = Installation failed: { $error }
plugins-downloading-of = { $done } of { $total } MB
plugins-downloading = { $done } MB
plugins-extracting = Unpacking…
plugins-kind-tool = Tool
plugins-kind-themes = Themes
plugins-kind-script = Script
plugins-version = version { $version }
plugins-uninstall = Uninstall
plugins-enabled = Enabled
plugins-welcome-title = Recommended plugins
unit-mb = MB
unit-kb = KB
settings-import-plugins-missing = Not installed on this computer: { $items }
settings-import-open-plugins = Open Plugins

settings-page-theme-tokens = Theme inspector
theme-tokens-colors = Colors
theme-tokens-icons = Icons
theme-tokens-syntax = Code highlighting
theme-tokens-hint = Keys of the theme “{ $theme }” with their current values. Click a row to copy the line for a theme file.
theme-tokens-icons-hint = A theme can replace any icon with its own SVG and recolor it with an "icon.<name>" key. Click an icon to copy its line.
theme-tokens-icon-hover = { $name } · Lucide: { $lucide }
theme-tokens-from-theme = set by the theme
theme-tokens-default = default
theme-tokens-copy = Click to copy the line for a theme file
settings-theme-create = Create theme…
settings-theme-create-hint = A new theme file based on the selected theme, with every key listed and commented out
settings-theme-open-folder = Open themes folder

## Settings components (export and import)
settings-component-appearance = Language, theme, zoom and background image
settings-component-keymap = Your changes to the shortcuts
settings-component-fonts = Interface, editor and console fonts
settings-component-plugins = Which plugins are installed and which are turned off
