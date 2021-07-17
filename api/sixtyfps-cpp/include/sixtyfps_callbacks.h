/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
#pragma once
#include <tuple>
#include "sixtyfps_properties_internal.h"

namespace sixtyfps::private_api {

/// A Callback stores a function pointer with no parameters and no return value.
/// It's possible to set that pointer via set_handler() and it can be invoked via call(). This is
/// used to implement callbacks in the `.60` language.
template<typename = void()>
struct Callback;
/// A Callback stores a function pointer with \a Arg parameters and a return value of type \a Ret.
/// It's possible to set that pointer via set_handler() and it can be invoked via call(). This is
/// used to implement callbacks in the `.60` language.
template<typename Ret, typename... Arg>
struct Callback<Ret(Arg...)>
{
    /// Constructs an empty callback that contains no handler.
    Callback() { cbindgen_private::sixtyfps_callback_init(&inner); }
    /// Destructs the callback.
    ~Callback() { cbindgen_private::sixtyfps_callback_drop(&inner); }
    Callback(const Callback &) = delete;
    Callback(Callback &&) = delete;
    Callback &operator=(const Callback &) = delete;

    /// Sets a new handler \a binding for this callback, that will be invoked when call() is called.
    template<typename F>
    void set_handler(F binding) const
    {
        cbindgen_private::sixtyfps_callback_set_handler(
                &inner,
                [](void *user_data, const void *arg, void *ret) {
                    *reinterpret_cast<Ret *>(ret) =
                            std::apply(*reinterpret_cast<F *>(user_data),
                                       *reinterpret_cast<const Tuple *>(arg));
                },
                new F(std::move(binding)),
                [](void *user_data) { delete reinterpret_cast<F *>(user_data); });
    }

    /// Invokes a previously set handler with the parameters \a arg and returns the return value of
    /// the handler.
    Ret call(const Arg &...arg) const
    {
        Ret r {};
        Tuple tuple { arg... };
        cbindgen_private::sixtyfps_callback_call(&inner, &tuple, &r);
        return r;
    }

private:
    using Tuple = std::tuple<Arg...>;
    cbindgen_private::CallbackOpaque inner;
};

/// A Callback stores a function pointer with \a Arg parameters and no return value.
/// It's possible to set that pointer via set_handler() and it can be invoked via call(). This is
/// used to implement callbacks in the `.60` language.
template<typename... Arg>
struct Callback<void(Arg...)>
{
    /// Constructs an empty callback that contains no handler.
    Callback() { cbindgen_private::sixtyfps_callback_init(&inner); }
    /// Destructs the callback.
    ~Callback() { cbindgen_private::sixtyfps_callback_drop(&inner); }
    Callback(const Callback &) = delete;
    Callback(Callback &&) = delete;
    Callback &operator=(const Callback &) = delete;

    /// Sets a new handler \a binding for this callback, that will be invoked when call() is called.
    template<typename F>
    void set_handler(F binding) const
    {
        cbindgen_private::sixtyfps_callback_set_handler(
                &inner,
                [](void *user_data, const void *arg, void *) {
                    std::apply(*reinterpret_cast<F *>(user_data),
                               *reinterpret_cast<const Tuple *>(arg));
                },
                new F(std::move(binding)),
                [](void *user_data) { delete reinterpret_cast<F *>(user_data); });
    }

    /// Invokes a previously set handler with the parameters \a arg.
    void call(const Arg &...arg) const
    {
        Tuple tuple { arg... };
        cbindgen_private::sixtyfps_callback_call(&inner, &tuple, reinterpret_cast<void *>(0x1));
    }

private:
    using Tuple = std::tuple<Arg...>;
    cbindgen_private::CallbackOpaque inner;
};

template<typename A, typename R>
struct CallbackSignatureHelper
{
    using Result = R(A);
};
template<typename R>
struct CallbackSignatureHelper<void, R>
{
    using Result = R();
};
template<typename A, typename R = void>
using CallbackHelper = Callback<typename CallbackSignatureHelper<A, R>::Result>;

} // namespace sixtyfps
