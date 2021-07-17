.. This file is part of the SixtyFPS Project -- https://sixtyfps.io
.. Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
.. Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>
..
.. LICENSE BEGIN
..   SPDX-License-Identifier: GPL-3.0-only
..   This file is also available under commercial licensing terms.
..   Please contact info@sixtyfps.io for more information.
.. LICENSE END
.. SixtyFPS C++ documentation master file

Welcome to SixtyFPS C++'s documentation!
========================================

.. toctree::
   :maxdepth: 2
   :hidden:
   :caption: Getting Started

   cmake.md

   First Steps <getting_started.md>

.. toctree::
   :maxdepth: 2
   :hidden:
   :caption: Reference

   api/library_root

   genindex

   language.rst

.. toctree::
   :maxdepth: 2
   :hidden:
   :caption: C++ / .60 Integration

   Type Mapping to C++ <types.md>

   Example Generated Code <generated_code.md>

.. image:: https://github.com/sixtyfpsui/sixtyfps/workflows/CI/badge.svg
   :target: https://github.com/sixtyfpsui/sixtyfps/actions
   :alt: GitHub CI Build Status

.. image:: https://img.shields.io/github/discussions/sixtyfpsui/sixtyfps
   :target: https://github.com/sixtyfpsui/sixtyfps/discussions
   :alt: GitHub Discussions

`SixtyFPS <https://sixtyfps.io/>`_ is a toolkit to efficiently develop fluid graphical user interfaces for any display: embedded devices and desktop applications.
SixtyFPS C++ is the C++ API to interact with a SixtyFPS UI from C++.

The .60 Markup Language
=======================

SixtyFPS comes with a markup language that is specifically designed for user interfaces. This language provides a
powerful way to describe graphical elements, their placement, and the flow of data through the different states. It is a familiar syntax to describe the hierarchy
of elements and property bindings. Here's the obligatory "Hello World":

.. code-block:: 60-no-preview

    HelloWorld := Window {
        width: 400px;
        height: 400px;

        Text {
           y: parent.width / 2;
           x: parent.x + 200px;
           text: "Hello, world";
           color: blue;
        }
    }

Check out the `language reference <markdown/langref.html>`_ for more details.

Architecture
============

An application is composed of the business logic written in C++ and the `.60` user interface design markup, which
is compiled to native code.

.. image:: https://sixtyfps.io/resources/architecture.drawio.svg
  :alt: Architecture Overview

Developing
==========

You can create and edit `.60` files using our `SixtyFPS Visual Studio Code Extension <https://marketplace.visualstudio.com/items?itemName=SixtyFPS.sixtyfps-vscode>`_,
which features syntax highlighting and live design preview.

For a quick edit and preview cycle, you can also use the :code:`sixtyfps-viewer` command line tool, which can be installed using :code:`cargo install sixtyfps-viewer`,
if you have `Cargo <https://marketplace.visualstudio.com/items?itemName=SixtyFPS.sixtyfps-vscode>`_ installed.

In the next section you will learn how to install the SixtyFPS C++ library and the CMake build system integration.

