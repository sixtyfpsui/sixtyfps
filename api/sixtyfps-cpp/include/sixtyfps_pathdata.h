/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
#pragma once
#include <initializer_list>
#include <string_view>
#include "sixtyfps_pathdata_internal.h"

namespace sixtyfps::private_api {

using cbindgen_private::types::PathArcTo;
using cbindgen_private::types::PathCubicTo;
using cbindgen_private::types::PathElement;
using cbindgen_private::types::PathEvent;
using cbindgen_private::types::PathLineTo;
using cbindgen_private::types::PathMoveTo;
using cbindgen_private::types::PathQuadraticTo;
using cbindgen_private::types::Point;

struct PathData
{
public:
    using Tag = cbindgen_private::types::PathData::Tag;

    PathData() : data(Data::None()) { }
    PathData(const PathElement *firstElement, size_t count)
        : data(Data::Elements(elements_from_array(firstElement, count)))
    {
    }

    PathData(const PathEvent *firstEvent, size_t event_count, const Point *firstCoordinate,
             size_t coordinate_count)
        : data(events_from_array(firstEvent, event_count, firstCoordinate, coordinate_count))
    {
    }

    friend bool operator==(const PathData &a, const PathData &b)
    {
        if (a.data.tag != b.data.tag)
            return false;
        switch (a.data.tag) {
        case cbindgen_private::types::PathData::Tag::Elements:
            return a.data.elements._0 == b.data.elements._0;
        case cbindgen_private::types::PathData::Tag::Events:
            return a.data.events._0 == b.data.events._0 && b.data.events._0 == b.data.events._0;
        case cbindgen_private::types::PathData::Tag::None:
            return true;
        }
        return false; // unreachable
    }
    friend bool operator!=(const PathData &a, const PathData &b) { return !(a == b); }

private:
    static SharedVector<PathElement> elements_from_array(const PathElement *firstElement,
                                                         size_t count)
    {
        SharedVector<PathElement> tmp;
        sixtyfps_new_path_elements(&tmp, firstElement, count);
        return tmp;
    }

    static cbindgen_private::types::PathData events_from_array(const PathEvent *firstEvent,
                                                               size_t event_count,
                                                               const Point *firstCoordinate,
                                                               size_t coordinate_count)
    {
        SharedVector<PathEvent> events;
        SharedVector<Point> coordinates;
        sixtyfps_new_path_events(&events, &coordinates, firstEvent, event_count, firstCoordinate,
                                 coordinate_count);
        return Data::Events(events, coordinates);
    }

    using Data = cbindgen_private::types::PathData;
    Data data;
};

}
