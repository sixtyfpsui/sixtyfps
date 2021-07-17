/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

#define CATCH_CONFIG_MAIN
#include "catch2/catch.hpp"

#include <sixtyfps.h>
#include <sixtyfps_interpreter.h>

SCENARIO("Value API")
{
    using namespace sixtyfps::interpreter;
    Value value;

    REQUIRE(value.type() == Value::Type::Void);

    SECTION("Construct a string")
    {
        REQUIRE(!value.to_string().has_value());
        sixtyfps::SharedString cpp_str("Hello World");
        value = Value(cpp_str);
        REQUIRE(value.type() == Value::Type::String);

        auto string_opt = value.to_string();
        REQUIRE(string_opt.has_value());
        REQUIRE(string_opt.value() == "Hello World");
    }

    SECTION("Construct a number")
    {
        REQUIRE(!value.to_number().has_value());
        const double number = 42.0;
        value = Value(number);
        REQUIRE(value.type() == Value::Type::Number);

        auto number_opt = value.to_number();
        REQUIRE(number_opt.has_value());
        REQUIRE(number_opt.value() == number);
    }

    SECTION("Construct a bool")
    {
        REQUIRE(!value.to_bool().has_value());
        value = Value(true);
        REQUIRE(value.type() == Value::Type::Bool);

        auto bool_opt = value.to_bool();
        REQUIRE(bool_opt.has_value());
        REQUIRE(bool_opt.value() == true);
    }

    SECTION("Construct an array")
    {
        REQUIRE(!value.to_array().has_value());
        sixtyfps::SharedVector<Value> array { Value(42.0), Value(true) };
        value = Value(array);
        REQUIRE(value.type() == Value::Type::Array);

        auto array_opt = value.to_array();
        REQUIRE(array_opt.has_value());

        auto extracted_array = array_opt.value();
        REQUIRE(extracted_array.size() == 2);
        REQUIRE(extracted_array[0].to_number().value() == 42);
        REQUIRE(extracted_array[1].to_bool().value());
    }

    SECTION("Construct a brush")
    {
        REQUIRE(!value.to_brush().has_value());
        sixtyfps::Brush brush(sixtyfps::Color::from_rgb_uint8(255, 0, 255));
        value = Value(brush);
        REQUIRE(value.type() == Value::Type::Brush);

        auto brush_opt = value.to_brush();
        REQUIRE(brush_opt.has_value());
        REQUIRE(brush_opt.value() == brush);
    }

    SECTION("Construct a struct")
    {
        REQUIRE(!value.to_struct().has_value());
        sixtyfps::interpreter::Struct struc;
        value = Value(struc);
        REQUIRE(value.type() == Value::Type::Struct);

        auto struct_opt = value.to_struct();
        REQUIRE(struct_opt.has_value());
    }

    SECTION("Construct a model")
    {
        // And test that it is properly destroyed when the value is destroyed
        struct M : sixtyfps::VectorModel<Value>
        {
            bool *destroyed;
            explicit M(bool *destroyed) : destroyed(destroyed) { }
            void play()
            {
                this->push_back(Value(4.));
                this->set_row_data(0, Value(9.));
            }
            ~M() { *destroyed = true; }
        };
        bool destroyed = false;
        auto m = std::make_shared<M>(&destroyed);
        {
            Value value(m);
            REQUIRE(value.type() == Value::Type::Model);
            REQUIRE(!destroyed);
            m->play();
            m = nullptr;
            REQUIRE(!destroyed);
            // play a bit with the value to test the copy and move
            Value v2 = value;
            Value v3 = std::move(v2);
            REQUIRE(!destroyed);
        }
        REQUIRE(destroyed);
    }

    SECTION("Compare Values")
    {
        Value str1 { sixtyfps::SharedString("Hello1") };
        Value str2 { sixtyfps::SharedString("Hello2") };
        Value fl1 { 10. };
        Value fl2 { 12. };

        REQUIRE(str1 == str1);
        REQUIRE(str1 != str2);
        REQUIRE(str1 != fl2);
        REQUIRE(fl1 == fl1);
        REQUIRE(fl1 != fl2);
        REQUIRE(Value() == Value());
        REQUIRE(Value() != str1);
        REQUIRE(str1 == sixtyfps::SharedString("Hello1"));
        REQUIRE(str1 != sixtyfps::SharedString("Hello2"));
        REQUIRE(sixtyfps::SharedString("Hello2") == str2);
        REQUIRE(fl1 != sixtyfps::SharedString("Hello2"));
        REQUIRE(fl2 == 12.);
    }
}

SCENARIO("Struct API")
{
    using namespace sixtyfps::interpreter;
    Struct struc;

    REQUIRE(!struc.get_field("not_there"));

    struc.set_field("field_a", Value(sixtyfps::SharedString("Hallo")));

    auto value_opt = struc.get_field("field_a");
    REQUIRE(value_opt.has_value());
    auto value = value_opt.value();
    REQUIRE(value.to_string().has_value());
    REQUIRE(value.to_string().value() == "Hallo");

    int count = 0;
    for (auto [k, value] : struc) {
        REQUIRE(count == 0);
        count++;
        REQUIRE(k == "field_a");
        REQUIRE(value.to_string().value() == "Hallo");
    }

    struc.set_field("field_b", Value(sixtyfps::SharedString("World")));
    std::map<std::string, sixtyfps::SharedString> map;
    for (auto [k, value] : struc)
        map[std::string(k)] = *value.to_string();

    REQUIRE(map
            == std::map<std::string, sixtyfps::SharedString> {
                    { "field_a", sixtyfps::SharedString("Hallo") },
                    { "field_b", sixtyfps::SharedString("World") } });
}

SCENARIO("Struct Iterator Constructor")
{
    using namespace sixtyfps::interpreter;

    std::vector<std::pair<std::string_view, Value>> values = { { "field_a", Value(true) },
                                                               { "field_b", Value(42.0) } };

    Struct struc(values.begin(), values.end());

    REQUIRE(!struc.get_field("foo").has_value());
    REQUIRE(struc.get_field("field_a").has_value());
    REQUIRE(struc.get_field("field_a").value().to_bool().value());
    REQUIRE(struc.get_field("field_b").value().to_number().value() == 42.0);
}

SCENARIO("Struct Initializer List Constructor")
{
    using namespace sixtyfps::interpreter;

    Struct struc({ { "field_a", Value(true) }, { "field_b", Value(42.0) } });

    REQUIRE(!struc.get_field("foo").has_value());
    REQUIRE(struc.get_field("field_a").has_value());
    REQUIRE(struc.get_field("field_a").value().to_bool().value());
    REQUIRE(struc.get_field("field_b").value().to_number().value() == 42.0);
}

SCENARIO("Struct empty field iteration")
{
    using namespace sixtyfps::interpreter;
    Struct struc;
    REQUIRE(struc.begin() == struc.end());
}

SCENARIO("Struct field iteration")
{
    using namespace sixtyfps::interpreter;

    Struct struc({ { "field_a", Value(true) }, { "field_b", Value(42.0) } });

    auto it = struc.begin();
    auto end = struc.end();
    REQUIRE(it != end);

    auto check_valid_entry = [](const auto &key, const auto &value) -> bool {
        if (key == "field_a")
            return value == Value(true);
        if (key == "field_b")
            return value == Value(42.0);
        return false;
    };

    std::set<std::string> seen_fields;

    for (; it != end; ++it) {
        const auto [key, value] = *it;
        REQUIRE(check_valid_entry(key, value));
        auto [insert_it, value_inserted] = seen_fields.insert(std::string(key));
        REQUIRE(value_inserted);
    }
}

SCENARIO("Component Compiler")
{
    using namespace sixtyfps::interpreter;
    using namespace sixtyfps;

    ComponentCompiler compiler;

    SECTION("configure include paths")
    {
        SharedVector<SharedString> in_paths;
        in_paths.push_back("path1");
        in_paths.push_back("path2");
        compiler.set_include_paths(in_paths);

        auto out_paths = compiler.include_paths();
        REQUIRE(out_paths.size() == 2);
        REQUIRE(out_paths[0] == "path1");
        REQUIRE(out_paths[1] == "path2");
    }

    SECTION("configure style")
    {
        REQUIRE(compiler.style() == "");
        compiler.set_style("ugly");
        REQUIRE(compiler.style() == "ugly");
    }

    SECTION("Compile failure from source")
    {
        auto result = compiler.build_from_source("Syntax Error!!", "");
        REQUIRE_FALSE(result.has_value());
    }

    SECTION("Compile from source")
    {
        auto result = compiler.build_from_source("export Dummy := Rectangle {}", "");
        REQUIRE(result.has_value());
    }

    SECTION("Compile failure from path")
    {
        auto result = compiler.build_from_path(SOURCE_DIR "/file-not-there.60");
        REQUIRE_FALSE(result.has_value());
        auto diags = compiler.diagnostics();

        REQUIRE(diags.size() == 1);
        REQUIRE(diags[0].message.starts_with("Could not load"));
    }

    SECTION("Compile from path")
    {
        auto result = compiler.build_from_path(SOURCE_DIR "/tests/test.60");
        REQUIRE(result.has_value());
    }
}

SCENARIO("Component Definition Properties")
{
    using namespace sixtyfps::interpreter;
    using namespace sixtyfps;

    ComponentCompiler compiler;
    auto comp_def = *compiler.build_from_source(
            "export Dummy := Rectangle { property <string> test; callback dummy; }", "");
    auto properties = comp_def.properties();
    REQUIRE(properties.size() == 1);
    REQUIRE(properties[0].property_name == "test");
    REQUIRE(properties[0].property_type == Value::Type::String);

    auto callback_names = comp_def.callbacks();
    REQUIRE(callback_names.size() == 1);
    REQUIRE(callback_names[0] == "dummy");
}

SCENARIO("Component Definition Properties / Two-way bindings")
{
    using namespace sixtyfps::interpreter;
    using namespace sixtyfps;

    ComponentCompiler compiler;
    auto comp_def = *compiler.build_from_source(
            "export Dummy := Rectangle { property <string> test <=> sub_object.test; "
            "    sub_object := Rectangle { property <string> test; }"
            "}",
            "");
    auto properties = comp_def.properties();
    REQUIRE(properties.size() == 1);
    REQUIRE(properties[0].property_name == "test");
    REQUIRE(properties[0].property_type == Value::Type::String);
}

SCENARIO("Invoke callback")
{
    using namespace sixtyfps::interpreter;
    using namespace sixtyfps;

    ComponentCompiler compiler;

    SECTION("valid")
    {
        auto result = compiler.build_from_source(
                "export Dummy := Rectangle { callback foo(string, int) -> string; }", "");
        REQUIRE(result.has_value());
        auto instance = result->create();
        REQUIRE(instance->set_callback("foo", [](auto args) {
            SharedString arg1 = *args[0].to_string();
            int arg2 = int(*args[1].to_number());
            std::string res = std::string(arg1) + ":" + std::to_string(arg2);
            return Value(SharedString(res));
        }));
        Value args[] = { SharedString("Hello"), 42. };
        auto res = instance->invoke_callback("foo", Slice<Value> { args, 2 });
        REQUIRE(res.has_value());
        REQUIRE(*res->to_string() == SharedString("Hello:42"));
    }

    SECTION("invalid")
    {
        auto result = compiler.build_from_source(
                "export Dummy := Rectangle { callback foo(string, int) -> string; }", "");
        REQUIRE(result.has_value());
        auto instance = result->create();
        REQUIRE(!instance->set_callback("bar", [](auto) { return Value(); }));
        Value args[] = { SharedString("Hello"), 42. };
        auto res = instance->invoke_callback("bar", Slice<Value> { args, 2 });
        REQUIRE(!res.has_value());
    }
}

SCENARIO("Array between .60 and C++")
{
    using namespace sixtyfps::interpreter;
    using namespace sixtyfps;

    ComponentCompiler compiler;

    auto result = compiler.build_from_source(
            "export Dummy := Rectangle { property <[int]> array: [1, 2, 3]; }", "");
    REQUIRE(result.has_value());
    auto instance = result->create();

    SECTION(".60 to C++")
    {
        auto maybe_array = instance->get_property("array");
        REQUIRE(maybe_array.has_value());
        REQUIRE(maybe_array->type() == Value::Type::Array);

        auto array = *maybe_array;
        REQUIRE(array == sixtyfps::SharedVector<Value> { Value(1.), Value(2.), Value(3.) });
    }

    SECTION("C++ to .60")
    {
        sixtyfps::SharedVector<Value> cpp_array { Value(4.), Value(5.), Value(6.) };

        instance->set_property("array", Value(cpp_array));
        auto maybe_array = instance->get_property("array");
        REQUIRE(maybe_array.has_value());
        REQUIRE(maybe_array->type() == Value::Type::Array);

        auto actual_array = *maybe_array;
        REQUIRE(actual_array == cpp_array);
    }
}

SCENARIO("Component Definition Name")
{
    using namespace sixtyfps::interpreter;
    using namespace sixtyfps;

    ComponentCompiler compiler;
    auto comp_def = *compiler.build_from_source("export IHaveAName := Rectangle { }", "");
    REQUIRE(comp_def.name() == "IHaveAName");
}

SCENARIO("Send key events")
{
    using namespace sixtyfps::interpreter;
    using namespace sixtyfps;

    ComponentCompiler compiler;
    auto comp_def = compiler.build_from_source(R"(
        export Dummy := Rectangle {
            forward-focus: scope;
            property <string> result;
            scope := FocusScope {
                key-pressed(event) => {
                    result += event.text;
                    return accept;
                }
            }
        }
    )",
                                               "");
    REQUIRE(comp_def.has_value());
    auto instance = comp_def->create();
    sixtyfps::testing::send_keyboard_string_sequence(&*instance, "Hello keys!", {});
    REQUIRE(*instance->get_property("result")->to_string() == "Hello keys!");
}
