// Copyright 2019 Fabian Schuiki
// Copyright 2019 Florian Zaruba
// Copyright 2022 Michael Rogenmoser

// SPDX-License-Identifier: Apache-2.0
#![recursion_limit = "256"]

#[macro_use]
extern crate log;

use anyhow::{anyhow, Context as _, Error, Result};
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
    parse_sv_pp, preprocess, unwrap_node, Define, DefineText, Locate, RefNode, SyntaxTree,
};

pub mod doc;
mod printer;

pub fn do_pickle<'a>(
    prefix: Option<&'a str>,
    suffix: Option<&'a str>,
    exclude_rename: HashSet<&'a str>,
    exclude: HashSet<&'a str>,
    library_bundle: LibraryBundle,
    mut syntax_trees: Vec<ParsedFile>,
    mut out: Box<dyn Write>,
) -> Result<Pickle<'a>> {
    let mut pickle = Pickle {
        // Collect renaming options.
        prefix: prefix,
        suffix: suffix,
        exclude_rename,
        exclude,
        // Create a rename table.
        rename_table: HashMap::new(),
        replace_table: vec![],
        inst_table: HashSet::new(),
        libs: library_bundle,
        used_libs: vec![],
    };

    // Gather information for pickling.
    for pf in &syntax_trees {
        for node in &pf.ast {
            trace!("{:#?}", node);
            match node {
                // Module declarations.
                RefNode::ModuleDeclarationAnsi(x) => {
                    // unwrap_node! gets the nearest ModuleIdentifier from x
                    let id = unwrap_node!(x, ModuleIdentifier).unwrap();
                    pickle.register_declaration(&pf.ast, id);
                }
                RefNode::ModuleDeclarationNonansi(x) => {
                    let id = unwrap_node!(x, ModuleIdentifier).unwrap();
                    pickle.register_declaration(&pf.ast, id);
                }
                // Interface Declaration.
                RefNode::InterfaceDeclaration(x) => {
                    let id = unwrap_node!(x, InterfaceIdentifier).unwrap();
                    pickle.register_declaration(&pf.ast, id);
                }
                // Package declarations.
                RefNode::PackageDeclaration(x) => {
                    let id = unwrap_node!(x, PackageIdentifier).unwrap();
                    pickle.register_declaration(&pf.ast, id);
                }
                _ => (),
            }
        }
    }

    let mut library_files: Vec<ParsedFile> = vec![];
    for pf in &syntax_trees {
        for node in &pf.ast {
            match node {
                RefNode::ModuleInstantiation(x) => {
                    let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                    pickle.register_instantiation(&pf.ast, id.clone());

                    let (inst_name, _) = get_identifier(&pf.ast, id);
                    if !pickle.rename_table.contains_key(&inst_name) {
                        info!("Could not find {}, checking libraries...", &inst_name);
                        pickle.load_library_module(&inst_name, &mut library_files);
                    }
                }
                _ => (),
            }
        }
    }

    syntax_trees.extend(library_files);

    // Emit the pickled source files.
    for pf in &syntax_trees {
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
                _ => (),
            }
        }
        // Replace according to `replace_table`.
        // Apply the replacements.
        debug!("Replace Table: {:?}", pickle.replace_table);
        let mut pos = 0;
        for (offset, len, repl) in pickle.replace_table.iter() {
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
) -> Result<Vec<ParsedFile>> {
    // Parse the input files.
    let mut syntax_trees = vec![];

    for bundle in file_list {
        let bundle_include_dirs: Vec<_> = bundle.include_dirs.iter().map(Path::new).collect();
        let bundle_defines = defines_to_sv_parser(&bundle.defines);

        // For each file in the file bundle preprocess and parse it.
        // Use a neat trick of `collect` here, which allows you to collect a
        // `Result<T>` iterator into a `Result<Vec<T>>`, i.e. bubbling up the
        // error.
        let v: Result<Vec<ParsedFile>> = bundle
            .files
            .par_iter()
            .map(|filename| -> Result<_> {
                parse_file(
                    &filename,
                    &bundle_include_dirs,
                    &bundle_defines,
                    strip_comments,
                )
            })
            .collect();
        syntax_trees.extend(v?);
    }

    Ok(syntax_trees)
}

pub fn just_preprocess(syntax_trees: Vec<ParsedFile>, mut out: Box<dyn Write>) -> Result<()> {
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
    for (_old_name, new_name) in &pickle.rename_table {
        if !pickle.inst_table.contains(new_name) {
            top_modules.push(new_name.to_string());
        }
    }

    let mut base_files = Vec::new();
    let mut bundles = Vec::new();
    for bundle in file_list {
        if bundle.include_dirs == include_dirs && bundle.defines == defines {
            base_files.extend(bundle.files.clone());
        } else {
            bundles.push(bundle);
        }
    }
    base_files.extend(pickle.used_libs.clone());
    bundles.push(FileBundle {
        include_dirs: include_dirs.clone(),
        export_incdirs: HashMap::new(),
        defines: defines.clone(),
        files: base_files,
    });

    let json = serde_json::to_string_pretty(&Manifest {
        sources: bundles,
        tops: top_modules,
        undefined: undef_modules,
    })
    .unwrap();

    let path = Path::new(manifest_file);
    let mut out = Box::new(BufWriter::new(File::create(&path).unwrap())) as Box<dyn Write>;
    writeln!(out, "{}", json).unwrap();

    Ok(())
}

/// Struct containing information about
/// what should be pickled and how.
#[derive(Debug)]
pub struct Pickle<'a> {
    /// Optional name prefix.
    pub prefix: Option<&'a str>,
    /// Optional name suffix.
    pub suffix: Option<&'a str>,
    /// Declarations which are excluded from re-naming.
    pub exclude_rename: HashSet<&'a str>,
    /// Declarations which are excluded from the pickled sources.
    pub exclude: HashSet<&'a str>,
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
}

impl<'a> Pickle<'a> {
    /// Register a declaration such as a package or module.
    pub fn register_declaration(&mut self, syntax_tree: &SyntaxTree, id: RefNode) {
        let (module_name, loc) = get_identifier(syntax_tree, id);
        println!("module_name: {:?}", module_name);
        if self.exclude_rename.contains(module_name.as_str())
            || self.exclude.contains(module_name.as_str())
        {
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
        let (inst_name, _) = get_identifier(&syntax_tree, id);
        self.inst_table.insert(inst_name);
    }

    /// Register a usage of the identifier.
    pub fn register_usage(&mut self, syntax_tree: &SyntaxTree, id: RefNode) {
        let (inst_name, loc) = get_identifier(&syntax_tree, id);
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
        let (inst_name, loc) = get_identifier(&syntax_tree, id);
        if self.exclude.contains(inst_name.as_str()) {
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
                            self.register_declaration(&pf.ast, id);
                        }
                        RefNode::ModuleDeclarationNonansi(x) => {
                            let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                            self.register_declaration(&pf.ast, id);
                        }
                        _ => (),
                    }
                }
                // look for all module instantiations
                for node in &pf.ast {
                    match node {
                        RefNode::ModuleInstantiation(x) => {
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
                        _ => (),
                    }
                }
                // add the parsed file to the vector.
                files.push(pf);
            }
            Err(e) => info!("error loading library: {}", e),
        }
    }
}

// Returns true if this file has a library extension (.v or .sv).
pub fn has_libext(p: &Path) -> bool {
    match p.extension().and_then(OsStr::to_str) {
        Some("sv") => true,
        Some("v") => true,
        _ => false,
    }
}

// Given a library filename, return the module name that this file must contain. Library files
// must be named as module_name.v or module_name.sv.
pub fn lib_module(p: &Path) -> Option<String> {
    p.with_extension("").file_name()?.to_str().map(String::from)
}

// Convert the preprocessor defines into the appropriate format which is understood by `sv-parser`
pub fn defines_to_sv_parser(
    defines: &HashMap<String, Option<String>>,
) -> HashMap<String, Option<Define>> {
    return defines
        .iter()
        .map(|(name, value)| {
            // If there is a define text add it.
            let define_text = match value {
                Some(x) => Some(DefineText::new(String::from(x), None)),
                None => None,
            };
            (
                name.clone(),
                Some(Define::new(name.clone(), vec![], define_text)),
            )
        })
        .collect();
}

pub fn parse_file(
    filename: &str,
    bundle_include_dirs: &Vec<&Path>,
    bundle_defines: &HashMap<String, Option<Define>>,
    strip_comments: bool,
) -> Result<ParsedFile> {
    info!("{:?}", filename);

    // Preprocess the verilog files.
    let pp = preprocess(
        filename,
        &bundle_defines,
        &bundle_include_dirs,
        strip_comments,
        false,
    )
    .with_context(|| format!("Failed to preprocess `{}`", filename))?;

    let buffer = pp.0.text().to_string();
    let syntax_tree = parse_sv_pp(pp.0, pp.1, false)
        .or_else(|err| -> Result<_> {
            let printer = Arc::new(Mutex::new(printer::Printer::new()));
            let mut printer = &mut *printer.lock().unwrap();
            print_parse_error(&mut printer, &err, false)?;
            Err(Error::new(err))
        })?
        .0;

    Ok(ParsedFile {
        path: String::from(filename),
        source: buffer,
        ast: syntax_tree,
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
        return parse_file(&f, &bundle_include_dirs, &bundle_defines, true);
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
