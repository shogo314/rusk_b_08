mod support;
use support::Case;

const APPLY: &str = r#"
extern fn apply<F,T,R>(f:F,x:T)->R {
    code:"@{return R} @{fn}(Obj<@{F}> f,Obj<@{T}> x,Loc loc){return @{invoke F(T)->R}(f,x,loc);}",
}
extern fn apply0<F,R>(f:F)->R {
    code:"@{return R} @{fn}(Obj<@{F}> f,Loc loc){return @{invoke F()->R}(f,loc);}",
}
"#;

fn equivalent(source: &str, expected: &str) {
    equivalent_program(&format!("{APPLY}\n{source}"), expected);
}

fn equivalent_program(source: &str, expected: &str) {
    let case = Case::new(source);
    for options in [vec![], vec!["--no-opt"]] {
        let out = if options.is_empty() {
            case.run("")
        } else {
            case.run_with_options("", &options)
        };
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, expected.as_bytes());
    }
}

#[test]
fn native_callbacks_infer_signatures_and_preserve_capture_sharing_and_copying() {
    equivalent(
        r#"
fn square(x:Int)->Int{return x*x;}
let counter:=1;let f:=(n:Int)=>(counter+=n);let shared:=f;let detached=f;
wt apply(f,2),apply(shared,3),counter,apply(detached,10),counter;
counter:=100;wt apply(f,1),counter,apply(square,3);
wt apply((n:Int)=>"ok",1),apply((xs:Vec<Int>)=>xs.len(),[]);
let total:=0;let action:=()=>{total+=1;};apply0(action);wt total;
"#,
        "3 6 6 11 6\n7 100 9\nok 0\n1\n",
    );
}

#[test]
fn externally_held_callbacks_keep_captured_character_owners_alive() {
    equivalent_program(
        include_str!("../examples/beta08/extern_callback_lifetime.rusk"),
        "Z b\n",
    );
}

#[test]
fn callbacks_can_infer_other_callback_types_without_marker_order_dependence() {
    equivalent(
        r#"
extern fn nested<F,G,R>(f:F)->R {
    code:"@{return R} @{fn}(Obj<@{F}> f,Loc loc){auto run=@{invoke G()->R};auto g=@{invoke F()->G}(f,loc);return run(g,loc);}",
}
let n:=4;wt nested(()=>()=>n);
"#,
        "4\n",
    );
}

#[test]
fn callback_signature_mismatches_and_invalid_markers_fail_before_output() {
    for source in [
        "apply((x:Str)=>x,1);",
        "apply(1,2);",
        "apply0((x:Int)=>x);",
        "let result:Int:=apply((x:Int)=>\"no\",1);",
        r#"extern fn bad<F>(f:F){code:"@{invoke F(Missing)->Int}",}"#,
        r#"extern fn bad<F>(f:F){code:"@{return Vec<Int,Str>}",}"#,
    ] {
        let case = Case::new(&format!("{APPLY}\n{source}"));
        let out = case.compile();
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{source}");
        assert!(
            error.contains("main.rusk:") && !error.contains("syntax error"),
            "{error}"
        );
        assert!(!case.0.join("main.cpp").exists());
    }
}

#[test]
fn callbacks_do_not_grant_native_effects_or_erase_capture_restrictions() {
    for (source, expected) in [
        (
            "readonly let x:=1;let f:=()=>x;apply0(f);",
            "cannot prove readonly",
        ),
        (
            "fn check(noescape x:Int){let f:=()=>x;apply0(f);}check(1);",
            "cannot prove noescape",
        ),
    ] {
        let out = Case::new(&format!("{APPLY}\n{source}")).compile();
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{source}");
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn failures_inside_callbacks_preserve_prior_output_and_source_locations() {
    let case = Case::new(&format!(
        "{APPLY}\nlet values:=[1];wt \"before\";apply((n:Int)=>values[n],2);"
    ));
    let mut previous = None;
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        assert!(!out.status.success());
        assert_eq!(out.stdout, b"before\n");
        let error = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(
            error.contains("main.rusk:") && error.contains("index"),
            "{error}"
        );
        if let Some(previous) = previous {
            assert_eq!(previous, error);
        }
        previous = Some(error);
    }
}
