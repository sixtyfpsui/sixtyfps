#!/bin/bash -e
# This file is part of the SixtyFPS Project -- https://sixtyfps.io
# Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
# Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>
#
# LICENSE BEGIN
# SPDX-License-Identifier: GPL-3.0-only
# This file is also available under commercial licensing terms.
# Please contact info@sixtyfps.io for more information.
# LICENSE END
cargo publish --manifest-path sixtyfps_runtime/corelib_macros/Cargo.toml
cargo publish --manifest-path sixtyfps_compiler/Cargo.toml
cargo publish --manifest-path sixtyfps_runtime/corelib/Cargo.toml
cargo publish --manifest-path api/sixtyfps-rs/sixtyfps-macros/Cargo.toml
cargo publish --manifest-path sixtyfps_runtime/rendering_backends/gl/Cargo.toml
cargo publish --manifest-path api/sixtyfps-rs/sixtyfps-build/Cargo.toml
cargo publish --manifest-path sixtyfps_runtime/rendering_backends/qt/Cargo.toml
sleep 30
cargo publish --manifest-path sixtyfps_runtime/rendering_backends/default/Cargo.toml
sleep 30
cargo publish --manifest-path sixtyfps_runtime/interpreter/Cargo.toml
cargo publish --manifest-path api/sixtyfps-rs/Cargo.toml
cargo publish --manifest-path tools/lsp/Cargo.toml
cargo publish --manifest-path tools/viewer/Cargo.toml

