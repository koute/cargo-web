use wasm_context::{
    Export,
    ImportExport,
    Context
};

pub fn process( ctx: &mut Context ) {
    let table = ctx.tables.values_mut().next().unwrap();
    *table.as_export_mut() = Export::some( "__web_table".to_owned() );
}
