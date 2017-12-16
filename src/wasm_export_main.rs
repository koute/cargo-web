use wasm_context::{
    FunctionKind,
    FnTy,
    Export,
    ImportExport,
    Opcode,
    Context
};

pub fn process( ctx: &mut Context ) {
    if let Some( function_index ) = ctx.start {
        let start_fn = ctx.functions.get_mut( &function_index ).unwrap();
        *start_fn.as_export_mut() = Export::some( "__web_main".to_owned() );
    }

    let type_index = ctx.get_or_add_fn_type( FnTy { params: vec![], return_type: None } );
    let function_type = ctx.add_function( FunctionKind::Definition {
        type_index,
        export: Export::none(),
        name: None,
        locals: vec![],
        opcodes: vec![ Opcode::End ],
    });
    ctx.start = Some( function_type );
}
