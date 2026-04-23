//! Selector matching against a [`UiTree`].
//!
//! The rules are strict: every field populated on the selector must match
//! the candidate node's corresponding field. Traversal is depth-first so
//! "first match" is deterministic across runs — agents rely on that
//! stability to avoid flaky scripts.

use super::{Selector, UiNode, UiTree};

/// Find all nodes matching `selector` in depth-first order.
pub fn find(tree: &UiTree, selector: &Selector) -> Vec<UiNode> {
    let mut out = Vec::new();
    collect(&tree.root, selector, &mut out);
    out
}

fn collect(node: &UiNode, selector: &Selector, out: &mut Vec<UiNode>) {
    if matches(node, selector) {
        out.push(node.clone());
    }
    for child in &node.children {
        collect(child, selector, out);
    }
}

pub fn matches(node: &UiNode, selector: &Selector) -> bool {
    if let Some(id) = &selector.id {
        if node.id.as_deref() != Some(id.as_str()) {
            return false;
        }
    }
    if let Some(label) = &selector.label {
        if node.label.as_deref() != Some(label.as_str()) {
            return false;
        }
    }
    if let Some(role) = &selector.role {
        if !node.role.eq_ignore_ascii_case(role)
            && node.platform_role.as_deref() != Some(role.as_str())
        {
            return false;
        }
    }
    if let Some(text) = &selector.text {
        if node.value.as_deref() != Some(text.as_str()) {
            return false;
        }
    }
    if let Some(needle) = &selector.contains {
        let hay_label = node.label.as_deref().unwrap_or("");
        let hay_value = node.value.as_deref().unwrap_or("");
        if !hay_label.contains(needle.as_str()) && !hay_value.contains(needle.as_str()) {
            return false;
        }
    }
    if let Some(true) = selector.visible {
        if !node.enabled || node.bounds.w <= 0 || node.bounds.h <= 0 {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::super::Bounds;
    use super::*;

    fn make_tree() -> UiTree {
        UiTree {
            root: UiNode {
                id: None,
                role: "root".to_string(),
                label: None,
                value: None,
                enabled: true,
                focused: false,
                bounds: Bounds {
                    x: 0,
                    y: 0,
                    w: 100,
                    h: 100,
                },
                platform_role: None,
                children: vec![
                    UiNode {
                        id: Some("ok".to_string()),
                        role: "button".to_string(),
                        label: Some("OK".to_string()),
                        value: None,
                        enabled: true,
                        focused: false,
                        bounds: Bounds {
                            x: 10,
                            y: 10,
                            w: 40,
                            h: 20,
                        },
                        platform_role: Some("android.widget.Button".to_string()),
                        children: vec![],
                    },
                    UiNode {
                        id: None,
                        role: "textfield".to_string(),
                        label: Some("Search".to_string()),
                        value: Some("hello".to_string()),
                        enabled: false,
                        focused: false,
                        bounds: Bounds {
                            x: 50,
                            y: 10,
                            w: 40,
                            h: 20,
                        },
                        platform_role: None,
                        children: vec![],
                    },
                ],
            },
        }
    }

    #[test]
    fn matches_by_label_exact() {
        let tree = make_tree();
        let hits = find(
            &tree,
            &Selector {
                label: Some("OK".to_string()),
                ..Default::default()
            },
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id.as_deref(), Some("ok"));
    }

    #[test]
    fn matches_by_contains_case_sensitive() {
        let tree = make_tree();
        let hits = find(
            &tree,
            &Selector {
                contains: Some("hello".to_string()),
                ..Default::default()
            },
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].role, "textfield");
    }

    #[test]
    fn matches_by_platform_role_or_canonical_role() {
        let tree = make_tree();
        let hits_canonical = find(
            &tree,
            &Selector {
                role: Some("button".to_string()),
                ..Default::default()
            },
        );
        let hits_platform = find(
            &tree,
            &Selector {
                role: Some("android.widget.Button".to_string()),
                ..Default::default()
            },
        );
        assert_eq!(hits_canonical.len(), 1);
        assert_eq!(hits_platform.len(), 1);
    }

    #[test]
    fn visible_filter_excludes_disabled() {
        let tree = make_tree();
        let hits = find(
            &tree,
            &Selector {
                role: Some("textfield".to_string()),
                visible: Some(true),
                ..Default::default()
            },
        );
        assert!(
            hits.is_empty(),
            "disabled textfield should not match visible=true"
        );
    }
}
