//! Documentation generation
//!
//! This module implements AST analysis, data preparation, and documentation
//! generation.

use sv_parser::{NodeEvent, RefNode, SyntaxTree};

pub struct Doc<'a> {
    ast: &'a SyntaxTree,
}

impl<'a> Doc<'a> {
    /// Generate documentation from an AST.
    pub fn new(ast: &'a SyntaxTree) -> Self {
        let mut doc = Self { ast };
        doc.analyze();
        doc
    }

    /// Gather documentation in the AST.
    fn analyze(&mut self) {
        let mut comments = vec![];
        let mut stack = vec![];
        stack.push(Scope::default());

        // Visit the AST, gobble up comments, and process all nodes that make
        // their way into the documentation.
        for event in self.ast.into_iter().event() {
            match event {
                NodeEvent::Enter(node) => match node {
                    RefNode::Comment(comment) => {
                        let s = self.ast.get_str(&comment.nodes.0).unwrap();
                        if s.starts_with("//!") {
                            stack.last_mut().unwrap().comments.push(&s[3..]);
                        } else if s.starts_with("///") {
                            comments.push(&s[3..]);
                        }
                    }
                    RefNode::TypeDeclaration(..) | RefNode::ModuleDeclaration(..) => {
                        stack.push(Scope::new(node.clone(), std::mem::take(&mut comments)));
                    }
                    RefNode::SourceText(..)
                    | RefNode::WhiteSpace(..)
                    | RefNode::Locate(..)
                    | RefNode::Description(..)
                    | RefNode::DescriptionPackageItem(..)
                    | RefNode::PackageItem(..)
                    | RefNode::PackageOrGenerateItemDeclaration(..)
                    | RefNode::DataDeclaration(..) => (),
                    _ => {
                        if !comments.is_empty() {
                            debug!("Flushing unused comments");
                            comments.clear();
                        }
                        trace!("{:?}", node);
                    }
                },
                NodeEvent::Leave(node) => {
                    // If we are leaving the current node on the stack, pop that
                    // node off the stack and add it as a child to its parent.
                    if stack.last().and_then(|s| s.node.clone()) == Some(node) {
                        let n = stack.pop().unwrap();
                        stack.last_mut().unwrap().children.push(n);
                    }
                }
            }
        }
        assert_eq!(stack.len(), 1);
        let root = stack.into_iter().next().unwrap();

        debug!("Analyzed {:#?}", root);
    }
}

/// A documentation nesting level.
#[derive(Default, Debug)]
struct Scope<'a> {
    node: Option<RefNode<'a>>,
    comments: Vec<&'a str>,
    children: Vec<Scope<'a>>,
}

impl<'a> Scope<'a> {
    pub fn new(node: RefNode<'a>, comments: Vec<&'a str>) -> Self {
        Self {
            node: Some(node),
            comments,
            ..Default::default()
        }
    }
}
