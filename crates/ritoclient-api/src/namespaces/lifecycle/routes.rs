//! The routes of `/riot-client-lifecycle`.

ritoclient_core::routes! {
    namespace = "riot-client-lifecycle";

    /// `POST /riot-client-lifecycle/v1/hide` - "Hide the UX." Sends the window
    /// to the tray.
    HIDE = 1, "hide";

    /// `POST /riot-client-lifecycle/v1/show` - "Show the UX."
    SHOW = 1, "show";
}
