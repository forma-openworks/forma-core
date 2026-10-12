//! Вшивает строки оболочки из `locales/<язык>/*.ftl`.

fn main() {
    forma_build::locales("locales", "LOCALES");
}
