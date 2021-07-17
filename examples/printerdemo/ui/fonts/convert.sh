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

# This script converts the NotoSans font from https://www.google.com/get/noto/#sans-lgc
# to a subset for this demo

# You need to install `pyftsubset` from the `fonttools`. That's available via `brew install fonttools`,
# or `sudo apt-get install fonttools`.

cp NotoSans-unhinted/LICENSE_OFL.txt .

for weight in Light Regular Bold; do
    pyftsubset NotoSans-unhinted/NotoSans-$weight.ttf --unicodes="U+0020-007F,U+2026" --output-file=NotoSans-$weight.ttf
done

