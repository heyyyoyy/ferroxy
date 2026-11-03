use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
};

use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config `{path}`")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid config `{path}`")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub server: ServerConfig,
    pub routes: Vec<RouteConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    pub https_listen: SocketAddr,
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

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|err| ConfigError::Io {
            path: path.to_path_buf(),
            source: err,
        })?;
        let config: Self = text.parse().map_err(|err| ConfigError::Parse {
            path: path.to_path_buf(),
            source: err,
        })?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

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

        assert_eq!(
            cfg.server.https_listen,
            "127.0.0.1:8443"
                .parse::<SocketAddr>()
                .expect("socket address")
        );
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

    #[test]
    fn rejects_invalid_https_listen_values() {
        for value in [
            "not-an-address",
            "127.0.0.1",
            "127.0.0.1:99999",
            "localhost:8443",
            "",
        ] {
            let doc = format!(
                r#"
[server]
https_listen = "{value}"

[[routes]]
host = "example.com"
upstream = "http://127.0.0.1:3000"
"#
            );

            let err = doc.parse::<Config>().unwrap_err().to_string();

            assert!(
                err.contains("invalid socket address"),
                "value `{value}` produced: {err}"
            );
        }
    }

    fn write_config(dir: &Path, contents: &str) -> PathBuf {
        let path = dir.join("config.toml");
        std::fs::write(&path, contents).expect("write fixture");
        path
    }

    #[test]
    fn load_reads_valid_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write_config(dir.path(), MINIMAL);

        let cfg = Config::load(&path).expect("valid file");
        assert_eq!(cfg.routes.len(), 1);
    }

    #[test]
    fn load_reports_missing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("absent.toml");

        let err = Config::load(&path).unwrap_err();

        assert_matches!(&err, ConfigError::Io { .. }, "unexpected: {err}");
        assert!(err.to_string().contains("absent.toml"), "{err}");
    }
}
