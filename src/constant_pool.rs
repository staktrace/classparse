use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;

use crate::{err, read_u1, read_u2, read_u4, read_u8, BootstrapMethodRef};

#[derive(Debug)]
pub(crate) enum ConstantPoolRef<'a> {
    Unresolved(u16),
    Resolved(Rc<ConstantPoolEntry<'a>>),
}

impl<'a> ConstantPoolRef<'a> {
    fn resolve(&mut self, my_index: usize, pool: &[Rc<ConstantPoolEntry<'a>>]) -> Result<bool, String> {
        match self {
            ConstantPoolRef::Unresolved(ix) => {
                let target = *ix as usize;
                if target == my_index {
                    return Err(format!("Constant pool entry at index {} could not be resolved due to self-reference", my_index));
                }
                if target >= pool.len() {
                    return Err(format!("Constant pool entry at index {} references out-of-bounds index {}", my_index, target));
                }
                if !pool[target].is_resolved() {
                    return Ok(false);
                }
                *self = ConstantPoolRef::Resolved(pool[target].clone());
                Ok(true)
            }
            ConstantPoolRef::Resolved(_) => Ok(true),
        }
    }

    fn is_resolved(&self) -> bool {
        match self {
            ConstantPoolRef::Unresolved(_) => false,
            ConstantPoolRef::Resolved(_) => true,
        }
    }

    fn get(&self) -> &Rc<ConstantPoolEntry<'a>> {
        match self {
            ConstantPoolRef::Unresolved(_) => panic!("Called get on a unresolved ConstantPoolRef"),
            ConstantPoolRef::Resolved(target) => target,
        }
    }
}

trait RefCellDeref<'a> {
    fn resolve(&self, cp_index: usize, pool: &[Rc<ConstantPoolEntry<'a>>]) -> Result<bool, String>;
    fn ensure_type(&self, allowed: ConstantPoolEntryTypes) -> Result<bool, String>;
}

impl<'a> RefCellDeref<'a> for RefCell<ConstantPoolRef<'a>> {
    fn resolve(&self, cp_index: usize, pool: &[Rc<ConstantPoolEntry<'a>>]) -> Result<bool, String> {
        self.borrow_mut().resolve(cp_index, pool)
    }

    fn ensure_type(&self, allowed: ConstantPoolEntryTypes) -> Result<bool, String> {
        self.borrow().get().ensure_type(allowed)
    }
}

#[derive(Debug)]
pub enum ReferenceKind {
    GetField,
    GetStatic,
    PutField,
    PutStatic,
    InvokeVirtual,
    InvokeStatic,
    InvokeSpecial,
    NewInvokeSpecial,
    InvokeInterface,
}

bitflags! {
    pub(crate) struct ConstantPoolEntryTypes: u16 {
        const ZERO = 0x0001;
        const UTF8 = 0x0002;
        const INTEGER = 0x0004;
        const FLOAT = 0x0008;
        const LONG = 0x0010;
        const DOUBLE = 0x0020;
        const CLASS_INFO = 0x0040;
        const STRING = 0x0080;
        const FIELD_REF = 0x0100;
        const METHOD_REF = 0x0200;
        const INTERFACE_METHOD_REF = 0x0400;
        const NAME_AND_TYPE = 0x0800;
        const METHOD_HANDLE = 0x1000;
        const METHOD_TYPE = 0x2000;
        const INVOKE_DYNAMIC = 0x4000;
        const UNUSED = 0x8000;

        const CLASS_OR_ZERO = Self::ZERO.bits() | Self::CLASS_INFO.bits();
        const NEW_METHOD_REFS = Self::METHOD_REF.bits() | Self::INTERFACE_METHOD_REF.bits();
        const CONSTANTS = Self::INTEGER.bits() | Self::FLOAT.bits() | Self::LONG.bits() | Self::DOUBLE.bits() | Self::STRING.bits();
        const UTF8_OR_ZERO = Self::ZERO.bits() | Self::UTF8.bits();
        const NAME_AND_TYPE_OR_ZERO = Self::ZERO.bits() | Self::NAME_AND_TYPE.bits();
    }
}

#[derive(Debug)]
pub(crate) enum ConstantPoolEntry<'a> {
    Zero,
    Utf8(Cow<'a, str>),
    Integer(i32),
    Float(f32),
    Long(i64),
    Double(f64),
    ClassInfo(RefCell<ConstantPoolRef<'a>>),
    String(RefCell<ConstantPoolRef<'a>>),
    FieldRef(RefCell<ConstantPoolRef<'a>>, RefCell<ConstantPoolRef<'a>>),
    MethodRef(RefCell<ConstantPoolRef<'a>>, RefCell<ConstantPoolRef<'a>>),
    InterfaceMethodRef(RefCell<ConstantPoolRef<'a>>, RefCell<ConstantPoolRef<'a>>),
    NameAndType(RefCell<ConstantPoolRef<'a>>, RefCell<ConstantPoolRef<'a>>),
    MethodHandle(ReferenceKind, RefCell<ConstantPoolRef<'a>>),
    MethodType(RefCell<ConstantPoolRef<'a>>),
    InvokeDynamic(BootstrapMethodRef, RefCell<ConstantPoolRef<'a>>),
    Unused,
}

impl<'a> ConstantPoolEntry<'a> {
    fn resolve(&self, my_index: usize, pool: &[Rc<ConstantPoolEntry<'a>>]) -> Result<bool, String> {
        match self {
            ConstantPoolEntry::ClassInfo(x) => x.resolve(my_index, pool),
            ConstantPoolEntry::String(x) => x.resolve(my_index, pool),
            ConstantPoolEntry::FieldRef(x, y) => Ok(x.resolve(my_index, pool)? && y.resolve(my_index, pool)?),
            ConstantPoolEntry::MethodRef(x, y) => Ok(x.resolve(my_index, pool)? && y.resolve(my_index, pool)?),
            ConstantPoolEntry::InterfaceMethodRef(x, y) => Ok(x.resolve(my_index, pool)? && y.resolve(my_index, pool)?),
            ConstantPoolEntry::NameAndType(x, y) => Ok(x.resolve(my_index, pool)? && y.resolve(my_index, pool)?),
            ConstantPoolEntry::MethodHandle(_, y) => y.resolve(my_index, pool),
            ConstantPoolEntry::MethodType(x) => x.resolve(my_index, pool),
            ConstantPoolEntry::InvokeDynamic(_, y) => y.resolve(my_index, pool),
            _ => Ok(true),
        }
    }

    fn is_resolved(&self) -> bool {
        match self {
            ConstantPoolEntry::ClassInfo(x) => x.borrow().is_resolved(),
            ConstantPoolEntry::String(x) => x.borrow().is_resolved(),
            ConstantPoolEntry::FieldRef(x, y) => x.borrow().is_resolved() && y.borrow().is_resolved(),
            ConstantPoolEntry::MethodRef(x, y) => x.borrow().is_resolved() && y.borrow().is_resolved(),
            ConstantPoolEntry::InterfaceMethodRef(x, y) => x.borrow().is_resolved() && y.borrow().is_resolved(),
            ConstantPoolEntry::NameAndType(x, y) => x.borrow().is_resolved() && y.borrow().is_resolved(),
            ConstantPoolEntry::MethodHandle(_, y) => y.borrow().is_resolved(),
            ConstantPoolEntry::MethodType(x) => x.borrow().is_resolved(),
            ConstantPoolEntry::InvokeDynamic(_, y) => y.borrow().is_resolved(),
            _ => true,
        }
    }

    fn get_type(&self) -> ConstantPoolEntryTypes {
        match self {
            ConstantPoolEntry::Zero => ConstantPoolEntryTypes::ZERO,
            ConstantPoolEntry::Utf8(_) => ConstantPoolEntryTypes::UTF8,
            ConstantPoolEntry::Integer(_) => ConstantPoolEntryTypes::INTEGER,
            ConstantPoolEntry::Float(_) => ConstantPoolEntryTypes::FLOAT,
            ConstantPoolEntry::Long(_) => ConstantPoolEntryTypes::LONG,
            ConstantPoolEntry::Double(_) => ConstantPoolEntryTypes::DOUBLE,
            ConstantPoolEntry::ClassInfo(_) => ConstantPoolEntryTypes::CLASS_INFO,
            ConstantPoolEntry::String(_) => ConstantPoolEntryTypes::STRING,
            ConstantPoolEntry::FieldRef(_, _) => ConstantPoolEntryTypes::FIELD_REF,
            ConstantPoolEntry::MethodRef(_, _) => ConstantPoolEntryTypes::METHOD_REF,
            ConstantPoolEntry::InterfaceMethodRef(_, _) => ConstantPoolEntryTypes::INTERFACE_METHOD_REF,
            ConstantPoolEntry::NameAndType(_, _) => ConstantPoolEntryTypes::NAME_AND_TYPE,
            ConstantPoolEntry::MethodHandle(_, _) => ConstantPoolEntryTypes::METHOD_HANDLE,
            ConstantPoolEntry::MethodType(_) => ConstantPoolEntryTypes::METHOD_TYPE,
            ConstantPoolEntry::InvokeDynamic(_, _) => ConstantPoolEntryTypes::INVOKE_DYNAMIC,
            ConstantPoolEntry::Unused => ConstantPoolEntryTypes::UNUSED,
        }
    }

    fn validate(&self, major_version: u16) -> Result<bool, String> {
        match self {
            ConstantPoolEntry::ClassInfo(x) => x.ensure_type(ConstantPoolEntryTypes::UTF8),
            ConstantPoolEntry::String(x) => x.ensure_type(ConstantPoolEntryTypes::UTF8),
            ConstantPoolEntry::FieldRef(x, y) => Ok(x.ensure_type(ConstantPoolEntryTypes::CLASS_INFO)? && y.ensure_type(ConstantPoolEntryTypes::NAME_AND_TYPE)?),
            ConstantPoolEntry::MethodRef(x, y) => Ok(x.ensure_type(ConstantPoolEntryTypes::CLASS_INFO)? && y.ensure_type(ConstantPoolEntryTypes::NAME_AND_TYPE)?),
            ConstantPoolEntry::InterfaceMethodRef(x, y) => Ok(x.ensure_type(ConstantPoolEntryTypes::CLASS_INFO)? && y.ensure_type(ConstantPoolEntryTypes::NAME_AND_TYPE)?),
            ConstantPoolEntry::NameAndType(x, y) => Ok(x.ensure_type(ConstantPoolEntryTypes::UTF8)? && y.ensure_type(ConstantPoolEntryTypes::UTF8)?),
            ConstantPoolEntry::MethodHandle(x, y) => y.ensure_type(match x {
                ReferenceKind::GetField |
                ReferenceKind::GetStatic |
                ReferenceKind::PutField |
                ReferenceKind::PutStatic => ConstantPoolEntryTypes::FIELD_REF,
                ReferenceKind::InvokeVirtual |
                ReferenceKind::NewInvokeSpecial => ConstantPoolEntryTypes::METHOD_REF,
                ReferenceKind::InvokeStatic |
                ReferenceKind::InvokeSpecial => if major_version < 52 { ConstantPoolEntryTypes::METHOD_REF } else { ConstantPoolEntryTypes::NEW_METHOD_REFS },
                ReferenceKind::InvokeInterface => ConstantPoolEntryTypes::INTERFACE_METHOD_REF,
            }),
            ConstantPoolEntry::MethodType(x) => x.ensure_type(ConstantPoolEntryTypes::UTF8),
            ConstantPoolEntry::InvokeDynamic(_, y) => y.ensure_type(ConstantPoolEntryTypes::NAME_AND_TYPE),
            _ => Ok(true),
        }
    }

    fn ensure_type(&self, allowed: ConstantPoolEntryTypes) -> Result<bool, String> {
        if allowed.contains(self.get_type()) {
            Ok(true)
        } else {
            err("Unexpected constant pool reference type for")
        }
    }

    pub(crate) fn utf8(&self) -> Cow<'a, str> {
        match self {
            ConstantPoolEntry::Utf8(x) => x.clone(),
            _ => panic!("Attempting to get utf-8 data from non-utf8 constant pool entry!"),
        }
    }

    pub(crate) fn classinfo_utf8(&self) -> Cow<'a, str> {
        match self {
            ConstantPoolEntry::ClassInfo(x) => x.borrow().get().utf8(),
            _ => panic!("Attempting to get classinfo data from non-classinfo constant pool entry!"),
        }
    }
}

fn read_unresolved_cp_ref<'a>(bytes: &'a [u8], ix: &mut usize) -> Result<RefCell<ConstantPoolRef<'a>>, String> {
    Ok(RefCell::new(ConstantPoolRef::Unresolved(read_u2(bytes, ix)?)))
}
fn read_constant_utf8<'a>(bytes: &'a [u8], ix: &mut usize) -> Result<ConstantPoolEntry<'a>, String> {
    let length = read_u2(bytes, ix)? as usize;
    if bytes.len() < *ix + length {
        return Err(format!("Unexpected end of stream reading CONSTANT_Utf8 at index {}", *ix));
    }
    let modified_utf8_data = &bytes[*ix .. *ix + length];
    *ix += length;
    Ok(ConstantPoolEntry::Utf8(cesu8::from_java_cesu8(modified_utf8_data).map_err(|e| format!("Error reading CONSTANT_Utf8 at indices {}..{}: {}", *ix - length, *ix, e))?))
}

fn read_constant_integer<'a>(bytes: &'a [u8], ix: &mut usize) -> Result<ConstantPoolEntry<'a>, String> {
    Ok(ConstantPoolEntry::Integer(read_u4(bytes, ix)? as i32))
}

fn read_constant_float<'a>(bytes: &'a [u8], ix: &mut usize) -> Result<ConstantPoolEntry<'a>, String> {
    Ok(ConstantPoolEntry::Float(f32::from_bits(read_u4(bytes, ix)?)))
}

fn read_constant_long<'a>(bytes: &'a [u8], ix: &mut usize) -> Result<ConstantPoolEntry<'a>, String> {
    Ok(ConstantPoolEntry::Long(read_u8(bytes, ix)? as i64))
}

fn read_constant_double<'a>(bytes: &'a [u8], ix: &mut usize) -> Result<ConstantPoolEntry<'a>, String> {
    Ok(ConstantPoolEntry::Double(f64::from_bits(read_u8(bytes, ix)?)))
}

fn read_constant_class<'a>(bytes: &'a [u8], ix: &mut usize) -> Result<ConstantPoolEntry<'a>, String> {
    let name_ref = read_unresolved_cp_ref(bytes, ix)?;
    Ok(ConstantPoolEntry::ClassInfo(name_ref))
}

fn read_constant_string<'a>(bytes: &'a [u8], ix: &mut usize) -> Result<ConstantPoolEntry<'a>, String> {
    let value_ref = read_unresolved_cp_ref(bytes, ix)?;
    Ok(ConstantPoolEntry::String(value_ref))
}

fn read_constant_fieldref<'a>(bytes: &'a [u8], ix: &mut usize) -> Result<ConstantPoolEntry<'a>, String> {
    let class_ref = read_unresolved_cp_ref(bytes, ix)?;
    let name_and_type_ref = read_unresolved_cp_ref(bytes, ix)?;
    Ok(ConstantPoolEntry::FieldRef(class_ref, name_and_type_ref))
}

fn read_constant_methodref<'a>(bytes: &'a [u8], ix: &mut usize) -> Result<ConstantPoolEntry<'a>, String> {
    let class_ref = read_unresolved_cp_ref(bytes, ix)?;
    let name_and_type_ref = read_unresolved_cp_ref(bytes, ix)?;
    Ok(ConstantPoolEntry::MethodRef(class_ref, name_and_type_ref))
}

fn read_constant_interfacemethodref<'a>(bytes: &'a [u8], ix: &mut usize) -> Result<ConstantPoolEntry<'a>, String> {
    let class_ref = read_unresolved_cp_ref(bytes, ix)?;
    let name_and_type_ref = read_unresolved_cp_ref(bytes, ix)?;
    Ok(ConstantPoolEntry::InterfaceMethodRef(class_ref, name_and_type_ref))
}

fn read_constant_nameandtype<'a>(bytes: &'a [u8], ix: &mut usize) -> Result<ConstantPoolEntry<'a>, String> {
    let name_ref = read_unresolved_cp_ref(bytes, ix)?;
    let descriptor_ref = read_unresolved_cp_ref(bytes, ix)?;
    Ok(ConstantPoolEntry::NameAndType(name_ref, descriptor_ref))
}

fn read_constant_methodhandle<'a>(bytes: &'a [u8], ix: &mut usize) -> Result<ConstantPoolEntry<'a>, String> {
    let reference_kind = match read_u1(bytes, ix)? {
        1 => ReferenceKind::GetField,
        2 => ReferenceKind::GetStatic,
        3 => ReferenceKind::PutField,
        4 => ReferenceKind::PutStatic,
        5 => ReferenceKind::InvokeVirtual,
        6 => ReferenceKind::InvokeStatic,
        7 => ReferenceKind::InvokeSpecial,
        8 => ReferenceKind::NewInvokeSpecial,
        9 => ReferenceKind::InvokeInterface,
        n => return Err(format!("Unexpected reference kind {} when reading CONSTANT_methodhandle at index {}", n, *ix - 1)),
    };
    let reference_ref = read_unresolved_cp_ref(bytes, ix)?;
    Ok(ConstantPoolEntry::MethodHandle(reference_kind, reference_ref))
}

fn read_constant_methodtype<'a>(bytes: &'a [u8], ix: &mut usize) -> Result<ConstantPoolEntry<'a>, String> {
    let descriptor_ref = read_unresolved_cp_ref(bytes, ix)?;
    Ok(ConstantPoolEntry::MethodType(descriptor_ref))
}

fn read_constant_invokedynamic<'a>(bytes: &'a [u8], ix: &mut usize) -> Result<ConstantPoolEntry<'a>, String> {
    let bootstrap_method_ref = BootstrapMethodRef::Unresolved(read_u2(bytes, ix)?);
    let name_and_type_ref = read_unresolved_cp_ref(bytes, ix)?;
    Ok(ConstantPoolEntry::InvokeDynamic(bootstrap_method_ref, name_and_type_ref))
}

fn resolve_constant_pool<'a>(constant_pool: &[Rc<ConstantPoolEntry<'a>>]) -> Result<(), String> {
    let mut resolved_count = 0;
    while resolved_count < constant_pool.len() {
        let mut count = 0;
        for (i, cp_entry) in constant_pool.iter().enumerate() {
            if cp_entry.resolve(i, &constant_pool)? {
                count += 1;
            }
        }
        if count == resolved_count {
            return err("Unable to resolve all constant pool entries");
        }
        resolved_count = count;
    }
    Ok(())
}

fn validate_constant_pool<'a>(constant_pool: &[Rc<ConstantPoolEntry<'a>>], major_version: u16) -> Result<(), String> {
    for (i, cp_entry) in constant_pool.iter().enumerate() {
        cp_entry.validate(major_version).map_err(|e| format!("{} constant pool entry {}", e, i))?;
    }
    Ok(())
}

pub(crate) fn read_constant_pool<'a>(bytes: &'a [u8], ix: &mut usize, constant_pool_count: u16, major_version: u16) -> Result<Vec<Rc<ConstantPoolEntry<'a>>>, String> {
    let mut constant_pool = Vec::new();
    constant_pool.push(Rc::new(ConstantPoolEntry::Zero));
    let mut cp_ix = 1;
    while cp_ix < constant_pool_count {
        let constant_type = read_u1(bytes, ix)?;
        constant_pool.push(Rc::new(match constant_type {
            1 => read_constant_utf8(bytes, ix)?,
            3 => read_constant_integer(bytes, ix)?,
            4 => read_constant_float(bytes, ix)?,
            5 => read_constant_long(bytes, ix)?,
            6 => read_constant_double(bytes, ix)?,
            7 => read_constant_class(bytes, ix)?,
            8 => read_constant_string(bytes, ix)?,
            9 => read_constant_fieldref(bytes, ix)?,
            10 => read_constant_methodref(bytes, ix)?,
            11 => read_constant_interfacemethodref(bytes, ix)?,
            12 => read_constant_nameandtype(bytes, ix)?,
            15 => read_constant_methodhandle(bytes, ix)?,
            16 => read_constant_methodtype(bytes, ix)?,
            18 => read_constant_invokedynamic(bytes, ix)?,
            n => return Err(format!("Unexpected constant pool entry type {} at index {}", n, *ix - 1)),
        }));
        cp_ix += 1;
        if constant_type == 5 || constant_type == 6 {
            // long and double types take up two entries in the constant pool,
            // so eat up another index.
            cp_ix += 1;
            constant_pool.push(Rc::new(ConstantPoolEntry::Unused));
        }
    }
    resolve_constant_pool(&constant_pool)?;
    validate_constant_pool(&constant_pool, major_version)?;
    Ok(constant_pool)
}

pub(crate) fn read_cp_ref<'a>(bytes: &'a [u8], ix: &mut usize, pool: &[Rc<ConstantPoolEntry<'a>>], allowed: ConstantPoolEntryTypes) -> Result<Rc<ConstantPoolEntry<'a>>, String> {
    let cp_index = read_u2(bytes, ix)? as usize;
    if cp_index >= pool.len() {
        return Err(format!("Out-of-bounds index {} in constant pool reference for", cp_index));
    }
    pool[cp_index].ensure_type(allowed)?;
    Ok(pool[cp_index].clone())
}