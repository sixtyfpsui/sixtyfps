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
project(SixtyFPS LANGUAGES CXX)

include(FetchContent)
FetchContent_Declare(
    Corrosion
    GIT_REPOSITORY https://github.com/AndrewGaspar/corrosion.git
    GIT_TAG b7a78deb2632b502717c7b636a925677a99e06a1
)
FetchContent_MakeAvailable(Corrosion)
corrosion_import_crate(MANIFEST_PATH "${CMAKE_CURRENT_SOURCE_DIR}/../../Cargo.toml"
    CRATES sixtyfps-compiler sixtyfps-cpp xtask)

if (CMAKE_VERSION VERSION_GREATER_EQUAL 3.19.0)
    # For the CMake build don't rely on qmake being in PATH but use CMake to locate Qt. This
    # means usually CMAKE_PREFIX_PATH is set.
    find_package(Qt5 5.15 QUIET COMPONENTS Core Widgets)
    if (TARGET Qt5::qmake)
        set_property(
            TARGET sixtyfps-cpp
            APPEND
            PROPERTY CORROSION_ENVIRONMENT_VARIABLES
            QMAKE=$<TARGET_PROPERTY:Qt5::qmake,LOCATION>
        )
    else()
        set_property(
            TARGET sixtyfps-cpp
            APPEND
            PROPERTY CORROSION_ENVIRONMENT_VARIABLES
            SIXTYFPS_NO_QT=1
        )
    endif()
endif()

if (CMAKE_VERSION VERSION_GREATER_EQUAL 3.19.0)
    set_property(
        TARGET xtask
        PROPERTY CORROSION_USE_HOST_BUILD 1
    )
    set_property(
        TARGET sixtyfps-compiler
        PROPERTY CORROSION_USE_HOST_BUILD 1
    )
elseif(CMAKE_CROSSCOMPILING)
    message(FATAL_ERROR "Cross-compiling SixtyFPS requires CMake 3.19 or newer")
endif()

add_library(SixtyFPS INTERFACE)
add_library(SixtyFPS::SixtyFPS ALIAS SixtyFPS)
target_link_libraries(SixtyFPS INTERFACE sixtyfps-cpp)
target_compile_features(SixtyFPS INTERFACE cxx_std_17)

file(GLOB api_headers RELATIVE "${CMAKE_CURRENT_SOURCE_DIR}/include/"
    "${CMAKE_CURRENT_SOURCE_DIR}/include/*.h")

foreach(header IN LISTS api_headers)
    set_property(TARGET SixtyFPS APPEND PROPERTY PUBLIC_HEADER include/${header})
endforeach()

set(generated_headers
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/sixtyfps_internal.h
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/sixtyfps_string_internal.h
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/sixtyfps_brush_internal.h
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/sixtyfps_sharedvector_internal.h
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/sixtyfps_properties_internal.h
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/sixtyfps_image_internal.h
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/sixtyfps_color_internal.h
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/sixtyfps_pathdata_internal.h
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/sixtyfps_qt_internal.h
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/sixtyfps_backend_internal.h
    ${CMAKE_CURRENT_BINARY_DIR}/generated_include/sixtyfps_interpreter_internal.h
)

file(GLOB generated_headers_dependencies
    "${CMAKE_CURRENT_SOURCE_DIR}/../../sixtyfps_runtime/corelib/*.rs")

add_custom_target(
    generated_headers_target
    COMMAND
    xtask cbindgen --output-dir "${CMAKE_CURRENT_BINARY_DIR}/generated_include/"
    BYPRODUCTS ${generated_headers}
    DEPENDS ${generated_headers_dependencies}
)

add_dependencies(SixtyFPS generated_headers_target)
foreach(header IN LISTS generated_headers)
    set_property(TARGET SixtyFPS APPEND PROPERTY PUBLIC_HEADER ${header})
endforeach()

target_include_directories(SixtyFPS INTERFACE
    $<BUILD_INTERFACE:${CMAKE_CURRENT_BINARY_DIR}/generated_include>
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
    $<INSTALL_INTERFACE:include/sixtyfps>
)

add_executable(SixtyFPS::sixtyfps-compiler ALIAS sixtyfps-compiler)
include(${CMAKE_CURRENT_LIST_DIR}/cmake/SixtyFPSMacro.cmake)

export(TARGETS SixtyFPS sixtyfps-cpp
    NAMESPACE SixtyFPS:: FILE "${CMAKE_BINARY_DIR}/lib/cmake/SixtyFPS/SixtyFPSTargets.cmake")
install(EXPORT SixtyFPSTargets NAMESPACE SixtyFPS:: DESTINATION lib/cmake/SixtyFPS)
install(TARGETS SixtyFPS sixtyfps-cpp
    EXPORT SixtyFPSTargets  LIBRARY DESTINATION lib PUBLIC_HEADER DESTINATION include/sixtyfps)

include(CMakePackageConfigHelpers)
include(GNUInstallDirs)

install(FILES $<TARGET_FILE:sixtyfps-cpp-shared> TYPE LIB)
if(WIN32)
    install(FILES $<TARGET_LINKER_FILE:sixtyfps-cpp-shared> TYPE LIB)
    # Copy the dll to the top-level bin directory, where the examples will
    # will also be located, so that they can find the dll.
    add_custom_target(SixtyFPS_dll_convenience ALL DEPENDS sixtyfps-cpp-shared
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
            $<TARGET_FILE:sixtyfps-cpp-shared>
            ${CMAKE_BINARY_DIR}/bin/$<TARGET_FILE_NAME:sixtyfps-cpp-shared>)
endif()

install(PROGRAMS $<TARGET_FILE:sixtyfps-compiler> TYPE BIN)

set(SIXTYFPS_LIB_PROPERTIES "")
foreach(prop
        IMPORTED_LOCATION IMPORTED_LOCATION_DEBUG IMPORTED_LOCATION_RELEASE
        IMPORTED_LOCATION_RELWITHDEBINFO IMPORTED_LOCATION_MINSIZEREL
        IMPORTED_IMPLIB IMPORTED_IMPLIB_DEBUG IMPORTED_IMPLIB_RELEASE)
  get_target_property(value sixtyfps-cpp-shared ${prop})
  if(value)
    get_filename_component(value ${value} NAME)
    list(APPEND SIXTYFPS_LIB_PROPERTIES ${prop} "\${_IMPORT_PREFIX}/${CMAKE_INSTALL_LIBDIR}/${value}")
  endif()
endforeach()



configure_package_config_file("cmake/SixtyFPSConfig.cmake.in" "${CMAKE_CURRENT_BINARY_DIR}/lib/cmake/SixtyFPS/SixtyFPSConfig.cmake" INSTALL_DESTINATION lib/cmake/SixtyFPS)

write_basic_package_version_file(
  ${CMAKE_CURRENT_BINARY_DIR}/lib/cmake/SixtyFPS/SixtyFPSConfigVersion.cmake
  VERSION 0.1.0
  COMPATIBILITY ExactVersion
)

install(FILES
    "${CMAKE_CURRENT_BINARY_DIR}/lib/cmake/SixtyFPS/SixtyFPSConfig.cmake"
    "${CMAKE_CURRENT_BINARY_DIR}/lib/cmake/SixtyFPS/SixtyFPSConfigVersion.cmake"
    "${CMAKE_CURRENT_LIST_DIR}/cmake/SixtyFPSMacro.cmake"
    DESTINATION lib/cmake/SixtyFPS
)

set(CPACK_PACKAGE_NAME "SixtyFPS")
set(CPACK_PACKAGE_VENDOR "SixtyFPS")
set(CPACK_VERBATIM_VARIABLES true)
set(CPACK_PACKAGE_VERSION_MAJOR 0)
set(CPACK_PACKAGE_VERSION_MINOR 1)
set(CPACK_PACKAGE_VERSION_PATCH 0)
set(CPACK_PACKAGE_HOMEPAGE_URL "https://sixtyfps.io")
set(CPACK_RESOURCE_FILE_LICENSE "${CMAKE_CURRENT_LIST_DIR}/../../LICENSE.md")
set(CPACK_RESOURCE_FILE_README "${CMAKE_CURRENT_LIST_DIR}/README.md")
set(CPACK_STRIP_FILES ON)
include(CPack)

if(BUILD_TESTING)
    FetchContent_Declare(
        Catch2
        GIT_REPOSITORY https://github.com/catchorg/Catch2.git
        GIT_TAG        v2.13.1
    )

    FetchContent_MakeAvailable(Catch2)

    find_package(Threads REQUIRED)

    macro(sixtyfps_test NAME)
        add_executable(test_${NAME} tests/${NAME}.cpp)
        target_link_libraries(test_${NAME} PRIVATE SixtyFPS Catch2::Catch2)
        target_compile_definitions(test_${NAME} PRIVATE
            SOURCE_DIR=\"${CMAKE_CURRENT_SOURCE_DIR}/\"
        )
        add_test(test_${NAME} ${CMAKE_CURRENT_BINARY_DIR}/test_${NAME})
        # Somehow the wrong relative rpath ends up in the binary, requiring us to change the
        # working directory.
        if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
            set_property(TEST test_${NAME} PROPERTY WORKING_DIRECTORY "${CMAKE_BINARY_DIR}")
        endif()
        if(MSVC)
            target_compile_options(test_${NAME} PRIVATE /W3)
        else()
            target_compile_options(test_${NAME} PRIVATE -Wall -Wextra -Werror)
        endif()

    endmacro(sixtyfps_test)
    sixtyfps_test(datastructures)
    sixtyfps_test(interpreter)
    sixtyfps_test(eventloop)
    target_link_libraries(test_eventloop PRIVATE Threads::Threads)
endif()
