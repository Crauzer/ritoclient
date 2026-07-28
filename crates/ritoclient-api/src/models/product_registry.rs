//! Data types for `/rnet-product-registry`.
//!
//! The re-exports below are the public names; the definitions live in
//! [`super::flat`]. Everything hand-written about these types is here.

use std::path::PathBuf;

pub use super::flat::{
    RnetProductRegistryPatchline as Patchline, RnetProductRegistryProduct as Product,
    RnetProductRegistrySecondaryPatchline as SecondaryPatchline,
};

impl Product {
    pub fn patchline(&self, id: &str) -> Option<&Patchline> {
        self.patchlines.iter().find(|p| p.id == id)
    }

    /// Only the patchlines that are actually on disk.
    pub fn installed_patchlines(&self) -> impl Iterator<Item = &Patchline> {
        self.patchlines.iter().filter(|p| p.is_installed())
    }
}

impl Patchline {
    /// Whether this patchline is on disk.
    ///
    /// **This is the install test.** `/product-launcher/…/eligibility` is not:
    /// it answers `true` for `pbe` on a machine with no PBE install, because it
    /// asks whether the account *may* launch it.
    pub fn is_installed(&self) -> bool {
        !self.install_full_path.is_empty()
    }

    /// The install root, when installed.
    pub fn install_root(&self) -> Option<PathBuf> {
        self.is_installed()
            .then(|| PathBuf::from(&self.install_full_path))
    }

    /// The directory a named secondary patchline resolves to.
    ///
    /// For League, `secondary_dir(ids::secondary_patchlines::GAME_PATCH)` is the
    /// `Game` subdirectory - a **declared relative path**, so it is derived
    /// rather than assumed. A patchline that declares no such secondary gets
    /// `None`: guessing here would defeat the point of asking.
    pub fn secondary_dir(&self, id: &str) -> Option<PathBuf> {
        let root = self.install_root()?;
        let relative = self.secondary_patchlines.iter().find(|s| s.id == id)?;
        if relative.relative_path.is_empty() {
            return None;
        }
        Some(root.join(&relative.relative_path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::ids::{products, secondary_patchlines};

    /// Recorded from a live client (EUW, 2026-07-27), trimmed to the fields we
    /// read. `pbe` is entitled but not installed on that machine, which is what
    /// makes it the useful half of the fixture.
    const RECORDED: &str = r#"[
      {
        "id": "league_of_legends",
        "patchlines": [
          {
            "id": "live",
            "install_id": "league_of_legends.live",
            "full_name": "League of Legends",
            "install_full_path": "C:/Riot Games/League of Legends",
            "install_dir": "League of Legends",
            "primary_executable": "LeagueClient.exe",
            "release_id": "ED5FB7B738681EE8",
            "configuration_status": "has_configuration",
            "vanguard_dependency": true,
            "launch_disabled": false,
            "secondary_patchlines": [{ "id": "game_patch", "relativePath": "Game" }]
          },
          {
            "id": "pbe",
            "install_id": "league_of_legends.pbe",
            "full_name": "League of Legends PBE",
            "install_full_path": "",
            "install_dir": "League of Legends (PBE)",
            "primary_executable": "",
            "release_id": "",
            "configuration_status": "unsupported_region",
            "vanguard_dependency": false,
            "launch_disabled": false,
            "secondary_patchlines": []
          }
        ]
      },
      { "id": "bacon", "patchlines": [] }
    ]"#;

    fn league_fixture() -> Product {
        serde_json::from_str::<Vec<Product>>(RECORDED)
            .unwrap()
            .into_iter()
            .find(|p| p.id == products::LEAGUE_OF_LEGENDS)
            .unwrap()
    }

    #[test]
    fn parses_the_recorded_response() {
        let league = league_fixture();
        assert_eq!(league.patchlines.len(), 2);

        let live = league.patchline("live").unwrap();
        assert_eq!(live.full_name, "League of Legends");
        assert_eq!(live.release_id, "ED5FB7B738681EE8");
        assert_eq!(live.primary_executable, "LeagueClient.exe");
        assert!(live.vanguard_dependency);
    }

    /// The whole reason to prefer this over `…/eligibility`, which answers
    /// `true` for a patchline that is not on the machine at all.
    #[test]
    fn an_empty_install_path_means_not_installed() {
        let league = league_fixture();
        assert!(league.patchline("live").unwrap().is_installed());
        assert!(!league.patchline("pbe").unwrap().is_installed());

        let installed: Vec<&str> = league
            .installed_patchlines()
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(installed, vec!["live"]);
    }

    /// `Game` is a declared relative path. Deriving it is the point.
    #[test]
    fn secondary_dir_comes_from_the_declared_secondary_patchline() {
        let league = league_fixture();
        let live = league.patchline("live").unwrap();

        assert_eq!(
            live.secondary_dir(secondary_patchlines::GAME_PATCH)
                .unwrap(),
            PathBuf::from("C:/Riot Games/League of Legends").join("Game")
        );
        assert_eq!(
            live.install_root().unwrap(),
            PathBuf::from("C:/Riot Games/League of Legends")
        );
    }

    /// An installed patchline that declares no `game_patch` must not silently
    /// fall back to a hardcoded `Game` - that guess is exactly what this
    /// endpoint exists to remove.
    #[test]
    fn a_missing_secondary_yields_no_dir() {
        let patchline: Patchline = serde_json::from_str(
            r#"{ "id": "live", "install_full_path": "C:/Games/LoL", "secondary_patchlines": [] }"#,
        )
        .unwrap();

        assert!(patchline.is_installed());
        assert!(
            patchline
                .secondary_dir(secondary_patchlines::GAME_PATCH)
                .is_none()
        );
    }

    #[test]
    fn an_uninstalled_patchline_has_neither_path() {
        let league = league_fixture();
        let pbe = league.patchline("pbe").unwrap();
        assert!(pbe.install_root().is_none());
        assert!(
            pbe.secondary_dir(secondary_patchlines::GAME_PATCH)
                .is_none()
        );
    }

    /// The namespace is absent from swagger and already split across `v1`/`v4`,
    /// so it *will* grow and rename fields. Neither may break parsing - which is
    /// why tolerance is a generator policy rather than a per-type choice.
    #[test]
    fn tolerates_unknown_keys_and_missing_ones() {
        let products: Vec<Product> = serde_json::from_str(
            r#"[{ "id": "league_of_legends", "brand_new_key": { "nested": 1 },
                  "patchlines": [{ "id": "live", "another_new_one": true }] }]"#,
        )
        .unwrap();

        let live = products[0].patchline("live").unwrap();
        assert!(!live.is_installed());
        assert_eq!(live.release_id, "");
        assert!(live.secondary_patchlines.is_empty());
    }

    /// Insurance for the day the odd camelCase spelling is normalised.
    #[test]
    fn accepts_either_spelling_of_the_relative_path() {
        for body in [
            r#"{ "id": "game_patch", "relativePath": "Game" }"#,
            r#"{ "id": "game_patch", "relative_path": "Game" }"#,
        ] {
            let secondary: SecondaryPatchline = serde_json::from_str(body).unwrap();
            assert_eq!(secondary.relative_path, "Game");
        }
    }

    #[test]
    fn no_league_product_yields_nothing() {
        let products: Vec<Product> = serde_json::from_str(r#"[{ "id": "valorant" }]"#).unwrap();
        assert!(!products.iter().any(|p| p.id == products::LEAGUE_OF_LEGENDS));
    }
}
