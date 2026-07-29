//! Hide the Riot Client's window, then put it back.
//!
//! ```text
//! cargo run -p ritoclient --example hide_and_show
//! ```
//!
//! `hide` sends the window to the **tray**, not the taskbar - there is no
//! `minimize` on this plugin; it was probed and 404s. The client keeps running
//! either way, which is required rather than incidental: a launched game holds a
//! live remoting session with it for the whole run.
//!
//! Hiding is idempotent, so hiding an already-hidden window is a no-op that
//! still answers 204. That is what lets
//! [`hide_for_play_session`](ritoclient::session::hide_for_play_session)
//! re-assert it blind after a game exits - the client un-hides *itself* at that
//! point, so one hide would last exactly as long as the game did.

use std::thread::sleep;
use std::time::Duration;

use ritoclient::prelude::*;
use ritoclient::{Client, live_lockfile};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if live_lockfile().is_none() {
        println!("No live Riot Client - start it and re-run.");
        return Ok(());
    }

    let client = Client::new()?;

    println!("hiding...");
    client.lifecycle().hide()?;

    sleep(Duration::from_secs(3));

    println!("restoring...");
    client.lifecycle().show()?;

    Ok(())
}
