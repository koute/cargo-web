use std::collections::HashMap;
use std::mem;

use sha1::Sha1;

use wasm_context::{
    FnTy,
    FunctionKind,
    FunctionIndex,
    TypeIndex,
    Import,
    Export,
    Context,
    Opcode
};

fn hash( string: &str ) -> String {
    let mut hasher = Sha1::new();
    hasher.update( string.as_bytes() );
    format!( "{}", hasher.digest() )
}

pub struct JsSnippet {
    pub name: String,
    pub code: String,
    pub ty: FnTy
}

impl JsSnippet {
    pub fn arg_count( &self ) -> usize {
        self.ty.params.len()
    }
}

struct Snippet {
    name: String,
    code: String,
    ty: FnTy,
    function_index: FunctionIndex
}

pub fn process_and_extract( ctx: &mut Context ) -> Vec< JsSnippet > {
    let mut shim_map: HashMap< FunctionIndex, TypeIndex > = HashMap::new();
    let mut snippet_offset_to_type_index = HashMap::new();
    let mut snippet_index_by_offset = HashMap::new();
    let mut snippet_index_by_hash: HashMap< String, usize > = HashMap::new();
    let mut snippets = Vec::new();

    for (&function_index, function) in &ctx.functions {
        if let &FunctionKind::Import { type_index, ref import, .. } = function {
            if import.module == "env" && import.field.starts_with( "__js_" ) {
                shim_map.insert( function_index, type_index );
            }
        }
    }

    for (_, function) in &ctx.functions {
        if let &FunctionKind::Definition { ref opcodes, .. } = function {
            for (index, opcode) in opcodes.iter().enumerate() {
                match opcode {
                    &Opcode::Call( function_index ) => {
                        if let Some( &type_index ) = shim_map.get( &function_index ) {
                            match opcodes[ index - 1 ] {
                                Opcode::I32Const( offset ) => {
                                    if let Some( previous_ty ) = snippet_offset_to_type_index.get( &offset ).cloned() {
                                        if type_index != previous_ty {
                                            panic!( "internal error: same snippet of JS (by offset) is used with two different shims; please report this!" );
                                        }
                                    }

                                    snippet_offset_to_type_index.insert( offset, type_index );
                                },
                                _ => panic!( "internal error: unexpected way of calling JS shims; please report this!" )
                            }
                        }
                    },
                    _ => {}
                }
            }
        }
    }

    let mut data_entries = Vec::new();
    mem::swap( &mut data_entries, &mut ctx.data );

    data_entries.retain( |data| {
        if data.offset.len() != 2 {
            return true;
        }

        let (offset, type_index) = match (&data.offset[ 0 ], &data.offset[ 1 ]) {
            (&Opcode::I32Const( offset ), &Opcode::End) => {
                if let Some( &type_index ) = snippet_offset_to_type_index.get( &offset ) {
                    (offset, type_index)
                } else {
                    return true;
                }
            },
            _ => return true
        };

        let mut value_slice = data.value.as_slice();
        // Strip a trailing null if present.
        if value_slice.last().map( |&last_byte| last_byte == 0 ).unwrap_or( false ) {
            let len = value_slice.len();
            value_slice = &value_slice[ 0..len - 1 ];
        }

        let code = String::from_utf8( value_slice.to_owned() ).unwrap();
        let code_hash = hash( &code );

        let shim_ty = ctx.fn_ty_by_index( type_index ).unwrap();
        let ty = FnTy {
            params: shim_ty.params.iter().cloned().take( shim_ty.params.len() - 1 ).collect(),
            return_type: shim_ty.return_type.clone()
        };

        if let Some( &snippet_index ) = snippet_index_by_hash.get( &code_hash ) {
            let snippet: &Snippet = &snippets[ snippet_index ];
            if snippet.ty != ty {
                panic!( "internal error: same snippet of JS (by value) is used with two different shims; please report this!" );
            }

            snippet_index_by_offset.insert( offset, snippet_index );
        } else {
            let snippet = Snippet {
                name: format!( "__extjs_{}", code_hash ),
                code,
                ty,
                function_index: 0xFFFFFFFF
            };

            snippet_index_by_offset.insert( offset, snippets.len() );
            snippet_index_by_hash.insert( code_hash, snippets.len() );
            snippets.push( snippet );
        }

        return false;
    });

    ctx.data = data_entries;

    for snippet in &mut snippets {
        let type_index = ctx.get_or_add_fn_type( snippet.ty.clone() );
        let function_index = ctx.add_function( FunctionKind::Import {
            export: Export::none(),
            import: Import { module: "env".to_owned(), field: snippet.name.clone() },
            name: Some( snippet.name.clone() ),
            type_index
        });

        snippet.function_index = function_index;
    }

    for function_index in shim_map.keys() {
        ctx.functions.remove( function_index );
    }

    ctx.patch_code( |opcodes| {
        let should_process = opcodes.iter().any( |opcode| {
            match opcode {
                &Opcode::Call( function_index ) => shim_map.contains_key( &function_index ),
                _ => false
            }
        });

        if !should_process {
            return;
        }

        let mut new_opcodes = Vec::with_capacity( opcodes.len() );
        let mut old_opcodes = Vec::new();
        mem::swap( opcodes, &mut old_opcodes );

        for opcode in old_opcodes {
            match opcode {
                Opcode::Call( function_index ) if shim_map.contains_key( &function_index ) => {
                    // Pop the last argument which was a pointer to the code.
                    let offset = match new_opcodes.pop().unwrap() {
                        Opcode::I32Const( offset ) => offset,
                        _ => panic!()
                    };
                    let &snippet_index = snippet_index_by_offset.get( &offset ).unwrap();
                    let snippet = &snippets[ snippet_index ];
                    new_opcodes.push( Opcode::Call( snippet.function_index ) );
                }
                opcode => {
                    new_opcodes.push( opcode );
                }
            }
        }

        *opcodes = new_opcodes;
    });

    snippets.into_iter().map( |snippet| {
        JsSnippet {
            name: snippet.name,
            code: snippet.code,
            ty: snippet.ty
        }
    }).collect()
}
