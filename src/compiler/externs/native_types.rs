//! Explicit dependencies of native type expressions and their ABI adapters.
use super::*;

#[derive(Clone)]
pub(in crate::compiler) struct Binding {
    pub alias: String,
    pub template: String,
    pub bindings: BTreeMap<String, String>,
    pub span: Span,
    pub request: Span,
    pub native: Option<String>,
    pub references: BTreeSet<usize>,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Declaration {
    Type(String),
    Value(usize),
}

impl Compiler {
    pub(in crate::compiler) fn prepare_native_types(&mut self) -> Result<(), String> {
        loop {
            let todo = self
                .native_type_bindings
                .iter()
                .filter(|(_, binding)| binding.native.is_none())
                .map(|(ty, binding)| (ty.clone(), binding.clone()))
                .collect::<Vec<_>>();
            if todo.is_empty() {
                break;
            }
            for (ty, binding) in todo {
                self.rendering_native_types.push(ty.clone());
                let result = self.render_native(
                    &binding.template,
                    &binding.bindings,
                    None,
                    &binding.span,
                    false,
                    None,
                );
                self.rendering_native_types.pop();
                let native = result.map_err(|error| {
                    format!(
                        "{error}\nwhile specializing '{ty}' at {}:{}:{}",
                        binding.request.file, binding.request.line, binding.request.column
                    )
                })?;
                self.native_type_bindings.get_mut(&ty).unwrap().native = Some(native);
            }
        }
        let mut output = String::new();
        let mut complete = BTreeSet::new();
        let mut visiting = BTreeSet::new();
        let mut budget = 16384;
        for ty in self.native_type_bindings.keys() {
            let request = &self.native_type_bindings[ty].request;
            self.native_type_declaration(
                &Declaration::Type(ty.clone()),
                &mut complete,
                &mut visiting,
                &mut budget,
                &mut output,
            )
            .map_err(|error| {
                format!(
                    "{error}\nwhile specializing '{ty}' at {}:{}:{}",
                    request.file, request.line, request.column
                )
            })?;
        }
        self.native_type_prelude = output;
        Ok(())
    }

    fn native_alias_dependencies(
        &self,
        ty: &str,
        seen: &mut BTreeSet<String>,
        budget: &mut usize,
    ) -> Result<BTreeSet<String>, String> {
        if *budget == 0 {
            return Err("native type dependency work limit exceeded".into());
        }
        *budget -= 1;
        if self.native_type_bindings.contains_key(ty) {
            return Ok(BTreeSet::from([ty.into()]));
        }
        if !seen.insert(ty.into()) {
            return Ok(BTreeSet::new());
        }
        let mut aliases = BTreeSet::new();
        if let Some(deps) = self.native_type_dependencies.get(ty) {
            for dep in deps {
                aliases.extend(self.native_alias_dependencies(dep, seen, budget)?);
            }
        }
        Ok(aliases)
    }

    fn native_type_declaration(
        &self,
        node: &Declaration,
        complete: &mut BTreeSet<Declaration>,
        visiting: &mut BTreeSet<Declaration>,
        budget: &mut usize,
        output: &mut String,
    ) -> Result<(), String> {
        if *budget == 0 {
            return Err("native type dependency work limit exceeded".into());
        }
        *budget -= 1;
        if complete.contains(node) {
            return Ok(());
        }
        let span = match node {
            Declaration::Type(ty) => &self.native_type_bindings[ty].span,
            Declaration::Value(id) => &self.functions[*id].name.span,
        };
        if visiting.len() >= 128 {
            return Err(span.error("native type declaration dependency depth limit exceeded"));
        }
        if !visiting.insert(node.clone()) {
            let (span, label) = match node {
                Declaration::Type(ty) => (&self.native_type_bindings[ty].span, ty.clone()),
                Declaration::Value(id) => (
                    &self.functions[*id].name.span,
                    self.functions[*id].name.text.clone(),
                ),
            };
            return Err(span.error(format!(
                "cyclic native type declaration dependency involving '{label}'"
            )));
        }
        let (types, values) = match node {
            Declaration::Type(ty) => (
                self.native_type_dependencies
                    .get(ty)
                    .cloned()
                    .unwrap_or_default(),
                self.native_type_bindings[ty].references.clone(),
            ),
            Declaration::Value(id) => (
                self.functions[*id]
                    .params
                    .iter()
                    .chain(std::iter::once(&self.functions[*id].result))
                    .cloned()
                    .collect(),
                BTreeSet::new(),
            ),
        };
        let mut aliases = BTreeSet::new();
        for ty in types {
            aliases.extend(self.native_alias_dependencies(&ty, &mut BTreeSet::new(), budget)?);
        }
        for alias in aliases {
            self.native_type_declaration(
                &Declaration::Type(alias),
                complete,
                visiting,
                budget,
                output,
            )?;
        }
        for id in values {
            self.native_type_declaration(
                &Declaration::Value(id),
                complete,
                visiting,
                budget,
                output,
            )?;
        }
        match node {
            Declaration::Type(ty) => {
                let binding = &self.native_type_bindings[ty];
                output.push_str(&format!(
                    "using {}={};\n",
                    binding.alias,
                    binding.native.as_ref().expect("prepared native type")
                ));
            }
            Declaration::Value(id) => {
                let f = &self.functions[*id];
                let result = if f.result == "Unit" {
                    "void"
                } else {
                    &self.types[&f.result]
                };
                let mut params = f
                    .params
                    .iter()
                    .map(|ty| format!("const Obj<{}>&", self.types[ty]))
                    .collect::<Vec<_>>();
                params.push("Loc".into());
                output.push_str(&format!(
                    "{result} rusk_value_ref_{id}({});\n",
                    params.join(",")
                ));
            }
        }
        visiting.remove(node);
        complete.insert(node.clone());
        Ok(())
    }
}
