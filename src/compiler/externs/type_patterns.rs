//! Overlap of native type declarations, with independent variables per declaration.
use super::*;

#[derive(Clone)]
enum Term {
    Variable(usize),
    Type(String, Vec<Term>),
}

pub(super) fn pattern(name: &Name, parameters: &[Name]) -> String {
    if tuples::type_parts(&name.text).is_some() || parameters.is_empty() {
        name.text.clone()
    } else {
        format!(
            "{}<{}>",
            name.text,
            parameters
                .iter()
                .map(|p| p.text.as_str())
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

fn term(text: &str, parameters: &[Name], offset: usize) -> Term {
    if let Some(index) = parameters.iter().position(|p| p.text == text) {
        Term::Variable(offset + index)
    } else if let Some((name, arguments)) = tuples::type_parts(text) {
        Term::Type(
            name.into(),
            arguments
                .iter()
                .map(|a| term(a, parameters, offset))
                .collect(),
        )
    } else {
        Term::Type(text.into(), vec![])
    }
}

pub(in crate::compiler) fn validate(name: &Name, parameters: &[Name]) -> Result<(), String> {
    if !template::valid_type(&pattern(name, parameters)) {
        return Err(name.span.error("invalid native type pattern"));
    }
    Ok(())
}

struct Solver {
    bindings: Vec<Option<Term>>,
    work: usize,
}

impl Solver {
    fn step(&mut self, depth: usize) -> Result<(), ()> {
        if depth > 64 || self.work >= 4096 {
            return Err(());
        }
        self.work += 1;
        Ok(())
    }
    fn occurs(&mut self, variable: usize, value: &Term, depth: usize) -> Result<bool, ()> {
        self.step(depth)?;
        match value {
            Term::Variable(index) => {
                if *index == variable {
                    return Ok(true);
                }
                match self.bindings[*index].clone() {
                    Some(bound) => self.occurs(variable, &bound, depth + 1),
                    None => Ok(false),
                }
            }
            Term::Type(_, arguments) => {
                for argument in arguments {
                    if self.occurs(variable, argument, depth + 1)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }
    fn unify(&mut self, left: &Term, right: &Term, depth: usize) -> Result<bool, ()> {
        self.step(depth)?;
        if let Term::Variable(index) = left
            && let Some(bound) = self.bindings[*index].clone()
        {
            return self.unify(&bound, right, depth + 1);
        }
        if let Term::Variable(index) = right
            && let Some(bound) = self.bindings[*index].clone()
        {
            return self.unify(left, &bound, depth + 1);
        }
        match (left, right) {
            (Term::Variable(a), Term::Variable(b)) if a == b => Ok(true),
            (Term::Variable(index), value) | (value, Term::Variable(index)) => {
                if self.occurs(*index, value, depth + 1)? {
                    Ok(false)
                } else {
                    self.bindings[*index] = Some(value.clone());
                    Ok(true)
                }
            }
            (Term::Type(a, arguments), Term::Type(b, actual)) => {
                if a != b || arguments.len() != actual.len() {
                    return Ok(false);
                }
                for (a, b) in arguments.iter().zip(actual) {
                    if !self.unify(a, b, depth + 1)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
        }
    }
}

pub(in crate::compiler) fn overlap(
    a: &Name,
    ap: &[Name],
    b: &Name,
    bp: &[Name],
) -> Result<bool, String> {
    Solver {
        bindings: vec![None; ap.len() + bp.len()],
        work: 0,
    }
    .unify(
        &term(&pattern(a, ap), ap, 0),
        &term(&pattern(b, bp), bp, ap.len()),
        0,
    )
    .map_err(|()| {
        b.span
            .error("native type pattern comparison limit exceeded")
    })
}
