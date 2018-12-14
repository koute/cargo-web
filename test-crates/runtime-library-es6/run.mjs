"use strict";

import fs from "fs";
import factory from "./target/wasm32-unknown-unknown/debug/runtime-library-es6";

const bytecode = fs.readFileSync( "./target/wasm32-unknown-unknown/debug/runtime-library-es6.wasm" );
const wasm = new WebAssembly.Module( bytecode );

const instance = factory();
const compiled = new WebAssembly.Instance( wasm, instance.imports );
const exports = instance.initialize( compiled );

console.log( "Result is", exports.add( 1, 2 ) );
