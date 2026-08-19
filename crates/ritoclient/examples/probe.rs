//! Read-only survey of what *this* machine's Riot Client reports.
//!
//! Touches nothing. It is the cheap first step when something launcher-shaped
//! misbehaves, the way to see registry fields this crate does not model yet, and
//! how [`ritoclient::ids`] gets refreshed after a client update.
//!
//! ```text
//! cargo run -p ritoclient --example probe
//! ```
//!
//! Set `LTK_LEAGUE_PATH` to exercise the `associated_client` lookup in
//! `RiotClientInstalls.json`; without it only the `rc_default` / `rc_live`
//! fallbacks are covered.

use std::path::PathBuf;

use ritoclient::ids::{patchlines, products};
use ritoclient::prelude::*;
use ritoclient::{
    Client, EndpointMeta, Method, Presence, Route, installs, live_lockfile, processes,
};

const REGISTRY_PRODUCTS: Route = Route::new("rnet-product-registry", 4, "products");
const INSTALL_STATES: Route = Route::new("rnet-product-registry", 1, "install-states");
const PATCH_INSTALLS: Route = Route::new("patch", 1, "installs");
const VANGUARD_STATUS: Route = Route::new("vanguard", 1, "status");
const PROCESS_CONTROL: Route = Route::new("process-control", 1, "process");

/// What the account is entitled to, which is not what is installed.
const PRODUCT_DEFINITIONS: Route = Route::new("product-metadata", 1, "definitions/products");

/// Carries `locale_data.available_locales` - the authoritative locale set.
const PRODUCT_SETTINGS: Route = Route::new(
    "data-store",
    1,
    "product-settings/products/{productId}/patchlines/{patchlineId}",
);

/// The locale currently selected, as a bare JSON string.
const PRODUCT_LOCALE: Route = Route::new(
    "product-integration",
    1,
    "locale/products/{productId}/patchlines/{patchlineId}",
);

fn main() {
    let product_root = std::env::var("LTK_LEAGUE_PATH").ok().map(PathBuf::from);

    println!(
        "installs file:   {}",
        installs::default_installs_path().display()
    );
    println!("LTK_LEAGUE_PATH: {product_root:?}");

    // Debug redacts the password - this output ends up in bug reports.
    match live_lockfile() {
        Some(lockfile) => println!("lockfile:        {lockfile:?}"),
        None => {
            println!(
                "lockfile:        none - no Riot Client is running, so nothing else can answer"
            );
            return;
        }
    }

    for (name, pid) in processes::list_riot_processes() {
        println!("running:         {name} (pid {pid})");
    }

    println!(
        "\n{:#?}",
        ritoclient::availability(product_root.as_deref(), "leagueclient.exe")
    );

    let Ok(client) = Client::new() else {
        println!("could not build an HTTP client");
        return;
    };

    // A client idling in the tray is alive but has no API: its whole surface
    // collapses to the argv handoff, so every namespace below answers 404
    // RESOURCE_NOT_FOUND. That a lockfile parses and a pid is live does not mean
    // the API is there. A launch recovers from it by waking the client; this
    // does not, so say so rather than printing a wall of 404s.
    if is_tray_idle(&client) {
        println!(
            "\nThe Riot Client is idling in the tray, where it serves no API. \
             Open its window and re-run."
        );
        return;
    }

    println!("\n{:#?}", client.product_registry().products());

    sessions(&client);

    // The parsed view drops anything this crate does not model, so the raw body
    // is the only way to notice a field worth modelling.
    for route in [
        REGISTRY_PRODUCTS,
        INSTALL_STATES,
        PATCH_INSTALLS,
        VANGUARD_STATUS,
        PROCESS_CONTROL,
    ] {
        dump(&client, route);
    }

    // What `ids` is built from. Re-run after a client update and diff.
    dump(&client, PRODUCT_DEFINITIONS);

    let league = [
        ("productId", products::LEAGUE_OF_LEGENDS),
        ("patchlineId", patchlines::LIVE),
    ];
    dump(&client, PRODUCT_SETTINGS.bind(&league));
    dump(&client, PRODUCT_LOCALE.bind(&league));

    // Unversioned, so it cannot be a `Route` - the raw-path escape hatch covers
    // the handful of routes that sit outside the namespace convention.
    dump(&client, "/riotclient/region-locale");

    // Everything the workspace declares, checked against this client. The
    // endpoint tables carry the verb, so each row is asked honestly - see
    // `confirm_endpoint` for what that means per method.
    println!();
    for meta in ritoclient::namespaces::endpoints() {
        confirm_endpoint(&client, meta);
    }

    // `/help` names functions but gives no paths, so a spelling not yet
    // declared as an endpoint has to be confirmed before it is trusted. A GET
    // against a POST-only route answers 405, which proves the spelling without
    // firing the call.
    for route in [
        Route::new("riot-client-lifecycle", 1, "minimize"),
        Route::new("riot-client-lifecycle", 1, "quit"),
        Route::new("launch-restriction", 1, "restrictions"),
    ] {
        confirm(&client, route);
    }
}

/// Sessions the client is tracking, parsed rather than dumped.
///
/// The one namespace this survey must never `dump`: the raw body carries an
/// `rso_auth.authorization-key` inside `launchConfiguration`, and this output
/// goes into bug reports. The parsed view cannot carry it, because the field is
/// not on the type - so this is safe by construction rather than by remembering.
///
/// Worth a section of its own because a session that fails to parse is invisible
/// everywhere else: the lookup answers `None`, the watcher reads that as "no
/// session yet", and a launcher-shaped misbehaviour with no error attached is
/// exactly what this survey is the first step for.
fn sessions(client: &Client) {
    let Some(sessions) = client.product_session().external_sessions() else {
        println!("\nsessions: the client reported none, or the response did not parse");
        return;
    };

    println!("\nsessions: {} tracked", sessions.len());
    for (id, session) in &sessions {
        println!(
            "  {id}: {}/{} phase={} playing={} ended={} reason={} code={} release={}",
            session.product_id,
            session.patchline_id,
            session.phase(),
            session.is_playing(),
            session.has_ended(),
            session.exit_reason(),
            session.exit_code,
            session.version,
        );
    }
}

fn is_tray_idle(client: &Client) -> bool {
    client
        .probe(REGISTRY_PRODUCTS)
        .is_ok_and(|presence| presence == Presence::Absent)
}

/// `GET` a path and print its body, pretty-printed when it is JSON.
fn dump(client: &Client, path: impl Into<String>) {
    let path = path.into();

    match client.get(&path).send() {
        Ok(response) if response.is_success() => {
            let body = response
                .json::<serde_json::Value>()
                .ok()
                .and_then(|value| serde_json::to_string_pretty(&value).ok())
                .unwrap_or_else(|| response.body().to_string());

            println!("\nGET {path}\n{body}");
        }
        Ok(response) => println!(
            "\nGET {path} -> {} {}",
            response.status(),
            explain(&response)
        ),
        Err(e) => println!("\nGET {path} -> {e}"),
    }
}

/// Confirm a declared endpoint's spelling against the live client, without
/// firing anything mutating.
///
/// The metadata row carries the verb, so the check is honest per endpoint: a
/// GET endpoint is simply asked, and for a mutating one the same GET drawing a
/// 405 proves the route is live and gated exactly as declared - existence
/// confirmed without invoking the call.
fn confirm_endpoint(client: &Client, meta: EndpointMeta) {
    let path = meta.route.path();

    // A parameterized path cannot be probed blind; its placeholders are bound
    // by the callers earlier in this survey.
    if path.contains('{') {
        println!("{} {path} (parameterized; not probed blind)", meta.method);
        return;
    }

    match client.get(&path).send() {
        Ok(response) => {
            let meaning = match response.status().as_u16() {
                405 if meta.method != Method::Get => "exists, method-gated as declared".to_string(),
                200 | 204 if meta.method == Method::Get => "answered".to_string(),
                _ => explain(&response),
            };
            println!(
                "{} {path} -> {} ({meaning})",
                meta.method,
                response.status()
            );
        }
        Err(e) => println!("{} {path} -> {e}", meta.method),
    }
}

/// Confirm a route exists without invoking it.
fn confirm(client: &Client, path: impl Into<String>) {
    let path = path.into();

    match client.get(&path).send() {
        Ok(response) => {
            let meaning = match response.status().as_u16() {
                405 => "exists, POST-only".to_string(),
                200 | 204 => "exists, answered".to_string(),
                _ => explain(&response),
            };
            println!("GET {path} -> {} ({meaning})", response.status());
        }
        Err(e) => println!("GET {path} -> {e}"),
    }
}

/// The two flavours of 404 mean opposite things and are worth telling apart.
fn explain(response: &ritoclient::Response) -> String {
    match response.riot_error() {
        Some(error) if error.is_resource_not_found() => "no such route".to_string(),
        Some(error) if error.is_rpc_error() => "registered, handler unavailable".to_string(),
        Some(error) => error.error_code,
        None => response.body().trim().chars().take(120).collect(),
    }
}
