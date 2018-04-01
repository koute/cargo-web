use wasm_context::{
    Export,
    ImportExport,
    FunctionKind,
    Context
};

pub fn process( ctx: &mut Context ) -> Option< String > {
    let main_index = ctx.functions.iter().find( |&(_, function)| {
        match *function {
            FunctionKind::Definition { ref export, .. } => {
                export.names.iter().any( |name| name == "main" )
            },
            _ => false
        }
    }).map( |(&index, _)| index );

    let start_index = ctx.start.take();
    if main_index.is_some() {
        Some( "main".to_owned() )
    } else if let Some( start_index ) = start_index {
        let start_fn = ctx.functions.get_mut( &start_index ).unwrap();
        *start_fn.as_export_mut() = Export::some( "__web_main".to_owned() );
        Some( "__web_main".to_owned() )
    } else {
        None
    }
}
