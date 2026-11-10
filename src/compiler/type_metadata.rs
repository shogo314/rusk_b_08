//! Trusted physical and sharing information, independent of display/type names.
use super::*;
use crate::hir::{Scalar, Type};

#[derive(Clone)]
pub(super) struct Metadata {
    pub kind: Type,
    pub span: Span,
}

pub(super) fn references(kind: &Type) -> Vec<&str> {
    let mut children = match kind {
        Type::Buffer(t) | Type::Vector(t) | Type::Set(t) | Type::Iterator { element: t, .. } => {
            vec![t.as_str()]
        }
        Type::Dict { key, value } => vec![key.as_str(), value.as_str()],
        _ => vec![],
    };
    children.extend(kind.retained_fields().iter().map(|(_, ty)| ty.as_str()));
    children
}

pub(super) fn parse_properties(
    name: &Name,
    properties: &BTreeMap<String, String>,
    origin: Origin,
) -> Result<Option<Type>, String> {
    let text = properties.get("representation");
    let retained = properties.get("retains");
    if text.is_none() && retained.is_none() {
        return Ok(None);
    }
    if origin != Origin::StandardLibrary {
        return Err(name
            .span
            .error("native type representations are reserved for the embedded standard library"));
    }
    let retained = retained
        .map(|text| parse_retained(text))
        .transpose()
        .map_err(|e| name.span.error(e))?
        .unwrap_or_default();
    let Some(text) = text else {
        return Ok(Some(Type::Shared(retained)));
    };
    if !retained.is_empty() && !text.starts_with("iterator<") {
        return Err(name
            .span
            .error("native retained members require an iterator or an opaque representation"));
    }
    let scalar = match text.as_str() {
        "i64" => Some(Scalar::I64),
        "bool" => Some(Scalar::Bool),
        "char" => Some(Scalar::Char),
        "unit" => Some(Scalar::Unit),
        _ => None,
    };
    if let Some(s) = scalar {
        return Ok(Some(Type::Scalar(s)));
    }
    let kind = match tuples::type_parts(text) {
        Some((role, args)) => match (role, args.as_slice()) {
            ("buffer", [t]) => Type::Buffer((*t).into()),
            ("vector", [t]) => Type::Vector((*t).into()),
            ("iterator", [t]) => Type::Iterator {
                element: (*t).into(),
                retained,
            },
            ("set", [t]) => Type::Set((*t).into()),
            ("dict", [key, value]) => Type::Dict {
                key: (*key).into(),
                value: (*value).into(),
            },
            _ => return Err(name.span.error("invalid native type representation")),
        },
        None => return Err(name.span.error("invalid native type representation")),
    };
    Ok(Some(kind))
}

fn parse_retained(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut fields = Vec::new();
    for item in text.split(';') {
        let (name, ty) = item
            .split_once(':')
            .ok_or("invalid native retained member")?;
        if !valid_member(name) || ty.is_empty() || fields.iter().any(|(n, _)| n == name) {
            return Err("invalid or duplicate native retained member".into());
        }
        fields.push((name.to_string(), ty.to_string()));
        if fields.len() > 64 {
            return Err("native retained member limit exceeded".into());
        }
    }
    Ok(fields)
}

pub(super) fn valid_member(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub(super) fn substitute(kind: &Type, bindings: &BTreeMap<String, String>) -> Type {
    let ty = |t: &str| generics::substitute(t, bindings);
    match kind {
        Type::Buffer(t) => Type::Buffer(ty(t)),
        Type::Vector(t) => Type::Vector(ty(t)),
        Type::Set(t) => Type::Set(ty(t)),
        Type::Iterator { element, retained } => Type::Iterator {
            element: ty(element),
            retained: retained
                .iter()
                .map(|(name, t)| (name.clone(), ty(t)))
                .collect(),
        },
        Type::Shared(retained) => Type::Shared(
            retained
                .iter()
                .map(|(name, t)| (name.clone(), ty(t)))
                .collect(),
        ),
        Type::Dict { key, value } => Type::Dict {
            key: ty(key),
            value: ty(value),
        },
        _ => kind.clone(),
    }
}

impl Compiler {
    pub(super) fn scalar_representation_checks(&self) -> String {
        let mut checks = String::new();
        for (name, meta) in &self.native_types {
            let Type::Scalar(kind) = meta.kind else {
                continue;
            };
            let native = &self.types[name];
            let plain = format!("!std::is_const_v<{native}> && !std::is_volatile_v<{native}>");
            let condition = match kind {
                Scalar::I64 => format!(
                    "std::is_integral_v<{native}> && std::is_signed_v<{native}> && sizeof({native}) == sizeof(std::int64_t) && {plain}"
                ),
                Scalar::Bool => format!("std::is_same_v<{native},bool>"),
                Scalar::Char => format!("std::is_same_v<{native},char>"),
                Scalar::Unit => {
                    format!("std::is_empty_v<{native}> && std::is_trivial_v<{native}> && {plain}")
                }
            };
            let message =
                serde_json::to_string(&format!("native representation mismatch for {name}"))
                    .unwrap();
            checks += &format!("static_assert({condition},{message});\n");
        }
        if checks.is_empty() {
            checks
        } else {
            format!("#include <type_traits>\n{checks}")
        }
    }
    pub(super) fn validate_representation(
        &mut self,
        name: &Name,
        kind: &Type,
    ) -> Result<(), String> {
        for child in references(kind) {
            self.check_type(&Name {
                text: child.into(),
                span: name.span.clone(),
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const TYPES: &str = r#"
extern type Int {cpp_name:"std::int64_t",representation:"i64",}
extern type Unit {cpp_name:"Unit",representation:"unit",}
extern type Char {cpp_name:"char",representation:"char",}
extern type Number {cpp_name:"std::int64_t",representation:"i64",}
"#;
    fn compile(text: &str, origin: Origin, emit: crate::backend::Emit) -> Result<String, String> {
        let statements = crate::parse(text, "renamed-types.rusk").unwrap();
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
        let mut core =
            crate::parse(include_str!("../../lib/core.rusk"), "renamed-core.rusk").unwrap();
        let mut collections = crate::parse(
            include_str!("../../lib/collections.rusk"),
            "renamed-collections.rusk",
        )
        .unwrap();
        for statement in &mut core {
            if let Stmt::Symbol(_, properties) = statement {
                let code = properties.get_mut("code").unwrap();
                *code = code.replace("/* RUSK_IO_RUNTIME */", include_str!("../../lib/io.hpp"));
                code.push_str(include_str!("../../lib/scalar.hpp"));
            }
        }
        for statement in &mut collections {
            if let Stmt::Symbol(_, properties) = statement {
                properties
                    .get_mut("code")
                    .unwrap()
                    .push_str(include_str!("../../lib/containers.hpp"));
            }
        }
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
                Source::standard(core.clone()),
                Source::standard(collections.clone()),
                Source::standard(
                    crate::parse(
                        include_str!("../../lib/vec.rusk"),
                        "vector-declarations.rusk",
                    )
                    .unwrap(),
                ),
                Source::standard(crate::parse(definitions, "renamed-types.rusk").unwrap()),
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
            crate::parse(
                "extern type Misdeclared {cpp_name:\"std::string\",representation:\"i64\",}",
                "renamed-types.rusk",
            )
            .unwrap(),
        );
        let cpp = Compiler::default()
            .compile(
                &[
                    Source::standard(core),
                    Source::standard(collections),
                    Source::standard(
                        crate::parse(
                            include_str!("../../lib/vec.rusk"),
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
            hir.contains(
                "\"Cursor<Envelope<Number>>\": Iterator {\n            element: \"Number\""
            ),
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
        let error =
            compile(&source, Origin::StandardLibrary, crate::backend::Emit::Cpp).unwrap_err();
        assert!(error.contains("unknown type 'Missing'"), "{error}");
    }
}

#[cfg(test)]
mod retained_tests {
    use super::*;
    const TYPES: &str = r#"
extern type Int {cpp_name:"std::int64_t",representation:"i64",}
extern type Unit {cpp_name:"Unit",representation:"unit",}
"#;
    fn compile(definitions: &str, user: &str) -> Result<String, String> {
        Compiler::default().compile(
            &[
                Source::standard(
                    crate::parse(&format!("{TYPES}{definitions}"), "shared-definitions.rusk")
                        .unwrap(),
                ),
                Source::user(crate::parse(user, "shared-use.rusk").unwrap()),
            ],
            crate::backend::Emit::Lir,
        )
    }
    const WRAPPER: &str = r#"
extern type<T> Capsule {cpp_name:"std::shared_ptr<{T}>",retains:"value:T",}
extern fn capsule<T>(x:T)->Capsule<T>{code:"unused",effects:"read_fresh",aliases:"return.value=0",}
extern fn unwrap<T>(x:Capsule<T>)->T{code:"unused",effects:"read_fresh",aliases:"return=0.value",}
extern fn same<T>(x:T)->T{code:"unused",effects:"read_fresh",aliases:"return=0",}
extern fn replace(x:Int,y:Int){cpp_name:"replace",code:"unused",effects:"write_first",}
"#;
    #[test]
    fn native_shared_members_follow_declarations_and_flow_through_rusk_functions() {
        let hir = compile(WRAPPER, "let x:=capsule(3);let y:=unwrap(x);").unwrap();
        assert!(hir.contains("Shared("), "{hir}");
        assert!(hir.contains("Retained("), "{hir}");
        compile(
            WRAPPER,
            "fn keep(readonly noescape x:Int){let boxed:=capsule(x);let y:=unwrap(boxed);}keep(3);",
        )
        .unwrap();
        for user in [
            "fn bad(readonly x:Int){let held:=capsule(x);replace(unwrap(held),3);}bad(3);",
            "fn relay<T>(x:Capsule<T>)->Capsule<T>{return same(x);}fn bad(readonly x:Int){replace(unwrap(relay(capsule(x))),3);}bad(3);",
            "fn leak(noescape x:Int)->Capsule<Int>{return capsule(x);}leak(3);",
            "fn relay<T>(x:Capsule<T>)->T{return unwrap(x);}fn leak(noescape x:Int)->Int{return relay(capsule(x));}leak(3);",
        ] {
            let error = compile(WRAPPER, user).unwrap_err();
            assert!(
                error.contains("readonly") || error.contains("noescape"),
                "{error}"
            );
        }
    }
    #[test]
    fn retained_and_alias_annotations_reject_invalid_shapes_and_untrusted_origins() {
        for definitions in [
            "extern type<T> Bad{cpp_name:\"int\",retains:\"value:Missing\",}",
            "extern type<T> Bad{cpp_name:\"int\",retains:\"value:T;value:T\",}",
            "extern type<T> Bad{cpp_name:\"int\",retains:\"value:\",}",
            "extern type Bad{cpp_name:\"int\",representation:\"i64\",retains:\"value:Int\",}",
            "extern fn bad<T>(x:T)->T{code:\"unused\",aliases:\"return=0\",}",
            "extern fn bad<T>(x:T)->T{code:\"unused\",effects:\"read_fresh\",aliases:\"return=1\",}",
        ] {
            let error = compile(definitions, "").unwrap_err();
            assert!(error.contains("shared-definitions.rusk:"), "{error}");
        }
        for user in [
            "extern type<T> Spoof{cpp_name:\"int\",retains:\"value:T\",}",
            "extern type Spoof{cpp_name:\"int\",retains:\"value:Int\",}",
            "extern fn spoof<T>(x:T)->T{code:\"unused\",aliases:\"return=0\",effects:\"read_fresh\",}",
            "extern fn spoof(x:Int)->Int{cpp_name:\"spoof\",code:\"unused\",aliases:\"return=0\",}",
        ] {
            let error = compile("", user).unwrap_err();
            assert!(error.contains("reserved"), "{error}");
        }
        for aliases in ["return.missing=0", "return.value=0.value"] {
            let definitions = format!(
                r#"extern type<T> Capsule{{cpp_name:"std::shared_ptr<{{T}}>",retains:"value:T",}}extern fn make<T>(x:T)->Capsule<T>{{code:"unused",effects:"read_fresh",aliases:"{aliases}",}}"#
            );
            let error = compile(&definitions, "make(3);").unwrap_err();
            assert!(error.contains("native alias member"), "{error}");
        }
    }
}
