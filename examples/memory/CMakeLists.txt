# This file is part of the SixtyFPS Project -- https://sixtyfps.io
# Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
# Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>
#
# LICENSE BEGIN
# SPDX-License-Identifier: GPL-3.0-only
# This file is also available under commercial licensing terms.
# Please contact info@sixtyfps.io for more information.
# LICENSE END
cmake_minimum_required(VERSION 3.16)
project(memory LANGUAGES CXX)

if (NOT TARGET SixtyFPS::SixtyFPS)
    find_package(SixtyFPS REQUIRED)
endif()

# Note, we can't name it simply "memory" because there is a C++ standard header with the same name
# so `#include<memory>` includes the generated binary when it exists
add_executable(memory_game memory.cpp)
target_link_libraries(memory_game PRIVATE SixtyFPS::SixtyFPS)
sixtyfps_target_60_sources(memory_game memory.60)
