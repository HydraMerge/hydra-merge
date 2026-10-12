//! Branch names grouped into folders by `/`. Pure, no egui.

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Node<'a> {
    Leaf {
        /// Last path segment, shown in the row.
        label: &'a str,
        /// The whole branch name, used for actions.
        full: &'a str,
    },
    Folder {
        name: &'a str,
        /// `feature/ui`: unique within one tree, used as the collapse state id.
        path: &'a str,
        children: Vec<Node<'a>>,
        /// Leaves below this folder at any depth.
        count: usize,
    },
}

/// Folders first, then leaves, each group in name order.
pub(super) fn build_tree<'a>(names: &[&'a str]) -> Vec<Node<'a>> {
    build_level(names, 0)
}

/// `names` share their first `skip` bytes (a folder prefix ending with `/`).
fn build_level<'a>(names: &[&'a str], skip: usize) -> Vec<Node<'a>> {
    let mut folders: Vec<(&'a str, Vec<&'a str>)> = Vec::new();
    let mut leaves: Vec<Node<'a>> = Vec::new();
    for full in names {
        let rest = &full[skip..];
        match rest.split_once('/') {
            Some((head, _)) if !head.is_empty() => {
                match folders.iter_mut().find(|(n, _)| *n == head) {
                    Some((_, members)) => members.push(full),
                    None => folders.push((head, vec![full])),
                }
            }
            _ => leaves.push(Node::Leaf { label: rest, full }),
        }
    }
    folders.sort_by(|a, b| a.0.cmp(b.0));
    leaves.sort_by(|a, b| leaf_label(a).cmp(leaf_label(b)));
    let mut nodes: Vec<Node<'a>> = folders
        .into_iter()
        .map(|(name, members)| {
            let path = &members[0][..skip + name.len()];
            Node::Folder {
                name,
                path,
                count: members.len(),
                children: build_level(&members, skip + name.len() + 1),
            }
        })
        .collect();
    nodes.extend(leaves);
    nodes
}

fn leaf_label<'a>(node: &Node<'a>) -> &'a str {
    match node {
        Node::Leaf { label, .. } => label,
        Node::Folder { name, .. } => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_by_slash_with_folders_first() {
        let names = ["test", "sre/b", "app", "sre/a", "feat/ui/x", "feat/api"];
        let tree = build_tree(&names);
        let top: Vec<&str> = tree.iter().map(leaf_label).collect();
        assert_eq!(top, vec!["feat", "sre", "app", "test"]);
        let Node::Folder {
            path,
            children,
            count,
            ..
        } = &tree[0]
        else {
            panic!("feat is a folder");
        };
        assert_eq!((*path, *count), ("feat", 2));
        assert_eq!(
            children.iter().map(leaf_label).collect::<Vec<_>>(),
            vec!["ui", "api"]
        );
        let Node::Folder { path, children, .. } = &children[0] else {
            panic!("feat/ui is a folder");
        };
        assert_eq!(*path, "feat/ui");
        assert_eq!(
            children[0],
            Node::Leaf {
                label: "x",
                full: "feat/ui/x"
            }
        );
    }

    #[test]
    fn empty_and_flat_lists() {
        assert!(build_tree(&[]).is_empty());
        let flat = build_tree(&["b", "a"]);
        assert_eq!(
            flat.iter().map(leaf_label).collect::<Vec<_>>(),
            vec!["a", "b"]
        );
    }
}
