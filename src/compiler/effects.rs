use super::*;

impl Compiler {
    pub(super) fn record_native_dependency(
        &mut self,
        owner: usize,
        dependency: usize,
        route: Option<&crate::native::effects::composition::Specification>,
        span: &Span,
    ) -> Result<(), String> {
        self.native_info_mut(owner)
            .dependencies
            .get_or_insert_with(BTreeSet::new);
        if let Some(route) = route {
            if self.functions[owner].origin != Origin::StandardLibrary {
                return Err(span
                    .error("native call mappings are reserved for the embedded standard library"));
            }
            let outer = &self.functions[owner];
            let child = &self.functions[dependency];
            let mapped = route
                .resolve(
                    dependency,
                    &outer.params,
                    &outer.result,
                    &child.params,
                    &child.result,
                    &self.ir_types(),
                )
                .map_err(|e| span.error(e))?;
            let mappings = &mut self.native_info_mut(owner).mapped_dependencies;
            if !mappings.contains(&mapped) {
                if mappings.len() >= 4096 {
                    return Err(span.error("native call mapping work limit exceeded"));
                }
                mappings.push(mapped);
            }
        } else {
            self.native_info_mut(owner)
                .dependencies
                .get_or_insert_with(BTreeSet::new)
                .insert(dependency);
        }
        Ok(())
    }

    pub(super) fn validate_lowering_source(
        &self,
        name: &Name,
        properties: &BTreeMap<String, String>,
        origin: Origin,
    ) -> Result<(), String> {
        if [
            "lower",
            "lower_owned",
            "lower_borrowed",
            "lower_rooted",
            "operation",
        ]
        .iter()
        .any(|p| properties.contains_key(*p))
            && origin != Origin::StandardLibrary
        {
            return Err(name.span.error(
                "native lowering definitions are reserved for the embedded standard library",
            ));
        }
        Ok(())
    }

    pub(super) fn register_native_lowering(
        &mut self,
        index: usize,
        name: &Name,
        params: &[String],
        result: &str,
        properties: &BTreeMap<String, String>,
        origin: Origin,
    ) -> Result<(), String> {
        self.validate_lowering_source(name, properties, origin)?;
        let parse = |key| {
            properties
                .get(key)
                .map(|s| {
                    crate::native::lowering::Template::parse(s, params.len())
                        .map_err(|e| name.span.error(e))
                })
                .transpose()
        };
        let scalar = parse("lower")?;
        let owned = parse("lower_owned")?;
        let borrowed = parse("lower_borrowed")?;
        let rooted = parse("lower_rooted")?;
        if scalar.is_some()
            && !self
                .ir_types()
                .get(result)
                .is_some_and(crate::ir::hir::Type::leaf)
        {
            return Err(name
                .span
                .error("scalar lowering requires a declared scalar representation"));
        }
        if (scalar.is_some() && owned.is_some())
            || ((borrowed.is_some() || rooted.is_some()) && owned.is_none())
        {
            return Err(name
                .span
                .error("incompatible native lowering representations"));
        }
        let assignment = match properties.get("operation").map(String::as_str) {
            None => false,
            Some("assign")
                if scalar.is_some()
                    && result == "Unit"
                    && params.len() == 2
                    && params[0] == params[1]
                    && properties
                        .get("effects")
                        .is_some_and(|e| e == "write_first") =>
            {
                true
            }
            Some(descriptor) if descriptor.starts_with("enum_") => {
                self.register_enum_operation(index, name, descriptor, params, result, properties)?;
                false
            }
            _ => return Err(name.span.error("invalid native operation descriptor")),
        };
        if scalar.is_some() || owned.is_some() {
            self.native_info_mut(index).lowering = Some(crate::native::lowering::Lowering {
                scalar,
                owned,
                borrowed,
                rooted,
                assignment,
            });
        }
        Ok(())
    }

    pub(super) fn declared_native_effects(
        &self,
        name: &Name,
        parameters: &[String],
        result: &str,
        properties: &BTreeMap<String, String>,
        origin: Origin,
    ) -> Result<Option<crate::analysis::effects::Summary>, String> {
        if properties.contains_key("aliases") && origin != Origin::StandardLibrary {
            return Err(name
                .span
                .error("native alias contracts are reserved for the embedded standard library"));
        }
        let Some(specification) = properties.get("effects") else {
            if properties.contains_key("aliases") {
                return Err(name.span.error("native alias contract requires effects"));
            }
            return Ok(None);
        };
        if origin != Origin::StandardLibrary {
            return Err(name
                .span
                .error("native effect contracts are reserved for the embedded standard library"));
        }
        let types = self.ir_types();
        let mut summary =
            crate::native::effects::contract(specification, parameters, result, &types)
                .map_err(|error| name.span.error(error))?;
        if let Some(aliases) = properties.get("aliases") {
            crate::native::effects::aliases::apply(
                &mut summary,
                aliases,
                parameters,
                result,
                &types,
            )
            .map_err(|error| name.span.error(error))?;
        }
        Ok(Some(summary))
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod mapped_tests;
