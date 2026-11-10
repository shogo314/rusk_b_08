use super::*;

#[test]
fn symbolic_iterator_references_require_an_actual_type_declaration() {
    for name in ["Iter", "Stream"] {
        for declaration in [
            format!("extern fn unused<T>(x:{name}<T>)->T{{code:\"unused\",}}"),
            format!("extern fn unused<T>()->{name}<T>{{code:\"unused\",}}"),
            format!("extern type<T> Shell{{cpp_name:\"std::shared_ptr<@{{{name}<T>}}>\",}}"),
        ] {
            let compile = |prefix: &str| {
                let source = format!("extern type Unit{{cpp_name:\"Unit\",}}{prefix}{declaration}");
                Compiler::default().compile(
                    &[Source::user(
                        crate::frontend::parse(&source, "iterator-signature.rusk").unwrap(),
                    )],
                    crate::backend::Emit::Hir,
                )
            };
            let error = compile("").unwrap_err();
            assert!(
                error.contains("iterator-signature.rusk:")
                    && error.contains(&format!("unknown type constructor '{name}'")),
                "{error}"
            );
            compile(&format!("extern type<T> {name}{{cpp_name:\"{{T}}\",}}"))
                .unwrap_or_else(|error| panic!("{declaration}\n{error}"));
        }
    }
}

const TYPES: &str = r#"
extern type Int {cpp_name:"std::int64_t",representation:"i64",}
extern type Unit {cpp_name:"Unit",representation:"unit",}
extern fn copy(x:Int)->Int {cpp_name:"field_copy",code:"Obj<std::int64_t> field_copy(Obj<std::int64_t> x,Loc){return make_obj<std::int64_t>(*x);}",}
"#;
const DERIVE: &str = r#"
extern fn unrelated_template_name<Element>(value:Element)->Element {
    derive:"record_copy",
    code:"/* source_derivation */ Obj<@{Element}> @{fn}(const Obj<@{Element}>& value,Loc loc){return make_obj<@{Element}>(@{Element}{@{fields Element as Member,Position}@{boxed_call copy(Member)->Member}(value->field_@{index Position},loc),@{end}});}",
}
"#;

#[test]
fn record_copy_derivation_uses_the_declared_role_and_source_body() {
    let standard = Source::standard(
        crate::frontend::parse(&format!("{TYPES}{DERIVE}"), "arbitrary-standard-name.rusk")
            .unwrap(),
    );
    let user = Source::user(
        crate::frontend::parse(
            "struct Row{a:Int;b:Int;}let a:=Row{a:1,b:2};let b=a;",
            "caller.rusk",
        )
        .unwrap(),
    );
    let cpp = Compiler::default()
        .compile(&[standard, user], crate::backend::Emit::Cpp)
        .unwrap();
    assert!(cpp.contains("/* source_derivation */ Obj<RuskRecord0> deep_copy"));
    assert!(cpp.contains("field_copy(value->field_0,loc),field_copy(value->field_1,loc)"));
    assert!(!cpp.contains("unrelated_template_name("));
}

#[test]
fn record_copy_derivations_validate_trust_shape_duplicates_and_unused_references() {
    for (definition, standard, expected) in [
        (
            DERIVE.to_string(),
            false,
            "reserved for the embedded standard library",
        ),
        (
            DERIVE.replace("record_copy", "unknown_role"),
            true,
            "invalid native copy derivation",
        ),
        (
            DERIVE.replace("->Element", "->Unit"),
            true,
            "invalid native copy derivation",
        ),
        (
            DERIVE.replace("<Element>", "<Element,Other>"),
            true,
            "invalid native copy derivation",
        ),
        (
            format!("{DERIVE}{DERIVE}"),
            true,
            "duplicate native derivation",
        ),
        (
            DERIVE.replace("copy(Member)->Member", "copy(Missing)->Member"),
            true,
            "unknown type",
        ),
    ] {
        let types = Source::standard(crate::frontend::parse(TYPES, "types.rusk").unwrap());
        let statements = crate::frontend::parse(&definition, "<stdlib/collections.rusk>").unwrap();
        let source = if standard {
            Source::standard(statements)
        } else {
            Source::user(statements)
        };
        let error = Compiler::default()
            .compile(&[types, source], crate::backend::Emit::Cpp)
            .unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
}
#[test]
fn enum_copy_derivation_uses_source_provenance_and_a_name_independent_role() {
    let mut declarations=crate::frontend::parse(include_str!("../../../lib/collections.rusk"),"other-library.rusk").unwrap().into_iter()
            .filter(|s|matches!(s,Stmt::GenericExternFunction(_,_,_,_,p) if p.get("derive").is_some_and(|kind|kind == "enum_copy"))).collect::<Vec<_>>();
    for statement in &mut declarations {
        if let Stmt::GenericExternFunction(name, _, _, _, properties) = statement {
            name.text = "renamed_sum_implementation".into();
            properties
                .get_mut("code")
                .unwrap()
                .insert_str(0, "/* source_sum_derivation */ ");
        }
    }
    let user = Source::user(
        crate::frontend::parse(
            "enum Event{Empty,Present(Int),}let a:=Event::Present(1);let b=a;",
            "caller.rusk",
        )
        .unwrap(),
    );
    let types = Source::standard(crate::frontend::parse(TYPES, "types.rusk").unwrap());
    let cpp = Compiler::default()
        .compile(
            &[types, Source::standard(declarations), user],
            crate::backend::Emit::Cpp,
        )
        .unwrap();
    assert!(cpp.contains("/* source_sum_derivation */ Obj<RuskEnum0> deep_copy"));
    assert!(cpp.contains("field_copy(rusk_enum_payload<RuskEnum0,1>(snapshot).field_0,loc)"));
    assert!(!cpp.contains("renamed_sum_implementation("));
}
#[test]
fn aggregate_assignment_roles_use_source_bodies_and_adapter_ordering() {
    for (role, source) in [
        (
            "record_assign",
            "struct Row{a:Int;b:Int;}let x:=Row{a:1,b:2};let y:=Row{a:3,b:4};x=y;",
        ),
        (
            "enum_assign",
            "enum E{Empty,Value(Int),}let x:=E::Empty;let y:=E::Value(3);x=y;",
        ),
    ] {
        let mut declarations = crate::frontend::parse(
                include_str!("../../../lib/collections.rusk"),
                "other-library.rusk",
            )
            .unwrap()
            .into_iter()
            .filter(
                |s| matches!(s,Stmt::GenericExternFunction(_,_,_,_,p) if p.get("derive").is_some_and(|kind| kind == role || matches!(kind.as_str(), "record_copy" | "enum_copy"))),
            )
            .collect::<Vec<_>>();
        for statement in &mut declarations {
            if let Stmt::GenericExternFunction(name, _, _, _, properties) = statement
                && properties["derive"] == role
            {
                name.text = "unrelated_assignment_name".into();
                properties
                    .get_mut("code")
                    .unwrap()
                    .insert_str(0, "/* source_assignment */ ");
            }
        }
        let cpp = Compiler::default()
            .compile(
                &[
                    Source::standard(crate::frontend::parse(TYPES, "types.rusk").unwrap()),
                    Source::standard(declarations),
                    Source::user(crate::frontend::parse(source, "caller.rusk").unwrap()),
                ],
                crate::backend::Emit::Cpp,
            )
            .unwrap();
        let body = cpp.find("/* source_assignment */ void ").unwrap();
        let adapter = cpp.find(" rusk_rooted_ref_").unwrap();
        assert!(adapter < body);
        assert!(cpp[body..].contains("auto snapshot=rusk_rooted_ref_"));
        assert!(!cpp.contains("unrelated_assignment_name("));
    }
}

#[test]
fn assignment_derivations_reject_invalid_signatures_and_untrusted_origins() {
    let definition = r#"extern fn custom_assignment<T>(target:T,source:T)->Unit{derive:"record_assign",code:"unused",}"#;
    for (source, standard, expected) in [
        (
            definition.to_string(),
            false,
            "reserved for the embedded standard library",
        ),
        (
            definition.replace("record_assign", "enum_assign"),
            false,
            "reserved for the embedded standard library",
        ),
        (
            definition.replace("source:T", "source:Int"),
            true,
            "invalid native assignment derivation",
        ),
        (
            definition.replace("->Unit", "->T"),
            true,
            "invalid native assignment derivation",
        ),
        (
            definition.replace("<T>", "<T,R>"),
            true,
            "invalid native assignment derivation",
        ),
        (
            format!("{definition}{definition}"),
            true,
            "duplicate native derivation",
        ),
    ] {
        let declarations = crate::frontend::parse(&source, "<stdlib/collections.rusk>").unwrap();
        let source = if standard {
            Source::standard(declarations)
        } else {
            Source::user(declarations)
        };
        let error = Compiler::default()
            .compile(
                &[
                    Source::standard(crate::frontend::parse(TYPES, "types.rusk").unwrap()),
                    source,
                ],
                crate::backend::Emit::Cpp,
            )
            .unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
}
#[test]
fn tuple_input_derivations_validate_role_shape_and_source_trust() {
    let definition =
        r#"extern fn custom_product_reader<T>()->T{derive:"tuple_read",code:"unused",}"#;
    for (source, standard, expected) in [
        (
            definition.to_string(),
            false,
            "reserved for the embedded standard library",
        ),
        (
            definition.replace("()->T", "(x:T)->T"),
            true,
            "invalid native input derivation",
        ),
        (
            definition.replace("()->T", "()->Unit"),
            true,
            "invalid native input derivation",
        ),
        (
            format!("{definition}{definition}"),
            true,
            "duplicate native derivation",
        ),
    ] {
        let statements = crate::frontend::parse(&source, "<stdlib/collections.rusk>").unwrap();
        let declarations = if standard {
            Source::standard(statements)
        } else {
            Source::user(statements)
        };
        let error = Compiler::default()
            .compile(
                &[
                    Source::standard(crate::frontend::parse(TYPES, "types.rusk").unwrap()),
                    declarations,
                ],
                crate::backend::Emit::Cpp,
            )
            .unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
}
#[test]
fn tuple_input_role_uses_the_standard_template_with_an_unrelated_name() {
    let mut declarations = crate::frontend::parse(
            include_str!("../../../lib/collections.rusk"),
            "renamed-library.rusk",
        )
        .unwrap()
        .into_iter()
        .filter(|s| matches!(s,Stmt::GenericExternFunction(_,_,_,_,p) if p.contains_key("derive") && p["derive"] != "vector_read"))
        .collect::<Vec<_>>();
    for statement in &mut declarations {
        if let Stmt::GenericExternFunction(name, _, _, _, properties) = statement
            && properties["derive"] == "tuple_read"
        {
            name.text = "renamed_product_reader".into();
            properties
                .get_mut("code")
                .unwrap()
                .insert_str(0, "/* input_source */ ");
        }
    }
    let types = format!(
        r#"{TYPES}
extern type Bool{{cpp_name:"bool",representation:"bool",}}
extern fn __read__()->Int{{cpp_name:"input_field",code:"Obj<std::int64_t> input_field(Loc){{return make_obj<std::int64_t>(1);}}",}}
"#
    );
    let cpp = Compiler::default()
        .compile(
            &[
                Source::standard(crate::frontend::parse(&types, "types.rusk").unwrap()),
                Source::standard(declarations),
                Source::user(crate::frontend::parse("in row:(Int,Int);", "caller.rusk").unwrap()),
            ],
            crate::backend::Emit::Cpp,
        )
        .unwrap();
    assert!(cpp.contains("/* input_source */ Obj<RuskRecord0>"));
    assert!(cpp.contains("auto value_0=input_field(loc);auto value_1=input_field(loc);"));
    assert!(!cpp.contains("renamed_product_reader("));
}
#[test]
fn dimensions_input_roles_validate_trust_and_bind_the_result_pattern() {
    let definition = r#"extern fn arbitrary_reader<T>()->Box<T>{derive:"vector_read",code:"@{arguments from 1 as P,I}@{end}",}"#;
    let types = format!(r#"{TYPES} extern type<T> Box{{cpp_name:"std::shared_ptr<{{T}}>",}}"#);
    for (source, standard, expected) in [
        (
            definition.to_string(),
            false,
            Some("reserved for the embedded standard library"),
        ),
        (definition.to_string(), true, None),
        (
            definition.replace("()->Box<T>", "(x:T)->Box<T>"),
            true,
            Some("invalid native input derivation"),
        ),
        (
            definition.replace("()->Box<T>", "()->Int"),
            true,
            Some("invalid native input derivation"),
        ),
        (
            definition.replace("<T>()", "<T,R>()"),
            true,
            Some("invalid native input derivation"),
        ),
        (
            format!("{definition}{definition}"),
            true,
            Some("duplicate native derivation"),
        ),
        (
            definition.replace("as P,I", "as T,I"),
            true,
            Some("shadows"),
        ),
        (
            definition.replace("Box<T>", "Missing<T>"),
            true,
            Some("unknown type constructor"),
        ),
    ] {
        let statements = crate::frontend::parse(&source, "<stdlib/collections.rusk>").unwrap();
        let declarations = if standard {
            Source::standard(statements)
        } else {
            Source::user(statements)
        };
        let result = Compiler::default().compile(
            &[
                Source::standard(crate::frontend::parse(&types, "types.rusk").unwrap()),
                declarations,
            ],
            crate::backend::Emit::Cpp,
        );
        if let Some(expected) = expected {
            let error = result.unwrap_err();
            assert!(error.contains(expected), "{error}");
        } else {
            result.unwrap();
        }
    }
}

#[test]
fn dimensions_input_role_uses_the_source_body_under_an_unrelated_name() {
    let mut declarations = crate::frontend::parse(include_str!("../../../lib/collections.rusk"), "other-standard.rusk")
            .unwrap().into_iter().filter(|s| matches!(s,Stmt::GenericExternFunction(_,_,_,_,p) if p.get("derive").is_some_and(|v| v == "vector_read"))).collect::<Vec<_>>();
    for statement in &mut declarations {
        if let Stmt::GenericExternFunction(name, _, _, _, properties) = statement {
            name.text = "unrelated_dimensions_name".into();
            properties
                .get_mut("code")
                .unwrap()
                .insert_str(0, "/* dimensions_source */ ");
        }
    }
    let types = format!(
        r#"{TYPES}
extern type<T> Vec{{cpp_name:"RuskVec<{{T}}>",representation:"vector<T>",}}
extern fn __read__()->Int{{cpp_name:"field_input",code:"Obj<std::int64_t> field_input(Loc){{return make_obj<std::int64_t>(1);}}",}}
"#
    );
    let cpp = Compiler::default()
        .compile(
            &[
                Source::standard(crate::frontend::parse(&types, "types.rusk").unwrap()),
                Source::standard(declarations),
                Source::user(
                    crate::frontend::parse("in rows(2,3):Vec<Vec<Int>>;", "caller.rusk").unwrap(),
                ),
            ],
            crate::backend::Emit::Cpp,
        )
        .unwrap();
    assert_eq!(
        cpp.matches("/* dimensions_source */ Obj<RuskVec<").count(),
        2
    );
    assert!(cpp.contains("Obj<std::int64_t> d0,Obj<std::int64_t> d1,Loc loc"));
    assert!(cpp.contains("field_input(loc)"));
    assert!(!cpp.contains("unrelated_dimensions_name("));
}
#[test]
fn tuple_output_roles_validate_shape_trust_and_duplicate_definitions() {
    let definition =
        r#"extern fn arbitrary_output<T>(value:T)->Unit{derive:"tuple_write",code:"unused",}"#;
    for (source, standard, expected) in [
        (
            definition.to_string(),
            false,
            "reserved for the embedded standard library",
        ),
        (
            definition.replace("(value:T)", "()"),
            true,
            "invalid native output derivation",
        ),
        (
            definition.replace("->Unit", "->T"),
            true,
            "invalid native output derivation",
        ),
        (
            definition.replace("<T>", "<T,R>"),
            true,
            "invalid native output derivation",
        ),
        (
            definition.replace("<T>(value:T)", "<Unit>(value:Unit)"),
            true,
            "invalid native output derivation",
        ),
        (
            format!("{definition}{definition}"),
            true,
            "duplicate native derivation",
        ),
    ] {
        let statements = crate::frontend::parse(&source, "<stdlib/collections.rusk>").unwrap();
        let source = if standard {
            Source::standard(statements)
        } else {
            Source::user(statements)
        };
        let error = Compiler::default()
            .compile(
                &[
                    Source::standard(crate::frontend::parse(TYPES, "types.rusk").unwrap()),
                    source,
                ],
                crate::backend::Emit::Cpp,
            )
            .unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn tuple_output_uses_named_source_templates_for_body_lowering_and_effects() {
    let mut declarations=crate::frontend::parse(include_str!("../../../lib/collections.rusk"),"renamed-output-library.rusk").unwrap().into_iter().filter(|s| matches!(s,Stmt::GenericExternFunction(_,_,_,_,p) if p.get("derive").is_some_and(|v|v=="tuple_write"))).collect::<Vec<_>>();
    for statement in &mut declarations {
        if let Stmt::GenericExternFunction(name, _, _, _, properties) = statement {
            name.text = "unrelated_output_name".into();
            properties
                .get_mut("code")
                .unwrap()
                .insert_str(0, "/* output_source */ ");
            properties
                .get_mut("lower")
                .unwrap()
                .insert_str(0, "/* output_lower */ ");
        }
    }
    let types = format!(
        r#"{TYPES}
extern type Bool{{cpp_name:"bool",representation:"bool",}}
extern fn __write__(value:Int){{cpp_name:"field_output",effects:"io_write",code:"void field_output(Obj<std::int64_t>,Loc){{}}",}}
"#
    );
    let sources = [
        Source::standard(crate::frontend::parse(&types, "types.rusk").unwrap()),
        Source::standard(declarations),
        Source::user(crate::frontend::parse("__write__((1,2));", "caller.rusk").unwrap()),
    ];
    let cpp = Compiler::default()
        .compile(&sources, crate::backend::Emit::Cpp)
        .unwrap();
    assert!(cpp.contains("/* output_source */ void rusk_algorithm_"));
    assert!(cpp.contains("/* output_lower */"));
    assert!(!cpp.contains("unrelated_output_name("));
    let hir = Compiler::default()
        .compile(&sources, crate::backend::Emit::Hir)
        .unwrap();
    let output = hir
        .split("name: \"__write__\"")
        .nth(2)
        .unwrap()
        .split("name:")
        .next()
        .unwrap();
    assert!(output.contains("source: NativeContract"), "{hir}");
}
#[test]
fn tuple_comparison_roles_validate_signatures_and_trust() {
    for role in [
        "tuple_eq", "tuple_ne", "tuple_lt", "tuple_le", "tuple_gt", "tuple_ge",
    ] {
        let definition = format!(
            r#"extern fn arbitrary_compare<T>(a:T,b:T)->Bool{{derive:"{role}",code:"unused",}}"#
        );
        for (source, standard, expected) in [
            (
                definition.clone(),
                false,
                "reserved for the embedded standard library",
            ),
            (
                definition.replace("a:T,b:T", "a:T"),
                true,
                "invalid native comparison derivation",
            ),
            (
                definition.replace("->Bool", "->T"),
                true,
                "invalid native comparison derivation",
            ),
            (
                definition.replace("<T>", "<T,R>"),
                true,
                "invalid native comparison derivation",
            ),
            (
                definition.replace("<T>(a:T,b:T)", "<Bool>(a:Bool,b:Bool)"),
                true,
                "invalid native comparison derivation",
            ),
            (
                format!("{definition}{definition}"),
                true,
                "duplicate native derivation",
            ),
        ] {
            let statements = crate::frontend::parse(&source, "<stdlib/collections.rusk>").unwrap();
            let source = if standard {
                Source::standard(statements)
            } else {
                Source::user(statements)
            };
            let error = Compiler::default()
                .compile(
                    &[
                        Source::standard(crate::frontend::parse(TYPES, "types.rusk").unwrap()),
                        source,
                    ],
                    crate::backend::Emit::Cpp,
                )
                .unwrap_err();
            assert!(error.contains(expected), "{role}: {error}");
        }
    }
}

#[test]
fn tuple_comparison_bodies_lowering_and_contracts_follow_source_roles() {
    let mut declarations=crate::frontend::parse(include_str!("../../../lib/collections.rusk"),"renamed-compare-library.rusk").unwrap().into_iter().filter(|s| matches!(s,Stmt::GenericExternFunction(_,_,_,_,p) if p.get("derive").is_some_and(|v|v=="tuple_eq"))).collect::<Vec<_>>();
    for statement in &mut declarations {
        if let Stmt::GenericExternFunction(name, _, _, _, properties) = statement {
            name.text = "unrelated_equal_name".into();
            properties
                .get_mut("code")
                .unwrap()
                .insert_str(0, "/* equal_source */ ");
            properties
                .get_mut("lower")
                .unwrap()
                .insert_str(0, "/* equal_lower */ ");
        }
    }
    for (contract, expected) in [
        (r#",effects:"read_fresh""#, "NativeContract"),
        ("", "Opaque"),
    ] {
        let types = format!(
            r#"{TYPES}
extern type Bool{{cpp_name:"bool",representation:"bool",}}
extern fn __eq__(a:Int,b:Int)->Bool{{cpp_name:"field_equal",code:"Obj<bool> field_equal(Obj<std::int64_t> a,Obj<std::int64_t> b,Loc){{return make_obj<bool>(*a==*b);}}"{contract},}}
"#
        );
        let sources = [
            Source::standard(crate::frontend::parse(&types, "types.rusk").unwrap()),
            Source::standard(declarations.clone()),
            Source::user(crate::frontend::parse("__eq__((1,2),(1,3));", "caller.rusk").unwrap()),
        ];
        let cpp = Compiler::default()
            .compile(&sources, crate::backend::Emit::Cpp)
            .unwrap();
        assert!(cpp.contains("/* equal_source */ Obj<bool> rusk_algorithm_"));
        assert!(cpp.contains("/* equal_lower */"));
        assert!(!cpp.contains("unrelated_equal_name("));
        let hir = Compiler::default()
            .compile(&sources, crate::backend::Emit::Hir)
            .unwrap();
        let section = hir
            .split("name: \"__eq__\"")
            .nth(2)
            .unwrap()
            .split("name:")
            .next()
            .unwrap();
        assert!(section.contains(&format!("source: {expected}")), "{hir}");
    }
}
#[test]
fn tuple_predeclarations_follow_trusted_representations_instead_of_scalar_names() {
    let mut declarations=crate::frontend::parse(include_str!("../../../lib/collections.rusk"),"renamed-policy.rusk").unwrap().into_iter().filter(|s| matches!(s,Stmt::GenericExternFunction(_,_,_,_,p) if p.get("derive").is_some_and(|v|v=="tuple_eq"))).collect::<Vec<_>>();
    for statement in &mut declarations {
        if let Stmt::GenericExternFunction(_, _, _, _, properties) = statement {
            properties
                .get_mut("code")
                .unwrap()
                .insert_str(0, "/* representation_selected */ ");
        }
    }
    let types = format!(
        r#"{TYPES}
extern type Bool{{cpp_name:"bool",representation:"bool",}}
extern type Number{{cpp_name:"std::int64_t",representation:"i64",}}
extern fn __eq__(a:Number,b:Number)->Bool{{cpp_name:"number_equal",effects:"read_fresh",code:"unused",}}
"#
    );
    for (ty, expected) in [("Number", true), ("Score", false)] {
        let user = format!(
            r#"
extern type Score{{cpp_name:"std::int64_t",}}
extern fn value()->{ty}{{cpp_name:"get_value",code:"unused",}}
fn __eq__<T>(a:(T,T),b:(T,T))->Bool{{return false;}}
let x:=value();__eq__((x,x),(x,x));
"#
        );
        let cpp = Compiler::default()
            .compile(
                &[
                    Source::standard(crate::frontend::parse(&types, "types.rusk").unwrap()),
                    Source::standard(declarations.clone()),
                    Source::user(crate::frontend::parse(&user, "caller.rusk").unwrap()),
                ],
                crate::backend::Emit::Cpp,
            )
            .unwrap();
        assert_eq!(
            cpp.contains("/* representation_selected */"),
            expected,
            "{ty}"
        );
    }
}
