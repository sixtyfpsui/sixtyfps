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
#include "sixtyfps_string_internal.h"

namespace sixtyfps {

/// A string type used by the SixtyFPS run-time.
///
/// SharedString uses implicit data sharing to make it efficient to pass around copies. When
/// copying, a reference to the data is cloned, not the data itself.
///
/// The class provides constructors from std::string_view as well as the automatic conversion to
/// a std::string_view.
///
/// For convenience, it's also possible to convert a number to a string using
/// SharedString::from_number(double).
///
/// Under the hood the string data is UTF-8 encoded and it is always terminated with a null
/// character.
struct SharedString
{
    /// Creates an empty default constructed string.
    SharedString() { cbindgen_private::sixtyfps_shared_string_from_bytes(this, "", 0); }
    /// Creates a new SharedString from the string view \a s. The underlying string data
    /// is copied.
    SharedString(std::string_view s)
    {
        cbindgen_private::sixtyfps_shared_string_from_bytes(this, s.data(), s.size());
    }
    /// Creates a new SharedString from the null-terminated string pointer \a s. The underlying
    /// string data is copied. It is assumed that the string is UTF-8 encoded.
    SharedString(const char *s) : SharedString(std::string_view(s)) { }
    /// Creates a new SharedString from \a other.
    SharedString(const SharedString &other)
    {
        cbindgen_private::sixtyfps_shared_string_clone(this, &other);
    }
    /// Destroys this SharedString and frees the memory if this is the last instance
    /// referencing it.
    ~SharedString() { cbindgen_private::sixtyfps_shared_string_drop(this); }
    /// Assigns \a other to this string and returns a reference to this string.
    SharedString &operator=(const SharedString &other)
    {
        cbindgen_private::sixtyfps_shared_string_drop(this);
        cbindgen_private::sixtyfps_shared_string_clone(this, &other);
        return *this;
    }
    /// Assigns the string view \a s to this string and returns a reference to this string.
    /// The underlying string data is copied.  It is assumed that the string is UTF-8 encoded.
    SharedString &operator=(std::string_view s)
    {
        cbindgen_private::sixtyfps_shared_string_drop(this);
        cbindgen_private::sixtyfps_shared_string_from_bytes(this, s.data(), s.size());
        return *this;
    }
    /// Assigns null-terminated string pointer \a s to this string and returns a reference
    /// to this string. The underlying string data is copied. It is assumed that the string
    /// is UTF-8 encoded.
    SharedString &operator=(const char *s) { return *this = std::string_view(s); }

    /// Move-assigns \a other to this SharedString instance.
    SharedString &operator=(SharedString &&other)
    {
        std::swap(inner, other.inner);
        return *this;
    }

    /// Provides a view to the string data. The returned view is only valid as long as at
    /// least this SharedString exists.
    operator std::string_view() const
    {
        return cbindgen_private::sixtyfps_shared_string_bytes(this);
    }
    /// Provides a raw pointer to the string data. The returned pointer is only valid as long as at
    /// least this SharedString exists.
    auto data() const -> const char *
    {
        return cbindgen_private::sixtyfps_shared_string_bytes(this);
    }

    /// Returns a pointer to the first character. It is only safe to dereference the pointer if the
    /// string contains at least one character.
    const char *begin() const { return data(); }
    /// Returns a point past the last character of the string. It is not safe to dereference the
    /// pointer, but it is suitable for comparison.
    const char *end() const { return &*std::string_view(*this).end(); }

    /// \return true if the string contains no characters; false otherwise.
    bool empty() const { return std::string_view(*this).empty(); }

    /// \return true if the string starts with the specified prefix string; false otherwise
    bool starts_with(std::string_view prefix) const
    {
        return std::string_view(*this).substr(0, prefix.size()) == prefix;
    }

    /// \return true if the string ends with the specified prefix string; false otherwise
    bool ends_with(std::string_view prefix) const
    {
        std::string_view self_view(*this);
        return self_view.size() >= prefix.size()
                && self_view.compare(self_view.size() - prefix.size(), std::string_view::npos,
                                     prefix)
                == 0;
    }

    /// Creates a new SharedString from the given number \a n. The string representation of the
    /// number uses a minimal formatting scheme: If \a n has no fractional part, the number will be
    /// formatted as an integer.
    ///
    /// For example:
    /// \code
    ///     auto str = sixtyfps::SharedString::from_number(42); // creates "42"
    ///     auto str2 = sixtyfps::SharedString::from_number(100.5) // creates "100.5"
    /// \endcode
    static SharedString from_number(double n) { return SharedString(n); }

    /// Returns true if \a a is equal to \a b; otherwise returns false.
    friend bool operator==(const SharedString &a, const SharedString &b)
    {
        return std::string_view(a) == std::string_view(b);
    }
    /// Returns true if \a a is not equal to \a b; otherwise returns false.
    friend bool operator!=(const SharedString &a, const SharedString &b)
    {
        return std::string_view(a) != std::string_view(b);
    }

    /// Returns true if \a a is lexicographically less than \a b; false otherwise.
    friend bool operator<(const SharedString &a, const SharedString &b)
    {
        return std::string_view(a) < std::string_view(b);
    }
    /// Returns true if \a a is lexicographically less or equal than \a b; false otherwise.
    friend bool operator<=(const SharedString &a, const SharedString &b)
    {
        return std::string_view(a) <= std::string_view(b);
    }
    /// Returns true if \a a is lexicographically greater than \a b; false otherwise.
    friend bool operator>(const SharedString &a, const SharedString &b)
    {
        return std::string_view(a) > std::string_view(b);
    }
    /// Returns true if \a a is lexicographically greater or equal than \a b; false otherwise.
    friend bool operator>=(const SharedString &a, const SharedString &b)
    {
        return std::string_view(a) >= std::string_view(b);
    }

    /// Writes the \a shared_string to the specified \a stream and returns a reference to the
    /// stream.
    friend std::ostream &operator<<(std::ostream &stream, const SharedString &shared_string)
    {
        return stream << std::string_view(shared_string);
    }

    /// Concatenates \a a and \a and returns the result as a new SharedString.
    friend SharedString operator+(const SharedString &a, std::string_view b)
    {
        SharedString a2 = a;
        return a2 += b;
    }
    /// Move-concatenates \a b to \a and returns a reference to \a a.
    friend SharedString operator+(SharedString &&a, std::string_view b)
    {
        a += b;
        return a;
    }
    /// Appends \a other to this string and returns a reference to this.
    SharedString &operator+=(std::string_view other)
    {
        cbindgen_private::sixtyfps_shared_string_append(this, other.data(), other.size());
        return *this;
    }

private:
    /// Use SharedString::from_number
    explicit SharedString(double n)
    {
        cbindgen_private::sixtyfps_shared_string_from_number(this, n);
    }
    void *inner; // opaque
};

namespace private_api {
inline cbindgen_private::Slice<uint8_t> string_to_slice(std::string_view str)
{
    return cbindgen_private::Slice<uint8_t> {
        const_cast<unsigned char *>(reinterpret_cast<const unsigned char *>(str.data())), str.size()
    };
}
}

}
