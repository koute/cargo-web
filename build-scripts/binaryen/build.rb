#!/usr/bin/false

run "tar -xf binaryen-#{VERSION}.tgz"
run "mv binaryen-#{VERSION} binaryen"

run "install -d #{DESTDIR}/binaryen"
run "ln -sf /usr/bin/python3 #{TMP_ROOT}/usr/local/bin/python"

chdir( "binaryen" ) do
    run "sed -i 's/-Werror/-Wno-error/g' CMakeLists.txt"

    mkdir_p "build"
    chdir( "build" ) do
        run %{
            cmake ..
                -DCMAKE_INSTALL_PREFIX:PATH=""
                -DCMAKE_BUILD_TYPE=Release
                -DCMAKE_C_FLAGS="#{EXTRA_CFLAGS}"
                -DCMAKE_CXX_FLAGS="#{EXTRA_CFLAGS}"
        }

        run "nice -n 20 make -j #{CPU_COUNT}"
        run "make -j #{CPU_COUNT} install DESTDIR=#{(DESTDIR + "/binaryen").shellescape}"
    end
end

run "strip #{DESTDIR}/binaryen/bin/* || true"
run "install -m644 binaryen/LICENSE #{DESTDIR}"
