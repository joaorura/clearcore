//! Enforces repository-wide Cargo manifest policy.

mod manifest;
mod syntax;

use std::{fs, io, path::Path};

use manifest::{
    DependencyDeclaration, dependency_uses_defaults, parse_manifest, validate_member_path,
};

const WORKSPACE_MEMBER: &str = "<workspace>";

/// A repository-wide Cargo manifest policy violation.
#[derive(Debug, Eq, PartialEq)]
pub enum PolicyViolation {
    /// A workspace package enables one or more default features.
    NonemptyDefaultFeatures { member: String },
    /// A third-party dependency allows its upstream default features.
    DependencyUsesDefaultFeatures { member: String, dependency: String },
}

/// Checks the root workspace policy and every registered member manifest.
///
/// # Errors
/// Returns an I/O error when a manifest cannot be read or its policy-relevant TOML cannot be
/// interpreted unambiguously.
pub fn check_workspace(workspace_root: &Path) -> io::Result<Vec<PolicyViolation>> {
    let root_path = workspace_root.join("Cargo.toml");
    let root = read_manifest(&root_path)?;
    let members = root
        .members
        .as_ref()
        .ok_or_else(|| invalid_data(&root_path, "workspace.members is required"))?;
    let mut violations = Vec::new();

    for (dependency, declaration) in &root.workspace_dependencies {
        match declaration {
            DependencyDeclaration::Path
            | DependencyDeclaration::ThirdParty {
                default_features_disabled: true,
            } => {}
            DependencyDeclaration::ThirdParty {
                default_features_disabled: false,
            } => violations.push(PolicyViolation::DependencyUsesDefaultFeatures {
                member: WORKSPACE_MEMBER.to_owned(),
                dependency: dependency.clone(),
            }),
            DependencyDeclaration::Inherited { .. } => {
                return Err(invalid_data(
                    &root_path,
                    "workspace dependencies cannot inherit from themselves",
                ));
            }
        }
    }

    for member in members {
        validate_member_path(member).map_err(|error| invalid_data(&root_path, &error))?;
        let manifest_path = workspace_root.join(member).join("Cargo.toml");
        let manifest = read_manifest(&manifest_path)?;

        if manifest.nonempty_default_features {
            violations.push(PolicyViolation::NonemptyDefaultFeatures {
                member: member.clone(),
            });
        }

        for (dependency, declaration) in &manifest.dependencies {
            if dependency_uses_defaults(dependency, declaration, &root.workspace_dependencies)
                .map_err(|error| invalid_data(&manifest_path, &error))?
            {
                violations.push(PolicyViolation::DependencyUsesDefaultFeatures {
                    member: member.clone(),
                    dependency: dependency.clone(),
                });
            }
        }
    }

    Ok(violations)
}

/// Reports whether a manifest declares nonempty default features or cannot be parsed safely.
#[must_use]
pub fn has_nonempty_default_features(manifest: &str) -> bool {
    match parse_manifest(manifest) {
        Ok(parsed) => parsed.nonempty_default_features,
        Err(_) => true,
    }
}

fn read_manifest(path: &Path) -> io::Result<manifest::Manifest> {
    let content = fs::read_to_string(path)?;
    parse_manifest(&content).map_err(|error| invalid_data(path, &error))
}

fn invalid_data(path: &Path, error: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{}: {error}", path.display()),
    )
}
