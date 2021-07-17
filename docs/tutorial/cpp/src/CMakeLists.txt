# This file is part of the SixtyFPS Project -- https://sixtyfps.io
# Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
# Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>
#
# LICENSE BEGIN
# SPDX-License-Identifier: GPL-3.0-only
# This file is also available under commercial licensing terms.
# Please contact info@sixtyfps.io for more information.
# LICENSE END
cmake_minimum_required(VERSION 3.14)

if (NOT TARGET SixtyFPS::SixtyFPS)
    find_package(SixtyFPS REQUIRED)
endif()

add_executable(memory_tutorial_initial main_initial.cpp)
target_link_libraries(memory_tutorial_initial PRIVATE SixtyFPS::SixtyFPS)
sixtyfps_target_60_sources(memory_tutorial_initial memory.60)

add_executable(memory_tutorial_tiles_from_cpp main_tiles_from_cpp.cpp)
target_link_libraries(memory_tutorial_tiles_from_cpp PRIVATE SixtyFPS::SixtyFPS)
sixtyfps_target_60_sources(memory_tutorial_tiles_from_cpp memory_tiles_from_cpp.60)

add_executable(memory_tutorial_game_logic main_game_logic.cpp)
target_link_libraries(memory_tutorial_game_logic PRIVATE SixtyFPS::SixtyFPS)
sixtyfps_target_60_sources(memory_tutorial_game_logic memory_game_logic.60)
