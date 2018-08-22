use std::process::exit;
use std::path::{Path, PathBuf};

use package::{
    PrebuiltPackage,
    download_package
};
use utils::find_cmd;

fn emscripten_package() -> Option< PrebuiltPackage > {
    let package =
        if cfg!( target_os = "linux" ) && cfg!( target_arch = "x86_64" ) {
            PrebuiltPackage {
                url: "https://github.com/koute/emscripten-build/releases/download/emscripten-1.38.11-1/emscripten-1.38.11-1-x86_64-unknown-linux-gnu.tgz",
                name: "emscripten",
                version: "1.38.11-1",
                arch: "x86_64-unknown-linux-gnu",
                hash: "cc2727143297c37323c051e2d170fb3b31f43a38999954f812cdca1232555fd5",
                size: 211418062
            }
        } else if cfg!( target_os = "linux" ) && cfg!( target_arch = "x86" ) {
            PrebuiltPackage {
                url: "https://github.com/koute/emscripten-build/releases/download/emscripten-1.38.11-1/emscripten-1.38.11-1-i686-unknown-linux-gnu.tgz",
                name: "emscripten",
                version: "1.38.11-1",
                arch: "i686-unknown-linux-gnu",
                hash: "6be9f70fa79c096688aa630b8fe33df7e6f35716088c28c6fb058921c0ba927e",
                size: 223690133
            }
        } else {
            return None;
        };

    Some( package )
}

fn binaryen_package() -> Option< PrebuiltPackage > {
    let package =
        if cfg!( target_os = "linux" ) && cfg!( target_arch = "x86_64" ) {
            PrebuiltPackage {
                url: "https://github.com/koute/emscripten-build/releases/download/emscripten-1.38.11-1/binaryen-1.38.11-1-x86_64-unknown-linux-gnu.tgz",
                name: "binaryen",
                version: "1.38.11-1",
                arch: "x86_64-unknown-linux-gnu",
                hash: "1035effafac158fc1342a80a16cd800b4fd8418b6100b1e7cc4fd385cd13d801",
                size: 15048525
            }
        } else if cfg!( target_os = "linux" ) && cfg!( target_arch = "x86" ) {
            PrebuiltPackage {
                url: "https://github.com/koute/emscripten-build/releases/download/emscripten-1.38.11-1/binaryen-1.38.11-1-i686-unknown-linux-gnu.tgz",
                name: "binaryen",
                version: "1.38.11-1",
                arch: "i686-unknown-linux-gnu",
                hash: "6bca8f2378e86dfa3305bffc3dc231ace15f93904dab138692c8f8fb3814d337",
                size: 15042751
            }
        } else {
            return None;
        };

    Some( package )
}

fn check_emscripten() {
    let possible_commands =
        if cfg!( windows ) {
            &[ "emcc.bat" ]
        } else {
            &[ "emcc" ]
        };

    if find_cmd( possible_commands ).is_some() {
        return;
    }

    eprintln!( "error: you don't have Emscripten installed!" );
    eprintln!( "" );

    if Path::new( "/usr/bin/pacman" ).exists() {
        eprintln!( "You can most likely install it like this:" );
        eprintln!( "  sudo pacman -S emscripten" );
    } else if Path::new( "/usr/bin/apt-get" ).exists() {
        eprintln!( "You can most likely install it like this:" );
        eprintln!( "  sudo apt-get install emscripten" );
    } else if cfg!( target_os = "linux" ) {
        eprintln!( "You can most likely find it in your distro's repositories." );
    } else if cfg!( target_os = "windows" ) {
        eprintln!( "Download and install emscripten from the official site: http://kripken.github.io/emscripten-site/docs/getting_started/downloads.html" );
    }

    if cfg!( unix ) {
        if cfg!( target_os = "linux" ) {
            eprintln!( "If not you can install it manually like this:" );
        } else {
            eprintln!( "You can install it manually like this:" );
        }
        eprintln!( "  curl -O https://s3.amazonaws.com/mozilla-games/emscripten/releases/emsdk-portable.tar.gz" );
        eprintln!( "  tar -xzf emsdk-portable.tar.gz" );
        eprintln!( "  source emsdk_portable/emsdk_env.sh" );
        eprintln!( "  emsdk update" );
        eprintln!( "  emsdk install sdk-incoming-64bit" );
        eprintln!( "  emsdk activate sdk-incoming-64bit" );
    }

    exit( 101 );
}

pub struct Emscripten {
    pub binaryen_path: Option< PathBuf >,
    pub emscripten_path: PathBuf,
    pub emscripten_llvm_path: PathBuf
}

pub fn initialize_emscripten(
    use_system_emscripten: bool,
    targeting_webasm: bool
) -> Option< Emscripten > {

    if use_system_emscripten {
        check_emscripten();
        return None;
    }

    let emscripten_package = match emscripten_package() {
        Some( pkg ) => pkg,
        None => {
            check_emscripten();
            return None;
        }
    };

    let binaryen_package = if targeting_webasm {
        match binaryen_package() {
            Some( pkg ) => Some( pkg ),
            None => {
                check_emscripten();
                return None;
            }
        }
    } else {
        None
    };


    let emscripten_root = download_package( &emscripten_package );
    let emscripten_path = emscripten_root.join( "emscripten" );
    let emscripten_llvm_path = emscripten_root.join( "emscripten-fastcomp" );
    let binaryen_path = if let Some( binaryen_package ) = binaryen_package {
        let binaryen_root = download_package( &binaryen_package );
        Some( binaryen_root.join( "binaryen" ) )
    } else {
        None
    };

    Some( Emscripten {
        binaryen_path,
        emscripten_path,
        emscripten_llvm_path
    })
}
