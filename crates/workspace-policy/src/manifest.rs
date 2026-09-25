use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path},
};

use crate::syntax::{Value, parse_key_path, parse_value, split_assignment, statements};

#[derive(Clone, Debug)]
pub(crate) enum DependencyDeclaration {
    Path,
    ThirdParty { default_features_disabled: bool },
    Inherited { default_features: Option<bool> },
}

#[derive(Debug, Default)]
pub(crate) struct Manifest {
    pub(crate) members: Option<Vec<String>>,
    pub(crate) nonempty_default_features: bool,
    default_features_seen: bool,
    pub(crate) workspace_dependencies: BTreeMap<String, DependencyDeclaration>,
    pub(crate) dependencies: Vec<(String, DependencyDeclaration)>,
}

#[derive(Clone, Copy)]
enum Section {
    Other,
    Workspace,
    WorkspaceDependencies,
    Features,
    Dependencies,
}

pub(crate) fn parse_manifest(input: &str) -> Result<Manifest, String> {
    let mut manifest = Manifest::default();
    let mut section = Section::Other;
    let mut section_path = Vec::new();
    let (mut headers, mut dependencies) = (BTreeSet::new(), BTreeSet::new());

    for statement in statements(input)? {
        if statement.starts_with('[') {
            (section, section_path) = parse_section(&statement)?;
            if !headers.insert(section_path.clone()) {
                return Err("duplicate table header".to_owned());
            }
            continue;
        }

        match section {
            Section::Workspace => parse_workspace_entry(&statement, &mut manifest)?,
            Section::WorkspaceDependencies => {
                let (name, dependency) = parse_dependency_entry(&statement)?;
                if manifest
                    .workspace_dependencies
                    .insert(name, dependency)
                    .is_some()
                {
                    return Err("duplicate workspace dependency".to_owned());
                }
            }
            Section::Features => parse_feature_entry(&statement, &mut manifest)?,
            Section::Dependencies => {
                let dependency = parse_dependency_entry(&statement)?;
                if !dependencies.insert((section_path.clone(), dependency.0.clone())) {
                    return Err("duplicate dependency declaration".to_owned());
                }
                manifest.dependencies.push(dependency);
            }
            Section::Other => {}
        }
    }

    Ok(manifest)
}

pub(crate) fn dependency_uses_defaults(
    name: &str,
    declaration: &DependencyDeclaration,
    workspace_dependencies: &BTreeMap<String, DependencyDeclaration>,
) -> Result<bool, String> {
    match declaration {
        DependencyDeclaration::Path => Ok(false),
        DependencyDeclaration::ThirdParty {
            default_features_disabled,
        } => Ok(!default_features_disabled),
        DependencyDeclaration::Inherited { default_features } => {
            let inherited = workspace_dependencies
                .get(name)
                .ok_or_else(|| format!("workspace dependency `{name}` is not declared"))?;
            match inherited {
                DependencyDeclaration::Path => Ok(false),
                DependencyDeclaration::ThirdParty {
                    default_features_disabled,
                } => Ok(!default_features_disabled || *default_features == Some(true)),
                DependencyDeclaration::Inherited { .. } => {
                    Err("workspace dependencies cannot inherit from themselves".to_owned())
                }
            }
        }
    }
}

pub(crate) fn validate_member_path(member: &str) -> Result<(), String> {
    if member.is_empty() || member.chars().any(|character| matches!(character, '*' | '?' | '[')) {
        return Err("empty and glob workspace members are unsupported".to_owned());
    }
    if Path::new(member)
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_)))
    {
        return Err("workspace member must remain beneath the workspace root".to_owned());
    }
    Ok(())
}

fn parse_section(statement: &str) -> Result<(Section, Vec<String>), String> {
    if statement.starts_with("[[") {
        return Err("array-of-table headers are unsupported".to_owned());
    }
    let inner = statement
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .ok_or_else(|| "malformed table header".to_owned())?;
    let path = parse_key_path(inner)?;

    let section = if path.as_slice() == ["workspace"] {
        Section::Workspace
    } else if path.as_slice() == ["workspace", "dependencies"] {
        Section::WorkspaceDependencies
    } else if path.as_slice() == ["features"] {
        Section::Features
    } else if is_dependency_section(&path) {
        Section::Dependencies
    } else if path.iter().any(|part| is_dependency_name(part)) {
        return Err("dependency table syntax is unsupported".to_owned());
    } else {
        Section::Other
    };
    Ok((section, path))
}

fn parse_workspace_entry(statement: &str, manifest: &mut Manifest) -> Result<(), String> {
    let (key, value) = split_assignment(statement)?;
    if parse_single_key(key)? != "members" {
        return Ok(());
    }
    if manifest.members.is_some() {
        return Err("duplicate workspace.members declaration".to_owned());
    }
    let Value::Array(values) = parse_value(value)? else {
        return Err("workspace.members must be an array".to_owned());
    };
    let mut members = Vec::new();
    for value in values {
        let Value::String(member) = value else {
            return Err("workspace.members entries must be strings".to_owned());
        };
        members.push(member);
    }
    manifest.members = Some(members);
    Ok(())
}

fn parse_feature_entry(statement: &str, manifest: &mut Manifest) -> Result<(), String> {
    let (key, value) = split_assignment(statement)?;
    if parse_single_key(key)? != "default" {
        return Ok(());
    }
    if manifest.default_features_seen {
        return Err("duplicate features.default declaration".to_owned());
    }
    manifest.default_features_seen = true;
    let Value::Array(features) = parse_value(value)? else {
        return Err("features.default must be an array".to_owned());
    };
    if !features
        .iter()
        .all(|feature| matches!(feature, Value::String(_)))
    {
        return Err("features.default entries must be strings".to_owned());
    }
    manifest.nonempty_default_features = !features.is_empty();
    Ok(())
}

fn parse_dependency_entry(statement: &str) -> Result<(String, DependencyDeclaration), String> {
    let (key, value) = split_assignment(statement)?;
    let name = parse_single_key(key)?;
    let declaration = match parse_value(value)? {
        Value::String(_) => DependencyDeclaration::ThirdParty {
            default_features_disabled: false,
        },
        Value::Table(fields) => parse_dependency_table(&fields)?,
        Value::Bool(_) | Value::Array(_) => {
            return Err("unsupported dependency declaration".to_owned());
        }
    };
    Ok((name, declaration))
}

fn parse_dependency_table(
    fields: &BTreeMap<String, Value>,
) -> Result<DependencyDeclaration, String> {
    validate_dependency_fields(fields)?;
    let has_path = fields.contains_key("path");
    let workspace = optional_bool(fields, "workspace")?;
    let default_features = optional_bool(fields, "default-features")?;
    let has_external_source = ["version", "git", "registry", "registry-index"]
        .iter()
        .any(|key| fields.contains_key(*key));

    if has_path && workspace.is_some() {
        return Err("path and workspace dependency sources cannot be combined".to_owned());
    }
    if has_path {
        return Ok(DependencyDeclaration::Path);
    }
    if let Some(inherited) = workspace {
        if !inherited || has_external_source {
            return Err("invalid workspace dependency inheritance".to_owned());
        }
        return Ok(DependencyDeclaration::Inherited { default_features });
    }
    if !has_external_source {
        return Err("dependency source is missing".to_owned());
    }
    Ok(DependencyDeclaration::ThirdParty {
        default_features_disabled: default_features == Some(false),
    })
}

fn validate_dependency_fields(fields: &BTreeMap<String, Value>) -> Result<(), String> {
    for (key, value) in fields {
        let valid = match key.as_str() {
            "version" | "git" | "registry" | "registry-index" | "path" | "package"
            | "branch" | "tag" | "rev" => matches!(value, Value::String(_)),
            "default-features" | "workspace" | "optional" | "public" => {
                matches!(value, Value::Bool(_))
            }
            "features" => matches!(value, Value::Array(values) if values.iter().all(|item| matches!(item, Value::String(_)))),
            _ => false,
        };
        if !valid {
            return Err(format!("unsupported dependency field `{key}`"));
        }
    }
    Ok(())
}

fn optional_bool(fields: &BTreeMap<String, Value>, key: &str) -> Result<Option<bool>, String> {
    match fields.get(key) {
        None => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(format!("dependency field `{key}` must be boolean")),
    }
}

fn parse_single_key(input: &str) -> Result<String, String> {
    let path = parse_key_path(input)?;
    if path.len() != 1 {
        return Err("dotted assignment keys are unsupported".to_owned());
    }
    path.into_iter()
        .next()
        .ok_or_else(|| "empty key".to_owned())
}

fn is_dependency_section(path: &[String]) -> bool {
    (path.len() == 1 && is_dependency_name(&path[0]))
        || (path.len() == 3 && path[0] == "target" && is_dependency_name(&path[2]))
}

fn is_dependency_name(name: &str) -> bool {
    matches!(name, "dependencies" | "dev-dependencies" | "build-dependencies")
}
