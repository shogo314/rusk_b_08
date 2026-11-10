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
        self.native_info_mut(index).enum_operation = Some(operation);
        Ok(())
    }

    pub(super) fn guarded_enum_field(&self, test: usize, getter: usize) -> Option<(usize, usize)> {
        for id in [test, getter] {
            if self.functions[id].origin != Origin::StandardLibrary
                || self.native_dependencies(id).is_some_and(|d| !d.is_empty())
                || !self.native_mappings(id).is_empty()
            {
                return None;
            }
        }
        let Operation::Test(tested) = self.native_info.get(&test)?.enum_operation? else {
            return None;
        };
        let Operation::Field { variant, field } = self.native_info.get(&getter)?.enum_operation?
        else {
            return None;
        };
        (tested == variant && self.functions[test].params == self.functions[getter].params)
            .then_some((variant, field))
    }
}

#[cfg(test)]
mod tests;
