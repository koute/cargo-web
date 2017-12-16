use std::hash::{Hash, Hasher};
use std::collections::HashMap;
use std::mem;
use std::str;
use std::iter;

use ordermap::OrderMap;
use parity_wasm::elements as pw;
use parity_wasm::elements::Deserialize;

trait IterExt: Iterator + Sized {
    fn enumerate_u32( self ) -> iter::Map< iter::Enumerate< Self >, fn( (usize, Self::Item) ) -> (u32, Self::Item) > {
        self.enumerate().map( |(index, value)| (index as u32, value) )
    }
}

impl< T: Iterator > IterExt for T {}

pub type TypeIndex = u32;
pub type FunctionIndex = u32;
pub type TableIndex = u32;
pub type MemoryIndex = u32;
pub type GlobalIndex = u32;

pub use parity_wasm::elements::ValueType;
pub use parity_wasm::elements::Opcode;

#[derive(Clone, PartialEq, Debug)]
pub struct FnTy {
    pub params: Vec< ValueType >,
    pub return_type: Option< ValueType >
}

impl Eq for FnTy {}
impl Hash for FnTy {
    fn hash< H: Hasher >( &self, state: &mut H ) {
        for param in &self.params {
            (*param as i32).hash( state );
        }
        self.return_type.clone().map( |ret| ret as i32 ).hash( state );
    }
}

pub trait ImportExport {
    fn is_imported( &self ) -> bool;
    fn as_export( &self ) -> &Export;
    fn as_export_mut( &mut self ) -> &mut Export;
    fn is_exported( &self ) -> bool {
        !self.as_export().names.is_empty()
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Import {
    pub module: String,
    pub field: String
}

#[derive(Clone, PartialEq, Debug)]
pub struct Export {
    names: Vec< String >
}

impl Export {
    pub fn none() -> Self {
        Export {
            names: Vec::new()
        }
    }

    pub fn some( name: String ) -> Self {
        Export {
            names: vec![ name ]
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Local {
    pub count: u32,
    pub ty: ValueType,
    pub name: Option< String >
}

#[derive(Clone, PartialEq, Debug)]
pub enum FunctionKind {
    Import {
        export: Export,
        type_index: TypeIndex,
        import: Import
    },
    Definition {
        export: Export,
        type_index: TypeIndex,
        name: Option< String >,
        locals: Vec< Local >,
        opcodes: Vec< Opcode >
    }
}

impl ImportExport for FunctionKind {
    fn is_imported( &self ) -> bool {
        match self {
            &FunctionKind::Import { .. } => true,
            _ => false
        }
    }

    fn as_export( &self ) -> &Export {
        match self {
            &FunctionKind::Import { ref export, .. } => export,
            &FunctionKind::Definition { ref export, .. } => export,
        }
    }

    fn as_export_mut( &mut self ) -> &mut Export {
        match self {
            &mut FunctionKind::Import { ref mut export, .. } => export,
            &mut FunctionKind::Definition { ref mut export, .. } => export,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Limits {
    pub min: u32,
    pub max: Option< u32 >
}

#[derive(Clone, PartialEq, Debug)]
pub enum TableKind {
    Import {
        export: Export,
        limits: Limits,
        import: Import,
    },
    Definition {
        export: Export,
        limits: Limits
    }
}

impl ImportExport for TableKind {
    fn is_imported( &self ) -> bool {
        match self {
            &TableKind::Import { .. } => true,
            _ => false
        }
    }

    fn as_export( &self ) -> &Export {
        match self {
            &TableKind::Import { ref export, .. } => export,
            &TableKind::Definition { ref export, .. } => export,
        }
    }

    fn as_export_mut( &mut self ) -> &mut Export {
        match self {
            &mut TableKind::Import { ref mut export, .. } => export,
            &mut TableKind::Definition { ref mut export, .. } => export,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum MemoryKind {
    Import {
        export: Export,
        limits: Limits,
        import: Import
    },
    Definition {
        export: Export,
        limits: Limits
    }
}

impl ImportExport for MemoryKind {
    fn is_imported( &self ) -> bool {
        match self {
            &MemoryKind::Import { .. } => true,
            _ => false
        }
    }

    fn as_export( &self ) -> &Export {
        match self {
            &MemoryKind::Import { ref export, .. } => export,
            &MemoryKind::Definition { ref export, .. } => export,
        }
    }

    fn as_export_mut( &mut self ) -> &mut Export {
        match self {
            &mut MemoryKind::Import { ref mut export, .. } => export,
            &mut MemoryKind::Definition { ref mut export, .. } => export,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct GlobalType {
    ty: ValueType,
    is_mutable: bool
}

#[derive(Clone, PartialEq, Debug)]
pub enum GlobalKind {
    Import {
        export: Export,
        global_type: GlobalType,
        import: Import
    },
    Definition {
        export: Export,
        global_type: GlobalType,
        initializer: Vec< Opcode >
    }
}

impl ImportExport for GlobalKind {
    fn is_imported( &self ) -> bool {
        match self {
            &GlobalKind::Import { .. } => true,
            _ => false
        }
    }

    fn as_export( &self ) -> &Export {
        match self {
            &GlobalKind::Import { ref export, .. } => export,
            &GlobalKind::Definition { ref export, .. } => export,
        }
    }

    fn as_export_mut( &mut self ) -> &mut Export {
        match self {
            &mut GlobalKind::Import { ref mut export, .. } => export,
            &mut GlobalKind::Definition { ref mut export, .. } => export,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct FnPointerTable {
    members: Vec< FunctionIndex >,
    offset: Vec< Opcode >
}

#[derive(Clone, PartialEq, Debug)]
pub struct Data {
    pub offset: Vec< Opcode >,
    pub value: Vec< u8 >
}

#[derive(Clone, Debug)]
pub struct Context {
    pub types: OrderMap< TypeIndex, FnTy >,
    pub functions: OrderMap< FunctionIndex, FunctionKind >,
    pub tables: OrderMap< TableIndex, TableKind >,
    pub memories: OrderMap< MemoryIndex, MemoryKind >,
    pub globals: OrderMap< GlobalIndex, GlobalKind >,
    pub start: Option< FunctionIndex >,
    pub fn_pointer_tables: Option< FnPointerTable >,
    pub data: Vec< Data >,
    pub module_name: Option< String >,
    next_function_index: u32,
    next_type_index: u32
}

fn take< T: Default >( reference: &mut T ) -> T {
    let mut out = T::default();
    mem::swap( &mut out, reference );
    out
}

// This is based on code from wasm-gc.
fn decode_name_map< F: for< 'a > FnMut( u32, &'a str ) > ( p: &mut &[u8], mut callback: F ) -> Result< (), pw::Error > {
    let count = u32::from( pw::VarUint32::deserialize( p )? );
    for _ in 0..count {
        let index = u32::from( pw::VarUint32::deserialize( p )? );
        let name_length = u32::from( pw::VarUint32::deserialize( p )? );
        let (name, next_p) = p.split_at( name_length as usize );
        *p = next_p;

        let name = str::from_utf8( name ).expect( "function has an ill-formed name" );
        callback( index, name );
    }

    Ok(())
}

fn write_with_length< F: FnOnce( &mut Vec< u8 > ) >( output: &mut Vec< u8 >, callback: F ) {
    let mut buffer = Vec::new();
    callback( &mut buffer );

    pw::Serialize::serialize( pw::VarUint32::from( buffer.len() ), output ).unwrap();
    output.extend_from_slice( buffer.as_slice() );
}

fn write_name_map( output: &mut Vec< u8 >, map: &[(u32, String)] ) {
    pw::Serialize::serialize( pw::VarUint32::from( map.len() ), output ).unwrap();
    for &(index, ref name) in map {
        pw::Serialize::serialize( pw::VarUint32::from( index ), output ).unwrap();
        write_string( output, name );
    }
}

fn write_string( output: &mut Vec< u8 >, string: &str ) {
    pw::Serialize::serialize( pw::VarUint32::from( string.len() ), output ).unwrap();
    output.extend_from_slice( string.as_bytes() );
}

fn serialize_name_section(
    module_name: Option< String >,
    function_names: Vec< (u32, String) >,
    local_names: Vec< (u32, Vec< (u32, String) >) >
) -> Vec< u8 > {
    let mut output = Vec::new();
    write_with_length( &mut output, move |body| {
        write_string( body, "name" );

        if let Some( module_name ) = module_name {
            pw::Serialize::serialize( pw::VarUint7::from( 0 ), body ).unwrap();
            write_string( body, &module_name );
        }

        if !function_names.is_empty() {
            pw::Serialize::serialize( pw::VarUint7::from( 1 ), body ).unwrap();
            write_with_length( body, |inner_body| {
                write_name_map( inner_body, &function_names );
            });
        }

        if !local_names.is_empty() {
            pw::Serialize::serialize( pw::VarUint7::from( 2 ), body ).unwrap();
            write_with_length( body, |inner_body| {
                pw::Serialize::serialize( pw::VarUint32::from( local_names.len() ), inner_body ).unwrap();
                for (function_index, names) in local_names {
                    pw::Serialize::serialize( pw::VarUint32::from( function_index ), inner_body ).unwrap();
                    write_name_map( inner_body, &names );
                }
            });
        }
    });

    output
}

struct Entities< T > {
    index_map: HashMap< u32, u32 >,
    entries: Vec< (u32, T) >
}

fn preprocess_entities< T: ImportExport >( map: OrderMap< u32, T > ) -> Entities< T > {
    let mut index_map = HashMap::with_capacity( map.len() );
    let (imports, definitions): (Vec< _ >, Vec< _ >) = map.into_iter().partition( |&(_, ref entity)| entity.is_imported() );
    let mut entries = Vec::new();

    let import_count = imports.len() as u32;
    for (new_index, (old_index, entry)) in imports.into_iter().enumerate_u32() {
        index_map.insert( old_index, new_index );
        entries.push( (new_index, entry) );
    }

    for (index_offset, (old_index, entry)) in definitions.into_iter().enumerate_u32() {
        let new_index = import_count + index_offset;
        index_map.insert( old_index, new_index );
        entries.push( (new_index, entry) );
    }

    Entities {
        index_map,
        entries
    }
}

impl Context {
    pub fn new() -> Self {
        Context {
            types: Default::default(),
            functions: Default::default(),
            tables: Default::default(),
            memories: Default::default(),
            globals: Default::default(),
            start: Default::default(),
            fn_pointer_tables: Default::default(),
            data: Default::default(),
            module_name: Default::default(),
            next_function_index: 0,
            next_type_index: 0
        }
    }

    pub fn from_module( mut module: pw::Module ) -> Self {
        let mut ctx = Self::new();

        let mut next_table_index = 0;
        let mut next_memory_index = 0;
        let mut next_global_index = 0;
        let mut function_imports_count = 0;

        let mut function_sections = Vec::new();
        let mut exports_sections = Vec::new();

        let sections = take( module.sections_mut() );
        for section in sections {
            match section {
                pw::Section::Type( mut section ) => {
                    for ty in take( section.types_mut() ) {
                        let pw::Type::Function( mut ty ) = ty;
                        ctx.types.insert( ctx.next_type_index, FnTy {
                            params: take( ty.params_mut() ),
                            return_type: ty.return_type()
                        });
                        ctx.next_type_index += 1;
                    }
                },
                pw::Section::Import( mut section ) => {
                    for entry in take( section.entries_mut() ) {
                        let import = Import {
                            module: entry.module().to_owned(),
                            field: entry.field().to_owned()
                        };

                        match entry.external() {
                            &pw::External::Function( type_index ) => {
                                assert!( ctx.types.get( &type_index ).is_some() );
                                ctx.functions.insert( ctx.next_function_index, FunctionKind::Import {
                                    export: Export::none(),
                                    type_index,
                                    import
                                });
                                ctx.next_function_index += 1;
                                function_imports_count += 1;
                            },
                            &pw::External::Table( ref table_type ) => {
                                let limits = Limits {
                                    min: table_type.limits().initial(),
                                    max: table_type.limits().maximum()
                                };
                                ctx.tables.insert( next_table_index, TableKind::Import {
                                    export: Export::none(),
                                    limits,
                                    import
                                });
                                next_table_index += 1;
                            },
                            &pw::External::Memory( ref memory_type ) => {
                                let limits = Limits {
                                    min: memory_type.limits().initial(),
                                    max: memory_type.limits().maximum()
                                };
                                ctx.memories.insert( next_memory_index, MemoryKind::Import {
                                    export: Export::none(),
                                    limits,
                                    import
                                });
                                next_memory_index += 1;
                            },
                            &pw::External::Global( ref global_type ) => {
                                ctx.globals.insert( next_global_index, GlobalKind::Import {
                                    export: Export::none(),
                                    global_type: GlobalType {
                                        ty: global_type.content_type(),
                                        is_mutable: global_type.is_mutable()
                                    },
                                    import
                                });
                                next_global_index += 1;
                            }
                        }
                    }
                },
                pw::Section::Table( mut section ) => {
                    for table_type in take( section.entries_mut() ) {
                        let limits = Limits {
                            min: table_type.limits().initial(),
                            max: table_type.limits().maximum()
                        };
                        ctx.tables.insert( next_table_index, TableKind::Definition {
                            export: Export::none(),
                            limits
                        });
                        next_table_index += 1;
                    }
                },
                pw::Section::Memory( mut section ) => {
                    for memory_type in take( section.entries_mut() ) {
                        let limits = Limits {
                            min: memory_type.limits().initial(),
                            max: memory_type.limits().maximum()
                        };
                        ctx.memories.insert( next_memory_index, MemoryKind::Definition {
                            export: Export::none(),
                            limits
                        });
                        next_memory_index += 1;
                    }
                },
                pw::Section::Global( mut section ) => {
                    for mut global in take( section.entries_mut() ) {
                        ctx.globals.insert( next_global_index, GlobalKind::Definition {
                            export: Export::none(),
                            global_type: GlobalType {
                                ty: global.global_type().content_type(),
                                is_mutable: global.global_type().is_mutable()
                            },
                            initializer: take( global.init_expr_mut().code_mut() )
                        });
                        next_global_index += 1;
                    }
                },
                pw::Section::Export( mut section ) => {
                    exports_sections.push( section );
                },
                pw::Section::Start( function_index ) => {
                    ctx.start = Some( function_index );
                },
                pw::Section::Element( mut section ) => {
                    let entries = take( section.entries_mut() );
                    assert_eq!( entries.len(), 1, "multiple Element tables are not supported" );
                    assert!( ctx.fn_pointer_tables.is_none(), "duplicate Element table" );
                    let mut entry = entries.into_iter().next().unwrap();
                    ctx.fn_pointer_tables = Some( FnPointerTable {
                        members: take( entry.members_mut() ),
                        offset: take( entry.offset_mut().code_mut() )
                    });
                },
                pw::Section::Code( mut section ) => {
                    for mut body in take( section.bodies_mut() ) {
                        ctx.functions.insert( ctx.next_function_index, FunctionKind::Definition {
                            export: Export::none(),
                            name: None,
                            type_index: 0xFFFFFFFF,
                            locals: take( body.locals_mut() ).into_iter()
                                .map( |local| {
                                    Local {
                                        count: local.count(),
                                        ty: local.value_type(),
                                        name: None
                                    }
                                }).collect(),
                            opcodes: take( body.code_mut().elements_mut() )
                        });
                        ctx.next_function_index += 1;
                    }
                },
                pw::Section::Function( mut section ) => {
                    // We'll convert it later since those can appear before
                    // the functions themselves are actually defined.
                    function_sections.push( section );
                },
                pw::Section::Data( mut section ) => {
                    for mut entry in take( section.entries_mut() ) {
                        ctx.data.push( Data {
                            offset: take( entry.offset_mut().code_mut() ),
                            value: take( entry.value_mut() )
                        });
                    }
                },
                pw::Section::Custom( mut section ) => {
                    if section.name() == "name" {
                        let payload = take( section.payload_mut() );

                        let mut p: &[u8] = &payload;
                        while p.len() > 0 {
                            let kind = u8::from( pw::VarUint7::deserialize( &mut p ).unwrap() );
                            let payload_length = u32::from( pw::VarUint32::deserialize( &mut p ).unwrap() );
                            let (mut payload, next_p) = p.split_at( payload_length as usize );
                            p = next_p;

                            match kind {
                                0 => {
                                    ctx.module_name = Some(
                                        String::from_utf8( payload.to_vec() )
                                            .expect( "module has an ill-formed name" )
                                    );
                                },
                                1 => {
                                    decode_name_map( &mut payload, |function_index, function_name| {
                                        match ctx.functions.get_mut( &function_index ).unwrap() {
                                            &mut FunctionKind::Definition { ref mut name, .. } => {
                                                assert!( name.is_none() );
                                                *name = Some( function_name.to_owned() );
                                            },
                                            _ => panic!()
                                        }
                                    }).unwrap();
                                },
                                2 => {
                                    let count = u32::from( pw::VarUint32::deserialize( &mut payload ).unwrap() );
                                    for _ in 0..count {
                                        let function_index = u32::from( pw::VarUint32::deserialize( &mut payload ).unwrap() );
                                        decode_name_map( &mut payload, |local_index, name| {
                                            match ctx.functions.get_mut( &function_index ).unwrap() {
                                                &mut FunctionKind::Definition { ref mut locals, .. } => {
                                                    let local = &mut locals[ local_index as usize ];
                                                    assert!( local.name.is_none(), "duplicate local variable name" );
                                                    local.name = Some( name.to_owned() );
                                                },
                                                _ => panic!()
                                            }
                                        }).unwrap();
                                    }

                                },
                                kind => panic!( "unknown name section chunk type: {}", kind )
                            }
                        }
                    } else {
                        panic!( "unsupported custom section: '{}'", section.name() );
                    }
                },
                pw::Section::Unparsed { .. } => { unimplemented!() },
            }
        }

        for mut section in function_sections {
            let mut function_index = function_imports_count;
            for entry in take( section.entries_mut() ) {
                match ctx.functions.get_mut( &function_index ).unwrap() {
                    &mut FunctionKind::Definition { ref mut type_index, .. } => {
                        assert_eq!( *type_index, 0xFFFFFFFF, "function type was already set" );
                        *type_index = entry.type_ref();
                    },
                    _ => panic!()
                }
                function_index += 1;
            }
        }

        for mut section in exports_sections {
            for mut entry in take( section.entries_mut() ) {
                let name = take( entry.field_mut() );
                match entry.internal() {
                    &pw::Internal::Function( function_index ) => {
                        ctx.functions.get_mut( &function_index ).unwrap().as_export_mut().names.push( name );
                    },
                    &pw::Internal::Table( table_index ) => {
                        ctx.tables.get_mut( &table_index ).unwrap().as_export_mut().names.push( name );
                    },
                    &pw::Internal::Memory( memory_index ) => {
                        ctx.memories.get_mut( &memory_index ).unwrap().as_export_mut().names.push( name );
                    },
                    &pw::Internal::Global( global_index ) => {
                        ctx.globals.get_mut( &global_index ).unwrap().as_export_mut().names.push( name );
                    }
                }
            }
        }

        ctx
    }

    pub fn into_module( self ) -> pw::Module {
        fn process_opcodes(
            function_index_map: &HashMap< FunctionIndex, FunctionIndex >,
            type_index_map: &HashMap< TypeIndex, TypeIndex >,
            global_index_map: &HashMap< GlobalIndex, GlobalIndex >,
            opcodes: &mut Vec< Opcode >
        ) {
            for opcode in opcodes {
                match opcode {
                    &mut Opcode::Call( ref mut index ) => {
                        *index = function_index_map.get( &index ).cloned().unwrap();
                    },
                    &mut Opcode::CallIndirect( ref mut index, _ ) => {
                        *index = type_index_map.get( &index ).cloned().unwrap();
                    },
                    &mut Opcode::GetGlobal( ref mut index ) |
                    &mut Opcode::SetGlobal( ref mut index ) => {
                        *index = global_index_map.get( &index ).cloned().unwrap();
                    },
                    _ => {}
                }
            }
        }

        let mut sections = Vec::new();

        let mut type_map = HashMap::new();
        let mut section_types = Vec::with_capacity( self.types.len() );
        let mut section_imports = Vec::new();
        let mut section_functions = Vec::new();
        let mut section_tables = Vec::with_capacity( self.tables.len() );
        let mut section_memories = Vec::with_capacity( self.memories.len() );
        let mut section_code = Vec::new();
        let mut section_exports = Vec::new();
        let mut section_elements = Vec::new();
        let mut section_globals = Vec::new();
        let mut section_data = Vec::new();
        let mut function_names = Vec::new();
        let mut function_variable_names = Vec::new();

        for (new_type_index, (old_type_index, ty)) in self.types.into_iter().enumerate_u32() {
            type_map.insert( old_type_index, new_type_index );
            let params = ty.params;
            let return_ty = ty.return_type;
            section_types.push( pw::Type::Function( pw::FunctionType::new( params, return_ty ) ) );
        }

        let functions = preprocess_entities( self.functions );
        let tables = preprocess_entities( self.tables );
        let memories = preprocess_entities( self.memories );
        let globals = preprocess_entities( self.globals );

        for (new_index, function) in functions.entries {
            let export = match function {
                FunctionKind::Import { type_index, import, export } => {
                    let type_index = type_map.get( &type_index ).cloned().unwrap();
                    section_imports.push( pw::ImportEntry::new(
                        import.module,
                        import.field,
                        pw::External::Function( type_index )
                    ));

                    export
                },
                FunctionKind::Definition { name, type_index, locals, mut opcodes, export } => {
                    let type_index = type_map.get( &type_index ).cloned().unwrap();
                    let mut local_names = Vec::new();
                    let locals = locals.into_iter().enumerate_u32().map( |(local_index, local)| {
                        if let Some( local_name ) = local.name {
                            local_names.push( (local_index, local_name) );
                        }
                        pw::Local::new( local.count, local.ty )
                    }).collect();

                    process_opcodes( &functions.index_map, &type_map, &globals.index_map, &mut opcodes );

                    if let Some( name ) = name {
                        function_names.push( (new_index, name) );
                    }
                    if !local_names.is_empty() {
                        function_variable_names.push( (new_index, local_names) );
                    }
                    section_functions.push( pw::Func::new( type_index ) );
                    section_code.push( pw::FuncBody::new( locals, pw::Opcodes::new( opcodes ) ) );

                    export
                }
            };

            for name in export.names {
                section_exports.push( pw::ExportEntry::new( name, pw::Internal::Function( new_index ) ) );
            }
        }

        for (new_index, table) in tables.entries {
            let export = match table {
                TableKind::Import { import, limits, export } => {
                    section_imports.push( pw::ImportEntry::new(
                        import.module,
                        import.field,
                        pw::External::Table( pw::TableType::new( limits.min, limits.max ) )
                    ));

                    export
                },
                TableKind::Definition { limits, export } => {
                    section_tables.push( pw::TableType::new( limits.min, limits.max ) );
                    export
                }
            };

            for name in export.names {
                section_exports.push( pw::ExportEntry::new( name, pw::Internal::Table( new_index ) ) );
            }
        }

        for (new_index, memory) in memories.entries {
            let export = match memory {
                MemoryKind::Import { import, limits, export } => {
                    section_imports.push( pw::ImportEntry::new(
                        import.module,
                        import.field,
                        pw::External::Memory( pw::MemoryType::new( limits.min, limits.max ) )
                    ));
                    export
                },
                MemoryKind::Definition { limits, export } => {
                    section_memories.push( pw::MemoryType::new( limits.min, limits.max ) );
                    export
                }
            };

            for name in export.names {
                section_exports.push( pw::ExportEntry::new( name, pw::Internal::Memory( new_index ) ) );
            }
        }

        for (new_index, global) in globals.entries {
            let export = match global {
                GlobalKind::Import { global_type, import, export } => {
                    section_imports.push( pw::ImportEntry::new(
                        import.module,
                        import.field,
                        pw::External::Global( pw::GlobalType::new( global_type.ty, global_type.is_mutable ) )
                    ));

                    export
                },
                GlobalKind::Definition { global_type, mut initializer, export } => {
                    process_opcodes( &functions.index_map, &type_map, &globals.index_map, &mut initializer );

                    let global_type = pw::GlobalType::new( global_type.ty, global_type.is_mutable );
                    let entry = pw::GlobalEntry::new( global_type, pw::InitExpr::new( initializer ) );
                    section_globals.push( entry );

                    export
                }
            };

            for name in export.names {
                section_exports.push( pw::ExportEntry::new( name, pw::Internal::Global( new_index ) ) );
            }
        }

        for mut pointer_table in self.fn_pointer_tables {
            process_opcodes( &functions.index_map, &type_map, &globals.index_map, &mut pointer_table.offset );
            for function_index in &mut pointer_table.members {
                *function_index = functions.index_map.get( &function_index ).cloned().unwrap();
            }

            let entry = pw::ElementSegment::new( 0, pw::InitExpr::new( pointer_table.offset ), pointer_table.members );
            section_elements.push( entry );
        }

        for data in self.data {
            section_data.push( pw::DataSegment::new( 0, pw::InitExpr::new( data.offset ), data.value ) );
        }

        if !section_types.is_empty() {
            sections.push( pw::Section::Type( pw::TypeSection::with_types( section_types ) ) );
        }

        if !section_imports.is_empty() {
            sections.push( pw::Section::Import( pw::ImportSection::with_entries( section_imports ) ) );
        }

        if !section_functions.is_empty() {
            sections.push( pw::Section::Function( pw::FunctionSection::with_entries( section_functions ) ) );
        }

        if !section_tables.is_empty() {
            sections.push( pw::Section::Table( pw::TableSection::with_entries( section_tables ) ) );
        }

        if !section_memories.is_empty() {
            sections.push( pw::Section::Memory( pw::MemorySection::with_entries( section_memories ) ) );
        }

        if !section_globals.is_empty() {
            sections.push( pw::Section::Global( pw::GlobalSection::with_entries( section_globals ) ) );
        }

        if !section_exports.is_empty() {
            sections.push( pw::Section::Export( pw::ExportSection::with_entries( section_exports ) ) );
        }

        if let Some( mut start ) = self.start {
            start = functions.index_map.get( &start ).cloned().unwrap();
            sections.push( pw::Section::Start( start ) );
        }

        if !section_elements.is_empty() {
            sections.push( pw::Section::Element( pw::ElementSection::with_entries( section_elements ) ) );
        }

        if !section_code.is_empty() {
            sections.push( pw::Section::Code( pw::CodeSection::with_bodies( section_code ) ) );
        }

        if !section_data.is_empty() {
            sections.push( pw::Section::Data( pw::DataSection::with_entries( section_data ) ) );
        }

        if self.module_name.is_some() || !function_names.is_empty() || !function_variable_names.is_empty() {
            let name_section_bytes = serialize_name_section( self.module_name, function_names, function_variable_names );
            sections.push( pw::Section::Custom(
                pw::CustomSection::deserialize( &mut name_section_bytes.as_slice() ).unwrap()
            ));
        }

        pw::Module::new( sections )
    }

    pub fn add_function( &mut self, function: FunctionKind ) -> FunctionIndex {
        let function_index = self.next_function_index;
        self.next_function_index += 1;
        self.functions.insert( function_index, function );
        function_index
    }

    pub fn fn_ty_by_index( &self, index: TypeIndex ) -> Option< &FnTy > {
        self.types.get( &index )
    }

    pub fn get_or_add_fn_type( &mut self, ty: FnTy ) -> u32 {
        if let Some( (&type_index, _) ) = self.types.iter().find( |&(_, rty)| ty == *rty ) {
            return type_index;
        }

        let type_index = self.next_type_index;
        self.next_type_index += 1;
        self.types.insert( type_index, ty );
        type_index
    }

    pub fn patch_code< F >( &mut self, mut callback: F ) where F: for <'r> FnMut( &'r mut Vec< Opcode > ) {
        for function in self.functions.values_mut() {
            match function {
                &mut FunctionKind::Definition { ref mut opcodes, .. } => {
                    callback( opcodes );
                },
                _ => {}
            }
        }

        // TODO: Other places where opcodes are used.
    }
}

#[test]
fn test_serialization_deserialization() {
    let mut ctx = Context::new();
    let type_index = ctx.get_or_add_fn_type( FnTy { params: vec![ ValueType::I32 ], return_type: None } );
    ctx.module_name = Some( "module_1".to_owned() );
    ctx.add_function( FunctionKind::Definition {
        export: Export::some( "foobar".to_owned() ),
        type_index,
        name: Some( "func_1".to_owned() ),
        locals: vec![
            Local {
                count: 1,
                ty: ValueType::I32,
                name: Some( "v1".to_owned() )
            }
        ],
        opcodes: vec![]
    });

    let new_ctx = Context::from_module( ctx.clone().into_module() );
    assert_eq!( new_ctx.module_name, ctx.module_name );
    assert_eq!( new_ctx.functions, ctx.functions );
    assert_eq!( new_ctx.types, ctx.types );
}

#[test]
fn test_function_import_removal() {
    let mut ctx = Context::new();
    let type_index = ctx.get_or_add_fn_type( FnTy { params: vec![], return_type: None } );
    ctx.add_function( FunctionKind::Import {
        type_index,
        export: Export::none(),
        import: Import {
            module: "env".to_owned(),
            field: "foobar".to_owned()
        }
    });
    ctx.add_function( FunctionKind::Definition {
        type_index,
        export: Export::some( "main".to_owned() ),
        name: Some( "main".to_owned() ),
        locals: vec![
            Local {
                count: 1,
                ty: ValueType::I32,
                name: Some( "v1".to_owned() )
            }
        ],
        opcodes: vec![
            Opcode::Call( 1 )
        ],
    });
    ctx.start = Some( 1 );

    ctx.functions.remove( &0 );
    let new_ctx = Context::from_module( ctx.clone().into_module() );
    assert_eq!( new_ctx.functions.len(), 1 );

    match new_ctx.functions[ &0 ] {
        FunctionKind::Definition { ref name, ref export, ref opcodes, .. } => {
            assert_eq!( name.as_ref().unwrap(), "main" );
            assert_eq!( *export, Export::some( "main".to_owned() ) );
            assert_eq!( *opcodes, &[Opcode::Call( 0 )] );
        },
        _ => panic!()
    }

    assert_eq!( new_ctx.start, Some( 0 ) );
}
