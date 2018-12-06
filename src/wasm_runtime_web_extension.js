"use strict";

if( typeof Rust === "undefined" ) {
    var Rust = {};
}

Rust.{{{module_name}}} = (function( module_factory ) {
    var instance = module_factory();

    var getURL = ( typeof browser === "object" && browser !== null
        ? browser.runtime.getURL
        : chrome.runtime.getURL );

    return WebAssembly.instantiateStreaming( fetch( getURL( "{{{wasm_filename}}}" ), {credentials: "same-origin"} ), instance.imports )
        .then( function( result ) {
            var exports = instance.initialize( result.instance );
            console.log( "Finished loading Rust wasm module '{{{module_name}}}'" );
            return exports;
        })
        .catch( function( error ) {
            // The toString is needed to workaround a bug in Firefox (see issue #147)
            console.log( "Error loading Rust wasm module '{{{module_name}}}':", error.toString() );
            throw error;
        });
}( {{{factory}}} ));
