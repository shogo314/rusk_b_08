use crate::ast::{Span, Stmt};
use std::{
    borrow::Cow,
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};

pub struct Loaded {
    pub statements: Vec<Stmt>,
    pub dependencies: Vec<PathBuf>,
}

enum Files<'a> {
    Disk,
    Embedded(&'a [(&'a str, &'a str)]),
}
impl Files<'_> {
    fn canonical(&self, path: &Path) -> Result<PathBuf, String> {
        if matches!(self, Self::Disk) {
            return path.canonicalize().map_err(|e| e.to_string());
        }
        let mut normalized = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Normal(name) => normalized.push(name),
                Component::CurDir => {}
                Component::ParentDir if normalized.pop() => {}
                _ => return Err("embedded resource path escapes its root".into()),
            }
        }
        Ok(normalized)
    }
    fn read(&self, path: &Path) -> Result<Cow<'_, str>, String> {
        match self {
            Self::Disk => std::fs::read_to_string(path)
                .map(Cow::Owned)
                .map_err(|e| e.to_string()),
            Self::Embedded(files) => files
                .iter()
                .find(|(name, _)| Path::new(name) == path)
                .map(|(_, text)| Cow::Borrowed(*text))
                .ok_or_else(|| format!("unknown embedded resource '{}'", path.display())),
        }
    }
    fn display(&self, path: &Path) -> String {
        match self {
            Self::Disk => path.display().to_string(),
            Self::Embedded(_) => format!("<stdlib/{}>", path.display()),
        }
    }
}

struct Loader<'a> {
    files: Files<'a>,
    loaded: BTreeSet<PathBuf>,
    active: Vec<PathBuf>,
    dependencies: BTreeSet<PathBuf>,
}
impl<'a> Loader<'a> {
    fn new(files: Files<'a>) -> Self {
        Self {
            files,
            loaded: BTreeSet::new(),
            active: vec![],
            dependencies: BTreeSet::new(),
        }
    }
    fn expand(
        &mut self,
        path: &Path,
        from: Option<&Span>,
        output: &mut Vec<Stmt>,
    ) -> Result<(), String> {
        let error = |message: String| {
            from.map_or_else(
                || format!("{}: {message}", self.files.display(path)),
                |span| span.error(&message),
            )
        };
        let canonical = self
            .files
            .canonical(path)
            .map_err(|e| error(format!("cannot load '{}': {e}", path.display())))?;
        if self.active.contains(&canonical) {
            let mut cycle = self
                .active
                .iter()
                .map(|p| self.files.display(p))
                .collect::<Vec<_>>();
            cycle.push(self.files.display(&canonical));
            return Err(error(format!("include cycle: {}", cycle.join(" -> "))));
        }
        if self.loaded.contains(&canonical) {
            return Ok(());
        }
        let text = self
            .files
            .read(&canonical)
            .map_err(|e| error(format!("cannot read '{}': {e}", canonical.display())))?;
        let statements = crate::parse(&text, &self.files.display(&canonical))?;
        self.active.push(canonical.clone());
        for statement in statements {
            match statement {
                Stmt::Include(name, span) => {
                    self.expand(&canonical.parent().unwrap().join(name), Some(&span), output)?
                }
                Stmt::Symbol(name, mut properties) => {
                    let file = properties.remove("code_file");
                    let files = properties.remove("code_files");
                    if file.is_some() && files.is_some() {
                        return Err(name
                            .span
                            .error("native symbol cannot specify both code_file and code_files"));
                    }
                    let resources = if let Some(file) = file {
                        Some(vec![file])
                    } else if let Some(files) = files {
                        let paths = files
                            .split(';')
                            .take(4097)
                            .map(|p| p.trim().to_string())
                            .collect::<Vec<_>>();
                        if paths.len() > 4096 || paths.iter().any(String::is_empty) {
                            return Err(name.span.error(
                                "invalid native code_files list or resource limit exceeded",
                            ));
                        }
                        Some(paths)
                    } else {
                        None
                    };
                    if let Some(resources) = resources {
                        if properties.contains_key("code") {
                            return Err(name.span.error(
                                "native symbol cannot combine code with code_file or code_files",
                            ));
                        }
                        let mut code = String::new();
                        for resource in resources {
                            let path = canonical.parent().unwrap().join(resource);
                            let path = self.files.canonical(&path).map_err(|e| {
                                name.span.error(format!("cannot load native code: {e}"))
                            })?;
                            let text = self.files.read(&path).map_err(|e| {
                                name.span.error(format!(
                                    "cannot read native code '{}': {e}",
                                    path.display()
                                ))
                            })?;
                            code.push_str(&text);
                            self.dependencies.insert(path);
                        }
                        properties.insert("code".into(), code);
                    }
                    output.push(Stmt::Symbol(name, properties));
                }
                statement => output.push(statement),
            }
        }
        self.active.pop();
        self.loaded.insert(canonical.clone());
        self.dependencies.insert(canonical);
        Ok(())
    }
    fn load(mut self, path: &Path) -> Result<Loaded, String> {
        let mut statements = vec![];
        self.expand(path, None, &mut statements)?;
        Ok(Loaded {
            statements,
            dependencies: self.dependencies.into_iter().collect(),
        })
    }
}

pub fn load(path: &Path) -> Result<Loaded, String> {
    Loader::new(Files::Disk).load(path)
}
pub(crate) fn embedded(root: &str, files: &[(&str, &str)]) -> Result<Loaded, String> {
    Loader::new(Files::Embedded(files)).load(Path::new(root))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_includes_and_native_resources_share_relative_resolution_and_deduplication() {
        let files = [
            (
                "prelude.rusk",
                "include \"dir/library.rusk\";include \"dir/../dir/library.rusk\";",
            ),
            (
                "dir/library.rusk",
                "extern symbol Code{code_file:\"native.hpp\",}",
            ),
            (
                "dir/native.hpp",
                "literal /* RUSK_IO_RUNTIME */ @{T} {Int}\n",
            ),
        ];
        let loaded = embedded("prelude.rusk", &files).unwrap();
        assert_eq!(loaded.statements.len(), 1);
        assert_eq!(loaded.dependencies.len(), 3);
        let Stmt::Symbol(name, properties) = &loaded.statements[0] else {
            panic!()
        };
        assert_eq!(name.span.file, "<stdlib/dir/library.rusk>");
        assert_eq!(properties["code"], files[2].1);
        assert!(!properties.contains_key("code_file"));
    }
    #[test]
    fn embedded_resources_cannot_fall_back_to_the_filesystem_and_cycles_keep_locations() {
        for source in [
            "include \"../outside.rusk\";",
            "extern symbol Code{code_file:\"../outside.hpp\",}",
            "extern symbol Code{code_file:\"unknown.hpp\",}",
        ] {
            let error = embedded("root.rusk", &[("root.rusk", source)])
                .err()
                .unwrap();
            assert!(error.contains("<stdlib/root.rusk>:1:"), "{error}");
        }
        let error = embedded(
            "a.rusk",
            &[
                ("a.rusk", "include \"b.rusk\";"),
                ("b.rusk", "include \"a.rusk\";"),
            ],
        )
        .err()
        .unwrap();
        assert!(
            error.contains("include cycle") && error.contains("<stdlib/b.rusk>:1:"),
            "{error}"
        );
    }
}

#[cfg(test)]
mod resource_list_tests {
    use super::*;
    #[test]
    fn ordered_resource_lists_preserve_bytes_and_track_each_file() {
        let files = [
            (
                "root.rusk",
                "extern symbol Data{code_files:\"left.hpp;right.hpp\",}",
            ),
            ("left.hpp", "left"),
            ("right.hpp", "right\n"),
        ];
        let loaded = embedded("root.rusk", &files).unwrap();
        let Stmt::Symbol(_, properties) = &loaded.statements[0] else {
            panic!()
        };
        assert_eq!(properties["code"], "leftright\n");
        assert_eq!(loaded.dependencies.len(), 3);
        let declaration = format!(
            "extern symbol Data{{code_files:\"{}\",}}",
            vec!["left.hpp"; 4097].join(";")
        );
        let error = embedded("root.rusk", &[("root.rusk", &declaration)])
            .err()
            .unwrap();
        assert!(error.contains("resource limit exceeded"), "{error}");
        for declaration in [
            "extern symbol Bad{code_files:\"\",}",
            "extern symbol Bad{code_files:\"left.hpp;\",}",
            "extern symbol Bad{code_file:\"left.hpp\",code_files:\"right.hpp\",}",
            "extern symbol Bad{code:\"code\",code_files:\"left.hpp\",}",
        ] {
            assert!(embedded("root.rusk", &[("root.rusk", declaration)]).is_err());
        }
    }
}
