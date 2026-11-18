use hickory_resolver::TokioResolver;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use std::{io, net::SocketAddr, time::Duration};

/// Async system DNS only: no unbounded getaddrinfo task or fallback public server.
pub(super) struct SystemDns {
    pub(super) resolver: TokioResolver,
    pub(super) budget: Duration,
}
impl SystemDns {
    pub(super) fn new(budget: Duration) -> io::Result<Self> {
        let resolver = TokioResolver::builder_tokio()
            .map_err(|_| io::Error::other("System DNS configuration is unavailable"))?
            .build()
            .map_err(|_| io::Error::other("System DNS resolver initialization failed"))?;
        Ok(Self { resolver, budget })
    }
    pub(super) async fn lookup(&self, host: &str) -> io::Result<Vec<std::net::IpAddr>> {
        let lookup = tokio::time::timeout(self.budget, self.resolver.lookup_ip(host))
            .await
            .map_err(|_| {
                io::Error::new(io::ErrorKind::TimedOut, "DNS resolution deadline exceeded")
            })?
            .map_err(|_| io::Error::other("DNS resolution failed"))?;
        Ok(lookup.iter().collect())
    }
}
impl Resolve for SystemDns {
    fn resolve(&self, name: Name) -> Resolving {
        let resolver = Self {
            resolver: self.resolver.clone(),
            budget: self.budget,
        };
        Box::pin(async move {
            let addresses = resolver
                .lookup(name.as_str())
                .await?
                .into_iter()
                .map(|ip| SocketAddr::new(ip, 0))
                .collect::<Vec<_>>();
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}
