//! Documentation generation
//!
//! This module implements AST analysis, data preparation, and documentation
//! generation.

use crate::ParsedFile;
use sv_parser::{self as sv, RefNode};

mod html;
mod raw;
pub use html::*;
pub use raw::*;

/// Documentation generated for a syntax tree.
pub struct Doc<'a> {
    /// The raw documentation.
    pub raw: Vec<(&'a ParsedFile, RawDoc<'a>)>,
    /// The documentation items.
    pub data: Context,
}

impl<'a> Doc<'a> {
    /// Generate documentation from an AST.
    pub fn new(files: impl IntoIterator<Item = &'a ParsedFile>) -> Self {
        // Process the files into raw documentation.
        let raw: Vec<_> = files
            .into_iter()
            .map(|pf| (pf, RawDoc::new(&pf.ast)))
            .collect();

        // Analyze the files into proper documentation nodes.
        let mut data = Context::default();
        for (_pf, raw) in &raw {
            data.analyze_scopes(&raw, &raw.root.children);
        }
        debug!("{:#?}", data);

        // Package up.
        Self { raw, data }
    }
}

#[derive(Default, Debug)]
pub struct Context {
    params: Vec<ParamItem>,
    ports: Vec<PortItem>,
    types: Vec<TypeItem>,
    modules: Vec<ModuleItem>,
    packages: Vec<PackageItem>,
    vars: Vec<VarItem>,
}

impl Context {
    fn analyze_scope(&mut self, raw: &RawDoc, scope: &Scope) {
        let node = match &scope.node {
            Some(n) => n,
            None => return,
        };
        match node {
            RefNode::PackageDeclaration(decl) => {
                self.packages
                    .push(PackageItem::from(raw, scope, &(decl.nodes.3).nodes.0))
            }
            RefNode::ModuleDeclaration(decl) => self.modules.push(match decl {
                sv::ModuleDeclaration::Nonansi(decl) => {
                    ModuleItem::from(raw, scope, &(decl.nodes.0).nodes.3.nodes.0)
                }
                sv::ModuleDeclaration::Ansi(decl) => {
                    ModuleItem::from(raw, scope, &(decl.nodes.0).nodes.3.nodes.0)
                }
                _ => return,
            }),
            RefNode::TypeDeclaration(decl) => self.types.push(match decl {
                sv::TypeDeclaration::DataType(decl) => {
                    TypeItem::from(raw, scope, &(decl.nodes.2).nodes.0, &decl.nodes.1)
                }
                _ => return,
            }),
            RefNode::NetDeclaration(decl) => match decl {
                sv::NetDeclaration::NetTypeIdentifier(decl) => {
                    for decl_assign in decl.nodes.2.nodes.0.contents() {
                        self.vars.push(VarItem::from(
                            raw,
                            scope,
                            decl.nodes
                                .0
                                .into_iter()
                                .chain(decl.nodes.1.iter().flat_map(|n| n.into_iter())),
                            decl_assign,
                        ));
                    }
                }
                sv::NetDeclaration::NetType(decl) => {
                    for decl_assign in decl.nodes.5.nodes.0.contents() {
                        self.vars.push(VarItem::from(
                            raw,
                            scope,
                            decl.nodes
                                .0
                                .into_iter()
                                .chain(decl.nodes.1.iter().flat_map(|n| n.into_iter()))
                                .chain(decl.nodes.2.iter().flat_map(|n| n.into_iter()))
                                .chain(decl.nodes.3.into_iter())
                                .chain(decl.nodes.4.iter().flat_map(|n| n.into_iter())),
                            decl_assign,
                        ));
                    }
                }
                _ => return,
            },
            RefNode::AnsiPortDeclaration(sv::AnsiPortDeclaration::Net(decl)) => {
                self.ports.push(PortItem::from(
                    raw,
                    scope,
                    &decl.nodes.1.nodes.0,
                    &decl.nodes.0,
                ));
            }
            RefNode::ParameterDeclaration(sv::ParameterDeclaration::Param(decl)) => {
                for assign in decl.nodes.2.nodes.0.contents() {
                    self.params
                        .push(ParamItem::from_param(raw, scope, assign, &decl.nodes.1));
                }
            }
            RefNode::ParameterDeclaration(sv::ParameterDeclaration::Type(decl)) => {
                for assign in decl.nodes.2.nodes.0.contents() {
                    self.params.push(ParamItem::from_type(raw, scope, assign));
                }
            }
            RefNode::LocalParameterDeclaration(sv::LocalParameterDeclaration::Param(decl)) => {
                for assign in decl.nodes.2.nodes.0.contents() {
                    self.params
                        .push(ParamItem::from_param(raw, scope, assign, &decl.nodes.1));
                }
            }
            RefNode::LocalParameterDeclaration(sv::LocalParameterDeclaration::Type(decl)) => {
                for assign in decl.nodes.2.nodes.0.contents() {
                    self.params.push(ParamItem::from_type(raw, scope, assign));
                }
            }
            _ => {
                warn!("Discarding raw doc for {}", node);
                trace!("{:?}", node);
            }
        }
    }

    fn analyze_scopes<'a>(
        &mut self,
        raw: &RawDoc,
        scopes: impl IntoIterator<Item = &'a Scope<'a>>,
    ) {
        for scope in scopes {
            self.analyze_scope(raw, scope);
        }
    }
}

/// Documentation for a package.
#[derive(Debug)]
pub struct PackageItem {
    /// Documentation text.
    pub doc: String,
    /// Package name.
    pub name: String,
    /// The package contents.
    pub content: Context,
}

impl PackageItem {
    fn from(raw: &RawDoc, scope: &Scope, name: &sv::Identifier) -> Self {
        let mut content = Context::default();
        content.analyze_scopes(raw, &scope.children);
        Self {
            doc: parse_docs(raw, &scope.comments),
            name: parse_ident(raw, name),
            content,
        }
    }
}

/// Documentation for a module.
#[derive(Debug)]
pub struct ModuleItem {
    /// Documentation text.
    pub doc: String,
    /// Module name.
    pub name: String,
    /// The module contents.
    pub content: Context,
}

impl ModuleItem {
    fn from(raw: &RawDoc, scope: &Scope, name: &sv::Identifier) -> Self {
        let mut content = Context::default();
        content.analyze_scopes(raw, &scope.children);
        Self {
            doc: parse_docs(raw, &scope.comments),
            name: parse_ident(raw, name),
            content,
        }
    }
}

/// Documentation for a parameter.
#[derive(Debug)]
pub struct ParamItem {
    /// Documentation text.
    pub doc: String,
    /// Parameter name.
    pub name: String,
    /// Parameter type.
    pub ty: String,
}

impl ParamItem {
    fn from_param<'a>(
        raw: &RawDoc,
        scope: &Scope,
        assign: &sv::ParamAssignment,
        ty: &sv::DataTypeOrImplicit,
    ) -> Self {
        Self {
            doc: parse_docs(raw, &scope.comments),
            name: parse_ident(raw, &assign.nodes.0.nodes.0),
            ty: raw.ast.get_str(ty).unwrap_or_default().trim().to_string(),
        }
    }

    fn from_type<'a>(raw: &RawDoc, scope: &Scope, assign: &sv::TypeAssignment) -> Self {
        Self {
            doc: parse_docs(raw, &scope.comments),
            name: parse_ident(raw, &assign.nodes.0.nodes.0),
            ty: String::from("type"),
        }
    }
}

/// Documentation for a port.
#[derive(Debug)]
pub struct PortItem {
    /// Documentation text.
    pub doc: String,
    /// Port name.
    pub name: String,
    /// Port type.
    pub ty: String,
}

impl PortItem {
    fn from<'a>(
        raw: &RawDoc,
        scope: &Scope,
        name: &sv::Identifier,
        ty: &Option<sv::NetPortHeaderOrInterfacePortHeader>,
    ) -> Self {
        Self {
            doc: parse_docs(raw, &scope.comments),
            name: parse_ident(raw, name),
            ty: raw.ast.get_str(ty).unwrap().trim().to_string(),
        }
    }
}

/// Documentation for a type.
#[derive(Debug)]
pub struct TypeItem {
    /// Documentation text.
    pub doc: String,
    /// Type name.
    pub name: String,
    /// Inner type.
    pub ty: String,
}

impl TypeItem {
    fn from(raw: &RawDoc, scope: &Scope, name: &sv::Identifier, ty: &sv::DataType) -> Self {
        Self {
            doc: parse_docs(raw, &scope.comments),
            name: parse_ident(raw, name),
            ty: raw.ast.get_str(ty).unwrap().trim().to_string(),
        }
    }
}

/// Documentation for a variable.
#[derive(Debug)]
pub struct VarItem {
    /// Documentation text.
    pub doc: String,
    /// Variable name.
    pub name: String,
    /// Variable type.
    pub ty: String,
}

impl VarItem {
    fn from<'a>(
        raw: &RawDoc,
        scope: &Scope,
        ty_nodes: impl Iterator<Item = RefNode<'a>>,
        decl_assign: &sv::NetDeclAssignment,
    ) -> Self {
        Self {
            doc: parse_docs(raw, &scope.comments),
            name: parse_ident(raw, &decl_assign.nodes.0.nodes.0),
            ty: raw
                .ast
                .get_str(ty_nodes.collect::<Vec<_>>())
                .unwrap()
                .trim()
                .to_string(),
        }
    }
}

fn parse_docs(_raw: &RawDoc, comments: &[&str]) -> String {
    // Compute the common number of leading spaces in all non-empty lines.
    let common_spaces = comments
        .iter()
        .copied()
        .filter(|s| !s.is_empty())
        .map(|s| s.chars().take_while(|c| c.is_whitespace()).count())
        .min()
        .unwrap_or(0);

    // Gobble up the comments, stripping the leading spaces.
    let mut first = true;
    let mut result = String::new();
    for comment in comments {
        if !first {
            result.push('\n');
        }
        first = false;
        result.extend(comment.chars().skip(common_spaces));
    }
    result
}

fn parse_ident(raw: &RawDoc, ident: &sv::Identifier) -> String {
    raw.ast
        .get_str(match ident {
            sv::Identifier::SimpleIdentifier(si) => &si.nodes.0,
            sv::Identifier::EscapedIdentifier(si) => &si.nodes.0,
        })
        .unwrap()
        .to_string()
}
