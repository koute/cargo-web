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
    Instruction
};

fn hash( string: &str ) -> String {
    let mut hasher = Sha1::new();
    hasher.update( string.as_bytes() );
    format!( "{}", hasher.digest() )
}

pub struct JsSnippet {
    pub name: String,
    pub code: String,
    pub arg_count: usize
}

impl JsSnippet {
    pub fn arg_count( &self ) -> usize {
        self.arg_count
    }
}

struct Snippet {
    name: String,
    code: String,
    ty: FnTy,
    function_index: FunctionIndex,
    offset: i32
}

pub fn process_and_extract( ctx: &mut Context ) -> Vec< JsSnippet > {
    let mut shim_map: HashMap< FunctionIndex, TypeIndex > = HashMap::new();
    let mut snippet_offset_to_type_index = HashMap::new();
    let mut snippet_index_by_offset = HashMap::new();
    let mut snippet_index_by_hash: HashMap< String, usize > = HashMap::new();
    let mut snippets = Vec::new();
    let mut output = Vec::new();

    for (&function_index, function) in &ctx.functions {
        if let &FunctionKind::Import { type_index, ref import, .. } = function {
            if import.module == "env" && import.field.starts_with( "__js_" ) {
                shim_map.insert( function_index, type_index );
            }
        }
    }

    for (_, function) in &ctx.functions {
        if let &FunctionKind::Definition { ref instructions, .. } = function {
            for (index, instruction) in instructions.iter().enumerate() {
                match instruction {
                    &Instruction::Call( function_index ) => {
                        if let Some( &type_index ) = shim_map.get( &function_index ) {
                            match instructions[ index - 1 ] {
                                Instruction::I32Const( offset ) => {
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
                panic!(
                    "internal error: same snippet of JS (by value) is used with two different shims; please report this!\nfn 1: {:?}\nfn 2: {:?}\nhash: {}\noffset 1: {}\noffset 2: {}\nsnippet:\n\"{}\"",
                    ty,
                    snippet.ty,
                    code_hash,
                    snippet.offset,
                    offset,
                    snippet.code
                );
            }

            snippet_index_by_offset.insert( offset, snippet_index );
        } else {
            let snippet = Snippet {
                name: format!( "__extjs_{}", code_hash ),
                code,
                ty,
                function_index: 0xFFFFFFFF,
                offset
            };

            snippet_index_by_offset.insert( offset, snippets.len() );
            snippet_index_by_hash.insert( code_hash, snippets.len() );
            snippets.push( snippet );
        }
    }

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

    {
        let mut sorted_snippets: Vec< _ > = snippets.iter_mut().collect();
        sorted_snippets.sort_by( |a, b| a.name.cmp( &b.name ) );

        for snippet in sorted_snippets {
            let type_index = ctx.get_or_add_fn_type( snippet.ty.clone() );
            let function_index = ctx.add_function( FunctionKind::Import {
                export: Export::none(),
                import: Import { module: "env".to_owned(), field: snippet.name.clone() },
                name: Some( snippet.name.clone() ),
                type_index
            });

            snippet.function_index = function_index;
        }
    }

    for function_index in shim_map.keys() {
        ctx.functions.remove( function_index );
    }

    ctx.patch_code( |instructions| {
        let should_process = instructions.iter().any( |instruction| {
            match instruction {
                &Instruction::Call( function_index ) => shim_map.contains_key( &function_index ),
                _ => false
            }
        });

        if !should_process {
            return;
        }

        let mut new_instructions = Vec::with_capacity( instructions.len() );
        let mut old_instructions = Vec::new();
        mem::swap( instructions, &mut old_instructions );

        for instruction in old_instructions {
            match instruction {
                Instruction::Call( function_index ) if shim_map.contains_key( &function_index ) => {
                    // Pop the last argument which was a pointer to the code.
                    let offset = match new_instructions.pop().unwrap() {
                        Instruction::I32Const( offset ) => offset,
                        _ => panic!()
                    };
                    let &snippet_index = snippet_index_by_offset.get( &offset ).unwrap();
                    let snippet = &snippets[ snippet_index ];
                    new_instructions.push( Instruction::Call( snippet.function_index ) );
                }
                instruction => {
                    new_instructions.push( instruction );
                }
            }
        }

        *instructions = new_instructions;
    });

    output.extend( snippets.into_iter().map( |snippet| {
        JsSnippet {
            name: snippet.name,
            code: snippet.code,
            arg_count: snippet.ty.params.len()
        }
    }));

    output
}
