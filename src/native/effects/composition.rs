//! Explicit argument/result routes for calls made by trusted native bodies.
use super::*;
use crate::{
    analysis::effects::{ParameterEffects, Projection, SummarySource},
    ir::hir::Type,
};

#[derive(Clone)]
pub(crate) struct Specification {
    arguments: Vec<(usize, Vec<String>)>,
    result: Destination<Vec<String>>,
}

#[derive(Clone, PartialEq, Eq)]
enum Destination<P> {
    Discard,
    Return(P),
    Store { parameter: usize, path: P },
}

impl Specification {
    /// `via (0.source,1) into return.cached`, `into 0.cached`, or `into discard`.
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let (arguments, result) = text
            .split_once(" into ")
            .ok_or("native call mapping requires into")?;
        let arguments = arguments
            .strip_prefix('(')
            .and_then(|s| s.strip_suffix(')'))
            .ok_or("native call mapping requires an argument list")?;
        let arguments = if arguments.is_empty() {
            vec![]
        } else {
            arguments
                .split(',')
                .take(65)
                .map(|a| {
                    let parsed = aliases::parse(&format!("return={}", a.trim()), usize::MAX)?;
                    let (_, parameter, path) = parsed.into_iter().next().unwrap();
                    Ok((parameter, path))
                })
                .collect::<Result<Vec<_>, String>>()?
        };
        if arguments.len() > 64 {
            return Err("native call mapping argument limit exceeded".into());
        }
        let result = if result == "discard" {
            Destination::Discard
        } else if result == "return" || result.starts_with("return.") {
            Destination::Return(aliases::parse(&format!("{result}=0"), 1)?.remove(0).0)
        } else {
            let (_, parameter, path) =
                aliases::parse(&format!("return={result}"), usize::MAX)?.remove(0);
            Destination::Store { parameter, path }
        };
        Ok(Self { arguments, result })
    }

    pub(crate) fn validate_bounds(&self, count: usize) -> Result<(), String> {
        if self.arguments.iter().any(|(p, _)| *p >= count) {
            return Err("native call mapping argument out of range".into());
        }
        if let Destination::Store { parameter, .. } = self.result
            && parameter >= count
        {
            return Err("native call mapping destination out of range".into());
        }
        Ok(())
    }

    pub(crate) fn validate_arity(&self, count: usize) -> Result<(), String> {
        if self.arguments.len() != count {
            return Err("native call mapping arity mismatch".into());
        }
        Ok(())
    }

    pub(crate) fn resolve(
        &self,
        function: usize,
        outer_parameters: &[String],
        outer_result: &str,
        parameters: &[String],
        result: &str,
        types: &BTreeMap<String, Type>,
    ) -> Result<Dependency, String> {
        self.validate_bounds(outer_parameters.len())?;
        self.validate_arity(parameters.len())?;
        let arguments = self
            .arguments
            .iter()
            .zip(parameters)
            .map(|((parameter, path), expected)| {
                let path = aliases::resolve(&outer_parameters[*parameter], path, types)?;
                if crate::analysis::effects::project_type(
                    &outer_parameters[*parameter],
                    &path,
                    types,
                )? != *expected
                {
                    return Err("native call mapping argument type mismatch".into());
                }
                Ok((*parameter, path))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let project = |root: &str, path: &[String]| -> Result<Vec<Projection>, String> {
            let path = aliases::resolve(root, path, types)?;
            if crate::analysis::effects::project_type(root, &path, types)? != result {
                return Err("native call mapping result type mismatch".into());
            }
            Ok(path)
        };
        let result_path = match &self.result {
            Destination::Discard => Destination::Discard,
            Destination::Return(path) => Destination::Return(project(outer_result, path)?),
            Destination::Store { parameter, path } => Destination::Store {
                parameter: *parameter,
                path: project(&outer_parameters[*parameter], path)?,
            },
        };
        Ok(Dependency {
            function,
            arguments,
            result: result_path,
        })
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Dependency {
    pub function: usize,
    arguments: Vec<(usize, Vec<Projection>)>,
    result: Destination<Vec<Projection>>,
}

/// Only resolved child effects can discharge a native dependency. Unknown
/// externs and native call cycles remain opaque during the fixed point;
/// returned provenance can conservatively widen without losing known effects.
pub(crate) fn compose(
    base: &Summary,
    dependencies: &[Dependency],
    summaries: &[Summary],
) -> Option<Summary> {
    let mut result = base.clone();
    for dependency in dependencies {
        let child = &summaries[dependency.function];
        if !matches!(
            child.source,
            SummarySource::Inferred | SummarySource::NativeContract
        ) || [
            child.io,
            child.external_write,
            child.may_fail,
            child.may_diverge,
        ]
        .contains(&Effect::Unknown)
            || child.parameters.iter().any(|p| {
                [p.read, p.write, p.structural, p.escape, p.retained].contains(&Effect::Unknown)
            })
        {
            return None;
        }
        result.io = result.io.max(child.io);
        result.external_write = result.external_write.max(child.external_write);
        result.may_fail = result.may_fail.max(child.may_fail);
        result.may_diverge = result.may_diverge.max(child.may_diverge);
        for ((parameter, _), effect) in dependency.arguments.iter().zip(&child.parameters) {
            let parent = &mut result.parameters[*parameter];
            // Parameter effects are currently whole-argument may sets. A
            // projected write/retention must conservatively affect its root.
            let passed = ParameterEffects {
                escape: effect.retained,
                ..effect.clone()
            };
            parent.read = parent.read.max(passed.read);
            parent.write = parent.write.max(passed.write);
            parent.structural = parent.structural.max(passed.structural);
            parent.escape = parent.escape.max(passed.escape);
            parent.retained = parent.retained.max(passed.retained);
        }
        if let Destination::Return(target) = &dependency.result {
            if child.returned.unknown {
                result.returned.unknown = true;
                for parameter in &mut result.parameters {
                    parameter.escape = parameter.escape.max(Effect::May);
                }
            }
            for alias in &child.returned.aliases {
                let (parameter, input) = &dependency.arguments[alias.parameter];
                let mut source = input.clone();
                source.extend(alias.parameter_path.iter().cloned());
                let mut path = target.clone();
                path.extend(alias.result_path.iter().cloned());
                if source.len() > 16 || path.len() > 16 {
                    return None;
                }
                if path.is_empty() {
                    result.returned.fresh = false;
                }
                result.returned.aliases.insert(ReturnAlias {
                    result_path: path,
                    parameter: *parameter,
                    parameter_path: source,
                });
                result.parameters[*parameter].escape =
                    result.parameters[*parameter].escape.max(Effect::May);
            }
        }
        if let Destination::Store { parameter, .. } = &dependency.result {
            // The body stores the returned object or its contents into an
            // existing argument. Root destinations denote content assignment,
            // never rebinding the caller's argument reference. Effects are
            // whole-argument may sets until a native heap model is available.
            let target = &mut result.parameters[*parameter];
            target.read = target.read.max(Effect::May);
            target.write = target.write.max(Effect::May);
            target.structural = target.structural.max(Effect::May);
            if child.returned.unknown {
                for parameter in &mut result.parameters {
                    parameter.escape = parameter.escape.max(Effect::May);
                    parameter.retained = parameter.retained.max(Effect::May);
                }
            } else {
                for alias in &child.returned.aliases {
                    let parameter = dependency.arguments[alias.parameter].0;
                    let effect = &mut result.parameters[parameter];
                    effect.escape = effect.escape.max(Effect::May);
                    effect.retained = effect.retained.max(Effect::May);
                }
            }
        }
        if result.returned.aliases.len() > 64 {
            return None;
        }
    }
    // Native bodies have no CFG/heap model. A direct write or a child can
    // replace a routed member with another argument, after which later writes
    // and retention no longer necessarily affect the original route's root.
    // Widen across arguments as well as returned origins. Legacy dependencies
    // without routes retain their existing whole-operation contracts.
    if !dependencies.is_empty()
        && (result.external_write != Effect::No
            || result
                .parameters
                .iter()
                .any(|p| p.write != Effect::No || p.structural != Effect::No))
    {
        let structural = if result.external_write != Effect::No {
            Effect::May
        } else {
            result
                .parameters
                .iter()
                .map(|p| p.structural)
                .max()
                .unwrap_or(Effect::No)
        };
        let retained = result
            .parameters
            .iter()
            .map(|p| p.retained)
            .max()
            .unwrap_or(Effect::No);
        for parameter in &mut result.parameters {
            parameter.write = parameter.write.max(Effect::May);
            parameter.structural = parameter.structural.max(structural);
            parameter.retained = parameter.retained.max(retained);
            parameter.escape = parameter.escape.max(Effect::May);
        }
        result.returned.unknown = true;
    }
    if result.returned.unknown {
        result.returned = crate::analysis::effects::ReturnOrigins {
            unknown: true,
            ..Default::default()
        };
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn types() -> BTreeMap<String, Type> {
        [
            ("Int".into(), Type::Scalar(crate::ir::hir::Scalar::I64)),
            ("Other".into(), Type::Scalar(crate::ir::hir::Scalar::I64)),
            (
                "Box".into(),
                Type::Shared(vec![("value".into(), "Int".into())]),
            ),
            (
                "Outer".into(),
                Type::Shared(vec![("child".into(), "Box".into())]),
            ),
        ]
        .into()
    }
    #[test]
    fn mapped_calls_compose_nested_aliases_and_keep_discarded_returns_local() {
        let types = types();
        let parameters = vec!["Outer".into()];
        let base = contract("read_fresh", &parameters, "Outer", &types).unwrap();
        let mut child = contract("read_fresh", &["Box".into()], "Box", &types).unwrap();
        aliases::apply(
            &mut child,
            "return.value=0.value",
            &["Box".into()],
            "Box",
            &types,
        )
        .unwrap();
        let routed = Specification::parse("(0.child) into return.child")
            .unwrap()
            .resolve(0, &parameters, "Outer", &["Box".into()], "Box", &types)
            .unwrap();
        let result = compose(&base, &[routed], &[child.clone()]).unwrap();
        result
            .validate_signature(&parameters, "Outer", &types, false)
            .unwrap();
        assert!(result.returned.fresh);
        assert_eq!(result.parameters[0].retained, Effect::No);
        assert!(result.returned.aliases.contains(&ReturnAlias {
            result_path: vec![Projection::Retained(0), Projection::Retained(0)],
            parameter: 0,
            parameter_path: vec![Projection::Retained(0), Projection::Retained(0)]
        }));
        let discard = Specification::parse("(0.child) into discard")
            .unwrap()
            .resolve(0, &parameters, "Outer", &["Box".into()], "Box", &types)
            .unwrap();
        let result = compose(&base, &[discard], &[child]).unwrap();
        assert_eq!(result.parameters[0].escape, Effect::No);
        assert!(result.returned.aliases.is_empty());
    }
    #[test]
    fn mapped_calls_propagate_mutation_retention_io_failure_and_uncertainty() {
        let types = types();
        let parameters = vec!["Outer".into()];
        let base = contract("read_fresh", &parameters, "Outer", &types).unwrap();
        let routed = Specification::parse("(0.child) into return.child")
            .unwrap()
            .resolve(0, &parameters, "Outer", &["Box".into()], "Box", &types)
            .unwrap();
        let mut child = contract("read_fresh", &["Box".into()], "Box", &types).unwrap();
        child.parameters[0].write = Effect::May;
        child.parameters[0].structural = Effect::May;
        child.parameters[0].retained = Effect::May;
        child.parameters[0].escape = Effect::May;
        child.io = Effect::May;
        child.may_fail = Effect::May;
        child.may_diverge = Effect::May;
        let result = compose(&base, std::slice::from_ref(&routed), &[child]).unwrap();
        result
            .validate_signature(&parameters, "Outer", &types, false)
            .unwrap();
        assert_eq!(result.parameters[0].retained, Effect::May);
        assert_eq!(result.parameters[0].write, Effect::May);
        assert_eq!(result.parameters[0].structural, Effect::May);
        assert_eq!(result.io, Effect::May);
        assert_eq!(result.may_fail, Effect::May);
        assert!(result.returned.unknown);
        assert!(result.returned.aliases.is_empty());
        assert!(compose(&base, std::slice::from_ref(&routed), &[Summary::opaque(1)]).is_none());
        assert!(compose(&base, &[routed], &[Summary::unanalyzed(1)]).is_none());
    }
    #[test]
    fn mapped_base_writes_widen_later_effects_across_all_input_roots() {
        let types = types();
        let parameters = vec!["Outer".into(), "Outer".into()];
        let base = contract("checked_mutate_fresh", &parameters, "Outer", &types).unwrap();
        let mut child = contract("read_fresh", &["Box".into()], "Box", &types).unwrap();
        child.parameters[0].retained = Effect::May;
        child.parameters[0].escape = Effect::May;
        let route = Specification::parse("(0.child) into return.child")
            .unwrap()
            .resolve(0, &parameters, "Outer", &["Box".into()], "Box", &types)
            .unwrap();
        let result = compose(&base, &[route], &[child]).unwrap();
        result
            .validate_signature(&parameters, "Outer", &types, false)
            .unwrap();
        assert_eq!(result.parameters[1].write, Effect::May);
        assert_eq!(result.parameters[1].retained, Effect::May);
        assert!(result.returned.unknown);
        assert_eq!(compose(&base, &[], &[]), Some(base));
    }

    #[test]
    fn mapped_stored_results_retain_aliases_but_independent_copies_do_not() {
        let types = types();
        let parameters = vec!["Outer".into(), "Box".into()];
        let base = contract("read_fresh", &parameters, "Unit", &types).unwrap();
        let mut shared = contract("read_fresh", &["Box".into()], "Box", &types).unwrap();
        aliases::apply(
            &mut shared,
            "return.value=0.value",
            &["Box".into()],
            "Box",
            &types,
        )
        .unwrap();
        let route = Specification::parse("(1) into 0.child")
            .unwrap()
            .resolve(0, &parameters, "Unit", &["Box".into()], "Box", &types)
            .unwrap();
        let result = compose(&base, std::slice::from_ref(&route), &[shared]).unwrap();
        result
            .validate_signature(&parameters, "Unit", &types, false)
            .unwrap();
        assert!(result.parameters.iter().all(|p| p.retained == Effect::May));
        assert!(result.parameters.iter().all(|p| p.write == Effect::May));
        assert!(result.returned.aliases.is_empty());

        let independent = contract("read_fresh", &["Box".into()], "Box", &types).unwrap();
        let result = compose(&base, std::slice::from_ref(&route), &[independent]).unwrap();
        assert!(result.parameters.iter().all(|p| p.retained == Effect::No));
        assert_eq!(result.parameters[0].structural, Effect::May);

        let mut unknown = contract("read_fresh", &["Box".into()], "Box", &types).unwrap();
        unknown.returned = crate::analysis::effects::ReturnOrigins {
            unknown: true,
            ..Default::default()
        };
        let result = compose(&base, std::slice::from_ref(&route), &[unknown]).unwrap();
        assert!(result.parameters.iter().all(|p| p.retained == Effect::May));
        assert!(compose(&base, &[route], &[Summary::opaque(1)]).is_none());
    }

    #[test]
    fn mapped_storage_validates_root_contents_nested_fields_and_logical_types() {
        let types = types();
        let parameters = vec!["Outer".into(), "Box".into()];
        Specification::parse("(1) into 1")
            .unwrap()
            .resolve(0, &parameters, "Unit", &["Box".into()], "Box", &types)
            .unwrap();
        Specification::parse("(1.value) into 0.child.value")
            .unwrap()
            .resolve(0, &parameters, "Unit", &["Int".into()], "Int", &types)
            .unwrap();
        for target in ["2.child", "0.missing", "0", "0.child.value"] {
            assert!(
                Specification::parse(&format!("(1) into {target}"))
                    .unwrap()
                    .resolve(0, &parameters, "Unit", &["Box".into()], "Box", &types)
                    .is_err(),
                "{target}"
            );
        }
        assert!(
            Specification::parse("(1.value) into 0.child.value")
                .unwrap()
                .resolve(0, &parameters, "Unit", &["Int".into()], "Other", &types)
                .is_err()
        );
        for target in ["0.", "-1.child", "0..child"] {
            assert!(Specification::parse(&format!("(1) into {target}")).is_err());
        }
        assert!(
            Specification::parse("(0) into 1")
                .unwrap()
                .validate_bounds(1)
                .is_err()
        );
    }

    #[test]
    fn mapped_call_routes_validate_shapes_logical_types_arity_and_limits() {
        let types = types();
        let parameters = vec!["Outer".into()];
        for text in [
            "0.child into return.child",
            "(0.child)",
            "(0.child) into value",
            "(0.) into return",
            "(0.child,) into return",
        ] {
            assert!(Specification::parse(text).is_err(), "{text}");
        }
        for text in [
            "(1.child) into return.child",
            "() into return.child",
            "(0.missing) into return.child",
            "(0.child) into return",
        ] {
            assert!(
                Specification::parse(text)
                    .unwrap()
                    .resolve(0, &parameters, "Outer", &["Box".into()], "Box", &types)
                    .is_err(),
                "{text}"
            );
        }
        assert!(
            Specification::parse(&format!("({}) into return", vec!["0"; 65].join(","))).is_err()
        );
        assert!(
            Specification::parse("(0.child.value) into return")
                .unwrap()
                .resolve(0, &parameters, "Other", &["Int".into()], "Int", &types)
                .is_err()
        );
    }
}
