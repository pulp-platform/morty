// Copyright 2019 Fabian Schuiki
// Copyright 2019 Florian Zaruba

// SPDX-License-Identifier: Apache-2.0

use clap::{App, Arg};
use moore_common::source::Span;
use moore_svlog_syntax::ast;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

fn main() {
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
        .arg(
            Arg::with_name("minimize")
                .long("minimize")
                .help("Minimize the output"),
        )
        .arg(
            Arg::with_name("strip_comments")
                .long("strip-comments")
                .help("Strip comments from the output"),
        )
        .arg(
            Arg::with_name("INPUT")
                .help("The input files to compile")
                .multiple(true),
        )
        .get_matches();

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

        // println!("{:?}", u);
        file_list.extend(u);
    }

    if let Some(file_names) = matches.values_of("INPUT") {
        file_list.push(FileBundle {
            include_dirs,
            defines,
            files: file_names.into_iter().map(String::from).collect(),
        });
    }

    // Parse the input files.
    let mut buffer = String::new();
    let sm = moore_common::source::get_source_manager();
    let minimize = matches.is_present("minimize");
    let strip_comments = matches.is_present("strip_comments");
    for bundle in file_list {
        let bundle_include_dirs: Vec<_> = bundle.include_dirs.iter().map(Path::new).collect();
        let bundle_defines: Vec<_> = bundle
            .defines
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_ref().map(|x| x.as_str())))
            .collect();
        for filename in bundle.files {
            // Add the file to the source manager.
            let source = match sm.open(&filename) {
                Some(s) => s,
                None => panic!("Unable to open input file '{}'", filename),
            };

            // Preprocess the file and accumulate the contents into the pickle buffer.
            let preproc = moore_svlog_syntax::preproc::Preprocessor::new(
                source,
                &bundle_include_dirs,
                &bundle_defines,
            );
            let mut has_whitespace = false;
            let mut has_newline = false;
            use moore_svlog_syntax::cat::CatTokenKind;
            for res in preproc {
                let res = res.unwrap();
                if minimize {
                    match res.0 {
                        CatTokenKind::Newline => has_newline = true,
                        CatTokenKind::Whitespace | CatTokenKind::Comment => has_whitespace = true,
                        _ => {
                            if has_newline {
                                // buffer.push('\n');
                                buffer.push(' ');
                            } else if has_whitespace {
                                buffer.push(' ');
                            }
                            has_whitespace = false;
                            has_newline = false;
                            buffer.push_str(&res.1.extract());
                        }
                    }
                } else {
                    if strip_comments && res.0 == CatTokenKind::Comment {
                        continue;
                    }
                    buffer.push_str(&res.1.extract());
                }
            }
            buffer.push_str("\n");
        }
    }

    if matches.is_present("preproc") {
        println!("{}", buffer);
        return;
    }

    // Parse the preprocessed file.
    let source = sm.add("preproc", &buffer);
    let preproc = moore_svlog_syntax::preproc::Preprocessor::new(source, &[], &[]);
    let lexer = moore_svlog_syntax::lexer::Lexer::new(preproc);
    let ast = match moore_svlog_syntax::parser::parse(lexer) {
        Ok(x) => x,
        Err(()) => std::process::exit(1),
    };
    // eprintln!("parsed {} items", ast.items.len());

    // Walk the AST.
    let mut visitor = AstVisitor::default();
    visitor.visit_root(&ast);
    // eprintln!("{:#?}", visitor);

    // Collect renaming options.
    let prefix = matches.value_of("prefix");
    let suffix = matches.value_of("suffix");
    let mut exclude = HashSet::new();
    exclude.extend(matches.values_of("exclude").into_iter().flat_map(|v| v));
    // exclude.insert("billywig".to_owned());
    // eprintln!("exclude: {:?}", exclude);

    // Create a rename table.
    let mut rename_table = HashMap::new();
    let mut replace_table = Vec::new();

    for (module_name, module_span) in &visitor.module_decls {
        if exclude.contains(module_name.as_str()) {
            continue;
        }
        let mut new_name = module_name.clone();
        if let Some(prefix) = prefix {
            new_name = format!("{}{}", prefix, new_name);
        }
        if let Some(suffix) = suffix {
            new_name = format!("{}{}", new_name, suffix);
        }
        rename_table.insert(module_name, new_name.clone());
        replace_table.push((module_span.begin, module_span.end, new_name));
    }
    // eprintln!("{:#?}", rename_table);

    // Rename instances.
    for (inst_name, inst_span) in &visitor.module_insts {
        let new_name = match rename_table.get(&inst_name) {
            Some(x) => x,
            None => continue,
        };
        replace_table.push((inst_span.begin, inst_span.end, new_name.clone()));
    }

    // Apply the replacements.
    replace_table.sort();
    // eprintln!("{:#?}", replace_table);
    let mut pos = 0;
    for (begin, end, repl) in replace_table {
        print!("{}", &buffer[pos..begin]);
        print!("{}", repl);
        pos = end;
    }
    print!("{}", &buffer[pos..]);
}

#[derive(Debug, Default)]
struct AstVisitor {
    module_decls: Vec<(String, Span)>,
    pkg_decls: Vec<(String, Span)>,
    module_insts: Vec<(String, Span)>,
}

impl AstVisitor {
    fn visit_root(&mut self, root: &ast::Root) {
        for item in &root.items {
            match item {
                ast::Item::Module(decl) => self.visit_module(decl),
                ast::Item::Package(decl) => self.visit_package(decl),
                _ => (),
            }
        }
    }

    fn visit_module(&mut self, module: &ast::ModDecl) {
        self.module_decls
            .push((module.name.to_string(), module.name_span));
        self.visit_hierachy_items(&module.items);
    }

    fn visit_package(&mut self, pkg: &ast::PackageDecl) {
        self.pkg_decls.push((pkg.name.to_string(), pkg.name_span));
        self.visit_hierachy_items(&pkg.items);
    }

    fn visit_hierachy_items(&mut self, items: &[ast::HierarchyItem]) {
        for hitem in items {
            match hitem {
                ast::HierarchyItem::Inst(inst) => self
                    .module_insts
                    .push((inst.target.name.to_string(), inst.target.span)),
                ast::HierarchyItem::GenerateRegion(_, items) => self.visit_hierachy_items(items),
                ast::HierarchyItem::GenerateFor(gen) => self.visit_hierachy_items(&gen.block.items),
                ast::HierarchyItem::GenerateIf(gen) => {
                    self.visit_hierachy_items(&gen.main_block.items);
                    if let Some(gen) = &gen.else_block {
                        self.visit_hierachy_items(&gen.items);
                    }
                }
                _ => (),
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct FileBundle {
    include_dirs: Vec<String>,
    defines: HashMap<String, Option<String>>,
    files: Vec<String>,
}
