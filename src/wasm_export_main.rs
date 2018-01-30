use wasm_context::{
    Export,
    ImportExport,
    Context
};

pub fn process( ctx: &mut Context ) -> Option< String > {
    if let Some( function_index ) = ctx.start {
        let start_fn = ctx.functions.get_mut( &function_index ).unwrap();
        *start_fn.as_export_mut() = Export::some( "__web_main".to_owned() );
        ctx.start = None;

        Some( "__web_main".to_owned() )
    } else {
        None
    }
}
