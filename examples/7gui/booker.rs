/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

use chrono::NaiveDate;
use sixtyfps::SharedString;

sixtyfps::sixtyfps!(import { Booker } from "booker.60";);

pub fn main() {
    let booker = Booker::new();
    booker.on_validate_date(|date: SharedString| {
        NaiveDate::parse_from_str(date.as_str(), "%d.%m.%Y").is_ok()
    });
    booker.on_compare_date(|date1: SharedString, date2: SharedString| {
        let date1 = match NaiveDate::parse_from_str(date1.as_str(), "%d.%m.%Y") {
            Err(_) => return false,
            Ok(x) => x,
        };
        let date2 = match NaiveDate::parse_from_str(date2.as_str(), "%d.%m.%Y") {
            Err(_) => return false,
            Ok(x) => x,
        };
        date1 <= date2
    });

    booker.run();
}
