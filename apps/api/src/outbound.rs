//! Outbound HTTP (reqwest over rustls with ring and the webpki roots). Every client has a
//! connect timeout, never follows redirects, and resolves names through a guard that refuses
//! loopback, private, link-local, shared, multicast and metadata addresses, so a name pointing
//! inward can't reach the internal network; an IP-literal URL (which skips name resolution) is
//! checked the same way before the request. Hosts the operator configured (Gotenberg, a local
//! S3) are trusted and skip the guard.

use std::{
    collections::HashSet,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use reqwest::dns::{Addrs, Name, Resolve, Resolving};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// The outbound client: requests go through `send`, which checks the URL host first.
#[derive(Clone)]
pub struct Outbound {
    client: reqwest::Client,
    trusted: Arc<HashSet<String>>,
}

impl Outbound {
    /// A client whose guard lets `trusted` host names (any case) through.
    pub fn new(trusted: &[String]) -> Result<Self, String> {
        let trusted: Arc<HashSet<String>> =
            Arc::new(trusted.iter().map(|h| h.to_ascii_lowercase()).collect());
        Ok(Self {
            client: client(trusted.clone())?,
            trusted,
        })
    }

    /// Request builder of the underlying client.
    pub fn request(&self, method: reqwest::Method, url: &str) -> reqwest::RequestBuilder {
        self.client.request(method, url)
    }

    /// Send `req` after refusing an untrusted IP-literal host that isn't public.
    pub async fn send(&self, req: reqwest::RequestBuilder) -> Result<reqwest::Response, String> {
        // Errors drop the URL: a presigned one carries a live signature.
        let req = req
            .build()
            .map_err(|e| format!("request: {}", e.without_url()))?;
        if !self.allowed(req.url()) {
            return Err(format!(
                "{} is not a public address",
                req.url().host_str().unwrap_or("?")
            ));
        }
        self.client
            .execute(req)
            .await
            .map_err(|e| format!("send: {}", e.without_url()))
    }

    fn allowed(&self, url: &reqwest::Url) -> bool {
        let Some(host) = url.host_str() else {
            return true;
        };
        match host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<IpAddr>()
        {
            Ok(ip) => self.trusted.contains(&ip.to_string()) || is_public(ip),
            // A name: the resolver guards it.
            Err(_) => true,
        }
    }
}

fn client(trusted: Arc<HashSet<String>>) -> Result<reqwest::Client, String> {
    let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| format!("tls: {e}"))?
    .with_root_certificates(roots)
    .with_no_client_auth();
    reqwest::Client::builder()
        .tls_backend_preconfigured(tls)
        .connect_timeout(CONNECT_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        // A proxy would resolve names itself, past the guard.
        .no_proxy()
        .dns_resolver(Arc::new(Guard { trusted }))
        .build()
        .map_err(|e| format!("http client: {e}"))
}

/// Host of `url` for the trusted list (an operator-configured endpoint).
pub fn host_of(url: &str) -> Option<String> {
    reqwest::Url::parse(url).ok()?.host_str().map(|h| {
        h.trim_start_matches('[')
            .trim_end_matches(']')
            .to_ascii_lowercase()
    })
}

/// Resolves like the system, then drops every address that isn't public.
struct Guard {
    trusted: Arc<HashSet<String>>,
}

impl Resolve for Guard {
    fn resolve(&self, name: Name) -> Resolving {
        let trusted = self.trusted.contains(&name.as_str().to_ascii_lowercase());
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let found = tokio::net::lookup_host((host.as_str(), 0)).await?;
            let addrs: Vec<SocketAddr> = found.filter(|a| trusted || is_public(a.ip())).collect();
            if addrs.is_empty() {
                return Err(format!("{host} resolves to no public address").into());
            }
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

/// True for an address on the public internet.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => public_v4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => public_v4(v4),
            None => public_v6(v6),
        },
    }
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local() // 169.254/16, cloud metadata included
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || a == 0
        || (a == 100 && (64..=127).contains(&b)) // shared / CGNAT 100.64/10
        || (a == 192 && b == 0 && ip.octets()[2] == 0) // 192.0.0/24 protocol assignments
        || (a == 198 && (18..=19).contains(&b)) // benchmarking 198.18/15
        || a >= 240) // reserved
}

fn public_v6(ip: Ipv6Addr) -> bool {
    let seg = ip.segments();
    let first = seg[0];
    // NAT64 (64:ff9b::/96) carries an IPv4 address: judge that one.
    if seg[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
        let [.., a, b, c, d] = ip.octets();
        return public_v4(Ipv4Addr::new(a, b, c, d));
    }
    !(ip.is_unspecified()
        || first == 0x2002 // 6to4 embeds any IPv4
        || (first == 0x2001 && seg[1] == 0) // Teredo
        || (first == 0x64 && seg[1] == 0xff9b) // local-use NAT64 64:ff9b:1::/48
        || (first & 0xffc0) == 0xfec0 // site local fec0::/10
        || ip.is_loopback()
        || ip.is_multicast()
        || (first & 0xfe00) == 0xfc00 // unique local fc00::/7
        || (first & 0xffc0) == 0xfe80 // link local fe80::/10
        || (first == 0x2001 && ip.segments()[1] == 0x0db8) // documentation
        || ip.segments()[..6] == [0, 0, 0, 0, 0, 0]) // IPv4-compatible ::/96
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_match_the_ip_check() {
        assert_eq!(host_of("http://[::1]:9000/x").as_deref(), Some("::1"));
        assert_eq!(
            host_of("http://Gotenberg:3000").as_deref(),
            Some("gotenberg")
        );
        let out = Outbound::new(&["::1".to_owned()]).unwrap();
        assert!(out.allowed(&reqwest::Url::parse("http://[::1]:9000/").unwrap()));
    }

    #[test]
    fn inward_addresses_are_refused() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.1",
            "::1",
            "::",
            "fc00::1",
            "fd12::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "::ffff:169.254.169.254",
            "::127.0.0.1",
            "64:ff9b::a9fe:a9fe",
            "64:ff9b::10.0.0.1",
            "64:ff9b:1::1",
            "2002:a9fe:a9fe::1",
            "2001:0:4136:e378::1",
            "fec0::1",
        ] {
            assert!(!is_public(ip.parse().unwrap()), "{ip}");
        }
        for ip in [
            "1.1.1.1",
            "8.8.8.8",
            "185.199.108.153",
            "2606:4700::1111",
            "64:ff9b::808:808",
        ] {
            assert!(is_public(ip.parse().unwrap()), "{ip}");
        }
    }

    #[tokio::test]
    async fn an_ip_literal_url_is_checked_before_sending() {
        let out = Outbound::new(&[]).unwrap();
        for url in [
            "http://127.0.0.1:9/",
            "http://169.254.169.254/latest",
            "http://[::1]:9/",
        ] {
            let err = out
                .send(out.request(reqwest::Method::GET, url))
                .await
                .unwrap_err();
            assert!(err.contains("not a public address"), "{url}: {err}");
        }
        let trusted = Outbound::new(&["127.0.0.1".to_owned()]).unwrap();
        let err = trusted
            .send(trusted.request(reqwest::Method::GET, "http://127.0.0.1:9/"))
            .await
            .unwrap_err();
        assert!(err.starts_with("send:"), "trusted literal is sent: {err}");
    }

    #[test]
    fn host_of_reads_configured_urls() {
        assert_eq!(
            host_of("http://Gotenberg:3000/").as_deref(),
            Some("gotenberg")
        );
        assert_eq!(host_of("nope"), None);
    }

    #[tokio::test]
    async fn the_guard_refuses_a_name_that_resolves_inward_unless_trusted() {
        let guarded = Guard {
            trusted: Arc::new(HashSet::new()),
        };
        let name: Name = "localhost".parse().unwrap();
        assert!(guarded.resolve(name).await.is_err());
        let trusting = Guard {
            trusted: Arc::new(HashSet::from(["localhost".to_owned()])),
        };
        let name: Name = "localhost".parse().unwrap();
        assert!(trusting.resolve(name).await.unwrap().next().is_some());
    }
}
