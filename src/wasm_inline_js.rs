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
    let mut snippet_function_index_to_offset : HashMap<(u32, usize), i32> = HashMap::new();
    let mut snippet_offset_to_type_index : HashMap<i32, u32> = HashMap::new();
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

   
    for (&loop_function_index, function) in &ctx.functions {
        if let &FunctionKind::Definition { ref opcodes, .. } = function {

            // we have to run a medium-effort interpreter for the wasm because it could store the offset,
            // and then retreive it right before the Call() instruction, instead of assuming it's immediately
            // before (like it usually is in release builds)
            let mut local_map : HashMap<u32, i32> = HashMap::new();
            let mut local : i32 = 0;

            for ( index, opcode) in opcodes.iter().enumerate() {
                match opcode {
                    &Opcode::Call( target_function_index ) => {
                        let offset = local;

                        if let Some( &type_index ) = shim_map.get( &target_function_index ) {
                            if let Some( previous_ty ) = snippet_offset_to_type_index.get( &offset ).cloned() {
                                if type_index != previous_ty {
                                    panic!( "internal error: same snippet of JS (by offset) is used with two different shims; please report this!" );
                                }
                            }
                            
                            snippet_offset_to_type_index.insert( offset, type_index );
                            snippet_function_index_to_offset.insert( (loop_function_index, index), offset );
                        }
                    },
                    &Opcode::I32Const( value ) => {
                        local = value;
                    },
                    &Opcode::SetLocal( index ) => {
                        let local_value = local_map.entry(index).or_insert(0);
                        *local_value = local.clone();
                    },
                    &Opcode::GetLocal( index ) => {
                        if let Some( value ) = local_map.get(&index) {
                            local = value.clone();
                        } else {
                            //panic!("Get local at unset location");
                        }
                    },
                    _ => {
                    }
                }
            }
        }
    }

    let mut data_entries = Vec::new();
    mem::swap( &mut data_entries, &mut ctx.data );

    fn add_js_snippet( ctx: &mut Context, value_slice: &[u8], snippet_index_by_hash: &mut HashMap< String, usize >, snippet_index_by_offset: &mut HashMap< i32, usize >, snippets: &mut Vec< Snippet >, offset: i32, type_index: u32 ) {
        let code = match String::from_utf8( value_slice.to_owned() ) {
            Ok( code ) => code,
            Err( _ ) => {
                panic!( "You have invalid UTF-8 in one of your `js!` snippets! (offset = {}, length = {})", offset, value_slice.len() );
            }
        };

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
    }

    // For older Rust nightlies.
    data_entries.retain( |data| {
        if data.offset.len() != 2 {
            return true;
        }

        let (offset, type_index) = match data.constant_offset() {
            Some( offset ) => {
                if let Some( &type_index ) = snippet_offset_to_type_index.get( &offset ) {
                    (offset, type_index)
                } else {
                    return true;
                }
            },
            _ => return true
        };

        let slice = data.value.as_slice();
        let slice = &slice[ 0..slice.iter().position( |&byte| byte == 0 ).unwrap_or( slice.len() ) ];
        if slice.len() + 1 < data.value.len() {
            return true;
        }

        add_js_snippet( ctx, slice, &mut snippet_index_by_hash, &mut snippet_index_by_offset, &mut snippets, offset, type_index );
        return false;
    });

    // For newer Rust nightlies.
    for (offset, type_index) in snippet_offset_to_type_index {
        if snippet_index_by_offset.contains_key( &offset ) {
            continue; // Already done.
        }

        let (data, data_offset) = data_entries.iter()
            .filter_map( |data| {
                if let Some( offset ) = data.constant_offset() {
                    Some( (data, offset) )
                } else {
                    None
                }
            })
            .find( |&(data, data_offset)| offset >= data_offset && offset < (data_offset + data.value.len() as i32) )
            .expect( "js! snippet not found in data section" );

        let relative_offset = offset - data_offset;
        let slice = &data.value[ relative_offset as usize.. ];
        let slice = &slice[ 0..slice.iter().position( |&byte| byte == 0 ).unwrap_or( slice.len() ) ];

        // TODO: Purge this with the help of the new "linking" WASM section?
        add_js_snippet( ctx, slice, &mut snippet_index_by_hash, &mut snippet_index_by_offset, &mut snippets, offset, type_index );
    }

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

    ctx.patch_code_by_index( |&fn_index, opcodes| {
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

        for (opcode_index, opcode) in old_opcodes.into_iter().enumerate() {
            match opcode {
                Opcode::Call( function_index ) if shim_map.contains_key( &function_index ) => {
                    let offset = snippet_function_index_to_offset.get(&(fn_index, opcode_index)).unwrap();

                    match new_opcodes.pop().unwrap() {
                        Opcode::I32Const( _ ) => {},
                        Opcode::GetLocal( _ ) => {},
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
