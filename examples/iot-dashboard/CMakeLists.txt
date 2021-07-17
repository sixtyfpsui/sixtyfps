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
project(sixtyfps_cpp_iot_dashboard LANGUAGES CXX)

if (NOT TARGET SixtyFPS::SixtyFPS)
    find_package(SixtyFPS REQUIRED)
endif()

FetchContent_Declare(fmt
  GIT_REPOSITORY https://github.com/fmtlib/fmt.git
  GIT_TAG 7.1.3
)
FetchContent_MakeAvailable(fmt)

add_executable(iot_dashboard main.cpp dashboard.cpp)
target_compile_definitions(iot_dashboard PRIVATE
   SOURCE_DIR=\"${CMAKE_CURRENT_SOURCE_DIR}\"
)
target_link_libraries(iot_dashboard PRIVATE SixtyFPS::SixtyFPS fmt::fmt )
