/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
use anyhow::Context;
use anyhow::Result;
use lazy_static::lazy_static;
use std::str::FromStr;
use std::{path::Path, path::PathBuf};
use structopt::StructOpt;

#[derive(Copy, Clone, Debug)]
struct LicenseTagStyle {
    tag_start: &'static str,
    line_prefix: &'static str,
    line_indentation: &'static str,
    tag_end: &'static str,
}

impl LicenseTagStyle {
    fn c_style_comment_style() -> Self {
        Self {
            tag_start: "LICENSE BEGIN\n",
            line_prefix: "",
            line_indentation: "    ",
            tag_end: "LICENSE END */\n",
        }
    }

    fn shell_comment_style() -> Self {
        Self {
            tag_start: "# LICENSE BEGIN\n",
            line_prefix: "#",
            line_indentation: " ",
            tag_end: "# LICENSE END\n",
        }
    }

    fn rst_comment_style() -> Self {
        Self {
            tag_start: ".. LICENSE BEGIN\n",
            line_prefix: "..",
            line_indentation: "   ",
            tag_end: ".. LICENSE END\n",
        }
    }
}

struct SourceFileWithTags<'a> {
    source: &'a str,
    tag_style: &'a LicenseTagStyle,
    tag_location: Option<std::ops::Range<usize>>,
}

impl<'a> SourceFileWithTags<'a> {
    fn new(source: &'a str, style: &'a LicenseTagStyle) -> Self {
        let location = match source.find(style.tag_start) {
            Some(start) => {
                let end = source[start..]
                    .find(style.tag_end)
                    .map(|idx| start + idx + style.tag_end.len())
                    .unwrap_or_default();
                if end > start {
                    Some(std::ops::Range { start, end })
                } else {
                    None
                }
            }
            None => None,
        };

        Self { source, tag_style: style, tag_location: location }
    }

    fn has_tag(&self) -> bool {
        self.tag_location.is_some()
    }

    fn found_tag(&self) -> &'a str {
        let tag_loc = match &self.tag_location {
            Some(loc) => loc,
            None => return "",
        };

        &self.source[tag_loc.start..tag_loc.end]
    }

    fn tag_matches(&self, expected_tag: &LicenseHeader) -> bool {
        let tag_loc = match &self.tag_location {
            Some(loc) => loc,
            None => return false,
        };

        let expected_tag_str = expected_tag.to_string(self.tag_style);
        let found_tag = &self.source[tag_loc.start..tag_loc.end];
        expected_tag_str == found_tag
    }

    fn replace_tag(&self, replacement: &LicenseHeader) -> String {
        let new_header = replacement.to_string(self.tag_style);

        match &self.tag_location {
            Some(loc) => {
                self.source[0..loc.start].to_string() + &new_header + &self.source[loc.end..]
            }
            None => new_header + self.source,
        }
    }
}

#[test]
fn test_license_tag_c_style() {
    let style = LicenseTagStyle::c_style_comment_style();
    {
        let test_source = SourceFileWithTags::new(
            r#"/* LICENSE BEGIN
        foobar
        LICENSE END */

blah"#,
            &style,
        );
        assert_eq!(
            test_source.replace_tag(&LicenseHeader(&["TEST_LICENSE"])),
            r#"/* LICENSE BEGIN
    TEST_LICENSE
LICENSE END */

blah"#
                .to_string()
        );
    }
    {
        let test_source = SourceFileWithTags::new(r#"blah"#, &style);
        assert_eq!(
            test_source.replace_tag(&LicenseHeader(&["TEST_LICENSE"])),
            r#"/* LICENSE BEGIN
    TEST_LICENSE
LICENSE END */
blah"#
                .to_string()
        );
    }
}

#[test]
fn test_license_tag_hash() {
    let style = LicenseTagStyle::shell_comment_style();
    {
        let test_source = SourceFileWithTags::new(
            r#"# LICENSE BEGIN
# Some Text
# LICENSE END

blah"#,
            &style,
        );
        assert_eq!(
            test_source.replace_tag(&LicenseHeader(&["TEST_LICENSE"])),
            r#"# LICENSE BEGIN
# TEST_LICENSE
# LICENSE END

blah"#
                .to_string()
        );
    }
    {
        let test_source = SourceFileWithTags::new(r#"blah"#, &style);
        assert_eq!(
            test_source.replace_tag(&LicenseHeader(&["TEST_LICENSE"])),
            r#"# LICENSE BEGIN
# TEST_LICENSE
# LICENSE END
blah"#
                .to_string()
        );
    }
}

#[derive(Copy, Clone, Debug)]
enum LicenseLocation {
    Tag(LicenseTagStyle),
    Crate,
    NoLicense,
}

lazy_static! {
    // cspell:disable
    static ref LICENSE_LOCATION_FOR_FILE: Vec<(regex::Regex, LicenseLocation)> = [
        ("^helper_crates/const-field-offset/src/lib.rs$", LicenseLocation::NoLicense), // Upstream fork
        ("^helper_crates/const-field-offset/Cargo.toml$", LicenseLocation::NoLicense), // Upstream fork
        (".+webpack\\..+\\.js$", LicenseLocation::NoLicense),
        (".+\\.rs$", LicenseLocation::Tag(LicenseTagStyle::c_style_comment_style())),
        (".+\\.js$", LicenseLocation::Tag(LicenseTagStyle::c_style_comment_style())),
        (".+\\.ts$", LicenseLocation::Tag(LicenseTagStyle::c_style_comment_style())),
        (".+\\.mjs$", LicenseLocation::Tag(LicenseTagStyle::c_style_comment_style())),
        (".+\\.h$", LicenseLocation::Tag(LicenseTagStyle::c_style_comment_style())),
        (".+\\.cpp$", LicenseLocation::Tag(LicenseTagStyle::c_style_comment_style())),
        (".+\\.md$", LicenseLocation::NoLicense),
        (".+\\.png$", LicenseLocation::NoLicense),
        (".+\\.jpg$", LicenseLocation::NoLicense),
        (".+\\.svg$", LicenseLocation::NoLicense),
        (".+\\.json$", LicenseLocation::NoLicense),
        (".+\\.html$", LicenseLocation::NoLicense),
        (".+\\.xml$", LicenseLocation::NoLicense),
        (".+\\.ttf$", LicenseLocation::NoLicense),
        (".+\\.ui$", LicenseLocation::NoLicense),
        (".+\\.rst$", LicenseLocation::Tag(LicenseTagStyle::rst_comment_style())),
        (".+\\.yaml$", LicenseLocation::Tag(LicenseTagStyle::shell_comment_style())),
        (".*\\.gitignore$", LicenseLocation::NoLicense),
        (".*\\.gitattributes$", LicenseLocation::NoLicense),
        (".+\\.sublime-settings$", LicenseLocation::NoLicense),
        (".+\\.sublime-commands$", LicenseLocation::NoLicense),
        (".+\\.sublime-syntax$", LicenseLocation::Tag(LicenseTagStyle::shell_comment_style())),
        (".+\\.tmPreferences$", LicenseLocation::NoLicense),
        ("\\.clang-format$", LicenseLocation::NoLicense),
        (".+Dockerfile.*$", LicenseLocation::Tag(LicenseTagStyle::shell_comment_style())),
        ("^api/sixtyfps-cpp/docs/Pipfile$", LicenseLocation::NoLicense),
        ("^api/sixtyfps-cpp/docs/conf.py$", LicenseLocation::NoLicense),
        ("^api/sixtyfps-cpp/docs/_static/.+$", LicenseLocation::NoLicense),
        ("^api/sixtyfps-cpp/docs/_templates/.+$", LicenseLocation::NoLicense),
        ("^docs/tutorial/theme/.+$", LicenseLocation::NoLicense),
        ("\\.cargo/config$", LicenseLocation::Tag(LicenseTagStyle::shell_comment_style())),
        ("^Cargo.toml$", LicenseLocation::NoLicense),
        (".+Cargo.toml$", LicenseLocation::Crate),
        (".+\\.toml$", LicenseLocation::NoLicense),
        (".*CMakeLists.txt$", LicenseLocation::Tag(LicenseTagStyle::shell_comment_style())),
        (".*\\.cmake$", LicenseLocation::Tag(LicenseTagStyle::shell_comment_style())),
        (".+\\.cmake.in$", LicenseLocation::Tag(LicenseTagStyle::shell_comment_style())),
        (".+\\.sh$", LicenseLocation::Tag(LicenseTagStyle::shell_comment_style())),
        (".+\\.60$", LicenseLocation::Tag(LicenseTagStyle::c_style_comment_style())),
        (".*README$", LicenseLocation::NoLicense),
        (".*README\\.txt$", LicenseLocation::NoLicense),
        ("LICENSE\\..*", LicenseLocation::NoLicense),
        ("LICENSE-DejaVu", LicenseLocation::NoLicense),
        ("^examples/slide_puzzle/plaster-font/OFL.txt$", LicenseLocation::NoLicense),
        ("^examples/printerdemo/ui/fonts/LICENSE_OFL.txt$", LicenseLocation::NoLicense),
    ]
    .iter()
    .map(|(re, ty)| (regex::Regex::new(re).unwrap(), *ty))
    .collect();
    // cspell:enable
}

pub struct LicenseHeader<'a>(&'a [&'a str]);

impl<'a> LicenseHeader<'a> {
    fn to_string(&self, style: &LicenseTagStyle) -> String {
        let mut result = style.tag_start.to_string();
        for line in self.0 {
            result += style.line_prefix;
            if !line.is_empty() {
                result += style.line_indentation;
            }
            result += line;
            result += "\n";
        }
        result += style.tag_end;
        result
    }
}

const EXPECTED_SPDX_EXPRESSION: &str = "GPL-3.0-only";
const EXPECTED_SPDX_ID: &str = "SPDX-License-Identifier: GPL-3.0-only";

const EXPECTED_HEADER: LicenseHeader<'static> = LicenseHeader(&[
    EXPECTED_SPDX_ID,
    "This file is also available under commercial licensing terms.",
    "Please contact info@sixtyfps.io for more information.",
]);

const EXPECTED_HOMEPAGE: &str = "https://sixtyfps.io";
const EXPECTED_REPOSITORY: &str = "https://github.com/sixtyfpsui/sixtyfps";

fn collect_files() -> Result<Vec<PathBuf>> {
    let root = super::root_dir();
    let ls_files_output = super::run_command(
        "git",
        &["ls-files", "-z"],
        std::iter::empty::<(std::ffi::OsString, std::ffi::OsString)>(),
    )?
    .stdout;
    let mut files = Vec::new();
    for path in ls_files_output.split(|ch| *ch == 0) {
        if path.is_empty() {
            continue;
        }
        let path = PathBuf::from_str(
            std::str::from_utf8(path)
                .context("Error decoding git ls-files command output as utf-8")?,
        )
        .context("Failed to decide path output in git ls-files")?;

        if !path.is_dir() {
            files.push(root.join(path));
        }
    }
    Ok(files)
}

enum CargoDependency {
    Simple { _version: String },
    Full { path: String, version: String },
}

impl CargoDependency {
    fn new(encoded_value: &toml_edit::Value) -> Option<Self> {
        match encoded_value {
            toml_edit::Value::String(s) => {
                return Some(Self::Simple { _version: s.value().clone() })
            }
            toml_edit::Value::Float(_) => None,
            toml_edit::Value::DateTime(_) => None,
            toml_edit::Value::Boolean(_) => None,
            toml_edit::Value::Array(_) => None,
            toml_edit::Value::Integer(_) => None,
            toml_edit::Value::InlineTable(table) => {
                if let (Some(path), Some(version)) = (table.get("path"), table.get("version")) {
                    Some(Self::Full {
                        path: path.as_str().unwrap_or_default().to_owned(),
                        version: version.as_str().unwrap_or_default().to_owned(),
                    })
                } else {
                    None
                }
            }
        }
    }
}

struct CargoToml {
    doc: toml_edit::Document,
    edited: bool,
}

impl CargoToml {
    fn new(path: &Path) -> Result<Self> {
        let source = &std::fs::read_to_string(path).context("Error reading file")?;
        Ok(Self { doc: source.parse()?, edited: false })
    }

    fn is_workspace(&self) -> bool {
        self.doc.as_table().get("workspace").is_some()
    }

    fn package(&self) -> Result<&toml_edit::Table> {
        self.doc
            .as_table()
            .get("package")
            .map(|p| p.as_table())
            .flatten()
            .ok_or_else(|| anyhow::anyhow!("Invalid Cargo.toml -- cannot find package section"))
    }

    fn dependencies(&self, dep_type: &str) -> Vec<(String, CargoDependency)> {
        match self.doc.as_table().get(dep_type).map(|d| d.as_table()).flatten() {
            Some(dep_table) => dep_table
                .iter()
                .filter_map(|(name, entry)| {
                    entry.as_value().and_then(|entry| {
                        CargoDependency::new(entry).map(|entry| (name.to_owned(), entry))
                    })
                })
                .collect(),
            None => Vec::new(),
        }
    }

    fn check_and_fix_package_string_field<'a>(
        &mut self,
        fix_it: bool,
        field: &'a str,
        expected_str: &'a str,
    ) -> Result<()> {
        match self.package()?.get(field) {
            Some(field_value) => match field_value.as_str() {
                Some(text) => {
                    if text != expected_str {
                        if fix_it {
                            self.doc["package"][field] = toml_edit::value(expected_str);
                            self.edited = true;
                        } else {
                            return Err(anyhow::anyhow!(
                                "Incorrect {}. Found {} expected {}",
                                field,
                                text,
                                expected_str
                            ));
                        }
                    }
                }
                None => return Err(anyhow::anyhow!("{} field is not a string", field)),
            },
            None => {
                if fix_it {
                    self.doc["package"][field] = toml_edit::value(expected_str);
                    self.edited = true;
                } else {
                    return Err(anyhow::anyhow!("Missing {} field", field));
                }
            }
        };
        Ok(())
    }

    fn published(&self) -> Result<bool> {
        Ok(self.package()?.get("publish").map(|v| v.as_bool().unwrap()).unwrap_or(true))
    }

    fn save_if_changed(&self, path: &Path) -> Result<()> {
        if self.edited {
            std::fs::write(path, &self.doc.to_string()).context("Error writing new Cargo.toml")
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, StructOpt)]
pub struct LicenseHeaderCheck {
    #[structopt(long)]
    fix_it: bool,

    #[structopt(long)]
    show_all: bool,

    #[structopt(long)]
    verbose: bool,
}

impl LicenseHeaderCheck {
    pub fn check_license_headers(&self) -> Result<(), Box<dyn std::error::Error>> {
        let mut seen_errors = false;
        for path in &collect_files()? {
            let result = self
                .check_file(path.as_path())
                .with_context(|| format!("checking {}", &path.to_string_lossy()));

            if result.is_err() {
                seen_errors = true;
                if self.show_all {
                    eprintln!("Error: {:?}", result);
                } else {
                    return result.map_err(|e| e.into());
                }
            }
        }
        if seen_errors {
            Err(anyhow::anyhow!("Encountered one or multiple errors. See above for details.")
                .into())
        } else {
            println!("All files are ok.");
            Ok(())
        }
    }

    fn check_file_tags(&self, path: &Path, style: &LicenseTagStyle) -> Result<()> {
        let source = &std::fs::read_to_string(path).context("Error reading file")?;

        let source = SourceFileWithTags::new(source, style);

        if !source.has_tag() {
            if self.fix_it {
                eprintln!(
                    "Fixing up {} as instructed. It's missing a license header.",
                    path.to_str().unwrap()
                );
                let source = source.replace_tag(&EXPECTED_HEADER);
                std::fs::write(path, &source).context("Error writing source")
            } else {
                Err(anyhow::anyhow!("Missing tag"))
            }
        } else if source.tag_matches(&EXPECTED_HEADER) {
            Ok(())
        } else if self.fix_it {
            let source = source.replace_tag(&EXPECTED_HEADER);
            std::fs::write(path, &source).context("Error writing new source")
        } else {
            Err(anyhow::anyhow!(
                "unexpected header.\nexpected: {}\nfound: {}",
                EXPECTED_HEADER.to_string(style),
                source.found_tag()
            ))
        }
    }

    fn check_cargo_toml(&self, path: &Path) -> Result<()> {
        let mut doc = CargoToml::new(path)?;

        if doc.is_workspace() {
            return Ok(());
        }

        doc.check_and_fix_package_string_field(self.fix_it, "license", EXPECTED_SPDX_EXPRESSION)?;

        if !doc.published()? {
            // Skip further tests for package that are not published
            return Ok(());
        }

        doc.check_and_fix_package_string_field(self.fix_it, "homepage", EXPECTED_HOMEPAGE)?;
        doc.check_and_fix_package_string_field(self.fix_it, "repository", EXPECTED_REPOSITORY)?;

        if doc.package()?["description"].is_none() {
            return Err(anyhow::anyhow!("Missing description field"));
        }

        // Check that version of sixtyfps- dependencies are matching this version
        let expected_version = format!(
            "={}",
            doc.package()?.get("version").unwrap().as_value().unwrap().as_str().unwrap()
        );

        for (dep_name, dep) in doc
            .dependencies("dependencies")
            .iter()
            .chain(doc.dependencies("build-dependencies").iter())
        {
            if dep_name.starts_with("sixtyfps") {
                match dep {
                    CargoDependency::Simple { .. } => {
                        return Err(anyhow::anyhow!(
                            "sixtyfps package '{}' outside of the repository?",
                            dep_name
                        ))
                    }
                    CargoDependency::Full { path, version } => {
                        if path.is_empty() {
                            return Err(anyhow::anyhow!(
                                "sixtyfps package '{}' outside of the repository?",
                                dep_name
                            ));
                        }
                        if version != &expected_version {
                            return Err(anyhow::anyhow!(
                                "Version \"{}\" must be specified for dependency {}",
                                expected_version,
                                dep_name
                            ));
                        }
                    }
                }
            }
        }

        if self.fix_it {
            doc.save_if_changed(path)?;
        }

        Ok(())
    }

    fn check_file(&self, path: &Path) -> Result<()> {
        let repo_relative_path = path.strip_prefix(super::root_dir())?;
        let path_str = repo_relative_path.to_str().unwrap();
        let location = LICENSE_LOCATION_FOR_FILE
            .iter()
            .find_map(|(regex, style)| if regex.is_match(path_str) { Some(style) } else { None })
            .with_context(|| "Cannot determine the expected license header style. Please the license checking xtask.")?;

        match location {
            LicenseLocation::Tag(tag_style) => self.check_file_tags(path, tag_style),
            LicenseLocation::Crate => self.check_cargo_toml(path),
            LicenseLocation::NoLicense => {
                if self.verbose {
                    println!("Skipping {} as configured", path_str);
                }
                Ok(())
            }
        }
    }
}
