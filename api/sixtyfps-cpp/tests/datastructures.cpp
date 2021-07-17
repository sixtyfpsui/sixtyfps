/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

#include <chrono>
#define CATCH_CONFIG_MAIN
#include "catch2/catch.hpp"

#include <sixtyfps.h>
#include <sixtyfps_image.h>

SCENARIO("SharedString API")
{
    sixtyfps::SharedString str;

    REQUIRE(str.empty());

    SECTION("Construct from string_view")
    {
        std::string foo("Foo");
        std::string_view foo_view(foo);
        str = foo_view;
        REQUIRE(str == "Foo");
    }

    SECTION("Construct from char*")
    {
        str = "Bar";
        REQUIRE(str == "Bar");
    }
}

TEST_CASE("Basic SharedVector API", "[vector]")
{
    sixtyfps::SharedVector<int> vec;
    REQUIRE(vec.empty());

    SECTION("Initializer list")
    {
        sixtyfps::SharedVector<int> vec({ 1, 4, 10 });
        REQUIRE(vec.size() == 3);
        REQUIRE(vec[0] == 1);
        REQUIRE(vec[1] == 4);
        REQUIRE(vec[2] == 10);
    }
}

TEST_CASE("Property Tracker")
{
    using namespace sixtyfps::private_api;
    PropertyTracker tracker1;
    PropertyTracker tracker2;
    Property<int> prop(42);

    auto r = tracker1.evaluate([&]() { return tracker2.evaluate([&]() { return prop.get(); }); });
    REQUIRE(r == 42);

    prop.set(1);
    REQUIRE(tracker2.is_dirty());
    REQUIRE(tracker1.is_dirty());

    r = tracker1.evaluate(
            [&]() { return tracker2.evaluate_as_dependency_root([&]() { return prop.get(); }); });
    REQUIRE(r == 1);
    prop.set(100);
    REQUIRE(tracker2.is_dirty());
    REQUIRE(!tracker1.is_dirty());
}

TEST_CASE("Image")
{
    using namespace sixtyfps;

    // ensure a backend exists, using private api
    private_api::ComponentWindow wnd;

    Image img;
    {
        auto size = img.size();
        REQUIRE(size.width == 0.);
        REQUIRE(size.height == 0.);
    }

    img = Image::load_from_path(SOURCE_DIR "/../../vscode_extension/extension-logo.png");
    {
        auto size = img.size();
        REQUIRE(size.width == 128.);
        REQUIRE(size.height == 128.);
    }
}
