// This is copied almost verbatim from: https://github.com/alexcrichton/wasm-gc
// Commit: b261ea6339c9987c81fcc0c691189018cd514928

use std::path::Path;
use std::collections::{BTreeSet, HashSet};
use std::str;

use rustc_demangle;
use parity_wasm::elements::*;

pub fn run<I: AsRef<Path>, O: AsRef<Path>>(input: I, output: O) {
    let input = input.as_ref();
    let output = output.as_ref();

    let mut module = deserialize_file(input)
        .expect("Failed to load module");

    let analysis = {
        let mut cx = LiveContext::new(&module);

        cx.blacklist.insert("__ashldi3");
        cx.blacklist.insert("__ashlti3");
        cx.blacklist.insert("__ashrdi3");
        cx.blacklist.insert("__ashrti3");
        cx.blacklist.insert("__lshrdi3");
        cx.blacklist.insert("__lshrti3");
        cx.blacklist.insert("__floatsisf");
        cx.blacklist.insert("__floatsidf");
        cx.blacklist.insert("__floatdidf");
        cx.blacklist.insert("__floattisf");
        cx.blacklist.insert("__floattidf");
        cx.blacklist.insert("__floatunsisf");
        cx.blacklist.insert("__floatunsidf");
        cx.blacklist.insert("__floatundidf");
        cx.blacklist.insert("__floatuntisf");
        cx.blacklist.insert("__floatuntidf");
        cx.blacklist.insert("__fixsfsi");
        cx.blacklist.insert("__fixsfdi");
        cx.blacklist.insert("__fixsfti");
        cx.blacklist.insert("__fixdfsi");
        cx.blacklist.insert("__fixdfdi");
        cx.blacklist.insert("__fixdfti");
        cx.blacklist.insert("__fixunssfsi");
        cx.blacklist.insert("__fixunssfdi");
        cx.blacklist.insert("__fixunssfti");
        cx.blacklist.insert("__fixunsdfsi");
        cx.blacklist.insert("__fixunsdfdi");
        cx.blacklist.insert("__fixunsdfti");
        cx.blacklist.insert("__udivsi3");
        cx.blacklist.insert("__umodsi3");
        cx.blacklist.insert("__udivmodsi4");
        cx.blacklist.insert("__udivdi3");
        cx.blacklist.insert("__udivmoddi4");
        cx.blacklist.insert("__umoddi3");
        cx.blacklist.insert("__udivti3");
        cx.blacklist.insert("__udivmodti4");
        cx.blacklist.insert("__umodti3");
        cx.blacklist.insert("memcpy");
        cx.blacklist.insert("memmove");
        cx.blacklist.insert("memset");
        cx.blacklist.insert("memcmp");
        cx.blacklist.insert("__powisf2");
        cx.blacklist.insert("__powidf2");
        cx.blacklist.insert("__addsf3");
        cx.blacklist.insert("__adddf3");
        cx.blacklist.insert("__subsf3");
        cx.blacklist.insert("__subdf3");
        cx.blacklist.insert("__divsi3");
        cx.blacklist.insert("__divdi3");
        cx.blacklist.insert("__divti3");
        cx.blacklist.insert("__divdf3");
        cx.blacklist.insert("__divsf3");
        cx.blacklist.insert("__modsi3");
        cx.blacklist.insert("__moddi3");
        cx.blacklist.insert("__modti3");
        cx.blacklist.insert("__divmodsi4");
        cx.blacklist.insert("__divmoddi4");
        cx.blacklist.insert("__muldi3");
        cx.blacklist.insert("__multi3");
        cx.blacklist.insert("__muldf3");
        cx.blacklist.insert("__mulsf3");
        cx.blacklist.insert("__mulosi4");
        cx.blacklist.insert("__mulodi4");
        cx.blacklist.insert("__muloti4");
        cx.blacklist.insert("rust_eh_personality");

        if let Some(section) = module.export_section() {
            for (i, entry) in section.entries().iter().enumerate() {
                cx.add_export_entry(entry, i as u32);
            }
        }
        if let Some(section) = module.import_section() {
            for (i, entry) in section.entries().iter().enumerate() {
                // debug!("import {:?}", entry);
                if let External::Memory(_) = *entry.external() {
                    cx.add_import_entry(entry, i as u32);
                }
            }
        }
        if let Some(section) = module.data_section() {
            for entry in section.entries() {
                cx.add_data_segment(entry);
            }
        }
        if let Some(tables) = module.table_section() {
            for i in 0..tables.entries().len() as u32 {
                cx.add_table(i);
            }
        }
        if let Some(elements) = module.elements_section() {
            for seg in elements.entries() {
                cx.add_element_segment(seg);
            }
        }
        if let Some(i) = module.start_section() {
            cx.add_function(i);
        }
        cx.analysis
    };

    let cx = RemapContext::new(&module, &analysis);
    for i in (0..module.sections().len()).rev() {
        let retain = match module.sections_mut()[i] {
            Section::Reloc(_) |
            Section::Unparsed { .. } => {
                // info!("unparsed section");
                continue
            }
            Section::Name(_) => {
                // Section::Name is only emitted when calling module.parse_names()
                unreachable!()
            },
            Section::Custom(ref mut s) if s.name() == "name" => {
                cx.remap_name_section(s);
                continue
            }
            Section::Custom(_) => {
                // info!("skipping custom section: {}", s.name());
                continue
            }
            Section::Type(ref mut s) => cx.remap_type_section(s),
            Section::Import(ref mut s) => cx.remap_import_section(s),
            Section::Function(ref mut s) => cx.remap_function_section(s),
            Section::Table(ref mut s) => cx.remap_table_section(s),
            Section::Memory(ref mut s) => cx.remap_memory_section(s),
            Section::Global(ref mut s) => cx.remap_global_section(s),
            Section::Export(ref mut s) => cx.remap_export_section(s),
            Section::Start(ref mut i) => { cx.remap_function_idx(i); true }
            Section::Element(ref mut s) => cx.remap_element_section(s),
            Section::Code(ref mut s) => cx.remap_code_section(s),
            Section::Data(ref mut s) => cx.remap_data_section(s),
        };
        if !retain {
            // debug!("remove empty section");
            module.sections_mut().remove(i);
        }
    }

    serialize_to_file(output, module).unwrap();
}

#[derive(Default)]
struct Analysis {
    functions: BTreeSet<u32>,
    codes: BTreeSet<u32>,
    tables: BTreeSet<u32>,
    memories: BTreeSet<u32>,
    globals: BTreeSet<u32>,
    types: BTreeSet<u32>,
    imports: BTreeSet<u32>,
    exports: BTreeSet<u32>,
}

enum Memories<'a> {
    Exported(&'a MemorySection),
    Imported(&'a MemoryType),
}

impl<'a> Memories<'a> {
    fn has_entry(&self, idx: usize) -> bool {
        match *self {
            Memories::Exported(memory_section) => idx < memory_section.entries().len(),
            Memories::Imported(_) => idx == 0,
        }
    }
}

struct LiveContext<'a> {
    blacklist: HashSet<&'static str>,
    function_section: Option<&'a FunctionSection>,
    type_section: Option<&'a TypeSection>,
    code_section: Option<&'a CodeSection>,
    table_section: Option<&'a TableSection>,
    memories: Option<Memories<'a>>,
    global_section: Option<&'a GlobalSection>,
    import_section: Option<&'a ImportSection>,
    analysis: Analysis,
}

impl<'a> LiveContext<'a> {
    fn new(module: &'a Module) -> LiveContext<'a> {
        let memories = module.memory_section().map(Memories::Exported).or_else(|| {
            if let Some(import_section) = module.import_section() {
                for entry in import_section.entries() {
                    if let External::Memory(ref memory_type) = *entry.external() {
                        return Some(Memories::Imported(memory_type));
                    }
                }
            }

            None
        });

        LiveContext {
            blacklist: HashSet::new(),
            function_section: module.function_section(),
            type_section: module.type_section(),
            code_section: module.code_section(),
            table_section: module.table_section(),
            memories: memories,
            global_section: module.global_section(),
            import_section: module.import_section(),
            analysis: Analysis::default(),
        }
    }

    fn add_function(&mut self, mut idx: u32) {
        if !self.analysis.functions.insert(idx) {
            return
        }
        if let Some(imports) = self.import_section {
            if idx < imports.functions() as u32 {
                // debug!("adding import: {}", idx);
                let import = imports.entries().get(idx as usize).expect("expected an imported function with this index");
                self.analysis.imports.insert(idx);
                return self.add_import_entry(import, idx);
            }
            idx -= imports.functions() as u32;
        }

        // debug!("adding function: {}", idx);
        self.analysis.codes.insert(idx);
        let functions = self.function_section.expect("no functions section");
        self.add_type(functions.entries()[idx as usize].type_ref());
        let codes = self.code_section.expect("no codes section");
        self.add_func_body(&codes.bodies()[idx as usize]);
    }

    fn add_table(&mut self, idx: u32) {
        if !self.analysis.tables.insert(idx) {
            return
        }
        let tables = self.table_section.expect("no table section");
        let table = &tables.entries()[idx as usize];
        drop(table);
    }

    fn add_memory(&mut self, idx: u32) {
        if !self.analysis.memories.insert(idx) {
            return
        }
        let memories = self.memories.as_ref().expect("no memory section or imported memory");
        assert!(memories.has_entry(idx as usize));
    }

    fn add_global(&mut self, idx: u32) {
        if !self.analysis.globals.insert(idx) {
            return
        }
        let globals = self.global_section.expect("no global section");
        let global = &globals.entries()[idx as usize];
        self.add_global_type(global.global_type());
        self.add_init_expr(global.init_expr());
    }

    fn add_global_type(&mut self, t: &GlobalType) {
        self.add_value_type(&t.content_type());
    }

    fn add_init_expr(&mut self, t: &InitExpr) {
        for instruction in t.code() {
            self.add_instruction(instruction);
        }
    }

    fn add_type(&mut self, idx: u32) {
        if !self.analysis.types.insert(idx) {
            return
        }
        let types = self.type_section.expect("no types section");
        match types.types()[idx as usize] {
            Type::Function(ref f) => {
                for param in f.params() {
                    self.add_value_type(param);
                }
                if let Some(ref ret) = f.return_type() {
                    self.add_value_type(ret);
                }
            }
        }
    }

    fn add_value_type(&mut self, value: &ValueType) {
        match *value {
            ValueType::I32 => {}
            ValueType::I64 => {}
            ValueType::F32 => {}
            ValueType::F64 => {}
            ValueType::V128 => {}
        }
    }

    fn add_func_body(&mut self, body: &FuncBody) {
        for local in body.locals() {
            self.add_value_type(&local.value_type());
        }
        self.add_instructions(body.code());
    }

    fn add_instructions(&mut self, code: &Instructions) {
        for instruction in code.elements() {
            self.add_instruction(instruction);
        }
    }

    fn add_instruction(&mut self, code: &Instruction) {
        match *code {
            Instruction::Block(ref b) |
            Instruction::Loop(ref b) |
            Instruction::If(ref b) => self.add_block_type(b),
            Instruction::Call(f) => self.add_function(f),
            Instruction::CallIndirect(t, _) => self.add_type(t),
            Instruction::GetGlobal(i) |
            Instruction::SetGlobal(i) => self.add_global(i),
            _ => {}
        }
    }

    fn add_block_type(&mut self, bt: &BlockType) {
        match *bt {
            BlockType::Value(ref v) => self.add_value_type(v),
            BlockType::NoResult => {}
        }
    }

    fn add_export_entry(&mut self, entry: &ExportEntry, idx: u32) {
        if self.blacklist.contains(entry.field()) {
            return
        }
        self.analysis.exports.insert(idx);
        match *entry.internal() {
            Internal::Function(i) => self.add_function(i),
            Internal::Table(i) => self.add_table(i),
            Internal::Memory(i) => self.add_memory(i),
            Internal::Global(i) => self.add_global(i),
        }
    }

    fn add_import_entry(&mut self, entry: &ImportEntry, idx: u32) {
        match *entry.external() {
            External::Function(i) => self.add_type(i),
            External::Table(_) => {},
            External::Memory(_) => {
                self.add_memory(0);
                self.analysis.imports.insert(idx);
            },
            External::Global(_) => {},
        }
    }

    fn add_data_segment(&mut self, data: &DataSegment) {
        self.add_memory(data.index());
        self.add_init_expr(data.offset().as_ref().unwrap());
    }

    fn add_element_segment(&mut self, seg: &ElementSegment) {
        for member in seg.members() {
            self.add_function(*member);
        }
        self.add_table(seg.index());
        self.add_init_expr(seg.offset().as_ref().unwrap());
    }
}

struct RemapContext<'a> {
    analysis: &'a Analysis,
    functions: Vec<u32>,
    globals: Vec<u32>,
    types: Vec<u32>,
    tables: Vec<u32>,
    memories: Vec<u32>,
    nimported_functions: u32,
}

impl<'a> RemapContext<'a> {
    fn new(m: &Module, analysis: &'a Analysis) -> RemapContext<'a> {
        fn remap(max: u32, retained: &BTreeSet<u32>) -> Vec<u32> {
            let mut v = Vec::with_capacity(max as usize);
            let mut offset = 0;
            for i in 0..max {
                if retained.contains(&i) {
                    v.push(i - offset);
                } else {
                    v.push(u32::max_value());
                    offset += 1;
                }
            }
            return v
        }

        let nfuncs = m.function_section().map(|m| m.entries().len() as u32);
        let nimported_functions = m.import_section().map(|m| {
            m.entries()
                .into_iter()
                .filter(|entry| {
                    match *entry.external() {
                        External::Function(_) => true,
                        _ => false,
                    }
                })
                .count() as u32
        });
        let functions = remap(nfuncs.unwrap_or(0) + nimported_functions.unwrap_or(0),
                              &analysis.functions);

        let nglobals = m.global_section().map(|m| m.entries().len() as u32);
        let globals = remap(nglobals.unwrap_or(0), &analysis.globals);

        let nmem = m.memory_section().map(|m| m.entries().len() as u32).unwrap_or_else(|| {
            if let Some(import_section) = m.import_section() {
                for entry in import_section.entries() {
                    if let External::Memory(_) = *entry.external() {
                        return 1;
                    }
                }
            }

            0
        });
        let memories = remap(nmem, &analysis.memories);

        let ntables = m.table_section().map(|m| m.entries().len() as u32);
        let tables = remap(ntables.unwrap_or(0), &analysis.tables);

        let ntypes = m.type_section().map(|m| m.types().len() as u32);
        let types = remap(ntypes.unwrap_or(0), &analysis.types);

        RemapContext {
            analysis,
            functions,
            globals,
            memories,
            tables,
            types,
            nimported_functions: nimported_functions.unwrap_or(0),
        }
    }

    fn retain<T>(&self, set: &BTreeSet<u32>, list: &mut Vec<T>, name: &str) {
        self.retain_offset(set, list, 0, name);
    }

    fn retain_offset<T>(&self,
                        set: &BTreeSet<u32>,
                        list: &mut Vec<T>,
                        offset: u32,
                        _name: &str) {
        for i in (0..list.len()).rev().map(|x| x as u32) {
            if !set.contains(&(i + offset)) {
                // debug!("removing {} {}", name, i + offset);
                list.remove(i as usize);
            }
        }
    }

    fn remap_type_section(&self, s: &mut TypeSection) -> bool {
        self.retain(&self.analysis.types, s.types_mut(), "type");
        for t in s.types_mut() {
            self.remap_type(t);
        }
        s.types().len() > 0
    }

    fn remap_type(&self, t: &mut Type) {
        match *t {
            Type::Function(ref mut t) => self.remap_function_type(t),
        }
    }

    fn remap_function_type(&self, t: &mut FunctionType) {
        for param in t.params_mut() {
            self.remap_value_type(param);
        }
        if let Some(m) = t.return_type_mut().as_mut() {
            self.remap_value_type(m);
        }
    }

    fn remap_value_type(&self, t: &mut ValueType) {
        drop(t);
    }

    fn remap_import_section(&self, s: &mut ImportSection) -> bool {
        self.retain(&self.analysis.imports, s.entries_mut(), "import");
        for i in s.entries_mut() {
            self.remap_import_entry(i);
        }
        s.entries().len() > 0
    }

    fn remap_import_entry(&self, s: &mut ImportEntry) {
        // debug!("remap import entry {:?}", s);
        match *s.external_mut() {
            External::Function(ref mut f) => self.remap_type_idx(f),
            External::Table(_) => {}
            External::Memory(_) => {}
            External::Global(_) => {}
        }
    }

    fn remap_function_section(&self, s: &mut FunctionSection) -> bool {
        self.retain_offset(&self.analysis.functions,
                           s.entries_mut(),
                           self.nimported_functions,
                           "function");
        for f in s.entries_mut() {
            self.remap_func(f);
        }
        s.entries().len() > 0
    }

    fn remap_func(&self, f: &mut Func) {
        self.remap_type_idx(f.type_ref_mut());
    }

    fn remap_table_section(&self, s: &mut TableSection) -> bool {
        self.retain(&self.analysis.tables, s.entries_mut(), "table");
        for t in s.entries_mut() {
            drop(t); // TODO
        }
        s.entries().len() > 0
    }

    fn remap_memory_section(&self, s: &mut MemorySection) -> bool {
        self.retain(&self.analysis.memories, s.entries_mut(), "memory");
        for m in s.entries_mut() {
            drop(m); // TODO
        }
        s.entries().len() > 0
    }

    fn remap_global_section(&self, s: &mut GlobalSection) -> bool {
        self.retain(&self.analysis.globals, s.entries_mut(), "global");
        for g in s.entries_mut() {
            self.remap_global_entry(g);
        }
        s.entries().len() > 0
    }

    fn remap_global_entry(&self, s: &mut GlobalEntry) {
        self.remap_global_type(s.global_type_mut());
        self.remap_init_expr(s.init_expr_mut());
    }

    fn remap_global_type(&self, s: &mut GlobalType) {
        drop(s);
    }

    fn remap_init_expr(&self, s: &mut InitExpr) {
        for code in s.code_mut() {
            self.remap_instruction(code);
        }
    }

    fn remap_export_section(&self, s: &mut ExportSection) -> bool {
        self.retain(&self.analysis.exports, s.entries_mut(), "export");
        for s in s.entries_mut() {
            self.remap_export_entry(s);
        }
        s.entries().len() > 0
    }

    fn remap_export_entry(&self, s: &mut ExportEntry) {
        match *s.internal_mut() {
            Internal::Function(ref mut i) => self.remap_function_idx(i),
            Internal::Table(ref mut i) => self.remap_table_idx(i),
            Internal::Memory(ref mut i) => self.remap_memory_idx(i),
            Internal::Global(ref mut i) => self.remap_global_idx(i),
        }
    }

    fn remap_element_section(&self, s: &mut ElementSection) -> bool {
        for s in s.entries_mut() {
            self.remap_element_segment(s);
        }
        true
    }

    fn remap_element_segment(&self, s: &mut ElementSegment) {
        let mut i = s.index();
        self.remap_table_idx(&mut i);
        assert_eq!(s.index(), i);
        for m in s.members_mut() {
            self.remap_function_idx(m);
        }
        self.remap_init_expr(s.offset_mut().as_mut().unwrap());
    }

    fn remap_code_section(&self, s: &mut CodeSection) -> bool {
        self.retain(&self.analysis.codes, s.bodies_mut(), "code");
        for s in s.bodies_mut() {
            self.remap_func_body(s);
        }
        s.bodies().len() > 0
    }

    fn remap_func_body(&self, b: &mut FuncBody) {
        self.remap_code(b.code_mut());
    }

    fn remap_code(&self, c: &mut Instructions) {
        for op in c.elements_mut() {
            self.remap_instruction(op);
        }
    }

    fn remap_instruction(&self, op: &mut Instruction) {
        match *op {
            Instruction::Block(ref mut b) |
            Instruction::Loop(ref mut b) |
            Instruction::If(ref mut b) => self.remap_block_type(b),
            Instruction::Call(ref mut f) => self.remap_function_idx(f),
            Instruction::CallIndirect(ref mut t, _) => self.remap_type_idx(t),
            Instruction::GetGlobal(ref mut i) |
            Instruction::SetGlobal(ref mut i) => self.remap_global_idx(i),
            _ => {}
        }
    }

    fn remap_block_type(&self, bt: &mut BlockType) {
        match *bt {
            BlockType::Value(ref mut v) => self.remap_value_type(v),
            BlockType::NoResult => {}
        }
    }

    fn remap_data_section(&self, s: &mut DataSection) -> bool {
        for data in s.entries_mut() {
            self.remap_data_segment(data);
        }
        true
    }

    fn remap_data_segment(&self, segment: &mut DataSegment) {
        let mut i = segment.index();
        self.remap_memory_idx(&mut i);
        assert_eq!(segment.index(), i);
        self.remap_init_expr(segment.offset_mut().as_mut().unwrap());
    }

    fn remap_type_idx(&self, i: &mut u32) {
        *i = self.types[*i as usize];
        assert!(*i != u32::max_value());
    }

    fn remap_function_idx(&self, i: &mut u32) {
        *i = self.functions[*i as usize];
        assert!(*i != u32::max_value());
    }

    fn remap_global_idx(&self, i: &mut u32) {
        *i = self.globals[*i as usize];
        assert!(*i != u32::max_value());
    }

    fn remap_table_idx(&self, i: &mut u32) {
        *i = self.tables[*i as usize];
        assert!(*i != u32::max_value());
    }

    fn remap_memory_idx(&self, i: &mut u32) {
        *i = self.memories[*i as usize];
        assert!(*i != u32::max_value());
    }

    fn remap_name_section(&self, s: &mut CustomSection) {
        let data = s.payload_mut();
        *data = self.rebuild_name_section(data)
            .expect("malformed name section");
    }

    fn rebuild_name_section(&self, mut data: &[u8]) -> Result<Vec<u8>, Error> {
        // if true { return Ok(data.to_vec()) }
        let mut res = Vec::new();
        while data.len() > 0 {
            let name_type = u8::from(VarUint7::deserialize(&mut data)?);
            let name_payload_len = u32::from(VarUint32::deserialize(&mut data)?);
            let (mut bytes, rest) = data.split_at(name_payload_len as usize);
            data = rest;

            match name_type {
                // module name, we leave this unmangled
                0 => {
                    VarUint7::from(name_type).serialize(&mut res)?;
                    VarUint32::from(name_payload_len).serialize(&mut res)?;
                    res.extend(bytes);
                }

                // function map
                1 => {
                    let mut map = self.decode_name_map(&mut bytes)?;
                    map.retain(|m| self.functions[m.0 as usize] != u32::max_value());
                    for slot in map.iter_mut() {
                        self.remap_function_idx(&mut slot.0);
                    }
                    let mut tmp = Vec::new();
                    self.serialize_name_map(&map, &mut tmp);

                    VarUint7::from(name_type).serialize(&mut res)?;
                    VarUint32::from(tmp.len()).serialize(&mut res)?;
                    res.extend(tmp);
                }

                // local names
                2 => {
                    let count = u32::from(VarUint32::deserialize(&mut bytes)?);
                    let mut locals = Vec::new();
                    for _ in 0..count {
                        let index = u32::from(VarUint32::deserialize(&mut bytes)?);
                        let map = self.decode_name_map(&mut bytes)?;
                        let new_index = self.functions[index as usize];
                        if new_index == u32::max_value() {
                            continue
                        }
                        locals.push((new_index, map));
                    }

                    let mut tmp = Vec::new();
                    VarUint32::from(locals.len()).serialize(&mut tmp).unwrap();
                    for (index, map) in locals {
                        VarUint32::from(index).serialize(&mut tmp).unwrap();
                        self.serialize_name_map(&map, &mut tmp);
                    }

                    VarUint7::from(name_type).serialize(&mut res)?;
                    VarUint32::from(tmp.len()).serialize(&mut res)?;
                    res.extend(tmp);
                }

                n => panic!("unknown name subsection type: {}", n),
            }
        }
        Ok(res)
    }

    fn decode_name_map<'b>(&self, bytes: &mut &'b [u8])
        -> Result<Vec<(u32, &'b str)>, Error>
    {
        let count = u32::from(VarUint32::deserialize(bytes)?);
        let mut names = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let index = u32::from(VarUint32::deserialize(bytes)?);
            let name_len = u32::from(VarUint32::deserialize(bytes)?);
            let (name, rest) = bytes.split_at(name_len as usize);
            *bytes = rest;
            let name = str::from_utf8(name)
                .expect("ill-formed utf-8 in name subsection");
            names.push((index, name));
        }
        Ok(names)
    }

    fn serialize_name_map(&self, names: &[(u32, &str)], dst: &mut Vec<u8>) {
        VarUint32::from(names.len()).serialize(dst).unwrap();
        for &(index, name) in names {
            let name = format!("{}", rustc_demangle::demangle(name));
            VarUint32::from(index).serialize(dst).unwrap();
            VarUint32::from(name.len()).serialize(dst).unwrap();
            dst.extend(name.as_bytes());
        }
    }
}
