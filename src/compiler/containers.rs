use super::tuples::type_parts;
use super::*;

#[derive(Clone)]
pub(super) enum Shape {
    Dict,
    Set,
}
#[derive(Clone)]
pub(super) struct Container {
    pub shape: Shape,
    pub arguments: Vec<String>,
    pub native: String,
}

impl Compiler {
    fn key_type(&self, ty: &str) -> bool {
        matches!(ty, "Int" | "Bool" | "Char" | "Str")
            || (ty.starts_with("Tuple<")
                && self
                    .records
                    .get(ty)
                    .is_some_and(|r| r.fields.iter().all(|(_, t)| self.key_type(&t.text))))
    }
    pub(super) fn specialize_container(&mut self, name: &Name) -> Result<bool, String> {
        let Some((base, args)) = type_parts(&name.text) else {
            return Ok(false);
        };
        if !matches!(base, "Dict" | "Set") {
            return Ok(false);
        }
        let count = if base == "Dict" { 2 } else { 1 };
        if args.len() != count {
            return Err(name
                .span
                .error(format!("{base} expects {count} type arguments")));
        }
        let arguments = args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        for arg in &arguments {
            self.check_type(&Name {
                text: arg.clone(),
                span: name.span.clone(),
            })?;
        }
        if !self.key_type(&arguments[0]) {
            return Err(name
                .span
                .error("hash keys require basic types or tuples of basic types"));
        }
        if self.containers.len() >= 512 {
            return Err(name.span.error("container specialization limit exceeded"));
        }
        let native = format!("RuskContainer{}", self.containers.len());
        let shape = match base {
            "Dict" => Shape::Dict,
            _ => Shape::Set,
        };
        self.types.insert(name.text.clone(), native.clone());
        self.containers.insert(
            name.text.clone(),
            Container {
                shape: shape.clone(),
                arguments,
                native: native.clone(),
            },
        );
        let ty = &name.text;
        self.native_function(
            "copy",
            vec![ty.clone()],
            ty,
            |cpp| {
                format!("Obj<{native}> {cpp}(Obj<{native}> x,Loc loc){{return deep_copy(x,loc);}}")
            },
            &name.span,
        )?;
        let assign = "x->entries=std::move(snapshot->entries);++x->version;";
        self.native_function("__assign__",vec![ty.clone(),ty.clone()],"Unit",|cpp|format!("void {cpp}(Obj<{native}> x,Obj<{native}> y,Loc loc){{auto snapshot=deep_copy(y,loc);{assign}}}"),&name.span)?;
        Ok(true)
    }
    pub(super) fn read_function(
        &mut self,
        ty: &str,
        dimensions: usize,
        span: &Span,
    ) -> Result<usize, String> {
        let args = (0..dimensions)
            .map(|_| Value {
                ty: "Int".into(),
                id: ValueId::Probe,
            })
            .collect::<Vec<_>>();
        if let Some(index) = self.resolve("__read__", &args, Some(ty), span)? {
            return Ok(index);
        }
        self.check_type(&Name {
            text: ty.into(),
            span: span.clone(),
        })?;
        let kind = if vector_element(ty).is_some() {
            if dimensions == 0 {
                return Err(span.error("array input requires one length per array dimension"));
            }
            "vector_read"
        } else if ty.starts_with("Tuple<") && dimensions == 0 {
            "tuple_read"
        } else {
            return Err(span.error(format!(
                "no input reader for '{ty}' with {dimensions} dimensions"
            )));
        };
        let index = self.native_function(
            "__read__",
            vec!["Int".into(); dimensions],
            ty,
            |_| String::new(),
            span,
        )?;
        self.native_derivation_tasks
            .insert(index, (kind.into(), ty.into()));
        Ok(index)
    }

    pub(super) fn prepare_container_copies(&mut self) -> Result<(), String> {
        let todo = self
            .containers
            .iter()
            .filter(|(ty, _)| !self.container_copies.contains_key(*ty))
            .map(|(ty, c)| (ty.clone(), c.clone()))
            .collect::<Vec<_>>();
        for (ty, c) in todo {
            let mut copies = vec![];
            for argument in &c.arguments {
                let span = Span {
                    file: "<stdlib/containers>".into(),
                    line: 1,
                    column: 1,
                };
                let f = self
                    .resolve(
                        "copy",
                        &[Value {
                            ty: argument.clone(),
                            id: ValueId::Probe,
                        }],
                        None,
                        &span,
                    )?
                    .ok_or_else(|| {
                        span.error(format!("container element '{argument}' requires copy"))
                    })?;
                self.same(argument, &self.functions[f].result, &span)?;
                self.used.insert(f);
                copies.push(f);
            }
            self.container_copies.insert(ty, copies);
        }
        Ok(())
    }
    pub(super) fn container_cpp(&self) -> String {
        let mut code = String::new();
        for native in self.types.values().filter(|n| n.starts_with("RuskRecord")) {
            code += &format!("struct {native};\n");
        }
        for c in self.containers.values() {
            code += &format!("struct {};\n", c.native);
        }
        for c in self.containers.values() {
            let native = &c.native;
            let args = c
                .arguments
                .iter()
                .map(|t| self.types[t].clone())
                .collect::<Vec<_>>()
                .join(",");
            let base = match c.shape {
                Shape::Dict => "RuskDict",
                Shape::Set => "RuskSet",
            };
            code += &format!("struct {native}:{base}<{args}>{{}};\n");
        }
        code
    }
    pub(super) fn container_declarations(&self) -> String {
        self.containers
            .values()
            .map(|c| {
                format!(
                    "Obj<{}> deep_copy(const Obj<{}>& value,Loc loc=Loc{{\"<copy>\",0,0}});\n",
                    c.native, c.native
                )
            })
            .collect()
    }
    // Compatibility for the existing physical hash-key representation. Normal
    // Tuple comparison now resolves logical field operations in the library.
    pub(super) fn key_equal_cpp(&self) -> String {
        let keys = self
            .records
            .iter()
            .filter(|(ty, _)| self.key_type(ty))
            .collect::<Vec<_>>();
        let mut code = String::new();
        for (ty, _) in &keys {
            let native = &self.types[*ty];
            code.push_str(&format!(
                "bool operator==(const {native}& a,const {native}& b);\n"
            ));
        }
        for (ty, record) in keys {
            let native = &self.types[ty];
            let comparisons = (0..record.fields.len())
                .map(|i| format!("value_equal(a.field_{i},b.field_{i})"))
                .collect::<Vec<_>>()
                .join(" && ");
            code.push_str(&format!(
                "bool operator==(const {native}& a,const {native}& b){{return {comparisons};}}\n"
            ));
        }
        code
    }

    pub(super) fn key_hash_cpp(&self) -> String {
        let keys = self
            .records
            .iter()
            .filter(|(ty, _)| self.key_type(ty))
            .collect::<Vec<_>>();
        let mut code = String::new();
        for (ty, _) in &keys {
            code += &format!(
                "std::size_t rusk_hash_value(const Obj<{}>& x);\n",
                self.types[*ty]
            );
        }
        for (ty, record) in keys {
            code += &format!(
                "std::size_t rusk_hash_value(const Obj<{}>& x){{std::size_t hash=0;",
                self.types[ty]
            );
            for i in 0..record.fields.len() {
                code += &format!("rusk_hash_combine(hash,rusk_hash_value(x->field_{i}));");
            }
            code += "return hash;}\n";
        }
        code
    }
    pub(super) fn container_copy_cpp(&self) -> String {
        let mut code = String::new();
        for (ty, c) in &self.containers {
            let native = &c.native;
            let copies = self.container_copies[ty]
                .iter()
                .map(|&i| self.functions[i].cpp.clone())
                .collect::<Vec<_>>();
            let body = match &c.shape {
                Shape::Dict => format!(
                    "auto result=make_obj<{native}>({native}{{}});for(const auto& entry:value->entries){{auto key={}(entry.first,loc);auto item={}(entry.second,loc);result->entries.emplace(key,item);}}return result;",
                    copies[0], copies[1]
                ),
                Shape::Set => format!(
                    "auto result=make_obj<{native}>({native}{{}});for(const auto& entry:value->entries)result->entries.insert({}(entry,loc));return result;",
                    copies[0]
                ),
            };
            code +=
                &format!("Obj<{native}> deep_copy(const Obj<{native}>& value,Loc loc){{{body}}}\n");
        }
        code
    }
}
