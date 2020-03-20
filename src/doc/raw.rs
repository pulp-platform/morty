//! Raw documentation analysis
//!
//! This module implements the first level of syntax tree analysis to associate
//! doc comments with items in the SV source files.

use sv_parser::{NodeEvent, RefNode, SyntaxTree};

/// Raw documentation information.
pub struct RawDoc<'a> {
    /// The syntax tree for which the documentation was generated.
    pub ast: &'a SyntaxTree,
    /// The root documentation scope.
    pub root: Scope<'a>,
}

impl<'a> RawDoc<'a> {
    /// Generate raw documentation from an AST.
    pub fn new(ast: &'a SyntaxTree) -> Self {
        let mut comments = vec![];
        let mut stack = vec![];
        #[derive(PartialEq)]
        enum LastComment {
            None,
            Local,
            Parent,
        }
        let mut last_comment = LastComment::None;
        stack.push(Scope::default());

        // Visit the AST, gobble up comments, and process all nodes that make
        // their way into the documentation.
        for event in ast.into_iter().event() {
            match event {
                NodeEvent::Enter(node) => match node {
                    RefNode::Comment(comment) => {
                        let s = ast.get_str(&comment.nodes.0).unwrap();
                        if s.starts_with("//!") {
                            let comments = &mut stack.last_mut().unwrap().comments;
                            if !comments.is_empty() && last_comment != LastComment::Parent {
                                comments.push("");
                            }
                            last_comment = LastComment::Parent;
                            comments.push(&s[3..]);
                        } else if s.starts_with("///") {
                            if !comments.is_empty() && last_comment != LastComment::Local {
                                comments.push("");
                            }
                            last_comment = LastComment::Local;
                            comments.push(&s[3..]);
                        }
                    }
                    RefNode::TypeDeclaration(..) | RefNode::ModuleDeclaration(..) => {
                        last_comment = LastComment::None;
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
                        last_comment = LastComment::None;
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

        Self { ast, root }
    }
}

/// A documentation nesting level.
#[derive(Default, Debug)]
pub struct Scope<'a> {
    /// The node in the syntax tree.
    pub node: Option<RefNode<'a>>,
    /// Comments associated with this node.
    pub comments: Vec<&'a str>,
    /// Subscopes with additional documentation nodes.
    pub children: Vec<Scope<'a>>,
}

impl<'a> Scope<'a> {
    /// Create a new documentation scope.
    fn new(node: RefNode<'a>, comments: Vec<&'a str>) -> Self {
        Self {
            node: Some(node),
            comments,
            ..Default::default()
        }
    }
}
