#!/usr/bin/false

NAME = "emscripten"
VERSION = "1.38.11"
RELEASE = "1"

FILES = [
    [
        "https://github.com/kripken/emscripten/archive/#{VERSION}.tar.gz",
        "5521e8eefbee284b6a72797c7f63ce606d37647930cd8f4d48d45d02c4e1da95",
        "emscripten-#{VERSION}.tgz"
    ],
    [
        "https://github.com/kripken/emscripten-fastcomp/archive/#{VERSION}.tar.gz",
        "55ddc1b1f045a36ac34ab60bb0e1a0370a40249eba8d41cd4e427be95beead18",
        "emscripten_fastcomp-#{VERSION}.tgz"
    ],
    [
        "https://github.com/kripken/emscripten-fastcomp-clang/archive/#{VERSION}.tar.gz",
        "1d2ac9f8dab54f0f17e4a77c3cd4653fe9f890831ef6e405320850fd7351f795",
        "emscripten_fastcomp_clang-#{VERSION}.tgz"
    ]
]

INSTALL_CMAKE = true
