### The shell of Forma apps: window caption, menus, workspaces, status bar, welcome
### screen, dialogs and notifications. Each app adds its own strings:
### `app-tagline` (welcome screen) and `app-description` (About window).

## Menus

menu-file = File
menu-view = View
menu-window = Window
menu-help = Help
menu-compact = Menu
menu-open-editor = Open editor
menu-new-workspace = New workspace
menu-manage-settings = Manage settings
window-minimize = Minimize
window-maximize = Maximize
window-restore = Restore Down
window-close = Close

## Actions

action-settings = Settings…
action-export-settings = Export settings…
action-import-settings = Import settings…
action-quit = Quit
action-about = About
action-find-action = Find action…
action-zoom-in = Zoom in
action-zoom-out = Zoom out
action-zoom-reset = Reset zoom
action-quick-theme = Quick switch theme…
action-return-windows = Return all detached windows
action-reset-workspace = Reset workspace layout
action-reset-all-workspaces = Reset all workspaces
action-done = Done
action-close = Close
action-rename = Rename
action-cancel = Cancel

action-group-file = File
action-group-view = View
action-group-window = Window
action-group-help = Help

## Workspaces

workspace-untitled = Workspace
workspace-copy-name = { $name } (copy)
workspace-new-copy = Copy of current
workspace-rename = Rename…
workspace-rename-title = Workspace name
workspace-duplicate = Duplicate
workspace-delete = Delete
workspace-more = { $count ->
    [one] { $count } more workspace
   *[other] { $count } more workspaces
}
layout-save-default = Save layout as default
layout-save-default-hint = Everyone without a layout of their own gets this one
layout-apply-default = Apply default layout
layout-reset-default = Reset default layout to factory
toast-default-layout-saved = Layout saved as default
toast-default-layout-failed = Couldn't save the default layout
toast-default-layout-applied = Default layout applied
toast-default-layout-reset = Default layout reset to factory

## Status bar and About

status-renderer = Renderer
status-detached-windows = { $count ->
    [one] { $count } detached window
   *[other] { $count } detached windows
}
about-version = Version { $version }
about-forma = Part of the FORMA project

## Plugins and themes

plugin-vs-dark-name = Visual Studio Dark
plugin-vs-dark-about = A dark theme inspired by Visual Studio 2022: a blue status bar and familiar code colors.
dialog-install-plugin = Install a plugin from disk
toast-plugin-installed = Plugin installed: { $name }
toast-plugin-failed = Plugin error: { $error }
toast-plugin-install-failed = Couldn't install { $name }: { $error }
theme-new-name = My theme
toast-theme-created = Theme created: { $path }
toast-theme-create-failed = Couldn't create the theme: { $error }

## Startup

shell-error-vulkan = Couldn't start { $app } with Vulkan.

    Make sure your graphics driver supports Vulkan.

    Details: { $details }
