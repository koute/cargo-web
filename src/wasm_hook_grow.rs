use std::mem;

use wasm_context::{
    FunctionKind,
    FnTy,
    Import,
    Export,
    Instruction,
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

    ctx.patch_code( |instructions| {
        let should_process = instructions.iter().any( |instruction| {
            match instruction {
                &Instruction::GrowMemory( _ ) => true,
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
            let should_insert = match &instruction {
                &Instruction::GrowMemory( _ ) => true,
                &_ => false
            };

            new_instructions.push( instruction );
            if should_insert {
                new_instructions.push( Instruction::Call( on_grow_function_index ) );
            }
        }

        *instructions = new_instructions;
    });
}
