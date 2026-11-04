use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
};

use serde::Deserialize;

/// Errors produced by [`Config::load`].
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The configuration file could not be read.
    #[error("failed to read config `{path}`")]
    Io {
        /// Path of the configuration file that was requested.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The configuration file is not valid TOML or has an unexpected shape.
    #[error("invalid config `{path}`")]
    Parse {
        /// Path of the configuration file that was requested.
        path: PathBuf,
        /// Underlying TOML deserialization error, including line and column.
        #[source]
        source: toml::de::Error,
    },
}

/// Errors produced while parsing a [`HostPattern`].
#[derive(Debug, thiserror::Error)]
pub enum HostPatternError {
    /// The host string is empty.
    #[error("host must not be empty")]
    Empty,
    /// The host string does not match DNS, wildcard, or IP syntax.
    #[error("invalid host `{0}`")]
    Invalid(String),
}

/// Root configuration, mirroring the TOML file.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Listener and server-wide settings.
    pub server: ServerConfig,
    /// Routes evaluated by host and path specificity.
    pub routes: Vec<RouteConfig>,
}

/// Listener and server-wide settings.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    /// Address the HTTPS listener binds to.
    pub https_listen: SocketAddr,
}

/// A single proxy route.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteConfig {
    /// Host to match: DNS name, `*.` wildcard, or IP literal.
    pub host: HostPattern,
    /// Path prefix to match. Always starts with `/` and defaults to `/`.
    #[serde(default = "RouteConfig::default_path_prefix")]
    pub path_prefix: String,
    /// Upstream base URL. Only `http://` is supported.
    pub upstream: String,
    /// Forward the original `Host` header to the upstream. Defaults to `true`.
    #[serde(default = "RouteConfig::default_bool_true")]
    pub preserve_host: bool,
    /// Remove `path_prefix` from the upstream request path. Defaults to `false`.
    #[serde(default)]
    pub strip_prefix: bool,
}

/// Host selector for a route: an exact DNS name, a single-label DNS
/// wildcard (`*.example.com`), or an IP literal.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(try_from = "String")]
pub enum HostPattern {
    /// Exact DNS name, normalized to lowercase.
    Dns(String),
    /// `*.example.com`; matches exactly one additional label.
    Wildcard {
        /// Domain suffix after the leading `*.`, normalized to lowercase.
        suffix: String,
    },
    /// IPv4 or IPv6 literal, brackets stripped.
    Ip(std::net::IpAddr),
}

impl TryFrom<String> for HostPattern {
    type Error = HostPatternError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.is_empty() {
            return Err(HostPatternError::Empty);
        }
        if let Ok(ip) = value.parse::<std::net::IpAddr>() {
            return Ok(Self::Ip(ip));
        }
        let normalized = value.to_ascii_lowercase();

        if let Some(suffix) = normalized.strip_prefix("*.") {
            if suffix.is_empty() || suffix.contains("*") || suffix.contains(":") {
                return Err(HostPatternError::Invalid(value));
            }
            return Ok(Self::Wildcard {
                suffix: suffix.to_owned(),
            });
        }
        if normalized.contains('*') || normalized.contains(':') {
            return Err(HostPatternError::Invalid(value));
        }

        Ok(Self::Dns(normalized.to_owned()))
    }
}

impl FromStr for Config {
    type Err = toml::de::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        toml::from_str(s)
    }
}

impl Config {
    /// Reads and parses a configuration file.
    ///
    /// Returns [`ConfigError::Io`] when the file cannot be read and
    /// [`ConfigError::Parse`] when it is not valid TOML or does not
    /// match the expected schema.
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

impl RouteConfig {
    fn default_path_prefix() -> String {
        "/".to_owned()
    }

    fn default_bool_true() -> bool {
        true
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
        assert_eq!(
            cfg.routes[0].host,
            HostPattern::Dns("example.com".to_owned())
        );
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
        let a = HostPattern::Dns("a.example.com".to_owned());
        let b = HostPattern::Dns("b.example.com".to_owned());

        let hosts: Vec<_> = cfg.routes.iter().map(|r| &r.host).collect();
        assert_eq!(hosts, [&a, &b]);
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

    const ROUTE_MINIMAL: &str = r#"
[server]
https_listen = "127.0.0.1:8443"

[[routes]]
host = "example.com"
upstream = "http://127.0.0.1:3000"
"#;

    #[test]
    fn applies_route_defaults() {
        let cfg: Config = ROUTE_MINIMAL.parse().expect("valid config");
        let route = &cfg.routes[0];

        assert_eq!(route.path_prefix, "/");
        assert!(route.preserve_host, "preserve_host defaults to true");
        assert!(!route.strip_prefix, "strip_prefix defaults to false");
    }

    #[test]
    fn explicit_route_fields_override_defaults() {
        let doc = r#"
[server]
https_listen = "127.0.0.1:8443"

[[routes]]
host = "example.com"
path_prefix = "/api"
upstream = "http://127.0.0.1:3000"
preserve_host = false
strip_prefix = true
"#;
        let cfg: Config = doc.parse().expect("valid config");
        let route = &cfg.routes[0];

        assert_eq!(route.path_prefix, "/api");
        assert!(!route.preserve_host);
        assert!(route.strip_prefix);
    }

    #[test]
    fn path_prefix_is_kept_verbatim() {
        // Config does not normalize the prefix: no slashes are added or removed.
        for prefix in ["/", "/api", "/api/", "/api/v1"] {
            let doc = format!(
                r#"
[server]
https_listen = "127.0.0.1:8443"

[[routes]]
host = "example.com"
path_prefix = "{prefix}"
upstream = "http://127.0.0.1:3000"
"#
            );
            let cfg: Config = doc.parse().expect("valid config");

            assert_eq!(cfg.routes[0].path_prefix, prefix);
        }
    }

    #[test]
    fn rejects_non_bool_route_flags() {
        for field in ["preserve_host", "strip_prefix"] {
            let doc = format!(
                r#"
[server]
https_listen = "127.0.0.1:8443"

[[routes]]
host = "example.com"
upstream = "http://127.0.0.1:3000"
{field} = "false"
"#
            );
            let err = doc.parse::<Config>().unwrap_err().to_string();

            assert!(err.contains("invalid type"), "{field}: {err}");
            assert!(err.contains(field), "{field}: {err}");
        }
    }

    #[test]
    fn defaults_are_applied_per_route() {
        let doc = r#"
[server]
https_listen = "127.0.0.1:8443"

[[routes]]
host = "a.example.com"
upstream = "http://127.0.0.1:3001"

[[routes]]
host = "b.example.com"
path_prefix = "/api"
upstream = "http://127.0.0.1:3002"
strip_prefix = true
"#;
        let cfg: Config = doc.parse().expect("valid config");

        assert_eq!(cfg.routes[0].path_prefix, "/");
        assert!(cfg.routes[0].preserve_host);
        assert!(!cfg.routes[0].strip_prefix);

        assert_eq!(cfg.routes[1].path_prefix, "/api");
        assert!(cfg.routes[1].preserve_host);
        assert!(cfg.routes[1].strip_prefix);
    }

    #[test]
    fn parses_host_patterns() {
        let cases = [
            ("example.com", HostPattern::Dns("example.com".to_owned())),
            ("Example.COM", HostPattern::Dns("example.com".to_owned())),
            (
                "*.example.com",
                HostPattern::Wildcard {
                    suffix: "example.com".to_owned(),
                },
            ),
            ("127.0.0.1", HostPattern::Ip("127.0.0.1".parse().unwrap())),
            ("::1", HostPattern::Ip("::1".parse().unwrap())),
        ];

        for (input, expected) in cases {
            let actual = HostPattern::try_from(input.to_owned())
                .unwrap_or_else(|e| panic!("input `{input}`: {e}"));
            assert_eq!(actual, expected, "input `{input}`");
        }
    }

    #[test]
    fn rejects_invalid_host_patterns() {
        for input in [
            "",
            "*",
            "a*b.com",
            "*.",
            "*.ex*ample.com",
            "example.com:8443",
            "a.*.com",
        ] {
            assert!(
                HostPattern::try_from(input.to_owned()).is_err(),
                "input `{input}` should be rejected"
            );
        }
    }
}
