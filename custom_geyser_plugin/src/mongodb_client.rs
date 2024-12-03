/// A concurrent implementation for writing accounts into the MongoDB in parallel.
use {
    crate::geyser_plugin_mongodb::{GeyserPluginMongoDBConfig, GeyserPluginMongoDbError}, chrono::Utc, crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender}, log::*, mongodb::{bson::doc, options::{ClientOptions, Tls, TlsOptions}, Client }, openssl::ssl::{SslConnector, SslFiletype, SslMethod}, serde::{Deserialize, Serialize}, solana_geyser_plugin_interface::geyser_plugin_interface::{
        GeyserPluginError, ReplicaAccountInfoV3, ReplicaBlockInfoV3, SlotStatus,
    }, solana_measure::measure::Measure, solana_metrics::*, solana_sdk::timing::AtomicInterval, std::{
        collections::HashSet,
        sync::{
            atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
            Arc, Mutex,
        },
        thread::{self, sleep, Builder, JoinHandle},
        time::Duration,
    }
};

/// The maximum asynchronous requests allowed in the channel to avoid excessive
/// memory usage. The downside -- calls after this threshold is reached can get blocked.
const MAX_ASYNC_REQUESTS: usize = 40960;
const SAFE_BATCH_STARTING_SLOT_CUSHION: u64 = 2 * 40960;
const DEFAULT_MONGO_DB_PORT: u16 = 27017;
const DEFAULT_THREADS_COUNT: usize = 100;
const DEFAULT_ACCOUNTS_INSERT_BATCH_SIZE: usize = 10;
const ACCOUNT_COLUMN_COUNT: usize = 10;
const DEFAULT_PANIC_ON_DB_ERROR: bool = false;
const DEFAULT_STORE_ACCOUNT_HISTORICAL_DATA: bool = false;

struct MongodbClientWrapper {
    client: mongodb::Client,
    accounts_collection:mongodb::Collection<DbAccountInfo>,
    slots_collection:mongodb::Collection<SlotMetadata>,
    transactions_colection:mongodb::Collection<TransactionLog>,
    token_owner_index_collection: Option<mongodb::Collection<TokenSecondaryIndexEntry>>,
    token_mint_index_collection: Option<mongodb::Collection<TokenSecondaryIndexEntry>>,
}

pub struct SimpleMongoDbClient {
    batch_size: usize,
    slots_at_startup: HashSet<u64>, //Hashset may consume sig memory if many slots are processed at startup. consider using a bitmap here?
    pending_account_updates: Vec<DbAccountInfo>,
    index_token_owner: bool,
    index_token_mint: bool,
    pending_token_owner_index: Vec<TokenSecondaryIndexEntry>,
    pending_token_mint_index: Vec<TokenSecondaryIndexEntry>,
    client: tokio::sync::Mutex<MongodbClientWrapper>, //allow thread-safe access to client wrapper
}

struct MongodbClientWorker {
    client: SimpleMongoDbClient,
    /// Indicating if accounts notification during startup is done.
    is_startup_done: bool,
}

#[derive(Clone,Debug, Serialize, Deserialize)]
pub struct SlotMetadata{
    pub slot: u64,
    pub parent:Option<String>,
    pub status: SlotStatus,
    pub blockhash:Option<String>,
    pub leader:Option<u8>,
    pub timestamp:Option<u8>
}

#[derive(Clone, PartialEq, Debug)]
pub struct DbAccountInfo {
    pub pubkey: Vec<u8>, //using fixed-sized array, [u8; 32] for pubkeys may improve cache locality?
    pub lamports: i64,
    pub owner: Vec<u8>,
    pub executable: bool,
    pub rent_epoch: i64,
    pub data: Vec<u8>,
    pub slot: i64,
    pub write_version: i64,
    pub txn_signature: Option<Vec<u8>>,
}


impl DbAccountInfo {
    fn new<T: ReadableAccountInfo>(account: &T, slot: u64) -> DbAccountInfo {
        let data = account.data().to_vec();
        Self {
            pubkey: account.pubkey().to_vec(),
            lamports: account.lamports(),
            owner: account.owner().to_vec(),
            executable: account.executable(),
            rent_epoch: account.rent_epoch(),
            data,
            slot: slot as i64,
            write_version: account.write_version(),
            txn_signature: account.txn_signature().map(|v| v.to_vec()),
        }
    }
}
pub trait ReadableAccountInfo: Sized {
    fn pubkey(&self) -> &[u8];
    fn owner(&self) -> &[u8];
    fn lamports(&self) -> i64;
    fn executable(&self) -> bool;
    fn rent_epoch(&self) -> i64;
    fn data(&self) -> &[u8];
    fn write_version(&self) -> i64;
    fn txn_signature(&self) -> Option<&[u8]>;
}

impl ReadableAccountInfo for DbAccountInfo {
    fn pubkey(&self) -> &[u8] {
        &self.pubkey
    }

    fn owner(&self) -> &[u8] {
        &self.owner
    }

    fn lamports(&self) -> i64 {
        self.lamports
    }

    fn executable(&self) -> bool {
        self.executable
    }

    fn rent_epoch(&self) -> i64 {
        self.rent_epoch
    }

    fn data(&self) -> &[u8] {
        &self.data
    }

    fn write_version(&self) -> i64 {
        self.write_version
    }

    fn txn_signature(&self) -> Option<&[u8]> {
        self.txn_signature.as_deref()
    }
}

impl<'a> ReadableAccountInfo for ReplicaAccountInfoV3<'a> {
    fn pubkey(&self) -> &[u8] {
        self.pubkey
    }

    fn owner(&self) -> &[u8] {
        self.owner
    }

    fn lamports(&self) -> i64 {
        self.lamports as i64
    }

    fn executable(&self) -> bool {
        self.executable
    }

    fn rent_epoch(&self) -> i64 {
        self.rent_epoch as i64
    }

    fn data(&self) -> &[u8] {
        self.data
    }

    fn write_version(&self) -> i64 {
        self.write_version as i64
    }

    fn txn_signature(&self) -> Option<&[u8]> {
        self.txn.map(|v| v.signature().as_ref())
    }
}

pub trait MongoDBClient {
    fn join(&mut self) -> thread::Result<()> {
        Ok(())
    }
    fn update_account(
        &mut self,
        account: DbAccountInfo,
        is_startup: bool,
    ) -> Result<(), GeyserPluginError>;

    fn update_slot_status(
        &mut self,
        slot: u64,
        parent: Option<u64>,
        status: SlotStatus,
    ) -> Result<(), GeyserPluginError>;

    fn notify_end_of_startup(&mut self) -> Result<(), GeyserPluginError>;

    fn log_transaction(
        &mut self,
        transaction_log_info: LogTransactionRequest,
    ) -> Result<(), GeyserPluginError>;

    fn update_block_metadata(
        &mut self,
        block_info: UpdateBlockMetadataRequest,
    ) -> Result<(), GeyserPluginError>;
}

impl SimpleMongoDbClient {
    
}