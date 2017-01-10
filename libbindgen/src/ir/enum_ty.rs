//! Intermediate representation for C/C++ enumerations.

use clang;
use ir::annotations::Annotations;
use parse::{ClangItemParser, ParseError};
use super::context::{BindgenContext, ItemId};
use super::item::Item;
use super::ty::TypeKind;

/// A C/C++ enumeration.
#[derive(Debug)]
pub struct Enum {
    /// The representation used for this enum; it should be an `IntKind` type or
    /// an alias to one.
    ///
    /// It's `None` if the enum is a forward declaration and isn't defined
    /// anywhere else, see `tests/headers/func_ptr_in_struct.h`.
    repr: Option<ItemId>,

    /// The different variants, with explicit values.
    variants: Vec<EnumVariant>,
}

impl Enum {
    /// Construct a new `Enum` with the given representation and variants.
    pub fn new(repr: Option<ItemId>, variants: Vec<EnumVariant>) -> Self {
        Enum {
            repr: repr,
            variants: variants,
        }
    }

    /// Get this enumeration's representation.
    pub fn repr(&self) -> Option<ItemId> {
        self.repr
    }

    /// Get this enumeration's variants.
    pub fn variants(&self) -> &[EnumVariant] {
        &self.variants
    }

    /// Construct an enumeration from the given Clang type.
    pub fn from_ty(ty: &clang::Type,
                   ctx: &mut BindgenContext)
                   -> Result<Self, ParseError> {
        use clang_sys::*;
        if ty.kind() != CXType_Enum {
            return Err(ParseError::Continue);
        }

        let declaration = ty.declaration().canonical();
        let repr = declaration.enum_type()
            .and_then(|et| Item::from_ty(&et, None, None, ctx).ok());
        let mut variants = vec![];

        // Assume signedness since the default type by the C standard is an int.
        let is_signed =
            repr.and_then(|r| ctx.resolve_type(r).safe_canonical_type(ctx))
                .map_or(true, |ty| {
                    match *ty.kind() {
                        TypeKind::Int(ref int_kind) => int_kind.is_signed(),
                        ref other => {
                            panic!("Since when enums can be non-integers? {:?}",
                               other)
                        }
                    }
                });

        let type_name = ty.spelling();
        let type_name = if type_name.is_empty() { None } else { Some(type_name) };
        let type_name = type_name.as_ref().map(String::as_str);

        declaration.visit(|cursor| {
            if cursor.kind() == CXCursor_EnumConstantDecl {
                let value = if is_signed {
                    cursor.enum_val_signed().map(EnumVariantValue::Signed)
                } else {
                    cursor.enum_val_unsigned().map(EnumVariantValue::Unsigned)
                };
                if let Some(val) = value {
                    let name = cursor.spelling();
                    let should_constify = ctx.type_chooser()
                        .map_or(false, |c| {
                            c.constify_enum_variant(type_name, &name, val)
                        }) ||
                        Annotations::new(&cursor).map_or(false, |anno| {
                            anno.constify_enum_variant()
                        });
                    let comment = cursor.raw_comment();
                    variants.push(
                        EnumVariant::new(name, comment, val, should_constify));
                }
            }
            CXChildVisit_Continue
        });
        Ok(Enum::new(repr, variants))
    }
}

/// A single enum variant, to be contained only in an enum.
#[derive(Debug)]
pub struct EnumVariant {
    /// The name of the variant.
    name: String,

    /// An optional doc comment.
    comment: Option<String>,

    /// The integer value of the variant.
    val: EnumVariantValue,

    /// Whether this value should explicitly be constified.
    ///
    /// This allows us to solve situations as described in #392.
    force_constify: bool,
}

/// A constant value assigned to an enumeration variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EnumVariantValue {
    /// A signed constant.
    Signed(i64),

    /// An unsigned constant.
    Unsigned(u64),
}

impl EnumVariant {
    /// Construct a new enumeration variant from the given parts.
    pub fn new(name: String,
               comment: Option<String>,
               val: EnumVariantValue,
               force_constify: bool)
               -> Self {
        EnumVariant {
            name: name,
            comment: comment,
            val: val,
            force_constify: force_constify,
        }
    }

    /// Get this variant's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get this variant's value.
    pub fn val(&self) -> EnumVariantValue {
        self.val
    }

    /// Returns whether this variant should be enforced to be a constant by code
    /// generation.
    pub fn force_constification(&self) -> bool {
        self.force_constify
    }
}
