//! GPU passthrough.
//!
//! Vendor lives in `machine.toml` (a host property — the user's GPU
//! doesn't change per-project). Per-project `.claude-sandbox.toml`
//! carries a `gpu: bool` toggle ("this project wants GPU access");
//! when true, we emit the vendor's flags plus any user-defined
//! escape-hatch args.

use serde::{Deserialize, Serialize};

/// Which GPU vendor's flags to emit when a project sets `gpu = true`.
/// `Custom` is an explicit "use only my extra_args, no built-in flags"
/// for drivers we don't recognise. `None` is the default for hosts
/// without a usable GPU (or users who don't want passthrough).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GpuVendor {
    None,
    Nvidia,
    Amd,
    Intel,
    Custom,
}

impl Default for GpuVendor {
    fn default() -> Self { Self::None }
}

impl GpuVendor {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Nvidia => "nvidia",
            Self::Amd => "amd",
            Self::Intel => "intel",
            Self::Custom => "custom",
        }
    }

    /// Parse from a free-form string (used by the cfg wizard prompt).
    /// Returns None for unrecognised input so the wizard can re-prompt
    /// with a list rather than silently accepting nonsense.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" | "" => Some(Self::None),
            "nvidia" => Some(Self::Nvidia),
            "amd" => Some(Self::Amd),
            "intel" => Some(Self::Intel),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }
}

/// Best-guess vendor for the running host. Used as the wizard default;
/// the user can override. Detection order matches "discrete-card-first":
///   1. NVIDIA driver loaded         → Nvidia
///   2. AMD compute device exists    → Amd  (KFD = AMD's compute driver)
///   3. PCI vendor ID of /sys/class/drm/card0
///   4. Otherwise                    → None
pub fn probe() -> GpuVendor {
    if std::path::Path::new("/proc/driver/nvidia/version").exists() {
        return GpuVendor::Nvidia;
    }
    if std::path::Path::new("/dev/kfd").exists() {
        return GpuVendor::Amd;
    }
    // /sys/class/drm/card0/device/vendor reads as `0x10de\n` for NVIDIA,
    // `0x1002\n` for AMD, `0x8086\n` for Intel. The card0 is usually the
    // primary GPU; on multi-GPU hosts, this picks one and the user can
    // override if they need a specific one.
    if let Ok(raw) = std::fs::read_to_string("/sys/class/drm/card0/device/vendor") {
        match raw.trim() {
            "0x10de" => return GpuVendor::Nvidia,
            "0x1002" => return GpuVendor::Amd,
            "0x8086" => return GpuVendor::Intel,
            _ => {}
        }
    }
    GpuVendor::None
}

/// Built-in podman flags for a vendor. Concatenated with the user's
/// extra_args in [`flags`]; this is just the canonical-recipe layer.
fn builtin_flags(vendor: GpuVendor) -> Vec<String> {
    match vendor {
        GpuVendor::None | GpuVendor::Custom => Vec::new(),
        // Standard NVIDIA CDI spec emitted by nvidia-container-toolkit.
        // Requires the toolkit to be installed on the host.
        GpuVendor::Nvidia => vec![
            "--device".into(),
            "nvidia.com/gpu=all".into(),
        ],
        // AMD: DRM render nodes + KFD (compute). `keep-groups` passes
        // the host user's supplementary groups (typically `render` and
        // `video`) into the container so device-file permissions match.
        GpuVendor::Amd => vec![
            "--device".into(), "/dev/dri".into(),
            "--device".into(), "/dev/kfd".into(),
            "--group-add".into(), "keep-groups".into(),
        ],
        // Intel: just DRM render nodes (i915). No compute device.
        GpuVendor::Intel => vec![
            "--device".into(), "/dev/dri".into(),
            "--group-add".into(), "keep-groups".into(),
        ],
    }
}

/// Compose the full GPU arg list. When `project_enabled = false`,
/// returns empty regardless of vendor — the per-project toggle is the
/// final say. Otherwise: built-in flags for the vendor, then the user's
/// `extra_args` appended verbatim (for driver-specific escape hatches
/// the canonical recipes don't cover).
pub fn flags(vendor: GpuVendor, extra_args: &[String], project_enabled: bool) -> Vec<String> {
    if !project_enabled {
        return Vec::new();
    }
    let mut v = builtin_flags(vendor);
    v.extend(extra_args.iter().cloned());
    v
}
