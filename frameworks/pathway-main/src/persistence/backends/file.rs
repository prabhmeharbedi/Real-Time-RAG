// Copyright © 2024 Pathway

use log::{error, warn};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use futures::channel::oneshot;

use crate::fs_helpers::ensure_directory;
use crate::persistence::backends::PersistenceBackend;
use crate::persistence::Error;

use super::BackendPutFuture;

const TEMPORARY_OBJECT_SUFFIX: &str = ".tmp";

#[derive(Debug)]
pub struct FilesystemKVStorage {
    root_path: PathBuf,
}

impl FilesystemKVStorage {
    pub fn new(root_path: &Path) -> Result<Self, Error> {
        ensure_directory(root_path)?;
        Ok(Self {
            root_path: root_path.to_path_buf(),
        })
    }

    fn write_file(temp_path: &Path, final_path: &Path, value: &[u8]) -> Result<(), Error> {
        let mut output_file = File::create(temp_path)?;
        output_file.write_all(value)?;
        // Note: if we need Pathway to tolerate not only Pathway failures,
        // but only OS crash or power loss, the below line must be uncommented.
        // output_file.sync_all()?;
        std::fs::rename(temp_path, final_path)?;
        Ok(())
    }
}

impl PersistenceBackend for FilesystemKVStorage {
    fn list_keys(&self) -> Result<Vec<String>, Error> {
        let mut keys = Vec::new();

        for entry in std::fs::read_dir(&self.root_path)? {
            if let Err(e) = entry {
                error!("Error while doing the folder scan: {e}. Output may duplicate a part of previous run");
                continue;
            }
            let entry = entry.unwrap();
            let file_type = entry.file_type();
            match file_type {
                Ok(file_type) => {
                    if !file_type.is_file() {
                        continue;
                    }
                    match entry.file_name().into_string() {
                        Ok(key) => {
                            let is_temporary = key.ends_with(TEMPORARY_OBJECT_SUFFIX);
                            if !is_temporary {
                                keys.push(key);
                            }
                        }
                        Err(name) => warn!("Non-Unicode file name: {name:?}"),
                    };
                }
                Err(e) => {
                    error!("Couldn't detect file type for {entry:?}: {e}");
                }
            }
        }

        Ok(keys)
    }

    fn get_value(&self, key: &str) -> Result<Vec<u8>, Error> {
        Ok(std::fs::read(self.root_path.join(key))?)
    }

    fn put_value(&mut self, key: &str, value: Vec<u8>) -> BackendPutFuture {
        let (sender, receiver) = oneshot::channel();

        let tmp_path = self
            .root_path
            .join(key.to_owned() + TEMPORARY_OBJECT_SUFFIX);
        let final_path = self.root_path.join(key);
        let put_value_result = Self::write_file(&tmp_path, &final_path, &value);
        sender
            .send(put_value_result)
            .expect("The receiver must still be listening for the result of the put_value");
        receiver
    }

    fn remove_key(&mut self, key: &str) -> Result<(), Error> {
        std::fs::remove_file(self.root_path.join(key))?;
        Ok(())
    }
}
