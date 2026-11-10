//! How a [`FoldedSnapshot`] is written: exactly the object the derived, flattened form wrote, with
//! the shared rows in the `messages` the result itself keeps empty.
//!
//! CDXC:SessionChat 2026-11-10 WHY: the fold holds its rows as shared `ChatRow`s (one copy with the authoritative list) rather than inside the wire type's `messages`, so writing it cannot be derived. `messages` is the result's first field, so it is written first from `rows`; every other entry of the result is passed through as the derived code writes it (`EntriesInto` forwards them and drops the empty `messages`), then the frame keys, which gives the record the same bytes as before (memharness replay: every saved record identical).

use serde::ser::{Error, Impossible, SerializeMap, SerializeStruct, Serializer};
use serde::Serialize;

use crate::session::fold::FoldedSnapshot;

impl Serialize for FoldedSnapshot {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("messages", &self.rows)?;
        self.result.serialize(EntriesInto {
            map: &mut map,
            skipping: false,
        })?;
        for (key, value) in &self.extra {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

/// The key the result writes its own (empty) rows under.
const ROWS_KEY: &str = "messages";

/// A serializer that writes the entries of the map or struct it is given into `map`, leaving out
/// [`ROWS_KEY`].
struct EntriesInto<'a, M> {
    map: &'a mut M,
    /// The key just seen was [`ROWS_KEY`], so its value is dropped too.
    skipping: bool,
}

/// Whether `key` is [`ROWS_KEY`]. Keys here are the derived code's field names, plain strings.
fn is_rows_key<K: Serialize + ?Sized>(key: &K) -> bool {
    matches!(serde_json::to_value(key), Ok(serde_json::Value::String(name)) if name == ROWS_KEY)
}

impl<M: SerializeMap> SerializeMap for EntriesInto<'_, M> {
    type Ok = ();
    type Error = M::Error;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), M::Error> {
        self.skipping = is_rows_key(key);
        if self.skipping {
            return Ok(());
        }
        self.map.serialize_key(key)
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), M::Error> {
        if std::mem::take(&mut self.skipping) {
            return Ok(());
        }
        self.map.serialize_value(value)
    }

    fn serialize_entry<K: Serialize + ?Sized, V: Serialize + ?Sized>(
        &mut self,
        key: &K,
        value: &V,
    ) -> Result<(), M::Error> {
        if is_rows_key(key) {
            return Ok(());
        }
        self.map.serialize_entry(key, value)
    }

    fn end(self) -> Result<(), M::Error> {
        Ok(())
    }
}

impl<M: SerializeMap> SerializeStruct for EntriesInto<'_, M> {
    type Ok = ();
    type Error = M::Error;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), M::Error> {
        if key == ROWS_KEY {
            return Ok(());
        }
        self.map.serialize_entry(key, value)
    }

    fn end(self) -> Result<(), M::Error> {
        Ok(())
    }
}

/// Anything but a map or a struct is not a result.
fn not_an_object<E: Error>() -> E {
    E::custom("a folded snapshot's result writes as an object")
}

impl<'a, M: SerializeMap> Serializer for EntriesInto<'a, M> {
    type Ok = ();
    type Error = M::Error;
    type SerializeSeq = Impossible<(), M::Error>;
    type SerializeTuple = Impossible<(), M::Error>;
    type SerializeTupleStruct = Impossible<(), M::Error>;
    type SerializeTupleVariant = Impossible<(), M::Error>;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Impossible<(), M::Error>;

    fn serialize_map(self, _len: Option<usize>) -> Result<Self, M::Error> {
        Ok(self)
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self, M::Error> {
        Ok(self)
    }

    fn serialize_bool(self, _: bool) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_i8(self, _: i8) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_i16(self, _: i16) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_i32(self, _: i32) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_i64(self, _: i64) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_u8(self, _: u8) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_u16(self, _: u16) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_u32(self, _: u32) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_u64(self, _: u64) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_f32(self, _: f32) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_f64(self, _: f64) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_char(self, _: char) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_str(self, _: &str) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_none(self) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_some<T: Serialize + ?Sized>(self, _: &T) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_unit(self) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
    ) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: &T,
    ) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<(), M::Error> {
        Err(not_an_object())
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, M::Error> {
        Err(not_an_object())
    }
    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, M::Error> {
        Err(not_an_object())
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, M::Error> {
        Err(not_an_object())
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, M::Error> {
        Err(not_an_object())
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, M::Error> {
        Err(not_an_object())
    }
}
