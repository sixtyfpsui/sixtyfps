#!/usr/bin/env node
/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

// import "sixtyfps";
require("sixtyfps");
// import * as demo from "../ui/printerdemo.60";
let demo = require("../ui/printerdemo.60");
let window = new demo.MainWindow();

window.ink_levels = [
    { color: "#00ffff", level: 0.30 },
    { color: "#ff00ff", level: 0.80 },
    { color: "#ffff00", level: 0.60 },
    { color: "#000000", level: 0.90 }];

window.fax_number_erase.setHandler(function () {
    window.fax_number = window.fax_number.substring(0, window.fax_number.length - 1);
})
window.fax_send.setHandler(function () {
    console.log("Send fax to " + window.fax_number);
    window.fax_number = "";
})

window.run();

