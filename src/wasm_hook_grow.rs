use std::mem;

use wasm_context::{
    FunctionKind,
    FnTy,
    Import,
    Export,
    Opcode,
    Context
};

pub fn process( ctx: &mut Context ) {
    let type_index = ctx.get_or_add_fn_type( FnTy { params: vec![], return_type: None } );
    let on_grow_function_index = ctx.add_function( FunctionKind::Import {
        type_index,
        export: Export::none(),
        import: Import {
            module: "env".to_owned(),
            field: "__web_on_grow".to_owned()
        },
        name: Some( "__web_on_grow".to_owned() )
    });

    ctx.patch_code( |opcodes| {
        let should_process = opcodes.iter().any( |opcode| {
            match opcode {
                &Opcode::GrowMemory( _ ) => true,
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
            let should_insert = match &opcode {
                &Opcode::GrowMemory( _ ) => true,
                &_ => false
            };

            new_opcodes.push( opcode );
            if should_insert {
                new_opcodes.push( Opcode::Call( on_grow_function_index ) );
            }
        }

        *opcodes = new_opcodes;
    });
}
