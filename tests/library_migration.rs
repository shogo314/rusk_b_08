mod support;
use support::Case;

#[test]
fn binary_search_is_an_ordinary_declaration_and_works_under_an_unrelated_name() {
    let declaration =
        include_str!("../lib/algorithms.rusk").replace("first_true", "boundary_search");
    let case = Case::new(&format!(
        r#"{declaration}
let threshold:=40;let predicate:=(x:Int)=>x*x>=threshold;
wt first_true(0,100,predicate),boundary_search(0,100,predicate);
wt boundary_search(-9223372036854775808,9223372036854775807,(x:Int)=>x>=0);
let calls:=0;wt boundary_search(3,3,(x:Int)=>{{calls+=1;true}}),calls;
fn first_true(a:Int,b:Int,flag:Bool)->Int{{return if flag{{a}}else{{b}};}}
wt first_true(2,9,false);
"#
    ));
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
        assert_eq!(out.stdout, b"7 7\n0\n3 0\n9\n");
    }
}

#[test]
fn library_search_preserves_empty_interval_failures_and_prior_output() {
    let case = Case::new("wt \"before\";first_true(5,2,(x:Int)=>true);");
    let mut previous = None;
    for options in [vec![], vec!["--no-opt"]] {
        let out = case.run_with_options("", &options);
        let error = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(!out.status.success());
        assert_eq!(out.stdout, b"before\n");
        assert!(
            error.contains("main.rusk:1:") && error.contains("invalid binary search interval"),
            "{error}"
        );
        if let Some(previous) = previous {
            assert_eq!(previous, error);
        }
        previous = Some(error);
    }
}
