use super::tuples::type_parts;
use super::*;

#[derive(Clone)]
pub(super) struct Definition {
    pub name: Name,
    pub parameters: Vec<Name>,
    pub variants: Vec<(Name, Vec<Name>)>,
}
#[derive(Clone)]
pub(super) struct Enum {
    pub definition: String,
    pub arguments: Vec<String>,
    pub variants: Vec<(Name, Vec<Name>)>,
}
impl Compiler {
    pub(super) fn register_enum(
        &mut self,
        name: &Name,
        parameters: &[Name],
        variants: &[(Name, Vec<Name>)],
    ) -> Result<(), String> {
        if self.types.contains_key(&name.text)
            || self.generic_records.contains_key(&name.text)
            || self.generic_extern_types.contains_key(&name.text)
            || self.enum_definitions.contains_key(&name.text)
        {
            return Err(name.span.error("duplicate type"));
        }
        if name.text.starts_with("__Rusk")
            || matches!(
                name.text.as_str(),
                "Vec" | "Tuple" | "Iter" | "Dict" | "Set"
            )
        {
            return Err(name.span.error("reserved built-in type constructor"));
        }
        if variants.is_empty() {
            return Err(name.span.error("enum requires at least one variant"));
        }
        let mut names = BTreeSet::new();
        for p in parameters {
            if !names.insert(&p.text) {
                return Err(p.span.error("duplicate type parameter"));
            }
        }
        names.clear();
        for (v, _) in variants {
            if !names.insert(&v.text) {
                return Err(v.span.error("duplicate enum variant"));
            }
        }
        self.enum_definitions.insert(
            name.text.clone(),
            Definition {
                name: name.clone(),
                parameters: parameters.to_vec(),
                variants: variants.to_vec(),
            },
        );
        Ok(())
    }
    pub(super) fn specialize_enum(&mut self, name: &Name) -> Result<bool, String> {
        let (base, args) = type_parts(&name.text).unwrap_or((&name.text, vec![]));
        let Some(d) = self.enum_definitions.get(base).cloned() else {
            return Ok(false);
        };
        if args.len() != d.parameters.len() {
            return Err(name.span.error(format!(
                "{base} expects {} type arguments",
                d.parameters.len()
            )));
        }
        self.check_enum_cycle(name, &mut vec![], &mut BTreeSet::new())?;
        if self.preparing_enums.len() >= 64 || self.enums.len() >= 512 {
            return Err(name
                .span
                .error("recursive enum or enum specialization limit exceeded"));
        }
        if self.preparing_enums.contains(&name.text) {
            return Err(name
                .span
                .error("recursive enum types are not implemented yet"));
        }
        for arg in &args {
            self.check_type(&Name {
                text: (*arg).into(),
                span: name.span.clone(),
            })?;
        }
        self.preparing_enums.push(name.text.clone());
        let bindings = d
            .parameters
            .iter()
            .zip(&args)
            .map(|(p, a)| (p.text.clone(), (*a).to_string()))
            .collect();
        let mut variants = d.variants.clone();
        for (_, fields) in &mut variants {
            for field in fields {
                field.text = generics::substitute(&field.text, &bindings);
                self.check_type(field)?;
            }
        }
        self.preparing_enums.pop();
        let native = format!("RuskEnum{}", self.enums.len());
        self.types.insert(name.text.clone(), native.clone());
        self.enums.insert(
            name.text.clone(),
            Enum {
                definition: base.into(),
                arguments: args.iter().map(|s| s.to_string()).collect(),
                variants,
            },
        );
        let ty = &name.text;
        let copy_index = self.native_function(
            "copy",
            vec![ty.clone()],
            ty,
            |cpp| {
                format!("Obj<{native}> {cpp}(Obj<{native}> x,Loc loc){{return deep_copy(x,loc);}}")
            },
            &name.span,
        )?;
        self.enum_copy_functions.insert(ty.clone(), copy_index);
        self.register_derived_copy(copy_index);
        let assign_index = self.native_function(
            "__assign__",
            vec![ty.clone(); 2],
            "Unit",
            |_| String::new(),
            &name.span,
        )?;
        self.native_derivation_tasks
            .insert(assign_index, ("enum_assign".into(), ty.clone()));
        Ok(true)
    }
    fn constructor_parts(&self, name: &str) -> Option<(String, String)> {
        let (owner, variant) = name.rsplit_once("::")?;
        let base = type_parts(owner).map_or(owner, |(b, _)| b);
        self.enum_definitions
            .contains_key(base)
            .then(|| (owner.into(), variant.into()))
    }
    pub(super) fn enum_argument_hint(
        &self,
        name: &str,
        values: &[Value],
        expected: Option<&str>,
    ) -> Option<String> {
        let (owner, variant) = self.constructor_parts(name)?;
        let (base, args) = type_parts(&owner).unwrap_or((&owner, vec![]));
        let d = self.enum_definitions.get(base)?;
        let (_, fields) = d.variants.iter().find(|(v, _)| v.text == variant)?;
        let mut bindings = BTreeMap::new();
        if args.len() == d.parameters.len() {
            bindings.extend(
                d.parameters
                    .iter()
                    .zip(args)
                    .map(|(p, a)| (p.text.clone(), a.into())),
            );
        }
        if let Some(e) = expected {
            let pattern = if d.parameters.is_empty() {
                base.to_string()
            } else {
                format!(
                    "{base}<{}>",
                    d.parameters
                        .iter()
                        .map(|p| p.text.clone())
                        .collect::<Vec<_>>()
                        .join(",")
                )
            };
            generics::unify(&pattern, e, &d.parameters, &mut bindings);
        }
        for (field, value) in fields.iter().zip(values) {
            generics::unify(&field.text, &value.ty, &d.parameters, &mut bindings);
        }
        let hint = generics::substitute(&fields.get(values.len())?.text, &bindings);
        (!d.parameters.iter().any(|p| {
            generics::substitute(&hint, &BTreeMap::from([(p.text.clone(), "?".into())])) != hint
        }))
        .then_some(hint)
    }
    pub(super) fn enum_constructor(
        &mut self,
        name: &str,
        values: &[Value],
        expected: Option<&str>,
        span: &Span,
    ) -> Result<Option<Value>, String> {
        let Some((owner, variant)) = self.constructor_parts(name) else {
            return Ok(None);
        };
        let (base, args) = type_parts(&owner).unwrap_or((&owner, vec![]));
        let d = self.enum_definitions[base].clone();
        let (index, (_, fields)) = d
            .variants
            .iter()
            .enumerate()
            .find(|(_, (v, _))| v.text == variant)
            .ok_or_else(|| span.error(format!("unknown enum variant '{variant}'")))?;
        if fields.len() != values.len() {
            return Err(span.error("enum variant argument count mismatch"));
        }
        let mut bindings = BTreeMap::new();
        if !args.is_empty() {
            if args.len() != d.parameters.len() {
                return Err(span.error("enum type argument count mismatch"));
            }
            bindings.extend(
                d.parameters
                    .iter()
                    .zip(args)
                    .map(|(p, a)| (p.text.clone(), a.into())),
            );
        }
        if let Some(e) = expected {
            let pattern = if d.parameters.is_empty() {
                base.to_string()
            } else {
                format!(
                    "{base}<{}>",
                    d.parameters
                        .iter()
                        .map(|p| p.text.clone())
                        .collect::<Vec<_>>()
                        .join(",")
                )
            };
            if !generics::unify(&pattern, e, &d.parameters, &mut bindings) {
                return Err(span.error("enum constructor type context mismatch"));
            }
        }
        for (field, value) in fields.iter().zip(values) {
            if !generics::unify(&field.text, &value.ty, &d.parameters, &mut bindings) {
                return Err(span.error("enum variant argument type mismatch"));
            }
        }
        let arguments = d
            .parameters
            .iter()
            .map(|p| {
                bindings.get(&p.text).cloned().ok_or_else(|| {
                    span.error(format!(
                        "cannot infer enum type parameter '{}'; add type arguments or annotation",
                        p.text
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let ty = if arguments.is_empty() {
            base.into()
        } else {
            format!("{base}<{}>", arguments.join(","))
        };
        self.check_type(&Name {
            text: ty.clone(),
            span: span.clone(),
        })?;
        Ok(Some(self.emit(
            &ty,
            Rvalue::Variant {
                index,
                fields: values.to_vec(),
            },
        )))
    }
    pub(super) fn option_element(&self, ty: &str) -> Option<String> {
        self.enums
            .get(ty)
            .filter(|e| e.definition == "Option")
            .map(|e| e.arguments[0].clone())
    }
    pub(super) fn prepare_enum_copies(&mut self) -> Result<(), String> {
        let todo = self
            .enums
            .iter()
            .filter(|(ty, _)| !self.enum_copies.contains_key(*ty))
            .map(|(ty, _)| ty.clone())
            .collect::<Vec<_>>();
        for ty in todo {
            let index = self.enum_copy_functions[&ty];
            let code = self.render_derived_copy(
                "enum_copy",
                &ty,
                index,
                &self.functions[index].name.span.clone(),
            )?;
            self.enum_copies.insert(ty, code);
        }
        Ok(())
    }
    fn check_enum_cycle(
        &self,
        name: &Name,
        path: &mut Vec<String>,
        checked: &mut BTreeSet<String>,
    ) -> Result<(), String> {
        if path.len() > 64 || name.text.len() > 16_384 || path.contains(&name.text) {
            return Err(name
                .span
                .error("recursive enum type or type expansion limit exceeded"));
        }
        if checked.contains(&name.text) {
            return Ok(());
        }
        let (base, args) = type_parts(&name.text).unwrap_or((&name.text, vec![]));
        path.push(name.text.clone());
        let mut fields = vec![];
        if let Some(d) = self.enum_definitions.get(base) {
            let bindings = d
                .parameters
                .iter()
                .zip(&args)
                .map(|(p, a)| (p.text.clone(), (*a).to_string()))
                .collect();
            fields.extend(d.variants.iter().flat_map(|(_, fs)| fs).map(|f| Name {
                text: generics::substitute(&f.text, &bindings),
                span: f.span.clone(),
            }));
        } else if let Some(r) = self.records.get(&name.text) {
            fields.extend(r.fields.iter().map(|(_, t)| t.clone()));
        } else if let Some(r) = self.generic_records.get(base) {
            let bindings = r
                .parameters
                .iter()
                .zip(&args)
                .map(|(p, a)| (p.text.clone(), (*a).to_string()))
                .collect();
            fields.extend(r.fields.iter().map(|(_, f)| Name {
                text: generics::substitute(&f.text, &bindings),
                span: f.span.clone(),
            }));
        }
        // Traverse type arguments as well, including containers and phantom arguments.
        fields.extend(args.iter().map(|a| Name {
            text: (*a).into(),
            span: name.span.clone(),
        }));
        for field in fields {
            self.check_enum_cycle(&field, path, checked)?;
        }
        path.pop();
        checked.insert(name.text.clone());
        Ok(())
    }
    pub(super) fn enum_forwards(&self) -> String {
        self.enums
            .keys()
            .map(|ty| format!("struct {};\n", self.types[ty]))
            .collect()
    }
    pub(super) fn enum_cpp(&self) -> String {
        let mut code = String::new();
        for (ty, e) in &self.enums {
            let native = &self.types[ty];
            code += &format!("struct {native}{{\n");
            for (index, (_, fields)) in e.variants.iter().enumerate() {
                code += &format!("struct Variant{index}{{");
                for (i, f) in fields.iter().enumerate() {
                    code += &format!("Obj<{}> field_{i};", self.types[&f.text]);
                }
                code += "};\n";
            }
            let nullable = if e.variants.len() == 2 {
                e.variants
                    .iter()
                    .position(|(_, f)| f.len() == 1)
                    .filter(|&i| e.variants[1 - i].1.is_empty())
            } else {
                None
            };
            if let Some(index) = nullable {
                code += &format!(
                    "static constexpr bool nullable=true;static constexpr std::size_t present={index},absent={};Obj<{}> value;\n}};\n",
                    1 - index,
                    self.types[&e.variants[index].1[0].text]
                );
            } else {
                code += &format!(
                    "static constexpr bool nullable=false;std::variant<{}> value;\n}};\n",
                    (0..e.variants.len())
                        .map(|i| format!("Variant{i}"))
                        .collect::<Vec<_>>()
                        .join(",")
                );
            }
            code += &format!(
                "Obj<{native}> deep_copy(const Obj<{native}>& value,Loc loc=Loc{{\"<copy>\",0,0}});\n"
            );
        }
        code
    }
    pub(super) fn enum_copy_cpp(&self) -> String {
        self.enum_copies.values().cloned().collect()
    }
}
