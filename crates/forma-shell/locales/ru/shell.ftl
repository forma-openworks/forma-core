### Оболочка программ Forma: шапка окна, меню, рабочие пространства, строка состояния,
### экран приветствия, диалоги и уведомления. Программа добавляет свои строки:
### `app-tagline` (экран приветствия) и `app-description` (окно «О программе»).

## Меню

menu-file = Файл
menu-view = Вид
menu-window = Окно
menu-help = Справка
menu-compact = Меню
menu-open-editor = Открыть редактор
menu-new-workspace = Новое рабочее пространство
menu-manage-settings = Управление настройками
window-minimize = Свернуть
window-maximize = Развернуть
window-restore = Свернуть в окно
window-close = Закрыть

## Действия

action-settings = Настройки…
action-export-settings = Экспорт настроек…
action-import-settings = Импорт настроек…
action-quit = Выход
action-about = О программе
action-find-action = Найти действие…
action-zoom-in = Увеличить масштаб
action-zoom-out = Уменьшить масштаб
action-zoom-reset = Сбросить масштаб
action-quick-theme = Быстрая смена темы…
action-return-windows = Вернуть все отдельные окна
action-reset-workspace = Сбросить раскладку пространства
action-reset-all-workspaces = Сбросить все пространства
action-done = Готово
action-close = Закрыть
action-rename = Переименовать
action-cancel = Отмена

action-group-file = Файл
action-group-view = Вид
action-group-window = Окно
action-group-help = Справка

## Рабочие пространства

workspace-untitled = Пространство
workspace-copy-name = { $name } (копия)
workspace-new-copy = Копия текущего
workspace-rename = Переименовать…
workspace-rename-title = Название пространства
workspace-duplicate = Дублировать
workspace-delete = Удалить
workspace-more = { $count ->
    [one] Ещё { $count } рабочее пространство
    [few] Ещё { $count } рабочих пространства
   *[many] Ещё { $count } рабочих пространств
}
layout-save-default = Сохранить раскладку как стандартную
layout-save-default-hint = Эту раскладку получат все, у кого ещё нет своей
layout-apply-default = Применить стандартную раскладку
layout-reset-default = Сбросить стандартную раскладку к заводской
toast-default-layout-saved = Раскладка сохранена как стандартная
toast-default-layout-failed = Не удалось сохранить стандартную раскладку
toast-default-layout-applied = Применена стандартная раскладка
toast-default-layout-reset = Стандартная раскладка сброшена к заводской

## Строка состояния и «О программе»

status-renderer = Рендер
status-detached-windows = { $count ->
    [one] { $count } отдельное окно
    [few] { $count } отдельных окна
   *[many] { $count } отдельных окон
}
about-version = Версия { $version }
about-forma = Часть проекта FORMA

## Плагины и темы

plugin-vs-dark-name = Visual Studio Dark
plugin-vs-dark-about = Тёмная тема по мотивам Visual Studio 2022: синяя строка состояния и привычные цвета кода.
dialog-install-plugin = Установить плагин из файла
toast-plugin-installed = Плагин установлен: { $name }
toast-plugin-failed = Ошибка плагина: { $error }
toast-plugin-install-failed = Не удалось установить { $name }: { $error }
theme-new-name = Моя тема
toast-theme-created = Тема создана: { $path }
toast-theme-create-failed = Не удалось создать тему: { $error }

## Запуск

shell-error-vulkan = Не удалось запустить { $app } через Vulkan.

    Проверьте, что драйвер видеокарты поддерживает Vulkan.

    Подробности: { $details }
