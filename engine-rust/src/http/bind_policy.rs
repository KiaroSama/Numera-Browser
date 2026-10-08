use super::tokens::TokenStore;

/// This is the source policy decision, not DNS resolution or a bound socket.
/// The caller normalizes its selected host before checking and binding it.
pub fn refusal(
    host: &str,
    tokens: &TokenStore,
    environment: &str,
    insecure_override: &str,
) -> Option<String> {
    if ["127.0.0.1", "localhost", "::1", "0:0:0:0:0:0:0:1"]
        .contains(&host.trim().to_lowercase().as_str())
        || !environment.trim().is_empty()
        || !tokens.list().is_empty()
        || !insecure_override.trim().is_empty()
    {
        return None;
    }
    Some(format!(
        concat!(
            "Refusing to serve on {} without authentication.\n",
            "  This API can create profiles, launch browsers and export cookies, and\n",
            "  on a non-loopback address anything that can reach the port can use it.\n",
            "  Fix it with either:\n",
            "    numera token create \"<what it is for>\"   (recommended)\n",
            "    set NUMERA_API_TOKEN=<secret>\n",
            "  Then set NUMERA_ALLOWED_ORIGINS if a browser page will call it.\n",
            "  To bind anyway - only when something else in front already\n",
            "  authenticates - set NUMERA_ALLOW_INSECURE_BIND=1."
        ),
        host
    ))
}
