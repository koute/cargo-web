#!/usr/bin/false

NAME = "emscripten"
VERSION = "1.37.26"
RELEASE = "1"

FILES = [
    [
        "https://github.com/kripken/emscripten/archive/#{VERSION}.tar.gz",
        "0ba9ed1b8e648957a3a055451850cb034c23a8373ed2b22e0a9ee4caf87c7cb0",
        "emscripten-#{VERSION}.tgz"
    ],
    [
        "https://github.com/kripken/emscripten-fastcomp/archive/#{VERSION}.tar.gz",
        "88ed2945ae466895330e8cdd5cc8f868ef72a3349e2ef603afc496dba469f65c",
        "emscripten_fastcomp-#{VERSION}.tgz"
    ],
    [
        "https://github.com/kripken/emscripten-fastcomp-clang/archive/#{VERSION}.tar.gz",
        "5784d1bab4b9b26cd2a9f5ed9476dcbc1dd1db1fc4823d43dd6eabb215afc648",
        "emscripten_fastcomp_clang-#{VERSION}.tgz"
    ]
]

INSTALL_CMAKE = true
