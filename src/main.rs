// Copyright 2019 Fabian Schuiki
// Copyright 2019 Florian Zaruba

// SPDX-License-Identifier: Apache-2.0

use anyhow::Error;
use clap::{App, Arg};
use log::{debug, info, trace};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::process::exit;
use sv_parser::preprocess;
use sv_parser::Error as SvParserError;
mod printer;
use sv_parser::{parse_sv_str, unwrap_node, Define, DefineText, Locate, RefNode, SyntaxTree};
extern crate log;
extern crate simple_logger;
use std::io::Write;
use tempfile::tempdir;

/// Struct containing information about
/// what should be pickled and how.
#[derive(Debug)]
struct Pickle<'a> {
    /// Optional name prefix.
    prefix: Option<&'a str>,
    /// Optional name suffix.
    suffix: Option<&'a str>,
    /// Declarations which are excluded from re-naming.
    exclude: HashSet<&'a str>,
    /// Table containing thing that should be re-named.
    rename_table: HashMap<String, String>,
    /// Locations of text which should be replaced.
    replace_table: Vec<(usize, usize, String)>,
}

impl<'a> Pickle<'a> {
    /// Register a declaration such as a package or module.
    fn register_declaration(&mut self, syntax_tree: &SyntaxTree, id: RefNode) -> () {
        let (module_name, loc) = get_identifier(syntax_tree, id);
        if self.exclude.contains(module_name.as_str()) {
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
        self.rename_table.insert(module_name, new_name.clone());
    }
    /// Register a usage of the identifier.
    fn register_usage(&mut self, syntax_tree: &SyntaxTree, id: RefNode) -> () {
        let (inst_name, loc) = get_identifier(&syntax_tree, id);
        let new_name = match self.rename_table.get(&inst_name) {
            Some(x) => x,
            None => return,
        };
        debug!("Usage `{}`: {:?}", inst_name, loc);
        self.replace_table
            .push((loc.offset, loc.len, new_name.clone()));
    }
}

fn main() -> Result<(), Error> {
    let matches = App::new(env!("CARGO_PKG_NAME"))
        .version(clap::crate_version!())
        .author(clap::crate_authors!())
        .about(clap::crate_description!())
        .arg(
            Arg::with_name("inc")
                .short("I")
                .value_name("DIR")
                .help("Add a search path for SystemVerilog includes")
                .multiple(true)
                .takes_value(true)
                .number_of_values(1),
        )
        .arg(
            Arg::with_name("exclude")
                .short("e")
                .long("exclude")
                .value_name("MODULE")
                .help("Add modules which should not be renamed")
                .multiple(true)
                .takes_value(true)
                .number_of_values(1),
        )
        .arg(
            Arg::with_name("v")
                .short("v")
                .multiple(true)
                .help("Sets the level of verbosity"),
        )
        .arg(
            Arg::with_name("prefix")
                .short("p")
                .long("prefix")
                .value_name("PREFIX")
                .help("Prepend a name to all global names")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("def")
                .short("D")
                .value_name("DEFINE")
                .help("Define a preprocesor macro")
                .multiple(true)
                .takes_value(true)
                .number_of_values(1),
        )
        .arg(
            Arg::with_name("suffix")
                .short("s")
                .long("suffix")
                .value_name("SUFFIX")
                .help("Append a name to all global names")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("preproc")
                .short("E")
                .help("Write preprocessed input files to stdout"),
        )
        .arg(
            Arg::with_name("file_list")
                .short("f")
                .value_name("LIST")
                .help("Gather files from a manifest")
                .multiple(true)
                .takes_value(true)
                .number_of_values(1),
        )
        // Currently not available.
        // .arg(
        //     Arg::with_name("minimize")
        //         .long("minimize")
        //         .help("Minimize the output"),
        // )
        // .arg(
        //     Arg::with_name("strip_comments")
        //         .long("strip-comments")
        //         .help("Strip comments from the output"),
        // )
        .arg(
            Arg::with_name("INPUT")
                .help("The input files to compile")
                .multiple(true),
        )
        .get_matches();

    // Instantiate a new logger with the verbosity level the user requested.
    simple_logger::init_with_level(match matches.occurrences_of("v") {
        0 => log::Level::Warn,
        1 => log::Level::Info,
        2 => log::Level::Debug,
        3 | _ => log::Level::Trace,
    })
    .unwrap();

    let mut file_list = Vec::new();

    // Handle user defines.
    let defines: HashMap<_, _> = match matches.values_of("def") {
        Some(args) => args
            .map(|x| {
                let mut iter = x.split("=");
                (
                    iter.next().unwrap().to_string(),
                    iter.next().map(String::from),
                )
            })
            .collect(),
        None => HashMap::new(),
    };

    // Prepare a list of include paths.
    let include_dirs: Vec<_> = matches
        .values_of("inc")
        .into_iter()
        .flat_map(|args| args)
        .map(|x| x.to_string())
        .collect();

    for path in matches
        .values_of("file_list")
        .into_iter()
        .flat_map(|args| args)
    {
        let file = File::open(path).unwrap();
        let reader = BufReader::new(file);

        // Read the JSON contents of the file as an instance of `User`.
        let mut u: Vec<FileBundle> = serde_json::from_reader(reader).unwrap();
        for fb in &mut u {
            fb.defines.extend(defines.clone());
            fb.include_dirs.extend(include_dirs.clone());
        }
        file_list.extend(u);
    }

    if let Some(file_names) = matches.values_of("INPUT") {
        file_list.push(FileBundle {
            include_dirs,
            defines,
            files: file_names.into_iter().map(String::from).collect(),
        });
    }

    let mut exclude = HashSet::new();
    exclude.extend(matches.values_of("exclude").into_iter().flat_map(|v| v));

    let mut pickle = Pickle {
        // Collect renaming options.
        prefix: matches.value_of("prefix"),
        suffix: matches.value_of("suffix"),
        exclude: exclude,
        // Create a rename table.
        rename_table: HashMap::new(),
        replace_table: Vec::new(),
    };

    // Parse the input files.
    let mut buffer = String::new();
    // let minimize = matches.is_present("minimize");
    // let strip_comments = matches.is_present("strip_comments");
    for bundle in file_list {
        let bundle_include_dirs: Vec<_> = bundle.include_dirs.iter().map(Path::new).collect();
        // Convert the preprocessor defines into the appropriate format which is understood by `sv-parser`
        let bundle_defines: HashMap<_, _> = bundle
            .defines
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
        // For each file in the file bundle preprocess and parse it.
        for filename in bundle.files {
            info!("{:?}", filename);
            // Preprocess the verilog files.
            match preprocess(filename, &bundle_defines, &bundle_include_dirs, false) {
                Ok(preprocessed) => {
                    buffer.push_str(preprocessed.0.text());
                }
                Err(err) => {
                    eprintln!("{:?}", err);
                    exit(1);
                }
            }
            // buffer.push_str(&std::fs::read_to_string(filename).unwrap());
        }

        // Just preprocess.
        if matches.is_present("preproc") {
            println!("{}", buffer);
            return Ok(());
        }

        // Create a temporary file where the pickled sources live.
        let dir = tempdir()?;

        let file_path = dir.path().join("pickle.sv");
        let mut tmpfile = File::create(&file_path)?;

        writeln!(tmpfile, "{}", buffer)?;
        let mut printer = printer::Printer::new();

        // Parse the preprocessed SV file.
        match parse_sv_str(
            buffer.as_str(),
            file_path,
            &bundle_defines,
            &bundle_include_dirs,
            false,
        ) {
            Ok((syntax_tree, _)) => {
                // SV parser implements an iterator on the AST.
                for node in &syntax_tree {
                    trace!("{:?}", node);
                    match node {
                        // Module declarations.
                        RefNode::ModuleDeclarationAnsi(x) => {
                            // unwrap_node! gets the nearest ModuleIdentifier from x
                            let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                            pickle.register_declaration(&syntax_tree, id);
                        }
                        RefNode::ModuleDeclarationNonansi(x) => {
                            let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                            pickle.register_declaration(&syntax_tree, id);
                        }
                        // Instantiations, end-labels.
                        RefNode::ModuleIdentifier(x) => {
                            let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                            pickle.register_usage(&syntax_tree, id);
                        }
                        // Interface Declaration.
                        RefNode::InterfaceDeclaration(x) => {
                            let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                            pickle.register_declaration(&syntax_tree, id);
                        }
                        // Interface identifier.
                        RefNode::InterfaceIdentifier(x) => {
                            let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                            pickle.register_usage(&syntax_tree, id);
                        }
                        // Package declarations.
                        RefNode::PackageDeclaration(x) => {
                            let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                            pickle.register_declaration(&syntax_tree, id);
                        }
                        // Package Qualifier (i.e., explicit package constants).
                        RefNode::ClassQualifierOrPackageScope(x) => {
                            if let Some(id) = unwrap_node!(x, SimpleIdentifier) {
                                pickle.register_usage(&syntax_tree, id);
                            }
                        }
                        // Package Import.
                        RefNode::PackageIdentifier(x) => {
                            let id = unwrap_node!(x, SimpleIdentifier).unwrap();
                            pickle.register_usage(&syntax_tree, id);
                        }
                        _ => (),
                    }
                }
            }
            Err(err) => {
                print_parse_error(&mut printer, err, false)?;
                exit(1);
            }
        }

        // Replace according to `replace_table`.
        // Apply the replacements.
        pickle.replace_table.sort();
        debug!("{:?}", pickle.replace_table);
        let mut pos = 0;
        for (offset, len, repl) in pickle.replace_table.iter() {
            trace!("Replacing: {},{}, {}", offset, len, repl);
            print!("{}", &buffer[pos..*offset]);
            print!("{}", repl);
            pos = offset + len;
        }
        print!("{}", &buffer[pos..]);
    }
    Ok(())
}

fn get_identifier(st: &SyntaxTree, node: RefNode) -> (String, Locate) {
    // unwrap_node! can take multiple types
    match unwrap_node!(node, SimpleIdentifier, EscapedIdentifier) {
        Some(RefNode::SimpleIdentifier(x)) => {
            // Original string can be got by SyntaxTree::get_str(self, locate: &Locate)
            return (String::from(st.get_str(&x.nodes.0).unwrap()), x.nodes.0);
        }
        Some(RefNode::EscapedIdentifier(x)) => {
            return (String::from(st.get_str(&x.nodes.0).unwrap()), x.nodes.0);
        }
        _ => panic!("No identifier found."),
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct FileBundle {
    include_dirs: Vec<String>,
    defines: HashMap<String, Option<String>>,
    files: Vec<String>,
}

#[cfg_attr(tarpaulin, skip)]
fn print_parse_error(
    printer: &mut printer::Printer,
    error: SvParserError,
    single: bool,
) -> Result<(), Error> {
    match error {
        SvParserError::Parse(Some((path, pos))) => {
            printer.print_parse_error(&path, pos, single)?;
        }
        SvParserError::Include { source: x } => {
            if let SvParserError::File { path: x, .. } = *x {
                printer.print_error(&format!("failed to include '{}'", x.to_string_lossy()))?;
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
