/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
#pragma once

#include <cstddef>
#include <new>
#include <algorithm>
#include <optional>

namespace vtable {

template<typename T>
struct VRefMut
{
    const T *vtable;
    void *instance;
};

struct Layout {
    std::size_t size;
    std::size_t align;
};

// For the C++'s purpose, they are all the same
template<typename T>
using VRef = VRefMut<T>;
template<typename T>
using VBox = VRefMut<T>;

template<typename T>
using Pin = T;

/*
template<typename T>
struct VBox {
    const T *vtable;
    void *instance;
};

template<typename T>
struct VRef {
    const T *vtable;
    const void *instance;
};
*/

struct AllowPin;

template<typename Base, typename T, typename Flag = void>
struct VOffset
{
    const T *vtable;
    std::uintptr_t offset;
};

template<typename VTable, typename X>
struct VRcInner {
    template<typename VTable_, typename X_> friend class VRc;
    template<typename VTable_, typename X_> friend class VWeak;
private:
    VRcInner() : layout {} {}
    const VTable *vtable = &X::static_vtable;
    int strong_ref = 1;
    int weak_ref = 1;
    std::uint16_t data_offset = offsetof(VRcInner, data);
    union {
        X data;
        Layout layout;
    };

    void *data_ptr() { return reinterpret_cast<char *>(this) + data_offset; }
    ~VRcInner() = delete;
};

struct Dyn {};

template<typename VTable, typename X = Dyn>
class VRc {
    VRcInner<VTable, X> *inner;
    VRc(VRcInner<VTable, X> *inner) : inner(inner) {}
    template<typename VTable_, typename X_> friend class VWeak;
public:
    ~VRc() {
        if (!--inner->strong_ref) {
            Layout layout = inner->vtable->drop_in_place({inner->vtable, &inner->data});
            layout.size += inner->data_offset;
            layout.align = std::max<size_t>(layout.align, alignof(VRcInner<VTable, Dyn>));
            inner->layout = layout;
            if (!--inner->weak_ref) {
                inner->vtable->dealloc(inner->vtable, reinterpret_cast<uint8_t*>(inner),  layout);
            }
        }
    }
    VRc(const VRc &other): inner(other.inner) {
        inner->strong_ref++;
    }
    VRc &operator=(const VRc &other) {
        if (inner == other.inner)
            return *this;
        this->~VRc();
        new(this) VRc(other);
        return *this;
    }
    /// Construct a new VRc holding an X.
    ///
    /// The type X must have a static member `static_vtable` of type VTable
    template<typename ...Args> static VRc make(Args... args) {
        auto mem = ::operator new(sizeof(VRcInner<VTable, X>), static_cast<std::align_val_t>(alignof(VRcInner<VTable, X>)));
        auto inner = new (mem) VRcInner<VTable, X>;
        new (&inner->data) X(args...);
        return VRc(inner);
    }

    const X* operator->() const {
        return &inner->data;
    }
    const X& operator*() const {
        return inner->data;
    }

    VRc<VTable, Dyn> into_dyn() const { return *reinterpret_cast<const VRc<VTable, Dyn> *>(this); }

    VRef<VTable> borrow() const { return { inner->vtable, inner->data_ptr() }; }

    friend bool operator==(const VRc &a, const VRc &b) {
        return a.inner == b.inner;
    }
    friend bool operator!=(const VRc &a, const VRc &b) {
        return a.inner != b.inner;
    }
};

template<typename VTable, typename X = Dyn>
class VWeak {
    VRcInner<VTable, X> *inner = nullptr;
public:
    VWeak() = default;
    ~VWeak() {
        if (inner && !--inner->weak_ref) {
            inner->vtable->dealloc(inner->vtable, reinterpret_cast<uint8_t*>(inner), inner->layout);
        }
    }
    VWeak(const VWeak &other): inner(other.inner) {
        inner && inner->weak_ref++;
    }
    VWeak(const VRc<VTable, X> &other): inner(other.inner) {
        inner && inner->weak_ref++;
    }
    VWeak &operator=(const VWeak &other) {
        if (inner == other.inner)
            return *this;
        this->~VWeak();
        new(this) VWeak(other);
        return *this;
    }

    std::optional<VRc<VTable, X>> lock() const {
        if (!inner || inner->strong_ref == 0)
            return {};
        inner->strong_ref++;
        return { VRc<VTable, X>(inner) };
    }

    VWeak<VTable, Dyn> into_dyn() const { return *reinterpret_cast<const VWeak<VTable, Dyn> *>(this); }

    friend bool operator==(const VWeak &a, const VWeak &b) {
        return a.inner == b.inner;
    }
    friend bool operator!=(const VWeak &a, const VWeak &b) {
        return a.inner != b.inner;
    }
};



} //namespace vtable

