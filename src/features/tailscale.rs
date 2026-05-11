use crate::config::TailscaleSpec;

pub fn on_start_commands(spec: &TailscaleSpec, container_name: &str) -> Vec<String> {
    if !spec.enabled {
        return Vec::new();
    }
    let hostname = spec.hostname.clone().unwrap_or_else(|| container_name.to_string());
    vec![
        "pidof tailscaled >/dev/null || \
         (tailscaled --tun=userspace-networking --statedir=/var/lib/tailscale > /var/log/tailscaled.log 2>&1 &)".into(),
        format!(
            "tailscale up --authkey=\"${{{authkey}}}\" --hostname=\"{hostname}\" --accept-dns=false --accept-routes=false || true",
            authkey = spec.authkey_env
        ),
    ]
}

pub fn passthrough_env(spec: &TailscaleSpec) -> Vec<String> {
    if spec.enabled {
        vec![spec.authkey_env.clone()]
    } else {
        vec![]
    }
}
