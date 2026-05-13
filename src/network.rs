use std::net::TcpListener;

use crate::error::{Error, Result};
use crate::podman::args::PortMapping;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortRequest {
    pub host: Option<u16>,
    pub container: u16,
    pub strict: bool,
}

pub fn parse(spec: &str) -> Result<PortRequest> {
    let (strict, body) = if let Some(rest) = spec.strip_prefix('!') {
        (true, rest)
    } else {
        (false, spec)
    };
    let (lhs, rhs) = body.split_once(':').ok_or_else(|| {
        Error::Config(format!("port spec '{spec}' missing colon"))
    })?;
    let host = if lhs.is_empty() {
        None
    } else {
        Some(lhs.parse().map_err(|_| Error::Config(format!("bad host port in '{spec}'")))?)
    };
    let container = rhs
        .parse()
        .map_err(|_| Error::Config(format!("bad container port in '{spec}'")))?;
    Ok(PortRequest {
        host,
        container,
        strict,
    })
}

pub fn resolve(reqs: &[PortRequest]) -> Result<Vec<PortMapping>> {
    let mut out = Vec::new();
    let mut taken = std::collections::HashSet::new();
    for r in reqs {
        let host = pick_host_port(r, &taken)?;
        taken.insert(host);
        out.push(PortMapping {
            host,
            container: r.container,
        });
    }
    Ok(out)
}

fn pick_host_port(r: &PortRequest, taken: &std::collections::HashSet<u16>) -> Result<u16> {
    match r.host {
        None => {
            // ephemeral
            let l = TcpListener::bind("127.0.0.1:0")
                .map_err(|e| Error::Other(format!("ephemeral port bind: {e}")))?;
            let p = l.local_addr().unwrap().port();
            drop(l);
            Ok(p)
        }
        Some(p) => {
            if r.strict {
                if !port_free(p) || taken.contains(&p) {
                    return Err(Error::Other(format!("port {p} unavailable (strict)")));
                }
                return Ok(p);
            }
            for delta in 0..=20u16 {
                let candidate = p.saturating_add(delta);
                if candidate == 0 {
                    break;
                }
                if !taken.contains(&candidate) && port_free(candidate) {
                    return Ok(candidate);
                }
            }
            // Fallback: ephemeral
            let l = TcpListener::bind("127.0.0.1:0")?;
            Ok(l.local_addr().unwrap().port())
        }
    }
}

fn port_free(p: u16) -> bool {
    TcpListener::bind(("127.0.0.1", p)).is_ok()
}
