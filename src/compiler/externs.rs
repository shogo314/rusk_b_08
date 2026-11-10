//! Initial generic extern prototype: opaque native types and static native bodies.
use super::*;
pub(super) mod native_types;
mod template;
pub(super) mod type_patterns;
use template::Part;

#[derive(Clone)]
pub(super) struct ExternalType {
    pattern: String,
    parameters: Vec<Name>,
    native: String,
    span: Span,
    representation: Option<crate::hir::Type>,
}

impl Compiler {
    pub(super) fn register_return_abi(
        &mut self,
        index: usize,
        name: &Name,
        properties: &BTreeMap<String, String>,
        result: &str,
    ) -> Result<(), String> {
        if let Some(abi) = properties.get("return_abi") {
            if abi != "object" {
                return Err(name
                    .span
                    .error("invalid native return ABI; expected object"));
            }
            if result == "Unit" {
                self.native_unit_objects.insert(index);
            }
        }
        Ok(())
    }
    pub(super) fn abi_reference_name(&self, index: usize, object: bool) -> String {
        if self.functions[index].result != "Unit"
            || object == self.native_unit_objects.contains(&index)
        {
            self.functions[index].cpp.clone()
        } else {
            (if object {
                crate::native_abi::UnitBridge::Box
            } else {
                crate::native_abi::UnitBridge::Discard
            })
            .name(index)
        }
    }
    pub(super) fn abi_reference(&mut self, index: usize, object: bool) -> String {
        if self.functions[index].result == "Unit"
            && object != self.native_unit_objects.contains(&index)
        {
            self.native_unit_bridges.insert((
                index,
                if object {
                    crate::native_abi::UnitBridge::Box
                } else {
                    crate::native_abi::UnitBridge::Discard
                },
            ));
            if self.functions[index].body.is_none()
                && template::identifier(&self.functions[index].cpp)
            {
                self.native_references.insert(index);
            }
        }
        self.abi_reference_name(index, object)
    }
    pub(super) fn register_extern_type(
        &mut self,
        name: &Name,
        parameters: &[Name],
        properties: &BTreeMap<String, String>,
        origin: Origin,
    ) -> Result<(), String> {
        generics::parameters_valid(parameters)?;
        let base = tuples::type_parts(&name.text).map_or(name.text.as_str(), |(base, _)| base);
        if name.text.starts_with("__Rusk")
            || (base == "Vec" && origin != Origin::StandardLibrary)
            || matches!(
                tuples::type_parts(&name.text).map_or(name.text.as_str(), |(base, _)| base),
                "Tuple" | "Option"
            )
        {
            return Err(name
                .span
                .error("generic extern requires an unreserved type constructor name"));
        }
        if self.types.contains_key(&name.text)
            || self.generic_records.contains_key(&name.text)
            || self.generic_extern_types.contains_key(&name.text)
        {
            return Err(name.span.error("duplicate type"));
        }
        validate(name, properties, &["cpp_name", "representation"])?;
        let representation = type_metadata::parse_properties(name, properties, origin)?;
        let pattern = type_patterns::pattern(name, parameters);
        let mut bound = BTreeMap::new();
        generics::unify(&pattern, &pattern, parameters, &mut bound);
        for parameter in parameters {
            if !bound.contains_key(&parameter.text) {
                return Err(parameter
                    .span
                    .error("native type parameter is not bound by the type pattern"));
            }
        }
        self.generic_extern_types.insert(
            name.text.clone(),
            ExternalType {
                pattern,
                parameters: parameters.to_vec(),
                native: property(name, properties, "cpp_name")?,
                span: name.span.clone(),
                representation,
            },
        );
        Ok(())
    }

    pub(super) fn register_extern_function(
        &mut self,
        name: &Name,
        parameters: &[Name],
        arguments: &[(DeclaredName, Name)],
        result: &Option<Name>,
        properties: &BTreeMap<String, String>,
        origin: Origin,
    ) -> Result<(), String> {
        validate(
            name,
            properties,
            &[
                "code",
                "effects",
                "lower",
                "lower_owned",
                "lower_borrowed",
                "lower_rooted",
                "operation",
                "return_abi",
                "derive",
            ],
        )?;
        if properties.get("return_abi").is_some_and(|v| v != "object") {
            return Err(name
                .span
                .error("invalid native return ABI; expected object"));
        }
        if properties.contains_key("effects") && origin != Origin::StandardLibrary {
            return Err(name
                .span
                .error("native effect contracts are reserved for the embedded standard library"));
        }
        self.validate_lowering_source(name, properties, origin)?;
        property(name, properties, "code")?;
        let mut parts = Vec::new();
        for key in [
            "code",
            "lower",
            "lower_owned",
            "lower_borrowed",
            "lower_rooted",
        ] {
            if let Some(text) = properties.get(key) {
                parts.extend(template::parse(text, true).map_err(|e| name.span.error(e))?);
            }
        }
        let mut calls = vec![];
        let mut callbacks = vec![];
        if !properties.contains_key("derive") {
            let actual = arguments
                .iter()
                .map(|(_, ty)| ty.text.clone())
                .collect::<Vec<_>>();
            let mut budget = 4096;
            for part in &parts {
                match part {
                    Part::Call {
                        name: target,
                        arguments,
                        result,
                        ..
                    } => {
                        calls.push(generics::CallRequirement {
                            name: target.clone(),
                            arguments: template::expand_arguments(arguments, &actual, &mut budget)
                                .map_err(|e| name.span.error(e))?,
                            result: result.clone(),
                        });
                    }
                    Part::Invoke {
                        name: target,
                        arguments,
                        result,
                        ..
                    } => {
                        callbacks.push(generics::CallbackRequirement {
                            ty: target.clone(),
                            parameters: template::expand_arguments(arguments, &actual, &mut budget)
                                .map_err(|e| name.span.error(e))?,
                            result: result.clone(),
                        });
                    }
                    _ => {}
                }
            }
        }
        for (argument, _) in arguments {
            if argument.binding.is_some() || argument.access.is_some() || argument.escape.is_some()
            {
                return Err(argument
                    .span
                    .error("modifiers on extern parameters require a Rusk function body"));
            }
        }
        let definition = generics::GenericFunction {
            name: name.clone(),
            parameters: parameters.to_vec(),
            arguments: arguments.to_vec(),
            result: result.clone(),
            body: None,
            foreign: Some(properties.clone()),
            origin,
            callbacks,
            calls,
        };
        if let Some(kind) = properties.get("derive") {
            if origin != Origin::StandardLibrary {
                return Err(name.span.error(
                    "native derivation definitions are reserved for the embedded standard library",
                ));
            }
            generics::parameters_valid(parameters)?;
            let assignment = matches!(kind.as_str(), "record_assign" | "enum_assign");
            let output = kind == "tuple_write";
            let comparison = matches!(
                kind.as_str(),
                "tuple_eq" | "tuple_ne" | "tuple_lt" | "tuple_le" | "tuple_gt" | "tuple_ge"
            );
            let vector_input = kind == "vector_read";
            let input = kind == "tuple_read" || vector_input;
            let valid_kind = assignment
                || input
                || output
                || comparison
                || matches!(kind.as_str(), "record_copy" | "enum_copy");
            if !valid_kind
                || parameters.len() != 1
                || arguments.len()
                    != if assignment || comparison {
                        2
                    } else if input {
                        0
                    } else {
                        1
                    }
                || arguments
                    .iter()
                    .any(|(_, ty)| ty.text != parameters[0].text)
                || result.as_ref().is_none_or(|r| {
                    if vector_input {
                        let mut bindings = BTreeMap::new();
                        !generics::unify(&r.text, &r.text, parameters, &mut bindings)
                            || !bindings.contains_key(&parameters[0].text)
                    } else {
                        r.text
                            != if comparison {
                                "Bool"
                            } else if assignment || output {
                                "Unit"
                            } else {
                                &parameters[0].text
                            }
                    }
                })
                || ((assignment || output) && parameters[0].text == "Unit")
                || (comparison && parameters[0].text == "Bool")
                || properties.keys().any(|key| {
                    !matches!(key.as_str(), "code" | "derive")
                        && !((output || comparison)
                            && matches!(
                                key.as_str(),
                                "effects"
                                    | "lower"
                                    | "lower_owned"
                                    | "lower_borrowed"
                                    | "lower_rooted"
                            ))
                })
            {
                return Err(name.span.error(if assignment {
                    "invalid native assignment derivation; expected one type parameter and (T,T)->Unit"
                }else if comparison {
                    "invalid native comparison derivation; expected one type parameter and (T,T)->Bool"
                }else if output {
                    "invalid native output derivation; expected one type parameter and (T)->Unit"
                }else if vector_input {
                    "invalid native input derivation; expected one type parameter bound by the result pattern and no declared arguments"
                }else if input {
                    "invalid native input derivation; expected one type parameter and ()->T"
                }else{
                    "invalid native copy derivation; expected one type parameter and (T)->T"
                }));
            }
            if self
                .native_derivations
                .insert(kind.clone(), definition)
                .is_some()
            {
                return Err(name.span.error("duplicate native derivation definition"));
            }
            Ok(())
        } else {
            self.register_generic_function(definition)
        }
    }

    pub(super) fn specialize_extern_type(&mut self, name: &Name) -> Result<bool, String> {
        let Some((base, args)) = tuples::type_parts(&name.text) else {
            return Ok(false);
        };
        let Some(arity) = self.external_type_arity(base) else {
            return Ok(false);
        };
        if args.len() != arity {
            return Err(name
                .span
                .error(format!("type '{base}' expects {} type arguments", arity)));
        }
        let Some((template, bindings)) = self.generic_extern_types.values().find_map(|template| {
            let mut bindings = BTreeMap::new();
            generics::unify(
                &template.pattern,
                &name.text,
                &template.parameters,
                &mut bindings,
            )
            .then(|| (template.clone(), bindings))
        }) else {
            return Ok(false);
        };
        if self.preparing_extern_types.len() >= 64
            || !self.preparing_extern_types.insert(name.text.clone())
        {
            return Err(name.span.error("recursive generic extern type expansion"));
        }
        let result = (|| {
            for ty in args {
                self.check_type(&Name {
                    text: ty.into(),
                    span: name.span.clone(),
                })?;
            }
            let parts =
                template::parse_type(&template.native).map_err(|e| template.span.error(e))?;
            let native = if parts.iter().any(|part| matches!(part, Part::Call { .. })) {
                if self.native_type_bindings.len() >= 512 {
                    return Err(name
                        .span
                        .error("native type reference specialization limit exceeded"));
                }
                let alias = format!("RuskNativeType{}", self.native_type_bindings.len());
                self.native_type_bindings.insert(
                    name.text.clone(),
                    native_types::Binding {
                        alias: alias.clone(),
                        template: template.native.clone(),
                        bindings: bindings.clone(),
                        span: template.span.clone(),
                        request: name.span.clone(),
                        native: None,
                        references: BTreeSet::new(),
                    },
                );
                alias
            } else {
                self.rendering_native_types.push(name.text.clone());
                let native = self.render_native(
                    &template.native,
                    &bindings,
                    None,
                    &template.span,
                    false,
                    None,
                );
                self.rendering_native_types.pop();
                native?
            };
            self.types.insert(name.text.clone(), native);
            if let Some(kind) = template.representation {
                let kind = type_metadata::substitute(&kind, &bindings);
                self.native_types.insert(
                    name.text.clone(),
                    type_metadata::Metadata {
                        kind: kind.clone(),
                        span: template.span.clone(),
                    },
                );
                self.validate_representation(
                    &Name {
                        text: name.text.clone(),
                        span: template.span.clone(),
                    },
                    &kind,
                )?;
            }
            Ok(true)
        })();
        self.preparing_extern_types.remove(&name.text);
        result.map_err(|error: String| {
            format!(
                "{error}\nwhile specializing '{}' at {}:{}:{}",
                name.text, name.span.file, name.span.line, name.span.column
            )
        })
    }

    fn external_type_arity(&self, base: &str) -> Option<usize> {
        self.generic_extern_types.values().find_map(|definition| {
            tuples::type_parts(&definition.pattern)
                .filter(|(name, _)| *name == base)
                .map(|(_, args)| args.len())
        })
    }

    // Check symbolic patterns without choosing arbitrary types for their parameters.
    fn validate_symbolic_type(
        &self,
        ty: &str,
        parameters: &[Name],
        span: &Span,
    ) -> Result<(), String> {
        if !template::valid_type(ty) {
            return Err(span.error("invalid native type reference"));
        }
        if parameters.iter().any(|p| p.text == ty) || self.types.contains_key(ty) {
            return Ok(());
        }
        let Some((base, args)) = tuples::type_parts(ty) else {
            return Err(span.error(format!("unknown type '{ty}'")));
        };
        let arity = if let Some(arity) = self.external_type_arity(base) {
            Some(arity)
        } else if let Some(d) = self.generic_records.get(base) {
            Some(d.parameters.len())
        } else if let Some(d) = self.enum_definitions.get(base) {
            Some(d.parameters.len())
        } else {
            match base {
                "Iter" => Some(1),
                "Tuple" if args.len() >= 2 => Some(args.len()),
                _ => None,
            }
        }
        .or_else(|| {
            self.types.keys().find_map(|t| {
                tuples::type_parts(t)
                    .filter(|(b, _)| *b == base)
                    .map(|(_, a)| a.len())
            })
        });
        let Some(arity) = arity else {
            return Err(span.error(format!("unknown type constructor '{base}'")));
        };
        if parameters.iter().any(|p| p.text == base) || args.len() != arity {
            return Err(span.error(format!("type '{base}' expects {arity} type arguments")));
        }
        for arg in args {
            self.validate_symbolic_type(arg, parameters, span)?;
        }
        Ok(())
    }

    fn validate_scoped_native_type(
        &self,
        ty: &str,
        parameters: &[Name],
        indices: &BTreeSet<String>,
        span: &Span,
    ) -> Result<(), String> {
        if ty
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .any(|word| indices.contains(word))
        {
            return Err(span.error("native field index cannot be used as a type"));
        }
        self.validate_symbolic_type(ty, parameters, span)
    }

    fn validate_native_parts(
        &self,
        parts: Vec<Part>,
        parameters: &[Name],
        indices: &BTreeSet<String>,
        span: &Span,
    ) -> Result<(), String> {
        for part in parts {
            match part {
                Part::IfType {
                    ty,
                    pattern,
                    parameters: names,
                    matched,
                    otherwise,
                } => {
                    self.validate_scoped_native_type(&ty, parameters, indices, span)?;
                    let mut scoped = parameters.to_vec();
                    let mut bound = BTreeMap::new();
                    for name in &names {
                        if parameters.iter().any(|p| &p.text == name) || indices.contains(name) {
                            return Err(span.error(format!(
                                "native type binding '{name}' shadows an existing binding"
                            )));
                        }
                        scoped.push(Name {
                            text: name.clone(),
                            span: span.clone(),
                        });
                    }
                    self.validate_scoped_native_type(&pattern, &scoped, indices, span)?;
                    let locals = names
                        .iter()
                        .map(|name| Name {
                            text: name.clone(),
                            span: span.clone(),
                        })
                        .collect::<Vec<_>>();
                    generics::unify(&pattern, &pattern, &locals, &mut bound);
                    if names.iter().any(|name| !bound.contains_key(name)) {
                        return Err(span.error("native type binding is not bound by the pattern"));
                    }
                    self.validate_native_parts(matched, &scoped, indices, span)?;
                    self.validate_native_parts(otherwise, parameters, indices, span)?;
                }
                Part::Index(index) => {
                    if !indices.contains(&index) {
                        return Err(span.error(format!("unbound native field index '{index}'")));
                    }
                }
                Part::Arguments {
                    element,
                    index,
                    parts,
                    ..
                } => {
                    for name in [&element, &index] {
                        if parameters.iter().any(|p| &p.text == name) || indices.contains(name) {
                            return Err(span.error(format!(
                                "native arguments binding '{name}' shadows an existing binding"
                            )));
                        }
                    }
                    let mut parameters = parameters.to_vec();
                    parameters.push(Name {
                        text: element,
                        span: span.clone(),
                    });
                    let mut indices = indices.clone();
                    indices.insert(index);
                    self.validate_native_parts(parts, &parameters, &indices, span)?;
                }
                Part::Variants { ty, index, parts } => {
                    self.validate_scoped_native_type(&ty, parameters, indices, span)?;
                    if parameters.iter().any(|p| p.text == index) || indices.contains(&index) {
                        return Err(span.error(format!(
                            "native variants binding '{index}' shadows an existing binding"
                        )));
                    }
                    let mut indices = indices.clone();
                    indices.insert(index);
                    self.validate_native_parts(parts, parameters, &indices, span)?;
                }
                Part::Fields {
                    ty,
                    variant,
                    element,
                    index,
                    parts,
                    tuple_only: _,
                } => {
                    self.validate_scoped_native_type(&ty, parameters, indices, span)?;
                    if let Some(variant) = variant
                        && variant.parse::<usize>().is_err()
                        && !indices.contains(&variant)
                    {
                        return Err(span.error(format!("unbound native variant index '{variant}'")));
                    }
                    for name in [&element, &index] {
                        if parameters.iter().any(|p| &p.text == name) || indices.contains(name) {
                            return Err(span.error(format!(
                                "native fields binding '{name}' shadows an existing binding"
                            )));
                        }
                    }
                    let mut parameters = parameters.to_vec();
                    parameters.push(Name {
                        text: element,
                        span: span.clone(),
                    });
                    let mut indices = indices.clone();
                    indices.insert(index);
                    self.validate_native_parts(parts, &parameters, &indices, span)?;
                }
                Part::Type(ty)
                | Part::NonUnitType(ty)
                | Part::ReturnType(ty)
                | Part::Finish(ty) => {
                    self.validate_scoped_native_type(&ty, parameters, indices, span)?
                }
                Part::Invoke {
                    name,
                    arguments,
                    result,
                    ..
                } => {
                    self.validate_scoped_native_type(&name, parameters, indices, span)?;
                    for ty in &arguments {
                        if template::argument_offset(ty).is_none() {
                            self.validate_scoped_native_type(ty, parameters, indices, span)?;
                        }
                    }
                    self.validate_scoped_native_type(&result, parameters, indices, span)?;
                }
                Part::Call {
                    name,
                    arguments,
                    result,
                    ..
                } => {
                    if let Some((_, args)) = tuples::type_parts(&name) {
                        for arg in args {
                            self.validate_scoped_native_type(arg, parameters, indices, span)?;
                        }
                    }
                    for ty in &arguments {
                        if template::argument_offset(ty).is_none() {
                            self.validate_scoped_native_type(ty, parameters, indices, span)?;
                        }
                    }
                    self.validate_scoped_native_type(&result, parameters, indices, span)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub(super) fn validate_extern_declarations(&self) -> Result<(), String> {
        for definition in self.generic_extern_types.values() {
            self.validate_symbolic_type(
                &definition.pattern,
                &definition.parameters,
                &definition.span,
            )?;
            if let Some(kind) = &definition.representation {
                for child in type_metadata::references(kind) {
                    self.validate_symbolic_type(child, &definition.parameters, &definition.span)?;
                }
            }
            self.validate_native_parts(
                template::parse_type(&definition.native).map_err(|e| definition.span.error(e))?,
                &definition.parameters,
                &BTreeSet::new(),
                &definition.span,
            )?;
        }
        for definition in self
            .generic_functions
            .iter()
            .chain(self.native_derivations.values())
        {
            let Some(properties) = &definition.foreign else {
                continue;
            };
            for ty in definition
                .arguments
                .iter()
                .map(|(_, ty)| ty)
                .chain(definition.result.iter())
            {
                self.validate_symbolic_type(&ty.text, &definition.parameters, &ty.span)?;
            }
            for key in [
                "code",
                "lower",
                "lower_owned",
                "lower_borrowed",
                "lower_rooted",
            ] {
                if let Some(text) = properties.get(key) {
                    let parts =
                        template::parse(text, true).map_err(|e| definition.name.span.error(e))?;
                    if properties
                        .get("derive")
                        .is_none_or(|kind| kind != "vector_read")
                    {
                        template::validate_argument_bounds(&parts, definition.arguments.len())
                            .map_err(|e| definition.name.span.error(e))?;
                    }
                    self.validate_native_parts(
                        parts,
                        &definition.parameters,
                        &BTreeSet::new(),
                        &definition.name.span,
                    )?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn render_native(
        &mut self,
        text: &str,
        bindings: &BTreeMap<String, String>,
        function: Option<&str>,
        span: &Span,
        body: bool,
        owner: Option<usize>,
    ) -> Result<String, String> {
        let parts = if body {
            template::parse(text, true)
        } else {
            template::parse_type(text)
        }
        .map_err(|e| span.error(e))?;
        self.render_native_parts(parts, bindings, function, span, owner, &mut 4096)
    }

    fn expand_native_arguments(
        &self,
        arguments: &[String],
        bindings: &BTreeMap<String, String>,
        owner: Option<usize>,
        budget: &mut usize,
        span: &Span,
    ) -> Result<Vec<String>, String> {
        // Substitute explicit types before splicing. Concrete argument types must
        // never be interpreted as local binder names a second time.
        let patterns = arguments
            .iter()
            .map(|ty| {
                if template::argument_offset(ty).is_some() {
                    ty.clone()
                } else {
                    generics::substitute(ty, bindings)
                }
            })
            .collect::<Vec<_>>();
        let actual = owner
            .map(|id| self.functions[id].params.as_slice())
            .unwrap_or_default();
        template::expand_arguments(&patterns, actual, budget).map_err(|e| span.error(e))
    }

    fn render_native_parts(
        &mut self,
        parts: Vec<Part>,
        bindings: &BTreeMap<String, String>,
        function: Option<&str>,
        span: &Span,
        owner: Option<usize>,
        budget: &mut usize,
    ) -> Result<String, String> {
        let mut output = String::new();
        for part in parts {
            if *budget == 0 {
                return Err(span.error("native field expansion work limit exceeded"));
            }
            *budget -= 1;
            match part {
                Part::Else => return Err(span.error("unexpected native type branch")),
                Part::IfType {
                    ty,
                    pattern,
                    parameters,
                    matched,
                    otherwise,
                } => {
                    if parameters.len() > *budget {
                        return Err(span.error("native field expansion work limit exceeded"));
                    }
                    *budget -= parameters.len();
                    let ty = generics::substitute(&ty, bindings);
                    // Alpha-rename the pattern's variables before inserting outer
                    // concrete types. A concrete type with the same spelling as
                    // a local binder must remain a concrete type.
                    let renames = parameters
                        .iter()
                        .enumerate()
                        .map(|(i, name)| (name.clone(), format!("${i}")))
                        .collect::<BTreeMap<_, _>>();
                    let pattern =
                        generics::substitute(&generics::substitute(&pattern, &renames), bindings);
                    let locals = parameters
                        .iter()
                        .enumerate()
                        .map(|(i, _)| Name {
                            text: format!("${i}"),
                            span: span.clone(),
                        })
                        .collect::<Vec<_>>();
                    let mut inferred = BTreeMap::new();
                    let matches = generics::unify(&pattern, &ty, &locals, &mut inferred);
                    let mut scoped = bindings.clone();
                    if matches {
                        for (i, name) in parameters.into_iter().enumerate() {
                            scoped.insert(name, inferred[&format!("${i}")].clone());
                        }
                    }
                    output.push_str(&self.render_native_parts(
                        if matches { matched } else { otherwise },
                        &scoped,
                        function,
                        span,
                        owner,
                        budget,
                    )?);
                }
                Part::Index(index) => {
                    let value = bindings
                        .get(&index)
                        .filter(|v| v.parse::<usize>().is_ok())
                        .ok_or_else(|| {
                            span.error(format!("unbound native field index '{index}'"))
                        })?;
                    output.push_str(value);
                }
                Part::Arguments {
                    offset,
                    element,
                    index,
                    parts,
                } => {
                    let owner = owner
                        .ok_or_else(|| span.error("argument reference outside native function"))?;
                    let arguments = self.functions[owner].params.clone();
                    if offset > arguments.len() {
                        return Err(span.error("native argument offset out of range"));
                    }
                    for (i, ty) in arguments.into_iter().enumerate().skip(offset) {
                        if *budget == 0 {
                            return Err(span.error("native field expansion work limit exceeded"));
                        }
                        *budget -= 1;
                        let mut scoped = bindings.clone();
                        scoped.insert(element.clone(), ty);
                        scoped.insert(index.clone(), i.to_string());
                        output.push_str(&self.render_native_parts(
                            parts.clone(),
                            &scoped,
                            function,
                            span,
                            Some(owner),
                            budget,
                        )?);
                    }
                }
                Part::Variants { ty, index, parts } => {
                    let ty = generics::substitute(&ty, bindings);
                    self.check_type(&Name {
                        text: ty.clone(),
                        span: span.clone(),
                    })?;
                    let count = self
                        .enums
                        .get(&ty)
                        .ok_or_else(|| {
                            span.error(format!(
                                "native variants reference requires an enum; got '{ty}'"
                            ))
                        })?
                        .variants
                        .len();
                    for i in 0..count {
                        if *budget == 0 {
                            return Err(span.error("native field expansion work limit exceeded"));
                        }
                        *budget -= 1;
                        let mut scoped = bindings.clone();
                        scoped.insert(index.clone(), i.to_string());
                        output.push_str(&self.render_native_parts(
                            parts.clone(),
                            &scoped,
                            function,
                            span,
                            owner,
                            budget,
                        )?);
                    }
                }
                Part::Fields {
                    ty,
                    variant,
                    element,
                    index,
                    parts,
                    tuple_only,
                } => {
                    let ty = generics::substitute(&ty, bindings);
                    self.check_type(&Name {
                        text: ty.clone(),
                        span: span.clone(),
                    })?;
                    if tuple_only && !ty.starts_with("Tuple<") {
                        return Err(
                            span.error(format!("native tuple fields require a Tuple; got '{ty}'"))
                        );
                    }
                    if self.callables.contains_key(&ty)
                        && !owner
                            .is_some_and(|id| self.functions[id].origin == Origin::StandardLibrary)
                    {
                        return Err(span.error("callable environment fields are private"));
                    }
                    let fields = if let Some(variant) = variant {
                        let variant = bindings
                            .get(&variant)
                            .unwrap_or(&variant)
                            .parse::<usize>()
                            .map_err(|_| span.error("unbound native variant index"))?;
                        self.enums
                            .get(&ty)
                            .ok_or_else(|| {
                                span.error(format!(
                                    "native payload reference requires an enum; got '{ty}'"
                                ))
                            })?
                            .variants
                            .get(variant)
                            .ok_or_else(|| span.error("native variant index out of range"))?
                            .1
                            .clone()
                    } else {
                        self.records
                            .get(&ty)
                            .ok_or_else(|| {
                                span.error(format!(
                                    "native fields reference requires a struct or Tuple; got '{ty}'"
                                ))
                            })?
                            .fields
                            .iter()
                            .map(|(_, field)| field.clone())
                            .collect()
                    };
                    // Each iteration has a lexical binding scope. Expansion
                    // reuses ordinary typed references, including dependencies.
                    for (i, field) in fields.into_iter().enumerate() {
                        if *budget == 0 {
                            return Err(span.error("native field expansion work limit exceeded"));
                        }
                        *budget -= 1;
                        let mut scoped = bindings.clone();
                        scoped.insert(element.clone(), field.text.clone());
                        scoped.insert(index.clone(), i.to_string());
                        let rendered = self
                            .render_native_parts(
                                parts.clone(),
                                &scoped,
                                function,
                                span,
                                owner,
                                budget,
                            )
                            .map_err(|error| {
                                format!(
                                    "{error}\nwhile expanding field {i} of '{ty}' at {}:{}:{}",
                                    field.span.file, field.span.line, field.span.column
                                )
                            })?;
                        output.push_str(&rendered);
                    }
                }
                Part::Text(text) => output.push_str(&text),
                Part::SelfName => output.push_str(
                    function
                        .ok_or_else(|| span.error("function reference outside native function"))?,
                ),
                Part::Type(ty) => {
                    let ty = generics::substitute(&ty, bindings);
                    self.check_type(&Name {
                        text: ty.clone(),
                        span: span.clone(),
                    })?;
                    if owner.is_none()
                        && function.is_none()
                        && let Some(current) = self.rendering_native_types.last()
                    {
                        self.native_type_dependencies
                            .entry(current.clone())
                            .or_default()
                            .insert(ty.clone());
                    }
                    output.push_str(&self.types[&ty]);
                }
                Part::NonUnitType(ty) => {
                    let ty = generics::substitute(&ty, bindings);
                    self.check_type(&Name {
                        text: ty.clone(),
                        span: span.clone(),
                    })?;
                    if ty == "Unit" {
                        return Err(span.error(
                            "native nonunit reference requires a value type other than Unit",
                        ));
                    }
                    output.push_str(&self.types[&ty]);
                }
                Part::ReturnType(ty) => {
                    let ty = generics::substitute(&ty, bindings);
                    self.check_type(&Name {
                        text: ty.clone(),
                        span: span.clone(),
                    })?;
                    output.push_str(&if ty == "Unit" {
                        "void".into()
                    } else {
                        format!("Obj<{}>", self.types[&ty])
                    });
                }
                Part::Finish(ty) => {
                    let ty = generics::substitute(&ty, bindings);
                    self.check_type(&Name {
                        text: ty.clone(),
                        span: span.clone(),
                    })?;
                    output.push_str(&format!(
                        "rusk_finish_return<{},{}>",
                        self.types[&ty],
                        ty == "Unit"
                    ));
                }
                Part::Invoke {
                    name,
                    arguments,
                    result,
                    kind,
                } => {
                    let ty = generics::substitute(&name, bindings);
                    let callback = self.callables.get(&ty).cloned().ok_or_else(|| {
                        span.error(format!("type '{ty}' is not statically callable"))
                    })?;
                    let arguments =
                        self.expand_native_arguments(&arguments, bindings, owner, budget, span)?;
                    if callback.parameters != arguments {
                        return Err(span.error("native callback parameter type mismatch"));
                    }
                    self.same(
                        &generics::substitute(&result, bindings),
                        &callback.result,
                        span,
                    )?;
                    self.used.insert(callback.function);
                    if matches!(kind, template::InvokeKind::Writes) {
                        self.native_effect_references
                            .insert((callback.function, crate::effects::Query::Writes));
                        output.push_str(&format!("rusk_writes_{}", callback.function));
                    } else {
                        if let Some(owner) = owner {
                            self.native_contract_dependencies
                                .entry(owner)
                                .or_default()
                                .insert(callback.function);
                        }
                        output.push_str(&self.abi_reference(
                            callback.function,
                            matches!(kind, template::InvokeKind::Boxed),
                        ));
                    }
                }
                Part::Call {
                    name,
                    arguments,
                    result,
                    kind,
                } => {
                    let name = generics::substitute(&name, bindings);
                    let result = generics::substitute(&result, bindings);
                    self.check_type(&Name {
                        text: result.clone(),
                        span: span.clone(),
                    })?;
                    let arguments =
                        self.expand_native_arguments(&arguments, bindings, owner, budget, span)?;
                    let mut values = vec![];
                    for ty in arguments {
                        self.check_type(&Name {
                            text: ty.clone(),
                            span: span.clone(),
                        })?;
                        values.push(Value {
                            ty,
                            id: ValueId::Probe,
                        });
                    }
                    let index = self
                        .resolve_callable(&name, &values, Some(&result), span)?
                        .ok_or_else(|| {
                            if values.is_empty()
                                && name == result
                                && self.types.contains_key(&result)
                            {
                                span.error(format!(
                                    "type '{result}' has no zero-argument constructor"
                                ))
                            } else {
                                span.error(format!("no matching native callable '{name}'"))
                            }
                        })?;
                    self.same(&result, &self.functions[index].result, span)?;
                    if owner.is_none()
                        && function.is_none()
                        && let Some(current) = self.rendering_native_types.last()
                        && let Some(binding) = self.native_type_bindings.get_mut(current)
                    {
                        binding.references.insert(index);
                    }
                    if let template::CallKind::Effect(query) = kind {
                        // A semantic query does not call or take the address of
                        // a native definition, so it needs no target prototype.
                        self.native_effect_references.insert((index, query));
                        output.push_str(&format!("rusk_{}_{index}", query.name()));
                        continue;
                    }
                    if matches!(kind, template::CallKind::Value)
                        && result != "Unit"
                        && !self
                            .ir_types()
                            .get(&result)
                            .is_some_and(crate::hir::Type::scalar_value)
                    {
                        return Err(span.error(
                            "native value references require Unit or a declared scalar result",
                        ));
                    }
                    if self.functions[index].body.is_none()
                        && template::identifier(&self.functions[index].cpp)
                    {
                        self.native_references.insert(index);
                    }
                    self.used.insert(index);
                    if let Some(owner) = owner {
                        self.native_contract_dependencies
                            .entry(owner)
                            .or_default()
                            .insert(index);
                    }
                    match kind {
                        template::CallKind::Effect(query) => {
                            unreachable!("effect query handled above: {query:?}");
                        }
                        template::CallKind::Value => {
                            self.native_value_references.insert(index);
                            output.push_str(&format!("rusk_value_ref_{index}"));
                        }
                        template::CallKind::Rooted => {
                            self.native_rooted_references.insert(index);
                            self.abi_reference(index, true);
                            output.push_str(&format!("rusk_rooted_ref_{index}"));
                        }
                        template::CallKind::Object => {
                            self.native_owned_references.insert(index);
                            self.abi_reference(index, true);
                            output.push_str(&format!("rusk_owned_ref_{index}"));
                        }
                        template::CallKind::Owned => {
                            output.push_str(&self.abi_reference(index, false));
                        }
                        template::CallKind::Boxed => {
                            output.push_str(&self.abi_reference(index, true));
                        }
                    }
                }
            }
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            crate::parse(&format!("{TYPES}{DERIVE}"), "arbitrary-standard-name.rusk").unwrap(),
        );
        let user = Source::user(
            crate::parse(
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
            let types = Source::standard(crate::parse(TYPES, "types.rusk").unwrap());
            let statements = crate::parse(&definition, "<stdlib/collections.rusk>").unwrap();
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
        let mut declarations=crate::parse(include_str!("../../lib/collections.rusk"),"other-library.rusk").unwrap().into_iter()
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
            crate::parse(
                "enum Event{Empty,Present(Int),}let a:=Event::Present(1);let b=a;",
                "caller.rusk",
            )
            .unwrap(),
        );
        let types = Source::standard(crate::parse(TYPES, "types.rusk").unwrap());
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
            let mut declarations = crate::parse(
                include_str!("../../lib/collections.rusk"),
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
                        Source::standard(crate::parse(TYPES, "types.rusk").unwrap()),
                        Source::standard(declarations),
                        Source::user(crate::parse(source, "caller.rusk").unwrap()),
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
            let declarations = crate::parse(&source, "<stdlib/collections.rusk>").unwrap();
            let source = if standard {
                Source::standard(declarations)
            } else {
                Source::user(declarations)
            };
            let error = Compiler::default()
                .compile(
                    &[
                        Source::standard(crate::parse(TYPES, "types.rusk").unwrap()),
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
            let statements = crate::parse(&source, "<stdlib/collections.rusk>").unwrap();
            let declarations = if standard {
                Source::standard(statements)
            } else {
                Source::user(statements)
            };
            let error = Compiler::default()
                .compile(
                    &[
                        Source::standard(crate::parse(TYPES, "types.rusk").unwrap()),
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
        let mut declarations = crate::parse(
            include_str!("../../lib/collections.rusk"),
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
                    Source::standard(crate::parse(&types, "types.rusk").unwrap()),
                    Source::standard(declarations),
                    Source::user(crate::parse("in row:(Int,Int);", "caller.rusk").unwrap()),
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
            let statements = crate::parse(&source, "<stdlib/collections.rusk>").unwrap();
            let declarations = if standard {
                Source::standard(statements)
            } else {
                Source::user(statements)
            };
            let result = Compiler::default().compile(
                &[
                    Source::standard(crate::parse(&types, "types.rusk").unwrap()),
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
        let mut declarations = crate::parse(include_str!("../../lib/collections.rusk"), "other-standard.rusk")
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
                    Source::standard(crate::parse(&types, "types.rusk").unwrap()),
                    Source::standard(declarations),
                    Source::user(
                        crate::parse("in rows(2,3):Vec<Vec<Int>>;", "caller.rusk").unwrap(),
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
            let statements = crate::parse(&source, "<stdlib/collections.rusk>").unwrap();
            let source = if standard {
                Source::standard(statements)
            } else {
                Source::user(statements)
            };
            let error = Compiler::default()
                .compile(
                    &[
                        Source::standard(crate::parse(TYPES, "types.rusk").unwrap()),
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
        let mut declarations=crate::parse(include_str!("../../lib/collections.rusk"),"renamed-output-library.rusk").unwrap().into_iter().filter(|s| matches!(s,Stmt::GenericExternFunction(_,_,_,_,p) if p.get("derive").is_some_and(|v|v=="tuple_write"))).collect::<Vec<_>>();
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
            Source::standard(crate::parse(&types, "types.rusk").unwrap()),
            Source::standard(declarations),
            Source::user(crate::parse("__write__((1,2));", "caller.rusk").unwrap()),
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
                let statements = crate::parse(&source, "<stdlib/collections.rusk>").unwrap();
                let source = if standard {
                    Source::standard(statements)
                } else {
                    Source::user(statements)
                };
                let error = Compiler::default()
                    .compile(
                        &[
                            Source::standard(crate::parse(TYPES, "types.rusk").unwrap()),
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
        let mut declarations=crate::parse(include_str!("../../lib/collections.rusk"),"renamed-compare-library.rusk").unwrap().into_iter().filter(|s| matches!(s,Stmt::GenericExternFunction(_,_,_,_,p) if p.get("derive").is_some_and(|v|v=="tuple_eq"))).collect::<Vec<_>>();
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
                Source::standard(crate::parse(&types, "types.rusk").unwrap()),
                Source::standard(declarations.clone()),
                Source::user(crate::parse("__eq__((1,2),(1,3));", "caller.rusk").unwrap()),
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
        let mut declarations=crate::parse(include_str!("../../lib/collections.rusk"),"renamed-policy.rusk").unwrap().into_iter().filter(|s| matches!(s,Stmt::GenericExternFunction(_,_,_,_,p) if p.get("derive").is_some_and(|v|v=="tuple_eq"))).collect::<Vec<_>>();
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
                        Source::standard(crate::parse(&types, "types.rusk").unwrap()),
                        Source::standard(declarations.clone()),
                        Source::user(crate::parse(&user, "caller.rusk").unwrap()),
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
}
