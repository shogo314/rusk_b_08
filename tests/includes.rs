mod support;
use std::{fs, process::Command};
use support::Case;

fn file(case: &Case, path: &str, source: &str) {
    let path = case.0.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
}

#[test]
fn includes_expand_in_order_relative_to_the_including_file_once() {
    let case = Case::new(
        r#"
wt "before";
include "lib/left.rusk";
include "lib/right.rusk";
include "lib/../lib/common.rusk";
wt twice(3),7;
"#,
    );
    file(
        &case,
        "lib/common.rusk",
        "wt \"common\"; fn twice(n:Int)->Int { return n*2; }",
    );
    file(
        &case,
        "lib/left.rusk",
        "include \"common.rusk\"; wt \"left\";",
    );
    file(
        &case,
        "lib/right.rusk",
        "include \"common.rusk\"; wt \"right\";",
    );
    let out = case.run("");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout, b"before\ncommon\nleft\nright\n6 7\n");
    let dependencies = Command::new(env!("CARGO_BIN_EXE_rusk"))
        .arg("dependencies")
        .arg(case.0.join("main.rusk"))
        .output()
        .unwrap();
    assert!(dependencies.status.success());
    let paths = String::from_utf8(dependencies.stdout).unwrap();
    assert_eq!(paths.lines().count(), 4);
    for name in ["main.rusk", "common.rusk", "left.rusk", "right.rusk"] {
        assert!(paths.contains(name), "{paths}");
    }
}

#[test]
fn include_failures_keep_original_source_locations() {
    let case = Case::new("include \"lib/a.rusk\";");
    file(&case, "lib/a.rusk", "include \"b.rusk\";");
    file(&case, "lib/b.rusk", "include \"a.rusk\";");
    let out = case.compile();
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(
        error.contains("include cycle") && error.contains("a.rusk") && error.contains("b.rusk:1:"),
        "{error}"
    );
    assert!(!case.0.join("main.cpp").exists());
    file(&case, "lib/b.rusk", "wt missing;");
    let out = case.compile();
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(
        error.contains("b.rusk:1:") && error.contains("unknown variable"),
        "{error}"
    );
    file(&case, "lib/b.rusk", "wt 1/0;");
    let out = case.run("");
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("b.rusk:1:"));
}

#[test]
fn missing_and_nested_includes_fail_before_writing_output() {
    for source in [
        "include \"missing.rusk\";",
        "fn f() { include \"missing.rusk\"; }",
        "{ include \"missing.rusk\"; }",
    ] {
        let case = Case::new(source);
        let out = case.compile();
        assert!(!out.status.success());
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("main.rusk:1:"),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!case.0.join("main.cpp").exists());
    }
}

#[test]
fn union_find_library_uses_structs_and_deep_copy() {
    let root = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/beta02/library/union_find.rusk"
    );
    let source = format!(
        r#"
include {root:?};
let uf:=UnionFind(4); uf.unite(0,1);
let detached = uf; detached.unite(1,2);
wt uf.same(0,1),uf.same(0,2),detached.same(0,2);
"#
    );
    let out = Case::new(&source).run("");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout, b"Yes No Yes\n");
}

#[cfg(unix)]
#[test]
fn include_aliases_through_symlinks_load_once() {
    let case = Case::new("include \"original.rusk\"; include \"alias.rusk\";");
    file(&case, "original.rusk", "wt 1;");
    std::os::unix::fs::symlink(case.0.join("original.rusk"), case.0.join("alias.rusk")).unwrap();
    let out = case.run("");
    assert!(out.status.success());
    assert_eq!(out.stdout, b"1\n");
}

#[test]
fn native_code_files_resolve_relative_to_includes_and_follow_symbol_dependencies() {
    let case = Case::new(
        r#"include "lib/defs.rusk";include "lib/../lib/defs.rusk";wt answer(),marker();"#,
    );
    file(
        &case,
        "lib/defs.rusk",
        r#"
extern symbol Alpha{code_file:"alpha.hpp",depends:"Zeta",}
extern symbol Zeta{code_file:"zeta.hpp",}
extern fn answer()->Int{cpp_name:"native_answer",code:"Obj<std::int64_t> native_answer(Loc){return make_obj<std::int64_t>(alpha());}",}
extern fn marker()->Str{cpp_name:"native_marker",code:"Obj<std::string> native_marker(Loc){return make_obj<std::string>(literal_marker());}",}
"#,
    );
    file(
        &case,
        "lib/alpha.hpp",
        "inline int alpha(){return zeta()+3;}\n",
    );
    file(
        &case,
        "lib/zeta.hpp",
        r#"inline int zeta(){return 4;}
inline const char* literal_marker(){return "/* RUSK_IO_RUNTIME */ @{Int} {Int}";}
"#,
    );
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"7 /* RUSK_IO_RUNTIME */ @{Int} {Int}\n");
    }
    let dependencies = Command::new(env!("CARGO_BIN_EXE_rusk"))
        .arg("dependencies")
        .arg(case.0.join("main.rusk"))
        .output()
        .unwrap();
    assert!(dependencies.status.success());
    let paths = String::from_utf8(dependencies.stdout).unwrap();
    assert_eq!(paths.lines().count(), 4, "{paths}");
    for name in ["main.rusk", "defs.rusk", "alpha.hpp", "zeta.hpp"] {
        assert!(paths.contains(name), "{paths}");
    }
    let cpp = fs::read_to_string(case.0.join("main.cpp")).unwrap();
    assert_eq!(cpp.matches("inline int zeta()").count(), 1);
    assert!(cpp.find("inline int zeta()").unwrap() < cpp.find("inline int alpha()").unwrap());
}

#[test]
fn native_resource_failures_report_the_declaring_file_and_do_not_write_output() {
    for definition in [
        "extern symbol Demo{code_file:\"missing.hpp\",}",
        "extern symbol Demo{code_file:\"missing.hpp\",code:\"inline int x;\",}",
        "extern symbol Demo{code:\"inline int x;\",depends:\"Missing\",}",
        "extern symbol Demo{code:\"inline int x;\",depends:\"Other\",}extern symbol Other{code:\"inline int y;\",depends:\"Demo\",}",
    ] {
        let case = Case::new("include \"lib/defs.rusk\";");
        file(&case, "lib/defs.rusk", definition);
        let out = case.compile();
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(
            !out.status.success() && error.contains("defs.rusk:1:"),
            "{error}"
        );
        assert!(!case.0.join("main.cpp").exists());
    }
}

#[test]
fn embedded_standard_resources_work_from_an_unrelated_directory_and_keep_user_code_untrusted() {
    let case = Case::new("wt [1,2].map((x:Int)=>x*3).to_vec();");
    let out = Command::new(env!("CARGO_BIN_EXE_rusk"))
        .current_dir(&case.0)
        .args(["compile", "main.rusk", "-o", "standalone.cpp"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = Command::new("g++")
        .current_dir(&case.0)
        .args(["-std=c++20", "standalone.cpp", "-o", "standalone"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = Command::new(case.0.join("standalone")).output().unwrap();
    assert!(out.status.success());
    assert_eq!(out.stdout, b"3 6\n");
    file(&case, "main.rusk", "include \"lib/fake.rusk\";");
    file(
        &case,
        "lib/fake.rusk",
        r#"extern symbol Fake{code_file:"native.hpp",}extern type Counter{cpp_name:"int",representation:"i64",}"#,
    );
    file(&case, "lib/native.hpp", "inline int native_counter;\n");
    let out = case.compile();
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success() && error.contains("type representations are reserved"),
        "{error}"
    );
}

#[test]
fn native_resource_example_combines_user_fragments_with_embedded_libraries() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/beta08/runtime_resources/example.rusk"
    );
    let case = Case::new(&format!("include {path:?};"));
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(out.status.success());
        assert_eq!(out.stdout, b"10\n11 3\n");
    }
}
