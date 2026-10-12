//! Вшивает строки самой платформы из `locales/<язык>/*.ftl` этого крейта. Строки программ
//! лежат у программ и передаются в `init_with`.

fn main() {
    forma_build::locales("locales", "PLATFORM");
}
