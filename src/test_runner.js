if( typeof Module === "undefined" ) {
    var Module = {};
}

ASYNC_TEST_PRIVATE = {};

eval((function() {
    const is_node_js = typeof process === "object" && typeof require === "function";
    const originals = {
        console_log: console.log.bind( console ),
        console_warn: console.warn.bind( console ),
        console_error: console.error.bind( console )
    };

    if( is_node_js ) {
        originals.process_stdout_write = process.stdout.write.bind( process.stdout );
        originals.process_stderr_write = process.stderr.write.bind( process.stderr );
        print = originals.process_stdout_write;
    } else {
        let buffer = "";
        print = function( text ) {
            buffer += text;
            while( 1 ) {
                const index = buffer.indexOf( "\n" );
                if( index === -1 ) {
                    break;
                }

                originals.console_log( buffer.slice( 0, index ) );
                buffer = buffer.slice( index + 1 );
            }
        };
    }

    const get_filter = function( args ) {
        for( var i = 0; i < args.length; ++i ) {
            const arg = args[ i ];
            if( arg.startsWith( "--" ) ) {
                if(
                    arg === "--skip" ||
                    arg === "--logfile" ||
                    arg === "--test-threads" ||
                    arg === "--color" ||
                    arg === "--format"
                ) {
                    j++;
                }

                continue;
            }

            return args[ i ];
        }

        return null;
    };

    const get_async_tests = function( filter, exports ) {
        const tests = {};
        const keys = Object.keys( exports );
        for( let i = 0; i < keys.length; ++i ) {
            const symbol = keys[ i ];
            const matched = symbol.match( /^_?__async_test__(.+)/ );
            if( matched ) {
                const name = matched[ 1 ];
                if( filter !== null ) {
                    if( name.indexOf( filter ) === -1 ) {
                        continue;
                    }
                }

                tests[ name ] = exports[ symbol ];
            }
        }

        return tests;
    };

    const show_summary = function( state ) {
        let status = "ok";
        if( state.failed.length !== 0 ) {
            status = "FAILED";
            for( let i = 0; i < state.failed.length; ++i ) {
                const failed_test_name = state.failed[ i ];
                const output = state.outputs[ failed_test_name ];
                if( output.length !== 0 ) {
                    print( "---- " + failed_test_name + "----\n" );
                    print( output + "\n" );
                }
            }

            print( "failures (async):\n" );
            for( let i = 0; i < state.failed.length; ++i ) {
                const failed_test_name = state.failed[ i ];
                print( "    " + failed_test_name + "\n" );
            }

            print( "\n" );
        }

        print( "test result (async): " + status + ". " + state.n_passed + " passed; " + state.failed.length + " failed\n\n" );
    };

    const clear_timeout = function( state ) {
        if( state.current_timeout !== null ) {
            clearTimeout( state.current_timeout );
            state.current_timeout = null;
        }
    };

    const clean_up = function() {
        if( is_node_js ) {
            process.stdout.write = originals.process_stdout_write;
            process.stderr.write = originals.process_stderr_write;
        }

        console.log = originals.console_log;
        console.warn = originals.console_warn;
        console.error = originals.console_error;

        ASYNC_TEST_PRIVATE.resolve = null;
        ASYNC_TEST_PRIVATE.reject = null;
    };

    const run_tests = function( state, tests ) {
        const test_name = Object.keys( tests )[0];
        if( !test_name ) {
            target.run_main();
            exit( 0 );

            return;
        }

        if( state.failed.length === 0 && state.n_passed === 0 ) {
            print( "running " + Object.keys( tests ).length + " async test(s)\n" );
        }

        const callback = tests[ test_name ];
        delete tests[ test_name ];
        state.outputs[ test_name ] = "";

        print( "test " + test_name + " ... " );

        ASYNC_TEST_PRIVATE.resolve = function() {
            state.n_passed += 1;
            clean_up();
            print( "ok\n" );
            clear_timeout( state );
            run_tests.apply( null, [state, tests] );
        };

        ASYNC_TEST_PRIVATE.reject = function( error ) {
            if( error !== null && typeof error === "object" && error.stack ) {
                state.outputs[ test_name ] += "Rejected with error: " + error.stack + "\n";
            } else {
                state.outputs[ test_name ] += "Rejected with error: " + error + "\n";
            }
            state.failed.push( test_name );
            clean_up();
            print( "FAILED\n" );
            clear_timeout( state );
            run_tests.apply( null, [state, tests] );
        };

        setTimeout( function() {
            state.current_timeout = setTimeout( function() {
                ASYNC_TEST_PRIVATE.reject( "Timeout!" );
            }, 5000 );

            if( is_node_js ) {
                process.stdout.write = function( text ) {
                    outputs[ name ] += text;
                };

                process.stderr.write = function( text ) {
                    outputs[ name ] += text;
                };
            }

            console.log = function() {
                const text = Array.prototype.slice.call( arguments ).join( ", " ) + "\n";
                outputs[ name ] += text;
            };

            console.warn = console.log;
            console.error = console.log;

            try {
                callback();
            } catch( error ) {
                ASYNC_TEST_PRIVATE.reject( error );
            }
        }, 0 );
    };

    const exit = function( status ) {
        if( state.failed.length !== 0 && status === 0 ) {
            status = 101;
        }

        env.real_exit( status );
    };

    const state = {
        current_timeout: null,
        outputs: {},
        failed: [],
        n_passed: 0
    };

    const env = {
        args: null,
        real_exit: null,
        code: "",
        target: null
    };

    const target = {
        run_main: null,
        exports: null
    };

    if( is_node_js ) {
        const fs = require( "fs" );
        const path = require( "path" );

        env.target = process.argv.splice( 2, 1 )[ 0 ];
        const artifact = path.resolve( process.argv.splice( 2, 1 )[ 0 ] );
        env.code = fs.readFileSync( artifact, {encoding: "utf-8"} );
        if( env.target === "wasm32-unknown-unknown" ) {
            const dir = path.dirname( artifact );
            const xs = Array.from( dir );
            const dir_codepoints = [];
            for( let i = 0; i < xs.length; ++i ) {
                dir_codepoints.push( xs[ i ].charCodeAt( 0 ) );
            }

            env.code = "__dirname = String.fromCodePoint(" + dir_codepoints + ");\n" + env.code;
        }
        process.argv[ 1 ] = artifact;

        env.real_exit = process.exit.bind( this );
        process.exit = exit;

        env.args = process.argv.slice( 2 );
    } else {
        env.target = __cargo_web.target;
        env.real_exit = Module[ "onExit" ];
        env.args = Module[ "arguments" ];
    }

    if( env.target === "asmjs-unknown-emscripten" || env.target === "wasm32-unknown-emscripten" ) {
        if( env.target === "wasm32-unknown-emscripten" ) {
            Module[ "locateFile" ] = function( path, script_directory ) {
                return path;
            }
        }

        Module[ "preInit" ] = function() {
            target.exports = Module;
            const real_main = Module[ "_main" ];
            let main_args = null;

            target.run_main = function() {
                Module[ "noExitRuntime" ] = false;
                real_main.apply( null, main_args );
                show_summary( state );
            };

            const filter = get_filter( env.args );
            const tests = get_async_tests( filter, target.exports );
            Module[ "_main" ] = function() {
                main_args = arguments;
                run_tests( state, tests );
                throw "SimulateInfiniteLoop";
            };
        };
    } else if( env.target === "wasm32-unknown-unknown" ) {
        const run = function( real_instance ) {
            const instance = { exports: {} };
            for( let key in real_instance.exports ) {
                instance.exports[ key ] = real_instance.exports[ key ];
            }

            target.exports = instance.exports;
            const real_main = target.exports[ "main" ];
            let main_args = null;

            target.run_main = function() {
                real_main.apply( null, main_args );
                show_summary( state );
            };

            const filter = get_filter( env.args );
            const tests = get_async_tests( filter, target.exports );
            target.exports[ "main" ] = function() {
                main_args = arguments;
                run_tests( state, tests );
            };

            return instance;
        };

        if( is_node_js ) {
            const ctor = WebAssembly.Instance;
            WebAssembly.Instance = function( mod, imports ) {
                WebAssembly.Instance = ctor;
                const real_instance = new WebAssembly.Instance( mod, imports );
                return run( real_instance );
            };
        } else {
            if( WebAssembly.instantiateStreaming ) {
                delete WebAssembly.instantiateStreaming;
            }

            const original_instantiate = WebAssembly.instantiate;
            WebAssembly.instantiate = function( mod, imports ) {
                WebAssembly.instantiate = original_instantiate;
                return WebAssembly.instantiate( mod, imports ).then( run );
            };
        }
    } else {
        throw "Unknown target: " + env.target;
    }

    return env.code;
})());
