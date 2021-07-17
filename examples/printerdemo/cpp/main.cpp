/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
#include "printerdemo.h"

#include <ctime>

struct InkLevelModel : sixtyfps::Model<InkLevel>
{
    int row_count() const override { return m_data.size(); }
    InkLevel row_data(int i) const override { return m_data[i]; }

    std::vector<InkLevel> m_data = { { sixtyfps::Color::from_rgb_uint8(255, 255, 0), 0.9 },
                                     { sixtyfps::Color::from_rgb_uint8(0, 255, 255), 0.5 },
                                     { sixtyfps::Color::from_rgb_uint8(255, 0, 255), 0.8 },
                                     { sixtyfps::Color::from_rgb_uint8(0, 0, 0), 0.1 } };
};

int main()
{
    auto printer_demo = MainWindow::create();
    printer_demo->set_ink_levels(std::make_shared<InkLevelModel>());
    printer_demo->on_quit([] { std::exit(0); });


    auto printer_queue = std::make_shared<sixtyfps::VectorModel<PrinterQueueItem>>();
    auto default_queue = printer_demo->get_printer_queue();
    for (int i = 0; i < default_queue->row_count(); ++i) {
        printer_queue->push_back(default_queue->row_data(i));
    }
    printer_demo->set_printer_queue(printer_queue);

    printer_demo->on_start_job([=](sixtyfps::SharedString name) {
        std::time_t now = std::chrono::system_clock::to_time_t(std::chrono::system_clock::now());
        char time_buf[100] = { 0 };
        std::strftime(time_buf, sizeof(time_buf), "%H:%M:%S %d/%m/%Y", std::localtime(&now));
        PrinterQueueItem item;
        item.status = "WAITING...";
        item.progress = 0;
        item.title = std::move(name);
        item.owner = "joe@example.com";
        item.pages = 1;
        item.size = "100kB";
        item.submission_date = time_buf;
        printer_queue->push_back(item);
    });

    printer_demo->on_cancel_job([=](int index) {
        printer_queue->erase(int(index));
    });

    sixtyfps::Timer printer_queue_progress_timer(std::chrono::seconds(1), [=]() {
        if (printer_queue->row_count() > 0) {
            auto top_item = printer_queue->row_data(0);
            top_item.progress += 1;
            if (top_item.progress > 100) {
                printer_queue->erase(0);
            } else {
                top_item.status = "PRINTING";
                printer_queue->set_row_data(0, top_item);
            }
        }
    });

    printer_demo->run();
}
