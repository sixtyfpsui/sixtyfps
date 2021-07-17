/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
#pragma once
#include "sixtyfps_sharedvector_internal.h"
#include <atomic>
#include <algorithm>
#include <initializer_list>

namespace sixtyfps {

/// SharedVector is a vector template class similar to std::vector that's primarily used for passing
/// data in and out of the SixtyFPS run-time library. It uses implicit-sharing to make creating
/// copies cheap. Only when a function changes the vector's data, a copy is is made.
template<typename T>
struct SharedVector
{
    /// Creates a new, empty vector.
    SharedVector()
        : inner(const_cast<SharedVectorHeader *>(reinterpret_cast<const SharedVectorHeader *>(
                cbindgen_private::sixtyfps_shared_vector_empty())))
    {
    }
    /// Creates a new vector that holds all the elements of the given std::initializer_list \a args.
    SharedVector(std::initializer_list<T> args) : SharedVector()
    {
        auto new_array = SharedVector::with_capacity(args.size());
        auto new_data = reinterpret_cast<T *>(new_array.inner + 1);
        auto input_it = args.begin();
        for (std::size_t i = 0; i < args.size(); ++i, ++input_it) {
            new (new_data + i) T(*input_it);
            new_array.inner->size++;
        }
        *this = std::move(new_array);
    }

    /// Creates a new vector that is a copy of \a other.
    SharedVector(const SharedVector &other) : inner(other.inner)
    {
        if (inner->refcount > 0) {
            ++inner->refcount;
        }
    }
    /// Destroys this vector. The underlying data is destroyed if no other
    /// vector references it.
    ~SharedVector() { drop(); }
    /// Assigns the data of \a other to this vector and returns a reference to this vector.
    SharedVector &operator=(const SharedVector &other)
    {
        if (other.inner == inner) {
            return *this;
        }
        drop();
        inner = other.inner;
        if (inner->refcount > 0) {
            ++inner->refcount;
        }
        return *this;
    }
    /// Move-assign's \a other to this vector and returns a reference to this vector.
    SharedVector &operator=(SharedVector &&other)
    {
        std::swap(inner, other.inner);
        return *this;
    }

    /// Returns a const pointer to the first element of this vector.
    const T *cbegin() const { return reinterpret_cast<const T *>(inner + 1); }

    /// Returns a const pointer that points past the last element of this vector. The
    /// pointer cannot be dereferenced, it can only be used for comparison.
    const T *cend() const { return cbegin() + inner->size; }

    /// Returns a const pointer to the first element of this vector.
    const T *begin() const { return cbegin(); }
    /// Returns a const pointer that points past the last element of this vector. The
    /// pointer cannot be dereferenced, it can only be used for comparison.
    const T *end() const { return cend(); }

    /// Returns a pointer to the first element of this vector.
    T *begin()
    {
        detach(inner->size);
        return reinterpret_cast<T *>(inner + 1);
    }

    /// Returns a pointer that points past the last element of this vector. The
    /// pointer cannot be dereferenced, it can only be used for comparison.
    T *end()
    {
        detach(inner->size);
        return begin() + inner->size;
    }

    /// Returns the number of elements in this vector.
    std::size_t size() const { return inner->size; }

    /// Returns true if there are no elements on this vector; false otherwise.
    bool empty() const { return inner->size == 0; }

    /// This indexing operator returns a reference to the \a `index`th element of this vector.
    T &operator[](std::size_t index) { return begin()[index]; }
    /// This indexing operator returns a const reference to the \a `index`th element of this vector.
    const T &operator[](std::size_t index) const { return begin()[index]; }

    /// Returns a reference to the \a `index`th element of this vector.
    const T &at(std::size_t index) const { return begin()[index]; }

    /// Appends the \a value as a new element to the end of this vector.
    void push_back(const T &value)
    {
        detach(inner->size + 1);
        new (end()) T(value);
        inner->size++;
    }
    /// Moves the \a value as a new element to the end of this vector.
    void push_back(T &&value)
    {
        detach(inner->size + 1);
        new (end()) T(std::move(value));
        inner->size++;
    }

    /// Returns true if the vector \a a has the same number of elements as \a b
    /// and all the elements also compare equal; false otherwise.
    friend bool operator==(const SharedVector &a, const SharedVector &b)
    {
        if (a.size() != a.size())
            return false;
        return std::equal(a.cbegin(), a.cend(), b.cbegin());
    }
    /// Returns false if the vector \a a has the same number of elements as \a b
    /// and all the elements also compare equal; true otherwise.
    friend bool operator!=(const SharedVector &a, const SharedVector &b) { return !(a == b); }

private:
    void detach(std::size_t expected_capacity)
    {
        if (inner->refcount == 1 && expected_capacity <= inner->capacity) {
            return;
        }
        auto new_array = SharedVector::with_capacity(expected_capacity);
        auto old_data = reinterpret_cast<const T *>(inner + 1);
        auto new_data = reinterpret_cast<T *>(new_array.inner + 1);
        for (std::size_t i = 0; i < inner->size; ++i) {
            new (new_data + i) T(old_data[i]);
            new_array.inner->size++;
        }
        *this = std::move(new_array);
    }

    void drop()
    {
        if (inner->refcount > 0 && (--inner->refcount) == 0) {
            auto b = begin(), e = end();
            for (auto it = b; it < e; ++it) {
                it->~T();
            }
            cbindgen_private::sixtyfps_shared_vector_free(reinterpret_cast<uint8_t *>(inner),
                                                          sizeof(SharedVectorHeader)
                                                                  + inner->capacity * sizeof(T),
                                                          alignof(SharedVectorHeader));
        }
    }

    static SharedVector with_capacity(std::size_t capacity)
    {
        auto mem = cbindgen_private::sixtyfps_shared_vector_allocate(
                sizeof(SharedVectorHeader) + capacity * sizeof(T), alignof(SharedVectorHeader));
        return SharedVector(new (mem) SharedVectorHeader { { 1 }, 0, capacity });
    }

#if !defined(DOXYGEN)
    // Unfortunately, this cannot be generated by cbindgen because std::atomic is not understood
    struct SharedVectorHeader
    {
        std::atomic<std::intptr_t> refcount;
        std::size_t size;
        std::size_t capacity;
    };
    static_assert(alignof(T) <= alignof(SharedVectorHeader),
                  "Not yet supported because we would need to add padding");
    SharedVectorHeader *inner;
    explicit SharedVector(SharedVectorHeader *inner) : inner(inner) { }
#endif
};

#if !defined(DOXYGEN) // Hide these from Doxygen as Slice is private API
template<typename T>
bool operator==(cbindgen_private::Slice<T> a, cbindgen_private::Slice<T> b)
{
    if (a.len != b.len)
        return false;
    return std::equal(a.ptr, a.ptr + a.len, b.ptr);
}
template<typename T>
bool operator!=(cbindgen_private::Slice<T> a, cbindgen_private::Slice<T> b)
{
    return !(a != b);
}
#endif // !defined(DOXYGEN)

}
