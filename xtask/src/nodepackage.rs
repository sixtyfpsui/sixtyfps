/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
use anyhow::Context;
use xshell::{cmd, cp, pushd, read_file, rm_rf, write_file};

pub fn generate() -> Result<(), Box<dyn std::error::Error>> {
    let root = super::root_dir();
    let node_dir = root.join("api").join("sixtyfps-node");

    let cargo_toml_path = node_dir.join("native").join("Cargo.toml");

    println!("Removing relative paths from {}", cargo_toml_path.to_string_lossy());

    let toml_source =
        read_file(cargo_toml_path.clone()).context("Failed to read Node Cargo.toml")?;

    let mut toml: toml_edit::Document = toml_source.parse().context("Error parsing Cargo.toml")?;

    // Remove all `path = ` entries from dependencies
    for dep_key in ["dependencies", "build-dependencies"].iter() {
        let dep_table = match toml[dep_key].as_table_mut() {
            Some(table) => table,
            _ => continue,
        };
        let deps: Vec<_> = dep_table.iter().map(|(name, _)| name.to_string()).collect();
        deps.iter().for_each(|name| {
            dep_table[name].as_inline_table_mut().map(|dep_config| dep_config.remove("path"));
        });
    }

    let edited_toml = toml.to_string();

    write_file(cargo_toml_path.clone(), edited_toml).context("Error writing Cargo.toml")?;

    println!("Putting LICENSE.md in place for the source package");

    cp(root.join("LICENSE.md"), node_dir.join("LICENSE.md"))
        .context("Error copying LICENSE.md into the node dir for packaging")?;

    let package_json_source =
        read_file(node_dir.join("package.json")).context("Error reading package.json")?;

    let package_json: serde_json::Value = serde_json::from_str(&package_json_source)?;

    let file_name = node_dir.join(format!(
        "{}-{}.tgz",
        package_json["name"].as_str().unwrap(),
        package_json["version"].as_str().unwrap()
    ));

    rm_rf(file_name.clone()).context("Error deleting old archive")?;

    println!("Running npm package to create the tarball");

    {
        let _p = pushd(node_dir.clone())
            .context(format!("Error changing to node directory {}", node_dir.to_string_lossy()))?;

        cmd!("npm pack").run()?;
    }

    println!("Reverting Cargo.toml");

    write_file(cargo_toml_path, toml_source).context("Error writing Cargo.toml")?;

    rm_rf(node_dir.join("LICENSE.md")).context("Error deleting LICENSE.md copy")?;

    println!("Source package created and located in {}", file_name.to_string_lossy());

    Ok(())
}
