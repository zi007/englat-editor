//! The system region new documents follow (#838): the paper File › New starts on, the default
//! measurement unit and the decimal separator.
//!
//! - Windows: the preferred language with the home location from Settings › Time & language ›
//!   Region (`GlobalizationPreferences.HomeGeographicRegion`, a safe Windows Runtime API), since
//!   the language list alone says `en-US` for an English interface in Poland.
//! - Elsewhere: the locale the UI crate reads ([`englat_ui_egui::i18n::system_region`]).

/// The system region.
pub fn system() -> englat_engine::Region {
    // Runs without preferences (agents' and screenshot runs) behave the same on every machine:
    // en-US unless `ENGLAT_EDITOR_REGION` says otherwise.
    if std::env::var_os("ENGLAT_EDITOR_NO_PREFS").is_some() && std::env::var_os("ENGLAT_EDITOR_REGION").is_none() {
        return englat_engine::Region::default();
    }
    let region = englat_ui_egui::i18n::system_region();
    #[cfg(target_os = "windows")]
    {
        if std::env::var_os("ENGLAT_EDITOR_REGION").is_none() {
            return match windows::System::UserProfile::GlobalizationPreferences::HomeGeographicRegion() {
                Ok(home) => region.with_territory(&home.to_string()),
                Err(e) => {
                    log::debug!("system home region: {e}");
                    region
                }
            };
        }
    }
    region
}
