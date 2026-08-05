//! `ScVal` construction and extraction helpers for the common primitive
//! types, so callers don't have to re-derive the same `nativeToScVal` /
//! `scValToNative`-equivalent boilerplate at every call site (this started
//! as copy-pasted `sc_address`/`sc_string` helpers duplicated across the
//! `tests/` files in this repo).
//!
//! Deliberately narrow in scope: this covers the primitives every contract
//! call needs (address, string, symbol, bytes, bool, the integer widths),
//! plus `ScVal::Vec` support for both mixed-type vecs (`vec_val`/
//! `to_vec_val`, working in terms of raw `ScVal` elements) and homogeneous
//! typed vecs (`vec_of_addresses`, `vec_of_u128`, `to_vec_of_i128`, etc.,
//! working in terms of `Vec<T>` directly -- built on the generic
//! `vec_of`/`to_vec_of` combinators, which take a per-element encoder/
//! decoder if you need a primitive type without a named wrapper yet).
//! `ScVal::Map` and struct-shaped `ScVal`s aren't covered -- say the word if
//! you need `Map`, same shape of addition as `Vec` was. Reach for
//! `soroban_client::xdr::ScVal` directly for anything more complex than
//! that.
//!
//! Usage is namespaced under `utils::` rather than re-exported at the crate
//! root, since names like `i128`/`u128`/`bool` would otherwise shadow the
//! primitive types of the same name.

use crate::error::{Result, SorobanUtilsError};
use soroban_client::address::{Address, AddressTrait};
use soroban_client::xdr::{Int128Parts, ScString, ScSymbol, ScVal, ScVec, UInt128Parts};

fn xdr_err(context: &str, e: impl std::fmt::Debug) -> SorobanUtilsError {
    SorobanUtilsError::Xdr(format!("{context}: {e:?}"))
}

fn type_mismatch(expected: &str, got: &ScVal) -> SorobanUtilsError {
    SorobanUtilsError::Xdr(format!("expected ScVal::{expected}, got {got:?}"))
}

// ---------------------------------------------------------------------
// Construction (native -> ScVal), equivalent to `nativeToScVal(x, {type})`
// ---------------------------------------------------------------------

/// `nativeToScVal(account, { type: "address" })` equivalent. Accepts either
/// a `G...` account address or a `C...` contract address.
pub fn address(value: &str) -> Result<ScVal> {
    Address::new(value)
        .map_err(|e| xdr_err("invalid address", e))?
        .to_sc_val()
        .map_err(|e| xdr_err("failed to encode address", e))
}

/// `nativeToScVal(value, { type: "string" })` equivalent.
pub fn string(value: &str) -> Result<ScVal> {
    Ok(ScVal::String(ScString(
        value
            .as_bytes()
            .to_vec()
            .try_into()
            .map_err(|e| xdr_err("string too long for ScString", e))?,
    )))
}

/// `nativeToScVal(value, { type: "symbol" })` equivalent. Symbols are
/// limited to 32 characters by the protocol.
pub fn symbol(value: &str) -> Result<ScVal> {
    Ok(ScVal::Symbol(ScSymbol(
        value
            .as_bytes()
            .to_vec()
            .try_into()
            .map_err(|e| xdr_err("symbol too long (max 32 chars)", e))?,
    )))
}

/// `nativeToScVal(value, { type: "bytes" })` equivalent.
pub fn bytes(value: &[u8]) -> Result<ScVal> {
    Ok(ScVal::Bytes(
        value
            .to_vec()
            .try_into()
            .map_err(|e| xdr_err("failed to encode bytes", e))?,
    ))
}

pub fn boolean(value: bool) -> ScVal {
    ScVal::Bool(value)
}

pub fn void() -> ScVal {
    ScVal::Void
}

pub fn i32_val(value: i32) -> ScVal {
    ScVal::I32(value)
}

pub fn u32_val(value: u32) -> ScVal {
    ScVal::U32(value)
}

pub fn i64_val(value: i64) -> ScVal {
    ScVal::I64(value)
}

pub fn u64_val(value: u64) -> ScVal {
    ScVal::U64(value)
}

/// `nativeToScVal(value, { type: "i128" })` equivalent.
pub fn i128_val(value: i128) -> ScVal {
    let raw = value.to_be_bytes();
    let hi = i64::from_be_bytes(raw[0..8].try_into().expect("8 bytes"));
    let lo = u64::from_be_bytes(raw[8..16].try_into().expect("8 bytes"));
    ScVal::I128(Int128Parts { hi, lo })
}

/// `nativeToScVal(value, { type: "u128" })` equivalent.
pub fn u128_val(value: u128) -> ScVal {
    let raw = value.to_be_bytes();
    let hi = u64::from_be_bytes(raw[0..8].try_into().expect("8 bytes"));
    let lo = u64::from_be_bytes(raw[8..16].try_into().expect("8 bytes"));
    ScVal::U128(UInt128Parts { hi, lo })
}

/// `ScVal::Vec` equivalent -- a Soroban `Vec<T>` argument/return value,
/// built from an already-encoded `Vec<ScVal>` (mix element construction with
/// the other `utils::` functions above, e.g.
/// `utils::vec_val(vec![utils::address(a)?, utils::i128_val(1)])`).
///
/// An empty Rust `Vec` encodes as `ScVal::Vec(Some(ScVec(empty)))`, matching
/// how the JS/TS SDK's `nativeToScVal([], { type: "vec" })` behaves --
/// *not* `ScVal::Vec(None)`, which the protocol reserves for a genuinely
/// absent/null vec (distinct from an empty one). See `to_vec_val` below for
/// how that `None` case is handled on the way back out.
pub fn vec_val(items: Vec<ScVal>) -> Result<ScVal> {
    Ok(ScVal::Vec(Some(ScVec(items.try_into().map_err(|e| {
        xdr_err("failed to encode Vec<ScVal> as ScVec", e)
    })?))))
}

/// Encode a homogeneous Rust slice into `ScVal::Vec`, given a per-element
/// encoder. This is what the typed `vec_of_*` wrappers below are built
/// from -- reach for it directly for a primitive type that doesn't have a
/// named wrapper yet, e.g. `utils::vec_of(&roles, |r| utils::symbol(r))`.
pub fn vec_of<T>(items: &[T], mut encode: impl FnMut(&T) -> Result<ScVal>) -> Result<ScVal> {
    let encoded = items.iter().map(&mut encode).collect::<Result<Vec<_>>>()?;
    vec_val(encoded)
}

/// `Vec<&str>` (account/contract addresses) -> `ScVal::Vec`.
pub fn vec_of_addresses(items: &[&str]) -> Result<ScVal> {
    vec_of(items, |a| address(a))
}

/// `Vec<&str>` -> `ScVal::Vec` of `ScVal::String`.
pub fn vec_of_strings(items: &[&str]) -> Result<ScVal> {
    vec_of(items, |s| string(s))
}

/// `Vec<&str>` -> `ScVal::Vec` of `ScVal::Symbol`.
pub fn vec_of_symbols(items: &[&str]) -> Result<ScVal> {
    vec_of(items, |s| symbol(s))
}

/// `Vec<i128>` -> `ScVal::Vec` of `ScVal::I128`.
pub fn vec_of_i128(items: &[i128]) -> Result<ScVal> {
    vec_of(items, |v| Ok(i128_val(*v)))
}

/// `Vec<u128>` -> `ScVal::Vec` of `ScVal::U128`.
pub fn vec_of_u128(items: &[u128]) -> Result<ScVal> {
    vec_of(items, |v| Ok(u128_val(*v)))
}

/// `Vec<u64>` -> `ScVal::Vec` of `ScVal::U64`.
pub fn vec_of_u64(items: &[u64]) -> Result<ScVal> {
    vec_of(items, |v| Ok(u64_val(*v)))
}

/// `Vec<i64>` -> `ScVal::Vec` of `ScVal::I64`.
pub fn vec_of_i64(items: &[i64]) -> Result<ScVal> {
    vec_of(items, |v| Ok(i64_val(*v)))
}

/// `Vec<u32>` -> `ScVal::Vec` of `ScVal::U32`.
pub fn vec_of_u32(items: &[u32]) -> Result<ScVal> {
    vec_of(items, |v| Ok(u32_val(*v)))
}

/// `Vec<i32>` -> `ScVal::Vec` of `ScVal::I32`.
pub fn vec_of_i32(items: &[i32]) -> Result<ScVal> {
    vec_of(items, |v| Ok(i32_val(*v)))
}

// ---------------------------------------------------------------------
// Extraction (ScVal -> native), equivalent to `scValToNative(val)` for a
// known expected shape.
// ---------------------------------------------------------------------

pub fn to_address(value: &ScVal) -> Result<String> {
    match value {
        ScVal::Address(addr) => Ok(addr.to_string()),
        other => Err(type_mismatch("Address", other)),
    }
}

pub fn to_string_val(value: &ScVal) -> Result<String> {
    match value {
        ScVal::String(s) => {
            String::from_utf8(s.0.to_vec()).map_err(|e| xdr_err("ScString was not valid UTF-8", e))
        }
        other => Err(type_mismatch("String", other)),
    }
}

pub fn to_symbol(value: &ScVal) -> Result<String> {
    match value {
        ScVal::Symbol(s) => {
            String::from_utf8(s.0.to_vec()).map_err(|e| xdr_err("ScSymbol was not valid UTF-8", e))
        }
        other => Err(type_mismatch("Symbol", other)),
    }
}

pub fn to_bytes(value: &ScVal) -> Result<Vec<u8>> {
    match value {
        ScVal::Bytes(b) => Ok(b.to_vec()),
        other => Err(type_mismatch("Bytes", other)),
    }
}

pub fn to_bool(value: &ScVal) -> Result<bool> {
    match value {
        ScVal::Bool(b) => Ok(*b),
        other => Err(type_mismatch("Bool", other)),
    }
}

pub fn to_i32(value: &ScVal) -> Result<i32> {
    match value {
        ScVal::I32(v) => Ok(*v),
        other => Err(type_mismatch("I32", other)),
    }
}

pub fn to_u32(value: &ScVal) -> Result<u32> {
    match value {
        ScVal::U32(v) => Ok(*v),
        other => Err(type_mismatch("U32", other)),
    }
}

pub fn to_i64(value: &ScVal) -> Result<i64> {
    match value {
        ScVal::I64(v) => Ok(*v),
        other => Err(type_mismatch("I64", other)),
    }
}

pub fn to_u64(value: &ScVal) -> Result<u64> {
    match value {
        ScVal::U64(v) => Ok(*v),
        other => Err(type_mismatch("U64", other)),
    }
}

pub fn to_i128(value: &ScVal) -> Result<i128> {
    match value {
        ScVal::I128(Int128Parts { hi, lo }) => {
            let mut raw = [0u8; 16];
            raw[0..8].copy_from_slice(&hi.to_be_bytes());
            raw[8..16].copy_from_slice(&lo.to_be_bytes());
            Ok(i128::from_be_bytes(raw))
        }
        other => Err(type_mismatch("I128", other)),
    }
}

pub fn to_u128(value: &ScVal) -> Result<u128> {
    match value {
        ScVal::U128(UInt128Parts { hi, lo }) => {
            let mut raw = [0u8; 16];
            raw[0..8].copy_from_slice(&hi.to_be_bytes());
            raw[8..16].copy_from_slice(&lo.to_be_bytes());
            Ok(u128::from_be_bytes(raw))
        }
        other => Err(type_mismatch("U128", other)),
    }
}

/// `ScVal::Vec` -> `Vec<ScVal>`. Elements are left as `ScVal` -- decode each
/// one with the matching `to_*` function above once you know its shape
/// (`ScVal` doesn't carry element-type info the way a typed Rust `Vec<T>`
/// would).
///
/// `ScVal::Vec(None)` -- the protocol's "absent vec", distinct from an empty
/// one -- decodes to an empty `Vec` here rather than erroring, since callers
/// almost always want to treat "no vec" and "empty vec" the same way. Match
/// on the raw `ScVal` yourself first if that distinction matters to you.
pub fn to_vec_val(value: &ScVal) -> Result<Vec<ScVal>> {
    match value {
        ScVal::Vec(Some(v)) => Ok(v.0.to_vec()),
        ScVal::Vec(None) => Ok(Vec::new()),
        other => Err(type_mismatch("Vec", other)),
    }
}

/// Decode `ScVal::Vec` into a homogeneous Rust `Vec<T>`, given a per-element
/// decoder. Errors as soon as any element doesn't match what `decode`
/// expects -- this is what the typed `to_vec_of_*` wrappers below are built
/// from.
pub fn to_vec_of<T>(value: &ScVal, mut decode: impl FnMut(&ScVal) -> Result<T>) -> Result<Vec<T>> {
    to_vec_val(value)?.iter().map(&mut decode).collect()
}

/// `ScVal::Vec` of `ScVal::Address` -> `Vec<String>`.
pub fn to_vec_of_addresses(value: &ScVal) -> Result<Vec<String>> {
    to_vec_of(value, to_address)
}

/// `ScVal::Vec` of `ScVal::String` -> `Vec<String>`.
pub fn to_vec_of_strings(value: &ScVal) -> Result<Vec<String>> {
    to_vec_of(value, to_string_val)
}

/// `ScVal::Vec` of `ScVal::Symbol` -> `Vec<String>`.
pub fn to_vec_of_symbols(value: &ScVal) -> Result<Vec<String>> {
    to_vec_of(value, to_symbol)
}

/// `ScVal::Vec` of `ScVal::I128` -> `Vec<i128>`.
pub fn to_vec_of_i128(value: &ScVal) -> Result<Vec<i128>> {
    to_vec_of(value, to_i128)
}

/// `ScVal::Vec` of `ScVal::U128` -> `Vec<u128>`.
pub fn to_vec_of_u128(value: &ScVal) -> Result<Vec<u128>> {
    to_vec_of(value, to_u128)
}

/// `ScVal::Vec` of `ScVal::U64` -> `Vec<u64>`.
pub fn to_vec_of_u64(value: &ScVal) -> Result<Vec<u64>> {
    to_vec_of(value, to_u64)
}

/// `ScVal::Vec` of `ScVal::I64` -> `Vec<i64>`.
pub fn to_vec_of_i64(value: &ScVal) -> Result<Vec<i64>> {
    to_vec_of(value, to_i64)
}

/// `ScVal::Vec` of `ScVal::U32` -> `Vec<u32>`.
pub fn to_vec_of_u32(value: &ScVal) -> Result<Vec<u32>> {
    to_vec_of(value, to_u32)
}

/// `ScVal::Vec` of `ScVal::I32` -> `Vec<i32>`.
pub fn to_vec_of_i32(value: &ScVal) -> Result<Vec<i32>> {
    to_vec_of(value, to_i32)
}
