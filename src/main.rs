// Copyright 2019 Fabian Schuiki
// Copyright 2019 Florian Zaruba

// SPDX-License-Identifier: Apache-2.0
#![recursion_limit = "256"]

#[macro_use]
extern crate log;

use crate::pickle::Pickle;
use anyhow::Result;
use clap::{Arg, ArgAction, Command};
use log::LevelFilter;
use simple_logger::SimpleLogger;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process;

use morty::*;

mod pickle;

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
                .action(ArgAction::Append)
                .num_args(1),
        )
        .arg(
            Arg::new("exclude_rename")
                .short('e')
                .long("exclude-rename")
                .value_name("MODULE|INTERFACE|PACKAGE")
                .help("Add module, interface, package which should not be renamed")
                .action(ArgAction::Append)
                .num_args(1),
        )
        .arg(
            Arg::new("exclude")
                .long("exclude")
                .value_name("MODULE|INTERFACE|PACKAGE")
                .help("Do not include module, interface, package in the pickled file list")
                .action(ArgAction::Append)
                .num_args(1),
        )
        .arg(
            Arg::new("v")
                .short('v')
                .action(ArgAction::Count)
                .num_args(0)
                .help("Sets the level of verbosity"),
        )
        .arg(
            Arg::new("prefix")
                .short('p')
                .long("prefix")
                .value_name("PREFIX")
                .help("Prepend a name to all global names")
                .num_args(1),
        )
        .arg(
            Arg::new("def")
                .short('D')
                .value_name("DEFINE")
                .help("Define a preprocesor macro")
                .action(ArgAction::Append)
                .num_args(1),
        )
        .arg(
            Arg::new("suffix")
                .short('s')
                .long("suffix")
                .value_name("SUFFIX")
                .help("Append a name to all global names")
                .num_args(1),
        )
        .arg(
            Arg::new("preproc")
                .short('E')
                .help("Write preprocessed input files to stdout")
                .num_args(0)
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("file_list")
                .short('f')
                .value_name("LIST")
                .help("Gather files from a manifest")
                .action(ArgAction::Append)
                .num_args(1),
        )
        .arg(
            Arg::new("flist")
                .long("flist")
                .value_name("FILE_LIST")
                .help("Gather files from a file list")
                .num_args(1)
                .action(ArgAction::Append),
        )
        .arg(
            Arg::new("strip_comments")
                .long("strip-comments")
                .help("Strip comments from the output")
                .num_args(0)
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("INPUT")
                .help("The input files to compile")
                .action(ArgAction::Append)
                .num_args(1..),
        )
        .arg(
            Arg::new("docdir")
                .short('d')
                .long("doc")
                .value_name("OUTDIR")
                .help("Generate documentation in a directory")
                .num_args(1),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .value_name("FILE")
                .help("Write output to file")
                .num_args(1),
        )
        .arg(
            Arg::new("library_file")
                .long("library-file")
                .help("File to search for SystemVerilog modules")
                .value_name("FILE")
                .action(ArgAction::Append)
                .num_args(1),
        )
        .arg(
            Arg::new("library_dir")
                .short('y')
                .long("library-dir")
                .help("Directory to search for SystemVerilog modules")
                .value_name("DIR")
                .action(ArgAction::Append)
                .num_args(1),
        )
        .arg(
            Arg::new("manifest")
                .long("manifest")
                .value_name("FILE")
                .help("Output a JSON-encoded source information manifest to FILE")
                .num_args(1),
        )
        .arg(
            Arg::new("top_module")
                .long("top")
                .value_name("TOP_MODULE")
                .help("Top module, strips all unneeded files. May be incompatible with `--propagate_defines`.")
                .num_args(1),
        )
        .arg(
            Arg::new("graph_file")
                .long("graph_file")
                .value_name("FILE")
                .help("Output a DOT graph of the parsed modules")
                .num_args(1),
        )
        .arg(
            Arg::new("ignore_unparseable")
                .short('i')
                .help("Ignore files that cannot be parsed")
                .num_args(0)
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("keep_defines")
                .long("keep_defines")
                .help("Prevents removal of `define statements.")
                .num_args(0)
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("propagate_defines")
                .long("propagate_defines")
                .help("Propagate defines from first files to the following files. Enables sequential.")
                .num_args(0)
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("sequential")
                .short('q')
                .long("sequential")
                .help("Enforce sequential processing of files. Slows down performance, but can avoid STACK_OVERFLOW.")
                .num_args(0)
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("keep_timeunits")
                .long("keep_timeunits")
                .help("Keeps timeunits declared throughout the design, may result in bad pickles.")
                .num_args(0)
                .action(ArgAction::SetTrue),
        )
        .get_matches();

    let logger_level = matches.get_count("v");

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
    let defines: HashMap<_, _> = match matches.get_many::<String>("def") {
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
        .get_many::<String>("inc")
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
    for dir in matches
        .get_many::<String>("library_dir")
        .into_iter()
        .flatten()
    {
        for entry in std::fs::read_dir(dir).unwrap_or_else(|e| {
            eprintln!("error accessing library directory `{}`: {}", dir, e);
            process::exit(1)
        }) {
            let dir = entry.unwrap();
            library_paths.push(dir.path());
        }
    }

    if let Some(library_names) = matches.get_many::<String>("library_file") {
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

    for path in matches
        .get_many::<String>("file_list")
        .into_iter()
        .flatten()
    {
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

    let mut all_files = Vec::<String>::new();

    if let Some(file_names) = matches.get_many::<String>("INPUT") {
        all_files.extend(file_names.map(|x| x.to_string()).collect::<Vec<_>>());
    }

    for path in matches.get_many::<String>("flist").into_iter().flatten() {
        let file = File::open(path).unwrap_or_else(|e| {
            eprintln!("error opening `{}`: {}", path, e);
            process::exit(1)
        });
        let lines = BufReader::new(file).lines();

        let proper_lines: Vec<String> = lines.filter_map(|x| x.ok()).collect();

        all_files.extend(proper_lines);
    }

    let mut stdin_incdirs = include_dirs;
    let mut stdin_defines = HashMap::<String, Option<String>>::new();

    let stdin_files = all_files
        .into_iter()
        .map(String::from)
        .filter_map(|file_str| {
            let split_str = file_str.splitn(3, '+').collect::<Vec<_>>();
            if split_str.len() > 1 {
                match split_str[1] {
                    "define" => {
                        let def_str = split_str[2];
                        match def_str.split_once('=') {
                            Some((def, val)) => {
                                stdin_defines.insert(def.to_string(), Some(val.to_string()));
                            }
                            None => {
                                stdin_defines.insert(def_str.to_string(), None);
                            }
                        }
                        None
                    }
                    "incdir" => {
                        stdin_incdirs.push(split_str[2].to_string());
                        None
                    }
                    _ => {
                        eprintln!("Unimplemented argument, ignoring for now: {}", split_str[1]);
                        None
                    }
                }
            } else {
                Some(file_str)
            }
        })
        .collect();

    stdin_defines.extend(defines);

    file_list.push(FileBundle {
        include_dirs: stdin_incdirs.clone(),
        export_incdirs: HashMap::new(),
        defines: stdin_defines.clone(),
        files: stdin_files,
    });

    let (mut exclude_rename, mut exclude) = (HashSet::new(), HashSet::new());
    exclude_rename.extend(
        matches
            .get_many::<String>("exclude_rename")
            .into_iter()
            .flatten(),
    );
    exclude.extend(matches.get_many::<String>("exclude").into_iter().flatten());

    let strip_comments = matches.get_flag("strip_comments");

    let mut pickle = Pickle::new();
    pickle.add_files(
        &file_list,
        strip_comments,
        matches.get_flag("ignore_unparseable"),
    )?;

    pickle.add_libs(library_bundle)?;

    let out = match matches.get_one::<String>("output") {
        Some(file) => {
            info!("Setting output to `{}`", file);
            let path = Path::new(file);
            Box::new(BufWriter::new(File::create(path).unwrap_or_else(|e| {
                eprintln!("could not create `{}`: {}", file, e);
                process::exit(1);
            }))) as Box<dyn Write>
        }
        None => Box::new(io::stdout()) as Box<dyn Write>,
    };

    // Just preprocess.
    if matches.get_flag("preproc") {
        return pickle.just_preprocess(out);
    }

    info!("Finished reading {} source files.", pickle.all_files.len());

    // Emit documentation if requested.
    if let Some(dir) = matches.get_one::<String>("docdir") {
        info!("Generating documentation in `{}`", dir);
        return pickle.build_doc(dir);
    }

    pickle.build_graph()?;

    if let Some(top) = matches.get_one::<String>("top_module") {
        pickle.prune_graph(top)?;
    }

    if !matches.get_flag("keep_defines") {
        pickle.remove_macros()?;
    }

    pickle.rename(
        matches.get_one::<String>("prefix"),
        matches.get_one::<String>("suffix"),
        exclude_rename,
    )?;

    // TODO: add transforms
    //   - replace interfaces
    //   - replace impossible parameters
    //   - replace types (and uniquify/elaborate)

    pickle.get_pickle(out, exclude)?;

    if let Some(graph_file) = matches.get_one::<String>("graph_file") {
        let graph_path = Path::new(graph_file);
        let graph_out =
            Box::new(BufWriter::new(File::create(&graph_path).unwrap())) as Box<dyn Write>;

        pickle.get_dot(graph_out)?;
    }

    // if the user requested a manifest we need to compute the information and output it in json
    // form
    if let Some(manifest_file) = matches.get_one::<String>("manifest") {
        let manifest_path = Path::new(manifest_file);
        let manifest_out =
            Box::new(BufWriter::new(File::create(&manifest_path).unwrap())) as Box<dyn Write>;

        pickle.get_manifest(manifest_out, file_list, stdin_incdirs, stdin_defines)?;
    }

    Ok(())
}
