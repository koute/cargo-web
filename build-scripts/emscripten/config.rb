#!/usr/bin/false

NAME = "emscripten"
VERSION = "1.37.21"
RELEASE = "1"

FILES = [
    [
        "https://github.com/kripken/emscripten/archive/#{VERSION}.tar.gz",
        "947035529633de60fedc5997c58acf3f87b1edafc583df3920f5028068fa7971",
        "emscripten-#{VERSION}.tgz"
    ],
    [
        "https://github.com/kripken/emscripten-fastcomp/archive/#{VERSION}.tar.gz",
        "c575314d426080449c349b1c02b21cc1428b2391313af603fc7d1d167c654c81",
        "emscripten_fastcomp-#{VERSION}.tgz"
    ],
    [
        "https://github.com/kripken/emscripten-fastcomp-clang/archive/#{VERSION}.tar.gz",
        "93cefc9e968c6a2fa45da2618a3c3de3303dfe3001fb36afa67d5fae968f3081",
        "emscripten_fastcomp_clang-#{VERSION}.tgz"
    ]
]

INSTALL_CMAKE = true
