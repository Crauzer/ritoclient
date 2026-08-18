//! The routes of `/riotclientapp`.

ritoclient_core::routes! {
    namespace = "riotclientapp";

    /// `POST /riotclientapp/v1/new-args` - hand a duplicate instance's argv to
    /// the running client.
    ///
    /// Its 204 acknowledges the arguments and reports nothing about what the client
    /// does with them. On 136 a launch pair sent here launches. See the module docs.
    NEW_ARGS = 1, "new-args";
}
