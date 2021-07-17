/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

#[cfg(test)]
fn do_test(snippet: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut compiler = sixtyfps_interpreter::ComponentCompiler::default();
    let component =
        spin_on::spin_on(compiler.build_from_source(snippet.into(), Default::default()));

    #[cfg(feature = "display-diagnostics")]
    sixtyfps_interpreter::print_diagnostics(&compiler.diagnostics());

    for d in compiler.diagnostics() {
        if d.level() == sixtyfps_interpreter::DiagnosticLevel::Error {
            return Err(d.message().to_owned().into());
        }
    }

    if component.is_none() {
        Err(String::from("Failure").into())
    } else {
        Ok(())
    }
}

include!(env!("TEST_FUNCTIONS"));

fn main() {
    println!("Nothing to see here, please run me through cargo test :)");
}
