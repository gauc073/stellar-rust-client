//! `ScVal` construction and extraction helpers for the common primitive
//! types, so callers don't have to re-derive the same `nativeToScVal` /
//! `scValToNative`-equivalent boilerplate at every call site (this started
//! as copy-pasted `sc_address`/`sc_string` helpers duplicated across the
//! `tests/` files in this repo).
//!
//! Deliberately narrow in scope: this covers the primitives every contract
//! call needs (address, string, symbol, bytes, bool, and the integer
//! widths), not a full generic XDR <-> native converter for arbitrary
//! `Vec`/`Map`/struct-shaped `ScVal`s -- that's squarely `soroban-client`'s
//! and `stellar-xdr`'s job, and duplicating it here would just be a worse
//! copy. Reach for `soroban_client::xdr::ScVal` directly for anything more
//! complex than what's below.
//!
//! Usage is namespaced under `scval::` rather than re-exported at the crate
//! root, since names like `i128`/`u128`/`bool` would otherwise shadow the
//! primitive types of the same name.

use crate::error::{Result, SorobanUtilsError};
use soroban_client::address::{Address, AddressTrait};
use soroban_client::xdr::{Int128Parts, ScString, ScSymbol, ScVal, UInt128Parts};

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
