/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
import * as monaco from 'monaco-editor';


export const sixtyfps_language = <monaco.languages.IMonarchLanguage>{
    defaultToken: 'invalid',

    root_keywords: [
        'import', 'from', 'export'
    ],
    inner_keywords: [
        'property', 'callback', 'animate', 'states', 'transitions', 'if', 'for'
    ],
    lang_keywords: [
        'root', 'parent', 'this', 'if'
    ],
    type_keywords: ['int', 'string', 'float', 'length', 'physical_length', 'duration'],
    escapes: /\\(?:[abfnrtv\\"']|x[0-9A-Fa-f]{1,4}|u[0-9A-Fa-f]{4}|U[0-9A-Fa-f]{8})/,

    symbols: /[\#\!\%\&\*\+\-\.\/\:\;\<\=\>\@\^\|_\?,()]+/,

    tokenizer: {
        root: [
            [/[a-zA-Z_][a-zA-Z0-9_]*/, {
                cases: {
                    '@root_keywords': { token: 'keyword' },
                    '@default': 'identifier'
                }
            }],
            { include: '@whitespace' },
            { include: '@numbers' },
            [/"/, 'string', '@string'],
            [/\{/, '', '@inner'],
            [/@symbols/, ''],
        ],
        inner: [
            [/[a-zA-Z_][a-zA-Z0-9_\-]*\s*:=/, 'variable.parameter'],
            [/[a-zA-Z_][a-zA-Z0-9_\-]*\s*:\s*\{/, 'variable.parameter', '@binding_1'],
            [/[a-zA-Z_][a-zA-Z0-9_\-]*\s*:/, 'variable.parameter', '@binding_0'],
            [/[a-zA-Z_][a-zA-Z0-9_\-]*/, {
                cases: {
                    '@inner_keywords': { token: 'keyword' },
                    '@default': 'identifier'
                }
            }],
            { include: '@whitespace' },
            { include: '@numbers' },
            [/"/, 'string', '@string'],
            [/\{/, '', '@push'],
            [/\}/, '', '@pop'],
            [/:=/, ''],
            [/<=>/, '', '@binding_0'],
            [/=>\s*{/, '', '@binding_1'],
            [/\</, '', '@type'],
            [/\[/, '', 'binding_1'],
            [/@symbols/, ''],
        ],


        type: [
            [/[a-zA-Z_][a-zA-Z0-9_]*/, {
                cases: {
                    '@type_keywords': { token: 'keyword.type' },
                    '@default': 'identifier'
                }
            }],
            { include: '@whitespace' },
            { include: '@numbers' },
            [/"/, 'string', '@string'],
            [/\</, '', '@push'],
            [/\>/, '', '@pop'],
            [/@symbols/, ''],
        ],

        binding_0: [
            { include: '@whitespace' },
            [/\{/, '', '@binding_1'],
            [/;/, '', '@pop'],
            // that should not be needed, but ends recovering after a for
            [/\}/, '', '@pop'],
            [/[a-zA-Z_][a-zA-Z0-9_]*/, {
                cases: {
                    '@lang_keywords': { token: 'keyword.type' },
                    '@default': 'identifier'
                }
            }],
            { include: '@numbers' },
            [/"/, 'string', '@string'],
            [/@symbols/, ''],
        ],

        // inside a '{'
        binding_1: [
            [/[a-zA-Z_][a-zA-Z0-9_]*/, {
                cases: {
                    '@lang_keywords': { token: 'keyword.type' },
                    '@default': 'identifier'
                }
            }],
            { include: '@whitespace' },
            { include: '@numbers' },
            [/"/, 'string', '@string'],
            [/\{/, '', '@push'],
            [/\}/, '', '@pop'],
            [/\[/, '', '@push'],
            [/\]/, '', '@pop'],
            [/@symbols/, ''],
        ],

        whitespace: [
            [/[ \t\r\n]+/, 'white'],
            [/\/\*/, 'comment', '@comment'],
            [/\/\/.*$/, 'comment']
        ],
        string: [
            [/[^\\"]+/, 'string'],
            [/@escapes/, 'string.escape'],
            [/\\./, 'string.escape.invalid'],
            [/"/, 'string', '@pop']
        ],
        comment: [
            [/[^\/*]+/, 'comment'],
            [/\/\*/, 'comment', '@push'],
            ['\\*/', 'comment', '@pop'],
            [/[\/*]/, 'comment']
        ],

        numbers: [
            [/\d+(\.\d+)?\w*/, { token: 'number' }],
            [/#[0-9a-fA-F]+/, { token: 'number' }],
        ]
    },
}