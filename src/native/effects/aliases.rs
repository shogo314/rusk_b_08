//! Typed returned-sharing contracts; native source text is never inspected.
use super::*;
use crate::{analysis::effects::Projection, ir::hir::Type};

type Mapping = (Vec<String>, usize, Vec<String>);

pub(crate) fn parse(text: &str, parameters: usize) -> Result<Vec<Mapping>, String> {
    fn path(text: &str) -> Result<Vec<String>, String> {
        if text.is_empty() {
            return Ok(vec![]);
        }
        let parts = text.split('.').collect::<Vec<_>>();
        if parts.len() > 16
            || parts.iter().any(|p| {
                let mut chars = p.chars();
                !chars
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                    || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
            })
        {
            return Err("invalid native alias path".into());
        }
        Ok(parts.into_iter().map(str::to_owned).collect())
    }
    let mut mappings = vec![];
    for mapping in text.split(';') {
        let (target, source) = mapping
            .split_once('=')
            .ok_or("invalid native alias mapping")?;
        let target = if target == "return" {
            vec![]
        } else {
            path(
                target
                    .strip_prefix("return.")
                    .ok_or("native alias target must be return")?,
            )?
        };
        let (argument, suffix) = source.split_once('.').unwrap_or((source, ""));
        if argument.is_empty() || !argument.bytes().all(|c| c.is_ascii_digit()) {
            return Err("invalid native alias argument".into());
        }
        let argument = argument
            .parse::<usize>()
            .map_err(|_| "invalid native alias argument")?;
        if argument >= parameters {
            return Err("native alias argument out of range".into());
        }
        // A trailing separator does not designate the root.
        if target.is_empty() && mapping.starts_with("return.") || source.ends_with('.') {
            return Err("invalid native alias path".into());
        }
        mappings.push((target, argument, path(suffix)?));
        if mappings.len() > 64 {
            return Err("native alias mapping limit exceeded".into());
        }
    }
    Ok(mappings)
}

pub(crate) fn resolve(
    root: &str,
    path: &[String],
    types: &BTreeMap<String, Type>,
) -> Result<Vec<Projection>, String> {
    let mut ty = root.to_owned();
    let mut projections = vec![];
    for name in path {
        let kind = types.get(&ty).ok_or("unknown native alias type")?;
        let field = kind
            .retained_fields()
            .iter()
            .position(|(n, _)| n == name)
            .map(Projection::Retained)
            .or_else(|| {
                kind.fields()
                    .and_then(|fields| fields.iter().position(|(n, _)| n == name))
                    .map(Projection::Field)
            })
            .ok_or("unknown native alias member")?;
        ty = crate::analysis::effects::project_type(&ty, std::slice::from_ref(&field), types)?;
        projections.push(field);
    }
    Ok(projections)
}

pub(crate) fn apply(
    base: &mut Summary,
    text: &str,
    parameters: &[String],
    result: &str,
    types: &BTreeMap<String, Type>,
) -> Result<(), String> {
    let mut candidate = base.clone();
    for (target, argument, source) in parse(text, parameters.len())? {
        let target = resolve(result, &target, types)?;
        let source = resolve(&parameters[argument], &source, types)?;
        if target.is_empty() {
            candidate.returned.fresh = false;
        }
        candidate.returned.aliases.insert(ReturnAlias {
            result_path: target,
            parameter: argument,
            parameter_path: source,
        });
        candidate.parameters[argument].escape = Effect::May;
    }
    // A conditional deep contract may already be opaque. Sharing annotations
    // must not turn its unknown effects into a trusted implementation.
    if base.source != crate::analysis::effects::SummarySource::Opaque {
        candidate.validate_signature(parameters, result, types, false)?;
        *base = candidate;
    } else {
        for alias in candidate.returned.aliases {
            if crate::analysis::effects::project_type(result, &alias.result_path, types)?
                != crate::analysis::effects::project_type(
                    &parameters[alias.parameter],
                    &alias.parameter_path,
                    types,
                )?
            {
                return Err("effect summary alias type mismatch".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
