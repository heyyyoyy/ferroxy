use std::str::FromStr;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub server: ServerConfig,
    pub routes: Vec<RouteConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    pub https_listen: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteConfig {
    pub host: String,
    pub upstream: String,
}

impl FromStr for Config {
    type Err = toml::de::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        toml::from_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
[server]
https_listen = "127.0.0.1:8443"

[[routes]]
host = "example.com"
upstream = "http://127.0.0.1:3000"
"#;

    #[test]
    fn parses_minimal_config() {
        let cfg: Config = MINIMAL.parse().expect("valid config");

        assert_eq!(cfg.server.https_listen, "127.0.0.1:8443");
        assert_eq!(cfg.routes.len(), 1);
        assert_eq!(cfg.routes[0].host, "example.com");
        assert_eq!(cfg.routes[0].upstream, "http://127.0.0.1:3000");
    }

    #[test]
    fn parses_multiple_routes_preserving_order() {
        let doc = r#"
[server]
https_listen = "127.0.0.1:8443"

[[routes]]
host = "a.example.com"
upstream = "http://127.0.0.1:3001"

[[routes]]
host = "b.example.com"
upstream = "http://127.0.0.1:3002"
"#;
        let cfg: Config = doc.parse().expect("valid config");

        let hosts: Vec<_> = cfg.routes.iter().map(|r| r.host.as_str()).collect();
        assert_eq!(hosts, ["a.example.com", "b.example.com"]);
    }

    #[test]
    fn rejects_unknown_field_in_server() {
        let doc = r#"
[server]
https_listen = "127.0.0.1:8443"
listen = "0.0.0.0:80"

[[routes]]
host = "example.com"
upstream = "http://127.0.0.1:3000"
"#;
        let err = doc.parse::<Config>().unwrap_err().to_string();

        assert!(err.contains("unknown field"), "unexpected error: {err}");
        assert!(err.contains("listen"), "unexpected error: {err}");
    }

    #[test]
    fn rejects_unknown_field_in_route() {
        let doc = r#"
[server]
https_listen = "127.0.0.1:8443"

[[routes]]
host = "example.com"
upstream = "http://127.0.0.1:3000"
port = 8080
"#;
        let err = doc.parse::<Config>().unwrap_err().to_string();

        assert!(err.contains("unknown field"), "unexpected error: {err}");
        assert!(err.contains("port"), "unexpected error: {err}");
    }

    #[test]
    fn rejects_unknown_top_level_key() {
        let doc = r#"
name = "ferroxy"

[server]
https_listen = "127.0.0.1:8443"

[[routes]]
host = "example.com"
upstream = "http://127.0.0.1:3000"
"#;
        let err = doc.parse::<Config>().unwrap_err().to_string();

        assert!(err.contains("unknown field"), "unexpected error: {err}");
        assert!(err.contains("name"), "unexpected error: {err}");
    }

    #[test]
    fn rejects_missing_routes() {
        let doc = r#"
[server]
https_listen = "127.0.0.1:8443"
"#;
        let err = doc.parse::<Config>().unwrap_err().to_string();

        assert!(err.contains("missing field"), "unexpected error: {err}");
        assert!(err.contains("routes"), "unexpected error: {err}");
    }

    #[test]
    fn rejects_missing_https_listen() {
        let doc = r#"
[server]

[[routes]]
host = "example.com"
upstream = "http://127.0.0.1:3000"
"#;
        let err = doc.parse::<Config>().unwrap_err().to_string();

        assert!(err.contains("missing field"), "unexpected error: {err}");
        assert!(err.contains("https_listen"), "unexpected error: {err}");
    }

    #[test]
    fn allows_empty_routes_list_at_parse_stage() {
        // Parsing accepts an empty route list; validation rejects it.
        let doc = r#"
routes = []

[server]
https_listen = "127.0.0.1:8443"
"#;
        let cfg: Config = doc.parse().expect("valid config");
        assert!(cfg.routes.is_empty());
    }

    #[test]
    fn rejects_malformed_toml() {
        assert!("[server".parse::<Config>().is_err());
        assert!("https_listen = 42".parse::<Config>().is_err());
    }
}
