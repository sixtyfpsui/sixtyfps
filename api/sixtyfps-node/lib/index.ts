/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

// Load the native library with `process.dlopen` instead of with `require`.
// This is only done for autotest that do not require nom or neon_cli to
// copy the lib to its right place
/**
 * @hidden
 */
function load_native_lib() {
    const os = require('os');
    (process as any).dlopen(module, process.env.SIXTYFPS_NODE_NATIVE_LIB,
        os.constants.dlopen.RTLD_NOW);
    return module.exports;
}

/**
 * @hidden
 */
let native = !process.env.SIXTYFPS_NODE_NATIVE_LIB ? require('../native/index.node') : load_native_lib();

/**
 * @hidden
 */
class Component {
    protected comp: any;

    constructor(comp: any) {
        this.comp = comp;
    }

    run() {
        this.comp.run();
    }

    show() {
        this.comp.show();
    }

    hide() {
        this.comp.hide();
    }

    send_mouse_click(x: number, y: number) {
        this.comp.send_mouse_click(x, y)
    }

    send_keyboard_string_sequence(s: String) {
        this.comp.send_keyboard_string_sequence(s)
    }
}

/**
 * @hidden
 */
interface Callback {
    (): any;
    setHandler(cb: any): void;
}

require.extensions['.60'] =
    function (module, filename) {
        var c = native.load(filename);
        module.exports[c.name()] = function (init_properties: any) {
            let comp = c.create(init_properties);
            let ret = new Component(comp);
            c.properties().forEach((x: string) => {
                Object.defineProperty(ret, x, {
                    get() { return comp.get_property(x); },
                    set(newValue) { comp.set_property(x, newValue); },
                    enumerable: true,
                })
            });
            c.callbacks().forEach((x: string) => {
                Object.defineProperty(ret, x, {
                    get() {
                        let callback = function () { return comp.invoke_callback(x, [...arguments]); } as Callback;
                        callback.setHandler = function (callback) { comp.connect_callback(x, callback) };
                        return callback;
                    },
                    enumerable: true,
                })
            });
            return ret;
        }
    }

/**
 * ModelPeer is the interface that the run-time implements. An instance is
 * set on dynamic Model<T> instances and can be used to notify the run-time
 * of changes in the structure or data of the model.
 */
interface ModelPeer {
    /**
     * Call this function from our own model to notify that fields of data
     * in the specified row have changed.
     * @argument row
     */
    rowDataChanged(row: number): void;
    /**
     * Call this function from your own model to notify that one or multiple
     * rows were added to the model, starting at the specified row.
     * @param row
     * @param count
     */
    rowAdded(row: number, count: number): void;
    /**
     * Call this function from your own model to notify that one or multiple
     * rows were removed from the model, starting at the specified row.
     * @param row
     * @param count
     */
    rowRemoved(row: number, count: number): void;
}

/**
 * Model<T> is the interface for feeding dynamic data into
 * `.60` views.
 *
 * A model is organized like a table with rows of data. The
 * fields of the data type T behave like columns.
 */
interface Model<T> {
    /**
     * Implementations of this function must return the current number of rows.
     */
    rowCount(): number;
    /**
     * Implementations of this function must return the data at the specified row.
     * @param row
     */
    rowData(row: number): T;
    /**
     * Implementations of this function must store the provided data parameter
     * in the model at the specified row.
     * @param row
     * @param data
     */
    setRowData(row: number, data: T): void;
    /**
     * This public member is set by the run-time and implementation must use this
     * to notify the run-time of changes in the model.
     */
    notify: ModelPeer;
}

/**
 * @hidden
 */
class NullPeer implements ModelPeer {
    rowDataChanged(row: number): void { }
    rowAdded(row: number, count: number): void { }
    rowRemoved(row: number, count: number): void { }
}

/**
 * ArrayModel wraps a JavaScript array for use in `.60` views. The underlying
 * array can be modified with the [[ArrayModel.push]] and [[ArrayModel.remove]] methods.
 */
class ArrayModel<T> implements Model<T> {
    /**
     * @hidden
     */
    private a: Array<T>
    notify: ModelPeer;

    /**
     * Creates a new ArrayModel.
     *
     * @param arr
     */
    constructor(arr: Array<T>) {
        this.a = arr;
        this.notify = new NullPeer();
    }

    rowCount() {
        return this.a.length;
    }
    rowData(row: number) {
        return this.a[row];
    }
    setRowData(row: number, data: T) {
        this.a[row] = data;
        this.notify.rowDataChanged(row);
    }
    /**
     * Pushes new values to the array that's backing the model and notifies
     * the run-time about the added rows.
     * @param values
     */
    push(...values: T[]) {
        let size = this.a.length;
        Array.prototype.push.apply(this.a, values);
        this.notify.rowAdded(size, arguments.length);
    }
    // FIXME: should this be named splice and have the splice api?
    /**
     * Removes the specified number of element from the array that's backing
     * the model, starting at the specified index. This is equivalent to calling
     * Array.slice() on the array and notifying the run-time about the removed
     * rows.
     * @param index
     * @param size
     */
    remove(index: number, size: number) {
        let r = this.a.splice(index, size);
        this.notify.rowRemoved(index, size);
    }

    get length(): number {
        return this.a.length;
    }

    values(): IterableIterator<T> {
        return this.a.values();
    }

    entries(): IterableIterator<[number, T]> {
        return this.a.entries()
    }
}

module.exports = {
    private_api: native,
    ArrayModel: ArrayModel,
    Timer: {
        singleShot: native.singleshot_timer,
    },
};
