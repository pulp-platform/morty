// Copyright 2022 PULP-platform

// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use assert_cmd::prelude::*; // Add methods on commands
use predicates::prelude::*; // Used for writing assertions
use std::fs;
use std::path::Path;
use std::process::Command; // Run programs
                           // use std::fs::File;

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
}
