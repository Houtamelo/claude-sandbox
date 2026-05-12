//! GPU vendor flag composition. Doesn't exercise probe() against a real
//! /sys filesystem — that's by design (depends on the host) and the
//! function is one syscall thick.

use claude_sandbox::features::gpu::{flags, GpuVendor};

#[test]
fn project_disabled_yields_empty_even_with_vendor() {
    assert!(flags(GpuVendor::Nvidia, &[], false).is_empty());
    assert!(flags(GpuVendor::Amd, &["--device".into(), "/dev/foo".into()], false).is_empty());
}

#[test]
fn none_vendor_returns_only_extra_args() {
    let extra = vec!["--device".into(), "/dev/dri/renderD129".into()];
    let out = flags(GpuVendor::None, &extra, true);
    assert_eq!(out, extra);
}

#[test]
fn custom_vendor_returns_only_extra_args() {
    // The whole point of `custom`: skip built-ins, use the escape hatch.
    let extra = vec!["--security-opt".into(), "label=type:foo".into()];
    let out = flags(GpuVendor::Custom, &extra, true);
    assert_eq!(out, extra);
}

#[test]
fn nvidia_emits_cdi_spec() {
    let out = flags(GpuVendor::Nvidia, &[], true);
    assert!(out.windows(2).any(|w| w == ["--device", "nvidia.com/gpu=all"]),
            "missing NVIDIA CDI flags: {out:?}");
}

#[test]
fn amd_emits_dri_kfd_and_keep_groups() {
    let out = flags(GpuVendor::Amd, &[], true);
    assert!(out.windows(2).any(|w| w == ["--device", "/dev/dri"]));
    assert!(out.windows(2).any(|w| w == ["--device", "/dev/kfd"]));
    assert!(out.windows(2).any(|w| w == ["--group-add", "keep-groups"]),
            "AMD must pass host's render/video groups: {out:?}");
}

#[test]
fn intel_emits_dri_no_kfd() {
    let out = flags(GpuVendor::Intel, &[], true);
    assert!(out.windows(2).any(|w| w == ["--device", "/dev/dri"]));
    assert!(!out.windows(2).any(|w| w == ["--device", "/dev/kfd"]),
            "Intel iGPU shouldn't include /dev/kfd (AMD compute): {out:?}");
    assert!(out.windows(2).any(|w| w == ["--group-add", "keep-groups"]));
}

#[test]
fn extra_args_are_appended_in_all_variants() {
    // Per requirement: extra_args must be supported in ALL variants,
    // including the canonical recipes. The escape hatch needs to be
    // universal so users can layer on driver-specific quirks without
    // having to disable the built-in.
    let extra = vec!["--ipc".into(), "host".into()];
    for v in [GpuVendor::Nvidia, GpuVendor::Amd, GpuVendor::Intel,
              GpuVendor::Custom, GpuVendor::None] {
        let out = flags(v, &extra, true);
        assert!(out.ends_with(&extra),
                "{v:?} should end with extra_args; got {out:?}");
    }
}

#[test]
fn parse_accepts_canonical_lowercase() {
    assert_eq!(GpuVendor::parse("nvidia"), Some(GpuVendor::Nvidia));
    assert_eq!(GpuVendor::parse("amd"), Some(GpuVendor::Amd));
    assert_eq!(GpuVendor::parse("intel"), Some(GpuVendor::Intel));
    assert_eq!(GpuVendor::parse("none"), Some(GpuVendor::None));
    assert_eq!(GpuVendor::parse("custom"), Some(GpuVendor::Custom));
}

#[test]
fn parse_is_case_insensitive_and_trims() {
    assert_eq!(GpuVendor::parse("  NVIDIA  "), Some(GpuVendor::Nvidia));
    assert_eq!(GpuVendor::parse("Amd"), Some(GpuVendor::Amd));
}

#[test]
fn parse_returns_none_for_garbage() {
    assert_eq!(GpuVendor::parse("Apple"), None);
    assert_eq!(GpuVendor::parse("nvidiax"), None);
}

#[test]
fn parse_empty_string_means_none_vendor() {
    // "" → None lets users hit enter past the prompt with no default to
    // explicitly mean "no GPU". The wizard provides a non-empty default
    // so this path is only hit on explicit empty input.
    assert_eq!(GpuVendor::parse(""), Some(GpuVendor::None));
}
