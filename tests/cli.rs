mod support;
use std::{fs, process::Command};
use support::Case;

fn cli(case: &Case, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rusk"))
        .current_dir(&case.0)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn help_and_version_succeed_without_reading_a_source_file() {
    let case = Case::new("not a valid program");
    for (args, expected) in [
        (vec!["--help"], "dependencies"),
        (vec!["-h"], "compile"),
        (vec!["compile", "--help"], "--emit"),
        (vec!["dependencies", "--help"], "<FILE>"),
        (vec!["help", "compile"], "--no-opt"),
    ] {
        let output = cli(&case, &args);
        assert!(output.status.success(), "{args:?}: {output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("Usage:") && text.contains(expected), "{text}");
    }
    let output = cli(&case, &["--version"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
        output.stdout,
        format!("rusk {}\n", env!("CARGO_PKG_VERSION")).as_bytes()
    );
    assert!(!case.0.join("a.cpp").exists());
}

#[test]
fn argument_errors_exit_with_two_and_do_not_generate_output() {
    let case = Case::new("wt 7;");
    for args in [
        vec![],
        vec!["unknown"],
        vec!["compile"],
        vec!["dependencies"],
        vec!["compile", "main.rusk", "--emit", "unknown"],
        vec!["compile", "main.rusk", "--emit"],
        vec!["compile", "main.rusk", "--output"],
        vec!["compile", "main.rusk", "--unknown"],
        vec!["compile", "main.rusk", "extra.rusk"],
        vec!["dependencies", "main.rusk", "--no-opt"],
        vec!["dependencies", "main.rusk", "extra.rusk"],
    ] {
        let output = cli(&case, &args);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
        assert!(output.stdout.is_empty(), "{args:?}: {output:?}");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("Usage:") || error.contains("--help"),
            "{args:?}: {error}"
        );
        assert!(!case.0.join("a.cpp").exists(), "{args:?}");
    }
}

#[test]
fn default_output_and_optimization_flags_keep_the_existing_compilation() {
    let case = Case::new("let value:=3;let alias:=value;value+=4;wt value,alias;");
    let output = case.run("");
    assert!(output.status.success());
    assert_eq!(output.stdout, b"7 7\n");
    let expected = fs::read(case.0.join("main.cpp")).unwrap();
    let output = cli(&case, &["compile", "main.rusk"]);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
    assert_eq!(fs::read(case.0.join("a.cpp")).unwrap(), expected);

    assert!(case.compile_with_options(&["--no-opt"]).status.success());
    let expected = fs::read(case.0.join("main.cpp")).unwrap();
    let output = cli(
        &case,
        &[
            "compile",
            "main.rusk",
            "--emit",
            "hir",
            "--emit=cpp",
            "-o",
            "unused.cpp",
            "--output=reference.cpp",
            "--no-opt",
            "--no-opt",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(fs::read(case.0.join("reference.cpp")).unwrap(), expected);
    assert!(!case.0.join("unused.cpp").exists());
}

#[test]
fn emission_stages_and_output_paths_work_with_options_before_the_input() {
    let case = Case::new("wt 1;");
    for stage in ["cst", "ast", "hir", "lir", "cpp"] {
        let output = cli(
            &case,
            &["compile", "main.rusk", "--emit", stage, "-o", "expected"],
        );
        assert!(output.status.success(), "{stage}: {output:?}");
        let expected = fs::read(case.0.join("expected")).unwrap();
        let emit = format!("--emit={stage}");
        let output = cli(
            &case,
            &["compile", &emit, "--output", "actual output", "main.rusk"],
        );
        assert!(output.status.success(), "{stage}: {output:?}");
        assert!(output.stdout.is_empty() && output.stderr.is_empty());
        assert!(!expected.is_empty());
        assert_eq!(fs::read(case.0.join("actual output")).unwrap(), expected);
    }
    assert!(!case.0.join("a.cpp").exists());
}

#[test]
fn dependencies_list_files_without_type_checking_or_generating_cpp() {
    let case = Case::new("include \"helper.rusk\";wt missing;");
    fs::write(
        case.0.join("helper.rusk"),
        "fn twice(x:Int)->Int{return x*2;}",
    )
    .unwrap();
    let output = cli(&case, &["dependencies", "main.rusk"]);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).unwrap();
    let mut actual: Vec<_> = text.lines().map(std::path::PathBuf::from).collect();
    actual.sort();
    let mut expected = vec![
        fs::canonicalize(case.0.join("main.rusk")).unwrap(),
        fs::canonicalize(case.0.join("helper.rusk")).unwrap(),
    ];
    expected.sort();
    assert_eq!(actual, expected);
    assert!(!case.0.join("a.cpp").exists());
}

#[test]
fn source_and_io_errors_still_exit_with_one() {
    let case = Case::new("let value:Int;wt value;");
    for (args, message) in [
        (vec!["compile", "main.rusk"], "used before initialization"),
        (vec!["compile", "missing.rusk"], "missing.rusk"),
        (vec!["dependencies", "missing.rusk"], "missing.rusk"),
    ] {
        let output = cli(&case, &args);
        assert_eq!(output.status.code(), Some(1), "{args:?}: {output:?}");
        assert!(output.stdout.is_empty());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains(message), "{args:?}: {error}");
        assert!(!error.contains("Usage:"), "{args:?}: {error}");
        assert!(!case.0.join("a.cpp").exists());
    }
}

#[test]
fn dash_prefixed_files_can_be_passed_after_the_option_separator() {
    let case = Case::new("wt 7;");
    fs::rename(case.0.join("main.rusk"), case.0.join("-source.rusk")).unwrap();
    let output = cli(&case, &["compile", "--", "-source.rusk"]);
    assert!(output.status.success(), "{output:?}");
    assert!(case.0.join("a.cpp").exists());
}

#[cfg(unix)]
#[test]
fn file_paths_need_not_be_utf8() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let case = Case::new("wt 7;");
    let file = OsString::from_vec(b"source-\xff.rusk".to_vec());
    fs::rename(case.0.join("main.rusk"), case.0.join(&file)).unwrap();
    for stage in ["cst", "ast", "cpp"] {
        let output = Command::new(env!("CARGO_BIN_EXE_rusk"))
            .current_dir(&case.0)
            .args(["compile", "--emit", stage])
            .arg(&file)
            .output()
            .unwrap();
        assert!(output.status.success(), "{stage}: {output:?}");
        assert!(!fs::read(case.0.join("a.cpp")).unwrap().is_empty());
    }
}
