/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
#pragma once
#include <string_view>
#include <memory>

namespace sixtyfps::cbindgen_private {
struct PropertyAnimation;
}

#include "sixtyfps_properties_internal.h"

namespace sixtyfps::private_api {

using cbindgen_private::StateInfo;

inline void sixtyfps_property_set_animated_binding_helper(
        const cbindgen_private::PropertyHandleOpaque *handle, void (*binding)(void *, int32_t *),
        void *user_data, void (*drop_user_data)(void *),
        const cbindgen_private::PropertyAnimation *animation_data,
        cbindgen_private::PropertyAnimation (*transition_data)(void *, uint64_t *))
{
    cbindgen_private::sixtyfps_property_set_animated_binding_int(
            handle, binding, user_data, drop_user_data, animation_data, transition_data);
}

inline void sixtyfps_property_set_animated_binding_helper(
        const cbindgen_private::PropertyHandleOpaque *handle, void (*binding)(void *, float *),
        void *user_data, void (*drop_user_data)(void *),
        const cbindgen_private::PropertyAnimation *animation_data,
        cbindgen_private::PropertyAnimation (*transition_data)(void *, uint64_t *))
{
    cbindgen_private::sixtyfps_property_set_animated_binding_float(
            handle, binding, user_data, drop_user_data, animation_data, transition_data);
}

inline void sixtyfps_property_set_animated_binding_helper(
        const cbindgen_private::PropertyHandleOpaque *handle, void (*binding)(void *, Color *),
        void *user_data, void (*drop_user_data)(void *),
        const cbindgen_private::PropertyAnimation *animation_data,
        cbindgen_private::PropertyAnimation (*transition_data)(void *, uint64_t *))
{
    cbindgen_private::sixtyfps_property_set_animated_binding_color(
            handle, binding, user_data, drop_user_data, animation_data, transition_data);
}

inline void sixtyfps_property_set_animated_binding_helper(
        const cbindgen_private::PropertyHandleOpaque *handle, void (*binding)(void *, Brush *),
        void *user_data, void (*drop_user_data)(void *),
        const cbindgen_private::PropertyAnimation *animation_data,
        cbindgen_private::PropertyAnimation (*transition_data)(void *, uint64_t *))
{
    cbindgen_private::sixtyfps_property_set_animated_binding_brush(
            handle, binding, user_data, drop_user_data, animation_data, transition_data);
}

template<typename T>
struct Property
{
    Property() { cbindgen_private::sixtyfps_property_init(&inner); }
    ~Property() { cbindgen_private::sixtyfps_property_drop(&inner); }
    Property(const Property &) = delete;
    Property(Property &&) = delete;
    Property &operator=(const Property &) = delete;
    explicit Property(const T &value) : value(value)
    {
        cbindgen_private::sixtyfps_property_init(&inner);
    }

    /* Should it be implicit?
    void operator=(const T &value) {
        set(value);
    }*/

    void set(const T &value) const
    {
        if (this->value != value) {
            this->value = value;
            cbindgen_private::sixtyfps_property_set_changed(&inner, &this->value);
        }
    }

    const T &get() const
    {
        cbindgen_private::sixtyfps_property_update(&inner, &value);
        return value;
    }

    template<typename F>
    void set_binding(F binding) const
    {
        cbindgen_private::sixtyfps_property_set_binding(
                &inner,
                [](void *user_data, void *value) {
                    *reinterpret_cast<T *>(value) = (*reinterpret_cast<F *>(user_data))();
                },
                new F(binding), [](void *user_data) { delete reinterpret_cast<F *>(user_data); },
                nullptr, nullptr);
    }

    inline void set_animated_value(const T &value,
                                   const cbindgen_private::PropertyAnimation &animation_data) const;
    template<typename F>
    inline void
    set_animated_binding(F binding, const cbindgen_private::PropertyAnimation &animation_data) const
    {
        private_api::sixtyfps_property_set_animated_binding_helper(
                &inner,
                [](void *user_data, T *value) {
                    *reinterpret_cast<T *>(value) = (*reinterpret_cast<F *>(user_data))();
                },
                new F(binding), [](void *user_data) { delete reinterpret_cast<F *>(user_data); },
                &animation_data, nullptr);
    }

    template<typename F, typename Trans>
    inline void set_animated_binding_for_transition(F binding, Trans animation) const
    {
        struct UserData
        {
            F binding;
            Trans animation;
        };
        private_api::sixtyfps_property_set_animated_binding_helper(
                &inner,
                [](void *user_data, T *value) {
                    *reinterpret_cast<T *>(value) =
                            reinterpret_cast<UserData *>(user_data)->binding();
                },
                new UserData { binding, animation },
                [](void *user_data) { delete reinterpret_cast<UserData *>(user_data); }, nullptr,
                [](void *user_data, uint64_t *instant) {
                    return reinterpret_cast<UserData *>(user_data)->animation(instant);
                });
    }

    bool is_dirty() const { return cbindgen_private::sixtyfps_property_is_dirty(&inner); }

    static void link_two_way(const Property<T> *p1, const Property<T> *p2)
    {
        auto value = p2->get();
        cbindgen_private::PropertyHandleOpaque handle {};
        if ((p2->inner._0 & 0b10) == 0b10) {
            std::swap(handle, const_cast<Property<T> *>(p2)->inner);
        }
        auto common_property = std::make_shared<Property<T>>(handle, std::move(value));
        struct TwoWayBinding
        {
            std::shared_ptr<Property<T>> common_property;
        };
        auto del_fn = [](void *user_data) { delete reinterpret_cast<TwoWayBinding *>(user_data); };
        auto call_fn = [](void *user_data, void *value) {
            *reinterpret_cast<T *>(value) =
                    reinterpret_cast<TwoWayBinding *>(user_data)->common_property->get();
        };
        auto intercept_fn = [](void *user_data, const void *value) {
            reinterpret_cast<TwoWayBinding *>(user_data)->common_property->set(
                    *reinterpret_cast<const T *>(value));
            return true;
        };
        auto intercept_binding_fn = [](void *user_data, void *value) {
            cbindgen_private::sixtyfps_property_set_binding_internal(
                    &reinterpret_cast<TwoWayBinding *>(user_data)->common_property->inner, value);
            return true;
        };
        cbindgen_private::sixtyfps_property_set_binding(&p1->inner, call_fn,
                                                        new TwoWayBinding { common_property },
                                                        del_fn, intercept_fn, intercept_binding_fn);
        cbindgen_private::sixtyfps_property_set_binding(&p2->inner, call_fn,
                                                        new TwoWayBinding { common_property },
                                                        del_fn, intercept_fn, intercept_binding_fn);
    }

    /// Internal (private) constructor used by link_two_way
    explicit Property(cbindgen_private::PropertyHandleOpaque inner, T value)
        : inner(inner), value(std::move(value))
    {
    }

private:
    cbindgen_private::PropertyHandleOpaque inner;
    mutable T value {};
    template<typename F>
    friend void set_state_binding(const Property<StateInfo> &property, F binding);
};

template<>
inline void Property<int32_t>::set_animated_value(
        const int32_t &new_value, const cbindgen_private::PropertyAnimation &animation_data) const
{
    cbindgen_private::sixtyfps_property_set_animated_value_int(&inner, value, new_value,
                                                               &animation_data);
}

template<>
inline void
Property<float>::set_animated_value(const float &new_value,
                                    const cbindgen_private::PropertyAnimation &animation_data) const
{
    cbindgen_private::sixtyfps_property_set_animated_value_float(&inner, value, new_value,
                                                                 &animation_data);
}

template<typename F>
void set_state_binding(const Property<StateInfo> &property, F binding)
{
    cbindgen_private::sixtyfps_property_set_state_binding(
            &property.inner,
            [](void *user_data) -> int32_t { return (*reinterpret_cast<F *>(user_data))(); },
            new F(binding), [](void *user_data) { delete reinterpret_cast<F *>(user_data); });
}

/// PropertyTracker allows keeping track of when properties change and lazily evaluate code
/// if necessary.
/// Once constructed, you can call evaluate() with a functor that will be invoked. Any
/// Property<T> types that have their value read from within the invoked functor or any code that's
/// reached from there are added to internal book-keeping. When after returning from evaluate(),
/// any of these accessed properties change their value, the property tracker's is_dirt() function
/// will return true.
///
/// PropertyTracker instances nest, so if during the evaluation of one tracker, another tracker's
/// evaluate() function gets called and properties from within that evaluation change their value
/// later, both tracker instances will report true for is_dirty(). If you would like to disable the
/// nesting, use the evaluate_as_dependency_root() function instead.
struct PropertyTracker
{
    /// Constructs a new property tracker instance.
    PropertyTracker() { cbindgen_private::sixtyfps_property_tracker_init(&inner); }
    /// Destroys the property tracker.
    ~PropertyTracker() { cbindgen_private::sixtyfps_property_tracker_drop(&inner); }
    /// The copy constructor is intentionally deleted, property trackers cannot be copied.
    PropertyTracker(const PropertyTracker &) = delete;
    /// The assignment operator is intentionally deleted, property trackers cannot be copied.
    PropertyTracker &operator=(const PropertyTracker &) = delete;

    /// Returns true if any properties accessed during the last evaluate() call have changed their
    /// value since then.
    bool is_dirty() const { return cbindgen_private::sixtyfps_property_tracker_is_dirty(&inner); }

    /// Invokes the provided functor \a f and tracks accessed to any properties during that
    /// invocation.
    template<typename F>
    auto evaluate(const F &f) const -> std::enable_if_t<std::is_same_v<decltype(f()), void>>
    {
        cbindgen_private::sixtyfps_property_tracker_evaluate(
                &inner, [](void *f) { (*reinterpret_cast<const F *>(f))(); }, const_cast<F *>(&f));
    }

    /// Invokes the provided functor \a f and tracks accessed to any properties during that
    /// invocation. Use this overload if your functor returns a value, as evaluate() will pass it on
    /// and return it.
    template<typename F>
    auto evaluate(const F &f) const
            -> std::enable_if_t<!std::is_same_v<decltype(f()), void>, decltype(f())>
    {
        decltype(f()) result;
        this->evaluate([&] { result = f(); });
        return result;
    }

    /// Invokes the provided functor \a f and tracks accessed to any properties during that
    /// invocation.
    ///
    /// This starts a new dependency chain and if called during the evaluation of another
    /// property tracker, the outer tracker will not be notified if any accessed properties change.
    template<typename F>
    auto evaluate_as_dependency_root(const F &f) const
            -> std::enable_if_t<std::is_same_v<decltype(f()), void>>
    {
        cbindgen_private::sixtyfps_property_tracker_evaluate_as_dependency_root(
                &inner, [](void *f) { (*reinterpret_cast<const F *>(f))(); }, const_cast<F *>(&f));
    }

    /// Invokes the provided functor \a f and tracks accessed to any properties during that
    /// invocation. Use this overload if your functor returns a value, as evaluate() will pass it on
    /// and return it.
    ///
    /// This starts a new dependency chain and if called during the evaluation of another
    /// property tracker, the outer tracker will not be notified if any accessed properties change.
    template<typename F>
    auto evaluate_as_dependency_root(const F &f) const
            -> std::enable_if_t<!std::is_same_v<decltype(f()), void>, decltype(f())>
    {
        decltype(f()) result;
        this->evaluate_as_dependency_root([&] { result = f(); });
        return result;
    }

private:
    cbindgen_private::PropertyTrackerOpaque inner;
};

} // namespace sixtyfps::private_api
