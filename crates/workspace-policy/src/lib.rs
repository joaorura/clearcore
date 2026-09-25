//! Enforces repository-wide Cargo manifest policy.

use std::{fs, io, path::Path};

/// A repository-wide Cargo manifest policy violation.
#[derive(Debug, Eq, PartialEq)]
pub enum PolicyViolation {
    /// A workspace package enables one or more default features.
    NonemptyDefaultFeatures { member: String },
    /// A third-party dependency allows its upstream default features.
    DependencyUsesDefaultFeatures { member: String, dependency: String },
}

/// Checks every registered workspace member manifest.
///
/// # Errors
/// Returns an I/O error when the root or a registered member manifest cannot be read.
pub fn check_workspace(workspace_root: &Path) -> io::Result<Vec<PolicyViolation>> {
    let workspace_manifest = fs::read_to_string(workspace_root.join("Cargo.toml"))?;
    let mut violations = Vec::new();

    for member in workspace_members(&workspace_manifest) {
        let manifest = fs::read_to_string(workspace_root.join(&member).join("Cargo.toml"))?;

        if has_nonempty_default_features(&manifest) {
            violations.push(PolicyViolation::NonemptyDefaultFeatures {
                member: member.clone(),
            });
        }

        for dependency in dependencies_using_default_features(&manifest) {
            violations.push(PolicyViolation::DependencyUsesDefaultFeatures {
                member: member.clone(),
                dependency,
            });
        }
    }

    Ok(violations)
}

/// Reports whether a manifest declares a nonempty `default` feature.
#[must_use]
pub fn has_nonempty_default_features(manifest: &str) -> bool {
    let mut in_features = false;

    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            in_features = line == "[features]";
            continue;
        }

        if !in_features {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        if key.trim() == "default" {
            return value.trim() != "[]";
        }
    }

    false
}

fn workspace_members(manifest: &str) -> Vec<String> {
    let mut in_workspace = false;
    let mut member_value = String::new();
    let mut collecting_members = false;

    for line in manifest.lines().map(strip_comment).map(str::trim) {
        if line.starts_with('[') && !collecting_members {
            in_workspace = line == "[workspace]";
            continue;
        }

        if in_workspace && !collecting_members {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if key.trim() != "members" {
                continue;
            }
            collecting_members = true;
            member_value.push_str(value);
        } else if collecting_members {
            member_value.push_str(line);
        }

        if collecting_members && member_value.contains(']') {
            break;
        }
    }

    quoted_values(&member_value)
}

fn dependencies_using_default_features(manifest: &str) -> Vec<String> {
    let mut in_dependencies = false;
    let mut violations = Vec::new();
    let mut pending: Option<(String, String, usize)> = None;

    for line in manifest.lines().map(strip_comment).map(str::trim) {
        if let Some((dependency, declaration, depth)) = pending.as_mut() {
            declaration.push_str(line);
            *depth += brace_depth(line);
            if *depth == 0 {
                if third_party_defaults_enabled(declaration) {
                    violations.push(dependency.clone());
                }
                pending = None;
            }
            continue;
        }

        if line.starts_with('[') {
            in_dependencies = is_dependency_section(line);
            continue;
        }

        if !in_dependencies || line.is_empty() {
            continue;
        }

        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let dependency = name.trim().trim_matches('"').to_owned();
        let depth = brace_depth(value);
        if depth > 0 {
            pending = Some((dependency, value.to_owned(), depth));
        } else if third_party_defaults_enabled(value) {
            violations.push(dependency);
        }
    }

    violations
}

fn strip_comment(line: &str) -> &str {
    line.split_once('#').map_or(line, |(content, _)| content)
}

fn quoted_values(value: &str) -> Vec<String> {
    value
        .split('"')
        .enumerate()
        .filter_map(|(index, part)| (index % 2 == 1).then(|| part.to_owned()))
        .collect()
}

fn is_dependency_section(line: &str) -> bool {
    let section = line.trim_matches(['[', ']']);
    matches!(section, "dependencies" | "dev-dependencies" | "build-dependencies")
        || section.ends_with(".dependencies")
        || section.ends_with(".dev-dependencies")
        || section.ends_with(".build-dependencies")
}

fn brace_depth(value: &str) -> usize {
    value.chars().fold(0, |depth, character| match character {
        '{' => depth + 1,
        '}' => depth.saturating_sub(1),
        _ => depth,
    })
}

fn third_party_defaults_enabled(declaration: &str) -> bool {
    let compact: String = declaration
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    let is_path_dependency = compact.starts_with('{') && compact.contains("path=");

    !is_path_dependency && !compact.contains("default-features=false")
}
