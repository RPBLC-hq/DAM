use serde::{Deserialize, Serialize};

use crate::{TrafficObservation, TrafficProtocol, normalize_host};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TrafficMatch {
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub ips: Vec<String>,
    #[serde(default)]
    pub urls: Vec<String>,
    #[serde(default)]
    pub ports: Vec<u16>,
    #[serde(default)]
    pub protocols: Vec<TrafficProtocol>,
    #[serde(default)]
    pub process_names: Vec<String>,
}

impl TrafficMatch {
    pub fn is_empty(&self) -> bool {
        self.domains.is_empty()
            && self.ips.is_empty()
            && self.urls.is_empty()
            && self.ports.is_empty()
            && self.protocols.is_empty()
            && self.process_names.is_empty()
    }

    pub fn matches(&self, observation: &TrafficObservation) -> bool {
        !self.is_empty()
            && matches_domains(&self.domains, &observation.host)
            && matches_ips(&self.ips, &observation.host)
            && matches_urls(&self.urls, observation)
            && matches_ports(&self.ports, observation.port)
            && matches_protocols(&self.protocols, observation.protocol)
            && matches_process_names(&self.process_names, observation.process_name.as_deref())
    }

    pub fn normalized_domains(&self) -> impl Iterator<Item = String> + '_ {
        self.domains
            .iter()
            .map(|domain| normalize_host(domain))
            .filter(|domain| !domain.is_empty() && !domain.starts_with("*."))
    }

    pub fn normalized_route_hosts(&self) -> impl Iterator<Item = String> + '_ {
        self.domains
            .iter()
            .chain(self.urls.iter())
            .map(|value| normalize_host(value))
            .filter(|host| !host.is_empty() && !host.starts_with("*."))
    }
}

pub(crate) fn domain_matches(pattern: &str, host: &str) -> bool {
    let pattern = normalize_host(pattern);
    let host = normalize_host(host);
    if pattern.is_empty() || host.is_empty() {
        return false;
    }

    if let Some(suffix) = pattern.strip_prefix("*.") {
        return host == suffix || host.ends_with(&format!(".{suffix}"));
    }

    if let Some(suffix) = pattern.strip_prefix('.') {
        return host == suffix || host.ends_with(&format!(".{suffix}"));
    }

    pattern == host
}

fn matches_domains(domains: &[String], host: &str) -> bool {
    domains.is_empty() || domains.iter().any(|domain| domain_matches(domain, host))
}

fn matches_ips(ips: &[String], host: &str) -> bool {
    ips.is_empty()
        || ips
            .iter()
            .any(|ip| normalize_host(ip).eq_ignore_ascii_case(&normalize_host(host)))
}

fn matches_urls(urls: &[String], observation: &TrafficObservation) -> bool {
    urls.is_empty() || urls.iter().any(|url| url_matches(url, observation))
}

fn matches_ports(ports: &[u16], port: Option<u16>) -> bool {
    ports.is_empty() || port.is_some_and(|port| ports.contains(&port))
}

fn matches_protocols(protocols: &[TrafficProtocol], protocol: TrafficProtocol) -> bool {
    protocols.is_empty() || protocols.contains(&protocol)
}

fn matches_process_names(process_names: &[String], process_name: Option<&str>) -> bool {
    process_names.is_empty()
        || process_name.is_some_and(|name| {
            process_names
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(name))
        })
}

fn url_matches(pattern: &str, observation: &TrafficObservation) -> bool {
    let trimmed = pattern.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.starts_with('/') {
        return observation
            .path
            .as_deref()
            .is_some_and(|path| path.starts_with(trimmed));
    }

    let host = normalize_host(trimmed);
    if host.is_empty() || !domain_matches(&host, &observation.host) {
        return false;
    }

    let Some(path) = pattern_path(trimmed) else {
        return true;
    };
    observation
        .path
        .as_deref()
        .is_some_and(|observed_path| observed_path.starts_with(&path))
}

fn pattern_path(pattern: &str) -> Option<String> {
    let without_scheme = pattern
        .strip_prefix("https://")
        .or_else(|| pattern.strip_prefix("http://"))
        .or_else(|| pattern.strip_prefix("wss://"))
        .or_else(|| pattern.strip_prefix("ws://"))
        .unwrap_or(pattern);
    let (_, path) = without_scheme.split_once('/')?;
    if path.is_empty() {
        None
    } else {
        Some(format!("/{path}"))
    }
}
