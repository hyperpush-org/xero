//! React Native / Expo project detection.
//!
//! Checks whether the current project is an RN or Expo app and whether
//! Metro is running. Used by the inspector UI to determine whether to
//! show the "Inspect" toggle.

use std::path::Path;

/// Check if the given project root is a React Native or Expo project
/// by looking for `react-native` or `expo` in package.json dependencies.
pub fn detect_rn_project(project_root: &Path) -> bool {
    let pkg_path = project_root.join("package.json");
    let Ok(contents) = std::fs::read_to_string(&pkg_path) else {
        return false;
    };
    let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&contents) else {
        return false;
    };

    has_dependency(&pkg, "react-native") || has_dependency(&pkg, "expo")
}

fn has_dependency(pkg: &serde_json::Value, name: &str) -> bool {
    for section in &["dependencies", "devDependencies", "peerDependencies"] {
        if pkg.get(section).and_then(|v| v.get(name)).is_some() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detects_react_native_in_dependencies() {
        let pkg = json!({ "dependencies": { "react-native": "0.75.0" } });
        assert!(has_dependency(&pkg, "react-native"));
        assert!(!has_dependency(&pkg, "expo"));
    }

    #[test]
    fn detects_expo_in_dependencies() {
        let pkg = json!({ "dependencies": { "expo": "~51.0.0", "react": "18.2.0" } });
        assert!(has_dependency(&pkg, "expo"));
    }

    #[test]
    fn returns_false_for_non_rn_project() {
        let pkg = json!({ "dependencies": { "express": "4.18.0" } });
        assert!(!has_dependency(&pkg, "react-native"));
        assert!(!has_dependency(&pkg, "expo"));
    }
}
