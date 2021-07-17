/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
#pragma once

#include <sixtyfps_interpreter.h>

#include <optional>
#include <string_view>
#include <vector>
#include <unordered_set>

struct PropertyDeclaration
{
    std::string name;
    std::string type_name;
};

/**
   The Widget base class is a wrapper around sixtyfps::interpreter::ComponentInstance that allows
   conveniently reading and writing properties of an element of which the properties have been
   forwarded via two-way bindings.

   When an instance of a Widget sub-class is added to the DashboardBuilder, the value of
   type_name() is used to create an element declaration in the generated .60 code ("SomeElement {
   ... }"), the element is given an automatically generated name and all properties returned by the
   properties() function are forwarded. For example two instances of a "Clock" element become this
   in .60:

   export MainWindow := Window {
       ...
       widget_1 := Clock {
       }
       widget_2 := Clock {
       }

       property <string> widget_1__time <=> widget_1.time;
       property <string> widget_2__time <=> widget_2.time;
   }

   The DashboardBuilder calls connect_ui() to inform the instance about the "widget_1__" and
   "widget_2__" prefix and passes a reference to the MainWindow as ComponentInstance. Subsequently
   calls to set_property("time", some_value) translate to setting "widget_1__time" or
   "widget_2__time", depending on the Widget instance.
 */
class Widget
{
public:
    virtual ~Widget() { }
    virtual std::string type_name() const = 0;
    virtual std::vector<PropertyDeclaration> properties() const = 0;

    void set_property(std::string_view name, const sixtyfps::interpreter::Value &value);

    std::optional<sixtyfps::interpreter::Value> property(std::string_view name) const;

    void connect_ui(const sixtyfps::ComponentHandle<sixtyfps::interpreter::ComponentInstance> &ui,
                    std::string_view properties_prefix);

    std::pair<std::string, std::vector<PropertyDeclaration>>
    generate_forwarding_two_way_property_bindings(std::string_view widget_name) const;

private:
    std::string qualified_property_name(std::string_view name) const;

    std::optional<sixtyfps::ComponentHandle<sixtyfps::interpreter::ComponentInstance>> m_ui;
    std::string m_properties_prefix;
};

using WidgetPtr = std::shared_ptr<Widget>;

struct WidgetLocation
{
    int row = 0;
    int column = 0;
    std::optional<int> row_span;
    std::optional<int> col_span;

    std::string location_bindings() const;
};

/**
   The DashboardBuilder is dynamically builds the .60 code that represents the IOT-Dashboard demo
   and allows placing widgets into the top-bar or the main grid. All the properties of the added
   widgets are forwarded and their name prefix is registered with the individual widget instances.
*/
struct DashboardBuilder
{
    void add_grid_widget(WidgetPtr widget, const WidgetLocation &location);
    void add_top_bar_widget(WidgetPtr widget);

    std::optional<sixtyfps::ComponentHandle<sixtyfps::interpreter::ComponentInstance>>
    build(sixtyfps::interpreter::ComponentCompiler &compiler) const;

private:
    int register_widget(WidgetPtr widget);

    std::unordered_set<std::string> widgets_used = { "TopBar", "MenuBar" };
    std::vector<int> top_bar_widgets;
    std::vector<std::pair<int, WidgetLocation>> grid_widgets;

    std::vector<std::pair<std::string, WidgetPtr>> widgets;
};
