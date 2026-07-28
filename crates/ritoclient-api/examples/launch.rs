//! Start a game through the Riot Client, printing progress as it goes.
//!
//! ```text
//! cargo run -p ritoclient-api --example launch
//! ```
//!
//! **This actually launches the game.** It is the counterpart to `probe`: that
//! one proves we can see the client, this one proves we can drive it, with no UI
//! in the way.
//!
//! It is also the only way to watch the tray-idle path happen - `wakingClient`
//! followed by a run of `waitingForClient` - which no unit test can reach. That
//! path is worth understanding: a client idling in the tray exposes *two*
//! functions, and the launch route is not one of them, so the launch has to wake
//! it and then wait for the product-launcher plugin to register.
//!
//! Set `LTK_LEAGUE_PATH` to pick the Riot Client that owns a specific install;
//! without it the machine's default client is used.
//!
//! Windows only - everywhere else this reports `UnsupportedPlatform`.

use std::path::PathBuf;

use ritoclient_api::ids::{patchlines, products};
use ritoclient_api::{LaunchObserver, LaunchProgress, LaunchStage, LaunchTarget};

/// The executable to watch for. This crate names no products, so the caller
/// says which process means "the game is up".
const GAME_PROCESS: &str = "leagueclient.exe";

/// Prints each stage as it arrives.
///
/// A launch is one blocking call that can spend most of a minute inside a single
/// step, so without these there is no way to tell waiting from hanging.
struct Printer;

impl LaunchObserver for Printer {
    fn on_progress(&self, progress: LaunchProgress) {
        match progress.stage {
            LaunchStage::WaitingForClient => println!(
                "  waiting for the client... {}s / {}s",
                progress.waited_secs, progress.timeout_secs
            ),
            stage => println!("  {stage:?}"),
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let product_root = std::env::var("LTK_LEAGUE_PATH").ok().map(PathBuf::from);

    let target = LaunchTarget {
        product_id: products::LEAGUE_OF_LEGENDS.to_string(),
        patchline_id: patchlines::LIVE.to_string(),
    };

    println!(
        "{:#?}",
        ritoclient_api::availability(product_root.as_deref(), GAME_PROCESS)
    );
    println!("\nlaunching {}/{}", target.product_id, target.patchline_id);

    let outcome = ritoclient_api::launch(product_root.as_deref(), &target, GAME_PROCESS, &Printer)?;

    // "Delivered" is not "running": the client may still patch, or wait for a
    // login. The session id is the key into `/product-session/v1/external-sessions`,
    // which is what a real "did it start?" check should follow.
    println!("\n{outcome:#?}");

    Ok(())
}
