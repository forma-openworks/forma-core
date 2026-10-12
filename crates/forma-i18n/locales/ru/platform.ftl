# Самоназвание языка — показывается в настройках.
language-name = Русский

### Платформа Forma: окно настроек, внешний вид, раскладка клавиш,
### «Найти действие», экспорт и импорт настроек.

settings-section-platform = Платформа Forma
settings-search-hint = Поиск настроек
settings-nothing-found = Ничего не найдено
settings-import = Импорт…
settings-export = Экспорт…
settings-ok = ОК
settings-apply = Применить
settings-cancel = Отмена
settings-reset = Сбросить к стандартному

settings-page-appearance = Внешний вид
settings-page-keymap = Раскладка клавиш
settings-group-language = Язык
settings-group-look = Оформление
settings-language = Язык интерфейса
settings-language-system = Как в системе
settings-theme = Тема
theme-dark = Тёмная
theme-light = Светлая
theme-default = По умолчанию
settings-zoom = Масштаб интерфейса

keymap-search-hint = Поиск по названию или сочетанию
keymap-reset-all = Сбросить все изменения
keymap-add = Добавить сочетание
keymap-remove = Убрать { $shortcut }
keymap-reset = Вернуть стандартные сочетания
keymap-press = Нажмите сочетание клавиш…
keymap-conflict = Уже назначено: { $actions }
keymap-conflict-remove = Добавить и убрать у других
keymap-add-confirm = Добавить
keymap-none = нет сочетания

find-action-hint = Найти действие или настройку…
find-action-settings = Настройки

settings-export-title = Экспорт настроек
settings-export-hint = Отметьте, что положить в архив. Его можно импортировать в { $app } на другом компьютере или в другую программу Forma: она применит то, что поддерживает.
settings-export-button = Экспортировать…
dialog-export-settings = Куда сохранить настройки
toast-settings-exported = Настройки сохранены в { $path }
toast-settings-export-failed = Не удалось сохранить настройки: { $error }

settings-import-title = Импорт настроек
settings-import-from = Архив из { $app } { $version } от { $date }
settings-import-apply = Применится
settings-import-partial = Применится частично. Пропустится: { $items }
settings-import-applied = Применено
settings-import-applied-partial = Применено частично. Пропущено: { $items }
settings-import-unsupported = Не поддерживается этой программой
settings-import-invalid = Испорчено: { $error }
settings-import-missing = в архиве нет файла
settings-import-button = Импортировать
settings-import-done-title = Настройки импортированы
dialog-import-settings = Архив настроек
toast-settings-import-failed = Не удалось прочитать архив настроек: { $error }

settings-background = Фоновая картинка
settings-background-image = Картинка
settings-background-from-theme = из темы
settings-background-choose = Выбрать…
settings-background-opacity = Непрозрачность
settings-background-fit = Заполнение
fit-cover = Закрыть всё окно
fit-contain = Вписать целиком
fit-stretch = Растянуть
fit-tile = Плиткой
fit-center = По центру

settings-page-fonts = Шрифты
fonts-ui = Интерфейс
fonts-editor = Редактор кода
fonts-console = Консоль
fonts-family = Шрифт
fonts-size = Размер
fonts-line-height = Высота строки
fonts-default = По умолчанию
fonts-loading = загружаю список шрифтов…
fonts-custom = Использовать свой шрифт вместо { $font }
fonts-ctrl-wheel = Ctrl+колёсико меняет размер шрифта в редакторе и консоли
fonts-ui-size = Текст интерфейса

theme-switcher-title = Тема
theme-switcher-hint = Наведение или ↑↓ — посмотреть, Enter — оставить, Esc — вернуть прежнюю.

settings-page-plugins = Плагины
plugins-recommended = Рекомендуемое
plugins-installed = Установленные
plugins-install = Установить
plugins-install-file = Установить из файла…
plugins-install-file-hint = .zip с plugin.toml внутри — например, набор тем
plugins-open-folder = Открыть папку плагинов
plugins-none-installed = Плагинов пока нет
plugins-is-installed = Установлен
plugins-coming = Появится позже
plugins-homepage = Сайт
plugins-failed = Не удалось установить: { $error }
plugins-downloading-of = { $done } из { $total } МБ
plugins-downloading = { $done } МБ
plugins-extracting = Распаковка…
plugins-kind-tool = Программа
plugins-kind-themes = Темы
plugins-kind-script = Скрипт
plugins-version = версия { $version }
plugins-uninstall = Удалить
plugins-enabled = Включён
plugins-welcome-title = Рекомендуемые плагины
unit-mb = МБ
unit-kb = КБ
settings-import-plugins-missing = На этом компьютере не установлены: { $items }
settings-import-open-plugins = Открыть «Плагины»

settings-page-theme-tokens = Инспектор темы
theme-tokens-colors = Цвета
theme-tokens-icons = Значки
theme-tokens-syntax = Подсветка кода
theme-tokens-hint = Ключи темы «{ $theme }» и их текущие значения. Щелчок по строке копирует её для файла темы.
theme-tokens-icons-hint = Тема может заменить любой значок своим SVG и перекрасить его ключом «icon.<имя>». Щелчок по значку копирует строку.
theme-tokens-icon-hover = { $name } · Lucide: { $lucide }
theme-tokens-from-theme = задан темой
theme-tokens-default = по умолчанию
theme-tokens-copy = Щелчок копирует строку для файла темы
settings-theme-create = Создать тему…
settings-theme-create-hint = Новый файл темы на основе выбранной: все ключи перечислены и закомментированы
settings-theme-open-folder = Открыть папку тем

## Компоненты настроек (экспорт и импорт)
settings-component-appearance = Язык, тема, масштаб и фоновая картинка
settings-component-keymap = Ваши изменения сочетаний клавиш
settings-component-fonts = Шрифты интерфейса, редактора и консоли
settings-component-plugins = Какие плагины установлены и какие выключены
