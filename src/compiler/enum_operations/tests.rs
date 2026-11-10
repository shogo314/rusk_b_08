use super::*;
const OPERATIONS: &str = r#"
enum Selection<T>{Empty,Present(Int,T)}
extern fn __coalesce_test__<T>(x:Selection<T>)->Bool{
 operation:"enum_test:1",effects:"read_fresh",
 code:"Obj<@{Bool}> @{fn}(Obj<@{Selection<T>}> x,Loc){return make_obj<@{Bool}>(rusk_enum_index<@{Selection<T>}>(x->value)==1);}",
}
extern fn __coalesce_value__<T>(x:Selection<T>)->T{
 operation:"enum_field:1:1",effects:"checked_variant_field:1:1",return_abi:"object",
 code:"Obj<@{T}> @{fn}(Obj<@{Selection<T>}> x,Loc loc){if(rusk_enum_index<@{Selection<T>}>(x->value)!=1)fail(loc,\"absent selection\");return rusk_enum_payload<@{Selection<T>},1>(x->value).field_1;}",
}
"#;
fn compile(
    standard: &str,
    user: &str,
    emit: crate::backend::Emit,
    optimize: bool,
) -> Result<String, String> {
    Compiler::with_optimization(optimize).compile(
        &[
            crate::stdlib::load()?,
            Source::standard(
                crate::frontend::parse(standard, "unrelated-enum-operations.rusk").unwrap(),
            ),
            Source::user(crate::frontend::parse(user, "enum-operation-use.rusk").unwrap()),
        ],
        emit,
    )
}
#[test]
fn coalesce_enum_operations_use_declared_variant_and_logical_payload_types() {
    let user = r#"
let x:=Selection::Present(99,[1]);let held:=x??[9];held[0]=7;
let selected:Int:=Selection::Empty??4;
let ordinary:Int:=None??5;wt held,selected,ordinary;
"#;
    let hir = compile(OPERATIONS, user, crate::backend::Emit::Hir, true).unwrap();
    let main = hir.split("main: Body").nth(1).unwrap();
    assert!(
        main.contains("VariantTest") && main.contains("index: 1"),
        "{main}"
    );
    assert!(
        main.contains("VariantField") && main.contains("variant: 1"),
        "{main}"
    );
    let dir = std::env::temp_dir().join(format!(
        "rusk-enum-operations-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&dir).unwrap();
    for optimize in [true, false] {
        let cpp = compile(OPERATIONS, user, crate::backend::Emit::Cpp, optimize).unwrap();
        std::fs::write(dir.join("main.cpp"), cpp).unwrap();
        let build = std::process::Command::new("g++")
            .args(["-std=c++20", "-O2"])
            .arg(dir.join("main.cpp"))
            .arg("-o")
            .arg(dir.join("main"))
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "{}",
            String::from_utf8_lossy(&build.stderr)
        );
        let out = std::process::Command::new(dir.join("main"))
            .output()
            .unwrap();
        assert!(out.status.success());
        assert_eq!(out.stdout, b"7 4 5\n");
    }
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn enum_operation_descriptors_reject_forgery_bad_shapes_and_conflicting_contracts() {
    let forged = r#"extern fn spoof<T>(x:Option<T>)->Bool{code:"unused",operation:"enum_test:0",}"#;
    assert!(
        compile("", forged, crate::backend::Emit::Hir, true)
            .unwrap_err()
            .contains("reserved")
    );
    for operation in [
        "enum_test",
        "enum_test:bad",
        "enum_field:0",
        "enum_unknown:0",
    ] {
        let definition =
            format!(r#"extern fn bad<T>(x:T)->Bool{{operation:"{operation}",code:"unused",}}"#);
        assert!(compile(&definition, "", crate::backend::Emit::Hir, true).is_err());
    }
    for (parameter, result, operation, effects, extra) in [
        ("Int", "Bool", "enum_test:0", "read_fresh", ""),
        ("Option<Int>", "Int", "enum_test:0", "read_fresh", ""),
        ("Option<Int>", "Bool", "enum_test:3", "read_fresh", ""),
        (
            "Option<Int>",
            "Bool",
            "enum_test:0",
            "checked_read_fresh",
            "",
        ),
        (
            "Option<Int>",
            "Bool",
            "enum_test:0",
            "read_fresh",
            "aliases:\"return=0\",",
        ),
        (
            "Option<Int>",
            "Bool",
            "enum_field:0:0",
            "checked_variant_field:0:0",
            "",
        ),
        ("Option<Int>", "Int", "enum_field:0:0", "read_fresh", ""),
        (
            "Option<Unit>",
            "Unit",
            "enum_field:0:0",
            "checked_variant_field:0:0",
            "",
        ),
    ] {
        let definition = format!(
            r#"extern fn bad<T>(x:T)->{result}{{operation:"{operation}",effects:"{effects}",{extra}code:"unused",}}extern fn input()->{parameter}{{cpp_name:"input",code:"unused",}}"#
        );
        let error = compile(
            &definition,
            "bad(input());",
            crate::backend::Emit::Hir,
            true,
        )
        .unwrap_err();
        assert!(
            error.contains("enum operation")
                || error.contains("alias type mismatch")
                || error.contains("enum field contract"),
            "{error}"
        );
    }
}
#[test]
fn enum_operation_reification_does_not_skip_referenced_calls() {
    let source = r#"
extern fn __coalesce_test__<T>(x:Selection<T>)->Bool{operation:"enum_test:1",effects:"read_fresh",code:"@{call observe(Selection<T>)->Bool}",}
extern fn __coalesce_value__<T>(x:Selection<T>)->T{operation:"enum_field:1:0",effects:"checked_variant_field:1:0",code:"unused",}
enum Selection<T>{Empty,Present(T)}
"#;
    let user = "extern fn observe(x:Selection<Int>)->Bool{cpp_name:\"observe\",code:\"unused\",}let x:=Selection::Present(3);let y:=x??4;";
    let hir = compile(source, user, crate::backend::Emit::Hir, true).unwrap();
    let main = hir.split("main: Body").nth(1).unwrap();
    assert!(main.contains("Call("), "{main}");
    assert!(!main.contains("VariantTest"), "{main}");
}
