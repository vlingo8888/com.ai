//! The part of redb's API the engine's caches use (`compile_cache`, `cache_store`), in memory: redb
//! needs nightly Rust on WASI, and in the browser a cache only has to last the session.

use std::collections::{BTreeMap, HashMap};
use std::marker::PhantomData;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct Error;

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("in-memory cache error")
    }
}

impl std::error::Error for Error {}

/// Key and value types of a table
pub trait Value {
    type Ref<'a>;
    fn to_bytes(value: &Self::Ref<'_>) -> Vec<u8>;
    fn from_bytes(bytes: &[u8]) -> Self::Ref<'_>;
}

impl Value for &'static [u8] {
    type Ref<'a> = &'a [u8];
    fn to_bytes(value: &&[u8]) -> Vec<u8> {
        value.to_vec()
    }
    fn from_bytes(bytes: &[u8]) -> &[u8] {
        bytes
    }
}

impl Value for &'static str {
    type Ref<'a> = &'a str;
    fn to_bytes(value: &&str) -> Vec<u8> {
        value.as_bytes().to_vec()
    }
    fn from_bytes(bytes: &[u8]) -> &str {
        std::str::from_utf8(bytes).unwrap_or_default()
    }
}

pub struct TableDefinition<K, V> {
    name: &'static str,
    types: PhantomData<(K, V)>,
}

impl<K, V> TableDefinition<K, V> {
    pub const fn new(name: &'static str) -> Self {
        TableDefinition { name, types: PhantomData }
    }
}

impl<K, V> Clone for TableDefinition<K, V> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K, V> Copy for TableDefinition<K, V> {}

type Tables = Arc<Mutex<HashMap<&'static str, BTreeMap<Vec<u8>, Vec<u8>>>>>;

pub struct Database {
    tables: Tables,
}

pub struct Builder;

impl Builder {
    pub fn new() -> Self {
        Builder
    }

    pub fn set_cache_size(&mut self, _bytes: usize) -> &mut Self {
        self
    }

    pub fn create(&self, _path: impl AsRef<Path>) -> Result<Database, Error> {
        Ok(Database { tables: Tables::default() })
    }
}

impl Database {
    pub fn begin_read(&self) -> Result<ReadTransaction, Error> {
        Ok(ReadTransaction { tables: self.tables.clone() })
    }

    pub fn begin_write(&self) -> Result<WriteTransaction, Error> {
        Ok(WriteTransaction { tables: self.tables.clone() })
    }
}

pub enum Durability {
    None,
    Immediate,
}

pub struct ReadTransaction {
    tables: Tables,
}

pub struct WriteTransaction {
    tables: Tables,
}

impl ReadTransaction {
    pub fn open_table<K, V>(&self, def: TableDefinition<K, V>) -> Result<Table<K, V>, Error> {
        Ok(Table { tables: self.tables.clone(), name: def.name, types: PhantomData })
    }
}

impl WriteTransaction {
    pub fn open_table<K, V>(&self, def: TableDefinition<K, V>) -> Result<Table<K, V>, Error> {
        Ok(Table { tables: self.tables.clone(), name: def.name, types: PhantomData })
    }

    pub fn set_durability(&mut self, _durability: Durability) {}

    /// Writes are applied as they are made
    pub fn commit(self) -> Result<(), Error> {
        Ok(())
    }
}

/// A key or value read back from a table
pub struct AccessGuard<T> {
    bytes: Vec<u8>,
    types: PhantomData<T>,
}

impl<T: Value> AccessGuard<T> {
    fn new(bytes: Vec<u8>) -> Self {
        AccessGuard { bytes, types: PhantomData }
    }

    pub fn value(&self) -> T::Ref<'_> {
        T::from_bytes(&self.bytes)
    }
}

type Entries<K, V> = Box<dyn Iterator<Item = Result<(AccessGuard<K>, AccessGuard<V>), Error>>>;

pub trait ReadableTable<K, V> {
    fn iter(&self) -> Result<Entries<K, V>, Error>;
}

pub trait ReadableTableMetadata {
    fn len(&self) -> Result<u64, Error>;
}

pub struct Table<K, V> {
    tables: Tables,
    name: &'static str,
    types: PhantomData<(K, V)>,
}

impl<K: Value, V: Value> Table<K, V> {
    fn with<R>(&self, f: impl FnOnce(&mut BTreeMap<Vec<u8>, Vec<u8>>) -> R) -> R {
        let mut tables = self.tables.lock().unwrap_or_else(|e| e.into_inner());
        f(tables.entry(self.name).or_default())
    }

    pub fn get<'a>(&self, key: K::Ref<'a>) -> Result<Option<AccessGuard<V>>, Error> {
        let key = K::to_bytes(&key);
        Ok(self.with(|t| t.get(&key).cloned()).map(AccessGuard::new))
    }

    pub fn insert<'a>(&mut self, key: K::Ref<'a>, value: V::Ref<'a>) -> Result<Option<AccessGuard<V>>, Error> {
        let (key, value) = (K::to_bytes(&key), V::to_bytes(&value));
        Ok(self.with(|t| t.insert(key, value)).map(AccessGuard::new))
    }

    pub fn remove<'a>(&mut self, key: K::Ref<'a>) -> Result<Option<AccessGuard<V>>, Error> {
        let key = K::to_bytes(&key);
        Ok(self.with(|t| t.remove(&key)).map(AccessGuard::new))
    }
}

impl<K: Value + 'static, V: Value + 'static> ReadableTable<K, V> for Table<K, V> {
    fn iter(&self) -> Result<Entries<K, V>, Error> {
        let entries: Vec<_> = self.with(|t| t.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
        Ok(Box::new(entries.into_iter().map(|(k, v)| Ok((AccessGuard::new(k), AccessGuard::new(v))))))
    }
}

impl<K: Value, V: Value> ReadableTableMetadata for Table<K, V> {
    fn len(&self) -> Result<u64, Error> {
        Ok(self.with(|t| t.len() as u64))
    }
}
