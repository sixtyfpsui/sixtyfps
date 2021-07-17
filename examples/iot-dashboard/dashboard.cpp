/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

#include "dashboard.h"

#include <fmt/core.h>

void Widget::set_property(std::string_view name, const sixtyfps::interpreter::Value &value)
{
    if (m_ui)
        (*m_ui)->set_property(qualified_property_name(name), value);
}

std::optional<sixtyfps::interpreter::Value> Widget::property(std::string_view name) const
{
    if (m_ui)
        return (*m_ui)->get_property(qualified_property_name(name));
    return {};
}

void Widget::connect_ui(
        const sixtyfps::ComponentHandle<sixtyfps::interpreter::ComponentInstance> &ui,
        std::string_view properties_prefix)
{
    m_ui = ui;
    m_properties_prefix = properties_prefix;
}

std::string Widget::qualified_property_name(std::string_view name) const
{
    std::string qname(m_properties_prefix);
    qname += name;
    return qname;
}

std::string WidgetLocation::location_bindings() const
{
    auto maybe_binding = [](std::string_view name, const auto &opt_value) -> std::string {
        if (opt_value.has_value()) {
            return fmt::format("               {}: {};\n", name, opt_value.value());
        } else {
            return "";
        }
    };

    return fmt::format(
            R"60(row: {};
               col: {};
{}{})60",
            row, column, maybe_binding("rowspan", row_span), maybe_binding("colspan", col_span));
}

void DashboardBuilder::add_grid_widget(WidgetPtr widget, const WidgetLocation &location)
{
    auto widget_id = register_widget(widget);
    grid_widgets.push_back({ widget_id, location });
}

void DashboardBuilder::add_top_bar_widget(WidgetPtr widget)
{
    auto widget_id = register_widget(widget);
    top_bar_widgets.push_back(widget_id);
}

int DashboardBuilder::register_widget(WidgetPtr widget)
{
    auto widget_type_name = widget->type_name();
    widgets_used.insert(widget_type_name);

    auto widget_id = int(widgets.size());
    auto widget_name = fmt::format("widget_{}", widget_id);
    widgets.push_back({ widget_name, widget });
    return widget_id;
}

std::optional<sixtyfps::ComponentHandle<sixtyfps::interpreter::ComponentInstance>>
DashboardBuilder::build(sixtyfps::interpreter::ComponentCompiler &compiler) const
{
    std::string widget_imports;

    for (const auto &widget : widgets_used) {
        if (widget_imports.size() > 0) {
            widget_imports.append(", ");
        }
        widget_imports.append(widget);
    }

    if (widget_imports.size() > 0) {
        widget_imports = fmt::format("import {{ {} }} from \"iot-dashboard.60\";", widget_imports);
    }

    // Vector of name/type_name of properties forwarded through the MainContent {} element.
    std::string main_content_properties;
    std::string main_grid;
    std::string top_bar;
    std::string exposed_properties;

    for (const auto &[widget_id, location] : grid_widgets) {
        const auto &[widget_name, widget_ptr] = widgets[widget_id];

        main_grid.append(fmt::format(
                R"60(
            {0} := {1} {{
                {2}
            }}
        )60",
                widget_name, widget_ptr->type_name(), location.location_bindings()));

        std::string properties_prefix = widget_name;
        properties_prefix.append("__");

        for (const auto &property : widget_ptr->properties()) {
            std::string forwarded_property_name = properties_prefix;
            forwarded_property_name.append(property.name);

            main_content_properties.append(fmt::format("    property <{0}> {1} <=> {2}.{3};\n",
                                                       property.type_name, forwarded_property_name,
                                                       widget_name, property.name));

            exposed_properties.append(fmt::format("    property <{0}> {1} <=> main_content.{1};\n",
                                                  property.type_name, forwarded_property_name));
        }
    }

    for (const auto widget_id : top_bar_widgets) {
        const auto &[widget_name, widget_ptr] = widgets[widget_id];

        top_bar.append(fmt::format(
                R"60(
            {0} := {1} {{
            }}
        )60",
                widget_name, widget_ptr->type_name()));

        std::string properties_prefix = widget_name;
        properties_prefix.append("__");

        for (const auto &property : widget_ptr->properties()) {
            std::string forwarded_property_name = properties_prefix;
            forwarded_property_name.append(property.name);

            exposed_properties.append(fmt::format("    property <{0}> {1} <=> {2}.{3};\n",
                                                  property.type_name, forwarded_property_name,
                                                  widget_name, property.name));
        }
    }

    auto source_code = fmt::format(
            R"60(

{0}

MainContent := VerticalLayout {{
{4}

    spacing: 24px;
    TopBar {{
        @children
    }}

    GridLayout {{
        spacing: 6px;
        padding-left: 19px;
        padding-top: 0px;
        padding-right: 17px;
        padding-bottom: 24px;

        {2}
    }}
}}

MainWindow := Window {{
    title: "IOT dashboard";

{3}

    HorizontalLayout {{
        padding: 0; spacing: 0;
        MenuBar {{
        }}
        main_content := MainContent {{
            {1}
        }}
    }}
}}
)60",
            widget_imports, top_bar, main_grid, exposed_properties, main_content_properties);

    auto definition = compiler.build_from_source(source_code, SOURCE_DIR);

    for (auto diagnostic : compiler.diagnostics()) {
        std::cerr << (diagnostic.level == sixtyfps::interpreter::DiagnosticLevel::Warning
                              ? "warning: "
                              : "error: ")
                  << diagnostic.message << std::endl;
        std::cerr << "location: " << diagnostic.source_file;
        if (diagnostic.line > 0)
            std::cerr << ":" << diagnostic.line;
        if (diagnostic.column > 0)
            std::cerr << ":" << diagnostic.column;
        std::cerr << std::endl;
    }

    if (!definition) {
        std::cerr << "compilation failure!" << std::endl;
        std::cerr << "generated source:" << std::endl << source_code << std::endl;
        return {};
    }

    // std::cerr << source_code << std::endl;

    auto ui = definition->create();

    for (const auto &entry : widgets) {
        auto [widget_name, widget_ptr] = entry;

        std::string properties_prefix = widget_name;
        properties_prefix += "__";

        widget_ptr->connect_ui(ui, properties_prefix);
    }

    return ui;
}
