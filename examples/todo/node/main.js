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
let sixtyfps = require("sixtyfps");
// import * as demo from "../ui/todo.60";
let demo = require("../ui/todo.60");
let app = new demo.MainWindow();

let model = new sixtyfps.ArrayModel([
    {
        title: "Implement the .60 file",
        checked: true
    },
    {
        title: "Do the Rust part",
        checked: false
    },
    {
        title: "Make the C++ code",
        checked: false
    },
    {
        title: "Write some JavaScript code",
        checked: true
    },
    {
        title: "Test the application",
        checked: false
    },
    {
        title: "Ship to customer",
        checked: false
    },
    {
        title: "???",
        checked: false
    },
    {
        title: "Profit",
        checked: false
    },
]);
app.todo_model = model;

app.todo_added.setHandler(function (text) {
    model.push({ title: text, checked: false })
})

app.remove_done.setHandler(function () {
    let offset = 0;
    const length = model.length;
    for (let i = 0; i < length; ++i) {
        if (model.rowData(i - offset).checked) {
            model.remove(i - offset, 1);
            offset++;
        }
    }
})

app.run();

