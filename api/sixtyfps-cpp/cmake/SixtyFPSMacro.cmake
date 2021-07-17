# This file is part of the SixtyFPS Project -- https://sixtyfps.io
# Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
# Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>
#
# LICENSE BEGIN
# SPDX-License-Identifier: GPL-3.0-only
# This file is also available under commercial licensing terms.
# Please contact info@sixtyfps.io for more information.
# LICENSE END

function(SIXTYFPS_TARGET_60_SOURCES target)
    foreach (it IN ITEMS ${ARGN})
        get_filename_component(_60_BASE_NAME ${it} NAME_WE)
        get_filename_component(_60_ABSOLUTE ${it} REALPATH BASE_DIR ${CMAKE_CURRENT_SOURCE_DIR})

        if(CMAKE_GENERATOR STREQUAL "Ninja")
            # this code is inspired from the llvm source
            # https://github.com/llvm/llvm-project/blob/a00290ed10a6b4e9f6e9be44ceec367562f270c6/llvm/cmake/modules/TableGen.cmake#L13

            file(RELATIVE_PATH _60_BASE_NAME_REL ${CMAKE_BINARY_DIR} ${CMAKE_CURRENT_BINARY_DIR}/${_60_BASE_NAME})

            add_custom_command(
                OUTPUT ${CMAKE_CURRENT_BINARY_DIR}/${_60_BASE_NAME}.h
                COMMAND SixtyFPS::sixtyfps-compiler ${_60_ABSOLUTE}
                    -o ${_60_BASE_NAME_REL}.h  --depfile ${_60_BASE_NAME_REL}.d
                DEPENDS SixtyFPS::sixtyfps-compiler ${_60_ABSOLUTE}
                COMMENT "Generating ${_60_BASE_NAME}.h"
                DEPFILE ${CMAKE_CURRENT_BINARY_DIR}/${_60_BASE_NAME}.d
                WORKING_DIRECTORY ${CMAKE_BINARY_DIR}
            )
        else(CMAKE_GENERATOR STREQUAL "Ninja")
            get_filename_component(_60_DIR ${_60_ABSOLUTE} DIRECTORY )
            file(GLOB ALL_60S "${_60_DIR}/*.60")
            add_custom_command(
                OUTPUT ${CMAKE_CURRENT_BINARY_DIR}/${_60_BASE_NAME}.h
                COMMAND SixtyFPS::sixtyfps-compiler ${_60_ABSOLUTE}
                    -o ${CMAKE_CURRENT_BINARY_DIR}/${_60_BASE_NAME}.h
                DEPENDS SixtyFPS::sixtyfps-compiler ${_60_ABSOLUTE} ${ALL_60S}
                COMMENT "Generating ${_60_BASE_NAME}.h"
            )
        endif(CMAKE_GENERATOR STREQUAL "Ninja")

        target_sources(${target} PRIVATE ${CMAKE_CURRENT_BINARY_DIR}/${_60_BASE_NAME}.h)
    endforeach()
    target_include_directories(${target} PRIVATE ${CMAKE_CURRENT_BINARY_DIR})
endfunction()
