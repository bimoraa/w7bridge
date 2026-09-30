use super::Config;

#[test]
fn defaults_are_closed_and_example_is_valid() {

    let config = Config::parse(include_str!("../../w7bridge.example.toml")).unwrap();
    assert!(config.projects.is_empty());
    assert_eq!(config.execution.concurrency, 1);
    assert_eq!(config.execution.timeout_seconds, 60);
    assert_eq!(config.execution.output_bytes, 65_536);

}

#[test]
fn invalid_versions_unknown_fields_and_unbounded_limits_are_rejected() {

    for source in [
        "version = 2",
        "version = 1\nunknown = true",
        "version = 1\n[execution]\nunknown = 1",
        "version = 1\n[execution]\ntimeout_seconds = 0",
        "version = 1\n[execution]\noutput_bytes = 1048577",
        "version = 1\n[execution]\nconcurrency = 9",
        "version = 1\n[[projects]]\nid = 'sample'\nroot = '/tmp'\nunknown = 1",
        "version = 1\n[[projects]]\nid = 'sample'\nroot = '/tmp'\n[projects.commands.test]\nexecutable = '/tmp/tool'\nunknown = 1",
    ] {

        assert!(Config::parse(source).is_err(), "잘못된 설정을 허용했어: {source}");

    }

}
