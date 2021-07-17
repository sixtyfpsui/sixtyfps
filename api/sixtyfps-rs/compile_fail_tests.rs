/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
/**
Test that the tokenizer properly rejects tokens with spaces.

This should work:

```
mod x {
    use sixtyfps::*;
    sixtyfps!{ Hello := Rectangle { } }
}
```

But his not:

```compile_fail
mod x {
    use sixtyfps::*;
    sixtyfps!{ Hello : = Rectangle { } }
}
```

*/
#[cfg(doctest)]
const basic: u32 = 0;
