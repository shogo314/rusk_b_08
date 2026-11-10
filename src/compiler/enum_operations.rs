//! Trusted, nominal-name-independent meanings of closed enum operations.
use super::*;

#[derive(Clone, Copy)]
pub(super) enum Operation {
    Test(usize),
    Field { variant: usize, field: usize },
}

pub(super) fn parse(descriptor: &str) -> Result<Operation, String> {
    let number = |s: &str| {
        s.parse::<usize>()
            .map_err(|_| "invalid native enum operation descriptor".to_string())
    };
    match descriptor.split(':').collect::<Vec<_>>().as_slice() {
        ["enum_test", variant] => Ok(Operation::Test(number(variant)?)),
        ["enum_field", variant, field] => Ok(Operation::Field {
            variant: number(variant)?,
            field: number(field)?,
        }),
        _ => Err("invalid native enum operation descriptor".into()),
    }
}

impl Compiler {
    pub(super) fn register_enum_operation(
        &mut self,
        index: usize,
        name: &Name,
        descriptor: &str,
        params: &[String],
        result: &str,
        properties: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        let invalid = || name.span.error("invalid native enum operation descriptor");
        if params.len() != 1 {
            return Err(invalid());
        }
        let types = self.ir_types();
        let Some(crate::ir::hir::Type::Enum(variants)) = types.get(&params[0]) else {
            return Err(invalid());
        };
        if properties.contains_key("aliases") {
            return Err(invalid());
        }
        let operation = parse(descriptor).map_err(|_| invalid())?;
        match operation {
            Operation::Test(variant) => {
                if result != "Bool"
                    || variant >= variants.len()
                    || properties.get("effects").is_none_or(|s| s != "read_fresh")
                {
                    return Err(invalid());
                }
            }
            Operation::Field { variant, field } => {
                if variants
                    .get(variant)
                    .and_then(|(_, fields)| fields.get(field))
                    .is_none_or(|ty| ty != result)
                    || properties.get("effects")
                        != Some(&format!("checked_variant_field:{variant}:{field}"))
                    || (result == "Unit"
                        && properties.get("return_abi").is_none_or(|s| s != "object"))
                {
                    return Err(invalid());
                }
            }
        }
        self.native_enum_operations.insert(index, operation);
        Ok(())
    }

    pub(super) fn guarded_enum_field(&self, test: usize, getter: usize) -> Option<(usize, usize)> {
        for id in [test, getter] {
            if self.functions[id].origin != Origin::StandardLibrary
                || self
                    .native_contract_dependencies
                    .get(&id)
                    .is_some_and(|d| !d.is_empty())
                || self
                    .native_mapped_dependencies
                    .get(&id)
                    .is_some_and(|d| !d.is_empty())
            {
                return None;
            }
        }
        let Operation::Test(tested) = *self.native_enum_operations.get(&test)? else {
            return None;
        };
        let Operation::Field { variant, field } = *self.native_enum_operations.get(&getter)? else {
            return None;
        };
        (tested == variant && self.functions[test].params == self.functions[getter].params)
            .then_some((variant, field))
    }
}

#[cfg(test)]
mod tests {
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
        let forged =
            r#"extern fn spoof<T>(x:Option<T>)->Bool{code:"unused",operation:"enum_test:0",}"#;
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
}
