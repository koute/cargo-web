function __initialize( __wasm_module, __load_asynchronously ) {
    return (function( module_factory ) {
        var instance = module_factory();
        if( __load_asynchronously ) {
            return WebAssembly.instantiate( __wasm_module, instance.imports )
                .then( function( wasm_instance ) {
                    var exports = instance.initialize( wasm_instance );
                    console.log( "Finished loading Rust wasm module '{{{module_name}}}'" );
                    return exports;
                })
                .catch( function( error ) {
                    console.log( "Error loading Rust wasm module '{{{module_name}}}':", error );
                    throw error;
                });
        } else {
            var instance = new WebAssembly.Instance( __wasm_module, instance.imports );
            return instance.initialize( wasm_instance );
        }
    }( {{{factory}}} ));
}
