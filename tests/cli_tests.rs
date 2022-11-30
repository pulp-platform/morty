// Copyright 2022 PULP-platform

// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use assert_cmd::prelude::*; // Add methods on commands
use predicates::prelude::*; // Used for writing assertions
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
        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Print version information\n"));

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
}
