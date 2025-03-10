use std::collections::{hash_map, HashMap};
use std::error::Error;
use std::marker::PhantomData;
use std::sync::Arc;

use parking_lot::RwLock;

use protocol::codec::ProtocolCodec;
use protocol::traits::{
    IntoIteratorByRef, StorageAdapter, StorageBatchModify, StorageIterator, StorageSchema,
};
use protocol::types::Bytes;
use protocol::{async_trait, Display, From, ProtocolError, ProtocolErrorKind, ProtocolResult};

type Category = HashMap<Vec<u8>, Vec<u8>>;

#[derive(Debug)]
pub struct MemoryAdapter {
    db: Arc<RwLock<HashMap<String, Category>>>,
}

impl MemoryAdapter {
    pub fn new() -> Self {
        MemoryAdapter {
            db: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for MemoryAdapter {
    fn default() -> Self {
        MemoryAdapter {
            db: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

pub struct MemoryIterator<'a, S: StorageSchema> {
    inner: hash_map::Iter<'a, Vec<u8>, Vec<u8>>,
    pin_s: PhantomData<S>,
}

impl<'a, S: StorageSchema> Iterator for MemoryIterator<'a, S> {
    type Item = ProtocolResult<(<S as StorageSchema>::Key, <S as StorageSchema>::Value)>;

    fn next(&mut self) -> Option<Self::Item> {
        let kv_decode = |(k_bytes, v_bytes): (&Vec<u8>, &Vec<u8>)| -> ProtocolResult<_> {
            let k_bytes = Bytes::copy_from_slice(k_bytes.as_ref());
            let key = <_>::decode(k_bytes)?;
            let v_bytes = Bytes::copy_from_slice(v_bytes.as_ref());
            let val = <_>::decode(v_bytes)?;

            Ok((key, val))
        };

        self.inner.next().map(kv_decode)
    }
}

pub struct MemoryIntoIterator<'a, S: StorageSchema> {
    inner: parking_lot::RwLockReadGuard<'a, HashMap<String, Category>>,
    pin_s: PhantomData<S>,
}

impl<'a, 'b: 'a, S: StorageSchema> IntoIterator for &'b MemoryIntoIterator<'a, S> {
    type IntoIter = StorageIterator<'a, S>;
    type Item = ProtocolResult<(<S as StorageSchema>::Key, <S as StorageSchema>::Value)>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(MemoryIterator {
            inner: self
                .inner
                .get(&S::category().to_string())
                .expect("impossible, already ensure we have category in prepare_iter")
                .iter(),
            pin_s: PhantomData::<S>,
        })
    }
}

impl<'c, S: StorageSchema> IntoIteratorByRef<S> for MemoryIntoIterator<'c, S> {
    fn ref_to_iter<'a, 'b: 'a>(&'b self) -> StorageIterator<'a, S> {
        self.into_iter()
    }
}

#[async_trait]
impl StorageAdapter for MemoryAdapter {
    async fn insert<S: StorageSchema>(
        &self,
        key: <S as StorageSchema>::Key,
        val: <S as StorageSchema>::Value,
    ) -> ProtocolResult<()> {
        let key = key.encode()?.to_vec();
        let val = val.encode()?.to_vec();

        let mut db = self.db.write();
        let db = db
            .entry(S::category().to_string())
            .or_insert_with(HashMap::new);

        db.insert(key, val);

        Ok(())
    }

    async fn get<S: StorageSchema>(
        &self,
        key: <S as StorageSchema>::Key,
    ) -> ProtocolResult<Option<<S as StorageSchema>::Value>> {
        let key = key.encode()?;

        let mut db = self.db.write();
        let db = db
            .entry(S::category().to_string())
            .or_insert_with(HashMap::new);

        let opt_bytes = db.get(&key.to_vec()).cloned();

        if let Some(bytes) = opt_bytes {
            let val = <_>::decode(Bytes::copy_from_slice(&bytes))?;

            Ok(Some(val))
        } else {
            Ok(None)
        }
    }

    async fn remove<S: StorageSchema>(&self, key: <S as StorageSchema>::Key) -> ProtocolResult<()> {
        let key = key.encode()?.to_vec();

        let mut db = self.db.write();
        let db = db
            .entry(S::category().to_string())
            .or_insert_with(HashMap::new);

        db.remove(&key);

        Ok(())
    }

    async fn contains<S: StorageSchema>(
        &self,
        key: <S as StorageSchema>::Key,
    ) -> ProtocolResult<bool> {
        let key = key.encode()?.to_vec();

        let mut db = self.db.write();
        let db = db
            .entry(S::category().to_string())
            .or_insert_with(HashMap::new);

        Ok(db.get(&key).is_some())
    }

    async fn batch_modify<S: StorageSchema>(
        &self,
        keys: Vec<<S as StorageSchema>::Key>,
        vals: Vec<StorageBatchModify<S>>,
    ) -> ProtocolResult<()> {
        if keys.len() != vals.len() {
            return Err(MemoryAdapterError::BatchLengthMismatch.into());
        }

        let mut pairs: Vec<(Bytes, Option<Bytes>)> = Vec::with_capacity(keys.len());

        for (key, value) in keys.into_iter().zip(vals.into_iter()) {
            let key = key.encode()?;

            let value = match value {
                StorageBatchModify::Insert(value) => Some(value.encode()?),
                StorageBatchModify::Remove => None,
            };

            pairs.push((key, value))
        }

        let mut db = self.db.write();
        let db = db
            .entry(S::category().to_string())
            .or_insert_with(HashMap::new);

        for (key, value) in pairs.into_iter() {
            match value {
                Some(value) => db.insert(key.to_vec(), value.to_vec()),
                None => db.remove(&key.to_vec()),
            };
        }

        Ok(())
    }

    fn prepare_iter<'a, 'b: 'a, S: StorageSchema + 'static, P: AsRef<[u8]> + 'a>(
        &'b self,
        _prefix: &P,
    ) -> ProtocolResult<Box<dyn IntoIteratorByRef<S> + 'a>> {
        {
            self.db
                .write()
                .entry(S::category().to_string())
                .or_insert_with(HashMap::new);
        }

        Ok(Box::new(MemoryIntoIterator {
            inner: self.db.read(),
            pin_s: PhantomData::<S>,
        }))
    }
}

#[derive(Debug, Display, From)]
pub enum MemoryAdapterError {
    #[display(fmt = "batch length do not match")]
    BatchLengthMismatch,
}

impl Error for MemoryAdapterError {}

impl From<MemoryAdapterError> for ProtocolError {
    fn from(err: MemoryAdapterError) -> ProtocolError {
        ProtocolError::new(ProtocolErrorKind::Storage, Box::new(err))
    }
}
