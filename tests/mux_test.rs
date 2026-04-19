use stagent::mux::{
    Backend, CmuxTarget, build_cmux_respawn_command, parse_cmux_identify_output,
    parse_cmux_new_split_output, select_backend,
};

#[test]
fn test_select_backend_prefers_cmux_over_tmux() {
    assert_eq!(select_backend(true, true), Backend::Cmux);
}

#[test]
fn test_select_backend_falls_back_to_tmux() {
    assert_eq!(select_backend(false, true), Backend::Tmux);
}

#[test]
fn test_select_backend_falls_back_to_full_screen() {
    assert_eq!(select_backend(false, false), Backend::FullScreen);
}

#[test]
fn test_parse_cmux_identify_output_returns_caller_target() {
    let output = r#"{
  "socket_path" : "/tmp/cmux.sock",
  "caller" : {
    "surface_ref" : "surface:15",
    "workspace_ref" : "workspace:3"
  },
  "focused" : {
    "surface_ref" : "surface:14",
    "workspace_ref" : "workspace:2"
  }
}"#;

    let target = parse_cmux_identify_output(output)
        .expect("json should parse")
        .expect("caller should be present");

    assert_eq!(
        target,
        CmuxTarget {
            workspace_ref: "workspace:3".to_string(),
            surface_ref: "surface:15".to_string(),
        }
    );
}

#[test]
fn test_parse_cmux_identify_output_without_caller_returns_none() {
    let output = r#"{
  "socket_path" : "/tmp/cmux.sock",
  "caller" : null,
  "focused" : {
    "surface_ref" : "surface:14",
    "workspace_ref" : "workspace:2"
  }
}"#;

    let target = parse_cmux_identify_output(output).expect("json should parse");
    assert!(target.is_none(), "caller should be absent");
}

#[test]
fn test_parse_cmux_new_split_output_extracts_refs() {
    let target =
        parse_cmux_new_split_output("OK surface:17 workspace:4").expect("should parse refs");

    assert_eq!(
        target,
        CmuxTarget {
            workspace_ref: "workspace:4".to_string(),
            surface_ref: "surface:17".to_string(),
        }
    );
}

#[test]
fn test_build_cmux_respawn_command_quotes_shell_args() {
    let target = CmuxTarget {
        workspace_ref: "workspace:3".to_string(),
        surface_ref: "surface:15".to_string(),
    };
    let argv = vec![
        "/tmp/stagent bin".to_string(),
        "--output".to_string(),
        "/tmp/review output.diff".to_string(),
    ];

    let cmd = build_cmux_respawn_command(&target, &argv);

    assert_eq!(cmd[0], "cmux");
    assert_eq!(cmd[1], "respawn-pane");
    assert!(cmd.contains(&"--workspace".to_string()));
    assert!(cmd.contains(&"workspace:3".to_string()));
    assert!(cmd.contains(&"--surface".to_string()));
    assert!(cmd.contains(&"surface:15".to_string()));
    assert!(cmd.contains(&"--command".to_string()));
    assert!(
        cmd.last().unwrap().contains("'/tmp/stagent bin'"),
        "command should quote paths with spaces: {:?}",
        cmd
    );
    assert!(
        cmd.last().unwrap().contains("'/tmp/review output.diff'"),
        "command should quote arg with spaces: {:?}",
        cmd
    );
}
