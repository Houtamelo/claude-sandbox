pub fn extra_args(enabled: bool) -> Vec<String> {
    if enabled {
        vec!["--device".into(), "nvidia.com/gpu=all".into()]
    } else {
        vec![]
    }
}
