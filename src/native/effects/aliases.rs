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
mod tests {
    use super::*;
    fn types() -> BTreeMap<String, Type> {
        [
            ("Int".into(), Type::Scalar(crate::ir::hir::Scalar::I64)),
            ("Other".into(), Type::Scalar(crate::ir::hir::Scalar::I64)),
            ("Unit".into(), Type::Scalar(crate::ir::hir::Scalar::Unit)),
            (
                "Source".into(),
                Type::Shared(vec![("value".into(), "Int".into())]),
            ),
            (
                "Cursor".into(),
                Type::Iterator {
                    element: "Int".into(),
                    retained: vec![("owner".into(), "Source".into())],
                },
            ),
        ]
        .into()
    }
    #[test]
    fn aliases_preserve_nested_owner_identity_and_distinguish_return_escape_from_retention() {
        let types = types();
        let parameters = vec!["Cursor".into()];
        let mut summary = contract("read_fresh", &parameters, "Cursor", &types).unwrap();
        apply(
            &mut summary,
            "return.owner=0.owner",
            &parameters,
            "Cursor",
            &types,
        )
        .unwrap();
        assert!(summary.returned.fresh);
        assert_eq!(summary.parameters[0].escape, Effect::May);
        assert_eq!(summary.parameters[0].retained, Effect::No);
        assert!(summary.returned.aliases.contains(&ReturnAlias {
            result_path: vec![Projection::Retained(0)],
            parameter: 0,
            parameter_path: vec![Projection::Retained(0)]
        }));
        let mut identity = contract("read_fresh", &parameters, "Cursor", &types).unwrap();
        apply(&mut identity, "return=0", &parameters, "Cursor", &types).unwrap();
        assert!(!identity.returned.fresh);
        let mut value = contract("read_fresh", &parameters, "Int", &types).unwrap();
        apply(
            &mut value,
            "return=0.owner.value",
            &parameters,
            "Int",
            &types,
        )
        .unwrap();
        assert_eq!(
            value.returned.aliases.first().unwrap().parameter_path,
            vec![Projection::Retained(0), Projection::Retained(0)]
        );
    }
    #[test]
    fn aliases_validate_logical_types_paths_bounds_and_unknown_effects() {
        let types = types();
        let parameters = vec!["Cursor".into()];
        for text in ["return.owner=0", "return.missing=0.owner", "return=0.owner"] {
            let mut summary = contract("read_fresh", &parameters, "Cursor", &types).unwrap();
            assert!(
                apply(&mut summary, text, &parameters, "Cursor", &types).is_err(),
                "{text}"
            );
        }
        for text in [
            "",
            "return.=0",
            "return=0.",
            "return.owner=1",
            "value=0",
            "return=+0",
            "return=0..owner",
        ] {
            assert!(parse(text, 1).is_err(), "{text}");
        }
        assert!(parse(&vec!["return=0"; 65].join(";"), 1).is_err());
        assert!(parse(&format!("return=0.{}", vec!["value"; 17].join(".")), 1).is_err());
        let mut summary = contract("read_fresh", &parameters, "Other", &types).unwrap();
        assert!(
            apply(
                &mut summary,
                "return=0.owner.value",
                &parameters,
                "Other",
                &types
            )
            .is_err()
        );
        let mut opaque = Summary::opaque(1);
        apply(&mut opaque, "return=0", &parameters, "Cursor", &types).unwrap();
        assert_eq!(opaque, Summary::opaque(1));
    }
}
