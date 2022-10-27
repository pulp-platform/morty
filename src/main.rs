// Copyright 2019 Fabian Schuiki
// Copyright 2019 Florian Zaruba

// SPDX-License-Identifier: Apache-2.0
#![recursion_limit = "256"]

#[macro_use]
extern crate log;

use anyhow::Result;
use clap::{Arg, Command};
use log::LevelFilter;
use simple_logger::SimpleLogger;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process;

use morty::*;

fn main() -> Result<()> {
    let matches = Command::new(env!("CARGO_PKG_NAME"))
        .version(clap::crate_version!())
        .author(clap::crate_authors!())
        .about(clap::crate_description!())
        .arg(
            Arg::new("inc")
                .short('I')
                .value_name("DIR")
                .help("Add a search path for SystemVerilog includes")
                .multiple_occurrences(true)
                .takes_value(true),
        )
        .arg(
            Arg::new("exclude_rename")
                .short('e')
                .long("exclude-rename")
                .value_name("MODULE|INTERFACE|PACKAGE")
                .help("Add module, interface, package which should not be renamed")
                .multiple_occurrences(true)
                .takes_value(true),
        )
        .arg(
            Arg::new("exclude")
                .long("exclude")
                .value_name("MODULE|INTERFACE|PACKAGE")
                .help("Do not include module, interface, package in the pickled file list")
                .multiple_occurrences(true)
                .takes_value(true),
        )
        .arg(
            Arg::new("v")
                .short('v')
                .multiple_occurrences(true)
                .help("Sets the level of verbosity"),
        )
        .arg(
            Arg::new("prefix")
                .short('p')
                .long("prefix")
                .value_name("PREFIX")
                .help("Prepend a name to all global names")
                .takes_value(true),
        )
        .arg(
            Arg::new("def")
                .short('D')
                .value_name("DEFINE")
                .help("Define a preprocesor macro")
                .multiple_occurrences(true)
                .takes_value(true),
        )
        .arg(
            Arg::new("suffix")
                .short('s')
                .long("suffix")
                .value_name("SUFFIX")
                .help("Append a name to all global names")
                .takes_value(true),
        )
        .arg(
            Arg::new("preproc")
                .short('E')
                .help("Write preprocessed input files to stdout"),
        )
        .arg(
            Arg::new("file_list")
                .short('f')
                .value_name("LIST")
                .help("Gather files from a manifest")
                .multiple_occurrences(true)
                .takes_value(true),
        )
        .arg(
            Arg::new("strip_comments")
                .long("strip-comments")
                .help("Strip comments from the output"),
        )
        .arg(
            Arg::new("INPUT")
                .help("The input files to compile")
                .multiple_occurrences(true),
        )
        .arg(
            Arg::new("docdir")
                .short('d')
                .long("doc")
                .value_name("OUTDIR")
                .help("Generate documentation in a directory")
                .takes_value(true),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .value_name("FILE")
                .help("Write output to file")
                .takes_value(true),
        )
        .arg(
            Arg::new("library_file")
                .long("library-file")
                .help("File to search for SystemVerilog modules")
                .value_name("FILE")
                .takes_value(true)
                .multiple_occurrences(true),
        )
        .arg(
            Arg::new("library_dir")
                .short('y')
                .long("library-dir")
                .help("Directory to search for SystemVerilog modules")
                .value_name("DIR")
                .takes_value(true)
                .multiple_occurrences(true),
        )
        .arg(
            Arg::new("manifest")
                .long("manifest")
                .value_name("FILE")
                .help("Output a JSON-encoded source information manifest to FILE")
                .takes_value(true),
        )
        .arg(
            Arg::new("top_module")
                .long("top")
                .value_name("TOP_MODULE")
                .help("Top module, strip all unneeded modules")
                .takes_value(true),
        )
        .arg(
            Arg::new("graph_file")
                .long("graph_file")
                .value_name("FILE")
                .help("Output a DOT graph of the parsed modules")
                .takes_value(true),
        )
        .arg(
            Arg::new("ignore_unparseable")
                .short('i')
                .help("Ignore files that cannot be parsed"),
        )
        .arg(
            Arg::new("keep_defines")
                .long("keep_defines")
                .help("Prevents removal of `define statements."),
        )
        .arg(
            Arg::new("propagate_defines")
                .help("Propagate defines from first files to the following files. Incompatible with `--top`."),
        )
        .arg(
            Arg::new("sequential")
                .short('q')
                .help("Enforce sequential processing of files. Slows down performance, but can avoid STACK_OVERFLOW.")
        )
        .get_matches();

    let logger_level = matches.occurrences_of("v");

    // Instantiate a new logger with the verbosity level the user requested.
    SimpleLogger::new()
        .with_level(match logger_level {
            0 => LevelFilter::Warn,
            1 => LevelFilter::Info,
            2 => LevelFilter::Debug,
            _ => LevelFilter::Trace,
        })
        .with_utc_timestamps()
        .init()
        .unwrap();

    let mut file_list = Vec::new();

    // Handle user defines.
    let defines: HashMap<_, _> = match matches.values_of("def") {
        Some(args) => args
            .map(|x| {
                let mut iter = x.split('=');
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
        .flatten()
        .map(|x| x.to_string())
        .collect();

    // a hashmap from 'module name' to 'path' for all libraries.
    let mut library_files = HashMap::new();
    // a list of paths for all library files
    let mut library_paths: Vec<PathBuf> = Vec::new();

    // we first accumulate all library files from the 'library_dir' and 'library_file' options into
    // a vector of paths, and then construct the library hashmap.
    for dir in matches.values_of("library_dir").into_iter().flatten() {
        for entry in std::fs::read_dir(dir).unwrap_or_else(|e| {
            eprintln!("error accessing library directory `{}`: {}", dir, e);
            process::exit(1)
        }) {
            let dir = entry.unwrap();
            library_paths.push(dir.path());
        }
    }

    if let Some(library_names) = matches.values_of("library_file") {
        let files = library_names.map(PathBuf::from).collect();
        library_paths.push(files);
    }

    for p in &library_paths {
        // must have the library extension (.v or .sv).
        if has_libext(p) {
            if let Some(m) = lib_module(p) {
                library_files.insert(m, p.to_owned());
            }
        }
    }

    let library_bundle = LibraryBundle {
        include_dirs: include_dirs.clone(),
        defines: defines.clone(),
        files: library_files,
    };

    for path in matches.values_of("file_list").into_iter().flatten() {
        let file = File::open(path).unwrap_or_else(|e| {
            eprintln!("error opening `{}`: {}", path, e);
            process::exit(1)
        });
        let reader = BufReader::new(file);

        // Read the JSON contents of the file as an instance of `User`.
        let mut u: Vec<FileBundle> = serde_json::from_reader(reader).unwrap_or_else(|e| {
            eprintln!("error parsing json in `{}`: {}", path, e);
            process::exit(1)
        });
        for fb in &mut u {
            for (_k, v) in fb.export_incdirs.clone() {
                fb.include_dirs.extend(v);
            }
            fb.defines.extend(defines.clone());
            fb.include_dirs.extend(include_dirs.clone());
        }
        file_list.extend(u);
    }

    if let Some(file_names) = matches.values_of("INPUT") {
        file_list.push(FileBundle {
            include_dirs: include_dirs.clone(),
            export_incdirs: HashMap::new(),
            defines: defines.clone(),
            files: file_names.map(String::from).collect(),
        });
    }

    let (mut exclude_rename, mut exclude) = (HashSet::new(), HashSet::new());
    exclude_rename.extend(matches.values_of("exclude_rename").into_iter().flatten());
    exclude.extend(matches.values_of("exclude").into_iter().flatten());

    let strip_comments = matches.is_present("strip_comments");

    let syntax_trees = build_syntax_tree(
        &file_list,
        strip_comments,
        matches.is_present("ignore_unparseable"),
        matches.is_present("propagate_defines"),
        matches.is_present("sequential"),
    )?;

    let out = match matches.value_of("output") {
        Some(file) => {
            info!("Setting output to `{}`", file);
            let path = Path::new(file);
            Box::new(BufWriter::new(File::create(&path).unwrap_or_else(|e| {
                eprintln!("could not create `{}`: {}", file, e);
                process::exit(1);
            }))) as Box<dyn Write>
        }
        None => Box::new(io::stdout()) as Box<dyn Write>,
    };

    // Just preprocess.
    if matches.is_present("preproc") {
        return just_preprocess(syntax_trees, out);
    }

    info!("Finished reading {} source files.", syntax_trees.len());

    // Emit documentation if requested.
    if let Some(dir) = matches.value_of("docdir") {
        info!("Generating documentation in `{}`", dir);
        return build_doc(syntax_trees, dir);
    }

    let pickle = do_pickle(
        matches.value_of("prefix"),
        matches.value_of("suffix"),
        exclude_rename,
        exclude,
        library_bundle,
        syntax_trees,
        out,
        matches.value_of("top_module"),
        matches.contains_id("keep_defines"),
    )?;

    if let Some(graph_file) = matches.value_of("graph_file") {
        write_dot_graph(&pickle, graph_file)?;
    }

    // if the user requested a manifest we need to compute the information and output it in json
    // form
    if let Some(manifest_file) = matches.value_of("manifest") {
        write_manifest(
            manifest_file,
            pickle,
            file_list,
            include_dirs,
            defines,
            matches.value_of("top_module"),
        )?;
    }

    Ok(())
}
