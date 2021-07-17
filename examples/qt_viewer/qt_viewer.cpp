/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

#include <QtWidgets/QtWidgets>
#include <sixtyfps_interpreter.h>

#include "ui_interface.h"


struct LoadedFile {
    sixtyfps::ComponentHandle<sixtyfps::interpreter::ComponentInstance> instance;
    QWidget *widget;
};

void show_diagnostics(QWidget *root, const sixtyfps::SharedVector< sixtyfps::interpreter::Diagnostic > &diags) {
    QString text;

    for (auto diagnostic : diags) {
        text += (diagnostic.level == sixtyfps::interpreter::DiagnosticLevel::Warning
                              ? QApplication::translate("qt_viewer", "warning: %1\n")
                              : QApplication::translate("qt_viewer", "error: %1\n")
                 ).arg(QString::fromUtf8(diagnostic.message.data()));

        text += QApplication::translate("qt_viewer", "location: %1").arg(QString::fromUtf8(diagnostic.source_file.data()));
        if (diagnostic.line > 0)
            text += ":" + QString::number(diagnostic.line);
        if (diagnostic.column > 0)
            text += ":" + QString::number(diagnostic.column);
        text += "\n";
    }

    QMessageBox::critical(root, QApplication::translate("qt_viewer", "Compilation error"), text, QMessageBox::StandardButton::Ok);
}

int main(int argc, char **argv) {
    QApplication app(argc, argv);
    std::unique_ptr<LoadedFile> loaded_file;

    QWidget main;
    Ui::Interface ui;
    ui.setupUi(&main);
    QHBoxLayout layout(ui.my_content);

    sixtyfps::interpreter::Value::Type currentType;

    auto load_file = [&](const QString &fileName) {
        loaded_file.reset();
        sixtyfps::interpreter::ComponentCompiler compiler;
        auto def = compiler.build_from_path(fileName.toUtf8().data());
        if (!def) {
            show_diagnostics(&main, compiler.diagnostics());
            return;
        }
        auto instance = def->create();
        QWidget *wid = instance->qwidget();
        if (!wid) {
            QMessageBox::critical(&main, QApplication::translate("qt_viewer", "No Qt backend"),
                QApplication::translate("qt_viewer", "SixtyFPS is not running with the Qt backend."),
                QMessageBox::StandardButton::Ok);
            return;
        }
        wid->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Expanding);
        layout.addWidget(wid);
        loaded_file = std::make_unique<LoadedFile>(LoadedFile{ instance, wid });
    };

    auto args = app.arguments();
    if (args.count() == 2) {
        load_file(args.at(1));
    }

    QObject::connect(ui.load_button, &QPushButton::clicked, [&] {
        QString fileName = QFileDialog::getOpenFileName(
            &main, QApplication::translate("qt_viewer", "Open SixtyFPS File"), {},
            QApplication::translate("qt_viewer", "SixtyFPS File (*.60)"));
        if (fileName.isEmpty())
            return;
        load_file(fileName);
    });

    QObject::connect(ui.prop_name, &QLineEdit::textChanged, [&] {
        if (!loaded_file) return;
        if (auto val = loaded_file->instance->get_property(ui.prop_name->text().toUtf8().data())) {
            currentType = val->type();
            switch (currentType) {
                case sixtyfps::interpreter::Value::Type::String:
                    ui.prop_value->setText(QString::fromUtf8(val->to_string()->data()));
                    break;

                case sixtyfps::interpreter::Value::Type::Number:
                    ui.prop_value->setText(QString::number(val->to_number().value()));
                    break;

                default:
                    ui.prop_value->clear();
                    break;
            }
        }
    });

    QObject::connect(ui.set_button, &QPushButton::clicked, [&] {
        if (!loaded_file) return;
        sixtyfps::interpreter::Value val;
        switch (currentType) {
        case sixtyfps::interpreter::Value::Type::String:
            val = sixtyfps::SharedString(ui.prop_value->text().toUtf8().data());
            break;
        case sixtyfps::interpreter::Value::Type::Number: {
            bool ok;
            val = ui.prop_value->text().toDouble(&ok);
            if (!ok) {
                QMessageBox::critical(&main, QApplication::translate("qt_viewer", "Set Property Error"),
                    QApplication::translate("qt_viewer", "Invalid conversion to number"), QMessageBox::StandardButton::Ok);
                return;
            }
            break;
        }
        default:
            QMessageBox::critical(&main, QApplication::translate("qt_viewer", "Set Property Error"),
                QApplication::translate("qt_viewer", "Cannot set properties of this type"), QMessageBox::StandardButton::Ok);
            return;
        }
        if (!loaded_file->instance->set_property(ui.prop_name->text().toUtf8().data(), val)) {
            QMessageBox::critical(&main, QApplication::translate("qt_viewer", "Set Property Error"),
                QApplication::translate("qt_viewer", "Could not set property"), QMessageBox::StandardButton::Ok);
        }
    });

    main.show();
    return app.exec();
}

