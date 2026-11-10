//! Trusted physical and sharing information, independent of display/type names.
use super::*;
use crate::ir::hir::{Scalar, Type};

#[derive(Clone)]
pub(super) struct Metadata {
    pub kind: Type,
    pub span: Span,
}

pub(super) fn references(kind: &Type) -> Vec<&str> {
    let mut children = match kind {
        Type::Buffer(t) | Type::Vector(t) | Type::Set(t) | Type::Iterator { element: t, .. } => {
            vec![t.as_str()]
        }
        Type::Dict { key, value } => vec![key.as_str(), value.as_str()],
        _ => vec![],
    };
    children.extend(kind.retained_fields().iter().map(|(_, ty)| ty.as_str()));
    children
}

pub(super) fn parse_properties(
    name: &Name,
    properties: &BTreeMap<String, String>,
    origin: Origin,
) -> Result<Option<Type>, String> {
    let text = properties.get("representation");
    let retained = properties.get("retains");
    if text.is_none() && retained.is_none() {
        return Ok(None);
    }
    if origin != Origin::StandardLibrary {
        return Err(name
            .span
            .error("native type representations are reserved for the embedded standard library"));
    }
    let retained = retained
        .map(|text| parse_retained(text))
        .transpose()
        .map_err(|e| name.span.error(e))?
        .unwrap_or_default();
    let Some(text) = text else {
        return Ok(Some(Type::Shared(retained)));
    };
    if !retained.is_empty() && !text.starts_with("iterator<") {
        return Err(name
            .span
            .error("native retained members require an iterator or an opaque representation"));
    }
    let scalar = match text.as_str() {
        "i64" => Some(Scalar::I64),
        "bool" => Some(Scalar::Bool),
        "char" => Some(Scalar::Char),
        "unit" => Some(Scalar::Unit),
        _ => None,
    };
    if let Some(s) = scalar {
        return Ok(Some(Type::Scalar(s)));
    }
    let kind = match tuples::type_parts(text) {
        Some((role, args)) => match (role, args.as_slice()) {
            ("buffer", [t]) => Type::Buffer((*t).into()),
            ("vector", [t]) => Type::Vector((*t).into()),
            ("iterator", [t]) => Type::Iterator {
                element: (*t).into(),
                retained,
            },
            ("set", [t]) => Type::Set((*t).into()),
            ("dict", [key, value]) => Type::Dict {
                key: (*key).into(),
                value: (*value).into(),
            },
            _ => return Err(name.span.error("invalid native type representation")),
        },
        None => return Err(name.span.error("invalid native type representation")),
    };
    Ok(Some(kind))
}

fn parse_retained(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut fields = Vec::new();
    for item in text.split(';') {
        let (name, ty) = item
            .split_once(':')
            .ok_or("invalid native retained member")?;
        if !valid_member(name) || ty.is_empty() || fields.iter().any(|(n, _)| n == name) {
            return Err("invalid or duplicate native retained member".into());
        }
        fields.push((name.to_string(), ty.to_string()));
        if fields.len() > 64 {
            return Err("native retained member limit exceeded".into());
        }
    }
    Ok(fields)
}

pub(super) fn valid_member(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub(super) fn substitute(kind: &Type, bindings: &BTreeMap<String, String>) -> Type {
    let ty = |t: &str| generics::substitute(t, bindings);
    match kind {
        Type::Buffer(t) => Type::Buffer(ty(t)),
        Type::Vector(t) => Type::Vector(ty(t)),
        Type::Set(t) => Type::Set(ty(t)),
        Type::Iterator { element, retained } => Type::Iterator {
            element: ty(element),
            retained: retained
                .iter()
                .map(|(name, t)| (name.clone(), ty(t)))
                .collect(),
        },
        Type::Shared(retained) => Type::Shared(
            retained
                .iter()
                .map(|(name, t)| (name.clone(), ty(t)))
                .collect(),
        ),
        Type::Dict { key, value } => Type::Dict {
            key: ty(key),
            value: ty(value),
        },
        _ => kind.clone(),
    }
}

impl Compiler {
    pub(super) fn scalar_representation_checks(&self) -> String {
        let mut checks = String::new();
        for (name, meta) in &self.native_types {
            let Type::Scalar(kind) = meta.kind else {
                continue;
            };
            let native = &self.types[name];
            let plain = format!("!std::is_const_v<{native}> && !std::is_volatile_v<{native}>");
            let condition = match kind {
                Scalar::I64 => format!(
                    "std::is_integral_v<{native}> && std::is_signed_v<{native}> && sizeof({native}) == sizeof(std::int64_t) && {plain}"
                ),
                Scalar::Bool => format!("std::is_same_v<{native},bool>"),
                Scalar::Char => format!("std::is_same_v<{native},char>"),
                Scalar::Unit => {
                    format!("std::is_empty_v<{native}> && std::is_trivial_v<{native}> && {plain}")
                }
            };
            let message =
                serde_json::to_string(&format!("native representation mismatch for {name}"))
                    .unwrap();
            checks += &format!("static_assert({condition},{message});\n");
        }
        if checks.is_empty() {
            checks
        } else {
            format!("#include <type_traits>\n{checks}")
        }
    }
    pub(super) fn validate_representation(
        &mut self,
        name: &Name,
        kind: &Type,
    ) -> Result<(), String> {
        for child in references(kind) {
            self.check_type(&Name {
                text: child.into(),
                span: name.span.clone(),
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod retained_tests;
