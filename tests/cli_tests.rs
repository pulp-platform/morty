// Copyright 2022 PULP-platform

// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use assert_cmd::prelude::*; // Add methods on commands
use predicates::prelude::*; // Used for writing assertions
use std::fs;
use std::path::Path;
use std::process::Command; // Run programs
                           // use std::fs::File;
use std::io::{self, Write};
use tempfile;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_help_print_check() -> Result<()> {
        let mut cmd = Command::cargo_bin("morty")?;
        cmd.arg("-h");
        cmd.assert().success().stdout(predicate::str::contains(
            "A SystemVerilog source file pickler.\n",
        ));

        Ok(())
    }

    #[test]
    fn test_doc_generation() -> Result<()> {
        let mut cmd = Command::cargo_bin("morty")?;
        cmd.arg("test/doc.sv").arg("--doc=test/doc");

        cmd.assert().success();

        assert!(Path::new("test/doc/index.html").exists());

        assert!(std::fs::read_to_string("test/doc/index.html")
            .unwrap()
            .contains("First-in First-out Queue"));

        Ok(())
    }

    #[test]
    fn test_package_2() -> Result<()> {
        let mut cmd = Command::cargo_bin("morty")?;

        cmd.arg("test/package.sv").arg("test/package_import_2.sv");

        cmd.assert().success();

        Ok(())
    }

    #[test]
    fn test_package() -> Result<()> {
        let mut cmd = Command::cargo_bin("morty")?;
        cmd.arg("test/package.sv")
            .arg("test/package_import.sv")
            .arg("-I")
            .arg("test");

        cmd.assert().success();

        Ok(())
    }

    #[test]
    fn test_import() -> Result<()> {
        let mut cmd = Command::cargo_bin("morty")?;
        cmd.arg("test/import.sv");

        cmd.assert().success();
        Ok(())
    }

    #[test]
    fn test_preprocess() -> Result<()> {
        let mut cmd = Command::cargo_bin("morty")?;
        cmd.arg("test/preprocess.sv")
            .arg("-I")
            .arg("test")
            .arg("-E");

        cmd.assert().success();

        Ok(())
    }

    #[test]
    fn test_infer_dot_star() -> Result<(), Box<dyn std::error::Error>> {
        let expected_output = fs::read_to_string("test/infer_dot_star/expected/expected.sv")?;
        let mut cmd = Command::cargo_bin("morty")?;
        cmd.arg("--infer_dot_star")
            .arg(Path::new("test/infer_dot_star/top.sv").as_os_str())
            .arg(Path::new("test/infer_dot_star/submodule.sv").as_os_str());
        let binding = cmd.assert().success();
        // we have to do it this complex such that windows tests are passing
        // windows has a different output format and injects \r into the output
        let output = &binding.get_output().stdout;
        let output_str = String::from_utf8(output.clone()).unwrap();
        let expected_output_stripped = expected_output.replace(&['\r'][..], "");
        let output_str_stripped = output_str.replace(&['\r'][..], "");
        let compare_fn = predicate::str::contains(expected_output_stripped);
        assert_eq!(compare_fn.eval(&output_str_stripped), true);
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn test_cva6() -> Result<(), Box<dyn std::error::Error>> {
        // debug with -- --nocapture
        let tempdir = tempfile::tempdir()?;
        println!("tempdir: {:?}", tempdir.path());
        let clone_output = Command::new("sh")
          .current_dir(&tempdir)
          .arg("-c")
          .arg("git clone https://github.com/pulp-platform/cva6.git --branch bender-changes && cd cva6 && git submodule update --init --recursive && echo hubus")
          .output()
          .expect("failed to execute process");
        io::stdout().write_all(&clone_output.stdout).unwrap();
        io::stderr().write_all(&clone_output.stderr).unwrap();
        println!("status: {}", clone_output.status);
        assert!(clone_output.status.success());
        let bender_output = Command::new("sh")
            .current_dir(&tempdir)
            .arg("-c")
            .arg("cd cva6 && bender sources -f -t cv64a6_imafdc_sv39 > sources.json")
            .output()
            .expect("failed to execute process");
        io::stdout().write_all(&bender_output.stdout).unwrap();
        io::stderr().write_all(&bender_output.stderr).unwrap();
        println!("status2: {}", bender_output.status);
        assert!(bender_output.status.success());

        let mut cmd = Command::cargo_bin("morty")?;
        cmd.arg("-f")
            .arg(
                Path::new(&tempdir.path())
                    .join("cva6/sources.json")
                    .as_os_str(),
            )
            .arg("-o")
            //.arg(Path::new(&tempdir.path()).join("output.sv").as_os_str())
            .arg(Path::new("output.sv").as_os_str())
            .arg("--top")
            .arg("cva6");
        let binding = cmd.assert().success();
        let output = &binding.get_output().stdout;
        let output_str = String::from_utf8(output.clone()).unwrap();
        println!("output: {}", output_str);

        Ok(())
    }
}
