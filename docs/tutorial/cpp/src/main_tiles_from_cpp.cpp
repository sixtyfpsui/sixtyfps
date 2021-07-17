/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
// main.cpp

#include "memory_tiles_from_cpp.h" // generated header from memory_tiles_from_cpp.60
// ANCHOR: main
// ...

#include <random> // Added

int main()
{
    auto main_window = MainWindow::create();
    auto old_tiles = main_window->get_memory_tiles();
    std::vector<TileData> new_tiles;
    new_tiles.reserve(old_tiles->row_count() * 2);
    for (int i = 0; i < old_tiles->row_count(); ++i) {
        new_tiles.push_back(old_tiles->row_data(i));
        new_tiles.push_back(old_tiles->row_data(i));
    }
    std::default_random_engine rng {};
    std::shuffle(new_tiles.begin(), new_tiles.end(), rng);
    auto tiles_model = std::make_shared<sixtyfps::VectorModel<TileData>>(new_tiles);
    main_window->set_memory_tiles(tiles_model);

    main_window->run();
}
// ANCHOR_END: main