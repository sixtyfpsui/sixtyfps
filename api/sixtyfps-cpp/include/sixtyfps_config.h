/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
#pragma once

#include <cstdint>

#if UINTPTR_MAX == 0xFFFFFFFF
#    define SIXTYFPS_TARGET_32
#elif UINTPTR_MAX == 0xFFFFFFFFFFFFFFFFu
#    define SIXTYFPS_TARGET_64
#endif

#if !defined(DOXYGEN)
#    if defined(_MSC_VER)
#        define SIXTYFPS_DLL_IMPORT __declspec(dllimport)
#    elif defined(__GNUC__)
#        if defined(_WIN32) || defined(_WIN64)
#            define SIXTYFPS_DLL_IMPORT __declspec(dllimport)
#        else
#            define SIXTYFPS_DLL_IMPORT __attribute__((visibility("default")))
#        endif
#    else
#        define SIXTYFPS_DLL_IMPORT
#    endif
#endif // !defined(DOXYGEN)