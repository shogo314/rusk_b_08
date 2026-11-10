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
mod tests;
