// Copyright 2022 Michael Rogenmoser

// SPDX-License-Identifier: Apache-2.0

use crate::{
    defines_to_sv_parser, doc, get_identifier, print_parse_error, printer, FileBundle,
    LibraryBundle, Manifest, ParsedFile,
};
use anyhow::{anyhow, Context, Error, Result};
use chrono::Local;
use petgraph::algo::dijkstra;
use petgraph::graph::{Graph, NodeIndex};
use petgraph::{Incoming, Outgoing};
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};
use sv_parser::{parse_sv_pp, preprocess, unwrap_locate, unwrap_node, Defines, Locate, RefNode};

/// Struct used for transformations
#[derive(Debug)]
pub struct Pickle {
    /// All files
    pub all_files: Vec<ParsedFile>,
    /// libraries
    pub libs: Option<LibraryBundle>,
    /// Module hierarchy graph
    pub module_graph: Graph<String, ()>,
    /// Map for module names to graph nodes
    pub module_graph_nodes: HashMap<String, NodeIndex>,
    /// Map of all declarations
    pub declarations: HashMap<String, (SVConstructType, /* file id */ usize, Locate)>,
    /// Map of all usages
    pub usages: HashMap<String, Vec<(/* file id */ usize, Locate)>>,
    /// list of replacements
    pub replace_table: Vec<(/* file id */ usize, Locate, String)>,
}

impl Default for Pickle {
    fn default() -> Self {
        Self::new()
    }
}

impl Pickle {
    pub fn new() -> Self {
        Self {
            all_files: Vec::new(),
            libs: None,
            module_graph: Graph::new(),
            module_graph_nodes: HashMap::new(),
            declarations: HashMap::new(),
            usages: HashMap::new(),
            replace_table: Vec::new(),
        }
    }

    /// Parse a single file and add it to all_files
    pub fn parse_file(
        &mut self,
        filename: &str,
        bundle_include_dirs: &[&Path],
        bundle_defines: &Defines,
        strip_comments: bool,
    ) -> Result<Defines> {
        info!("Adding {:?}", filename);

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
            let printer = Arc::new(Mutex::new(printer::printer::Printer::new()));
            let printer = &mut *printer.lock().unwrap();
            print_parse_error(printer, &err, false)?;
            Err(Error::new(err))
        })?;

        self.all_files.push(ParsedFile {
            path: String::from(filename),
            source: buffer,
            ast: syntax_tree.0,
            defines: syntax_tree.1.clone(),
        });

        Ok(syntax_tree.1)
    }

    /// Parse and add multiple files
    pub fn add_files(
        &mut self,
        file_list: &Vec<FileBundle>,
        strip_comments: bool,
        ignore_unparseable: bool,
        propagate_defines: bool,
    ) -> Result<()> {
        let mut internal_defines: Defines = HashMap::new();

        for bundle in file_list {
            let bundle_include_dirs: Vec<_> = bundle.include_dirs.iter().map(Path::new).collect();

            if propagate_defines {
                internal_defines.extend(defines_to_sv_parser(&bundle.defines));
            } else {
                internal_defines = defines_to_sv_parser(&bundle.defines);
            }

            let v = bundle.files.iter().map(|filename| -> Result<_> {
                let pf_defines = self.parse_file(
                    filename,
                    &bundle_include_dirs,
                    &internal_defines,
                    strip_comments,
                )?;
                if propagate_defines {
                    internal_defines.extend(pf_defines);
                }
                Ok(())
            });

            if ignore_unparseable {
                v.filter_map(|r| r.map_err(|e| warn!("Continuing with {:?}", e)).ok())
                    .for_each(drop);
            } else {
                v.collect::<Result<Vec<_>>>()?;
            };
        }

        Ok(())
    }

    /// Add Library Bundle
    pub fn add_libs(&mut self, libs: LibraryBundle) -> Result<()> {
        self.libs = Some(libs);

        Ok(())
    }

    /// Helper function to register all declarations
    fn register_declarations(&mut self) -> Result<()> {
        for i in 0..self.all_files.len() {
            let pf = &self.all_files[i];
            for node in &pf.ast {
                match node {
                    // Module declarations.
                    RefNode::ModuleDeclaration(x) => {
                        // unwrap_node! gets the nearest ModuleIdentifier from x
                        let id = unwrap_node!(x, ModuleIdentifier).unwrap();
                        let (module_name, _loc) = get_identifier(&pf.ast, id);
                        info!("module_name: {:?}", module_name);

                        if self.declarations.contains_key(&module_name) {
                            return Err(anyhow!("Module {} declared mutliple times!", module_name));
                        }
                        self.module_graph_nodes.insert(
                            module_name.clone(),
                            self.module_graph.add_node(module_name.clone()),
                        );
                        self.declarations.insert(
                            module_name.clone(),
                            (SVConstructType::Module, i, Locate::try_from(x).unwrap()),
                        );
                        self.usages.insert(module_name, Vec::new());
                    }
                    // Interface Declaration.
                    RefNode::InterfaceDeclaration(x) => {
                        let id = unwrap_node!(x, InterfaceIdentifier).unwrap();
                        let (module_name, _loc) = get_identifier(&pf.ast, id);
                        info!("module_name: {:?}", module_name);

                        if self.declarations.contains_key(&module_name) {
                            return Err(anyhow!(
                                "Interface {} declared mutliple times!",
                                module_name
                            ));
                        }
                        self.module_graph_nodes.insert(
                            module_name.clone(),
                            self.module_graph.add_node(module_name.clone()),
                        );
                        self.declarations.insert(
                            module_name.clone(),
                            (SVConstructType::Interface, i, Locate::try_from(x).unwrap()),
                        );
                        self.usages.insert(module_name, Vec::new());
                    }
                    // Package declarations.
                    RefNode::PackageDeclaration(x) => {
                        let id = unwrap_node!(x, PackageIdentifier).unwrap();
                        let (module_name, _loc) = get_identifier(&pf.ast, id);
                        info!("module_name: {:?}", module_name);

                        if self.declarations.contains_key(&module_name) {
                            return Err(anyhow!(
                                "Package {} declared mutliple times!",
                                module_name
                            ));
                        }
                        self.module_graph_nodes.insert(
                            module_name.clone(),
                            self.module_graph.add_node(module_name.clone()),
                        );
                        self.declarations.insert(
                            module_name.clone(),
                            (SVConstructType::Package, i, Locate::try_from(x).unwrap()),
                        );
                        self.usages.insert(module_name, Vec::new());
                    }
                    _ => (),
                }
            }
        }
        Ok(())
    }

    /// Load a module from the library
    fn _load_library_module(&self, module_name: &str) -> Result<()> {
        let mut used_libs = vec![];
        if let Some(libs) = &self.libs {
            let rm = libs.load_module(module_name, &mut used_libs);
            match rm {
                Ok(_pf) => {
                    unimplemented!();
                    // TODO: register all declarations and instantiations
                    // TODO: add file to all_files
                    // TODO: check how used_libs is supposed to be used
                }
                Err(e) => info!("error loading library: {}", e),
            }
        }

        Ok(())
    }

    /// Helper function to find and register instantiations (when finding usages)
    fn find_and_register_instantiations(
        &self,
        file_id: usize,
        parent_node: RefNode,
    ) -> Vec<(String, SVConstructType, Locate)> {
        let mut mapping = Vec::new();

        for node in parent_node {
            match node {
                RefNode::ModuleInstantiation(x) => {
                    let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                    let (inst_name, _) = get_identifier(&self.all_files[file_id].ast, id.clone());
                    mapping.push((
                        inst_name.clone(),
                        SVConstructType::Module,
                        Locate::try_from(x).unwrap(),
                    ));
                }
                RefNode::PackageImportItem(x) => {
                    let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                    let (inst_name, _) = get_identifier(&self.all_files[file_id].ast, id.clone());
                    mapping.push((
                        inst_name,
                        SVConstructType::Package,
                        Locate::try_from(x).unwrap(),
                    ));
                }
                RefNode::PackageScope(x) => {
                    let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                    let (inst_name, _) = get_identifier(&self.all_files[file_id].ast, id.clone());
                    mapping.push((
                        inst_name,
                        SVConstructType::Package,
                        Locate::try_from(x).unwrap(),
                    ));
                }
                RefNode::InterfacePortHeader(x) => {
                    let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                    let (inst_name, _) = get_identifier(&self.all_files[file_id].ast, id.clone());
                    mapping.push((
                        inst_name,
                        SVConstructType::Interface,
                        Locate::try_from(x).unwrap(),
                    ));
                }
                RefNode::InterfaceInstantiation(x) => {
                    let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                    let (inst_name, _) = get_identifier(&self.all_files[file_id].ast, id.clone());
                    mapping.push((
                        inst_name,
                        SVConstructType::Interface,
                        Locate::try_from(x).unwrap(),
                    ));
                }
                RefNode::ClassScope(x) => {
                    let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                    let (inst_name, _) = get_identifier(&self.all_files[file_id].ast, id.clone());
                    mapping.push((
                        inst_name,
                        SVConstructType::Package,
                        Locate::try_from(x).unwrap(),
                    ));
                }
                _ => {}
            }
        }
        mapping
    }

    /// Helper function to register all module usages in the source files
    fn register_usages(&mut self) -> Result<()> {
        for i in 0..self.all_files.len() {
            let pf = &self.all_files[i];

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
                            warn!(
                                "Global package import in {}:\n\t{}",
                                &pf.path,
                                &pf.source[Locate::try_from(x).unwrap().offset
                                    ..(Locate::try_from(x).unwrap().offset
                                        + Locate::try_from(x).unwrap().len)]
                            );
                            Some((name, Locate::try_from(x).unwrap()))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            // register usages of global packages
            for package in global_packages {
                if !self.module_graph_nodes.contains_key(&package.0) {
                    self.module_graph_nodes.insert(
                        package.0.clone(),
                        self.module_graph.add_node(package.0.clone()),
                    );
                    self.usages.insert(package.0.clone(), Vec::new());
                }
                if let Some(decl) = self.declarations.get(&package.0) {
                    if decl.0 != SVConstructType::Package {
                        warn!(
                            "Possibly type mismatch for {}: declaring {:?}, instantiating {:?}",
                            &package.0,
                            decl.0,
                            SVConstructType::Package
                        );
                    }
                }
                self.usages
                    .get_mut(&package.0)
                    .unwrap()
                    .push((i, package.1));
            }

            for node in &pf.ast {
                match node {
                    // Module declarations.
                    RefNode::ModuleDeclaration(x) => {
                        // unwrap_node! gets the nearest ModuleIdentifier from x
                        let parent_id = unwrap_node!(x, ModuleIdentifier).unwrap();
                        let (parent_name, _) = get_identifier(&pf.ast, parent_id).clone();
                        let instantiations = self.find_and_register_instantiations(i, x.into());

                        for inst in instantiations {
                            // Add nodes in case they are not present
                            if !self.module_graph_nodes.contains_key(&inst.0) {
                                self.module_graph_nodes.insert(
                                    inst.0.clone(),
                                    self.module_graph.add_node(inst.0.clone()),
                                );
                                self.usages.insert(inst.0.clone(), Vec::new());
                            }
                            if let Some(decl) = self.declarations.get(&inst.0) {
                                if decl.0 != inst.1 {
                                    warn!("Possibly type mismatch for {}: declaring {:?}, instantiating {:?}", &inst.0, decl.0, inst.1);
                                }
                            }
                            self.usages.get_mut(&inst.0).unwrap().push((i, inst.2));

                            self.module_graph.update_edge(
                                self.module_graph_nodes[&parent_name],
                                self.module_graph_nodes[&inst.0],
                                (),
                            );
                        }
                        for package in global_packages {
                            self.module_graph.update_edge(
                                self.module_graph_nodes[&parent_name],
                                self.module_graph_nodes[&package.0],
                                (),
                            );
                        }
                    }
                    // Interface Declaration.
                    RefNode::InterfaceDeclaration(x) => {
                        // unwrap_node! gets the nearest ModuleIdentifier from x
                        let parent_id = unwrap_node!(x, InterfaceIdentifier).unwrap();
                        let (parent_name, _) = get_identifier(&pf.ast, parent_id);
                        let instantiations = self.find_and_register_instantiations(i, x.into());

                        for inst in instantiations {
                            // Add nodes in case they are not present
                            if !self.module_graph_nodes.contains_key(&inst.0) {
                                self.module_graph_nodes.insert(
                                    inst.0.clone(),
                                    self.module_graph.add_node(inst.0.clone()),
                                );
                                self.usages.insert(inst.0.clone(), Vec::new());
                            }
                            if let Some(decl) = self.declarations.get(&inst.0) {
                                if decl.0 != inst.1 {
                                    warn!("Possibly type mismatch for {}: declaring {:?}, instantiating {:?}", &inst.0, decl.0, inst.1);
                                }
                            }
                            self.usages.get_mut(&inst.0).unwrap().push((i, inst.2));

                            self.module_graph.update_edge(
                                self.module_graph_nodes[&parent_name],
                                self.module_graph_nodes[&inst.0],
                                (),
                            );
                        }
                        for package in global_packages {
                            self.module_graph.update_edge(
                                self.module_graph_nodes[&parent_name],
                                self.module_graph_nodes[&package.0],
                                (),
                            );
                        }
                    }
                    // Package declarations.
                    RefNode::PackageDeclaration(x) => {
                        // unwrap_node! gets the nearest ModuleIdentifier from x
                        let parent_id = unwrap_node!(x, PackageIdentifier).unwrap();
                        let (parent_name, _) = get_identifier(&pf.ast, parent_id);
                        let instantiations = self.find_and_register_instantiations(i, x.into());

                        for inst in instantiations {
                            // Add nodes in case they are not present
                            if !self.module_graph_nodes.contains_key(&inst.0) {
                                self.module_graph_nodes.insert(
                                    inst.0.clone(),
                                    self.module_graph.add_node(inst.0.clone()),
                                );
                                self.usages.insert(inst.0.clone(), Vec::new());
                            }
                            if let Some(decl) = self.declarations.get(&inst.0) {
                                if decl.0 != inst.1 {
                                    warn!("Possibly type mismatch for {}: declaring {:?}, instantiating {:?}", &inst.0, decl.0, inst.1);
                                }
                            }
                            self.usages.get_mut(&inst.0).unwrap().push((i, inst.2));

                            self.module_graph.update_edge(
                                self.module_graph_nodes[&parent_name],
                                self.module_graph_nodes[&inst.0],
                                (),
                            );
                        }
                        for package in global_packages {
                            self.module_graph.update_edge(
                                self.module_graph_nodes[&parent_name],
                                self.module_graph_nodes[&package.0],
                                (),
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// Build Graph
    pub fn build_graph(&mut self) -> Result<()> {
        // Register declarations
        self.register_declarations()?;

        // Register usages
        self.register_usages()?;

        // TODO: find additional declarations for undeclared but used modules in library

        Ok(())
    }

    /// Prune Graph
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
        self.refactor_graph_nodes(None, None)?;

        Ok(())
    }

    /// Helper function to fix NodeIndex references
    fn refactor_graph_nodes(
        &mut self,
        graph: Option<&Graph<String, ()>>,
        nodes: Option<&mut HashMap<String, NodeIndex>>,
    ) -> Result<()> {
        let int_graph = match graph {
            Some(x) => x,
            None => &self.module_graph,
        };

        let int_nodes = match nodes {
            Some(x) => x,
            None => &mut self.module_graph_nodes,
        };

        let mut new_nodes = HashMap::<String, NodeIndex>::new();

        for node in int_graph.node_indices() {
            new_nodes.insert(int_graph[node].clone(), node);
        }

        *int_nodes = new_nodes;

        Ok(())
    }

    /// Add replacements to remove macro definitions
    pub fn remove_macros(&mut self) -> Result<()> {
        for i in 0..self.all_files.len() {
            let pf = &self.all_files[i];
            for node in &pf.ast {
                if let RefNode::TextMacroDefinition(x) = node {
                    let loc = Locate::try_from(x).unwrap();
                    self.replace_table.push((i, loc, "".to_string()));
                }
            }
        }

        Ok(())
    }

    /// Add replacements to remove timeunit and timeprecision
    pub fn remove_timeunits(&mut self) -> Result<()> {
        for i in 0..self.all_files.len() {
            let pf = &self.all_files[i];
            for node in &pf.ast {
                if let RefNode::TimeunitsDeclaration(x) = node {
                    let loc = Locate::try_from(x).unwrap();
                    self.replace_table.push((i, loc, "".to_string()));
                }
            }
        }

        Ok(())
    }

    /// Add replacements to remove timeunit and timeprecision
    pub fn infer_dot_star(&mut self) -> Result<()> {
        for i in 0..self.all_files.len() {
            let pf = &self.all_files[i];

            for node in &pf.ast {
                if let RefNode::ModuleInstantiation(x) = node {
                    let asterisk = unwrap_node!(x, NamedPortConnectionAsterisk);
                    if asterisk.is_some() {
                        let id = unwrap_node!(x, ModuleIdentifier).unwrap();
                        let (inst_name, _) = get_identifier(&pf.ast, id).clone();
                        let list_of_ports_node =
                            unwrap_node!(x, ListOfPortConnectionsNamed).unwrap();
                        let mut already_added_ports = Vec::new();
                        for port in list_of_ports_node {
                            if let RefNode::PortIdentifier(p) = port {
                                // we need this check to be sure that it is a port and not an
                                // expression
                                let port_node = unwrap_node!(p, SimpleIdentifier).unwrap();
                                let (port_name, _) = get_identifier(&pf.ast, port_node.clone());
                                already_added_ports.push(port_name.clone());
                            }
                        }
                        let module_declaration = self.get_node_from_locate(
                            self.declarations[&inst_name].1,
                            self.declarations[&inst_name].2,
                        )?;
                        let mut all_port_of_module = Vec::new();
                        for n in module_declaration {
                            match n {
                                RefNode::PortDeclaration(p) => {
                                    let id = unwrap_node!(p, PortIdentifier).unwrap();
                                    let (port_name, _) = get_identifier(
                                        &self.all_files[self.declarations[&inst_name].1].ast,
                                        id,
                                    );
                                    println!("{}", port_name);
                                    all_port_of_module.push(port_name);
                                }
                                RefNode::AnsiPortDeclaration(p) => {
                                    let id = unwrap_node!(p, PortIdentifier).unwrap();
                                    let (port_name, _) = get_identifier(
                                        &self.all_files[self.declarations[&inst_name].1].ast,
                                        id,
                                    );
                                    all_port_of_module.push(port_name);
                                }
                                _ => {}
                            }
                        }
                        let mut ports_string = String::new();
                        for port in all_port_of_module {
                            if !already_added_ports.contains(&port) {
                                ports_string.push_str(&format!(".{}({}), ", port, port));
                            }
                        }
                        ports_string = ports_string.trim_end_matches(", ").to_string();
                        let loc = unwrap_locate!(asterisk.unwrap()).unwrap();
                        self.replace_table.push((i, *loc, ports_string));
                    }
                }
            }
        }

        Ok(())
    }

    fn get_node_from_locate(&self, file_id: usize, location: Locate) -> Result<RefNode> {
        let node = self.all_files[file_id].ast.into_iter().find(|x|
            match x {
                RefNode::ModuleDeclaration(y) => Locate::try_from(*y).unwrap(),
                RefNode::InterfaceDeclaration(y) => Locate::try_from(*y).unwrap(),
                RefNode::PackageDeclaration(y) => Locate::try_from(*y).unwrap(),
                RefNode::DescriptionPackageItem(y) => Locate::try_from(*y).unwrap(),
                RefNode::TextMacroDefinition(y) => Locate::try_from(*y).unwrap(),
                RefNode::ModuleInstantiation(y) => Locate::try_from(*y).unwrap(),
                RefNode::PackageImportItem(y) => Locate::try_from(*y).unwrap(),
                RefNode::PackageScope(y) => Locate::try_from(*y).unwrap(),
                RefNode::InterfacePortHeader(y) => Locate::try_from(*y).unwrap(),
                RefNode::InterfaceInstantiation(y) => Locate::try_from(*y).unwrap(),
                RefNode::ClassScope(y) => Locate::try_from(*y).unwrap(),
                _ => Locate { offset: 0, line: 0, len: 0 }
            } == location
        );

        match node {
            Some(x) => Ok(x),
            None => Err(anyhow!(
                "Internal error matching the specified location: {:?}",
                location
            )),
        }
    }

    /// Add replacements to rename modules with prefix and suffix
    pub fn rename(
        &mut self,
        prefix: Option<&String>,
        suffix: Option<&String>,
        exclude_rename: HashSet<&String>,
    ) -> Result<()> {
        info!("Prefixing: {:?}, Suffixing: {:?}", prefix, suffix);
        for (name, module) in &self.declarations {
            if exclude_rename.contains(name) {
                continue;
            }
            let mut new_string = name.to_string();
            if let Some(ref pre) = prefix {
                new_string = format!("{}{}", pre, new_string);
            }
            if let Some(ref suf) = suffix {
                new_string = format!("{}{}", new_string, suf);
            }

            let found_node = self.get_node_from_locate(module.1, module.2)?;
            let node_named = unwrap_node!(
                unwrap_node!(
                    found_node,
                    ModuleIdentifier,
                    PackageIdentifier,
                    InterfaceIdentifier
                )
                .unwrap(),
                SimpleIdentifier,
                EscapedIdentifier
            )
            .unwrap();
            let found_locate = match node_named {
                RefNode::SimpleIdentifier(x) => x.nodes.0,
                RefNode::EscapedIdentifier(x) => x.nodes.0,
                _ => unimplemented!(),
            };

            self.replace_table
                .push((module.1, found_locate, new_string.to_string()));

            for (use_file_id, use_locate) in &self.usages[name] {
                let use_node = self.get_node_from_locate(*use_file_id, *use_locate)?;
                let use_identifier = unwrap_node!(
                    unwrap_node!(
                        use_node,
                        ModuleIdentifier,
                        InterfaceIdentifier,
                        ClassScope,
                        PackageIdentifier
                    )
                    .unwrap(),
                    SimpleIdentifier,
                    EscapedIdentifier
                )
                .unwrap();
                let use_final_locate = match use_identifier {
                    RefNode::SimpleIdentifier(x) => x.nodes.0,
                    RefNode::EscapedIdentifier(x) => x.nodes.0,
                    _ => unimplemented!(),
                };
                self.replace_table
                    .push((*use_file_id, use_final_locate, new_string.to_string()))
            }
        }

        Ok(())
    }

    /// Helper function to get string of Locate in a file with needed string replacements
    fn get_replaced_string(&self, file_id: usize, location: Locate) -> Result<String> {
        let int_offset = location.offset;
        let int_len = location.len;

        let mut replacements = self
            .replace_table
            .iter()
            .filter(|x| x.0 == file_id)
            .map(|x| (x.1.offset, x.1.len, x.2.clone()))
            .filter(|x| x.0 > int_offset)
            .map(|x| (x.0 - int_offset, x.1, x.2))
            .filter(|x| x.0 < int_len)
            .collect::<Vec<_>>();

        replacements.sort_by(|a, b| a.0.cmp(&b.0));

        // Error on overlapping -> TODO: figure out how to handle overlapped replacements
        for i in 1..replacements.len() {
            if replacements[i - 1].0 + replacements[i - 1].1 > replacements[i].0 {
                eprintln!(
                    "Replacement offset error, the selected replacements may not be supported yet\n{:?}",
                    replacements[i-1]
                );
                unimplemented!();
            }
        }

        let needed_string = self.all_files[file_id]
            .ast
            .get_str(&location)
            .unwrap()
            .to_string();

        let mut out_string = "".to_owned();
        let mut pos = 0;

        for (offset, len, repl) in replacements.iter() {
            info!(
                "Replacing {:?} with {:?}",
                &needed_string[*offset..*offset + *len],
                &repl
            );
            out_string.push_str(&needed_string[pos..*offset]);
            out_string.push_str(repl);
            pos = offset + len;
        }
        out_string.push_str(&needed_string[pos..]);
        if !needed_string.ends_with('\n') {
            out_string.push('\n');
        }

        Ok(out_string)
    }

    /// Export AST to a pickled file
    pub fn get_pickle(&mut self, mut out: Box<dyn Write>, exclude: HashSet<&String>) -> Result<()> {
        write!(
            out,
            "// Compiled by morty-{} / {}\n\n",
            env!("CARGO_PKG_VERSION"),
            Local::now()
        )
        .unwrap();

        let mut internal_graph = self.module_graph.clone();
        let mut internal_nodes = self.module_graph_nodes.clone();

        let mut keeper_nodes = Vec::new();
        let mut keeper_names = Vec::new();
        for (name, node) in &internal_nodes {
            if self.declarations.contains_key(name) {
                keeper_nodes.push(*node);
                keeper_names.push(name);
            }
        }

        internal_graph.retain_nodes(|_, n| keeper_nodes.contains(&n));
        self.refactor_graph_nodes(Some(&internal_graph), Some(&mut internal_nodes))?;

        let mut limit = internal_nodes.len();
        while !internal_nodes.is_empty() {
            if limit == 0 {
                // Can be caused by cyclical module instantiations
                return Err(anyhow!(
                    "Unable to print individual modules, loop not terminating."
                ));
            }
            limit -= 1;
            for (name, node) in &internal_nodes {
                if internal_graph.neighbors_directed(*node, Outgoing).count() == 0 {
                    // Remove node from graph
                    if internal_graph.remove_node(*node).is_none() {
                        return Err(anyhow!("Unable to remove node {} from graph", name));
                    }

                    if self.declarations.contains_key(name) && !exclude.contains(name) {
                        writeln!(
                            out,
                            "{:}",
                            self.get_replaced_string(
                                self.declarations[name].1,
                                self.declarations[name].2
                            )?
                        )?;
                    }
                    break;
                }
            }
            self.refactor_graph_nodes(Some(&internal_graph), Some(&mut internal_nodes))?;
        }

        Ok(())
    }

    /// Export AST to a pickled file
    pub fn get_classic_pickle(
        &mut self,
        mut out: Box<dyn Write>,
        _exclude: HashSet<&String>,
    ) -> Result<()> {
        write!(
            out,
            "// Compiled by morty-{} / {}\n\n",
            env!("CARGO_PKG_VERSION"),
            Local::now()
        )
        .unwrap();

        for i in 0..self.all_files.len() {
            // I don't think exclude is handled properly in the classic version...

            for node in &self.all_files[i].ast {
                if let RefNode::SourceText(x) = node {
                    let source_locate = Locate::try_from(x).unwrap();
                    writeln!(out, "{:}", self.get_replaced_string(i, source_locate)?)?;
                }
            }
        }

        Ok(())
    }

    /// get .dot file for the module graph
    pub fn get_dot(&self, mut out: Box<dyn Write>) -> Result<()> {
        writeln!(
            out,
            "{:?}",
            petgraph::dot::Dot::with_config(
                &self.module_graph,
                &[petgraph::dot::Config::EdgeNoLabel]
            )
        )?;

        Ok(())
    }

    /// get manifest file
    pub fn get_manifest(
        &self,
        mut out: Box<dyn Write>,
        file_list: Vec<FileBundle>,
        include_dirs: Vec<String>,
        defines: HashMap<String, Option<String>>,
    ) -> Result<()> {
        let mut undef_modules = Vec::new();
        // find undefined modules
        for module in self.module_graph_nodes.keys() {
            if !self.declarations.contains_key(module) {
                undef_modules.push(module.to_string());
            }
        }

        let mut top_modules = Vec::new();
        // find top modules
        for (name, node) in &self.module_graph_nodes {
            if self
                .module_graph
                .neighbors_directed(*node, Incoming)
                .count()
                == 0
            {
                top_modules.push(name.to_string());
            }
        }

        let mut base_files = Vec::new();
        let mut bundles = Vec::<FileBundle>::new();
        let mut needed_files = HashSet::new();
        for module in self.declarations.values() {
            needed_files.insert(self.all_files[module.1].path.clone());
        }
        for mut bundle in file_list {
            if bundle.include_dirs == include_dirs && bundle.defines == defines {
                base_files.extend(bundle.files.clone());
                // May need to disable the following for backwards compatibility
                base_files.retain(|v| needed_files.clone().contains(v));
            } else {
                // May need to disable the following for backwards compatibility
                bundle.files.retain(|v| needed_files.clone().contains(v));
                if !bundle.files.is_empty() {
                    bundles.push(bundle);
                }
            }
        }
        // TODO: add libs
        // base_files.extend()

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

        writeln!(out, "{}", json)?;

        Ok(())
    }

    /// Function to only preprocess files
    pub fn just_preprocess(&self, mut out: Box<dyn Write>) -> Result<()> {
        write!(
            out,
            "// Compiled by morty-{} / {}\n\n",
            env!("CARGO_PKG_VERSION"),
            Local::now()
        )
        .unwrap();
        for pf in &self.all_files {
            eprintln!("{}:", pf.path);

            for node in &pf.ast {
                if let RefNode::SourceText(x) = node {
                    writeln!(out, "{:}", pf.ast.get_str(x).unwrap())?;
                }
            }
        }
        Ok(())
    }

    /// Function to build documentation
    pub fn build_doc(&self, dir: &str) -> Result<()> {
        let doc = doc::Doc::new(&self.all_files);
        let mut html = doc::Renderer::new(Path::new(dir));
        html.render(&doc)?;
        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub enum SVConstructType {
    Module,
    Interface,
    Package,
    _Class,
}
