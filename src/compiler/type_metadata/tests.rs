use super::*;
const TYPES: &str = r#"
extern type Int {cpp_name:"std::int64_t",representation:"i64",}
extern type Unit {cpp_name:"Unit",representation:"unit",}
extern type Char {cpp_name:"char",representation:"char",}
extern type Number {cpp_name:"std::int64_t",representation:"i64",}
"#;
fn compile(text: &str, origin: Origin, emit: crate::backend::Emit) -> Result<String, String> {
    let statements = crate::frontend::parse(text, "renamed-types.rusk").unwrap();
    Compiler::default().compile(&[Source { statements, origin }], emit)
}
#[test]
fn inline_storage_and_assignment_follow_representation_instead_of_type_names() {
    let definitions = r#"
extern type Number {cpp_name:"std::int64_t",representation:"i64",}
extern fn number(x:Int)->Number {cpp_name:"number",code:"Obj<std::int64_t> number(Obj<std::int64_t> x,Loc){return make_obj<std::int64_t>(*x);}",lower:"*$0",effects:"read_fresh",}
extern fn __assign__(x:Number,y:Number) {cpp_name:"store_number",code:"void store_number(Obj<std::int64_t> x,Obj<std::int64_t> y,Loc){*x=*y;}",lower:"*$0 = *$1",operation:"assign",effects:"write_first",}
extern fn __write__(x:Number) {cpp_name:"print_number",code:"void print_number(Obj<std::int64_t> x,Loc){rusk_write(*x);}",lower:"rusk_write(*$0)",effects:"io_write",}
let x:=number(3);let alias:=x;alias=number(9);wt x;
"#;
    let core = crate::frontend::parse(include_str!("../../../lib/core.rusk"), "renamed-core.rusk")
        .unwrap();
    let collections = crate::frontend::parse(
        include_str!("../../../lib/collections.rusk"),
        "renamed-collections.rusk",
    )
    .unwrap();
    let temp = std::env::temp_dir().join(format!(
        "rusk-native-representation-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&temp).unwrap();
    for optimize in [true, false] {
        let sources = [
            Source::standard(crate::stdlib::runtime()),
            Source::standard(core.clone()),
            Source::standard(collections.clone()),
            Source::standard(
                crate::frontend::parse(
                    include_str!("../../../lib/vec.rusk"),
                    "vector-declarations.rusk",
                )
                .unwrap(),
            ),
            Source::standard(crate::frontend::parse(definitions, "renamed-types.rusk").unwrap()),
        ];
        let cpp = Compiler::with_optimization(optimize)
            .compile(&sources, crate::backend::Emit::Cpp)
            .unwrap();
        let main = cpp.split("int main()").nth(1).unwrap();
        if optimize {
            assert!(main.contains("RuskInlineScalar<std::int64_t>"), "{main}");
            assert!(
                !main.contains("make_obj<std::int64_t>")
                    && !main.contains("store_number(")
                    && !main.contains("print_number("),
                "{main}"
            );
        }
        let source = temp.join("main.cpp");
        let executable = temp.join("main");
        std::fs::write(&source, cpp).unwrap();
        let build = std::process::Command::new("g++")
            .args(["-std=c++20", "-O2"])
            .arg(&source)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "{}",
            String::from_utf8_lossy(&build.stderr)
        );
        let run = std::process::Command::new(&executable).output().unwrap();
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(run.stdout, b"9\n");
    }
    let invalid = Source::standard(
        crate::frontend::parse(
            "extern type Misdeclared {cpp_name:\"std::string\",representation:\"i64\",}",
            "renamed-types.rusk",
        )
        .unwrap(),
    );
    let cpp = Compiler::default()
        .compile(
            &[
                Source::standard(crate::stdlib::runtime()),
                Source::standard(core),
                Source::standard(collections),
                Source::standard(
                    crate::frontend::parse(
                        include_str!("../../../lib/vec.rusk"),
                        "vector-declarations.rusk",
                    )
                    .unwrap(),
                ),
                invalid,
            ],
            crate::backend::Emit::Cpp,
        )
        .unwrap();
    let source = temp.join("invalid.cpp");
    std::fs::write(&source, cpp).unwrap();
    let build = std::process::Command::new("g++")
        .args(["-std=c++20", "-c"])
        .arg(&source)
        .arg("-o")
        .arg(temp.join("invalid.o"))
        .output()
        .unwrap();
    assert!(!build.status.success());
    assert!(
        String::from_utf8_lossy(&build.stderr)
            .contains("native representation mismatch for Misdeclared")
    );
    std::fs::remove_dir_all(temp).unwrap();
}
#[test]
fn buffer_projection_and_generic_shape_references_preserve_logical_type_identity() {
    let definitions = r#"
extern type Text {cpp_name:"std::string",representation:"buffer<Glyph>",}
extern type Glyph {cpp_name:"char",representation:"char",}
extern type<vector> Bucket {cpp_name:"std::vector<std::shared_ptr<{vector}>>",representation:"vector<vector>",}
extern fn bucket()->Bucket<Number> {cpp_name:"make_bucket",code:"unused",}
extern fn fetch(x:Text,index:Number)->Glyph {cpp_name:"fetch_glyph",code:"unused",effects:"element",}
let values:=bucket();
"#;
    let hir = compile(
        &format!("{TYPES}{definitions}"),
        Origin::StandardLibrary,
        crate::backend::Emit::Hir,
    )
    .unwrap();
    assert!(
        hir.contains("\"Text\": Buffer(\n            \"Glyph\""),
        "{hir}"
    );
    assert!(
        hir.contains("\"Bucket<Number>\": Vector(\n            \"Number\""),
        "{hir}"
    );
    let error=compile(&format!("{TYPES} extern fn number()->Number{{cpp_name:\"number\",code:\"unused\",}} let x:=number();x:=1;"),Origin::StandardLibrary,crate::backend::Emit::Cpp).unwrap_err();
    assert!(error.contains("Number") && error.contains("Int"), "{error}");
}
#[test]
fn patterned_iterator_shapes_follow_declarations_with_unrelated_names() {
    let definitions = r#"
extern type<T> Envelope {cpp_name:"std::shared_ptr<{T}>",}
extern type<T> Cursor<Envelope<T>> {cpp_name:"std::shared_ptr<{T}>",representation:"iterator<T>",}
extern fn cursor()->Cursor<Envelope<Number>> {cpp_name:"cursor",code:"unused",}
let iterator:=cursor();
"#;
    let hir = compile(
        &format!("{TYPES}{definitions}"),
        Origin::StandardLibrary,
        crate::backend::Emit::Hir,
    )
    .unwrap();
    assert!(
        hir.contains("\"Cursor<Envelope<Number>>\": Iterator {\n            element: \"Number\""),
        "{hir}"
    );
    let error = compile(definitions, Origin::User, crate::backend::Emit::Hir).unwrap_err();
    assert!(
        error.contains("native type representations are reserved"),
        "{error}"
    );
}
#[test]
fn untrusted_and_invalid_representations_are_rejected_even_when_unused() {
    for declaration in [
        "extern type Hidden {cpp_name:\"int\",representation:\"i64\",}",
        "extern type<T> Hidden {cpp_name:\"int\",representation:\"i64\",}",
    ] {
        let error = compile(declaration, Origin::User, crate::backend::Emit::Cpp).unwrap_err();
        assert!(
            error.contains("type representations are reserved"),
            "{error}"
        );
    }
    for representation in ["unknown", "vector<Int,Int>", "buffer<Missing>"] {
        let source = format!(
            "{TYPES} extern type Hidden {{cpp_name:\"int\",representation:\"{representation}\",}}"
        );
        let error =
            compile(&source, Origin::StandardLibrary, crate::backend::Emit::Cpp).unwrap_err();
        assert!(
            error.contains("renamed-types.rusk:")
                && (error.contains("invalid native type representation")
                    || error.contains("unknown type")),
            "{error}"
        );
    }
    let source = format!(
        "{TYPES} extern type<T> Hidden {{cpp_name:\"int\",representation:\"buffer<Missing>\",}}"
    );
    let error = compile(&source, Origin::StandardLibrary, crate::backend::Emit::Cpp).unwrap_err();
    assert!(error.contains("unknown type 'Missing'"), "{error}");
}
