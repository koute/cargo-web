#!/usr/bin/false
# This build script is based on the PKGBUILD from Arch Linux.

run "tar -xf emscripten-#{VERSION}.tgz"
run "tar -xf emscripten_fastcomp-#{VERSION}.tgz"
run "tar -xf emscripten_fastcomp_clang-#{VERSION}.tgz"

run "mv emscripten-#{VERSION} emscripten"
run "mv emscripten-fastcomp-#{VERSION} emscripten-fastcomp"
run "mv emscripten-fastcomp-clang-#{VERSION} emscripten-fastcomp-clang"

ln_s "../../emscripten-fastcomp-clang", "emscripten-fastcomp/tools/clang"

chdir( "emscripten-fastcomp" ) do
    mkdir_p "build"
    chdir( "build" ) do
        run %{
            cmake ..
                -DPYTHON_EXECUTABLE=/usr/bin/python2
                -DCMAKE_BUILD_TYPE=Release
                -DCMAKE_SKIP_RPATH=YES
                -DCMAKE_C_FLAGS="#{EXTRA_CFLAGS}"
                -DCMAKE_CXX_FLAGS="#{EXTRA_CFLAGS}"
                -DLLVM_TARGETS_TO_BUILD="X86;JSBackend"
                -DLLVM_BUILD_RUNTIME=OFF
                -DLLVM_INCLUDE_EXAMPLES=OFF
                -DLLVM_INCLUDE_TESTS=OFF
                -DLLVM_ENABLE_TERMINFO=OFF
                -DLLVM_ENABLE_LIBEDIT=OFF
                -DCLANG_INCLUDE_TESTS=OFF
        }

        run "nice -n 20 make -j #{CPU_COUNT}"
    end
end

mkdir_p "#{DESTDIR}/emscripten-fastcomp"
run "cp -rup emscripten-fastcomp/build/bin/* #{DESTDIR}/emscripten-fastcomp"
run "chmod 0755 #{DESTDIR}/emscripten-fastcomp/*"
run "strip #{DESTDIR}/emscripten-fastcomp/* || true"
run "install -m644 emscripten-fastcomp/emscripten-version.txt #{DESTDIR}/emscripten-fastcomp/emscripten-version.txt"
run "rm -Rf #{DESTDIR}/emscripten-fastcomp/*-test"
run "rm -Rf #{DESTDIR}/emscripten-fastcomp/llvm-lit"

run "install -d #{DESTDIR}/emscripten"
run %{
    cp -rup
        emscripten/em*
        emscripten/cmake
        emscripten/src
        emscripten/system
        emscripten/third_party
        emscripten/tools
        #{DESTDIR}/emscripten
}

run "install -m644 emscripten/LICENSE #{DESTDIR}"
