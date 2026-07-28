//! Well-known identifiers the Riot Client uses.
//!
//! **Reference values only - nothing in this crate branches on them.** They are
//! here so a caller can write `ids::products::LEAGUE_OF_LEGENDS` instead of a
//! bare string literal.
//!
//! They live at the crate root rather than inside a namespace module because no
//! namespace owns them: a product id is an argument to `product-launcher`,
//! `rnet-product-registry`, `product-session` and `patch` alike.
//!
//! One submodule per kind of identifier - a patchline is a release of a product,
//! while a secondary patchline is a nested install *under* a patchline, and
//! passing one where the other belongs would otherwise typecheck.
//!
//! # Refreshing these
//!
//! Captured from a live client (EUW, 2026-07-28) with
//! `cargo run --example probe`, which dumps both sources:
//!
//! | Route | Enumerates |
//! | --- | --- |
//! | `GET /product-metadata/v1/definitions/products` | entitled products and their patchlines |
//! | `GET /data-store/v1/product-settings/products/{p}/patchlines/{pl}` | `locale_data.available_locales` |
//!
//! These lists are a spelling aid, not a source of truth. Entitlement varies per
//! account, installed-ness per machine, and available locales per patchline, so
//! anything that has to be correct - a picker, a validation - calls the routes
//! above instead.

/// Product ids, with the display name the client reports for each.
pub mod products {
    /// League of Legends.
    pub const LEAGUE_OF_LEGENDS: &str = "league_of_legends";

    /// Reports its name as "Teamfight Tactics", and is a distinct product from
    /// [`TEAMFIGHTTACTICS`].
    pub const LEAGUE_OF_LEGENDS_GAME: &str = "league_of_legends_game";

    /// Teamfight Tactics.
    pub const TEAMFIGHTTACTICS: &str = "teamfighttactics";

    /// VALORANT.
    pub const VALORANT: &str = "valorant";

    /// Legends of Runeterra.
    pub const BACON: &str = "bacon";

    /// 2XKO.
    pub const LION: &str = "lion";

    /// League of Legends: Wild Rift.
    pub const WILDRIFT: &str = "wildrift";

    /// Riot Mobile.
    pub const RITOPLUS: &str = "ritoplus";

    /// Arcane.
    pub const ARCANE: &str = "arcane";

    /// The Riot Client, which is a product in the registry like any other.
    pub const RIOT_CLIENT: &str = "riot_client";

    pub const ALL: &[&str] = &[
        ARCANE,
        BACON,
        LEAGUE_OF_LEGENDS,
        LEAGUE_OF_LEGENDS_GAME,
        LION,
        RIOT_CLIENT,
        RITOPLUS,
        TEAMFIGHTTACTICS,
        VALORANT,
        WILDRIFT,
    ];
}

/// Patchline ids - the releases a product declares.
///
/// Which ones exist is per product; most declare only [`patchlines::LIVE`].
/// `Product::patchlines` is the answer for a given install.
pub mod patchlines {
    /// A product's live release.
    pub const LIVE: &str = "live";

    /// The public beta environment.
    ///
    /// Declared whether or not it is installed, which is why entitlement and
    /// installed-ness are different questions - see
    /// [`Patchline::is_installed`](crate::models::product_registry::Patchline::is_installed).
    pub const PBE: &str = "pbe";

    /// A promotional PC patchline.
    pub const PCPROMOLIVE: &str = "pcpromolive";

    pub const ALL: &[&str] = &[LIVE, PBE, PCPROMOLIVE];
}

/// Secondary patchline ids - the nested installs a patchline declares.
pub mod secondary_patchlines {
    /// Holds League's game data - the `Game` subdirectory. Other products
    /// declare their own, which is why the relative path is read rather than
    /// assumed; see
    /// [`Patchline::secondary_dir`](crate::models::product_registry::Patchline::secondary_dir).
    pub const GAME_PATCH: &str = "game_patch";
}

/// Locale ids, spelled `xx_YY` as the client spells them.
///
/// The union of every `locale_data.available_locales` observed. Availability is
/// per product and per patchline - League offers all of these, other products
/// fewer.
pub mod locales {
    pub const AR_AE: &str = "ar_AE";
    pub const CS_CZ: &str = "cs_CZ";
    pub const DE_DE: &str = "de_DE";
    pub const EL_GR: &str = "el_GR";
    pub const EN_AU: &str = "en_AU";
    pub const EN_GB: &str = "en_GB";
    pub const EN_PH: &str = "en_PH";
    pub const EN_SG: &str = "en_SG";
    pub const EN_US: &str = "en_US";
    pub const ES_AR: &str = "es_AR";
    pub const ES_ES: &str = "es_ES";
    pub const ES_MX: &str = "es_MX";
    pub const FR_FR: &str = "fr_FR";
    pub const HU_HU: &str = "hu_HU";
    pub const ID_ID: &str = "id_ID";
    pub const IT_IT: &str = "it_IT";
    pub const JA_JP: &str = "ja_JP";
    pub const KO_KR: &str = "ko_KR";
    pub const PL_PL: &str = "pl_PL";
    pub const PT_BR: &str = "pt_BR";
    pub const RO_RO: &str = "ro_RO";
    pub const RU_RU: &str = "ru_RU";
    pub const TH_TH: &str = "th_TH";
    pub const TR_TR: &str = "tr_TR";
    pub const VI_VN: &str = "vi_VN";
    pub const ZH_CN: &str = "zh_CN";
    pub const ZH_MY: &str = "zh_MY";
    pub const ZH_TW: &str = "zh_TW";

    pub const ALL: &[&str] = &[
        AR_AE, CS_CZ, DE_DE, EL_GR, EN_AU, EN_GB, EN_PH, EN_SG, EN_US, ES_AR, ES_ES, ES_MX, FR_FR,
        HU_HU, ID_ID, IT_IT, JA_JP, KO_KR, PL_PL, PT_BR, RO_RO, RU_RU, TH_TH, TR_TR, VI_VN, ZH_CN,
        ZH_MY, ZH_TW,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_locale_is_well_formed() {
        for locale in locales::ALL {
            let (language, region) = locale.split_once('_').expect("locale must be xx_YY");
            assert_eq!(language.len(), 2, "{locale}");
            assert_eq!(region.len(), 2, "{locale}");
            assert!(language.chars().all(|c| c.is_ascii_lowercase()), "{locale}");
            assert!(region.chars().all(|c| c.is_ascii_uppercase()), "{locale}");
        }
    }

    /// The `ALL` slices are maintained by hand beside the constants, so they
    /// drift the moment someone adds one and forgets the other. The counts are
    /// the tripwire: a deliberate addition updates both and this test with it.
    #[test]
    fn the_all_lists_are_complete_and_unique() {
        for (label, list, expected) in [
            ("products", products::ALL, 10),
            ("patchlines", patchlines::ALL, 3),
            ("locales", locales::ALL, 28),
        ] {
            assert_eq!(list.len(), expected, "{label} changed size");

            let mut sorted = list.to_vec();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), list.len(), "duplicate in {label}");
        }
    }

    /// A hyphen would need percent-encoding nowhere, but it would break the
    /// snake_case assumption callers make when deriving install ids.
    #[test]
    fn product_ids_are_snake_case() {
        for product in products::ALL {
            assert!(
                product
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "{product}"
            );
        }
    }
}
