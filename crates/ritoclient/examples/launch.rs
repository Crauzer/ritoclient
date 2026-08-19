//! Start a game through the Riot Client, printing progress as it goes, then
//! follow the session to its end, printing each event.
//!
//! ```text
//! cargo run -p ritoclient --example launch
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

use ritoclient::ids::{patchlines, products};
use ritoclient::{LaunchStage, LaunchTarget, Launcher};

/// The executable to watch for. This crate names no products, so the caller
/// says which process means "the game is up".
const GAME_PROCESS: &str = "leagueclient.exe";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let product_root = std::env::var("LTK_LEAGUE_PATH").ok().map(PathBuf::from);
    let target = LaunchTarget::new(products::LEAGUE_OF_LEGENDS, patchlines::LIVE);

    // A launch is one blocking call that can spend most of a minute inside a
    // single step, so without progress there is no telling waiting from hanging.
    let mut builder =
        Launcher::builder(target, GAME_PROCESS).on_progress(|progress| match progress.stage {
            LaunchStage::WaitingForClient => println!(
                "  waiting for the client... {}s / {}s",
                progress.waited_secs, progress.timeout_secs
            ),
            stage => println!("  {stage:?}"),
        });
    if let Some(root) = &product_root {
        builder = builder.product_root(root);
    }
    let launcher = builder.build()?;

    println!("{:#?}", launcher.availability());
    println!("\nlaunching {}", launcher.target());

    let outcome = launcher.launch()?;

    // "Delivered" is not "running": the client may still patch, or wait for a
    // login. The session id is the key into `/product-session/v1/external-sessions`,
    // which is what a real "did it start?" check should follow.
    println!("\n{outcome:#?}");

    let Some(session_id) = outcome.session_id else {
        println!("\nno session id came back, so there is nothing to follow");
        return Ok(());
    };

    // The watcher runs on its own thread and stops itself at the terminal
    // event, so all this loop does is keep the process alive to see it.
    println!("\nfollowing session {session_id} to its end (Ctrl+C to give up)");
    let watch = launcher.watch_session(&session_id, |event| println!("  {event:?}"));
    while watch.is_watching() {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    Ok(())
}
