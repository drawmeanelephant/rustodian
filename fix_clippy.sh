sed -i 's/metadata: Default::default()/metadata: rustodian_types::ProjectMetadata::default()/g' crates/rustodian-storage/src/log_store.rs
sed -i 's/metadata: Default::default()/metadata: rustodian_types::ProjectMetadata::default()/g' crates/rustodian-storage/src/store.rs
sed -i 's/println!("{}", err.to_string());/println!("{}", err);/g' crates/rustodian-storage/src/store.rs
