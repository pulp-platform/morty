// Copyright 2019 Fabian Schuiki
// Copyright 2019 Florian Zaruba
// Copyright 2022 Michael Rogenmoser

// SPDX-License-Identifier: Apache-2.0
#![recursion_limit = "256"]

#[macro_use]
extern crate log;

use anyhow::{anyhow, Context as _, Error, Result};
use chrono::Local;
use petgraph::algo::dijkstra;
use petgraph::graph::{Graph, NodeIndex};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;
use std::convert::TryFrom;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use sv_parser::Error as SvParserError;
use sv_parser::{
    parse_sv_pp, preprocess, unwrap_node, Define, DefineText, Defines, Locate, RefNode, SyntaxTree,
};

pub mod doc;
mod printer;

pub fn do_pickle<'a>(
    prefix: Option<&'a String>,
    suffix: Option<&'a String>,
    exclude_rename: HashSet<&'a String>,
    exclude: HashSet<&'a String>,
    library_bundle: LibraryBundle,
    mut syntax_trees: Vec<ParsedFile>,
    mut out: Box<dyn Write>,
    top_module: Option<&'a String>,
    keep_defines: bool,
    propagate_defines: bool,
    remove_timeunits: bool,
) -> Result<Pickle<'a>> {
    let mut pickle = Pickle::new(
        // Collect renaming options.
        prefix,
        suffix,
        exclude_rename,
        exclude,
        library_bundle,
    );

    // Gather information for pickling.
    for pf in &syntax_trees {
        // println!("{}", pf.ast);
        for node in &pf.ast {
            trace!("{:#?}", node);
            match node {
                // Module declarations.
                RefNode::ModuleDeclarationAnsi(x) => {
                    // unwrap_node! gets the nearest ModuleIdentifier from x
                    let id = unwrap_node!(x, ModuleIdentifier).unwrap();
                    pickle.register_declaration(&pf.ast, id, pf.path.clone());
                }
                RefNode::ModuleDeclarationNonansi(x) => {
                    let id = unwrap_node!(x, ModuleIdentifier).unwrap();
                    pickle.register_declaration(&pf.ast, id, pf.path.clone());
                }
                // Interface Declaration.
                RefNode::InterfaceDeclaration(x) => {
                    let id = unwrap_node!(x, InterfaceIdentifier).unwrap();
                    pickle.register_declaration(&pf.ast, id, pf.path.clone());
                }
                // Package declarations.
                RefNode::PackageDeclaration(x) => {
                    let id = unwrap_node!(x, PackageIdentifier).unwrap();
                    pickle.register_declaration(&pf.ast, id, pf.path.clone());
                }
                _ => (),
            }
        }
    }

    let mut library_files: Vec<ParsedFile> = vec![];
    for pf in &syntax_trees {
        // global package import
        let global_packages = &pf
            .ast
            .into_iter()
            .filter_map(|node| {
                if let RefNode::DescriptionPackageItem(x) = node {
                    if let Some(package_import) = unwrap_node!(x, PackageImportDeclaration) {
                        let (name, _loc) = get_identifier(
                            &pf.ast,
                            unwrap_node!(package_import, SimpleIdentifier).unwrap(),
                        );
                        eprintln!(
                            "Global package import in {}:\n\t{}",
                            &pf.path,
                            &pf.source[Locate::try_from(x).unwrap().offset
                                ..(Locate::try_from(x).unwrap().offset
                                    + Locate::try_from(x).unwrap().len)]
                        );
                        Some(name)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        for node in &pf.ast {
            match node {
                // Module declarations.
                RefNode::ModuleDeclarationAnsi(x) => {
                    // unwrap_node! gets the nearest ModuleIdentifier from x
                    let parent_id = unwrap_node!(x, ModuleIdentifier).unwrap();
                    let (parent_name, _) = get_identifier(&pf.ast, parent_id);

                    let my_ref_node: RefNode = x.into();
                    pickle.find_and_register_instantiations(
                        &pf.ast,
                        my_ref_node,
                        &parent_name,
                        &mut library_files,
                    );
                    for package in global_packages {
                        pickle.add_dependency_relation(package, &parent_name);
                    }
                }
                RefNode::ModuleDeclarationNonansi(x) => {
                    let parent_id = unwrap_node!(x, ModuleIdentifier).unwrap();
                    let (parent_name, _) = get_identifier(&pf.ast, parent_id);

                    let my_ref_node: RefNode = x.into();
                    pickle.find_and_register_instantiations(
                        &pf.ast,
                        my_ref_node,
                        &parent_name,
                        &mut library_files,
                    );
                    for package in global_packages {
                        pickle.add_dependency_relation(package, &parent_name);
                    }
                }
                // Interface Declaration.
                RefNode::InterfaceDeclaration(x) => {
                    let parent_id = unwrap_node!(x, InterfaceIdentifier).unwrap();
                    let (parent_name, _) = get_identifier(&pf.ast, parent_id);

                    let my_ref_node: RefNode = x.into();
                    pickle.find_and_register_instantiations(
                        &pf.ast,
                        my_ref_node,
                        &parent_name,
                        &mut library_files,
                    );
                    for package in global_packages {
                        pickle.add_dependency_relation(package, &parent_name);
                    }
                }
                // Package declarations.
                RefNode::PackageDeclaration(x) => {
                    let parent_id = unwrap_node!(x, PackageIdentifier).unwrap();
                    let (parent_name, _) = get_identifier(&pf.ast, parent_id);

                    let my_ref_node: RefNode = x.into();
                    pickle.find_and_register_instantiations(
                        &pf.ast,
                        my_ref_node,
                        &parent_name,
                        &mut library_files,
                    );
                    for package in global_packages {
                        pickle.add_dependency_relation(package, &parent_name);
                    }
                }
                _ => (),
            }
        }
    }

    syntax_trees.extend(library_files);
    write!(
        out,
        "// Compiled by morty-{} / {}\n\n",
        env!("CARGO_PKG_VERSION"),
        Local::now()
    )
    .unwrap();

    if let Some(top) = top_module {
        if propagate_defines {
            warn!(
                "Pickle might be non-functional as some files can be excluded due to use of --top={}.\
                \n\tThis might lead to required components being excluded. Use at your own risk!!!",
                top
            );
        }
        pickle.prune_graph(top)?;
    }

    let needed_files = pickle
        .module_file_map
        .clone()
        .into_values()
        .collect::<Vec<_>>();

    // Emit the pickled source files.
    for pf in &syntax_trees {
        if top_module.is_some() && !needed_files.contains(&pf.path) {
            continue;
        }
        // For each file, start with a clean replacement table.
        pickle.replace_table.clear();
        // Iterate again and check for usage
        for node in &pf.ast {
            match node {
                // Instantiations, end-labels.
                RefNode::ModuleIdentifier(x) => {
                    let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                    pickle.register_usage(&pf.ast, id);
                }
                // Interface identifier.
                RefNode::InterfaceIdentifier(x) => {
                    let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                    pickle.register_usage(&pf.ast, id);
                }
                // Package Qualifier (i.e., explicit package constants).
                RefNode::ClassScope(x) => {
                    let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                    pickle.register_usage(&pf.ast, id);
                }
                // Package Import.
                RefNode::PackageIdentifier(x) => {
                    let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                    pickle.register_usage(&pf.ast, id);
                }
                // Check whether we want to exclude the given module from the file sources.
                RefNode::ModuleDeclarationAnsi(x) => {
                    let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                    pickle.register_exclude(&pf.ast, id, Locate::try_from(x).unwrap())
                }
                RefNode::ModuleDeclarationNonansi(x) => {
                    let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                    pickle.register_exclude(&pf.ast, id, Locate::try_from(x).unwrap())
                }
                RefNode::InterfaceDeclaration(x) => {
                    let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                    pickle.register_exclude(&pf.ast, id, Locate::try_from(x).unwrap())
                }
                RefNode::PackageDeclaration(x) => {
                    let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                    pickle.register_exclude(&pf.ast, id, Locate::try_from(x).unwrap())
                }
                RefNode::TimeunitsDeclaration(x) => {
                    let loc = Locate::try_from(x).unwrap();
                    if remove_timeunits {
                        pickle
                            .replace_table
                            .push((loc.offset, loc.len, "".to_string()));
                    }
                }
                _ => (),
            }
        }

        // Find macros to be removed
        let mut new_replace_table = Vec::new();

        if !keep_defines {
            for node in &pf.ast {
                if let RefNode::TextMacroDefinition(x) = node {
                    let loc = Locate::try_from(x).unwrap();
                    new_replace_table.push((loc.offset, loc.len, "".to_string()));
                }
            }
        }

        new_replace_table.append(&mut pickle.replace_table);

        // sort replace table
        new_replace_table.sort_by(|a, b| a.0.cmp(&b.0));

        // Error on overlapping -> correct overlapping!
        for i in 0..new_replace_table.len() - 1 {
            if new_replace_table[i].0 + new_replace_table[i].1 > new_replace_table[i + 1].0 {
                eprintln!(
                    "Offset error, please contact Michael\n{:?}",
                    new_replace_table[i]
                );
            }
        }

        // Replace according to `replace_table`.
        // Apply the replacements.
        debug!("Replace Table: {:?}", new_replace_table);
        let mut pos = 0;
        for (offset, len, repl) in new_replace_table.iter() {
            // Because we are partially stripping modules it can be the case that we don't need to apply some of the upcoming replacements.
            if pos > *offset {
                continue;
            }
            trace!("Replacing: {},{}, {}", offset, len, repl);
            write!(out, "{}", &pf.source[pos..*offset]).unwrap();
            write!(out, "{}", repl).unwrap();
            pos = offset + len;
        }
        write!(out, "{}", &pf.source[pos..]).unwrap();
        // Make sure that each file ends with a newline.
        if !pf.source.ends_with('\n') {
            writeln!(out).unwrap();
        }
    }

    Ok(pickle)
}

pub fn build_syntax_tree(
    file_list: &Vec<FileBundle>,
    strip_comments: bool,
    ignore_unparseable: bool,
    propagate_defines: bool,
    force_sequential: bool,
) -> Result<Vec<ParsedFile>> {
    // Parse the input files.
    let mut syntax_trees = vec![];

    let mut internal_defines: Defines = HashMap::new();

    for bundle in file_list {
        let bundle_include_dirs: Vec<_> = bundle.include_dirs.iter().map(Path::new).collect();

        if propagate_defines {
            internal_defines.extend(defines_to_sv_parser(&bundle.defines));
        } else {
            internal_defines = defines_to_sv_parser(&bundle.defines);
        }

        // For each file in the file bundle preprocess and parse it.
        // Use a neat trick of `collect` here, which allows you to collect a
        // `Result<T>` iterator into a `Result<Vec<T>>`, i.e. bubbling up the
        // error.
        let v = if force_sequential | propagate_defines {
            let tmp = bundle.files.iter().map(|filename| -> Result<_> {
                let pf = parse_file(
                    filename,
                    &bundle_include_dirs,
                    &internal_defines,
                    strip_comments,
                )?;
                if propagate_defines {
                    internal_defines.extend(pf.defines.clone());
                }
                Ok(pf)
            });
            if ignore_unparseable {
                tmp.filter_map(|r| r.map_err(|e| warn!("Continuing with {:?}", e)).ok())
                    .collect()
            } else {
                tmp.collect::<Result<Vec<ParsedFile>>>()?
            }
        } else {
            let tmp = bundle.files.par_iter().map(|filename| -> Result<_> {
                parse_file(
                    filename,
                    &bundle_include_dirs,
                    &internal_defines,
                    strip_comments,
                )
            });
            if ignore_unparseable {
                tmp.filter_map(|r| r.map_err(|e| warn!("Continuing with {:?}", e)).ok())
                    .collect()
            } else {
                tmp.collect::<Result<Vec<ParsedFile>>>()?
            }
        };
        syntax_trees.extend(v);
    }

    Ok(syntax_trees)
}

pub fn just_preprocess(syntax_trees: Vec<ParsedFile>, mut out: Box<dyn Write>) -> Result<()> {
    write!(
        out,
        "// Compiled by morty-{} / {}\n\n",
        env!("CARGO_PKG_VERSION"),
        Local::now()
    )
    .unwrap();
    for pf in syntax_trees {
        eprintln!("{}:", pf.path);
        writeln!(out, "{:}", pf.source).unwrap();
    }
    Ok(())
}

pub fn build_doc(syntax_trees: Vec<ParsedFile>, dir: &str) -> Result<()> {
    let doc = doc::Doc::new(&syntax_trees);
    let mut html = doc::Renderer::new(Path::new(dir));
    html.render(&doc)?;
    Ok(())
}

pub fn write_manifest(
    manifest_file: &str,
    pickle: Pickle,
    file_list: Vec<FileBundle>,
    include_dirs: Vec<String>,
    defines: HashMap<String, Option<String>>,
    top_module: Option<&String>,
) -> Result<()> {
    let mut undef_modules = Vec::new();

    // find undefined modules
    for name in &pickle.inst_table {
        if !pickle.rename_table.contains_key(name) {
            undef_modules.push(name.to_string());
        }
    }

    let mut top_modules = Vec::new();

    // find top modules
    match top_module {
        Some(x) => {
            top_modules.push(pickle.rename_table[x].to_string());
        }
        None => {
            for new_name in pickle.rename_table.values() {
                if !pickle.inst_table.contains(new_name) {
                    top_modules.push(new_name.to_string());
                }
            }
        }
    }

    let mut base_files = Vec::new();
    let mut bundles = Vec::new();
    let pickled_files = pickle
        .module_file_map
        .clone()
        .into_values()
        .collect::<Vec<_>>();
    for mut bundle in file_list {
        if bundle.include_dirs == include_dirs && bundle.defines == defines {
            base_files.extend(bundle.files.clone());
            if top_module.is_some() {
                base_files.retain(|v| pickled_files.clone().contains(v));
            }
        } else {
            if top_module.is_some() {
                bundle.files.retain(|v| pickled_files.contains(v));
            }
            if !bundle.files.is_empty() {
                bundles.push(bundle);
            }
        }
    }
    base_files.extend(pickle.used_libs);
    if !base_files.is_empty() {
        bundles.push(FileBundle {
            include_dirs,
            export_incdirs: HashMap::new(),
            defines,
            files: base_files,
        });
    }

    let json = serde_json::to_string_pretty(&Manifest {
        sources: bundles,
        tops: top_modules,
        undefined: undef_modules,
    })
    .unwrap();

    let path = Path::new(manifest_file);
    let mut out = Box::new(BufWriter::new(File::create(path).unwrap())) as Box<dyn Write>;
    writeln!(out, "{}", json).unwrap();

    Ok(())
}

/// Write module graph to file
pub fn write_dot_graph(pickle: &Pickle, graph_file: &str) -> Result<()> {
    let path = Path::new(graph_file);
    let mut out = Box::new(BufWriter::new(File::create(path).unwrap())) as Box<dyn Write>;
    writeln!(
        out,
        "{:?}",
        petgraph::dot::Dot::with_config(
            &pickle.module_graph,
            &[petgraph::dot::Config::EdgeNoLabel]
        )
    )
    .unwrap();
    Ok(())
}

/// Struct containing information about
/// what should be pickled and how.
#[derive(Debug)]
pub struct Pickle<'a> {
    /// Optional name prefix.
    pub prefix: Option<&'a String>,
    /// Optional name suffix.
    pub suffix: Option<&'a String>,
    /// Declarations which are excluded from re-naming.
    pub exclude_rename: HashSet<&'a String>,
    /// Declarations which are excluded from the pickled sources.
    pub exclude: HashSet<&'a String>,
    /// Table containing thing that should be re-named.
    pub rename_table: HashMap<String, String>,
    /// Locations of text which should be replaced.
    pub replace_table: Vec<(usize, usize, String)>,
    /// A set of instantiated modules.
    pub inst_table: HashSet<String>,
    /// Information for library files
    pub libs: LibraryBundle,
    /// List of library files used during parsing.
    pub used_libs: Vec<String>,
    /// Module hierarchy graph
    pub module_graph: Graph<String, ()>,
    /// Map for module names to graph nodes
    pub module_graph_nodes: HashMap<String, NodeIndex>,
    /// Map module name to declaration file
    pub module_file_map: HashMap<String, String>,
}

impl<'a> Pickle<'a> {
    pub fn new(
        prefix: Option<&'a String>,
        suffix: Option<&'a String>,
        exclude_rename: HashSet<&'a String>,
        exclude: HashSet<&'a String>,
        libs: LibraryBundle,
    ) -> Self {
        Self {
            prefix,
            suffix,
            exclude_rename,
            exclude,
            // Create a rename table.
            rename_table: HashMap::new(),
            replace_table: vec![],
            inst_table: HashSet::new(),
            libs,
            used_libs: vec![],
            // Create graph.
            module_graph: Graph::new(),
            module_graph_nodes: HashMap::new(),
            module_file_map: HashMap::new(),
        }
    }

    /// Register a declaration such as a package or module.
    pub fn register_declaration(&mut self, syntax_tree: &SyntaxTree, id: RefNode, file: String) {
        let (module_name, loc) = get_identifier(syntax_tree, id);
        info!("module_name: {:?}", module_name);
        self.module_graph_nodes.insert(
            module_name.clone(),
            self.module_graph.add_node(module_name.clone()),
        );
        self.module_file_map.insert(module_name.clone(), file);
        if self.exclude_rename.contains(&module_name) || self.exclude.contains(&module_name) {
            return;
        }
        let mut new_name = module_name.clone();
        if let Some(prefix) = self.prefix {
            new_name = format!("{}{}", prefix, new_name);
        }
        if let Some(suffix) = self.suffix {
            new_name = format!("{}{}", new_name, suffix);
        }
        debug!("Declaration `{}`: {:?}", module_name, loc);
        self.rename_table.insert(module_name, new_name);
    }

    pub fn register_instantiation(&mut self, syntax_tree: &SyntaxTree, id: RefNode) {
        let (inst_name, _) = get_identifier(syntax_tree, id.clone());
        self.inst_table.insert(inst_name.clone());

        if let Some((parent_name, _)) = get_calling_module(syntax_tree, id) {
            self.add_dependency_relation(&inst_name, &parent_name);
        }
    }

    pub fn register_instantiation_with_parent(
        &mut self,
        syntax_tree: &SyntaxTree,
        id: RefNode,
        parent_name: &str,
    ) {
        let (inst_name, _) = get_identifier(syntax_tree, id.clone());
        self.inst_table.insert(inst_name.clone());

        self.add_dependency_relation(&inst_name, parent_name);
    }

    pub fn add_dependency_relation(&mut self, inst_name: &str, parent_name: &str) {
        if !self.module_graph_nodes.contains_key(inst_name) {
            self.module_graph_nodes.insert(
                inst_name.to_string(),
                self.module_graph.add_node(inst_name.to_string()),
            );
        }
        self.module_graph.update_edge(
            self.module_graph_nodes[parent_name],
            self.module_graph_nodes[inst_name],
            (),
        );
    }

    pub fn find_and_register_instantiations(
        &mut self,
        syntax_tree: &SyntaxTree,
        id: RefNode,
        parent_name: &str,
        library_files: &mut Vec<ParsedFile>,
    ) {
        for node in id {
            match node {
                RefNode::ModuleInstantiation(x) => {
                    let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                    self.register_instantiation_with_parent(syntax_tree, id.clone(), parent_name);

                    let (inst_name, _) = get_identifier(syntax_tree, id.clone());
                    if !self.rename_table.contains_key(&inst_name) {
                        info!("Could not find {}, checking libraries...", &inst_name);
                        self.load_library_module(&inst_name, library_files);
                    }
                }
                RefNode::PackageImportItem(x) => {
                    let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                    self.register_instantiation_with_parent(syntax_tree, id.clone(), parent_name);

                    let (inst_name, _) = get_identifier(syntax_tree, id);
                    if !self.rename_table.contains_key(&inst_name) {
                        info!("Could not find {}, checking libraries...", &inst_name);
                        self.load_library_module(&inst_name, library_files);
                    }
                }
                RefNode::PackageScope(x) => {
                    let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                    self.register_instantiation_with_parent(syntax_tree, id.clone(), parent_name);

                    let (inst_name, _) = get_identifier(syntax_tree, id);
                    if !self.rename_table.contains_key(&inst_name) {
                        info!("Could not find {}, checking libraries...", &inst_name);
                        self.load_library_module(&inst_name, library_files);
                    }
                }
                RefNode::InterfacePortHeader(x) => {
                    let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                    self.register_instantiation_with_parent(syntax_tree, id.clone(), parent_name);

                    let (inst_name, _) = get_identifier(syntax_tree, id);
                    if !self.rename_table.contains_key(&inst_name) {
                        info!("Could not find {}, checking libraries...", &inst_name);
                        self.load_library_module(&inst_name, library_files);
                    }
                }
                RefNode::ClassScope(x) => {
                    let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                    self.register_instantiation_with_parent(syntax_tree, id.clone(), parent_name);

                    let (inst_name, _) = get_identifier(syntax_tree, id);
                    if !self.rename_table.contains_key(&inst_name) {
                        info!("Could not find {}, checking libraries...", &inst_name);
                        self.load_library_module(&inst_name, library_files);
                    }
                }
                _ => (),
            }
        }
    }

    /// Register a usage of the identifier.
    pub fn register_usage(&mut self, syntax_tree: &SyntaxTree, id: RefNode) {
        let (inst_name, loc) = get_identifier(syntax_tree, id);
        let new_name = match self.rename_table.get(&inst_name) {
            Some(x) => x,
            None => return,
        };
        debug!("Usage `{}`: {:?}", inst_name, loc);
        self.replace_table
            .push((loc.offset, loc.len, new_name.clone()));
    }

    // Check whether a given declaration should be striped from the sources.
    pub fn register_exclude(&mut self, syntax_tree: &SyntaxTree, id: RefNode, locate: Locate) {
        let (inst_name, loc) = get_identifier(syntax_tree, id);
        if self.exclude.contains(&inst_name) {
            debug!("Exclude `{}`: {:?}", inst_name, loc);
            self.replace_table
                .push((locate.offset, locate.len, "".to_string()));
        }
    }

    // Load the module with name 'module_name' and append the resulting ParsedFile to 'files'.
    // This function may recursively load other modules if the library uses another library module.
    // If no module is found in the library bundle, this function does nothing.
    pub fn load_library_module(&mut self, module_name: &str, files: &mut Vec<ParsedFile>) {
        let rm = self.libs.load_module(module_name, &mut self.used_libs);
        match rm {
            Ok(pf) => {
                // register all declarations from this library file.
                for node in &pf.ast {
                    match node {
                        RefNode::ModuleDeclarationAnsi(x) => {
                            let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                            self.register_declaration(&pf.ast, id, pf.path.clone());
                        }
                        RefNode::ModuleDeclarationNonansi(x) => {
                            let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                            self.register_declaration(&pf.ast, id, pf.path.clone());
                        }
                        _ => (),
                    }
                }
                // look for all module instantiations
                for node in &pf.ast {
                    if let RefNode::ModuleInstantiation(x) = node {
                        let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                        self.register_instantiation(&pf.ast, id.clone());

                        // if this module is undefined, recursively attempt to load a library
                        // module for it.
                        let (inst_name, _) = get_identifier(&pf.ast, id);
                        info!(
                            "Instantiation `{}` in library module `{}`",
                            &inst_name, &module_name
                        );
                        if !self.rename_table.contains_key(&inst_name) {
                            info!("load library module {}", &inst_name);
                            self.load_library_module(&inst_name, files);
                        }
                    }
                }
                // add the parsed file to the vector.
                files.push(pf);
            }
            Err(e) => info!("error loading library: {}", e),
        }
    }

    pub fn prune_graph(&mut self, top_module: &str) -> Result<()> {
        if !self.module_graph_nodes.contains_key(top_module) {
            return Err(anyhow!("Module {} not found!", top_module));
        }
        let test_weights = dijkstra(
            &self.module_graph,
            self.module_graph_nodes[top_module],
            None,
            |_| 1,
        );

        self.module_graph
            .retain_nodes(|_, n| test_weights.contains_key(&n));
        self.module_graph_nodes
            .retain(|_, v| test_weights.contains_key(v));

        let test_keys = self.module_graph_nodes.clone();
        self.module_file_map
            .retain(|k, _| test_keys.contains_key(k));

        self.inst_table.retain(|k| test_keys.contains_key(k));

        self.rename_table.retain(|k, _| test_keys.contains_key(k));

        Ok(())
    }
}

// Returns true if this file has a library extension (.v or .sv).
pub fn has_libext(p: &Path) -> bool {
    matches!(
        p.extension().and_then(OsStr::to_str),
        Some("sv") | Some("v")
    )
}

// Given a library filename, return the module name that this file must contain. Library files
// must be named as module_name.v or module_name.sv.
pub fn lib_module(p: &Path) -> Option<String> {
    p.with_extension("").file_name()?.to_str().map(String::from)
}

// Convert the preprocessor defines into the appropriate format which is understood by `sv-parser`
pub fn defines_to_sv_parser(defines: &HashMap<String, Option<String>>) -> Defines {
    return defines
        .iter()
        .map(|(name, value)| {
            // If there is a define text add it.
            let define_text = value
                .as_ref()
                .map(|x| DefineText::new(String::from(x), None));
            (
                name.clone(),
                Some(Define::new(name.clone(), vec![], define_text)),
            )
        })
        .collect();
}

pub fn parse_file(
    filename: &str,
    bundle_include_dirs: &[&Path],
    bundle_defines: &HashMap<String, Option<Define>>,
    strip_comments: bool,
) -> Result<ParsedFile> {
    info!("{:?}", filename);

    // Preprocess the verilog files.
    let pp = preprocess(
        filename,
        bundle_defines,
        bundle_include_dirs,
        strip_comments,
        false,
    )
    .with_context(|| format!("Failed to preprocess `{}`", filename))?;

    let buffer = pp.0.text().to_string();
    let syntax_tree = parse_sv_pp(pp.0, pp.1, false).or_else(|err| -> Result<_> {
        let printer = Arc::new(Mutex::new(printer::Printer::new()));
        let printer = &mut *printer.lock().unwrap();
        print_parse_error(printer, &err, false)?;
        Err(Error::new(err))
    })?;

    Ok(ParsedFile {
        path: String::from(filename),
        source: buffer,
        ast: syntax_tree.0,
        defines: syntax_tree.1,
    })
}

pub fn get_identifier(st: &SyntaxTree, node: RefNode) -> (String, Locate) {
    // unwrap_node! can take multiple types
    match unwrap_node!(node, SimpleIdentifier, EscapedIdentifier) {
        Some(RefNode::SimpleIdentifier(x)) => {
            // Original string can be got by SyntaxTree::get_str(self, locate: &Locate)
            (String::from(st.get_str(&x.nodes.0).unwrap()), x.nodes.0)
        }
        Some(RefNode::EscapedIdentifier(x)) => {
            (String::from(st.get_str(&x.nodes.0).unwrap()), x.nodes.0)
        }
        _ => panic!("No identifier found."),
    }
}

pub fn get_calling_module(st: &SyntaxTree, node: RefNode) -> Option<(String, Locate)> {
    let unwrapped_node = unwrap_node!(node.clone(), SimpleIdentifier, EscapedIdentifier).unwrap();
    // let (_, loc0) = get_identifier(st, unwrapped_node.clone());

    // TODO MICHAERO: THIS IS SUUUPER SLOW!!! Especially for packages that have many sub packages...

    // unwrap_node! can take multiple types
    for st_node in st {
        match st_node {
            // Module declarations.
            RefNode::ModuleDeclarationAnsi(x) => {
                // unwrap_node! gets the nearest ModuleIdentifier from x
                let id = unwrap_node!(x, ModuleIdentifier).unwrap();
                // pickle.register_declaration(&pf.ast, id);
                let (module_name, module_loc) = get_identifier(st, id);
                // println!("module_name: {:?}", module_name);
                // println!("{:?}", x.nodes);
                let my_ref_node: RefNode = x.into();
                if my_ref_node
                    .into_iter()
                    .filter_map(|sub_node| {
                        unwrap_node!(sub_node, SimpleIdentifier, EscapedIdentifier)
                    })
                    .any(|id| id == unwrapped_node)
                {
                    return Some((module_name, module_loc));
                }
            }
            RefNode::ModuleDeclarationNonansi(x) => {
                // unwrap_node! gets the nearest ModuleIdentifier from x
                let id = unwrap_node!(x, ModuleIdentifier).unwrap();
                // pickle.register_declaration(&pf.ast, id);
                let (module_name, module_loc) = get_identifier(st, id);
                // println!("module_name: {:?}", module_name);
                // println!("{:?}", x.nodes);
                let my_ref_node: RefNode = x.into();
                if my_ref_node
                    .into_iter()
                    .filter_map(|sub_node| {
                        unwrap_node!(sub_node, SimpleIdentifier, EscapedIdentifier)
                    })
                    .any(|id| id == unwrapped_node)
                {
                    return Some((module_name, module_loc));
                }
            }
            // Interface Declaration.
            RefNode::InterfaceDeclaration(x) => {
                // unwrap_node! gets the nearest InterfaceIdentifier from x
                let id = unwrap_node!(x, InterfaceIdentifier).unwrap();
                let (module_name, module_loc) = get_identifier(st, id);
                // println!("module_name: {:?}", module_name);
                // println!("{:?}", x.nodes);
                let my_ref_node: RefNode = x.into();
                if my_ref_node
                    .into_iter()
                    .filter_map(|sub_node| {
                        unwrap_node!(sub_node, SimpleIdentifier, EscapedIdentifier)
                    })
                    .any(|id| id == unwrapped_node)
                {
                    return Some((module_name, module_loc));
                }
            }
            // Package declarations.
            RefNode::PackageDeclaration(x) => {
                // unwrap_node! gets the nearest PackageIdentifier from x
                let id = unwrap_node!(x, PackageIdentifier).unwrap();
                let (module_name, module_loc) = get_identifier(st, id);
                // println!("module_name: {:?}", module_name);
                // println!("{:?}", x.nodes);
                let my_ref_node: RefNode = x.into();
                if my_ref_node
                    .into_iter()
                    .filter_map(|sub_node| {
                        unwrap_node!(sub_node, SimpleIdentifier, EscapedIdentifier)
                    })
                    .any(|id| id == unwrapped_node)
                {
                    return Some((module_name, module_loc));
                }
            }

            _ => {}
        }
    }
    // println!("{}", st);
    // println!("{:?}", loc0);
    eprintln!("Possible global package import, not properly parsed! TODO MICHAERO better error reporting to fix issue, link all modules/packages/interfaces in file to the dependency.");
    // panic!("No calling module found.");
    None
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Manifest {
    // list of file bundles
    pub sources: Vec<FileBundle>,
    // list of top modules
    pub tops: Vec<String>,
    // list of undefined modules
    pub undefined: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FileBundle {
    pub include_dirs: Vec<String>,

    #[serde(default)]
    pub export_incdirs: HashMap<String, Vec<String>>,
    pub defines: HashMap<String, Option<String>>,
    pub files: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LibraryBundle {
    pub include_dirs: Vec<String>,
    pub defines: HashMap<String, Option<String>>,
    pub files: HashMap<String, PathBuf>,
}

impl LibraryBundle {
    pub fn load_module(
        &self,
        module_name: &str,
        files: &mut Vec<String>,
    ) -> Result<ParsedFile, Error> {
        // check if the module is in the hashmap
        let f = match self.files.get(module_name) {
            Some(p) => p.to_string_lossy(),
            None => {
                return Err(anyhow!("module {} not found in libraries", module_name));
            }
        };

        let bundle_include_dirs: Vec<_> = self.include_dirs.iter().map(Path::new).collect();
        let bundle_defines = defines_to_sv_parser(&self.defines);

        files.push(f.to_string());

        // if so, parse the file and return the result (comments are always stripped).
        parse_file(&f, &bundle_include_dirs, &bundle_defines, true)
    }
}

/// A parsed input file.
pub struct ParsedFile {
    /// The path to the file.
    pub path: String,
    /// The contents of the file.
    pub source: String,
    /// The parsed AST of the file.
    pub ast: SyntaxTree,
    /// Internal defines
    pub defines: Defines,
}

#[cfg_attr(tarpaulin, skip)]
pub fn print_parse_error(
    printer: &mut printer::Printer,
    error: &SvParserError,
    single: bool,
) -> Result<()> {
    match error {
        SvParserError::Parse(Some((path, pos))) => {
            printer.print_parse_error(path, *pos, single)?;
        }
        SvParserError::Include { source: x } => {
            if let SvParserError::File { path: x, .. } = x.as_ref() {
                printer.print_error(&format!("failed to include '{}'", x.display()))?;
            }
        }
        SvParserError::DefineArgNotFound(x) => {
            printer.print_error(&format!("define argument '{}' is not found", x))?;
        }
        SvParserError::DefineNotFound(x) => {
            printer.print_error(&format!("define '{}' is not found", x))?;
        }
        x => {
            printer.print_error(&format!("{}", x))?;
        }
    }

    Ok(())
}
