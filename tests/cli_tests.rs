// Copyright 2022 PULP-platform

// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use assert_cmd::prelude::*; // Add methods on commands
use predicates::prelude::*; // Used for writing assertions
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
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Print version"));

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

    #[cfg(target_os = "linux")]
    #[test]
    fn test_cva6() -> Result<(), Box<dyn std::error::Error>> {
        // debug with -- --nocapture
        let tempdir = tempfile::tempdir()?;
        println!("tempdir: {:?}", tempdir.path());
        let clone_output = Command::new("sh")
          .current_dir(&tempdir)
          .arg("-c")
          .arg("git clone https://github.com/pulp-platform/cva6.git --branch bender-changes && cd cva6 && git submodule update --init --recursive")
          .output()
          .expect("failed to execute process");
        io::stdout().write_all(&clone_output.stdout).unwrap();
        io::stderr().write_all(&clone_output.stderr).unwrap();
        assert!(clone_output.status.success());
        let bender_output = Command::new("sh")
            .current_dir(&tempdir)
            .arg("-c")
            .arg("cd cva6 && bender sources -f -t cv64a6_imafdc_sv39 > sources.json")
            .output()
            .expect("failed to execute process");
        io::stdout().write_all(&bender_output.stdout).unwrap();
        io::stderr().write_all(&bender_output.stderr).unwrap();
        assert!(bender_output.status.success());

        let mut cmd = Command::cargo_bin("morty")?;
        cmd.arg("-f")
            .arg(
                Path::new(&tempdir.path())
                    .join("cva6/sources.json")
                    .as_os_str(),
            )
            .arg("-o")
            .arg(
                Path::new(&tempdir.path())
                    .join("cva6/output.sv")
                    .as_os_str(),
            );
        // TODO: add --top test when it is working
        //.arg("--top")
        //.arg("cva6");
        let binding = cmd.assert().success();
        let output = &binding.get_output().stdout;
        let output_str = String::from_utf8(output.clone()).unwrap();
        println!("output: {}", output_str);

        let slang_output = Command::new("sh")
            .current_dir(&tempdir)
            .arg("-c")
            .arg("cd cva6 && echo $PATH && slang output.sv --top cva6 -Wrange-width-oob")
            .output()
            .expect("failed to execute process");
        io::stdout().write_all(&slang_output.stdout).unwrap();
        io::stderr().write_all(&slang_output.stderr).unwrap();
        assert!(slang_output.status.success());
        Ok(())
    }
}
